// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Quoted content is content for every law that reads words by substring (R4 S0, the A3
//! residuals): a waiver, an approval bypass or bound, a refund, an indecision, its companion
//! and a contradiction marker inside quotes are what the workflow matches or writes, never a
//! policy. Measured on 007592ab9: a line filter over 'no need to ask me' was read as a waiver
//! and dropped (READY, the whole file written), and 'without approval' or 'refund' in a filter
//! refused the request. The same words outside the quotes still govern.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use nika_compile_reader::gates;
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb, Plan};
use serde_json::{Value, json};

/// Policy phrases, each quoted as content.
const PHRASES: &[&str] = &[
    "no need to ask me",
    "without approval",
    "refund",
    "not decided",
    "pas encore décidé",
    "ask me the question",
    "these two instructions contradict",
    "until I approve",
    "wait until I approve it",
];

fn ready_plan(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    assert!(out.questions.is_empty(), "{intent}: {out:#?}");
    out.provenance.plan.unwrap()
}

/// A quoted phrase a line filter matches is that filter's text: the rule is kept, whole, and
/// nothing else is read from it.
#[test]
fn a_quoted_policy_phrase_in_a_filter_is_the_matched_text() {
    for phrase in PHRASES {
        let intent = format!(
            "Read ./notes.txt, keep only the lines containing '{phrase}' and write them to ./out.txt"
        );
        let plan = ready_plan(&intent);
        let rules = plan["rules"].as_array().unwrap();
        assert!(
            rules.iter().any(|rule| rule["lines"] == json!(true)
                && rule["text"].as_str().unwrap().contains(phrase)),
            "{intent}: the filter is dropped: {plan:#}"
        );
        assert_eq!(plan["unknowns"], json!([]), "{intent}");
        assert_eq!(plan["constraints"], json!([]), "{intent}");
        let effects = plan["effects"].as_array().unwrap();
        assert!(
            effects.iter().all(|e| e["policy"] == json!("automatic")),
            "{intent}: {effects:?}"
        );
    }
}

/// A quoted phrase written to a file is the written text, byte for byte.
#[test]
fn a_quoted_policy_phrase_written_to_a_file_is_the_written_text() {
    for phrase in PHRASES.iter().chain(&["maybe later", "refund policy v2"]) {
        let intent = format!("write '{phrase}' to ./a.txt");
        let out = compile(&CompileRequest::create(&intent)).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        assert_eq!(
            doc["const"]["output_content"],
            json!(phrase),
            "{intent}: {doc:#}"
        );
        assert!(
            doc["tasks"]["write_output"].get("when").is_none(),
            "{intent}: no gate"
        );
    }
}

fn policies(intent: &str) -> Vec<(EffectVerb, EffectPolicy)> {
    let reading = lexicon::read(&lexicon::fold_apostrophes(intent));
    reading
        .plan
        .effects
        .iter()
        .map(|e| (e.verb, e.policy))
        .collect()
}

/// Metamorphic pairs: the same words outside the quotes still govern.
#[test]
fn the_same_phrase_outside_quotes_still_governs() {
    // A waiver.
    assert_eq!(gates::waiver_polarity("no need to ask me"), Some(true));
    assert_eq!(
        gates::waiver_polarity("write 'no need to ask me' to ./a.txt"),
        None
    );
    assert_eq!(
        gates::waiver_polarity("mais pas sans me demander"),
        Some(false)
    );
    // An approval bypass.
    assert!(gates::bypass_stated("send it without approval"));
    assert!(!gates::bypass_stated("write 'without approval' to ./a.txt"));
    assert!(!gates::bypass_stated(
        "keep the lines containing \"without approval\""
    ));
    // A refund nothing carries.
    let refunds = |intent: &str| {
        let mut plan = Plan::default();
        gates::backstop(intent, &mut plan);
        plan.unknowns.iter().any(|u| u.contains("refund"))
    };
    assert!(refunds("Read ./orders.csv, then refund the late orders"));
    assert!(!refunds("write 'refund policy' to ./a.txt"));
    // An indecision about an effect.
    assert_eq!(
        policies("Read ./notes.md; the write to ./out.md is not decided yet"),
        [(EffectVerb::Write, EffectPolicy::Undecided)]
    );
    assert_eq!(
        policies("write 'not decided yet' to ./a.txt"),
        [(EffectVerb::Write, EffectPolicy::Automatic)]
    );
    // A prohibition bounded by an approval is a gate; a quoted bound is the banned text.
    assert_eq!(
        policies("never write to ./a.txt until I approve"),
        [(EffectVerb::Write, EffectPolicy::HumanFirst)]
    );
    for banned in [
        "never write 'until I approve' to ./a.txt",
        "never write 'wait until I approve it' to ./a.txt",
    ] {
        assert_eq!(
            policies(banned),
            [(EffectVerb::Write, EffectPolicy::Forbidden)],
            "{banned}"
        );
    }
}
