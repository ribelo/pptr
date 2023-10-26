#![allow(dead_code)]
use std::{
    any::{type_name, TypeId},
    collections::VecDeque,
    sync::{Arc, OnceLock, RwLock},
};

use hashbrown::HashMap;
use indexmap::IndexSet;
use tokio::sync::{mpsc, watch};

use crate::{
    address::PuppetAddress,
    errors::{
        KillPuppetError, PermissionDenied, PuppetAlreadyExist, PuppetCannotHandleMessage,
        PuppetDoesNotExist, PuppetError, PuppeterSendCommandError, PuppeterSendMessageError,
        PuppeterSpawnError, ResetPuppetError, StartPuppetError, StopPuppetError,
    },
    id::{Id, Pid},
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    puppet::{BoxedAny, Handler, LifecycleStatus, Puppet, PuppetHandler, PuppetStruct},
    puppet_box::{BoxedPuppet, PuppetBox, PuppetState},
};

pub static PUPPETER: OnceLock<Puppeter> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub struct Puppeter {
    pub(crate) puppets: Arc<RwLock<HashMap<Pid, BoxedPuppet>>>,
    pub(crate) master_to_puppets: Arc<RwLock<HashMap<Id, IndexSet<Pid>>>>,
    pub(crate) puppet_to_master: Arc<RwLock<HashMap<Pid, Id>>>,
    pub(crate) state: Arc<RwLock<HashMap<Id, BoxedAny>>>,
}

pub trait Master: Send + 'static {}

impl Master for Puppeter {}

// STATUS

impl Puppeter {
    pub fn get_puppet<S>(&self) -> Option<PuppetBox<S>>
    where
        PuppetBox<S>: Puppet,
        S: PuppetState,
    {
        let id = Pid::new::<PuppetBox<S>>();
        self.puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .map(|boxed| unsafe { *boxed.downcast_unchecked::<PuppetBox<S>>() })
    }

    pub(crate) fn get_status<S>(&self) -> Option<LifecycleStatus>
    where
        S: PuppetState,
        PuppetBox<S>: Puppet,
    {
        let id = Pid::new::<PuppetBox<S>>();
        self.get_puppet::<S>()
            .map(|puppet| *puppet.status_rx.borrow())
    }

    pub(crate) fn set_status<S>(&self, status: LifecycleStatus)
    where
        S: PuppetState,
        PuppetBox<S>: Puppet,
    {
        let id = Pid::new::<PuppetBox<S>>();
        self.get_puppet()
            .map(|puppet| puppet.status_tx.send(status));
    }
}

/// INFO

impl Puppeter {
    pub fn is_puppet_exists<S>(&self) -> bool
    where
        S: PuppetState,
        PuppetBox<S>: Puppet,
    {
        let id = Pid::new::<PuppetBox<S>>();
        self.get_puppet().is_some()
    }
}

/// ADDRESS

impl Puppeter {
    pub fn get_address<S>(&self) -> Option<PuppetAddress<PuppetBox<S>>>
    where
        S: PuppetState,
        PuppetBox<S>: Puppet,
    {
        self.get_puppet::<S>().map(PuppetAddress::from)
    }
}

/// RELATIONS

