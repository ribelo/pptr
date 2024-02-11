use std::any::Any;

// pub mod address;
pub mod address;
pub mod errors;
pub mod executor;
pub mod master_of_puppets;
pub mod message;
pub mod pid;
pub mod puppet;
pub mod supervision;

pub type BoxedAny = Box<dyn Any + Send + Sync>;

pub mod prelude {
    pub use crate::address::Address;
    pub use crate::errors::CriticalError;
    pub use crate::errors::NonCriticalError;
    pub use crate::errors::PuppetError;
    pub use crate::executor::ConcurrentExecutor;
    pub use crate::executor::Executor;
    pub use crate::executor::SequentialExecutor;
    pub use crate::master_of_puppets::MasterOfPuppets;
    pub use crate::message::Message;
    pub use crate::pid::Pid;
    pub use crate::puppet::Handler;
    pub use crate::puppet::Lifecycle;
    pub use crate::puppet::Puppet;
    pub use crate::puppet::PuppetBuilder;
    pub use crate::puppet::Puppeter;
    pub use crate::supervision::strategy::*;
    pub use master_of_puppets_derive::Message;
    pub use master_of_puppets_derive::Puppet;
}
