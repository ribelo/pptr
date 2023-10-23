#![allow(dead_code)]
use std::{
    any::{type_name, TypeId},
    sync::{Arc, OnceLock, RwLock},
};

use hashbrown::{HashMap, HashSet};
use indexmap::IndexSet;
use tokio::sync::mpsc;

use crate::{
    address::{CommandAddress, PuppetAddress},
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    puppet::{
        BoxedAny, Handler, LifecycleStatus, Puppet, PuppetHandler, PuppetLifecycle, PuppetStruct,
    },
    Id, PuppeterError,
};

pub static PUPPETER: OnceLock<Puppeter> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub struct Puppeter {
    pub(crate) status: Arc<RwLock<HashMap<Id, LifecycleStatus>>>,
    pub(crate) puppet_name: Arc<RwLock<HashMap<Id, String>>>,
    pub(crate) puppet_address: Arc<RwLock<HashMap<Id, BoxedAny>>>,
    pub(crate) command_address: Arc<RwLock<HashMap<Id, CommandAddress>>>,
    pub(crate) master: Arc<RwLock<HashMap<Id, IndexSet<Id>>>>,
    pub(crate) slave: Arc<RwLock<HashMap<Id, Id>>>,
    pub(crate) state: Arc<RwLock<HashMap<Id, BoxedAny>>>,
}

pub trait Master: Send + 'static {}

impl Master for Puppeter {}

pub fn puppeter() -> &'static Puppeter {
    PUPPETER.get_or_init(Default::default)
}

impl Puppeter {
    pub(crate) fn get_puppet_name<M>(&self) -> Option<String>
    where
        M: Master,
    {
        let puppeter: Id = TypeId::of::<M>().into();
        let master: Id = TypeId::of::<M>().into();
        if master == puppeter {
            Some("Puppeter".to_string())
        } else {
            let id: Id = TypeId::of::<M>().into();
            self.puppet_name
                .read()
                .expect("Failed to acquire read lock")
                .get(&id)
                .cloned()
        }
    }

    pub(crate) fn is_puppet_exists<M>(&self) -> bool
    where
        M: Master,
    {
        let id: Id = TypeId::of::<M>().into();
        self.puppet_address
            .read()
            .expect("Failed to acquire read lock")
            .contains_key(&id)
            || id == TypeId::of::<Self>().into()
    }

    pub(crate) fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.status
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
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

