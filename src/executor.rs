use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::{
    errors::{CriticalError, PuppetError},
    message::Message,
    puppet::{Handler, Puppeter},
};

#[async_trait]
pub trait Executor<E>
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Puppeter,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), CriticalError>
    where
        P: Handler<E>;
}

pub struct SequentialExecutor;
pub struct ConcurrentExecutor;

#[async_trait]
impl<E> Executor<E> for SequentialExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Puppeter,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), CriticalError>
    where
        P: Handler<E>,
    {
        let pid = puppeter.pid;
        let response = puppet.handle_message(msg, puppeter).await;
        if let Err(err) = &response {
            puppeter.report_failure(puppet, err.clone()).await?;
        }
        if let Some(reply_address) = reply_address {
            if reply_address.send(response).is_err() {
                return puppeter
                    .report_failure(
                        puppet,
                        CriticalError::new(pid, "Failed to send response over the oneshot channel")
                            .into(),
                    )
                    .await;
            }
        }
        Ok(())
    }
}

#[allow(clippy::expect_used)]
#[async_trait]
impl<E> Executor<E> for ConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Puppeter,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), CriticalError>
    where
        P: Handler<E> + Clone,
    {
        let cloned_puppet = puppet.clone();
        let cloned_puppeter = puppeter.clone();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let mut local_puppet = cloned_puppet;
            let mut local_puppeter = cloned_puppeter;
            let result = SequentialExecutor::execute(
                &mut local_puppet,
                &mut local_puppeter,
                msg,
                reply_address,
            )
            .await;
            tx.send(result)
                .expect("Failed to send response over the oneshot channel");
        });
        if let Err(err) = rx.await {
            panic!(
                "Failed to receive response from the oneshot channel: {:?}",
                err
            );
        };
        Ok(())
    }
}
