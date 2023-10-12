use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemImpl, Type};

pub fn expand_sync(context_type: Type, ast: &ItemImpl) -> TokenStream {
    let self_ty = &ast.self_ty;
    // Finding the method named `handle` within the implementation block
    let call_method = ast
        .items
        .iter()
        .find_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                if method.sig.ident == "handle" {
                    Some(method.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("No method named handle found");

    // Collecting the types of the arguments (skipping `self`) of the `handle` method
    let arg_types: Vec<_> = call_method
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|arg| match arg {
            syn::FnArg::Typed(pat) => pat.ty.clone(),
            _ => panic!("Expected a typed argument"),
        })
        .collect();

    // Mapping the argument types to calls of `::from_context(context)` on the first segment of their type paths
    let types_calls: Vec<_> = arg_types
        .iter()
        .map(|x| {
            if let syn::Type::Path(type_path) = *x.clone() {
                let first_segment = &type_path.path.segments.first().unwrap().ident;
                quote! { #first_segment::construct(from) }
            } else {
                panic!("Expected a TypePath")
            }
        })
        .collect();

    // Generating the token stream for the output of the macro
    let gen = quote! {
        #ast  // Including the original item implementation

        // Implementing the `Handler` trait for `SomeStruct`, with a tuple of argument types
        impl Handler<#context_type, (#(#arg_types),*)> for #self_ty {
            // Defining the `call` method which calls the `handle` method with the correct context extraction
            fn call(self, from: &mut Context) {
                self.handle(#(#types_calls),*);
            }
        }
    };

    // Converting the generated token stream into the output token stream of the macro
    gen.into()
}

pub fn expand_async(ast: &ItemImpl) -> proc_macro::TokenStream {
    let self_ty = &ast.self_ty;
    // Finding the method named `handle` within the implementation block
    let call_method = ast
        .items
        .iter()
        .find_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                if method.sig.ident == "handle" {
                    Some(method.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("No method named handle found");

    // Collecting the types of the arguments (skipping `self`) of the `handle` method
    let arg_types: Vec<_> = call_method
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|arg| match arg {
            syn::FnArg::Typed(pat) => pat.ty.clone(),
            _ => panic!("Expected a typed argument"),
        })
        .collect();

    // Mapping the argument types to calls of `::from_context(context)` on the first segment of their type paths
    let types_calls: Vec<_> = arg_types
        .iter()
        .map(|x| {
            if let syn::Type::Path(type_path) = *x.clone() {
                let first_segment = &type_path.path.segments.first().unwrap().ident;
                quote! { #first_segment::construct(from) }
            } else {
                panic!("Expected a TypePath")
            }
        })
        .collect();

    // Generating the token stream for the output of the macro
    let gen = quote! {
        #ast  // Including the original item implementation

        // Implementing the `Handler` trait for `SomeStruct`, with a tuple of argument types
        #[async_trait]
        impl AsyncHandler<Context, (#(#arg_types),*)> for #self_ty {
            // Defining the `call` method which calls the `handle` method with the correct context extraction
            async fn async_call(self, from: &mut Context) {
                self.handle(#(#types_calls),*).await;
            }
        }
    };

    // Converting the generated token stream into the output token stream of the macro
    gen.into()
}
