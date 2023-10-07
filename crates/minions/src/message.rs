use std::fmt::Debug;

use minions_derive::Message;
use thiserror::Error;
use tokio::sync::oneshot;

use crate::MinionsError;

pub trait Message: Send {
    type Response: Send + 'static;
}

#[derive(Debug, Clone, Message, strum::Display)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    Terminate,
}

impl Copy for ServiceCommand {}

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
    pub(crate) reply_address: Option<oneshot::Sender<Result<M::Response, MinionsError>>>,
}

pub(crate) struct Postman<M>
where
    M: Message,
{
    tx: tokio::sync::mpsc::Sender<Packet<M>>,
}

impl<M> Clone for Postman<M>
where
    M: Message,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<M> Postman<M>
where
    M: Message + 'static,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<Packet<M>>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, message: M) -> Result<(), MinionsError> {
        let packet = Packet {
            message,
            reply_address: None,
        };
        self.tx
            .send(packet)
            .await
            .map_err(|_| MinionsError::MessageSendError)?;
        Ok(())
    }

    pub async fn send_and_await_response(&self, message: M) -> Result<M::Response, MinionsError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<M::Response, MinionsError>>();

        let packet = Packet {
            message,
            reply_address: Some(res_tx),
        };

        self.tx
            .send(packet)
            .await
            .map_err(|_| MinionsError::MessageSendError)?;

        match res_rx.await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(MinionsError::MessageResponseReceiveError),
        }
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
