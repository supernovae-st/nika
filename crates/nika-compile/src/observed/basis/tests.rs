// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source basis law (C9 · F4) on the decisions of real, deterministic compiles over worlds
//! built as the host observer builds them (the same pure half, its row envelope, its kinds beside
//! the rows): what moves a basis, what holds it, and what stays unjudged. Crafted records appear
//! only where the law is asked about labels it must not trust or paths it must match exactly.
use super::*;
use crate::{CompileRequest, CompileStatus, compile, observation};
use serde_json::{Map, Value, json};

const REQUEST: &str = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json";
const SOURCE: &str = "./data/input.csv";
const INPUT: &str =
    "id,amount_usd,status\nA001,10,paid\nA002,260,paid\nA003,300,open\nA004,120,paid\n";

/// What the host observer (`nika_cli_host::compile::observe::world`) records of each file, from
/// the same pure half it calls: the row envelope it writes, and the kinds beside the rows.
fn observed(files: &[(&str, &str)]) -> Value {
    let mut rows = Vec::new();
    let mut kinds = Map::new();
    for (path, text) in files {
        let jsonl = std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));
        let (kind, sample) = if jsonl {
            ("jsonl", observation::records(&observation::jsonl(text)))
        } else {
            ("csv", observation::csv(text, false))
        };
        let mut row = json!({
            "path": path, "state": "observed", "complete": false, "kind": kind,
            "columns": sample.columns, "bytes": text.len(),
            "peek_sha256": crate::surface::sha256(text),
        });
        if let Some(common) = sample.common {
            row["common_columns"] = json!(common);
        }
        if let Some(delimiter) = sample.delimiter {
            row["delimiter"] = json!(delimiter.to_string());
        }
        if !sample.values.is_empty() {
            row["values"] = Value::Object(sample.values.into_iter().collect());
        }
        rows.push(row);
        kinds.insert((*path).to_owned(), sample.kinds);
    }
    json!({"observed": rows, "kinds": kinds})
}

/// The host's row for a stated path it could not read as records.
fn unavailable(path: &str, state: &str) -> Value {
    json!({"observed": [{"path": path, "state": state, "complete": false}]})
}

/// The decision of the real, deterministic compile of `intent` over `world`, which is READY.
fn decision(intent: &str, world: &Value) -> Value {
    let out =
        compile(&CompileRequest::create(intent).with_knowledge(world.clone())).expect("compiles");
    assert_eq!(out.status, CompileStatus::Ready, "{:?}", out.diagnostics);
    out.provenance.decision.expect("a decision")
}

fn moved(basis: Basis) -> Vec<String> {
    match basis {
        Basis::Moved(words) => words,
        other => panic!("expected a moved basis, got {other:?}"),
    }
}

fn judged(fresh: &str) -> Basis {
    let recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    basis(
        Some(&recorded),
        Some(&observed(&[(SOURCE, fresh)])),
        REQUEST,
    )
}

#[test]
fn the_real_compile_records_the_basis_this_law_judges() {
    let recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    let grounding = recorded["grounding"].as_array().expect("grounding");
    assert!(
        grounding
            .iter()
            .any(|e| e["field"] == "amount_usd" && e["source"] == SOURCE),
        "{grounding:?}"
    );
    assert!(
        recorded["numbers"]
            .as_array()
            .is_some_and(|n| n.iter().any(|e| e["bound_by"] == "observed numbers")),
        "{recorded}"
    );
    assert_eq!(Basis::sources(Some(&recorded)), [SOURCE]);
    assert_eq!(
        judged(INPUT),
        Basis::Holds(2),
        "the key and its number policy"
    );
}

#[test]
fn a_renamed_or_removed_key_moves_the_basis() {
    let words = moved(judged(&INPUT.replace("amount_usd", "amount")));
    assert!(
        words.contains(&format!(
            "`{SOURCE}` no longer has `amount_usd`: it has id, amount, status"
        )),
        "{words:?}"
    );
    let removed = "id,status\nA001,paid\nA002,paid\nA003,open\n";
    let words = moved(judged(removed));
    assert!(
        words
            .iter()
            .any(|w| w.contains("no longer has `amount_usd`")),
        "{words:?}"
    );
}

