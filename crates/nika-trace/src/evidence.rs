// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace evidence <trace>` — the exportable evidence pack (A5): ONE
//! directory an auditor takes offline and believes the run. The pack
//! COMPUTATION lives in the forensics plane ([`nika_dap::evidence`]);
//! this shell owns the verb seam: the enrolled-key read, the `--json`
//! projection, and the one-line human summary (render stays).
//!
//! Two disclosure classes (T9): the DEFAULT is the redacted pack —
//! the run's INTEGRITY, not its CONTENT (payloads ride as sha256
//! placeholders); `--full` ships the exact journal bytes, and says so.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::VerbOutput;

/// The clap surface of `nika trace evidence` — lives here so main.rs stays
/// under the 1500-line file cap (the `verbs::key::KeyAction` precedent).
#[derive(clap::Args)]
pub struct EvidenceArgs {
    /// Trace NDJSON path or a name from `trace ls` (default: latest).
    pub trace: Option<PathBuf>,
    /// Output directory (default: `<trace-stem>.evidence/` · never clobbered).
    #[arg(short, long)]
    pub out: Option<PathBuf>,
    /// The workflow file that ran — hash-checked; unlocks the
    /// boundary, the trifecta verdict and the receipt.
    #[arg(long)]
    pub workflow: Option<String>,
    /// Print the pack manifest to stdout (no directory written).
    #[arg(long)]
    pub json: bool,
    /// Carry the run's CONTENT verbatim (model outputs · tool results ·
    /// file reads). Default: the redacted pack — it proves the run's
    /// INTEGRITY, not its CONTENT; disclosure stays operator-side.
    #[arg(long)]
    pub full: bool,
}

/// `nika trace evidence <trace>` — export the pack. The enrolled-key set is
/// the machine's own custody ([`nika_dap::evidence::candidate_pubkeys`]).
/// `full` picks the exact-bytes class; the default is the redacted one.
#[must_use]
pub fn export(
    trace: &str,
    out: Option<&Path>,
    workflow: Option<&str>,
    json: bool,
    full: bool,
) -> VerbOutput {
    let keys = nika_dap::evidence::candidate_pubkeys();
    let build = if full {
        nika_dap::evidence::build_full
    } else {
        nika_dap::evidence::build
    };
    let pack = match build(Path::new(trace), workflow.map(Path::new), &keys) {
        Ok(pack) => pack,
        Err(e) => return VerbOutput::env(e.to_string()),
    };
    if json {
        return VerbOutput::ok(
            serde_json::to_string_pretty(&pack.manifest).unwrap_or_else(|_| "null".to_owned()),
        );
    }
    let dir = out.map_or_else(
        || nika_dap::evidence::default_out(Path::new(trace)),
        Path::to_path_buf,
    );
    match nika_dap::evidence::write(&dir, &pack) {
        Ok(()) => VerbOutput::ok(human_summary(&dir, &pack.manifest)),
        Err(e) => VerbOutput::env(e.to_string()),
    }
}

/// A hash for eyeballing: the leading 12 chars + an ellipsis.
fn clip(text: &str) -> String {
    if text.chars().count() <= 12 {
        return text.to_owned();
    }
    format!("{}…", text.chars().take(12).collect::<String>())
}

/// The human close: where the pack landed + each claim's one-line truth.
/// The journal line names the disclosure CLASS plainly — the redacted
/// projection's placeholder count, or the full pack's content warning
/// (a silent full pack is the leak wearing a new hat).
fn human_summary(dir: &Path, manifest: &Value) -> String {
    use std::fmt::Write as _;
    let mut out = format!("evidence pack: {}", dir.display());
    let head = manifest["trace"]["head"]
        .as_str()
        .map_or_else(|| "no head".to_owned(), |h| format!("head {}", clip(h)));
    let _ = write!(
        out,
        "\n  journal.ndjson — {} · chain {} · {head}",
        journal_word(manifest),
        manifest["trace"]["chain"].as_str().unwrap_or("?")
    );
    let _ = write!(
        out,
        "\n  pack.json — evidence_format 1 · {} · {}",
        seal_word(manifest),
        boundary_word(manifest)
    );
    if let Some(proves) = manifest["receipt"]["proves"].as_str() {
        let _ = write!(out, "\n  receipt.json — proves {}", clip(proves));
    }
    let verify_word = if is_redacted(manifest) {
        "the auditor's checks (integrity class · the disclosure paths)"
    } else {
        "the auditor's three commands"
    };
    let _ = write!(out, "\n  VERIFY.md — {verify_word}");
    out
}

/// The manifest's disclosure class — an absent `redaction` section is a
/// pre-T9 manifest, which shipped the exact bytes: that IS the full class.
fn is_redacted(manifest: &Value) -> bool {
    manifest["redaction"]["class"].as_str() == Some("redacted")
}

/// The one-line journal truth: the redacted class names its placeholder
/// count; the full class wears the content warning.
fn journal_word(manifest: &Value) -> String {
    if is_redacted(manifest) {
        let n = manifest["redaction"]["placeholders"].as_u64().unwrap_or(0);
        format!("redacted projection ({n} payload fields → sha256)")
    } else {
        "FULL CONTENT (the exact bytes — handle as sensitive)".to_owned()
    }
}

