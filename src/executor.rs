use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::{
    errors::PuppetError, master::puppeter, message::Message, message::Packet, puppet::Handler,
};

#[async_trait]
pub trait Executor<P, M>: Send
where
    P: Handler<M>,
    M: Message,
{
    async fn execute_message(
        puppet: &mut P,
        message: M,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<M>>::Response, PuppetError<P>>>>,
    );

    async fn report_failure(err: &PuppetError<P>) {
        puppeter().report_failure::<P>(err).await;
    }
}

pub struct SequentialExecutor;
pub struct ConcurrentExecutor;
#[cfg(feature = "parallel")]
pub struct ParallelExecutor;

#[async_trait]
impl<P, M> Executor<P, M> for SequentialExecutor
where
    P: Handler<M>,
    M: Message,
{
    async fn execute_message(
        puppet: &mut P,
        message: M,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<M>>::Response, PuppetError<P>>>>,
    ) {
        let response = puppet.handle_message(message).await;
        if let Some(reply_address) = reply_address {
            if let Err(_) = reply_address.send(response) {
                let err = &PuppetError::Critical("Can't send response".to_string());
                <P as Executor<_, _>>::report_failure(err).await;
            }
        };
    }
}

#[async_trait]
impl<P, M> Executor<P, M> for ConcurrentExecutor
where
    P: Handler<M>,
    M: Message,
{
    async fn execute_message(
        puppet: &mut P,
        message: M,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<M>>::Response, PuppetError<P>>>>,
    ) -> Result<(), PuppetError<P>> {
        tokio::spawn(async move {
            let response = puppet.handle_message(message).await;
            if let Some(reply_address) = reply_address {
                reply_address
                    .send(response)
                    .map_err(|_| PuppetError::Critical("Can't send response".to_string()));
            }
        });
        Ok(())
    }
}
