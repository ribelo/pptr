use darling::FromDeriveInput;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

#[derive(Default, Debug, FromDeriveInput)]
#[darling(default, attributes(minion))]
struct MinionOpts {
    name: Option<String>,
    execution: Option<syn::Path>,
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

    let minion_name = match &opts.name {
        Some(name) => {
            quote! { #name }
        }
        None => quote! { type_name::<Self>().to_string() },
    };
    let execution_type = match &opts.execution {
        Some(execution) => {
            let execution_type = {
                syn::Type::Path(syn::TypePath {
                    qself: None,
                    path: execution.clone(),
                })
            };
            quote! { #execution_type }
        }
        None => quote! { ExecutionType::Sequential },
    };

    let gen = quote! {
        impl MinionConfig for #name {

            fn name(&self) -> String {
                #minion_name
            }

            fn execution_type(&self) -> ExecutionType {
                #execution_type
            }
        }
    };

    gen
}
