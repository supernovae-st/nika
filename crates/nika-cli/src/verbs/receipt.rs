// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace receipt` — the receipt surface (F-P16 · NEP-0014 law 3
//! · the dossier doors live under `trace` since RAMS-15).
//!
//! `explain` renders a receipt's readable projection as stable text: a
//! READING, never a proof — the digest is never recomputed here
//! (`verify` owns the proof, the two never share a trust level). The
//! projection registry and the schema ratchet live in
//! [`nika_runtime::proof::receipt`]; this shell owns the verb seam (the
//! bounded read + the exit codes).

use std::path::PathBuf;

use super::VerbOutput;

/// The clap surface of `nika trace receipt` (the `verbs::key::KeyAction`
/// precedent — main.rs stays under the file cap).
#[derive(clap::Subcommand)]
pub enum ReceiptAction {
    /// Render a receipt's readable projection (stable text · a READING,
    /// never a proof — `verify` owns the proof).
    Explain {
        /// The receipt JSON file (an evidence pack's `receipt.json`).
        file: PathBuf,
    },
}

/// A receipt is a small artifact — the read stays bounded anyway (the
/// NEP-0012 law-1 posture: bound before read, never trust the size).
const MAX_RECEIPT_BYTES: u64 = 1 << 20; // 1 MiB

/// `nika trace receipt explain <file>` — the stable readable projection.
#[must_use]
pub fn explain(file: &std::path::Path) -> VerbOutput {
    let label = file.display().to_string();
    let Ok(meta) = std::fs::metadata(file) else {
        return VerbOutput::env(format!("cannot read {label}"));
    };
    if meta.len() > MAX_RECEIPT_BYTES {
        return VerbOutput::env(format!(
            "{label}: {} bytes — over the receipt bound ({MAX_RECEIPT_BYTES} bytes)",
            meta.len()
        ));
    }
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => return VerbOutput::env(format!("cannot read {label}: {e}")),
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(receipt) => VerbOutput::ok(nika_runtime::proof::receipt::explain_receipt(&receipt)),
        Err(e) => VerbOutput::env(format!("{label}: not a JSON receipt ({e})")),
    }
}

/// The verb dispatch.
#[must_use]
pub fn run(action: ReceiptAction) -> VerbOutput {
    match action {
        ReceiptAction::Explain { file } => explain(&file),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A staged receipt — the engine's own fold, so the explain render
    /// exercises the real projection (never a hand-minted shape).
    /// Unique-per-test staging (parallel tests share one process — the
    /// `evidence.rs` precedent: the test name namespaces the dir, so no
    /// sibling's cleanup can race this one's read).
    fn staged(test: &str, name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-receipt-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("staged");
        path
    }

    /// POSITIVE — a real receipt renders the stable projection (the
    /// reading-not-proof header · the folded fields · the projections).
    #[test]
    fn explain_renders_a_real_receipt() {
        let receipt = nika_runtime::proof::receipt::build_receipt(
            "blake3:fixedsem",
            serde_json::json!({"attempts": 1}),
            serde_json::json!({"outcome": "success"}),
            vec![serde_json::json!({"assert": "no_secret_egress", "level": "StaticProof"})],
            "blake3:lock",
        );
        let file = staged(
            "render",
            "receipt.json",
            &serde_json::to_string_pretty(&receipt).expect("serializes"),
        );
        let out = explain(&file);
        assert_eq!(out.code, super::super::exit::OK);
        assert!(
            out.text
                .contains("a READING, never a proof — the digest is NOT recomputed here"),
            "{}",
            out.text
        );
        assert!(out.text.contains("blake3:fixedsem"), "{}", out.text);
        assert!(out.text.contains("no_secret_egress"), "{}", out.text);
        let _ = std::fs::remove_dir_all(file.parent().expect("parent"));
    }

    /// NEGATIVE — an unreadable path and a non-JSON body are the
    /// environment class, named (never a panic).
    #[test]
    fn explain_refuses_unreadable_and_non_json() {
        let missing = explain(std::path::Path::new("/nonexistent/receipt.json"));
        assert_eq!(missing.code, super::super::exit::ENV);
        assert!(missing.text.contains("cannot read"), "{}", missing.text);

        let file = staged("refuse", "broken.json", "{not json");
        let out = explain(&file);
        assert_eq!(out.code, super::super::exit::ENV);
        assert!(out.text.contains("not a JSON receipt"), "{}", out.text);
        let _ = std::fs::remove_dir_all(file.parent().expect("parent"));
    }
}
