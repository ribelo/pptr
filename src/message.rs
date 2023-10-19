use std::{
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::minion::{self, Handler, Minion};
use async_trait::async_trait;
#[cfg(feature = "rayon")]
use pollster::FutureExt;
#[cfg(feature = "rayon")]
use rayon;
use tokio::sync::oneshot;

use crate::MinionsError;

pub trait Message: Send {}

#[derive(Debug, Clone, strum::Display)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    Terminate,
}

impl Copy for ServiceCommand {}

impl Message for ServiceCommand {}

pub type ReplyAddress<T> = oneshot::Sender<Result<T, MinionsError>>;
pub type MaybeReplyAddress<T> = Option<ReplyAddress<T>>;
pub type MessageResponse<A, H> = <H as Handler<A>>::Response;

#[async_trait]
pub trait Envelope<A: Minion>: Send {
    async fn handle_message(&mut self, minion: &mut A) -> Result<(), MinionsError>;
    async fn reply_error(&mut self, err: MinionsError) -> Result<(), MinionsError>;
}

pub struct Packet<A, M>
where
    A: Handler<M>,
    M: Message,
{
    message: Option<M>,
    reply_address: Option<oneshot::Sender<Result<A::Response, MinionsError>>>,
    _phantom: PhantomData<A>,
}

impl<A, M> Packet<A, M>
where
    A: Handler<M>,
    M: Message,
{
    pub fn without_reply(message: M) -> Self {
        Self {
            message: Some(message),
            reply_address: None,
            _phantom: PhantomData,
        }
    }
    pub fn with_reply(
        message: M,
        reply_address: oneshot::Sender<Result<A::Response, MinionsError>>,
    ) -> Self {
        Self {
            message: Some(message),
            reply_address: Some(reply_address),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<A, M> Envelope<A> for Packet<A, M>
where
    A: Handler<M>,
    M: Message + 'static,
{
    async fn handle_message(&mut self, minion: &mut A) -> Result<(), MinionsError> {
        let execution_variant = minion::execution::ExecutionVariant::from_type::<A::Exec>();
        let msg = self.message.take().unwrap();
        let reply_address = self.reply_address.take();
        match execution_variant {
            minion::execution::ExecutionVariant::Sequential => {
                let response = minion.handle_message(&msg).await;
                if let Some(reply_address) = reply_address {
                    reply_address
                        .send(Ok(response))
                        .unwrap_or_else(|_| println!("Message response send error"));
                }
            }
            minion::execution::ExecutionVariant::Concurrent => {
                let mut cloned_minion = minion.clone();
                tokio::spawn(async move {
                    let response = cloned_minion.handle_message(&msg).await;
                    if let Some(reply_address) = reply_address {
                        reply_address
                            .send(Ok(response))
                            .unwrap_or_else(|_| println!("Message response send error"));
                    };
                });
            }
            #[cfg(feature = "rayon")]
            minion::execution::ExecutionVariant::Parallel => {
                let mut cloned_minion = minion.clone();
                rayon::spawn(move || {
                    let response = cloned_minion.handle_message(&msg).block_on();
                    if let Some(reply_address) = reply_address {
                        reply_address
                            .send(Ok(response))
                            .unwrap_or_else(|_| println!("Message response send error"));
                    };
                });
            }
        };
        Ok(())
    }
    async fn reply_error(&mut self, err: MinionsError) -> Result<(), MinionsError> {
        if let Some(reply_address) = self.reply_address.take() {
            reply_address
                .send(Err(err))
                .unwrap_or_else(|_| println!("Message response send error"));
        }
        Ok(())
    }
}

pub(crate) struct Postman<A>
where
    A: Minion,
{
    tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<A>>>,
}

impl<A> Clone for Postman<A>
where
    A: Minion,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<A> Postman<A>
where
    A: Minion,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<A>>>) -> Self {
        Self { tx }
    }

    #[inline(always)]
    pub async fn send<M>(&self, message: M) -> Result<(), MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let packet = Packet::without_reply(message);
        self.tx
            .send(Box::new(packet))
            .await
            .map_err(|_| MinionsError::MessageSendError)?;
        Ok(())
    }

    #[inline(always)]
    pub async fn send_and_await_response<M>(&self, message: M) -> Result<A::Response, MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<A::Response, MinionsError>>();

        let packet = Packet::with_reply(message, res_tx);
        self.tx
            .send(Box::new(packet))
            .await
            .map_err(|_| MinionsError::MessageSendError)?;

        match res_rx.await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(MinionsError::MessageResponseReceiveError),
        }
    }
}

pub struct ServicePacket {
    pub(crate) cmd: ServiceCommand,
    pub(crate) reply_address: oneshot::Sender<Result<(), MinionsError>>,
}

#[derive(Clone)]
pub(crate) struct ServicePostman {
    tx: tokio::sync::mpsc::Sender<ServicePacket>,
}

impl ServicePostman {
    pub fn new(tx: tokio::sync::mpsc::Sender<ServicePacket>) -> Self {
        Self { tx }
    }

    pub async fn send_and_await_response(
        &self,
        command: ServiceCommand,
    ) -> Result<(), MinionsError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<(), MinionsError>>();
        let packet = ServicePacket {
            cmd: command,
            reply_address: res_tx,
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

pub(crate) struct Mailbox<A>
where
    A: Minion,
{
    rx: tokio::sync::mpsc::Receiver<Box<dyn Envelope<A>>>,
}

impl<A: Minion> fmt::Debug for Mailbox<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox").field("rx", &self.rx).finish()
    }
}

impl<A> Mailbox<A>
where
    A: Minion,
{
    pub fn new(rx: tokio::sync::mpsc::Receiver<Box<dyn Envelope<A>>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<Box<dyn Envelope<A>>>
    where
        A: Minion,
    {
        self.rx.recv().await
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMailbox {
    rx: tokio::sync::mpsc::Receiver<ServicePacket>,
}

impl ServiceMailbox {
    pub fn new(rx: tokio::sync::mpsc::Receiver<ServicePacket>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<ServicePacket> {
        self.rx.recv().await
    }
}
