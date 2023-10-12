use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemImpl, Type};

#[derive(PartialEq)]
enum HandlerType {
    Sync,
    Async,
}

fn extract_method_and_args(
    ast: &ItemImpl,
    handler_type: HandlerType,
) -> (Box<syn::Type>, Vec<syn::Type>) {
    let expected_method_name = match handler_type {
        HandlerType::Sync => "handle",
        HandlerType::Async => "async_handle",
    };
    let expected_type = if handler_type == HandlerType::Async {
        "async"
    } else {
        "sync"
    };

    let call_method = ast
        .items
        .iter()
        .find_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                let is_async = method.sig.asyncness.is_some();
                let is_matching_type = (is_async && handler_type == HandlerType::Async)
                    || (!is_async && handler_type == HandlerType::Sync);

                if method.sig.ident == expected_method_name {
                    if is_matching_type {
                        return Some(method.clone());
                    } else {
                        let found_type = if is_async { "async" } else { "sync" };
                        panic!(
                            "Type mismatch: Expected a {} method, but found a {} method",
                            expected_type, found_type
                        );
                    }
                } else {
                    let unexpected_name = if handler_type == HandlerType::Sync {
                        "async_handle"
                    } else {
                        "handle"
                    };
                    if method.sig.ident == unexpected_name {
                        panic!(
                            "Name mismatch: Expected to find a method named '{}', but found '{}'",
                            expected_method_name, unexpected_name
                        );
                    }
                }
            }
            None
        })
        .unwrap_or_else(|| {
            panic!(
                "No appropriate method found. Expected a {} method named '{}'",
                expected_type, expected_method_name
            )
        });

    let mut arg_iter = call_method.sig.inputs.iter().skip(1); // skip `self`
    let msg_arg = match arg_iter.next() {
        Some(syn::FnArg::Typed(pat)) => pat.clone(),
        _ => panic!("Expected a typed argument"),
    };

    let msg_type = &msg_arg.ty;
    let msg_name = match &*msg_arg.pat {
        syn::Pat::Ident(pat_ident) => &pat_ident.ident,
        _ => panic!("Expected an identifier for the argument"),
    };

    if !matches!(
        msg_name.to_string().as_str(),
        "message" | "msg" | "event" | "evt"
    ) {
        panic!("The first argument must be the message and should be named either `message`, `msg`, `event`, or `evt`. Found: '{}'", msg_name);
    }

    let arg_types: Vec<_> = arg_iter
        .map(|arg| match arg {
            syn::FnArg::Typed(pat) => (*pat.ty).clone(),
            _ => panic!("Expected a typed argument"),
        })
        .collect();

    (msg_type.clone(), arg_types)
}

pub fn get_types_calls(arg_types: &[Type]) -> Vec<TokenStream> {
    arg_types
        .iter()
        .map(|x| {
            if let syn::Type::Path(type_path) = x {
                let first_segment = &type_path.path.segments.first().unwrap().ident;
                quote! { #first_segment::construct(from) }
            } else {
                panic!("Expected a TypePath")
            }
        })
        .collect()
}

pub fn expand_sync(context_type: Type, ast: &ItemImpl) -> proc_macro::TokenStream {
    // Finding the method named `handle` within the implementation block
    let self_ty = &ast.self_ty;
    let (msg_type, arg_types) = extract_method_and_args(ast, HandlerType::Sync);

    // Mapping the argument types to calls of `::from_context(context)` on the first segment of their type paths
    let types_calls = get_types_calls(&arg_types);

    // Generating the token stream for the output of the macro
    let gen = quote! {
        #ast  // Including the original item implementation

        // Implementing the `Handler` trait for `SomeStruct`, with a tuple of argument types
        impl Handler<#msg_type, #context_type, (#(#arg_types),*)> for #self_ty {
            // Defining the `call` method which calls the `handle` method with the correct context extraction
            fn call(self, msg: #msg_type, from: &mut #context_type) {
                self.handle(msg, #(#types_calls),*);
            }
        }
    };

    // Converting the generated token stream into the output token stream of the macro
    gen.into()
}

pub fn expand_async(context_type: Type, ast: &ItemImpl) -> proc_macro::TokenStream {
    // Finding the method named `handle` within the implementation block
    let self_ty = &ast.self_ty;
    let (msg_type, arg_types) = extract_method_and_args(ast, HandlerType::Async);

    // Mapping the argument types to calls of `::from_context(context)` on the first segment of their type paths
    let types_calls = get_types_calls(&arg_types);

    // Generating the token stream for the output of the macro
    let gen = quote! {
        #ast  // Including the original item implementation

        // Implementing the `Handler` trait for `SomeStruct`, with a tuple of argument types
        #[async_trait]
        impl AsyncHandler<#msg_type, #context_type, (#(#arg_types),*)> for #self_ty {
            // Defining the `call` method which calls the `handle` method with the correct context extraction
            async fn async_call(self, msg: #msg_type, from: &mut #context_type) {
                self.handle(#(#types_calls),*).await;
            }
        }
    };

    // Converting the generated token stream into the output token stream of the macro
    gen.into()
}
