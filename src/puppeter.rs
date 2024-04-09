use atomic_take::AtomicTake;
use indexmap::IndexSet;
use rustc_hash::{FxHashMap, FxHasher};
use std::{
    any::Any,
    future::Future,
    hash::BuildHasherDefault,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

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
};

pub type BoxedAny = Box<dyn Any + Send + Sync>;

/// Type alias for a tuple containing a `watch::Sender<LifecycleStatus>` and `watch::Receiver<LifecycleStatus>`.
///
/// `StatusChannels` is used for monitoring the lifecycle statuses of all actors in a system.
/// The `watch::Sender<LifecycleStatus>` is used for broadcasting status updates, while the receiver end can be subscribed
/// to in order to observe these status changes.
pub type StatusChannels = (
    watch::Sender<LifecycleStatus>,
    watch::Receiver<LifecycleStatus>,
);

/// `FxIndexSet` is an alias for `IndexSet` from the `indexmap` crate, bundled with `FxHasher`.
type FxIndexSet<T> = IndexSet<T, BuildHasherDefault<FxHasher>>;

/// The `Puppeter` struct is the central entity responsible for managing actors within the system.
///
/// It handles the lifecycle, communication, and resource management of actors, as well as dealing
/// with critical errors that may occur during system operation.
///
/// The struct maintains mappings between actor process IDs (`Pid`) and various components such as
/// `Postman` instances for message passing, `ServicePostman` instances for handling service requests,
/// and `StatusChannels` for monitoring actor status. It also keeps track of the relationships between
/// master actors and their child actors (puppets) using the `master_to_puppets` and `puppet_to_master`
/// fields.
///
/// Additionally, the `Puppeter` manages shared resources accessible by the actors, utilizing a
/// dedicated `tokio` executor for running actor tasks. Critical errors that cannot be handled
/// internally are reported through the `failure_tx` and `failure_rx` fields, which are used for
/// sending and receiving `CriticalError` instances, respectively.
///
/// # Fields
///
/// * `message_postmans`: A mapping between a process ID (`Pid`) and its corresponding `Postman`
///   instance, used for message passing between actors.
/// * `service_postmans`: A mapping between a `Pid` and its associated `ServicePostman` instance,
///   responsible for handling service requests.
/// * `statuses`: A mapping between a `Pid` and its `StatusChannels`, used for monitoring the status
///   of actors.
/// * `master_to_puppets`: A mapping between a master actor's `Pid` and the `Pid`s of its child actors
///   (puppets).
/// * `puppet_to_master`: A mapping from a child actor's (puppet's) `Pid` to its master actor's `Pid`.
/// * `resources`: A collection of shared resources accessible by the actors, stored as a mapping
///   between a resource ID (`Id`) and an `Arc<Mutex<BoxedAny>>`.
/// * `executor`: A dedicated `tokio` executor used for running actor tasks.
/// * `failure_tx`: An unbounded sender for reporting critical errors that cannot be handled
///   internally.
/// * `failure_rx`: A receiver for critical errors reported by the system, wrapped in an `Arc` and
///   `AtomicTake` for concurrent access.
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
    /// Constructs a new instance of Puppeter, which acts as a manager for various actors within
    /// the framework.
    ///
    /// This function initializes the internal communication channels, dedicated executor, and
    /// other necessary components based on the system's available resources (e.g., CPU cores).
    ///
    /// # Examples
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// let pptr = Puppeter::new();
    /// // Now you can use `puppeter` to manage actors.
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if it fails to determine the number of available CPU cores on the system.
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

    /// Sets a callback function to be called in the event of an unrecoverable error.
    ///
    /// This method allows setting a user-defined function that will be executed if a critical
    /// error occurs that cannot be internally handled by Puppet or any Master in the actor
    /// hierarchy. This function serves as a last resort and is typically used to perform cleanup
    /// or logging before shutting down the actor or system in a controlled manner.
    ///
    /// # Example Usage
    ///
    /// ```rust
    /// # use std::pin::Pin;
    /// # use pptr::puppeter::Puppeter;
    ///
    /// # let pptr = Puppeter::new();
    /// pptr.on_unrecoverable_failure(|pptr, error| {
    ///     Box::pin(async move {
    ///         println!("Unrecoverable error encountered: {:?}", error);
    ///         // Application cleanup and termination logic here
    ///     })
    /// });
    /// ```
    ///
    /// # Panics
    ///
    /// This method does not panic by itself, but the user-supplied callback function can panic if
    /// not properly handled.
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

    /// Awaits an unrecoverable error that cannot be handled internally or by any actor in the
    /// hierarchy.
    ///
    /// This function listens for a critical error that is deemed unrecoverable, meaning it cannot
    /// be gracefully handled by the current puppet or any supervising puppet in its hierarchy.
    /// This is typically used to perform cleanup or logging before shutting down the actor or
    /// system in a controlled manner.
    ///
    /// # Example Usage
    ///
    /// ```rust
    /// # use std::pin::Pin;
    /// # use pptr::puppeter::Puppeter;
    /// # use tokio::time::{timeout, Duration};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let pptr = Puppeter::new();
    /// let result = timeout(Duration::from_millis(100), pptr.wait_for_unrecoverable_failure()).await;
    ///
    /// match result {
    ///     Ok(err) => println!("Unrecoverable error encountered: {:?}", err),
    ///     Err(_) => println!("No error encountered within 100 milliseconds"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the internal mechanism for receiving critical errors is not initialized or if
    /// receiving fails.
    pub async fn wait_for_unrecoverable_failure(self) -> CriticalError {
        self.failure_rx.take().unwrap().recv().await.unwrap()
    }

    /// Registers a puppet within the system with all necessary data.
    ///
    /// This function is intended for internal use within the framework to associate a puppet with
    /// its communication and lifecycle channels. It ensures that the puppet is ready to receive
    /// and process messages, respond to service packets, and report its lifecycle status.
    ///
    /// # Panics
    ///
    /// This function can panic if it fails to acquire a lock on the internal mutex.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the puppet cannot be registered due to a conflict with an
    /// existing registration.
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

    /// Checks if a puppet of the specific type is registered and exists.
    ///
    /// This function inspects the internal registry to determine if a puppet,
    /// identified by its PID (`puppet`), is present. It is a convenience method
    /// for quickly verifying the existence of a puppet without needing to retrieve
    /// its entire data structure.
    ///
    /// # Panics
    ///
    /// This function can panic if it fails to acquire a lock on the internal mutex.
    #[must_use]
    pub fn is_puppet_exists<P>(&self) -> bool
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.is_puppet_exists_by_pid(puppet)
    }

    /// Intenal function to check if a puppet exists by its PID.
    pub(crate) fn is_puppet_exists_by_pid(&self, puppet: Pid) -> bool {
        self.get_puppet_master_by_pid(puppet).is_some()
    }

    /// Retrieves a `Postman` instance for the specified Puppet type `P`.
    ///
    /// # Panics
    ///
    /// This method panics if it fails to acquire the mutex lock on the internal
    /// storage of `Postman` instances. This is a critical error indicating that
    /// the locking mechanism preventing data races is compromised.
    #[must_use]
    pub(crate) fn get_postman<P>(&self) -> Option<Postman<P>>
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

    /// Retrieves the `ServicePostman` instance associated with the given puppet `Pid`.
    ///
    /// This function looks up the `ServicePostman` instance in the internal storage using the
    /// provided `puppet` identifier. If a matching instance is found, it is cloned and returned
    /// wrapped in an `Option`. If no instance is found for the given `Pid`, `None` is returned.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal storage of
    /// `ServicePostman` instances, indicating a critical error in the locking mechanism.
    pub(crate) fn get_service_postman_by_pid(&self, puppet: Pid) -> Option<ServicePostman> {
        self.service_postmans
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .cloned()
    }

    /// Updates the status of the puppet identified by `puppet` to the given `status`.
    ///
    /// This function retrieves the status entry associated with the `puppet` identifier
    /// from the internal status storage. If an entry is found, it sends the new `status`
    /// value to the associated channel, triggering a status update notification.
    ///
    /// The status is only updated if the new `status` differs from the current value.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal
    /// status storage, indicating a critical error in the locking mechanism.
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

    /// Subscribes to the status updates of the puppet associated with the type `P`.
    ///
    /// Returns a `watch::Receiver` that can be used to receive status updates for the puppet.
    /// If no status entry is found for the given puppet type, `None` is returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::prelude::*;
    /// # #[derive(Debug, Clone, Default)]
    /// # struct Puppet;
    /// # impl Lifecycle for Puppet {
    /// #     type Supervision = OneForAll;
    /// # }
    /// let pptr = Puppeter::new();
    /// if let Some(receiver) = pptr.subscribe_puppet_status::<Puppet>() {
    ///     // Use the receiver to get status updates for Puppet
    ///     let status = receiver.borrow();
    ///     println!("Current status: {:?}", status);
    /// }
    /// ```
    #[must_use]
    pub fn subscribe_puppet_status<P>(&self) -> Option<watch::Receiver<LifecycleStatus>>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.subscribe_puppet_status_by_pid(puppet)
    }

    /// Subscribes to the status updates of the puppet identified by `puppet`.
    ///
    /// Returns a cloned `watch::Receiver` that can be used to receive status updates for the puppet.
    /// If no status entry is found for the given `puppet` identifier, `None` is returned.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal status storage,
    /// indicating a critical error in the locking mechanism.
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

    /// Retrieves the current status of the puppet associated with the type `P`.
    ///
    /// Returns the current `LifecycleStatus` of the puppet wrapped in an `Option`.
    /// If no status entry is found for the given puppet type, `None` is returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use pptr::prelude::*;
    /// # #[derive(Debug, Clone, Default)]
    /// # struct Puppet;
    /// # impl Lifecycle for Puppet {
    /// #     type Supervision = OneForAll;
    /// # }
    /// # let pptr = Puppeter::new();
    /// if let Some(status) = pptr.get_puppet_status::<Puppet>() {
    ///     println!("Current status: {:?}", status);
    /// }
    /// ```
    #[must_use]
    pub fn get_puppet_status<P>(&self) -> Option<LifecycleStatus>
    where
        P: Lifecycle,
    {
        let puppet = Pid::new::<P>();
        self.get_puppet_status_by_pid(puppet)
    }

    /// Retrieves the current status of the puppet identified by `puppet`.
    ///
    /// Returns the current `LifecycleStatus` of the puppet wrapped in an `Option`.
    /// If no status entry is found for the given `puppet` identifier, `None` is returned.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal status storage,
    /// indicating a critical error in the locking mechanism.
    pub(crate) fn get_puppet_status_by_pid(&self, puppet: Pid) -> Option<LifecycleStatus> {
        self.statuses
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .map(|(_, rx)| *rx.borrow())
    }

    /// Checks if the puppet identified by `puppet` is associated with the master identified by `master`.
    ///
    /// Returns `Some(true)` if the `puppet` is associated with the `master`, `Some(false)` if not,
    /// and `None` if no entry is found for the given `master` identifier.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal `master_to_puppets`
    /// storage, indicating a critical error in the locking mechanism.
    pub(crate) fn puppet_has_puppet_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.master_to_puppets
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&master)
            .map(|puppets| puppets.contains(&puppet))
    }

    /// Checks if the puppet identified by `puppet` has permission from the master identified by `master`.
    ///
    /// Returns `Some(true)` if the `puppet` has permission from the `master`, `Some(false)` if not,
    /// and `None` if no entry is found for the given `master` identifier.
    ///
    /// # Panics
    ///
    /// This function is a wrapper around `puppet_has_puppet_by_pid`, so the same panic conditions apply.
    pub(crate) fn puppet_has_permission_by_pid(&self, master: Pid, puppet: Pid) -> Option<bool> {
        self.puppet_has_puppet_by_pid(master, puppet)
    }

    /// Retrieves the master identifier associated with the given `puppet` identifier.
    ///
    /// Returns `Some(Pid)` if a master is found for the `puppet`, or `None` if no association exists.
    ///
    /// # Panics
    ///
    /// This function may panic if it fails to acquire the mutex lock on the internal `puppet_to_master`
    /// storage, indicating a critical error in the locking mechanism.
    pub(crate) fn get_puppet_master_by_pid(&self, puppet: Pid) -> Option<Pid> {
        self.puppet_to_master
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&puppet)
            .copied()
    }

    /// Sets the master for a given puppet.
    ///
    /// This method allows changing the master of a puppet, transferring control
    /// from the old master to a new master. The old master must have permission
    /// over the puppet, otherwise an error is returned.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub fn set_puppet_master<P, O, M>(&self) -> Result<(), PuppetOperationError>
    where
        P: Lifecycle,
        O: Lifecycle,
        M: Lifecycle,
    {
        let old_master = Pid::new::<O>();
        let new_master = Pid::new::<M>();
        let puppet = Pid::new::<P>();
        self.set_puppet_master_by_pid(old_master, new_master, puppet)
    }

    /// Sets the master for a puppet using their Pids.
    ///
    /// This is the internal implementation of `set_puppet_master` that operates
    /// directly on Pids. It transfers a puppet from its old master to a new master,
    /// checking permissions and updating the internal mappings.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
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

    /// Retrieves the set of puppets controlled by a given master.
    ///
    /// This method returns an optional `FxIndexSet` containing the `Pid`s of all puppets
    /// currently under the control of the specified master. If the master has no puppets,
    /// or if the master does not exist, `None` is returned.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub(crate) fn get_puppets_by_pid(&self, master: Pid) -> Option<FxIndexSet<Pid>> {
        self.master_to_puppets
            .lock()
            .expect("Failed to acquire mutex lock")
            .get(&master)
            .cloned()
    }

    /// Detaches a puppet from its current master, making it its own master.
    ///
    /// This method allows a master to relinquish control over a puppet, effectively
    /// making the puppet independent. The master must have permission over the puppet,
    /// otherwise an error is returned.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetOperationError` if:
    /// - The specified puppet does not exist.
    /// - The master does not have permission to detach the puppet.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
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

    /// Deletes a puppet, removing it from the control of its master.
    ///
    /// This method allows a master to completely remove a puppet from the system,
    /// deleting all associated data. The master must have permission over the puppet,
    /// otherwise an error is returned.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetOperationError` if:
    /// - The specified puppet does not exist.
    /// - The master does not have permission to delete the puppet.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub fn delete_puppet<O, P>(&self) -> Result<(), PuppetOperationError>
    where
        O: Lifecycle,
        P: Lifecycle,
    {
        let master = Pid::new::<O>();
        let puppet = Pid::new::<P>();
        self.delete_puppet_by_pid(master, puppet)
    }

    /// Deletes a puppet by its `Pid`, removing it from the control of its master.
    ///
    /// This is the internal implementation of `delete_puppet` that operates directly
    /// on `Pid`s. It removes a puppet from the system, deleting all associated data.
    /// The master must have permission over the puppet, otherwise an error is returned.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetOperationError` if:
    /// - The specified puppet does not exist.
    /// - The master does not have permission to delete the puppet.
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
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

    /// Sends a message of type `E` to the puppet handler of type `P`.
    ///
    /// This method sends a message to the puppet's message handler of the specified type.
    /// It returns a `Result` indicating the success or failure of the send operation.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if:
    /// - The puppet does not exist.
    /// - The message fails to send.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let result = pptr.send::<Puppet, _>(Message::new()).await;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub async fn send<P, E>(&self, message: E) -> Result<(), PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        if let Some(postman) = self.get_postman::<P>() {
            Ok(postman.send(message).await?)
        } else {
            Err(PuppetDoesNotExistError::new(Pid::new::<P>()).into())
        }
    }

    /// Sends a message of type `E` to the puppet handler of type `P` and awaits a response.
    ///
    /// This method sends a message to the puppet's message handler of the specified type
    /// and awaits a response. It returns a `Result` containing the response or an error.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if:
    /// - The puppet does not exist.
    /// - The message fails to send or receive a response.
    /// - Handler return error.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let response = pptr.ask::<Puppet, _>(MyMessage::new()).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub async fn ask<P, E>(&self, message: E) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        if let Some(postman) = self.get_postman::<P>() {
            Ok(postman.send_and_await_response::<E>(message, None).await?)
        } else {
            Err(PuppetDoesNotExistError::new(Pid::new::<P>()).into())
        }
    }

    /// Sends a message of type `E` to the puppet handler of type `P` with a timeout and awaits a response.
    ///
    /// This method sends a message to the puppet's message handler of the specified type
    /// and awaits a response, with a specified timeout duration. It returns a `Result`
    /// containing the response or an error.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendMessageError` if:
    /// - The puppet does not exist.
    /// - The message fails to send or receive a response within the timeout duration.
    /// - Handler return error.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let response = pptr.ask_with_timeout::<Puppet, _>(MyMessage::new(), Duration::from_secs(5)).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub(crate) async fn ask_with_timeout<P, E>(
        &self,
        message: E,
        duration: std::time::Duration,
    ) -> Result<ResponseFor<P, E>, PuppetSendMessageError>
    where
        P: Handler<E>,
        E: Message,
    {
        if let Some(postman) = self.get_postman::<P>() {
            Ok(postman
                .send_and_await_response::<E>(message, Some(duration))
                .await?)
        } else {
            Err(PuppetDoesNotExistError::new(Pid::new::<P>()).into())
        }
    }

    /// Sends a message of type `E` to the puppet handler of type `P` without awaiting a response.
    ///
    /// This method sends a message to the puppet's message handler of the specified type
    /// asynchronously, without awaiting a response. It spawns a new async task to handle
    /// the send operation.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// pptr.cast::<Puppet, _>(MyMessage::new());
    /// ```
    pub fn cast<P, E>(&self, message: E)
    where
        P: Handler<E>,
        E: Message,
    {
        let cloned_self = self.clone();
        tokio::spawn(async move {
            _ = cloned_self.send::<P, E>(message).await;
        });
    }

    /// Sends a command to a puppet by its `Pid`, with the master's permission.
    ///
    /// This method sends a `ServiceCommand` to a puppet identified by its `Pid`, ensuring
    /// that the master has permission over the puppet. It returns a `Result` indicating
    /// the success or failure of the send operation.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetSendCommandError` if:
    /// - The specified puppet does not exist.
    /// - The master does not have permission to send commands to the puppet.
    /// - The command fails to send or receive a response.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let result = pptr.send_command_by_pid(master_pid, puppet_pid, ServiceCommand::Start).await;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
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

    /// Spawns a new puppet and links it to the specified master.
    ///
    /// This method creates a new puppet using the provided `PuppetBuilder` and links it to
    /// the specified master `Pid`. It returns an `Address<P>` for the newly spawned puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if:
    /// - The specified master does not exist and is not the same as the puppet's `Pid`.
    /// - The puppet fails to spawn or initialize.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let address = pptr.spawn::<Master, Puppet>(builder).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub async fn spawn<P, M>(&self, puppet: P) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        self.puppet_builder::<P>(puppet)
            .spawn_link_by_pid(Pid::new::<M>())
            .await
    }

    /// Spawns a new independent puppet and links it to itself.
    ///
    /// This method creates a new puppet using the provided `PuppetBuilder` and links it to
    /// itself, making it an independent puppet. It returns an `Address<P>` for the newly
    /// spawned puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if:
    /// - The puppet fails to spawn or initialize.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let address = pptr.spawn_self::<Puppet>(builder).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    #[allow(clippy::impl_trait_in_params)]
    pub async fn spawn_self<P>(&self, puppet: P) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        self.spawn::<P, P>(puppet).await
    }

    /// Spawns a new puppet and links it to the specified master.
    ///
    /// This method creates a new puppet using the provided `PuppetBuilder` and links it to
    /// the specified master `Pid`. It returns an `Address<P>` for the newly spawned puppet.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if:
    /// - The specified master does not exist and is not the same as the puppet's `Pid`.
    /// - The puppet fails to spawn or initialize.
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// let address = pptr.spawn::<Master, Puppet>(builder).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the mutex lock is poisoned, indicating a failure in lock acquisition.
    pub(crate) async fn spawn_puppet_by_pid<P>(
        &self,
        mut builder: PuppetBuilder<P>,
        master_pid: Pid,
    ) -> Result<Address<P>, PuppetError>
    where
        P: Lifecycle,
    {
        let puppet_pid = Pid::new::<P>();
        if !self.is_puppet_exists_by_pid(master_pid) && master_pid != puppet_pid {
            return Err(PuppetDoesNotExistError::new(master_pid).into());
        }

        let mut puppet = builder.puppet.take().unwrap();
        let pid = Pid::new::<P>();
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let (message_tx, message_rx) =
            mpsc::channel::<Box<dyn Envelope<P>>>(builder.messages_buffer_size.get());
        let (command_tx, command_rx) =
            mpsc::channel::<ServicePacket>(builder.commands_buffer_size.get());
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(command_tx);
        self.register_puppet_by_pid::<P>(
            master_pid,
            postman.clone(),
            service_postman,
            status_tx,
            status_rx.clone(),
        )?;
        let retry_config = builder.retry_config.take().unwrap_or_default();

        let puppeter = Context {
            pid,
            pptr: self.clone(),
            retry_config,
        };

        let handle = PuppetHandle {
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

    /// Creates a new `PuppetBuilder` for the specified puppet type `P`.
    ///
    /// This method returns a `PuppetBuilder` instance associated with the current `Puppeter`.
    /// The generic parameter `P` specifies the type of the puppet and must implement the `Lifecycle`
    /// trait.
    ///
    /// # Returns
    ///
    /// A new `PuppetBuilder` instance for the specified puppet type `P`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pptr::prelude::*;
    ///
    /// #[derive(Debug, Default, Clone)]
    /// struct Puppet;
    ///
    /// impl Lifecycle for Puppet {
    ///     type Supervision = OneForAll;
    /// }
    ///
    /// let pptr = Puppeter::new();
    /// let builder = pptr.puppet_builder(Puppet::default());
    #[must_use]
    pub fn puppet_builder<P>(&self, puppet: P) -> PuppetBuilder<P>
    where
        P: Lifecycle,
    {
        PuppetBuilder::new(puppet, self.clone())
    }

    /// Adds a new resource to the resource collection.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime. If a
    /// resource with the same type already exists, an error will be returned.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// pptr.add_resource::<i32>(10).unwrap();
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex lock fails.
    ///
    /// # Errors
    ///
    /// Returns a `ResourceAlreadyExist` error if a resource of the same type already exists in the
    /// collection.
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
            Err(ResourceAlreadyExist { id })
        }
    }

    /// Returns a cloned copy of the resource of type `T`, if it exists.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let value = pptr.get_resource::<i32>().unwrap();
    /// assert_eq!(value, 10);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex lock fails.
    ///
    /// # Returns
    ///
    /// `Some(T)` if the resource exists, otherwise `None`.
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

    /// Borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let result = pptr.with_resource::<i32, _, _>(|value| {
    ///     assert_eq!(*value, 10);
    ///     "success"
    /// }).unwrap();
    /// assert_eq!(result, "success");
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex lock fails.
    ///
    /// # Returns
    ///
    /// `Some(R)` if the resource exists, where `R` is the return type of the closure `f`.
    /// Returns `None` if the resource doesn't exist.
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

    /// Borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let result = pptr.with_expected_resource::<i32, _, _>(|value| {
    ///     assert_eq!(*value, 10);
    ///     "success"
    /// });
    /// assert_eq!(result, "success");
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - The internal mutex lock fails.
    /// - The resource of type `T` doesn't exist.
    /// - Downcasting the resource to type `T` fails.
    ///
    /// # Returns
    ///
    /// The return value `R` of the closure `f`.
    pub fn with_expected_resource<T, F, R>(&self, f: F) -> R
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&T) -> R,
    {
        let resource = {
            let id = Id::new::<T>();
            let resources_guard = self.resources.lock().expect("Failed to acquire mutex lock");
            Arc::clone(resources_guard.get(&id).expect("Resource doesn't exist"))
        };

        let boxed = resource
            .lock()
            .expect("Failed to acquire mutex lock on resource");
        let any_ref = boxed
            .downcast_ref::<T>()
            .expect("Failed to downcast resource");
        f(any_ref)
    }

    /// Mutably borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let result = pptr.with_resource_mut::<i32, _, _>(|value| {
    ///     *value += 1;
    ///     "success"
    /// }).unwrap();
    /// assert_eq!(result, "success");
    /// assert_eq!(pptr.expect_resource::<i32>(), 11);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex lock fails.
    ///
    /// # Returns
    ///
    /// `Some(R)` if the resource exists, where `R` is the return type of the closure `f`.
    /// Returns `None` if the resource doesn't exist.
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
    /// Mutably borrows the resource of type `T` and passes it to the provided closure `f`.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let result = pptr.with_expected_resource_mut::<i32, _, _>(|value| {
    ///     *value += 1;
    ///     "success"
    /// }).unwrap();
    /// assert_eq!(result, "success");
    /// assert_eq!(pptr.expect_resource::<i32>(), 11);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - The internal mutex lock fails.
    /// - The resource of type `T` doesn't exist.
    /// - Downcasting the resource to type `T` fails.
    ///
    /// # Returns
    ///
    /// `Some(R)`, where `R` is the return type of the closure `f`.
    pub fn with_expected_resource_mut<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + Clone + 'static,
        F: FnOnce(&mut T) -> R,
    {
        let resource = {
            let id = Id::new::<T>();
            let resources_guard = self.resources.lock().expect("Failed to acquire mutex lock");
            Arc::clone(resources_guard.get(&id).expect("Resource doesn't exist"))
        };

        let mut boxed = resource
            .lock()
            .expect("Failed to acquire mutex lock on resource");
        let any_mut = boxed
            .downcast_mut::<T>()
            .expect("Failed to downcast resource");
        Some(f(any_mut))
    }

    /// Returns a cloned copy of the resource of type `T`, or panics if it doesn't exist.
    ///
    /// The resource must implement `Send`, `Sync`, `Clone`, and have a `'static` lifetime.
    ///
    /// # Example Usage
    ///
    /// ```
    /// # use pptr::puppeter::Puppeter;
    /// # let pptr = Puppeter::new();
    /// # pptr.add_resource::<i32>(10).unwrap();
    /// let value = pptr.expect_resource::<i32>();
    /// assert_eq!(value, 10);
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex lock fails or if the resource doesn't exist.
    #[must_use]
    pub fn expect_resource<T>(&self) -> T
    where
        T: Send + Sync + Clone + 'static,
    {
        self.get_resource::<T>().expect("Resource doesn't exist")
    }

    pub fn task<F, Fut, O>(&self, f: F) -> JoinHandle<O>
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let cloned_self = self.clone();
        tokio::spawn(f(cloned_self))
    }
}

