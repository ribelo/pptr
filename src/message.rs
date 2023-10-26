use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

use crate::{
    errors::{PostmanError, PuppetError},
    executor::Executor,
    master::puppeter,
    puppet::{self, Handler, Puppet},
    Id,
};
use async_trait::async_trait;
#[cfg(feature = "rayon")]
use rayon;
use tokio::sync::{mpsc, oneshot};

pub trait Message: Send + 'static {}

#[derive(Debug, Clone, strum::Display)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    Kill,
}

impl Message for ServiceCommand {}

#[async_trait]
pub trait Envelope<P: Puppet>: Send {
    async fn handle_message(&mut self, puppet: &mut P);
    async fn reply_error(&mut self, err: PuppetError<P>);
}

pub struct Packet<P, E>
where
    P: Handler<E>,
    E: Message,
{
    message: Option<E>,
    reply_address: Option<oneshot::Sender<Result<P::Response, PuppetError<P>>>>,
    _phantom: PhantomData<P>,
}

impl<P, E> Packet<P, E>
where
    P: Handler<E>,
    E: Message,
{
    pub fn without_reply(message: E) -> Self {
        Self {
            message: Some(message),
            reply_address: None,
            _phantom: PhantomData,
        }
    }
    pub fn with_reply(
        message: E,
        reply_address: oneshot::Sender<Result<P::Response, PuppetError<P>>>,
    ) -> Self {
        Self {
            message: Some(message),
            reply_address: Some(reply_address),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<P, M> Envelope<P> for Packet<P, M>
where
    P: Handler<M>,
    M: Message + 'static,
{
    async fn handle_message(&mut self, puppet: &mut P) {
        let msg = self.message.take().unwrap();
        let reply_address = self.reply_address.take();
        P::Executor::execute_message(puppet, msg, reply_address).await
    }
    async fn reply_error(&mut self, err: PuppetError<P>) {
        if let Some(reply_address) = self.reply_address.take() {
            let _ = reply_address.send(Err(err));
        }
    }
}

#[derive(Debug)]
pub(crate) struct Postman<P>
where
    P: Puppet,
{
    tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<P>>>,
}

impl<P> Clone for Postman<P>
where
    P: Puppet,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<P> Postman<P>
where
    P: Puppet,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<P>>>) -> Self {
        Self { tx }
    }

    #[inline(always)]
    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError<P>>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let packet = Packet::without_reply(message);
        self.tx
            .send(Box::new(packet))
            .await
            .map_err(|_| PostmanError::SendError)?;
        Ok(())
    }

    #[inline(always)]
    pub async fn send_and_await_response<E>(
        &self,
        message: E,
    ) -> Result<P::Response, PostmanError<P>>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let (res_tx, res_rx) =
            tokio::sync::oneshot::channel::<Result<P::Response, PuppetError<P>>>();

        let packet = Packet::with_reply(message, res_tx);
        self.tx
            .send(Box::new(packet))
            .await
            .map_err(|_| PostmanError::SendError)?;

        match res_rx.await {
            Ok(response) => response.map_err(|e| e.into()),
            Err(_) => Err(PostmanError::ResponseReceiveError),
        }
    }
}

pub struct ServicePacket<P>
where
    P: Puppet,
{
    pub(crate) cmd: ServiceCommand,
    pub(crate) reply_address: oneshot::Sender<Result<(), PuppetError<P>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ServicePostman<P>
where
    P: Puppet,
{
    tx: tokio::sync::mpsc::Sender<ServicePacket<P>>,
}

impl<P> ServicePostman<P>
where
    P: Puppet,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<ServicePacket<P>>) -> Self {
        Self { tx }
    }

    pub async fn send_and_await_response(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PostmanError<P>> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<(), PuppetError<P>>>();
        let packet = ServicePacket {
            cmd: command,
            reply_address: res_tx,
        };
        self.tx
            .send(packet)
            .await
            .map_err(|_| PostmanError::SendError)?;

        match res_rx.await {
            Ok(response) => response.map_err(|e| e.into()),
            Err(_) => Err(PostmanError::ResponseReceiveError),
        }
    }
}

pub(crate) struct Mailbox<A>
where
    A: Puppet,
{
    rx: mpsc::Receiver<Box<dyn Envelope<A>>>,
}

impl<A: Puppet> fmt::Debug for Mailbox<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox").field("rx", &self.rx).finish()
    }
}

impl<A> Mailbox<A>
where
    A: Puppet,
{
    pub fn new(rx: mpsc::Receiver<Box<dyn Envelope<A>>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<Box<dyn Envelope<A>>>
    where
        A: Puppet,
    {
        self.rx.recv().await
    }
    pub async fn cleanup(&mut self) {
        let duration = std::time::Duration::from_millis(100);
        while let Ok(Some(_)) = tokio::time::timeout(duration, self.recv()).await {}
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMailbox<P: Puppet> {
    rx: tokio::sync::mpsc::Receiver<ServicePacket<P>>,
}

impl<P> ServiceMailbox<P>
where
    P: Puppet,
{
    pub fn new(rx: tokio::sync::mpsc::Receiver<ServicePacket<P>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<ServicePacket<P>> {
        self.rx.recv().await
    }
}
