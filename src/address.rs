use std::fmt;

use crate::{
    message::{Message, Postman, ServiceCommand, ServicePostman},
    minion::{Handler, Minion},
    Id, MinionsError,
};

pub struct Address<A>
where
    A: Minion,
{
    pub id: Id,
    pub name: String,
    pub(crate) tx: Postman<A>,
    pub(crate) command_tx: ServicePostman,
}

impl<A> Clone for Address<A>
where
    A: Minion,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            tx: self.tx.clone(),
            command_tx: self.command_tx.clone(),
        }
    }
}

impl<A: Minion> PartialEq for Address<A> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<A: Minion> Eq for Address<A> {}

impl<A: Minion> PartialOrd for Address<A> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl<A> Address<A>
where
    A: Minion,
{
    pub async fn send<M>(&self, message: impl Into<M>) -> Result<(), MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        self.tx.send(message.into()).await
    }
    pub async fn ask<M>(&self, message: impl Into<M>) -> Result<A::Response, MinionsError>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        self.tx.send_and_await_response(message.into()).await
    }
}

impl<A: Minion> fmt::Display for Address<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address {{ id: {}, name: {} }}", self.id, self.name)
    }
}

impl<A: Minion> fmt::Debug for Address<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Address")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}
