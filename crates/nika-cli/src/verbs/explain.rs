// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain NIKA-XXXX` — teach one error code (spec §2).
//!
//! Reads the canonical registry (`nika-error::codes`) — cause category,
//! severity, slug and the fix-form help text. Never invents: an unknown
//! code is a finding (`exit 2`), not a guess.

use nika_error::codes::{Category, Severity, code_help, lookup};

use crate::verbs::VerbOutput;

/// The `nika explain <code>` verb. Accepts `NIKA-440` or bare `440`.
#[must_use]
pub fn run(wire: &str) -> VerbOutput {
    let normalized = if wire.starts_with("NIKA-") {
        wire.to_owned()
    } else {
        format!("NIKA-{wire}")
    };
    let Some(code) = lookup(&normalized) else {
        return VerbOutput::file(format!(
            "unknown code `{wire}` — the registry knows NIKA-001..NIKA-9999 \
             (allocated ranges only); see docs.nika.sh/errors"
        ));
    };
    let text = format!(
        "{code} · {category} · {severity} · {slug}\n\n  {help}\n",
        category = category_label(code.category),
        severity = severity_label(code.severity),
        slug = code.slug,
        help = code_help(code),
    );
    VerbOutput::ok(text)
}

/// Stable lowercase label — the OUTPUT contract must not ride a `Debug`
/// derive (enum renames would silently change the surface).
fn category_label(category: Category) -> &'static str {
    match category {
        Category::Core => "core",
        Category::Shell => "shell",
        Category::FileIo => "file-io",
        Category::Http => "http",
        Category::Auth => "auth",
        Category::Mcp => "mcp",
        Category::Schema => "schema",
        Category::Binding => "binding",
        Category::Provider => "provider",
        Category::Verb => "verb",
        Category::Runtime => "runtime",
        Category::Memory => "memory",
        Category::WasmPlugin => "wasm-plugin",
        Category::Sandbox => "sandbox",
        Category::Screen => "screen",
        Category::Ocr => "ocr",
        Category::A11y => "a11y",
        Category::Input => "input",
        Category::Browser => "browser",
        Category::Vision => "vision",
        Category::Audio => "audio",
        _ => "other", // #[non_exhaustive] future categories
    }
}

/// Stable lowercase severity label (same contract rule).
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "other", // #[non_exhaustive] future severities
    }
}
