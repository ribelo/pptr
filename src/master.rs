#![allow(dead_code)]
use std::{
    any::{type_name, Any, TypeId},
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
    puppet::{Handler, LifecycleStatus, Puppet, PuppetHandler, PuppetLifecycle, PuppetStruct},
    Id, PuppeterError,
};

/// Type alias definitions in the Puppeter framework.

/// `BoxedAddress` is a type alias for a boxed dynamic object that implements traits for Any, Send
/// and Sync. This boxed object represents address which could be of any type and safely share
/// references across threads.
pub type BoxedAddres = Box<dyn Any + Send + Sync>;

/// `BoxedSupervisionStrategy`  is a type alias for a boxed dynamic object that implements traits
/// for Any, Send and Sync. This signals that the object within the box  is a supervision strategy
/// which could be of any type and safely share references across threads.
pub type BoxedSupervisionStrategy = Box<dyn Any + Send + Sync>;

/// `BoxedHandlerStrategy` is a type alias for a boxed dynamic object that implements traits for
/// Any, Send and Sync. This denotes that the object is a handler strategy that could be of any
/// type, and could be shared safely across multiple threads.
pub type BoxedHandlerStrategy = Box<dyn Any + Send + Sync>;

/// `BoxedState` is a type alias for a boxed dynamic object that implements traits for Any, Send
/// and Sync. It represents the box carrying the state object which can be of any type and safely
/// share references across threads.
pub type BoxedState = Box<dyn Any + Send + Sync>;

/// `MasterId` is a type alias for ID of specific objects related to the Puppeter framework. They
/// are used as an Identifier for master, puppet, and state respectively. The exact type of `Id`
/// depends on the specific implementation, but generally it is expected to be a unique identifier
/// in the scope of the master or puppet.
pub type MasterId = Id;

/// `PuppetId` is a type alias for ID of specific objects related to the Puppeter framework. They
/// are used as an Identifier for master, puppet, and state respectively. The exact type of `Id`
/// depends on the specific implementation, but generally it is expected to be a unique identifier
/// in the scope of the master or puppet.
pub type PuppetId = Id;

/// `StateId` is a type alias for ID of specific objects related to the Puppeter framework. They
/// are used as an Identifier for master, puppet, and state respectively. The exact type of `Id`
/// depends on the specific implementation, but generally it is expected to be a unique identifier
/// in the scope of the master or puppet.
pub type StateId = Id;

/// Represents the runtime state of the Puppeter framework.
///
/// `Puppeter` struct is a thread-safe structure for managing all the metadata related to the
/// actors (or puppets as referred in the struct) and their masters.
///
/// # Fields
///
/// * `puppet_names`: Mapping between PuppetId and its name.
///
/// * `puppet_statuses`: Dictating the lifecycle status of each puppet.
///
/// * `puppet_addresses`: Providing the Boxed actor address related to each PuppetId.
///
/// * `command_addresses`: Stores the command addresses used by each puppet.
///
/// * `supervision_strategies`: Holds the supervision strategies for each puppet.
///
/// * `handler_strategies`: Contains the handler strategies separately for every puppet.
///
/// * `master_to_puppets`: Maps MasterId to a set of associated PuppetId.
///
/// * `puppet_to_master`: Links each puppet to its master.
///
/// * `state`: Holds the provided states.
#[derive(Clone, Debug, Default)]
pub struct Puppeter {
    pub(crate) puppet_names: Arc<RwLock<HashMap<PuppetId, String>>>,
    pub(crate) puppet_statuses: Arc<RwLock<HashMap<PuppetId, LifecycleStatus>>>,
    pub(crate) puppet_addresses: Arc<RwLock<HashMap<PuppetId, BoxedAddres>>>,
    pub(crate) command_addresses: Arc<RwLock<HashMap<PuppetId, CommandAddress>>>,
    pub(crate) supervision_strategies: Arc<RwLock<HashMap<PuppetId, BoxedSupervisionStrategy>>>,
    pub(crate) handler_strategies: Arc<RwLock<HashMap<PuppetId, BoxedHandlerStrategy>>>,

    pub(crate) master_to_puppets: Arc<RwLock<HashMap<MasterId, IndexSet<PuppetId>>>>,
    pub(crate) puppet_to_master: Arc<RwLock<HashMap<PuppetId, MasterId>>>,

