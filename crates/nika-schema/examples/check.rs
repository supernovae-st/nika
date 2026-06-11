// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the static pre-flight, runnable today.
//!
//! Usage: `cargo run -p nika-schema --example check -- [--json] [--infer-permits] workflow.nika.yaml`
//!
//! Prints the plan (waves), the cost envelope, secret leaks, type/tool/
//! schema findings and capability escapes — with ZERO API calls and zero
//! tokens spent. `--json` emits the full machine-readable report (the
//! agent surface: a generator loop reads findings + their deterministic
//! suggestions, repairs, re-checks until clean). The polished surface
//! ships with `nika-cli` (step 19); this example IS the seam, available
//! now.

// A console demo's whole job is printing — same exemption as the
// nika-catalog-verify binary (the established precedent).
#![allow(
    clippy::disallowed_macros,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines
)]

use std::process::ExitCode;

use nika_schema::{FileId, ParseMode, check, infer_permits, parse};

fn main() -> ExitCode {
    // `--infer-permits` synthesizes the tightest boundary instead of
    // checking. Unknown flags are REJECTED — a typo'd mode flag silently
    // degrading to a plain check (exit 0) would let an operator ship a
    // check report as their permits file.
    const USAGE: &str = "usage: check [--json] [--infer-permits] <workflow.nika.yaml>";
    let mut infer_mode = false;
    let mut json_mode = false;
    let mut path: Option<String> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--infer-permits" => infer_mode = true,
            "--json" => json_mode = true,
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

    let report = match check(&wf) {
        Ok(r) => r,
        Err(errors) => {
            if json_mode {
                let payload = serde_json::json!({
                    "clean": false,
                    "conformance_errors": errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
                });
                println!("{payload:#}");
            } else {
                eprintln!("CHECK ✗  {} core-conformance violation(s):", errors.len());
                for e in errors {
                    eprintln!("  · {e}");
                }
            }
            return ExitCode::FAILURE;
        }
    };

    // ── --json · the full machine-readable report (agent surface) ───
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

    // ── plan ────────────────────────────────────────────────────────
    println!("PLAN     {} wave(s)", report.waves.len());
    for (n, wave) in report.waves.iter().enumerate() {
        let ids: Vec<&str> = wave
            .iter()
            .map(|&i| wf.tasks[i].value.id.value.as_str())
            .collect();
        println!("  wave {n} · {}", ids.join(" ∥ "));
    }

    // ── cost envelope (structural interval · ADR-092 #5) ────────────
    if report.cost.tasks.is_empty() {
        println!("COST     no inference tasks · $0.00");
    } else {
        let bound = if report.cost.has_unbounded {
            "FLOOR (unbounded tasks present)"
        } else {
            "ceiling"
        };
        let spread = report.cost.bounded_total_usd - report.cost.min_path_total_usd;
        if spread > f64::EPSILON {
            println!(
                "COST     ${:.4} – ${:.4} {bound} (cheapest path: gates closed · first try)",
                report.cost.min_path_total_usd, report.cost.bounded_total_usd
            );
        } else {
            println!(
                "COST     ${:.4} worst-case {bound}",
                report.cost.bounded_total_usd
            );
        }
        for t in &report.cost.tasks {
            let iter = if t.iterations > 1 {
                format!(" ×{}", t.iterations)
            } else {
                String::new()
            };
            let retries = if t.attempts > 1 {
                format!(" ×{} retries", t.attempts)
            } else {
                String::new()
            };
            let gate = if t.gated { " (when:-gated)" } else { "" };
            let fanout = format!("{iter}{retries}{gate}");
            match (t.usd, &t.unbounded_reason) {
                (Some(usd), _) => println!(
                    "  {} · {} · ≤{} tokens{fanout} · ${usd:.4}",
                    t.task,
                    t.model.as_deref().unwrap_or("?"),
                    t.max_tokens.unwrap_or(0),
                ),
                (None, reason) => {
                    use nika_schema::check::UnboundedReason as R;
                    let why = match reason {
                        Some(R::NoTokenLimit) => "no max_tokens declared",
                        Some(R::NoPrice) => "no catalog price (local/unknown model)",
                        Some(R::UnknownIterations) => "for_each over an expression (unknown count)",
                        _ => "unbounded",
                    };
                    println!(
                        "  {} · {} · ⚠ UNBOUNDED — {why}",
                        t.task,
                        t.model.as_deref().unwrap_or("?"),
                    );
                }
            }
        }
    }

    // ── secret leaks + egresses (IFC · ADR-092) ─────────────────────
    if report.secret_leaks.is_empty() && report.secret_egresses.is_empty() {
        println!("SECRETS  no information-flow escapes");
    } else {
        for l in &report.secret_leaks {
            println!(
                "SECRETS  ⚠ leak into {} (task `{}`) — {}",
                l.sink, l.task, l.trace
            );
        }
        for e in &report.secret_egresses {
            println!(
                "SECRETS  ⚠ EGRESS via outputs.{} — {} (a secret leaves the run)",
                e.output, e.trace
            );
        }
    }

    // ── dataflow schema typing (ADR-092 #4) ─────────────────────────
    if report.schema_findings.is_empty() {
        println!("TYPES    every deep output reference fits its declared shape");
    } else {
        for f in &report.schema_findings {
            println!(
                "TYPES    ✗ {} (at `{}`) — {}",
                f.reference, f.site, f.detail
            );
        }
    }

    // ── unknown nika: builtins (closed catalog · did-you-mean) ──────
    for t in &report.unknown_tools {
        let fix = t
            .suggestion
            .as_deref()
            .map_or_else(String::new, |s| format!(" — did you mean `{s}`?"));
        println!(
            "TOOLS    ✗ `{}` (task `{}`) is not a canonical builtin{fix}",
            t.tool, t.task
        );
    }

    // ── structured-output schema lint ───────────────────────────────
    for l in &report.schema_lints {
        println!(
            "SCHEMA   ✗ task `{}` at `{}` — {}",
            l.task, l.path, l.detail
        );
    }

    // ── permits ─────────────────────────────────────────────────────
    if wf.permits.is_none() {
        println!("PERMITS  no boundary declared (engine floor only)");
    } else if report.capability_escapes.is_empty() {
        println!("PERMITS  body fits the declared boundary");
    } else {
        for e in &report.capability_escapes {
            println!(
                "PERMITS  ✗ [{}] task `{}` · {}",
                e.category, e.task, e.detail
            );
        }
    }

    if report.is_clean() {
        println!("VERDICT  ✓ clean — audited before a single token was spent");
        ExitCode::SUCCESS
    } else {
        println!("VERDICT  ✗ findings above");
        ExitCode::FAILURE
    }
}
