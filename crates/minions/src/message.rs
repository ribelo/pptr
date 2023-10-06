use std::{any::type_name, fmt::Debug};

use minions_derive::Message;
use thiserror::Error;
use tokio::sync::oneshot;

use crate::{
    impl_error_log_method,
    minion::{Address, Minion},
};

pub trait Messageable: Send + Sync {
    type Response: Send + Sync;
}

#[derive(Debug, Error)]
pub enum MessageBuilderError {
    #[error("Missing message")]
    MissingMessage,
    #[error("Missing sender")]
    MissingSender,
    #[error("Missing recipient")]
    MissingRecipient,
}

#[derive(Debug, Default)]
pub(crate) struct Message<M>
where
    M: Messageable,
{
    pub data: M,
    pub sender: Address,
    pub recipient: Address,
    pub awaiting_response: bool,
}

#[derive(Debug, Default)]
pub struct MessageBuilder<M: Messageable> {
    data: Option<M>,
    sender: Option<Address>,
    recipient: Option<Address>,
    awaiting_response: bool,
}

impl<M: Messageable> MessageBuilder<M> {
    pub fn new() -> Self {
        Self {
            data: None,
            sender: None,
            recipient: None,
            awaiting_response: false,
        }
    }

    pub fn data(mut self, message: impl Into<M>) -> Self {
        self.data = Some(message.into());
        self
    }

    pub fn sender(mut self, address: impl Into<Address>) -> Self {
        self.sender = Some(address.into());
        self
    }

    pub fn recipient(mut self, recipent: impl Into<Address>) -> Self {
        self.recipient = Some(recipent.into());
        self
    }

    pub fn await_response(mut self) -> Self {
        self.awaiting_response = true;
        self
    }

    pub fn build(self) -> Result<Message<M>, MessageBuilderError> {
        Ok(Message {
            data: self.data.ok_or(MessageBuilderError::MissingMessage)?,
            sender: self.sender.ok_or(MessageBuilderError::MissingSender)?,
            recipient: self
                .recipient
                .ok_or(MessageBuilderError::MissingRecipient)?,
            awaiting_response: self.awaiting_response,
        })
    }
}

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("Missing envelope")]
    MissingEnvelope,
    #[error("Invalid envelope")]
    InvalidEnvelope,
}

pub(crate) struct Packet<M: Messageable> {
    pub(crate) envelope: Message<M>,
    pub(crate) reply_address: Option<oneshot::Sender<M::Response>>,
}

pub(crate) struct PacketBuilder<M: Messageable> {
    envelope_builder: MessageBuilder<M>,
    reply_address: Option<oneshot::Sender<M::Response>>,
}

impl<M: Messageable> PacketBuilder<M> {
    pub fn new() -> Self {
        Self {
            envelope_builder: MessageBuilder::new(),
            reply_address: None,
        }
    }

    // Delegated methods for building Envelope
    pub fn message(mut self, message: impl Into<M>) -> Self {
        self.envelope_builder = self.envelope_builder.data(message);
        self
    }

    pub fn sender(mut self, address: impl Into<Address>) -> Self {
        self.envelope_builder = self.envelope_builder.sender(address);
        self
    }

    pub fn recipient(mut self, recipient: impl Into<Address>) -> Self {
        self.envelope_builder = self.envelope_builder.recipient(recipient.into());
        self
    }

    pub fn reply_address(mut self, reply_address: oneshot::Sender<M::Response>) -> Self {
        self.reply_address = Some(reply_address);
        self.envelope_builder = self.envelope_builder.await_response();
        self
    }

    pub fn build(self) -> Result<Packet<M>, PacketError> {
        let envelope = self
            .envelope_builder
            .build()
            .map_err(|_| PacketError::InvalidEnvelope)?;
        let reply_address = self.reply_address;

        Ok(Packet {
            envelope,
            reply_address,
        })
    }
}

#[derive(Debug, Error)]
pub enum PostmanError {
    #[error("Packet build failed for sender: {sender}, recipient: {recipient}")]
    PacketBuildFailed { sender: Address, recipient: Address },

    #[error("Packet send failed for sender: {sender}, recipient: {recipient}")]
    PacketSendFailed { sender: Address, recipient: Address },

    #[error("Response receive failed for sender: {sender}, recipient: {recipient}")]
    ResponseReceiveFailed { sender: Address, recipient: Address },
}

#[derive(Clone, Debug)]
pub(crate) struct Postman<M>
where
    M: Messageable,
{
    tx: tokio::sync::mpsc::Sender<Packet<M>>,
}

