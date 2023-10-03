use std::any::Any;

use async_trait::async_trait;

#[async_trait]
pub trait Context {
    fn provide_context<T>(&self, context: T) -> Option<T>
    where
        T: Any + Clone + Send + Sync;

    fn get_context<T>(&self) -> Option<T>
    where
        T: Any + Clone + Send + Sync;

    fn with_context<T, R, F>(&self, f: F) -> R
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(Option<&T>) -> R + Send;

    fn with_context_mut<T, R, F>(&self, f: F) -> R
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(Option<&mut T>) -> R + Send;
}
