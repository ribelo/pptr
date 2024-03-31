use std::num::NonZeroUsize;

use async_recursion::async_recursion;
use async_trait::async_trait;
use tokio::sync::watch;
use tracing::debug;

use crate::{
    address::Address,
    errors::{
        CriticalError, PuppetDoesNotExistError, PuppetError, PuppetOperationError,
        PuppetSendCommandError, PuppetSendMessageError, ResourceAlreadyExist,
    },
    executor::Executor,
    message::{Mailbox, Message, RestartStage, ServiceCommand, ServiceMailbox},
    pid::Pid,
    puppeter::Puppeter,
    supervision::{RetryConfig, RetryConfigBuilder, SupervisionStrategy},
};

/// A trait that manages the entire lifecycle of puppets (actors) in an actor model.
///
/// The `Lifecycle` trait defines methods for handling the initialization, starting,
/// stopping, and resetting of puppets. It also specifies the associated type
/// `Supervision`, which represents the supervision strategy used for managing the
/// puppet's lifecycle.
///
/// Implementors of this trait must be `Send`, `Sync`, `Sized`, `Clone`, and `'static`.
#[allow(unused_variables)]
#[async_trait]
pub trait Lifecycle: Send + Sync + Sized + Clone + 'static {
    /// The supervision strategy used for managing the puppet's lifecycle.
    type Supervision: SupervisionStrategy + Send + Sync;

    /// Resets the puppet to its initial state.
    ///
    /// This method is called when the puppet needs to be reset to its initial state.
    /// It takes a reference to the puppet's context (`ctx`) and returns a new instance
    /// of the puppet on success, or a `CriticalError` if the reset operation fails.
    ///
    /// The default implementation clones the current instance of the puppet.
    async fn reset(&self, ctx: &Context) -> Result<Self, CriticalError> {
        Ok(self.clone())
    }

    /// Initializes the puppet.
    ///
    /// This method is called when the puppet is being initialized. It takes a mutable
    /// reference to the puppet instance and a reference to the puppet's context (`ctx`).
    ///
    /// The default implementation logs a debug message indicating that the puppet is
    /// being initialized.
    async fn on_init(&mut self, ctx: &Context) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %ctx.pid, "Initializing puppet");
        Ok(())
    }

    /// Starts the puppet.
    ///
    /// This method is called when the puppet is being started. It takes a mutable
    /// reference to the puppet instance and a reference to the puppet's context (`ctx`).
    ///
    /// The default implementation logs a debug message indicating that the puppet is
    /// being started.
    async fn on_start(&mut self, ctx: &Context) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %ctx.pid, "Starting puppet" );
        Ok(())
    }

    /// Stops the puppet.
    ///
    /// This method is called when the puppet is being stopped. It takes a mutable
    /// reference to the puppet instance and a reference to the puppet's context (`ctx`).
    ///
    /// The default implementation logs a debug message indicating that the puppet is
    /// being stopped.
    async fn on_stop(&mut self, ctx: &Context) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %ctx.pid, "Stopping puppet");
        Ok(())
    }
}

/// A marker trait indicating that a type can be used as a puppet (actor).
///
/// Types implementing this trait must be `Send`, `Sync`, `Clone`, and `'static`.
pub trait Puppet: Send + Sync + Clone + 'static {}
/// Blanket implementation of the `Puppet` trait for types satisfying the necessary bounds.
///
/// This implementation automatically implements the `Puppet` trait for any type that is
/// `Send`, `Sync`, `Clone`, and `'static`.
impl<T> Puppet for T where T: Send + Sync + Clone + 'static {}

/// Represents the lifecycle status of a puppet.
///
/// The `LifecycleStatus` enum defines the possible states a puppet can be in during its lifecycle.
#[derive(Debug, Clone, Copy, strum::Display, PartialEq, Eq)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Deactivating,
    Inactive,
    Restarting,
    Failed,
}

