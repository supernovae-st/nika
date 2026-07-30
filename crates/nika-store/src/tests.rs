// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The crate test dir-module — the F-P8 suites, relocated out of
//! `tests/` so the mutation gate (`cargo mutants -- --lib`, Gate 5)
//! exercises them: integration targets never run under `--lib`, and a
//! gate that runs zero tests kills zero mutants. `src/tests.rs` is the
//! prod-LOC-exempt convention the hygiene vectors share (`rs_prod_files`
//! excludes the basename); each submodule carries its own
//! `#![allow(clippy::expect_used)]` header, the skip the unwrap vector
//! keys on. `crate::` resolves to the crate root — semantics unchanged.

#[cfg(test)]
mod acceptance;
#[cfg(test)]
mod api;
#[cfg(test)]
mod hardening;
#[cfg(test)]
mod tamper_property;

/// The test-side re-construction of the fold's set digest (H13): sort +
/// dedup, then blake3 over the concatenated 64-hex words. A sibling of
/// the impl's construction, written twice on purpose — a divergence in
/// either (the sort · the dedup · the framing) fails the cross-check.
#[cfg(test)]
pub(crate) fn set_digest(digests: &[String]) -> String {
    let mut sorted = digests.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut hasher = blake3::Hasher::new();
    for digest in &sorted {
        hasher.update(digest.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}