#[test]
fn new_or_reordered_rows_a_new_peek_and_another_column_order_hold() {
    let appended = format!("{INPUT}A031,700,paid\n");
    let reordered =
        "id,amount_usd,status\nA004,120,paid\nA003,300,open\nA002,260,paid\nA001,10,paid\n";
    let columns = "status,id,amount_usd\npaid,A001,10\npaid,A002,260\nopen,A003,300\n";
    for fresh in [appended.as_str(), reordered, columns] {
        let now = observed(&[(SOURCE, fresh)]);
        let then = observed(&[(SOURCE, INPUT)]);
        assert_ne!(
            now["observed"][0]["peek_sha256"], then["observed"][0]["peek_sha256"],
            "the peek changed"
        );
        assert_eq!(judged(fresh), Basis::Holds(2), "{fresh}");
    }
}

#[test]
fn a_source_that_shows_no_record_now_moves_the_basis() {
    let recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    for (state, words) in [
        ("absent", "absent"),
        ("unreadable", "unreadable"),
        ("outside_project", "outside the project"),
        ("empty", "empty"),
        ("unknown", "not readable as records"),
    ] {
        let fresh = unavailable(SOURCE, state);
        let words_now = moved(basis(Some(&recorded), Some(&fresh), REQUEST));
        assert!(
            words_now.contains(&format!(
                "`{SOURCE}` is {words} now: `amount_usd` cannot be read from it"
            )),
            "{words_now:?}"
        );
    }
}

#[test]
fn a_source_the_fresh_observation_does_not_cover_is_unjudged() {
    let recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    let missing = format!("`{SOURCE}` was not observed again");
    assert_eq!(
        basis(Some(&recorded), None, REQUEST),
        Basis::Unjudged(vec![missing.clone()])
    );
    let other = observed(&[("./data/other.csv", INPUT)]);
    assert_eq!(
        basis(Some(&recorded), Some(&other), REQUEST),
        Basis::Unjudged(vec![missing.clone()])
    );
    let twice = observed(&[(SOURCE, INPUT), (SOURCE, INPUT)]);
    assert_eq!(
        basis(Some(&recorded), Some(&twice), REQUEST),
        Basis::Unjudged(vec![missing]),
        "two rows for one source are no answer"
    );
}

/// A key every sampled record held, now missing from some: the obligation the compiler opens for
/// records lacking it would be asked again. The deterministic reader asks a JSONL rule as code, so
/// the recorded entry is shaped as the grounding law writes it (`grounding::Entry::to_json`) over
/// a real observed sample.
#[test]
fn a_key_some_sampled_records_now_lack_moves_the_basis() {
    let intent = "read ./data/input.jsonl, keep the rows where amount_usd is over 250, write them to ./out/result.json";
    let source = "./data/input.jsonl";
    let every = "{\"id\":\"A001\",\"amount_usd\":10}\n{\"id\":\"A002\",\"amount_usd\":260}\n";
    let then = observed(&[(source, every)]);
    let recorded = json!({"grounding": [{
        "rule": "keep the rows where amount_usd is over 250", "field": "amount_usd",
        "source": source, "revision": then["observed"][0]["peek_sha256"],
        "grade": "observed_partial", "in_every_sampled_record": true, "bound_by": "request",
        "admissible": true, "open": null,
    }]});
    let fresh = |text: &str| basis(Some(&recorded), Some(&observed(&[(source, text)])), intent);
    assert_eq!(fresh(every), Basis::Holds(1));
    let some = "{\"id\":\"A001\",\"amount_usd\":10}\n{\"id\":\"A002\"}\n";
    assert_eq!(
        moved(fresh(some)),
        [format!(
            "`amount_usd` is now missing from some sampled records of `{source}`"
        )]
    );
    let none = "{\"id\":\"A001\",\"amount\":10}\n{\"id\":\"A002\",\"amount\":260}\n";
    assert_eq!(
        moved(fresh(none)),
        [format!(
            "`amount_usd` is no longer in the sampled records of `{source}`"
        )]
    );
}

#[test]
fn values_no_longer_all_numbers_move_a_policy_chosen_from_kinds() {
    let text = format!("{INPUT}A005,n/a,paid\n");
    let words = moved(judged(&text));
    assert_eq!(
        words,
        [format!(
            "`amount_usd` in `{SOURCE}` now has sampled values that are not numbers (text 1)"
        )]
    );
    let blank = format!("{INPUT}A006,,paid\n");
    assert!(moved(judged(&blank))[0].contains("(empty 1)"));
    let decimal = format!("{INPUT}A007,700.5,paid\n");
    assert_eq!(
        judged(&decimal),
        Basis::Holds(2),
        "a decimal text is a number"
    );
}

