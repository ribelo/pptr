use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

pub fn expand(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let gen = quote! {
        impl Message for #name {
        }
    };

    gen
}
