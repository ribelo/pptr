#![feature(lazy_cell)]
#![feature(downcast_unchecked)]
#![feature(associated_type_defaults)]
#![feature(async_fn_in_trait)]
#![feature(const_trait_impl)]

use std::{
    any::{type_name, TypeId},
    fmt,
    hash::{Hash, Hasher},
};

use ahash::AHasher;
use puppet::LifecycleStatus;
use thiserror::Error;

pub mod address;
pub mod errors;
pub mod executor;
pub mod master;
pub mod message;
pub mod puppet;
pub mod state;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id {
    id: u64,
    get_name: fn() -> String,
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", (self.get_name)())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Id")
            .field("id", &self.id)
            .field("name", &(&self.get_name)())
            .finish()
    }
}

impl std::hash::Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.id);
    }
}

impl Id {
    pub fn new<T>() -> Self
    where
        T: 'static,
    {
        let type_id = TypeId::of::<T>();
        let mut hasher = AHasher::default();
        type_id.hash(&mut hasher);
        Self {
            id: hasher.finish(),
            get_name: Self::name::<T>,
        }
    }
    fn name<T>() -> String
    where
        T: 'static,
    {
        std::any::type_name::<T>().to_string()
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        (value.get_name)()
    }
}

#[derive(Debug)]
pub enum PuppetIdentifier {
    Name(String),
    Id(Id),
}

impl fmt::Display for PuppetIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PuppetIdentifier::Name(name) => write!(f, "Name: {}", name),
            PuppetIdentifier::Id(id) => write!(f, "Id: {}", id),
        }
    }
}

impl From<&'static str> for PuppetIdentifier {
    fn from(s: &str) -> Self {
        PuppetIdentifier::Name(s.to_string())
    }
}

impl From<String> for PuppetIdentifier {
    fn from(s: String) -> Self {
        PuppetIdentifier::Name(s)
    }
}

impl From<Id> for PuppetIdentifier {
    fn from(id: Id) -> Self {
        PuppetIdentifier::Id(id)
    }
}

pub mod prelude {
    #[cfg(feature = "derive")]
    pub use puppeter_derive::*;

    pub use crate::{
        address::PuppetAddress,
        master::Puppeter,
        message::{Message, ServiceCommand},
        puppet::Puppet,
        // state::{expect_state, get_state, provide_state, with_state, with_state_mut},
    };
}
