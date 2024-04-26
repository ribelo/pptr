//! Defines message types and communication primitives for the puppet system.
//!
//! This module provides the core components for message-based communication between puppets
//! and the service that manages them. It defines traits for messages and envelopes, as well
//! as structs for packets and postmen.
//!
//! The main types and traits in this module include:
//!
//! - [`Message`]: A marker trait for types that can be used as messages.
//! - [`Envelope`]: A trait for message envelopes that can be handled by puppets.
//! - [`Packet`]: A struct representing a message packet with an optional reply address.
//! - [`ServicePacket`]: A struct representing a packet sent to a service for processing.
//! - [`Postman`]: A struct for sending messages to puppets.
//! - [`ServicePostman`]: A struct for sending commands to services.
//!
use std::{fmt, marker::PhantomData};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use crate::{
    errors::{PostmanError, PuppetError},
    executor::Executor,
    pid::Pid,
    prelude::CriticalError,
    puppet::{Context, Handler, Puppet, ResponseFor},
};

/// A marker trait for types that can be used as messages.
///
/// This trait is automatically implemented for any type that satisfies the following bounds:
/// - `fmt::Debug`: The type must implement the `Debug` trait for debugging and error reporting.
/// - `Send`: The type must be safe to send across thread boundaries.
/// - `'static`: The type must have a static lifetime.
///
/// By implementing this trait, a type indicates that it can be used as a message in a messaging
/// system or communication protocol.
///
pub trait Message: fmt::Debug + Send + 'static {}
impl<T> Message for T where T: fmt::Debug + Send + 'static {}

/// An envelope trait is the visitor pattern for message handling.
///
/// This trait allows sending messages that implement the `Envelope` trait,
/// enabling the sending of any type as a message.
///
/// The `handle_message` method takes a mutable reference to the puppet and the
/// context on which the message should be executed.
///
/// The `reply_error` method is a convenience function for sending an error as a
/// response.
#[async_trait]
pub trait Envelope<P>: Send
where
    P: Puppet,
{
    /// Handles the message using the provided puppet and context.
    async fn handle_message(&mut self, puppet: &mut P, ctx: &mut Context<P>);
    /// Sends an error as a response using the provided context.
    async fn reply_error(&mut self, ctx: &Context<P>, err: PuppetError);
}

/// A type alias for a one-shot sender used to send a reply.
///
/// The `ReplySender` is a type alias for `oneshot::Sender` that sends a `Result` containing either
/// the response of type `T` or a `PuppetError`.
pub type ReplySender<T> = oneshot::Sender<Result<T, PuppetError>>;
/// A type alias for a one-shot receiver used to receive a reply.
///
/// The `ReplyReceiver` is a type alias for `oneshot::Receiver` that receives a `Result` containing
/// either the response of type `T` or a `PuppetError`.
pub type ReplyReceiver<T> = oneshot::Receiver<Result<T, PuppetError>>;

/// Represents a packet that wraps a message and specifies its type and reply address.
///
/// The `Packet` struct is used to encapsulate a message of type `E` along with an optional reply
/// address. It is generic over two type parameters:
/// - `P`: The handler type that can handle the message type `E`.
/// - `E`: The message type that implements the `Message` trait.
///
/// The packet can be created with or without a reply address using the provided constructor methods.
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
    /// Creates a new `Packet` without a reply address.
    ///
    /// This method is a shorthand for creating a packet that does not expect a response.
    ///
    /// # Returns
    ///
    /// A new `Packet` instance containing the provided message and no reply address.
    #[must_use]
    pub fn without_reply(message: E) -> Self {
        Self {
            message: Some(message),
            reply_address: None,
            _phantom: PhantomData,
        }
    }

    /// Creates a new `Packet` with a reply address.
    ///
    /// This method is a shorthand for creating a packet that expects a response.
    ///
    /// # Returns
    ///
    /// A new `Packet` instance containing the provided message and reply address.
    #[must_use]
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
    async fn handle_message(&mut self, puppet: &mut P, ctx: &mut Context<P>) {
        if let Some(msg) = self.message.take() {
            let reply_address = self.reply_address.take();
            dbg!("4");
            if let Err(err) =
                <P as Handler<E>>::Executor::execute(puppet, ctx, msg, reply_address).await
            {
                dbg!("5");
                self.reply_error(ctx, err).await;
            }
        } else {
            let err = ctx.critical_error("Packet has no message");
            self.reply_error(ctx, err).await;
        }
    }
    async fn reply_error(&mut self, ctx: &Context<P>, err: PuppetError) {
        if let Some(reply_address) = self.reply_address.take() {
            if reply_address.send(Err(err)).is_err() {
                let err =
                    CriticalError::new(ctx.pid, "Failed to send response over the oneshot channel");
                ctx.report_unrecoverable_failure(err);
            }
        }
    }
}

/// Represents a packet of data sent to a service for processing.
///
/// A `ServicePacket` contains an `ServiceCommand` and an reply address.
/// The reply address is used to send a response back to the sender of the packet.
///
/// The `cmd` field holds the command to be executed by the service.
/// The `reply_address` field contains a `Sender` for sending the result of the command
/// back to the caller, if a reply is expected.
pub struct ServicePacket {
    pub(crate) cmd: Option<ServiceCommand>,
    pub(crate) reply_address: Option<oneshot::Sender<Result<(), PuppetError>>>,
}

