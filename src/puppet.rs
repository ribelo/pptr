use std::{any::type_name, num::NonZeroUsize};

use async_recursion::async_recursion;
use async_trait::async_trait;
use tokio::sync::watch;
use tracing::debug;

use crate::{
    address::Address,
    errors::{
        CriticalError, PuppetDoesNotExistError, PuppetError, PuppetOperationError,
        PuppetSendCommandError, PuppetSendMessageError,
    },
    executor::Executor,
    master_of_puppets::MasterOfPuppets,
    message::{Mailbox, Message, RestartStage, ServiceCommand, ServiceMailbox},
    pid::Pid,
    supervision::{RetryConfig, SupervisionStrategy},
};

#[allow(unused_variables)]
#[async_trait]
pub trait Lifecycle: Send + Sync + Sized + Clone + 'static {
    type Supervision: SupervisionStrategy + Send + Sync;

    async fn reset(&self, puppeter: &Puppeter) -> Result<Self, CriticalError>;

    async fn on_init(&mut self, puppeter: &Puppeter) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %type_name::<Self>(), "Initializing puppet");
        Ok(())
    }
    async fn on_start(&mut self, puppeter: &Puppeter) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %type_name::<Self>(), "Starting puppet" );
        Ok(())
    }

    async fn on_stop(&mut self, puppeter: &Puppeter) -> Result<(), PuppetError> {
        tracing::debug!(puppet = %type_name::<Self>(), "Stopping puppet");
        Ok(())
    }
}

pub trait Puppet: Send + Sync + Clone + 'static {}
impl<T> Puppet for T where T: Send + Sync + Clone + 'static {}

#[derive(Debug, Clone, Copy, strum::Display, PartialEq, Eq)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Deactivating,
    Inactive,
    Restarting,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Puppeter {
    pub pid: Pid,
    pub(crate) master_of_puppets: MasterOfPuppets,
    pub(crate) retry_config: RetryConfig,
}

pub struct PuppetBuilder<P>
where
    P: Lifecycle,
{
    pub pid: Pid,
    pub puppet: Option<P>,
    pub messages_bufer_size: NonZeroUsize,
    pub commands_bufer_size: NonZeroUsize,
    pub retry_config: Option<RetryConfig>,
}

impl<P> PuppetBuilder<P>
where
    P: Lifecycle,
{
    pub fn new(state: P) -> Self {
        Self {
            pid: Pid::new::<P>(),
            puppet: Some(state),
            // SAFETY: NonZeroUsize::new_unchecked is safe because the value is known to be non-zero
            messages_bufer_size: unsafe { NonZeroUsize::new_unchecked(1024) },
            // SAFETY: NonZeroUsize::new_unchecked is safe because the value is known to be non-zero
            commands_bufer_size: unsafe { NonZeroUsize::new_unchecked(16) },
            retry_config: Some(RetryConfig::default()),
        }
    }

    #[must_use]
    pub fn with_messages_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.messages_bufer_size = size;
        self
    }

    #[must_use]
    pub fn with_commands_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.commands_bufer_size = size;
        self
    }

    pub async fn spawn(self, mop: &MasterOfPuppets) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        mop.spawn::<P, P>(self).await
    }

    pub async fn spawn_link<M>(self, mop: &MasterOfPuppets) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        mop.spawn::<M, P>(self).await
    }
}

impl Puppeter {
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
                            let error =
                                PuppetError::critical(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(PuppetError::critical(self.pid, &err));
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
                            let error =
                                PuppetError::critical(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(PuppetError::critical(self.pid, &err));
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
                            let error =
                                PuppetError::critical(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `stop_all_puppets`,
                            // Mark tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(PuppetError::critical(self.pid, &err));
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
                            let error =
                                PuppetError::critical(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `on_stop`,
                            // Mark tree as poisoned.
                            if let Err(err) = self.report_failure(puppet, error.clone()).await {
                                return Err(PuppetError::critical(self.pid, &err));
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

    #[must_use]
    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: Lifecycle,
    {
        self.master_of_puppets.is_puppet_exists::<P>()
    }

    #[must_use]
    pub fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.master_of_puppets.get_puppet_status_by_pid(puppet)
    }

    pub(crate) fn set_status(&self, status: LifecycleStatus) {
        self.master_of_puppets.set_status_by_pid(self.pid, status);
    }

    #[must_use]
    pub fn has_puppet<M, P>(&self) -> Option<bool>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.master_of_puppets
            .puppet_has_puppet_by_pid(master_pid, puppet_pid)
    }

    #[must_use]
    pub fn get_master<P>(&self) -> Option<Pid>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.master_of_puppets.get_puppet_master_by_pid(puppet)
    }

    pub fn set_master<P, M>(&self) -> Result<(), PuppetOperationError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.master_of_puppets
            .set_puppet_master_by_pid(self.pid, master_pid, puppet_pid)
    }

    pub fn detach_puppet<P>(&self) -> Result<(), PuppetOperationError>
    where
        P: Lifecycle,
    {
        let puppet_pid = Pid::new::<P>();
        self.master_of_puppets
            .detach_puppet_by_pid(self.pid, puppet_pid)
    }

    #[must_use]
    pub fn has_permission<M, P>(&self) -> Option<bool>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();
        self.master_of_puppets
            .puppet_has_permission_by_pid(master_pid, puppet_pid)
    }

