// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect-safe retry law (#1371 · `NIKA-SEC-016`) — a declared `retry:`
//! on a `nika:fetch` whose method replays side effects (POST · PUT ·
//! DELETE · PATCH) WITHOUT an `idempotency-key` header is a FINDING at
//! check, never a green audit that double-charges at run.
//!
//! The predicate is [`nika_types::net::retry_is_effect_safe`] — the ONE the
//! fetch builtin's transient classification judges with, so check ≡ run by
//! construction (the `exec_floor` precedent: an L0 leaf both sides depend
//! on, never a hand-mirrored table). The run side of the law: a keyless
//! effect-capable call types EVERY failure `transient: false` — one
//! attempt, never a blind replay of an ambiguous effect (the server may
//! have committed before the socket dropped or the 500 was emitted). A
//! plain `retry:` on such a call is therefore DEAD config, and an
//! `on_codes:` retry is worse — it fires regardless of `transient`, a
//! blind replay the file must never author. Both shapes are refused here
//! wholesale, with the repair that discharges the hazard: pair the key, or
//! drop the `retry:`.
//!
//! SCOPE (sound, never a false red — the `consent.rs` discipline):
//!
//! - **literal method only** — a `${{ }}` method is not in the closed
//!   effect-replaying set, so it makes no static claim (the runtime
//!   classification re-judges the resolved call; the belt holds there).
//! - **literal header keys only** — a `headers:` that is not an object,
//!   or whose keys carry a `${{ }}` island, makes the key's ABSENCE
//!   undecidable: no claim. A literal `idempotency-key` (any case ·
//!   RFC 9110 §5.1) discharges the hazard — the receiver dedups the
//!   replay.
//!
//! Born as the fetch arm of the advisory `retry-effects` hint (P0-17);
//! promoted here per the write-conflict precedent (F-P15: an error owns
//! its repair, never a hint). The `exec:` · `mcp:*` · `nika:notify` arms
//! stay advisory — their effect contract is genuinely unknowable, not
//! provably keyless.

use nika_schema::raw::{RawAction, RawWorkflow};

/// The wire code of an effect-safe-retry refusal (spec 05-errors).
pub(crate) const RETRY_SAFETY_CODE: &str = "NIKA-SEC-016";

/// One effect-safe-retry finding — the check-time twin of the runtime's
/// non-transient classification of the same call.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct RetrySafetyFinding {
    /// The offending task.
    pub task: String,
    /// The witness sentence (the method + the law it breaks).
    pub detail: String,
    /// The repair (pair the `idempotency-key` header · or drop the `retry:`).
    pub fix: String,
}

impl RetrySafetyFinding {
    /// The canonical spec code this finding stamps.
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        RETRY_SAFETY_CODE
    }
}

/// Scan every task for a declared `retry:` (`max_attempts > 1`) on a
/// provably keyless, effect-replaying `nika:fetch` — one finding per task.
/// DAG-independent (a per-task syntactic judgment): runs even when
/// conformance fails, like the exec-floor and permits-fit lanes.
pub(crate) fn scan(wf: &RawWorkflow) -> Vec<RetrySafetyFinding> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let t = &task.value;
        let retries = t.retry.as_ref().is_some_and(|r| r.value.max_attempts > 1);
        if !retries {
            continue;
        }
        push_retry_safety(t, &mut out);
    }
    out
}

fn push_retry_safety(t: &nika_schema::raw::RawTask, out: &mut Vec<RetrySafetyFinding>) {
    let RawAction::Invoke(a) = &t.action else {
        return;
    };
    let Some(tool) = a.tool().map(|t| t.value.as_str()) else {
        return; // a `workflow:` child call — the child's own check owns its effects
    };
    if tool != "nika:fetch" {
        return;
    }
    let args = a.args.as_ref().map(|s| &s.value);
    let Some(method) = args
        .and_then(|v| v.get("method"))
        .and_then(serde_json::Value::as_str)
    else {
        return; // absent method = the GET default — replays nothing
    };
    if !nika_types::net::method_replays_effects(method) {
        return; // GET · HEAD retry free · an unrecognized method is the shape ladder's finding
    }
    // A templated method never reaches here (it is not in the closed set —
    // no static claim; the runtime classification re-judges it resolved).
    //
    // The keyless judgment (sound, never a false red): NO `headers:` at
    // all is provably keyless; a literal `idempotency-key` key (any case)
    // discharges the hazard; a non-object `headers:` or a templated key
    // makes the key's ABSENCE undecidable — silence, the runtime
    // classification owns the resolved call.
    let keyless = match args.and_then(|v| v.get("headers")) {
        None => true,
        Some(serde_json::Value::Object(map)) => {
            if map.keys().any(|k| k.contains("${{")) {
                return; // a templated key hides whether the contract rides — no claim
            }
            !map.keys()
                .any(|k| nika_types::net::is_idempotency_key_header(k))
        }
        Some(_) => return, // a whole-headers template is undecidable — no claim
    };
    if !keyless {
        return; // the declared key lets the receiver dedup the replay
    }
    let id = t.id.value.as_str();
    let upper = method.to_ascii_uppercase();
    out.push(RetrySafetyFinding {
        task: id.to_owned(),
        detail: format!(
            "`{id}` declares `retry:` on `nika:fetch` with method {upper} and no \
             `idempotency-key` header — a retry replays the request's side effects \
             at-least-once: the failure may be ambiguous (the server may have committed \
             before the socket dropped or the error was emitted), so the engine types \
             the call's failures non-transient and an `on_codes:` retry would \
             blind-replay the ambiguous effect"
        ),
        fix: String::from(
            "pair an `idempotency-key` header (the receiver dedups the replay) or drop \
             the `retry:` — GET/HEAD retry free",
        ),
    });
}