/// Represents the context of a puppet.
///
/// The `Context` struct contains information about a puppet's context, including its process ID (`pid`),
/// the `Puppeter` instance, and the retry configuration.
#[derive(Clone, Debug)]
pub struct Context {
    pub pid: Pid,
    pub(crate) pptr: Puppeter,
    pub(crate) retry_config: RetryConfig,
}

/// A builder for creating and configuring a puppet.
///
/// The `PuppetBuilder` struct allows for the creation and configuration of a puppet instance.
/// It provides methods to set various options such as the message buffer size, command buffer size,
/// and retry configuration.
pub struct PuppetBuilder<P>
where
    P: Lifecycle,
{
    pub pid: Pid,
    pub puppet: Option<P>,
    pub messages_buffer_size: NonZeroUsize,
    pub commands_buffer_size: NonZeroUsize,
    pub retry_config: Option<RetryConfig>,
}

impl<P> PuppetBuilder<P>
where
    P: Lifecycle,
{
    /// Creates a new `PuppetBuilder` with the provided puppet state.
    ///
    /// This method initializes a new `PuppetBuilder` instance with default values for the
    /// message buffer size, command buffer size, and retry configuration.
    pub fn new(state: P) -> Self {
        Self {
            pid: Pid::new::<P>(),
            puppet: Some(state),
            // SAFETY: NonZeroUsize::new_unchecked is safe because the value is known to be non-zero
            messages_buffer_size: unsafe { NonZeroUsize::new_unchecked(1024) },
            // SAFETY: NonZeroUsize::new_unchecked is safe because the value is known to be non-zero
            commands_buffer_size: unsafe { NonZeroUsize::new_unchecked(16) },
            retry_config: Some(RetryConfigBuilder::default().build()),
        }
    }

    /// Sets the message buffer size for the puppet.
    ///
    /// This method allows configuring the size of the message buffer used by the puppet.
    /// It takes a `NonZeroUsize` value representing the desired buffer size and returns
    /// the updated `PuppetBuilder` instance.
    #[must_use]
    pub fn with_messages_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.messages_buffer_size = size;
        self
    }

    /// Sets the command buffer size for the puppet.
    ///
    /// This method allows configuring the size of the command buffer used by the puppet.
    /// It takes a `NonZeroUsize` value representing the desired buffer size and returns
    /// the updated `PuppetBuilder` instance.
    #[must_use]
    pub fn with_commands_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.commands_buffer_size = size;
        self
    }

    /// Spawns an independent puppet (actor) on the `pptr` runtime.
    ///
    /// This method spawns a puppet without a manager, making it independent. It takes a reference
    /// to the `Puppeter` runtime and returns a `Result` containing the `Address` of the spawned
    /// puppet on success, or a `PuppetError` on failure.
    pub async fn spawn(self, pptr: &Puppeter) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        pptr.spawn::<P, P>(self).await
    }

    /// Spawns a puppet (actor) with a manager on the `pptr` runtime.
    ///
    /// This method spawns a puppet `P` with a manager `M`. It takes a reference to the `Puppeter`
    /// runtime and returns a `Result` containing the `Address` of the spawned puppet on success,
    /// or a `PuppetError` on failure.
    pub async fn spawn_link<M>(self, pptr: &Puppeter) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        pptr.spawn::<M, P>(self).await
    }
}

