#![allow(dead_code)]
use std::{
    any::{type_name, TypeId},
    collections::VecDeque,
    sync::{Arc, OnceLock, RwLock},
};

use hashbrown::HashMap;
use indexmap::IndexSet;
use tokio::sync::mpsc;

use crate::{
    address::{CommandAddress, PuppetAddress},
    errors::{
        KillPuppetError, PermissionDenied, PuppetAlreadyExist, PuppetCannotHandleMessage,
        PuppetDoesNotExist, PuppetError, PuppeterSendCommandError, PuppeterSendMessageError,
        PuppeterSpawnError, ResetPuppetError, StartPuppetError, StopPuppetError,
    },
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    puppet::{BoxedAny, Handler, LifecycleStatus, Puppet, PuppetHandler, PuppetStruct},
    Id,
};

pub static PUPPETER: OnceLock<Puppeter> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub struct Puppeter {
    pub(crate) puppet_statuses: Arc<RwLock<HashMap<Id, LifecycleStatus>>>,
    pub(crate) puppet_addresses: Arc<RwLock<HashMap<Id, BoxedAny>>>,
    pub(crate) command_addresses: Arc<RwLock<HashMap<Id, CommandAddress>>>,
    pub(crate) master_to_puppets: Arc<RwLock<HashMap<Id, IndexSet<Id>>>>,
    pub(crate) puppet_to_master: Arc<RwLock<HashMap<Id, Id>>>,
    pub(crate) state: Arc<RwLock<HashMap<Id, BoxedAny>>>,
}

pub trait Master: Send + 'static {}

impl Master for Puppeter {}

pub fn puppeter() -> &'static Puppeter {
    PUPPETER.get_or_init(Default::default)
}

impl Puppeter {
    pub fn new() -> Self {
        PUPPETER.get_or_init(Default::default).clone()
    }
}

// STATUS

impl Puppeter {
    /// Returns the lifecycle status of a specific type of puppet.
    ///
    /// This function uses a type ID to look up the lifecycle status of a puppet
    /// within the framework. If a puppet of the specified type exists, its
    /// `LifecycleStatus` is returned. If no such puppet exists, then `None`
    /// is returned.
    ///
    /// Internally, this function calls the `get_status_by_id` method with the
    /// type ID of `P`.
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
    /// This function will panic if it cannot get the lifecycle status of the
    /// puppet, which can occur if the framework fails to acquire a read
    /// lock on the puppet status.
    pub(crate) fn get_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id = Id::new::<P>();
        self.get_status_by_id(id)
    }

    /// Returns the lifecycle status of a puppet associated with the specified
    /// ID.
    ///
    /// This function obtains a read lock on the puppet_statuses hashmap,
    /// searches for the ID, then returns a clone of the found lifecycle
    /// status. If the given id does not exist in the hashmap, it returns
    /// None.
    ///
    /// # Example
    ///
    /// ```
    /// let status = puppet.get_status_by_id(Id);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if it fails to acquire the read lock on the
    /// puppet_statuses hashmap.
    pub fn get_status_by_id(&self, id: Id) -> Option<LifecycleStatus> {
        self.puppet_statuses
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
    }

    pub(crate) fn set_status<P>(&self, status: LifecycleStatus) -> Option<LifecycleStatus>
    where
        P: Puppet,
    {
        let id = Id::new::<P>();
        self.set_status_by_id(id, status)
    }

    /// Sets the lifecycle status of an actor by its ID.
    ///
    /// This function acquires a write lock on the actor statuses and updates
    /// the status of the given actor. If the actor doesn't exist, an `None`
    /// is returned.
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
    /// This function will panic if it fails to acquire the write lock on the
    /// `puppet_statuses` map.
    pub(crate) fn set_status_by_id(
        &self,
        id: Id,
        status: LifecycleStatus,
    ) -> Option<LifecycleStatus> {
        self.puppet_statuses
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, status)
    }
}

/// INFO

impl Puppeter {
    /// Checks whether a puppet actor of the given Master type is already
    /// present in the framework.
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
    /// Panics if the system fails to acquire a read lock on the internal puppet
    /// addresses registry.
    pub fn is_puppet_exists<M>(&self) -> bool
    where
        M: Master,
    {
        let id = Id::new::<M>();
        self.is_puppet_exists_by_id(id) || id == Id::new::<Self>()
    }

    pub fn is_puppet_exists_by_id(&self, id: Id) -> bool {
        self.puppet_addresses
            .read()
            .expect("Failed to acquire read lock")
            .contains_key(&id)
    }
}