#[cfg(test)]
mod tests {
    use super::RETRY_SAFETY_CODE;
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// One fetch task under a fitting boundary, so the ONLY dirtiness a
    /// case can carry is this lane's finding.
    fn fetch_wf(task_yaml: &str) -> String {
        format!(
            "nika: w\npermits:\n  net: {{ http: [\"api.example.com\"] }}\n  tools: [\"nika:fetch\"]\ntasks:\n  pay:\n{task_yaml}"
        )
    }

    /// THE issue's repro (#1371): `retry×3` on a keyless `nika:fetch` POST
    /// checked GREEN and triple-charged at run (3 calls · 3 charges the run
    /// never admitted). The check now refuses — a finding, not a hint.
    #[test]
    fn the_issues_repro_is_a_sec016_finding() {
        let r = report(&fetch_wf(
            "    retry: { max_attempts: 3 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/charge\", method: POST, body: { a: 1 } }\n",
        ));
        assert!(
            !r.is_clean(),
            "the repro must not check clean: {:#?}",
            r.findings
        );
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "retry_safety")
            .expect("the retry-safety row in findings[]");
        assert_eq!(hit.code.as_deref(), Some("NIKA-SEC-016"));
        assert_eq!(
            hit.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-016")
        );
        assert_eq!(hit.task.as_deref(), Some("pay"));
        assert!(hit.message.contains("POST"), "{}", hit.message);
        assert!(
            hit.message.contains("idempotency-key"),
            "the teaching names the key: {}",
            hit.message
        );
    }

    /// Every effect-replaying method is refused — the law is the method
    /// table, not the issue's one POST.
    #[test]
    fn every_mutating_method_is_refused() {
        for method in ["POST", "PUT", "DELETE", "PATCH"] {
            let r = report(&fetch_wf(&format!(
                "    retry: {{ max_attempts: 2 }}\n    invoke:\n      tool: nika:fetch\n      args: {{ url: \"https://api.example.com/x\", method: {method} }}\n"
            )));
            assert_eq!(
                r.retry_safety_findings.len(),
                1,
                "{method} without the key must earn the finding: {:#?}",
                r.findings
            );
            assert_eq!(r.retry_safety_findings[0].wire_code(), "NIKA-SEC-016");
            assert!(
                r.retry_safety_findings[0].detail.contains(method),
                "the witness names the method: {}",
                r.retry_safety_findings[0].detail
            );
        }
    }

    /// The declared `idempotency-key` header discharges the hazard — the
    /// receiver dedups the replay, retry keeps working, the file checks
    /// clean (and the old advisory hint is gone with the promotion).
    #[test]
    fn the_declared_key_discharges_the_hazard() {
        for header in ["idempotency-key", "Idempotency-Key"] {
            let r = report(&fetch_wf(&format!(
                "    retry: {{ max_attempts: 2 }}\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://api.example.com/charge\"\n        method: POST\n        headers: {{ {header}: \"order-7\" }}\n        body: {{ a: 1 }}\n"
            )));
            assert!(
                r.retry_safety_findings.is_empty(),
                "{header} discharges the hazard: {:#?}",
                r.retry_safety_findings
            );
            assert!(
                r.is_clean(),
                "{header} + retry is the legal shape: {:#?}",
                r.findings
            );
        }
    }

    /// Read-only retry behavior is UNCHANGED: explicit GET, the GET
    /// default (no `method:`), and HEAD never earn the finding.
    #[test]
    fn get_and_head_retry_make_no_claim() {
        for args in [
            "{ url: \"https://api.example.com/x\", method: GET }",
            "{ url: \"https://api.example.com/x\" }",
            "{ url: \"https://api.example.com/x\", method: HEAD }",
        ] {
            let r = report(&fetch_wf(&format!(
                "    retry: {{ max_attempts: 3 }}\n    invoke:\n      tool: nika:fetch\n      args: {args}\n"
            )));
            assert!(
                r.retry_safety_findings.is_empty(),
                "{args} retries free: {:#?}",
                r.retry_safety_findings
            );
            assert!(r.is_clean(), "{args} stays clean: {:#?}", r.findings);
        }
    }

    /// `max_attempts: 1` is no retry at all — no claim.
    #[test]
    fn a_single_attempt_is_no_retry() {
        let r = report(&fetch_wf(
            "    retry: { max_attempts: 1 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/charge\", method: POST }\n",
        ));
        assert!(r.retry_safety_findings.is_empty());
        assert!(r.is_clean(), "{:#?}", r.findings);
    }

    /// `on_codes:` does NOT buy off the law: it retries regardless of
    /// `transient`, so on a keyless mutating call it is the one shape
    /// that would still blind-replay at run — refused wholesale.
    #[test]
    fn on_codes_does_not_buy_off_the_law() {
        let r = report(&fetch_wf(
            "    retry: { max_attempts: 3, on_codes: [NIKA-BUILTIN-FETCH-001] }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/charge\", method: POST }\n",
        ));
        assert_eq!(
            r.retry_safety_findings.len(),
            1,
            "the on_codes blind replay is refused: {:#?}",
            r.findings
        );
    }

    /// A templated method makes NO claim — it is not in the closed
    /// effect-replaying set statically; the runtime classification
    /// re-judges the resolved call (the belt holds there).
    #[test]
    fn a_templated_method_makes_no_claim() {
        let r = report(&fetch_wf(
            "    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/charge\", method: \"${{ inputs.m }}\" }\n",
        ));
        assert!(
            r.retry_safety_findings.is_empty(),
            "no lie in the refusal direction: {:#?}",
            r.retry_safety_findings
        );
    }

    /// An undecidable `headers:` shape makes NO claim: a whole-headers
    /// template, or a templated header KEY, hides whether the key rides
    /// the call — silence, never a false red.
    #[test]
    fn an_undecidable_headers_shape_makes_no_claim() {
        for headers in ["\"${{ inputs.h }}\"", "{ \"${{ inputs.k }}\": \"v\" }"] {
            let r = report(&fetch_wf(&format!(
                "    retry: {{ max_attempts: 2 }}\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://api.example.com/charge\"\n        method: POST\n        headers: {headers}\n"
            )));
            assert!(
                r.retry_safety_findings.is_empty(),
                "headers {headers} is undecidable — no claim: {:#?}",
                r.retry_safety_findings
            );
        }
    }

    /// The other retry-effects classes stay OUT of this lane: `exec:`,
    /// `mcp:*`, `nika:notify` keep their advisory hint (their effect
    /// contract is genuinely unknowable, not provably keyless).
    #[test]
    fn other_retried_effects_stay_out_of_this_lane() {
        for yaml in [
            "nika: w\npermits:\n  exec: true\ntasks:\n  t:\n    retry: { max_attempts: 2 }\n    exec: { shell: \"./deploy.sh\" }\n",
            "nika: w\npermits:\n  tools: [\"mcp:slack/send\"]\ntasks:\n  t:\n    retry: { max_attempts: 2 }\n    invoke: { tool: \"mcp:slack/send\", args: { text: \"hi\" } }\n",
            "nika: w\npermits:\n  tools: [\"nika:notify\"]\n  net: { http: [\"hooks.example.com\"] }\ntasks:\n  t:\n    retry: { max_attempts: 2 }\n    invoke: { tool: \"nika:notify\", args: { target: \"https://hooks.example.com/x\", message: \"boom\" } }\n",
        ] {
            let r = report(yaml);
            assert!(
                r.retry_safety_findings.is_empty(),
                "no SEC-016 outside keyless-mutating-fetch: {:#?}",
                r.retry_safety_findings
            );
        }
    }

    /// The emitted⊆registered ratchet (the `exec_floor.rs` pattern): the
    /// wire code this lane stamps must exist in the vendored canon
    /// registry — an unregistered refusal 404s the `docs_url` every
    /// finding carries.
    #[test]
    fn the_retry_safety_code_is_registered_in_the_canon() {
        let registered: std::collections::BTreeSet<String> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code.to_string())
            .collect();
        assert!(
            registered.contains(RETRY_SAFETY_CODE),
            "`{RETRY_SAFETY_CODE}` is not in the canon registry (spec/05-errors.md SSOT)"
        );
    }
}
