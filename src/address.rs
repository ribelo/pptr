use std::fmt;

use crate::{
    errors::MessageError,
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
    pub name: String,
    pub(crate) tx: Postman<P>,
}

impl<P: Puppet> Clone for PuppetAddress<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            tx: self.tx.clone(),
        }
    }
}

impl<P: Puppet> PuppetAddress<P> {
    async fn send<E>(&self, message: E) -> Result<(), MessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        self.tx.send(message).await
    }

    async fn ask<E>(&self, message: E) -> Result<P::Response, MessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        self.tx.send_and_await_response(message).await
    }

    async fn ask_with_timeout<E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, MessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        tokio::select! {
            result = self.tx.send_and_await_response(message) => {
                match result {
                    Ok(response) => Ok(response),
                    Err(_) => Err(MessageError::SendError)
                }
            },
            _ = tokio::time::sleep(duration) => Err(MessageError::ResponseTimeout),
        }
    }
}

impl<P: Puppet> fmt::Display for PuppetAddress<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address {{ id: {}, name: {} }}", self.id, self.name)
    }
}

#[derive(Debug, Clone)]
pub struct CommandAddress {
    pub id: Id,
    pub name: String,
    pub(crate) command_tx: ServicePostman,
}

impl CommandAddress {
    pub async fn send_command(&self, command: ServiceCommand) -> Result<(), MessageError> {
        self.command_tx.send_and_await_response(command).await
    }
}
