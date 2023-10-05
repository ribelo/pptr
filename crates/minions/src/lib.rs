pub mod context;
pub mod dispatcher;
pub mod gru;
pub mod message;
pub mod minion;

#[macro_export]
macro_rules! impl_error_log_method {
    ($type_name:ty) => {
        impl $type_name {
            pub fn log(&self) {
                tracing::error!("{}", self);
            }
        }
    };
}
