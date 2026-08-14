//! `nika trace verify` — how far does this journal's proof honestly reach?
//!
//! Every journal line (0.96+) carries a `chain` field: the sha256 of
//! the PREVIOUS line's exact bytes (genesis: a constant tag). Verify
//! walks the file and recomputes — any edited, inserted, dropped or
//! reordered line breaks every hash after it. That is the first of
//! FOUR tiers; the verb reports the HIGHEST honestly-attained one,
//! never more:
//!
//! - **OK** — the chain is intact (tamper-evident, not tamper-proof:
//!   with no external trust root, an attacker can rewrite the whole
//!   chain; compare the head against the one the run printed).
//! - **INCOMPLETE** (F-P2) — the chain is intact but the run never
//!   reached a lifecycle-terminal frame (kill -9 · crash between
//!   writes): the finding rides the verifier — the dying run writes
//!   nothing. The exit stays OK (the journal is honestly what it is);
//!   the ladder below still climbs over the complete prefix.
//! - **SEALED** — the terminal `run_sealed` event's ed25519 signature
//!   verifies against a custody-resolved key (`--key` ·
//!   `~/.nika/keys/run-signing.pub` · the `retired.pub` ledger),
//!   matched by the seal's `key_id` fingerprint. A re-written journal
//!   now needs the key, not just write access.
//! - **ANCHORED** — SEALED, plus the detached `<trace>.anchor.json`
//!   sidecar verifies fully OFFLINE: the recomputed head is the one
//!   the Rekor v2 entry binds (digest + Ed25519ph signature with the
//!   SAME key), the checkpoint is the pinned shard key's signature,
//!   the RFC 6962 inclusion proof recomputes its root, and the RFC
//!   3161 token verifies against the pinned TSA leaf with the head as
//!   its imprint. Any gap drops the report to SEALED with the reason
//!   printed — and is a FILE failure (a forged anchor is the forgery
//!   class, not a softer finding).
//! - **REPLAYED** — ANCHORED or not, `--replay <fresh.ndjson>` runs
//!   the `trace reproduce` comparison: the recorded run re-executes
//!   identically. Verify NEVER re-executes; without the flag the tier
//!   is stated as not attempted, never faked.
//!
//! Two scoped stated verdicts ride beside the ladder — they never move
//! the tier nor the exit code (the F-P2 INCOMPLETE posture): the
//! NEP-0007 permit-witness **FINDING**, and the F-P18 **COST-REPLAY**
//! leg (NEP-0017 · the boot pin's pricing table against this engine's:
//! `unrecorded` on a pre-law journal · **REFUSED** on an unknown table
//! version, both identities named · the budget verdict re-judged
//! consistency-grade when the pinned table IS this engine's).
//!
//! Exit codes (the house taxonomy): 0 = the reported tier holds ·
//! 2 (FILE) = broken chain · forged seal · forged anchor · replay
//! divergence · 3 (ENV) = unchained (a pre-chain journal — stated,
//! never guessed) · missing input · `--anchored` with no sidecar.

use std::path::PathBuf;

use nika_dap::anchor::tier;

use super::VerbOutput;

/// The verify surface's knobs — the tier ladder's explicit asks.
#[derive(Clone, Debug, Default)]
pub struct VerifyOptions {
    /// A candidate run public key (the minisign box) — checked before
    /// the custody defaults.
    pub key: Option<PathBuf>,
    /// Require the anchor tier: a MISSING sidecar becomes the ENV
    /// failure the operator asked for (a broken one is FILE anyway).
    pub anchored: bool,
    /// A fresh journal of the same workflow for the REPLAYED tier.
    pub replay: Option<PathBuf>,
}

/// `nika trace verify <trace>` — today's byte-stable surface: the
/// default ladder (no explicit key, no required anchor, no replay).
#[must_use]
pub fn verify(trace: &str) -> VerbOutput {
    verify_with(trace, &VerifyOptions::default())
}

