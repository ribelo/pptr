use async_trait::async_trait;

use crate::{message::Message, message::Packet, PuppeterError};

use super::{Handler, Puppet};

#[async_trait]
pub trait Executor<P, M>: Send
where
    P: Handler<M>,
    M: Message,
{
    async fn execute_message(&self, puppet: P, message: M) -> Result<(), PuppeterError>;
}

pub struct SequentialExecutor;
pub struct ConcurrentExecutor;
#[cfg(feature = "parallel")]
pub struct ParallelExecutor;

// #[async_trait]
// impl<P, M> Executor<P> for Packet<P, M> {
//     async fn execute_message(&self, puppet: &mut P) -> Result<(),
// PuppeterError> {         todo!()
//     }
// }

impl<P, M> Executor<P, M> for SequentialExecutor
where
    P: Handler<M>,
    M: Message,
{
    async fn execute_message(&self, puppet: P, message: M) -> Result<(), PuppeterError> {
        puppet.handle_message(message).await;
        Ok(())
    }
}
