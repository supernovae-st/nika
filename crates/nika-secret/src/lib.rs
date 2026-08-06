#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow secret resolution — the `secrets:` namespace seam
//! (MINOR-B) · descended from `nika-runtime` 2026-08-06 at the 15k
//! prod-LOC wall (the `nika-proof`/`nika-dap`/`nika-cap` precedent):
//! the RESOLUTION half is pure and depends on no runtime type, so it
//! moves whole. The dynamic scrub (`RedactingSink`) stays behind — it
//! implements the runtime's own `EventSink` seam.
//!
//! The envelope's `secrets:` block declares each value as a STORE REFERENCE
//! (`{ source, key }` · never an inline literal · spec 01 §secrets). At run
//! time the engine resolves those references into the values the
//! `${{ secrets.X }}` namespace exposes — but resolution READS a store
//! (env var · file · vault), which is an EFFECT. So the runtime (L3) never
//! reads env/files itself: the COMPOSER (L4 · nika-cli, the sanctioned
//! `std::env` boundary) injects a [`WorkflowSecretResolver`].
//!
//! ## Sovereignty + masking
//!
//! A resolved value flows ONLY where the static IFC (`nika check`
//! secret-flow analysis · ADR-092) sanctions it, and is NEVER written to the
//! event stream (notes carry the program/model name, not field values). The
//! resolved values live in the `Scope`'s `secrets`
//! map for the duration of the run and are dropped with it.
//!
//! The "never written" half is enforced DYNAMICALLY (S1 · journal
//! hygiene): the IFC sees declared flows, not a value that surfaces
//! through a side channel — an `exec` that cats a file-sourced secret,
//! an mcp tool that echoes its input. Such a value lands in a task's
//! OUTPUT and would ride the terminal frame's `outcome`/`output`
//! payload into `.nika/traces/*.ndjson` in plaintext. `RedactingSink`
//! wraps the run's ONE sink seam and rewrites every occurrence of a
//! resolved value to [`REDACTED`] before any lane (journal · `--json` ·
//! the live fold) sees the event. Redaction follows PROVENANCE — any
//! value the map resolved, down to one byte (P0-19 · the PIN/OTP
//! class) — never a length floor; a short needle only narrows its blast
//! radius to the value-payload fields (the scrub half lives in
//! `nika-runtime::secret`, which owns the event lane it masks).
//!
//! ## Fail-closed
//!
//! A reference that does not resolve (env var unset · file missing) is
//! simply OMITTED from the namespace — so `${{ secrets.X }}` then raises the
//! loud unresolved class (`NIKA-1702`), a clean typed error, BEFORE the
//! value is ever needed. It is NEVER a panic, and NEVER a silent empty value
//! (which would turn `${{ secrets.X }}` into the literal string "null"). A
//! workflow that declares a secret it never references is unaffected (the
//! omission only bites a reference) — the same posture as today, where every
//! `secrets.X` was unresolved.

use std::collections::BTreeMap;

use nika_schema::types::{SecretRef, SecretSource};
use serde_json::Value;

/// Resolve a workflow `secrets:` reference into its value.
///
/// The composer (L4) implements this over the real stores (env · file ·
/// vault). The runtime calls it once per declared secret at run start.
pub trait WorkflowSecretResolver: Send + Sync {
    /// Resolve one `{ source, key }` reference to its plaintext value.
    ///
    /// The implementor MUST NOT log the returned value. `name` is the
    /// `secrets.<name>` key (for diagnostics only · safe to log).
    ///
    /// # Errors
    ///
    /// [`SecretResolveError`] when the reference cannot be resolved (store
    /// miss · source not supported by this resolver). The run fails cleanly.
    fn resolve(&self, name: &str, reference: &SecretRef) -> Result<String, SecretResolveError>;
}

/// A workflow secret reference that did not resolve (MINOR-B).
///
/// Surfaced by a [`WorkflowSecretResolver`] when a store lookup misses. The
/// runtime treats it as « leave this `secrets.<name>` unbound » → a later
/// `${{ secrets.<name>` reference raises `NIKA-1702` (the loud unresolved
/// class · clean typed error · never a panic). The message NEVER contains a
/// value (there is none — resolution failed) nor the store contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretResolveError {
    /// The `secrets.<name>` that failed (safe to surface · not the value).
    pub name: String,
    /// Why it failed (store miss · unsupported source · safe to surface).
    pub reason: String,
}

impl std::fmt::Display for SecretResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secrets.{} did not resolve · {}", self.name, self.reason)
    }
}

impl std::error::Error for SecretResolveError {}

/// The no-op resolver — the default when the composer injects none. Every
/// `secrets.X` then resolves to `None` (NIKA-1702 · the prior behavior ·
/// secrets fail CLOSED). Used by tests + any headless run with no store.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSecrets;

impl WorkflowSecretResolver for NoSecrets {
    fn resolve(&self, name: &str, _reference: &SecretRef) -> Result<String, SecretResolveError> {
        Err(SecretResolveError {
            name: name.to_owned(),
            reason: "no secret resolver is configured for this run".to_owned(),
        })
    }
}

/// Resolve every declared `secrets:` reference into the `secrets` namespace
/// map (`<name>` → the value as a JSON string · MINOR-B).
///
/// Called once at run start. A reference that does not resolve is OMITTED
/// (fail-closed · the later `${{ secrets.X }}` reference then raises
/// `NIKA-1702`, never reads "null") — a declared-but-unreferenced secret is
/// therefore harmless. The returned map is bound in the run's
/// `Scope`.
///
/// `declared` is `wf.secrets` (`<name>` → reference).
#[must_use]
pub fn resolve_secrets<R: WorkflowSecretResolver + ?Sized>(
    resolver: &R,
    declared: &[(
        nika_schema::Spanned<String>,
        nika_schema::Spanned<SecretRef>,
    )],
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (name, reference) in declared {
        // A miss leaves the secret UNBOUND (omitted) — fail-closed: the
        // reference site raises NIKA-1702, never reads a silent "null".
        if let Ok(value) = resolver.resolve(&name.value, &reference.value) {
            out.insert(name.value.clone(), Value::String(value));
        }
    }
    out
}

/// Whether a [`SecretSource`] is runtime-resolvable by the canonical
/// composer resolver today (`env` · `file`). `vault` is not yet wired — the
/// checker WARNs rather than letting a green check fail at runtime
/// (NIKA-1702). Exposed so the CLI's `check` can surface the warning.
#[must_use]
pub fn source_is_runtime_resolvable(source: SecretSource) -> bool {
    matches!(source, SecretSource::Env | SecretSource::File)
}

// ─────────────────────── S1 · journal hygiene ───────────────────────

/// The redaction marker — a resolved secret value that reaches an event
/// payload is rewritten to these exact bytes. Fixed width: the marker
/// must not encode anything about the value it hides, not even a length.
/// The masking token every redacted value becomes.
pub const REDACTED: &str = "***";
