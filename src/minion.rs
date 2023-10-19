use core::fmt;
use std::{
    any::{type_name, Any},
    sync::OnceLock,
};

use async_trait::async_trait;

use crate::{
    address::Address,
    gru::{self, set_status},
    message::{Mailbox, Message, ServiceCommand, ServiceMailbox},
    Id, MinionsError,
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

#[derive(Debug)]
pub struct BoxedAny(Box<dyn Any + Send + Sync>);

impl BoxedAny {
    pub fn new<A>(minion: A) -> Self
    where
        A: Any + Send + Sync,
    {
        Self(Box::new(minion))
    }

    pub fn downcast_ref_unchecked<T>(&self) -> &T
    where
        T: Any + Send + Sync + 'static,
    {
        unsafe { self.0.downcast_ref_unchecked::<T>() }
    }

    pub fn downcast_mut_unchecked<T>(&mut self) -> &mut T
    where
        T: Any + Send + Sync + 'static,
    {
        unsafe { self.0.downcast_mut_unchecked::<T>() }
    }
}

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
        pub fn from_type<A: 'static + ?Sized>() -> Self {
            let type_id_a = TypeId::of::<A>();

            if type_id_a == TypeId::of::<Sequential>() {
                Self::Sequential
            } else if type_id_a == TypeId::of::<Concurrent>() {
                Self::Concurrent
            } else {
                #[cfg(feature = "rayon")]
                if type_id_a == TypeId::of::<Parallel>() {
                    return Self::Parallel;
                }

                unreachable!()
            }
        }
    }
}

#[async_trait]
pub trait Minion: Send + Sync + Sized + Clone + 'static {
    fn name(&self) -> String {
        type_name::<Self>().to_string()
    }

    async fn pre_start(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    async fn post_start(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    async fn pre_stop(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    async fn post_stop(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    async fn start(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Starting minion {}", self.name());
        set_status::<Self>(LifecycleStatus::Activating);
        self.pre_start().await?;
        set_status::<Self>(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Stopping minion {}", self.name());
        set_status::<Self>(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        set_status::<Self>(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    async fn restart(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Restarting minion {}", self.name());
        set_status::<Self>(LifecycleStatus::Restarting);
        self.pre_stop().await?;
        self.post_stop().await?;
        self.pre_start().await?;
        self.post_start().await?;
        set_status::<Self>(LifecycleStatus::Active);
        Ok(())
    }

    async fn fail(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Failing minion {}", self.name());
        set_status::<Self>(LifecycleStatus::Failed);
        self.pre_stop().await?;
        self.post_stop().await?;
        Ok(())
    }

    #[inline(always)]
    async fn handle_command(&mut self, cmd: ServiceCommand) -> Result<(), MinionsError> {
        match cmd {
            ServiceCommand::Start => self.start().await,
            ServiceCommand::Stop => self.stop().await,
            ServiceCommand::Restart => self.restart().await,
            ServiceCommand::Terminate => self.fail().await,
        }
    }
}

#[async_trait]
pub trait Handler<M: Message>: Minion {
    type Response: Send + 'static;
    type Exec: execution::ExecutionType = execution::Sequential;

    async fn handle_message(&mut self, msg: &M) -> Self::Response;
}

pub(crate) struct MinionHandler<A: Minion> {
    pub(crate) id: Id,
    pub(crate) name: String,
    pub(crate) rx: Mailbox<A>,
    pub(crate) command_rx: ServiceMailbox,
}

impl<A: Minion> fmt::Display for MinionHandler<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Minion {{ id: {}, name: {} }}", self.id, self.name)
    }
}

impl<A: Minion> fmt::Debug for MinionHandler<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MinionHandler")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("rx", &self.rx) // Dodaj, jeżeli Mailbox implementuje Debug
            .field("command_rx", &self.command_rx) // Dodaj, jeżeli ServiceMailbox implementuje Debug
            .finish()
    }
}

pub struct MinionStruct<A: Minion> {
    pub(crate) minion: A,
    pub(crate) buffer_size: usize,
    pub(crate) commands_buffer_size: usize,
}

impl<A: Minion> MinionStruct<A> {
    pub fn new(minion: A) -> Self {
        Self {
            minion,
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

    pub fn spawn(self) -> Result<Address<A>, MinionsError> {
        gru::spawn(self)
    }
}

impl<A: Minion> From<A> for MinionStruct<A> {
    fn from(value: A) -> Self {
        MinionStruct::new(value)
    }
}
