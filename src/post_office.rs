use std::sync::{Arc, Mutex, RwLock};

use indexmap::IndexSet;
use rustc_hash::FxHashMap as HashMap;
use tokio::sync::{mpsc, watch};

use crate::{
    address::Address,
    errors::{
        PermissionDeniedError, PostmanError, PuppetAlreadyExist, PuppetDoesNotExistError,
        PuppetError, PuppetOperationError, PuppetRegisterError, PuppetSendCommandError,
        PuppetSendMessageError,
    },
    master::run_puppet_loop,
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    pid::Pid,
    puppet::{
        Handler, Lifecycle, LifecycleStatus, Puppet, PuppetBuilder, PuppetHandle, PuppetState,
        ResponseFor,
    },
    BoxedAny,
};

#[derive(Debug, Clone)]
pub struct PostOffice {
    inner: Arc<Mutex<PostOfficeInner>>,
}

#[derive(Debug)]
pub struct PostOfficeInner {
    pub(crate) message_postmans: HashMap<Pid, BoxedAny>,
    pub(crate) service_postmans: HashMap<Pid, ServicePostman>,
    pub(crate) statuses: HashMap<
        Pid,
        (
            watch::Sender<LifecycleStatus>,
            watch::Receiver<LifecycleStatus>,
        ),
    >,
    pub(crate) master_to_puppets: HashMap<Pid, IndexSet<Pid>>,
    pub(crate) puppet_to_master: HashMap<Pid, Pid>,
}

impl Default for PostOffice {
    fn default() -> Self {
        Self::new()
    }
}