/// ADDRESS

impl Puppeter {
    pub fn get_address<P>(&self) -> Option<PuppetAddress<P>>
    where
        P: Puppet,
    {
        let id = Id::new::<P>();
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
        let id = Id::new::<P>();
        self.puppet_addresses
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, Box::new(address));
    }

    pub fn get_command_address<M, P>(
        &self,
    ) -> Result<Option<CommandAddress>, PermissionDenied<M, P>>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.get_command_address_by_id(master, puppet)
    }

    pub fn get_command_address_by_id(
        &self,
        master: Id,
        puppet: Id,
    ) -> Result<Option<CommandAddress>, PermissionDenied<M, P>> {
        match self.has_permission_by_id(master, puppet) {
            Some(true) => {
                Ok(self
                    .command_addresses
                    .read()
                    .expect("Failed to acquire read lock")
                    .get(&puppet)
                    .cloned())
            }
            Some(false) => {
                Err(PermissionDenied {
                    master: master.into(),
                    puppet: puppet.into(),
                    message: "Can't get puppet command address from another master".to_string(),
                })
            }
            None => Ok(None),
        }
    }

    pub fn set_command_address<P>(&self, command_address: CommandAddress)
    where
        P: Puppet,
    {
        let id = Id::new::<P>();
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
        let id = Id::new::<P>();
        self.get_master_by_id(id)
    }

    pub fn get_master_by_id(&self, id: Id) -> Id {
        self.puppet_to_master
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
            .unwrap()
    }

    pub fn set_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.set_master_by_id(master, puppet);
    }

    pub fn set_master_by_id(&self, master: Id, puppet: Id) {
        self.master_to_puppets
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .insert(puppet);
        self.puppet_to_master
            .write()
            .expect("Failed to acquire write lock")
            .insert(puppet, master);
    }

    pub fn remove_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.remove_puppet_by_id(master, puppet);
    }

    pub fn remove_puppet_by_id(&self, master: Id, puppet: Id) {
        self.puppet_statuses
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
        self.puppet_addresses
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
        self.command_addresses
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
        self.master_to_puppets
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .remove(&puppet);
        self.puppet_to_master
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
    }

    pub fn has_puppet<M, P>(&self) -> Option<bool>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.has_puppet_by_id(master, puppet)
    }

    pub fn has_puppet_by_id(&self, master: Id, puppet: Id) -> Option<bool> {
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .map(|puppets| puppets.contains(&puppet))
    }

    pub fn get_puppets<M>(&self) -> IndexSet<Id>
    where
        M: Master,
    {
        let id = Id::new::<M>();
        self.get_puppets_by_id(id)
    }

    pub fn get_puppets_by_id(&self, id: Id) -> IndexSet<Id> {
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn release_puppet<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        self.remove_puppet::<M, P>();
        self.set_master::<Self, P>();
    }

    pub fn release_puppet_by_id(&self, master: Id, puppet: Id) {
        self.remove_puppet_by_id(master, puppet);
        self.set_master_by_id(Id::new::<Self>(), puppet);
    }

    pub fn release_all_puppets<M>(&self)
    where
        M: Master,
    {
        let master = Id::new::<M>();
        self.release_all_puppets_by_id(master);
    }

    pub fn release_all_puppets_by_id(&self, master: Id) {
        let puppets = self.get_puppets_by_id(master);
        for puppet in puppets {
            self.release_puppet_by_id(master, puppet);
        }
    }

    pub fn has_permission<M, P>(&self) -> Option<bool>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.has_permission_by_id(master, puppet)
    }

    pub fn has_permission_by_id(&self, master: Id, puppet: Id) -> Option<bool> {
        if let Some(has) = self.has_puppet_by_id(master, puppet) {
            let puppeter_id = Id::new::<Self>();
            Some(has || master == puppeter_id)
        } else {
            None
        }
    }
}

/// LIFECYCLE
/// START

