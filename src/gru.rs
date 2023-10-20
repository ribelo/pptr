#![allow(dead_code)]
use std::{
    any::{type_name, TypeId},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use hashbrown::{HashMap, HashSet};
use tokio::sync::mpsc;

use crate::{
    address::{CommandAddress, PuppetAddress},
    master::{Master, PuppetCommander, PuppetCommunicator, PuppetInfo, PuppetLifecycle},
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    puppet::{BoxedAny, Handler, LifecycleStatus, Puppet, PuppetHandler, PuppetStruct},
    Id, PuppeterError,
};

#[derive(Clone, Debug, Default)]
pub struct Puppeter {
    pub(crate) status: Arc<RwLock<HashMap<Id, LifecycleStatus>>>,
    pub(crate) puppet_address: Arc<RwLock<HashMap<Id, BoxedAny>>>,
    pub(crate) command_address: Arc<RwLock<HashMap<Id, CommandAddress>>>,
    pub(crate) master: Arc<RwLock<HashMap<Id, HashSet<Id>>>>,
    pub(crate) slave: Arc<RwLock<HashMap<Id, Id>>>,
    pub(crate) state: Arc<RwLock<HashMap<Id, BoxedAny>>>,
}

impl Puppeter {
    pub fn new() -> Self {
        Default::default()
    }

    pub(crate) fn set_status<P>(&self, status: LifecycleStatus) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.status
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, status)
    }

    pub(crate) fn set_address<P>(&self, address: PuppetAddress<P>)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.puppet_address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, BoxedAny::new(address));
    }

    pub(crate) fn set_command_address<P>(&self, address: CommandAddress)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.command_address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, address);
    }

    pub(crate) fn set_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();

        self.master
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .insert(slave);

        self.slave
            .write()
            .expect("Failed to acquire write lock")
            .insert(slave, master);
    }

    pub(crate) fn remove_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.master
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .remove(&slave);
        self.slave
            .write()
            .expect("Failed to acquire write lock")
            .remove(&slave);
    }

    pub(crate) fn release_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        self.remove_puppet::<M, P>();
        self.set_puppet::<Self, P>();
    }

    pub(crate) fn get_master<P>(&self) -> Id
    where
        P: Puppet,
    {
        let slave: Id = TypeId::of::<P>().into();
        self.slave
            .read()
            .expect("Failed to acquire read lock")
            .get(&slave)
            .cloned()
            .unwrap()
    }

    pub(crate) async fn stop_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        if let Some(slaves) = self.master.read().unwrap().get(&master) {
            for slave in slaves {
                if let Some(service_address) = self.command_address.read().unwrap().get(slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::Stop)
                        .await
                        .unwrap();
                }
            }
        }
    }

    pub(crate) async fn kill_puppets<M>(&self)
    where
        M: Master,
    {
        let master: Id = TypeId::of::<M>().into();
        if let Some(slaves) = self.master.read().unwrap().get(&master) {
            for slave in slaves {
                if let Some(service_address) = self.command_address.read().unwrap().get(slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::Terminate)
                        .await
                        .unwrap();
                }
            }
        }
    }

    // pub fn with_state<T>(self, state: T) -> Self
    // where
    //     T: Any + Clone + Send + Sync,
    // {
    //     let id: Id = TypeId::of::<T>().into();
    //     self.state
    //         .write()
    //         .expect("Failed to acquire write lock")
    //         .insert(id, BoxedAny::new(state));
    //     self
    // }
    //
    // pub fn with_minion<A>(self, minion: impl Into<PuppetStruct<A>>) -> Result<Self, PuppeterError>
    // where
    //     A: Puppet,
    // {
    //     self.spawn(minion)?;
    //     Ok(self)
    // }
}

impl PuppetInfo for Puppeter {
    fn is_puppet_exists<A>(&self) -> bool
    where
        A: Puppet,
    {
        let id: Id = TypeId::of::<A>().into();
        self.puppet_address
            .read()
            .expect("Failed to acquire read lock")
            .contains_key(&id)
    }

