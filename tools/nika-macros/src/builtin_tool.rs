// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `#[builtin_tool]` — attribute macro that generates `BuiltinTool` impl from an async fn.
//!
//! ```ignore
//! #[builtin_tool(name = "echo", description = "Echo back")]
//! async fn echo_tool(params: EchoParams) -> Result<EchoResponse, NikaError> { ... }
//! ```
//!
//! Generates:
//! - `pub struct EchoToolStub;` (ZST)
//! - `impl BuiltinTool for EchoToolStub` with name, description, call
//!
//! # Error type contract
//!
//! The error type (default: `crate::NikaError`, configurable via `error = "..."`)
//! must have these two constructors:
//! - `fn invalid_params(tool: &str, reason: impl Display) -> Self`
//! - `fn tool_error(tool: &str, reason: impl Display) -> Self`

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, FnArg, ItemFn};

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the function
    let func: ItemFn = match parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    // Parse attributes as key-value pairs
    let attrs = match parse_kv_attrs(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    // Validate: must be async
    if func.sig.asyncness.is_none() {
        return syn::Error::new_spanned(func.sig.fn_token, "#[builtin_tool] requires an async fn")
            .to_compile_error();
    }

    // Extract params type from first argument
    let params_type = match extract_params_type(&func) {
        Ok(ty) => ty,
        Err(e) => return e.to_compile_error(),
    };

    // Parse paths with proper error handling (no .expect())
    let error_path: syn::Path = match syn::parse_str(
        attrs.error.as_deref().unwrap_or("crate::NikaError"),
    ) {
        Ok(p) => p,
        Err(e) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("invalid error path: {e}"),
            )
            .to_compile_error();
        }
    };
    let trait_path: syn::Path = match syn::parse_str(
        attrs
            .trait_path
            .as_deref()
            .unwrap_or("crate::runtime::builtin::BuiltinTool"),
    ) {
        Ok(p) => p,
        Err(e) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("invalid trait_path: {e}"),
            )
            .to_compile_error();
        }
    };

    // Generate stub struct name from function name
    let fn_name = &func.sig.ident;
    let stub_name = generate_stub_name(fn_name);

    let tool_name = &attrs.name;
    let tool_desc = attrs.description.as_deref().unwrap_or("");
    let nika_prefix = format!("nika:{}", tool_name);

    quote! {
        // Keep the original function
        #func

        /// Auto-generated ZST stub for the builtin tool.
        pub struct #stub_name;

        impl #trait_path for #stub_name {
            fn name(&self) -> &'static str {
                #tool_name
            }

            fn description(&self) -> &'static str {
                #tool_desc
            }

            fn call<'a>(
                &'a self,
                args: String,
            ) -> ::std::pin::Pin<
                Box<dyn ::std::future::Future<Output = Result<String, #error_path>> + Send + 'a>,
            > {
                Box::pin(async move {
                    let params: #params_type = serde_json::from_str(&args)
                        .map_err(|e| #error_path::invalid_params(
                            #nika_prefix,
                            e,
                        ))?;
                    let result = #fn_name(params).await?;
                    serde_json::to_string(&result)
                        .map_err(|e| #error_path::tool_error(
                            #nika_prefix,
                            e,
                        ))
                })
            }
        }
    }
}

/// Parsed key-value attributes.
struct KvAttrs {
    name: String,
    description: Option<String>,
    error: Option<String>,
    trait_path: Option<String>,
}

/// Parse `name = "...", description = "...", error = "...", trait_path = "..."`
fn parse_kv_attrs(tokens: TokenStream) -> Result<KvAttrs, syn::Error> {
    // Parse as punctuated list of Meta::NameValue
    let pairs: syn::punctuated::Punctuated<syn::MetaNameValue, syn::Token![,]> =
        syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::parse_terminated,
            tokens,
        )?;

    let mut name = None;
    let mut description = None;
    let mut error = None;
    let mut trait_path = None;

    for pair in &pairs {
        let key = pair
            .path
            .get_ident()
            .ok_or_else(|| syn::Error::new_spanned(&pair.path, "expected simple identifier"))?
            .to_string();

        // syn::Lit and syn::Expr are #[non_exhaustive] — wildcard required
        #[allow(clippy::wildcard_enum_match_arm)]
        let value = match &pair.value {
            syn::Expr::Lit(lit) => match &lit.lit {
                syn::Lit::Str(s) => s.value(),
                other => {
                    return Err(syn::Error::new_spanned(other, "expected string literal"));
                }
            },
            other => {
                return Err(syn::Error::new_spanned(other, "expected string literal"));
            }
        };

        match key.as_str() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            "error" => error = Some(value),
            "trait_path" => trait_path = Some(value),
            other => {
                return Err(syn::Error::new_spanned(
                    &pair.path,
                    format!("unknown attribute: {other}"),
                ));
            }
        }
    }

    let name = name.ok_or_else(|| {
        syn::Error::new(proc_macro2::Span::call_site(), "missing `name` attribute")
    })?;

    Ok(KvAttrs {
        name,
        description,
        error,
        trait_path,
    })
}

/// Extract the params type from the first function argument.
fn extract_params_type(func: &ItemFn) -> Result<syn::Type, syn::Error> {
    let first_arg = func.sig.inputs.first().ok_or_else(|| {
        syn::Error::new_spanned(
            &func.sig,
            "#[builtin_tool] function must have exactly one parameter",
        )
    })?;

    match first_arg {
        FnArg::Typed(pat_type) => Ok((*pat_type.ty).clone()),
        FnArg::Receiver(_) => Err(syn::Error::new_spanned(
            first_arg,
            "#[builtin_tool] function must not take self",
        )),
    }
}

/// Generate stub struct name from function name.
/// `echo_tool` → `EchoToolStub`, `sleep` → `SleepStub`
fn generate_stub_name(fn_name: &syn::Ident) -> syn::Ident {
    let name_str = fn_name.to_string();
    let pascal: String = name_str
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect();
    format_ident!("{}Stub", pascal)
}
