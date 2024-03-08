use std::fmt;

use thiserror::Error;

use crate::{
    pid::{Id, Pid},
    puppet::{Lifecycle, LifecycleStatus},
};

/// Error returned when a referenced puppet does not exist.
///
/// This error is constructed with the [`Pid`] of the non-existent puppet.
#[derive(Debug, Error)]
#[error("Puppet does not exist: {puppet}")]
pub struct PuppetDoesNotExistError {
    pub(crate) puppet: Pid,
}

impl PuppetDoesNotExistError {
    /// Creates a new `PuppetDoesNotExistError` with the given `puppet` ID.
    ///
    /// The `puppet` parameter is the [`Pid`] of the non-existent puppet that caused the error.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::pid::Pid;
    /// # use pptr::errors::PuppetDoesNotExistError;
    /// # #[derive(Debug, Clone)]
    /// # struct MyPuppet;
    /// # impl pptr::puppet::Lifecycle for MyPuppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let pid = Pid::new::<MyPuppet>();
    /// let err = PuppetDoesNotExistError::new(pid);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }

    /// Creates a new `PuppetDoesNotExistError` for a non-existent puppet of type `P`.
    ///
    /// This function generates a new [`Pid`] for the given puppet type `P` and constructs
    /// a `PuppetDoesNotExistError` with that ID.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PuppetDoesNotExistError;
    /// # #[derive(Debug, Clone)]
    /// # struct MyPuppet;
    /// # impl pptr::puppet::Lifecycle for MyPuppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let err = PuppetDoesNotExistError::from_type::<MyPuppet>();
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
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

/// Error type representing a resource that already exists.
///
/// This error is returned when attempting to create a resource that already exists,
/// as identified by its unique `id`.
#[derive(Debug, Error)]
#[error("Resource already exist")]
pub struct ResourceAlreadyExist {
    pub(crate) id: Id,
}

impl ResourceAlreadyExist {
    /// Creates a new `ResourceAlreadyExist` error with the given `id`.
    ///
    /// The `id` parameter is the unique identifier of the existing resource that caused the error.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::ResourceAlreadyExist;
    /// # use pptr::pid::Id;
    /// let id = Id::new::<i32>();
    /// let error = ResourceAlreadyExist::new(id);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self { id }
    }
}

/// Error type representing a puppet that already exists.
///
/// This error is returned when attempting to create a puppet with an ID that is
/// already associated with an existing puppet.
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
    /// Creates a new `PuppetAlreadyExist` error with the given `puppet` ID.
    ///
    /// The `puppet` parameter is the [`Pid`] of the already existing puppet that caused the error.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::pid::Pid;
    /// # use pptr::errors::PuppetAlreadyExist;
    /// # #[derive(Debug, Clone)]
    /// # struct MyPuppet;
    /// # impl pptr::puppet::Lifecycle for MyPuppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let pid = Pid::new::<MyPuppet>();
    /// let err = PuppetAlreadyExist::new(pid);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn new(puppet: Pid) -> Self {
        Self { puppet }
    }

    /// Creates a new `PuppetAlreadyExist` error for an existing puppet of type `P`.
    ///
    /// This function generates a new [`Pid`] for the given puppet type `P` and constructs
    /// a `PuppetAlreadyExist` error with that ID.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PuppetAlreadyExist;
    /// # #[derive(Debug, Clone)]
    /// # struct MyPuppet;
    /// # impl pptr::puppet::Lifecycle for MyPuppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let err = PuppetAlreadyExist::from_type::<MyPuppet>();
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn from_type<P>() -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>())
    }
}

