use std::{
    any::{type_name, TypeId},
    fmt,
};

use thiserror::Error;

use crate::{
    pid::Pid,
    puppet::{Lifecycle, LifecycleStatus, Puppet, Puppeter},
};

#[derive(Debug, Error)]
#[error("Puppet does not exist: {puppet}")]
pub struct PuppetDoesNotExistError {
    pub(crate) puppet: Pid,
}

impl PuppetDoesNotExistError {
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }
    pub fn from_type<P>() -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>())
    }
}

impl From<PuppetDoesNotExistError> for CriticalError {
    fn from(value: PuppetDoesNotExistError) -> Self {
        CriticalError::new(value.puppet, value.to_string())
    }
}

impl From<PuppetDoesNotExistError> for PuppetError {
    fn from(value: PuppetDoesNotExistError) -> Self {
        CriticalError::from(value).into()
    }
}

#[derive(Debug, Error)]
#[error("Puppet already exist: {puppet}")]
pub struct PuppetAlreadyExist {
    pub(crate) puppet: Pid,
}

impl From<PuppetAlreadyExist> for PuppetError {
    fn from(value: PuppetAlreadyExist) -> Self {
        CriticalError::new(value.puppet, value.to_string()).into()
    }
}

impl PuppetAlreadyExist {
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }
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
    pub fn new(master: Pid, puppet: Pid) -> Self {
        Self {
            master,
            puppet,
            message: None,
        }
    }
    pub fn from_type<M, P>() -> Self
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        Self::new(Pid::new::<M>(), Pid::new::<P>())
    }
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn message_or_default(&self) -> String {
        self.message
            .clone()
            .unwrap_or_else(|| "No message".to_string())
    }
}

impl From<PermissionDeniedError> for CriticalError {
    fn from(value: PermissionDeniedError) -> Self {
        CriticalError::new(value.puppet, value.to_string())
    }
}

impl From<PermissionDeniedError> for PuppetError {
    fn from(value: PermissionDeniedError) -> Self {
        CriticalError::from(value).into()
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
        NonCriticalError::new(value.puppet, value.to_string()).into()
    }
}

impl PuppetCannotHandleMessage {
    pub fn new(puppet: Pid, status: LifecycleStatus) -> Self {
        Self { puppet, status }
    }
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

impl NonCriticalError {
    pub fn new(puppet: Pid, message: impl ToString) -> Self {
        Self {
            puppet,
            message: message.to_string(),
        }
    }
    pub fn from_type<P>(message: impl ToString) -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>(), message)
    }
}

#[derive(Error, Debug, Clone)]
#[error("Critical error occurred in puppet {puppet}: '{message}'. Supervisor will be notified.")]
pub struct CriticalError {
    pub puppet: Pid,
    pub message: String,
}

impl CriticalError {
    pub fn new(puppet: Pid, message: impl ToString) -> Self {
        Self {
            puppet,
            message: message.to_string(),
        }
    }
    pub fn from_type<P>(message: impl ToString) -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>(), message)
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
    pub fn non_critical(puppet: Pid, message: impl ToString) -> Self {
        NonCriticalError::new(puppet, message).into()
    }
    pub fn critical(puppet: Pid, message: impl ToString) -> Self {
        CriticalError::new(puppet, message).into()
    }
}

#[derive(Debug, Error)]
#[error("Reached max retry limit: {message}")]
pub struct RetryError {
    pub message: String,
}

impl RetryError {
    pub fn new(message: impl ToString) -> Self {
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
    fn from(err: PuppetSendCommandError) -> PuppetError {
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
    ReceiveError { puppet: Pid },
    #[error("Can't receive response. Channel closed.")]
    ResponseReceiveError { puppet: Pid },
    #[error("Can't reveive response. Response timeout.")]
    ResponseTimeout { puppet: Pid },
    #[error(transparent)]
    PuppetError(#[from] PuppetError),
}

impl From<PostmanError> for PuppetError {
    fn from(err: PostmanError) -> PuppetError {
        match err {
            PostmanError::SendError { puppet } => CriticalError::new(puppet, err).into(),
            PostmanError::ReceiveError { puppet } => CriticalError::new(puppet, err).into(),
            PostmanError::ResponseReceiveError { puppet } => CriticalError::new(puppet, err).into(),
            PostmanError::ResponseTimeout { puppet } => CriticalError::new(puppet, err).into(),
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
    fn from(err: PuppetRegisterError) -> PuppetError {
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
