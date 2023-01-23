#![feature(proc_macro_quote)]

use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{ImplItem, ImplItemMethod, Type, Ident, parse_macro_input, FnArg, PatType, Pat, PatIdent};
use syn::punctuated::Punctuated;
use syn::token::Comma;

#[proc_macro_attribute]
pub fn impl_wasm_linker(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(item as syn::ItemImpl);
    let items = item.items;
    let mut quotes = vec![];
    let name = item.self_ty;
    for item in items {
        quotes.push(
            quote!{
                #item
            }
        );
        handle_item(&mut quotes, item, name.clone());
    }
    let token_stream: proc_macro::TokenStream = quote! {
        impl #name {
            #(#quotes)*
        }
    }.into();
    println!("{}", token_stream);
    token_stream
}

fn handle_item(fn_items: &mut Vec<TokenStream>, item: ImplItem, name: Box<Type>) {
    let method = match item {
        ImplItem::Method(method) => {
            method
        }
        _ => {return;}
    };
    let sig = method.sig.clone();
    let num_of_args = sig.inputs.len() - 1;
    let name_fn = method.sig.ident.clone();
    let link_name = proc_macro2::Ident::new(format!("link_{}", name_fn).as_str(), Span::call_site());

    let func_wrap_name = proc_macro2::Ident::new(
        format!("func_wrap{}_async", num_of_args).as_str(), Span::call_site());


    let impl_name = match name.as_ref() {
        Type::Path(path) => {
            path.path.get_ident().unwrap()
        }
        _ => {panic!()}
    };

    let module_name = proc_macro2::Literal::string(impl_name.to_string().to_lowercase().as_str());
    let function_name = proc_macro2::Literal::string(name_fn.to_string().as_str());

    let impl_type = name.clone();
    let mut fn_args = Punctuated::<FnArg, Comma>::new();
    for (i, x) in sig.clone().inputs.into_iter().enumerate() {
        if i != 0 {
            fn_args.push(x);
        }
    }

    let fn_args_idents: Vec<_> = fn_args.iter().map(|fn_arg| {
        match fn_arg {
            FnArg::Receiver(r) => panic!(),
            FnArg::Typed(t) => {
                match t {
                    PatType { attrs, pat, colon_token, ty } => {
                        match pat.as_ref() {
                            Pat::Ident(ident) => {ident.ident.clone()}
                            _ => {panic!()}
                        }
                    }
                }
            }
        }
    }).collect();

    let mut fn_arg_idents_2 = Punctuated::<Ident, Comma>::new();
    for x in fn_args_idents {
        fn_arg_idents_2.push(x);
    }

    let link_quote = quote! {
      pub fn #link_name<T: AsRef<Self> + Send>(linker: &mut Linker<T>) {
            async fn #name_fn<T: AsRef<#impl_type> + Send>(caller: Caller<'_, T>, #fn_args) {
                let this = caller.data().as_ref();
                this.#name_fn(#fn_arg_idents_2).await;
            }
            linker
        .#func_wrap_name(#module_name, #function_name,
        |caller: Caller<'_, T>, #fn_args| {
                Box::new(#name_fn(caller, #fn_arg_idents_2))
            })
        .unwrap();
        }

    };
    //println!("{}", link_quote);
    fn_items.push(link_quote)
}