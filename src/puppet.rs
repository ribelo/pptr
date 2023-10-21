use core::fmt;
use std::{
    any::{type_name, Any, TypeId},
    sync::OnceLock,
};

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    address::{CommandAddress, PuppetAddress},
    master::{puppeter, Master},
    message::{Mailbox, Message, ServiceCommand, ServiceMailbox},
    prelude::Puppeter,
    Id, PuppeterError,
};

static DEFAULT_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
static DEFAULT_SERVICE_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();

#[derive(Error, Debug)]
pub enum HandlerError {
    #[error("Non-critical error occurred: '{0}'. Supervisor will not be notified.")]
    NonCritical(String),

    #[error("Critical error occurred: '{0}'. Supervisor will be notified.")]
    Critical(String),
}

#[derive(Debug, Clone, strum::Display, PartialEq, Eq)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Deactivating,
    Inactive,
    Restarting,
    Failed,
}

impl LifecycleStatus {
    pub fn should_handle_message(&self) -> bool {
        matches!(self, LifecycleStatus::Active)
    }
    pub fn should_drop_message(&self) -> bool {
        matches!(self, LifecycleStatus::Inactive | LifecycleStatus::Failed)
    }
    pub fn should_wait_for_activation(&self) -> bool {
        matches!(
            self,
            LifecycleStatus::Activating | LifecycleStatus::Restarting
        )
    }
}

impl Copy for LifecycleStatus {}

pub mod execution {
    use std::any::TypeId;

    pub trait ExecutionType {}

    pub struct Sequential;
    pub struct Concurrent;
    #[cfg(feature = "rayon")]
    pub struct Parallel;

    impl ExecutionType for Sequential {}
    impl ExecutionType for Concurrent {}
    #[cfg(feature = "rayon")]
    impl ExecutionType for Parallel {}

    pub enum ExecutionVariant {
        Sequential,
        Concurrent,
        #[cfg(feature = "rayon")]
        Parallel,
    }

    impl ExecutionVariant {
        pub fn from_type<P: 'static + ?Sized>() -> Self {
            let type_id = TypeId::of::<P>();

            if type_id == TypeId::of::<Sequential>() {
                Self::Sequential
            } else if type_id == TypeId::of::<Concurrent>() {
                Self::Concurrent
            } else {
                #[cfg(feature = "rayon")]
                if type_id == TypeId::of::<Parallel>() {
                    return Self::Parallel;
                }

                unreachable!()
            }
        }
    }
}

#[async_trait]
pub trait PuppetLifecycle: Puppet {
    type SupervisionStrategy: SupervisorStrategy = supervisor_strategy::OneToOne;

    fn get_id(&self) -> Id {
        TypeId::of::<Self>().into()
    }

    fn set_status(&mut self, status: LifecycleStatus) {
        puppeter().set_status::<Self>(status);
    }

    async fn start_tree(&mut self, sender: Id) -> Result<(), PuppeterError> {
        self.set_status(LifecycleStatus::Activating);
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        self.start_puppets().await?;
        Ok(())
    }

    async fn start_puppets(&mut self) -> Result<(), PuppeterError> {
        let puppets = puppeter().get_puppets::<Self>();
        for id in puppets {
            if let Some(service_address) = puppeter().get_command_address_by_id(&id) {
                service_address
                    .send_command(ServiceCommand::InitiateStart {
                        sender: self.get_id(),
                    })
                    .await
                    .unwrap();
            }
        }
        Ok(())
    }

