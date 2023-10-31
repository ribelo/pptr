use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::{
    errors::{CriticalError, PuppetError},
    message::Message,
    message::Packet,
    puppet::{Handler, Puppet, PuppetState},
};

#[async_trait]
pub trait Executor<E>
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut Puppet<P>,
        msg: E,
        reply_address: Option<
            oneshot::Sender<Result<<Puppet<P> as Handler<E>>::Response, PuppetError>>,
        >,
    ) where
        P: PuppetState,
        Puppet<P>: Handler<E>;
}

pub struct SequentialExecutor;
pub struct ConcurrentExecutor;
#[cfg(feature = "parallel")]
pub struct ParallelExecutor;

#[async_trait]
impl<E> Executor<E> for SequentialExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut Puppet<P>,
        msg: E,
        reply_address: Option<
            oneshot::Sender<Result<<Puppet<P> as Handler<E>>::Response, PuppetError>>,
        >,
    ) where
        P: PuppetState,
        Puppet<P>: Handler<E> + Clone,
    {
        let response = puppet.handle_message(msg).await;
        if let Err(err) = &response {
            puppet.report_failure(err.clone()).await;
        }
        if let Some(reply_address) = reply_address {
            if reply_address.send(response).is_err() {
                puppet
                    .report_failure(
                        CriticalError::new(
                            puppet.pid,
                            "Failed to send response over the oneshot channel",
                        )
                        .into(),
                    )
                    .await;
            }
        }
    }
}

#[async_trait]
impl<E> Executor<E> for ConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut Puppet<P>,
        msg: E,
        reply_address: Option<
            oneshot::Sender<Result<<Puppet<P> as Handler<E>>::Response, PuppetError>>,
        >,
    ) where
        P: PuppetState,
        Puppet<P>: Handler<E>,
    {
        let mut cloned_puppet = puppet.clone();
        tokio::spawn(async move {
            let response = cloned_puppet.handle_message(msg).await;
            if let Err(err) = &response {
                cloned_puppet.report_failure(err.clone()).await;
            }
            if let Some(reply_address) = reply_address {
                if reply_address.send(response).is_err() {
                    cloned_puppet
                        .report_failure(
                            CriticalError::new(
                                cloned_puppet.pid,
                                "Failed to send response over the oneshot channel",
                            )
                            .into(),
                        )
                        .await;
                }
            }
        });
    }
}
