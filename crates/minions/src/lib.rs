#![feature(lazy_cell)]
#![feature(downcast_unchecked)]

use std::{
    any::TypeId,
    fmt,
    hash::{Hash, Hasher},
};

use ahash::AHasher;
use minion::LifecycleStatus;
use thiserror::Error;

pub mod address;
pub mod context;
pub mod gru;
pub mod message;
pub mod minion;

#[derive(Debug, Error)]
pub enum MinionsError {
    #[error("Minion already exists: {0}")]
    MinionAlreadyExists(String),
    #[error("Minion does not exist: {0}")]
    MinionDoesNotExist(String),
    #[error("Minion cannot handle message. Status: {0}")]
    MinionCannotHandleMessage(LifecycleStatus),
    #[error("Timed out waiting for response from actor.")]
    MessageResponseTimeout,
    #[error("Error sending message.")]
    MessageSendError,
    #[error("Error receiving message.")]
    MessageReceiveError,
    #[error("Error receiving response from actor.")]
    MessageResponseReceiveError,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id {
    hash: u64,
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MinionId({})", self.hash)
    }
}

impl From<TypeId> for Id {
    fn from(value: TypeId) -> Self {
        let mut hasher = AHasher::default();
        value.hash(&mut hasher);
        Id {
            hash: hasher.finish(),
        }
    }
}

impl std::hash::Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}
