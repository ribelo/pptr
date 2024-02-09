use std::fmt;

use tokio::sync::watch;

use crate::{
    errors::{PostmanError, PuppetError},
    master_of_puppets::MasterOfPuppets,
    message::{Message, Postman},
    pid::Pid,
    puppet::{Handler, Lifecycle, LifecycleStatus, PuppetBuilder, ResponseFor},
};

#[derive(Clone)]
pub struct Address<S>
where
    S: Lifecycle,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_tx: Postman<S>,
    pub(crate) master_of_puppets: MasterOfPuppets,
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
    #[must_use]
    pub fn get_status(&self) -> LifecycleStatus {
        *self.status_rx.borrow()
    }

    #[must_use]
    pub fn status_subscribe(&self) -> watch::Receiver<LifecycleStatus> {
        self.status_rx.clone()
    }

    pub fn on_status_change<F>(&self, f: F)
    where
        F: Fn(LifecycleStatus) + Send + 'static,
    {
        let mut rx = self.status_subscribe();
        tokio::spawn(async move {
            while (rx.changed().await).is_ok() {
                f(*rx.borrow());
            }
        });
    }

    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx.send::<E>(message).await
    }

    pub async fn ask<E>(&self, message: E) -> Result<ResponseFor<S, E>, PostmanError>
    where
        S: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx
            .send_and_await_response::<E>(message, None)
            .await
    }

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

    #[allow(clippy::impl_trait_in_params)]
    pub async fn spawn<P>(
        &self,
        builder: impl Into<PuppetBuilder<P>> + Send,
    ) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        self.master_of_puppets.spawn::<S, P>(builder).await
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
