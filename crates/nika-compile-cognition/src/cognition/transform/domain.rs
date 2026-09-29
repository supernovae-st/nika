// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The domain laws of a verified program (R4 A11): what it returns on sources other than the
//! seat's own example. A source with no row, or with no row the program keeps, is a valid input
//! of every workflow. The probes are the source with no row and each source holding one row of
//! the seat's own example: a row the clause excludes makes a source the program keeps nothing
//! of. The coverage is therefore the seat's own rows, never every predicate that keeps none.
//!
//! These laws judge values, never meaning: a program that omits the clause's predicate, or that
//! returns another shape than the request asks, can hold them. What the whole request asks is
//! the whole-request verifier's to judge; nothing here realizes a clause.

use super::{
    AuthoringPolicy, CompileOutcome, DiagnosticKind, ProposedTransform, ProviderInferDyn, Refusal,
    run,
};
use nika_compile_reader::aggregate::{AggOp, ranking_cue};
use serde_json::{Value, json};

/// The floor refusal, repaired from as it is stated.
pub(super) const EMPTY_DOMAIN: &str = "on a source with no row, or no row the program keeps, it returns null, which the workflow cannot write: return the value of the empty set (0 for a sum or a count, an empty list for rows) or stop with a stated error";

/// The identity refusal of a sum or a count, repaired from as it is stated.
pub(super) const EMPTY_SUM: &str = "the clause is a sum or a count: on a source with no row it must return the number 0 (the sum or the count of nothing), and a number on every source, never null and never an error";

/// Whether a refusal is a domain law's: the refusals a seat is sent back, within the request's
/// repairs.
fn repairable(why: &str) -> bool {
    why == EMPTY_DOMAIN || why == EMPTY_SUM
}

/// The floor, for every clause: the program returns no null on a probe, which the workflow
/// cannot write (`… | add` over no kept row). A stated error stops the run by name and holds
/// here: an average, a minimum or a maximum of no row has no value and may stop so, and rows
/// or groups of no row are an empty list.
pub(super) fn floor(program: &str, example: &[Value]) -> Result<(), Refusal> {
    if probes(example).any(|input| matches!(run(program, &input), Ok(Value::Null))) {
        return Err(Refusal(EMPTY_DOMAIN.to_owned()));
    }
    Ok(())
}

/// The identity of a sum or a count: when the clause reads as one and the program's value on
/// the seat's example is a scalar (a number, or one field holding a number), it returns exactly
/// 0 in that shape on the source with no row, and a number in that shape on each one-row
/// source. Null or a stated error is no sum of nothing: the request resolves.
///
/// The law is conditional: the seat chose its example's shape, so a scalar there neither
/// proves the request asked a scalar nor that the program is the whole sum it asked.
pub(super) fn identity(
    clause: &str,
    program: &str,
    example: &[Value],
    expected: &Value,
) -> Result<(), Refusal> {
    let Some((name, _)) = scalar(expected) else {
        return Ok(());
    };
    if !additive(clause) {
        return Ok(());
    }
    let stated = |input: &Value| {
        run(program, input).ok().and_then(|value| {
            scalar(&value)
                .filter(|(key, _)| *key == name)
                .map(|(_, n)| n)
        })
    };
    let mut probes = probes(example);
    let zero = probes.next().is_some_and(|none| stated(&none) == Some(0.0));
    if zero && probes.all(|one| stated(&one).is_some()) {
        return Ok(());
    }
    Err(Refusal(EMPTY_SUM.to_owned()))
}

/// The number a value states and the one field naming it, if any: a number, or an object
/// holding exactly one field whose value is a number (`{"total": 70}`).
fn scalar(value: &Value) -> Option<(Option<String>, f64)> {
    match value {
        Value::Number(n) => n.as_f64().map(|n| (None, n)),
        Value::Object(fields) if fields.len() == 1 => fields
            .iter()
            .next()
            .and_then(|(key, v)| v.as_f64().map(|n| (Some(key.clone()), n))),
        _ => None,
    }
}

/// Whether the clause is a sum or a count the reader reads: its leading word is one of the
/// reader's closed sum or count words, and nothing in it ranks the rows.
fn additive(clause: &str) -> bool {
    let lead = clause
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|c: char| !c.is_alphanumeric());
    matches!(AggOp::from_word(lead), Some(AggOp::Sum | AggOp::Count)) && !ranking_cue(clause)
}

