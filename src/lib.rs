#![feature(lazy_cell)]
#![feature(downcast_unchecked)]
#![feature(associated_type_defaults)]
#![feature(async_fn_in_trait)]
#![feature(const_trait_impl)]

use std::{
    any::TypeId,
    fmt,
    hash::{Hash, Hasher},
};

use ahash::AHasher;
use puppet::LifecycleStatus;
use thiserror::Error;

pub mod address;
pub mod master;
pub mod message;
pub mod puppet;
pub mod state;
pub mod supervision;

#[derive(Debug, Error)]
pub enum PuppeterError {
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
    #[error("Error sending response.")]
    MessageResponseSendError,
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

pub mod prelude {
    #[cfg(feature = "derive")]
    pub use minions_derive::*;

    pub use crate::{
        address::PuppetAddress,
        master::Puppeter,
        message::{Message, ServiceCommand},
        puppet::{execution, Puppet},
        // state::{expect_state, get_state, provide_state, with_state, with_state_mut},
    };
}
