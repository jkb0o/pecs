extern crate proc_macro;
use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::*;
use syn::{self, ext::IdentExt, token::Comma, Token, Type};

#[proc_macro]
pub fn promise(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ctx = Context::new();
    let core = ctx.core_path();
    let promise = syn::parse_macro_input!(input as Promise);
    let state = &promise.state;
    let body = &promise.body;
    let args = &promise.system_args;
    let in_type = if let Some(state_type) = &promise.state_type {
        if promise.value.is_some() {
            quote! { ::bevy::prelude::In<(#core::AsyncState<#state_type>, #core::AsyncValue<_>)> }
        } else {
            quote! { ::bevy::prelude::In<#core::AsyncState<#state_type>> }
        }
    } else {
        if promise.value.is_some() {
            quote! { ::bevy::prelude::In<(#core::AsyncState<_>, #core::AsyncValue<_>)> }
        } else {
            quote! { ::bevy::prelude::In<#core::AsyncState<_>> }
        }
    };

    proc_macro::TokenStream::from(if let Some(value) = &promise.value {
        quote! {
            |::bevy::prelude::In((#core::AsyncState(mut #state), #core::AsyncValue(#value))): #in_type, #args| {
                #body
            }
        }
    } else if let Some(default_state) = &promise.default_state {
        quote! {
            #core::Promise::new(#default_state, |::bevy::prelude::In(#core::AsyncState(mut #state)): #in_type, #args| {
                #body
            })
        }
    } else {
        quote! {
            #core::Promise::new((), move |::bevy::prelude::In(#core::AsyncState(mut #state)): #in_type, #args| {
                #body
            })
        }
    })
}

struct Promise {
    state: Ident,
    state_type: Option<Type>,
    value: Option<Ident>,
    default_state: Option<Ident>,
    system_args: TokenStream,
    body: TokenStream,
}

impl syn::parse::Parse for Promise {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let default_state = if input.peek(Ident::peek_any) {
            let state = Some(input.parse()?);
            input.parse::<Token![=>]>()?;
            state
        } else {
            None
        };

        input.parse::<Token![|]>()?;
        let state = input.parse::<syn::Ident>()?;
        let state_type = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(Comma) {
            input.parse::<Comma>()?;
        }
        let value = if input.peek(Ident::peek_any) && !input.peek2(Token![:]) {
            let value = if input.peek(Token![_]) {
                let token = input.parse::<Token![_]>()?;
                Some(Ident::new("_", token.span))
            } else {
                Some(input.parse::<Ident>()?)
            };
            if input.peek(Comma) {
                input.parse::<Comma>()?;
            }
            value
        } else {
            None
        };
        let system_args = input.step(|cursor| {
            let mut rest = *cursor;
            let mut result = quote! {};
            while let Some((tt, next)) = rest.token_tree() {
                match &tt {
                    TokenTree::Punct(punct) if punct.as_char() == '|' => {
                        return Ok((result, next));
                    }
                    t => {
                        result = quote! { #result #t };
                        rest = next;
                    }
                }
            }
            Err(cursor.error("Expected `|` "))
        })?;

        let body = input.parse::<TokenStream>()?;
        Ok(Promise {
            state,
            state_type,
            default_state,
            value,
            body,
            system_args,
        })
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
        let Some(_pkg) = pkg.as_str() else { return context };
        // in future, macro may be used from inside the workspace
        if false
        /* pkg.trim() == "bevy_promise_http" */
        {
            context.core_path = quote! { ::bevy_promise_core };
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
