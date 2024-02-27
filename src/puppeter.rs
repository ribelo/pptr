use std::{
    hash::BuildHasherDefault,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
};

use atomic_take::AtomicTake;
use indexmap::IndexSet;
use rustc_hash::{FxHashMap, FxHasher};
use tokio::sync::{mpsc, watch};

use crate::{
    address::Address,
    errors::{
        PermissionDeniedError, PuppetAlreadyExist, PuppetCannotHandleMessage,
        PuppetDoesNotExistError, PuppetError, PuppetOperationError, PuppetSendCommandError,
        PuppetSendMessageError, ResourceAlreadyExist,
    },
    executor::DedicatedExecutor,
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    pid::{Id, Pid},
    prelude::CriticalError,
    puppet::{
        Context, Handler, Lifecycle, LifecycleStatus, PuppetBuilder, PuppetHandle, ResponseFor,
    },
    BoxedAny,
};

pub type StatusChannels = (
    watch::Sender<LifecycleStatus>,
    watch::Receiver<LifecycleStatus>,
);

type FxIndexSet<T> = IndexSet<T, BuildHasherDefault<FxHasher>>;

#[derive(Clone, Debug)]
pub struct Puppeter {
    pub(crate) message_postmans: Arc<Mutex<FxHashMap<Pid, BoxedAny>>>,
    pub(crate) service_postmans: Arc<Mutex<FxHashMap<Pid, ServicePostman>>>,
    pub(crate) statuses: Arc<Mutex<FxHashMap<Pid, StatusChannels>>>,
    pub(crate) master_to_puppets: Arc<Mutex<FxHashMap<Pid, FxIndexSet<Pid>>>>,
    pub(crate) puppet_to_master: Arc<Mutex<FxHashMap<Pid, Pid>>>,
    pub(crate) resources: Arc<Mutex<FxHashMap<Id, Arc<Mutex<BoxedAny>>>>>,
    pub(crate) executor: DedicatedExecutor,
    pub(crate) failure_tx: mpsc::UnboundedSender<CriticalError>,
    pub(crate) failure_rx: Arc<AtomicTake<mpsc::UnboundedReceiver<CriticalError>>>,
}

