use std::any::{Any, TypeId};

use crate::{gru, puppet::BoxedAny, Id};

// pub fn provide_state<T>(state: T) -> Option<T>
// where
//     T: Any + Clone + Send + Sync,
// {
//     let id: Id = TypeId::of::<T>().into();
//     gru()
//         .state
//         .write()
//         .expect("Failed to acquire write lock")
//         .insert(id, BoxedAny::new(state))
//         .map(|boxed_any| boxed_any.downcast_ref_unchecked::<T>().clone())
// }
//
// pub fn get_state<T>() -> Option<T>
// where
//     T: Any + Clone + Send + Sync,
// {
//     let id: Id = TypeId::of::<T>().into();
//     gru()
//         .state
//         .read()
//         .expect("Failed to acquire write lock")
//         .get(&id)
//         .map(|boxed_any| boxed_any.downcast_ref_unchecked::<T>())
//         .cloned()
// }
//
// pub fn with_state<T, R, F>(f: F) -> R
// where
//     T: Any + Clone + Send + Sync,
//     F: FnOnce(Option<&T>) -> R + Send,
// {
//     let id: Id = TypeId::of::<T>().into();
//     match gru()
//         .state
//         .read()
//         .expect("Failed to acquire read lock")
//         .get(&id)
//     {
//         Some(boxed_any) => {
//             let typed_context = boxed_any.downcast_ref_unchecked::<T>();
//             f(Some(typed_context))
//         }
//         None => f(None),
//     }
// }
//
// pub fn with_state_mut<T, R, F>(f: F) -> R
// where
//     T: Any + Clone + Send + Sync,
//     F: FnOnce(Option<&mut T>) -> R + Send,
// {
//     let id: Id = TypeId::of::<T>().into();
//     match gru()
//         .state
//         .write()
//         .expect("Failed to acquire read lock")
//         .get_mut(&id)
//     {
//         Some(boxed_any) => {
//             let mut typed_context = boxed_any.downcast_mut_unchecked::<T>();
//             f(Some(&mut typed_context))
//         }
//         None => f(None),
//     }
// }
//
// pub fn expect_state<T>() -> T
// where
//     T: Any + Clone + Send + Sync,
// {
//     get_state::<T>().expect("Context not found")
// }
