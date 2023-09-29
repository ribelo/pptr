use std::{
    any::{Any, TypeId},
    fmt::Debug,
    future::Future,
    sync::Arc,
};

use async_trait::async_trait;
use dashmap::DashMap;
use thiserror::Error;

pub trait Message: Debug + Send + Sync + 'static {}

#[async_trait]
pub trait Actor: Send + 'static {
    async fn handle(&mut self, ox: OxFrame, msg: Box<dyn Message>);
}

struct ActorHandle {
    rx: tokio::sync::mpsc::UnboundedReceiver<Box<dyn Message>>,
}

async fn run_actor<S>(mut handle: ActorHandle, mut actor: impl Actor, ox: OxFrame)
where
    S: Send + 'static,
{
    println!("run_actor");
    while let Some(msg) = handle.rx.recv().await {
        println!("got msg: {:?}", msg);
        actor.handle(ox.clone(), msg).await;
    }
}

#[derive(Clone)]
pub struct OxFrame {
    inner: Arc<OxFrameInner>,
}

struct OxFrameInner {
    txs: DashMap<TypeId, tokio::sync::mpsc::UnboundedSender<Box<dyn Message>>>,
    context: DashMap<TypeId, Box<dyn Any + Send + Sync + 'static>>,
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
                txs: DashMap::new(),
                context: DashMap::new(),
            }),
        }
    }

    pub fn spawn<M: Message>(&self, actor: impl Actor) -> Result<(), OxFrameError> {
        if let Some(_tx) = self.inner.txs.get(&TypeId::of::<M>()) {
            Err(OxFrameError::ActorAlreadySpawned)
        } else {
            let id = TypeId::of::<M>();
            println!("spawn actor: {:?}", id);
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Box<dyn Message>>();
            let handler = ActorHandle { rx };
            tokio::spawn(run_actor::<M>(handler, actor, self.clone()));
            println!("spawned actor: {:?}", id);
            self.inner.txs.insert(id, tx);
            Ok(())
        }
    }

    pub fn send<M: Message>(&self, msg: M) {
        if let Some(tx) = self.inner.txs.get(&TypeId::of::<M>()) {
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
        pub struct TestMessage {};

        impl Message for TestMessage {};

        #[derive(Debug)]
        pub struct TestActor {
            i: i32,
        }

        #[async_trait]
        impl Actor for TestActor {
            async fn handle(&mut self, ox: OxFrame, msg: Box<dyn Message>) {
                self.i += 1;
                println!("TestActor::handle {}", self.i);
                ox.send(TestMessage {});
            }
        }

        let ox_frame = OxFrame::new();

        ox_frame.spawn::<TestMessage>(TestActor { i: 0 }).unwrap();
        ox_frame.send(TestMessage {});
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
