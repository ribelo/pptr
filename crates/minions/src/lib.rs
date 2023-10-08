#![feature(lazy_cell)]
#![feature(downcast_unchecked)]

use minion::LifecycleStatus;
use thiserror::Error;

pub mod address;
pub mod context;
pub mod gru;
pub mod message;
pub mod minion;

#[derive(Debug, Error)]
pub enum MinionsError {
    #[error("Minion already exists: {0}")]
    MinionAlreadyExists(String),
    #[error("Minion does not exist: {0}")]
    MinionDoesNotExist(String),
    #[error("Minion cannot handle message. Status: {0}")]
    MinionCannotHandleMessage(LifecycleStatus),
    #[error("Timed out waiting for response from actor.")]
    MessageResponseTimeout,
    #[error("Error sending message.")]
    MessageSendError,
    #[error("Error receiving message.")]
    MessageReceiveError,
    #[error("Error receiving response from actor.")]
    MessageResponseReceiveError,
}
