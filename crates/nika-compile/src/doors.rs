// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The deterministic doors of the Compile core (ADR-140): the HOT admission over the reader's
//! own vocabulary, the record replay — an answer round replays the plan or the native
//! candidate its previous round produced, zero calls — and the records every door writes into
//! the provenance (the route, the retrieval, the plan, the obligation ledger, the intent
//! digest). The seats' doors (a model proposes, writes or sketches; the compiler judges) live
//! above, in `nika-compile-cognition`, and read these through [`crate::surface`].

use std::collections::BTreeSet;

use super::{
    CompileError, CompileOutcome, CompileRequest, DiagnosticKind, HotPolicy, QuestionType,
    Strategy,
    gates::backstop,
    lexicon::{self, Reading},
    plan::Plan,
};
use serde_json::{Value, json};

/// The strict admission contract, the legacy one, or none.
pub(crate) fn admit_hot(
    intent: &str,
    reading: &Reading,
    hot: HotPolicy,
) -> Result<(), Vec<String>> {
    match hot {
        HotPolicy::Off => Err(vec!["hot policy off".to_owned()]),
        HotPolicy::Legacy => {
            if reading.complete() {
                Ok(())
            } else {
                Err(vec!["reading incomplete".to_owned()])
            }
        }
        HotPolicy::Strict => {
            let mut why = reading.hot_rejections();
            why.extend(super::hot::rejections(intent, reading));
            if why.is_empty() { Ok(()) } else { Err(why) }
        }
    }
}

/// Under the strict contract, a lexical WARM may only settle an otherwise explicit reading.
#[must_use]
pub fn lexical_rest_is_explicit(intent: &str, reading: &Reading) -> bool {
    let mut why = reading.hot_rejections();
    why.extend(super::hot::rejections(intent, reading));
    why.iter().all(|why| why.contains("ambiguous clause"))
}

/// Recall only: what the embedded candidate index returns for the request text and, once a
/// plan exists, for its operation words. Recorded so recall can be measured against labeled
/// cases; nothing here selects a candidate, ranks a verdict or widens authority.
pub fn record_retrieval(out: &mut CompileOutcome, intent: &str, plan: Option<&Plan>) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    if decision.get("intent_sha256").is_none() {
        // The key a transport files a recorded plan under; the same fold as the reader.
        decision["intent_sha256"] = json!(intent_sha256(intent));
    }
    if decision.get("retrieval").is_none() {
        decision["retrieval"] = json!({});
    }
    let project = |hits: Vec<super::retrieve::Hit>| -> serde_json::Value {
        json!(
            hits.iter()
                .map(|hit| {
                    json!({
                        "id": hit.id,
                        "kind": if hit.kind == super::retrieve::HitKind::Skeleton { "skeleton" } else { "family" },
                        "score": (hit.score * 1000.0).round() / 1000.0,
                    })
                })
                .collect::<Vec<_>>()
        )
    };
    if decision["retrieval"].get("by_intent").is_none() {
        decision["retrieval"]["by_intent"] = project(super::retrieve::retrieve(intent, 10));
    }
    if let Some(plan) = plan {
        let mut words: Vec<&str> = plan.steps.iter().map(|step| step.op.word()).collect();
        words.extend(plan.effects.iter().map(|effect| effect.verb.word()));
        words.extend(
            plan.obligations
                .iter()
                .map(|obligation| obligation.kind.word()),
        );
        decision["retrieval"]["by_ops"] = project(super::retrieve::retrieve_by_ops(&words, 10));
    }
    out.provenance.decision = Some(decision);
}

pub fn record_route(out: &mut CompileOutcome, route: &[String]) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["route"] = json!(route);
    out.provenance.decision = Some(decision);
}

/// The sha256 (lowercase hex) of an intent as the compiler reads it: typographic
/// apostrophes folded, nothing else changed. A transport keys a recorded plan by this
/// value so an answer round can find the plan its previous round produced; the compiler
/// records it in `provenance.decision.intent_sha256` on every general-path outcome.
#[must_use]
pub fn intent_sha256(intent: &str) -> String {
    super::surface::sha256(&lexicon::fold_apostrophes(intent))
}

/// The provenance projection of a settled plan: the plan itself plus the strategy that
/// settled it, so the record replays under the same name.
/// The plan projection with its strategy word and the obligation ledger the plan states
/// (every duty typed with its state), for provenance and for the answer-round replay.
#[must_use]
pub fn plan_record(plan: &Plan, strategy: Option<Strategy>) -> Value {
    let mut record = plan.to_json();
    if let Some(strategy) = strategy {
        record["strategy"] = json!(strategy.word());
    }
    record
}

