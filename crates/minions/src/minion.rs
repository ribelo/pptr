use std::{
    any::{type_name, Any, TypeId},
    fmt::{self, Debug},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use ahash::AHasher;
use async_trait::async_trait;
use thiserror::Error;

use crate::{
    gru::{self, GruError},
    message::{Mailbox, Message, Postman, ServiceMessage},
};
//
static DEFAULT_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
static DEFAULT_SERVICE_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();

#[derive(Debug, Error)]
pub enum MinionError {}

#[derive(Debug, Clone, strum::Display)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Inactive,
    Paused,
    Reloading,
    Failed,
}

#[derive(Debug)]
pub enum TransitionStatus {
    Success,
    Failed { strategy: Option<RecoveryStrategy> },
}

#[derive(Debug)]
pub enum RecoveryStrategy {
    RestartActor {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    StopActor {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    Retry {
        reason: Option<String>,
        max_retries: usize,
        delay: Option<std::time::Duration>,
        on_max_retries_exceeded: Box<RecoveryStrategy>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    Panic {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct MinionId {
    hash: u64,
}

impl fmt::Display for MinionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MinionId({})", self.hash)
    }
}

impl<A: Minion> From<A> for MinionId {
    fn from(value: A) -> Self {
        let id = TypeId::of::<A>();
        let mut hasher = AHasher::default();
        id.hash(&mut hasher);
        MinionId {
            hash: hasher.finish(),
        }
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
pub trait Minion: Send + Sync + 'static {
    type Msg: Message + Clone;

    async fn handle_message(
        &self,
        msg: Self::Msg,
    ) -> Result<<Self::Msg as Message>::Response, MinionError>;
}

pub(crate) struct MinionHandle<A>
where
    A: Minion,
{
    pub id: MinionId,
    pub status: Arc<Mutex<LifecycleStatus>>,
    pub(crate) rx: Mailbox<A::Msg>,
    pub(crate) service_rx: Mailbox<ServiceMessage>,
}

#[derive(Debug)]
pub(crate) struct MinionInstance<A>
where
    A: Minion,
{
    pub id: MinionId,
    pub status: Arc<Mutex<LifecycleStatus>>,
    pub(crate) tx: Postman<A::Msg>,
    pub(crate) service_tx: Postman<ServiceMessage>,
}

impl<A> Clone for MinionInstance<A>
where
    A: Minion,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            status: self.status.clone(),
            tx: self.tx.clone(),
            service_tx: self.service_tx.clone(),
        }
    }
}

#[derive(Debug)]
pub struct MinionStruct<A: Minion> {
    pub(crate) minion: A,
    pub(crate) buffer_size: usize,
    pub(crate) service_buffer_size: usize,
}

impl<A: Minion> MinionStruct<A> {
    pub fn new(minion: A) -> Self {
        Self {
            minion,
            buffer_size: *DEFAULT_BUFFER_SIZE.get_or_init(|| 1024),
            service_buffer_size: *DEFAULT_SERVICE_BUFFER_SIZE.get_or_init(|| 1),
        }
    }

    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    pub fn with_service_buffer_size(mut self, service_buffer_size: usize) -> Self {
        self.service_buffer_size = service_buffer_size;
        self
    }

    pub fn spawn(self) -> Result<(), GruError> {
        gru::spawn(self)
    }
}

impl<A: Minion> From<A> for MinionStruct<A> {
    fn from(value: A) -> Self {
        MinionStruct::new(value)
    }
}
