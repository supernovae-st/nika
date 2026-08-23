// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reading the DECLARATION (D-2026-08-04-N1 · P3 B4.5) — WHICH
//! adapter this machine drives, read from the environment until the
//! B6 registry ships. It lives here, beside the adapter it builds:
//! nika-runtime carries only the two lines that wrap the result in a
//! seat (and sat 27 LOC over its 15k wall when it carried this).
//!
//! The seam was dead before this: `AgentVerb::with_harness_seat`
//! existed and nothing called it, so no `agent:` task could ever reach
//! a harness. This module is the wire, and it stays deliberately thin:
//! one env-declared adapter, no discovery, no fallback. A misconfigured
//! seat REFUSES loudly at composition (a silent native fallback would
//! be the substitution A-4 forbids — the operator asked for a harness).
//!
//! The env shape (provisional · the B6 registry replaces it with rows):
//!
//! ```text
//! NIKA_HARNESS_ADAPTER=codex                # the id (never a class token)
//! NIKA_HARNESS_COMMAND=codex-acp            # argv[0], never a shell line
//! NIKA_HARNESS_ARGS=--experimental-acp      # space-separated session argv
//! NIKA_HARNESS_VERSION_ARGS=--version       # optional · wrapper commands
//! NIKA_HARNESS_MIN=1.0                      # optional pin floor
//! NIKA_HARNESS_MAX_MAJOR=2                  # optional pin cap
//! NIKA_HARNESS_ENV=NO_COLOR,GEMINI_PROFILE  # optional passthrough names
//! ```

use crate::spawn::SpawnedHarness;

/// The declared adapter id, when the machine declares one (B6 · the
/// boot-manifest `harness_seat` stamp reads it — the trace names the
/// execution override beside the resolver's plan).
#[must_use]
pub fn declared_adapter_id() -> Option<String> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (compose's own)
    std::env::var("NIKA_HARNESS_ADAPTER")
        .ok()
        .filter(|v| !v.is_empty())
}

/// Read the configured adapter from the environment.
///
/// `Ok(None)` when no adapter is declared — the native loop keeps the
/// task, unchanged. `Err` when a seat IS declared but cannot be built:
/// the operator asked for a harness, so a silent native fallback would
/// substitute a different execution model behind their back (A-4).
///
/// # Errors
///
/// A declared adapter whose id collides with an access-class token,
/// whose command is missing, or whose pin does not parse.
pub fn seat_from_env() -> Result<Option<SpawnedHarness>, String> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (compose.rs' own)
    let lookup = |name: &str| std::env::var(name).ok();
    seat_from_lookup(&lookup)
}

