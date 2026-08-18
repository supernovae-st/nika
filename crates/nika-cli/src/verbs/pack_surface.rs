// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded self-contained surface (spec §2) — `nika spec` ·
//! `nika spec --schema` · `nika try`.
//!
//! Everything reads `nika-pack` (the spec snapshot baked at build):
//! offline forever, version-pinned, zero network. `try`
//! executes the embedded example through the shipped L3 runtime (see
//! `verbs::run::example`).

use crate::verbs::VerbOutput;

/// `nika spec` — the embedded spec identity (+ `--canon` = the SSOT).
#[must_use]
pub fn spec(canon: bool) -> VerbOutput {
    if canon {
        return VerbOutput::ok(nika_pack::canon().to_owned());
    }
    VerbOutput::ok(format!(
        "nika language pack {} (embedded · self-contained)\n\n{}",
        nika_pack::pack_version(),
        nika_pack::manifest()
    ))
}

/// `nika spec --schema` — the JSON Schema for `*.nika.yaml` (machine surface).
#[must_use]
pub fn schema() -> VerbOutput {
    VerbOutput::ok(nika_pack::schema_json().to_owned())
}

// The showroom listing lives in `verbs::examples` (the organized
// corpus experience — tiers · titles · verb chips · full filenames);
// `run` executes via `verbs::run::example`.

/// The pack identity's two dumps behind one verb — the spec card,
/// `--canon`, or the JSON Schema — one verb over the pack identity.
#[must_use]
pub fn spec_or_schema(canon: bool, want_schema: bool) -> crate::verbs::VerbOutput {
    if want_schema { schema() } else { spec(canon) }
}

#[cfg(test)]
mod tests {
    /// The embedded `canon.yaml` (what `nika spec --canon` prints) is a
    /// vendored snapshot of the spec-repo SSOT. Its `builtins:` count MUST
    /// equal the catalog's `all_builtins()` length — the proven 1:1 (the
    /// catalog itself asserts `all_builtins().len() == 26`). When the embed
    /// drifts behind the SSOT (e.g. a builtin is added but `sync-pack.sh`
    /// wasn't re-run), this test is the structural catch: `nika spec
    /// --canon` would otherwise report a stale count to every consumer.
    ///
    /// Scans the top `counts:` block for the unique `builtins: <N>` line
    /// (the `builtins:` SECTION header below carries `count:`, not the key
    /// `builtins:`, so the line-scan is unambiguous). A malformed/missing
    /// line fails loudly rather than silently passing.
    #[test]
    fn embedded_canon_builtins_count_matches_catalog() {
        let canon = nika_pack::canon();
        // The unique `builtins: <N>` count line (None if the line is absent
        // or unparseable — that case must also fail the assertion below).
        let embedded: Option<usize> = canon.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix("builtins:")
                .and_then(|rest| rest.trim().parse::<usize>().ok())
        });
        let catalog = nika_catalog::all_builtins().len();
        // Comparing Option<usize> to Some(catalog) catches BOTH the count
        // mismatch (e.g. Some(22) ≠ Some(23)) AND a missing/malformed line
        // (None ≠ Some(23)) — no unwrap/expect/panic, fails loudly either way.
        assert_eq!(
            embedded,
            Some(catalog),
            "embedded canon.yaml `builtins:` count {embedded:?} is out of sync \
             with nika_catalog::all_builtins() ({catalog}) — re-vendor the pack \
             from the spec SSOT (scripts/sync-pack.sh <spec-checkout>)"
        );
    }
}
