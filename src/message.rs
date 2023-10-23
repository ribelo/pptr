use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

use crate::{
    errors::{PostmanError, PuppetError},
    master::puppeter,
    puppet::{self, Handler, Puppet},
    Id,
};
use async_trait::async_trait;
#[cfg(feature = "rayon")]
use pollster::FutureExt;
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
    async fn reply_error(&mut self, err: PuppetError);
}

pub struct Packet<P, M>
where
    P: Handler<M>,
    M: Message,
{
    message: Option<M>,
    reply_address: Option<oneshot::Sender<Result<P::Response, PuppetError>>>,
    _phantom: PhantomData<P>,
}

impl<P, M> Packet<P, M>
where
    P: Handler<M>,
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
        reply_address: oneshot::Sender<Result<P::Response, PuppetError>>,
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
        let execution_variant = puppet::execution::ExecutionVariant::from_type::<P::Exec>();
        let msg = self.message.take().unwrap();
        let reply_address = self.reply_address.take();
        match execution_variant {
            puppet::execution::ExecutionVariant::Sequential => {
                let response = puppet.handle_message(msg).await;
                if let Err(error) = &response {
                    puppeter().report_failure::<P>(error).await;
                }
                if let Some(reply_address) = reply_address {
                    reply_address
                        .send(response)
                        .unwrap_or_else(|_| println!("Message response send error"));
                }
            }
            puppet::execution::ExecutionVariant::Concurrent => {
                let mut cloned_minion = puppet.clone();
                tokio::spawn(async move {
                    let response = cloned_minion.handle_message(msg).await;
                    if let Err(error) = &response {
                        puppeter().report_failure::<P>(error).await;
                    }
                    if let Some(reply_address) = reply_address {
                        reply_address
                            .send(response)
                            .unwrap_or_else(|_| println!("Message response send error"));
                    };
                });
            }
            #[cfg(feature = "rayon")]
            puppet::execution::ExecutionVariant::Parallel => {
                let mut cloned_minion = puppet.clone();
                rayon::spawn(move || {
                    let response = cloned_minion.handle_message(msg).block_on();
                    if let Err(error) = &response {
                        puppeter().report_failure::<P>(error).block_on();
                    }
                    if let Some(reply_address) = reply_address {
                        reply_address
                            .send(response)
                            .unwrap_or_else(|_| println!("Message response send error"));
                    };
                });
            }
        };
    }
    async fn reply_error(&mut self, err: PuppetError) {
        if let Some(reply_address) = self.reply_address.take() {
            let _ = reply_address.send(Err(err));
        }
    }
}

#[derive(Debug)]
pub(crate) struct Postman<A>
where
    A: Puppet,
{
    tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<A>>>,
}

impl<A> Clone for Postman<A>
where
    A: Puppet,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<A> Postman<A>
where
    A: Puppet,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<A>>>) -> Self {
        Self { tx }
    }

    #[inline(always)]
    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        A: Handler<E>,
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
    pub async fn send_and_await_response<E>(&self, message: E) -> Result<A::Response, PostmanError>
    where
        A: Handler<E>,
        E: Message + 'static,
    {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<A::Response, PuppetError>>();

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

pub struct ServicePacket {
    pub(crate) cmd: ServiceCommand,
    pub(crate) reply_address: oneshot::Sender<Result<(), PuppetError>>,
}

#[derive(Debug, Clone)]
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
    ) -> Result<(), PostmanError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<(), PuppetError>>();
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
