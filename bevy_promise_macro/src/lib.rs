extern crate proc_macro;
use proc_macro2::{TokenStream, Ident, TokenTree};
use quote::*;
use syn::{self, Token, token::Comma, ext::IdentExt};

#[proc_macro]
pub fn promise(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ctx = Context::new();
    let core = ctx.core_path();
    let promise = syn::parse_macro_input!(input as Promise);
    // panic!("parsed");
    let state = &promise.state;
    let body = &promise.body;
    let args = &promise.system_args;

    proc_macro::TokenStream::from(if let Some(value) =  &promise.value {
        quote! {
            |::bevy::prelude::In((#core::AsyncState(#state), #core::AsyncValue(#value))), #args| {
                #body
            }
        }
    } else {
        quote! {
            #core::Promise::new(|::bevy::prelude::In(#core::AsyncState(#state)), #args| {
                #body
            })
        }
    })
}

struct Promise {
    state: Ident,
    value: Option<Ident>,
    system_args: TokenStream,
    body: TokenStream
}

impl syn::parse::Parse for Promise {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<Token![|]>()?;
        let state = input.parse::<syn::Ident>()?;
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
        
        // let parser = Punctuated::<PatType, Token![,]>::parse_terminated;
        // let args = parser.parse(input.clone())?;
        // let args = Punctuated::<FnArg, Comma>::parse_terminated(input)?;
        // let args = input.parse::<Punctuated::<FnArg, Token![,]>>()?;
        let system_args = input.step(|cursor| {
            let mut rest = *cursor;
            let mut result = quote!{ };
            while let Some((tt, next)) = rest.token_tree() {
                match &tt {
                    TokenTree::Punct(punct) if punct.as_char() == '|' => {
                        return Ok((result, next));
                    }
                    t => {
                        result = quote!{ #result #t };
                        rest = next;
                    }
                }
            }
            Err(cursor.error("Expected `|` "))
        })?;
        // panic!("parsed terminated");
        // input.parse::<Token![|]>()?;
        
        let body = input.parse::<TokenStream>()?;
        Ok(Promise { state, value, body, system_args })
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
        if false { //pkg.trim() == "bevy_promise_http" {
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