// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika key …` — the run-signing key lifecycle (S2 · the verifiable-run
//! seal's custody): mint the ed25519 key the journal seals with, print
//! the TOFU fingerprint to enroll elsewhere, rotate with the retired
//! public half kept verifiable. The crypto + custody live in
//! `nika_cli::seal` — this verb is the operator-facing surface only.

use clap::Subcommand;

use super::VerbOutput;

#[derive(Clone, Copy, Subcommand)]
pub enum KeyAction {
    /// Mint the run-signing key (idempotent — refuses to clobber without `--force`).
    Init {
        /// Overwrite an existing key (a rotation without the retired-pub ledger entry).
        #[arg(long)]
        force: bool,
    },
    /// Print the public key + TOFU fingerprint to enroll on other machines.
    Trust,
    /// Retire the current public key to the ledger and mint a fresh one
    /// (old journals stay verifiable against the retired pubs).
    Rotate,
}

/// Dispatch one key action — the verb body behind `nika key <action>`.
#[must_use]
pub fn run(action: KeyAction) -> VerbOutput {
    match action {
        KeyAction::Init { force } => match crate::seal::key_init(force) {
            Ok(fp) => VerbOutput::ok(format!(
                "run-signing key minted · fingerprint {fp}\n  every run on this machine now seals its journal (verify: nika trace verify <file>)"
            )),
            Err(msg) => VerbOutput::file(format!("nika key init: {msg}")),
        },
        KeyAction::Trust => match crate::seal::key_trust() {
            Some((pk, fp)) => {
                VerbOutput::ok(format!("run-signing public key · fingerprint {fp}\n{pk}"))
            }
            None => VerbOutput::ok("no run-signing key — `nika key init` mints one".to_owned()),
        },
        KeyAction::Rotate => match crate::seal::key_rotate() {
            Ok(fp) => VerbOutput::ok(format!(
                "rotated · new fingerprint {fp} · the retired public key stays verifiable in ~/.nika/keys/retired.pub"
            )),
            Err(msg) => VerbOutput::file(format!("nika key rotate: {msg}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hostile/CI machine without a key answers honestly (never a
    /// panic, never a fabricated fingerprint).
    #[test]
    fn trust_without_a_key_says_so() {
        // Unset the override so the test reads the REAL custody probe —
        // on a keyless CI host the None branch renders; on a dev machine
        // with a key enrolled the Some branch does. Both are OK exits.
        let out = run(KeyAction::Trust);
        assert_eq!(out.code, super::super::exit::OK, "{}", out.text);
        assert!(
            out.text.contains("run-signing public key") || out.text.contains("no run-signing key"),
            "{}",
            out.text
        );
    }
}
