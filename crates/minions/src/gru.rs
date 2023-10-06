use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    fmt,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

pub static GRU: LazyLock<Gru> = LazyLock::new(|| Gru::new());

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    address::Address,
    message::{Mailbox, Message, Packet, Postman, ServiceMessage},
    minion::{LifecycleStatus, Minion, MinionHandle, MinionId, MinionInstance, MinionStruct},
};

#[derive(Debug, Error)]
pub enum GruError {
    #[error("Minion already exists: {0}")]
    MinionAlreadyExists(String),
    #[error("Minion does not exist: {0}")]
    MinionDoesNotExist(String),
}

impl GruError {
    pub fn actor_already_exists<A>() -> Self
    where
        A: Minion,
    {
        Self::MinionAlreadyExists(type_name::<A>().to_string())
    }
}

#[derive(Clone, Default)]
pub struct Gru {
    pub(crate) actors: Arc<RwLock<HashMap<MinionId, Box<dyn Any + Send + Sync>>>>,
    pub(crate) context: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

fn instance_exists<A>() -> bool
where
    A: Minion,
{
    GRU.actors
        .read()
        .expect("Failed to acquire read lock")
        .contains_key(&TypeId::of::<A>().into())
}

fn get_instance<A>() -> Option<MinionInstance<A>>
where
    A: Minion,
{
    GRU.actors
        .read()
        .expect("Failed to acquire read lock")
        .get(&TypeId::of::<A>().into())
        .and_then(|any_actor| any_actor.downcast_ref::<MinionInstance<A>>())
        .cloned()
}

fn get_postman<A>() -> Option<Postman<A::Msg>>
where
    A: Minion,
{
    GRU.actors
        .read()
        .expect("Failed to acquire read lock")
        .get(&TypeId::of::<A>().into())
        .and_then(|any_actor| any_actor.downcast_ref::<MinionInstance<A>>())
        .map(|instance| instance.tx.clone())
}

pub(crate) async fn run_minion_loop<A>(
    mut handle: MinionHandle<A>,
    actor: impl Minion<Msg = A::Msg>,
) where
    A: Minion,
{
    loop {
        tokio::select! {
            Some(Packet {message, reply_address}) = handle.rx.recv() => {
                match actor.handle_message(message).await {
                    Ok(response) => {
                        if let Some(tx) = reply_address {
                            tx.send(response).expect("Cannot send response");
                        }
                    }
                    Err(err) => panic!("{}", err),
                }
            }
        }
    }
}

fn create_minion_entities<A>(minion: &MinionStruct<A>) -> (MinionHandle<A>, MinionInstance<A>)
where
    A: Minion,
{
    let id = TypeId::of::<A>().into();
    let (tx, rx) = tokio::sync::mpsc::channel::<Packet<A::Msg>>(minion.buffer_size);
    let (service_tx, service_rx) =
        tokio::sync::mpsc::channel::<Packet<ServiceMessage>>(minion.service_buffer_size);

    let status = Arc::new(Mutex::new(LifecycleStatus::Activating));
    let handler = MinionHandle {
        id,
        status: status.clone(),
        rx: Mailbox::new(rx),
        service_rx: Mailbox::new(service_rx),
    };
    let instance = MinionInstance {
        id,
        status,
        tx: Postman::new(tx),
        service_tx: Postman::new(service_tx),
    };
    (handler, instance)
}

pub fn spawn<A: Minion>(minion: impl Into<MinionStruct<A>>) -> Result<(), GruError> {
    let minion = minion.into();
    let id = TypeId::of::<A>().into();
    let name = type_name::<A>().to_string();
    if instance_exists::<A>() {
        Err(GruError::MinionAlreadyExists(name))
    } else {
        let (handle, instance) = create_minion_entities(&minion);
        tokio::spawn(run_minion_loop(handle, minion.minion));
        GRU.actors
            .write()
            .expect("Failed to acquire write lock")
            .insert(id, Box::new(instance));
        Ok(())
    }
}

pub async fn send<A: Minion>(message: impl Into<A::Msg>) -> Result<(), GruError> {
    if let Some(postman) = get_postman::<A>() {
        postman.send(message.into()).await.unwrap();
        Ok(())
    } else {
        Err(GruError::MinionDoesNotExist(type_name::<A>().to_string()))
    }
}

impl Gru {
    pub fn new() -> Self {
        Default::default()
    }
}

// #[async_trait]
// impl Dispatcher for Gru {
//     async fn send<A>(&self, message: A::Msg) -> Result<(), MessageBuilderError>
//     where
//         A: Minion + Clone,
//         <A as Minion>::Msg: Clone,
//     {
//         if let Some(actor) = self.get_actor::<A>() {
//             println!("Sending message to actor: {:?}", actor.address);
//             let recipient_address = Address::from_type::<A>();
//             actor
//                 .tx
//                 .send(message, actor.address.clone(), recipient_address)
//                 .await
//                 .map_err(|err| {
//                     println!("errr {}", err);
//                     MessageBuilderError::message_send_error::<A>()
//                 })
//         } else {
//             Err(MessageBuilderError::actor_does_not_exist::<A>())
//         }
//     }
//
//     // async fn ask<A>(
//     //     &self,
//     //     message: A::Msg,
//     // ) -> Result<<<A as Minion>::Msg as Messageable>::Response, MessageError>
//     // where
//     //     A: Minion,
//     //     <A as Minion>::Msg: std::clone::Clone,
//     // {
//     //     if let Some(actor) = self.get_actor::<A>() {
//     //         let recipient_address = Address::from_type::<A>();
//     //         actor
//     //             .tx
//     //             .send_and_await_response(message, actor.address.clone(), recipient_address)
//     //             .await
//     //             .map_err(|_| MessageError::response_receive_error::<A>())
//     //     } else {
//     //         Err(MessageError::actor_does_not_exist::<A>())
//     //     }
//     // }
//     //
//     // async fn ask_with_timeout<A>(
//     //     &self,
//     //     message: A::Msg,
//     //     timeout: std::time::Duration,
//     // ) -> Result<<<A as Minion>::Msg as Messageable>::Response, MessageError>
//     // where
//     //     A: Minion,
//     //     <A as Minion>::Msg: std::clone::Clone,
//     // {
//     //     if let Some(actor) = self.get_actor::<A>() {
//     //         let recipient_address = Address::from_type::<A>();
//     //         tokio::time::timeout(
//     //             timeout,
//     //             actor
//     //                 .tx
//     //                 .send_and_await_response(message, actor.address.clone(), recipient_address)
//     //                 .await,
//     //         )
//     //         .await
//     //         .map_err(|_| MessageError::actor_response_timeout::<A>())?
//     //         .map_err(|_| MessageError::message_receive_error::<A>())
//     //     } else {
//     //         Err(MessageError::actor_does_not_exist::<A>())
//     //     }
//     // }
//
//     // async fn ask_service<A>(&self, message: ServiceCommand) -> Result<(), MessageError>
//     // where
//     //     A: Minion,
//     //     <A as Minion>::Msg: Clone,
//     // {
//     //     if let Some(actor) = self.get_actor::<A>() {
//     //         actor
//     //             .service_tx
//     //             .send_and_await_response::<A>(message)
//     //             .await
//     //             .map_err(|_| MessageError::response_receive_error::<A>())
//     //     } else {
//     //         Err(MessageError::actor_does_not_exist::<A>())
//     //     }
//     // }
// }

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
//     fn get_context<T: Any + Clone + Send + Sync>(&self) -> Option<T> {
//         self.context
//             .read()
//             .expect("Failed to acquire read lock")
//             .get(&TypeId::of::<T>())
//             .and_then(|ref_entry| ref_entry.downcast_ref::<T>())
//             .cloned()
//     }
//
//     fn with_context<T, R, F>(&self, f: F) -> R
//     where
//         T: Any + Clone + Send + Sync,
//         F: FnOnce(Option<&T>) -> R + Send,
//     {
//         match self
//             .context
//             .write()
//             .expect("Failed to acquire read lock")
//             .get(&TypeId::of::<T>())
//         {
//             Some(context) => {
//                 let typed_context = context.downcast_ref::<T>();
//                 if typed_context.is_none() {
//                     unreachable!()
//                 }
//                 f(typed_context)
//             }
//             None => f(None),
//         }
//     }
//
//     fn with_context_mut<T, R, F>(&self, f: F) -> R
//     where
//         T: Any + Clone + Send + Sync,
//         F: FnOnce(Option<&mut T>) -> R + Send,
//     {
//         match self
//             .context
//             .write()
//             .expect("Failed to acquire write lock")
//             .get_mut(&TypeId::of::<T>())
//         {
//             Some(context) => {
//                 let typed_context = context.downcast_mut::<T>();
//                 if typed_context.is_none() {
//                     unreachable!()
//                 }
//                 f(typed_context)
//             }
//             None => f(None),
//         }
//     }
// }

#[cfg(test)]
mod tests {

    use minions_derive::minion;

    use crate::minion::MinionError;

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone)]
        pub struct TestMessage {
            i: i32,
        }

        impl Message for TestMessage {
            type Response = i32;
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Minion for TestActor {
            type Msg = TestMessage;
            async fn handle_message(
                &self,
                msg: Self::Msg,
            ) -> Result<<Self::Msg as Message>::Response, MinionError> {
                println!("Received message: {:?}", msg);
                send::<Self>(TestMessage { i: msg.i + 1 }).await.unwrap();
                Ok(msg.i)
            }
        }

        spawn(TestActor { i: 0 }).unwrap();
        send::<TestActor>(TestMessage { i: 0 }).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
