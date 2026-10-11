// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_providers::{probe, resolve_access, route_identity}` and of
//! `nika_providers::profile::canonical_provider` after their items moved to the size-cap member
//! `nika_providers_access` (2026-10-10 · D-2026-07-09-N1 · the ADR-122 member note). This file
//! is an external consumer: it compiles against the providers paths, and they name the very
//! same items — the same types (a value of one path is a value of the other) and the same
//! functions (their signatures carry the member's types) — never copies. Nothing here reads the
//! environment, a home directory or the network.

use nika_providers::probe::{
    AccessClass, ExecutionLocus, KeyAuth, ModelListing, ProviderProbe, ProviderReadiness,
};
use nika_providers::resolve_access::{AccessCandidate, VerbNeeds};
use nika_providers_access as member;

/// Functions of the providers paths, typed with the member's types: a providers-side copy of
/// them could not be assigned here.
const CLASSIFY: fn(u16) -> member::probe::KeyAuth = nika_providers::probe::classify_http_status;
const CANDIDATES: fn(
    &[member::probe::ProviderProbe],
    &str,
) -> Vec<member::resolve_access::AccessCandidate> = nika_providers::resolve_access::candidates_for;
const CANONICAL: fn(&str) -> &str = nika_providers::profile::canonical_provider;
const ORIGIN: fn(&str) -> Option<String> = nika_providers::route_identity::route_origin;

#[test]
fn the_providers_paths_name_the_members_items() {
    // One probe row under the providers paths is the member's row.
    let locus: member::probe::ExecutionLocus =
        ExecutionLocus::classify(None, "http://127.0.0.1:11434");
    assert_eq!(locus, member::probe::ExecutionLocus::Loopback);
    let readiness: member::probe::ProviderReadiness =
        ProviderReadiness::new(true, true, None, None, false, locus, AccessClass::Local);
    let row: member::probe::ProviderProbe = ProviderProbe::new(
        "ollama",
        false,
        true,
        "",
        false,
        readiness,
        "http://127.0.0.1:11434",
    );
    let listing: member::probe::ModelListing = ModelListing::new("not probed");
    assert_eq!(listing.available(), None);
    // The functions answer as the member's, under every path.
    assert_eq!(CLASSIFY(401), KeyAuth::Unauthorized);
    assert_eq!(CLASSIFY(401), member::probe::classify_http_status(401));
    let candidates = CANDIDATES(std::slice::from_ref(&row), "ollama");
    assert_eq!(
        candidates,
        member::resolve_access::candidates_for(&[row], "ollama")
    );
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].access, "ollama");
    assert_eq!(candidates[0].class, AccessClass::Local);
    let local: member::resolve_access::AccessCandidate =
        AccessCandidate::new("ollama", AccessClass::Local, true);
    assert_eq!(local.class, candidates[0].class);
    let needs: member::resolve_access::VerbNeeds = VerbNeeds::new(true, false);
    assert_eq!(needs, member::resolve_access::VerbNeeds::new(true, false));
    assert_eq!(CANONICAL("grok"), member::canonical_provider("grok"));
    assert_eq!(CANONICAL("grok"), "xai");
    let endpoint = "https://api.deepseek.com/v1/chat/completions";
    assert_eq!(
        ORIGIN(endpoint),
        member::route_identity::route_origin(endpoint)
    );
    assert_eq!(
        ORIGIN(endpoint).as_deref(),
        Some("https://api.deepseek.com:443")
    );
}
