use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_str, Field, FieldMutability, ItemStruct, Token, Type, Visibility};

pub fn expand(ast: &mut ItemStruct) -> TokenStream {
    let address_ty: Type = parse_str("minion::Address").unwrap();
    let status_ty: Type = parse_str("Arc<std::sync::Mutex<minion::LifecycleStatus>>").unwrap();
    let tx_ty: Type = parse_str("minion::Postman<<Self as minion::Minion>::Msg>").unwrap();
    let gru_ty: Type = parse_str("Gru").unwrap();

    let new_fields = match ast.fields {
        syn::Fields::Named(ref mut fields) => &mut fields.named,
        _ => panic!("Minion struct must have named fields"),
    };

    new_fields.push(Field {
        ident: Some(Ident::new("_address", Span::call_site())),
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        attrs: vec![],
        ty: address_ty,
        colon_token: Some(Token![:](Span::call_site())),
    });

    new_fields.push(Field {
        ident: Some(Ident::new("_status", Span::call_site())),
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        attrs: vec![],
        ty: status_ty,
        colon_token: Some(Token![:](Span::call_site())),
    });

    new_fields.push(Field {
        ident: Some(Ident::new("_tx", Span::call_site())),
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        attrs: vec![],
        ty: tx_ty,
        colon_token: Some(Token![:](Span::call_site())),
    });

    new_fields.push(Field {
        ident: Some(Ident::new("_gru", Span::call_site())),
        vis: Visibility::Inherited,
        mutability: FieldMutability::None,
        attrs: vec![],
        ty: gru_ty,
        colon_token: Some(Token![:](Span::call_site())),
    });

    quote! {
        #ast
    }
}