impl ServicePacket {
    #[must_use]
    /// Creates a new `ServicePacket` with a reply address.
    ///
    /// This method is a shorthand for creating a service packet that expects a response.
    ///
    /// # Returns
    ///
    /// A new `ServicePacket` instance containing the provided command and reply address.
    pub fn with_reply(
        cmd: ServiceCommand,
        reply_address: oneshot::Sender<Result<(), PuppetError>>,
    ) -> Self {
        Self {
            cmd: Some(cmd),
            reply_address: Some(reply_address),
        }
    }

    /// Creates a new `ServicePacket` without a reply address.
    ///
    /// This method is a shorthand for creating a service packet that does not expect a response.
    ///
    /// # Returns
    ///
    /// A new `ServicePacket` instance containing the provided command and no reply address.
    #[must_use]
    pub fn without_reply(cmd: ServiceCommand) -> Self {
        Self {
            cmd: Some(cmd),
            reply_address: None,
        }
    }

    /// Handles the command contained in the `ServicePacket`.
    ///
    /// This method takes ownership of the command and reply address stored in the `ServicePacket`,
    /// handles the command using the provided `puppet` and `ctx`, and sends the response back
    /// through the reply address.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if:
    /// - The `ServicePacket` has no command.
    /// - The `ServicePacket` has no reply address.
    /// - Sending the response over the oneshot channel fails.
    ///
    /// If the command handling results in a critical error, it is reported using `ctx.report_failure()`.
    pub(crate) async fn handle_command<P>(
        &mut self,
        puppet: &mut P,
        ctx: &mut Context<P>,
    ) -> Result<(), PuppetError>
    where
        P: Puppet,
    {
        let cmd = self
            .cmd
            .take()
            .ok_or_else(|| PuppetError::critical(ctx.pid, "ServicePacket has no command"))?;

        let reply_address = self
            .reply_address
            .take()
            .ok_or_else(|| PuppetError::critical(ctx.pid, "ServicePacket has no reply address"))?;

        let response = ctx.handle_command(puppet, cmd).await;

        if let Err(PuppetError::Critical(err)) = &response {
            ctx.report_failure(puppet, err.clone()).await?;
        }

        reply_address.send(response).map_err(|_err| {
            PuppetError::critical(ctx.pid, "Failed to send response over the oneshot channel")
        })?;

        Ok(())
    }

    /// Replies with an error to the sender of the `ServicePacket`.
    ///
    /// If the `ServicePacket` has a reply address, this method sends the provided error
    /// over the oneshot channel. If the send operation fails, the error is silently ignored.
    pub(crate) fn reply_error(&mut self, err: PuppetError) {
        if let Some(reply_address) = self.reply_address.take() {
            let _ = reply_address.send(Err(err));
        }
    }
}

/// Represents the stages of a restart operation.
///
/// - `Start`: Indicates the start of the restart process.
/// - `Stop`: Indicates the stop phase of the restart process.
#[derive(Debug, Clone, strum::Display, PartialEq, Eq)]
pub enum RestartStage {
    Start,
    Stop,
}

/// Represents the commands that can be sent to a service.
///
/// - `Start`: Starts the puppet.
/// - `Stop`: Stops the puppet.
/// - `Restart`: Restarts the puppet. The `stage` field indicates the current stage of the restart process.
/// - `ReportFailure`: Reports a failure in the service identified by `pid` with the given `error`.
/// - `Fail`: Indicates a failure in the puppet.
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
    P: Puppet,
{
    tx: tokio::sync::mpsc::UnboundedSender<Box<dyn Envelope<P>>>,
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
    #[must_use]
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<Box<dyn Envelope<P>>>) -> Self {
        Self { tx }
    }

    pub(crate) fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let packet = Packet::<P, E>::without_reply(message);
        self.tx.send(Box::new(packet)).map_err(|_e| {
            PostmanError::SendError {
                puppet: Pid::new::<P>(),
            }
        })?;
        Ok(())
    }

    pub(crate) async fn send_and_await_response<E>(
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
        self.tx.send(Box::new(packet)).map_err(|_e| {
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
    pub fn new(tx: tokio::sync::mpsc::Sender<ServicePacket>) -> Self {
        Self { tx }
    }

    pub(crate) async fn send(
        &self,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PostmanError> {
        let packet = ServicePacket::without_reply(command);
        self.tx
            .send(packet)
            .await
            .map_err(|_e| PostmanError::SendError { puppet })
    }

    pub(crate) async fn send_and_await_response(
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
                        res.map_err(PostmanError::from)
                    })
                },
            )
        } else {
            (res_rx.await).map_or(Err(PostmanError::ResponseReceiveError { puppet }), |res| {
                res.map_err(PostmanError::from)
            })
        }
    }
}

pub(crate) struct Mailbox<P>
where
    P: Puppet,
{
    rx: mpsc::UnboundedReceiver<Box<dyn Envelope<P>>>,
}

impl<P> fmt::Debug for Mailbox<P>
where
    P: Puppet,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox").field("rx", &self.rx).finish()
    }
}

impl<P> Mailbox<P>
where
    P: Puppet,
{
    pub fn new(rx: mpsc::UnboundedReceiver<Box<dyn Envelope<P>>>) -> Self {
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
    pub fn new(rx: tokio::sync::mpsc::Receiver<ServicePacket>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<ServicePacket> {
        self.rx.recv().await
    }
}