/// Record the obligation ledger a plan states in the decision record (the assembler
/// overwrites it with the realized one when it emits): the plan record itself stays the
/// replayable identity of the plan, byte-identical across answer rounds.
pub fn record_ledger(out: &mut CompileOutcome, ledger: &super::ledger::Ledger) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["ledger"] = ledger.to_json();
    out.provenance.decision = Some(decision);
}

/// Replay a recorded plan for the same intent: straight to the deterministic assembler,
/// with zero reading, zero seat calls and zero provider calls. The record's own `strategy`
/// word is kept as the outcome's strategy; the route says `replayed plan`. A record that
/// does not parse, is not anchored in this intent or still carries unknown work is a
/// finding on `recorded_plan`, never a candidate.
///
/// # Errors
/// Returns representation failures while replaying an admitted record through assembly.
pub fn replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    if record.get("strategy").and_then(Value::as_str) == Some(Strategy::Native.word()) {
        native_replay(intent, record, request, out);
        return Ok(());
    }
    record_route(out, &["replayed plan".to_owned()]);
    record_retrieval(out, intent, None);
    let plan = match Plan::from_json(record) {
        Ok(plan) => plan,
        Err(why) => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                "recorded_plan",
                format!(
                    "The recorded plan cannot be replayed ({why}). Compile the intent again without it."
                ),
            );
            return Ok(());
        }
    };
    let strategy = record
        .get("strategy")
        .and_then(Value::as_str)
        .and_then(Strategy::parse);
    if !plan.anchored(intent) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded plan is not anchored in this request: an operation, effect or obligation names an excerpt the request does not contain. Compile the intent again without it.",
        );
        return Ok(());
    }
    if !plan.unknowns.is_empty() {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded plan still carries unresolved requested work; no substitute workflow was emitted.",
        );
        for unknown in &plan.unknowns {
            super::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
        }
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        record_ledger(out, &super::ledger::Ledger::extract(&plan));
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    // A record from an earlier engine may still carry a numeric rule as guidance.
    let mut plan = plan;
    super::shape::promote_stated_rules(&mut plan, intent);
    // A seat's plan (or a record with no strategy word) that works on nothing is asked,
    // never assembled; the reader's own HOT plan was already judged explicit.
    if strategy != Some(Strategy::Hot) && super::assemble::unfed(&plan, intent, out) {
        out.provenance.plan = Some(plan_record(&plan, strategy));
        return Ok(());
    }
    super::assemble::assemble(&plan, intent, request, out)?;
    record_retrieval(out, intent, Some(&plan));
    out.provenance.strategy = strategy;
    out.provenance.plan = Some(plan_record(&plan, strategy));
    Ok(())
}

/// The deterministic-only door: HOT under the request's contract, or an honest report.
pub(crate) fn hot(
    intent: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) -> Result<bool, CompileError> {
    let folded = lexicon::fold_apostrophes(intent);
    let intent = folded.as_str();
    let mut reading = lexicon::read(intent);
    backstop(intent, &mut reading.plan);
    // The deterministic door judges the reading with its stated rules promoted: a rule
    // carries its own constraint, and the words inside it are its literals.
    let mut admitted = reading.clone();
    super::shape::promote_stated_rules(&mut admitted.plan, intent);
    match admit_hot(intent, &admitted, request.hot) {
        Ok(()) => {
            record_route(out, &["hot".to_owned()]);
            super::assemble::assemble(&admitted.plan, intent, request, out)?;
            record_retrieval(out, intent, Some(&admitted.plan));
            out.provenance.strategy = Some(Strategy::Hot);
            out.provenance.plan = Some(plan_record(&admitted.plan, Some(Strategy::Hot)));
            Ok(true)
        }
        Err(why) => {
            if reading.plan.steps.is_empty()
                && reading.plan.effects.is_empty()
                && reading.ambiguous.is_empty()
                && reading.unresolved.len() <= 1
                && reading.clauses <= 1
            {
                // Nothing recognizable: keep the historical message of the exact-skeleton door.
                return Ok(false);
            }
            record_route(
                out,
                &[
                    format!("hot rejected: {}", why.join("; ")),
                    "needs cognition".to_owned(),
                ],
            );
            record_retrieval(out, intent, None);
            unresolved(&reading, out);
            if reading.unresolved.is_empty() && reading.ambiguous.is_empty() {
                super::finding(
                    out,
                    DiagnosticKind::Unknown,
                    "intent",
                    format!(
                        "The deterministic reader cannot admit this request on its own ({}). Permit an authoring model (`--authoring-model`) or a decision seat, or rephrase with explicit operations and literals.",
                        why.join("; ")
                    ),
                );
            }
            Ok(true)
        }
    }
}