/// The tiered verify — see the module doc for the ladder and the
/// exit-code law.
#[must_use]
pub fn verify_with(trace: &str, opts: &VerifyOptions) -> VerbOutput {
    let candidates = match crate::seal::candidate_pubkeys(opts.key.as_deref()) {
        Ok(candidates) => candidates,
        Err(e) => return VerbOutput::env(e),
    };
    // NEP-0012 law 1 · the journal is untrusted input: bound the total
    // read BEFORE it happens (the per-line cap rides `chain.rs` · this
    // is the whole-file half).
    if let Ok(meta) = std::fs::metadata(trace)
        && meta.len() > nika_dap::bounded::MAX_JOURNAL_BYTES as u64
    {
        return VerbOutput::env(format!(
            "{trace}: {} bytes — over the journal bound ({} bytes · NEP-0012 law 1 · a file beyond it is not a run this engine produced)",
            meta.len(),
            nika_dap::bounded::MAX_JOURNAL_BYTES
        ));
    }
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    match walk(&raw) {
        Verdict::Intact { events, head, .. } => tiered(
            trace,
            &raw,
            events,
            &head,
            ChainHeadline::Intact,
            opts,
            &candidates,
        ),
        Verdict::Incomplete { events, head, .. } => tiered(
            trace,
            &raw,
            events,
            &head,
            ChainHeadline::Incomplete,
            opts,
            &candidates,
        ),
        Verdict::TornTail { events, head, .. } => tiered(
            trace,
            &raw,
            events,
            &head,
            ChainHeadline::Torn,
            opts,
            &candidates,
        ),
        Verdict::Broken {
            line,
            recorded,
            computed,
            ..
        } => VerbOutput::file(format!(
            "BROKEN at line {line} — recorded chain {} · computed {}\n  every line from here on is unverified (edited, inserted, dropped or reordered)",
            short(&recorded),
            short(&computed),
        )),
        // F-P1 · the fortress line bound: beyond the verifier's bounds
        // is a FILE refusal (a 100 MB line is a DoS vector, never a
        // journal line — recognized, never partially read).
        Verdict::LineOverLong { line, got, .. } => VerbOutput::file(format!(
            "line {line} is {got} bytes — beyond the verifier's line bound ({} bytes)\n  a journal line is small (the seal's covers included); an oversized line is\n  the DoS class, refused before any parse (F-P1)",
            nika_dap::chain::MAX_LINE_BYTES,
        )),
        Verdict::Unchained => VerbOutput::env(format!(
            "unchained — {trace} predates the chain (pre-0.96 journal): nothing to verify, nothing to distrust"
        )),
        Verdict::Empty => VerbOutput::env(format!("{trace}: no events")),
        Verdict::Unreadable { line, .. } => VerbOutput::env(format!(
            "{trace}:{line}: not a journal — the line is not valid JSON"
        )),
        // The verdict is #[non_exhaustive]: a NEWER forensics crate may
        // learn classes this CLI cannot render — refuse honestly,
        // never mis-render one.
        _ => VerbOutput::env(format!(
            "{trace}: unknown verdict class — the forensics library is newer than this CLI"
        )),
    }
}

/// The variadic form (`nika trace verify .nika/traces/*.ndjson` — the
/// glob muscle memory types): each file verifies under its own header,
/// no stop-at-first, and the worst exit survives — the `check`
/// multi-file law, one voice across verbs. Store handles resolve
/// exactly as the single form does.
#[must_use]
pub fn verify_many(traces: &[std::path::PathBuf]) -> VerbOutput {
    verify_many_with(traces, &VerifyOptions::default())
}

/// The tiered variadic form — every file climbs the ladder under its
/// own header.
#[must_use]
pub fn verify_many_with(traces: &[std::path::PathBuf], opts: &VerifyOptions) -> VerbOutput {
    let mut blocks = Vec::with_capacity(traces.len());
    let mut worst = super::exit::OK;
    for path in traces {
        let resolved = super::trace::manage::resolve_store_handle(path);
        let out = verify_with(&resolved.to_string_lossy(), opts);
        let mut block = format!("{}\n", resolved.display());
        for line in out.text.lines() {
            block.push_str("  ");
            block.push_str(line);
            block.push('\n');
        }
        blocks.push(block);
        worst = worst.max(out.code);
    }
    VerbOutput {
        text: blocks.join("\n"),
        code: worst,
    }
}

/// The chain-intact headline the verify surface prints (the lifecycle
/// truth the walk attested, F-P2). `Intact`'s OK line stays byte-
/// identical to the pre-tier surface; `Torn` and `Incomplete` name the
/// crash signature the journal carries.
#[derive(Clone, Copy)]
enum ChainHeadline {
    /// Chain intact AND the run reached a lifecycle-terminal frame.
    Intact,
    /// Chain intact — the final line is torn (a crash mid-write).
    Torn,
    /// Chain intact — the run never reached a terminal frame (kill -9 ·
    /// crash between writes): a finding, never a silence.
    Incomplete,
}

/// The ladder above an intact chain: the OK line stays byte-identical
/// to the pre-tier surface; the tiers then speak for themselves.
/// `headline` carries the walk's lifecycle truth (the torn-tail OK
/// variant · the F-P2 INCOMPLETE class — the ladder still climbs over
/// the COMPLETE prefix). The EVALUATION lives
/// in the forensics crate (`nika_dap::anchor::tier::evaluate` — the
/// 15k descent); this verb keeps the OK line, the reproduce shim call
/// (the fs seam), and the `VerbOutput` envelope + exit code.
/// The `--replay` leg's fs seam: run the reproduce comparison and hand the
/// ladder what the outcome MEANS (never how it was obtained). `None` = the
/// flag was not passed — verify itself never re-executes.
fn replay_compare(trace: &str, opts: &VerifyOptions) -> Option<tier::ReplayCompare> {
    opts.replay.as_ref().map(|fresh| {
        let out = super::trace_reproduce::reproduce(trace, &fresh.to_string_lossy());
        match out.code {
            super::exit::OK => tier::ReplayCompare::Reproduced,
            super::exit::FILE => tier::ReplayCompare::Diverged(out.text),
            _ => tier::ReplayCompare::CannotRun(
                out.text
                    .lines()
                    .next()
                    .unwrap_or("the reproduce path cannot run")
                    .to_owned(),
            ),
        }
    })
}

