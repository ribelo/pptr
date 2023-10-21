use darling::FromDeriveInput;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

#[derive(Default, Debug, FromDeriveInput)]
#[darling(default, attributes(message))]
struct MinionOpts {
    name: Option<String>,
}

pub fn expand(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let opts = match MinionOpts::from_derive_input(ast) {
        Ok(opts) => opts,
        Err(e) => {
            let error_message = format!("Macro derivation failed: {}", e);
            return quote! {
                compile_error!(#error_message);
            };
        }
    };

    let puppet_name = match opts.name {
        Some(name) => {
            quote! {
                fn name(&self) -> String {
                    #name
                }
            }
        }
        None => quote! {
            fn name(&self) -> String {
                std::any::type_name::<Self>().to_string()
            }
        },
    };

    let gen = quote! {
        impl Puppet for #name {
            #puppet_name
        }
    };

    gen
}
