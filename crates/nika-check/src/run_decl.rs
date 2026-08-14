// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `run:` declaration's body-level law (F-P3) — `entropy: none` is a
//! STRICT determinism demand: the run refuses to consume ANY structural
//! entropy source. The declaration lives in the envelope; the sources
//! live in the body, so the judgment is the checker's (the parser is
//! shape-only).
//!
//! The structural entropy sources of the language today (`rand::` has
//! zero production hits — verified):
//!
//! - **a `retry:` jitter** — the full-jitter backoff reads the run's
//!   jitter stream (`(seed, task, attempt)`). `jitter` DEFAULTS true
//!   (spec 05 · anti-thundering-herd), so a bare `retry:` block is an
//!   entropy claim already; only a LIVE jitter is judged (attempts > 1 ·
//!   non-zero backoff — a declared-but-dead jitter consumes no entropy).
//!   Under `entropy: none` the author promised NO entropy — declare
//!   `entropy: { seeded: N }` to name the stream, or `jitter: false`
//!   (the capped backoff alone is deterministic).
//! - **the `nika:uuid` builtin** — non-hermetic BY DESIGN (its own
//!   contract: v4 is pure entropy, v7 mixes wall time + 74 random bits —
//!   a replay cannot reproduce it). An `invoke:` task calling it, or an
//!   `agent:` whitelist naming it exactly, breaks the strict demand.
//!
//! Honest scope: an agent glob whitelist that ADMITS `nika:uuid` without
//! naming it (`nika:u*` · `*`) is the undecidable-glob class (the
//! NIKA-DRIFT-001 precedent — glob ⊆ glob stays silent, never a false
//! positive), and the replay-divergence half of the contract (a seeded
//! run that still diverges) is the fixture's, not a static judgment.
//!
//! The wire code is the dedicated `NIKA-PARSE-028` mint (NEP-0010 ·
//! registered by the 87f764a pack resync — this lane's generic
//! NIKA-PARSE-019 era ended when the spec-side follow-up landed).

use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::RunEntropy;

/// The wire code of a body-level run-declaration contradiction — the
/// dedicated `entropy: none` × structural-source mint (NEP-0010).
pub(crate) const RUN_DECL_CODE: &str = "NIKA-PARSE-028";

/// One `run:` declaration the workflow body contradicts (F-P3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct RunDeclFinding {
    /// The offending task.
    pub task: String,
    /// The entropy source the task uses (`retry` jitter · `nika:uuid`).
    pub source: &'static str,
    /// The witness sentence (the declaration × the source).
    pub detail: String,
    /// The two repairs (declare the stream · or drop the source).
    pub fix: String,
}

impl RunDeclFinding {
    /// The canonical spec code this finding stamps.
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        RUN_DECL_CODE
    }
}