    async fn stop_master(&mut self) -> Result<(), PuppeterError> {
        self.set_status(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        self.stop().await?;
        self.set_status(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    async fn stop_puppets(&mut self) -> Result<(), PuppeterError> {
        let puppets = puppeter().get_puppets::<Self>();
        for id in puppets.into_iter().rev() {
            if let Some(service_address) = puppeter().get_command_address_by_id(&id) {
                service_address
                    .command_tx
                    .send_and_await_response(ServiceCommand::InitiateStart {
                        sender: self.get_id(),
                    })
                    .await
                    .unwrap();
            }
        }
        Ok(())
    }

    async fn restart_master<M>(&mut self) -> Result<(), PuppeterError>
    where
        M: Master,
    {
        self.restart_puppets().await?;
        self.set_status(LifecycleStatus::Restarting);
        *self = Default::default();
        self.pre_stop().await?;
        self.stop().await?;
        self.post_stop().await?;
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        self.start_puppets().await?;
        Ok(())
    }

    async fn restart_puppets(&mut self) -> Result<(), PuppeterError> {
        let puppets = puppeter().get_puppets::<Self>();
        for id in puppets.into_iter().rev() {
            if let Some(service_address) = puppeter().get_command_address_by_id(&id) {
                service_address
                    .command_tx
                    .send_and_await_response(ServiceCommand::RequestSelfRestart {
                        sender: self.get_id(),
                    })
                    .await
                    .unwrap();
            }
        }
        Ok(())
    }

    async fn handle_puppet_failure(&mut self) -> Result<(), PuppeterError> {
        match supervisor_strategy::SupervisionVariant::from_type::<Self>() {
            supervisor_strategy::SupervisionVariant::OneToOne => {}
            supervisor_strategy::SupervisionVariant::OneForAll => todo!(),
            supervisor_strategy::SupervisionVariant::RestForOne => todo!(),
        }
        if Self::SupervisionStrategy == supervisor_strategy::OneToOne {
            self.stop_master::<P>().await?;
            puppeter().remove_master::<P, Self>();
        } else if Self::SupervisionStrategy == supervisor_strategy::OneForAll {
            self.stop_master::<P>().await?;
            puppeter().remove_master::<P, Self>();
            self.fail::<P>().await?;
        } else if Self::SupervisionStrategy == supervisor_strategy::RestForOne {
            self.fail::<P>().await?;
            self.stop_master::<P>().await?;
            puppeter().remove_master::<P, Self>();
        }
        Ok(())
    }

    async fn master_suicide<M>(&mut self) -> Result<(), PuppeterError>
    where
        M: Master,
    {
        self.stop_master::<M>().await?;
        puppeter().remove_master::<M, Self>();
        Ok(())
    }

    async fn handle_command(&mut self, cmd: ServiceCommand) -> Result<(), PuppeterError> {
        match cmd {
            ServiceCommand::InitiateStart { sender } => self.start_tree().await,
            ServiceCommand::InitiateStop { sender } => self.stop_master().await,
            ServiceCommand::RequestSelfRestart { sender } => self.restart_master().await,
            ServiceCommand::RequestTreeRestart { sender } => self.restart_master().await,
            ServiceCommand::ForceTermination { sender } => self.master_suicide().await,
            ServiceCommand::ReportFailure { sender, message } => self.handle_puppet_failure().await,
        }
    }
}

#[async_trait]
pub trait Puppet: Master + Send + Sync + Sized + Clone + Default + 'static {
    fn name(&self) -> String {
        type_name::<Self>().to_string()
    }

    async fn pre_start(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn post_start(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn pre_stop(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn post_stop(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }
    async fn start(&mut self) -> Result<(), PuppeterError> {
        // tracing::debug!(
        //     "Received start command for master: {}",
        //     puppeter().get_puppet_name::<M>().unwrap()
        // );
        tracing::debug!("Starting puppet: {}", self.name());
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PuppeterError> {
        // tracing::debug!(
        //     "Received stop command for master: {}",
        //     puppeter().get_puppet_name::<M>().unwrap()
        // );
        tracing::debug!("Stopping puppet {}", self.name());
        Ok(())
    }

    async fn restart(&mut self) -> Result<(), PuppeterError> {
        tracing::debug!("Restarting puppet {}", self.name());
        Ok(())
    }

    async fn terminate(&mut self) -> Result<(), PuppeterError> {
        // tracing::debug!(
        //     "Received kill command for master: {}",
        //     puppeter().get_puppet_name::<M>().unwrap()
        // );
        tracing::debug!("Suicide puppet {}", self.name());
        Ok(())
    }

    fn spawn(&self) -> Result<PuppetAddress<Self>, PuppeterError>
    where
        Self: PuppetLifecycle,
    {
        puppeter().spawn::<Puppeter, Self>(self.clone())
    }

    fn create<P>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterError>
    where
        P: PuppetLifecycle,
    {
        puppeter().spawn::<Self, P>(puppet)
    }

    fn get_puppet_name<P>(&self) -> Option<String>
    where
        P: Master,
    {
        puppeter().get_puppet_name::<P>()
    }

    fn is_puppet_exists<M>(&self) -> bool
    where
        M: Master,
    {
        puppeter().is_puppet_exists::<M>()
    }

    fn has_puppet<P>(&self) -> bool
    where
        P: Puppet,
    {
        puppeter().has_puppet::<Self, P>()
    }

    fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        puppeter().get_status::<P>()
    }
    fn get_address<P>(&self) -> Option<PuppetAddress<P>>
    where
        P: Puppet,
    {
        puppeter().get_address::<P>()
    }
    fn get_command_address<P>(&self) -> Option<CommandAddress>
    where
        P: Puppet,
    {
        puppeter().get_command_address::<P>()
    }
    async fn send<P, E>(&self, message: E) -> Result<(), PuppeterError>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter().send::<Self, P, E>(message).await
    }
    async fn ask<P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter().ask::<Self, P, E>(message).await
    }
    async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterError>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter()
            .ask_with_timeout::<Self, P, E>(message, duration)
            .await
    }
    async fn send_command<P>(&self, command: ServiceCommand) -> Result<(), PuppeterError>
    where
        P: Puppet,
    {
        puppeter().send_command::<Self, P>(command).await
    }
}

#[async_trait]
pub trait Handler<M: Message>: Puppet {
    type Response: Send + 'static;
    type Exec: execution::ExecutionType = execution::Sequential;

    async fn try_handle_message(&mut self, msg: M) -> Self::Response;

    async fn handle_message(&mut self, msg: M) -> Self::Response;
}

pub(crate) struct PuppetHandler<P: Puppet> {
    pub(crate) id: Id,
    pub(crate) name: String,
    pub(crate) rx: Mailbox<P>,
    pub(crate) command_rx: ServiceMailbox,
}

impl<P: Puppet> fmt::Display for PuppetHandler<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Puppet {{ id: {}, name: {} }}", self.id, self.name)
    }
}

impl<P: Puppet> fmt::Debug for PuppetHandler<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PuppetHandler")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("rx", &self.rx) // Dodaj, jeżeli Mailbox implementuje Debug
            .field("command_rx", &self.command_rx) // Dodaj, jeżeli ServiceMailbox implementuje Debug
            .finish()
    }
}

pub struct PuppetStruct<P: Puppet> {
    pub(crate) Puppet: P,
    pub(crate) buffer_size: usize,
    pub(crate) commands_buffer_size: usize,
}

impl<P: Puppet> PuppetStruct<P> {
    pub fn new(puppet: P) -> Self {
        Self {
            Puppet: puppet,
            buffer_size: *DEFAULT_BUFFER_SIZE.get_or_init(|| 1024),
            commands_buffer_size: *DEFAULT_SERVICE_BUFFER_SIZE.get_or_init(|| 1),
        }
    }

    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    pub fn with_service_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }
}

impl<P: Puppet> From<P> for PuppetStruct<P> {
    fn from(value: P) -> Self {
        PuppetStruct::new(value)
    }
}