/// The one-line seal truth for the summary.
fn seal_word(manifest: &Value) -> String {
    if manifest["seal"]["present"] != json!(true) {
        return "seal absent (unsigned — tamper-evident chain only)".to_owned();
    }
    match manifest["seal"]["verifies"].as_bool() {
        Some(true) => format!(
            "seal verified (key {})",
            clip(manifest["seal"]["key_id"].as_str().unwrap_or("?"))
        ),
        Some(false) => "seal DOES NOT VERIFY — treat the pack as forged".to_owned(),
        None => "seal ungraded (key not enrolled — VERIFY.md step 2)".to_owned(),
    }
}

/// The one-line boundary truth for the summary.
fn boundary_word(manifest: &Value) -> String {
    if manifest["boundary"].is_null() {
        "boundary unavailable".to_owned()
    } else if manifest["boundary"]["declared"] == json!(true) {
        format!(
            "boundary declared ({})",
            manifest["boundary"]["source"].as_str().unwrap_or("?")
        )
    } else {
        "no boundary declared".to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A staged journal + the manifest the computation folds from it —
    /// the shell renders the summary, the manifest owns the detail.
    fn manifest(sealed: Option<bool>, boundary: &Value) -> Value {
        let seal = match sealed {
            None => json!({ "present": false }),
            Some(verifies) => json!({
                "present": true,
                "key_id": "789d23f0644ddae8",
                "verifies": verifies,
            }),
        };
        json!({
            "evidence_format": 1,
            "trace": { "chain": "intact", "head": "437fe3775b5e7083ae6e037bc8d3bb81775cc2852bac01d3fb64ae4e0d1016a1" },
            "seal": seal,
            "boundary": boundary,
            "receipt": { "present": false },
        })
    }

    /// The summary speaks the seal tier plainly: verified names the
    /// key, unsigned says tamper-evident only, ungraded points at the
    /// enrolment step, a non-verify screams.
    #[test]
    fn summary_speaks_every_seal_tier() {
        let dir = Path::new("/tmp/pack.evidence");
        let declared = json!({ "declared": true, "source": "journal" });

        let verified = human_summary(dir, &manifest(Some(true), &declared));
        assert!(
            verified.contains("seal verified (key 789d23f0644d…)"),
            "{verified}"
        );
        assert!(
            verified.contains("boundary declared (journal)"),
            "{verified}"
        );
        assert!(verified.contains("head 437fe3775b5e…"), "{verified}");

        let unsigned = human_summary(dir, &manifest(None, &declared));
        assert!(
            unsigned.contains("unsigned — tamper-evident chain only"),
            "{unsigned}"
        );

        let ungraded = human_summary(dir, &manifest(Some(false), &declared));
        assert!(ungraded.contains("DOES NOT VERIFY"), "{ungraded}");

        let null_grade = human_summary(
            dir,
            &json!({
                "trace": { "chain": "intact", "head": Value::Null },
                "seal": { "present": true, "key_id": "ab", "verifies": Value::Null },
                "boundary": Value::Null,
                "receipt": { "present": false },
            }),
        );
        assert!(null_grade.contains("key not enrolled"), "{null_grade}");
        assert!(null_grade.contains("boundary unavailable"), "{null_grade}");
        assert!(null_grade.contains("no head"), "{null_grade}");
    }

    /// The receipt line rides only when the manifest carries one.
    #[test]
    fn summary_mentions_the_receipt_only_when_present() {
        let dir = Path::new("/tmp/pack.evidence");
        let mut m = manifest(None, &Value::Null);
        m["receipt"] = json!({ "present": true, "proves": "25f10a4ad96f4952a872999e6a097b56" });
        let out = human_summary(dir, &m);
        assert!(out.contains("receipt.json — proves 25f10a4ad96f…"), "{out}");
        let without = human_summary(dir, &manifest(None, &Value::Null));
        assert!(!without.contains("receipt.json — proves"), "{without}");
    }

    /// A pack error maps to the environment class with its text
    /// verbatim (never a panic, never a swallowed reason).
    #[test]
    fn a_missing_journal_is_env_class() {
        let out = export("/nonexistent/trace.ndjson", None, None, false, false);
        assert_eq!(out.code, super::super::exit::ENV);
        assert!(out.text.contains("cannot read"), "{}", out.text);
    }

    /// The summary names the disclosure class plainly: the redacted
    /// default counts its placeholders and retitles VERIFY.md; the
    /// full class wears the content warning — and a pre-T9 manifest
    /// (no `redaction` section) reads as what it was: full bytes.
    #[test]
    fn summary_speaks_the_disclosure_class() {
        let dir = Path::new("/tmp/pack.evidence");

        let mut redacted = manifest(None, &Value::Null);
        redacted["redaction"] = json!({ "class": "redacted", "placeholders": 3 });
        let out = human_summary(dir, &redacted);
        assert!(
            out.contains("journal.ndjson — redacted projection (3 payload fields → sha256)"),
            "{out}"
        );
        assert!(
            out.contains("VERIFY.md — the auditor's checks (integrity class"),
            "{out}"
        );

        let mut full = manifest(None, &Value::Null);
        full["redaction"] = json!({ "class": "full" });
        let out = human_summary(dir, &full);
        assert!(out.contains("FULL CONTENT (the exact bytes"), "{out}");
        assert!(
            out.contains("VERIFY.md — the auditor's three commands"),
            "{out}"
        );

        let legacy = human_summary(dir, &manifest(None, &Value::Null));
        assert!(
            legacy.contains("FULL CONTENT"),
            "a pre-T9 manifest shipped the exact bytes: {legacy}"
        );
    }
}