pub fn unresolved(reading: &Reading, out: &mut CompileOutcome) {
    for clause in &reading.unresolved {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            format!(
                "Unresolved clause: {clause}. No requested operation was dropped; no substitute workflow was selected."
            ),
        );
    }
    for ambiguity in &reading.ambiguous {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            format!(
                "Ambiguous clause: {} (could be {}). A bounded decision seat or an explicit rephrase settles it; no substitute workflow was selected.",
                ambiguity.clause,
                ambiguity
                    .options
                    .iter()
                    .map(|op| op.word())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        );
    }
    for unknown in &reading.plan.unknowns {
        super::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
    }
    if !out
        .questions
        .iter()
        .any(|q| q.key == "intent.clarification")
    {
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
    }
    // The reading's own ledger: the plan's duties plus every clause the reader could not
    // settle, so the unresolved work is typed beside the plan.
    record_ledger(out, &super::ledger::Ledger::extract_reading(reading));
    out.provenance.plan = Some(reading.plan.to_json());
}

/// The trigger the request states is recorded beside the candidate, never in it: the same
/// reading the assembler makes (a cadence, a time of day, an event), bound to the schedule
/// answers when the human gives them.
fn record_trigger(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let Some(phrase) = record["trigger"].as_str().filter(|p| !p.trim().is_empty()) else {
        return;
    };
    if matches!(
        crate::trigger::classify(phrase),
        crate::trigger::TriggerForm::Sequence
    ) {
        return;
    }
    let mut plan = crate::plan::Plan::default();
    plan.trigger = Some(phrase.to_owned());
    if let Some(mut trigger) = crate::trigger::requirement(&plan, false) {
        let mut recognized = BTreeSet::new();
        crate::trigger::bind_schedule(&mut trigger, request, out, &mut recognized);
        super::finding(
            out,
            DiagnosticKind::Applied,
            "trigger",
            crate::trigger::note(&trigger),
        );
        out.requested_trigger = Some(trigger);
    }
}

/// Replay a native record: the same candidate with this round's answers, zero calls.
pub(crate) fn native_replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) {
    record_route(out, &["replayed native candidate".to_owned()]);
    if record["intent_sha256"].as_str() != Some(intent_sha256(intent).as_str()) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded native candidate was written for another request; compile the intent again without it.",
        );
        return;
    }
    out.provenance.strategy = Some(Strategy::Native);
    out.provenance.plan = Some(record.clone());
    native_apply(record, request, out);
}

/// Bake the answers into the recorded source and finish it: every unanswered question stays
/// mandatory and no candidate is emitted until all are answered; the `model` placeholder takes
/// the human's model.
/// Apply a native record to the request: the answers are baked into the recorded source,
/// the gaps disposed of, the model seated, and the candidate finished — or its questions
/// stay open. The seats' doors call it once a candidate is accepted; the replay calls it on
/// every answer round.
pub fn native_apply(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let mut source = record["source"].as_str().unwrap_or_default().to_owned();
    let mut open = false;
    record_trigger(record, request, out);
    for question in record["questions"].as_array().into_iter().flatten() {
        let key = question["key"].as_str().unwrap_or_default();
        if let Some(literal) = request.answers.get(key) {
            if !bake(&mut source, question, literal, out) {
                open = true;
            }
        } else {
            open = true;
            ask(question, out);
        }
    }
    if !open {
        grant_answered_paths(
            record["source"].as_str().unwrap_or_default(),
            &mut source,
            out,
        );
    }
    // An unused envelope model is not a runtime requirement. Ask only when Check
    // proves language work remains, including parametric fan-out calls.
    let language_work = crate::parse(&source)
        .is_ok_and(|wf| !nika_check::check(&wf).certificate.llm_calls.is_zero());
    if language_work
        && source.contains("model: mock/echo")
        && !seat_model(&mut source, request, out)
    {
        open = true;
    }
    dispose_gaps(record, request, out);
    if open {
        // Questions stay; the candidate waits for them (the same contract as the assembler).
        return;
    }
    super::finish(source, out);
}

