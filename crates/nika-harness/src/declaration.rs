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
//! NIKA_HARNESS_ADAPTER=codex-acp            # the id (never a class token)
//! NIKA_HARNESS_COMMAND=codex-acp            # argv[0], never a shell line
//! NIKA_HARNESS_ARGS=--experimental-acp      # space-separated session argv
//! NIKA_HARNESS_VERSION_ARGS=--version       # optional · wrapper commands
//! NIKA_HARNESS_MIN=1.0                      # optional pin floor
//! NIKA_HARNESS_MAX_MAJOR=2                  # optional pin cap
//! NIKA_HARNESS_ENV=NO_COLOR,GEMINI_PROFILE  # optional passthrough names
//! ```

/// Read the configured adapter from the environment.
///
/// `Ok(None)` when no adapter is declared — the native loop keeps the
/// task, unchanged. `Err` when a seat IS declared but cannot be built:
/// the operator asked for a harness, so a silent native fallback would
/// substitute a different execution model behind their back (A-4).
///
/// # Errors
///
/// A declared adapter whose id collides with an access-class token, or
/// whose command is missing.
use crate::spawn::SpawnedHarness;

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
        return Err(format!(
            "harness adapter `{id}` is declared but NIKA_HARNESS_COMMAND is unset \
             — name the binary to drive, or unset NIKA_HARNESS_ADAPTER"
        ));
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
}
