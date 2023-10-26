use core::fmt;
use std::{
    any::{type_name, Any},
    sync::OnceLock,
};

use async_trait::async_trait;

use crate::{
    address::PuppetAddress,
    errors::{
        PermissionDenied, PuppetError, PuppeterSendCommandError, PuppeterSendMessageError,
        PuppeterSpawnError,
    },
    executor::{Executor, SequentialExecutor},
    id::Id,
    master::Master,
    message::{Mailbox, Message, ServiceCommand, ServiceMailbox},
    prelude::Puppeter,
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

#[async_trait]
pub trait Puppet: Master + Send + Sync + Sized + Clone + Default + 'static {
    // type SupervisorStrategy: SupervisorStrategy = supervisor_strategy::OneToOne;

    async fn pre_start(&mut self) -> Result<(), PuppetError<Self>> {
        Ok(())
    }

    async fn start(&mut self) -> Result<(), PuppetError<Self>> {
        tracing::debug!("Starting puppet: {}", type_name::<Self>());
        Ok(())
    }

    async fn post_start(&mut self) -> Result<(), PuppetError<Self>> {
        Ok(())
    }

    async fn pre_stop(&mut self) -> Result<(), PuppetError<Self>> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PuppetError<Self>> {
        tracing::debug!("Stopping puppet {}", type_name::<Self>());
        Ok(())
    }

    async fn post_stop(&mut self) -> Result<(), PuppetError<Self>> {
        Ok(())
    }

    async fn restart(&mut self) -> Result<(), PuppetError<Self>> {
        tracing::debug!("Restarting puppet {}", type_name::<Self>());
        Ok(())
    }

    async fn suicide(&mut self) -> Result<(), PuppetError<Self>> {
        tracing::debug!("Suicide puppet {}", type_name::<Self>());
        Ok(())
    }
}

#[async_trait]
pub trait Handler<M: Message>: Puppet {
    type Response: Send + 'static;
    type Executor: Executor<Self, M> = SequentialExecutor;

    async fn handle_message(&mut self, msg: M) -> Result<Self::Response, PuppetError<Self>>;
}

pub(crate) struct PuppetHandler<P: Puppet> {
    pub(crate) id: Id,
    pub(crate) rx: Mailbox<P>,
    pub(crate) command_rx: ServiceMailbox<P>,
}

impl<P: Puppet> fmt::Display for PuppetHandler<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Puppet {{ id: {}, name: {} }}",
            self.id.id,
            (self.id.get_name)()
        )
    }
}

pub struct PuppetStruct<P: Puppet> {
    pub(crate) puppet: P,
    pub(crate) buffer_size: usize,
    pub(crate) commands_buffer_size: usize,
}

impl<P: Puppet> PuppetStruct<P> {
    pub fn new(puppet: P) -> Self {
        Self {
            puppet,
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
