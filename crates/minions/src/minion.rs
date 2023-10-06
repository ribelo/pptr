use std::{
    any::{type_name, Any, TypeId},
    fmt::{self, Debug},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use async_trait::async_trait;

use crate::message::Messageable;
//
// static DEFAULT_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
// static DEFAULT_SERVICE_BUFFER_SIZE: OnceLock<usize> = OnceLock::new();
//
// use ahash::AHasher;
// use async_trait::async_trait;
// use chrono::Utc;
// use minions_derive::LogError;
// use thiserror::Error;
// use tokio::sync::oneshot;
// use tracing::{event, Level};
//
// use crate::{
//     gru::{Gru, GruError},
//     message::{
//         Mailbox, Message, Messageable, Packet, Postman, ServiceMailbox, ServiceMessage,
//         ServicePostman,
//     },
// };
//
// pub(crate) type Sendable = dyn Send + Sync;
// pub(crate) type ActorResponse = oneshot::Sender<Box<Sendable>>;
//
#[derive(Debug, Error, LogError)]
pub enum MinionError {
    #[error("Transition to {step} failed. Reason: {reason:?}. Error Code: {error_code:?}. Timestamp: {timestamp:?}")]
    TransitionError {
        step: LifecycleStatus,
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<chrono::DateTime<Utc>>,
    },
    #[error("Failed to handle message: {message:?}. Reason: {reason:?}. Error Code: {error_code:?}. Timestamp: {timestamp:?}")]
    MessageError {
        message: Option<String>,
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<chrono::DateTime<Utc>>,
    },
    #[error("Failed to handle service message: {message:?}. Reason: {reason:?}. Error Code: {error_code:?}. Timestamp: {timestamp:?}")]
    ServiceMessageError {
        message: Option<String>,
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<chrono::DateTime<Utc>>,
    },
}

pub type TransitionResult = Result<TransitionStatus, MinionError>;

#[derive(Debug, strum::Display)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Inactive,
    Paused,
    Reloading,
    Failed,
}

#[derive(Debug)]
pub enum TransitionStatus {
    Success,
    Failed { strategy: Option<RecoveryStrategy> },
}

