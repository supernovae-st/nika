// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/details` — how the last workflow was built, on demand: the authoring
//! backend and model, the calls, tokens and time, the strategy, the
//! decision seat, the knowledge the seat read (the pinned snapshot, the
//! pack's digest, every reference, the instruction digest of every call
//! that carried it), the engine and spec identity. Read from the compiler's
//! own provenance and the session's record beside it, never invented;
//! advanced, never in the ordinary conversation (the third level of
//! disclosure).

use std::fmt::Write as _;

use serde_json::Value;

use super::SessionRuntime;

/// The first twelve characters of a digest a record states.
fn short(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| "none".to_owned(), |s| s.chars().take(12).collect())
}

/// The knowledge lines of the session's record (`decision.session.authoring`):
/// the strategy the policy carried, then the pack — presented to the seat
/// (with the calls that carried it), attached but never read (and why), or
/// carried from the round that authored a replayed candidate.
fn knowledge_lines(record: &Value, text: &mut String) {
    let _ = write!(
        text,
        "\n  authoring strategy: {} ({})",
        record["strategy"].as_str().unwrap_or("unknown"),
        record["source"].as_str().unwrap_or("unknown")
    );
    let knowledge = &record["knowledge"];
    if knowledge.is_null() {
        text.push_str("\n  knowledge: none attached");
        return;
    }
    let identity = &knowledge["identity"];
    let references = knowledge["references"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let bytes: u64 = references.iter().filter_map(|r| r["bytes"].as_u64()).sum();
    // The pack's and the calls' digests are printed whole: they are what an
    // auditor compares to the bytes a seat received.
    // The digest is the manifest's own claim; the manifest and rows digests are computed.
    let _ = write!(
        text,
        "\n  knowledge: {} · declared digest {} · manifest {} · rows {} · {} reference{} · {bytes} B · {}\n  pack sha256 {}",
        identity["version"].as_str().unwrap_or("unversioned"),
        short(&identity["digest"]),
        short(&identity["manifest_sha256"]),
        short(&identity["rows_sha256"]),
        references.len(),
        if references.len() == 1 { "" } else { "s" },
        knowledge["pack_builder"]
            .as_str()
            .unwrap_or("unknown builder"),
        knowledge["pack_sha256"].as_str().unwrap_or("none")
    );
    for reference in references {
        let _ = write!(
            text,
            "\n    {} {} · {} B · sha256 {}",
            reference["kind"].as_str().unwrap_or("?"),
            reference["id"].as_str().unwrap_or("?"),
            reference["bytes"].as_u64().unwrap_or(0),
            short(&reference["sha256"])
        );
    }
    let carried = knowledge["carried"].as_bool().unwrap_or(false);
    if knowledge["presented"].as_bool().unwrap_or(false) {
        let calls = knowledge["calls"].as_array().map_or(&[][..], Vec::as_slice);
        let _ = write!(
            text,
            "\n  presented to the seat in {} call{}{}",
            calls.len(),
            if calls.len() == 1 { "" } else { "s" },
            if carried {
                " of the round that authored this candidate (this answer round replayed it · zero calls)"
            } else {
                ""
            }
        );
        for call in calls {
            let _ = write!(
                text,
                "\n    {} · instruction sha256 {}",
                call["call"].as_str().unwrap_or("?"),
                call["instruction_sha256"].as_str().unwrap_or("none")
            );
        }
        seat_line(&knowledge["seat"], text);
    } else {
        let _ = write!(
            text,
            "\n  not presented: {}",
            knowledge["why"]
                .as_str()
                .unwrap_or("the native door did not read it")
        );
    }
}

/// What authored with the pack, in brief (kept with the knowledge record, so a replayed candidate
/// still names it): the model, where its calls went, how many, the usage the provider reported.
fn seat_line(seat: &Value, text: &mut String) {
    if !seat.is_object() {
        return;
    }
    if seat["backend"]["kind"] == "harness_infer" {
        let _ = write!(
            text,
            "\n    by subscription {} · {} call(s) · responding identities in backend receipt · cost unknown",
            seat["backend"]["adapter"].as_str().unwrap_or("unknown"),
            seat["calls"]
        );
        return;
    }
    let usage = match (
        seat["input_tokens"].as_u64(),
        seat["output_tokens"].as_u64(),
    ) {
        (Some(i), Some(o)) => format!("{i} in / {o} out tokens"),
        _ => "usage not reported by the provider".to_owned(),
    };
    let calls = seat["calls"].as_u64().unwrap_or(0);
    let _ = write!(
        text,
        "\n    by {} · host {} · {calls} call{} in that round · {usage} · {} ms",
        seat["model"].as_str().unwrap_or("unknown model"),
        seat["backend"]["host"].as_str().unwrap_or("unknown"),
        if calls == 1 { "" } else { "s" },
        seat["elapsed_ms"].as_u64().unwrap_or(0)
    );
}

/// The authoring receipt's lines: the model, the calls, tokens and time, where the calls really
/// went (the provider's own API, or the gateway its base URL is overridden to), the cost basis.
pub(super) fn receipt_lines(receipt: &nika_onboard::compile::AuthoringReceipt, text: &mut String) {
    if let Some(backend) = receipt
        .backend
        .as_ref()
        .filter(|b| b["kind"] == "harness_infer")
    {
        let _ = write!(
            text,
            "\n  authoring backend: subscription {} · requested {} · {} compiler calls · {} ms",
            backend["adapter"].as_str().unwrap_or("unknown"),
            backend["requested_model"]
                .as_str()
                .unwrap_or("harness default"),
            receipt.calls,
            receipt.elapsed_ms
        );
        if backend["carried_from_authoring_round"] == true {
            text.push_str("\n    receipt carried from the authoring round; this clarification replay made zero calls");
        }
        if let Some(calls) = backend["observed"].as_array() {
            for call in calls.iter().filter(|c| c["status"] == "returned") {
                let _ = write!(
                    text,
                    "\n    responding model: {} · usage marker {}",
                    call["observed_model"].as_str().unwrap_or("not reported"),
                    call["usage_observed"].as_bool().unwrap_or(false)
                );
            }
        }
        text.push_str("\n  cost: subscription invoice unknown · no numeric token meter reported · no paid provider fallback");
        return;
    }

    let _ = write!(
        text,
        "\n  authoring backend: {} · {} call{} · {} ms",
        receipt.model,
        receipt.calls,
        if receipt.calls == 1 { "" } else { "s" },
        receipt.elapsed_ms
    );
    if let (Some(i), Some(o)) = (receipt.input_tokens, receipt.output_tokens) {
        let _ = write!(text, " · {i} in / {o} out tokens");
    }
    if let Some(backend) = &receipt.backend {
        let _ = write!(
            text,
            "\n  sent to: {} · host {}{}",
            backend["provider"].as_str().unwrap_or("unknown provider"),
            backend["host"].as_str().unwrap_or("unknown"),
            if backend["base_url_overridden"].as_bool() == Some(true) {
                " (base URL overridden: a gateway or a local server, not the provider's own API)"
            } else {
                ""
            }
        );
    }
    text.push_str(
        "\n  cost: the compiler meters tokens, not money · a run's cost is in its result and `/proof`",
    );
}

impl SessionRuntime {
    /// The details card for the last compiler reading of this session.
    #[must_use]
    pub fn details(&self) -> String {
        let mut text = "Details · how the last workflow was built (advanced)".to_owned();
        let _ = write!(text, "\n  {}", self.intelligence_line());
        let _ = write!(text, "\n  {}", self.seat.line());
        let _ = write!(text, "\n  {}", self.authoring_context.line());
        let Some(out) = &self.last_outcome else {
            text.push_str(
                "\n  no workflow was read in this session yet · describe work to build and come back",
            );
            return text;
        };
        let prov = &out.provenance;
        let _ = write!(text, "\n  reading: {:?}", prov.cognition);
        match &prov.authoring {
            Some(receipt) => receipt_lines(receipt, &mut text),
            None => text
                .push_str("\n  authoring backend: none (no model call: the deterministic reading)"),
        }
        if let Some(strategy) = &prov.strategy {
            let _ = write!(text, "\n  strategy: {strategy:?}");
        }
        if let Some(skeleton) = &prov.skeleton {
            let _ = write!(text, "\n  skeleton: {skeleton}");
        }
        if let Some(decision) = &prov.decision {
            // The compiler records its route as the list of doors it tried.
            let route = match decision.get("route") {
                Some(Value::String(route)) => route.clone(),
                Some(Value::Array(steps)) => steps
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" → "),
                _ => "none recorded".to_owned(),
            };
            let seat = decision
                .get("seat")
                .and_then(|s| s.get("model"))
                .and_then(|m| m.as_str());
            let _ = write!(text, "\n  decision: route {route}");
            if let Some(seat) = seat {
                let _ = write!(text, " · seat {seat}");
            }
            let ledger = decision
                .get("ledger")
                .and_then(|l| l.as_array())
                .map_or(0, Vec::len);
            if ledger > 0 {
                let _ = write!(
                    text,
                    " · ledger {ledger} clause{}",
                    if ledger == 1 { "" } else { "s" }
                );
            }
            if let Some(record) = decision.pointer("/session/authoring") {
                knowledge_lines(record, &mut text);
            }
        }
        let _ = write!(
            text,
            "\n  engine: compiler {} · spec {}",
            prov.compiler_version,
            prov.spec_pin.chars().take(12).collect::<String>()
        );
        if let Some(trace) = &self.last_trace {
            let _ = write!(
                text,
                "\n  last run: trace `{}` (`/proof` judges it)",
                trace.display()
            );
        }
        if !self.routes.is_empty() {
            let _ = write!(
                text,
                "\n  routes: {} open line(s) routed this session (phase · act · how · the line's hash) · last:",
                self.routes.len()
            );
            for record in self.routes.iter().rev().take(5) {
                let _ = write!(text, "\n    {}", record.line());
            }
        }
        text.push_str(
            "\n  none of this is a proof: `/proof` judges a run's trace · `/meaning` shows what was kept of your request",
        );
        text
    }
}
