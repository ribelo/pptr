use std::any::Any;

use async_trait::async_trait;

use crate::OxFrameError;

#[async_trait]
pub trait Context {
    fn provide_context<T>(&self, context: T)
    where
        T: Any + Clone + Send + Sync;

    fn get_context<T>(&self) -> Option<T>
    where
        T: Any + Clone + Send + Sync;

    fn with_context<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&T) -> R + Send;

    fn with_context_mut<T, R, F>(&self, f: F) -> Result<R, OxFrameError>
    where
        T: Any + Clone + Send + Sync,
        F: FnOnce(&mut T) -> R + Send;
}
