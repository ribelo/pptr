use std::{
    any::{type_name, TypeId},
    fmt,
    hash::Hasher,
    sync::Arc,
};

use ahash::AHasher;
use tokio::sync::mpsc;

use crate::{
    message::Packet,
    minion::{Minion, MinionId},
};

#[derive(Clone, Debug)]
pub struct Address<A>
where
    A: Minion,
{
    pub id: MinionId,
    pub name: Arc<String>,
    tx: mpsc::Sender<Packet<A::Msg>>,
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

impl<A: Minion> Address<A> {
    pub fn new(tx: mpsc::Sender<Packet<A::Msg>>) -> Self
    where
        A: Minion,
    {
        Self {
            id: TypeId::of::<A>().into(),
            name: Arc::new(type_name::<A>().to_string()),
            tx,
        }
    }
}

impl<A: Minion> fmt::Display for Address<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address {{ id: {}, name: {} }}", self.id, self.name)
    }
}
