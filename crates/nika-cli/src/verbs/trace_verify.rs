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
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    match walk(&raw) {
        Verdict::Intact { events, head, .. } => {
            tiered(trace, &raw, events, &head, None, opts, &candidates)
        }
        Verdict::TornTail { events, head, .. } => {
            tiered(trace, &raw, events, &head, Some(()), opts, &candidates)
        }
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

/// The ladder above an intact chain: the OK line stays byte-identical
/// to the pre-tier surface; the tiers then speak for themselves.
/// `torn` marks the torn-tail OK variant (a crash mid-write — the
/// ladder still climbs over the COMPLETE prefix). The EVALUATION lives
/// in the forensics crate (`nika_dap::anchor::tier::evaluate` — the
/// 15k descent); this verb keeps the OK line, the reproduce shim call
/// (the fs seam), and the `VerbOutput` envelope + exit code.
fn tiered(
    trace: &str,
    raw: &str,
    events: usize,
    head: &str,
    torn: Option<()>,
    opts: &VerifyOptions,
    candidates: &[(String, String)],
) -> VerbOutput {
    let mut out = if torn.is_some() {
        format!(
            "OK — {events} events · chain intact · head {head}\n  the final line is TORN (a crash mid-write, not tampering) — the chain\n  covers every complete line"
        )
    } else {
        format!(
            "OK — {events} events · chain intact · head {head}\n  internally consistent (tamper-evident, not tamper-proof) — compare the head\n  against the one the run printed to close the loop"
        )
    };
    // The replay comparison is the CLI's fs seam — the ladder only
    // hears what the outcome MEANS.
    let compared = opts.replay.as_ref().map(|fresh| {
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
    });
    let report = tier::evaluate(
        trace,
        raw,
        events,
        head,
        opts.anchored,
        compared.as_ref(),
        candidates,
    );
    for line in &report.lines {
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
    use nika_dap::source_id::sha256_hex;

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
            &chained(&["workflow_started", "task_completed"]),
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
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("keypair");
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

    /// The key-id mismatch: the seal names a key no candidate carries
    /// — honest failure, never a pass.
    #[test]
    fn a_key_id_mismatch_across_all_candidates_fails() {
        let (pk_a, sk_a) = keypair();
        let (pk_b, _sk_b) = keypair();
        // Sealed with A's secret + pub recorded — the candidates only B.
        let journal = sealed_journal(&["workflow_started"], &sk_a, &pk_a);
        let trace = stage("mismatch.ndjson", &journal);
        let key = stage_key("mismatch.pub", &pk_b);
        let out = verify_with(&trace.to_string_lossy(), &opts_with_key(key.clone()));
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("no candidate matches"), "{}", out.text);
        let _ = std::fs::remove_file(trace);
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
    const FIXTURE_JOURNAL: &str =
        include_str!("../../../nika-dap/src/anchor/fixtures/journal.ndjson");
    const FIXTURE_SIDECAR: &str =
        include_str!("../../../nika-dap/src/anchor/fixtures/sidecar.json");
    const FIXTURE_PUBLIC_BOX: &str =
        include_str!("../../../nika-dap/src/anchor/fixtures/run-signing.pub");

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
}