impl Default for Puppeter {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::expect_used)]
impl Puppeter {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let cpus = NonZeroUsize::new(num_cpus::get()).expect("Failed to get number of CPUs");
        let executor = DedicatedExecutor::new(cpus);
        Self {
            message_postmans: Arc::default(),
            service_postmans: Arc::default(),
            statuses: Arc::default(),
            master_to_puppets: Arc::default(),
            puppet_to_master: Arc::default(),
            resources: Arc::default(),
            executor,
            failure_tx: tx,
            failure_rx: Arc::new(AtomicTake::new(rx)),
        }
    }

    pub async fn on_unrecoverable_failure<F>(&self, f: F)
    where
        F: FnOnce(
                Puppeter,
                CriticalError,
            ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
            + Send
            + 'static,
    {
        if let Some(mut rx) = self.failure_rx.take() {
            if let Some(err) = rx.recv().await {
                f(self.clone(), err).await;
            }
        }
    }

    pub async fn wait_for_unrecoverable_failure(self) -> CriticalError {
        self.failure_rx.take().unwrap().recv().await.unwrap()
    }

    pub fn register_puppet<M, P>(
        &self,
        postman: Postman<P>,
        service_postman: ServicePostman,
        status_tx: watch::Sender<LifecycleStatus>,
        status_rx: watch::Receiver<LifecycleStatus>,
    ) -> Result<(), PuppetError>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        // Create Pid references for the master and puppet
        let master = Pid::new::<M>();
        self.register_puppet_by_pid(master, postman, service_postman, status_tx, status_rx)
    }

    pub(crate) fn register_puppet_by_pid<P>(
        &self,
        master: Pid,
        postman: Postman<P>,
        service_postman: ServicePostman,
        status_tx: watch::Sender<LifecycleStatus>,
        status_rx: watch::Receiver<LifecycleStatus>,
    ) -> Result<(), PuppetError>
    where
        P: Lifecycle,
    {
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

        // Add the puppet's postman to the message_postmans map
        self.message_postmans
            .lock()
            .expect("Failed to acquire mutex lock")
            .insert(puppet, Box::new(postman));

        // Add the puppet's service postman to the service_postmans map
        self.service_postmans
            .lock()
            .expect("Failed to acquire mutex lock")
            .insert(puppet, service_postman);

        // Add the puppet to the master's set of puppets
        self.master_to_puppets
            .lock()
            .expect("Failed to acquire mutex lock")
            .entry(master)
            .or_default()
            .insert(puppet);

        // Associate the puppet with the master in the puppet_to_master map
        self.puppet_to_master
            .lock()
            .expect("Failed to acquire mutex lock")
            .insert(puppet, master);

        // Add the puppet's status receiver to the statuses map
        self.statuses
            .lock()
            .expect("Failed to acquire mutex lock")
            .insert(puppet, (status_tx, status_rx));

        // Return a successful result indicating the registration was successful
        Ok(())
    }

    #[must_use]
    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.is_puppet_exists_by_pid(puppet)
    }

    pub(crate) fn is_puppet_exists_by_pid(&self, puppet: Pid) -> bool {
        self.get_puppet_master_by_pid(puppet).is_some()
    }

    #[must_use]
    pub fn get_postman<P>(&self) -> Option<Postman<P>>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.message_postmans
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .and_then(|boxed| boxed.downcast_ref::<Postman<P>>())
            .cloned()
    }

    pub(crate) fn get_service_postman_by_pid(&self, puppet: Pid) -> Option<ServicePostman> {
        self.service_postmans
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .cloned()
    }

    pub(crate) fn set_status_by_pid(&self, puppet: Pid, status: LifecycleStatus) {
        self.statuses
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .map(|(tx, _)| {
                tx.send_if_modified(|current| {
                    if *current == status {
                        false
                    } else {
                        *current = status;
                        true
                    }
                })
            });
    }

    #[must_use]
    pub fn subscribe_puppet_status<P>(&self) -> Option<watch::Receiver<LifecycleStatus>>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.subscribe_puppet_status_by_pid(puppet)
    }

    pub(crate) fn subscribe_puppet_status_by_pid(
        &self,
        puppet: Pid,
    ) -> Option<watch::Receiver<LifecycleStatus>> {
        self.statuses
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .map(|(_, rx)| rx.clone())
    }

    #[must_use]
    pub fn get_puppet_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.get_puppet_status_by_pid(puppet)
    }

    pub(crate) fn get_puppet_status_by_pid(&self, puppet: Pid) -> Option<LifecycleStatus> {
        self.statuses
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .map(|(_, rx)| *rx.borrow())
    }

    pub(crate) fn puppet_has_puppet_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.master_to_puppets
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&master)
            .map(|puppets| puppets.contains(&puppet))
    }

    pub(crate) fn puppet_has_permission_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.puppet_has_puppet_by_pid(master, puppet)
    }

    pub(crate) fn get_puppet_master_by_pid(&self, puppet: Pid) -> Option<Pid> {
        self.puppet_to_master
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .copied()
    }

    pub fn set_puppet_master<O, M, P>(&self) -> Result<(), PuppetOperationError>
    where
        O: Lifecycle,
        M: Lifecycle,
        P: Lifecycle,
    {
        let old_master = Pid::new::<O>();
        let new_master = Pid::new::<M>();
        let puppet = Pid::new::<P>();
        self.set_puppet_master_by_pid(old_master, new_master, puppet)
    }

    pub(crate) fn set_puppet_master_by_pid(
        &self,
        old_master: Pid,
        new_master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.puppet_has_permission_by_pid(old_master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(old_master, puppet)
                    .with_message("Cannot change master of another master puppet")
                    .into())
            }
            Some(true) => {
                // Remove puppet from old master
                self.master_to_puppets
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .get_mut(&old_master)
                    .expect("Old master has no puppets")
                    .shift_remove(&puppet);

                // Add puppet to new master
                self.master_to_puppets
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .entry(new_master)
                    .or_default()
                    .insert(puppet);

                // Set new master for puppet
                self.puppet_to_master
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .insert(puppet, new_master);
                Ok(())
            }
        }
    }

    pub(crate) fn get_puppets_by_pid(&self, master: Pid) -> Option<FxIndexSet<Pid>> {
        self.master_to_puppets
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&master)
            .cloned()
    }

    pub(crate) fn detach_puppet_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.puppet_has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot detach puppet from another master")
                    .into())
            }
            Some(true) => {
                self.set_puppet_master_by_pid(master, puppet, puppet)?;
                Ok(())
            }
        }
    }

    pub fn delete_puppet<O, P>(&self) -> Result<(), PuppetOperationError>
    where
        O: Lifecycle,
        P: Lifecycle,
    {
        let master = Pid::new::<O>();
        let puppet = Pid::new::<P>();
        self.delete_puppet_by_pid(master, puppet)
    }

    pub(crate) fn delete_puppet_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetOperationError> {
        match self.puppet_has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot delete puppet of another master")
                    .into())
            }
            Some(true) => {
                // Delete address from addresses
                self.message_postmans
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .remove(&puppet);

                // Delete status from statuses
                self.statuses
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .remove(&puppet);

                // Delete puppet from master_to_puppets
                self.master_to_puppets
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .get_mut(&master)
                    .expect("Master has no puppets")
                    .shift_remove(&puppet);

                // Delete puppet from puppet_to_master
                self.puppet_to_master
                    .lock()
                    .expect("Failed to acquire mutex lock")
                    .remove(&puppet);

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

    pub(crate) async fn ask_with_timeout<P, E>(
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

    pub(crate) async fn send_command_by_pid(
        &self,
        master: Pid,
        puppet: Pid,
        command: ServiceCommand,
    ) -> Result<(), PuppetSendCommandError> {
        match self.puppet_has_permission_by_pid(master, puppet) {
            None => Err(PuppetDoesNotExistError::new(puppet).into()),
            Some(false) => {
                Err(PermissionDeniedError::new(master, puppet)
                    .with_message("Cannot send command to puppet of another master")
                    .into())
            }
            Some(true) => {
                let Some(serivce_address) = self.get_service_postman_by_pid(puppet) else {
                    return Err(PuppetDoesNotExistError::new(puppet).into());
                };
                Ok(serivce_address
                    .send_and_await_response(puppet, command, None)
                    .await?)
            }
        }
    }

    #[allow(clippy::impl_trait_in_params)]
    pub async fn spawn<M, P>(
        &self,
        builder: impl Into<PuppetBuilder<P>> + Send,
    ) -> Result<Address<P>, PuppetError>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let master = Pid::new::<M>();
        self.spawn_puppet_by_pid(master, builder).await
    }

    #[allow(clippy::impl_trait_in_params)]
    pub async fn spawn_self<P>(
        &self,
        builder: impl Into<PuppetBuilder<P>> + Send,
    ) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        let master = Pid::new::<P>();
        self.spawn_puppet_by_pid(master, builder).await
    }

    pub(crate) async fn spawn_puppet_by_pid<P>(
        &self,
        master_pid: Pid,
        builder: impl Into<PuppetBuilder<P>> + Send,
    ) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        let puppet_pid = Pid::new::<P>();
        if !self.is_puppet_exists_by_pid(master_pid) && master_pid != puppet_pid {
            return Err(PuppetDoesNotExistError::new(master_pid).into());
        }

        let mut builder = builder.into();
        let Some(mut puppet) = builder.puppet.take() else {
            return Err(PuppetError::critical(
                puppet_pid,
                "PuppetBuilder has no puppet",
            ));
        };
        let pid = Pid::new::<P>();
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let (message_tx, message_rx) =
            mpsc::channel::<Box<dyn Envelope<P>>>(builder.messages_buffer_size.into());
        let (command_tx, command_rx) =
            mpsc::channel::<ServicePacket>(builder.commands_buffer_size.into());
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(command_tx);
        self.register_puppet_by_pid::<P>(
            master_pid,
            postman.clone(),
            service_postman,
            status_tx,
            status_rx.clone(),
        )?;
        let Some(retry_config) = builder.retry_config.take() else {
            return Err(PuppetError::critical(
                puppet_pid,
                "PuppetBuilder has no RetryConfig",
            ));
        };

        let puppeter = Context {
            pid,
            pptr: self.clone(),
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
            pptr: self.clone(),
        };

        puppet.on_init(&puppeter).await?;
        puppeter.start(&mut puppet, false).await?;

        tokio::spawn(run_puppet_loop(puppet, puppeter, handle));
        Ok(address)
    }

    pub fn add_resource<T>(&self, resource: T) -> Result<(), ResourceAlreadyExist>
    where
        T: Send + Sync + Clone + 'static,
    {
        let id = Id::new::<T>();
        if let std::collections::hash_map::Entry::Vacant(e) = self
            .resources
            .lock()
            .expect("Failed to acquire mutex lock")
            .entry(id)
        {
            e.insert(Arc::new(Mutex::new(Box::new(resource))));
            Ok(())
        } else {
            Err(ResourceAlreadyExist)
        }
    }

    #[must_use]
    pub fn get_resource<T>(&self) -> Option<T>
    where
        T: Send + Sync + Clone + 'static,
    {
        let resource = {
            let id = Id::new::<T>();
            let resources_guard = self.resources.lock().expect("Failed to acquire mutex lock");
            Arc::clone(resources_guard.get(&id)?)
        };

        let boxed = resource
            .lock()
            .expect("Failed to acquire mutex lock on resource");
        let any_ref = boxed.downcast_ref::<T>()?;
        Some(any_ref.clone())
    }

    pub fn with_resource<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&T) -> R,
    {
        let resource = {
            let id = Id::new::<T>();
            let resources_guard = self.resources.lock().expect("Failed to acquire mutex lock");
            Arc::clone(resources_guard.get(&id)?)
        };

        let boxed = resource
            .lock()
            .expect("Failed to acquire mutex lock on resource");
        let any_ref = boxed.downcast_ref::<T>()?;
        Some(f(any_ref))
    }

    pub fn with_resource_mut<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&mut T) -> R,
    {
        let resource = {
            let id = Id::new::<T>();
            let resources_guard = self.resources.lock().expect("Failed to acquire mutex lock");
            Arc::clone(resources_guard.get(&id)?)
        };

        let mut boxed = resource
            .lock()
            .expect("Failed to acquire mutex lock on resource");
        let any_mut = boxed.downcast_mut::<T>()?;
        Some(f(any_mut))
    }

    #[must_use]
    pub fn expect_resource<T>(&self) -> T
    where
        T: Send + Sync + Clone + 'static,
    {
        self.get_resource::<T>().expect("Resource doesn't exist")
    }
}

