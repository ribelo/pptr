use std::fmt;

use tokio::sync::watch;

use crate::{
    errors::{PostmanError, PuppetError},
    message::{Message, Postman, ServiceCommand, ServicePostman},
    pid::Pid,
    post_office::PostOffice,
    puppet::{
        Handler, Lifecycle, LifecycleStatus, Puppet, PuppetBuilder, PuppetState, ResponseFor,
    },
};

#[derive(Debug, Clone)]
pub struct Address<S>
where
    S: PuppetState,
    Puppet<S>: Lifecycle,
{
    pub pid: Pid,
    pub(crate) status_rx: watch::Receiver<LifecycleStatus>,
    pub(crate) message_tx: Postman<S>,
    pub(crate) post_office: PostOffice,
}

impl<S> Address<S>
where
    S: PuppetState,
    Puppet<S>: Lifecycle,
{
    pub fn get_status(&self) -> LifecycleStatus {
        *self.status_rx.borrow()
    }

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
        Puppet<S>: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx.send::<E>(message).await
    }

    pub async fn ask<E>(&self, message: E) -> Result<ResponseFor<S, E>, PostmanError>
    where
        Puppet<S>: Handler<E>,
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
        Puppet<S>: Handler<E>,
        E: Message + 'static,
    {
        self.message_tx
            .send_and_await_response::<E>(message, Some(duration))
            .await
    }

    pub async fn spawn<P>(
        &self,
        builder: impl Into<PuppetBuilder<P>>,
    ) -> Result<Address<P>, PuppetError>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        self.post_office.spawn::<S, P>(builder).await
    }
}

impl<P> fmt::Display for Address<P>
where
    P: PuppetState,
    Puppet<P>: Lifecycle,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.pid)
    }
}
