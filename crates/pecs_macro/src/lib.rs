extern crate proc_macro;
use proc_macro2::TokenStream;
use quote::*;
use std::str::FromStr;
use syn::{self, token::Comma, LitInt, Pat, PatType, Token};

#[proc_macro]
/// Turns system-like expresion into
/// [`AsynFunction`](https://docs.rs/pecs/latest/pecs/struct.AsynFunction.html))
pub fn asyn(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ctx = Context::new();
    let promise = syn::parse_macro_input!(input as AsynFunc);
    proc_macro::TokenStream::from(promise.build_function(&ctx))
}

#[proc_macro]
pub fn impl_any_promises(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let num = syn::parse_macro_input!(input as LitInt);
    let num = match num.base10_parse::<u8>() {
        Ok(n) => n,
        Err(e) => return proc_macro::TokenStream::from(e.to_compile_error()),
    };
    proc_macro::TokenStream::from(impl_any_promises_internal(num))
}

#[proc_macro]
pub fn impl_all_promises(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let num = syn::parse_macro_input!(input as LitInt);
    let num = match num.base10_parse::<u8>() {
        Ok(n) => n,
        Err(e) => return proc_macro::TokenStream::from(e.to_compile_error()),
    };
    proc_macro::TokenStream::from(impl_all_promises_internal(num))
}

struct AsynFunc {
    force_loop: bool,
    state: Option<Pat>,
    result: Option<Pat>,
    system_args: Vec<syn::FnArg>,
    body: TokenStream,
}

fn closes_with_line(i: &mut syn::parse::ParseStream) -> bool {
    if i.peek(Token![|]) {
        i.parse::<Token![|]>().unwrap();
        true
    } else {
        false
    }
}
fn closes_with_arrow(i: &mut syn::parse::ParseStream) -> bool {
    if i.peek(Token![=>]) {
        i.parse::<Token![=>]>().unwrap();
        true
    } else {
        false
    }
}