pub(crate) async fn run_puppet_loop<P>(
    mut puppet: P,
    mut puppeter: Context,
    mut handle: PuppetHandle<P>,
) where
    P: Lifecycle,
{
    let mut puppet_status = handle.status_rx;

    loop {
        tokio::select! {
            res = puppet_status.changed() => {
                if let Ok(()) = res {
                    if matches!(*puppet_status.borrow(), LifecycleStatus::Inactive
                        | LifecycleStatus::Failed) {
                        tracing::info!(puppet = %puppeter.pid, "Stopping loop due to puppet status change");
                        break;
                    }
                } else {
                    tracing::debug!(puppet = %puppeter.pid, "Stopping loop due to closed puppet status channel");
                    break;
                }
            }
            res = handle.command_rx.recv() => {
                if let Some(mut service_packet) = res {
                    if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                        if let Err(err) = service_packet.handle_command(&mut puppet, &mut puppeter).await {
                            tracing::error!(puppet = %puppeter.pid, "Failed to handle command: {}", err);
                        }
                    } else {
                        tracing::debug!(puppet = %puppeter.pid, "Ignoring command due to non-Active puppet status");
                        let error_response = PuppetCannotHandleMessage::new(puppeter.pid, *puppet_status.borrow()).into();
                        service_packet.reply_error(error_response);
                    }
                } else {
                    tracing::debug!(puppet = %puppeter.pid, "Stopping loop due to closed command channel");
                    break;
                }
            }
            res = handle.message_rx.recv() => {
                if let Some(mut envelope) = res {
                    if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                        envelope.handle_message(&mut puppet, &mut puppeter).await;
                    } else {
                        let status = *puppet_status.borrow();
                        tracing::debug!(puppet = %puppeter.pid,  "Ignoring message due to non-Active puppet status");
                        envelope.reply_error(&puppeter, PuppetCannotHandleMessage::new(puppeter.pid, status).into()).await;
                    }
                } else {
                    tracing::debug!(puppet = %puppeter.pid,  "Stopping loop due to closed message channel");
                    break;
                }
            }
        }
    }
}