/// The source with no row, then each source holding one row of the seat's own example.
fn probes(example: &[Value]) -> impl Iterator<Item = Value> + '_ {
    std::iter::once(json!({"records": []}))
        .chain(example.iter().map(|row| json!({"records": [row]})))
}

/// The repairs a request may buy, the policy's own (at most five): one allowance serves every
/// transform step and the field-answer replay of a request, never one per clause, and zero buys
/// no call (R4 A11). The physical call ceiling still bounds each call.
pub(super) struct Repairs(u32);

impl Repairs {
    /// What the policy still grants this request: its repairs, less the attempts the request
    /// already recorded (a synthesis re-entered after a verifier repair spends the same
    /// allowance, never a new one).
    pub(super) fn granted(policy: &AuthoringPolicy, out: &CompileOutcome) -> Self {
        let spent = out
            .provenance
            .decision
            .as_ref()
            .and_then(|d| d["transform_repairs"].as_array())
            .map_or(0, Vec::len);
        let spent = u32::try_from(spent).unwrap_or(u32::MAX);
        Self(policy.repairs.min(5).saturating_sub(spent))
    }

    /// A program a domain law refused is repaired from once while the allowance lasts: the seat
    /// reads its own program and the stated defect, never the human. Any other refusal stands
    /// as it is. The attempt is told and recorded under `transform_repairs` only once the
    /// receipt shows what became of its call ([`transport`]): a call the ceiling refused was
    /// requested, never sent. The caller admits an answer under the same laws; no answer keeps
    /// the defect, named with what became of the repair.
    pub(super) async fn repair<P: ProviderInferDyn>(
        &mut self,
        policy: &AuthoringPolicy,
        provider: &P,
        state: &Value,
        clause: &str,
        refused: (String, Refusal),
        out: &mut CompileOutcome,
    ) -> Result<ProposedTransform, Refusal> {
        let (program, Refusal(why)) = refused;
        if self.0 == 0 || !repairable(&why) {
            return Err(Refusal(why));
        }
        self.0 -= 1;
        let journaled = journal(out).len();
        let mut asked = state.clone();
        asked["verifier"] = json!({"your_program": program, "refused": why});
        let answer = super::propose_as(policy, provider, "transform_repair", asked, out).await;
        let call = transport(&journal(out)[journaled.min(journal(out).len())..]);
        // Only an answer shows a call went out: a provider may fail, and a timeout may fire,
        // before any transport, so those say what they are and leave the transport unobserved.
        let (kind, told) = match call {
            "answered" => (DiagnosticKind::Applied, "its call returned an answer"),
            "admission_refused" => (
                DiagnosticKind::Unknown,
                "the call ceiling refused its call before any transport",
            ),
            "timeout" => (
                DiagnosticKind::Unknown,
                "its call timed out, its transport unobserved",
            ),
            "provider_error" => (
                DiagnosticKind::Unknown,
                "its provider call failed, its transport unobserved",
            ),
            _ => (DiagnosticKind::Unknown, "no request was sent"),
        };
        crate::finding(
            out,
            kind,
            "authoring_transform",
            format!(
                "The seat's program for `{}` was refused: {why}. A repair from that defect was requested: {told}.",
                clause.trim()
            ),
        );
        let attempt = json!({"clause": clause, "refused_jq": program, "why": why, "call": call});
        record(out, "transform_repairs", vec![attempt]);
        answer.map_err(|Refusal(none)| {
            Refusal(format!(
                "{why}; the repair from it got no answer ({told}: {none})"
            ))
        })
    }
}

