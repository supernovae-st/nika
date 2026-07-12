// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE live-Hub pin (`#[ignore]` — network + ~100 MiB · run it with
//! `cargo test -p nika-models --test live_hub -- --ignored`).
//!
//! The mocked suite proves the logic; THIS proves the couple: the real
//! Hub's tree API shape, the raw download host, the redirect chain and
//! the Range/206 resume behavior are third-party surfaces that move
//! without notice — when they do, this test names the break before a
//! user does (first proven live 2026-07-12: pull → one-dir layout →
//! missing-tokenizer note → resume at 56.7 MiB → rm sweep).
//!
//! One test on purpose (the essential-only law): every leg rides the
//! same pulled bytes — five downloads would prove nothing more.

// A test binary panics by design — the house test exemption.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

/// The corpus repo: bartowski's smallest instruct GGUF (~100 MiB
/// `Q4_K_M` · no `tokenizer.json` — which ALSO pins the advisory note).
const REPO: &str = "bartowski/SmolLM2-135M-Instruct-GGUF";
const GGUF: &str = "SmolLM2-135M-Instruct-Q4_K_M.gguf";

fn repo_dir() -> PathBuf {
    nika_models::store::models_root()
        .expect("HOME resolves on a dev machine")
        .join("bartowski")
        .join("SmolLM2-135M-Instruct-GGUF")
}

/// Best-effort cleanup — the test must be re-runnable and leave no
/// hundred-megabyte residue behind (pass or fail).
fn cleanup() {
    let _ = std::fs::remove_dir_all(repo_dir());
}

#[test]
#[ignore = "network + ~100 MiB — the live Hub couple; run explicitly with --ignored"]
fn the_hub_couple_holds_end_to_end() {
    cleanup();

    // ── 1 · pull: default quant resolves · size prints BEFORE bytes ──
    let receipt = nika_models::pull::run(REPO, true).unwrap_or_else(|e| {
        cleanup();
        panic!("live pull refused: {e}");
    });
    assert!(receipt.contains("pulled"), "{receipt}");
    assert!(
        receipt.contains("no tokenizer.json"),
        "this corpus repo ships none — the advisory note is part of the contract: {receipt}"
    );

    // ── 2 · one-dir law: the file is where list/rm/serve look ──
    let gguf = repo_dir().join(GGUF);
    assert!(gguf.is_file(), "the one canonical dir holds the GGUF");
    let full_len = gguf.metadata().map(|m| m.len()).unwrap_or(0);
    assert!(
        full_len > 50 * 1024 * 1024,
        "a real model, not an error page"
    );

    // ── 3 · resume: truncate to a prefix as the interrupted .part ·
    //        the re-pull must APPEND (Range/206), never restart ──
    let part = repo_dir().join(format!("{GGUF}.part"));
    let bytes = std::fs::read(&gguf).unwrap_or_else(|e| {
        cleanup();
        panic!("read back: {e}");
    });
    let half = bytes.len() / 2;
    if std::fs::write(&part, &bytes[..half]).is_err() {
        cleanup();
        panic!("staging the .part failed");
    }
    let _ = std::fs::remove_file(&gguf);
    let receipt = nika_models::pull::run(REPO, true).unwrap_or_else(|e| {
        cleanup();
        panic!("resume pull refused: {e}");
    });
    assert!(
        receipt.contains("resumed at"),
        "the receipt must speak the resume (a resumed pull must not read \
         like a fresh one): {receipt}"
    );
    let resumed_len = gguf.metadata().map(|m| m.len()).unwrap_or(0);
    assert_eq!(resumed_len, full_len, "the resumed file is byte-complete");

    // ── 4 · leave the machine as found ──
    cleanup();
    assert!(!repo_dir().exists(), "no residue");
}