    fn has_puppet<M, P>(&self) -> bool
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.master
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .map(|slaves| slaves.contains(&slave))
            .unwrap_or(false)
    }

    fn get_status<A>(&self) -> Option<LifecycleStatus>
    where
        A: Puppet,
    {
        let id: Id = TypeId::of::<A>().into();
        self.status
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
    }

    fn get_address<A>(&self) -> Option<PuppetAddress<A>>
    where
        A: Puppet,
    {
        let id: Id = TypeId::of::<A>().into();
        self.puppet_address
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .map(|boxed_any| {
                boxed_any
                    .downcast_ref_unchecked::<PuppetAddress<A>>()
                    .clone()
            })
    }

    fn get_command_address<A>(&self) -> Option<CommandAddress>
    where
        A: Puppet,
    {
        let id: Id = TypeId::of::<A>().into();
        self.command_address
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .map(|boxed_any| boxed_any.clone())
    }
}

#[async_trait]
impl Master for Puppeter {
    fn spawn<M, P>(
        &self,
        minion: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        if self.is_puppet_exists::<P>() {
            Err(PuppeterError::MinionAlreadyExists(
                type_name::<P>().to_string(),
            ))
        } else {
            let minion_struct = minion.into();
            let (handle, puppet_address, command_address) =
                create_puppeter_entities(&minion_struct);

            self.set_status::<P>(LifecycleStatus::Activating);
            self.set_address::<P>(puppet_address.clone());
            self.set_puppet::<M, P>();
            tokio::spawn(run_minion_loop(self.clone(), minion_struct.Puppet, handle));
            Ok(puppet_address)
        }
    }
}

impl PuppetLifecycle for Puppeter {
    async fn start<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        self.send_command::<M, P>(ServiceCommand::Start).await
    }

    async fn stop<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        self.send_command::<M, P>(ServiceCommand::Stop).await
    }

    async fn kill<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        if let Some(address) = self.get_command_address::<P>() {
            self.send_command::<M, P>(ServiceCommand::Terminate).await?;
            self.puppet_address
                .write()
                .expect("Failed to acquire write lock")
                .remove(&address.id);
            self.status
                .write()
                .expect("Failed to acquire write lock")
                .remove(&address.id);
            Ok(())
        } else {
            Err(PuppeterError::MinionDoesNotExist(
                type_name::<P>().to_string(),
            ))
        }
    }

    async fn restart<M, P>(&self) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        self.send_command::<M, P>(ServiceCommand::Restart).await
    }
}