impl PostOffice {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PostOfficeInner {
                message_postmans: HashMap::default(),
                service_postmans: HashMap::default(),
                statuses: HashMap::default(),
                master_to_puppets: HashMap::default(),
                puppet_to_master: HashMap::default(),
            })),
        }
    }

    pub fn with_store<R>(&self, f: Box<dyn FnOnce(&PostOfficeInner) -> R>) -> R {
        let inner = self.inner.lock().expect("Failed to acquire mutex lock");
        f(&inner)
    }

    pub fn with_store_mut<R>(&self, mut f: Box<dyn FnMut(&mut PostOfficeInner) -> R>) -> R {
        let mut inner = self.inner.lock().expect("Failed to acquire mutex lock");

        f(&mut inner)
    }

    pub fn register<M, P>(
        &self,
        postman: Postman<P>,
        service_postman: ServicePostman,
        status_tx: watch::Sender<LifecycleStatus>,
        status_rx: watch::Receiver<LifecycleStatus>,
    ) -> Result<(), PuppetRegisterError>
    where
        M: PuppetState,
        Puppet<M>: Lifecycle,
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        // Create Pid references for the master and puppet
        let master = Pid::new::<M>();
        let puppet = Pid::new::<P>();

        // Check if the puppet already exists by its Pid
        if self.is_puppet_exists_by_pid(puppet) {
            // If the puppet exists, return an error indicating it's already registered
            return Err(PuppetAlreadyExist::new(puppet).into());
        }

        // Check if the master exists
        if !self.is_puppet_exists_by_pid(master) && master != puppet {
            // If the master doesn't exist, return an error indicating it doesn't exist
            return Err(PuppetDoesNotExistError::new(master).into());
        }

        // Access the inner state by acquiring a lock on the mutex
        let mut inner = self.inner.lock().expect("Failed to acquire mutex lock");

        // Add the puppet's postman to the message_postmans map
        inner.message_postmans.insert(puppet, Box::new(postman));

        // Add the puppet's service postman to the service_postmans map
        inner.service_postmans.insert(puppet, service_postman);

        // Add the puppet to the master's set of puppets
        inner
            .master_to_puppets
            .entry(master)
            .or_default()
            .insert(puppet);

        // Associate the puppet with the master in the puppet_to_master map
        inner.puppet_to_master.insert(puppet, master);

        // Add the puppet's status receiver to the statuses map
        inner.statuses.insert(puppet, (status_tx, status_rx));

        // Return a successful result indicating the registration was successful
        Ok(())
    }

    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.is_puppet_exists_by_pid(puppet)
    }

    pub fn is_puppet_exists_by_pid(&self, puppet: Pid) -> bool {
        self.get_master_by_pid(puppet).is_some()
    }

    pub fn get_postman<P>(&self) -> Option<Postman<P>>
    where
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .message_postmans
            .get(&puppet)
            .and_then(|boxed| boxed.downcast_ref::<Postman<P>>())
            .cloned()
    }

    pub(crate) fn get_service_postman_by_pid(&self, puppet: Pid) -> Option<ServicePostman> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .service_postmans
            .get(&puppet)
            .cloned()
    }

    pub(crate) fn set_status_by_pid(&self, puppet: Pid, status: LifecycleStatus) {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .statuses
            .get(&puppet)
            .map(|(tx, _)| {
                tx.send_if_modified(|current| {
                    if *current != status {
                        *current = status;
                        true
                    } else {
                        false
                    }
                })
            });
    }

    pub fn subscribe_status_by_pid(&self, puppet: Pid) -> Option<watch::Receiver<LifecycleStatus>> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .statuses
            .get(&puppet)
            .map(|(_, rx)| rx.clone())
    }

    pub fn get_status_by_pid(&self, puppet: Pid) -> Option<LifecycleStatus> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .statuses
            .get(&puppet)
            .map(|(_, rx)| *rx.borrow())
    }

    pub fn has_puppet_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .master_to_puppets
            .get(&master)
            .map(|puppets| puppets.contains(&puppet))
    }

    pub fn has_permission_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.has_puppet_by_pid(master, puppet)
    }

    pub fn get_master_by_pid(&self, puppet: Pid) -> Option<Pid> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .puppet_to_master
            .get(&puppet)
            .cloned()
    }

    pub fn set_master_by_pid(&self, master: Pid, puppet: Pid) -> Result<(), PuppetOperationError> {
        match self.has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot change master of another master puppet")
                    .into())
            }
            Some(true) => {
                let mut inner = self.inner.lock().expect("Failed to acquire mutex lock");
                let old_master = self
                    .get_master_by_pid(puppet)
                    .expect("Puppet has no master");

                // Remove puppet from old master
                inner
                    .master_to_puppets
                    .get_mut(&old_master)
                    .expect("Old master has no puppets")
                    .shift_remove(&puppet);

                // Add puppet to new master
                inner
                    .master_to_puppets
                    .entry(master)
                    .or_default()
                    .insert(puppet);

                // Set new master for puppet
                inner.puppet_to_master.insert(puppet, master);
                Ok(())
            }
        }
    }

    pub fn get_puppets_by_pid(&self, master: Pid) -> Option<IndexSet<Pid>> {
        self.inner
            .lock()
            .expect("Failed to acquire mutex lock")
            .master_to_puppets
            .get(&master)
            .cloned()
    }

    pub fn detach_puppet_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot detach puppet from another master")
                    .into())
            }
            Some(true) => {
                self.set_master_by_pid(puppet, puppet).unwrap();
                Ok(())
            }
        }
    }

    pub fn delete_puppet_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot delete puppet of another master")
                    .into())
            }
            Some(true) => {
                let mut inner = self.inner.lock().expect("Failed to acquire mutex lock");
                // Delete address from addresses
                inner.message_postmans.remove(&puppet);

                // Delete puppet from master_to_puppets
                inner
                    .master_to_puppets
                    .get_mut(&master)
                    .expect("Master has no puppets")
                    .shift_remove(&puppet);

                // Delete puppet from puppet_to_master
                inner.puppet_to_master.remove(&puppet);

                // TODO: Break loop
                Ok(())
            }
        }
    }

    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        let address = self
            .get_postman::<P>()
            .ok_or_else(PuppetDoesNotExistError::from_type::<P>)?;
        Ok(address.send::<E>(message).await?)
    }

    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        let address = self
            .get_postman::<P>()
            .ok_or_else(PuppetDoesNotExistError::from_type::<P>)?;
        Ok(address.send_and_await_response::<E>(message, None).await?)
    }

    pub async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: PuppetState,
        Puppet<P>: Handler<E>,
        E: Message,
    {
        let address = self
            .get_postman::<P>()
            .ok_or_else(PuppetDoesNotExistError::from_type::<P>)?;
        Ok(address
            .send_and_await_response::<E>(message, Some(duration))
            .await?)
    }

    pub async fn send_command_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError> {
        match self.has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot send command to puppet of another master")
                    .into())
            }
            Some(true) => {
                let serivce_address = self.get_service_postman_by_pid(puppet).unwrap();
                Ok(serivce_address
                    .send_and_await_response(puppet, command, None)
                    .await?)
            }
        }
    }

    pub async fn spawn<M, P>(
        &self,
        builder: impl Into<PuppetBuilder<P>>,
    ) -> Result<Address<P>, PuppetError>
    where
        M: PuppetState,
        Puppet<M>: Lifecycle,
        P: PuppetState,
        Puppet<P>: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();

        if !self.is_puppet_exists::<M>() && master_pid != puppet_pid {
            return Err(PuppetDoesNotExistError::from_type::<M>().into());
        }

        let mut builder = builder.into();
        let pid = Pid::new::<P>();
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let (message_tx, message_rx) =
            mpsc::channel::<Box<dyn Envelope<P>>>(builder.messages_bufer_size.into());
        let (command_tx, command_rx) =
            mpsc::channel::<ServicePacket>(builder.commands_bufer_size.into());
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(command_tx);
        self.register::<M, P>(
            postman.clone(),
            service_postman,
            status_tx,
            status_rx.clone(),
        )?;
        let retry_config = builder.retry_config.take().unwrap();

        let mut puppet = Puppet {
            pid,
            state: builder.state.clone(),
            message_tx: postman.clone(),
            context: Default::default(),
            post_office: self.clone(),
            retry_config,
        };

        let handle = PuppetHandle {
            pid,
            status_rx: status_rx.clone(),
            message_rx: Mailbox::new(message_rx),
            command_rx: ServiceMailbox::new(command_rx),
        };

        let address = Address {
            pid,
            status_rx,
            message_tx: postman,
            post_office: self.clone(),
        };

        puppet.on_init().await?;
        puppet.start(false).await?;

        tokio::spawn(run_puppet_loop(puppet, handle));
        Ok(address)
    }
}
