use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use minions_derive::Message;
use tokio::sync::oneshot;

use crate::MinionsError;

pub trait Message: Send {
    type Response: Send + 'static;
}

#[derive(Debug, Clone)]
pub struct Msg<T>(pub T);

impl<T: Message> Deref for Msg<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Message> DerefMut for Msg<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Message, strum::Display)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    Terminate,
}

impl Copy for ServiceCommand {}

pub type ReplyAddress<T> = oneshot::Sender<Result<T, MinionsError>>;
pub type MaybeReplyAddress<T> = Option<ReplyAddress<T>>;
pub type MessageResponse<T> = <T as Message>::Response;
pub type MinionMessageResponse<M> = MessageResponse<<M as Message>::Response>;

pub(crate) struct Packet<M: Message> {
    pub message: M,
    pub(crate) reply_address: MaybeReplyAddress<M::Response>,
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

    #[inline(always)]
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

    #[inline(always)]
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