/// [`seat_from_env`] over an INJECTED lookup — the pure half (the
/// `compose.rs::ladder_key` pattern). Tests drive this: writing the
/// process environment would need `unsafe` under Rust 2024, and the
/// workspace forbids it; injecting the reader is both sound and the
/// house shape.
///
/// # Errors
///
/// Same as [`seat_from_env`].
pub fn seat_from_lookup(
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<SpawnedHarness>, String> {
    let var = |name: &str| lookup(name).filter(|v| !v.is_empty());

    let Some(id) = var("NIKA_HARNESS_ADAPTER") else {
        return Ok(None);
    };
    let Some(command) = var("NIKA_HARNESS_COMMAND") else {
        // B6 · the registry lane: a bare id reads the shipped row (the
        // kill-switch applies — a disabled row is no row). The env
        // COMMAND/ARGS/… override below stays the LOCAL-DEV lane (an
        // unpinned adapter is honest for a locally-built binary, never
        // for a shipped row).
        return registry_seat(&id, lookup);
    };

    let mut adapter = crate::HarnessAdapter::new(&id, command)
        .map_err(|e| format!("harness adapter `{id}`: {e}"))?;
    if let Some(args) = var("NIKA_HARNESS_ARGS") {
        adapter = adapter.with_args(split_ws(&args));
    }
    if let Some(args) = var("NIKA_HARNESS_VERSION_ARGS") {
        adapter = adapter.with_version_args(split_ws(&args));
    }
    if let Some(names) = var("NIKA_HARNESS_ENV") {
        adapter = adapter.with_passthrough_env(split_list(&names));
    }
    // A pin is optional here and REQUIRED by the B6 registry: an
    // unpinned adapter is honest for a locally-built binary, never for
    // a shipped row.
    if let Some(min) = var("NIKA_HARNESS_MIN") {
        let (major, minor) = crate::parse_version(&min).ok_or_else(|| {
            format!("harness adapter `{id}`: NIKA_HARNESS_MIN=`{min}` is not MAJOR.MINOR")
        })?;
        let max_major = match var("NIKA_HARNESS_MAX_MAJOR") {
            Some(m) => m.parse::<u32>().map_err(|_| {
                format!("harness adapter `{id}`: NIKA_HARNESS_MAX_MAJOR=`{m}` is not a number")
            })?,
            None => major,
        };
        adapter = adapter.with_version_pin(crate::VersionPin::new((major, minor), max_major));
    }

    Ok(Some(SpawnedHarness::new(adapter)))
}

/// The B6 registry lane of the declaration: a bare
/// `NIKA_HARNESS_ADAPTER=<id>` seats the shipped row's pinned identity
/// (command · argv · version pin), never an env hand-picked binary.
/// An unknown (or kill-switched) id refuses, naming the live rows.
fn registry_seat(
    id: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<SpawnedHarness>, String> {
    let rows = crate::registry::registry_with(&|name| lookup(name)).map_err(|e| e.to_string())?;
    let Some(row) = rows.iter().find(|r| r.adapter.id == id) else {
        let known: Vec<&str> = rows.iter().map(|r| r.adapter.id.as_str()).collect();
        return Err(format!(
            "harness adapter `{id}` is not in the registry (known: {}) — \
             or set NIKA_HARNESS_COMMAND for a local-dev adapter",
            if known.is_empty() {
                "none — all disabled".to_owned()
            } else {
                known.join(", ")
            }
        ));
    };
    Ok(Some(SpawnedHarness::new(row.adapter.clone())))
}

/// Seat a shipped registry row by product token (`claude-code` · `codex`).
/// Retired aliases resolve to the live row so an old env still composes.
///
/// # Errors
///
/// The id is unknown, kill-switched, or the row cannot be built.
pub fn seat_from_id(id: &str) -> Result<Option<SpawnedHarness>, String> {
    let canonical = nika_types::access::HarnessRuntime::lookup(id)
        .or_else(|| nika_types::access::HarnessRuntime::retired_alias(id))
        .map_or(id, |rt| rt.id);
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (kill-switch)
    let lookup = |name: &str| std::env::var(name).ok();
    registry_seat(canonical, &lookup)
}

fn split_ws(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arg_splitters_are_the_shapes_the_env_carries() {
        assert_eq!(
            split_ws("--experimental-acp --json"),
            vec!["--experimental-acp", "--json"]
        );
        assert!(split_ws("   ").is_empty());
        assert_eq!(
            split_list("NO_COLOR, GEMINI_PROFILE ,"),
            vec!["NO_COLOR", "GEMINI_PROFILE"]
        );
        assert!(split_list(",,").is_empty());
    }

    /// B6 · the registry lane: a bare known id seats the shipped row's
    /// pinned identity (never an env-picked binary); a kill-switched or
    /// unknown id refuses with the live rows named.
    #[test]
    fn a_bare_known_id_seats_the_registry_row() {
        let env = |name: &str| (name == "NIKA_HARNESS_ADAPTER").then(|| "gemini-cli".to_owned());
        let seat = seat_from_lookup(&env)
            .expect("a known id composes")
            .expect("and yields a seat");
        // The row's own identity — the probe pin, not an env var.
        let dbg = format!("{seat:?}");
        assert!(dbg.contains("gemini"), "{dbg}");
    }

    #[test]
    fn a_kill_switched_id_refuses_naming_the_live_rows() {
        let env = |name: &str| match name {
            "NIKA_HARNESS_ADAPTER" | "NIKA_HARNESS_DISABLE" => Some("codex".to_owned()),
            _ => None,
        };
        let err = seat_from_lookup(&env).expect_err("a disabled row is no row");
        assert!(err.contains("codex"), "{err}");
        assert!(err.contains("gemini-cli"), "the live rows are named: {err}");
    }

    #[test]
    fn an_unknown_id_refuses_and_teaches_the_local_dev_lane() {
        let env =
            |name: &str| (name == "NIKA_HARNESS_ADAPTER").then(|| "my-own-harness".to_owned());
        let err = seat_from_lookup(&env).expect_err("unknown ids refuse");
        assert!(err.contains("my-own-harness"), "{err}");
        assert!(err.contains("NIKA_HARNESS_COMMAND"), "{err}");
    }

    #[test]
    fn seat_from_id_seats_the_product_token() {
        let seat = seat_from_id("claude-code")
            .expect("known token composes")
            .expect("and yields a seat");
        let dbg = format!("{seat:?}");
        assert!(
            dbg.contains("claude-agent-acp") || dbg.contains("claude-code"),
            "{dbg}"
        );
    }
}
