use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, RwLock},
};

use anymap;
use async_trait::async_trait;
use dashmap::DashMap;
use thiserror::Error;

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait Message: AsAny + Debug + Send + Sync + 'static {}

#[async_trait]
pub trait Actor: AsAny + Default + Send + 'static {
    type Message: Message;
    async fn handle_message(&mut self, ox: OxFrame, msg: &Self::Message);
    // async fn handle_message(&mut self, ox: OxFrame, msg: &Self::Message) -> Box<dyn Any>;
    fn create_instance_factory(&self) -> Arc<dyn Fn() -> Box<dyn Any> + Send + Sync> {
        Arc::new(|| Box::<Self>::default())
    }
}

pub trait Routing {
    fn match_actors(&self, msg_id: TypeId) -> Option<Vec<ActorInstance>>;
    fn get_actor<A: Actor>(&self) -> Option<ActorInstance>;
    fn get_actor_by_id(&self, id: &TypeId) -> Option<ActorInstance>;
}

pub trait Dispatcher {
    fn send<A>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone;

    fn ask<A, R>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone,
        R: Send;

    fn broadcast<M>(&self, msg: M)
    where
        M: Message + Clone;
}

#[async_trait]
pub trait Context {
    fn provide_context<T>(&self, context: T)
    where
        T: Any + Clone + Send + Sync;

    fn get_context<T>(&self) -> Option<T>
    where
        T: Any + Clone + Send + Sync;

    fn with_context<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&T) -> R + Send;

    fn with_context_mut<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&mut T) -> R + Send;
}

struct ActorHandle {
    rx: tokio::sync::mpsc::UnboundedReceiver<Box<dyn Message>>,
    stop_signal_rx: tokio::sync::mpsc::Receiver<()>,
}