#[allow(unused_variables, clippy::unwrap_used)]
#[cfg(test)]
mod tests {

    use std::time::Duration;

    use async_trait::async_trait;

    use crate::{
        executor::SequentialExecutor, prelude::CriticalError, supervision::strategy::OneForAll,
    };

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct MasterActor {
        failures: usize,
    }

    #[async_trait]
    impl Lifecycle for MasterActor {
        type Supervision = OneForAll;

        async fn reset(&self, ctx: &Context) -> Result<Self, CriticalError> {
            println!("Resetting MasterActor");
            Err(CriticalError::new(ctx.pid, "Failed to reset MasterActor"))
        }
    }

    #[derive(Debug, Clone, Default)]
    struct PuppetActor {
        failures: usize,
    }

    #[async_trait]
    impl Lifecycle for PuppetActor {
        type Supervision = OneForAll;

        async fn reset(&self, ctx: &Context) -> Result<Self, CriticalError> {
            Ok(Self {
                failures: self.failures,
            })
        }
    }

    #[derive(Debug)]
    struct MasterMessage;

    #[derive(Debug)]
    struct PuppetMessage;

    #[derive(Debug)]
    struct MasterFailingMessage;

    #[derive(Debug)]
    struct PuppetFailingMessage;

    #[async_trait]
    impl Handler<MasterMessage> for MasterActor {
        type Response = ();
        type Executor = SequentialExecutor;
        async fn handle_message(
            &mut self,
            _msg: MasterMessage,
            _ctx: &Context,
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
            _msg: PuppetMessage,
            _ctx: &Context,
        ) -> Result<Self::Response, PuppetError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Handler<MasterFailingMessage> for MasterActor {
        type Response = ();
        type Executor = SequentialExecutor;
        async fn handle_message(
            &mut self,
            _msg: MasterFailingMessage,
            ctx: &Context,
        ) -> Result<Self::Response, PuppetError> {
            println!("Handling MasterFailingMessage. Failures: {}", self.failures);
            if self.failures < 3 {
                self.failures += 1;
                Err(PuppetError::critical(
                    ctx.pid,
                    "Failed to handle MasterFailingMessage",
                ))
            } else {
                Ok(())
            }
        }
    }

