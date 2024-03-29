use std::fmt;

use tokio::sync::watch;

use crate::{
    errors::{PostmanError, PuppetError},
    message::{Message, Postman},
    pid::Pid,
    puppet::{Handler, Lifecycle, LifecycleStatus, PuppetBuilder, ResponseFor},
    puppeter::Puppeter,
};

/// Represents an address to which messages can be sent to a puppet.
///
/// An `Address` is returned when spawning a puppet and provides a more efficient way
/// to send messages compared to using `pptr.send()`, as it involves one less layer of indirection.
///
/// # Example Usage
///
/// ```ignore
/// let address = pptr.spawn::<Master, Puppet>(builder).await?;
/// let status = address.get_status();
/// ```
#[derive(Clone)]
pub struct Address<S>
where
    S: Lifecycle,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_tx: Postman<S>,
    pub(crate) pptr: Puppeter,
}

impl<S: Lifecycle> fmt::Debug for Address<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Address")
            .field("pid", &self.pid)
            .field("status", &self.get_status())
            .finish_non_exhaustive()
    }
}

impl<S> Address<S>
where
    S: Lifecycle,
{
    /// Returns the current lifecycle status of the puppet.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let status = address.get_status();
    /// ```
    #[must_use]
    pub fn get_status(&self) -> LifecycleStatus {
        *self.status_rx.borrow()
    }

    /// Subscribes to changes in the puppet's lifecycle status.
    ///
    /// Returns a `watch::Receiver` that can be used to receive status change notifications.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let status_rx = address.status_subscribe();
    /// ```
    #[must_use]
    pub fn subscribe_status(&self) -> watch::Receiver<LifecycleStatus> {
        self.status_rx.clone()
    }

    /// Registers a callback function to be invoked when the puppet's status changes.
    ///
    /// The provided callback function will be executed asynchronously whenever the puppet's
    /// lifecycle status changes.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// address.on_status_change(|status| {
    ///     println!("Puppet status changed: {:?}", status);
    /// });
    /// ```
    pub fn on_status_change<F>(&self, f: F)
    where
        F: Fn(LifecycleStatus) + Send + 'static,
    {
        let mut rx = self.subscribe_status();
        tokio::spawn(async move {
            while (rx.changed().await).is_ok() {
                f(*rx.borrow());
            }
        });
    }

    /// Sends a message of type `E` to the puppet.
    ///
    /// Returns a `Result` indicating the success or failure of the send operation.
    ///
    /// # Errors
    ///
    /// Returns a `PostmanError` if the message fails to send.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let result = address.send(MyMessage::new()).await;
    /// ```
    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx.send::<E>(message).await
    }

    /// Sends a message of type `E` to the puppet and awaits a response.
    ///
    /// Returns a `Result` containing the response or an error.
    ///
    /// # Errors
    ///
    /// Returns a `PostmanError` if the message fails to send or receive a response.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let response = address.ask(MyMessage::new()).await?;
    /// ```
    pub async fn ask<E>(&self, message: E) -> Result<ResponseFor<S, E>, PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx
            .send_and_await_response::<E>(message, None)
            .await
    }

    /// Sends a message of type `E` to the puppet with a timeout and awaits a response.
    ///
    /// Returns a `Result` containing the response or an error.
    ///
    /// # Errors
    ///
    /// Returns a `PostmanError` if the message fails to send or receive a response within the timeout duration.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let response = address.ask_with_timeout(MyMessage::new(), Duration::from_secs(5)).await?;
    /// ```
    pub async fn ask_with_timeout<E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<S, E>, PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx
            .send_and_await_response::<E>(message, Some(duration))
            .await
    }

    /// Spawns a new puppet of type `P` using the provided `PuppetBuilder` and sets
    /// the current puppet as the puppet's master.
    ///
    /// Returns an `Address<P>` for the newly spawned puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet fails to spawn or initialize.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let address = address.spawn::<Puppet>(builder).await?;
    /// ```
    #[allow(clippy::impl_trait_in_params)]
    pub async fn spawn<P>(
        &self,
        builder: impl Into<PuppetBuilder<P>> + Send,
    ) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        self.pptr.spawn::<S, P>(builder).await
    }
}

impl<P> fmt::Display for Address<P>
where
    P: Lifecycle,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.pid)
    }
}