impl syn::parse::Parse for AsynFunc {
    fn parse(mut input: syn::parse::ParseStream) -> syn::Result<Self> {
        let args_done = if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            closes_with_line
        } else {
            closes_with_arrow
        };
        let force_loop = if input.peek(Token![loop]) {
            input.parse::<Token![loop]>()?;
            true
        } else {
            false
        };
        let mut system_args = vec![];
        let mut state = None;
        let mut result = None;
        let mut body = quote! {};
        while let Ok(pat) = input.parse() {
            if input.peek(Token![;]) {
                body = quote! { #pat };
                break;
            }
            if input.peek(Token![:]) {
                system_args.push(syn::FnArg::Typed(PatType {
                    attrs: vec![],
                    pat: Box::new(pat),
                    colon_token: input.parse()?,
                    ty: Box::new(input.parse()?),
                }));
            } else if !system_args.is_empty() {
                panic!("Invalid system args sequesnce for asyn! func")
            } else if state.is_none() {
                state = Some(pat);
            } else if result.is_none() {
                result = Some(pat)
            } else {
                panic!("Invalid system arg sequesnce for asyn! func")
            }
            if input.peek(Comma) {
                input.parse::<Comma>()?;
            }
            if args_done(&mut input) {
                break;
            }
        }

        if state.is_some() || !system_args.is_empty() {
            args_done(&mut input);
        }
        let rest_body = input.parse::<TokenStream>()?;
        body = quote! { #body #rest_body };
        Ok(AsynFunc {
            force_loop,
            state,
            result,
            system_args,
            body,
        })
    }
}

impl AsynFunc {
    fn build_function(&self, ctx: &Context) -> TokenStream {
        let core = ctx.core_path();
        let mut pats = quote! {};
        let mut types = quote! {};
        let mut asyn_spec = quote! {};
        for arg in self.system_args.iter() {
            let syn::FnArg::Typed(arg) = arg else {
                continue;
            };
            let pat = arg.pat.as_ref();
            let typ = arg.ty.as_ref();
            pats = quote! { #pats #pat, };
            types = quote! { #types #typ, };
        }
        let state_str = if let Some(state) = &self.state {
            state.to_token_stream().to_string()
        } else {
            "_".to_string()
        };
        let state_str = state_str.trim();
        let mutable =
            if false || state_str.starts_with("_") || state_str.starts_with("mut ") || state_str.starts_with("(") {
                quote! {}
            } else {
                quote! { mut }
            };

        if self.force_loop {
            asyn_spec = quote!(::<#core::PromiseState<_>, #core::PromiseResult<_, #core::Loop<_>>, _>);
        }

        let input = match (&self.state, &self.result) {
            (None, None) => quote! { _ },
            (Some(state), None) => quote! { (#mutable #state, _) },
            (Some(state), Some(result)) => quote! { (#mutable #state, #result) },
            _ => panic!("Invlid state/result arguments"),
        };
        let body = &self.body;
        quote! {
            #core::AsynFunction #asyn_spec {
                marker: ::core::marker::PhantomData::<(#types)>,
                body: |::bevy::prelude::In(#input), (#pats): (#types)| {
                    #body
                }
            }
        }
    }
}

struct Context {
    core_path: TokenStream,
    is_interal: bool,
}

impl Context {
    pub fn new() -> Context {
        let mut context = Context {
            core_path: quote! { ::pecs_core },
            is_interal: true,
        };
        let Some(manifest_path) = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .map(|mut path| { path.push("Cargo.toml"); path })
            else { return context };
        let Ok(manifest) = std::fs::read_to_string(&manifest_path) else {
            return context
        };
        let Ok(manifest) = toml::from_str::<toml::map::Map<String, toml::Value>>(&manifest) else {
            return context
        };

        let Some(pkg) = manifest.get("package") else { return context };
        let Some(pkg) = pkg.as_table() else { return context };
        let Some(pkg) = pkg.get("name") else { return context };
        let Some(pkg) = pkg.as_str() else { return context };
        // in future, macro may be used from inside the workspace
        if pkg.trim() == "pecs_core" {
            context.core_path = quote! { crate };
        } else {
            context.core_path = quote! { ::pecs::core };
            context.is_interal = false;
        };
        context
    }
    pub fn core_path(&self) -> &TokenStream {
        &self.core_path
    }
}

fn impl_any_promises_internal(elements: u8) -> TokenStream {
    let mut result = quote! {};
    for num_elements in 1..elements {
        let im = impl_any_promises_internal_for(num_elements);
        result = quote! {
            #result
            #im
        }
    }
    result
}

// Macro epansion example for 2 elements:
//
// impl<R0: 'static, R1: 'static, E0: 'static, E1: 'static> AnyPromises for (Promise<R0, E0, ()>, Promise<R1, E1, ()>) {
//     type Items = (PromiseId, PromiseId);
//     type Result = (Option<Result<R0, E0>>, Option<Result<R1, E1>>);
//     fn register(self) -> Promise<Self::Result, (), ()> {
//         let (p0, p1) = self;
//         let (id0, id1) = (p0.id, p1.id);
//         Promise::register(
//             move |world, any_id| {
//                 promise_register(
//                     world,
//                     p0.map_state(move |_| (any_id, id1))
//                         .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
//                             let (any_id, id1) = s.value.clone();
//                             Promise::<(), (), ()>::register(
//                                 move |world, _id| {
//                                     // discard rest promises
//                                     promise_discard::<R1, E1, ()>(world, id1);
//                                     // resolve p0
//                                     promise_resolve::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>), (), ()>(
//                                         world,
//                                         any_id,
//                                         (Some(r), None),
//                                         (),
//                                     );
//                                 },
//                                 move |_world, _id| {},
//                             )
//                         })),
//                 );
//                 promise_register(
//                     world,
//                     p1.map_state(move |_| (any_id, id0))
//                         .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
//                             let (any_id, id0) = s.value.clone();
//                             Promise::<(), (), ()>::register(
//                                 move |world, _id| {
//                                     // discard rest promises
//                                     promise_discard::<R0, E0, ()>(world, id0);
//                                     // resolve p0
//                                     promise_resolve::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>), (), ()>(
//                                         world,
//                                         any_id,
//                                         (None, Some(r)),
//                                         (),
//                                     );
//                                 },
//                                 move |_world, _id| {},
//                             )
//                         })),
//                 );
//             },
//             move |world, _id| {
//                 promise_discard::<R0, E0, ()>(world, id0);
//                 promise_discard::<R1, E1, ()>(world, id1);
//             },
//         )
//     }
// }
fn impl_any_promises_internal_for(elements: u8) -> TokenStream {
    let mut in_generics = quote! {};
    let mut for_args = quote! {};
    let mut type_items = quote! {};
    let mut type_result = quote! {};
    let mut promise_idents = quote! {};
    let mut promise_id_sources = quote! {};
    let mut promise_id_targets = quote! {};
    let mut register = quote! {};
    let mut discards = quote! {};
    for idx in 0..elements + 1 {
        let c = if idx == 0 { quote!() } else { quote!(,) };
        let r = format_ident!("R{idx}");
        let p = format_ident!("p{idx}");
        let id = format_ident!("id{idx}");
        in_generics = quote!(#in_generics #c #r: 'static);
        for_args = quote!(#for_args #c Promise<(), #r>);
        type_items = quote!(#type_items #c PromiseId);
        type_result = quote!(#type_result #c Option<#r>);
        promise_idents = quote!(#promise_idents #c #p);
        promise_id_targets = quote!(#promise_id_targets #c #id);
        promise_id_sources = quote!(#promise_id_sources #c #p.id);
        discards = quote! {
            #discards
            promise_discard::<(), #r>(world, #id);
        }
    }
    for idx in 0..elements + 1 {
        let p = format_ident!("p{idx}");
        let mut local_discards = quote! {};
        let mut local_value = quote! {};
        for local in 0..elements + 1 {
            let r = format_ident!("R{local}");
            let id = format_ident!("id{local}");
            let c = if local == 0 { quote!() } else { quote!(,) };
            if local != idx {
                local_discards = quote! {
                    #local_discards
                    promise_discard::<(), #r>(world, #id);
                }
            }
            if local == idx {
                local_value = quote!(#local_value #c Some(r) )
            } else {
                local_value = quote!(#local_value #c None )
            }
        }
        register = quote! {
            #register
            promise_register(world, #p.with((any_id, #promise_id_targets))
                .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
                    let (any_id, #promise_id_targets) = s.value.clone();
                    Promise::<(), ()>::register(
                        move |world, _id| {
                            #local_discards
                            promise_resolve::<(), (#type_result)>(
                                world,
                                any_id,
                                (),
                                (#local_value),
                            );
                        },
                        |_, _| {}
                    )
                })),
            );
        }
    }

    quote! {
        impl<#in_generics> AnyPromises for (#for_args) {
            // type Items = (#type_items);
            type Result = (#type_result);
            fn register(self) -> Promise<(), Self::Result> {
                let (#promise_idents) = self;
                let (#promise_id_targets) = (#promise_id_sources);
                Promise::register(
                    move |world, any_id| {
                        #register
                    }, move|world, _id|{
                        #discards
                    }
                )
            }
        }
    }
}

fn impl_all_promises_internal(elements: u8) -> TokenStream {
    let mut result = quote! {};
    for num_elements in 1..elements {
        let im = impl_all_promises_internal_for(num_elements);
        result = quote! {
            #result
            #im
        }
    }
    result
}

fn impl_all_promises_internal_for(elements: u8) -> TokenStream {
    let mut in_generics = quote! {};
    let mut for_args = quote! {};
    let mut type_items = quote! {};
    let mut type_result = quote! {};
    let mut promise_idents = quote! {};
    let mut promise_id_sources = quote! {};
    let mut promise_id_targets = quote! {};
    let mut value_names = quote! {};
    let mut value_unwraps = quote! {};
    let mut register = quote! {};
    let mut discards = quote! {};
    let mut if_all_passed = quote! {};
    let mut value_type = quote! {};
    let mut value_defaults = quote! {};
    let mut value_clones = quote! {};
    for idx in 0..elements + 1 {
        let c = if idx == 0 { quote!() } else { quote!(,) };
        let r = format_ident!("R{idx}");
        let p = format_ident!("p{idx}");
        let id = format_ident!("id{idx}");
        let v = format_ident!("v{idx}");
        let i = TokenStream::from_str(&format!("{idx}")).unwrap();
        in_generics = quote!(#in_generics #c #r: 'static);
        for_args = quote!(#for_args #c Promise<(), #r>);
        type_items = quote!(#type_items #c PromiseId);
        type_result = quote!(#type_result #c #r);
        promise_idents = quote!(#promise_idents #c #p);
        value_names = quote!(#value_names #c #v);
        value_unwraps = quote!(#value_unwraps #c #v.unwrap() );
        value_type = quote!(#value_type #c Option<#r>);
        value_defaults = quote!(#value_defaults #c None);
        promise_id_targets = quote!(#promise_id_targets #c #id);
        promise_id_sources = quote!(#promise_id_sources #c #p.id);
        value_clones = quote! {
            #value_clones
            let #v = value.clone();
        };
        discards = quote! {
            #discards
            promise_discard::<(), #r>(world, #id);
        };
        if_all_passed = quote! {
            #if_all_passed
            && value.#i.is_some()
        };
    }
    for idx in 0..elements + 1 {
        let p = format_ident!("p{idx}");
        let v = format_ident!("v{idx}");
        let i = TokenStream::from_str(&format!("{idx}")).unwrap();
        register = quote! {
            #register
            promise_register(world, #p.with((any_id, #v, #promise_id_targets))
                .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
                    let (any_id, mut value, #promise_id_targets) = s.value.clone();
                    Promise::<(), ()>::register(
                        move |world, _id| {
                            value.get_mut().#i = Some(r);
                            if { value.is_valid() && { let value = value.get_ref(); true #if_all_passed }} {
                                let (#value_names) = value.get();
                                promise_resolve::<(), (#type_result)>(
                                    world,
                                    any_id,
                                    (),
                                    (#value_unwraps),
                                );
                            }
                        },
                        move |world, _| {
                            #discards
                        }
                    )
                })),
            );
        }
    }

    quote! {
        impl<#in_generics> AllPromises for (#for_args) {
            type Result = (#type_result);
            fn register(self) -> Promise<(), Self::Result> {
                let (#promise_idents) = self;
                let (#promise_id_targets) = (#promise_id_sources);
                let value = MutPtr::<(#value_type)>::new((#value_defaults));
                #value_clones
                Promise::register(
                    move |world, any_id| {
                        #register
                    }, move|world, _id|{
                        #discards
                    }
                )
            }
        }
    }
}
