use async_trait::async_trait;

use crate::message::Message;
use crate::{Id, PuppeterError};

#[derive(Debug, Clone, strum::Display)]
pub enum ServiceCommand {
    InitiateStart { sender: Id },
    InitiateStop { sender: Id },
    RequestRestart { sender: Id },
    ForceTermination { sender: Id },
    ReportFailure { sender: Id, message: Option<String> },
}

impl Message for ServiceCommand {}

#[async_trait]
pub trait PuppetService {
    async fn handle_command(&mut self, command: ServiceCommand) -> Result<(), PuppeterError>;
}
