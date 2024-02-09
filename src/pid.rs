use std::{
    any::TypeId,
    fmt,
    hash::{Hash, Hasher},
};

use rustc_hash::FxHasher;

use crate::puppet::Lifecycle;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id(u64);

impl Id {
    #[must_use]
    pub fn new<T>() -> Self
    where
        T: 'static,
    {
        let type_id = TypeId::of::<T>();
        let mut hasher = FxHasher::default();
        type_id.hash(&mut hasher);
        Self(hasher.finish())
    }

    #[must_use]
    pub fn to_pid<P>(&self) -> Pid
    where
        P: Lifecycle,
    {
        Pid::new::<P>()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::hash::Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

#[derive(Clone, Copy, Eq)]
pub struct Pid {
    pub(crate) id: Id,
    pub(crate) name_fn: fn() -> String,
}

impl PartialEq for Pid {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for Pid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl Pid {
    #[must_use]
    pub fn new<P>() -> Self
    where
        P: Lifecycle,
    {
        let id = Id::new::<P>();
        Self {
            id,
            name_fn: Self::_name::<P>,
        }
    }

    #[must_use]
    pub fn to_id(&self) -> Id {
        self.id
    }

    fn _name<T>() -> String
    where
        T: 'static,
    {
        std::any::type_name::<T>().to_owned()
    }

    #[must_use]
    pub fn name(&self) -> String {
        (self.name_fn)()
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Debug for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Id")
            .field("id", &self.id)
            .field("name", &self.name())
            .finish_non_exhaustive()
    }
}

impl std::hash::Hash for Pid {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.id.0);
    }
}

impl From<Pid> for String {
    fn from(value: Pid) -> Self {
        value.name()
    }
}
