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
    master::{puppeter, Master},
    message::{Mailbox, Message, ServiceCommand, ServiceMailbox},
    prelude::Puppeter,
    Id,
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

    async fn _start(&mut self) -> Result<(), PuppetError<Self>> {
        self.set_status(LifecycleStatus::Activating);
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn _stop(&mut self) -> Result<(), PuppetError<Self>> {
        self.set_status(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        self.stop().await?;
        self.set_status(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    async fn _restart(&mut self) -> Result<(), PuppetError<Self>> {
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

    async fn _suicide(&mut self) -> Result<(), PuppetError<Self>> {
        self._stop().await?;
        // TODO:
        Ok(())
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

    fn spawn(&self) -> Result<PuppetAddress<Self>, PuppeterSpawnError<Self>> {
        puppeter().spawn::<Puppeter, Self>(self.clone())
    }

    fn create<P: Puppet>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterSpawnError<P>> {
        puppeter().spawn::<Self, P>(puppet)
    }

    fn is_puppet_exists<M>(&self) -> bool
    where
        M: Master,
    {
        puppeter().is_puppet_exists::<M>()
    }

    fn get_master(&self) -> Id {
        puppeter().get_master::<Self>()
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

    // TODO:

    // fn get_command_address<P>(&self) -> Result<Option<CommandAddress<P>>,
    // PermissionDenied<Self, P>> where
    //     P: Puppet,
    // {
    //     puppeter().get_command_address::<Self, P>()
    // }

    async fn send<P, E>(&self, message: E) -> Result<(), PuppeterSendMessageError<P>>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter().send::<P, E>(message).await
    }

    async fn ask<P, E>(&self, message: E) -> Result<P::Response, PuppeterSendMessageError<P>>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter().ask::<P, E>(message).await
    }

    async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterSendMessageError<P>>
    where
        P: Handler<E>,
        E: Message,
    {
        puppeter().ask_with_timeout::<P, E>(message, duration).await
    }

    async fn send_command<P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppeterSendCommandError<Self, P>>
    where
        P: Puppet,
    {
        puppeter().send_command::<Self, P>(command).await
    }

    async fn handle_command(&mut self, cmd: ServiceCommand) -> Result<(), PuppetError<Self>> {
        match cmd {
            ServiceCommand::Start => self._start().await,
            ServiceCommand::Stop => self._stop().await,
            ServiceCommand::Restart => self._restart().await,
            ServiceCommand::Kill => self._suicide().await,
        }
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
