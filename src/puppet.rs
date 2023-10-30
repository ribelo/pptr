use std::{
    any::type_name,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
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
    master::{create_puppet_entities, run_puppet_loop},
    message::{
        Envelope, Mailbox, Message, Postman, RestartStage, ServiceCommand, ServiceMailbox,
        ServicePacket, ServicePostman,
    },
    pid::{Id, Pid},
    post_office::{self, PostOffice},
    supervision::SupervisionConfig,
    BoxedAny,
};

#[async_trait]
pub trait Lifecycle: Send + Sync + Sized + 'static {
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

#[derive(Debug, Clone)]
pub struct Puppet<P>
where
    P: PuppetState,
    Self: Lifecycle,
{
    pub pid: Pid,
    pub state: P,
    pub(crate) context: Arc<Mutex<HashMap<Id, BoxedAny>>>,
    pub(crate) post_office: PostOffice,
    pub(crate) supervision_config: Arc<SupervisionConfig>,
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
            commands_bufer_size: NonZeroUsize::new(1).unwrap(),
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

    pub fn with_post_office(mut self, post_office: impl Into<PostOffice>) -> Self {
        self.post_office = Some(post_office.into());
        self
    }

    pub async fn spawn(mut self) -> Result<Address<P>, PuppetError>
    where
        Puppet<P>: Lifecycle,
    {
        let pid = Pid::new::<P>();
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let (tx, rx) = mpsc::channel::<Box<dyn Envelope<P>>>(self.messages_bufer_size.into());
        let (command_tx, command_rx) =
            mpsc::channel::<ServicePacket>(self.commands_bufer_size.into());
        let postman = Postman::new(tx);
        let service_postman = ServicePostman::new(command_tx);
        let post_office = self.post_office.unwrap_or_default();
        post_office.register::<P, P>(
            postman.clone(),
            service_postman,
            status_tx,
            status_rx.clone(),
        )?;
        let supervision_config = Arc::new(self.supervision_config.take().unwrap());

        let mut puppet = Puppet {
            pid,
            state: self.state.clone(),
            context: Default::default(),
            post_office,
            supervision_config,
        };

        let handle = PuppetHandle {
            pid,
            status_rx: status_rx.clone(),
            message_rx: Mailbox::new(rx),
            command_rx: ServiceMailbox::new(command_rx),
        };

        let address = Address {
            pid,
            status_rx,
            message_tx: postman,
        };

        puppet.start(false).await?;

        tokio::spawn(run_puppet_loop(puppet, handle));
        Ok(address)
    }

    pub async fn spawn_link<M>(
        self,
        master: impl Into<Puppet<M>>,
    ) -> Result<Address<P>, PuppetError>
    where
        Puppet<P>: Lifecycle,
        M: PuppetState,
        Puppet<M>: Lifecycle,
    {
        let master = master.into();
        master.spawn(self).await
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
        let retry_config = self.supervision_config.retry.clone();

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
                        if retry_config.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `on_start`
                            // ReportFailure.

                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            retry_config.maybe_wait().await;
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
                        if retry_config.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during start");
                            // If the maximum retry attempts are reached during `start_all_puppets`
                            // Mark the tree as poisoned.
                            self.report_failure(error.clone());
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            retry_config.maybe_wait().await;
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
        let retry_config = self.supervision_config.retry.clone();

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
                        if retry_config.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `stop_all_puppets`,
                            // Mark tree as poisoned.
                            self.report_failure(error.clone());
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            retry_config.maybe_wait().await;
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
                        if retry_config.increment_retry().is_err() {
                            let error =
                                CriticalError::new(self.pid, "Max retry reached during stop");
                            // If the maximum retry attempts are reached during `on_stop`,
                            // Mark tree as poisoned.
                            self.report_failure(error.clone());
                            // And return a fatal error indicating the failure.
                            return Err(error);
                        } else {
                            // Increment the retry count, wait according to the retry config, and
                            // continue to the next iteration of the loop.
                            retry_config.maybe_wait().await;
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
        println!("Restarting puppet {}", type_name::<Self>());
        self.stop(true).await?;
        self.start(true).await?;

        Ok(())
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
        self.post_office.set_status_by_pid(self.pid, status);
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
        let mut builder = builder.into();
        let (mut puppet, handle, postman, service_postman, address) =
            create_puppet_entities(self, &mut builder);
        self.post_office.register::<S, P>(
            postman.clone(),
            service_postman,
            address.status_rx.clone(),
        )?;

        puppet.start(false).await?;

        tokio::spawn(run_puppet_loop(puppet, handle));
        Ok(address)
    }

    pub fn report_failure(&self, error: impl Into<PuppetError>) -> JoinHandle<()> {
        let error = error.into();
        let master = self.get_master::<S>().expect("Master not found");
        let puppet = self.pid;
        let service_postman = self
            .post_office
            .get_service_postman_by_pid(master)
            .clone()
            .expect("Service postman not found");
        tokio::spawn(async move {
            service_postman
                .send_and_await_response(
                    puppet,
                    ServiceCommand::ReportFailure { puppet, error },
                    None,
                )
                .await
                .expect("Failed to report failure");
        })
    }

    pub async fn handle_child_error(&mut self, puppet: Pid, error: PuppetError) {
        tracing::error!("Error: {:?}", error);
        match error {
            // Do nothing
            PuppetError::NonCritical(_) => {}
            PuppetError::Critical(_) => {
                if let Err(err) = self
                    .send_command_by_pid(puppet, ServiceCommand::Restart { stage: None })
                    .await
                {
                    match PuppetError::from(err) {
                        // Do nothing
                        PuppetError::NonCritical(_) => {}
                        PuppetError::Critical(err) => {
                            // If the restart command fails, report the failure to the master.
                            let _ = self.report_failure(err).await;
                        }
                    }
                }
            }
        };
    }

    //
    // fn create<P: Puppet>(
    //     &self,
    //     puppet: impl Into<Puppeter<P>>,
    // ) -> Result<PuppetAddress<P>, PuppeterSpawnError<P>>
    // where
    //     Self: Puppet,
    // {
    //     self.puppeter.spawn::<Self, P>(puppet)
    // }
    //

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
}

pub type ResponseFor<P, E> = <Puppet<P> as Handler<E>>::Response;

#[async_trait]
pub trait Handler<E>: Lifecycle
where
    E: Message,
{
    type Response: Send + 'static;
    // type Executor: Executor<Self, M> = SequentialExecutor;

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
                println!("Sleeping: {:?}", msg);
                Err(CriticalError::new(self.pid, "foo").into())
            }
        }

        let builder = PuppetBuilder::new(SleepActor::default());
        let postman = builder.spawn().await.unwrap();
        let res = postman.ask(SleepMessage { i: 1 }).await;

        if let Err(err) = res {
            println!("Error: {}", err);
        }

        tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
    }
}
