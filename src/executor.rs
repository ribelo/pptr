use std::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
};

use thiserror::Error;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::{
    errors::PuppetError,
    message::Message,
    puppet::{Context, Handler},
};

/// The `Executor` trait defines the execution strategy for handling messages in a puppet.
///
/// It provides an `execute` method that takes a mutable reference to the puppet, the Puppeteer context,
/// the message to be handled, and an optional reply address for sending the response back to the sender.
///
/// The trait is generic over the message type `E`, which must implement the `Message` trait.
pub trait Executor<E>
where
    E: Message,
{
    fn execute<P>(
        puppet: &mut P,
        puppeteer: &mut Context<P>,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> impl Future<Output = Result<(), PuppetError>> + Send
    where
        P: Handler<E>;
}

/// The `SequentialExecutor` is an implementation of the `Executor` trait that executes messages sequentially.
///
/// It handles messages one at a time, ensuring that each message is fully processed before moving on to the next one.
/// This executor is suitable for scenarios where message ordering is important and concurrent execution is not required.
pub struct SequentialExecutor;

/// The `ConcurrentExecutor` is an implementation of the `Executor` trait that executes messages concurrently.
///
/// It spawns a new task for each message, allowing multiple messages to be processed simultaneously.
/// This executor is suitable for scenarios where high throughput and concurrent execution are desired.
pub struct ConcurrentExecutor;

/// The `DedicatedConcurrentExecutor` is an implementation of the `Executor` trait that executes messages concurrently
/// using a dedicated thread pool.
///
/// It assigns each message to a dedicated thread from the thread pool, allowing for concurrent execution while
/// maintaining a fixed number of threads. This executor is suitable for CPU-intensive tasks or scenarios where
/// fine-grained control over the execution environment is required.
pub struct DedicatedConcurrentExecutor;

impl<E> Executor<E> for SequentialExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        ctx: &mut Context<P>,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E>,
    {
        let pid = ctx.pid;
        let response = puppet.handle_message(msg, ctx).await;
        if let Err(err) = &response {
            ctx.report_failure(puppet, err.clone()).await?;
        }
        if let Some(reply_address) = reply_address {
            if reply_address.send(response).is_err() {
                return ctx
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

impl<E> Executor<E> for ConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        ctx: &mut Context<P>,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E> + Clone,
    {
        let cloned_puppet = puppet.clone();
        let cloned_ctx = ctx.clone();
        tokio::spawn(async move {
            let mut local_puppet = cloned_puppet;
            let mut local_puppeteer = cloned_ctx;
            let _result = SequentialExecutor::execute(
                &mut local_puppet,
                &mut local_puppeteer,
                msg,
                reply_address,
            )
            .await;
        });
        Ok(())
    }
}

/// A task representing a boxed future that returns `()`.
///
/// The `Task` struct wraps a `Pin<Box<dyn Future<Output = ()> + Send>>`,
/// allowing it to be sent across thread boundaries.
struct Task {
    fut: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for Task {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        Pin::new(&mut self.fut).poll(cx)
    }
}

/// Represents the possible errors that can occur during the execution of a `Job`.
#[derive(Debug, Error)]
pub enum JobError {
    #[error("Worker gone")]
    WorkerGone,
    #[error("Panic: {message}")]
    Panic { message: String },
}

/// Represents a job that can be executed asynchronously and returns a result of type `T`.
///
/// A `Job` is a future that resolves to a `Result<T, JobError>`. It can be canceled using
/// the associated `CancellationToken`.
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

/// The inner state of a `DedicatedExecutor`.
///
/// This struct holds the sender end of a channel for sending tasks to the executor
/// and a handle to the executor thread.
#[derive(Debug)]
pub struct DedicatedExecutorInner {
    tx: Option<std::sync::mpsc::Sender<Task>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// A dedicated executor that runs tasks on a separate thread.
///
/// The `DedicatedExecutor` spawns a new thread with a specified number of worker threads
/// and executes tasks sent to it through a channel. It provides a way to offload tasks
/// to a dedicated thread for concurrent execution.
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
    /// Creates a new `DedicatedExecutor` with the specified number of worker threads.
    ///
    /// This method sets up a channel for sending tasks to the executor, spawns a new thread
    /// that runs a Tokio runtime with the specified number of worker threads, and executes
    /// tasks received from the channel.
    ///
    /// # Panics
    ///
    /// Panics if the executor setup fails or if the runtime cannot be created.
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

    /// Spawns a future onto the `DedicatedExecutor`, returning a `Job` handle.
    ///
    /// This method takes a future `fut` of type `F` that returns a value of type `T`,
    /// wraps it in a `Task`, and sends it to the executor for execution. It returns a
    /// `Job` handle that can be used to await the result of the future or cancel it.
    ///
    /// # Panics
    ///
    /// This method does not panic.
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

impl<E> Executor<E> for DedicatedConcurrentExecutor
where
    E: Message,
{
    async fn execute<P>(
        puppet: &mut P,
        ctx: &mut Context<P>,
        msg: E,
        reply_address: Option<oneshot::Sender<Result<<P as Handler<E>>::Response, PuppetError>>>,
    ) -> Result<(), PuppetError>
    where
        P: Handler<E> + Clone,
    {
        let cloned_puppet = puppet.clone();
        let cloned_pptr = ctx.clone();
        ctx.pptr.executor.spawn(async move {
            let mut local_puppet = cloned_puppet;
            let mut local_pptr = cloned_pptr;
            let _result =
                SequentialExecutor::execute(&mut local_puppet, &mut local_pptr, msg, reply_address)
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