/// Error type representing a permission denied error.
///
/// This error occurs when a master tries to perform an operation on a puppet
/// without sufficient permissions.
///
/// # Example
///
/// ```
/// # use pptr::errors::PermissionDeniedError;
/// # use pptr::pid::Pid;
/// #
/// # #[derive(Debug, Clone)]
/// # struct Puppet;
/// # impl pptr::puppet::Lifecycle for Puppet {
/// #     type Supervision = pptr::supervision::strategy::OneToOne;
/// # }
/// #
/// # #[derive(Debug, Clone)]
/// # struct Master;
/// # impl pptr::puppet::Lifecycle for Master {
/// #     type Supervision = pptr::supervision::strategy::OneToOne;
/// # }
/// let master_pid = Pid::new::<Puppet>();
/// let puppet_pid = Pid::new::<Master>();
/// let error = PermissionDeniedError {
///     master: master_pid,
///     puppet: puppet_pid,
///     message: Some("Operation not allowed".to_string()),
/// };
/// ```
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
    /// Creates a new `PermissionDeniedError` with the given master and puppet PIDs.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PermissionDeniedError;
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Master;
    /// # impl pptr::puppet::Lifecycle for Master {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let master_pid = Pid::new::<Master>();
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PermissionDeniedError::new(master_pid, puppet_pid);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn new(master: Pid, puppet: Pid) -> Self {
        Self {
            master,
            puppet,
            message: None,
        }
    }
    /// Creates a new `PermissionDeniedError` using the generic types `M` and `P` to infer the master and puppet PIDs.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PermissionDeniedError;
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Master;
    /// # impl pptr::puppet::Lifecycle for Master {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let master_pid = Pid::new::<Master>();
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PermissionDeniedError::from_type::<Master, Puppet>();
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn from_type<M, P>() -> Self
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        Self::new(Pid::new::<M>(), Pid::new::<P>())
    }

    /// Sets a custom error message for the `PermissionDeniedError`.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PermissionDeniedError;
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Master;
    /// # impl pptr::puppet::Lifecycle for Master {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let master_pid = Pid::new::<Master>();
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PermissionDeniedError::new(master_pid, puppet_pid)
    ///     .with_message("Operation not allowed");
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn with_message<T: Into<String>>(mut self, message: T) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Returns the error message if set, or a default message if not.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PermissionDeniedError;
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Master;
    /// # impl pptr::puppet::Lifecycle for Master {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let master_pid = Pid::new::<Master>();
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PermissionDeniedError::new(master_pid, puppet_pid);
    /// assert_eq!(error.message_or_default(), "No message");
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
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

/// Error type representing a scenario where a puppet cannot handle a message due to its current lifecycle status.
///
/// This error occurs when a message is sent to a puppet that is not in a state to handle the message based on its
/// current lifecycle status. The error includes the puppet's PID and the current lifecycle status.
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
    /// Creates a new `PuppetCannotHandleMessage` error using the specified puppet PID and lifecycle status.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PuppetCannotHandleMessage;
    /// # use pptr::pid::Pid;
    /// # use pptr::puppet::LifecycleStatus;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let status = LifecycleStatus::Inactive;
    /// let error = PuppetCannotHandleMessage::new(puppet_pid, status);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn new(puppet: Pid, status: LifecycleStatus) -> Self {
        Self { puppet, status }
    }
    /// Creates a new `PuppetCannotHandleMessage` error using the generic type `P` to infer the puppet PID.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::PuppetCannotHandleMessage;
    /// # use pptr::pid::Pid;
    /// # use pptr::puppet::LifecycleStatus;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// let status = LifecycleStatus::Inactive;
    /// let error = PuppetCannotHandleMessage::from_type::<Puppet>(status);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn from_type<P>(status: LifecycleStatus) -> Self
    where
        P: Lifecycle,
    {
        Self::new(Pid::new::<P>(), status)
    }
}

/// Non-critical error type representing an error that occurred in a puppet.
///
/// This error indicates a non-critical error condition in a puppet, meaning that the supervisor will not be notified.
/// The error includes the puppet's PID and an error message describing the issue.
#[derive(Error, Debug, Clone)]
#[error(
    "Non-critical error occurred in puppet {puppet}: '{message}'. Supervisor will not be notified."
)]
pub struct NonCriticalError {
    pub puppet: Pid,
    pub message: String,
}

/// Critical error type representing a severe error that occurred in a puppet.
///
/// This error indicates a critical error condition in a puppet, meaning that the supervisor will be notified.
/// The error includes the puppet's PID and an error message describing the issue.
#[derive(Error, Debug, Clone)]
#[error("Critical error occurred in puppet {puppet}: '{message}'. Supervisor will be notified.")]
pub struct CriticalError {
    pub puppet: Pid,
    pub message: String,
}

impl CriticalError {
    /// Creates a new `CriticalError` with the specified puppet PID and error message.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::CriticalError;
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    ///
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = CriticalError::new(puppet_pid, "Something went wrong");
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn new<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        Self {
            puppet,
            message: message.to_string(),
        }
    }
}