/// Append `entries` to the decision's `key` list: a request keeps every synthesis and every
/// repair attempt it made, in order, never only the last.
pub(super) fn record(out: &mut CompileOutcome, key: &str, entries: Vec<Value>) {
    if entries.is_empty() {
        return;
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    let mut kept = decision[key].as_array().cloned().unwrap_or_default();
    kept.extend(entries);
    decision[key] = json!(kept);
    out.provenance.decision = Some(decision);
}

/// The calls the receipt journaled so far, in call order.
fn journal(out: &CompileOutcome) -> &[Value] {
    out.provenance
        .authoring
        .as_ref()
        .map_or(&[], |receipt| receipt.context.as_slice())
}

/// What became of the last call the receipt journaled in `after`, the entries a repair added
/// (`answered`, `admission_refused`, `timeout`, `provider_error`), or `not_sent` when it added
/// none: the receipt, never the request, says what became of the call, and only an answer
/// shows that it left.
fn transport(after: &[Value]) -> &'static str {
    let Some(result) = after.last().map(|entry| &entry["result"]) else {
        return "not_sent";
    };
    if result.get("stop_reason").is_some() {
        return "answered";
    }
    match result["failure_kind"].as_str() {
        Some("admission_refused") => "admission_refused",
        Some("timeout") => "timeout",
        _ => "provider_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUM: &str = "sum qty over the rows where status is shipped";
    /// The program B16's composed `DeepSeek` run generated for [`SUM`] (the live counterexample).
    const GENERATED: &str =
        ".records | map(select(.status == \"shipped\") | .qty | tonumber) | add";
    const KEPT: &str = ".records | map(select(.status == \"shipped\") | .qty | tonumber)";

    /// The seat's own example: a shipped row beside a pending one, the row the clause excludes.
    fn example() -> Vec<Value> {
        vec![
            json!({"status": "shipped", "qty": "40"}),
            json!({"status": "pending", "qty": "15"}),
        ]
    }

    fn refused(law: &str) -> Result<(), Refusal> {
        Err(Refusal(law.to_owned()))
    }

    #[test]
    fn the_floor_refuses_null_on_no_row_and_on_a_row_the_program_keeps_nothing_of() {
        assert_eq!(floor(GENERATED, &example()), refused(EMPTY_DOMAIN));
        // Guarded on the source with no row only: null on the pending row alone.
        let guarded = format!("if (.records | length) == 0 then 0 else ({GENERATED}) end");
        assert_eq!(floor(&guarded, &example()), refused(EMPTY_DOMAIN));
        let rows = ".records | map(select(.status == \"shipped\"))";
        let count = format!("{rows} | length");
        let sum = format!("{GENERATED} // 0");
        let groups = ".records | group_by(.status) | map({status: .[0].status, total: (map(.qty | tonumber) | add)})";
        for holds in [rows, &count, &sum, groups] {
            assert_eq!(floor(holds, &example()), Ok(()), "{holds}");
        }
    }

    #[test]
    fn a_sum_of_no_row_is_zero_never_null_and_never_an_error() {
        let guarded = format!("if (.records | length) == 0 then 0 else ({GENERATED}) end");
        let error = format!("{KEPT} | if length == 0 then error else add end");
        for wrong in [GENERATED, &guarded, &error] {
            assert_eq!(
                identity(SUM, wrong, &example(), &json!(40)),
                refused(EMPTY_SUM),
                "{wrong}"
            );
        }
        let equivalents = [
            format!("{GENERATED} // 0"),
            "reduce (.records[] | select(.status == \"shipped\") | .qty | tonumber) as $q (0; . + $q)"
                .to_owned(),
            "[.records[] | select(.status == \"shipped\") | .qty | tonumber] | add // 0".to_owned(),
        ];
        for sum in &equivalents {
            assert_eq!(identity(SUM, sum, &example(), &json!(40)), Ok(()), "{sum}");
        }
        let count = ".records | map(select(.status == \"shipped\")) | length";
        let counted = "count the rows where status is shipped";
        assert_eq!(identity(counted, count, &example(), &json!(1)), Ok(()));
        let error = ".records | map(select(.status == \"shipped\")) | if length == 0 then error else length end";
        assert_eq!(
            identity(counted, error, &example(), &json!(1)),
            refused(EMPTY_SUM)
        );
    }

    #[test]
    fn a_sum_named_by_one_field_is_zero_in_that_field() {
        let named = |program: &str| format!("{{\"total\": ({program})}}");
        let expected = json!({"total": 40});
        let null = named(GENERATED);
        assert_eq!(
            identity(SUM, &null, &example(), &expected),
            refused(EMPTY_SUM)
        );
        let zero = named(&format!("{GENERATED} // 0"));
        assert_eq!(identity(SUM, &zero, &example(), &expected), Ok(()));
        // The field keeps its name where no row is kept: another name is another value.
        let renamed = format!("if (.records | length) == 0 then {{\"sum\": 0}} else {zero} end");
        assert_eq!(
            identity(SUM, &renamed, &example(), &expected),
            refused(EMPTY_SUM)
        );
    }

    /// An average, a minimum or a maximum of no row has no value (labelled apart from the sum):
    /// a stated error there holds, 0 is never required, and null is the floor's to refuse.
    #[test]
    fn an_average_a_minimum_or_a_maximum_keeps_the_floor_alone() {
        let error = format!("{KEPT} | if length == 0 then error else add / length end");
        let average = "average qty over the rows where status is shipped";
        assert_eq!(identity(average, &error, &example(), &json!(40)), Ok(()));
        assert_eq!(floor(&error, &example()), Ok(()));
        let maximum = "maximum qty over the rows where status is shipped";
        let max = format!("{KEPT} | max");
        assert_eq!(identity(maximum, &max, &example(), &json!(40)), Ok(()));
        assert_eq!(floor(&max, &example()), refused(EMPTY_DOMAIN));
    }

    /// A ranked sum, grouped totals and two fields are no scalar sum of the clause: the floor
    /// alone judges them.
    #[test]
    fn a_ranking_a_group_or_two_fields_keep_the_floor_alone() {
        assert_eq!(
            identity(
                "sum qty over the top 2 rows",
                GENERATED,
                &example(),
                &json!(40)
            ),
            Ok(())
        );
        let groups = ".records | group_by(.status) | map({status: .[0].status, total: (map(.qty | tonumber) | add)})";
        let grouped =
            json!([{"status": "pending", "total": 15}, {"status": "shipped", "total": 40}]);
        assert_eq!(
            identity("sum qty per status", groups, &example(), &grouped),
            Ok(())
        );
        let two = format!("{{\"total\": ({GENERATED}), \"n\": ({KEPT} | length)}}");
        assert_eq!(
            identity(SUM, &two, &example(), &json!({"total": 40, "n": 1})),
            Ok(())
        );
    }

    /// The laws judge values, never meaning (R4 A11): a sum over every row, the clause's
    /// predicate omitted, holds the identity; the kept rows returned instead of their sum hold
    /// the floor. Both stay the whole-request verifier's to judge, never realized here.
    #[test]
    fn the_domain_laws_leave_meaning_to_the_whole_request_verifier() {
        let every_row = ".records | map(.qty | tonumber) | add // 0";
        assert_eq!(identity(SUM, every_row, &example(), &json!(55)), Ok(()));
        assert_eq!(floor(every_row, &example()), Ok(()));
        let rows = ".records | map(select(.status == \"shipped\"))";
        let shape = json!([{"status": "shipped", "qty": "40"}]);
        assert_eq!(identity(SUM, rows, &example(), &shape), Ok(()));
        assert_eq!(floor(rows, &example()), Ok(()));
    }

    /// A repair is told by what its call became in the receipt, never by the request for it.
    #[test]
    fn the_receipt_says_what_became_of_a_repair_call() {
        assert_eq!(transport(&[]), "not_sent");
        let entry = |result: Value| json!({"call": "transform_repair", "result": result});
        let answered = entry(json!({"stop_reason": "EndTurn", "usage_reported": true}));
        assert_eq!(transport(&[answered]), "answered");
        let refused = entry(json!({"failure_kind": "admission_refused"}));
        assert_eq!(transport(&[refused]), "admission_refused");
        let timeout = entry(json!({"failure_kind": "timeout"}));
        assert_eq!(transport(&[timeout]), "timeout");
        let failed = entry(json!({"failure_kind": "provider_error"}));
        assert_eq!(transport(&[failed]), "provider_error");
    }

    #[test]
    fn the_leading_word_reads_the_clause() {
        assert!(additive(SUM));
        assert!(additive("Total the qty of the shipped rows"));
        assert!(additive("count the rows where status is shipped"));
        assert!(!additive("average qty over the rows"));
        assert!(!additive("compute the total qty"));
        assert!(!additive("the highest total amount"));
        assert!(!additive("sum the qty of the most expensive rows"));
    }
}