#[derive(Debug)]
pub enum RecoveryStrategy {
    RestartActor {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    StopActor {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    Retry {
        reason: Option<String>,
        max_retries: usize,
        delay: Option<std::time::Duration>,
        on_max_retries_exceeded: Box<RecoveryStrategy>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
    Panic {
        reason: Option<String>,
        error_code: Option<i32>,
        timestamp: Option<std::time::SystemTime>,
    },
}

#[async_trait]
pub trait Minion: Send + Sync + 'static {
    type Msg: Messageable;

    async fn handle_message(
        &self,
        // gru: &Gru,
        // minion: &mut MinionInstance<T>,
        // msg: Envelope<Self::Msg>,
    ) -> Result<<Self::Msg as Messageable>::Response, MinionError>;
}
//
// pub(crate) struct MinionHandle<A>
// where
//     A: Minion + Clone,
// {
//     pub address: Address,
//     pub status: Arc<Mutex<LifecycleStatus>>,
//     pub(crate) rx: Mailbox<A::Msg>,
//     // pub(crate) service_rx: ServiceMailbox,
// }
//
// #[derive(Debug, Clone)]
// pub(crate) struct MinionInstance<A>
// where
//     A: Minion + Clone,
// {
//     pub address: Address,
//     pub state: A::State,
//     pub status: Arc<Mutex<LifecycleStatus>>,
//     pub(crate) tx: Postman<A::Msg>,
//     // pub(crate) service_tx: ServicePostman,
// }
//
// pub(crate) async fn run_actor_loop<A>(
//     mut handle: MinionHandle<A>,
//     mut actor: impl Minion<Msg = A::Msg>,
//     gru: Gru,
// ) where
//     A: Minion + Clone,
// {
//     loop {
//         tokio::select! {
//             Some(Packet {message, reply_address}) = handle.rx.recv() => {
//                 handle.address = envelope.sender;
//                 let response = actor.handle_message().await;
//                 // if let Some(responder) = reply_address {
//                 //     reply_address.send(response).expect("Cannot send response");
//                 // }
//             }
//         }
//     }
// }
//
// pub(crate) fn create_actor<A>(
//     actor: &A,
//     buffer_size: usize,
//     service_buffer_size: usize,
// ) -> (MinionHandle<A>, MinionInstance<A>)
// where
//     A: Minion + Clone,
//     <A as Minion>::Msg: Clone,
// {
//     let (tx, rx) = tokio::sync::mpsc::channel::<Packet<A::Msg>>(buffer_size);
//     // let (service_tx, service_rx) =
//     //     tokio::sync::mpsc::channel::<ServiceMessage>(service_buffer_size);
//
//     let address = Address::from_type::<A>();
//     let status = Arc::new(Mutex::new(LifecycleStatus::Activating));
//     let handler = MinionHandle {
//         address: address.clone(),
//         status: status.clone(),
//         rx: Mailbox::new(rx),
//         // service_rx: ServiceMailbox::new(service_rx), // ... reszta pól
//     };
//     let instance = MinionInstance {
//         address,
//         state: Default::default(),
//         status,
//         tx: Postman::new(tx),
//         // service_tx: ServicePostman::new(service_tx),
//     };
//     (handler, instance)
// }
//
// #[derive(Debug)]
// pub(crate) struct SpawnableActor<A: Minion> {
//     pub(crate) actor: A,
//     pub(crate) buffer_size: usize,
//     pub(crate) service_buffer_size: usize,
// }
//
// impl<A> SpawnableActor<A>
// where
//     A: Minion + Clone,
//     <A as Minion>::Msg: Clone,
// {
//     pub fn spawn(
//         &self,
//         gru: &Gru,
//         handle: MinionHandle<A>,
//         instance: MinionInstance<A>,
//         actor: impl Minion<Msg = A::Msg>,
//     ) -> Result<(), GruError> {
//         let id = TypeId::of::<A>();
//         if gru
//             .actors
//             .read()
//             .expect("Failed to acquire read lock")
//             .get(&id)
//             .is_some()
//         {
//             Err(GruError::actor_already_exists::<A>())
//         } else {
//             tokio::spawn(run_actor_loop(handle, actor, gru.clone()));
//             gru.actors
//                 .write()
//                 .expect("Failed to acquire write lock")
//                 .insert(id, Box::new(instance));
//             Ok(())
//         }
//     }
// }
//
// #[derive(Debug)]
// pub struct ActorBuilder<A: Minion + Clone> {
//     actor_type: A,
//     buffer_size: Option<usize>,
//     service_buffer_size: Option<usize>,
// }
//
// impl<A: Minion + Clone> ActorBuilder<A> {
//     pub fn new(actor_type: A) -> Self {
//         Self {
//             actor_type,
//             buffer_size: None,
//             service_buffer_size: None,
//         }
//     }
//
//     pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
//         self.buffer_size = Some(buffer_size);
//         self
//     }
//
//     pub fn with_service_buffer_size(mut self, service_buffer_size: usize) -> Self {
//         self.service_buffer_size = Some(service_buffer_size);
//         self
//     }
//
//     pub fn build(self) -> SpawnableActor<A> {
//         SpawnableActor {
//             actor: self.actor_type,
//             buffer_size: self
//                 .buffer_size
//                 .unwrap_or_else(|| *DEFAULT_BUFFER_SIZE.get_or_init(|| 1024)),
//             service_buffer_size: self
//                 .service_buffer_size
//                 .unwrap_or_else(|| *DEFAULT_SERVICE_BUFFER_SIZE.get_or_init(|| 1)),
//         }
//     }
// }
//
// impl<A: Minion + Clone> From<A> for SpawnableActor<A> {
//     fn from(value: A) -> Self {
//         ActorBuilder::new(value).build()
//     }
// }