    pub(crate) state: Arc<RwLock<HashMap<StateId, BoxedState>>>,
}

/// Represents the main trait for acting as a manager of puppet/actor instances within the Puppeter
/// framework.
///
/// This trait is used to ensure the working entity, in this case referred to as 'Master', can
/// function within multiple threads, thus, it needs to comply with Send to allow transfer of
/// ownership between threads, ensuring thread-safety which is critical in multi-threaded,
/// data-driven applications. The entity should also have a 'static lifetime, meaning it needs to
/// be available for the entire duration of the run-time of the application, in order to
/// effectively manage the actor model.
///
/// # Example
///
/// ```
/// struct MyMaster;
///
/// impl Master for MyMaster {
///     // Implementation specifics here
/// }
///
/// let my_master = MyMaster; // Now 'my_master' can manage Puppets/actors across threads with 'static lifetime
/// ```
///
/// It's left up to the implementing type how exactly it fulfills the obligations of the `Master`
/// role.
pub trait Master: Send + 'static {}

impl Master for Puppeter {}

// STATUS

impl Puppeter {
    /// Returns the lifecycle status of a specific type of puppet.
    ///
    /// This function uses a type ID to look up the lifecycle status of a puppet within the
    /// framework. If a puppet of the specified type exists, its `LifecycleStatus` is returned. If
    /// no such puppet exists, then `None` is returned.
    ///
    /// Internally, this function calls the `get_status_by_id` method with the type ID of `P`.
    ///
    /// # Type Parameters
    ///
    /// - `P`: The type of the puppet to get the lifecycle status of.
    ///
    /// # Example
    ///
    /// ```
    /// let status = puppet.get_status::<MyPuppet>();
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if it cannot get the lifecycle status of the puppet, which can
    /// occur if the framework fails to acquire a read lock on the puppet status.
    pub fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.get_status_by_id(id)
    }

    /// Returns the lifecycle status of a puppet associated with the specified ID.
    ///
    /// This function obtains a read lock on the puppet_statuses hashmap, searches for the ID, then
    /// returns a clone of the found lifecycle status. If the given id does not exist in the
    /// hashmap, it returns None.
    ///
    /// # Example
    ///
    /// ```
    /// let status = puppet.get_status_by_id(Id);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if it fails to acquire the read lock on the puppet_statuses hashmap.
    pub fn get_status_by_id(&self, id: Id) -> Option<LifecycleStatus> {
        self.puppet_statuses
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
    }

    pub fn set_status<P>(&self, status: LifecycleStatus) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.set_status_by_id(id, status)
    }

    /// Sets the lifecycle status of an actor by its ID.
    ///
    /// This function acquires a write lock on the actor statuses and updates the status of the
    /// given actor. If the actor doesn't exist, an `None` is returned.
    ///
    /// # Example
    ///
    /// ```
    /// let actor_id: Id = ...; // an existing actor ID.
    /// let status = LifecycleStatus::Active; // an instance of `LifecycleStatus` enum.
    /// puppeteer.set_status_by_id(actor_id, status);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if it fails to acquire the write lock on the `puppet_statuses`
    /// map.
    pub fn set_status_by_id(&self, id: Id, status: LifecycleStatus) -> Option<LifecycleStatus> {
        self.puppet_statuses
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, status)
    }
}

/// INFO

impl Puppeter {
    /// Returns the name of the puppet, if it exists.
    ///
    /// This function is used to get the name of a puppet associated with a given master. When the
    /// master is the `Puppeter` itself, it will return `Some("Puppeter")`. Otherwise, it will look
    /// up the puppet map for the given master and return the name if exists.
    ///
    /// # Example
    ///
    /// ```rust
    /// puppeter.get_puppet_name::<Master>();
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if a read lock on `puppet_names` cannot be acquired.
    pub fn get_puppet_name<P>(&self) -> Option<String>
    where
        P: Master,
    {
        let puppeter: Id = TypeId::of::<P>().into();
        let master: Id = TypeId::of::<P>().into();
        if master == Self {
            Some("Puppeter".to_string())
        } else {
            let id: Id = TypeId::of::<P>().into();
            self.puppet_names
                .read()
                .expect("Failed to acquire read lock")
                .get(&id)
                .cloned()
        }
    }

