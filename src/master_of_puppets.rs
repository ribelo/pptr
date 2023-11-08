use std::sync::{Arc, Mutex};

use indexmap::IndexSet;
use rustc_hash::FxHashMap as HashMap;
use tokio::sync::{mpsc, watch};

use crate::{
    address::Address,
    errors::{
        PermissionDeniedError, PuppetAlreadyExist, PuppetCannotHandleMessage,
        PuppetDoesNotExistError, PuppetError, PuppetOperationError, PuppetRegisterError,
        PuppetSendCommandError, PuppetSendMessageError,
    },
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    pid::Pid,
    puppet::{
        Handler, Lifecycle, LifecycleStatus, PuppetBuilder, PuppetHandle, Puppeter, ResponseFor,
    },
    BoxedAny,
};

#[derive(Debug, Clone)]
pub struct MasterOfPuppets {
    inner: Arc<Mutex<MasterOfPuppetsInner>>,
}

#[derive(Debug)]
pub struct MasterOfPuppetsInner {
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

impl Default for MasterOfPuppets {
    fn default() -> Self {
        Self::new()
    }
}

impl MasterOfPuppets {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MasterOfPuppetsInner {
                message_postmans: HashMap::default(),
                service_postmans: HashMap::default(),
                statuses: HashMap::default(),
                master_to_puppets: HashMap::default(),
                puppet_to_master: HashMap::default(),
            })),
        }
    }

    pub fn register<M, P>(
        &self,
        postman: Postman<P>,
        service_postman: ServicePostman,
        status_tx: watch::Sender<LifecycleStatus>,
        status_rx: watch::Receiver<LifecycleStatus>,
    ) -> Result<(), PuppetRegisterError>
    where
        M: Lifecycle,
        P: Lifecycle,
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
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.is_puppet_exists_by_pid(puppet)
    }

    pub fn is_puppet_exists_by_pid(&self, puppet: Pid) -> bool {
        self.get_master_by_pid(puppet).is_some()
    }

    pub fn get_postman<P>(&self) -> Option<Postman<P>>
    where
        P: Lifecycle,
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

    pub fn set_master_by_pid(
        &self,
        old_master: Pid,
        new_master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.has_permission_by_pid(old_master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(old_master, puppet)
                    .with_message("Cannot change master of another master puppet")
                    .into())
            }
            Some(true) => {
                let mut inner = self.inner.lock().expect("Failed to acquire mutex lock");

                // Remove puppet from old master
                inner
                    .master_to_puppets
                    .get_mut(&old_master)
                    .expect("Old master has no puppets")
                    .shift_remove(&puppet);

                // Add puppet to new master
                inner
                    .master_to_puppets
                    .entry(new_master)
                    .or_default()
                    .insert(puppet);

                // Set new master for puppet
                inner.puppet_to_master.insert(puppet, new_master);
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
                self.set_master_by_pid(master, puppet, puppet).unwrap();
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

                // Delete status from statuses
                inner.statuses.remove(&puppet);

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
        P: Handler<E>,
        E: Message,
    {
        let address = self
            .get_postman::<P>()
            .ok_or_else(PuppetDoesNotExistError::from_type::<P>)?;
        Ok(address.send::<E>(message).await?)
    }

    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
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
        P: Handler<E>,
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
        M: Lifecycle,
        P: Lifecycle,
    {
        let master_pid = Pid::new::<M>();
        let puppet_pid = Pid::new::<P>();

        if !self.is_puppet_exists::<M>() && master_pid != puppet_pid {
            return Err(PuppetDoesNotExistError::from_type::<M>().into());
        }

        let mut builder = builder.into();
        let mut puppet = builder.puppet.take().unwrap();
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

        let mut puppeter = Puppeter {
            pid,
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

        puppet.on_init(&puppeter).await?;
        puppeter.start(&mut puppet, false).await?;

        tokio::spawn(run_puppet_loop(puppet, puppeter, handle));
        Ok(address)
    }
}

pub(crate) async fn run_puppet_loop<P>(
    mut puppet: P,
    mut puppeter: Puppeter,
    mut handle: PuppetHandle<P>,
) where
    P: Lifecycle,
{
    let mut puppet_status = handle.status_rx;

    loop {
        tokio::select! {
            res = puppet_status.changed() => {
                match res {
                    Ok(_) => {
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Inactive
                            | LifecycleStatus::Failed) {
                            println!("Stopping loop due to puppet status change");
                            tracing::info!(puppet = puppeter.pid.to_string(),  "Stopping loop due to puppet status change");
                            break;
                        }
                    }
                    Err(_) => {
                        println!("Stopping loop due to closed puppet status channel");
                        tracing::debug!(puppet = puppeter.pid.to_string(),  "Stopping loop due to closed puppet status channel");
                        break;
                    }
                }
            }
            res = handle.command_rx.recv() => {
                match res {
                    Some(mut service_packet) => {
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                            service_packet.handle_command(&mut puppet, &mut puppeter).await;
                        } else {
                            tracing::debug!(puppet = puppeter.pid.to_string(),  "Ignoring command due to non-Active puppet status");
                            let error_response = PuppetCannotHandleMessage::new(puppeter.pid, *puppet_status.borrow()).into();
                            service_packet.reply_error(error_response).await;
                        }
                    }
                    None => {
                        tracing::debug!(puppet = puppeter.pid.to_string(),  "Stopping loop due to closed command channel");
                        break;
                    }
                }
            }
            res = handle.message_rx.recv() => {
                match res {
                    Some(mut envelope) => {
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                            // TODO: Handle error
                            envelope.handle_message(&mut puppet, &mut puppeter).await;
                        } else {
                            let status = *puppet_status.borrow();
                            tracing::debug!(puppet = puppeter.pid.to_string(),  "Ignoring message due to non-Active puppet status");
                            envelope.reply_error(PuppetCannotHandleMessage::new(puppeter.pid, status).into()).await;
                        }
                    }
                    None => {
                        tracing::debug!(puppet = puppeter.pid.to_string(),  "Stopping loop due to closed message channel");
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;

    use crate::{executor::SequentialExecutor, supervision::strategy::OneForAll};

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct MasterActor {}
    impl Lifecycle for MasterActor {
        type Supervision = OneForAll;
    }

    #[derive(Debug, Clone, Default)]
    struct PuppetActor {}

    impl Lifecycle for PuppetActor {
        type Supervision = OneForAll;
    }

    #[derive(Debug)]
    struct MasterMessage {}

    impl Message for MasterMessage {}

    #[derive(Debug)]
    struct PuppetMessage {}

    impl Message for PuppetMessage {}

    #[async_trait]
    impl Handler<MasterMessage> for MasterActor {
        type Response = ();
        type Executor = SequentialExecutor;
        async fn handle_message(
            &mut self,
            msg: MasterMessage,
            puppeter: &Puppeter,
        ) -> Result<Self::Response, PuppetError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Handler<PuppetMessage> for PuppetActor {
        type Response = ();
        type Executor = SequentialExecutor;
        async fn handle_message(
            &mut self,
            msg: PuppetMessage,
            puppeter: &Puppeter,
        ) -> Result<Self::Response, PuppetError> {
            Ok(())
        }
    }

    pub fn register_puppet<M, P>(mop: &MasterOfPuppets) -> Result<(), PuppetRegisterError>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let (message_tx, _message_rx) = mpsc::channel::<Box<dyn Envelope<P>>>(1);
        let (service_tx, _service_rx) = mpsc::channel::<ServicePacket>(1);
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(service_tx);

        mop.register::<M, P>(postman, service_postman, status_tx, status_rx)
    }

    #[tokio::test]
    async fn test_register() {
        let mop = MasterOfPuppets::new();

        let res = register_puppet::<MasterActor, PuppetActor>(&mop);

        // Master puppet doesn't exist

        assert!(res.is_err());

        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);

        // Master is same as puppet

        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);

        // Puppet already exists

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_is_puppet_exists() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        assert!(mop.is_puppet_exists::<PuppetActor>());
        assert!(!mop.is_puppet_exists::<MasterActor>());
    }

    #[tokio::test]
    async fn test_get_postman() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        assert!(mop.get_postman::<PuppetActor>().is_some());
        assert!(mop.get_postman::<MasterActor>().is_none());
    }

    #[tokio::test]
    async fn test_get_service_postman_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop.get_service_postman_by_pid(puppet_pid).is_some());
        let master_pid = Pid::new::<MasterActor>();
        assert!(mop.get_service_postman_by_pid(master_pid).is_none());
    }

    #[tokio::test]
    async fn test_get_status_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        assert_eq!(
            mop.get_status_by_pid(puppet_pid),
            Some(LifecycleStatus::Inactive)
        );
        let master_pid = Pid::new::<MasterActor>();
        assert!(mop.get_status_by_pid(master_pid).is_none());
    }

    #[tokio::test]
    async fn test_set_status_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        mop.set_status_by_pid(puppet_pid, LifecycleStatus::Active);
        assert_eq!(
            mop.get_status_by_pid(puppet_pid),
            Some(LifecycleStatus::Active)
        );
    }

    #[tokio::test]
    async fn test_subscribe_status_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        let rx = mop.subscribe_status_by_pid(puppet_pid).unwrap();
        mop.set_status_by_pid(puppet_pid, LifecycleStatus::Active);
        assert_eq!(*rx.borrow(), LifecycleStatus::Active);
    }

    #[tokio::test]
    async fn test_has_puppet_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop.has_puppet_by_pid(master_pid, puppet_pid).is_some());
        let master_pid = Pid::new::<PuppetActor>();
        assert!(mop.has_puppet_by_pid(master_pid, puppet_pid).is_none());
    }

    #[tokio::test]
    async fn test_has_permission_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop.has_permission_by_pid(master_pid, puppet_pid).is_some());
        let master_pid = Pid::new::<PuppetActor>();
        assert!(mop.has_permission_by_pid(master_pid, puppet_pid).is_none());
    }

    #[tokio::test]
    async fn test_get_master_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert_eq!(mop.get_master_by_pid(puppet_pid), Some(master_pid));
        assert_eq!(mop.get_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_set_master_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<PuppetActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop
            .set_master_by_pid(puppet_pid, master_pid, puppet_pid)
            .is_ok());
        assert_eq!(mop.get_master_by_pid(puppet_pid), Some(master_pid));
        assert_eq!(mop.get_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_get_puppets_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        let puppets = mop.get_puppets_by_pid(master_pid).unwrap();
        assert_eq!(puppets.len(), 2);
        assert!(puppets.contains(&puppet_pid));
    }

    #[tokio::test]
    async fn test_detach_puppet_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop.detach_puppet_by_pid(master_pid, puppet_pid).is_ok());
        assert_eq!(mop.get_master_by_pid(puppet_pid), Some(puppet_pid));
        assert_eq!(mop.get_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_delete_puppet_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = register_puppet::<MasterActor, MasterActor>(&mop);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&mop);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(mop.delete_puppet_by_pid(master_pid, puppet_pid).is_ok());
        assert!(mop.get_postman::<PuppetActor>().is_none());
        assert!(mop.get_postman::<MasterActor>().is_some());
    }

    #[tokio::test]
    async fn test_spawn() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());
        let res = mop
            .spawn::<MasterActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_send() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());

        let res = mop
            .send::<PuppetActor, PuppetMessage>(PuppetMessage {})
            .await;
        assert!(res.is_ok());

        let res = mop
            .send::<MasterActor, MasterMessage>(MasterMessage {})
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());

        let res = mop
            .ask::<PuppetActor, PuppetMessage>(PuppetMessage {})
            .await;
        assert!(res.is_ok());

        let res = mop
            .ask::<MasterActor, MasterMessage>(MasterMessage {})
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask_with_timeout() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());
        let res = mop
            .ask_with_timeout::<PuppetActor, PuppetMessage>(
                PuppetMessage {},
                Duration::from_secs(1),
            )
            .await;
        assert!(res.is_ok());
        let res = mop
            .send::<MasterActor, MasterMessage>(MasterMessage {})
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_send_command_stop_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        let res = mop
            .send_command_by_pid(puppet_pid, puppet_pid, ServiceCommand::Stop)
            .await;
        assert!(res.is_ok());
        let status = mop.get_status_by_pid(puppet_pid).unwrap();
        assert_eq!(status, LifecycleStatus::Inactive);
    }

    #[tokio::test]
    async fn test_send_command_restart_by_pid() {
        let mop = MasterOfPuppets::new();
        let res = mop
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor {}))
            .await;
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        let res = mop
            .send_command_by_pid(
                puppet_pid,
                puppet_pid,
                ServiceCommand::Restart { stage: None },
            )
            .await;
        assert!(res.is_ok());
        let status = mop.get_status_by_pid(puppet_pid).unwrap();
        assert_eq!(status, LifecycleStatus::Active);
    }
}
