use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Procedural Macro `LogError`.
///
/// This macro is designed to derive additional functionality for any struct
/// by generating an implementation of a `log_error` method. The generated method
/// takes no parameters and returns no values; it uses the `tracing::error` macro
/// to record an error event, writing a display representation (`{}`) of the struct
/// to the application's log output.
///
/// # Example
/// Here is how you would use this macro. Remember that procedural macros make use of
/// attributes to annotate code.
///
/// ```rust
/// #[derive(thiserror:Error, LogError)]
/// struct MyError {
///     #[error("Some error message")]
///     SomeError,
/// }
///
/// let x = MyError { field: 42 };
/// x.log_error();
/// ```
pub fn expand(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let gen = quote! {
        impl #name {
            pub fn log_error(&self) {
                tracing::error!("{}", self);
            }
        }
    };

    gen
}