async fn run_actor_loop<M: Message>(
    mut handle: ActorHandle,
    mut actor: impl Actor<Message = M>,
    ox: OxFrame,
) {
    loop {
        tokio::select! {
            _ = handle.stop_signal_rx.recv() => {
                return;
            }
            Some(msg) = handle.rx.recv() => {
                match (*msg).as_any().downcast_ref::<M>() {
                    Some(msg) => {
                        actor.handle_message(ox.clone(), msg).await;
                    }
                    None => {
                        println!("unknown message type, not {:#?}, ", msg);
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ActorInstance {
    id: TypeId,
    factory: Arc<dyn Fn() -> Box<dyn Any> + Send + Sync>,
    tx: tokio::sync::mpsc::UnboundedSender<Box<dyn Message>>,
    stop_signal_tx: tokio::sync::mpsc::Sender<()>,
}

impl std::fmt::Debug for ActorInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorInstance")
            .field("id", &self.id)
            .field("factory", &"factory field hidden") // Custom message for the factory field
            .field("tx", &self.tx)
            .field("stop_signal_tx", &self.stop_signal_tx)
            .finish()
    }
}

impl ActorInstance {
    fn send<M>(&self, msg: M)
    where
        M: Message,
    {
        let msg = Box::new(msg) as Box<dyn Message>;

        self.tx
            .send(msg)
            .map_err(|_| OxFrameError::SendError)
            .unwrap();
    }

    pub fn stop(&self) {
        self.stop_signal_tx.try_send(()).unwrap();
    }
}

fn create_actor<A: Actor>(actor: &A) -> (ActorHandle, ActorInstance) {
    let actor_id = TypeId::of::<A>();

    let factory = actor.create_instance_factory();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Box<dyn Message>>();
    let (stop_signal_tx, stop_signal_rx) = tokio::sync::mpsc::channel::<()>(1);
    let handler = ActorHandle { rx, stop_signal_rx };
    let instance = ActorInstance {
        id: actor_id,
        tx,
        stop_signal_tx,
        factory,
    };
    (handler, instance)
}

#[derive(Clone, Default)]
pub struct OxFrame {
    inner: Arc<OxFrameInner>,
}

#[derive(Default)]
struct OxFrameInner {
    routing_table: OxRoutingTable,
    context: DashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

#[derive(Default)]
struct OxRoutingTable {
    actors: RwLock<HashMap<TypeId, ActorInstance>>,
    // relationships between <message and actors>
    messages: RwLock<HashMap<TypeId, Vec<TypeId>>>,
}

impl OxRoutingTable {
    fn register_actor(&self, instance: ActorInstance, actor_id: TypeId, msg_id: TypeId) {
        let mut actors_write_guard = self.actors.write().unwrap();
        actors_write_guard.insert(actor_id, instance);

        let mut messages_write_guard = self.messages.write().unwrap();
        messages_write_guard
            .entry(msg_id)
            .or_default()
            .push(actor_id);
    }
}

impl Routing for OxRoutingTable {
    fn match_actors(&self, msg_id: TypeId) -> Option<Vec<ActorInstance>> {
        let actors_read_guard = self.actors.read().unwrap();
        let messages_read_guard = self.messages.read().unwrap();
        messages_read_guard.get(&msg_id).map(|actor_ids| {
            actor_ids
                .iter()
                .filter_map(|actor_id| actors_read_guard.get(actor_id))
                .cloned()
                .collect::<Vec<_>>()
        })
    }

    fn get_actor<A: Actor>(&self) -> Option<ActorInstance> {
        self.get_actor_by_id(&TypeId::of::<A>())
    }

    fn get_actor_by_id(&self, id: &TypeId) -> Option<ActorInstance> {
        self.actors.read().unwrap().get(id).cloned()
    }
}

impl Dispatcher for OxRoutingTable {
    fn send<A>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone,
    {
        if let Some(actor) = self.get_actor::<A>() {
            actor.send(msg);
        }
    }

    fn ask<A, R>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone,
    {
        if let Some(actor) = self.get_actor::<A>() {
            actor.send(msg);
        }
    }

    fn broadcast<M>(&self, msg: M)
    where
        M: Message + Clone,
    {
        if let Some(actors) = self.match_actors(TypeId::of::<M>()) {
            for actor in actors {
                actor.send(msg.clone());
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum OxFrameError {
    #[error("actor already spawned {0}")]
    ActorAlreadySpawned(String),
    #[error("actor not exists {0}")]
    ActorNotExists(String),
    #[error("type mismatch {0}")]
    TypeMismatch(String),
    #[error("context not exists {0}")]
    ContextNotExists(String),
    #[error("send error")]
    SendError,
    #[error("receive error")]
    ReceiveError,
}

impl Routing for OxFrame {
    fn match_actors(&self, msg_id: TypeId) -> Option<Vec<ActorInstance>> {
        self.inner.routing_table.match_actors(msg_id)
    }

    fn get_actor<A: Actor>(&self) -> Option<ActorInstance> {
        self.inner.routing_table.get_actor::<A>()
    }

    fn get_actor_by_id(&self, id: &TypeId) -> Option<ActorInstance> {
        self.inner.routing_table.get_actor_by_id(id)
    }
}

impl Dispatcher for OxFrame {
    fn send<A>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone,
    {
        self.inner.routing_table.send::<A>(msg);
    }

    fn ask<A, R>(&self, msg: A::Message)
    where
        A: Actor,
        A::Message: Message + Clone,
    {
        self.inner.routing_table.send::<A>(msg);
    }

    fn broadcast<M>(&self, msg: M)
    where
        M: Message + Clone,
    {
        self.inner.routing_table.broadcast(msg)
    }
}

impl OxFrame {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn spawn<A: Actor>(&self, actor: A) -> Result<(), OxFrameError> {
        let actor_id = TypeId::of::<A>();
        if self
            .inner
            .routing_table
            .get_actor_by_id(&actor_id)
            .is_some()
        {
            Err(OxFrameError::ActorAlreadySpawned(
                type_name::<A::Message>().to_string(),
            ))
        } else {
            let msg_id = TypeId::of::<A::Message>();
            let (handle, instance) = create_actor(&actor);
            dbg!(&msg_id, &actor_id);
            self.inner
                .routing_table
                .register_actor(instance, actor_id, msg_id);
            tokio::spawn(run_actor_loop(handle, actor, self.clone()));
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

#[async_trait]
impl Context for OxFrame {
    fn provide_context<T: Any + Clone + Send + Sync>(&self, context: T) {
        self.inner
            .context
            .insert(TypeId::of::<T>(), Box::new(context));
    }

    fn get_context<T: Any + Clone + Send + Sync>(&self) -> Option<T> {
        self.inner
            .context
            .get(&TypeId::of::<T>())
            .and_then(|ref_entry| ref_entry.downcast_ref::<T>().cloned())
    }

    fn with_context<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&T) -> R + Send,
    {
        match self.inner.context.get(&TypeId::of::<T>()) {
            Some(context) => {
                let typed_context = context
                    .downcast_ref::<T>()
                    .ok_or(OxFrameError::TypeMismatch(type_name::<T>().to_string()))?;
                Ok(f(typed_context))
            }
            None => Err(OxFrameError::ContextNotExists(type_name::<T>().to_string())),
        }
    }

    fn with_context_mut<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&mut T) -> R + Send,
    {
        match self.inner.context.get_mut(&TypeId::of::<T>()) {
            Some(mut context) => {
                let typed_context = context
                    .downcast_mut::<T>()
                    .ok_or(OxFrameError::TypeMismatch(type_name::<T>().to_string()))?;
                Ok(f(typed_context))
            }
            None => Err(OxFrameError::ContextNotExists(type_name::<T>().to_string())),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn anymap() {
        let mut m = anymap::Map::<dyn Any + Send + Sync>::new();
        m.insert(0);
        m.insert("sex");
        dbg!(m);
    }

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug, Clone)]
        pub struct TestMessage {
            i: i32,
        }

        impl Message for TestMessage {}

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Actor for TestActor {
            type Message = TestMessage;
            async fn handle_message(&mut self, ox: OxFrame, msg: &TestMessage) {
                ox.with_context(|i: &i32| println!("ii: {}", i));
                ox.with_context_mut(|i: &mut i32| *i += 1);
                ox.send::<TestActor>(TestMessage { i: self.i });
            }
        }

        let ox_frame = OxFrame::new();
        ox_frame.provide_context(0_i32);

        ox_frame.spawn(TestActor { i: 0 }).unwrap();
        ox_frame.send::<TestActor>(TestMessage { i: 0 });
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
