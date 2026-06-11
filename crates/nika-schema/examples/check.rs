// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the static pre-flight, runnable today.
//!
//! Usage: `cargo run -p nika-schema --example check -- workflow.nika.yaml`
//!
//! Prints the plan (waves), the worst-case cost ceiling, secret leaks and
//! capability escapes — with ZERO API calls and zero tokens spent. The
//! polished surface ships with `nika-cli` (step 19); this example IS the
//! seam, available now.

// A console demo's whole job is printing — same exemption as the
// nika-catalog-verify binary (the established precedent).
#![allow(
    clippy::disallowed_macros,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines
)]

use std::process::ExitCode;

use nika_schema::{FileId, ParseMode, check, parse};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: check <workflow.nika.yaml>");
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

    let report = match check(&wf) {
        Ok(r) => r,
        Err(errors) => {
            eprintln!("CHECK ✗  {} core-conformance violation(s):", errors.len());
            for e in errors {
                eprintln!("  · {e}");
            }
            return ExitCode::FAILURE;
        }
    };

    // ── plan ────────────────────────────────────────────────────────
    println!("PLAN     {} wave(s)", report.waves.len());
    for (n, wave) in report.waves.iter().enumerate() {
        let ids: Vec<&str> = wave
            .iter()
            .map(|&i| wf.tasks[i].value.id.value.as_str())
            .collect();
        println!("  wave {n} · {}", ids.join(" ∥ "));
    }

    // ── cost ceiling ────────────────────────────────────────────────
    if report.cost.tasks.is_empty() {
        println!("COST     no inference tasks · $0.00");
    } else {
        let bound = if report.cost.has_unbounded {
            "FLOOR (unbounded tasks present)"
        } else {
            "ceiling"
        };
        println!(
            "COST     ${:.4} worst-case {bound}",
            report.cost.bounded_total_usd
        );
        for t in &report.cost.tasks {
            let fanout = if t.iterations > 1 {
                format!(" ×{}", t.iterations)
            } else {
                String::new()
            };
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

    // ── secret leaks ────────────────────────────────────────────────
    if report.secret_leaks.is_empty() {
        println!("SECRETS  no masking-boundary escapes");
    } else {
        for l in &report.secret_leaks {
            println!(
                "SECRETS  ⚠ `secrets.{}` flows into {} (task `{}`) — captured output can re-emit it",
                l.secret, l.sink, l.task
            );
        }
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
