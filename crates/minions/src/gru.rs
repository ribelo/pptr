use std::{
    any::{type_name, Any, TypeId},
    future::Future,
    mem,
    sync::{Arc, LazyLock, RwLock},
};

use hashbrown::HashMap;
use tokio::sync::{mpsc, watch};

use crate::{
    address::Address,
    message::{Mailbox, Message, Packet, Postman, ServiceCommand},
    minion::{LifecycleStatus, Minion, MinionHandle, MinionId, MinionInstance, MinionStruct},
    MinionsError,
};

pub static GRU: LazyLock<Gru> = LazyLock::new(Gru::new);

#[derive(Clone, Default)]
pub struct Gru {
    pub(crate) actors: Arc<RwLock<HashMap<MinionId, Box<dyn Any + Send + Sync>>>>,
    pub(crate) context: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

pub fn with_instance<A, R, F>(f: F) -> R
where
    A: Minion,
    F: FnOnce(Option<&MinionInstance<A>>) -> R + Send,
{
    let minion_id: MinionId = TypeId::of::<A>().into();
    match GRU
        .actors
        .read()
        .expect("Failed to acquire read lock")
        .get(&minion_id)
    {
        Some(any_actor) => {
            let instance = unsafe { any_actor.downcast_ref_unchecked::<MinionInstance<A>>() };
            f(Some(instance))
        }
        None => f(None),
    }
}

pub fn get_status<A>() -> Option<LifecycleStatus>
where
    A: Minion,
{
    let minion_id: MinionId = TypeId::of::<A>().into();
    GRU.actors
        .read()
        .expect("Failed to acquire read lock")
        .get(&minion_id)
        .map(|any_actor| unsafe { any_actor.downcast_ref_unchecked::<MinionInstance<A>>() })
        .map(|instance| *instance.status_tx.borrow())
}

pub fn set_status<A>(status: LifecycleStatus)
where
    A: Minion,
{
    let minion_id: MinionId = TypeId::of::<A>().into();
    if let Some(instance) = GRU
        .actors
        .write()
        .expect("Failed to acquire write lock")
        .get_mut(&minion_id)
        .map(|any_actor| unsafe { any_actor.downcast_mut_unchecked::<MinionInstance<A>>() })
    {
        instance.status_tx.send_if_modified(|old| {
            if *old != status {
                *old = status;
                true
            } else {
                false
            }
        });
    }
}

pub fn instance_exists<A>() -> bool
where
    A: Minion,
{
    let minion_id: MinionId = TypeId::of::<A>().into();
    GRU.actors
        .read()
        .expect("Failed to acquire read lock")
        .contains_key(&minion_id)
}

pub(crate) async fn run_minion_loop<A>(
    address: Address<A>,
    mut handle: MinionHandle<A>,
    mut actor: impl Minion<Msg = A::Msg>,
) where
    A: Minion,
{
    actor
        .start()
        .await
        .unwrap_or_else(|err| panic!("Minion {} failed to start. Err: {}", address, err));

    loop {
        let status = *handle.status_rx.borrow();

        if status.should_wait_for_activation() {
            continue;
        }

        tokio::select! {
            Some(Packet { message, .. }) = handle.command_rx.recv() => {
                actor.handle_command(message).await.unwrap_or_else(|_| panic!("Minion {} failed to handle command: {}", address, message));
            }
            Some(Packet { message, reply_address }) = handle.rx.recv() => {
                if status.should_handle_message() {
                    let response = actor.handle_message(message).await;
                    if let Some(tx) = reply_address {
                        tx.send(Ok(response)).unwrap_or_else(|_| panic!("Minion {} failed to send response", address));
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

fn create_minion_entities<A>(
    minion: &MinionStruct<A>,
) -> (MinionHandle<A>, MinionInstance<A>, Address<A>)
where
    A: Minion,
{
    let (tx, rx) = mpsc::channel::<Packet<A::Msg>>(minion.buffer_size);
    let (command_tx, command_rx) =
        mpsc::channel::<Packet<ServiceCommand>>(minion.commands_buffer_size);
    let (status_tx, status_rx) = watch::channel(LifecycleStatus::Activating);

    let handler = MinionHandle {
        status_rx,
        rx: Mailbox::new(rx),
        command_rx: Mailbox::new(command_rx),
    };
    let tx = Postman::new(tx);
    let command_tx = Postman::new(command_tx);
    let instance = MinionInstance {
        status_tx,
        tx: tx.clone(),
        command_tx: command_tx.clone(),
    };
    let address = Address {
        id: TypeId::of::<A>().into(),
        name: type_name::<A>().to_string(),
        tx,
        command_tx,
    };
    (handler, instance, address)
}

pub fn spawn<A>(minion: impl Into<MinionStruct<A>>) -> Result<Address<A>, MinionsError>
where
    A: Minion,
{
    let minion = minion.into();
    if instance_exists::<A>() {
        Err(MinionsError::MinionAlreadyExists(
            type_name::<A>().to_string(),
        ))
    } else {
        let (handle, instance, address) = create_minion_entities(&minion);
        GRU.actors
            .write()
            .expect("Failed to acquire write lock")
            .insert(address.id, Box::new(instance));
        tokio::spawn(run_minion_loop(address.clone(), handle, minion.minion));
        Ok(address)
    }
}

pub async fn send<A>(message: impl Into<A::Msg>) -> Result<(), MinionsError>
where
    A: Minion,
{
    let result = with_instance::<A, _, _>(|instance| {
        instance.map(|inst| (inst.tx.clone(), *inst.status_tx.borrow()))
    });
    match result {
        Some((postman, status)) => match status {
            LifecycleStatus::Active | LifecycleStatus::Activating | LifecycleStatus::Restarting => {
                postman.send(message.into()).await
            }
            _ => Err(MinionsError::MinionCannotHandleMessage(status)),
        },
        None => Err(MinionsError::MinionDoesNotExist(
            type_name::<A>().to_string(),
        )),
    }
}

pub async fn ask<A>(
    message: impl Into<A::Msg>,
) -> Result<<A::Msg as Message>::Response, MinionsError>
where
    A: Minion,
{
    let result = with_instance::<A, _, _>(|instance| {
        instance.map(|inst| (inst.tx.clone(), *inst.status_tx.borrow()))
    });

    match result {
        Some((postman, status)) => match status {
            LifecycleStatus::Active | LifecycleStatus::Activating | LifecycleStatus::Restarting => {
                postman.send_and_await_response(message.into()).await
            }
            _ => Err(MinionsError::MinionCannotHandleMessage(status)),
        },
        None => Err(MinionsError::MinionDoesNotExist(
            type_name::<A>().to_string(),
        )),
    }
}

pub async fn control<A>(command: ServiceCommand) -> Result<(), MinionsError>
where
    A: Minion,
{
    if let Some(postman) =
        with_instance::<A, _, _>(|instance| instance.map(|inst| inst.command_tx.clone()))
    {
        postman.send(command).await?;
        Ok(())
    } else {
        Err(MinionsError::MinionDoesNotExist(
            type_name::<A>().to_string(),
        ))
    }
}

impl Gru {
    pub fn new() -> Self {
        Default::default()
    }
}

// #[async_trait]
// impl Context for Gru {
//     fn provide_context<T: Any + Clone + Send + Sync>(&self, context: T) -> Option<T> {
//         self.context
//             .write()
//             .expect("Failed to acquire write lock")
//             .insert(TypeId::of::<T>(), Box::new(context))
//             .and_then(|box_any| box_any.downcast::<T>().ok().map(|boxed_value| *boxed_value))
//     }
//
// }

#[allow(dead_code)]
#[cfg(test)]
mod tests {

    use async_trait::async_trait;
    use minions_derive::Message;

    use crate::context::provide_context;

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

        #[async_trait]
        impl Minion for TestActor {
            type Msg = TestMessage;
            async fn handle_message(&mut self, msg: Self::Msg) -> <Self::Msg as Message>::Response {
                println!("Received message: {:?}", msg);
                // with_context(|ctx: Option<i32>| async move {
                //     println!("Context: {:?}", ctx);
                // })
                // .await;
                // send::<Self>(TestMessage { i: msg.i + 1 }).await.unwrap();
                msg.i
            }
        }

        provide_context::<i32>(0);
        spawn(TestActor { i: 0 }).unwrap();
        let x = ask::<TestActor>(TestMessage { i: 0 }).await.unwrap();
        println!("a {}", x);
        control::<TestActor>(ServiceCommand::Stop).await.unwrap();
        println!("b {}", x);
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
