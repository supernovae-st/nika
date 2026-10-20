// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a request proves read alone: only a sentence of the small admitted language — closed
//! read, rule and write productions, identities kept byte for byte — admitted by the strict HOT
//! door proves its write `Required` and its count's name free or stated. The positives are
//! requests the engine compiles READY under the strict door; every request outside the
//! language, however close, keeps `Unproven` and `Naming::Unknown`.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::Shape;
use nika_compile_reader::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan, Step as PlanStep};
use nika_compile_reader::rules::{Junction, Rule, synthesize};

use super::judge_tests::{budget, outcome, run, unwritten};
use super::provenance::{
    Production, Provenance, Written, contract_of_request, production, proven, read_request,
};
use super::{
    Contract, Naming, Obligation, Outcome, Presence, Requirement, RunEnd, Verdict, contract_of,
    judge,
};

const INPUT: &str = "./data/input.csv";
const RESULT: &str = "./out/result.json";
const ROWS: &str = "id,amount_usd,status\n1,5,paid\n2,20,late\n3,12,paid\n";
const PAID_ROWS: &str =
    r#"[{"id":1,"amount_usd":5,"status":"paid"},{"id":3,"amount_usd":12,"status":"paid"}]"#;
const COUNTED: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";
const KEPT: &str =
    "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json";
const COUNT_RULE: &str = "count the rows where status is paid";

fn write_of(contract: &Contract) -> &Obligation {
    contract
        .obligations
        .iter()
        .find(|obligation| obligation.target.is_some())
        .expect("the write")
}

fn namings(contract: &Contract) -> Vec<Naming> {
    let mut out = Vec::new();
    for obligation in &contract.obligations {
        if let Requirement::Computed { pipeline, .. } = &obligation.requirement {
            for step in &pipeline.steps {
                out.extend(
                    step.stages
                        .aggregates
                        .iter()
                        .map(|aggregate| aggregate.naming),
                );
            }
        }
    }
    out
}