impl Puppeter {
    pub async fn start_puppet<M, P>(&self) -> Result<(), StartPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.start_puppet_by_id(master, puppet).await
    }

    pub async fn start_puppet_by_id(&self, master: Id, puppet: Id) -> Result<(), StartPuppetError> {
        match self.get_command_address_by_id(master, puppet) {
            Err(error) => Err(error.with_message("Can't start puppet from another master"))?,
            Ok(None) => {
                Err(PuppetDoesNotExist {
                    name: puppet.into(),
                }
                .into())
            }
            Ok(Some(command_address)) => {
                let status = self.get_status_by_id(puppet).unwrap();
                match status {
                    LifecycleStatus::Active
                    | LifecycleStatus::Activating
                    | LifecycleStatus::Restarting => Ok(()),
                    _ => Ok(command_address.send_command(ServiceCommand::Stop).await?),
                }
            }
        }
    }

    pub async fn start_puppets<M, P>(&self) -> Result<(), StartPuppetError>
    where
        M: Master,
        P: Master,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.start_puppets_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn start_puppets_by_id(
        &self,
        master: Id,
        puppet: Id,
    ) -> Result<(), StartPuppetError> {
        let puppets = self.get_puppets_by_id(puppet);
        for id in puppets {
            self.start_puppet_by_id(master, id).await?;
        }
        Ok(())
    }

    pub async fn start_tree<M, P>(&self) -> Result<(), StartPuppetError>
    where
        M: Master,
        P: Master,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.start_tree_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn start_tree_by_id(&self, master: Id, puppet: Id) -> Result<(), StartPuppetError> {
        let mut queue = VecDeque::new();
        queue.push_back(puppet);

        while let Some(current_id) = queue.pop_front() {
            self.start_puppet_by_id(master, current_id).await?;

            let puppets_ids = self.get_puppets_by_id(current_id);
            for id in puppets_ids {
                queue.push_back(id);
            }
        }

        Ok(())
    }
}

/// STOP

impl Puppeter {
    pub async fn stop_puppet<M, P>(&self) -> Result<(), StopPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.stop_puppet_by_id(master, puppet).await
    }

    pub async fn stop_puppet_by_id(&self, master: Id, puppet: Id) -> Result<(), StopPuppetError> {
        match self.get_command_address_by_id(master, puppet) {
            Err(e) => Err(e.with_message("Can't stop puppet from another master"))?,
            Ok(None) => {
                Err(PuppetDoesNotExist {
                    name: puppet.into(),
                }
                .into())
            }
            Ok(Some(command_address)) => {
                let status = self.get_status_by_id(puppet).unwrap();
                match status {
                    LifecycleStatus::Inactive | LifecycleStatus::Deactivating => Ok(()),
                    _ => Ok(command_address.send_command(ServiceCommand::Stop).await?),
                }
            }
        }
    }

    pub async fn stop_puppets<M, P>(&self) -> Result<(), StopPuppetError>
    where
        M: Master,
        P: Master,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.stop_puppets_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn stop_puppets_by_id(&self, master: Id, puppet: Id) -> Result<(), StopPuppetError> {
        let puppets = self.get_puppets_by_id(puppet);
        for id in puppets.iter().rev() {
            self.stop_puppet_by_id(master, *id).await?
        }
        Ok(())
    }

    pub async fn stop_tree<M, P>(&self) -> Result<(), StopPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.stop_tree_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn stop_tree_by_id(&self, master: Id, puppet: Id) -> Result<(), StopPuppetError> {
        let mut stack = Vec::new();
        let mut queue = Vec::new();

        stack.push(puppet);

        while let Some(current_id) = stack.pop() {
            queue.push(current_id);

            for id in self.get_puppets_by_id(current_id) {
                stack.push(id);
            }
        }

        for id in queue.iter().rev() {
            self.stop_puppet_by_id(master, *id).await?;
        }

        Ok(())
    }
}

/// RESTART