    pub(crate) fn get_address<P>(&self) -> Option<PuppetAddress<P>>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.puppet_address
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .and_then(|boxed_any| boxed_any.downcast_ref::<PuppetAddress<P>>())
            .cloned()
    }

    pub(crate) fn set_address<P>(&self, address: PuppetAddress<P>)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.puppet_address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, Box::new(address));
    }

    pub(crate) fn get_command_address<A>(&self) -> Option<CommandAddress>
    where
        A: Puppet,
    {
        let id: Id = TypeId::of::<A>().into();
        self.get_command_address_by_id(&id)
    }

    pub(crate) fn get_command_address_by_id(&self, id: &Id) -> Option<CommandAddress> {
        self.command_address
            .read()
            .expect("Failed to acquire read lock")
            .get(id)
            .cloned()
    }

    pub(crate) fn set_command_address<P>(&self, command_address: CommandAddress)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.command_address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, command_address);
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

    pub(crate) fn get_puppets<M>(&self) -> IndexSet<Id>
    where
        M: Master,
    {
        let master: Id = TypeId::of::<M>().into();
        self.master
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn set_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.set_master_by_id(master, slave);
    }

    pub(crate) fn set_master_by_id(&self, master: Id, slave: Id) {
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

    pub(crate) fn remove_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.remove_master_by_id(master, slave);
    }

    pub(crate) fn remove_master_by_id(&self, master: Id, slave: Id) {
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
        self.remove_master::<M, P>();
        self.set_master::<Self, P>();
    }

    pub(crate) fn release_puppet_by_id(&self, master: Id, slave: Id) {
        self.remove_master_by_id(master, slave);
        self.set_master_by_id(TypeId::of::<Self>().into(), slave);
    }

    pub(crate) async fn start_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slaves_option = self.master.read().unwrap().get(&master).cloned();
        let command_addresses = self.command_address.read().unwrap().clone();
        if let Some(slaves) = slaves_option {
            for slave in slaves {
                if let Some(service_address) = command_addresses.get(&slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::InitiateStart)
                        .await
                        .unwrap();
                }
            }
        }
    }

    pub(crate) async fn stop_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();

        let slaves_option = self.master.read().unwrap().get(&master).cloned();

        let command_addresses = self.command_address.read().unwrap().clone();

        if let Some(slaves) = slaves_option {
            for slave in slaves.iter().rev() {
                if let Some(service_address) = command_addresses.get(slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::InitiateStop)
                        .await
                        .unwrap();
                }
            }
        }
    }

    pub(crate) async fn restart_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slaves_option = self.master.read().unwrap().get(&master).cloned();
        let command_addresses = self.command_address.read().unwrap().clone();
        if let Some(slaves) = slaves_option {
            for slave in slaves {
                if let Some(service_address) = command_addresses.get(&slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::RequestRestart)
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
        let slaves_option = self.master.read().unwrap().get(&master).cloned();

        let command_addresses = self.command_address.read().unwrap().clone();

        if let Some(slaves) = slaves_option {
            for slave in slaves {
                if let Some(service_address) = command_addresses.get(&slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::ForceTermination)
                        .await
                        .unwrap();
                }
            }
        }
    }

    pub(crate) fn has_puppet<M, P>(&self) -> bool
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

    pub(crate) fn spawn<M, P>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterError>
    where
        M: Master,
        P: PuppetLifecycle,
    {
        if self.is_puppet_exists::<P>() {
            Err(PuppeterError::MinionAlreadyExists(
                type_name::<P>().to_string(),
            ))
        } else {
            let puppet_struct = puppet.into();
            let (handle, puppet_address, command_address) =
                create_puppeter_entities(&puppet_struct);

            self.set_status::<P>(LifecycleStatus::Activating);
            self.set_address::<P>(puppet_address.clone());
            self.set_command_address::<P>(command_address);
            self.set_master::<M, P>();
            tokio::spawn(run_puppet_loop(puppet_struct.Puppet, handle));
            Ok(puppet_address)
        }
    }

    pub(crate) async fn send<M, P, E>(&self, message: E) -> Result<(), PuppeterError>
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

    pub(crate) async fn ask<M, P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
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

    pub(crate) async fn ask_with_timeout<M, P, E>(
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

    pub(crate) async fn send_command<M, P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppeterError>
    where
        M: Master,
        P: Puppet,
    {
        if let Some(command_address) = self.get_command_address::<P>() {
            command_address
                .command_tx
                .send_and_await_response(command)
                .await
        } else {
            Err(PuppeterError::MinionDoesNotExist(
                type_name::<P>().to_string(),
            ))
        }
    }

    pub(crate) async fn send_command_by_id<M>(
        &self,
        id: Id,
        command: ServiceCommand,
    ) -> Result<(), PuppeterError> {
        if let Some(command_address) = self.get_command_address_by_id(&id) {
            command_address
                .command_tx
                .send_and_await_response(command)
                .await
        } else {
            Err(PuppeterError::MinionDoesNotExist(id.to_string()))
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

pub fn get_puppet_name<P>() -> Option<String>
where
    P: Puppet,
{
    puppeter().get_puppet_name::<P>()
}

pub fn is_puppet_exists<P>() -> bool
where
    P: Puppet,
{
    puppeter().is_puppet_exists::<P>()
}

pub fn get_status<P>() -> Option<LifecycleStatus>
where
    P: Puppet,
{
    puppeter().get_status::<P>()
}

pub fn get_address<P>() -> Option<PuppetAddress<P>>
where
    P: Puppet,
{
    puppeter().get_address::<P>()
}

pub fn get_command_address<P>() -> Option<CommandAddress>
where
    P: Puppet,
{
    puppeter().get_command_address::<P>()
}

pub fn get_master<P>() -> Id
where
    P: Puppet,
{
    puppeter().get_master::<P>()
}

pub fn get_puppets<M>() -> IndexSet<Id>
where
    M: Master,
{
    puppeter().get_puppets::<M>()
}

pub(crate) async fn run_puppet_loop<P>(mut puppet: P, mut handle: PuppetHandler<P>)
where
    P: PuppetLifecycle,
{
    puppet
        .start_master::<Puppeter>()
        .await
        .unwrap_or_else(|err| panic!("{} failed to start. Err: {}", handle, err));

    loop {
        let Some(status) = PUPPETER.get().unwrap().get_status::<P>() else {
            break;
        };
        if status.should_wait_for_activation() {
            continue;
        }

        tokio::select! {
            Some(ServicePacket {cmd, reply_address}) = handle.command_rx.recv() => {
                let response = puppet.handle_command::<Puppeter>(cmd).await;
                reply_address.send(response).unwrap_or_else(|_| println!("{} failed to send response", handle));
            }
            Some(mut envelope) = handle.rx.recv() => {
                if status.should_handle_message() {
                    envelope.handle_message(&mut puppet).await.unwrap_or_else(|err| println!("{} failed to handle command. Err: {}", handle, err));
                }
                else if status.should_drop_message() {
                    envelope.reply_error(PuppeterError::MinionCannotHandleMessage(status)).await.unwrap_or_else(|_| println!("{} failed to send response", handle));
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

#[derive(Debug, Default, Clone)]
pub struct SleepActor {
    i: i32,
}

#[allow(dead_code)]
#[cfg(test)]
mod tests {

    use async_trait::async_trait;
    use minions_derive::{Master, Message, Puppet};

    use crate::prelude::execution;

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone, Message)]
        pub struct SleepMessage {
            i: i32,
        }

        #[derive(Debug, Clone)]
        pub struct SleepMessage2 {
            i: i32,
        }

        impl Message for SleepMessage2 {}

        #[derive(Debug, Default, Clone, Puppet, Master)]
        pub struct SleepActor {
            i: i32,
        }

        #[async_trait]
        impl Handler<SleepMessage> for SleepActor {
            type Response = i32;
            type Exec = execution::Concurrent;
            async fn handle_message(&mut self, msg: SleepMessage) -> i32 {
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

        SleepActor { i: 0 }.spawn();

        puppeter()
            .send::<Puppeter, SleepActor, _>(SleepMessage { i: 10 })
            .await
            .unwrap();
        puppeter()
            .send::<Puppeter, SleepActor, _>(SleepMessage { i: 10 })
            .await
            .unwrap();

        // // provide_state::<i32>(0);
        // // let mut set = tokio::task::JoinSet::new();
        // for _ in 0..10 {
        //     gru.send::<SleepActor, _>(SleepMessage { i: 10 })
        //         .await
        //         .unwrap();
        //     // set.spawn(ask::<SleepActor, _>(SleepMessage { i: 10 }));
        //     // set.spawn(ask::<TestActor>(TestMessage { i: 10 }));
        // }
        tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
        // // while let Some(Ok(res)) = set.join_next().await {
        // //     println!("Response: {:?}", res);
        // // }
    }
}
