// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace anchor` — notarize the journal head OUTSIDE the journal
//! (S3 · the verifiable-run wave).
//!
//! The chain makes a journal tamper-evident; the seal makes a forgery
//! need the run key. The anchor answers the last rewrite: an attacker
//! replacing journal + seal wholesale still cannot mint a head that
//! the public transparency log notarized BEFORE the rewrite — the
//! sidecar's Rekor entry + RFC 3161 timestamp predate any later edit.
//!
//! Anchoring is an explicit network act: this verb IS the opt-in (a
//! bare `nika run` never phones anywhere). The head is submitted to
//! the public Sigstore Rekor v2 shard and timestamped by the Sigstore
//! TSA; the proof lands in a DETACHED `<trace>.anchor.json` sidecar
//! the journal never sees. Everything fails closed — a torn or broken
//! journal is refused, the log's answer is verified before it is
//! trusted, and the sidecar is written atomically or not at all.
//!
//! The ORCHESTRATION lives in the forensics crate
//! (`nika_dap::anchor::run::anchor_journal` — the 15k descent); this
//! verb keeps the effect composer (`ReqwestHttp`), the receipt, and
//! the exit-code envelope.
//!
//! Exit codes (the house taxonomy): 0 anchored · 2 (FILE) the journal
//! itself refuses (broken chain · torn tail) · 3 (ENV) unchained or
//! unreadable input · no run key · network/service/verification
//! failure.

use super::VerbOutput;
use crate::anchor::run as anchor_run;

/// The verb body — compose the production client, delegate, render.
#[must_use]
pub fn run(trace: &str, rekor_url: &str, tsa_url: &str) -> VerbOutput {
    match submit_blocking(trace, rekor_url, tsa_url) {
        Ok(report) => VerbOutput::ok(render(&report)),
        Err(failure) => {
            let out = failure.describe(trace);
            if failure.is_file() {
                VerbOutput::file(out)
            } else {
                VerbOutput::env(out)
            }
        }
    }
}

/// The blocking composer (the `registry.rs` idiom): the production
/// client is `ReqwestHttp` with SSRF enforcement ON — anchoring talks
/// to public HTTPS hosts only.
fn submit_blocking(
    trace: &str,
    rekor_url: &str,
    tsa_url: &str,
) -> Result<anchor_run::AnchoredReport, anchor_run::AnchorFailure> {
    let config = nika_http::HttpConfig::default();
    let http = nika_http::ReqwestHttp::with_config(config).map_err(|e| {
        anchor_run::AnchorFailure::Submit(format!("cannot initialize the anchor client: {e}"))
    })?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            anchor_run::AnchorFailure::Submit(format!("cannot start the anchor runtime: {e}"))
        })?;
    rt.block_on(anchor_run::anchor_journal(&http, trace, rekor_url, tsa_url))
}

/// The receipt: what was notarized, where, and the path to verify it.
fn render(report: &anchor_run::AnchoredReport) -> String {
    use std::fmt::Write as _;
    let sidecar = &report.sidecar;
    let mut out = format!(
        "ANCHORED — {} events · head {} notarized outside the journal\n",
        report.events,
        super::trace_verify::sanitize(&report.head)
    );
    let _ = writeln!(
        out,
        "  rekor {} · index {} · the checkpoint + inclusion proof verified before this line printed",
        super::trace_verify::sanitize(&sidecar.rekor.url),
        super::trace_verify::sanitize(&sidecar.rekor.log_index)
    );
    if report.custom_shard {
        let _ = writeln!(
            out,
            "  note: a custom shard — the checkpoint is NOT the pinned Sigstore key's, so `trace verify` will not claim ANCHORED (the pin is the Sigstore public shard's)"
        );
    }
    let _ = writeln!(
        out,
        "  rfc3161 {} · gen_time {} (the trusted time)",
        sidecar.rfc3161.tsa_url, sidecar.rfc3161.gen_time
    );
    let _ = writeln!(
        out,
        "  sidecar: {} (detached — the journal was never touched; verify: nika trace verify)",
        report.path.display()
    );
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use nika_dap::chain::CHAIN_GENESIS;
    use nika_event::source_id::sha256_hex;

    use super::*;

    /// The chain-test idiom (nika-dap): each line carries the sha256 of
    /// the previous line's exact bytes.
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
        // The pid keeps the staged file this PROCESS's own. Without it the
        // name is a fixed slot in the shared system temp dir, so a second
        // `cargo test -p nika-cli` (a pre-push gate beside a manual run, two
        // agents, a retried CI leg) reads or unlinks the first one's file
        // mid-test and the assertion dies on a path that "does not exist".
        // Same idiom as mcp_pins.rs, which already carries the pid.
        let path = std::env::temp_dir().join(format!("nika-anchor-{}-{name}", std::process::id()));
        std::fs::write(&path, raw).expect("stage");
        path
    }

    /// A broken journal is a FILE-class refusal — the anchor never
    /// notarizes a chain that does not verify.
    #[test]
    fn a_broken_journal_refuses_as_file() {
        // Tamper the FIRST line — the next line's recorded chain breaks
        // (editing the LAST line is intra-chain invisible by design;
        // that is the printed-head anchor's job, per chain.rs).
        let raw = chained(&["workflow_started", "task_completed"]).replacen(
            "workflow_started",
            "workflow_startex",
            1,
        );
        let path = stage("broken.ndjson", &raw);
        let out = run(
            &path.to_string_lossy(),
            crate::anchor::DEFAULT_REKOR_URL,
            crate::anchor::DEFAULT_TSA_URL,
        );
        let _ = std::fs::remove_file(path);
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("BROKEN at line 2"), "{}", out.text);
    }

    /// A torn tail is refused honestly (FILE): the head would exclude
    /// live bytes. A pre-chain journal is the ENV class.
    #[test]
    fn torn_and_unchained_journals_refuse_in_their_classes() {
        let mut torn_raw = chained(&["workflow_started", "workflow_completed"]);
        torn_raw.push_str("{\"id\":{\"uuid\":\"01912345");
        let torn = stage("torn.ndjson", &torn_raw);
        let out = run(
            &torn.to_string_lossy(),
            crate::anchor::DEFAULT_REKOR_URL,
            crate::anchor::DEFAULT_TSA_URL,
        );
        let _ = std::fs::remove_file(torn);
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("TORN"), "{}", out.text);

        let unchained = stage("unchained.ndjson", "{\"kind\":\"workflow_started\"}\n");
        let out = run(
            &unchained.to_string_lossy(),
            crate::anchor::DEFAULT_REKOR_URL,
            crate::anchor::DEFAULT_TSA_URL,
        );
        let _ = std::fs::remove_file(unchained);
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("unchained"), "{}", out.text);
    }

    /// A keyless machine refuses BEFORE any network act (ENV) — the
    /// opt-in verb never half-acts. (On a machine WITH a key enrolled
    /// this test would reach for the network, so it runs only when the
    /// custody probe is empty.)
    #[test]
    fn a_keyless_machine_refuses_before_the_network() {
        if crate::seal::load_signing_key().is_some() {
            return; // a dev/CI machine with an enrolled key: nothing to prove here
        }
        let path = stage("intact.ndjson", &chained(&["workflow_started"]));
        let out = run(
            &path.to_string_lossy(),
            crate::anchor::DEFAULT_REKOR_URL,
            crate::anchor::DEFAULT_TSA_URL,
        );
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("no run-signing key"), "{}", out.text);
        // And no sidecar appeared beside the staged journal.
        assert!(!crate::anchor::sidecar_path(&path.to_string_lossy()).exists());
        let _ = std::fs::remove_file(path);
    }
}
