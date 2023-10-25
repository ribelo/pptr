use thiserror::Error;

use crate::{
    master::{puppeter, Master, Puppeter},
    prelude::Puppet,
    puppet::LifecycleStatus,
    PuppetIdentifier,
};

pub trait ReportFailure<P>
where
    P: Puppet,
{
    fn report_failure(&self) {
        puppeter().report_failure::<P>(self)
    }

    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet;
}

#[derive(Debug, Error)]
#[error("Puppet does not exist: {name}")]
pub struct PuppetDoesNotExist<P>
where
    P: Puppet,
{
    pub name: String,
    _phantom: std::marker::PhantomData<P>,
}

impl<P> ReportFailure<P> for PuppetDoesNotExist<P> {
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        tracing::warn!(name = %self.name, "Puppet does not exist.");
    }
}

#[derive(Debug, Error)]
#[error("Puppet already exist: {name}")]
pub struct PuppetAlreadyExist<P>
where
    P: Puppet,
{
    pub name: String,
    _phantom: std::marker::PhantomData<P>,
}

impl<P> ReportFailure<P> for PuppetAlreadyExist<P> {
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        tracing::warn!(name = %self.name, "Puppet already exist.");
    }
}

#[derive(Debug, Error)]
#[error("Permission denied: {message}")]
pub struct PermissionDenied<M, P>
where
    M: Master,
    P: Puppet,
{
    pub master: String,
    pub puppet: String,
    pub message: String,
    _phantom: std::marker::PhantomData<(M, P)>,
}

impl<M, P> ReportFailure<P> for PermissionDenied<M, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        tracing::warn!(
            master = %self.master,
            puppet = %self.puppet,
            message = %self.message,
            "Permission denied."
        );
    }
}

impl<M: Master, P: Puppet> PermissionDenied<M, P> {
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

#[derive(Debug, Error)]
#[error("Puppet {name} cannot handle message. Status: {status}.")]
pub struct PuppetCannotHandleMessage<P>
where
    P: Puppet,
{
    pub name: String,
    pub status: LifecycleStatus,
    _phantom: std::marker::PhantomData<P>,
}

impl<P> ReportFailure<P> for PuppetCannotHandleMessage<P> {
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        tracing::warn!(
            name = %self.name,
            status = ?self.status,
            "Puppet cannot handle message."
        );
    }
}

