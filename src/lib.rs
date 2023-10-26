#![feature(lazy_cell)]
#![feature(downcast_unchecked)]
#![feature(associated_type_defaults)]
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
pub mod id;
pub mod master;
pub mod message;
pub mod puppet;
pub mod puppet_box;
pub mod state;
pub mod supervision;

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
