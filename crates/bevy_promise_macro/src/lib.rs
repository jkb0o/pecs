extern crate proc_macro;
use proc_macro2::{Ident, TokenStream};
use quote::*;
use syn::{
    self,
    ext::IdentExt,
    token::{Colon, Comma},
    Pat, PatType, Token, Type,
};

#[proc_macro]
pub fn asyn(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ctx = Context::new();
    let promise = syn::parse_macro_input!(input as Promise);
    proc_macro::TokenStream::from(promise.build_function(&ctx))
}

struct Promise {
    state: Ident,
    value: Option<Pat>,
    default_state: Option<Ident>,
    system_args: Vec<syn::FnArg>,
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
            if input.peek(Token![|]) {
                input.parse::<Token![|]>()?;
                break;
            }
            system_args.push(input.parse()?);
        }
        // let system_args = input.step(|cursor| {
        //     let mut rest = *cursor;
        //     let mut result = quote! {};
        //     while let Some((tt, next)) = rest.token_tree() {
        //         match &tt {
        //             TokenTree::Punct(punct) if punct.as_char() == '|' => {
        //                 return Ok((result, next));
        //             }
        //             t => {
        //                 result = quote! { #result #t };
        //                 rest = next;
        //             }
        //         }
        //     }
        //     Err(cursor.error("Expected `|` "))
        // })?;

        let body = input.parse::<TokenStream>()?;
        Ok(Promise {
            state,
            default_state,
            value,
            body,
            system_args,
        })
    }
}

impl Promise {
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
            #core::PromiseFunction {
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
