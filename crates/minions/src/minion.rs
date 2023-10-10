use std::{
    any::{type_name, TypeId},
    fmt::{self},
    hash::{Hash, Hasher},
    sync::OnceLock,
};

use ahash::AHasher;
use async_trait::async_trait;
use tokio::sync::watch;

use crate::{
    address::Address,
    gru::{self, set_status},
    message::{Mailbox, Message, Postman, ServiceCommand},
    MinionsError,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct MinionId {
    hash: u64,
}

impl fmt::Display for MinionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MinionId({})", self.hash)
    }
}

impl From<TypeId> for MinionId {
    fn from(value: TypeId) -> Self {
        let mut hasher = AHasher::default();
        value.hash(&mut hasher);
        MinionId {
            hash: hasher.finish(),
        }
    }
}

impl std::hash::Hash for MinionId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

#[async_trait]
pub trait Minion: Send + Sized + 'static {
    type Msg: Message + Clone;

    #[inline(always)]
    async fn pre_start(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    #[inline(always)]
    async fn post_start(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    #[inline(always)]
    async fn pre_stop(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    #[inline(always)]
    async fn post_stop(&mut self) -> Result<(), MinionsError> {
        Ok(())
    }

    #[inline(always)]
    async fn start(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Starting minion {}", type_name::<Self>());
        set_status::<Self>(LifecycleStatus::Activating);
        self.pre_start().await?;
        set_status::<Self>(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    #[inline(always)]
    async fn stop(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Stopping minion {}", type_name::<Self>());
        set_status::<Self>(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        set_status::<Self>(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    #[inline(always)]
    async fn restart(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Restarting minion {}", type_name::<Self>());
        set_status::<Self>(LifecycleStatus::Restarting);
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    #[inline(always)]
    async fn fail(&mut self) -> Result<(), MinionsError> {
        tracing::debug!("Failing minion {}", type_name::<Self>());
        set_status::<Self>(LifecycleStatus::Failed);
        self.stop().await?;
        Ok(())
    }

    async fn handle_message(&mut self, msg: Self::Msg) -> <Self::Msg as Message>::Response;

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

pub(crate) struct MinionHandle<A>
where
    A: Minion,
{
    pub status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) rx: Mailbox<A::Msg>,
    pub(crate) command_rx: Mailbox<ServiceCommand>,
}

pub struct MinionInstance<A>
where
    A: Minion,
{
    pub status_tx: watch::Sender<LifecycleStatus>,
    pub(crate) tx: Postman<A::Msg>,
    pub(crate) command_tx: Postman<ServiceCommand>,
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
