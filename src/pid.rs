//! Provides unique identifiers for puppets and resources.
//!
//! This module defines two main types: `Id` and `Pid`.
//!
//! `Id` is an opaque type that wraps a `u64` value and provides a way to uniquely identify
//! and compare different entities in the system. It can be created using the `Id::new`
//! function, which generates a unique `Id` based on the `TypeId` of a given type `T`.
//!
//! `Pid` is similar to `Id` but includes an additional `name_fn` field that provides a way
//! to retrieve the name of the puppet type statically. It can be created using the
//! `Pid::new` function, which generates a unique `Pid` based on the `Id` of a puppet type
//! `P` and assigns the `_name` function as the `name_fn` field.
//!
//! # Examples
//!
//! ```
//! use pptr::prelude::*;
//!
//! # #[derive(Debug, Clone, Default)]
//! struct SomePuppet;
//! impl Puppet for SomePuppet {
//!     type Supervision = OneForAll;
//! }
//!
//! let pid = Pid::new::<SomePuppet>();
//! println!("Puppet ID: {}", pid);
//! println!("Puppet Name: {}", pid.name());
//! ```

use std::{
    any::TypeId,
    fmt,
    hash::{Hash, Hasher},
};

use rustc_hash::FxHasher;

use crate::puppet::Puppet;

/// A unique hashable ID used to identify puppets and resources.
///
/// `Id` is an opaque type that wraps a `u64` value. It provides a way to
/// uniquely identify and compare different entities in the system.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id(u64);

impl Id {
    /// Creates a new `Id` instance for the given type `T`.
    ///
    /// This function generates a unique `Id` based on the `TypeId` of `T`. It
    /// uses the `FxHasher` to hash the `TypeId` and create a deterministic `u64`
    /// value.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn new<T>() -> Self
    where
        T: 'static,
    {
        TypeId::of::<T>().into()
    }
}

impl From<TypeId> for Id {
    fn from(type_id: TypeId) -> Self {
        let mut hasher = FxHasher::default();
        type_id.hash(&mut hasher);
        Self(hasher.finish())
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

/// A unique hashable ID used to identify puppets.
///
/// `Pid` is similar to `Id` but includes an additional `name_fn` field that
/// provides a way to retrieve the name of the puppet type statically.
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
    /// Creates a new `Pid` instance for the given puppet type `P`.
    ///
    /// This function generates a unique `Pid` based on the `Id` of the puppet
    /// type `P` and assigns the `_name` function as the `name_fn` field.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn new<P>() -> Self
    where
        P: Puppet,
    {
        let id = Id::new::<P>();
        Self {
            id,
            name_fn: Self::_name::<P>,
        }
    }

    /// Converts the `Pid` to its corresponding `Id`.
    ///
    /// This function returns the `Id` value stored within the `Pid` instance.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn as_id(&self) -> Id {
        self.id
    }

    fn _name<T>() -> String
    where
        T: 'static,
    {
        std::any::type_name::<T>().to_owned()
    }

    /// Retrieves the name of the type `T`.
    ///
    /// This function uses `std::any::type_name` to get the name of the type `T`
    /// and returns it as an owned `String`.
    ///
    /// # Panics
    ///
    /// This function does not panic.
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

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    use super::*;

    #[derive(Clone, Default)]
    struct FirstPuppet;

    impl Puppet for FirstPuppet {
        type Supervision = OneToOne;
    }

    #[derive(Clone, Default)]
    struct SecondPuppet;

    impl Puppet for SecondPuppet {
        type Supervision = OneToOne;
    }

    #[test]
    fn test_id_new() {
        let id1 = Id::new::<i32>();
        let id2 = Id::new::<String>();
        assert_ne!(id1, id2);

        let id3 = Id::new::<i32>();
        assert_eq!(id1, id3);
    }

    #[test]
    fn test_pid_name() {
        let pid = Pid::new::<FirstPuppet>();
        assert!(pid.name().ends_with("FirstPuppet"));
    }

    #[test]
    fn test_pid_eq() {
        let pid1 = Pid::new::<FirstPuppet>();
        let pid2 = Pid::new::<FirstPuppet>();
        assert_eq!(pid1, pid2);

        let pid3 = Pid::new::<SecondPuppet>();
        assert_ne!(pid1, pid3);
    }

    #[test]
    fn test_pid_partial_ord() {
        let pid1 = Pid::new::<FirstPuppet>();
        let pid2 = Pid::new::<FirstPuppet>();
        assert_eq!(pid1.partial_cmp(&pid2), Some(std::cmp::Ordering::Equal));

        let pid3 = Pid::new::<SecondPuppet>();
        assert_eq!(
            pid1.partial_cmp(&pid3),
            pid1.as_id().partial_cmp(&pid3.as_id())
        );
    }

    #[test]
    fn test_pid_hash() {
        let pid1 = Pid::new::<FirstPuppet>();
        let pid2 = Pid::new::<FirstPuppet>();
        let mut hasher1 = std::collections::hash_map::DefaultHasher::new();
        let mut hasher2 = std::collections::hash_map::DefaultHasher::new();
        pid1.hash(&mut hasher1);
        pid2.hash(&mut hasher2);
        assert_eq!(hasher1.finish(), hasher2.finish());

        let pid3 = Pid::new::<SecondPuppet>();
        let mut hasher3 = std::collections::hash_map::DefaultHasher::new();
        pid3.hash(&mut hasher3);
        assert_ne!(hasher1.finish(), hasher3.finish());
    }

    #[test]
    fn test_pid_from_string() {
        let pid = Pid::new::<FirstPuppet>();
        let pid_string: String = pid.into();
        assert!(pid_string.ends_with("FirstPuppet"));
    }
}
