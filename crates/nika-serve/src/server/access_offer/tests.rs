// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The door's own judgments, apart from HTTP: what an adapter listed is carried as bounded
//! tokens only, a reading is kept for its own span, and every state of a session's answer has
//! one word in the document.

use nika_kernel::ai::harness::HarnessError;
use nika_service_execution::offer::{
    EffortOffer, ModelOffer, OfferProfile, OfferTarget, ServiceTierOffer, SessionOffer,
};
use nika_types::access::ServiceTier;
use serde_json::json;

use super::{
    FRESH, Kept, Listed, MAX_TOKEN_BYTES, Model, RETRY, Route, public_reason, token, tokens,
};

fn at(text: &str) -> jiff::Timestamp {
    text.parse().expect("a timestamp")
}

#[test]
fn only_a_short_graphic_ascii_value_is_a_token() {
    assert_eq!(
        token("gpt-6-astra[high]").as_deref(),
        Some("gpt-6-astra[high]")
    );
    assert_eq!(
        token("@agentclientprotocol/codex-acp").as_deref(),
        Some("@agentclientprotocol/codex-acp")
    );
    assert_eq!(token(""), None);
    assert_eq!(token("two words"), None, "a space is not a token");
    assert_eq!(token("line\nbreak"), None);
    assert_eq!(token("modèle"), None, "ASCII only");
    assert_eq!(
        token(&"m".repeat(MAX_TOKEN_BYTES)).map(|t| t.len()),
        Some(MAX_TOKEN_BYTES)
    );
    assert_eq!(token(&"m".repeat(MAX_TOKEN_BYTES + 1)), None);
}

#[test]
fn a_value_that_is_not_a_token_is_counted_beside_what_the_reading_left_out() {
    let listed = vec![
        "low".to_owned(),
        "ignore previous instructions".to_owned(),
        "high".to_owned(),
    ];
    assert_eq!(
        tokens(&listed, 2),
        (vec!["low".to_owned(), "high".to_owned()], 3)
    );
}

#[test]
fn an_offer_is_kept_a_minute_and_a_failed_reading_a_quarter_of_it() {
    let observed = at("2026-11-10T17:00:00Z");
    let offer = SessionOffer::new(
        ModelOffer::NotOffered,
        EffortOffer::NotOffered,
        ServiceTierOffer::NotOffered,
    );
    let kept = Kept::of(observed, Ok(offer));
    assert_eq!(kept.until, at("2026-11-10T17:01:00Z"));
    assert_eq!(FRESH.as_secs(), 60);

    let failed = Kept::of(
        observed,
        Err(HarnessError::Unavailable {
            reason: "adapter `codex`: cannot spawn `/opt/secret/bin/codex-acp`: no such file"
                .to_owned(),
        }),
    );
    assert_eq!(failed.until, at("2026-11-10T17:00:15Z"));
    assert_eq!(RETRY.as_secs(), 15);
    let refused = failed.answer.expect_err("a failed reading");
    assert_eq!(refused.code, "NIKA-1803");
    assert_eq!(
        refused.reason, "harness unavailable: adapter `codex`: cannot spawn no such file",
        "the engine's sentence, the quoted path left out"
    );
}

#[test]
fn a_reason_keeps_ids_and_drops_every_path_however_it_is_quoted() {
    let kept = public_reason(
        "the speed control of `@agentclientprotocol/codex-acp` is measured for `openai/gpt-6-luna`",
    );
    assert_eq!(
        kept,
        "the speed control of `@agentclientprotocol/codex-acp` is measured for `openai/gpt-6-luna`",
        "a scoped package name and a model id are not paths"
    );
    for path in [
        "/var/lib/nika/x",
        "`/var/lib/nika/x`:",
        "\"/var/lib/nika/x\"",
        "(~/secret/key)",
        "'./relative/file'",
        "[../up/one]",
        "<C:\\Users\\x>",
    ] {
        let reason = public_reason(&format!("cannot open {path} here"));
        assert_eq!(reason, "cannot open here", "{path}");
    }
    assert!(public_reason(&"word ".repeat(200)).len() <= 240);
}