impl Puppeter {
    pub fn get_master<S>(&self) -> Id
    where
        S: PuppetState,
        PuppetBox<S>: Puppet,
    {
        let id = Pid::new::<PuppetBox<S>>();
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
        let puppet = Pid::new::<P>();
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

    pub fn detach_puppet<P>(&self)
    where
        P: Puppet,
    {
        let master = self.get_master::<P>();
        let puppet = Pid::new::<P>();
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

    pub fn change_master<M, P>(&self)
    where
        M: Master,
        P: Puppet,
    {
        self.detach_puppet::<P>();
        self.set_master::<M, P>();
    }

    pub fn remove_puppet<P>(&self)
    where
        P: Puppet,
    {
        let master = self.get_master::<P>();
        let puppet = Pid::new::<P>();
        self.puppet_statuses
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
        self.puppet_addresses
            .write()
            .expect("Failed to acquire write lock")
            .remove(&puppet);
        self.master_to_puppets
            .write()
            .expect("Failed to acquire write lock")
            .entry(master)
            .or_default()
            .shift_remove(&puppet);
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
        let puppet = Pid::new::<P>();
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .map(|puppets| puppets.contains(&puppet))
    }

    pub fn get_puppets<M>(&self) -> IndexSet<Pid>
    where
        M: Master,
    {
        let master = Id::new::<M>();
        self.master_to_puppets
            .read()
            .expect("Failed to acquire read lock")
            .get(&master)
            .cloned()
            .unwrap_or_default()
    }

    pub fn release_puppet<P>(&self)
    where
        P: Puppet,
    {
        self.change_master::<Self, P>();
    }

    pub fn release_all_puppets<M>(&self)
    where
        M: Master,
    {
        let puppets = self.get_puppets::<M>();
        for puppet in puppets {
            puppet.release(self);
        }
    }
}

/// LIFECYCLE
/// START

impl Puppeter {
    pub async fn start_puppet<M, P>(&self) -> Result<(), StartPuppetError<M, P>>
    where
        M: Master,
        P: Puppet,
    {
        match self.has_puppet::<M, P>() {
            None => Err(PuppetDoesNotExist::<P>::new().into()),
            Some(false) => {
                Err(PermissionDenied::<M, P>::new()
                    .with_message("Cannot start another master puppet")
                    .into())
            }
            Some(true) => {
                match self.get_status::<P>().unwrap() {
                    LifecycleStatus::Active
                    | LifecycleStatus::Activating
                    | LifecycleStatus::Restarting => Ok(()),
                    _ => {
                        let command_address = self.get_address::<P>()?;
                        Ok(command_address.send_command(ServiceCommand::Start).await?)
                    }
                }
            }
        }
        // match (has_puppet, address) {
        //     (None, _)
        //     (Some(false), _) => Err(PermissionDenied::<M, P>::new())?,
        //     // Err(error) => Err(error.with_message("Can't start puppet from
        // another master"))?,     // Ok(None) =>
        // Err(PuppetDoesNotExist::new().into()),     //
        // Ok(Some(command_address)) => {     //     let status =
        // self.get_status::<P>().unwrap();     //     match status {
        //     //         LifecycleStatus::Active
        //     //         | LifecycleStatus::Activating
        //     //         | LifecycleStatus::Restarting => Ok(()),
        //     //         _ =>
        // Ok(command_address.send_command(ServiceCommand::Stop).await?),
        //     //     }
        //     // }
        // }
    }

    // TODO:

    pub async fn start_puppets<M, P>(&self) -> Result<(), StartPuppetError<M, P>>
    where
        M: Master,
        P: Master,
    {
        let master = Pid::new::<M>();
        let puppet = Pid::new::<P>();
        let puppets = self.get_puppets_by_id(puppet);
        for id in puppets {
            self.start_puppet_by_id(master, id).await?;
        }
        Ok(())
    }
    //
    //
    // pub async fn start_tree<M, P>(&self) -> Result<(), StartPuppetError>
    // where
    //     M: Master,
    //     P: Master,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     self.start_tree_by_id(master, puppet).await?;
    //     Ok(())
    // }
    //
    // pub async fn start_tree_by_id(&self, master: Id, puppet: Id) -> Result<(),
    // StartPuppetError> {     let mut queue = VecDeque::new();
    //     queue.push_back(puppet);
    //
    //     while let Some(current_id) = queue.pop_front() {
    //         self.start_puppet_by_id(master, current_id).await?;
    //
    //         let puppets_ids = self.get_puppets_by_id(current_id);
    //         for id in puppets_ids {
    //             queue.push_back(id);
    //         }
    //     }
    //
    //     Ok(())
    // }
}

/// STOP

impl Puppeter {
    // pub async fn stop_puppet<M, P>(&self) -> Result<(), StopPuppetError<M, P>>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     match self.get_command_address::<M, P>() {
    //         Err(e) => Err(e.with_message("Can't stop puppet from another
    // master"))?,         Ok(None) => Err(PuppetDoesNotExist::new::<P>()),
    //         Ok(Some(command_address)) => {
    //             let status = self.get_status::<P>().unwrap();
    //             match status {
    //                 LifecycleStatus::Inactive | LifecycleStatus::Deactivating =>
    // Ok(()),                 _ =>
    // Ok(command_address.send_command(ServiceCommand::Stop).await?),
    //             }
    //         }
    //     }
    // }

    // pub async fn stop_puppets<M, P>(&self) -> Result<(), StopPuppetError<M, P>>
    // where
    //     M: Master,
    //     P: Master,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     for id in puppets.iter().rev() {
    //         self.stop_puppet_by_id(master, *id).await?
    //     }
    //     Ok(())
    // }
    //
    // pub async fn stop_tree<M, P>(&self) -> Result<(), StopPuppetError>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     self.stop_tree_by_id(master, puppet).await?;
    //     Ok(())
    // }
    //
    // pub async fn stop_tree_by_id(&self, master: Id, puppet: Id) -> Result<(),
    // StopPuppetError> {     let mut stack = Vec::new();
    //     let mut queue = Vec::new();
    //
    //     stack.push(puppet);
    //
    //     while let Some(current_id) = stack.pop() {
    //         queue.push(current_id);
    //
    //         for id in self.get_puppets_by_id(current_id) {
    //             stack.push(id);
    //         }
    //     }
    //
    //     for id in queue.iter().rev() {
    //         self.stop_puppet_by_id(master, *id).await?;
    //     }
    //
    //     Ok(())
    // }
}

/// RESTART

impl Puppeter {
    // pub async fn restart_puppet<M, P>(&self) -> Result<(), ResetPuppetError<M,
    // P>> where
    //     M: Master,
    //     P: Puppet,
    // {
    //     match self.get_command_address::<M, P>() {
    //         Err(e) => Err(e.with_message("Can't restart puppet from another
    // master"))?,         Ok(None) => Err(PuppetDoesNotExist::new::<P>()),
    //         Ok(Some(command_address)) => {
    //             let status = self.get_status::<P>().unwrap();
    //             match status {
    //                 LifecycleStatus::Restarting => Ok(()),
    //                 _ => {
    //                     Ok(command_address
    //                         .send_command(ServiceCommand::Restart)
    //                         .await?)
    //                 }
    //             }
    //         }
    //     }
    // }

    // pub async fn restart_puppets<M, P>(&self) -> Result<(), ResetPuppetError<M,
    // P>> where
    //     M: Master,
    //     P: Master,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     for id in puppets.iter().rev() {
    //         self.restart_puppet_by_id(master, *id).await?;
    //     }
    //     Ok(())
    // }

    // pub async fn restart_tree<M, P>(&self) -> Result<(), ResetPuppetError>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     self.restart_tree_by_id(master, puppet).await?;
    //     Ok(())
    // }
    //
    // pub async fn restart_tree_by_id(&self, master: Id, puppet: Id) -> Result<(),
    // ResetPuppetError> {     let mut stack = Vec::new();
    //     let mut queue = Vec::new();
    //
    //     stack.push(puppet);
    //
    //     while let Some(current_id) = stack.pop() {
    //         queue.push(current_id);
    //
    //         for id in self.get_puppets_by_id(current_id) {
    //             stack.push(id);
    //         }
    //     }
    //
    //     for id in queue.iter().rev() {
    //         self.stop_puppet_by_id(master, *id).await?;
    //     }
    //
    //     Ok(())
    // }
}
//
// /// KILL
//
impl Puppeter {
    // pub async fn kill_puppet<M, P>(&self) -> Result<(), KillPuppetError<M, P>>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     match self.get_command_address::<M, P>() {
    //         Err(e) => Err(e.with_message("Can't kill puppet from another
    // master"))?,         Ok(None) => Err(PuppetDoesNotExist::new::<P>()),
    //         Ok(Some(command_address)) => {
    //             let status = self.get_status::<P>().unwrap();
    //             match status {
    //                 LifecycleStatus::Deactivating => Ok(()),
    //                 _ => {
    //
    // command_address.send_command(ServiceCommand::Kill).await?;
    // self.remove_puppet::<M, P>();                     Ok(())
    //                 }
    //             }
    //         }
    //     }
    // }

    // pub async fn kill_puppets<M, P>(&self) -> Result<(), KillPuppetError<M, P>>
    // where
    //     M: Master,
    //     P: Master,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     for id in puppets.iter().rev() {
    //         self.kill_puppet_by_id(master, *id).await?;
    //     }
    //     Ok(())
    // }

    // pub async fn kill_tree<M, P>(&self) -> Result<(), KillPuppetError>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     self.kill_tree_by_id(master, puppet).await?;
    //     Ok(())
    // }
    //
    // pub async fn kill_tree_by_id(&self, master: Id, puppet: Id) -> Result<(),
    // KillPuppetError> {     let mut stack = Vec::new();
    //     let mut queue = Vec::new();
    //
    //     stack.push(puppet);
    //
    //     while let Some(current_id) = stack.pop() {
    //         queue.push(current_id);
    //
    //         for id in self.get_puppets_by_id(current_id) {
    //             stack.push(id);
    //         }
    //     }
    //
    //     for id in queue.iter().rev() {
    //         self.kill_puppet_by_id(master, *id).await?;
    //     }
    //
    //     Ok(())
    // }
}

impl Puppeter {
    // pub(crate) fn spawn<M, P>(
    //     &self,
    //     puppet: impl Into<PuppetStruct<P>>,
    // ) -> Result<PuppetAddress<P>, PuppeterSpawnError<P>>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     if self.is_puppet_exists::<P>() {
    //         Err(PuppetAlreadyExist::new::<P>())?
    //     } else {
    //         let puppet_struct = puppet.into();
    //         let (handle, puppet_address, command_address) =
    //             create_puppeter_entities(&puppet_struct);
    //
    //         self.set_status::<P>(LifecycleStatus::Activating);
    //         self.set_address::<P>(puppet_address.clone());
    //         self.set_command_address::<P>(command_address);
    //         self.set_master::<M, P>();
    //         tokio::spawn(run_puppet_loop(puppet_struct.puppet, handle));
    //         Ok(puppet_address)
    //     }
    // }
    //
    // pub async fn send<P, E>(&self, message: E) -> Result<(),
    // PuppeterSendMessageError<P>> where
    //     P: Handler<E>,
    //     E: Message + 'static,
    // {
    //     let puppet = Id::new::<P>();
    //     if let Some(address) = self.get_address::<P>() {
    //         let status = self.get_status::<P>().unwrap();
    //         match status {
    //             LifecycleStatus::Active
    //             | LifecycleStatus::Activating
    //             | LifecycleStatus::Restarting => {
    //                 address
    //                     .tx
    //                     .send(message)
    //                     .await
    //                     .map_err(|err| PuppeterSendMessageError::from((err,
    // String::from(puppet))))             }
    //             _ => Err(PuppetCannotHandleMessage::new::<P>(status))?,
    //         }
    //     } else {
    //         Err(PuppetDoesNotExist::new::<P>())
    //     }
    // }
    //
    // pub async fn ask<P, E>(&self, message: E) -> Result<P::Response,
    // PuppeterSendMessageError<P>> where
    //     P: Handler<E>,
    //     E: Message + 'static,
    // {
    //     let puppet = Id::new::<P>();
    //     if let Some(address) = self.get_address::<P>() {
    //         let status = self.get_status::<P>().unwrap();
    //         match status {
    //             LifecycleStatus::Active
    //             | LifecycleStatus::Activating
    //             | LifecycleStatus::Restarting => {
    //                 address
    //                     .tx
    //                     .send_and_await_response(message)
    //                     .await
    //                     .map_err(|err| PuppeterSendMessageError::from((err,
    // String::from(puppet))))             }
    //             _ => Err(PuppetCannotHandleMessage::new::<P>(status))?,
    //         }
    //     } else {
    //         Err(PuppeterSendMessageError::PuppetDoesNotExist {
    //             name: puppet.into(),
    //         })
    //     }
    // }
    //
    // pub async fn ask_with_timeout<P, E>(
    //     &self,
    //     message: E,
    //     duration: std::time::Duration,
    // ) -> Result<P::Response, PuppeterSendMessageError<P>>
    // where
    //     P: Handler<E>,
    //     E: Message + 'static,
    // {
    //     let ask_future = self.ask::<P, E>(message);
    //     let timeout_future = tokio::time::sleep(duration);
    //     tokio::select! {
    //                 response = ask_future => response,
    //                 _ = timeout_future =>
    //     Err(PuppeterSendMessageError::RequestTimeout { name:
    // Id::new::<P>().into() }),         } }
    //
    // pub async fn send_command<M, P>(
    //     &self,
    //     command: ServiceCommand,
    // ) -> Result<(), PuppeterSendCommandError<M, P>>
    // where
    //     M: Master,
    //     P: Puppet,
    // {
    //     let master = Id::new::<M>();
    //     let puppet = Id::new::<P>();
    //     match self.get_command_address::<P>() {
    //         Err(e) => Err(e.with_message("Can't send command to another
    // master"))?,         Ok(None) =>
    // Err(PuppetDoesNotExist::new::<P>().into()),
    //         Ok(Some(command_address)) => {
    //             command_address
    //                 .command_tx
    //                 .send_and_await_response(command)
    //                 .await
    //                 .map_err(|err| PuppeterSendCommandError::from((err,
    // String::from(puppet))))         }
    //     }
    // }
    //
    // pub async fn report_failure<P>(&self, error: &impl Into<PuppetError<P>>)
    // where
    //     P: Puppet,
    // {
    //     let master = self.get_master::<P>();
    //     // self.send_command_by_id(master, puppet, ServiceCommand::ReportFailure
    //     // { puppet: (), message: () })
    // }
    //
    // // pub fn with_state<T>(self, state: T) -> Self
    // // where
    // //     T: Any + Clone + Send + Sync,
    // // {
    // //     let id: Id = TypeId::of::<T>().into();
    // //     self.state
    // //         .write()
    // //         .expect("Failed to acquire write lock")
    // //         .insert(id, BoxedAny::new(state));
    // //     self
    // // }
    // //
    // // pub fn with_minion<A>(self, minion: impl Into<PuppetStruct<A>>) ->
    // // Result<Self, PuppeterError> where
    // //     A: Puppet,
    // // {
    // //     self.spawn(minion)?;
    // //     Ok(self)
    // // }
}

pub(crate) async fn run_puppet_loop<S>(
    mut puppet: PuppetBox<S>,
    mut handle: PuppetHandler<PuppetBox<S>>,
) -> Result<(), PuppetError<PuppetBox<S>>>
where
    S: PuppetState + Clone,
    PuppetBox<S>: Puppet,
{
    puppet.box_start().await?;

    loop {
        let Some(status) = puppet.get_status::<PuppetBox<S>>() else {
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

                    envelope.reply_error(PuppetCannotHandleMessage::new(status).into()).await;
                }
            }
            else => {
                break;
            }
        }
    }
    Ok(())
}