    #[async_trait]
    impl Handler<PuppetFailingMessage> for PuppetActor {
        type Response = ();
        type Executor = SequentialExecutor;
        async fn handle_message(
            &mut self,
            _msg: PuppetFailingMessage,
            ctx: &Context,
        ) -> Result<Self::Response, PuppetError> {
            println!("Handling MasterFailingMessage. Failures: {}", self.failures);
            if self.failures < 3 {
                self.failures += 1;
                Err(PuppetError::critical(
                    ctx.pid,
                    "Failed to handle PuppetFailingMessage",
                ))
            } else {
                Ok(())
            }
        }
    }

    pub fn register_puppet<M, P>(pptr: &Puppeter) -> Result<(), PuppetError>
    where
        M: Lifecycle,
        P: Lifecycle,
    {
        let (message_tx, _message_rx) = mpsc::channel::<Box<dyn Envelope<P>>>(1);
        let (service_tx, _service_rx) = mpsc::channel::<ServicePacket>(1);
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(service_tx);

        pptr.register_puppet::<M, P>(postman, service_postman, status_tx, status_rx)
    }

    #[tokio::test]
    async fn test_register() {
        let pptr = Puppeter::new();

        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);

        // Master puppet doesn't exist

        assert!(res.is_err());

        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);

        // Master is same as puppet

        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);

        // Puppet already exists

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_is_puppet_exists() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        assert!(pptr.is_puppet_exists::<PuppetActor>());
        assert!(!pptr.is_puppet_exists::<MasterActor>());
    }

    #[tokio::test]
    async fn test_get_postman() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        assert!(pptr.get_postman::<PuppetActor>().is_some());
        assert!(pptr.get_postman::<MasterActor>().is_none());
    }

    #[tokio::test]
    async fn test_get_service_postman_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr.get_service_postman_by_pid(puppet_pid).is_some());
        let master_pid = Pid::new::<MasterActor>();
        assert!(pptr.get_service_postman_by_pid(master_pid).is_none());
    }

    #[tokio::test]
    async fn test_get_status_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        assert_eq!(
            pptr.get_puppet_status_by_pid(puppet_pid),
            Some(LifecycleStatus::Inactive)
        );
        let master_pid = Pid::new::<MasterActor>();
        assert!(pptr.get_puppet_status_by_pid(master_pid).is_none());
    }

    #[tokio::test]
    async fn test_set_status_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        pptr.set_status_by_pid(puppet_pid, LifecycleStatus::Active);
        assert_eq!(
            pptr.get_puppet_status_by_pid(puppet_pid),
            Some(LifecycleStatus::Active)
        );
    }

    #[tokio::test]
    async fn test_subscribe_status_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let puppet_pid = Pid::new::<PuppetActor>();
        let rx = pptr.subscribe_puppet_status_by_pid(puppet_pid).unwrap();
        pptr.set_status_by_pid(puppet_pid, LifecycleStatus::Active);
        assert_eq!(*rx.borrow(), LifecycleStatus::Active);
    }

    #[tokio::test]
    async fn test_has_puppet_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr
            .puppet_has_puppet_by_pid(master_pid, puppet_pid)
            .is_some());
        let master_pid = Pid::new::<PuppetActor>();
        assert!(pptr
            .puppet_has_puppet_by_pid(master_pid, puppet_pid)
            .is_none());
    }

    #[tokio::test]
    async fn test_has_permission_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr
            .puppet_has_permission_by_pid(master_pid, puppet_pid)
            .is_some());
        let master_pid = Pid::new::<PuppetActor>();
        assert!(pptr
            .puppet_has_permission_by_pid(master_pid, puppet_pid)
            .is_none());
    }

    #[tokio::test]
    async fn test_get_master_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        assert!(res.is_ok());
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert_eq!(pptr.get_puppet_master_by_pid(puppet_pid), Some(master_pid));
        assert_eq!(pptr.get_puppet_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_set_master_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        assert!(res.is_ok());
        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);
        assert!(res.is_ok());
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr
            .set_puppet_master_by_pid(puppet_pid, master_pid, puppet_pid)
            .is_ok());
        assert_eq!(pptr.get_puppet_master_by_pid(puppet_pid), Some(master_pid));
        assert_eq!(pptr.get_puppet_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_get_puppets_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        res.unwrap();
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        res.unwrap();
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        let puppets = pptr.get_puppets_by_pid(master_pid).unwrap();
        assert_eq!(puppets.len(), 2);
        assert!(puppets.contains(&puppet_pid));
    }

    #[tokio::test]
    async fn test_detach_puppet_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        res.unwrap();
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        res.unwrap();
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr.detach_puppet_by_pid(master_pid, puppet_pid).is_ok());
        assert_eq!(pptr.get_puppet_master_by_pid(puppet_pid), Some(puppet_pid));
        assert_eq!(pptr.get_puppet_master_by_pid(master_pid), Some(master_pid));
    }

    #[tokio::test]
    async fn test_delete_puppet_by_pid() {
        let pptr = Puppeter::new();
        let res = register_puppet::<MasterActor, MasterActor>(&pptr);
        res.unwrap();
        let res = register_puppet::<MasterActor, PuppetActor>(&pptr);
        res.unwrap();
        let master_pid = Pid::new::<MasterActor>();
        let puppet_pid = Pid::new::<PuppetActor>();
        assert!(pptr.delete_puppet_by_pid(master_pid, puppet_pid).is_ok());
        assert!(pptr.get_postman::<PuppetActor>().is_none());
        assert!(pptr.get_postman::<MasterActor>().is_some());
    }

    #[tokio::test]
    async fn test_spawn() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        res.unwrap();
        let res = pptr
            .spawn::<MasterActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        res.unwrap_err();
    }

    #[tokio::test]
    async fn test_send() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        res.unwrap();

        let res = pptr.send::<PuppetActor, PuppetMessage>(PuppetMessage).await;
        res.unwrap();

        let res = pptr.send::<MasterActor, MasterMessage>(MasterMessage).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        res.unwrap();

        let res = pptr.ask::<PuppetActor, PuppetMessage>(PuppetMessage).await;
        res.unwrap();

        let res = pptr.ask::<MasterActor, MasterMessage>(MasterMessage).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask_with_timeout() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        assert!(res.is_ok());
        let res = pptr
            .ask_with_timeout::<PuppetActor, PuppetMessage>(PuppetMessage, Duration::from_secs(1))
            .await;
        assert!(res.is_ok());
        let res = pptr.send::<MasterActor, MasterMessage>(MasterMessage).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn self_mutate_puppet() {
        #[derive(Debug, Clone, Default)]
        pub struct CounterPuppet {
            counter: Vec<i32>,
        }

        #[async_trait]
        impl Lifecycle for CounterPuppet {
            type Supervision = OneForAll;
            async fn reset(&self, _puppeter: &Context) -> Result<Self, CriticalError> {
                Ok(CounterPuppet::default())
            }
        }

        #[derive(Debug)]
        pub struct IncrementCounter;

        #[async_trait]
        impl Handler<IncrementCounter> for CounterPuppet {
            type Response = ();
            type Executor = SequentialExecutor;
            async fn handle_message(
                &mut self,
                _msg: IncrementCounter,
                puppeter: &Context,
            ) -> Result<Self::Response, PuppetError> {
                println!("Counter: {}", self.counter.len());
                if self.counter.len() < 10 {
                    self.counter.push(1);
                    puppeter.send::<Self, _>(IncrementCounter).await?;
                } else {
                    puppeter.send::<Self, _>(DebugCounterPuppet).await?;
                }
                Ok(())
            }
        }

        #[derive(Debug)]
        pub struct DebugCounterPuppet;

        #[async_trait]
        impl Handler<DebugCounterPuppet> for CounterPuppet {
            type Response = ();
            type Executor = SequentialExecutor;
            async fn handle_message(
                &mut self,
                _msg: DebugCounterPuppet,
                _puppeter: &Context,
            ) -> Result<Self::Response, PuppetError> {
                println!("Counter: {:?}", self.counter);
                Ok(())
            }
        }

        let pptr = Puppeter::new();
        let address = pptr
            .spawn::<CounterPuppet, CounterPuppet>(PuppetBuilder::new(CounterPuppet::default()))
            .await
            .unwrap();
        address.send(IncrementCounter).await.unwrap();
        // wait 1 second for the puppet to finish
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_successful_recovery_after_failure() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn::<PuppetActor, PuppetActor>(PuppetBuilder::new(PuppetActor::default()))
            .await;
        res.unwrap();

        loop {
            let res = pptr.ask::<PuppetActor, _>(PuppetFailingMessage).await;
            if res.is_ok() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    #[tokio::test]
    async fn test_failed_recovery_after_failure() {
        let pptr = Puppeter::new();
        let res = pptr
            .spawn_self::<MasterActor>(PuppetBuilder::new(MasterActor::default()))
            .await;
        res.unwrap();

        loop {
            let res = pptr.ask::<MasterActor, _>(MasterFailingMessage).await;
            if res.is_err() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_add_resouce() {
        let pptr = Puppeter::new();
        pptr.add_resource::<i32>(10).unwrap();
        pptr.with_resource::<i32, _, _>(|res| {
            assert_eq!(*res, 10);
        });
    }
}
