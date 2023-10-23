use crate::{
    address::{CommandAddress, PuppetAddress},
    master::Master,
    message::Message,
    PuppeterError,
};

use super::{
    lifecycle::{LifecycleStatus, PuppetLifecycle},
    service::ServiceCommand,
    Handler, Puppet, PuppetStruct,
};

pub trait PuppetManager {
    fn spawn(self) -> Result<PuppetAddress<Self>, PuppeterError>
    where
        Self: Puppet;

    fn create<P: Puppet>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterError>
    where
        P: PuppetLifecycle;

    fn get_puppet_name<P>(&self) -> Option<String>
    where
        P: Puppet;

    fn is_puppet_exists<P>(&self) -> bool
    where
        P: Puppet;

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

    fn get_command_address<P>(&self) -> Result<CommandAddress, PuppeterError>
    where
        P: Puppet;

    async fn send<P, E>(&self, message: E) -> Result<(), PuppeterError>
    where
        P: Handler<E>,
        E: Message;

    async fn ask<P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
    where
        P: Handler<E>,
        E: Message;

    async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterError>
    where
        P: Handler<E>,
        E: Message;

    async fn send_command<P>(&self, command: ServiceCommand) -> Result<(), PuppeterError>
    where
        P: Puppet;
}