#[test]
fn every_recorded_dependency_is_judged_not_one_surviving_key() {
    let request = "read ./data/input.csv, keep the rows where amount_usd is over 250 and where status is paid, write them to ./out/result.json";
    let recorded = decision(request, &observed(&[(SOURCE, INPUT)]));
    let fields: Vec<&str> = recorded["grounding"]
        .as_array()
        .expect("grounding")
        .iter()
        .filter_map(|e| e["field"].as_str())
        .collect();
    assert!(
        fields.contains(&"amount_usd") && fields.contains(&"status"),
        "{fields:?}"
    );
    let renamed = INPUT.replace("status", "state");
    let words = moved(basis(
        Some(&recorded),
        Some(&observed(&[(SOURCE, &renamed)])),
        request,
    ));
    assert_eq!(
        words,
        [format!(
            "`{SOURCE}` no longer has `status`: it has id, amount_usd, state"
        )],
        "the surviving key does not hold the basis"
    );
}

#[test]
fn labels_never_hold_a_basis() {
    let mut recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    let renamed = observed(&[(SOURCE, &INPUT.replace("amount_usd", "amount"))]);
    for entry in recorded["grounding"].as_array_mut().expect("grounding") {
        entry["grade"] = json!("declared");
        entry["admissible"] = json!(false);
        entry["in_every_sampled_record"] = json!(true);
    }
    assert!(
        moved(basis(Some(&recorded), Some(&renamed), REQUEST))[0].contains("no longer has"),
        "a flag never excludes a dependency, a grade never keeps one"
    );
    recorded["grounding"][0]
        .as_object_mut()
        .expect("an entry")
        .remove("source");
    let fresh = observed(&[(SOURCE, INPUT)]);
    assert_eq!(
        basis(Some(&recorded), Some(&fresh), REQUEST),
        Basis::Unjudged(vec![
            "a recorded key names no source or no field".to_owned()
        ])
    );
}

#[test]
fn an_asserted_key_answers_to_what_the_fresh_observation_shows() {
    let source = "./trips.csv";
    let asserted = |revision: &str| {
        json!({"grounding": [{
            "rule": "keep the trips where km is over 10", "field": "km", "source": source,
            "revision": revision, "grade": "user_asserted", "in_every_sampled_record": true,
            "bound_by": "request", "admissible": true,
        }]})
    };
    let never = asserted("absent");
    let intent = "read ./trips.csv (columns vehicle, driver, km)";
    let judge = |recorded: &Value, fresh: &Value| basis(Some(recorded), Some(fresh), intent);
    assert_eq!(
        judge(&never, &unavailable(source, "absent")),
        Basis::Holds(1)
    );
    let header = observed(&[(source, "vehicle,driver\nv1,d1\n")]);
    assert!(moved(judge(&never, &header))[0].contains("no longer has `km`"));
    let sample = observed(&[("./trips.jsonl", "{\"vehicle\":\"v1\"}\n")]);
    let partial = json!({"observed": [{
        "path": source, "state": "observed", "complete": false, "kind": "jsonl",
        "columns": sample["observed"][0]["columns"], "common_columns": ["vehicle"],
    }]});
    assert_eq!(
        judge(&never, &partial),
        Basis::Holds(1),
        "a bounded sample disproves nothing"
    );
    let seen_then = asserted(&"a".repeat(64));
    assert!(moved(judge(&seen_then, &unavailable(source, "absent")))[0].contains("is absent now"));
}

#[test]
fn each_dependency_is_matched_to_its_exact_source() {
    let entry = |source: &str| {
        json!({"rule": "keep the rows where amount is over 1", "field": "amount",
            "source": source, "grade": "declared", "in_every_sampled_record": true,
            "bound_by": "request", "admissible": true})
    };
    let recorded = json!({"grounding": [entry("./a.csv"), entry("./b.csv")]});
    assert_eq!(Basis::sources(Some(&recorded)), ["./a.csv", "./b.csv"]);
    let fresh = observed(&[("./a.csv", "amount\n1\n"), ("./b.csv", "total\n1\n")]);
    assert_eq!(
        moved(basis(Some(&recorded), Some(&fresh), "")),
        ["`./b.csv` no longer has `amount`: it has total"],
        "a key of another source never holds this one"
    );
    let spelled = observed(&[("a.csv", "amount\n1\n"), ("b.csv", "amount\n2\n")]);
    assert_eq!(
        basis(Some(&recorded), Some(&spelled), ""),
        Basis::Holds(2),
        "`./a.csv` and `a.csv` are one path"
    );
}

