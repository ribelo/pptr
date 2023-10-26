use std::fmt;

use crate::{
    errors::PostmanError,
    id::Pid,
    message::{Message, Postman, ServiceCommand, ServicePostman},
    puppet::{Handler, Puppet},
    puppet_box::{PuppetBox, PuppetState},
};

#[derive(Debug)]
pub struct PuppetAddress<P>
where
    P: Puppet,
{
    pub id: Pid,
    pub(crate) tx: Postman<P>,
    pub(crate) command_tx: ServicePostman<P>,
}

impl<P: Puppet> Clone for PuppetAddress<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            tx: self.tx.clone(),
            command_tx: self.command_tx.clone(),
        }
    }
}

impl<P: Puppet> PuppetAddress<P> {
    pub async fn send<E>(&self, message: E) -> Result<(), PostmanError<P>>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        self.tx.send(message).await
    }

    pub async fn ask<E>(&self, message: E) -> Result<P::Response, PostmanError<P>>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        self.tx.send_and_await_response(message).await
    }

    pub async fn ask_with_timeout<E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PostmanError<P>>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        tokio::select! {
            result = self.tx.send_and_await_response(message) => result,
            _ = tokio::time::sleep(duration) => Err(PostmanError::ResponseTimeout),
        }
    }
}

impl<P: Puppet> fmt::Display for PuppetAddress<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Address {{ id: {}, name: {} }}",
            self.id.id,
            String::from(self.id)
        )
    }
}

impl<S> From<PuppetBox<S>> for PuppetAddress<PuppetBox<S>>
where
    S: PuppetState,
    PuppetBox<S>: Puppet,
{
    fn from(puppet_box: PuppetBox<S>) -> Self {
        PuppetAddress {
            id: Pid::new::<PuppetBox<S>>(),
            tx: puppet_box.message_tx.clone(),
            command_tx: puppet_box.command_tx.clone(),
        }
    }
}
