#![allow(dead_code)]
use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Mutex},
};

pub mod context;
use async_trait::async_trait;
use context::Context;
use thiserror::Error;

pub trait Message: Debug + Send + Sync {
    type Response: Debug + Send + Sync;
}

pub type Sendable = dyn Send + Sync;

pub type ActorResponse = tokio::sync::oneshot::Sender<Box<Sendable>>;

pub struct ChannelMessage<M>
where
    M: Message,
{
    pub message: M,
    pub responder: Option<tokio::sync::oneshot::Sender<M::Response>>,
}

#[derive(Debug)]
pub struct ActorReceiver<M>
where
    M: Message,
{
    rx: tokio::sync::mpsc::Receiver<ChannelMessage<M>>,
}

impl<M: Message> ActorReceiver<M> {
    pub fn new(rx: tokio::sync::mpsc::Receiver<ChannelMessage<M>>) -> Self {
        Self { rx }
    }
    pub async fn recv(&mut self) -> Option<ChannelMessage<M>> {
        self.rx.recv().await
    }
}

#[derive(Clone, Debug)]
pub struct ActorSender<M>
where
    M: Message,
{
    tx: tokio::sync::mpsc::Sender<ChannelMessage<M>>,
}

impl<M> ActorSender<M>
where
    M: Message,
{
    pub fn new(tx: tokio::sync::mpsc::Sender<ChannelMessage<M>>) -> Self {
        Self { tx }
    }
    pub async fn send(
        &self,
        message: M,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<ChannelMessage<M>>> {
        let channel_message = ChannelMessage {
            message,
            responder: None,
        };
        self.tx.send(channel_message).await
    }

    pub async fn send_and_await_response(
        &self,
        message: M,
    ) -> Result<M::Response, tokio::sync::oneshot::error::RecvError> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();
        let channel_message = ChannelMessage {
            message,
            responder: Some(res_tx),
        };
        let x = self.tx.send(channel_message).await;
        println!("x {:#?}", x);
        match res_rx.await {
            Ok(response_box) => Ok(response_box),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
pub trait Actor: Clone + Default + Send + Sync + 'static {
    type Msg: Message;
    async fn handle_message(
        &mut self,
        ox: OxFrame,
        msg: Self::Msg,
    ) -> <<Self as Actor>::Msg as Message>::Response;
    fn create_instance_factory(&self) -> Arc<dyn Fn() -> Box<dyn Any> + Send + Sync> {
        Arc::new(|| Box::<Self>::default())
    }
}

struct ActorHandle<A>
where
    A: Actor,
{
    id: TypeId,
    rx: ActorReceiver<A::Msg>,
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
            Some(ChannelMessage { message, responder }) = handle.rx.recv() => {
                let response = actor.handle_message(ox.clone(), message).await;
                if let Some(responder) = responder {
                    responder.send(response).unwrap();
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
    tx: ActorSender<A::Msg>,
    stop_signal_tx: tokio::sync::mpsc::Sender<()>,
}

impl<A> Debug for ActorInstance<A>
where
    A: Actor,
    <A as Actor>::Msg: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorInstance")
            .field("id", &self.id)
            .field("factory", &"factory")
            .field("tx", &"tx")
            .field("stop_signal_tx", &"stop_signal_tx")
            .finish()
    }
}

fn create_actor<A>(actor: &A) -> (ActorHandle<A>, ActorInstance<A>)
where
    A: Actor,
    <A as Actor>::Msg: Clone,
{
    let factory = actor.create_instance_factory();
    let (tx, rx) = tokio::sync::mpsc::channel::<ChannelMessage<A::Msg>>(16);
    let (stop_signal_tx, stop_signal_rx) = tokio::sync::mpsc::channel::<()>(1);
    let handler = ActorHandle {
        id: TypeId::of::<A>(),
        rx: ActorReceiver::new(rx),
        stop_signal_rx,
    };
    let instance = ActorInstance {
        id: TypeId::of::<A>(),
        tx: ActorSender::new(tx),
        stop_signal_tx,
        factory,
    };
    (handler, instance)
}

#[derive(Clone, Default)]
pub struct OxFrame {
    inner: Arc<Mutex<OxFrameInner>>,
}

#[derive(Default)]
struct OxFrameInner {
    // <actor type_id, actor instance>
    actors: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    context: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

#[derive(Error, Debug)]
pub enum OxFrameError {
    #[error("Actor already exists: {0}")]
    ActorAlreadyExists(String),
    #[error("Actor does not exist: {0}")]
    ActorDoesNotExist(String),
    #[error("Type mismatch: expected {0}")]
    TypeMismatch(String),
    #[error("Context not found for type: {0}")]
    ContextNotFound(String),
    #[error("Error sending message to actor: {0}")]
    MessageSendError(String),
    #[error("Error receiving message from actor: {0}")]
    MessageReceiveError(String),
    #[error("Timed out waiting for response from actor: {0}")]
    ActorResponseTimeout(String),
    #[error("Error receiving response from actor: {0}")]
    ResponseReceiveError(String),
}

impl OxFrame {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn send<A>(&self, message: A::Msg) -> Result<(), OxFrameError>
    where
        A: Actor,
        <A as Actor>::Msg: Clone,
    {
        if let Some(actor) = self.get_actor::<A>() {
            actor.tx.send(message).await.map_err(|_| {
                OxFrameError::MessageSendError(type_name::<ActorInstance<A>>().to_string())
            })
        } else {
            Err(OxFrameError::ActorDoesNotExist(
                type_name::<ActorInstance<A>>().to_string(),
            ))
        }
    }

    pub async fn ask<A>(
        &self,
        message: A::Msg,
    ) -> Result<<<A as Actor>::Msg as Message>::Response, OxFrameError>
    where
        A: Actor,
        <A as Actor>::Msg: std::clone::Clone,
    {
        if let Some(actor) = self.get_actor::<A>() {
            actor
                .tx
                .send_and_await_response(message)
                .await
                .map_err(|_| {
                    OxFrameError::ResponseReceiveError(type_name::<ActorInstance<A>>().to_string())
                })
        } else {
            Err(OxFrameError::ActorDoesNotExist(
                type_name::<ActorInstance<A>>().to_string(),
            ))
        }
    }

    pub async fn ask_with_timeout<A>(
        &self,
        message: A::Msg,
        timeout: std::time::Duration,
    ) -> Result<<<A as Actor>::Msg as Message>::Response, OxFrameError>
    where
        A: Actor,
        <A as Actor>::Msg: std::clone::Clone,
    {
        if let Some(actor) = self.get_actor::<A>() {
            tokio::time::timeout(timeout, actor.tx.send_and_await_response(message))
                .await
                .map_err(|_| {
                    OxFrameError::ActorResponseTimeout(type_name::<ActorInstance<A>>().to_string())
                })?
                .map_err(|_| {
                    OxFrameError::MessageReceiveError(type_name::<ActorInstance<A>>().to_string())
                })
        } else {
            Err(OxFrameError::ActorDoesNotExist(
                type_name::<ActorInstance<A>>().to_string(),
            ))
        }
    }

    pub fn get_actor<A>(&self) -> Option<ActorInstance<A>>
    where
        A: Actor,
        <A as Actor>::Msg: std::clone::Clone,
    {
        let inner = self.inner.lock().unwrap();
        inner
            .actors
            .get(&TypeId::of::<A>())
            .and_then(|actor| actor.downcast_ref::<ActorInstance<A>>())
            .cloned()
    }

    pub fn spawn<A>(&self, actor: A) -> Result<(), OxFrameError>
    where
        A: Actor,
        <A as Actor>::Msg: std::clone::Clone,
    {
        let id = TypeId::of::<A>();
        println!("try to spwan {:#?}", id);
        let mut inner = self.inner.lock().unwrap();
        if inner.actors.get(&id).is_some() {
            Err(OxFrameError::ActorAlreadyExists(
                type_name::<A::Msg>().to_string(),
            ))
        } else {
            let (handle, instance) = create_actor::<A>(&actor);
            tokio::spawn(run_actor_loop(handle, actor, self.clone()));
            inner.actors.insert(id, Box::new(instance));
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
            .lock()
            .unwrap()
            .context
            .insert(TypeId::of::<T>(), Box::new(context));
    }

    fn get_context<T: Any + Clone + Send + Sync>(&self) -> Option<T> {
        self.inner
            .lock()
            .unwrap()
            .context
            .get(&TypeId::of::<T>())
            .and_then(|ref_entry| ref_entry.downcast_ref::<T>().cloned())
    }

    fn with_context<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&T) -> R + Send,
    {
        let inner = self.inner.lock().unwrap();
        match inner.context.get(&TypeId::of::<T>()) {
            Some(context) => {
                let typed_context = context
                    .downcast_ref::<T>()
                    .ok_or(OxFrameError::TypeMismatch(type_name::<T>().to_string()))?;
                Ok(f(typed_context))
            }
            None => Err(OxFrameError::ContextNotFound(type_name::<T>().to_string())),
        }
    }

    fn with_context_mut<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&mut T) -> R + Send,
    {
        let mut inner = self.inner.lock().unwrap();
        match inner.context.get_mut(&TypeId::of::<T>()) {
            Some(context) => {
                let typed_context = context
                    .downcast_mut::<T>()
                    .ok_or(OxFrameError::TypeMismatch(type_name::<T>().to_string()))?;
                Ok(f(typed_context))
            }
            None => Err(OxFrameError::ContextNotFound(type_name::<T>().to_string())),
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
            type Response = i32;
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Actor for TestActor {
            type Msg = TestMessage;
            async fn handle_message(
                &mut self,
                ox: OxFrame,
                msg: TestMessage,
            ) -> <<Self as Actor>::Msg as Message>::Response {
                println!("TestActor1");
                println!("i: {}", self.i);
                self.i += 1;
                let r = ox.ask::<TestActor2>(TestMessage { i: 0 }).await.unwrap();
                self.i
            }
        }

        #[derive(Debug, Default, Clone)]
        pub struct TestActor2 {
            foo: String,
        }

        #[async_trait]
        impl Actor for TestActor2 {
            type Msg = TestMessage;
            async fn handle_message(
                &mut self,
                ox: OxFrame,
                msg: TestMessage,
            ) -> <<Self as Actor>::Msg as Message>::Response {
                println!("TestActor2");
                println!("i: {}", self.foo);
                self.foo += "bar";
                ox.send::<TestActor>(msg.clone()).await.unwrap();
                0
            }
        }

        let ox_frame = OxFrame::new();
        ox_frame.provide_context(0_i32);

        ox_frame.spawn(TestActor { i: 0 }).unwrap();
        ox_frame
            .spawn(TestActor2 {
                foo: "bar".to_string(),
            })
            .unwrap();
        ox_frame
            .ask::<TestActor>(TestMessage { i: 0 })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
