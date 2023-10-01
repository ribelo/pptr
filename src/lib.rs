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

pub trait Message: AsAny + Debug + Send + Sync + 'static {
    type Response: Send;
}

#[async_trait]
pub trait Actor: AsAny + Default + Clone + Send + Sync + 'static {
    type Msg: Message;
    async fn handle_message(&mut self, ox: OxFrame, msg: &Self::Msg);
    // async fn handle_message(&mut self, ox: OxFrame, msg: &Self::Message) -> Box<dyn Any>;
    fn create_instance_factory(&self) -> Arc<dyn Fn() -> Box<dyn Any> + Send + Sync> {
        Arc::new(|| Box::<Self>::default())
    }
}

#[async_trait]
pub trait Dispatcher {
    async fn send<A>(&self, msg: A::Msg) -> Result<(), OxFrameError>
    where
        A: Actor + Dispatcher;

    // async fn ask<A, R>(&self, msg: A::Msg) ->
    // where
    //     A: Actor,
    //     A::Msg: Message + Clone,
    //     R: Send;
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

struct ActorHandle<A>
where
    A: Actor,
{
    rx: tokio::sync::mpsc::Receiver<
        Box<dyn Message<Response = <<A as Actor>::Msg as Message>::Response> + Send>,
    >,
    stop_signal_rx: tokio::sync::mpsc::Receiver<()>,
}

async fn run_actor_loop<A>(
    mut handle: ActorHandle<A>,
    mut actor: impl Actor<Msg = A::Msg>,
    ox: OxFrame,
) where
    A: Actor,
{
    loop {
        tokio::select! {
            _ = handle.stop_signal_rx.recv() => {
                return;
            }
            Some(msg) = handle.rx.recv() => {
                match (*msg).as_any().downcast_ref::<A::Msg>() {
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
pub struct ActorInstance<A>
where
    A: Actor,
{
    id: TypeId,
    factory: Arc<dyn Fn() -> Box<dyn Any> + Send + Sync>,
    tx: tokio::sync::mpsc::Sender<
        Box<dyn Message<Response = <<A as Actor>::Msg as Message>::Response> + Send>,
    >,
    stop_signal_tx: tokio::sync::mpsc::Sender<()>,
}

impl<A> std::fmt::Debug for ActorInstance<A>
where
    A: Actor,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorInstance")
            .field("id", &self.id)
            .field("factory", &"factory field hidden") // Custom message for the factory field
            .field("tx", &self.tx)
            .field("stop_signal_tx", &self.stop_signal_tx)
            .finish()
    }
}

#[async_trait]
impl<A> Dispatcher for ActorInstance<A>
where
    A: Actor,
{
    async fn send<T>(&self, msg: T::Msg) -> Result<(), OxFrameError>
    where
        T: Actor + Dispatcher,
    {
        (&self).send::<T>(msg).await
    }
}

#[async_trait]
impl<T: Actor> Dispatcher for &ActorInstance<T> {
    async fn send<A>(&self, msg: A::Msg) -> Result<(), OxFrameError>
    where
        A: Actor,
        A::Msg: Message<Response = <A::Msg as Message>::Response> + Send + 'static, // Dodano ograniczenie tutaj
    {
        // Zrzutowano msg na oczekiwany typ
        let msg =
            Box::new(msg) as Box<dyn Message<Response = <A::Msg as Message>::Response> + Send>;

        if self.tx.send(msg).await.is_err() {
            Err(OxFrameError::SendError)
        } else {
            Ok(())
        }
    }
}

fn create_actor<A>(actor: &A) -> (ActorHandle<A>, ActorInstance<A>)
where
    A: Actor,
{
    let actor_id = TypeId::of::<A>();

    let factory = actor.create_instance_factory();
    let (tx, rx) = tokio::sync::mpsc::channel::<
        Box<dyn Message<Response = <<A as Actor>::Msg as Message>::Response> + Send>,
    >(16);
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
    actors: RwLock<anymap::Map<dyn Any + Send + Sync>>,
    context: DashMap<TypeId, Box<dyn Any + Send + Sync>>,
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

#[async_trait]
impl Dispatcher for OxFrame {
    async fn send<A>(&self, msg: A::Msg) -> Result<(), OxFrameError>
    where
        A: Actor + Dispatcher,
    {
        if let Some(actor) = self.get_actor::<A>() {
            actor.send(msg).await
        } else {
            Err(OxFrameError::ActorNotExists(type_name::<A>().to_string()))
        }
    }
}

impl OxFrame {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_actor<A: Actor>(&self) -> Option<A> {
        self.inner.actors.read().unwrap().get::<A>().cloned()
    }

    pub fn spawn<A: Actor>(&self, actor: A) -> Result<(), OxFrameError> {
        if self.get_actor::<A>().is_some() {
            Err(OxFrameError::ActorAlreadySpawned(
                type_name::<A::Msg>().to_string(),
            ))
        } else {
            let (handle, instance) = create_actor(&actor);
            tokio::spawn(run_actor_loop(handle, actor, self.clone()));
            self.inner.actors.write().unwrap().insert(instance);
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

        impl Message for TestMessage {
            type Response = ();
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Actor for TestActor {
            type Msg = TestMessage;
            async fn handle_message(&mut self, ox: OxFrame, msg: &TestMessage) {
                ox.with_context(|i: &i32| println!("ii: {}", i));
                ox.with_context_mut(|i: &mut i32| *i += 1);
                ox.send(TestMessage { i: self.i });
            }
        }

        let ox_frame = OxFrame::new();
        ox_frame.provide_context(0_i32);

        ox_frame.spawn(TestActor { i: 0 }).unwrap();
        ox_frame.send::<TestActor>(TestMessage { i: 0 });
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