impl PuppetCommunicator for Puppeter {
    async fn send<M, P, E>(&self, message: E) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static,
    {
        if let Some(address) = self.get_address::<P>() {
            let status = self.get_status::<P>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => address.tx.send(message).await,
                _ => Err(PuppeterError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(PuppeterError::MinionDoesNotExist(
                type_name::<P>().to_string(),
            ))
        }
    }

    async fn ask<M, P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static,
    {
        if let Some(address) = self.get_address::<P>() {
            let status = self.get_status::<P>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => address.tx.send_and_await_response(message).await,
                _ => Err(PuppeterError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(PuppeterError::MinionDoesNotExist(
                type_name::<P>().to_string(),
            ))
        }
    }

    async fn ask_with_timeout<M, P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterError>
    where
        M: Master,
        P: Handler<E>,
        E: Message + 'static,
    {
        let ask_future = self.ask::<M, P, E>(message);
        let timeout_future = tokio::time::sleep(duration);
        tokio::select! {
            response = ask_future => response,
            _ = timeout_future => Err(PuppeterError::MessageResponseTimeout),
        }
    }
}

impl PuppetCommander for Puppeter {
    async fn send_command<M, P>(&self, command: ServiceCommand) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        if let Some(address) = self.get_command_address::<P>() {
            address.command_tx.send_and_await_response(command).await
        } else {
            Err(PuppeterError::MinionDoesNotExist(
                type_name::<P>().to_string(),
            ))
        }
    }
}

pub(crate) async fn run_minion_loop<A>(gru: Puppeter, mut minion: A, mut handle: PuppetHandler<A>)
where
    A: Puppet,
{
    minion
        .start()
        .await
        .unwrap_or_else(|err| panic!("{} failed to start. Err: {}", handle, err));

    loop {
        let Some(status) = gru.get_status::<A>() else {
            break;
        };
        if status.should_wait_for_activation() {
            continue;
        }

        tokio::select! {
            Some(ServicePacket {cmd, reply_address}) = handle.command_rx.recv() => {
                let response = minion.handle_command(cmd).await;
                reply_address.send(response).unwrap_or_else(|_| println!("{} failed to send response", handle));
            }
            Some(mut envelope) = handle.rx.recv() => {
                if status.should_handle_message() {
                    envelope.handle_message(&mut minion, &gru).await.unwrap_or_else(|err| println!("{} failed to handle command. Err: {}", handle, err));
                }
                else if status.should_drop_message() {
                    envelope.reply_error(PuppeterError::MinionCannotHandleMessage(status), &gru).await.unwrap_or_else(|_| println!("{} failed to send response", handle));
                }
            }
            else => {
                break;
            }
        }
    }
}

fn create_puppeter_entities<P>(
    minion: &PuppetStruct<P>,
) -> (PuppetHandler<P>, PuppetAddress<P>, CommandAddress)
where
    P: Puppet,
{
    let (tx, rx) = mpsc::channel::<Box<dyn Envelope<P>>>(minion.buffer_size);
    let (command_tx, command_rx) = mpsc::channel::<ServicePacket>(minion.commands_buffer_size);

    let handler = PuppetHandler {
        id: TypeId::of::<P>().into(),
        name: type_name::<P>().to_string(),
        rx: Mailbox::new(rx),
        command_rx: ServiceMailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = ServicePostman::new(command_tx);
    let puppet_address = PuppetAddress {
        id: TypeId::of::<P>().into(),
        name: type_name::<P>().to_string(),
        tx,
    };
    let command_address = CommandAddress {
        id: TypeId::of::<P>().into(),
        name: type_name::<P>().to_string(),
        command_tx,
    };
    (handler, puppet_address, command_address)
}

#[allow(dead_code)]
#[cfg(test)]
mod tests {

    use async_trait::async_trait;
    // use minions_derive::Message;

    // use crate::context::provide_context;

    use crate::prelude::execution;

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone)]
        pub struct TestMessage {
            i: i32,
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[derive(Debug, Clone)]
        pub struct SleepMessage {
            i: i32,
        }

        impl Message for SleepMessage {}

        #[derive(Debug, Clone)]
        pub struct SleepMessage2 {
            i: i32,
        }

        impl Message for SleepMessage2 {}

        #[derive(Debug, Default, Clone)]
        pub struct SleepActor {
            i: i32,
        }

        impl Master for SleepActor {}
        impl Puppet for SleepActor {}

        #[async_trait]
        impl Handler<SleepMessage> for SleepActor {
            type Response = i32;
            type Exec = execution::Concurrent;
            async fn handle_message(&mut self, msg: SleepMessage, gru: &Puppeter) -> i32 {
                println!("SleepActor Received message: {:?}", msg);
                // with_state(|i: Option<&i32>| {
                //     if i.is_some() {
                //         println!("SleepActor Context: {:?}", i);
                //     }
                // });
                // with_state_mut(|i: Option<&mut i32>| {
                //     *i.unwrap() += 1;
                // });
                tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
                msg.i
            }
        }

        let gru = Puppeter::new()
            .with_state(0)
            .with_minion(SleepActor { i: 0 })
            .unwrap();
        // provide_state::<i32>(0);
        // let mut set = tokio::task::JoinSet::new();
        for _ in 0..10 {
            gru.send::<SleepActor, _>(SleepMessage { i: 10 })
                .await
                .unwrap();
            // set.spawn(ask::<SleepActor, _>(SleepMessage { i: 10 }));
            // set.spawn(ask::<TestActor>(TestMessage { i: 10 }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
        // while let Some(Ok(res)) = set.join_next().await {
        //     println!("Response: {:?}", res);
        // }
    }
}
