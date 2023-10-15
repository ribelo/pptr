use std::any::{Any, TypeId};

use crate::{gru::GRU, minion::BoxedAny, Id};

pub fn provide_context<T>(context: T) -> Option<T>
where
    T: Any + Clone + Send + Sync,
{
    let id: Id = TypeId::of::<T>().into();
    GRU.context
        .write()
        .expect("Failed to acquire write lock")
        .insert(id, BoxedAny::new(context))
        .map(|boxed_any| boxed_any.downcast::<T>().clone())
}

pub fn get_context<T>() -> Option<T>
where
    T: Any + Clone + Send + Sync,
{
    let id: Id = TypeId::of::<T>().into();
    GRU.context
        .write()
        .expect("Failed to acquire write lock")
        .get(&id)
        .map(|boxed_any| boxed_any.downcast::<T>())
        .cloned()
}

pub fn with_context<T, R, F>(f: F) -> R
where
    T: Any + Clone + Send + Sync,
    F: FnOnce(Option<&T>) -> R + Send,
{
    let id: Id = TypeId::of::<T>().into();
    match GRU
        .context
        .write()
        .expect("Failed to acquire read lock")
        .get(&id)
    {
        Some(boxed_any) => {
            let typed_context = boxed_any.downcast::<T>();
            f(Some(typed_context))
        }
        None => f(None),
    }
}

pub async fn with_context_async<T, R, F, Fut>(f: F) -> R
where
    T: Any + Clone + Send + Sync,
    F: FnOnce(Option<&T>) -> Fut,
    Fut: std::future::Future<Output = R> + Send,
{
    let id: Id = TypeId::of::<T>().into();
    match GRU
        .context
        .write()
        .expect("Failed to acquire read lock")
        .get(&id)
    {
        Some(boxed_any) => {
            let typed_context = boxed_any.downcast::<T>();
            f(Some(typed_context)).await
        }
        None => f(None).await,
    }
}

// pub fn with_context_mut<T, R, F>(f: F) -> R
// where
//     T: Any + Clone + Send + Sync,
//     F: FnOnce(Option<&mut T>) -> R + Send,
// {
//     let id: Id = TypeId::of::<T>().into();
//     match GRU
//         .context
//         .write()
//         .expect("Failed to acquire read lock")
//         .get_mut(&id)
//     {
//         Some(boxed_any) => {
//             let mut typed_context = boxed_any.downcast::<T>();
//             f(Some(&mut typed_context))
//         }
//         None => f(None),
//     }
// }

pub fn expect_context<T>() -> T
where
    T: Any + Clone + Send + Sync,
{
    get_context::<T>().expect("Context not found")
}
