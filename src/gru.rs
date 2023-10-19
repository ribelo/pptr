use std::{
    any::{type_name, Any, TypeId},
    sync::{Arc, OnceLock, RwLock},
};

use hashbrown::HashMap;
use tokio::sync::mpsc;

use crate::{
    address::Address,
    message::{
        Envelope, Mailbox, Message, Postman, ServiceCommand, ServiceMailbox, ServicePacket,
        ServicePostman,
    },
    minion::{BoxedAny, Handler, LifecycleStatus, Minion, MinionHandler, MinionStruct},
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
        GRU.get_or_init(Default::default).clone()
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

    pub fn minion_exists<A>(&self) -> bool
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
    pub fn get_status<A>(&self) -> Option<LifecycleStatus>
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

    pub fn set_status<A>(&self, status: LifecycleStatus)
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.status
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, status);
    }

    pub fn get_address<A>(&self) -> Option<Address<A>>
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

    pub fn set_address<A>(&self, address: Address<A>)
    where
        A: Minion,
    {
        let id: Id = TypeId::of::<A>().into();
        self.address
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, BoxedAny::new(address));
    }

    pub fn spawn<A>(&self, minion: impl Into<MinionStruct<A>>) -> Result<Address<A>, MinionsError>
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
            tokio::spawn(run_minion_loop(minion_struct.minion, handle));
            Ok(address)
        }
    }

    pub async fn send<A, M>(&self, message: M) -> Result<(), MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if let Some(address) = self.get_address::<A>() {
            let status = self.get_status::<A>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => address.tx.send(message).await,
                _ => Err(MinionsError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    pub async fn ask<A, M>(&self, message: M) -> Result<A::Response, MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if let Some(address) = self.get_address::<A>() {
            let status = self.get_status::<A>().unwrap();
            match status {
                LifecycleStatus::Active
                | LifecycleStatus::Activating
                | LifecycleStatus::Restarting => address.tx.send_and_await_response(message).await,
                _ => Err(MinionsError::MinionCannotHandleMessage(status)),
            }
        } else {
            Err(MinionsError::MinionDoesNotExist(
                type_name::<A>().to_string(),
            ))
        }
    }

    pub async fn ask_with_timeout<A, M>(
        &self,
        message: M,
        duration: std::time::Duration,
    ) -> Result<A::Response, MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let ask_future = self.ask::<A, M>(message);
        let timeout_future = tokio::time::sleep(duration);
        tokio::select! {
            response = ask_future => response,
            _ = timeout_future => Err(MinionsError::MessageResponseTimeout),
        }
    }

    pub async fn send_command<A>(&self, command: ServiceCommand) -> Result<(), MinionsError>
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

    pub async fn start<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Start).await
    }

    pub async fn stop<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Stop).await
    }

    pub async fn kill<A>(&self) -> Result<(), MinionsError>
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

    pub async fn restart<A>(&self) -> Result<(), MinionsError>
    where
        A: Minion,
    {
        self.send_command::<A>(ServiceCommand::Restart).await
    }
}

pub fn minion_exists<A>() -> bool
where
    A: Minion,
{
    gru().minion_exists::<A>()
}

pub fn get_status<A>() -> Option<LifecycleStatus>
where
    A: Minion,
{
    gru().get_status::<A>()
}

pub fn set_status<A>(status: LifecycleStatus)
where
    A: Minion,
{
    gru().set_status::<A>(status)
}

pub fn get_address<A>() -> Option<Address<A>>
where
    A: Minion,
{
    gru().get_address::<A>()
}

pub fn spawn<A>(minion: impl Into<MinionStruct<A>>) -> Result<Address<A>, MinionsError>
where
    A: Minion,
{
    gru().spawn(minion)
}

pub async fn send<A, M>(message: M) -> Result<(), MinionsError>
where
    A: Handler<M>,
    M: Message + 'static,
{
    gru().send::<A, M>(message).await
}

pub async fn ask<A, M>(message: M) -> Result<A::Response, MinionsError>
where
    A: Handler<M>,
    M: Message + 'static,
{
    gru().ask::<A, M>(message).await
}

pub async fn ask_with_timeout<A, M>(
    message: M,
    duration: std::time::Duration,
) -> Result<A::Response, MinionsError>
where
    A: Handler<M>,
    M: Message + 'static,
{
    gru().ask_with_timeout::<A, M>(message, duration).await
}

pub async fn send_command<A>(command: ServiceCommand) -> Result<(), MinionsError>
where
    A: Minion,
{
    gru().send_command::<A>(command).await
}

pub async fn start<A>() -> Result<(), MinionsError>
where
    A: Minion,
{
    gru().start::<A>().await
}

pub async fn stop<A>() -> Result<(), MinionsError>
where
    A: Minion,
{
    gru().stop::<A>().await
}

pub async fn kill<A>() -> Result<(), MinionsError>
where
    A: Minion,
{
    gru().kill::<A>().await
}

pub async fn restart<A>() -> Result<(), MinionsError>
where
    A: Minion,
{
    gru().restart::<A>().await
}

pub(crate) async fn run_minion_loop<A>(mut minion: A, mut handle: MinionHandler<A>)
where
    A: Minion,
{
    minion
        .start()
        .await
        .unwrap_or_else(|err| panic!("{} failed to start. Err: {}", handle, err));

    loop {
        let Some(status) = get_status::<A>() else {
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
                    envelope.handle_message(&mut minion).await.unwrap_or_else(|err| println!("{} failed to handle command. Err: {}", handle, err));
                }
                else if status.should_drop_message() {
                    envelope.reply_error(MinionsError::MinionCannotHandleMessage(status)).await.unwrap_or_else(|_| println!("{} failed to send response", handle));
                }
            }
            else => {
                break;
            }
        }
    }
}

fn create_minion_entities<A>(minion: &MinionStruct<A>) -> (MinionHandler<A>, Address<A>)
where
    A: Minion,
{
    let (tx, rx) = mpsc::channel::<Box<dyn Envelope<A>>>(minion.buffer_size);
    let (command_tx, command_rx) = mpsc::channel::<ServicePacket>(minion.commands_buffer_size);

    let handler = MinionHandler {
        id: TypeId::of::<A>().into(),
        name: type_name::<A>().to_string(),
        rx: Mailbox::new(rx),
        command_rx: ServiceMailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = ServicePostman::new(command_tx);
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
    // use minions_derive::Message;

    // use crate::context::provide_context;

    use crate::{
        prelude::execution,
        state::{provide_state, with_state, with_state_mut},
    };

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

        impl Minion for SleepActor {}

        #[async_trait]
        impl Handler<SleepMessage> for SleepActor {
            type Response = i32;
            type Exec = execution::Sequential;
            async fn handle_message(&mut self, msg: &SleepMessage) -> i32 {
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

        #[async_trait]
        impl Handler<SleepMessage2> for SleepActor {
            type Response = i32;
            type Exec = execution::Sequential;
            async fn handle_message(&mut self, msg: &SleepMessage2) -> i32 {
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
            .unwrap();
        provide_state::<i32>(0);
        // let mut set = tokio::task::JoinSet::new();
        for _ in 0..10 {
            send::<SleepActor, _>(SleepMessage { i: 10 }).await.unwrap();
            // set.spawn(ask::<SleepActor, _>(SleepMessage { i: 10 }));
            // set.spawn(ask::<TestActor>(TestMessage { i: 10 }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000 * 5)).await;
        // while let Some(Ok(res)) = set.join_next().await {
        //     println!("Response: {:?}", res);
        // }
    }
}
