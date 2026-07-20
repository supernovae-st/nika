// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika sign <workflow.nika.yaml>` — author-binding: ONE detached
//! minisign over the EXACT bytes → `<file>.minisig` (the workflow itself
//! never changes; v1 signs raw bytes · canonical-YAML is a later wave).

use super::VerbOutput;
use clap::Args;

#[derive(Args)]
pub struct SignArgs {
    /// Workflow file (`*.nika.yaml`) to sign — or to verify with `--check`.
    pub file: String,
    /// Verify the `<file>.minisig` sidecar instead of minting it
    /// (exits: 0 valid · 2 FILE invalid/forged · 3 ENV missing/none).
    #[arg(long)]
    pub check: bool,
}

#[must_use]
pub fn run(args: &SignArgs) -> VerbOutput {
    use crate::seal::WorkflowSig as Ws;
    if args.check {
        return match crate::seal::check_workflow(std::path::Path::new(&args.file)) {
            Ws::Valid(fp) => VerbOutput::ok(format!("valid signature · key {fp}")),
            Ws::Invalid(why) => VerbOutput::file(format!("INVALID signature · {why}")),
            Ws::MissingSidecar => VerbOutput::env("no sidecar — `nika sign` mints one".to_owned()),
            Ws::NoEnrolledKey => VerbOutput::env("no enrolled key — `nika key init`".to_owned()),
        };
    }
    let Some((sk, pk_box)) = crate::seal::load_signing_key() else {
        return VerbOutput::env("no run-signing key — `nika key init` mints one".to_owned());
    };
    match crate::seal::sign_workflow_with(std::path::Path::new(&args.file), &sk, &pk_box) {
        Ok(fp) => VerbOutput::ok(format!("signed {} · key {fp}", args.file)),
        Err(msg) => VerbOutput::env(format!("nika sign: {msg}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(file: &std::path::Path, check: bool) -> SignArgs {
        SignArgs {
            file: file.to_string_lossy().into_owned(),
            check,
        }
    }

    /// `sign --check` without a sidecar is the ENV class (3) — the
    /// deterministic branch (no custody read happens before it).
    #[test]
    fn check_without_a_sidecar_is_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, "nika: v1\n").expect("fixture");
        let out = run(&args(&wf, true));
        assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
        assert!(out.text.contains("no sidecar"), "{}", out.text);
    }

    /// A garbage sidecar is the FILE class (2) — invalid, never valid,
    /// regardless of the machine's custody.
    #[test]
    fn check_a_garbage_sidecar_is_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, "nika: v1\n").expect("fixture");
        std::fs::write(dir.path().join("flow.nika.yaml.minisig"), "garbage\n").expect("sidecar");
        let out = run(&args(&wf, true));
        assert_eq!(out.code, super::super::exit::FILE, "{}", out.text);
        assert!(out.text.contains("INVALID signature"), "{}", out.text);
    }

    /// The sign half answers honestly about custody: on a keyless
    /// machine it is the ENV class naming `nika key init`; on a machine
    /// with an enrolled key it mints the sidecar and says so (the
    /// tolerant two-branch idiom the `key` verb's test already runs).
    #[test]
    fn sign_either_mints_the_sidecar_or_names_the_fix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wf = dir.path().join("flow.nika.yaml");
        std::fs::write(&wf, "nika: v1\n").expect("fixture");
        let out = run(&args(&wf, false));
        if out.code == super::super::exit::OK {
            assert!(dir.path().join("flow.nika.yaml.minisig").exists());
        } else {
            assert_eq!(out.code, super::super::exit::ENV, "{}", out.text);
            assert!(out.text.contains("key"), "{}", out.text);
        }
    }
}