#[test]
fn an_admitted_sentence_of_the_language_proves_its_write_and_its_free_count() {
    let (plan, provenance) = read_request(COUNTED);
    assert!(provenance.admitted(), "{provenance:?}");
    let produced = provenance
        .production
        .as_ref()
        .expect("a sentence of the language");
    assert_eq!(
        (
            produced.source.as_str(),
            produced.rule.as_str(),
            produced.target.as_str()
        ),
        (INPUT, COUNT_RULE, RESULT)
    );
    assert_eq!(produced.written, Written::Count);
    // Old: the plan alone proves neither the write nor the name.
    let old = contract_of(&plan, COUNTED, &BTreeMap::new());
    assert_eq!(write_of(&old).presence, Presence::Unproven);
    assert_eq!(namings(&old), [Naming::Unknown]);
    assert_eq!(
        outcome(&old, run(INPUT, ROWS, RESULT, r#"{"count":2}"#)),
        Outcome::Incomplete
    );
    let contract = contract_of_request(COUNTED, &BTreeMap::new());
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(namings(&contract), [Naming::Free]);
    for (output, expected) in [
        (r#"{"count":2}"#, Outcome::Passed),
        (r#"{"n":2}"#, Outcome::Passed),
        (r#"{"count":3}"#, Outcome::Failed),
        (r#"{"count":2,"n":2}"#, Outcome::Failed),
        ("{}", Outcome::Failed),
        ("2", Outcome::Failed),
    ] {
        assert_eq!(
            outcome(&contract, run(INPUT, ROWS, RESULT, output)),
            expected,
            "{output}"
        );
    }
    assert_eq!(
        outcome(&contract, unwritten(RunEnd::Completed, INPUT, ROWS, RESULT)),
        Outcome::Failed
    );
    let report = judge(
        &contract,
        &[run(INPUT, ROWS, RESULT, r#"{"count":2}"#)],
        &mut budget(),
    );
    assert_eq!(report.verdict(), Verdict::Certified);
}

#[test]
fn the_filter_sentence_proves_its_write_and_judges_the_kept_rows() {
    let (plan, provenance) = read_request(KEPT);
    assert_eq!(
        provenance
            .production
            .as_ref()
            .map(|produced| produced.written.clone()),
        Some(Written::Rows),
        "{provenance:?}"
    );
    let old = contract_of(&plan, KEPT, &BTreeMap::new());
    assert_eq!(
        outcome(&old, run(INPUT, ROWS, RESULT, PAID_ROWS)),
        Outcome::Incomplete
    );
    let contract = contract_of_request(KEPT, &BTreeMap::new());
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(
        outcome(&contract, run(INPUT, ROWS, RESULT, PAID_ROWS)),
        Outcome::Passed
    );
    for wrong in [
        r#"[{"id":1,"amount_usd":5,"status":"paid"}]"#,
        r#"[{"id":2,"amount_usd":20,"status":"late"}]"#,
        "[]",
    ] {
        assert_eq!(
            outcome(&contract, run(INPUT, ROWS, RESULT, wrong)),
            Outcome::Failed,
            "{wrong}"
        );
    }
}

/// The person's answer to « which observed field does `state` mean » binds the rule the
/// request's own contract judges, as the compile binds it in the candidate (the workspace
/// typed choice, 2026-10-19: the `status` filter wrote the two open rows, while the contract,
/// still on `state`, asked for none and withdrew the proposal). An unanswered contract keeps
/// the request's word, and a program a seat wrote is never renamed.
#[test]
fn an_answered_field_binds_the_rule_the_contract_judges() {
    const TICKETS: &str = "./tickets.csv";
    const OPEN: &str = "./open.csv";
    const TABLE: &str = "id,status,amount\n1,open,10\n2,closed,20\n3,open,30\n";
    const ASKED: &str =
        "Read ./tickets.csv, keep only the rows whose state is open and write them to ./open.csv";
    const KEPT_ROWS: &str = "id,status,amount\n1,open,10\n3,open,30\n";
    let field =
        |name: &str| BTreeMap::from([("const.rule_field_1".to_owned(), format!("\"{name}\""))]);
    let unanswered = contract_of_request(ASKED, &BTreeMap::new());
    assert_eq!(
        outcome(&unanswered, run(TICKETS, TABLE, OPEN, KEPT_ROWS)),
        Outcome::Failed,
        "the request's own word reads no `state` column: {unanswered:?}"
    );
    // The kept rows hold; this phrasing proves no unconditional write, so nothing certifies.
    let answered = contract_of_request(ASKED, &field("status"));
    assert_eq!(
        outcome(&answered, run(TICKETS, TABLE, OPEN, KEPT_ROWS)),
        Outcome::Incomplete,
        "{answered:?}"
    );
    for wrong in [
        "id,status,amount\n1,open,10\n",
        "id,status,amount\n2,closed,20\n",
    ] {
        assert_eq!(
            outcome(&answered, run(TICKETS, TABLE, OPEN, wrong)),
            Outcome::Failed,
            "{wrong}"
        );
    }
    // The same word answered is the request's word: nothing rebinds.
    assert_eq!(
        format!("{:?}", contract_of_request(ASKED, &field("state"))),
        format!("{unanswered:?}")
    );
}

#[test]
fn a_request_outside_the_language_proves_nothing() {
    for intent in [
        "read ./data/input.csv, count the rows where status is paid, write the result contingent on a nonempty input to ./out/result.json",
        "read ./data/input.csv, count the rows where status is paid, write the result under paid_count to ./out/result.json",
        "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json only when there are some",
        "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json if any",
        "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json and to ./out/copy.json",
        "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json, then tell me",
        "read ./data/input.csv, count the rows where status is paid or pending, write the count to ./out/result.json",
        "read ./data/input.csv, count the rows where status is paid and amount_usd is over 10, write the count to ./out/result.json",
        "read ./data/input.csv, count the paid rows where amount_usd is over 10, write the count to ./out/result.json",
        "read  ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json",
    ] {
        let (_, provenance) = read_request(intent);
        assert_eq!(provenance.production, None, "{intent}");
        let contract = contract_of_request(intent, &BTreeMap::new());
        assert!(
            contract
                .obligations
                .iter()
                .all(|obligation| obligation.presence != Presence::Required),
            "{intent}: {contract:?}"
        );
        assert!(
            namings(&contract)
                .iter()
                .all(|naming| *naming == Naming::Unknown),
            "{intent}"
        );
        let report = judge(
            &contract,
            &[run(INPUT, ROWS, RESULT, r#"{"count":2}"#)],
            &mut budget(),
        );
        assert!(!report.certified(), "{intent}");
    }
}

/// The plan of one read, one rule of `rule` and one write of `target`, built by hand: the
/// language's own parse is witnessed without depending on the reader's admission of each
/// phrasing.
fn hand_plan(rule: &str, target: &str) -> Plan {
    hand_plan_at(INPUT, rule, target)
}

/// The same plan reading `source`.
fn hand_plan_at(source: &str, rule: &str, target: &str) -> Plan {
    let mut plan = Plan::default();
    plan.steps.push(PlanStep::new(
        Op::Read,
        format!("read {source}"),
        source,
        Vec::new(),
    ));
    plan.steps
        .push(PlanStep::new(Op::Compute, rule, rule, Vec::new()));
    plan.rules
        .push(synthesize(rule, &[]).expect("a rule the closed grammar reads"));
    plan.effects.push(Effect::new(
        EffectVerb::Write,
        target,
        "write the result",
        EffectPolicy::Automatic,
    ));
    plan
}

#[test]
fn the_language_is_closed_and_keeps_its_identities_byte_for_byte() {
    let plan = hand_plan(COUNT_RULE, RESULT);
    let parse = |intent: &str| production(intent, &plan).map(|produced| produced.written);
    for (intent, written) in [
        (COUNTED, Some(Written::Count)),
        (
            "Read ./data/input.csv, count the rows where status is paid, and write it to ./out/result.json.",
            Some(Written::Count),
        ),
        (
            "read ./data/input.csv and count the rows where status is paid and write the count to ./out/result.json",
            Some(Written::Count),
        ),
        (
            "read ./data/input.csv, count the rows where status is paid, write the count as paid_count to ./out/result.json",
            Some(Written::Labelled("paid_count".to_owned())),
        ),
        (
            "read ./data/input.csv, count the rows where status is paid, write the count as Paid_Count to ./out/result.json",
            Some(Written::Labelled("Paid_Count".to_owned())),
        ),
        // Residue, another object, another path or source, a label or value outside its class,
        // a prefix, a doubled space, a comparison other than `is`.
        (
            "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json now",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status is paid, write them to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status is paid, write the count to ./out/other.json",
            None,
        ),
        (
            "read ./data/other.csv, count the rows where status is paid, write the count to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status is paid, write the count as paid-count to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status is \"paid\", write the count to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status is 10, write the count to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv, count the rows where status equals paid, write the count to ./out/result.json",
            None,
        ),
        (
            "please read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json",
            None,
        ),
        (
            "read ./data/input.csv,  count the rows where status is paid, write the count to ./out/result.json",
            None,
        ),
    ] {
        assert_eq!(parse(intent), written, "{intent}");
    }
}

#[test]
fn the_plan_must_agree_with_the_sentence_byte_for_byte() {
    let plan = hand_plan(COUNT_RULE, RESULT);
    let parse = |intent: &str| production(intent, &plan).map(|produced| produced.written);
    // The rule clause is the typed rule's own bytes: a capital letter in it is another rule text.
    assert_eq!(
        parse(
            "read ./data/input.csv, Count the rows where status is paid, write the count to ./out/result.json"
        ),
        None
    );
    let capital = hand_plan("Count the rows where status is paid", RESULT);
    assert_eq!(
        production(
            "read ./data/input.csv, Count the rows where status is paid, write the count to ./out/result.json",
            &capital
        )
        .map(|produced| produced.written),
        Some(Written::Count)
    );
    // The kind of the rule clause and of the typed rule agree.
    let kept = hand_plan("keep the rows where status is paid", RESULT);
    assert_eq!(production(COUNTED, &kept), None);
    assert_eq!(
        production(KEPT, &kept).map(|produced| produced.written),
        Some(Written::Rows)
    );
    // The plan holds nothing else.
    let mut extra = hand_plan(COUNT_RULE, RESULT);
    extra.unknowns.push("and archive the old ones".to_owned());
    assert_eq!(production(COUNTED, &extra), None);
    let mut twice = hand_plan(COUNT_RULE, RESULT);
    twice.effects.push(Effect::new(
        EffectVerb::Write,
        "./out/copy.json",
        "and a copy",
        EffectPolicy::Automatic,
    ));
    assert_eq!(production(COUNTED, &twice), None);
    let mut gated = hand_plan(COUNT_RULE, RESULT);
    gated.effects[0].policy = EffectPolicy::HumanFirst;
    assert_eq!(production(COUNTED, &gated), None);
}

#[test]
fn a_stated_label_keeps_its_bytes_and_nothing_is_proven_without_admission() {
    const LABELLED: &str = "read ./data/input.csv, count the rows where status is paid, write the count as Paid_Count to ./out/result.json";
    let plan = hand_plan(COUNT_RULE, RESULT);
    let produced = production(LABELLED, &plan).expect("a sentence of the language");
    let admitted = Provenance {
        production: Some(produced.clone()),
        ..Provenance::default()
    };
    let contract = proven(contract_of(&plan, LABELLED, &BTreeMap::new()), &admitted);
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(namings(&contract), [Naming::Stated]);
    for (output, expected) in [
        (r#"{"Paid_Count":2}"#, Outcome::Passed),
        (r#"{"paid_count":2}"#, Outcome::Failed),
        (r#"{"n":2}"#, Outcome::Failed),
        (r#"{"count":2}"#, Outcome::Failed),
        (r#"{"Paid_Count":3}"#, Outcome::Failed),
    ] {
        assert_eq!(
            outcome(&contract, run(INPUT, ROWS, RESULT, output)),
            expected,
            "{output}"
        );
    }
    // The same sentence on a refused reading proves nothing.
    let refused = Provenance {
        rejections: vec!["1 unresolved clause(s)".to_owned()],
        production: Some(produced),
    };
    let unchanged = proven(contract_of(&plan, LABELLED, &BTreeMap::new()), &refused);
    assert_eq!(write_of(&unchanged).presence, Presence::Unproven);
    assert_eq!(namings(&unchanged), [Naming::Unknown]);
    // A production owns its target only.
    let elsewhere = Provenance {
        production: Some(Production {
            target: "./out/other.json".to_owned(),
            ..production(COUNTED, &plan).expect("a sentence")
        }),
        ..Provenance::default()
    };
    let foreign = proven(contract_of(&plan, COUNTED, &BTreeMap::new()), &elsewhere);
    assert_eq!(write_of(&foreign).presence, Presence::Unproven);
    assert_eq!(namings(&foreign), [Naming::Unknown]);
}

#[test]
fn the_sentence_is_matched_over_the_callers_own_bytes() {
    // The reader reads the apostrophe-folded copy: its plan names the ASCII form.
    const CURLY: &str = "read ./data/O\u{2019}Neil.csv, count the rows where status is paid, write the count to ./out/result.json";
    const ASCII: &str = "read ./data/O'Neil.csv, count the rows where status is paid, write the count to ./out/result.json";
    let folded_source = hand_plan_at("./data/O'Neil.csv", COUNT_RULE, RESULT);
    let written =
        |intent: &str, plan: &Plan| production(intent, plan).map(|produced| produced.written);
    assert_eq!(written(CURLY, &folded_source), None);
    assert_eq!(written(ASCII, &folded_source), Some(Written::Count));
    let folded_target = hand_plan_at(INPUT, COUNT_RULE, "./out/O'Neil.json");
    assert_eq!(
        written(
            "read ./data/input.csv, count the rows where status is paid, write the count to ./out/O\u{2018}Neil.json",
            &folded_target
        ),
        None
    );
    assert_eq!(
        written(
            "read ./data/input.csv, count the rows where status is paid, write the count to ./out/O'Neil.json",
            &folded_target
        ),
        Some(Written::Count)
    );
    // Both entries hand the caller's bytes to the production.
    let (_, provenance) = read_request(CURLY);
    assert_eq!(provenance.production, None, "{provenance:?}");
    let contract = contract_of_request(CURLY, &BTreeMap::new());
    assert!(
        contract
            .obligations
            .iter()
            .all(|obligation| obligation.presence != Presence::Required),
        "{contract:?}"
    );
    assert!(
        namings(&contract)
            .iter()
            .all(|naming| *naming == Naming::Unknown)
    );
    // One terminal period is punctuation; a name ending in its own period is not proven.
    let plan = hand_plan(COUNT_RULE, RESULT);
    assert_eq!(
        written(format!("{COUNTED}.").as_str(), &plan),
        Some(Written::Count)
    );
    assert_eq!(written(format!("{COUNTED}..").as_str(), &plan), None);
}

const TOTAL: &str = "read ./data/input.csv, compute the total of the amount_usd column and write the total to ./out/result.json";
const GROUPED: &str = "read ./data/input.csv, compute the total of the amount_usd column per status and write it to ./out/result.json";
const SORTED: &str = "read ./data/input.csv, sort the rows by amount_usd descending and write them to ./out/result.json";
const TOP: &str =
    "read ./data/input.csv, keep the top 2 rows by amount_usd and write them to ./out/result.json";
const TOTAL_RULE: &str = "the total of the amount_usd column";
const GROUPED_RULE: &str = "the total of the amount_usd column per status";
const SORTED_RULE: &str = "sort the rows by amount_usd descending";
const TOP_RULE: &str = "keep the top 2 rows by amount_usd";
const ROW_1: &str = r#"{"id":1,"amount_usd":5,"status":"paid"}"#;
const ROW_2: &str = r#"{"id":2,"amount_usd":20,"status":"late"}"#;
const ROW_3: &str = r#"{"id":3,"amount_usd":12,"status":"paid"}"#;

/// The rows as one JSON list.
fn rows(items: &[&str]) -> String {
    format!("[{}]", items.join(","))
}

/// The outcome of the written result of `contract` on a completed run over `input`.
fn written_as(contract: &Contract, input: &str, output: &str) -> Outcome {
    outcome(contract, run(INPUT, input, RESULT, output))
}

/// What a contract judges of a run that wrote nothing.
fn absent(contract: &Contract) -> Outcome {
    outcome(contract, unwritten(RunEnd::Completed, INPUT, ROWS, RESULT))
}

#[test]
fn totals_sorts_and_cuts_are_sentences_of_the_language() {
    for (intent, rule, written) in [
        (TOTAL, TOTAL_RULE, Written::Labelled("total".to_owned())),
        (GROUPED, GROUPED_RULE, Written::Total),
        (SORTED, SORTED_RULE, Written::Rows),
        (TOP, TOP_RULE, Written::Rows),
    ] {
        let plan = hand_plan(rule, RESULT);
        let produced = production(intent, &plan).expect("a sentence of the language");
        assert_eq!(produced.written, written, "{intent}");
        assert_eq!(produced.target, RESULT, "{intent}");
        assert!(intent.contains(produced.rule.as_str()), "{intent}");
    }
    // The other names and directions of the closed words, each its own rule.
    for (intent, rule, written) in [
        (
            "read ./data/input.csv, compute the average of the amount_usd column, and write it to ./out/result.json",
            "the average of the amount_usd column",
            Written::Total,
        ),
        (
            "read ./data/input.csv, compute the maximum of the amount_usd column per status and write the maximum to ./out/result.json",
            "the maximum of the amount_usd column per status",
            Written::Labelled("maximum".to_owned()),
        ),
        (
            "Read ./data/input.csv, sort the rows by amount_usd and write them to ./out/result.json.",
            "sort the rows by amount_usd",
            Written::Rows,
        ),
        (
            "read ./data/input.csv, sort the rows by amount_usd ascending, write them to ./out/result.json",
            "sort the rows by amount_usd ascending",
            Written::Rows,
        ),
    ] {
        let plan = hand_plan(rule, RESULT);
        assert_eq!(
            production(intent, &plan).map(|produced| produced.written),
            Some(written),
            "{intent}"
        );
    }
}

#[test]
fn a_near_miss_of_a_total_a_sort_or_a_cut_proves_nothing() {
    let parse = |intent: &str, rule: &str| {
        production(intent, &hand_plan(rule, RESULT)).map(|produced| produced.written)
    };
    for (intent, rule) in [
        // A word outside the closed set, or the same word in another case.
        (
            "read ./data/input.csv, compute the totals of the amount_usd column and write it to ./out/result.json",
            TOTAL_RULE,
        ),
        (
            "read ./data/input.csv, compute the Total of the amount_usd column and write it to ./out/result.json",
            TOTAL_RULE,
        ),
        // A column not named as one, another name in the write, another field or group.
        (
            "read ./data/input.csv, compute the total of amount_usd and write it to ./out/result.json",
            TOTAL_RULE,
        ),
        (
            "read ./data/input.csv, compute the total of the amount_usd column and write the sum to ./out/result.json",
            TOTAL_RULE,
        ),
        (
            "read ./data/input.csv, compute the total of the id column and write it to ./out/result.json",
            TOTAL_RULE,
        ),
        (
            "read ./data/input.csv, compute the total of the amount_usd column per status and write it to ./out/result.json",
            TOTAL_RULE,
        ),
        (
            "read ./data/input.csv, compute the total of the amount_usd column and write it to ./out/result.json",
            GROUPED_RULE,
        ),
        // Another direction, another size, a size in words or zero, residue after the key.
        (
            "read ./data/input.csv, sort the rows by amount_usd ascending and write them to ./out/result.json",
            SORTED_RULE,
        ),
        (
            "read ./data/input.csv, sort the rows by amount_usd, descending, and write them to ./out/result.json",
            SORTED_RULE,
        ),
        (
            "read ./data/input.csv, keep the top 3 rows by amount_usd and write them to ./out/result.json",
            TOP_RULE,
        ),
        (
            "read ./data/input.csv, keep the top two rows by amount_usd and write them to ./out/result.json",
            TOP_RULE,
        ),
        (
            "read ./data/input.csv, keep the top 0 rows by amount_usd and write them to ./out/result.json",
            TOP_RULE,
        ),
        // A cut is no sort, and rows are no total.
        (
            "read ./data/input.csv, sort the rows by amount_usd descending and write them to ./out/result.json",
            TOP_RULE,
        ),
        (
            "read ./data/input.csv, keep the top 2 rows by amount_usd and write it to ./out/result.json",
            TOP_RULE,
        ),
    ] {
        assert_eq!(parse(intent, rule), None, "{intent} over {rule}");
    }
}

#[test]
fn a_proven_total_is_required_named_and_judged_by_its_value() {
    let plan = hand_plan(TOTAL_RULE, RESULT);
    let admitted = Provenance {
        production: production(TOTAL, &plan),
        ..Provenance::default()
    };
    // Old: the plan alone proves neither the write nor the name.
    let old = contract_of(&plan, TOTAL, &BTreeMap::new());
    assert_eq!(write_of(&old).presence, Presence::Unproven);
    assert_eq!(
        written_as(&old, ROWS, r#"{"total":37}"#),
        Outcome::Incomplete
    );
    assert_eq!(absent(&old), Outcome::Incomplete);
    let contract = proven(contract_of(&plan, TOTAL, &BTreeMap::new()), &admitted);
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(namings(&contract), [Naming::Stated]);
    for (output, expected) in [
        (r#"{"total":37}"#, Outcome::Passed),
        (r#"{"total":36}"#, Outcome::Failed),
        (r#"{"sum":37}"#, Outcome::Failed),
        ("37", Outcome::Failed),
    ] {
        assert_eq!(written_as(&contract, ROWS, output), expected, "{output}");
    }
    assert_eq!(absent(&contract), Outcome::Failed);
}

#[test]
fn a_proven_group_total_is_required_and_its_free_name_is_certified() {
    let plan = hand_plan(GROUPED_RULE, RESULT);
    let admitted = Provenance {
        production: production(GROUPED, &plan),
        ..Provenance::default()
    };
    let right = r#"[{"status":"paid","total":17},{"status":"late","total":20}]"#;
    let old = contract_of(&plan, GROUPED, &BTreeMap::new());
    assert_eq!(written_as(&old, ROWS, right), Outcome::Incomplete);
    let contract = proven(contract_of(&plan, GROUPED, &BTreeMap::new()), &admitted);
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(namings(&contract), [Naming::Free]);
    for (output, expected) in [
        (right, Outcome::Passed),
        (
            r#"[{"status":"late","sum":20},{"status":"paid","sum":17}]"#,
            Outcome::Passed,
        ),
        (r#"[{"status":"paid","total":17}]"#, Outcome::Failed),
        (
            r#"[{"status":"paid","total":37},{"status":"late","total":20}]"#,
            Outcome::Failed,
        ),
        (r#"{"total":37}"#, Outcome::Failed),
    ] {
        assert_eq!(written_as(&contract, ROWS, output), expected, "{output}");
    }
    assert_eq!(absent(&contract), Outcome::Failed);
}

#[test]
fn a_proven_sort_is_required_and_leaves_its_ties_free() {
    let plan = hand_plan(SORTED_RULE, RESULT);
    let admitted = Provenance {
        production: production(SORTED, &plan),
        ..Provenance::default()
    };
    let right = rows(&[ROW_2, ROW_3, ROW_1]);
    let old = contract_of(&plan, SORTED, &BTreeMap::new());
    assert_eq!(written_as(&old, ROWS, &right), Outcome::Incomplete);
    let contract = proven(contract_of(&plan, SORTED, &BTreeMap::new()), &admitted);
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(written_as(&contract, ROWS, &right), Outcome::Passed);
    for wrong in [
        rows(&[ROW_1, ROW_3, ROW_2]),
        rows(&[ROW_2, ROW_3]),
        rows(&[]),
    ] {
        assert_eq!(
            written_as(&contract, ROWS, &wrong),
            Outcome::Failed,
            "{wrong}"
        );
    }
    assert_eq!(absent(&contract), Outcome::Failed);
    // Two rows of equal keys come in either order; no other row moves.
    let tied = "id,amount_usd,status\n1,5,paid\n2,20,late\n3,5,paid\n";
    let tied_3 = r#"{"id":3,"amount_usd":5,"status":"paid"}"#;
    for valid in [rows(&[ROW_2, ROW_1, tied_3]), rows(&[ROW_2, tied_3, ROW_1])] {
        assert_eq!(
            written_as(&contract, tied, &valid),
            Outcome::Passed,
            "{valid}"
        );
    }
    assert_eq!(
        written_as(&contract, tied, &rows(&[ROW_1, ROW_2, tied_3])),
        Outcome::Failed
    );
}

#[test]
fn a_proven_cut_is_required_and_a_tie_at_its_edge_keeps_either_row() {
    let plan = hand_plan(TOP_RULE, RESULT);
    let admitted = Provenance {
        production: production(TOP, &plan),
        ..Provenance::default()
    };
    let right = rows(&[ROW_2, ROW_3]);
    let old = contract_of(&plan, TOP, &BTreeMap::new());
    assert_eq!(written_as(&old, ROWS, &right), Outcome::Incomplete);
    let contract = proven(contract_of(&plan, TOP, &BTreeMap::new()), &admitted);
    assert_eq!(write_of(&contract).presence, Presence::Required);
    assert_eq!(written_as(&contract, ROWS, &right), Outcome::Passed);
    for wrong in [
        rows(&[ROW_2, ROW_1]),
        rows(&[ROW_3, ROW_2]),
        rows(&[ROW_2, ROW_3, ROW_1]),
        rows(&[ROW_2]),
    ] {
        assert_eq!(
            written_as(&contract, ROWS, &wrong),
            Outcome::Failed,
            "{wrong}"
        );
    }
    assert_eq!(absent(&contract), Outcome::Failed);
    let edge = "id,amount_usd,status\n1,12,paid\n2,20,late\n3,12,paid\n";
    let edge_1 = r#"{"id":1,"amount_usd":12,"status":"paid"}"#;
    for valid in [rows(&[ROW_2, edge_1]), rows(&[ROW_2, ROW_3])] {
        assert_eq!(
            written_as(&contract, edge, &valid),
            Outcome::Passed,
            "{valid}"
        );
    }
}

#[test]
fn the_reader_admits_each_new_sentence_and_its_contract_is_proven() {
    for (intent, written) in [
        (TOTAL, Written::Labelled("total".to_owned())),
        (GROUPED, Written::Total),
        (SORTED, Written::Rows),
        (TOP, Written::Rows),
    ] {
        let (_, provenance) = read_request(intent);
        assert!(provenance.admitted(), "{intent}: {provenance:?}");
        let produced = provenance.production.as_ref().expect("a sentence");
        assert_eq!(produced.written, written, "{intent}");
        let contract = contract_of_request(intent, &BTreeMap::new());
        assert_eq!(write_of(&contract).presence, Presence::Required, "{intent}");
        assert_eq!(absent(&contract), Outcome::Failed, "{intent}");
    }
}

#[test]
fn a_rule_whose_words_match_but_whose_relation_differs_proves_nothing() {
    // The plan's rule keeps the sentence's words but types another key, direction or size.
    let typed = |text: &str, key: &str, descending: bool, limit: Option<u32>| {
        let mut shape = Shape::default();
        shape.sort_by = Some((key.to_owned(), descending));
        let rule = Rule::typed(text, Vec::new(), Junction::And, shape);
        let mut plan = hand_plan(text, RESULT);
        plan.rules = vec![limit.map_or(rule.clone(), |n| rule.with_limit(n))];
        plan
    };
    for (intent, plan) in [
        (SORTED, typed(SORTED_RULE, "id", true, None)),
        (SORTED, typed(SORTED_RULE, "amount_usd", false, None)),
        (SORTED, typed(SORTED_RULE, "amount_usd", true, Some(2))),
        (TOP, typed(TOP_RULE, "amount_usd", true, Some(3))),
        (TOP, typed(TOP_RULE, "amount_usd", false, Some(2))),
        (TOP, typed(TOP_RULE, "id", true, Some(2))),
    ] {
        assert_eq!(production(intent, &plan), None, "{intent}");
    }
    // The same words typed as the sentence states them are proven.
    assert_eq!(
        production(SORTED, &typed(SORTED_RULE, "amount_usd", true, None))
            .map(|produced| produced.written),
        Some(Written::Rows)
    );
    assert_eq!(
        production(TOP, &typed(TOP_RULE, "amount_usd", true, Some(2)))
            .map(|produced| produced.written),
        Some(Written::Rows)
    );
}
