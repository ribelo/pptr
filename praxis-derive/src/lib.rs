mod master;
mod message;
mod puppet;

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Message, attributes(message))]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    message::expand(&ast).into()
}

#[proc_macro_derive(Puppet)]
pub fn derive_puppet(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    puppet::expand(&ast).into()
}

#[proc_macro_derive(Master)]
pub fn derive_master(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    master::expand(&ast).into()
}
