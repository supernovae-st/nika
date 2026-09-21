// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Requested meaning survives the deterministic door, and a computation is stated as typed
//! stages: the product-convergence repros, their paraphrases and negatives, the typed
//! grouping, totals and derived outputs.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile_with_provider};
use serde_json::json;

mod common;
use common::{Provider, keys, policy};

/// Requested meaning survives the deterministic door. A mandatory clause is realized,
/// questioned or refused, never dropped into READY: the transformation a write names
/// (`write a 3-bullet summary to …`), the effect and the gate an asking clause names (`ask me
/// to confirm before writing it to …`), and an approval stated in any shape (`only after my
/// explicit approval`). Negatives prove nothing is invented: a plain write gets no gate, a copy
/// gets no draft.
#[test]
#[allow(clippy::too_many_lines)] // one table of wordings, one law
fn requested_meaning_survives_the_deterministic_door() {
    struct Case {
        intent: &'static str,
        transform: Option<bool>,
        gate: Option<bool>,
    }
    let cases = [
        // the product-convergence repros
        Case {
            intent: "Read ./notes/brief.md and write a 3-bullet summary to ./out/summary.md",
            transform: Some(true),
            gate: Some(false),
        },
        Case {
            intent: "Read ./draft.md, ask me to confirm before writing it to ./final.md",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md, draft a short announcement from it, and publish it to ./announce.md only after my explicit approval",
            transform: Some(true),
            gate: Some(true),
        },
        // paraphrases of the same obligations
        Case {
            intent: "Lis ./notes/brief.md et écris un résumé en 3 puces dans ./out/resume.md",
            transform: Some(true),
            gate: Some(false),
        },
        Case {
            intent: "Read ./notes/brief.md and write the summary to ./out/summary.md",
            transform: Some(true),
            gate: Some(false),
        },
        Case {
            intent: "Read ./notes/brief.md and write a 5-bullet summary to ./out/summary.md",
            transform: Some(true),
            gate: Some(false),
        },
        Case {
            intent: "Read ./notes/brief.md and write a French translation to ./out/summary.md",
            transform: Some(true),
            gate: Some(false),
        },
        Case {
            intent: "Read ./draft.md and write it to ./final.md only after I approve",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md and write it to ./final.md, but a human must approve the write first",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md and write it to ./final.md; ask me before writing",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md; don't write it to ./final.md until I approve",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Lis ./draft.md et attends ma validation avant d'écrire dans ./final.md",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md and publish it to ./final.md only if I explicitly say yes",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md, wait for my confirmation, then write it to ./final.md",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md and write it to ./final.md. Human approval required before the write.",
            transform: Some(false),
            gate: Some(true),
        },
        Case {
            intent: "Read ./draft.md and write it to ./final.md after my approval",
            transform: Some(false),
            gate: Some(true),
        },
        // negatives: nothing invented
        Case {
            intent: "Read ./draft.md and write it to ./final.md",
            transform: Some(false),
            gate: Some(false),
        },
        Case {
            intent: "Read ./notes/brief.md and write it to ./out/copy.md",
            transform: Some(false),
            gate: Some(false),
        },
        Case {
            intent: "Lis ./notes/brief.md et écris-le dans ./out/copie.md",
            transform: Some(false),
            gate: Some(false),
        },
        Case {
            intent: "Read every file in ./rfc/*.md and write them combined into ./all.md",
            transform: Some(false),
            gate: Some(false),
        },
    ];
    let mut realized = Vec::new();
    for case in &cases {
        let mut out = nika_compile::compile(&CompileRequest::create(case.intent)).unwrap();
        if keys(&out).contains(&"model") {
            // A drafted transformation needs a model: the only question a realized case asks.
            out = nika_compile::compile(
                &CompileRequest::create(case.intent).answer("model", r#""mock/echo""#),
            )
            .unwrap();
        }
        let candidate = out.candidate.clone().unwrap_or_default();
        let plan = out.provenance.plan.clone().unwrap_or_default();
        let ops: Vec<String> = plan["operations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["op"].as_str().map(str::to_owned))
            .collect();
        let has_transform = ops
            .iter()
            .any(|o| o != "read" && o != "fetch" && o != "lookup" && o != "search");
        let human_first = plan["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|e| e["policy"] == "human_first");
        if out.status != CompileStatus::Ready {
            // Not READY is never a false ready; the negatives, though, must stay admissible.
            assert!(
                case.gate != Some(false) || case.transform != Some(false),
                "a plain request no longer compiles: {} → {:?}",
                case.intent,
                out.diagnostics
            );
            continue;
        }
        realized.push(case.intent);
        assert!(!candidate.is_empty(), "{}", case.intent);
        if let Some(expected) = case.transform {
            assert_eq!(
                has_transform, expected,
                "transform on `{}`: ops {ops:?}\n{candidate}",
                case.intent
            );
            assert_eq!(
                candidate.contains("infer:"),
                expected,
                "infer on `{}`\n{candidate}",
                case.intent
            );
        }
        if let Some(expected) = case.gate {
            assert_eq!(
                human_first, expected,
                "policy on `{}`: {}",
                case.intent, plan["effects"]
            );
            assert_eq!(
                candidate.contains("nika:prompt"),
                expected,
                "gate on `{}`\n{candidate}",
                case.intent
            );
        }
        assert!(
            candidate.contains("nika:write"),
            "effect on `{}`\n{candidate}",
            case.intent
        );
        if case.gate == Some(true) {
            // The gate dominates the write: the review task precedes the write and the write
            // waits for its approval.
            let review = candidate.find("nika:prompt").unwrap_or(usize::MAX);
            let write = candidate.rfind("nika:write").unwrap_or(0);
            assert!(
                review < write,
                "the gate must precede the write on `{}`\n{candidate}",
                case.intent
            );
            assert!(
                candidate.contains("with.approved == true"),
                "the write must wait for the gate on `{}`\n{candidate}",
                case.intent
            );
        }
    }
    // The three repros are realized, not merely refused.
    for repro in [
        "Read ./notes/brief.md and write a 3-bullet summary to ./out/summary.md",
        "Read ./draft.md, ask me to confirm before writing it to ./final.md",
        "Read ./draft.md, draft a short announcement from it, and publish it to ./announce.md only after my explicit approval",
    ] {
        assert!(
            realized.contains(&repro),
            "{repro} is not READY: {realized:#?}"
        );
    }
}

/// A grouping stated as meaning: one output row per distinct value of a column with the
/// aggregates the request names, sorted and projected as the request fixes them. The
/// compiler validates every column and output name against the request, lowers the whole
/// computation to jq in a fixed order, and the CSV written carries the produced columns.
#[tokio::test]
async fn a_typed_grouping_is_lowered_with_its_aggregates_and_output_columns() {
    let intent = "Read ./umsatz/q3.csv (columns datum,filiale,betrag_cents) and write ./out/pro_filiale.csv with the columns filiale,summe_cents,anzahl: one row per filiale, summe_cents the sum of betrag_cents of that filiale, anzahl the number of its rows, sorted by filiale ascending.";
    let typed = json!({
        "steps": [
            {"op": "read", "detail": "./umsatz/q3.csv", "evidence": "Read ./umsatz/q3.csv"},
            {"op": "compute", "detail": "one row per filiale, summe_cents the sum of betrag_cents of that filiale, anzahl the number of its rows, sorted by filiale ascending", "evidence": "one row per filiale, summe_cents the sum of betrag_cents of that filiale, anzahl the number of its rows, sorted by filiale ascending",
             "computation": {"present": true, "polarity": "keep", "join": "and", "clauses": [], "group_by": "filiale",
                             "aggregations": [{"field": "betrag_cents", "op": "sum", "as": "summe_cents", "round": ""}, {"field": "", "op": "count", "as": "anzahl", "round": ""}],
                             "sort_by": "filiale", "order": "asc", "columns": ["filiale", "summe_cents", "anzahl"]}}
        ],
        "effects": [{"verb": "write", "target": "./out/pro_filiale.csv", "policy": "automatic", "evidence": "write ./out/pro_filiale.csv with the columns filiale,summe_cents,anzahl"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./umsatz/q3.csv (columns datum,filiale,betrag_cents)", "role": "operation"},
            {"text": "and write ./out/pro_filiale.csv with the columns filiale,summe_cents,anzahl:", "role": "effect"},
            {"text": "one row per filiale, summe_cents the sum of betrag_cents of that filiale, anzahl the number of its rows, sorted by filiale ascending.", "role": "operation"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &Provider::new(typed),
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let rules = plan["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1, "{plan:#}");
    assert_eq!(rules[0]["shape"]["group_by"], "filiale");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        candidate.contains(".records | group_by(.filiale) | map({\"filiale\": (.[0] | .filiale), \"summe_cents\": (map(.betrag_cents | tonumber) | add // 0), \"anzahl\": length}) | sort_by(.filiale) | map({\"filiale\": .filiale, \"summe_cents\": .summe_cents, \"anzahl\": .anzahl})"),
        "{candidate}"
    );
    assert!(
        candidate.contains("has(\"filiale\")") && candidate.contains("has(\"betrag_cents\")"),
        "the guard proves the source columns: {candidate}"
    );
    assert!(
        !candidate.contains("has(\"summe_cents\")"),
        "a produced name is never a source column: {candidate}"
    );
    assert!(
        candidate.contains("- filiale\n")
            && candidate.contains("- summe_cents\n")
            && candidate.contains("- anzahl\n"),
        "the CSV carries the produced columns: {candidate}"
    );
}

/// Totals over every row are the outputs the request names, lowered to one object and
/// exposed one by one, an average rounded as the request states.
#[tokio::test]
async fn typed_totals_become_named_outputs() {
    let intent = "Read ./encuesta.csv (columns respuesta_id,nota) and expose two outputs: respuestas, the number of rows, and nota_media, the average of nota rounded to 1 decimal. Write nothing.";
    let typed = json!({
        "steps": [
            {"op": "read", "detail": "./encuesta.csv", "evidence": "Read ./encuesta.csv"},
            {"op": "compute", "detail": "respuestas, the number of rows, and nota_media, the average of nota rounded to 1 decimal", "evidence": "respuestas, the number of rows, and nota_media, the average of nota rounded to 1 decimal",
             "computation": {"present": true, "polarity": "keep", "join": "and", "clauses": [], "group_by": "",
                             "aggregations": [{"field": "", "op": "count", "as": "respuestas", "round": ""}, {"field": "nota", "op": "avg", "as": "nota_media", "round": "1"}],
                             "sort_by": "", "order": "", "columns": []}}
        ],
        "effects": [{"verb": "write", "target": "nothing", "policy": "forbidden", "evidence": "Write nothing."}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./encuesta.csv (columns respuesta_id,nota)", "role": "operation"},
            {"text": "and expose two outputs: respuestas, the number of rows, and nota_media, the average of nota rounded to 1 decimal.", "role": "operation"},
            {"text": "Write nothing.", "role": "policy"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &Provider::new(typed),
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        candidate.contains(".records | {\"respuestas\": length, \"nota_media\": (((if length == 0 then 0 else ((map(.nota | tonumber) | add) / length) end) * 10 | round) / 10)}"),
        "{candidate}"
    );
    assert!(
        candidate.contains("respuestas: ${{ tasks.compute.output.respuestas }}"),
        "{candidate}"
    );
    assert!(
        candidate.contains("nota_media: ${{ tasks.compute.output.nota_media }}"),
        "{candidate}"
    );
}

/// An output the request defines as arithmetic over other outputs is a derived entry over
/// the totals, never an aggregate the model picks: `solde_cents = credit_cents - debit_cents`.
#[tokio::test]
async fn a_derived_output_is_arithmetic_over_the_totals() {
    let intent = "Read ./grand-livre.csv (columns date,libelle,debit_cents,credit_cents) and expose as outputs: ecritures, the number of rows; debit_cents, the sum of debit_cents; credit_cents, the sum of credit_cents; solde_cents, credit_cents minus debit_cents. Write nothing.";
    let typed = json!({
        "steps": [
            {"op": "read", "detail": "./grand-livre.csv", "evidence": "Read ./grand-livre.csv"},
            {"op": "compute", "detail": "ecritures, the number of rows; debit_cents, the sum of debit_cents; credit_cents, the sum of credit_cents; solde_cents, credit_cents minus debit_cents", "evidence": "ecritures, the number of rows; debit_cents, the sum of debit_cents; credit_cents, the sum of credit_cents; solde_cents, credit_cents minus debit_cents",
             "computation": {"present": true, "polarity": "keep", "join": "and", "clauses": [], "group_by": "",
                             "aggregations": [{"field": "", "op": "count", "as": "ecritures", "round": ""}, {"field": "debit_cents", "op": "sum", "as": "debit_cents", "round": ""}, {"field": "credit_cents", "op": "sum", "as": "credit_cents", "round": ""}],
                             "sort_by": "", "order": "", "columns": [],
                             "derived": [{"as": "solde_cents", "op": "sub", "left": "credit_cents", "right": "debit_cents"}]}}
        ],
        "effects": [{"verb": "write", "target": "nothing", "policy": "forbidden", "evidence": "Write nothing."}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./grand-livre.csv (columns date,libelle,debit_cents,credit_cents)", "role": "operation"},
            {"text": "and expose as outputs: ecritures, the number of rows; debit_cents, the sum of debit_cents; credit_cents, the sum of credit_cents; solde_cents, credit_cents minus debit_cents.", "role": "operation"},
            {"text": "Write nothing.", "role": "policy"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &Provider::new(typed),
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        candidate.contains("| . + {\"solde_cents\": (.credit_cents - .debit_cents)}"),
        "{candidate}\n{:#?}\n{:#?}",
        out.questions,
        out.diagnostics
    );
    assert!(
        candidate.contains("solde_cents: ${{ tasks.compute.output.solde_cents }}"),
        "{candidate}"
    );
}

/// A word the request lists as a column is a column, never the verb it spells: `name, email
/// e city` asks for no send, and a proposal that reads one is not feasible.
#[test]
fn a_listed_column_is_never_an_effect() {
    let intent = "Read ./contatos.csv, which has the columns nome,email,cidade, and write ./out/contatos.json as a JSON array with the keys name, email e city, sorted by email.";
    let out = nika_compile::compile(&CompileRequest::create(intent)).unwrap();
    let plan = out.provenance.plan.clone().unwrap_or_default();
    let verbs: Vec<String> = plan["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["verb"].as_str().map(str::to_owned))
        .collect();
    assert!(!verbs.iter().any(|v| v == "send"), "{plan:#}");
}

/// Semantics before syntax: the proposal states the filter as a typed predicate over the
/// request's own column and literal; the compiler validates it and lowers it to jq, and no
/// rule question is asked. A predicate naming a column the request never mentions is no
/// rule at all: the compiler asks instead of guessing.
#[tokio::test]
async fn a_typed_predicate_from_the_proposal_needs_no_rule_question() {
    let intent = "Read ./data/orders.csv (columns order_id,customer,amount,status) and keep the rows that matter, then write them to ./out/kept.csv.";
    let typed = json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv"},
            {"op": "compute", "detail": "the rows that matter", "evidence": "keep the rows that matter",
             "computation": {"present": true, "join": "and", "clauses": [{"field": "amount", "op": "gt", "value": "100", "value_field": ""}]}}
        ],
        "effects": [{"verb": "write", "target": "./out/kept.csv", "policy": "automatic", "evidence": "write them to ./out/kept.csv"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./data/orders.csv (columns order_id,customer,amount,status)", "role": "operation"},
            {"text": "and keep the rows that matter,", "role": "operation"},
            {"text": "then write them to ./out/kept.csv.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    // The literal 100 is not in the request: the predicate is refused and the rule is asked.
    let provider = Provider::new(typed.clone());
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    // With the threshold stated, the predicate is validated, lowered and never asked.
    let intent = "Read ./data/orders.csv (columns order_id,customer,amount,status) and keep the rows that matter, above 100, then write them to ./out/kept.csv.";
    let mut stated = typed;
    stated["regions"][1]["text"] = json!("and keep the rows that matter, above 100,");
    let provider = Provider::new(stated);
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert!(
        plan["rules"].as_array().is_some_and(|r| r.len() == 1),
        "{plan:#}"
    );
    assert_eq!(plan["rules"][0]["field"], "amount");
    assert_eq!(plan["rules"][0]["comparator"], ">");
    let ready = compile_with_provider(
        &CompileRequest::create(intent)
            .with_authoring_policy(policy())
            .answer("model", r#""mock/echo""#),
        &Provider::new(out.provenance.plan.clone().unwrap()),
    )
    .await;
    let _ = ready;
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        candidate.contains("select((.amount | tonumber) > 100)")
            || out.status != CompileStatus::Ready,
        "{candidate}"
    );
}

/// An exclusion is stated with its own polarity, never re-read as a keep: the rows kept are
/// the complement of the clauses, negated one by one with the junction flipped. `exclude the
/// rows whose amount is below 100 or whose status is refunded` keeps amount >= 100 and
/// status != refunded.
#[tokio::test]
async fn a_drop_predicate_is_lowered_as_its_complement() {
    let intent = "Read ./data/orders.csv (columns order_id,customer,amount,status) and exclude the rows whose amount is below 100 or whose status is refunded, then write the kept rows to ./out/kept.csv.";
    let typed = json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv"},
            {"op": "compute", "detail": "exclude the rows whose amount is below 100 or whose status is refunded", "evidence": "exclude the rows whose amount is below 100 or whose status is refunded",
             "computation": {"present": true, "polarity": "drop", "join": "or", "clauses": [
                {"field": "amount", "op": "lt", "value": "100", "value_field": ""},
                {"field": "status", "op": "eq", "value": "refunded", "value_field": ""}]}}
        ],
        "effects": [{"verb": "write", "target": "./out/kept.csv", "policy": "automatic", "evidence": "write the kept rows to ./out/kept.csv"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./data/orders.csv (columns order_id,customer,amount,status)", "role": "operation"},
            {"text": "and exclude the rows whose amount is below 100 or whose status is refunded,", "role": "operation"},
            {"text": "then write the kept rows to ./out/kept.csv.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &Provider::new(typed),
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let rules = plan["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1, "{plan:#}");
    let candidate = out.candidate.as_deref().unwrap_or_default();
    assert!(
        candidate.contains("select((.amount | tonumber) >= 100 and .status != \"refunded\")")
            || out.status != CompileStatus::Ready,
        "{candidate}"
    );
    assert!(
        !candidate.contains("(.amount | tonumber) < 100"),
        "the exclusion was re-read as a keep: {candidate}"
    );
}
