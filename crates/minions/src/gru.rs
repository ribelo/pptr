use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    sync::{Arc, RwLock},
};

const GRU: Gru = Gru::new();

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::message::{Messageable, Packet};

// #[derive(Debug, Error)]
// pub enum GruError {
//     #[error("Actor already exists: {0}")]
//     ActorAlreadyExists(String),
// }
//
// impl GruError {
//     pub fn actor_already_exists<A>() -> Self
//     where
//         A: Minion,
//     {
//         Self::ActorAlreadyExists(type_name::<A>().to_string())
//     }
// }

#[derive(Clone, Default)]
pub struct Gru {
    pub(crate) actors: Arc<RwLock<HashMap<TypeId, Box<dyn Any>>>>,
    pub(crate) context: Arc<RwLock<HashMap<TypeId, Box<dyn Any>>>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Address<M: Messageable> {
    pub tx: mpsc::Sender<Packet<M>>,
    pub name: String,
}

impl Address {
    pub fn new<T: Hash>(id: T, name: impl Into<String>) -> Self {
        let mut hasher = AHasher::default();
        id.hash(&mut hasher);
        Self {
            id: hasher.finish(),
            name: name.into(),
        }
    }

    pub fn from_type<T: 'static>() -> Self {
        let type_id = TypeId::of::<T>();
        let type_name = type_name::<T>().to_string();
        Address::new(type_id, type_name)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address {{ id: {}, name: {} }}", self.id, self.name)
    }
}

impl Hash for Address {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut hasher = AHasher::default();
        self.id.hash(&mut hasher);
        state.write_u64(hasher.finish());
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

impl Gru {
    pub fn new() -> Self {
        Default::default()
    }

    pub(crate) fn get_actor<A>(&self) -> Option<MinionInstance<A>>
    where
        A: Minion + Clone,
        <A as Minion>::Msg: std::clone::Clone,
    {
        self.actors
            .read()
            .expect("Failed to acquire read lock")
            .get(&TypeId::of::<A>())
            .and_then(|actor| actor.downcast_ref::<MinionInstance<A>>())
            .cloned()
    }

    pub fn spawn<A>(&self, actor: impl Into<SpawnableActor<A>>) -> Result<(), GruError>
    where
        A: Minion + Clone,
        <A as Minion>::Msg: std::clone::Clone,
    {
        let spawnable_actor: SpawnableActor<A> = actor.into();
        let id = TypeId::of::<A>();
        if self
            .actors
            .read()
            .expect("Failed to acquire read lock")
            .get(&id)
            .is_some()
        {
            Err(GruError::actor_already_exists::<A>())
        } else {
            let (handle, instance) = minion::create_actor::<A>(
                &spawnable_actor.actor,
                spawnable_actor.buffer_size,
                spawnable_actor.service_buffer_size,
            );
            spawnable_actor.spawn(
                &self,
                handle,
                instance.clone(),
                spawnable_actor.actor.clone(),
            );
            self.actors
                .write()
                .expect("Failed to acquire write lock")
                .insert(id, Box::new(instance));
            Ok(())
        }
    }

    // pub fn with_actor<A: Actor, R, F: FnOnce(&ActorInstance) -> R>(
    //     &self,
    //     f: F,
    // ) -> Result<R, OxFrameError> {
    //     let read_guard = self.inner.actors.read().unwrap();
    //     match read_guard.get(&TypeId::of::<A::Message>()) {
    //         Some(actor_instance) => Ok(f(actor_instance)),
    //         None => Err(OxFrameError::ActorNotExists(
    //             type_name::<A::Message>().to_string(),
    //         )),
    //     }
    // }
    //
    // pub fn terminate_actor<A: Actor>(&self) -> Result<(), OxFrameError> {
    //     if let Some(actor) = self
    //         .inner
    //         .actors
    //         .read()
    //         .unwrap()
    //         .get(&TypeId::of::<A::Message>())
    //     {
    //         actor.stop();
    //         Ok(())
    //     } else {
    //         Err(OxFrameError::ActorNotExists(
    //             type_name::<A::Message>().to_string(),
    //         ))
    //     }
    // }
    //
    // pub fn restart_actor<A: Actor>(&self) -> Result<(), OxFrameError> {
    //     if let Some(actor) = self
    //         .inner
    //         .actors
    //         .read()
    //         .unwrap()
    //         .get(&TypeId::of::<A::Message>())
    //     {
    //         actor.stop();
    //         let factory = (actor.factory)();
    //         if let Ok(actor) = factory.downcast::<A>() {
    //             self.spawn(*actor)?;
    //         }
    //         Ok(())
    //     } else {
    //         Err(OxFrameError::ActorNotExists(
    //             type_name::<A::Message>().to_string(),
    //         ))
    //     }
    // }
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

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone)]
        pub struct TestMessage {
            i: i32,
        }

        impl Messageable for TestMessage {
            type Response = i32;
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Minion for TestActor {
            type Msg = TestMessage;
            type State = ();
            async fn handle_message(&self) -> <<Self as Minion>::Msg as Messageable>::Response {
                println!("TestActor::handle_message");
                0
            }
        }

        let gru = Gru::new();
        // gru.provide_context(0_i32);

        gru.spawn(TestActor { i: 0 }).unwrap();
        gru.send::<TestActor>(TestMessage { i: 0 }).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