impl Context {
    /// Starts the puppet and its associated puppets.
    ///
    /// This method initializes and starts the puppet and its associated puppets based on the
    /// provided `is_restarting` flag. It handles retries and error reporting in case of failures.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet fails to start or if the maximum number of retries
    /// is reached during the start process.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    pub(crate) async fn start<P>(
        &self,
        puppet: &mut P,
        is_restarting: bool,
    ) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        // Determine the service command and initial status based on whether the service is
        // restarting or not.
        let (service_command, begin_status) = if is_restarting {
            (ServiceCommand::Start, LifecycleStatus::Activating)
        } else {
            (
                ServiceCommand::Restart {
                    stage: Some(RestartStage::Start),
                },
                LifecycleStatus::Restarting,
            )
        };

        // Flag to store if the `on_start` function has been completed.
        let mut on_start_done = false;

        // Flag to store if the `start_all_puppets` function has been completed.
        let mut start_all_puppets_done = false;

        loop {
            // Set the initial status of the puppet service.
            self.set_status(begin_status);

            if !on_start_done {
                // Perform the `on_start` function which initializes the puppet service.
                match puppet.on_start(self).await {
                    Ok(()) | Err(PuppetError::NonCritical(_)) => {
                        // If `on_start` succeeds or returns a non-critical error, set the status
                        // to `Active` and mark `on_start_done` as `true`.
                        on_start_done = true;
                        self.set_status(LifecycleStatus::Active);
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.retry_config.increment_retry().is_err() {
                            let error = self.critical_error("Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(self.critical_error(&err));
                            }
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        }
                        // Increment the retry count, wait according to the retry config, and
                        // continue to the next iteration of the loop.
                        self.retry_config.maybe_wait().await;
                        continue;
                    }
                }
            }

            if !start_all_puppets_done {
                // Start all puppets by calling the `start_all_puppets` function with the specified
                // service command.
                match self.start_all_puppets(&service_command).await {
                    Ok(()) | Err(PuppetError::NonCritical(_)) => {
                        // If `start_all_puppets` succeeds or returns a non-critical error, mark
                        // `start_all_puppets_done` as `true`.
                        start_all_puppets_done = true;
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.retry_config.increment_retry().is_err() {
                            let error = self.critical_error("Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(self.critical_error(&err));
                            }
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        }
                        // Increment the retry count, wait according to the retry config, and
                        // continue to the next iteration of the loop.
                        self.retry_config.maybe_wait().await;
                        continue;
                    }
                }
            }

            // If both the `on_start` and `start_all_puppets` functions are completed, exit the
            // loop.
            if on_start_done && start_all_puppets_done {
                break;
            }
        }

        Ok(())
    }

    /// Stops the puppet and its associated puppets.
    ///
    /// This method stops the puppet and its associated puppets based on the provided
    /// `is_restarting` flag. It handles retries and error reporting in case of failures.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet fails to stop or if the maximum number of retries
    /// is reached during the stop process.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    async fn stop<P>(&self, puppet: &mut P, is_restarting: bool) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        // Determine the service command and initial status based on whether the service is
        // restarting or not.
        let (service_command, begin_status) = if is_restarting {
            (ServiceCommand::Start, LifecycleStatus::Activating)
        } else {
            (
                ServiceCommand::Restart {
                    stage: Some(RestartStage::Start),
                },
                LifecycleStatus::Restarting,
            )
        };
        // Clone the retry config from the supervision config.
        // let retry_config = self.supervision_config.retry.clone();

        // Flag to store if the `on_stop` function has been completed.
        let mut on_stop_done = false;

        // Flag to store if the `stop_all_puppets` function has been completed.
        let mut stop_all_puppets_done = false;

        loop {
            // Set the initial status of the puppet service.
            self.set_status(begin_status);

            if !stop_all_puppets_done {
                match self.stop_all_puppets(&service_command).await {
                    Ok(()) | Err(PuppetError::NonCritical(_)) => stop_all_puppets_done = true,
                    Err(PuppetError::Critical(_)) => {
                        if self.retry_config.increment_retry().is_err() {
                            let error = self.critical_error("Max retry reached during stop");
                            // If the maximum retry attempts are reached during `stop_all_puppets`,
                            // Mark tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(self.critical_error(&err));
                            };
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        }
                        // Increment the retry count, wait according to the retry config, and
                        // continue to the next iteration of the loop.
                        self.retry_config.maybe_wait().await;
                        continue;
                    }
                }
            }

            if !on_stop_done {
                match puppet.on_stop(self).await {
                    Ok(()) | Err(PuppetError::NonCritical(_)) => {
                        // If `on_stop` succeeds or returns a non-critical error, set the status
                        // to `Inactive` and mark `on_stop_done` as `true`.
                        on_stop_done = true;
                        self.set_status(LifecycleStatus::Inactive);
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.retry_config.increment_retry().is_err() {
                            let error = self.critical_error("Max retry reached during stop");
                            // If the maximum retry attempts are reached during `on_stop`,
                            // Mark tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(self.critical_error(&err));
                            };
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        }
                        // Increment the retry count, wait according to the retry config, and
                        // continue to the next iteration of the loop.
                        self.retry_config.maybe_wait().await;
                        continue;
                    }
                }
            }

            // If both the `on_stop` and `stop_all_puppets` functions are completed, exit the
            // loop.
            if on_stop_done && stop_all_puppets_done {
                break;
            }
        }

        Ok(())
    }

    /// Restarts the puppet.
    ///
    /// This method restarts the puppet by stopping it, resetting its state, and starting it again.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet fails to restart.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    async fn restart<P>(&self, puppet: &mut P) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        self.stop(puppet, true).await?;
        // Reset state
        *puppet = puppet.reset(self).await?;
        self.start(puppet, true).await?;
        Ok(())
    }

    /// Fails the puppet and reports the failure.
    ///
    /// This method marks the puppet as failed and reports the failure to its associated puppets.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the failure reporting fails.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    pub(crate) async fn fail<P>(&self, puppet: &mut P) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        if let Err(err) = self.fail_all_puppets(puppet).await {
            self.set_status(LifecycleStatus::Failed);
            Err(err)
        } else {
            self.set_status(LifecycleStatus::Failed);
            Ok(())
        }
    }

    /// Checks if a puppet of the specified type exists.
    ///
    /// Returns `true` if a puppet of type `P` exists, otherwise `false`.
    #[must_use]
    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: Lifecycle,
    {
        self.pptr.is_puppet_exists::<P>()
    }

    /// Retrieves the current status of the puppet of the specified type.
    ///
    /// Returns the current `LifecycleStatus` of the puppet of type `P`, or `None` if the puppet
    /// does not exist.
    #[must_use]
    pub fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.pptr.get_puppet_status_by_pid(puppet)
    }

    /// Sets the status of the puppet.
    ///
    /// This method updates the status of the puppet to the provided `status`.
    pub(crate) fn set_status(&self, status: LifecycleStatus) {
        self.pptr.set_status_by_pid(self.pid, status);
    }

    /// Checks if the puppet of type `P` is associated with the master of type `M`.
    ///
    /// Returns `Some(true)` if the puppet is associated with the master, `Some(false)` if not,
    /// and `None` if either the puppet or master does not exist.
    #[must_use]
    pub fn has_puppet<M, P>(&self) -> Option<bool>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.pptr.puppet_has_puppet_by_pid(master_pid, puppet_pid)
    }

    /// Retrieves the master of the puppet of the specified type.
    ///
    /// Returns the `Pid` of the master associated with the puppet of type `P`, or `None` if the
    /// puppet does not exist or has no associated master.
    #[must_use]
    pub fn get_master<P>(&self) -> Option<Pid>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.pptr.get_puppet_master_by_pid(puppet)
    }

    /// Sets the master of the puppet of type `P` to the master of type `M`.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetOperationError` if the master or puppet does not exist, or if the master
    /// does not have permission to set the puppet's master.
    pub fn set_master<P, M>(&self) -> Result<(), PuppetOperationError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.pptr
            .set_puppet_master_by_pid(self.pid, master_pid, puppet_pid)
    }

    /// Detaches the puppet of the specified type from its current master.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetOperationError` if the puppet does not exist or if the current master
    /// does not have permission to detach the puppet.
    pub fn detach_puppet<P>(&self) -> Result<(), PuppetOperationError>
    where
        P: Lifecycle,
    {
        let puppet_pid = Pid::new::<P>();
        self.pptr.detach_puppet_by_pid(self.pid, puppet_pid)
    }

    /// Checks if the master of type `M` has permission over the puppet of type `P`.
    ///
    /// Returns `Some(true)` if the master has permission, `Some(false)` if not, and `None` if
    /// either the master or puppet does not exist.
    #[must_use]
    pub fn has_permission<M, P>(&self) -> Option<bool>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.pptr
            .puppet_has_permission_by_pid(master_pid, puppet_pid)
    }

    /// Spawns a new puppet of type `P` using the provided `PuppetBuilder`.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet fails to spawn or initialize.
    pub async fn spawn<P, B>(&self, builder: B) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
        B: Into<PuppetBuilder<P>> + Send,
    {
        self.pptr.spawn_puppet_by_pid::<P>(self.pid, builder).await
    }

    /// Reports an unrecoverable failure.
    ///
    /// This method sends the provided `CriticalError` to the failure channel, indicating an
    /// unrecoverable failure in the puppet system.
    ///
    /// # Panics
    ///
    /// Panics if sending the failure to the channel fails.
    pub fn report_unrecoverable_failure(&self, error: CriticalError) {
        self.pptr
            .failure_tx
            .send(error)
            .expect("Failed to report unrecoverable failure");
    }

    /// Reports a failure to the puppet's master.
    ///
    /// This method reports the provided `PuppetError` to the puppet's master for handling. If the
    /// error is non-critical, it is logged and ignored. If the error is critical and the puppet is
    /// its own master, it attempts to restart itself. If the puppet has a different master, it sends
    /// a failure report to the master for handling.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the failure reporting fails or if the puppet's master does not exist.
    #[async_recursion]
    pub async fn report_failure<P>(
        &self,
        puppet: &mut P,
        error: PuppetError,
    ) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        if matches!(error, PuppetError::NonCritical(_)) {
            debug!(error = %error, "Non critical error reported");
            return Ok(());
        }

        let Some(master_pid) = self.get_master::<P>() else {
            return self.fail(puppet).await;
        };

        if master_pid == self.pid {
            match self.restart(puppet).await {
                Ok(()) | Err(PuppetError::NonCritical(_)) => return Ok(()),
                Err(PuppetError::Critical(err)) => {
                    self.report_unrecoverable_failure(err);
                    Ok(())
                }
            }
        } else if let Some(service_postman) = self.pptr.get_service_postman_by_pid(master_pid) {
            service_postman
                .send(
                    self.pid,
                    ServiceCommand::ReportFailure {
                        pid: self.pid,
                        error,
                    },
                )
                .await
                .map_err(|err| self.critical_error(&err))
        } else {
            Err(PuppetDoesNotExistError::new(master_pid).into())
        }
    }

    /// Handles an error reported by a child puppet.
    ///
    /// This method handles the provided `PuppetError` reported by a child puppet identified by `pid`.
    /// If the error is non-critical, it is ignored. If the error is critical, it attempts to handle
    /// the failure based on the puppet's supervision strategy.
    pub async fn handle_child_error<P>(&mut self, puppet: &mut P, pid: Pid, error: PuppetError)
    where
        P: Lifecycle,
    {
        match error {
            // Do nothing
            PuppetError::NonCritical(_) => {}
            PuppetError::Critical(_) => {
                if let Err(err) =
                    <P as Lifecycle>::Supervision::handle_failure(&self.pptr, self.pid, pid).await
                {
                    match err {
                        // Do nothing
                        PuppetError::NonCritical(_) => {}
                        PuppetError::Critical(err) => {
                            // If the restart command fails, report the failure to the master.
                            let _ = self.report_failure(puppet, err.into()).await;
                        }
                    }
                }
            }
        };
    }

    /// Sends a message of type `E` to the puppet of type `P`.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if the message fails to send or if the puppet does not exist.
    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.pptr.send::<P, E>(message).await
    }

    /// Sends a message of type `E` to the puppet of type `P` and awaits a response.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if the message fails to send, if the puppet does not exist,
    /// or if receiving the response fails.
    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.pptr.ask::<P, E>(message).await
    }

    /// Sends a message of type `E` to the puppet of type `P` with a timeout and awaits a response.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if the message fails to send, if the puppet does not exist,
    /// if receiving the response fails, or if the timeout is exceeded.
    pub async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.pptr.ask_with_timeout::<P, E>(message, duration).await
    }

    /// Sends a message of type `E` to the puppet of type `P` without awaiting a response.
    ///
    /// This method sends the message asynchronously and does not wait for a response.
    pub fn cast<P, E>(&self, message: E)
    where
        P: Handler<E>,
        E: Message,
    {
        self.pptr.cast::<P, E>(message);
    }

    /// Sends a `ServiceCommand` to the puppet of type `P`.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendCommandError` if the command fails to send or if the puppet does not exist.
    pub async fn send_command<P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError>
    where
        P: Lifecycle,
    {
        self.send_command_by_pid(Pid::new::<P>(), command).await
    }

    /// Sends a `ServiceCommand` to the puppet identified by `puppet`.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendCommandError` if the command fails to send or if the puppet does not exist.
    pub(crate) async fn send_command_by_pid(
        &self,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError> {
        self.pptr
            .send_command_by_pid(self.pid, puppet, command)
            .await
    }

    /// Handles a `ServiceCommand` received by the puppet.
    ///
    /// This method processes the received `ServiceCommand` and performs the corresponding action on
    /// the puppet, such as starting, stopping, restarting, or failing.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the command handling fails.
    pub(crate) async fn handle_command<P>(
        &mut self,
        puppet: &mut P,
        cmd: ServiceCommand,
    ) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        match cmd {
            ServiceCommand::Start => Ok(self.start(puppet, false).await?),
            ServiceCommand::Stop => Ok(self.stop(puppet, false).await?),
            ServiceCommand::Restart { stage } => {
                match stage {
                    None => Ok(self.restart(puppet).await?),
                    Some(RestartStage::Start) => Ok(self.start(puppet, true).await?),
                    Some(RestartStage::Stop) => Ok(self.stop(puppet, true).await?),
                }
            }
            ServiceCommand::Fail => {
                self.fail(puppet).await?;
                Ok(())
            }
            ServiceCommand::ReportFailure { pid, error } => {
                self.handle_child_error(puppet, pid, error).await;
                Ok(())
            }
        }
    }

    /// Starts all the puppets associated with the current puppet.
    ///
    /// This method sends a start command to all the puppets associated with the current puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if starting any of the associated puppets fails.
    pub(crate) async fn start_all_puppets(
        &self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully started.
        let mut started_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.pptr.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet to start it.
            for puppet in puppets {
                if self.pid == puppet {
                    continue;
                }
                // Attempt to send the start command to the current puppet.
                match self.send_command_by_pid(puppet, command.clone()).await {
                    // If successful, push the puppet to our vector of started puppets.
                    Ok(()) => started_puppets.push(puppet),

                    // If an error occurs, stop all puppets that have been started so far.
                    Err(err) => {
                        for started_puppet in started_puppets {
                            // Attempt to send the stop command to the started puppet.
                            if let Err(err) = self
                                .send_command_by_pid(started_puppet, ServiceCommand::Stop)
                                .await
                            {
                                return Err(err.into());
                            }
                        }
                        // Return the error after stopping all started puppets.
                        return Err(err.into());
                    }
                }
            }
        }
        // If we reach here, all puppets were started successfully.
        Ok(())
    }

    /// Stops all the puppets associated with the current puppet.
    ///
    /// This method sends a stop command to all the puppets associated with the current puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if stopping any of the associated puppets fails.
    pub(crate) async fn stop_all_puppets(
        &self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully stopped.
        let mut stopped_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.pptr.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet in reverse to stop it.
            for puppet in puppets.iter().rev() {
                if self.pid == *puppet {
                    continue;
                }
                // Attempt to send the stop command to the current puppet.
                match self.send_command_by_pid(*puppet, command.clone()).await {
                    // If successful, push the puppet to our vector of stopped puppets.
                    Ok(()) => stopped_puppets.push(puppet),

                    // Stopping is crucial, so if an error occurs, poison all puppets.
                    Err(err) => {
                        for stopped_puppet in stopped_puppets {
                            // Attempt to send the start command to the stopped puppet.
                            if let Err(err) = self
                                .send_command_by_pid(*stopped_puppet, ServiceCommand::Start)
                                .await
                            {
                                return Err(err.into());
                            }
                        }
                        // Return the error after starting all stopped puppets.
                        return Err(err.into());
                    }
                }
            }
        }
        // If we reach here, all puppets were stopped successfully.
        Ok(())
    }

    /// Fails all the puppets associated with the current puppet.
    ///
    /// This method sends a fail command to all the puppets associated with the current puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if failing any of the associated puppets fails.
    pub(crate) async fn fail_all_puppets<P>(&self, puppet: &mut P) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.pptr.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet in reverse to stop it.
            for pid in puppets.iter().rev() {
                // Attempt to send the stop command to the current puppet.
                if let Err(error) = self.send_command_by_pid(*pid, ServiceCommand::Fail).await {
                    if let Err(err) = self.report_failure(puppet, error.into()).await {
                        return Err(self.critical_error(&err));
                    }
                }
            }
        };
        Ok(())
    }

    /// Adds a new resource to the resource collection.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime. If a
    /// resource with the same type already exists, an error will be returned.
    ///
    /// # Errors
    ///
    /// Returns a `ResourceAlreadyExist` error if a resource of the same type already exists in the
    /// collection.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock fails.
    pub fn add_resource<T>(&self, resource: T) -> Result<(), ResourceAlreadyExist>
    where
        T: Send + Sync + Clone + 'static,
    {
        self.pptr.add_resource(resource)
    }

    /// Retrieves a cloned copy of the resource of type `T`, if it exists.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// Returns `Some(T)` if the resource exists, otherwise `None`.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock fails.
    #[must_use]
    pub fn get_resource<T>(&self) -> Option<T>
    where
        T: Send + Sync + Clone + 'static,
    {
        self.pptr.get_resource::<T>()
    }

    /// Borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// Returns `Some(R)` if the resource exists, where `R` is the return type of the closure `f`.
    /// Returns `None` if the resource doesn't exist.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock fails.
    pub fn with_resource<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&T) -> R,
    {
        self.pptr.with_resource::<T, F, R>(f)
    }

    /// Mutably borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// Returns `Some(R)` if the resource exists, where `R` is the return type of the closure `f`.
    /// Returns `None` if the resource doesn't exist.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock fails.
    pub fn with_resource_mut<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&mut T) -> R,
    {
        self.pptr.with_resource_mut::<T, F, R>(f)
    }

    /// Retrieves a cloned copy of the resource of type `T`, or panics if it doesn't exist.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Panics
    ///
    /// Panics if the resource doesn't exist or if the mutex lock fails.
    #[must_use]
    pub fn expect_resource<T>(&self) -> T
    where
        T: Send + Sync + Clone + 'static,
    {
        self.pptr.expect_resource::<T>()
    }

    /// Creates a new non-critical `PuppetError` with the given error message.
    ///
    /// The error message can be any type that implements `ToString`.
    pub fn non_critical_error<E: ToString + ?Sized>(&self, error: &E) -> PuppetError {
        PuppetError::non_critical(self.pid, error)
    }

    /// Creates a new critical `PuppetError` with the given error message.
    ///
    /// The error message can be any type that implements `ToString`.
    pub fn critical_error<E: ToString + ?Sized>(&self, error: &E) -> PuppetError {
        PuppetError::critical(self.pid, error)
    }
}