/// The clauses the seat could not realize never vanish: each is a `Missed` diagnostic on every
/// transport (the clause verbatim, the candidate does not carry it) and an optional question
/// `gap.<n>` the human answers — « drop » or how it should be done — recorded as the human's
/// disposition in the decision record, never baked into the candidate (MP §3.1: nothing
/// important silently disappears into READY).
fn dispose_gaps(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let gaps: Vec<&str> = record["gaps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if gaps.is_empty() {
        return;
    }
    let mut dispositions = Vec::new();
    for (n, clause) in gaps.iter().enumerate() {
        let key = format!("gap.{}", n + 1);
        if let Some(answer) = request.answers.get(&key) {
            dispositions.push(json!({"clause": clause, "disposition": answer}));
            continue;
        }
        super::finding(
            out,
            DiagnosticKind::Missed,
            "gap",
            format!(
                "the seat could not realize « {clause} »; the candidate does not carry it — answer `{key}` with \"drop\" to accept that, or say in words how it should be done"
            ),
        );
        out.questions.push(super::CompileQuestion {
            key,
            label: format!(
                "« {clause} » is not in the candidate: drop it, or say how it should be done"
            ),
            answer_type: QuestionType::Text,
            why: "A clause the request states never disappears silently; the human disposes of it."
                .to_owned(),
            mandatory: false,
            options: Vec::new(),
        });
    }
    if !dispositions.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["gap_dispositions"] = json!(dispositions);
        out.provenance.decision = Some(decision);
    }
}

/// Bake one answered question into the source's `const:`; false when it could not be.
fn bake(source: &mut String, question: &Value, literal: &str, out: &mut CompileOutcome) -> bool {
    let key = question["key"].as_str().unwrap_or_default();
    let slug = key.trim_start_matches("const.");
    let Some(value) = super::literal_answer(Some(literal), key, out) else {
        return false;
    };
    let value = if question["answer_type"] == "literal" {
        value
    } else {
        match value {
            Value::String(s) => Value::String(s),
            other => Value::String(other.to_string()),
        }
    };
    let baked = crate::edit::literal_projection(source.as_str()).and_then(|before| {
        let mut after = before.clone();
        after["const"][slug] = value.clone();
        let edited = crate::edit_source::emit(source.as_str(), &before, &after, slug)?;
        // An answered endpoint also grants its host: the boundary is the compiler's to
        // complete from the answer, never the seat's to guess.
        if !grant_host(&mut after, &value) {
            return Some(edited);
        }
        let before = crate::edit::literal_projection(&edited)?;
        crate::edit_source::emit_at(&edited, &before, &after, &["permits", "net", "http"])
    });
    if let Some(edited) = baked {
        *source = edited;
        super::finding(
            out,
            DiagnosticKind::Applied,
            key,
            "Answer applied to the candidate's const.",
        );
        true
    } else {
        super::finding(
            out,
            DiagnosticKind::Missed,
            key,
            "The answer could not be baked into the candidate's const; it was not applied.",
        );
        false
    }
}

/// Add the host of an answered URL to `permits.net.http` when that list exists and lacks it.
fn grant_host(after: &mut Value, value: &Value) -> bool {
    let Some(host) = value.as_str().and_then(host_of) else {
        return false;
    };
    let Some(list) = after
        .pointer_mut("/permits/net/http")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if list.iter().any(|h| h.as_str() == Some(host)) {
        return false;
    }
    list.push(Value::String(host.to_owned()));
    true
}