#[test]
fn a_decision_without_a_source_dependency_has_no_basis() {
    let fresh = observed(&[(SOURCE, INPUT)]);
    assert_eq!(basis(None, Some(&fresh), REQUEST), Basis::None);
    let copy = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    assert_eq!(copy.status, CompileStatus::Ready);
    let recorded = copy.provenance.decision.as_ref();
    assert!(Basis::sources(recorded).is_empty());
    assert_eq!(basis(recorded, Some(&fresh), ""), Basis::None);
}

/// A decision that records no dependency is a legacy absence (`None`); a record that is present
/// but that this law cannot read — not a list, an entry not an object, a decision not an object
/// — is unjudged, never an absence that lets a proposal through as if it read nothing.
#[test]
fn a_present_record_this_law_cannot_read_is_unjudged_never_absent() {
    let fresh = observed(&[(SOURCE, INPUT)]);
    for absent in [json!({}), json!({"grounding": [], "numbers": []})] {
        assert_eq!(
            basis(Some(&absent), Some(&fresh), REQUEST),
            Basis::None,
            "{absent}"
        );
    }
    for unreadable in [
        json!({"grounding": {"field": "amount_usd"}}),
        json!({"grounding": "amount_usd"}),
        json!({"grounding": null}),
        json!({"numbers": 7}),
        json!({"grounding": ["amount_usd"]}),
        json!({"numbers": [["observed numbers"]]}),
        json!(["amount_usd"]),
    ] {
        assert!(
            matches!(
                basis(Some(&unreadable), Some(&fresh), REQUEST),
                Basis::Unjudged(_)
            ),
            "{unreadable}"
        );
    }
}

/// The kinds a fresh observation counted must be counts: a negative, fractional or textual count
/// cannot say the values are all numbers, so the policy chosen from them is unjudged. An empty
/// count map (no sampled value) contradicts nothing and holds.
#[test]
fn kind_counts_this_law_cannot_read_leave_the_policy_unjudged() {
    let recorded = decision(REQUEST, &observed(&[(SOURCE, INPUT)]));
    let with_counts = |counts: Value| {
        let mut fresh = observed(&[(SOURCE, INPUT)]);
        fresh["kinds"][SOURCE]["keys"]["amount_usd"] = counts;
        basis(Some(&recorded), Some(&fresh), REQUEST)
    };
    for counts in [
        json!({"number_text": 4, "text": -1}),
        json!({"number_text": 4, "text": 0.5}),
        json!({"number_text": "4"}),
    ] {
        assert!(
            matches!(with_counts(counts.clone()), Basis::Unjudged(_)),
            "{counts}"
        );
    }
    assert_eq!(with_counts(json!({})), Basis::Holds(2), "no sampled value");
}

#[test]
fn an_unknown_or_missing_numeric_discriminator_is_unjudged() {
    let fresh = observed(&[(SOURCE, INPUT)]);
    for entry in [
        json!({}),
        json!({"source": SOURCE, "field": "amount_usd"}),
        json!({"bound_by": "invented", "source": SOURCE, "field": "amount_usd"}),
    ] {
        let recorded = json!({"numbers": [entry]});
        assert!(
            matches!(
                basis(Some(&recorded), Some(&fresh), REQUEST),
                Basis::Unjudged(_)
            ),
            "a present numeric record must not disappear: {recorded}"
        );
    }
    for bound_by in ["answer", "pending", "unobserved", "grounding", "default"] {
        let recorded = json!({"numbers": [{"bound_by": bound_by}]});
        assert_eq!(
            basis(Some(&recorded), Some(&fresh), REQUEST),
            Basis::None,
            "a recognized policy not inferred from observed kinds creates no such dependency"
        );
    }
    let observed = json!({"numbers": [{"bound_by": "observed numbers", "source": SOURCE,
        "field": "amount_usd"}]});
    assert_eq!(
        basis(Some(&observed), Some(&fresh), REQUEST),
        Basis::Holds(1)
    );
}