impl Puppeter {
    pub async fn restart_puppet<M, P>(&self) -> Result<(), ResetPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.restart_puppet_by_id(master, puppet).await
    }

    pub async fn restart_puppet_by_id(
        &self,
        master: Id,
        puppet: Id,
    ) -> Result<(), ResetPuppetError> {
        match self.get_command_address_by_id(master, puppet) {
            Err(e) => Err(e.with_message("Can't restart puppet from another master"))?,
            Ok(None) => {
                Err(PuppetDoesNotExist {
                    name: puppet.into(),
                }
                .into())
            }
            Ok(Some(command_address)) => {
                let status = self.get_status_by_id(puppet).unwrap();
                match status {
                    LifecycleStatus::Restarting => Ok(()),
                    _ => {
                        Ok(command_address
                            .send_command(ServiceCommand::Restart)
                            .await?)
                    }
                }
            }
        }
    }

    pub async fn restart_puppets<M, P>(&self) -> Result<(), ResetPuppetError>
    where
        M: Master,
        P: Master,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.restart_puppets_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn restart_puppets_by_id(
        &self,
        master: Id,
        puppet: Id,
    ) -> Result<(), ResetPuppetError> {
        let puppets = self.get_puppets_by_id(puppet);
        for id in puppets.iter().rev() {
            self.restart_puppet_by_id(master, *id).await?;
        }
        Ok(())
    }

    pub async fn restart_tree<M, P>(&self) -> Result<(), ResetPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.restart_tree_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn restart_tree_by_id(&self, master: Id, puppet: Id) -> Result<(), ResetPuppetError> {
        let mut stack = Vec::new();
        let mut queue = Vec::new();

        stack.push(puppet);

        while let Some(current_id) = stack.pop() {
            queue.push(current_id);

            for id in self.get_puppets_by_id(current_id) {
                stack.push(id);
            }
        }

        for id in queue.iter().rev() {
            self.stop_puppet_by_id(master, *id).await?;
        }

        Ok(())
    }
}
//
// /// KILL
//
impl Puppeter {
    pub async fn kill_puppet<M, P>(&self) -> Result<(), KillPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.kill_puppet_by_id(master, puppet).await
    }

    pub async fn kill_puppet_by_id(&self, master: Id, puppet: Id) -> Result<(), KillPuppetError> {
        match self.get_command_address_by_id(master, puppet) {
            Err(e) => Err(e.with_message("Can't kill puppet from another master"))?,
            Ok(None) => {
                Err(PuppetDoesNotExist {
                    name: puppet.into(),
                }
                .into())
            }
            Ok(Some(command_address)) => {
                let status = self.get_status_by_id(puppet).unwrap();
                match status {
                    LifecycleStatus::Deactivating => Ok(()),
                    _ => {
                        command_address.send_command(ServiceCommand::Kill).await?;
                        self.remove_puppet_by_id(master, puppet);
                        Ok(())
                    }
                }
            }
        }
    }

    pub async fn kill_puppets<M, P>(&self) -> Result<(), KillPuppetError>
    where
        M: Master,
        P: Master,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.kill_puppets_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn kill_puppets_by_id(&self, master: Id, puppet: Id) -> Result<(), KillPuppetError> {
        let puppets = self.get_puppets_by_id(puppet);
        for id in puppets.iter().rev() {
            self.kill_puppet_by_id(master, *id).await?;
        }
        Ok(())
    }

    pub async fn kill_tree<M, P>(&self) -> Result<(), KillPuppetError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.kill_tree_by_id(master, puppet).await?;
        Ok(())
    }

    pub async fn kill_tree_by_id(&self, master: Id, puppet: Id) -> Result<(), KillPuppetError> {
        let mut stack = Vec::new();
        let mut queue = Vec::new();

        stack.push(puppet);

        while let Some(current_id) = stack.pop() {
            queue.push(current_id);

            for id in self.get_puppets_by_id(current_id) {
                stack.push(id);
            }
        }

        for id in queue.iter().rev() {
            self.kill_puppet_by_id(master, *id).await?;
        }

        Ok(())
    }
}

