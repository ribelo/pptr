use darling::FromDeriveInput;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

#[derive(Default, Debug, FromDeriveInput)]
#[darling(default, attributes(message))]
struct MessageOpts {
    response: Option<syn::Path>,
}

/// This procedural macro is used for deriving the `Message` trait on structs.
///
/// It accepts attributes to specify type for the `Response` associated type in the `Message` trait.
/// If no type is provided, `()` will be used as default. This procedural macro also provides
/// basic error handling functionalities in case invalid attributes are encountered.
///
/// # Parameters
///
/// `input`: The `TokenStream` that the macro was called on. This can be any struct or enum and
/// this input will be parsed into a `syn::DeriveInput`.
///
/// # Returns
///
/// A `TokenStream` representing the code that should be inserted at the location of the macro call.
///
/// # Example
///
/// ```rust
/// #[derive(Message)]
/// #[message(response = String)]
/// pub struct Command {
///     name: String,
///     args: Vec<String>,
/// }
/// ```
///
/// In the above example, the `Message` trait is being derived for `Command`
/// struct. The associated type `Response` for the `Message` trait is specified
/// to be `String` through the `message` attribute. This means when we're
/// handling a `Command` message, we're expecting a `String` in return.
pub fn expand(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let opts = match MessageOpts::from_derive_input(ast) {
        Ok(opts) => opts,
        Err(e) => {
            let error_message = format!("Macro derivation failed: {}", e);
            return quote! {
                compile_error!(#error_message);
            };
        }
    };

    let response_type = match opts.response {
        Some(response) => {
            let response_type = {
                syn::Type::Path(syn::TypePath {
                    qself: None,
                    path: response,
                })
            };
            quote! { #response_type }
        }
        None => quote! { () },
    };

    let gen = quote! {
        impl Messageable for #name {
            type Response = #response_type;
        }
    };

    gen
}