/// The buried-seal render: the chain is intact and one of its lines is a
/// SEAL with lines after it, so the walk's own headline (a crash reading,
/// or a flat OK) is replaced — the forgery class states itself.
fn tampered_verdict(events: usize, head: &str, lines: &[String]) -> VerbOutput {
    let mut out = format!(
        "TAMPERED — {events} events · chain intact · head {head}\n  the chain covers every line, and one of them is a SEAL with lines after it"
    );
    for line in lines {
        use std::fmt::Write as _;
        let _ = write!(out, "\n{line}");
    }
    VerbOutput {
        text: out,
        code: super::exit::FILE,
    }
}

fn tiered(
    trace: &str,
    raw: &str,
    events: usize,
    head: &str,
    headline: ChainHeadline,
    opts: &VerifyOptions,
    candidates: &[(String, String)],
) -> VerbOutput {
    let compared = replay_compare(trace, opts);
    let report = tier::evaluate(
        trace,
        raw,
        events,
        head,
        opts.anchored,
        compared.as_ref(),
        candidates,
    );
    // A BURIED seal overrides the walk's headline: the walk reads only the
    // LAST line, so an append after the seal came out as « never reached a
    // terminal frame … killed or crashed » (false — the journal carries a
    // `run_sealed` frame) or, when the appended line is itself terminal, as
    // a plain « OK · chain intact ». Both readings pointed away from the
    // tampering; the class states itself here (the ladder lines carry why).
    if let tier::SealTier::Buried { .. } = report.seal {
        return tampered_verdict(events, head, &report.lines);
    }
    let mut out = match headline {
        ChainHeadline::Torn => format!(
            "OK — {events} events · chain intact · head {head}\n  the final line is TORN (a crash mid-write, not tampering) — the chain\n  covers every complete line"
        ),
        ChainHeadline::Intact => format!(
            "OK — {events} events · chain intact · head {head}\n  internally consistent (tamper-evident, not tamper-proof) — compare the head\n  against the one the run printed to close the loop"
        ),
        // F-P2 · the killed run: the chain attests every complete line —
        // the lifecycle end is absent, said out loud. A FINDING (the
        // exit stays OK: the journal is honestly what it is), never the
        // `unknown` silence a truncated fold used to answer. The
        // attestation rides the VERIFIER — the dying run writes nothing.
        ChainHeadline::Incomplete => format!(
            "INCOMPLETE — {events} events · chain intact · head {head}\n  the journal never reached a terminal frame (no workflow_completed · workflow_failed ·\n  workflow_paused · workflow_cancelled · run_sealed) — the run was killed or crashed:\n  the chain attests every complete line; the lifecycle end is unattested, a finding\n  the verifier carries (the dying run can attest nothing)"
        ),
    };
    for line in &report.lines {
        use std::fmt::Write as _;
        let _ = write!(out, "\n{line}");
    }
    if let Some(finding) = witness_finding(raw) {
        use std::fmt::Write as _;
        let _ = write!(out, "\n{finding}");
    }
    // F-P18 · the cost-replay leg (NEP-0017 · la table de prix DANS le
    // pin): a SCOPED STATED VERDICT — it rides after the ladder and the
    // witness finding and never moves the chain verdict nor the exit
    // code (the F-P2 `Incomplete` posture). The judge is pure; the
    // LOCAL identity is this binary's compile-time catalog, injected
    // here (the leg reads no catalog itself).
    let snapshot = nika_catalog::pricing_snapshot();
    let leg = nika_dap::cost_replay::cost_replay_leg(
        raw,
        &nika_dap::cost_replay::PricingPin::new(
            nika_catalog::PRICING_SCHEMA,
            snapshot.as_of,
            snapshot.source_sha256_16,
        ),
    );
    for line in &leg.lines {
        use std::fmt::Write as _;
        let _ = write!(out, "\n{line}");
    }
    let code = match report.exit {
        tier::TierExit::Ok => super::exit::OK,
        tier::TierExit::File => super::exit::FILE,
        // TierExit is #[non_exhaustive]: a class newer than this CLI is
        // an era answer (ENV), never a guessed forgery.
        _ => super::exit::ENV,
    };
    VerbOutput { text: out, code }
}

/// NEP-0007 law 3 (spec 17 §the permit witness) — the REQUIRED-witness
/// rule: a chain-intact journal whose run exercised effects (an
/// `exec ·` / `invoke ·` / `agent ·` task started) and carries ZERO
/// `permit_checked` frames is a FINDING · the witness is absent (the
/// journal predates NEP-0007 or the engine is not conformant) · never
/// FORGED (the chain still binds every line) · never a crash (an
/// unreadable line is the walk's business, not this rule's).
fn witness_finding(raw: &str) -> Option<&'static str> {
    let mut effects = false;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("kind").and_then(serde_json::Value::as_str) {
            Some("permit_checked") => return None,
            Some("task_started") => {
                let note = v
                    .get("fields")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|fields| {
                        fields.iter().find(|f| {
                            f.get("key").and_then(serde_json::Value::as_str) == Some("note")
                        })
                    })
                    .and_then(|f| f.get("value"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if ["exec ·", "invoke ·", "agent ·"]
                    .iter()
                    .any(|p| note.starts_with(p))
                {
                    effects = true;
                }
            }
            _ => {}
        }
    }
    effects.then_some(
        "FINDING — the run exercised effects and carries zero permit_checked frames: the\n  permit witness is absent (NEP-0007 · the journal predates the witness or the\n  engine is not conformant) — the chain still binds every line",
    )
}

