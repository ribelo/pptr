use std::{
    any::type_name,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
};

use async_recursion::async_recursion;
use async_trait::async_trait;
use futures::Future;
use rustc_hash::FxHashMap as HashMap;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

use crate::{
    address::Address,
    errors::{
        CriticalError, PuppetError, PuppetRegisterError, PuppetSendCommandError,
        PuppetSendMessageError,
    },
    executor::{Executor, SequentialExecutor},
    master::run_puppet_loop,
    message::{
        Envelope, Mailbox, Message, Postman, RestartStage, ServiceCommand, ServiceMailbox,
        ServicePacket, ServicePostman,
    },
    pid::{Id, Pid},
    post_office::{self, PostOffice},
    supervision::{SupervisionConfig, SupervisionStrategy},
    BoxedAny,
};

#[async_trait]
pub trait Lifecycle: Send + Sync + Sized + 'static {
    async fn on_init(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Initializing puppet {}", type_name::<Self>());
        Ok(())
    }
    async fn on_start(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Starting puppet: {}", type_name::<Self>());
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Stopping puppet {}", type_name::<Self>());
        Ok(())
    }
}

pub trait PuppetState: Send + Sync + Default + Clone + Default + 'static {}
impl<T> PuppetState for T where T: Send + Sync + Default + Clone + Default + 'static {}

#[derive(Debug, Clone, Copy, strum::Display, PartialEq, Eq)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Deactivating,
    Inactive,
    Restarting,
    Failed,
}

#[derive(Debug)]
pub struct Puppet<P>
where
    P: PuppetState,
    Self: Lifecycle,
{
    pub pid: Pid,
    pub state: P,
    pub(crate) status_tx: watch::Sender<LifecycleStatus>,
    pub(crate) message_tx: Postman<P>,
    pub(crate) context: Arc<Mutex<HashMap<Id, BoxedAny>>>,
    pub(crate) post_office: PostOffice,
    pub(crate) supervision_config: SupervisionConfig,
}

pub struct PuppetBuilder<P> {
    pub state: P,
    pub messages_bufer_size: NonZeroUsize,
    pub commands_bufer_size: NonZeroUsize,
    pub(crate) post_office: Option<PostOffice>,
    pub supervision_config: Option<SupervisionConfig>,
}

impl<P> PuppetBuilder<P>
where
    P: PuppetState,
{
    pub fn new(state: P) -> Self {
        Self {
            state,
            messages_bufer_size: NonZeroUsize::new(1024).unwrap(),
            commands_bufer_size: NonZeroUsize::new(16).unwrap(),
            post_office: None,
            supervision_config: Some(Default::default()),
        }
    }

    pub fn with_messages_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.messages_bufer_size = size;
        self
    }

    pub fn with_commands_bufer_size(mut self, size: NonZeroUsize) -> Self {
        self.commands_bufer_size = size;
        self
    }

    pub fn with_supervision_config(mut self, config: impl Into<SupervisionConfig>) -> Self {
        self.supervision_config = Some(config.into());
        self
    }

    pub fn with_post_office(mut self, post_office: &PostOffice) -> Self {
        self.post_office = Some(post_office.clone());
        self
    }

    pub async fn spawn(mut self) -> Result<Address<P>, PuppetError>
    where
        Puppet<P>: Lifecycle,
    {
        let post_office = self.post_office.take().unwrap_or_default();
        post_office.spawn::<P, P>(self).await
    }

    pub async fn spawn_link<M>(mut self) -> Result<Address<P>, PuppetError>
    where
        Puppet<P>: Lifecycle,
        M: PuppetState,
        Puppet<M>: Lifecycle,
    {
        let post_office = self.post_office.take().unwrap_or_default();
        post_office.spawn::<M, P>(self).await
    }
}