/// Runs the main event loop for a puppet, handling lifecycle status changes, commands, and messages.
///
/// The loop continues running until one of the following conditions is met:
/// - The puppet's status changes to `Inactive` or `Failed`.
/// - The `puppet_status` channel is closed.
/// - The `command_rx` channel is closed.
/// - The `message_rx` channel is closed.
///
/// If the puppet's status is `Active`, incoming commands and messages are handled by the puppet.
/// Otherwise, commands are ignored and an error response is sent, while messages are met with an
/// error reply indicating the puppet's status.
///
/// # Panics
///
/// This function does not explicitly panic, but may propagate panics from the `handle_command` or
/// `handle_message` methods of the `Puppet` and `Puppeter` structs.
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
            Ok(()) = puppet_status.changed() => {
                if matches!(*puppet_status.borrow(), LifecycleStatus::Inactive
                    | LifecycleStatus::Failed) {
                    tracing::info!(puppet = %puppeter.pid, "Stopping loop due to puppet status change");
                    break;
                }
            }
            Some(mut service_packet) = handle.command_rx.recv() => {
                if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                    if let Err(err) = service_packet.handle_command(&mut puppet, &mut puppeter).await {
                        tracing::error!(puppet = %puppeter.pid, "Failed to handle command: {}", err);
                    }
                } else {
                    tracing::debug!(puppet = %puppeter.pid, "Ignoring command due to non-Active puppet status");
                    let status = *puppet_status.borrow();
                    let error_response = PuppetCannotHandleMessage::new(puppeter.pid, status).into();
                    service_packet.reply_error(error_response);
                }
            }
            Some(mut envelope) = handle.message_rx.recv() => {
                let status = *puppet_status.borrow();
                if matches!(status, LifecycleStatus::Active) {
                    envelope.handle_message(&mut puppet, &mut puppeter).await;
                } else {
                    tracing::debug!(puppet = %puppeter.pid,  "Ignoring message due to non-Active puppet status");
                    envelope.reply_error(&puppeter, PuppetCannotHandleMessage::new(puppeter.pid, status).into()).await;
                }
            }
            else => {
                tracing::debug!(puppet = %puppeter.pid, "Stopping loop due to closed channels");
                break;
            }
        }
    }
}