/// Represents the response type for a handler of message type `E` in puppet type `P`.
pub type ResponseFor<P, E> = <P as Handler<E>>::Response;

/// Defines the `Handler` trait for handling messages of type `E` in a puppet.
///
/// The `Handler` trait is implemented by puppets to define how they handle specific message types.
/// It requires the implementation of the `handle_message` method, which processes the received
/// message and returns a response.
#[async_trait]
pub trait Handler<E>: Lifecycle
where
    E: Message,
{
    /// The type of the response returned by the handler.
    type Response: Send + 'static;
    /// The type of the executor used to handle the message.
    type Executor: Executor<E> + Send + 'static;

    /// Handles the received message and returns a response.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the message handling fails.
    async fn handle_message(
        &mut self,
        msg: E,
        ctx: &Context,
    ) -> Result<Self::Response, PuppetError>;
}

/// Represents a handle to a puppet of type `P`.
///
/// The `PuppetHandle` provides access to the puppet's lifecycle status, message receiver, and
/// command receiver. It allows interacting with the puppet by sending messages and commands.
///
/// The generic parameter `P` specifies the type of the puppet and must implement the `Lifecycle`
/// trait.
///
/// # Fields
///
/// - `pid`: The process ID (`Pid`) of the puppet.
/// - `status_rx`: A `watch::Receiver` for receiving updates on the puppet's lifecycle status.
/// - `message_rx`: A `Mailbox<P>` for receiving messages specific to the puppet type `P`.
/// - `command_rx`: A `ServiceMailbox` for receiving general commands.
#[derive(Debug)]
pub(crate) struct PuppetHandle<P>
where
    P: Lifecycle,
{
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_rx: Mailbox<P>,
    pub(crate) command_rx: ServiceMailbox,
}

