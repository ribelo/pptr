use std::{
    any::TypeId,
    ops::{Deref, DerefMut},
};

use crate::{gru::Gru, magic_handler::ConstructFrom, Id};

#[derive(Debug)]
pub struct Res<T>(pub Option<T>);

impl<T> Deref for Res<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Res<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// impl<T: 'static + Clone + Send + Sync> ConstructFrom<Gru> for Res<T> {
//     fn construct(gru: &mut Gru) -> Option<Self>
//     where
//         Self: Sized,
//     {
//         let id: Id = TypeId::of::<T>().into();
//         gru.resources
//             .read()
//             .expect("Failed to acquire read lock")
//             .get(&id)
//             .map(|item| item.downcast::<T>().clone())
//             .map(Res)
//     }
// }
