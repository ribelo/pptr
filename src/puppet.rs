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
    Id, PermissionError, PuppetError,
};

static DEFAULT_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
static DEFAULT_SERVICE_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();

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

pub type BoxedAny = Box<dyn Any + Send + Sync>;

pub mod execution {
    use std::any::TypeId;

    pub trait ExecutionStrategy {}

    pub struct Sequential;
    pub struct Concurrent;
    #[cfg(feature = "rayon")]
    pub struct Parallel;

    impl ExecutionStrategy for Sequential {}
    impl ExecutionStrategy for Concurrent {}
    #[cfg(feature = "rayon")]
    impl ExecutionStrategy for Parallel {}

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

pub trait SupervisorStrategy {}

pub mod supervisor_strategy {
    pub struct OneToOne;
    pub struct OneForAll;
    pub struct RestForOne;

    pub enum SupervisionVariant {
        OneToOne,
        OneForAll,
        RestForOne,
    }

    impl SupervisionVariant {
        pub fn from_type<P: 'static + ?Sized>() -> Self {
            let type_id = std::any::TypeId::of::<P>();
            if type_id == std::any::TypeId::of::<OneToOne>() {
                Self::OneToOne
            } else if type_id == std::any::TypeId::of::<OneForAll>() {
                Self::OneForAll
            } else if type_id == std::any::TypeId::of::<RestForOne>() {
                Self::RestForOne
            } else {
                unreachable!()
            }
        }
    }

    impl super::SupervisorStrategy for OneToOne {}
    impl super::SupervisorStrategy for OneForAll {}
    impl super::SupervisorStrategy for RestForOne {}
}

#[async_trait]
pub trait Puppet: Master + Send + Sync + Sized + Clone + Default + 'static {
    type SupervisorStrategy: SupervisorStrategy = supervisor_strategy::OneToOne;

    async fn _start(&mut self) -> Result<(), PuppetError> {
        self.set_status(LifecycleStatus::Activating);
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn _stop(&mut self) -> Result<(), PuppetError> {
        self.set_status(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        self.stop().await?;
        self.set_status(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    async fn _restart(&mut self) -> Result<(), PuppetError> {
        self.set_status(LifecycleStatus::Restarting);
        self.pre_stop().await?;
        self.stop().await?;
        self.post_stop().await?;
        *self = Default::default();
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn _suicide(&mut self) -> Result<(), PuppetError> {
        self._stop().await?;
        // TODO:
        Ok(())
    }

    async fn _handle_puppet_failure(&mut self, error: PuppetError) {
        match error {
            PuppetError::NonCritical(msg) => {
                tracing::warn!("");
            }
            PuppetError::Critical(msg) => {
                tracing::warn!("");
                puppeter().report_failure::<Self>(error).await;
            }
            PuppetError::Fatal(msg) => {
                tracing::error!("");
                puppeter().report_failure::<Self>(error).await;
            }
        }
    }

    fn get_name<P: Master>(&self) -> Option<String> {
        puppeter().get_puppet_name::<P>()
    }

    fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        puppeter().get_status::<P>()
    }

    fn set_status(&self, status: LifecycleStatus) -> Option<LifecycleStatus> {
        puppeter().set_status::<Self>(status)
    }

    async fn pre_start(&mut self) -> Result<(), PuppetError> {
        Ok(())
    }

    async fn start(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Starting puppet: {}", self.get_name::<Self>().unwrap());
        Ok(())
    }

    async fn post_start(&mut self) -> Result<(), PuppetError> {
        Ok(())
    }

    async fn pre_stop(&mut self) -> Result<(), PuppetError> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Stopping puppet {}", self.get_name::<Self>().unwrap());
        Ok(())
    }

    async fn post_stop(&mut self) -> Result<(), PuppetError> {
        Ok(())
    }

    async fn restart(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Restarting puppet {}", self.get_name::<Self>().unwrap());
        Ok(())
    }

    async fn suicide(&mut self) -> Result<(), PuppetError> {
        tracing::debug!("Suicide puppet {}", self.get_name::<Self>().unwrap());
        Ok(())
    }

    fn spawn(&self) -> Result<PuppetAddress<Self>, PermissionError> {
        puppeter().spawn::<Puppeter, Self>(self.clone())
    }

    fn create<P: Puppet>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PermissionError> {
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

    fn get_master<P>(&self) -> Id {
        puppeter().get_master_by_id(&TypeId::of::<Self>().into())
    }

    fn has_puppet<P>(&self) -> Option<bool>
    where
        P: Puppet,
    {
        puppeter().has_puppet::<Self, P>()
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

    async fn send<P, E>(&self, message: E) -> Result<(), PuppetError>
    where
        P: Handler<E>,
        E: Message,
    {
        if let Err(err) = puppeter().send::<Self, P, E>(message).await {
            self._handle_puppet_failure(PuppetError::NonCritical(err.to_string()))
                .await;
            Ok(())
        } else {
            Ok(())
        }
    }

    async fn ask<P, E>(&self, message: E) -> Result<P::Response, PuppetError>
    where
        P: Handler<E>,
        E: Message,
    {
        match puppeter().ask::<P, E>(message).await {
            Ok(_) => todo!(),
            Err(_) => todo!(),
        }
    }

    async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppetError>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter()
            .ask_with_timeout::<Self, P, E>(message, duration)
            .await
    }

    async fn send_command<P>(&self, command: ServiceCommand) -> Result<(), PuppetError>
    where
        P: Puppet,
    {
        match (self.is_puppet_exists::<P>(), self.has_puppet::<P>()) {
            (false, _) => {
                Err(PermissionError::PuppetDoesNotExist(
                    self.get_name::<P>().unwrap().into(),
                ))
            }
            (true, Some(false)) => {
                Err(PermissionError::PermissionDenied {
                    master: self.get_name::<Self>().unwrap().into(),
                    puppet: self.get_name::<P>().unwrap().into(),
                    message: Some("Can't send command to puppet from another master.".to_string()),
                })
            }
            (true, Some(true)) => {
                puppeter().send_command::<Self, P>(command).await?;
                Ok(())
            }
        }
    }

    async fn handle_command(&mut self, cmd: ServiceCommand) -> Result<(), PermissionError> {
        match cmd {
            ServiceCommand::Start => self._start().await,
            ServiceCommand::Stop => self._stop().await,
            ServiceCommand::Restart => self._restart().await,
            ServiceCommand::Kill => self._suicide().await,
            ServiceCommand::ReportFailure { puppet, message } => todo!(),
        }
    }
}

#[async_trait]
pub trait Handler<M: Message>: Puppet {
    type Response: Send + 'static;
    type Exec: execution::ExecutionStrategy = execution::Sequential;

    async fn try_handle_message(&mut self, msg: M) -> Result<Self::Response, PuppetError> {
        match self.handle_message(msg).await {
            Ok(response) => Ok(response),
            Err(error) => {
                self._handle_puppet_failure(error).await;
                Err(error)
            }
        }
    }

    async fn handle_message(&mut self, msg: M) -> Result<Self::Response, PuppetError>;
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
