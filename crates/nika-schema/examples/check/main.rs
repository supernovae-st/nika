// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the static pre-flight, runnable today.
//!
//! Usage: `cargo run -p nika-schema --example check -- [--json] [--infer-permits] [--color=auto|always|never] workflow.nika.yaml`
//!
//! Renders the plan as a DAG in wave lanes, the cost envelope, secret
//! leaks, type/tool/schema findings and capability escapes — with ZERO
//! API calls and zero tokens spent. Colour is semantic-only through the
//! one theme seam (CLI presentation canon · nika-cli display contract
//! glyph grammar); `--json` emits the machine-readable report (the
//! agent repair-loop surface) and is never coloured. The polished
//! surface ships with `nika-cli` (step 19); this example IS the seam,
//! available now.

// A console demo's whole job is printing — same exemption as the
// nika-catalog-verify binary (the established precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]

mod render;
mod theme;

use std::process::ExitCode;

use nika_schema::{FileId, ParseMode, check, infer_permits, parse};

use theme::{ColorFlag, Theme};

fn main() -> ExitCode {
    // Unknown flags are REJECTED — a typo'd mode flag silently degrading
    // to a plain check (exit 0) would let an operator ship a check
    // report as their permits file.
    const USAGE: &str = "usage: check [--json] [--infer-permits] [--color=auto|always|never] [--ascii] <workflow.nika.yaml>";
    let mut infer_mode = false;
    let mut json_mode = false;
    let mut color = ColorFlag::Auto;
    let mut ascii = false;
    let mut path: Option<String> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--infer-permits" => infer_mode = true,
            "--json" => json_mode = true,
            "--ascii" => ascii = true,
            "--color=auto" => color = ColorFlag::Auto,
            "--color=always" => color = ColorFlag::Always,
            "--color=never" => color = ColorFlag::Never,
            flag if flag.starts_with("--") => {
                eprintln!("unknown flag `{flag}`");
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ if path.is_some() => {
                eprintln!("expected exactly one workflow path");
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ => path = Some(arg),
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let yaml = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    let wf = match parse(&yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => wf,
        Err(e) => {
            eprintln!("PARSE ✗  {e}");
            return ExitCode::FAILURE;
        }
    };

    // ── --infer-permits · write the boundary FOR the operator ───────
    if infer_mode {
        let inferred = infer_permits(&wf);
        if json_mode {
            // the agent shape: the paste-ready block + the honesty notes
            let payload = serde_json::json!({
                "permits_yaml": inferred.to_yaml(),
                "notes": inferred.notes,
            });
            println!("{payload:#}");
            return ExitCode::SUCCESS;
        }
        print!("{}", inferred.to_yaml());
        if !inferred.notes.is_empty() {
            println!("\n# review — effects too dynamic to pin statically:");
            for note in &inferred.notes {
                println!("#   · {note}");
            }
        }
        return ExitCode::SUCCESS;
    }

    // INFALLIBLE — conformance violations land in the report (rustc
    // model: one run = maximal information).
    let report = check(&wf);

    // ── --json · the full machine-readable report (agent surface) ───
    // NEVER coloured — the contract bytes are the contract.
    if json_mode {
        let clean = report.is_clean();
        match serde_json::to_value(&report) {
            Ok(mut payload) => {
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
                }
                println!("{payload:#}");
            }
            Err(e) => {
                eprintln!("cannot serialize report: {e}");
                return ExitCode::from(2);
            }
        }
        return if clean {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    // ── the human surface · DAG lanes + semantic sections ───────────
    let t = Theme::from_env(color, ascii);
    print!("{}", render::render(&report, &wf, &path, t));

    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
