use std::{any::type_name, fmt::Debug};

use minions_derive::Message;
use thiserror::Error;
use tokio::sync::oneshot;

use crate::minion::Minion;

pub trait Message: Debug + Send + Sync {
    type Response: Debug + Send + Sync;
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

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("Missing envelope")]
    MissingEnvelope,
    #[error("Invalid envelope")]
    InvalidEnvelope,
}

pub(crate) struct Packet<M: Message> {
    pub message: M,
    pub(crate) reply_address: Option<oneshot::Sender<M::Response>>,
}

#[derive(Debug, Error)]
pub enum PostmanError {
    // #[error("Packet build failed for sender: {sender}, recipient: {recipient}")]
    // PacketBuildFailed { sender: Address, recipient: Address },
    //
    #[error("Packet send failed: {0}")]
    PacketSendFailed(String),
    //
    #[error("Response receive failed: {0}")]
    ResponseReceiveFailed(String),
}
//
#[derive(Clone, Debug)]
pub(crate) struct Postman<M>
where
    M: Message,
{
    tx: tokio::sync::mpsc::Sender<Packet<M>>,
}

impl<M: Message> Postman<M> {
    pub fn new(tx: tokio::sync::mpsc::Sender<Packet<M>>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, message: M) -> Result<(), PostmanError> {
        let packet = Packet {
            message,
            reply_address: None,
        };
        self.tx
            .send(packet)
            .await
            .map_err(|err| PostmanError::PacketSendFailed(err.to_string()))?;
        Ok(())
    }

    pub async fn send_and_await_response(&self, message: M) -> Result<M::Response, PostmanError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<M::Response>();

        let packet = Packet {
            message,
            reply_address: Some(res_tx),
        };

        self.tx
            .send(packet)
            .await
            .map_err(|err| PostmanError::PacketSendFailed(err.to_string()))?;

        res_rx
            .await
            .map_err(|err| PostmanError::ResponseReceiveFailed(err.to_string()))
    }
}

#[derive(Debug)]
pub(crate) struct Mailbox<M>
where
    M: Message,
{
    rx: tokio::sync::mpsc::Receiver<Packet<M>>,
}

impl<M> Mailbox<M>
where
    M: Message,
{
    pub fn new(rx: tokio::sync::mpsc::Receiver<Packet<M>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<Packet<M>> {
        self.rx.recv().await
    }
}

#[derive(Debug, Clone, Message)]
pub(crate) enum ServiceMessage {
    Start,
    Stop,
    Pause,
    Resume,
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
