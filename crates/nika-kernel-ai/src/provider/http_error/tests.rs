// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The provider's own message: relayed for the person, one bounded line, no secret.

use super::{MESSAGE_CHARS, ProviderHttpError, WITHHELD};

const CREDIT: &str = "Your credit balance is too low to access the Anthropic API. Please go to \
                      Plans & Billing to upgrade or purchase credits.";

fn said(status: u16, message: &str, withheld: &[&str]) -> ProviderHttpError {
    ProviderHttpError::new(status, None, None, None).with_message(message, withheld)
}

#[test]
fn the_person_reads_what_the_provider_said() {
    let details = ProviderHttpError::new(400, None, Some("invalid_request_error"), None)
        .with_message(CREDIT, &[]);
    assert_eq!(details.message(), Some(CREDIT));
    assert_eq!(
        details.to_string(),
        format!(
            "provider API error (HTTP 400); type=invalid_request_error; the provider said: \
             \"{CREDIT}\"; usage and billing unknown"
        )
    );
}

#[test]
fn what_the_provider_said_is_relayed_never_classified() {
    for (status, transient) in [(400, false), (429, true), (503, true)] {
        let details = said(status, "insufficient_quota credit_balance_exhausted", &[]);
        assert!(!details.is_quota_exhausted(), "prose is never classified");
        assert_eq!(details.is_transient(), transient);
        assert_eq!((details.code(), details.error_type()), (None, None));
    }
}

#[test]
fn nothing_said_attaches_nothing() {
    for raw in ["", "  ", "\n\t\r", "\u{200B}\u{202E}\u{FEFF}"] {
        let details = said(400, raw, &[]);
        assert_eq!(details.message(), None, "{raw:?}");
        assert_eq!(
            details.to_string(),
            "provider API error (HTTP 400); usage and billing unknown"
        );
    }
}

#[test]
fn the_credential_the_call_sent_never_survives() {
    // Unprefixed and short: only the value itself can recognise these.
    for key in ["Q7Z9-K2M4", "8F3A9C1D", "ZZ.91~QX", "K2_M4+Z9/Q7"] {
        let spread: String = key.chars().flat_map(|c| [c, '\u{200B}']).collect();
        for echo in [key.to_owned(), spread] {
            for raw in [
                echo.clone(),
                format!("Invalid key {echo}."),
                format!("key '{echo}' refused; {echo} again"),
                format!("x{echo}y"),
            ] {
                let details = said(401, &raw, &[key]);
                let shown = format!("{details} {details:?}");
                assert!(!shown.contains(key), "{shown}");
                assert!(shown.contains(WITHHELD), "{shown}");
            }
        }
    }
}

#[test]
fn credential_shaped_words_are_withheld_without_being_known() {
    for secret in [
        "sk-ant-api03-AbCdEf0123456789",
        "sk-proj-****************************abcd",
        "sk-secret",
        "AIzaSyA1b2C3d4E5f6G7h8I9j0",
        "gsk_0123456789abcdefABCDEF",
        "hf_AbCdEfGhIjKlMnOp",
        "nvapi-0123456789abcdef",
        "xai-AbCdEf012345",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl",
        "AKIAIOSFODNN7EXAMPLE",
        "0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d",
        "AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEf",
        "baaf5fa1-4db8-4636-8dbd-81ce7cf789f0",
        "req_011CXyZ0123456789abc",
    ] {
        let details = said(401, &format!("refused: {secret}, retry"), &[]);
        let expected = format!("refused: {WITHHELD}, retry");
        assert_eq!(details.message(), Some(expected.as_str()), "{secret}");
    }
    for (raw, shown) in [
        (
            "Authorization: Bearer abc123",
            "Authorization: [withheld] [withheld]",
        ),
        (
            "Your api key: ****abcd is invalid",
            "Your api key: [withheld] is invalid",
        ),
        (
            "sent api_key=hunter2&model=x",
            "sent api_key=[withheld]&model=x",
        ),
        ("use Bearer tok", "use Bearer [withheld]"),
    ] {
        assert_eq!(said(401, raw, &[]).message(), Some(shown), "{raw}");
    }
    for kept in [
        "claude-haiku-4-5-20251001",
        "gpt-4o-mini-2024-07-18",
        "deepseek-v4-flash-0731",
        "Qwen2.5-72B-Instruct",
        "GenerateContentRequest.contents",
        "https://platform.openai.com/account/api-keys",
        "generativelanguage.googleapis.com/generate_content_free_tier_requests",
        "max_completion_tokens",
        "128000",
    ] {
        let raw = format!("see {kept} here");
        assert_eq!(said(400, &raw, &[]).message(), Some(raw.as_str()));
    }
}

#[test]
fn what_the_provider_said_is_one_line_a_terminal_cannot_be_steered_by() {
    let raw = "one\r\n\ttwo \u{1b}[31mred\u{1b}[0m \u{202E}desrever\u{202C} zero\u{200B}width \
               \u{E0041}\u{E0042}tags";
    assert_eq!(
        said(400, raw, &[]).message(),
        Some("one two [31mred [0m desrever zerowidth tags")
    );
}

#[test]
fn a_long_message_is_cut_at_its_bound() {
    for unit in ["word ", "\u{E9}"] {
        let raw = unit.repeat(1000);
        let details = said(500, &raw, &[]);
        let shown = details.message().unwrap_or_default();
        assert!(shown.ends_with('\u{2026}'), "{shown}");
        assert!(shown.chars().count() <= MESSAGE_CHARS + 1, "{shown}");
        assert!(
            raw.starts_with(shown.trim_end_matches('\u{2026}')),
            "{shown}"
        );
    }
}