impl<M> Postman<M>
where
    M: Messageable,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<Packet<M>>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        message: M,
        sender: impl Into<Address>,
        recipient: impl Into<Address>,
    ) -> Result<(), PostmanError> {
        let sender = sender.into();
        let recipient = recipient.into();
        let packet = PacketBuilder::new()
            .message(message)
            .sender(sender.clone())
            .recipient(recipient.clone())
            .build()
            .map_err(|_| PostmanError::PacketBuildFailed {
                sender: sender.clone(),
                recipient: recipient.clone(),
            })?;

        println!("closed? {}", self.tx.is_closed());
        self.tx.send(packet).await.map_err(|err| {
            println!("Packet send failed: {:?}", err);
            PostmanError::PacketSendFailed { sender, recipient }
        })?;
        Ok(())
    }

    pub async fn send_and_await_response(
        &self,
        message: M,
        sender: impl Into<Address>,
        recipient: impl Into<Address>,
    ) -> Result<M::Response, PostmanError> {
        let sender = sender.into();
        let recipient = recipient.into();
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        let packet = PacketBuilder::new()
            .message(message)
            .sender(sender.clone())
            .recipient(recipient.clone())
            .reply_address(res_tx)
            .build()
            .map_err(|_| PostmanError::PacketBuildFailed {
                sender: sender.clone(),
                recipient: recipient.clone(),
            })?;

        self.tx
            .send(packet)
            .await
            .map_err(|_| PostmanError::PacketSendFailed {
                sender: sender.clone(),
                recipient: recipient.clone(),
            })?;

        res_rx
            .await
            .map_err(|_| PostmanError::ResponseReceiveFailed { sender, recipient })
    }
}

#[derive(Debug)]
pub(crate) struct Mailbox<M>
where
    M: Messageable,
{
    rx: tokio::sync::mpsc::Receiver<Packet<M>>,
}

impl<M: Messageable> Mailbox<M> {
    pub fn new(rx: tokio::sync::mpsc::Receiver<Packet<M>>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<Packet<M>> {
        self.rx.recv().await
    }
}

#[derive(Debug, Message)]
pub(crate) enum ServiceMessage {
    Start,
    Stop,
    Pause,
    Resume,
}

#[derive(Debug, Clone)]
pub(crate) struct ServicePostman {
    tx: tokio::sync::mpsc::Sender<Packet<ServiceMessage>>,
}

impl ServicePostman {
    pub fn new(tx: tokio::sync::mpsc::Sender<Packet<ServiceMessage>>) -> Self {
        Self { tx }
    }

    pub async fn send_and_await_response(
        &self,
        service_message: ServiceMessage,
        sender: impl Into<Address>,
        recipient: impl Into<Address>,
    ) -> Result<(), PostmanError> {
        let sender = sender.into();
        let recipient = recipient.into();
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();

        let packet = PacketBuilder::new()
            .message(service_message)
            .sender(sender.clone())
            .recipient(recipient.clone())
            .reply_address(res_tx) // reply_address ustawia również `awaiting_response` w Envelope
            .build()
            .expect("Packet build failed");

        self.tx
            .send(packet)
            .await
            .map_err(|_| PostmanError::PacketSendFailed {
                sender: sender.clone(),
                recipient: recipient.clone(),
            })?;

        res_rx
            .await
            .map_err(|_| PostmanError::ResponseReceiveFailed { sender, recipient })
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMailbox {
    rx: tokio::sync::mpsc::Receiver<Packet<ServiceMessage>>,
}

impl ServiceMailbox {
    pub fn new(rx: tokio::sync::mpsc::Receiver<Packet<ServiceMessage>>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<Packet<ServiceMessage>> {
        self.rx.recv().await
    }
}

#[derive(Error, Debug)]
pub enum MessageError {
    #[error("Actor does not exist: {0}")]
    ActorDoesNotExist(String),
    #[error("Timed out waiting for response from actor: {actor} for message {msg}")]
    MessageResponseTimeout { msg: String, actor: String },
    #[error("Error sending message {msg} to actor: {actor}")]
    MessageSendError { msg: String, actor: String },
    #[error("Error receiving message {msg} from actor: {actor}")]
    MessageReceiveError { msg: String, actor: String },
    #[error("Error receiving response from actor: {actor} for message {msg}")]
    ResponseReceiveError { msg: String, actor: String },
}

impl_error_log_method!(MessageError);

impl MessageError {
    pub fn actor_does_not_exist<A>() -> Self
    where
        A: Minion,
    {
        Self::ActorDoesNotExist(type_name::<A>().to_string())
    }
    pub fn actor_response_timeout<A>() -> Self
    where
        A: Minion,
    {
        Self::MessageResponseTimeout {
            msg: type_name::<A::Msg>().to_string(),
            actor: type_name::<A>().to_string(),
        }
    }
    pub fn message_send_error<A>() -> Self
    where
        A: Minion,
    {
        Self::MessageSendError {
            msg: type_name::<A::Msg>().to_string(),
            actor: type_name::<A>().to_string(),
        }
    }
    pub fn message_receive_error<A>() -> Self
    where
        A: Minion,
    {
        Self::MessageReceiveError {
            msg: type_name::<A::Msg>().to_string(),
            actor: type_name::<A>().to_string(),
        }
    }
    pub fn response_receive_error<A: Minion>() -> Self {
        Self::ResponseReceiveError {
            msg: type_name::<A::Msg>().to_string(),
            actor: type_name::<A>().to_string(),
        }
    }
}
