use std::fmt;

use tokio::sync::watch;

use crate::{
    errors::{PostmanError, PuppetError},
    message::{Message, Postman},
    pid::Pid,
    puppet::{Handler, Puppet, PuppetStatus, ResponseFor},
    puppeteer::Puppeteer,
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
    S: Puppet,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<PuppetStatus>,
    pub(crate) message_tx: Postman<S>,
    pub(crate) pptr: Puppeteer,
}

impl<S: Puppet> fmt::Debug for Address<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Address")
            .field("pid", &self.pid)
            .field("status", &self.get_status())
            .finish_non_exhaustive()
    }
}

impl<S> Address<S>
where
    S: Puppet,
{
    /// Returns the current lifecycle status of the puppet.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let status = address.get_status();
    /// ```
    #[must_use]
    pub fn get_status(&self) -> PuppetStatus {
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
    pub fn subscribe_status(&self) -> watch::Receiver<PuppetStatus> {
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
        F: Fn(PuppetStatus) + Send + 'static,
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
    pub fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx.send::<E>(message)
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
    pub async fn spawn<P>(&self, puppet: P) -> Result<Address<P>, PuppetError>
    where
        P: Puppet,
    {
        self.pptr.spawn::<P, S>(puppet).await
    }
}

impl<P> fmt::Display for Address<P>
where
    P: Puppet,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.pid)
    }
}

#[cfg(test)]
mod tests {
    use crate::{prelude::*, puppet::PuppetStatus};
    use tokio::time::Duration;

    #[derive(Clone, Default)]
    struct TestAddressPuppet;

    impl Puppet for TestAddressPuppet {
        type Supervision = OneToOne;
    }

    #[tokio::test]
    async fn test_get_status() {
        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();
        assert_eq!(address.get_status(), PuppetStatus::Active);
    }

    #[tokio::test]
    async fn test_subscribe_status() {
        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();
        let status_rx = address.subscribe_status();
        assert_eq!(*status_rx.borrow(), PuppetStatus::Active);
    }

    #[tokio::test]
    async fn test_on_status_change() {
        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();

        assert_eq!(address.get_status(), PuppetStatus::Active);

        let (tx, rx) = std::sync::mpsc::channel();
        address.on_status_change(move |status| tx.send(status).unwrap());

        pptr.set_status_by_pid(address.pid, PuppetStatus::Inactive);
        tokio::time::sleep(Duration::from_secs(1)).await;

        assert_eq!(rx.recv().unwrap(), PuppetStatus::Inactive);
    }

    #[tokio::test]
    async fn test_send() {
        #[derive(Debug)]
        struct TestMessage;

        impl Handler<TestMessage> for TestAddressPuppet {
            type Response = ();
            type Executor = SequentialExecutor;

            async fn handle_message(
                &mut self,
                _: TestMessage,
                _: &Context<Self>,
            ) -> Result<(), PuppetError> {
                Ok(())
            }
        }

        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();
        assert!(address.send(TestMessage).is_ok());
    }

    #[tokio::test]
    async fn test_ask() {
        #[derive(Debug)]
        struct TestMessage;

        impl Handler<TestMessage> for TestAddressPuppet {
            type Response = String;
            type Executor = SequentialExecutor;

            async fn handle_message(
                &mut self,
                _: TestMessage,
                _: &Context<Self>,
            ) -> Result<String, PuppetError> {
                Ok("test".to_string())
            }
        }

        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();
        assert_eq!(address.ask(TestMessage).await.unwrap(), "test");
    }

    #[tokio::test]
    async fn test_ask_with_timeout() {
        #[derive(Debug)]
        struct TestMessage;

        impl Handler<TestMessage> for TestAddressPuppet {
            type Response = ();
            type Executor = SequentialExecutor;

            async fn handle_message(
                &mut self,
                _: TestMessage,
                _: &Context<Self>,
            ) -> Result<(), PuppetError> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(())
            }
        }

        let pptr = Puppeteer::new();
        let address = pptr.spawn_self(TestAddressPuppet).await.unwrap();
        assert!(address
            .ask_with_timeout(TestMessage, Duration::from_secs(1))
            .await
            .is_err());
        assert!(address
            .ask_with_timeout(TestMessage, Duration::from_secs(4))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_spawn() {
        #[derive(Clone, Default)]
        struct MasterPuppet;
        #[derive(Clone, Default)]
        struct ChildPuppet;

        impl Puppet for MasterPuppet {
            type Supervision = OneToOne;
        }

        impl Puppet for ChildPuppet {
            type Supervision = OneToOne;
        }

        let pptr = Puppeteer::new();
        let master_address = pptr.spawn_self(MasterPuppet).await.unwrap();
        let child_address = master_address.spawn(ChildPuppet).await.unwrap();
        assert_eq!(child_address.get_status(), PuppetStatus::Active);
    }
}
