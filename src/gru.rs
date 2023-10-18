use std::{
    any::{type_name, Any, TypeId},
    sync::{Arc, OnceLock, RwLock},
};

use hashbrown::HashMap;
use pollster::FutureExt;
use tokio::sync::mpsc;

use crate::{
    address::Address,
    message::{Mailbox, Message, Packet, Postman, ServiceCommand},
    minion::{execution, BoxedAny, LifecycleStatus, Minion, MinionHandle, MinionStruct},
    Id, MinionsError,
};

pub static GRU: OnceLock<Gru> = OnceLock::new();

pub fn gru() -> &'static Gru {
    GRU.get().expect("Gru not initialized")
}

#[derive(Clone, Default, Debug)]
pub struct Gru {
    pub(crate) status: Arc<RwLock<HashMap<Id, LifecycleStatus>>>,
    pub(crate) address: Arc<RwLock<HashMap<Id, BoxedAny>>>,
    pub(crate) state: Arc<RwLock<HashMap<Id, BoxedAny>>>,
}

#[allow(dead_code)]
impl Gru {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init(&self) {
        GRU.set(self.clone()).unwrap();
    }

    pub fn with_state<T>(&self, state: T) -> &Self
    where
        T: Any + Clone + Send + Sync,
    {
        let id: Id = TypeId::of::<T>().into();
        gru()
            .state
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, BoxedAny::new(state));
        self
    }

    pub fn with_minion<A>(&self, minion: impl Into<MinionStruct<A>>) -> Result<&Self, MinionsError>
    where
        A: Minion,
    {
        self.spawn(minion)?;
        Ok(self)
    }

    fn minion_exists<A>(&self) -> bool
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.address
            .read()
            .expect("Failed to acquire read lock")
            .contains_key(&id)
    }

    #[inline(always)]
    fn get_status<A>(&self) -> Option<LifecycleStatus>
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.status
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .cloned()
    }

    fn set_status<A>(&self, status: LifecycleStatus)
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.status
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, status);
    }

    fn get_address<A>(&self) -> Option<Address<A>>
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.address
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .map(|boxed_any| boxed_any.downcast_ref_unchecked::<Address<A>>().clone())
    }

    fn set_address<A>(&self, address: Address<A>)
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, BoxedAny::new(address));
    }

    fn spawn<A>(&self, minion: impl Into<MinionStruct<A>>) -> Result<Address<A>, MinionsError>
    where
        A: Minion,
    {
        if self.minion_exists::<A>() {
            Err(MinionsError::MinionAlreadyExists(
                type_name::<A>().to_string(),
            ))
        } else {
            let minion_struct = minion.into();
            let (handle, address) = create_minion_entities(&minion_struct);
            self.set_status::<A>(LifecycleStatus::Activating);
            self.set_address::<A>(address.clone());
            tokio::spawn(run_minion_loop(
                address.clone(),
                handle,
                minion_struct.minion,
            ));
            Ok(address)
        }
    }

    async fn send<A>(&self, message: impl Into<A::Msg>) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        if let Some(address) = self.get_address::<A>() {
            let status = self.get_status::<A>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => address.tx.send(message.into()).await,
                _ => Err(MinionsError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    async fn ask<A>(
        &self,
        message: impl Into<A::Msg>,
    ) -> Result<<A::Msg as Message>::Response, MinionsError>
    where
        A: Minion,
    {
        if let Some(address) = self.get_address::<A>() {
            let status = self.get_status::<A>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => {
                    address.tx.send_and_await_response(message.into()).await
                }
                _ => Err(MinionsError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    async fn ask_with_timeout<A>(
        &self,
        message: impl Into<A::Msg>,
        duration: std::time::Duration,
    ) -> Result<<A::Msg as Message>::Response, MinionsError>
    where
        A: Minion,
    {
        let ask_future = self.ask::<A>(message);
        let timeout_future = tokio::time::sleep(duration);
        tokio::select! {
            response = ask_future => response,
            _ = timeout_future => Err(MinionsError::MessageResponseTimeout),
        }
    }

    async fn send_command<A>(&self, command: ServiceCommand) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        if let Some(address) = self.get_address::<A>() {
            address.command_tx.send_and_await_response(command).await
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    async fn start<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Start).await
    }

    async fn stop<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Stop).await
    }

    async fn kill<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        if let Some(address) = self.get_address::<A>() {
            self.send_command::<A>(ServiceCommand::Terminate).await?;
            self.address
                .write()
                .expect("Failed to acquire write lock")
                .remove(&address.id);
            self.status
                .write()
                .expect("Failed to acquire write lock")
                .remove(&address.id);
            Ok(())
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    async fn restart<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Restart).await
    }
}

macro_rules! forward {
    ($fn:ident($($arg_name:ident: $arg_type:ty),*) => $out:ty) => {
        pub fn $fn<A: Minion>($($arg_name: $arg_type),*) -> $out {
            gru().$fn::<A>($($arg_name),*)
        }
    };
    (async $fn:ident($($arg_name:ident: $arg_type:ty),*) => $out:ty) => {
        pub async fn $fn<A: Minion>($($arg_name: $arg_type),*) -> $out {
            gru().$fn::<A>($($arg_name),*).await
        }
    };
}