impl Puppeter {
    pub(crate) fn spawn<M, P>(
        &self,
        puppet: impl Into<PuppetStruct<P>>,
    ) -> Result<PuppetAddress<P>, PuppeterSpawnError>
    where
        M: Master,
        P: Puppet,
    {
        if self.is_puppet_exists::<P>() {
            Err(PuppetAlreadyExist {
                name: Id::new::<P>().into(),
            })?
        } else {
            let puppet_struct = puppet.into();
            let (handle, puppet_address, command_address) =
                create_puppeter_entities(&puppet_struct);

            self.set_status::<P>(LifecycleStatus::Activating);
            self.set_address::<P>(puppet_address.clone());
            self.set_command_address::<P>(command_address);
            self.set_master::<M, P>();
            tokio::spawn(run_puppet_loop(puppet_struct.puppet, handle));
            Ok(puppet_address)
        }
    }

    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppeterSendMessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let puppet = Id::new::<P>();
        if let Some(address) = self.get_address::<P>() {
            let status = self.get_status::<P>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => {
                    address
                        .tx
                        .send(message)
                        .await
                        .map_err(|err| PuppeterSendMessageError::from((err, String::from(puppet))))
                }
                _ => {
                    Err(PuppetCannotHandleMessage {
                        name: puppet.into(),
                        status,
                    })?
                }
            }
        } else {
            Err(PuppeterSendMessageError::PuppetDoesNotExist {
                name: puppet.into(),
            })
        }
    }

    pub async fn ask<P, E>(&self, message: E) -> Result<P::Response, PuppeterSendMessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let puppet = Id::new::<P>();
        if let Some(address) = self.get_address::<P>() {
            let status = self.get_status::<P>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => {
                    address
                        .tx
                        .send_and_await_response(message)
                        .await
                        .map_err(|err| PuppeterSendMessageError::from((err, String::from(puppet))))
                }
                _ => {
                    Err(PuppetCannotHandleMessage {
                        name: puppet.into(),
                        status,
                    })?
                }
            }
        } else {
            Err(PuppeterSendMessageError::PuppetDoesNotExist {
                name: puppet.into(),
            })
        }
    }

    pub async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<P::Response, PuppeterSendMessageError>
    where
        P: Handler<E>,
        E: Message + 'static,
    {
        let ask_future = self.ask::<P, E>(message);
        let timeout_future = tokio::time::sleep(duration);
        tokio::select! {
                    response = ask_future => response,
                    _ = timeout_future =>
        Err(PuppeterSendMessageError::RequestTimeout { name: Id::new::<P>().into() }),         }
    }

    pub async fn send_command<M, P>(
        &self,
        command: ServiceCommand,
    ) -> Result<(), PuppeterSendCommandError>
    where
        M: Master,
        P: Puppet,
    {
        let master = Id::new::<M>();
        let puppet = Id::new::<P>();
        self.send_command_by_id(master, puppet, command).await
    }

    pub async fn send_command_by_id(
        &self,
        master: Id,
        puppet: Id,
        command: ServiceCommand,
    ) -> Result<(), PuppeterSendCommandError> {
        match self.get_command_address_by_id(master, puppet) {
            Err(e) => Err(e.with_message("Can't kill puppet from another master"))?,
            Ok(None) => {
                Err(PuppetDoesNotExist {
                    name: puppet.into(),
                }
                .into())
            }
            Ok(Some(command_address)) => {
                command_address
                    .command_tx
                    .send_and_await_response(command)
                    .await
                    .map_err(|err| PuppeterSendCommandError::from((err, String::from(puppet))))
            }
        }
    }

    pub async fn report_failure<P>(&self, error: &impl Into<PuppetError>)
    where
        P: Puppet,
    {
        let master = self.get_master::<P>();
        // self.send_command_by_id(master, puppet, ServiceCommand::ReportFailure
        // { puppet: (), message: () })
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
    // pub fn with_minion<A>(self, minion: impl Into<PuppetStruct<A>>) ->
    // Result<Self, PuppeterError> where
    //     A: Puppet,
    // {
    //     self.spawn(minion)?;
    //     Ok(self)
    // }
}

pub(crate) async fn run_puppet_loop<P>(
    mut puppet: P,
    mut handle: PuppetHandler<P>,
) -> Result<(), PuppeterSendCommandError>
where
    P: Puppet,
{
    puppet._start().await?;

    loop {
        let Some(status) = PUPPETER.get().unwrap().get_status::<P>() else {
            break;
        };
        if status.should_wait_for_activation() {
            continue;
        }
        tokio::select! {
            Some(ServicePacket {cmd, reply_address}) =
                handle.command_rx.recv() => {         let response =
                    puppet.handle_command(cmd).await;
                    reply_address.send(response).unwrap_or_else(|_| println!("{}
                            failed to send response", handle));
                }
            Some(mut envelope) = handle.rx.recv() => {
                if status.should_handle_message() {
                    envelope.handle_message(&mut puppet).await;
                } else if status.should_drop_message() {
                    envelope.reply_error(PuppetCannotHandleMessage{name: Id::new::<P>().into(), status}.into()).await;
                }
            }
            else => {
                break;
            }
        }
    }
    Ok(())
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
        id: Id::new::<P>(),
        rx: Mailbox::new(rx),
        command_rx: ServiceMailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = ServicePostman::new(command_tx);
    let puppet_address = PuppetAddress {
        id: Id::new::<P>(),
        tx,
    };
    let command_address = CommandAddress {
        id: Id::new::<P>(),
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
    use puppeter_derive::{Master, Message, Puppet};

    use crate::executor;

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

        #[derive(Debug, Default, Clone, Puppet)]
        pub struct SleepActor {
            i: i32,
        }

        #[async_trait]
        impl Handler<SleepMessage> for SleepActor {
            type Response = i32;
            type Executor = executor::SequentialExecutor;
            async fn handle_message(&mut self, msg: SleepMessage) -> Result<i32, PuppetError> {
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
                Ok(msg.i)
            }
        }

        let _ = SleepActor { i: 0 }.spawn();

        puppeter()
            .send::<SleepActor, _>(SleepMessage { i: 10 })
            .await
            .unwrap();
        puppeter()
            .send::<SleepActor, _>(SleepMessage { i: 10 })
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