#[allow(dead_code, unused_imports)]
#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::{executor::ConcurrentExecutor, supervision::strategy::OneForAll};

    use super::*;

    // #[tokio::test]
    // async fn it_works() {
    //     #[derive(Debug, Clone, Message)]
    //     pub struct SleepMessage {
    //         i: i32,
    //     }
    //
    //     #[derive(Debug, Default, Clone)]
    //     pub struct MasterActor {}
    //
    //     impl Lifecycle for MasterActor {
    //         type Supervision = OneForAll;
    //     }
    //
    //     #[derive(Debug, Default, Clone)]
    //     pub struct SleepActor {
    //         i: i32,
    //     }
    //
    //     #[async_trait]
    //     impl Lifecycle for SleepActor {
    //         type Supervision = OneForAll;
    //     }
    //
    //     #[async_trait]
    //     impl Handler<SleepMessage> for SleepActor {
    //         type Response = i32;
    //         type Executor = ConcurrentExecutor;
    //
    //         async fn handle_message(
    //             &mut self,
    //             msg: SleepMessage,
    //             puppeter: &Puppeter,
    //         ) -> Result<Self::Response, PuppetError> {
    //             println!("Sleeping: {:?}", self.i);
    //             tokio::time::sleep(Duration::from_secs(1)).await;
    //             Ok(1)
    //         }
    //     }
    //
    //     let post_office = MasterOfPuppets::default();
    //
    //     let master = PuppetBuilder::new(MasterActor::default())
    //         .with_post_office(&post_office)
    //         .spawn()
    //         .await
    //         .unwrap();
    //
    //     let sleep_actor = PuppetBuilder::new(SleepActor::default())
    //         .with_post_office(&post_office)
    //         .spawn_link::<MasterActor>()
    //         .await
    //         .unwrap();
    //
    //     for _ in 0..5 {
    //         sleep_actor
    //             .send(SleepMessage { i: 1 })
    //             .await
    //             .expect("Failed to send message");
    //     }
    //
    //     // if let Err(err) = res {}
    //
    //     tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
    // }
}