/// Complete the read or write boundary a seat left as its empty placeholder (`[""]`, the
/// one narrow shape the judge admits while a path is still asked) with the exact paths the
/// answers introduced, as `grant_host` completes an answered endpoint. The paths are the
/// capability inference's own (`nika check --infer-permits`), taken over the seat's source
/// and over the answered one: only their difference, in the direction the tool uses, bound
/// to a bare `${{ const.<slug> }}` (the inference resolves nothing else). A path that
/// escapes the workspace is never inferred; a glob, or a direction the seat declared with
/// any other entry, is never touched — the check then refuses the candidate, as before.
fn grant_answered_paths(seat_source: &str, source: &mut String, out: &mut CompileOutcome) {
    let (Ok(seat), Ok(answered)) = (crate::parse(seat_source), crate::parse(source)) else {
        return;
    };
    let (seat, answered) = (
        nika_check::infer_permits(&seat),
        nika_check::infer_permits(&answered),
    );
    for direction in ["read", "write"] {
        let introduced: Vec<String> = inferred_paths(&answered, direction)
            .difference(&inferred_paths(&seat, direction))
            .cloned()
            .collect();
        if introduced.is_empty()
            || introduced
                .iter()
                .any(|path| path.is_empty() || path.contains(['*', '?', '[']))
        {
            continue;
        }
        let Some(before) = crate::edit::literal_projection(source) else {
            return;
        };
        let placeholder = before
            .pointer(&format!("/permits/fs/{direction}"))
            .and_then(Value::as_array)
            .is_some_and(|list| matches!(list.as_slice(), [only] if only.as_str() == Some("")));
        if !placeholder {
            continue;
        }
        let mut after = before.clone();
        after["permits"]["fs"][direction] = json!(introduced);
        if let Some(edited) =
            crate::edit_source::emit_at(source, &before, &after, &["permits", "fs", direction])
        {
            *source = edited;
            super::finding(
                out,
                DiagnosticKind::Applied,
                &format!("permits.fs.{direction}"),
                format!(
                    "The answered path completes the empty placeholder the candidate declared: {}.",
                    introduced.join(" · ")
                ),
            );
        }
    }
}

/// The `permits.fs` entries one inference derived for a direction.
fn inferred_paths(inferred: &nika_check::InferredPermits, direction: &str) -> BTreeSet<String> {
    inferred
        .permits
        .fs
        .as_ref()
        .map(|fs| {
            if direction == "write" {
                &fs.write
            } else {
                &fs.read
            }
        })
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}

/// The host of an `http(s)://` URL, without its port: the form `permits.net.http` lists (the
/// assembler grants `url.host_str()`; the loopback declassification compares exact hosts).
fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map_or(authority, |close| &authority[..=close])
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    (!host.is_empty()).then_some(host)
}

/// Ask one recorded business question, mandatory.
fn ask(question: &Value, out: &mut CompileOutcome) {
    out.questions.push(super::CompileQuestion {
        key: question["key"].as_str().unwrap_or_default().to_owned(),
        label: question["label"].as_str().unwrap_or_default().to_owned(),
        answer_type: if question["answer_type"] == "literal" {
            QuestionType::Literal
        } else {
            QuestionType::Text
        },
        why: question["why"]
            .as_str()
            .filter(|w| !w.is_empty())
            .unwrap_or("The compiler cannot invent this business value.")
            .to_owned(),
        mandatory: true,
        options: Vec::new(),
    });
}

/// The model placeholder: the human's `model` answer replaces it; else the question is asked.
fn seat_model(source: &mut String, request: &CompileRequest, out: &mut CompileOutcome) -> bool {
    let Some(literal) = request.answers.get("model") else {
        super::question(
            out,
            "model",
            "Which model runs the language work of this workflow? Answer a `provider/name` string.",
            QuestionType::Text,
        );
        return false;
    };
    match super::literal_answer(Some(literal), "model", out) {
        Some(Value::String(model)) if model.contains('/') => {
            *source = source.replacen("model: mock/echo", &format!("model: {model}"), 1);
            true
        }
        _ => {
            super::finding(
                out,
                DiagnosticKind::Missed,
                "model",
                "Answer must be a `provider/name` string.",
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answered_url_grants_its_host_without_its_port() {
        assert_eq!(
            host_of("https://hooks.example.invalid/recap"),
            Some("hooks.example.invalid")
        );
        assert_eq!(host_of("http://127.0.0.1:8793/hook"), Some("127.0.0.1"));
        assert_eq!(host_of("http://[::1]:8080/x"), Some("[::1]"));
        assert_eq!(host_of("./out/report.md"), None);
        let mut doc = json!({"permits": {"net": {"http": []}}});
        assert!(grant_host(
            &mut doc,
            &json!("https://hooks.example.invalid/recap")
        ));
        assert!(!grant_host(
            &mut doc,
            &json!("https://hooks.example.invalid/again")
        ));
        assert_eq!(
            doc["permits"]["net"]["http"],
            json!(["hooks.example.invalid"])
        );
        let mut none = json!({"permits": {}});
        assert!(!grant_host(&mut none, &json!("https://x.invalid/")));
    }
}
