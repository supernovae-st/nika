// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This traversal's WHOLE JOB is to ask the
// real `nika-cli` binary whether a declared verb is reachable through
// argv — the same carve-out `bin_smoke.rs` documents, for the same
// reason: routing through the kernel seam would test the lib, and the
// v0.106.0 class this catches lives in the dispatcher, not the lib.
#![allow(clippy::disallowed_types)]

//! The verb FAMILY traversal — every declared verb answers, hidden ones
//! included.
//!
//! G3 (`wiring-g3-unprobed`) counts 25 verb declarations with no probe.
//! Most are `hide = true`, so `--help` never lists them and a human sweep
//! cannot see them: the only honest source is the `enum Command` the
//! prober itself reads, and this test derives from the same place so the
//! two can never disagree about what the family IS.
//!
//! The class this catches is on the record in `main.rs` — the v0.106.0
//! post-mortem, where `nika lsp` refused a flag its hosts always pass and
//! exited 2 *before the first byte of JSON-RPC*. The language server had
//! never once run in production. A verb that cannot be invoked is not a
//! verb, and nothing was watching that.
//!
//! **This axis is narrow, by construction.** `--help` returning 0 proves
//! clap accepts the verb and the binary dispatches it. It proves nothing
//! about what the verb then does: `nika lsp --help` is green on the same
//! binary that accepts `--clientProcessId` and throws it away. Read a pass
//! here as "reachable", never as "works".

use std::process::Command;

/// Verbs that must be in any honest derivation — the anti-vacuous floor.
/// If the `enum Command` scan silently returns nothing, every loop below
/// passes over an empty set and proves the harness instead of the CLI.
const CORE: &[&str] = &[
    "catalog", "check", "doctor", "lsp", "mcp", "new", "run", "test",
];

/// `PascalCase` → kebab, the same shape the prober's `pascal_to_kebab` uses.
fn kebab(variant: &str) -> String {
    let mut out = String::with_capacity(variant.len() + 4);
    for (i, c) in variant.char_indices() {
        if c.is_ascii_uppercase() && i != 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Every `enum Command` variant, kebabed — derived from the source the
/// wiring prober reads, never from `--help` (which hides most of them).
fn declared_verbs() -> Vec<String> {
    let main_rs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let src = std::fs::read_to_string(&main_rs)
        .unwrap_or_else(|e| panic!("read {}: {e}", main_rs.display()));

    let mut out: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut inside = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed.starts_with("enum Command");
            continue;
        }
        if trimmed == "}" && depth == 0 {
            break;
        }
        let opens = i32::try_from(line.matches('{').count()).unwrap_or(0);
        let closes = i32::try_from(line.matches('}').count()).unwrap_or(0);
        let was = depth;
        depth += opens - closes;
        // Only the enum's own level declares variants; a variant's inline
        // struct body is full of fields that also start uppercase.
        if was != 0 || trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }
        let name: String = trimmed
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        let leads_upper = name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        // `Lsp {` and `Dap,` and `Guard(GuardArgs)` are all variants; the
        // space before a struct-variant brace is why a first cut of this
        // scan found 11 of 25 and the floor test below caught it.
        let tail = trimmed[name.len()..].trim_start();
        if leads_upper && (tail.is_empty() || tail.starts_with(['{', '(', ','])) {
            let verb = kebab(&name);
            if !out.contains(&verb) {
                out.push(verb);
            }
        }
    }
    out
}

#[test]
fn the_derivation_finds_the_family_it_claims_to_walk() {
    let verbs = declared_verbs();
    for want in CORE {
        assert!(
            verbs.iter().any(|v| v == want),
            "the `enum Command` scan lost `{want}` — the parse broke, and a traversal \
             over a broken parse is a green that measured nothing.\nfound: {verbs:?}"
        );
    }
    assert!(
        verbs.len() >= CORE.len() * 2,
        "only {} verbs derived — the family is larger than that; the scan is truncating.\n{verbs:?}",
        verbs.len()
    );
}

#[test]
fn every_declared_verb_is_reachable_in_the_shipped_dispatcher() {
    // The v0.106.0 shape: a verb that exits before it speaks. Asked of the
    // REAL binary (CARGO_BIN_EXE), not of a library call — a verb can be
    // implemented and still be unreachable through argv, which is exactly
    // what happened to `lsp`.
    let verbs = declared_verbs();
    let mut broken: Vec<(String, Option<i32>)> = Vec::new();

    for verb in &verbs {
        let out = Command::new(env!("CARGO_BIN_EXE_nika"))
            .args([verb.as_str(), "--help"])
            .output()
            .unwrap_or_else(|e| panic!("spawn `{verb} --help`: {e}"));
        if !out.status.success() || out.stdout.is_empty() {
            broken.push((verb.clone(), out.status.code()));
        }
    }

    assert!(
        broken.is_empty(),
        "{} of {} declared verbs do not answer `--help`: {broken:?}\n\
         A verb clap declares and the dispatcher cannot reach is the v0.106.0 class — \
         the flag refusal that kept the language server from ever running.",
        broken.len(),
        verbs.len()
    );
}

#[test]
fn an_undeclared_verb_is_still_refused() {
    // The negative end. Without it, a dispatcher that answered `--help`
    // for ANY argv would pass the traversal above.
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["definitely-not-a-verb", "--help"])
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "an unknown verb still fails: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
