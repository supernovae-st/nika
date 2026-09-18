// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
const PLAIN: Theme = Theme::new(false, false, false);

#[test]
fn discovery_taglines_come_from_the_bodies() {
    let body = nika_pack::template("chain").expect("chain embedded");
    let tag = tagline("chain", body);
    assert!(!tag.is_empty(), "chain header carries a tagline");
    assert!(
        body.contains(tag.trim_end_matches('…')),
        "the bootstrap tagline comes from source"
    );
}

#[test]
fn the_ollama_note_drops_local_under_an_endpoint_override() {
    // P0-20 · « local » is a TOPOLOGY claim the menu cannot make
    // when an override (NIKA_OLLAMA_BASE_URL · OLLAMA_HOST) may
    // point the engine at a LAN box.
    assert!(ollama_note_for(false).contains("local"));
    let overridden = ollama_note_for(true);
    assert!(!overridden.contains("local"));
    assert!(overridden.contains("custom endpoint"));
    assert!(
        overridden.contains("zero key"),
        "the protocol truth must stay"
    );
}

#[test]
fn the_model_menu_derives_from_the_catalog_local_first() {
    let menu = model_menu();
    assert!(menu.len() >= 2, "catalog carries the menu providers");
    assert!(
        menu[0].0.starts_with("ollama/"),
        "local must be first in presentation order"
    );
    assert!(menu[1].0.starts_with("mock/"), "offline must be second");
    // Every entry is a full provider/model wire id from the catalog.
    assert!(menu.iter().all(|(m, _)| m.contains('/')));
}

#[test]
fn ask_model_reasks_an_unrecognized_pick() {
    let mut input = std::io::Cursor::new(b"gpt\n\n".to_vec());
    let mut out = Vec::new();
    let model = ask_model(&mut input, &mut out, PLAIN)
        .expect("io ok")
        .expect("not cancelled");
    assert!(
        model.starts_with("mock/"),
        "Enter after the re-ask must select mock"
    );
    let shown = String::from_utf8(out);
    assert!(shown.is_ok(), "wizard output must be UTF-8");
    let Some(shown) = shown.ok() else {
        return;
    };
    assert!(shown.contains("unrecognized"), "the typo must be said");
}

#[test]
fn resolve_model_accepts_only_a_menu_number_or_a_wire_id() {
    // FLIP (P0-8 · 2026-07-31): resolve_model is now honest — Option,
    // with the ASK LOOP owning the Enter default (default_model) and
    // the re-ask. « 99 » and « gpt » used to pin the SILENT mock
    // fallback.
    let menu = model_menu();
    assert!(
        default_model(&menu).starts_with("mock/"),
        "Enter must never fail"
    );
    assert_eq!(
        resolve_model("1", &menu).as_deref(),
        Some(menu[0].0.as_str())
    );
    assert_eq!(
        resolve_model("ollama/llama3.2:3b", &menu).as_deref(),
        Some("ollama/llama3.2:3b")
    );
    // A number off the menu or a word without `/` is unrecognized —
    // said + re-asked by the loop, never a silent mock.
    assert_eq!(resolve_model("99", &menu), None);
    assert_eq!(resolve_model("gpt", &menu), None);
    assert_eq!(
        resolve_model("", &menu),
        None,
        "Enter is the loop's default"
    );
}

#[test]
fn yaml_scalar_keeps_plain_bare_and_single_quotes_the_rest() {
    assert_eq!(yaml_scalar("mock/echo"), "mock/echo");
    assert_eq!(yaml_scalar("summarize"), "summarize");
    // Space · colon · backslash · quote → single-quoted, literal.
    assert_eq!(
        yaml_scalar("save to C:\\Users\\me"),
        "'save to C:\\Users\\me'"
    );
    assert_eq!(yaml_scalar("foo/bar: baz"), "'foo/bar: baz'");
    assert_eq!(yaml_scalar("it's a test"), "'it''s a test'");
    assert_eq!(yaml_scalar(""), "''");
}