    /// Checks whether a puppet actor of the given Master type is already present in the framework.
    ///
    /// This function checks the internal registry of puppet actors for
    /// an entry associated with the provided Master type identifier.
    ///
    /// # Example
    ///
    /// ```
    /// let exists = framework.is_puppet_exists::<MyPuppet>();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the system fails to acquire a read lock on the internal puppet addresses
    /// registry.
    pub fn is_puppet_exists<M>(&self) -> bool
    where
        M: Master,
    {
        let id: Id = TypeId::of::<M>().into();
        self.puppet_addresses
            .read()
            .expect("Failed to acquire read lock")
            .contains_key(&id)
            || id == TypeId::of::<Self>().into()
    }
}

/// ADDRESS

impl Puppeter {
    pub fn get_address<P>(&self) -> Option<PuppetAddress<P>>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.puppet_addresses
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .and_then(|boxed_any| boxed_any.downcast_ref::<PuppetAddress<P>>())
            .cloned()
    }

    pub fn set_address<P>(&self, address: PuppetAddress<P>)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.puppet_addresses
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, Box::new(address));
    }

    pub fn get_command_address<P>(&self) -> Option<CommandAddress>
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.get_command_address_by_id(&id)
    }

    pub fn get_command_address_by_id(&self, id: &Id) -> Option<CommandAddress> {
        self.command_addresses
            .read()
            .expect("Failed to acquire read lock")
            .get(id)
            .cloned()
    }

    pub fn set_command_address<P>(&self, command_address: CommandAddress)
    where
        P: Puppet,
    {
        let id: Id = TypeId::of::<P>().into();
        self.command_addresses
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, command_address);
    }
}

/// RELATIONS

impl Puppeter {
    pub fn get_master<P>(&self) -> Id
    where
        P: Puppet,
    {
        let puppet: Id = TypeId::of::<P>().into();
        self.get_master_by_id(puppet)
    }

    pub fn get_master_by_id(&self, id: Id) -> Id {
        self.puppet_to_master
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
            .unwrap()
    }

    pub fn has_puppet<M, P>(&self) -> bool
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let puppet: Id = TypeId::of::<P>().into();
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .map(|slaves| slaves.contains(&puppet))
            .unwrap_or(false)
    }

    pub fn has_puppet_by_id(&self, master: Id, slave: Id) -> bool {
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .map(|slaves| slaves.contains(&slave))
            .unwrap_or(false)
    }

    pub fn get_puppets<M>(&self) -> IndexSet<Id>
    where
        M: Master,
    {
        let master: Id = TypeId::of::<M>().into();
        self.get_puppets_by_id(master)
    }

    pub fn get_puppets_by_id(&self, id: Id) -> IndexSet<Id> {
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.set_master_by_id(master, slave);
    }

    pub fn set_master_by_id(&self, master: Id, slave: Id) {
        self.master_to_puppets
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .insert(slave);
        self.puppet_to_master
            .write()
            .expect("Failed to acquire write lock")
            .insert(slave, master);
    }

    pub fn remove_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slave: Id = TypeId::of::<P>().into();
        self.remove_master_by_id(master, slave);
    }

    pub fn remove_master_by_id(&self, master: Id, slave: Id) {
        self.master_to_puppets
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .remove(&slave);
        self.puppet_to_master
            .write()
            .expect("Failed to acquire write lock")
            .remove(&slave);
    }

    pub fn release_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        self.remove_master::<M, P>();
        self.set_master::<Self, P>();
    }

    pub fn release_puppet_by_id(&self, master: Id, slave: Id) {
        self.remove_master_by_id(master, slave);
        self.set_master_by_id(TypeId::of::<Self>().into(), slave);
    }

    pub fn release_all_puppets<M>(&self)
    where
        M: Master,
    {
        let master: Id = TypeId::of::<M>().into();
        self.release_all_puppets_by_id(master);
    }

    pub fn releasle_all_puppets_by_id(&self, master: Id) {
        let puppets = self.get_puppets_by_id(master);
        for puppet in puppets {
            self.release_puppet_by_id(master, puppet);
        }
    }
}

/// LIFECYCLE

impl Puppeter {
    pub async fn start_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slaves_option = self.master_to_puppets.read().unwrap().get(&master).cloned();
        let command_addresses = self.command_addresses.read().unwrap().clone();
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

    pub async fn stop_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();

        let slaves_option = self.master_to_puppets.read().unwrap().get(&master).cloned();