/// Scan a workflow for body-level contradictions of `entropy: none`
/// (F-P3). Silent on every other declaration — `ambient` is the honest
/// default, `seeded(N)` names its stream, and an absent `run:` block
/// declares nothing (the status quo can never be a violation).
#[must_use]
pub(crate) fn scan_run_decl(wf: &RawWorkflow) -> Vec<RunDeclFinding> {
    let strict = wf
        .run
        .as_ref()
        .is_some_and(|run| run.value.entropy == Some(RunEntropy::None));
    if !strict {
        return Vec::new();
    }
    let mut out = Vec::new();
    for task in &wf.tasks {
        let task = &task.value;
        let id = task.id.value.as_str();
        // A LIVE jitter only: `jitter` defaults true (spec 05 · the
        // anti-thundering-herd default), but the stream is consulted only
        // when a retry can actually fire (attempts > 1) and the backoff
        // is non-zero — a declared-but-dead jitter consumes no entropy.
        let live_jitter = task.retry.as_ref().is_some_and(|retry| {
            let cfg = &retry.value;
            cfg.jitter && cfg.max_attempts > 1 && cfg.backoff_ms > 0
        });
        if live_jitter {
            out.push(RunDeclFinding {
                task: id.to_owned(),
                source: "retry jitter",
                detail: "`entropy: none` demands strict determinism — the retry's backoff reads \
                     the run's jitter stream, a structural entropy source (F-P3)"
                    .to_owned(),
                fix: "set `jitter: false` (the capped backoff alone is deterministic) · or \
                      declare `entropy: { seeded: N }` and name the stream"
                    .to_owned(),
            });
        }
        let uuid_call = match &task.action {
            RawAction::Invoke(a) => a.tool().is_some_and(|t| t.value == "nika:uuid"),
            RawAction::Agent(a) => a.tools.iter().any(|t| t.value == "nika:uuid"),
            _ => false,
        };
        if uuid_call {
            out.push(RunDeclFinding {
                task: id.to_owned(),
                source: "nika:uuid",
                detail: "`entropy: none` demands strict determinism — `nika:uuid` is non-hermetic \
                     BY DESIGN (v4 is pure entropy · v7 mixes wall time + 74 random bits · a \
                     replay cannot reproduce it)"
                    .to_owned(),
                fix: "drop the uuid call (derive the id from run-seeded data) · or \
                      declare `entropy: { seeded: N }` and accept the seeded lane"
                    .to_owned(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn findings_of(yaml: &str) -> Vec<RunDeclFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        scan_run_decl(&wf)
    }

    const HEAD: &str = "nika: w\npermits: { exec: [\"flaky\"] }\n";

    #[test]
    fn entropy_none_times_retry_jitter_is_a_finding() {
        let y = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  flaky:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 2, backoff_ms: 1000, jitter: true }}\n"
        );
        let f = findings_of(&y);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].wire_code(), "NIKA-PARSE-028");
        assert_eq!(f[0].task, "flaky");
        assert_eq!(f[0].source, "retry jitter");
        assert!(
            f[0].detail.contains("strict determinism"),
            "{}",
            f[0].detail
        );
        assert!(f[0].fix.contains("seeded"), "{}", f[0].fix);
    }

    #[test]
    fn entropy_none_times_uuid_is_a_finding_both_surfaces() {
        let invoke = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  mint:\n    invoke: {{ tool: \"nika:uuid\" }}\n"
        );
        let f = findings_of(&invoke);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].source, "nika:uuid");
        assert!(f[0].detail.contains("non-hermetic"), "{}", f[0].detail);

        let agent = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  loop:\n    agent:\n      prompt: \"x\"\n      tools: [\"nika:uuid\"]\n"
        );
        let f = findings_of(&agent);
        assert_eq!(f.len(), 1, "the exact whitelist entry is judged: {f:?}");
    }

    #[test]
    fn the_strict_lane_stays_silent_when_the_body_is_clean() {
        // `jitter: false` — the deterministic capped backoff.
        let y = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  plain:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 3, backoff_ms: 1000, jitter: false }}\n"
        );
        assert!(findings_of(&y).is_empty());
        // A declared-but-DEAD jitter consumes no entropy (the stream is
        // never consulted): single-attempt · or a zero backoff.
        let one_attempt = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  plain:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 1, backoff_ms: 1000, jitter: true }}\n"
        );
        assert!(
            findings_of(&one_attempt).is_empty(),
            "max_attempts: 1 never retries — the jitter is dead"
        );
        let zero_backoff = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  plain:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 3, backoff_ms: 0, jitter: true }}\n"
        );
        assert!(
            findings_of(&zero_backoff).is_empty(),
            "backoff_ms: 0 delays nothing — the jitter is dead"
        );
    }

    #[test]
    fn the_jitter_default_is_judged_live() {
        // `jitter` defaults TRUE (spec 05 · anti-thundering-herd) — a bare
        // retry: block under entropy: none IS an entropy claim.
        let y = format!(
            "{HEAD}run: {{ entropy: none }}\ntasks:\n  flaky:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 2, backoff_ms: 1000 }}\n"
        );
        let f = findings_of(&y);
        assert_eq!(f.len(), 1, "the default jitter is a live source: {f:?}");
        assert_eq!(f[0].source, "retry jitter");
    }

    #[test]
    fn other_declarations_and_the_absent_block_never_fire() {
        for run_block in [
            "run: { entropy: { seeded: 42 } }",
            "run: { entropy: ambient }",
        ] {
            let y = format!(
                "{HEAD}{run_block}\ntasks:\n  flaky:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 2, backoff_ms: 1000, jitter: true }}\n"
            );
            assert!(
                findings_of(&y).is_empty(),
                "{run_block} names (or honestly keeps) its stream"
            );
        }
        // No run: block at all — the status quo declares nothing.
        let y = format!(
            "{HEAD}tasks:\n  flaky:\n    exec: {{ command: [\"flaky\"] }}\n    retry: {{ max_attempts: 2, backoff_ms: 1000, jitter: true }}\n"
        );
        assert!(findings_of(&y).is_empty());
    }

    /// The emitted⊆registered ratchet, run-declaration tier (the
    /// `composition.rs` / `permit_taint.rs` pattern): every F-P3 mint the
    /// engine stamps — the check-side 028 here, the parse-side 026/027 in
    /// nika-schema's envelope — must exist in the vendored canon registry.
    #[test]
    fn the_run_decl_codes_are_registered_in_the_canon() {
        let registered: std::collections::BTreeSet<String> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code.to_string())
            .collect();
        for code in ["NIKA-PARSE-026", "NIKA-PARSE-027", RUN_DECL_CODE] {
            assert!(
                registered.contains(code),
                "`{code}` is not in the canon registry (spec/05-errors.md SSOT)"
            );
        }
    }
}