#[test]
fn stamped_file_survives_a_hostile_model_string() {
    // The rust-pro HIGH: a YAML-significant model pick reached the
    // scalar unescaped -> the fresh scaffold failed its OWN check
    // under a green. Every stamp must round-trip through the REAL
    // parser+check clean. The intent no longer rides into the file at
    // all (there is no description slot), so the hostile-INTENT half
    // of this battery has no destination left to defend.
    let body = nika_pack::template("chain").expect("embedded");
    for model in ["mock/echo", "foo/bar: baz"] {
        let stamped = stamp(body, "hostile", Some(model));
        let parsed = nika_schema::parse(
            &stamped,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        assert!(parsed.is_ok(), "catalog model must parse");
        // The check ladder must not choke either (a dirty audit is
        // fine; a PARSE error at this point is the bug).
        let Some(wf) = parsed.ok() else {
            return;
        };
        let _ = nika_check::check(&wf);
    }
}

#[test]
fn every_embedded_template_audits_clean_or_is_a_documented_gap() {
    // The own-corpus law (#261): every embedded skeleton a fresh
    // scaffold can produce MUST audit clean — a red ladder on a first
    // scaffold is the self-contradiction the wizard exists to avoid.
    // This ratchet was MISSING (pack-integrity only hashes text), so
    // `api-upload-and-create` shipped in #257 failing its OWN
    // SECRETS-egress check, unnoticed, until a user-sim caught it.
    //
    // KNOWN GAP — an operator design call, NOT a template typo: the
    // ADR-092 flow model taints an authenticated `invoke`'s OUTPUT (a
    // secret in a fetch auth-header taints the response, exactly as a
    // secret in the body would), with no `infer`/`agent`-style prompt
    // exception and no output-declassification construct. So
    // EMPTY since 2026-07-10: the one former gap
    // (`api-upload-and-create` — a secret-authed response piped to
    // `outputs:` had NO sanctioned path) resolved via the
    // output-declassification this ratchet's note called for:
    // `egress: [{ to: "outputs" }]` (spec 01-envelope §egress · the
    // owner declassifies the workflow boundary itself). Every template
    // now passes its own audit; a dirty one fails this ratchet unless
    // a genuine flow-model design gap is documented here.
    const KNOWN_GAP: &[&str] = &[];
    let mut clean = 0_usize;
    let mut invitations = 0_usize;
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("template embedded");
        let parsed = nika_schema::parse(
            body,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        assert!(parsed.is_ok(), "{name}: template must parse");
        let Some(wf) = parsed.ok() else {
            return;
        };
        let is_gap = KNOWN_GAP.contains(&name.as_str());
        let report = nika_check::check(&wf);
        // #1066 amended the own-corpus law rather than retiring it. A
        // fresh skeleton may refuse for its OWN unfilled slots — that
        // refusal is the invitation to fill them, and it is why a
        // scaffold can no longer run and leave a file that reads like a
        // result. It may refuse for nothing else: a permits escape or a
        // dangling reference in a shipped template still fails here.
        let invitation =
            !report.slot_findings.is_empty() && report.findings.iter().all(|f| f.kind == "slot");
        if report.is_clean() {
            assert!(
                !is_gap,
                "{name}: now audits CLEAN — remove it from KNOWN_GAP, the design gap is resolved"
            );
            clean += 1;
        } else if invitation {
            invitations += 1;
        } else {
            assert!(
                is_gap,
                "{name}: a fresh scaffold FAILS its own `nika check` (own-corpus law · #261) — \
                     fix the template, or (if a genuine flow-model design gap) document it in KNOWN_GAP"
            );
        }
    }
    // Every shipped skeleton is one of the two healthy states, and both
    // states are populated — a floor on `clean` alone would go quietly
    // green on a pack where every template had become a form.
    assert_eq!(
        clean + invitations,
        nika_pack::template_names().len() - KNOWN_GAP.len(),
        "clean {clean} · invitations {invitations}"
    );
    assert!(clean >= 5, "expected >= 5 clean templates, got {clean}");
    assert!(
        invitations >= 1,
        "no skeleton invites its author to fill it"
    );
}

#[test]
fn stamp_fills_exactly_the_two_known_slots() {
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("embedded");
        let stamped = stamp(body, "field-demo", Some("mock/echo"));
        assert!(stamped.contains("nika: field-demo"), "{name}: id stamped");
        assert!(
            !stamped.contains("-template "),
            "{name}: no template id remnant"
        );
        if body.lines().any(|l| l.starts_with("model: ")) {
            assert!(
                stamped.contains("model: mock/echo"),
                "{name}: model stamped"
            );
        }
    }
}

#[test]
fn recovered_try_examples_parse_and_check() {
    for slug in [
        "01-hello",
        "03-exec-pipeline",
        "standup-digest",
        "05-fetch-chain",
    ] {
        let body = nika_pack::example(slug).expect(slug);
        let parsed = nika_schema::parse(
            body,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        assert!(parsed.is_ok(), "{slug} must parse");
        let Some(wf) = parsed.ok() else {
            return;
        };
        let report = nika_check::check(&wf);
        assert!(
            report.is_clean() || report.findings.iter().all(|f| f.kind == "slot"),
            "{slug}: recovered example fails check"
        );
    }
}

#[test]
fn stamp_comments_match_the_model_field() {
    let body = nika_pack::template("chain").expect("embedded");
    let openai = stamp(body, "hello", Some("openai/gpt-5.2"));
    let model = openai
        .lines()
        .find(|l| l.starts_with("model: "))
        .expect("model line");
    assert!(model.contains("openai/gpt-5.2"));
    assert!(
        !model.to_ascii_lowercase().contains("local"),
        "openai must not be described as local"
    );
    let mock = stamp(body, "hello", Some("mock/echo"));
    let mock_line = mock
        .lines()
        .find(|l| l.starts_with("model: "))
        .expect("model line");
    assert!(mock_line.contains("mock/echo"));
    assert!(
        !mock_line.to_ascii_lowercase().contains("local"),
        "mock must not be described as local"
    );
}

#[test]
fn template_takes_model_matches_every_body() {
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("embedded");
        let has = body.lines().any(|l| l.starts_with("model: "));
        assert_eq!(template_takes_model(body), has, "{name}");
    }
    // The split is real: both kinds exist in the embedded set.
    let kinds: Vec<bool> = nika_pack::template_names()
        .iter()
        .map(|n| template_takes_model(nika_pack::template(n).expect("embedded")))
        .collect();
    assert!(kinds.contains(&true) && kinds.contains(&false));
}
