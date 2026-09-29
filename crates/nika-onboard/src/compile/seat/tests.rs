// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

/// A cloud model the catalog does not price names its provider's row (an alias reads as the
/// row it names) and the priced models of that provider, each one priced; a priced cloud model,
/// a local engine, the mock and a line that is not a model are none.
#[test]
fn an_unpriced_cloud_model_names_its_row_and_the_priced_models() {
    let (row, model, priced) =
        unpriced_cloud(" deepseek/deepseek-unpriced-v0 ").expect("an unpriced cloud model");
    assert_eq!(
        (row.as_str(), model.as_str()),
        ("deepseek", "deepseek-unpriced-v0")
    );
    assert!(
        priced.iter().any(|m| m == "deepseek/deepseek-flash"),
        "{priced:?}"
    );
    for m in &priced {
        assert!(
            m.starts_with("deepseek/") && unpriced_cloud(m).is_none(),
            "{m}"
        );
    }
    let (row, _, priced) = unpriced_cloud("claude/claude-unpriced-v0").expect("unpriced");
    assert_eq!(row, "anthropic", "the alias reads as its row");
    assert!(
        priced.iter().all(|m| m.starts_with("anthropic/")),
        "{priced:?}"
    );
    for none in [
        "deepseek/deepseek-flash",
        "ollama/llama3.1",
        "mock/echo",
        "five lines",
    ] {
        assert_eq!(unpriced_cloud(none), None, "{none}");
    }
}

/// The neutral words: the warning names what is shown, the priced models are said joined, or
/// that none is known yet. No host gesture and no code citation ride them.
#[test]
fn the_pricing_words_leave_the_hosts_gestures_to_the_host() {
    assert_eq!(
        unpriced_warning("deepseek/x"),
        "`deepseek/x` is not priced in Nika's catalog: a run under a spending ceiling would refuse it"
    );
    assert_eq!(
        priced_words(
            "deepseek",
            &["deepseek/a".to_owned(), "deepseek/b".to_owned()]
        ),
        "priced for `deepseek`: deepseek/a · deepseek/b"
    );
    assert_eq!(
        priced_words("acme", &[]),
        "no priced model is known for `acme` yet"
    );
}

/// The static table, relocated whole: the provider's strongest, never the same model twice; no
/// escalation across an OpenAI-compatible gateway; nothing for a provider the table does not name
/// or a name that is not `provider/model`.
#[test]
fn the_stronger_model_table_names_each_providers_strongest_once() {
    assert_eq!(stronger_model("openai/gpt-5-mini"), Some("openai/gpt-5.2"));
    assert_eq!(
        stronger_model("openai/gpt-5.2"),
        None,
        "already the strongest"
    );
    assert_eq!(stronger_model("xai/grok-4.3"), Some("xai/grok-4.7"));
    assert_eq!(
        stronger_model("deepseek/deepseek-flash"),
        Some("deepseek/deepseek-v4-pro")
    );
    assert_eq!(
        stronger_model_under("gemini/gemini-2.5-flash", false),
        Some("gemini/gemini-2.5-pro")
    );
    assert_eq!(stronger_model_under("openai/gpt-oss-120b", true), None);
    assert_eq!(
        stronger_model_under("mistral/mistral-small-latest", true),
        Some("mistral/mistral-large-latest"),
        "the gateway fact is the openai provider's alone"
    );
    for none in ["ollama/qwen3.5:4b", "mock/echo", "five lines"] {
        assert_eq!(stronger_model(none), None, "{none}");
    }
}