        let command_addresses = self.command_addresses.read().unwrap().clone();

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

    pub async fn restart_puppets<M>(&self)
    where
        M: Puppet,
    {
        let master: Id = TypeId::of::<M>().into();
        let slaves_option = self.master_to_puppets.read().unwrap().get(&master).cloned();
        let command_addresses = self.command_addresses.read().unwrap().clone();
        if let Some(slaves) = slaves_option {
            for slave in slaves {
                if let Some(service_address) = command_addresses.get(&slave) {
                    service_address
                        .command_tx
                        .send_and_await_response(ServiceCommand::RequestSelfRestart)
                        .await
                        .unwrap();
                }
            }
        }
    }

    pub async fn kill_puppets<M>(&self)
    where
        M: Master,
    {
        let master: Id = TypeId::of::<M>().into();
        let slaves_option = self.master_to_puppets.read().unwrap().get(&master).cloned();

        let command_addresses = self.command_addresses.read().unwrap().clone();

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
}

impl Puppeter {
    pub fn spawn<M, P>(
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
            tokio::spawn(run_puppet_loop(self.clone(), puppet_struct.Puppet, handle));
            Ok(puppet_address)
        }
    }

    pub async fn send<M, P, E>(&self, message: E) -> Result<(), PuppeterError>
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

    pub async fn ask<M, P, E>(&self, message: E) -> Result<P::Response, PuppeterError>
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

    pub async fn ask_with_timeout<M, P, E>(
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

    pub async fn send_command<M, P>(&self, command: ServiceCommand) -> Result<(), PuppeterError>
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

    pub async fn send_command_by_id<M>(
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

pub(crate) async fn run_puppet_loop<P>(
    puppeter: Puppeter,
    mut puppet: P,
    mut handle: PuppetHandler<P>,
) where
    P: PuppetLifecycle,
{
    puppet
        .start_master::<Puppeter>()
        .await
        .unwrap_or_else(|err| panic!("{} failed to start. Err: {}", handle, err));

    loop {
        let Some(status) = puppeter.get_status::<P>() else {
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

    // #[tokio::test]
    // async fn it_works() {
    //     #[derive(Debug, Clone, Message)]
    //     pub struct SleepMessage {
    //         i: i32,
    //     }
    //
    //     #[derive(Debug, Clone)]
    //     pub struct SleepMessage2 {
    //         i: i32,
    //     }
    //
    //     impl Message for SleepMessage2 {}
    //
    //     #[derive(Debug, Default, Clone, Puppet, Master)]
    //     pub struct SleepActor {
    //         i: i32,
    //     }
    //
    //     #[async_trait]
    //     impl Handler<SleepMessage> for SleepActor {
    //         type Response = i32;
    //         type Exec = execution::Concurrent;
    //         async fn handle_message(&mut self, msg: SleepMessage) -> i32 {
    //             println!("SleepActor Received message: {:?}", msg);
    //             // with_state(|i: Option<&i32>| {
    //             //     if i.is_some() {
    //             //         println!("SleepActor Context: {:?}", i);
    //             //     }
    //             // });
    //             // with_state_mut(|i: Option<&mut i32>| {
    //             //     *i.unwrap() += 1;
    //             // });
    //             tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
    //             msg.i
    //         }
    //     }
    //
    //     SleepActor { i: 0 }.spawn();
    //
    //     puppeter()
    //         .send::<Puppeter, SleepActor, _>(SleepMessage { i: 10 })
    //         .await
    //         .unwrap();
    //     puppeter()
    //         .send::<Puppeter, SleepActor, _>(SleepMessage { i: 10 })
    //         .await
    //         .unwrap();
    //
    //     // // provide_state::<i32>(0);
    //     // // let mut set = tokio::task::JoinSet::new();
    //     // for _ in 0..10 {
    //     //     gru.send::<SleepActor, _>(SleepMessage { i: 10 })
    //     //         .await
    //     //         .unwrap();
    //     //     // set.spawn(ask::<SleepActor, _>(SleepMessage { i: 10 }));
    //     //     // set.spawn(ask::<TestActor>(TestMessage { i: 10 }));
    //     // }
    //     tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
    //     // // while let Some(Ok(res)) = set.join_next().await {
    //     // //     println!("Response: {:?}", res);
    //     // // }
    // }
}