forward!(minion_exists() => bool);
forward!(get_status() => Option<LifecycleStatus>);
forward!(set_status(status: LifecycleStatus) => ());
forward!(get_address() => Option<Address<A>>);
forward!(spawn(minion: impl Into<MinionStruct<A>>) => Result<Address<A>, MinionsError>);
forward!(async send(message: impl Into<A::Msg>) => Result<(), MinionsError>);
forward!(async ask(message: impl Into<A::Msg>) => Result<<A::Msg as Message>::Response, MinionsError>);
forward!(async ask_with_timeout(message: impl Into<A::Msg>, duratio: std::time::Duration) => Result<<A::Msg as Message>::Response, MinionsError>);
forward!(async send_command(command: ServiceCommand) => Result<(), MinionsError>);
forward!(async start() => Result<(), MinionsError>);
forward!(async stop() => Result<(), MinionsError>);
forward!(async kill() => Result<(), MinionsError>);
forward!(async restart() => Result<(), MinionsError>);

pub(crate) async fn run_minion_loop<A>(
    address: Address<A>,
    mut handle: MinionHandle<A>,
    mut minion: impl Minion<Msg = A::Msg>,
) where
    A: Minion,
{
    minion
        .start()
        .await
        .unwrap_or_else(|err| panic!("Minion {} failed to start. Err: {}", address, err));

    let execution_variant = execution::ExecutionVariant::from_type::<A::Exec>();

    loop {
        let Some(status) = get_status::<A>() else {
            break;
        };
        if status.should_wait_for_activation() {
            continue;
        }

        tokio::select! {
            Some(Packet { message, reply_address }) = handle.command_rx.recv() => {
                let response = minion.handle_command(message).await;
                if let Some(tx) = reply_address {
                    tx.send(response).unwrap_or_else(|_| panic!("Minion {} failed to send response", address));
                }
            }
            Some(Packet { message, reply_address }) = handle.rx.recv() => {
                if status.should_handle_message() {
                    use execution::ExecutionVariant as E;
                    match execution_variant {
                        E::Sequential => {
                            let response = minion.handle_message(message).await;
                            if let Some(tx) = reply_address {
                                tx.send(Ok(response)).unwrap_or_else(|_| panic!("Minion {} failed to send response", address));
                            }
                        },
                        E::Concurrent => {
                            let mut cloned_minion = minion.clone();
                            let cloned_address = address.clone();
                            tokio::spawn(async move {
                                let response = cloned_minion.handle_message(message).await;
                                if let Some(tx) = reply_address {
                                    tx.send(Ok(response)).unwrap_or_else(|_| panic!("Minion {} failed to send response", cloned_address));
                                }
                            });
                        },
                        E::Parallel => {
                            let mut cloned_minion = minion.clone();
                            let cloned_address = address.clone();
                            rayon::spawn(move || {
                                let response = cloned_minion.handle_message(message).block_on();
                                if let Some(tx) = reply_address {
                                    tx.send(Ok(response)).unwrap_or_else(|_| panic!("Minion {} failed to send response", cloned_address));
                                }
                            });
                        },
                    }
                } else if status.should_drop_message() {
                    if let Some(tx) = reply_address {
                        tx.send(Err(MinionsError::MinionCannotHandleMessage(status))).unwrap_or_else(|_| panic!("Minion {} failed to send response", address));
                    }
                }
            }
        }
    }
}

fn create_minion_entities<A>(minion: &MinionStruct<A>) -> (MinionHandle<A>, Address<A>)
where
    A: Minion,
{
    let (tx, rx) = mpsc::channel::<Packet<A::Msg>>(minion.buffer_size);
    let (command_tx, command_rx) =
        mpsc::channel::<Packet<ServiceCommand>>(minion.commands_buffer_size);

    let handler = MinionHandle {
        rx: Mailbox::new(rx),
        command_rx: Mailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = Postman::new(command_tx);
    let address = Address {
        id: TypeId::of::<A>().into(),
        name: type_name::<A>().to_string(),
        tx,
        command_tx,
    };
    (handler, address)
}

#[allow(dead_code)]
#[cfg(test)]
mod tests {

    use async_trait::async_trait;
    use minions_derive::Message;

    // use crate::context::provide_context;

    use crate::state::{provide_state, with_state, with_state_mut};

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone, Message)]
        #[message(response = i32)]
        pub struct TestMessage {
            i: i32,
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[derive(Debug, Clone, Message)]
        #[message(response = i32)]
        pub struct SleepMessage {
            i: i32,
        }

        #[derive(Debug, Default, Clone)]
        pub struct SleepActor {
            i: i32,
        }

        #[async_trait]
        impl Minion for SleepActor {
            type Msg = SleepMessage;
            type Exec = execution::Concurrent;
            async fn handle_message(&mut self, msg: Self::Msg) -> <Self::Msg as Message>::Response {
                println!("SleepActor Received message: {:?}", msg);
                with_state(|i: Option<&i32>| {
                    if i.is_some() {
                        println!("SleepActor Context: {:?}", i);
                    }
                });
                with_state_mut(|i: Option<&mut i32>| {
                    *i.unwrap() += 1;
                });
                tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
                msg.i
            }
        }

        Gru::new()
            .with_state(0)
            .with_minion(SleepActor { i: 0 })
            .unwrap()
            .init();
        provide_state::<i32>(0);
        spawn(SleepActor { i: 0 }).unwrap();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..10 {
            set.spawn(ask::<SleepActor>(SleepMessage { i: 10 }));
            // set.spawn(ask::<TestActor>(TestMessage { i: 10 }));
        }
        while let Some(Ok(res)) = set.join_next().await {
            println!("Response: {:?}", res);
        }
    }
}
