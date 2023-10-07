use std::any::{Any, TypeId};

use crate::gru::GRU;

pub fn provide_context<T>(context: T) -> Option<T>
where
    T: Any + Clone + Send + Sync,
{
    GRU.context
        .write()
        .expect("Failed to acquire write lock")
        .insert(TypeId::of::<T>(), Box::new(context))
        .map(|box_any| unsafe { *box_any.downcast_unchecked::<T>() })
}

pub fn get_context<T>() -> Option<T>
where
    T: Any + Clone + Send + Sync,
{
    GRU.context
        .read()
        .expect("Failed to acquire read lock")
        .get(&TypeId::of::<T>())
        .map(|ref_entry| unsafe { ref_entry.downcast_ref_unchecked::<T>() })
        .cloned()
}

pub fn with_context<T, R, F>(f: F) -> R
where
    T: Any + Clone + Send + Sync,
    F: FnOnce(Option<&T>) -> R + Send,
{
    match GRU
        .context
        .write()
        .expect("Failed to acquire read lock")
        .get(&TypeId::of::<T>())
    {
        Some(ref_entry) => {
            let typed_context = unsafe { ref_entry.downcast_ref_unchecked::<T>() };
            f(Some(typed_context))
        }
        None => f(None),
    }
}

pub fn with_context_mut<T, R, F>(f: F) -> R
where
    T: Any + Clone + Send + Sync,
    F: FnOnce(Option<&mut T>) -> R + Send,
{
    match GRU
        .context
        .write()
        .expect("Failed to acquire write lock")
        .get_mut(&TypeId::of::<T>())
    {
        Some(ref_entry) => {
            let typed_context = unsafe { ref_entry.downcast_mut_unchecked::<T>() };
            f(Some(typed_context))
        }
        None => f(None),
    }
}

pub fn expect_context<T>() -> T
where
    T: Any + Clone + Send + Sync,
{
    get_context::<T>().expect("Context not found")
}
