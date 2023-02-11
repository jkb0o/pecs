extern crate proc_macro;
use proc_macro2::TokenStream;
use quote::*;
use std::str::FromStr;
use syn::{self, token::Comma, LitInt, Pat, PatType, Token};

#[proc_macro]
/// Turns system-like expresion into
/// [`AsynFunction`](https://docs.rs/bevy_promise/latest/bevy_promise/struct.AsynFunction.html))
pub fn asyn(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ctx = Context::new();
    let promise = syn::parse_macro_input!(input as Asyn);
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

struct Asyn {
    state: Pat,
    value: Option<Pat>,
    system_args: Vec<syn::FnArg>,
    body: TokenStream,
}

fn closes_with_line(i: &mut syn::parse::ParseStream) -> syn::Result<bool> {
    if i.peek(Token![|]) {
        i.parse::<Token![|]>()?;
        Ok(true)
    } else {
        Ok(false)
    }
}
fn closes_with_arrow(i: &mut syn::parse::ParseStream) -> syn::Result<bool> {
    if i.peek(Token![=>]) {
        i.parse::<Token![=>]>()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

impl syn::parse::Parse for Asyn {
    fn parse(mut input: syn::parse::ParseStream) -> syn::Result<Self> {
        let closes = if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            closes_with_line
        } else {
            closes_with_arrow
        };
        let state = input.parse::<syn::Pat>()?;
        if input.peek(Comma) {
            input.parse::<Comma>()?;
        }
        let mut system_args = vec![];
        let value = if let Ok(pat) = input.parse() {
            if input.peek(Token![:]) {
                system_args.push(syn::FnArg::Typed(PatType {
                    attrs: vec![],
                    pat: Box::new(pat),
                    colon_token: input.parse()?,
                    ty: Box::new(input.parse()?),
                }));
                None
            } else {
                Some(pat)
            }
        } else {
            None
        };
        loop {
            if input.peek(Comma) {
                input.parse::<Comma>()?;
            }
            if closes(&mut input)? {
                break;
            }
            system_args.push(input.parse()?);
        }

        let body = input.parse::<TokenStream>()?;
        Ok(Asyn {
            state,
            value,
            body,
            system_args,
        })
    }
}

impl Asyn {
    fn build_function(&self, ctx: &Context) -> TokenStream {
        let core = ctx.core_path();
        let mut pats = quote! {};
        let mut types = quote! {};
        for arg in self.system_args.iter() {
            let syn::FnArg::Typed(arg) = arg else {
                continue;
            };
            let pat = arg.pat.as_ref();
            let typ = arg.ty.as_ref();
            pats = quote! { #pats #pat, };
            types = quote! { #types #typ, };
        }
        let input = &self.state;
        let mut input = quote! { #input };
        if let Some(value) = &self.value {
            input = quote! { (#input, #value) };
        }
        let body = &self.body;
        quote! {
            #core::AsynFunction {
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
            core_path: quote! { ::bevy_promise_core },
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
        if pkg.trim() == "bevy_promise_core" {
            context.core_path = quote! { crate };
        } else {
            context.core_path = quote! { ::bevy_promise::core };
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
        let e = format_ident!("E{idx}");
        let p = format_ident!("p{idx}");
        let id = format_ident!("id{idx}");
        in_generics = quote!(#in_generics #c #r: 'static, #e: 'static);
        for_args = quote!(#for_args #c Promise<#r, #e, ()>);
        type_items = quote!(#type_items #c PromiseId);
        type_result = quote!(#type_result #c Option<Result<#r, #e>>);
        promise_idents = quote!(#promise_idents #c #p);
        promise_id_targets = quote!(#promise_id_targets #c #id);
        promise_id_sources = quote!(#promise_id_sources #c #p.id);
        discards = quote! {
            #discards
            promise_discard::<#r, #e, ()>(world, #id);
        }
    }
    for idx in 0..elements + 1 {
        let p = format_ident!("p{idx}");
        let mut local_discards = quote! {};
        let mut local_value = quote! {};
        for local in 0..elements + 1 {
            let r = format_ident!("R{local}");
            let e = format_ident!("E{local}");
            let id = format_ident!("id{local}");
            let c = if local == 0 { quote!() } else { quote!(,) };
            if local != idx {
                local_discards = quote! {
                    #local_discards
                    promise_discard::<#r, #e, ()>(world, #id);
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
            promise_register(world, #p.map(move |_| (any_id, #promise_id_targets))
                .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
                    let (any_id, #promise_id_targets) = s.value.clone();
                    Promise::<(), (), ()>::register(
                        move |world, _id| {
                            #local_discards
                            promise_resolve::<(#type_result), (), ()>(
                                world,
                                any_id,
                                (#local_value),
                                (),
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
            fn register(self) -> Promise<Self::Result, (), ()> {
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

// Macro epansion example for 2 elements:
// impl<R0: 'static, E0: 'static, R1: 'static, E1: 'static> AllPromises for (Promise<R0, E0, ()>, Promise<R1, E1, ()>) {
//     type Items = (PromiseId, PromiseId);
//     type Result = (Result<R0, E0>, Result<R1, E1>);
//     fn register(self) -> Promise<Self::Result, (), ()> {
//         let (p0, p1) = self;
//         let (id0, id1) = (p0.id, p1.id);
//         let value = MutPtr::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>)>::new((None, None));
//         let v0 = value.clone();
//         let v1 = value.clone();
//         Promise::register(
//             move |world, any_id| {
//                 promise_register(
//                     world,
//                     p0.map_state(move |_| (any_id, v0, id0, id1))
//                         .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
//                             let (any_id, mut value, id0, id1) = s.value.clone();
//                             Promise::<(), (), ()>::register(
//                                 move |world, _id| {
//                                     value.get_mut().0 = Some(r);
//                                     if {
//                                         value.is_valid() && {
//                                             let value = value.get_ref();
//                                             true && value.0.is_some() && value.1.is_some()
//                                         }
//                                     } {
//                                         let (v0, v1) = value.get();
//                                         promise_resolve::<(Result<R0, E0>, Result<R1, E1>), (), ()>(
//                                             world,
//                                             any_id,
//                                             (v0.unwrap(), v1.unwrap()),
//                                             (),
//                                         );
//                                     }
//                                 },
//                                 move |world, _| {
//                                     promise_discard::<R0, E0, ()>(world, id0);
//                                     promise_discard::<R1, E1, ()>(world, id1);
//                                 },
//                             )
//                         })),
//                 );
//                 promise_register(
//                     world,
//                     p1.map_state(move |_| (any_id, v1, id0, id1))
//                         .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
//                             let (any_id, mut value, id0, id1) = s.value.clone();
//                             Promise::<(), (), ()>::register(
//                                 move |world, _id| {
//                                     value.get_mut().1 = Some(r);
//                                     if {
//                                         value.is_valid() && {
//                                             let value = value.get_ref();
//                                             true && value.0.is_some() && value.1.is_some()
//                                         }
//                                     } {
//                                         let (v0, v1) = value.get();
//                                         promise_resolve::<(Result<R0, E0>, Result<R1, E1>), (), ()>(
//                                             world,
//                                             any_id,
//                                             (v0.unwrap(), v1.unwrap()),
//                                             (),
//                                         );
//                                     }
//                                 },
//                                 move |world, _| {
//                                     promise_discard::<R0, E0, ()>(world, id0);
//                                     promise_discard::<R1, E1, ()>(world, id1);
//                                 },
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
        let e = format_ident!("E{idx}");
        let p = format_ident!("p{idx}");
        let id = format_ident!("id{idx}");
        let v = format_ident!("v{idx}");
        let i = TokenStream::from_str(&format!("{idx}")).unwrap();
        in_generics = quote!(#in_generics #c #r: 'static, #e: 'static);
        for_args = quote!(#for_args #c Promise<#r, #e, ()>);
        type_items = quote!(#type_items #c PromiseId);
        type_result = quote!(#type_result #c Result<#r, #e>);
        promise_idents = quote!(#promise_idents #c #p);
        value_names = quote!(#value_names #c #v);
        value_unwraps = quote!(#value_unwraps #c #v.unwrap() );
        value_type = quote!(#value_type #c Option<Result<#r,#e>>);
        value_defaults = quote!(#value_defaults #c None);
        promise_id_targets = quote!(#promise_id_targets #c #id);
        promise_id_sources = quote!(#promise_id_sources #c #p.id);
        value_clones = quote! {
            #value_clones
            let #v = value.clone();
        };
        discards = quote! {
            #discards
            promise_discard::<#r, #e, ()>(world, #id);
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
            promise_register(world, #p.map(move |_| (any_id, #v, #promise_id_targets))
                .then(AsynFunction::<_, _, ()>::new(|In((s, r)), ()| {
                    let (any_id, mut value, #promise_id_targets) = s.value.clone();
                    Promise::<(), (), ()>::register(
                        move |world, _id| {
                            value.get_mut().#i = Some(r);
                            if { value.is_valid() && { let value = value.get_ref(); true #if_all_passed }} {
                                let (#value_names) = value.get();
                                promise_resolve::<(#type_result), (), ()>(
                                    world,
                                    any_id,
                                    (#value_unwraps),
                                    (),
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
            // type Items = (#type_items);
            type Result = (#type_result);
            fn register(self) -> Promise<Self::Result, (), ()> {
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
