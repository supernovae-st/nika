// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Proc-macros for Nika workflow engine.
//!
//! Provides derive macros and attribute macros that eliminate boilerplate:
//!
//! - `#[derive(NikaErrorCode)]` — auto-generates `code()` method from `#[nika_code("NIKA-XXX")]`
//! - `#[derive(EventTaskId)]` — auto-generates `task_id()` from `#[has_task_id]` on variants
//! - `#[builtin_tool]` — generates `BuiltinTool` impl from an async function

extern crate proc_macro;

mod nika_error_code;

/// Derive macro that generates a `code() -> &'static str` method for error enums.
///
/// Each variant must be annotated with one of:
/// - `#[nika_code("NIKA-XXX")]` — returns the literal string
/// - `#[nika_code(delegate)]` — calls `.code()` on the inner value (tuple variants only)
///
/// # Example
///
/// ```ignore
/// #[derive(NikaErrorCode)]
/// enum MyError {
///     #[nika_code("MY-001")]
///     ParseError { details: String },
///
///     #[nika_code(delegate)]
///     Inner(OtherError),
/// }
/// ```
#[proc_macro_derive(NikaErrorCode, attributes(nika_code))]
pub fn derive_nika_error_code(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    nika_error_code::derive(&input).into()
}
