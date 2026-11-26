//! The macros of `rustrak-plugin`. Each one keeps the item it annotates as
//! written and registers it, so the plugin's manifest is its code.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};

use syn::{Expr, FnArg, Ident, ItemFn, LitStr, Token, Type};

/// `key = value` pairs and bare flags, in any order.
struct Args {
    path: Option<LitStr>,
    flags: Vec<Ident>,
    pairs: Vec<(Ident, Expr)>,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Args {
            path: None,
            flags: vec![],
            pairs: vec![],
        };
        if input.peek(LitStr) {
            args.path = Some(input.parse()?);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                args.pairs.push((name, input.parse()?));
            } else {
                args.flags.push(name);
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(args)
    }
}

impl Args {
    fn pair(&self, name: &str) -> Option<&Expr> {
        self.pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    fn only(&self, flags: &[&str], pairs: &[&str]) -> syn::Result<()> {
        if let Some(f) = self
            .flags
            .iter()
            .find(|f| !flags.contains(&f.to_string().as_str()))
        {
            return Err(syn::Error::new(f.span(), format!("unknown option `{f}`")));
        }
        if let Some((k, _)) = self
            .pairs
            .iter()
            .find(|(k, _)| !pairs.contains(&k.to_string().as_str()))
        {
            return Err(syn::Error::new(k.span(), format!("unknown option `{k}`")));
        }
        Ok(())
    }
}

/// Where `file` is under the plugin's own `src/server/`, measured from the
/// crate's root: the folders the crate itself sits in say nothing.
fn under_server(file: &std::path::Path, root: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let full = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    let mut clean = std::path::PathBuf::new();
    for part in full.components() {
        match part {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other),
        }
    }
    let rel = clean.strip_prefix(root).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    rel.strip_prefix("src/server/").map(str::to_string)
}

/// The call site's place under `src/server/`, if it is there.
fn call_site_under_server() -> Option<String> {
    let file = proc_macro::Span::call_site().local_file()?;
    let root = std::env::var_os("CARGO_MANIFEST_DIR")?;
    under_server(&file, std::path::Path::new(&root))
}

/// A route lives in `src/server/routes/`, so every plugin reads the same.
/// Only checked inside `src/server/`: tests and examples elsewhere are free.
fn placed() -> syn::Result<()> {
    match call_site_under_server() {
        Some(at) if !at.starts_with("routes/") => Err(syn::Error::new(
            Span::call_site(),
            format!("a route goes in src/server/routes/ (this is src/server/{at})"),
        )),
        _ => Ok(()),
    }
}

fn expand(result: syn::Result<Tokens>) -> TokenStream {
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

fn is_ctx(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => match &*r.elem {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "Ctx"),
            _ => false,
        },
        _ => false,
    }
}

fn route(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(attr as Args);
    let func = syn::parse_macro_input!(item as ItemFn);
    expand(route_impl(method, &args, &func).map(|t| quote! { #func #t }))
}

fn route_impl(method: &str, args: &Args, func: &ItemFn) -> syn::Result<Tokens> {
    placed()?;
    args.only(&["public", "instance"], &["requires"])?;
    let path = args.path.as_ref().ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            "the route's path comes first: #[get(\"/stats\")]",
        )
    })?;
    let p = path.value();
    // The path goes into the route's JSON line as it is: nothing to escape.
    if !p.starts_with('/')
        || p.contains(['{', '}', '?', '#', '"', '\\'])
        || p.contains(char::is_control)
    {
        return Err(syn::Error::new(
            path.span(),
            "a path starts with / and has no placeholders",
        ));
    }
    let (public, instance) = (args.flag("public"), args.flag("instance"));
    if public && instance {
        return Err(syn::Error::new(
            Span::call_site(),
            "a public route belongs to a project, not the instance",
        ));
    }
    // `requires` is a const expression (`Perm::X.read()`), so it joins the
    // route's line at compile time.
    let requires = match args.pair("requires") {
        Some(e) if public => {
            return Err(syn::Error::new_spanned(
                e,
                "a public route has no user to check",
            ));
        }
        Some(e) => quote! { "\"", #e, "\"" },
        None => quote! { "null" },
    };
    let head = format!(
        "{{\"method\":\"{method}\",\"path\":\"{p}\",\"auth\":\"{}\",\"scope\":\"{}\",\"requires\":",
        if public { "public" } else { "user" },
        if instance { "instance" } else { "project" },
    );

    let name = &func.sig.ident;
    let handler = format_ident!("__rustrak_route_{}", name);
    let mut extract = vec![];
    let mut call = vec![];
    for (i, input) in func.sig.inputs.iter().enumerate() {
        let FnArg::Typed(arg) = input else {
            return Err(syn::Error::new_spanned(input, "a route is a free function"));
        };
        if is_ctx(&arg.ty) {
            call.push(quote! { ctx });
        } else {
            let var = format_ident!("__arg{}", i);
            let ty = &arg.ty;
            extract.push(quote! {
                let #var = match <#ty as ::rustrak_plugin::__private::FromCtx>::from_ctx(ctx) {
                    ::core::result::Result::Ok(v) => v,
                    ::core::result::Result::Err(e) => return ::rustrak_plugin::IntoResponse::into_response(e),
                };
            });
            call.push(quote! { #var });
        }
    }
    Ok(quote! {
        #[doc(hidden)]
        fn #handler(ctx: &::rustrak_plugin::Ctx) -> ::rustrak_plugin::__private::Response {
            #(#extract)*
            ::rustrak_plugin::IntoResponse::into_response(#name(#(#call),*))
        }
        ::rustrak_plugin::__private::inventory::submit! {
            ::rustrak_plugin::__private::RouteDef {
                method: #method,
                path: #path,
                handler: #handler,
            }
        }
        const _: () = {
            // Evaluated in every build, so `requires` is checked (and its
            // `Perm` used) in the plugin's native unit tests too.
            #[allow(dead_code)]
            const PARTS: &[&str] = &[#head, #requires, "}\n"];
            #[allow(dead_code)]
            const LEN: usize = ::rustrak_plugin::__private::joined_len(PARTS);
            // The section is the component's: a native build has none.
            #[cfg(target_arch = "wasm32")]
            #[unsafe(link_section = "rustrak:routes")]
            #[used]
            static ROUTE: [u8; LEN] = ::rustrak_plugin::__private::joined::<LEN>(PARTS);
        };
    })
}

/// `#[get("/path")]`, with `public` (by the project's DSN key), `instance`
/// (not tied to a project) or `requires = Perm::X.read()`.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("GET", attr, item)
}

/// See [`get`].
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("POST", attr, item)
}

/// See [`get`].
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("PUT", attr, item)
}

/// See [`get`].
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("PATCH", attr, item)
}

/// See [`get`].
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    route("DELETE", attr, item)
}

#[cfg(test)]
mod tests {
    use super::under_server;
    use std::path::Path;

    #[test]
    fn a_file_is_placed_by_its_path_in_the_crate_not_on_disk() {
        let root = Path::new("/work/apps/server/tests/fixtures/hello-plugin");
        // The crate itself sits under a folder named `server`: that is not
        // the plugin's server/.
        assert_eq!(under_server(Path::new(".rustrak/plugin.rs"), root), None);
        assert_eq!(
            under_server(Path::new(".rustrak/../src/server/routes/hits.rs"), root),
            Some("routes/hits.rs".to_string())
        );
        assert_eq!(
            under_server(&root.join("src/server/models/hit.rs"), root),
            Some("models/hit.rs".to_string())
        );
        assert_eq!(under_server(Path::new("tests/ui/route.rs"), root), None);
    }
}