#[allow(unused_variables, clippy::unwrap_used)]
#[cfg(test)]
mod tests {

    use std::time::Duration;

    use async_trait::async_trait;

    use crate::{executor::SequentialExecutor, supervision::strategy::OneForAll};

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

    pub fn register_puppet<P, M>(pptr: &Puppeter) -> Result<(), PuppetError>
    where
        P: Lifecycle,
        M: Lifecycle,
    {
        let (message_tx, _message_rx) = mpsc::channel::<Box<dyn Envelope<P>>>(1);
        let (service_tx, _service_rx) = mpsc::channel::<ServicePacket>(1);
        let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
        let postman = Postman::new(message_tx);
        let service_postman = ServicePostman::new(service_tx);
        let master_pid = Pid::new::<M>();

        pptr.register_puppet_by_pid(master_pid, postman, service_postman, status_tx, status_rx)
    }

    #[tokio::test]
    async fn test_register() {
        let pptr = Puppeter::new();

        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);

        // Master puppet doesn't exist

        assert!(res.is_err());

        let res = register_puppet::<PuppetActor, PuppetActor>(&pptr);

        // Master is same as puppet

        assert!(res.is_ok());
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);

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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = register_puppet::<PuppetActor, MasterActor>(&pptr);
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
        let res = pptr.spawn::<_, PuppetActor>(PuppetActor::default()).await;
        res.unwrap();
        let res = pptr.spawn::<_, MasterActor>(PuppetActor::default()).await;
        res.unwrap_err();
    }

    #[tokio::test]
    async fn test_send() {
        let pptr = Puppeter::new();

        let res = pptr.spawn::<_, PuppetActor>(PuppetActor::default()).await;
        res.unwrap();

        let res = pptr.send::<PuppetActor, PuppetMessage>(PuppetMessage).await;
        res.unwrap();

        // Send without creating the puppet
        let res = pptr.send::<MasterActor, MasterMessage>(MasterMessage).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask() {
        let pptr = Puppeter::new();

        let res = pptr.spawn_self(PuppetActor::default()).await;
        res.unwrap();

        let res = pptr.ask::<PuppetActor, _>(PuppetMessage).await;
        res.unwrap();

        // Send without creating the puppet
        let res = pptr.ask::<MasterActor, _>(MasterMessage).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ask_with_timeout() {
        let pptr = Puppeter::new();

        let res = pptr.spawn_self(PuppetActor::default()).await;
        assert!(res.is_ok());

        let res = pptr
            .ask_with_timeout::<PuppetActor, _>(PuppetMessage, Duration::from_secs(1))
            .await;
        assert!(res.is_ok());

        let res = pptr
            .ask_with_timeout::<MasterActor, _>(MasterMessage, Duration::from_secs(1))
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn self_mutate_puppet() {
        #[derive(Debug, Clone, Default)]
        pub struct CounterPuppet {
            counter: i32,
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
            type Response = i32;
            type Executor = SequentialExecutor;
            async fn handle_message(
                &mut self,
                _msg: IncrementCounter,
                puppeter: &Context,
            ) -> Result<Self::Response, PuppetError> {
                println!("Counter: {}", self.counter);
                if self.counter < 10 {
                    self.counter += 1;
                    puppeter.send::<Self, _>(IncrementCounter).await?;
                } else {
                    puppeter.send::<Self, _>(DebugCounterPuppet).await?;
                }
                Ok(self.counter)
            }
        }

        #[derive(Debug)]
        pub struct DebugCounterPuppet;

        #[async_trait]
        impl Handler<DebugCounterPuppet> for CounterPuppet {
            type Response = i32;
            type Executor = SequentialExecutor;
            async fn handle_message(
                &mut self,
                _msg: DebugCounterPuppet,
                _puppeter: &Context,
            ) -> Result<Self::Response, PuppetError> {
                Ok(self.counter)
            }
        }

        let pptr = Puppeter::new();
        let address = pptr.spawn_self(CounterPuppet::default()).await.unwrap();
        address.send(IncrementCounter).await.unwrap();
        // wait 1 second for the puppet to finish
        tokio::time::sleep(Duration::from_secs(1)).await;
        let x = address.ask(DebugCounterPuppet).await.unwrap();
        assert_eq!(x, 10);
    }

    #[tokio::test]
    #[allow(unused_assignments)]
    async fn test_successful_recovery_after_failure() {
        let pptr = Puppeter::new();
        let res = pptr.spawn_self(PuppetActor::default()).await;
        res.unwrap();
        let mut success = false;

        loop {
            let res = pptr.ask::<PuppetActor, _>(PuppetFailingMessage).await;
            if res.is_ok() {
                success = true;
                break;
            }
        }
        assert!(success);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    #[tokio::test]
    #[allow(unused_assignments)]
    async fn test_failed_recovery_after_failure() {
        let pptr = Puppeter::new();
        let res = pptr.spawn_self(MasterActor::default()).await;
        res.unwrap();
        let mut success = false;

        loop {
            let res = pptr.ask::<MasterActor, _>(MasterFailingMessage).await;
            if res.is_err() {
                success = true;
                break;
            }
        }
        assert!(success);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
