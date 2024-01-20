use std::{fmt, marker::PhantomData};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use crate::{
    errors::{CriticalError, PostmanError, PuppetError},
    executor::Executor,
    pid::Pid,
    puppet::{Handler, Lifecycle, Puppeter, ResponseFor},
};

pub trait Message: fmt::Debug + Send + 'static {}
impl<T> Message for T where T: fmt::Debug + Send + 'static {}

#[async_trait]
pub trait Envelope<P>: Send
where
    P: Lifecycle,
{
    async fn handle_message(&mut self, puppet: &mut P, puppeter: &mut Puppeter);
    async fn reply_error(&mut self, err: PuppetError);
}

pub type ReplySender<T> = oneshot::Sender<Result<T, PuppetError>>;
pub type ReplyReceiver<T> = oneshot::Receiver<Result<T, PuppetError>>;

pub struct Packet<P, E>
where
    P: Handler<E>,
    E: Message,
{
    message: Option<E>,
    reply_address: Option<ReplySender<ResponseFor<P, E>>>,
    _phantom: PhantomData<P>,
}

impl<P, E> Packet<P, E>
where
    P: Handler<E>,
    E: Message,
{
    pub const fn without_reply(message: E) -> Self {
        Self {
            message: Some(message),
            reply_address: None,
            _phantom: PhantomData,
        }
    }
    pub fn with_reply(
        message: E,
        reply_address: oneshot::Sender<Result<ResponseFor<P, E>, PuppetError>>,
    ) -> Self {
        Self {
            message: Some(message),
            reply_address: Some(reply_address),
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<P, E> Envelope<P> for Packet<P, E>
where
    P: Handler<E>,
    E: Message + 'static,
{
    async fn handle_message(&mut self, puppet: &mut P, puppeter: &mut Puppeter) {
        let msg = self.message.take().unwrap();
        let reply_address = self.reply_address.take();
        <P as Handler<E>>::Executor::execute(puppet, puppeter, msg, reply_address).await;
    }
    async fn reply_error(&mut self, err: PuppetError) {
        if let Some(reply_address) = self.reply_address.take() {
            let _ = reply_address.send(Err(err));
        }
    }
}

pub struct ServicePacket {
    pub(crate) cmd: Option<ServiceCommand>,
    pub(crate) reply_address: Option<oneshot::Sender<Result<(), PuppetError>>>,
}

impl ServicePacket {
    #[must_use]
    pub fn with_reply(
        cmd: ServiceCommand,
        reply_address: oneshot::Sender<Result<(), PuppetError>>,
    ) -> Self {
        Self {
            cmd: Some(cmd),
            reply_address: Some(reply_address),
        }
    }

    #[must_use]
    pub const fn without_reply(cmd: ServiceCommand) -> Self {
        Self {
            cmd: Some(cmd),
            reply_address: None,
        }
    }

    pub(crate) async fn handle_command<P>(&mut self, puppet: &mut P, puppeter: &mut Puppeter)
    where
        P: Lifecycle,
    {
        let cmd = self.cmd.take().unwrap();
        let reply_address = self.reply_address.take();
        let response = puppeter.handle_command(puppet, cmd).await;
        if let Err(err) = &response {
            match err {
                // Do nothing
                PuppetError::NonCritical(_) => {}
                PuppetError::Critical(_) => {
                    puppeter.report_failure(puppet, err.clone()).await;
                }
            }
        }
        if let Some(reply_address) = reply_address {
            if reply_address.send(response).is_err() {
                puppeter
                    .report_failure(
                        puppet,
                        CriticalError::new(
                            puppeter.pid,
                            "Failed to send response over the oneshot channel",
                        )
                        .into(),
                    )
                    .await;
            }
        }
    }
    pub(crate) async fn reply_error(&mut self, err: PuppetError) {
        if let Some(reply_address) = self.reply_address.take() {
            let _ = reply_address.send(Err(err));
        }
    }
}

#[derive(Debug, Clone, strum::Display, PartialEq, Eq)]
pub enum RestartStage {
    Start,
    Stop,
}

#[derive(Debug, Clone, strum::Display)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart { stage: Option<RestartStage> },
    ReportFailure { pid: Pid, error: PuppetError },
    Fail,
}

#[derive(Debug)]
pub struct Postman<P>
where
    P: Lifecycle,
{
    tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<P>>>,
}

impl<P> Clone for Postman<P>
where
    P: Lifecycle,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<P> Postman<P>
where
    P: Lifecycle,
{
    #[must_use]
    pub fn new(tx: tokio::sync::mpsc::Sender<Box<dyn Envelope<P>>>) -> Self {
        Self { tx }
    }

    #[inline(always)]
    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let packet = Packet::<P, E>::without_reply(message);
        self.tx.send(Box::new(packet)).await.map_err(|_e| {
            PostmanError::SendError {
                puppet: Pid::new::<P>(),
            }
        })?;
        Ok(())
    }

    #[inline(always)]
    pub async fn send_and_await_response<E>(
        &self,
        message: E,
        duration: Option<std::time::Duration>,
    ) -> Result<ResponseFor<P, E>, PostmanError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let (res_tx, res_rx) =
            tokio::sync::oneshot::channel::<Result<ResponseFor<P, E>, PuppetError>>();

        let packet = Packet::<P, E>::with_reply(message, res_tx);
        self.tx.send(Box::new(packet)).await.map_err(|_e| {
            PostmanError::SendError {
                puppet: Pid::new::<P>(),
            }
        })?;

        if let Some(duration) = duration {
            (tokio::time::timeout(duration, res_rx).await).map_or_else(
                |_| {
                    Err(PostmanError::ResponseReceiveError {
                        puppet: Pid::new::<P>(),
                    })
                },
                |inner_res| {
                    inner_res.map_or_else(
                        |_| {
                            Err(PostmanError::ResponseReceiveError {
                                puppet: Pid::new::<P>(),
                            })
                        },
                        |res| res.map_err(PostmanError::from),
                    )
                },
            )
        } else {
            (res_rx.await).map_or_else(
                |_| {
                    Err(PostmanError::ResponseReceiveError {
                        puppet: Pid::new::<P>(),
                    })
                },
                |res| res.map_err(PostmanError::from),
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServicePostman {
    tx: tokio::sync::mpsc::Sender<ServicePacket>,
}

impl ServicePostman {
    #[must_use]
    pub const fn new(tx: tokio::sync::mpsc::Sender<ServicePacket>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, puppet: Pid, command: ServiceCommand) -> Result<(), PostmanError> {
        let packet = ServicePacket::without_reply(command);
        self.tx
            .send(packet)
            .await
            .map_err(|_e| PostmanError::SendError { puppet })
    }

    pub async fn send_and_await_response(
        &self,
        puppet: Pid,
        command: ServiceCommand,
        duration: Option<std::time::Duration>,
    ) -> Result<(), PostmanError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel::<Result<(), PuppetError>>();
        let packet = ServicePacket::with_reply(command, res_tx);
        self.tx
            .send(packet)
            .await
            .map_err(|_e| PostmanError::SendError { puppet })?;

        if let Some(duration) = duration {
            (tokio::time::timeout(duration, res_rx).await).map_or(
                Err(PostmanError::ResponseReceiveError { puppet }),
                |inner_res| {
                    inner_res.map_or(Err(PostmanError::ResponseReceiveError { puppet }), |res| {
                        res.map_err(|e| e.into())
                    })
                },
            )
        } else {
            (res_rx.await).map_or(Err(PostmanError::ResponseReceiveError { puppet }), |res| {
                res.map_err(|e| e.into())
            })
        }
    }
}

pub(crate) struct Mailbox<P>
where
    P: Lifecycle,
{
    rx: mpsc::Receiver<Box<dyn Envelope<P>>>,
}

impl<P> fmt::Debug for Mailbox<P>
where
    P: Lifecycle,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox").field("rx", &self.rx).finish()
    }
}

impl<P> Mailbox<P>
where
    P: Lifecycle,
{
    pub fn new(rx: mpsc::Receiver<Box<dyn Envelope<P>>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<Box<dyn Envelope<P>>> {
        self.rx.recv().await
    }
}

#[derive(Debug)]
pub(crate) struct ServiceMailbox {
    rx: tokio::sync::mpsc::Receiver<ServicePacket>,
}

impl ServiceMailbox {
    pub const fn new(rx: tokio::sync::mpsc::Receiver<ServicePacket>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<ServicePacket> {
        self.rx.recv().await
    }
}
