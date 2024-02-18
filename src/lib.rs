use std::any::Any;

// pub mod address;
pub mod address;
pub mod errors;
pub mod executor;
pub mod message;
pub mod pid;
pub mod puppet;
pub mod puppeter;
pub mod supervision;

pub type BoxedAny = Box<dyn Any + Send + Sync>;

pub mod prelude {
    pub use crate::address::Address;
    pub use crate::errors::CriticalError;
    pub use crate::errors::NonCriticalError;
    pub use crate::errors::PuppetError;
    pub use crate::executor::ExecutorType;
    pub use crate::message::Message;
    pub use crate::pid::Pid;
    pub use crate::puppet::Context;
    pub use crate::puppet::Handler;
    pub use crate::puppet::Lifecycle;
    pub use crate::puppet::Puppet;
    pub use crate::puppet::PuppetBuilder;
    pub use crate::puppeter::Puppeter;
    pub use crate::supervision::strategy::*;
}