fn create_puppeter_entities<P>(minion: &PuppetStruct<P>) -> (PuppetHandler<P>, PuppetAddress<P>)
where
    P: Puppet,
{
    let id = Pid::new::<P>();
    let (tx, rx) = mpsc::channel::<Box<dyn Envelope<P>>>(minion.buffer_size);
    let (command_tx, command_rx) = mpsc::channel::<ServicePacket<P>>(minion.commands_buffer_size);

    let handler = PuppetHandler {
        id,
        rx: Mailbox::new(rx),
        command_rx: ServiceMailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = ServicePostman::new(command_tx);
    let puppet_address = PuppetAddress { id, tx, command_tx };
    (handler, puppet_address)
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
    //     #[derive(Debug, Default, Clone, Puppet)]
    //     pub struct SleepActor {
    //         i: i32,
    //     }
    //
    //     #[async_trait]
    //     impl Handler<SleepMessage> for SleepActor {
    //         type Response = i32;
    //         type Executor = executor::SequentialExecutor;
    //         async fn handle_message(
    //             &mut self,
    //             msg: SleepMessage,
    //         ) -> Result<i32, PuppetError<Self>> {
    //             println!("SleepActor Received message: {:?}", msg);
    //             // with_state(|i: Option<&i32>| {
    //             //     if i.is_some() {
    //             //         println!("SleepActor Context: {:?}", i);
    //             //     }
    //             // });
    //             // with_state_mut(|i: Option<&mut i32>| {
    //             //     *i.unwrap() += 1;
    //             // });
    //             tokio::time::sleep(std::time::Duration::from_millis(1000 *
    // 5)).await;             Ok(msg.i)
    //         }
    //     }
    //
    //     let _ = SleepActor { i: 0 }.spawn();
    //
    //     puppeter()
    //         .send::<SleepActor, _>(SleepMessage { i: 10 })
    //         .await
    //         .unwrap();
    //     puppeter()
    //         .send::<SleepActor, _>(SleepMessage { i: 10 })
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