    pub async fn spawn<P, B>(&self, builder: B) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
        B: Into<PuppetBuilder<P>> + Send,
    {
        self.master_of_puppets
            .spawn_puppet_by_pid::<P>(self.pid, builder)
            .await
    }

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
                Err(PuppetError::Critical(_)) => {
                    panic!("Failed to restart last puppet {}", self.pid)
                }
            }
        } else if let Some(service_postman) = self
            .master_of_puppets
            .get_service_postman_by_pid(master_pid)
        {
            service_postman
                .send(
                    self.pid,
                    ServiceCommand::ReportFailure {
                        pid: self.pid,
                        error,
                    },
                )
                .await
                .map_err(|err| PuppetError::critical(self.pid, &err))
        } else {
            Err(PuppetDoesNotExistError::new(master_pid).into())
        }
    }

    pub async fn handle_child_error<P>(&mut self, puppet: &mut P, pid: Pid, error: PuppetError)
    where
        P: Lifecycle,
    {
        match error {
            // Do nothing
            PuppetError::NonCritical(_) => {}
            PuppetError::Critical(_) => {
                if let Err(err) = <P as Lifecycle>::Supervision::handle_failure(
                    &self.master_of_puppets,
                    self.pid,
                    pid,
                )
                .await
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

    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.master_of_puppets.send::<P, E>(message).await
    }

    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.master_of_puppets.ask::<P, E>(message).await
    }

    pub async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        self.master_of_puppets
            .ask_with_timeout::<P, E>(message, duration)
            .await
    }

    pub async fn send_command<P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError>
    where
        P: Lifecycle,
    {
        self.send_command_by_pid(Pid::new::<P>(), command).await
    }

    pub(crate) async fn send_command_by_pid(
        &self,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError> {
        self.master_of_puppets
            .send_command_by_pid(self.pid, puppet, command)
            .await
    }

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

    pub(crate) async fn start_all_puppets(
        &self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully started.
        let mut started_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.master_of_puppets.get_puppets_by_pid(self.pid) {
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
    pub(crate) async fn stop_all_puppets(
        &self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully stopped.
        let mut stopped_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.master_of_puppets.get_puppets_by_pid(self.pid) {
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

    pub(crate) async fn fail_all_puppets<P>(&self, puppet: &mut P) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.master_of_puppets.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet in reverse to stop it.
            for pid in puppets.iter().rev() {
                // Attempt to send the stop command to the current puppet.
                if let Err(error) = self.send_command_by_pid(*pid, ServiceCommand::Fail).await {
                    if let Err(err) = self.report_failure(puppet, error.into()).await {
                        PuppetError::critical(self.pid, &err);
                    }
                }
            }
        };
        Ok(())
    }
}

pub type ResponseFor<P, E> = <P as Handler<E>>::Response;

#[async_trait]
pub trait Handler<E>: Lifecycle
where
    E: Message,
{
    type Response: Send + 'static;
    type Executor: Executor<E> + Send + 'static;

    async fn handle_message(
        &mut self,
        msg: E,
        puppeter: &Puppeter,
    ) -> Result<Self::Response, PuppetError>;
}

#[derive(Debug)]
pub struct PuppetHandle<P>
where
    P: Lifecycle,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_rx: Mailbox<P>,
    pub(crate) command_rx: ServiceMailbox,
}

#[allow(dead_code, unused_imports)]
#[cfg(test)]
mod tests {

    use std::time::Duration;

    use master_of_puppets_derive::Message;

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
