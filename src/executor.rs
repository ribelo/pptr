use std::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::{
    errors::PuppetError,
    message::Message,
    puppet::{Context, Handler},
};

#[async_trait]
pub trait Executor<E>
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Context,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E>;
}

pub(crate) struct SequentialExecutor;
pub(crate) struct ConcurrentExecutor;
pub(crate) struct DedicatedConcurrentExecutor;

#[derive(Debug, Clone, Copy, Default)]
pub enum ExecutorType {
    #[default]
    Sequential,
    Concurrent,
    Dedicated,
}

#[async_trait]
impl<E> Executor<E> for SequentialExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Context,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
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
                        PuppetError::critical(
                            pid,
                            "Failed to send response over the oneshot channel",
                        ),
                    )
                    .await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<E> Executor<E> for ConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Context,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E> + Clone,
    {
        let cloned_puppet = puppet.clone();
        let cloned_puppeter = puppeter.clone();
        tokio::spawn(async move {
            let mut local_puppet = cloned_puppet;
            let mut local_puppeter = cloned_puppeter;
            let _result = SequentialExecutor::execute(
                &mut local_puppet,
                &mut local_puppeter,
                msg,
                reply_address,
            )
            .await;
        });
        Ok(())
    }
}

struct Task {
    fut: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for Task {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        Pin::new(&mut self.fut).poll(cx)
    }
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("Worker gone")]
    WorkerGone,
    #[error("Panic: {message}")]
    Panic { message: String },
}

#[derive(Debug)]
pub struct Job<T> {
    cancel: CancellationToken,
    rx: oneshot::Receiver<Result<T, JobError>>,
}

impl<T> Future for Job<T> {
    type Output = Result<T, JobError>;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.cancel.is_cancelled() {
            return std::task::Poll::Ready(Err(JobError::WorkerGone));
        }
        match Pin::new(&mut self.rx).poll(cx) {
            std::task::Poll::Ready(Ok(res)) => std::task::Poll::Ready(res),
            std::task::Poll::Ready(Err(_)) => std::task::Poll::Ready(Err(JobError::WorkerGone)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Debug)]
pub struct DedicatedExecutorInner {
    tx: Option<std::sync::mpsc::Sender<Task>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct DedicatedExecutor {
    pub num_threads: usize,
    inner: Arc<Mutex<DedicatedExecutorInner>>,
}

impl Drop for DedicatedExecutor {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        drop(inner.tx.take());
        if let Some(thread) = inner.thread.take() {
            thread.join().unwrap();
        }
    }
}

impl DedicatedExecutor {
    #[must_use]
    pub fn new(num_threads: NonZeroUsize) -> Self {
        let (tx_tasks, rx_tasks) = std::sync::mpsc::channel::<Task>();

        let thread = std::thread::Builder::new()
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(num_threads.get())
                    .build()
                    .expect("Cannot create runtime");

                runtime.block_on(async move {
                    let mut set = tokio::task::JoinSet::new();

                    while let Ok(task) = rx_tasks.recv() {
                        set.spawn(task);
                    }

                    while (set.join_next().await).is_some() {}
                });
            })
            .expect("executor setup");

        let inner = DedicatedExecutorInner {
            tx: Some(tx_tasks),
            thread: Some(thread),
        };
        Self {
            num_threads: num_threads.get(),
            inner: Arc::new(Mutex::new(inner)),
        }
    }
    pub fn spawn<F, T>(&self, fut: F) -> Job<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let cancel = CancellationToken::new();
        let cloned_cancel = cancel.clone();
        let wrapped_fut = Box::pin(async move {
            tokio::select! {
                () = cloned_cancel.cancelled() => {
                    let _ = tx.send(Err(JobError::WorkerGone));
                },
                res = fut => {
                    let _ = tx.send(Ok(res));
                }
            }
        });
        let task = Task { fut: wrapped_fut };
        if let Some(tx) = &self.inner.lock().unwrap().tx {
            let _ = tx.send(task);
        }
        Job { cancel, rx }
    }
}

#[async_trait]
impl<E> Executor<E> for DedicatedConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        puppeter: &mut Context,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E> + Clone,
    {
        let cloned_puppet = puppet.clone();
        let cloned_puppeter = puppeter.clone();
        puppeter.pptr.executor.spawn(async move {
            let mut local_puppet = cloned_puppet;
            let mut local_puppeter = cloned_puppeter;
            let _result = SequentialExecutor::execute(
                &mut local_puppet,
                &mut local_puppeter,
                msg,
                reply_address,
            )
            .await;
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_dedicated_executor() {
        let start_time = std::time::Instant::now();
        let executor = DedicatedExecutor::new(std::num::NonZeroUsize::new(4).unwrap());
        let x = executor.spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("Hello from the x");
            "x"
        });
        let y = executor.spawn(async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!("Hello from the y");
            "y"
        });
        let z = executor.spawn(async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            println!("Hello from the z");
            "z"
        });
        let r = tokio::join!(x, y, z);
        assert_eq!(r.0.unwrap(), "x");
        assert_eq!(r.1.unwrap(), "y");
        assert_eq!(r.2.unwrap(), "z");
        let elapsed = start_time.elapsed();
        assert!(elapsed.as_millis() < 3100);
    }

    #[tokio::test]
    async fn test_dedicated_executor_puppet() {}
}
