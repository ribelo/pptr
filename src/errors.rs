use std::fmt;

use thiserror::Error;

use crate::{
    pid::{Id, Pid},
    puppet::{Lifecycle, LifecycleStatus},
};

#[derive(Debug, Error)]
#[error("Puppet does not exist: {puppet}")]
pub struct PuppetDoesNotExistError {
    pub(crate) puppet: Pid,
}

impl PuppetDoesNotExistError {
    #[must_use]
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }
    #[must_use]
    pub fn from_type<P>() -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>())
    }
}

impl From<PuppetDoesNotExistError> for PuppetError {
    fn from(value: PuppetDoesNotExistError) -> Self {
        Self::critical(value.puppet, &value)
    }
}

#[derive(Debug, Error)]
#[error("Resource already exist")]
pub struct ResourceAlreadyExist {
    pub(crate) id: Id,
}

#[derive(Debug, Error)]
#[error("Puppet already exist: {puppet}")]
pub struct PuppetAlreadyExist {
    pub(crate) puppet: Pid,
}

impl From<PuppetAlreadyExist> for PuppetError {
    fn from(value: PuppetAlreadyExist) -> Self {
        Self::critical(value.puppet, &value)
    }
}

impl PuppetAlreadyExist {
    #[must_use]
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }
    #[must_use]
    pub fn from_type<P>() -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>())
    }
}

#[derive(Debug, Error)]
pub struct PermissionDeniedError {
    pub master: Pid,
    pub puppet: Pid,
    pub message: Option<String>,
}

impl fmt::Display for PermissionDeniedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Permission denied. Master: {}, Puppet: {}, Message: {}",
            self.master,
            self.puppet,
            self.message_or_default()
        )
    }
}

impl PermissionDeniedError {
    #[must_use]
    pub fn new(master: Pid, puppet: Pid) -> Self {
        Self {
            master,
            puppet,
            message: None,
        }
    }
    #[must_use]
    pub fn from_type<M, P>() -> Self
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        Self::new(Pid::new::<M>(), Pid::new::<P>())
    }

    #[must_use]
    pub fn with_message<T: Into<String>>(mut self, message: T) -> Self {
        self.message = Some(message.into());
        self
    }

    #[must_use]
    pub fn message_or_default(&self) -> String {
        self.message
            .clone()
            .unwrap_or_else(|| "No message".to_owned())
    }
}

impl From<PermissionDeniedError> for PuppetError {
    fn from(value: PermissionDeniedError) -> Self {
        Self::critical(value.puppet, &value)
    }
}

#[derive(Debug, Error)]
#[error("Puppet {puppet} cannot handle message. Status: {status}.")]
pub struct PuppetCannotHandleMessage {
    pub puppet: Pid,
    pub status: LifecycleStatus,
}

impl From<PuppetCannotHandleMessage> for PuppetError {
    fn from(value: PuppetCannotHandleMessage) -> Self {
        Self::non_critical(value.puppet, &value)
    }
}

impl PuppetCannotHandleMessage {
    #[must_use]
    pub fn new(puppet: Pid, status: LifecycleStatus) -> Self {
        Self { puppet, status }
    }
    #[must_use]
    pub fn from_type<P>(status: LifecycleStatus) -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>(), status)
    }
}

#[derive(Error, Debug, Clone)]
#[error(
    "Non-critical error occurred in puppet {puppet}: '{message}'. Supervisor will not be notified."
)]
pub struct NonCriticalError {
    pub puppet: Pid,
    pub message: String,
}

#[derive(Error, Debug, Clone)]
#[error("Critical error occurred in puppet {puppet}: '{message}'. Supervisor will be notified.")]
pub struct CriticalError {
    pub puppet: Pid,
    pub message: String,
}

impl CriticalError {
    pub fn new<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        Self {
            puppet,
            message: message.to_string(),
        }
    }
}

#[derive(Error, Debug, Clone)]
pub enum PuppetError {
    #[error(transparent)]
    NonCritical(#[from] NonCriticalError),
    #[error(transparent)]
    Critical(#[from] CriticalError),
}

impl PuppetError {
    pub fn non_critical<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        NonCriticalError {
            puppet,
            message: message.to_string(),
        }
        .into()
    }
    pub fn critical<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        CriticalError {
            puppet,
            message: message.to_string(),
        }
        .into()
    }
}

#[derive(Debug, Error)]
#[error("Reached max retry limit: {message}")]
pub struct RetryError {
    pub message: String,
}

impl RetryError {
    pub fn new<T: ToString + ?Sized>(message: &T) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppetSendMessageError {
    #[error(transparent)]
    PuppetDosNotExist(#[from] PuppetDoesNotExistError),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
}

impl From<PuppetSendMessageError> for PuppetError {
    fn from(err: PuppetSendMessageError) -> Self {
        match err {
            PuppetSendMessageError::PuppetDosNotExist(err) => err.into(),
            PuppetSendMessageError::PostmanError(err) => err.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppetSendCommandError {
    #[error(transparent)]
    PuppetDosNotExist(#[from] PuppetDoesNotExistError),
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDeniedError),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
}

impl From<PuppetSendCommandError> for PuppetError {
    fn from(err: PuppetSendCommandError) -> Self {
        match err {
            PuppetSendCommandError::PuppetDosNotExist(err) => err.into(),
            PuppetSendCommandError::PermissionDenied(err) => err.into(),
            PuppetSendCommandError::PostmanError(err) => err.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PostmanError {
    #[error("Can't send message. Channel closed.")]
    SendError { puppet: Pid },
    #[error("Can't receive message. Channel closed.")]
    ResponseReceiveError { puppet: Pid },
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

impl From<PostmanError> for PuppetError {
    fn from(err: PostmanError) -> Self {
        match err {
            PostmanError::SendError { puppet } | PostmanError::ResponseReceiveError { puppet } => {
                Self::critical(puppet, &err)
            }
            PostmanError::PuppetError(err) => err,
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppetRegisterError {
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExistError),
    #[error(transparent)]
    PuppetAlreadyExist(#[from] PuppetAlreadyExist),
}

impl From<PuppetRegisterError> for PuppetError {
    fn from(err: PuppetRegisterError) -> Self {
        match err {
            PuppetRegisterError::PuppetDoesNotExist(err) => err.into(),
            PuppetRegisterError::PuppetAlreadyExist(err) => err.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PuppetOperationError {
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDeniedError),
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExistError),
}
