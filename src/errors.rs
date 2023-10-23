use thiserror::Error;

use crate::{puppet::LifecycleStatus, PuppetIdentifier};

#[derive(Debug, Error)]
#[error("Puppet does not exist: {name}")]
pub struct PuppetDoesNotExist {
    pub name: String,
}

#[derive(Debug, Error)]
#[error("Puppet already exist: {name}")]
pub struct PuppetAlreadyExist {
    pub name: String,
}

#[derive(Debug, Error)]
#[error("Permission denied: {message}")]
pub struct PermissionDenied {
    pub master: String,
    pub puppet: String,
    pub message: String,
}

impl PermissionDenied {
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

#[derive(Debug, Error)]
#[error("Puppet {name} cannot handle message. Status: {status}.")]
pub struct PuppetCannotHandleMessage {
    pub name: String,
    pub status: LifecycleStatus,
}

#[derive(Debug, Error)]
pub enum StartPuppetError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

#[derive(Debug, Error)]
pub enum StopPuppetError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

#[derive(Debug, Error)]
pub enum ResetPuppetError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

impl From<StopPuppetError> for ResetPuppetError {
    fn from(value: StopPuppetError) -> Self {
        match value {
            StopPuppetError::PuppetDoesNotExist(e) => ResetPuppetError::PuppetDoesNotExist(e),
            StopPuppetError::PostmanError(e) => ResetPuppetError::PostmanError(e),
            StopPuppetError::PermissionDenied(e) => ResetPuppetError::PermissionDenied(e),
            StopPuppetError::PuppetError(e) => ResetPuppetError::PuppetError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum KillPuppetError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

impl From<StopPuppetError> for KillPuppetError {
    fn from(value: StopPuppetError) -> Self {
        match value {
            StopPuppetError::PuppetDoesNotExist(e) => KillPuppetError::PuppetDoesNotExist(e),
            StopPuppetError::PostmanError(e) => KillPuppetError::PostmanError(e),
            StopPuppetError::PermissionDenied(e) => KillPuppetError::PermissionDenied(e),
            StopPuppetError::PuppetError(e) => KillPuppetError::PuppetError(e),
        }
    }
}

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Timed out waiting for response from actor.")]
    ResponseTimeout,
    #[error("Error sending message.")]
    SendError,
    #[error("Error receiving message.")]
    ReceiveError,
    #[error("Error receiving response from actor.")]
    ResponseReceiveError,
    #[error("Error sending response.")]
    ResponseSendError,
}

#[derive(Debug, Error)]
pub enum IdentificationError {
    #[error("Puppet already exists: {0}")]
    PuppetAlreadyExists(PuppetIdentifier),
    #[error("Puppet does not exist: {0}")]
    PuppetDoesNotExist(PuppetIdentifier),
}

#[derive(Error, Debug)]
pub enum PuppetError {
    #[error("Non-critical error occurred: '{0}'. Supervisor will not be notified.")]
    NonCritical(String),

    #[error("Critical error occurred: '{0}'. Supervisor will be notified.")]
    Critical(String),

    #[error("Fatal error occurred: '{0}'. Supervisor will be notified.")]
    Fatal(String),

    #[error(transparent)]
    PuppetCannotHandleMessage(#[from] PuppetCannotHandleMessage),
}

impl From<PuppetError> for String {
    fn from(value: PuppetError) -> Self {
        match value {
            PuppetError::NonCritical(s) => s,
            PuppetError::Critical(s) => s,
            PuppetError::Fatal(s) => s,
            PuppetError::PuppetCannotHandleMessage(e) => e.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppeterSendMessageError {
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
    PuppetCannotHandleMessage(#[from] PuppetCannotHandleMessage),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

#[derive(Debug, Error)]
pub enum PostmanError {
    #[error("Can't send message. Channel closed.")]
    SendError,
    #[error("Can't receive message. Channel closed.")]
    ReceiveError,
    #[error("Can't receive response. Channel closed.")]
    ResponseReceiveError,
    #[error("Can't reveive response. Response timeout.")]
    ResponseTimeout,
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

impl From<(PostmanError, String)> for PuppeterSendMessageError {
    fn from(err: (PostmanError, String)) -> Self {
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

impl From<(PostmanError, String)> for PuppeterSendCommandError {
    fn from(err: (PostmanError, String)) -> Self {
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
pub enum PuppeterSpawnError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PuppetAlreadyExist(#[from] PuppetAlreadyExist),
}

#[derive(Debug, Error)]
pub enum PuppeterSendCommandError {
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDenied),
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExist),
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
    #[error("Puppet {name} send channel closed.")]
    PuppetSendChannelClosed { name: String },
    #[error("Puppet {name} receive channel closed.")]
    PuppetReceiveChannelClosed { name: String },
    #[error("Puppet {name} response channel closed.")]
    PuppetResponseChannelClosed { name: String },
}
