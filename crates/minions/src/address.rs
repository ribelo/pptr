use std::{
    any::{type_name, TypeId},
    fmt,
};

use crate::{
    gru::send,
    minion::{Minion, MinionId},
    MinionsError,
};

pub struct Address<A>
where
    A: Minion,
{
    pub id: MinionId,
    pub name: String,
    _phantom: std::marker::PhantomData<A>,
}

impl<A> Clone for Address<A>
where
    A: Minion,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            _phantom: std::marker::PhantomData::<A>,
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

impl<A: Minion> std::default::Default for Address<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Minion> Address<A> {
    pub fn new() -> Self
    where
        A: Minion,
    {
        Self {
            id: TypeId::of::<A>().into(),
            name: type_name::<A>().to_string(),
            _phantom: std::marker::PhantomData::<A>,
        }
    }

    pub async fn send(message: impl Into<A::Msg>) -> Result<(), MinionsError> {
        send::<A>(message.into()).await
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