#[derive(Debug, Error)]
pub enum StartPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PostmanError(#[from] PostmanError<P>),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied<M, P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<M, P> ReportFailure<P> for StartPuppetError<M, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            StartPuppetError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            StartPuppetError::PostmanError(e) => e.handle_error(puppeter),
            StartPuppetError::PermissionDenied(e) => e.handle_error(puppeter),
            StartPuppetError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

#[derive(Debug, Error)]
pub enum StopPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PostmanError(#[from] PostmanError<P>),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied<M, P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<M, P> ReportFailure<P> for StopPuppetError<M, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            StopPuppetError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            StopPuppetError::PostmanError(e) => e.handle_error(puppeter),
            StopPuppetError::PermissionDenied(e) => e.handle_error(puppeter),
            StopPuppetError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

#[derive(Debug, Error)]
pub enum ResetPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PostmanError(#[from] PostmanError<P>),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied<M, P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<M, P> ReportFailure<P> for ResetPuppetError<M, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            ResetPuppetError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            ResetPuppetError::PostmanError(e) => e.handle_error(puppeter),
            ResetPuppetError::PermissionDenied(e) => e.handle_error(puppeter),
            ResetPuppetError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

impl<M, P> From<StopPuppetError<M, P>> for ResetPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    fn from(value: StopPuppetError<M, P>) -> Self {
        match value {
            StopPuppetError::PuppetDoesNotExist(e) => ResetPuppetError::PuppetDoesNotExist(e),
            StopPuppetError::PostmanError(e) => ResetPuppetError::PostmanError(e),
            StopPuppetError::PermissionDenied(e) => ResetPuppetError::PermissionDenied(e),
            StopPuppetError::PuppetError(e) => ResetPuppetError::PuppetError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum KillPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PostmanError(#[from] PostmanError<P>),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied<M, P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<M, P> ReportFailure<P> for KillPuppetError<M, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            KillPuppetError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            KillPuppetError::PostmanError(e) => e.handle_error(puppeter),
            KillPuppetError::PermissionDenied(e) => e.handle_error(puppeter),
            KillPuppetError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

impl<M, P> From<StopPuppetError<M, P>> for KillPuppetError<M, P>
where
    M: Master,
    P: Puppet,
{
    fn from(value: StopPuppetError<M, P>) -> Self {
        match value {
            StopPuppetError::PuppetDoesNotExist(e) => KillPuppetError::PuppetDoesNotExist(e),
            StopPuppetError::PostmanError(e) => KillPuppetError::PostmanError(e),
            StopPuppetError::PermissionDenied(e) => KillPuppetError::PermissionDenied(e),
            StopPuppetError::PuppetError(e) => KillPuppetError::PuppetError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum IdentificationError {
    #[error("Puppet already exists: {0}")]
    PuppetAlreadyExists(PuppetIdentifier),
    #[error("Puppet does not exist: {0}")]
    PuppetDoesNotExist(PuppetIdentifier),
}

impl ReportFailure<Puppeter> for IdentificationError {
    fn handle_error(&self, puppeter: &Puppeter) {
        match self {
            IdentificationError::PuppetAlreadyExists(id) => {
                tracing::warn!(id = %id, "Puppet already exists.")
            }
            IdentificationError::PuppetDoesNotExist(id) => {
                tracing::warn!(id = %id, "Puppet does not exist.")
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum PuppetError<P: Puppet> {
    #[error("Non-critical error occurred: '{0}'. Supervisor will not be notified.")]
    NonCritical(String),

    #[error("Critical error occurred: '{0}'. Supervisor will be notified.")]
    Critical(String),

    #[error("Fatal error occurred: '{0}'. Supervisor will be notified.")]
    Fatal(String),

    #[error(transparent)]
    PuppetCannotHandleMessage(#[from] PuppetCannotHandleMessage<P>),
}

impl<P> ReportFailure<P> for PuppetError<P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        // TODO:
        match self {
            PuppetError::NonCritical(e) => tracing::warn!(%e, "Non-critical error occurred."),
            PuppetError::Critical(e) => tracing::error!(%e, "Critical error occurred."),
            PuppetError::Fatal(e) => tracing::error!(%e, "Fatal error occurred."),
            PuppetError::PuppetCannotHandleMessage(e) => e.handle_error(puppeter),
        }
    }
}

impl<P> From<PuppetError<P>> for String
where
    P: Puppet,
{
    fn from(value: PuppetError<P>) -> Self {
        match value {
            PuppetError::NonCritical(s) => s,
            PuppetError::Critical(s) => s,
            PuppetError::Fatal(s) => s,
            PuppetError::PuppetCannotHandleMessage(e) => e.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppeterSendMessageError<P>
where
    P: Puppet,
{
    #[error("Puppet {name} send channel closed.")]
    PuppetSendChannelClosed { name: String },
    #[error("Puppet {name} receive channel closed.")]
    PuppetReceiveChannelClosed { name: String },
    #[error("Puppet {name} response channel closed.")]
    PuppetResponseChannelClosed { name: String },
    #[error("Puppet {name} response timeout.")]
    RequestTimeout { name: String },
    #[error("Puppet {name} does not exist.")]
    PuppetDoesNotExist { name: String },
    #[error(transparent)]
    PuppetCannotHandleMessage(#[from] PuppetCannotHandleMessage<P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<P> ReportFailure<P> for PuppeterSendMessageError<P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            PuppeterSendMessageError::PuppetSendChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet send channel closed.")
            }
            PuppeterSendMessageError::PuppetReceiveChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet receive channel closed.")
            }
            PuppeterSendMessageError::PuppetResponseChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet response channel closed.")
            }
            PuppeterSendMessageError::RequestTimeout { name } => {
                tracing::warn!(name = %name, "Puppet response timeout.")
            }
            PuppeterSendMessageError::PuppetDoesNotExist { name } => {
                tracing::warn!(name = %name, "Puppet does not exist.")
            }
            PuppeterSendMessageError::PuppetCannotHandleMessage(e) => e.handle_error(puppeter),
            PuppeterSendMessageError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

#[derive(Debug, Error)]
pub enum PostmanError<P> {
    #[error("Can't send message. Channel closed.")]
    SendError,
    #[error("Can't receive message. Channel closed.")]
    ReceiveError,
    #[error("Can't receive response. Channel closed.")]
    ResponseReceiveError,
    #[error("Can't reveive response. Response timeout.")]
    ResponseTimeout,
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
}

impl<P> ReportFailure<P> for PostmanError<P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            PostmanError::SendError => tracing::warn!("Can't send message. Channel closed."),
            PostmanError::ReceiveError => tracing::warn!("Can't receive message. Channel closed."),
            PostmanError::ResponseReceiveError => {
                tracing::warn!("Can't receive response. Channel closed.")
            }
            PostmanError::ResponseTimeout => {
                tracing::warn!("Can't reveive response. Response timeout.")
            }
            PostmanError::PuppetError(e) => e.handle_error(puppeter),
        }
    }
}

impl<P> From<(PostmanError<P>, String)> for PuppeterSendMessageError<P>
where
    P: Puppet,
{
    fn from(err: (PostmanError<P>, String)) -> Self {
        let (postman_error, name) = err;
        match postman_error {
            PostmanError::SendError => PuppeterSendMessageError::PuppetSendChannelClosed { name },
            PostmanError::ReceiveError => {
                PuppeterSendMessageError::PuppetReceiveChannelClosed { name }
            }
            PostmanError::ResponseReceiveError => {
                PuppeterSendMessageError::PuppetResponseChannelClosed { name }
            }
            PostmanError::ResponseTimeout => PuppeterSendMessageError::RequestTimeout { name },
            PostmanError::PuppetError(e) => PuppeterSendMessageError::PuppetError(e),
        }
    }
}

impl<M, P> From<(PostmanError<P>, String)> for PuppeterSendCommandError<M, P>
where
    M: Master,
    P: Puppet,
{
    fn from(err: (PostmanError<P>, String)) -> Self {
        let (postman_error, name) = err;
        match postman_error {
            PostmanError::SendError => PuppeterSendCommandError::PuppetSendChannelClosed { name },
            PostmanError::ReceiveError => {
                PuppeterSendCommandError::PuppetReceiveChannelClosed { name }
            }
            PostmanError::ResponseReceiveError => {
                PuppeterSendCommandError::PuppetResponseChannelClosed { name }
            }
            PostmanError::ResponseTimeout => unreachable!(),
            PostmanError::PuppetError(e) => PuppeterSendCommandError::PuppetError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppeterSpawnError<P>
where
    P: Puppet,
{
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PuppetAlreadyExist(#[from] PuppetAlreadyExist<P>),
}

impl<P> ReportFailure<P> for PuppeterSpawnError<P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            PuppeterSpawnError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            PuppeterSpawnError::PuppetAlreadyExist(e) => e.handle_error(puppeter),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppeterSendCommandError<M, P>
where
    M: Master,
    P: Puppet,
{
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied<M, P>),
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist<P>),
    #[error(transparent)]
    PuppetError(#[from] PuppetError<P>),
    #[error("Puppet {name} send channel closed.")]
    PuppetSendChannelClosed { name: String },
    #[error("Puppet {name} receive channel closed.")]
    PuppetReceiveChannelClosed { name: String },
    #[error("Puppet {name} response channel closed.")]
    PuppetResponseChannelClosed { name: String },
}

impl<P> ReportFailure<P> for PuppeterSendCommandError<Puppeter, P>
where
    P: Puppet,
{
    fn handle_error(&self, puppeter: &Puppeter)
    where
        P: Puppet,
    {
        match self {
            PuppeterSendCommandError::PermissionDenied(e) => e.handle_error(puppeter),
            PuppeterSendCommandError::PuppetDoesNotExist(e) => e.handle_error(puppeter),
            PuppeterSendCommandError::PuppetError(e) => e.handle_error(puppeter),
            PuppeterSendCommandError::PuppetSendChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet send channel closed.")
            }
            PuppeterSendCommandError::PuppetReceiveChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet receive channel closed.")
            }
            PuppeterSendCommandError::PuppetResponseChannelClosed { name } => {
                tracing::warn!(name = %name, "Puppet response channel closed.")
            }
        }
    }
}
