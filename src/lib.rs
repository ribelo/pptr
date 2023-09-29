use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    future::Future,
    sync::{Arc, RwLock},
};

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
pub trait Actor<M>: Send + 'static {
    async fn handle(&mut self, ox: OxFrame, msg: &M);
}

struct ActorHandle {
    rx: tokio::sync::mpsc::UnboundedReceiver<Box<dyn Message>>,
}

async fn run_actor<M: Message>(mut handle: ActorHandle, mut actor: impl Actor<M>, ox: OxFrame) {
    while let Some(msg) = handle.rx.recv().await {
        match (*msg).as_any().downcast_ref::<M>() {
            Some(msg) => {
                actor.handle(ox.clone(), msg).await;
            }
            None => {
                println!("unknown message type, not {:#?}, ", msg);
            }
        }
    }
}

#[derive(Clone)]
pub struct OxFrame {
    inner: Arc<OxFrameInner>,
}

struct OxFrameInner {
    txs: RwLock<HashMap<TypeId, tokio::sync::mpsc::UnboundedSender<Box<dyn Message>>>>,
    context: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync + 'static>>>,
}

impl Default for OxFrame {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Error, Debug)]
pub enum OxFrameError {
    #[error("actor already spawned")]
    ActorAlreadySpawned,
}

impl OxFrame {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(OxFrameInner {
                txs: RwLock::new(HashMap::new()),
                context: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn spawn<M: Message>(&self, actor: impl Actor<M>) -> Result<(), OxFrameError> {
        let id = TypeId::of::<M>();
        let mut write_guard = self.inner.txs.write().unwrap();

        if write_guard.contains_key(&id) {
            return Err(OxFrameError::ActorAlreadySpawned);
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Box<dyn Message>>();
        let handler = ActorHandle { rx };
        tokio::spawn(run_actor::<M>(handler, actor, self.clone()));
        write_guard.insert(id, tx);

        Ok(())
    }

    pub fn send<M: Message>(&self, msg: M) {
        if let Some(tx) = self.inner.txs.read().unwrap().get(&TypeId::of::<M>()) {
            tx.send(Box::new(msg)).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn it_works() {
        #[derive(Debug)]
        pub struct TestMessage {
            i: i32,
        };

        impl Message for TestMessage {};

        #[derive(Debug)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Actor<TestMessage> for TestActor {
            async fn handle(&mut self, ox: OxFrame, msg: &TestMessage) {
                self.i += 1;
                println!("TestActor::handle {}, msg i {}", self.i, msg.i);
                ox.send(TestMessage { i: self.i });
            }
        }

        let ox_frame = OxFrame::new();

        ox_frame.spawn::<TestMessage>(TestActor { i: 0 }).unwrap();
        ox_frame.send(TestMessage { i: 0 });
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
