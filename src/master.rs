use async_trait::async_trait;

use crate::{
    address::{CommandAddress, PuppetAddress},
    prelude::{Message, Puppet, ServiceCommand},
    puppet::{Handler, LifecycleStatus, PuppetStruct},
    PuppeterError,
};

pub trait PuppetInfo {
    fn is_puppet_exists<P: Puppet>(&self) -> bool;
    fn has_puppet<M, P>(&self) -> bool
    where
        M: Master,
        P: Puppet;
    fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet;
    fn get_address<P>(&self) -> Option<PuppetAddress<P>>
    where
        P: Puppet;
    fn get_command_address<P>(&self) -> Option<CommandAddress>
    where
        P: Puppet;
}

pub trait PuppetLifecycle {
    async fn start<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet;
    async fn stop<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet;
    async fn kill<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet;
    async fn restart<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet;
}

#[async_trait]
pub trait Master: 'static {
    fn spawn<M, P>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterError>
    where
        M: Master,
        P: Puppet;
}

pub trait PuppetCommunicator {
    async fn send<M, P, E>(&self, message: E) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static;
    async fn ask<M, P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static;
    async fn ask_with_timeout<M, P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static;
}

pub trait PuppetCommander {
    async fn send_command<M, P>(&self, command: ServiceCommand) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet;
}