/// An error type representing errors that can occur in a puppet.
///
/// `PuppetError` is an enum with two variants:
///
/// - `NonCritical`: Represents a non-critical error that occurred in a puppet. This variant does
///   not cause a notification to the supervisor, but is reported if the caller is waiting for a
///   response.
/// - `Critical`: Represents a critical error that occurred in a puppet. This error causes a
///   notification to the supervisor and a restart according to the selected strategy.
#[derive(Error, Debug, Clone)]
pub enum PuppetError {
    #[error(transparent)]
    NonCritical(#[from] NonCriticalError),
    #[error(transparent)]
    Critical(#[from] CriticalError),
}

impl PuppetError {
    /// Constructs a new `PuppetError` variant representing a non-critical error.
    ///
    /// This function creates a `PuppetError::NonCritical` variant from the provided `puppet` identifier and
    /// `message`. The `message` can be any type that implements `ToString` and is converted to a `String`.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::{PuppetError, NonCriticalError};
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PuppetError::non_critical(puppet_pid, "Something went wrong");
    /// assert!(matches!(error, PuppetError::NonCritical(NonCriticalError { .. })));
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn non_critical<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        NonCriticalError {
            puppet,
            message: message.to_string(),
        }
        .into()
    }
    /// Constructs a new `PuppetError` variant representing a critical error.
    ///
    /// This function creates a `PuppetError::Critical` variant from the provided `puppet` identifier and
    /// `message`. The `message` can be any type that implements `ToString` and is converted to a `String`.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::errors::{PuppetError, CriticalError};
    /// # use pptr::pid::Pid;
    /// #
    /// # #[derive(Debug, Clone)]
    /// # struct Puppet;
    /// # impl pptr::puppet::Lifecycle for Puppet {
    /// #     type Supervision = pptr::supervision::strategy::OneToOne;
    /// # }
    /// #
    /// let puppet_pid = Pid::new::<Puppet>();
    /// let error = PuppetError::critical(puppet_pid, "Something went wrong");
    /// assert!(matches!(error, PuppetError::Critical(CriticalError { .. })));
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn critical<T: ToString + ?Sized>(puppet: Pid, message: &T) -> Self {
        CriticalError {
            puppet,
            message: message.to_string(),
        }
        .into()
    }
}

/// Represents an error that occurs when the maximum retry limit is reached.
///
/// This error type is used to indicate that an operation has been retried multiple times
/// and has reached the maximum retry limit without succeeding.
///
/// The `message` field contains a string describing the reason for the retry failure.
#[derive(Debug, Error)]
#[error("Reached max retry limit: {message}")]
pub struct RetryError {
    pub message: String,
}
#[derive(Debug, Error)]
#[error("Reached max retry limit: {message}")]
pub struct RetryError {
    pub message: String,
}

impl RetryError {
    <doc>
    pub fn new<T: ToString + ?Sized>(message: &T) -> Self {
        Self {
            message: message.to_string(),
        }
    }
    <doc>
}

/// Represents an error that can occur when sending a message to a puppet.
///
/// This error type encompasses two possible scenarios:
///
/// - `PuppetDosNotExist`: The specified puppet does not exist.
/// - `PostmanError`: An error occurred in the postman while attempting to send the message.
#[derive(Debug, Error)]
pub enum PuppetSendMessageError {
    #[error(transparent)]
    PuppetDosNotExist(#[from] PuppetDoesNotExistError),
    #[error(transparent)]
    PostmanError(#[from] PostmanError),
}

impl PuppetSendMessageError {
    #[must_use]
    pub fn get_puppet_error(&self) -> Option<&PuppetError> {
        match self {
            PuppetSendMessageError::PostmanError(PostmanError::PuppetError(err)) => Some(err),
            _ => None,
        }
    }
}

impl From<PuppetSendMessageError> for PuppetError {
    fn from(err: PuppetSendMessageError) -> Self {
        match err {
            PuppetSendMessageError::PuppetDosNotExist(err) => err.into(),
            PuppetSendMessageError::PostmanError(err) => err.into(),
        }
    }
}

/// Represents an error that can occur when sending a command to a puppet.
///
/// This error type encompasses three possible scenarios:
///
/// - `PuppetDosNotExist`: The specified puppet does not exist.
/// - `PermissionDenied`: The caller does not have permission to send the command to the puppet.
/// - `PostmanError`: An error occurred in the postman while attempting to send the command.
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

/// Represents errors that can occur in the postman.
///
/// This error type encompasses three possible scenarios:
///
/// - `SendError`: The message could not be sent because the channel is closed.
/// - `ResponseReceiveError`: The response could not be received because the channel is closed.
/// - `PuppetError`: An error occurred in the puppet while processing the message or command.
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

/// Represents errors that can occur when registering a puppet.
///
/// This error type encompasses two possible scenarios:
///
/// - `PuppetDoesNotExist`: The specified puppet does not exist.
/// - `PuppetAlreadyExist`: The specified puppet already exists and cannot be registered again.
///
/// # Panics
///
/// This type does not panic.
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

/// Represents errors that can occur during puppet operations.
///
/// This error type encompasses two possible scenarios:
///
/// - `PermissionDenied`: The caller does not have permission to perform the operation on the puppet.
/// - `PuppetDoesNotExist`: The specified puppet does not exist.
///
/// # Panics
///
/// This type does not panic.
#[derive(Debug, Error)]
pub enum PuppetOperationError {
    #[error(transparent)]
    PermissionDenied(#[from] PermissionDeniedError),
    #[error(transparent)]
    PuppetDoesNotExist(#[from] PuppetDoesNotExistError),
}
