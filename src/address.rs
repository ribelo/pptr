use std::fmt;

use crate::{
    errors::PostmanError,
    message::{Message, Postman, ServiceCommand, ServicePostman},
    puppet::{Handler, Puppet},
    Id,
};

#[derive(Debug)]
pub struct PuppetAddress<P>
where
    P: Puppet,
{
    pub id: Id,
    pub(crate) tx: Postman<P>,
}

impl<P: Puppet> Clone for PuppetAddress<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            tx: self.tx.clone(),
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

#[derive(Debug, Clone)]
pub struct CommandAddress {
    pub id: Id,
    pub(crate) command_tx: ServicePostman,
}

impl CommandAddress {
    pub async fn send_command(&self, command: ServiceCommand) -> Result<(), PostmanError<P>> {
        self.command_tx.send_and_await_response(command).await
    }
}
