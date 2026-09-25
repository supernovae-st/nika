// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The strict conversation reaches the SAME admission account, without network.
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::admission::{CostHostEvidence, CostReview, CostRoute};
use crate::{AdmissionState, ProvidersConfig};
use nika_types::cost::Cost;
use std::time::{Duration, Instant};
fn pending() -> PendingCostReview {
    let route = CostRoute::observe("deepseek/unpriced-fixture", ProvidersConfig::new()).unwrap();
    PendingCostReview::new(
        CostReview::new(
            "candidate".into(),
            "invocation".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap(),
        "source".into(),
        "inputs".into(),
    )
}
#[test]
fn matching_reply_consumes_one_live_review_with_the_same_finite_account() {
    let p = pending();
    let answer = p.challenge().response(true);
    let route = p.challenge().route.clone();
    let a = p.confirm(&answer, "candidate", &route).unwrap();
    let mut attempt = a
        .reserve(&route.provider, &route.model, &route.endpoint, 8192)
        .unwrap();
    assert!(
        a.reserve(&route.provider, &route.model, &route.endpoint, 8192)
            .is_err()
    );
    attempt.sent().unwrap();
    drop(attempt);
    let r = a.snapshot().unwrap();
    assert_eq!(r.unknown_calls, 1);
    assert_eq!(r.billed, None);
    assert_eq!(r.state, AdmissionState::Uncertain);
    assert!(
        a.reserve(&route.provider, &route.model, &route.endpoint, 8192)
            .is_err()
    );
}
#[test]
fn decline_cross_child_wrong_binding_and_expired_review_refuse() {
    let route = pending().challenge().route.clone();
    let p = pending();
    let no = p.challenge().response(false);
    assert!(p.confirm(&no, "candidate", &route).is_err());
    let stale = pending().challenge().response(true);
    assert!(pending().confirm(&stale, "candidate", &route).is_err());
    for field in [
        "model",
        "endpoint",
        "inputs",
        "source",
        "candidate",
        "invocation",
    ] {
        let p = pending();
        let mut response = p.challenge().response(true);
        match field {
            "model" => response.challenge.route.model.push('x'),
            "endpoint" => response.challenge.route.endpoint.push('x'),
            "inputs" => response.challenge.inputs_sha256.push('x'),
            "source" => response.challenge.source_sha256.push('x'),
            "candidate" => response.challenge.candidate.push('x'),
            _ => response.challenge.invocation.push('x'),
        }
        assert!(
            p.confirm(&response, "candidate", &route).is_err(),
            "{field}"
        );
    }
    let mut p = pending();
    p.review.reviewed_at = Instant::now()
        .checked_sub(Duration::from_secs(301))
        .unwrap();
    let response = p.challenge().response(true);
    assert!(p.confirm(&response, "candidate", &route).is_err());
    let p = pending();
    let response = p.challenge().response(true);
    assert!(p.confirm(&response, "changed", &route).is_err());
    let p = pending();
    let response = p.challenge().response(true);
    let mut changed = route;
    changed.endpoint.push_str("/changed");
    assert!(p.confirm(&response, "candidate", &changed).is_err());
}
#[test]
fn duplicate_frames_keys_unknown_versions_and_truncated_replies_refuse() {
    let encoded = serde_json::to_string(&pending().challenge().response(true)).unwrap();
    for data in [
        format!("{encoded}\n{encoded}"),
        encoded.replace("\"yes\":true", "\"yes\":true,\"yes\":true"),
        "{}".into(),
        String::new(),
        encoded[..encoded.len() - 1].into(),
    ] {
        assert!(
            serde_json::from_str::<CostResponse>(&data).is_err(),
            "{data}"
        );
    }
    let p = pending();
    let mut answer = p.challenge().response(true);
    answer.schema.push('2');
    let route = p.challenge().route.clone();
    assert!(p.confirm(&answer, "candidate", &route).is_err());
}
#[test]
fn display_uses_the_account_catalog_and_keeps_native_currency_separate() {
    let route = CostRoute::observe(
        "openai/gpt-oss-120b",
        ProvidersConfig::new()
            .with_base_url("openai", "https://api.scaleway.ai/v1/chat/completions"),
    )
    .unwrap();
    let p = PendingCostReview::new(
        CostReview::new(
            "candidate".into(),
            "invocation".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap(),
        "sha-source".into(),
        "sha-inputs".into(),
    );
    let text = p.challenge().display();
    for fact in [
        "openai/gpt-oss-120b at https://api.scaleway.ai\n",
        "EUR",
        "USD cost is unknown",
        "8192",
        "120 seconds",
        "1 requests",
        "final charge/invoice unknown",
        "no currency conversion",
    ] {
        assert!(text.contains(fact), "{fact}: {text}");
    }
    assert!(!text.contains("/v1/chat/completions"), "{text}");
    let details = p.challenge().details();
    for fact in [
        "Endpoint: https://api.scaleway.ai/v1/chat/completions\n",
        "Source SHA-256: sha-source\n",
        "Input SHA-256: sha-inputs\n",
        "no currency conversion",
    ] {
        assert!(details.contains(fact), "{fact}: {details}");
    }
}

/// A review whose every identity is a token no sentence of the copy contains.
fn distinct(invocation_default: Option<Cost>) -> PendingCostReview {
    let route = CostRoute::observe("deepseek/unpriced-fixture", ProvidersConfig::new()).unwrap();
    PendingCostReview::new(
        CostReview::new(
            "cand-7f3a".into(),
            "inv-7f3a".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            invocation_default,
            None,
        )
        .unwrap(),
        "src-7f3a".into(),
        "in-7f3a".into(),
    )
}

#[test]
fn the_first_screen_reads_as_one_decision_and_keeps_identities_on_details() {
    let p = distinct(Some(Cost::new(250_000_000)));
    let c = p.challenge();
    let first = c.display();
    let origin = origin(&c.route.endpoint);
    assert!(
        first.starts_with(&format!(
            "Fresh Run cost decision · deepseek/{} at {origin}\n",
            c.route.model
        )),
        "{first}"
    );
    for fact in [
        "USD cost is unknown; a charge is possible",
        "At most 1 requests; each at most 8192 output tokens and 120 seconds",
        "Any schema re-asks consume this same request bound.",
        "No automatic transport retry.",
        "(invocation: $0.250000; project: none)",
        "no hard cap is overridden",
        "Native currency: price and invoice unknown\n",
        "Approves this Run once; an authoring or Save approval never approves a Run.",
    ] {
        assert!(first.contains(fact), "{fact}: {first}");
    }
    assert!(
        first.ends_with("\nContinue once? yes / no / details"),
        "{first}"
    );
    assert_eq!(first.matches("Continue once?").count(), 1, "{first}");
    for identity in [
        "cand-7f3a",
        "inv-7f3a",
        "src-7f3a",
        "in-7f3a",
        "SHA-256",
        "CostHostEvidence",
        c.nonce.as_str(),
    ] {
        assert!(
            !first.contains(identity),
            "{identity} on the first screen: {first}"
        );
    }
    let details = c.details();
    for evidence in [
        format!("challenge {}", c.nonce),
        format!("Endpoint: {}\n", c.route.endpoint),
        "Source SHA-256: src-7f3a\n".into(),
        "Input SHA-256: in-7f3a\n".into(),
        "Candidate: cand-7f3a\n".into(),
        "Invocation: inv-7f3a\n".into(),
        "Native currency: price and invoice unknown\n".into(),
        "Host and cap evidence: candidate cand-7f3a".into(),
        "NotApplicable".into(),
    ] {
        assert!(details.contains(&evidence), "{evidence}: {details}");
    }
    assert!(details.ends_with("\nContinue once? yes / no"), "{details}");
}

#[test]
fn both_screens_are_pure_projections_of_the_same_challenge() {
    let p = distinct(None);
    let before = p.challenge().clone();
    let first = p.challenge().details();
    assert_eq!(p.challenge().details(), first);
    assert_eq!(p.challenge().display(), before.display());
    assert_eq!(p.challenge(), &before, "reading changed the challenge");
    assert!(
        first.contains(&format!("challenge {} ·", before.nonce)),
        "{first}"
    );
}

#[test]
fn the_origin_keeps_scheme_and_host_and_the_body_drops_only_the_closing_line() {
    for (endpoint, shown) in [
        (
            "https://api.scaleway.ai/v1/chat/completions",
            "https://api.scaleway.ai",
        ),
        (
            "https://user:secret@h.example:8443/v1?key=x#frag",
            "https://h.example:8443",
        ),
        ("https://api.deepseek.com", "https://api.deepseek.com"),
        ("host.example/v1", "host.example"),
    ] {
        assert_eq!(origin(endpoint), shown, "{endpoint}");
    }
    assert_eq!(question_body("A.\nB.\nContinue once? yes / no"), "A.\nB.");
    assert_eq!(
        question_body("A.\nContinue? yes / no"),
        "A.\nContinue? yes / no"
    );
}