// The walk + its verdict live in the forensics crate (one genesis tag ·
// one hash primitive · the sink imports the SAME constant) — re-exported
// here so every `super::trace_verify::walk` consumer reads unchanged.
pub(crate) use nika_dap::chain::{Verdict, walk};

/// First 16 chars when they LOOK like hex — an adversarial `chain`
/// string renders as its sanitized head, never verbatim.
fn short(hex: &str) -> String {
    sanitize(hex).chars().take(16).collect()
}

/// Strip control characters (terminal-escape injection: a journal
/// field must never drive the operator's terminal).
pub(crate) fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_dap::chain::CHAIN_GENESIS;
    use nika_event::source_id::sha256_hex;

    /// Build a chained journal the way the sink does (the nika-dap
    /// chain-test idiom).
    fn chained(kinds: &[&str]) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn stage(name: &str, raw: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("nika-verify-many-{name}"));
        std::fs::write(&path, raw).expect("stage");
        path
    }

    /// The glob form: every file gets its own header + verdict, the
    /// verify never stops at the first failure, and the worst exit
    /// survives (2 FILE from the tampered journal beats the intact 0).
    #[test]
    fn verify_many_reports_per_file_and_exits_worst_of() {
        let intact = stage(
            "intact.ndjson",
            &chained(&["workflow_started", "workflow_completed"]),
        );
        // Tamper a MIDDLE line — the next line's recorded chain breaks.
        // (An edited FINAL line is exactly what only the externally
        // anchored head can catch — the module doc's whole point.)
        let mut tampered_raw =
            chained(&["workflow_started", "task_completed", "workflow_completed"]);
        tampered_raw = tampered_raw.replacen("task_completed", "task_failedxx", 1);
        let tampered = stage("tampered.ndjson", &tampered_raw);

        let out = verify_many(&[intact.clone(), tampered.clone()]);
        assert_eq!(out.code, super::super::exit::FILE, "worst-of: {}", out.text);
        assert!(
            out.text.contains("nika-verify-many-intact.ndjson"),
            "first header: {}",
            out.text
        );
        assert!(
            out.text.contains("nika-verify-many-tampered.ndjson"),
            "second header (no stop-at-first): {}",
            out.text
        );
        assert!(out.text.contains("OK — 2 events"), "{}", out.text);
        assert!(out.text.contains("BROKEN at line"), "{}", out.text);
        let _ = std::fs::remove_file(intact);
        let _ = std::fs::remove_file(tampered);
    }

    // ── The tier ladder (A4) ─────────────────────────────────────────

    /// A fresh minisign keypair (the seal.rs test idiom — the box is
    /// trimmed exactly as the custody loaders hand it back).
    fn keypair() -> (String, minisign::SecretKey) {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        (
            pair.pk
                .to_box()
                .expect("pk box")
                .to_string()
                .trim()
                .to_owned(),
            pair.sk,
        )
    }

    /// A chained journal sealed with the given key — the seal line is
    /// the terminal event, chained like every other.
    fn sealed_journal(kinds: &[&str], sk: &minisign::SecretKey, pk_box: &str) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        let seal = crate::seal::seal_event(
            nika_types::id::EventId::generate(),
            nika_types::timestamp::Timestamp::from_unix_ms(1_700_000_000_000),
            &chain,
            kinds.len(),
            "wf-hash-test",
            "0.105.0-test",
            sk,
            pk_box,
        )
        .expect("the seal mints");
        let mut v = serde_json::to_value(&seal).expect("seal json");
        v["chain"] = serde_json::Value::String(chain);
        out.push_str(&serde_json::to_string(&v).expect("seal line"));
        out.push('\n');
        out
    }

    /// Stage a `--key` candidate file.
    fn stage_key(name: &str, pk_box: &str) -> std::path::PathBuf {
        stage(name, pk_box)
    }

    fn opts_with_key(key: std::path::PathBuf) -> VerifyOptions {
        VerifyOptions {
            key: Some(key),
            ..VerifyOptions::default()
        }
    }

    /// SEALED: the in-test sealed journal's signature verifies against
    /// the `--key` candidate — and the ladder says REPLAYED was not
    /// attempted, never faked.
    #[test]
    fn a_sealed_journal_attains_sealed_and_states_replay_honestly() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "task_completed"], &sk, &pk_box);
        let trace = stage("sealed.ndjson", &journal);
        let key = stage_key("sealed.pub", &pk_box);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(out.text.contains("OK — 3 events"), "{}", out.text);
        assert!(
            out.text
                .contains("SEALED — the run_sealed signature verifies"),
            "{}",
            out.text
        );
        assert!(
            out.text
                .contains(&format!("key {}", crate::seal::fingerprint(&pk_box))),
            "{}",
            out.text
        );
        assert!(out.text.contains("ANCHORED — no sidecar"), "{}", out.text);
        assert!(
            out.text.contains("REPLAYED — not attempted"),
            "{}",
            out.text
        );
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// (F-P2) A journal that never reached a terminal frame verifies
    /// INCOMPLETE — the finding is the VERIFIER's (the dying run writes
    /// nothing) and the exit stays OK: the journal is honestly what it
    /// is, the tier ladder still speaks.
    #[test]
    fn a_terminal_less_journal_verifies_incomplete() {
        let trace = stage(
            "incomplete.ndjson",
            &chained(&["workflow_started", "task_completed"]),
        );
        let out = verify_with(&trace.to_string_lossy(), &VerifyOptions::default());
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(out.text.contains("INCOMPLETE — 2 events"), "{}", out.text);
        assert!(
            out.text.contains("never reached a terminal frame"),
            "{}",
            out.text
        );
        let _ = std::fs::remove_file(trace);
    }

    /// (F-P2) The EXTENDED seal (teardown covers) attains SEALED through
    /// the real verify tier — the verifier signs off on the extended
    /// covers object exactly as on the classic four.
    #[test]
    fn an_extended_seal_attains_sealed_through_the_tier() {
        let (pk_box, sk) = keypair();
        let kinds = ["workflow_started", "workflow_completed"];
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut journal = String::new();
        for kind in kinds {
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000002"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": []
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            journal.push_str(&line);
            journal.push('\n');
        }
        let mut teardown = crate::seal::SealTeardown::new();
        teardown.outcome = Some("completed".to_owned());
        teardown.budgets = Some(serde_json::json!({ "priced_calls": 0 }));
        let seal = crate::seal::seal_event_with(
            nika_types::id::EventId::generate(),
            nika_types::timestamp::Timestamp::from_unix_ms(1_700_000_000_000),
            &chain,
            kinds.len(),
            "wf-hash-test",
            "0.105.0-test",
            Some(&teardown),
            &sk,
            &pk_box,
        )
        .expect("the extended seal mints");
        let mut v = serde_json::to_value(&seal).expect("seal json");
        v["chain"] = serde_json::Value::String(chain);
        journal.push_str(&serde_json::to_string(&v).expect("seal line"));
        journal.push('\n');
        let trace = stage("sealed-extended.ndjson", &journal);
        let key = stage_key("sealed-extended.pub", &pk_box);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("SEALED — the run_sealed signature verifies"),
            "the extended covers verify: {}",
            out.text
        );
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// A seal whose signature was tampered is the forgery class: SEAL
    /// FORGED, exit FILE — never a softer finding.
    #[test]
    fn a_tampered_seal_signature_is_forged() {
        let (pk_box, sk) = keypair();
        // One byte flipped inside the signature box.
        let journal = sealed_journal(&["workflow_started"], &sk, &pk_box)
            .replace("trusted comment", "truzted comment");
        let trace = stage("forged-sig.ndjson", &journal);
        let key = stage_key("forged-sig.pub", &pk_box);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("SEAL FORGED"), "{}", out.text);
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// The key-id mismatch: the seal names a key no candidate carries —
    /// honest failure, never a pass, AND never an accusation. The
    /// 2026-07-30 adversarial pass moved this from FILE (« SEAL FORGED »
    /// on an intact, genuinely sealed journal whose key simply was not in
    /// custody — the third-party case a transparency artifact exists for)
    /// to ENV: the signature is not judged. Non-zero either way, so a
    /// `verify && promote` gate still fails closed.
    #[test]
    fn a_key_id_mismatch_across_all_candidates_is_unattributable_never_forged() {
        let (pk_a, sk_a) = keypair();
        let (pk_b, _sk_b) = keypair();
        // Sealed with A's secret + pub recorded — the candidates only B.
        let journal = sealed_journal(&["workflow_started"], &sk_a, &pk_a);
        let trace = stage("mismatch.ndjson", &journal);
        let key = stage_key("mismatch.pub", &pk_b);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("SEAL UNATTRIBUTABLE"), "{}", out.text);
        assert!(out.text.contains("no candidate carries it"), "{}", out.text);
        assert!(
            !out.text.contains("FORGED"),
            "an absent key is never evidence of forgery: {}",
            out.text
        );
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// The 2026-07-30 adversarial pass · APPEND-AFTER-SEAL at the CLI
    /// plane: a valid seal with a line chained after it reports TAMPERED
    /// (exit FILE) and NEVER blames a crash. The two measured pre-fix
    /// renders are both asserted away: the appended line's kind decides
    /// whether the walk said « INCOMPLETE … killed or crashed » (a false
    /// statement — the journal carries a `run_sealed` frame) or « OK · chain
    /// intact » — and both exited 0 with the seal never mentioned.
    #[test]
    fn a_buried_seal_reports_tampered_and_never_blames_a_crash() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "workflow_completed"], &sk, &pk_box);
        let seal_line = journal.lines().last().expect("the seal line").to_owned();
        let key = stage_key("buried.pub", &pk_box);

        // Both appended kinds: an ordinary frame (walk → Incomplete) and a
        // terminal one (walk → Intact). Same verdict, same exit.
        for (name, kind) in [
            ("buried-plain.ndjson", "task_completed"),
            ("buried-terminal.ndjson", "workflow_completed"),
        ] {
            let appended = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-0000000000ff"},
                "timestamp": 9999, "kind": kind, "run": null,
                "correlation": null, "fields": [{"key": "task", "value": "appended"}],
                "chain": sha256_hex(seal_line.as_bytes()),
            });
            let forged = format!(
                "{journal}{}\n",
                serde_json::to_string(&appended).expect("appended line")
            );
            let trace = stage(name, &forged);
            let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
            assert_eq!(
                out.code,
                super::super::exit::FILE,
                "the append is the forgery class ({kind}): {}",
                out.text
            );
            assert!(out.text.contains("TAMPERED"), "{}", out.text);
            assert!(out.text.contains("SEAL BURIED"), "{}", out.text);
            assert!(
                !out.text.contains("killed or crashed"),
                "a seal proves the run ended — never a crash story ({kind}): {}",
                out.text
            );
            let _ = std::fs::remove_file(trace);
        }
        let _ = std::fs::remove_file(key);
    }

    /// A seal lifted onto a journal it did not mint (covers.head no
    /// longer matches the chain field) is forged, not sealed.
    #[test]
    fn a_transplanted_seal_is_forged() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started", "task_completed"], &sk, &pk_box);
        // Rewrite history BEFORE the seal: flip the first line's kind
        // and re-chain line 2 so the chain verifies, but leave the
        // seal line untouched — its covers.head now names a head that
        // never existed here.
        let raws: Vec<&str> = journal.lines().collect();
        let mut first: serde_json::Value = serde_json::from_str(raws[0]).expect("first");
        first["kind"] = serde_json::Value::String("workflow_started_x".to_owned());
        let first_raw = serde_json::to_string(&first).expect("first raw");
        let new_head = sha256_hex(first_raw.as_bytes());
        let mut second: serde_json::Value = serde_json::from_str(raws[1]).expect("second");
        second["chain"] = serde_json::Value::String(new_head.clone());
        let second_raw = serde_json::to_string(&second).expect("second raw");
        let seal_head = sha256_hex(second_raw.as_bytes());
        let mut seal: serde_json::Value = serde_json::from_str(raws[2]).expect("seal");
        seal["chain"] = serde_json::Value::String(seal_head);
        let forged = format!(
            "{first_raw}\n{second_raw}\n{}\n",
            serde_json::to_string(&seal).expect("seal raw")
        );
        let trace = stage("transplanted.ndjson", &forged);
        let key = stage_key("transplanted.pub", &pk_box);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(
            out.text.contains("covers.head is not its chain field"),
            "{}",
            out.text
        );
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// The frozen LIVE fixture (journal + sidecar + key — captured
    /// once through the live verb · lives with the trust plane in
    /// `nika_dap::anchor::fixtures`, included cross-crate here).
    const FIXTURE_JOURNAL: &str = include_str!("../../nika-dap/src/anchor/fixtures/journal.ndjson");
    const FIXTURE_SIDECAR: &str = include_str!("../../nika-dap/src/anchor/fixtures/sidecar.json");
    const FIXTURE_PUBLIC_BOX: &str =
        include_str!("../../nika-dap/src/anchor/fixtures/run-signing.pub");

    /// End to end: journal + sidecar + key — the full ladder to
    /// ANCHORED, fully offline.
    #[test]
    fn the_frozen_fixture_attains_anchored_offline() {
        let dir = std::env::temp_dir().join(format!("nika-a4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let trace = dir.join("fixture.ndjson");
        std::fs::write(&trace, FIXTURE_JOURNAL).expect("journal");
        std::fs::write(
            crate::anchor::sidecar_path(&trace.to_string_lossy()),
            FIXTURE_SIDECAR,
        )
        .expect("sidecar");
        let key = dir.join("run.pub");
        std::fs::write(&key, FIXTURE_PUBLIC_BOX).expect("key");

        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key));
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("SEALED — the run_sealed signature verifies · key 1e772a7b922d7be3"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("ANCHORED — rekor index 34612959"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("gen_time 2026-07-20T20:46:49Z"),
            "{}",
            out.text
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tier demotion honesty: a forged anchor over a GOOD seal reports
    /// SEALED, never ANCHORED — and is the FILE (forgery) class.
    #[test]
    fn a_bad_anchor_over_a_good_seal_demotes_to_sealed() {
        let dir = std::env::temp_dir().join(format!("nika-a4-forged-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let trace = dir.join("fixture.ndjson");
        std::fs::write(&trace, FIXTURE_JOURNAL).expect("journal");
        let forged = FIXTURE_SIDECAR.replacen("34612964", "34612965", 1);
        std::fs::write(
            crate::anchor::sidecar_path(&trace.to_string_lossy()),
            forged,
        )
        .expect("sidecar");
        let key = dir.join("run.pub");
        std::fs::write(&key, FIXTURE_PUBLIC_BOX).expect("key");

        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key));
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("ANCHOR FORGED"), "{}", out.text);
        assert!(out.text.contains("reported tier: SEALED"), "{}", out.text);
        assert!(!out.text.contains("ANCHORED — rekor"), "{}", out.text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `--anchored` without a sidecar is the ENV class the operator
    /// asked for (a missing input, not a forgery).
    #[test]
    fn a_required_but_missing_sidecar_is_env() {
        let (pk_box, sk) = keypair();
        let journal = sealed_journal(&["workflow_started"], &sk, &pk_box);
        let trace = stage("required.ndjson", &journal);
        let key = stage_key("required.pub", &pk_box);
        let opts = VerifyOptions {
            key: Some(key.clone()),
            anchored: true,
            replay: None,
        };
        let out = verify_with(&trace.to_string_lossy(), &opts);
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("REQUIRED"), "{}", out.text);
        let _ = std::fs::remove_file(trace);
        let _ = std::fs::remove_file(key);
    }

    /// The audit's anchor contract, CLI level (2026-07-29 · run 2): an
    /// UNSEALED journal under `--anchored` is the same ENV refusal as a
    /// missing sidecar on a sealed one — the tier needs a seal to build
    /// on, and the requirement used to vanish silently at the Unsealed
    /// early return (measured rc=0 on both missing and forged sidecars).
    #[test]
    fn a_required_anchor_on_an_unsealed_journal_is_env() {
        let journal = chained_with(&[("workflow_completed", &[])]);
        let trace = stage("unsealed-required.ndjson", &journal);
        let opts = VerifyOptions {
            key: None,
            anchored: true,
            replay: None,
        };
        let out = verify_with(&trace.to_string_lossy(), &opts);
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("REQUIRED"), "{}", out.text);
        assert!(out.text.contains("unsealed"), "{}", out.text);
        let _ = std::fs::remove_file(trace);
    }

    /// REPLAYED: the fixture journal compared against itself
    /// reproduces — the tier speaks; the exit stays 0.
    #[test]
    fn a_journal_replayed_against_itself_attains_replayed() {
        let dir = std::env::temp_dir().join(format!("nika-a4-replay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let trace = dir.join("fixture.ndjson");
        std::fs::write(&trace, FIXTURE_JOURNAL).expect("journal");
        let key = dir.join("run.pub");
        std::fs::write(&key, FIXTURE_PUBLIC_BOX).expect("key");
        let opts = VerifyOptions {
            key: Some(key),
            anchored: false,
            replay: Some(trace.clone()),
        };
        let out = verify_with(&trace.to_string_lossy(), &opts);
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("REPLAYED — the journal re-executes identically"),
            "{}",
            out.text
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A chained journal with explicit `fields` payloads — the witness
    /// rule reads `task_started.note` and the `permit_checked` kind.
    fn chained_with(frames: &[(&str, &[(&str, &str)])]) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for (kind, fields) in frames {
            let fields: Vec<serde_json::Value> = fields
                .iter()
                .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
                .collect();
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": fields
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// NEP-0007 law 3 — an effectful run (an `exec ·` task started) with
    /// zero `permit_checked` frames: FINDING in the text, exit stays OK
    /// (never FORGED · the chain holds · never a crash).
    #[test]
    fn absent_witness_on_effectful_run_is_a_finding_never_a_failure() {
        let raw = chained_with(&[
            ("workflow_started", &[]),
            (
                "task_started",
                &[("task", "stamp"), ("note", "exec · echo")],
            ),
            ("task_completed", &[("task", "stamp")]),
            ("workflow_completed", &[]),
        ]);
        let path = stage("witness-absent", &raw);
        let out = verify(&path.to_string_lossy());
        assert_eq!(
            out.code,
            super::super::exit::OK,
            "a finding never fails: {}",
            out.text
        );
        assert!(
            out.text.contains("FINDING"),
            "the finding line rides: {}",
            out.text
        );
        assert!(
            out.text.contains("permit_checked"),
            "it names the absent frame: {}",
            out.text
        );
    }

    /// One `permit_checked` frame silences the rule — and a run with NO
    /// effectful task (pure infer) never triggers it.
    #[test]
    fn witness_present_or_pure_infer_run_stays_clean() {
        let witnessed = chained_with(&[
            (
                "task_started",
                &[("task", "stamp"), ("note", "exec · echo")],
            ),
            (
                "permit_checked",
                &[("plane", "exec"), ("decision", "allow")],
            ),
            ("task_completed", &[("task", "stamp")]),
        ]);
        let path = stage("witness-present", &witnessed);
        let out = verify(&path.to_string_lossy());
        assert_eq!(out.code, super::super::exit::OK);
        assert!(
            !out.text.contains("FINDING"),
            "witnessed run is clean: {}",
            out.text
        );

        let infer_only = chained_with(&[
            (
                "task_started",
                &[("task", "think"), ("note", "infer · mock/echo")],
            ),
            ("task_completed", &[("task", "think")]),
        ]);
        let path = stage("witness-infer-only", &infer_only);
        let out = verify(&path.to_string_lossy());
        assert!(
            !out.text.contains("FINDING"),
            "no effects = no required witness: {}",
            out.text
        );
    }

    // ── The F-P18 cost-replay leg (NEP-0017) ─────────────────────────

    /// A chained journal with arbitrary-JSON field payloads — the cost
    /// leg reads numeric `cost_usd` / `total_cost_usd` and JSON-text
    /// `pricing` / `budget` (the `chained_with` shape, values untyped).
    fn chained_with_json(frames: &[(&str, &[(&str, serde_json::Value)])]) -> String {
        let mut chain = sha256_hex(CHAIN_GENESIS);
        let mut out = String::new();
        for (kind, fields) in frames {
            let fields: Vec<serde_json::Value> = fields
                .iter()
                .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
                .collect();
            let mut v = serde_json::json!({
                "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": kind, "run": null,
                "correlation": null, "fields": fields
            });
            v["chain"] = serde_json::Value::String(chain.clone());
            let line = serde_json::to_string(&v).expect("test json");
            chain = sha256_hex(line.as_bytes());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// This binary's compile-time pricing identity, as the boot pin
    /// journals it (JSON text — the nested-object idiom).
    fn local_pricing_field() -> serde_json::Value {
        let snapshot = nika_catalog::pricing_snapshot();
        serde_json::Value::String(
            serde_json::json!({
                "schema": nika_catalog::PRICING_SCHEMA,
                "as_of": snapshot.as_of,
                "sha256_16": snapshot.source_sha256_16,
            })
            .to_string(),
        )
    }

    /// A pre-F-P18 journal (no `pricing` key): the leg states
    /// `unrecorded` — the `LOCK_UNRECORDED` posture — and the trace
    /// verdict itself is untouched (exit stays OK).
    #[test]
    fn a_pre_law_journal_states_cost_replay_unrecorded() {
        let raw = chained_with_json(&[("workflow_started", &[]), ("workflow_completed", &[])]);
        let path = stage("cost-unrecorded", &raw);
        let out = verify(&path.to_string_lossy());
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text.contains("COST-REPLAY — unrecorded"),
            "{}",
            out.text
        );
        assert!(out.text.contains("pre-F-P18"), "{}", out.text);
    }

    /// The law's negative case through the real verb: the boot pin names
    /// a table this engine does not carry — the leg REFUSES cost-replay,
    /// naming BOTH identities, and the chain verdict stays OK (a scoped
    /// stated verdict never corrupts it).
    #[test]
    fn an_unknown_pricing_table_refuses_cost_replay() {
        let pin = serde_json::Value::String(
            serde_json::json!({
                "schema": "nika/model-pricing@1.1",
                "as_of": "2031-01-15",
                "sha256_16": "deadbeefdeadbeef",
            })
            .to_string(),
        );
        let raw = chained_with_json(&[
            ("workflow_started", &[("pricing", pin)]),
            ("workflow_completed", &[]),
        ]);
        let path = stage("cost-refused", &raw);
        let out = verify(&path.to_string_lossy());
        assert_eq!(
            out.code,
            super::super::exit::OK,
            "a refusal is the leg's stated posture, never a chain failure: {}",
            out.text
        );
        assert!(out.text.contains("COST-REPLAY — REFUSED"), "{}", out.text);
        assert!(
            out.text.contains("2031-01-15"),
            "the pinned side named: {}",
            out.text
        );
        assert!(
            out.text.contains(nika_catalog::pricing_snapshot().as_of),
            "the local side named: {}",
            out.text
        );
    }

    /// The positive half through the real verb: the pinned table IS
    /// this engine's — the leg re-judges the journaled budget verdict
    /// (within-budget PASS agrees) and the totals.
    #[test]
    fn a_pinned_matching_journal_rejudges_the_budget_verdict() {
        let raw = chained_with_json(&[
            (
                "workflow_started",
                &[
                    ("pricing", local_pricing_field()),
                    (
                        "budget",
                        serde_json::Value::String("{\"max_cost_usd\":0.05}".to_owned()),
                    ),
                ],
            ),
            (
                "task_completed",
                &[
                    ("task", serde_json::json!("think")),
                    ("cost_usd", serde_json::json!(0.01)),
                ],
            ),
            (
                "task_completed",
                &[
                    ("task", serde_json::json!("stamp")),
                    ("cost_usd", serde_json::json!(0.02)),
                ],
            ),
            (
                "workflow_completed",
                &[("total_cost_usd", serde_json::json!(0.03))],
            ),
        ]);
        let path = stage("cost-rejudged", &raw);
        let out = verify(&path.to_string_lossy());
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("COST-REPLAY — the pinned pricing table is this engine's"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("within, agrees with the run's PASS"),
            "{}",
            out.text
        );
        assert!(out.text.contains("totals: agrees"), "{}", out.text);
    }

    /// The consistency grade's teeth through the real verb: a journal
    /// whose re-summed spend crosses the journaled budget yet claims
    /// PASS — stated DIVERGES, the exit stays the chain's (the witness
    /// finding's posture, not the replay tier's FILE).
    #[test]
    fn a_rewritten_cost_story_states_divergence_without_moving_the_exit() {
        let raw = chained_with_json(&[
            (
                "workflow_started",
                &[
                    ("pricing", local_pricing_field()),
                    (
                        "budget",
                        serde_json::Value::String("{\"max_cost_usd\":0.05}".to_owned()),
                    ),
                ],
            ),
            (
                "task_completed",
                &[
                    ("task", serde_json::json!("think")),
                    ("cost_usd", serde_json::json!(0.06)),
                ],
            ),
            (
                "workflow_completed",
                &[("total_cost_usd", serde_json::json!(0.03))],
            ),
        ]);
        let path = stage("cost-diverged", &raw);
        let out = verify(&path.to_string_lossy());
        assert_eq!(
            out.code,
            super::super::exit::OK,
            "stated, never the chain verdict: {}",
            out.text
        );
        assert!(out.text.contains("DIVERGES"), "{}", out.text);
    }
}