impl<S> Puppet<S>
where
    S: PuppetState,
    Self: Lifecycle,
{
    /// Asynchronously starts the operation of the puppet service.
    ///
    /// Starts or restarts the puppet service by coordinating initialization functions and the
    /// execution of all puppets. Handles failure modes with configurable retry logic.
    ///
    /// # Arguments
    ///
    /// * `is_restarting` - A boolean indicating whether the service is being restarted or not.
    ///
    /// # Errors
    ///
    /// Returns `Err(PuppetError)` if a critical or fatal error occurs during operations.
    ///
    /// # Panics
    ///
    /// This method does not explicitly panic.
    ///
    /// Note: Called functions such as `on_start` may panic depending on your code.
    pub(crate) async fn start(&mut self, is_restarting: bool) -> Result<(), CriticalError> {
        // Determine the service command and initial status based on whether the service is
        // restarting or not.
        let (service_command, begin_status) = match is_restarting {
            true => (ServiceCommand::Start, LifecycleStatus::Activating),
            false => {
                (
                    ServiceCommand::Restart {
                        stage: Some(RestartStage::Start),
                    },
                    LifecycleStatus::Restarting,
                )
            }
        };

        // Clone the retry config from the supervision config.
        // let mut retry_config = self.supervision_config.retry;

        // Flag to store if the `on_start` function has been completed.
        let mut on_start_done = false;

        // Flag to store if the `start_all_puppets` function has been completed.
        let mut start_all_puppets_done = false;

        loop {
            // Set the initial status of the puppet service.
            self.set_status(begin_status);

            if !on_start_done {
                // Perform the `on_start` function which initializes the puppet service.
                match self.on_start().await {
                    Ok(_) | Err(PuppetError::NonCritical(_)) => {
                        // If `on_start` succeeds or returns a non-critical error, set the status
                        // to `Active` and mark `on_start_done` as `true`.
                        on_start_done = true;
                        self.set_status(LifecycleStatus::Active);
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.supervision_config.retry.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `on_start`
                            // ReportFailure.

                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            self.supervision_config.retry.maybe_wait().await;
                            continue;
                        }
                    }
                }
            }

            if !start_all_puppets_done {
                // Start all puppets by calling the `start_all_puppets` function with the specified
                // service command.
                match self.start_all_puppets(&service_command).await {
                    Ok(_) | Err(PuppetError::NonCritical(_)) => {
                        // If `start_all_puppets` succeeds or returns a non-critical error, mark
                        // `start_all_puppets_done` as `true`.
                        start_all_puppets_done = true;
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.supervision_config.retry.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            self.report_failure(error.clone().into()).await;
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            self.supervision_config.retry.maybe_wait().await;
                            continue;
                        }
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

    /// Asynchronously stops the operation of the puppet service.
    ///
    /// Stops or restarts the puppet service by coordinating initialization functions and the
    /// execution of all puppets. Handles failure modes with configurable retry logic.
    ///
    /// # Arguments
    ///
    /// * `is_restarting` - A boolean indicating whether the service is being restarted or not.
    ///
    /// # Errors
    ///
    /// Returns `Err(PuppetError)` if a critical or fatal error occurs during operations.
    ///
    /// # Panics
    ///
    /// This method does not explicitly panic.
    ///
    /// Note: Called functions such as `on_stop` may panic depending on your code.
    pub(crate) async fn stop(&mut self, is_restarting: bool) -> Result<(), CriticalError> {
        // Determine the service command and initial status based on whether the service is
        // restarting or not.
        let (service_command, begin_status) = match is_restarting {
            true => (ServiceCommand::Start, LifecycleStatus::Activating),
            false => {
                (
                    ServiceCommand::Restart {
                        stage: Some(RestartStage::Start),
                    },
                    LifecycleStatus::Restarting,
                )
            }
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
                    Ok(_) | Err(PuppetError::NonCritical(_)) => stop_all_puppets_done = true,
                    Err(PuppetError::Critical(_)) => {
                        if self.supervision_config.retry.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `stop_all_puppets`,
                            // Mark tree as poisoned.
                            self.report_failure(error.clone().into()).await;
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            self.supervision_config.retry.maybe_wait().await;
                            continue;
                        }
                    }
                }
            }

            if !on_stop_done {
                match self.on_stop().await {
                    Ok(_) | Err(PuppetError::NonCritical(_)) => {
                        // If `on_stop` succeeds or returns a non-critical error, set the status
                        // to `Inactive` and mark `on_stop_done` as `true`.
                        on_stop_done = true;
                        self.set_status(LifecycleStatus::Inactive);
                    }
                    Err(PuppetError::Critical(_)) => {
                        if self.supervision_config.retry.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `on_stop`,
                            // Mark tree as poisoned.
                            self.report_failure(error.clone().into()).await;
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            self.supervision_config.retry.maybe_wait().await;
                            continue;
                        }
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

    /// Restarts the running puppet actor.
    ///
    /// # Errors
    ///
    /// Returns `PuppetError` if either `stop` or `start` functions encounter any errors during
    /// the execution.
    ///
    /// # Panics
    ///
    /// This method does not explicitly panic.
    ///
    /// Note: Called functions such as `on_start` or `or_stop` may panic depending
    /// on your configuration.
    pub(crate) async fn restart(&mut self) -> Result<(), CriticalError> {
        self.stop(true).await?;
        self.start(true).await?;
        Ok(())
    }

    pub(crate) async fn fail(&mut self) {
        self.fail_all_puppets().await;
        self.set_status(LifecycleStatus::Failed);
    }

    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        self.post_office.is_puppet_exists::<P>()
    }

    pub fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.post_office.get_status_by_pid(puppet)
    }

    pub(crate) fn set_status(&self, status: LifecycleStatus) {
        self.status_tx.send_if_modified(|s| {
            if s != &status {
                *s = status;
                true
            } else {
                false
            }
        });
    }

    pub fn has_puppet<M, P>(&self) -> Option<bool>
    where
        M: PuppetState,
        Puppet<M>: Lifecycle,
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let master = Pid::new::<M>();
        let puppet = Pid::new::<P>();
        self.post_office.has_puppet_by_pid(master, puppet)
    }

    pub fn get_master<P>(&self) -> Option<Pid>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.post_office.get_master_by_pid(puppet)
    }

    pub fn has_permission<M, P>(&self) -> Option<bool>
    where
        M: PuppetState,
        Puppet<M>: Lifecycle,
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let master = Pid::new::<M>();
        let puppet = Pid::new::<P>();
        self.post_office.has_permission_by_pid(master, puppet)
    }

    pub async fn spawn<P>(
        &self,
        builder: impl Into<PuppetBuilder<P>>,
    ) -> Result<Address<P>, PuppetError>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        self.post_office.spawn::<S, P>(builder).await
    }

    #[async_recursion]
    pub async fn report_failure(&mut self, error: PuppetError) {
        let master = self.get_master::<S>().expect("Master not found");
        let puppet = self.pid;

        if master == puppet {
            if (self.restart().await).is_err() {
                self.fail().await;
            } else {
                println!("Restarted");
            }
        } else {
            let service_postman = self
                .post_office
                .get_service_postman_by_pid(master)
                .clone()
                .expect("Service postman not found");

            service_postman
                .send(puppet, ServiceCommand::ReportFailure { puppet, error })
                .await
                .expect("Failed to report failure");
        }
    }

    pub async fn handle_child_error(&mut self, puppet: Pid, error: PuppetError) {
        match error {
            // Do nothing
            PuppetError::NonCritical(_) => {}
            PuppetError::Critical(_) => {
                if let Err(err) = self
                    .supervision_config
                    .handle_failure(&self.post_office, self.pid, puppet)
                    .await
                {
                    match err {
                        // Do nothing
                        PuppetError::NonCritical(_) => {}
                        PuppetError::Critical(err) => {
                            // If the restart command fails, report the failure to the master.
                            let _ = self.report_failure(err.into()).await;
                        }
                    }
                }
            }
        };
    }

    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        self.post_office.send::<P, E>(message).await
    }

    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        self.post_office.ask::<P, E>(message).await
    }

    pub async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        self.post_office
            .ask_with_timeout::<P, E>(message, duration)
            .await
    }

    pub async fn send_command<P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        self.send_command_by_pid(Pid::new::<P>(), command).await
    }

    pub(crate) async fn send_command_by_pid(
        &self,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError> {
        self.post_office
            .send_command_by_pid(self.pid, puppet, command)
            .await
    }

    pub(crate) async fn handle_command(&mut self, cmd: ServiceCommand) -> Result<(), PuppetError> {
        match cmd {
            ServiceCommand::Start => Ok(self.start(false).await?),
            ServiceCommand::Stop => Ok(self.stop(false).await?),
            ServiceCommand::Restart { stage } => {
                match stage {
                    None => Ok(self.restart().await?),
                    Some(RestartStage::Start) => Ok(self.start(true).await?),
                    Some(RestartStage::Stop) => Ok(self.stop(true).await?),
                }
            }
            ServiceCommand::Fail => {
                self.fail().await;
                Ok(())
            }
            ServiceCommand::ReportFailure { puppet, error } => {
                self.handle_child_error(puppet, error).await;
                Ok(())
            }
        }
    }

    pub(crate) async fn start_all_puppets(
        &mut self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully started.
        let mut started_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.post_office.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet to start it.
            for puppet in puppets {
                if self.pid == puppet {
                    continue;
                }
                // Attempt to send the start command to the current puppet.
                match self.send_command_by_pid(puppet, command.clone()).await {
                    // If successful, push the puppet to our vector of started puppets.
                    Ok(_) => started_puppets.push(puppet),

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
        &mut self,
        command: &ServiceCommand,
    ) -> Result<(), PuppetError> {
        // Initialize a vector to hold the puppets that have been successfully stopped.
        let mut stopped_puppets = Vec::new();

        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.post_office.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet in reverse to stop it.
            for puppet in puppets.iter().rev() {
                if self.pid == *puppet {
                    continue;
                }
                // Attempt to send the stop command to the current puppet.
                match self.send_command_by_pid(*puppet, command.clone()).await {
                    // If successful, push the puppet to our vector of stopped puppets.
                    Ok(_) => stopped_puppets.push(puppet),

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

    pub(crate) async fn fail_all_puppets(&mut self) {
        // Try to fetch the puppets by the given pid.
        if let Some(puppets) = self.post_office.get_puppets_by_pid(self.pid) {
            // Iterate through each puppet in reverse to stop it.
            for puppet in puppets.iter().rev() {
                // Attempt to send the stop command to the current puppet.
                self.send_command_by_pid(*puppet, ServiceCommand::Fail)
                    .await;
            }
        };
    }
}

pub type ResponseFor<P, E> = <Puppet<P> as Handler<E>>::Response;

#[async_trait]
pub trait Handler<E>: Lifecycle
where
    E: Message,
{
    type Response: Send + 'static;
    type Executor: Executor<E> + Send + 'static = SequentialExecutor;

    async fn handle_message(&mut self, msg: E) -> Result<Self::Response, PuppetError>;
}

#[derive(Debug)]
pub struct PuppetHandle<P>
where
    P: PuppetState,
    Puppet<P>: Lifecycle,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_rx: Mailbox<P>,
    pub(crate) command_rx: ServiceMailbox,
}

#[allow(dead_code)]
#[cfg(test)]
mod tests {

    use praxis_derive::Message;

    use crate::errors::NonCriticalError;

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone, Message)]
        pub struct SleepMessage {
            i: i32,
        }

        #[derive(Debug, Default, Clone)]
        pub struct MasterActor {}

        impl Lifecycle for Puppet<MasterActor> {}

        #[derive(Debug, Default, Clone)]
        pub struct SleepActor {
            i: i32,
        }

        #[async_trait]
        impl Lifecycle for Puppet<SleepActor> {
            async fn on_start(&mut self) -> Result<(), PuppetError> {
                println!("Starting:");
                Ok(())
            }
            async fn on_stop(&mut self) -> Result<(), PuppetError> {
                println!("Stopping:");
                Ok(())
            }
        }

        #[async_trait]
        impl Handler<SleepMessage> for Puppet<SleepActor> {
            type Response = i32;

            async fn handle_message(
                &mut self,
                msg: SleepMessage,
            ) -> Result<Self::Response, PuppetError> {
                println!("Sleeping: {:?}", self.state.i);
                Err(CriticalError::new(self.pid, "foo").into())
            }
        }

        let post_office = PostOffice::default();

        let master = PuppetBuilder::new(MasterActor::default())
            .with_post_office(&post_office)
            .spawn()
            .await
            .unwrap();

        let sleep_actor = PuppetBuilder::new(SleepActor::default())
            .with_post_office(&post_office)
            .spawn_link::<MasterActor>()
            .await
            .unwrap();

        let res = sleep_actor.ask(SleepMessage { i: 1 }).await;

        // if let Err(err) = res {}

        tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
    }
}