/// An incoming grade is a claim, never the user's assertion of an unobserved key.
#[test]
fn a_user_asserted_label_needs_the_actual_request_and_exact_source() {
    let recorded = json!({"grounding": [{"source": SOURCE, "field": "amount_usd",
        "grade": "user_asserted", "bound_by": "request", "revision": "absent"}]});
    let fresh = unavailable(SOURCE, "absent");
    assert!(matches!(
        basis(Some(&recorded), Some(&fresh), REQUEST),
        Basis::Unjudged(_)
    ));
    let stated = format!("{REQUEST} (columns id, amount_usd, status)");
    assert_eq!(
        basis(Some(&recorded), Some(&fresh), &stated),
        Basis::Holds(1)
    );
    let other = stated.replace(SOURCE, "./data/other.csv");
    assert!(matches!(
        basis(Some(&recorded), Some(&fresh), &other),
        Basis::Unjudged(_)
    ));
    let partial = json!({"observed": [{"path": SOURCE, "state": "observed",
        "complete": false, "kind": "jsonl", "columns": ["id"], "common_columns": ["id"]}]});
    assert!(matches!(
        basis(Some(&recorded), Some(&partial), REQUEST),
        Basis::Moved(_)
    ));
    assert_eq!(
        basis(Some(&recorded), Some(&partial), &stated),
        Basis::Holds(1)
    );
}

const ANSWER_INTENT: &str =
    "Read ./orders.json, keep only the rows whose status is open and write them to ./out.json";

/// The actual question and answer round, not a fabricated decision, supplies the assertion.
fn answered_request() -> (CompileRequest, Value) {
    let request = CompileRequest::create(ANSWER_INTENT)
        .with_knowledge(unavailable("./orders.json", "absent"));
    let first = compile(&request).expect("first round");
    assert!(
        first
            .questions
            .iter()
            .any(|q| q.key == "const.rule_field_1")
    );
    let request = request
        .with_plan(first.provenance.plan.expect("question plan"))
        .answer("const.rule_field_1", "\"status\"");
    let answered = compile(&request).expect("answer round");
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:?}");
    (
        request,
        answered.provenance.decision.expect("grounding record"),
    )
}

#[test]
fn real_question_answers_hold_only_the_key_and_source_they_ground() {
    let (request, recorded) = answered_request();
    let fresh = unavailable("./orders.json", "absent");
    assert_eq!(
        basis_for(&request, Some(&recorded), Some(&fresh)),
        Basis::Holds(1)
    );
    let mut forged = recorded.clone();
    forged["grounding"][0]["field"] = json!("invented");
    assert!(matches!(
        basis_for(&request, Some(&forged), Some(&fresh)),
        Basis::Unjudged(_)
    ));
    forged["grounding"][0]["field"] = json!("status");
    forged["grounding"][0]["source"] = json!("./other.json");
    assert!(matches!(
        basis_for(
            &request,
            Some(&forged),
            Some(&unavailable("./other.json", "absent"))
        ),
        Basis::Unjudged(_)
    ));
    let mut stale = request.clone();
    stale.knowledge = Some(unavailable("./orders.json", "empty"));
    assert!(matches!(
        basis_for(&stale, Some(&recorded), Some(&fresh)),
        Basis::Unjudged(_)
    ));
    let header = observed(&[("./orders.json", "state\nopen\n")]);
    assert!(matches!(
        basis_for(&request, Some(&recorded), Some(&header)),
        Basis::Moved(_)
    ));
}

#[test]
fn a_replacement_uses_its_own_columns_and_subsequent_question_round() {
    let original = "Read ./old.json (columns id, status), keep only the rows whose status is open and write them to ./old-out.json";
    let (answered, recorded) = answered_request();
    let fresh = unavailable("./orders.json", "absent");
    let replacement = CompileRequest::create(original)
        .answer("intent.clarification", json!(ANSWER_INTENT).to_string());
    assert!(matches!(
        basis_for(&replacement, Some(&recorded), Some(&fresh)),
        Basis::Unjudged(_)
    ));
    let stated = replacement.clone().answer(
        "intent.clarification",
        json!(format!("{ANSWER_INTENT} (columns id, status)")).to_string(),
    );
    assert_eq!(
        basis_for(&stated, Some(&recorded), Some(&fresh)),
        Basis::Holds(1)
    );
    let mut kept = answered;
    kept.input = crate::types::Input::Create(original.to_owned());
    kept.answers.insert(
        "intent.clarification".to_owned(),
        json!(ANSWER_INTENT).to_string(),
    );
    assert_eq!(
        basis_for(&kept, Some(&recorded), Some(&fresh)),
        Basis::Holds(1)
    );
    for bad in ["null", "42", "[]", "\"\""] {
        let malformed = kept.clone().answer("intent.clarification", bad);
        assert!(matches!(
            basis_for(&malformed, Some(&recorded), Some(&fresh)),
            Basis::Unjudged(_)
        ));
    }
}