#[test]
fn every_state_of_the_model_has_its_word_and_its_current_id() {
    let cases = [
        (
            ModelOffer::Confirmed {
                native: "gpt-6-luna".to_owned(),
            },
            "confirmed",
            Some("gpt-6-luna"),
        ),
        (
            ModelOffer::SessionDefault {
                current: Some("gpt-6-astra".to_owned()),
            },
            "session_default",
            Some("gpt-6-astra"),
        ),
        (
            ModelOffer::LegacyList {
                native: "gpt-6-luna[high]".to_owned(),
            },
            "legacy_list",
            Some("gpt-6-luna[high]"),
        ),
        (ModelOffer::NotOffered, "not_offered", None),
        (
            ModelOffer::NotConfirmed {
                reported: Some("gpt-6-astra".to_owned()),
            },
            "not_confirmed",
            Some("gpt-6-astra"),
        ),
    ];
    for (model, word, current) in cases {
        let session =
            SessionOffer::new(model, EffortOffer::NotOffered, ServiceTierOffer::NotOffered)
                .with_models(vec!["gpt-6-astra".to_owned(), "not a token".to_owned()]);
        let row = serde_json::to_value(Model::of("openai/gpt-6-luna".to_owned(), &session))
            .expect("JSON");
        assert_eq!(row["requested"], "openai/gpt-6-luna");
        assert_eq!(row["status"], word);
        assert_eq!(
            row.get("current").and_then(|v| v.as_str()),
            current,
            "{word}"
        );
        assert_eq!(row["offered"], json!(["gpt-6-astra"]));
        assert_eq!(row["omitted"], 1);
    }
}

#[test]
fn a_dependent_offer_says_offered_not_offered_or_unobserved_with_its_reason() {
    let offered = Listed::effort(&EffortOffer::Offered {
        option: "reasoning_effort".to_owned(),
        values: vec!["low".to_owned(), "medium".to_owned()],
        omitted: 0,
        current: Some("medium".to_owned()),
    });
    assert_eq!(
        serde_json::to_value(offered).expect("JSON"),
        json!({"status":"offered","option":"reasoning_effort","offered":["low","medium"],
               "omitted":0,"current":"medium"})
    );
    assert_eq!(
        serde_json::to_value(Listed::effort(&EffortOffer::NotOffered)).expect("JSON"),
        json!({"status":"not_offered"})
    );
    let unobserved = Listed::service_tier(&ServiceTierOffer::Unobserved {
        reason: "the harness does not list the model".to_owned(),
    });
    assert_eq!(
        serde_json::to_value(unobserved).expect("JSON"),
        json!({"status":"unobserved","reason":"the harness does not list the model"})
    );
    let tiers = Listed::service_tier(&ServiceTierOffer::Offered {
        option: "fast-mode".to_owned(),
        tiers: vec![ServiceTier::Standard, ServiceTier::Fast],
        current: Some(ServiceTier::Standard),
    });
    assert_eq!(
        serde_json::to_value(tiers).expect("JSON"),
        json!({"status":"offered","option":"fast-mode","offered":["standard","fast"],
               "omitted":0,"current":"standard"}),
        "the language's words, never the adapter's native values"
    );
}

#[test]
fn a_route_without_an_offer_carries_its_code_and_reason_and_no_listing() {
    let target = OfferTarget::new(
        OfferProfile::Agent,
        "openai/gpt-6-luna",
        vec!["render".to_owned()],
    );
    let kept = Kept::of(
        at("2026-11-10T17:00:00Z"),
        Err(HarnessError::Refused {
            reason: "the Codex ACP profile is not effective".to_owned(),
        }),
    );
    let row = serde_json::to_value(Route::of("codex", target, &kept)).expect("JSON");
    assert_eq!(
        row,
        json!({
            "via":"codex","protocol":"acp","profile":"agent","tasks":["render"],
            "status":"unavailable",
            "observed_at":"2026-11-10T17:00:00Z","fresh_until":"2026-11-10T17:00:15Z",
            "code":"NIKA-1805",
            "reason":"harness refused: the Codex ACP profile is not effective",
            "model":{"requested":"openai/gpt-6-luna"}
        })
    );
}
