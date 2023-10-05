mod log_error;
mod message;
mod minion;

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, ItemStruct};

#[proc_macro_derive(Message, attributes(message))]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    message::expand(&ast).into()
}

#[proc_macro_derive(LogError)]
pub fn log_error_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    log_error::expand(&ast).into()
}

#[proc_macro_attribute]
pub fn minion(_attrs: TokenStream, input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as ItemStruct);
    minion::expand(&mut ast).into()
}