/// Admitted ranges belong to the exact words the caller accepted, not their byte offsets.
#[test]
fn a_basis_replacement_keeps_money_only_for_identical_input_bytes() {
    let spans = |words: &str| {
        crate::money::directives(words)
            .expect("money reading")
            .found
            .into_iter()
            .map(|directive| directive.span)
            .collect::<Vec<_>>()
    };
    let old = format!("{ANSWER_INTENT}. Budget: 5 USD");
    let replacement = format!("{ANSWER_INTENT}. Budget: 9 USD");
    assert_eq!(
        spans(&old),
        spans(&replacement),
        "same offsets, different grant"
    );
    let admitted = CompileRequest::create(&old).with_admitted_money(spans(&old));
    for text in [
        replacement.as_str(),
        "Read a different source. Budget: 9 USD",
    ] {
        let request = admitted
            .clone()
            .answer("intent.clarification", json!(text).to_string());
        let folded = effective_request(&request).expect("valid replacement");
        assert!(folded.money.is_empty(), "{text}: {:?}", folded.money);
        assert!(!folded.answers.contains_key("intent.clarification"));
        assert!(matches!(&folded.input, crate::types::Input::Create(words) if words == text));
    }
    let unchanged = admitted.answer("intent.clarification", json!(old).to_string());
    assert_eq!(
        effective_request(&unchanged).expect("same bytes").money,
        spans(&old)
    );
    let readmitted = CompileRequest::create(&replacement)
        .with_admitted_money(spans(&replacement))
        .answer("intent.clarification", json!(replacement).to_string());
    assert_eq!(
        effective_request(&readmitted)
            .expect("fresh admission")
            .money,
        spans(&replacement)
    );
}

/// A valid answered field must still replay after replacement. Old money ranges used to
/// make this effect-free replay refuse on unrelated bytes, losing a real user assertion.
#[test]
fn old_money_spans_do_not_break_a_replacements_actual_field_answer() {
    let (mut request, recorded) = answered_request();
    let old = "Budget: 5 USD. Read ./old.json and write it to ./old-out.json";
    let spans = crate::money::directives(old)
        .expect("old money")
        .found
        .into_iter()
        .map(|directive| directive.span)
        .collect();
    request.input = crate::types::Input::Create(old.to_owned());
    request = request
        .with_admitted_money(spans)
        .answer("intent.clarification", json!(ANSWER_INTENT).to_string());
    let fresh = unavailable("./orders.json", "absent");
    assert_eq!(
        basis_for(&request, Some(&recorded), Some(&fresh)),
        Basis::Holds(1)
    );
    let mut forged = recorded;
    forged["grounding"][0]["field"] = json!("invented");
    assert!(matches!(
        basis_for(&request, Some(&forged), Some(&fresh)),
        Basis::Unjudged(_)
    ));
}

#[test]
fn protected_parent_keeps_assertions_bound_to_the_only_source() {
    let source = "./records/input.jsonl";
    let intent = "Read ./records/input.jsonl (columns id, amount). Ne modifie rien dans ./records.";
    let recorded = json!({"grounding": [{
        "source": source, "field": "amount", "grade": "user_asserted",
        "bound_by": "request", "revision": "absent",
    }]});
    // A bounded sample does not disprove the request's explicit column assertion.
    let fresh = observed(&[(source, "{\"id\":1}\n")]);
    assert_eq!(
        basis(Some(&recorded), Some(&fresh), intent),
        Basis::Holds(1)
    );
    let two_sources = format!("{intent} Read ./other.jsonl.");
    assert!(matches!(
        basis(Some(&recorded), Some(&fresh), &two_sources),
        Basis::Moved(_)
    ));
    let unrelated = observed(&[("./other.jsonl", "{\"id\":1}\n")]);
    assert!(matches!(
        basis(Some(&recorded), Some(&unrelated), intent),
        Basis::Unjudged(_)
    ));
}
