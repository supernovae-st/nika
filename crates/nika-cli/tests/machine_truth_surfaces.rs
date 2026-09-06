// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// bin_smoke.rs: the contract under test IS the rendered binary surface.
#![allow(clippy::disallowed_types)]

//! The transversal machine-truth pin (RAMS-12) — reads the three
//! RENDERED surfaces (welcome · doctor · catalog) from the REAL binary
//! and pins every provider number to its named facet in
//! [`nika_cli_host::machine_truth::MachineTruth`].
//!
//! Regression gate for A-06: welcome said « 15 providers » · catalog
//! « 38 providers » · doctor listed 10 cloud rows — one bare word,
//! three facets, read as a contradiction by two personas (Dmitri ·
//! Yuki). The numbers may move with the build; what this test makes
//! impossible is a surface whose number stops matching its facet, or
//! two surfaces disagreeing on the SAME facet.

use std::process::Command;

use nika_cli_host::machine_truth::MachineTruth;
use nika_providers::ProviderRegistry;

/// The expected facets, derived through the same seam the renders use.
/// Counts are build facts (the registry's provider set is canonical);
/// env overrides move endpoints, never arity.
fn expected() -> MachineTruth {
    let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    MachineTruth::from_registry(&registry)
}

/// Run one verb on a fresh machine: cleared env (no keys · no
/// overrides · no user config), a scratch HOME, a scratch cwd — the
/// render a first-run user reads. Piped stdout keeps the plain theme,
/// and we strip ANSI defensively so a themed future cannot rot the
/// parse silently.
fn surface(scratch: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .env_clear()
        .env("HOME", scratch)
        .env("TERM", "dumb")
        .env("COLUMNS", "100")
        .current_dir(scratch)
        .output()
        .expect("binary runs");
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    strip_ansi(&text)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI … final byte in @-~ — skip the whole sequence.
            if chars.peek() == Some(&'[') {
                for f in chars.by_ref() {
                    if ('@'..='~').contains(&f) && f != '[' {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// First integer inside a string segment, or None.
fn leading_number(s: &str) -> Option<usize> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[test]
fn welcome_speaks_the_wired_facet() {
    let scratch = scratch_dir("mt-welcome");
    let text = surface(&scratch, &["welcome"]);
    let stats = text
        .lines()
        .find(|l| l.contains(" builtins · ") && l.contains("providers"))
        .expect("welcome renders the this-binary stats line");
    let providers_seg = stats
        .split('·')
        .find(|seg| seg.contains("providers"))
        .expect("stats line carries a providers segment");
    assert_eq!(
        leading_number(providers_seg),
        Some(expected().wired),
        "welcome's bare provider count IS the wired facet — the number \
         a first-run user compares against catalog's header. Line: {stats}"
    );
}

/// #1398 — the check refusal for a cataloged-but-unwired provider spoke
/// its own count (« 16 runnable ») while the card said 15 and the canon
/// 17. It now speaks the wired facet, the same number as the card and
/// the catalog header, split the same way.
#[test]
fn check_refusal_speaks_the_wired_facet() {
    let scratch = scratch_dir("mt-check");
    let file = scratch.join("azure.nika.yaml");
    std::fs::write(
        &file,
        "nika: azure-seat\nmodel: azure/gpt-4o\npermits: {}\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 5 }\n",
    )
    .expect("plant");
    let text = surface(&scratch, &["check", "azure.nika.yaml"]);
    let line = text
        .lines()
        .find(|l| l.contains("wired in this build"))
        .expect("the refusal speaks the wired facet");
    let facet = line
        .split("wired in this build")
        .next()
        .and_then(|head| head.trim_end().rsplit(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse::<usize>().ok());
    assert_eq!(
        facet,
        Some(expected().wired),
        "the refusal's number IS the wired facet the card prints. Line: {line}"
    );
}

#[test]
fn doctor_rows_are_the_wired_and_key_slot_facets() {
    let scratch = scratch_dir("mt-doctor");
    let truth = expected();

    // B-8b (a healthy machine reads calm): the default render folds the
    // advisory rows (unwired agents · unconfigured providers · the
    // config-less default) into ONE advisory line — the facet rows now
    // live under --verbose, and the fold line carries the SAME counts.
    // A-06's law survives the fold: no surface may disagree with itself
    // on a facet — the folded number IS the verbose row count.
    let loud = surface(&scratch, &["doctor", "--verbose"]);

    // The cloud rows: one line per key-taking provider, label `provider`.
    let cloud_rows = count_label(&loud, "provider");
    assert_eq!(
        cloud_rows, truth.cloud_key_slots,
        "doctor --verbose renders exactly one row per cloud key slot"
    );

    // The local summary line: `local  N providers (…)` — an Ok row,
    // never folded, present on BOTH lanes.
    let local_line = loud
        .lines()
        .find(|l| label_is(l, "local"))
        .expect("doctor renders the local providers line");
    let local_count = leading_number(local_line).expect("local line carries a count");
    assert_eq!(
        local_count + cloud_rows,
        truth.wired,
        "doctor's local count + cloud rows IS the wired facet. Line: {local_line}"
    );

    // The calm default: the unconfigured provider rows are gone, folded
    // into ONE advisory line that names the class WITH its count — the
    // same facet, folded, never dropped.
    let calm = surface(&scratch, &["doctor"]);
    assert_eq!(
        count_label(&calm, "provider"),
        0,
        "the calm default folds every unconfigured provider row:\n{calm}"
    );
    let fold = calm
        .lines()
        .find(|l| label_is(l, "advisory"))
        .expect("the calm default carries the advisory fold line");
    assert!(
        fold.contains(&format!("{} providers unconfigured", truth.cloud_key_slots)),
        "the fold names the same facet count. Line: {fold}"
    );
}

/// Count the rendered rows whose label cell is `label` (the fixed
/// `LABEL_COL` grid: glyph · label · detail).
fn count_label(text: &str, label: &str) -> usize {
    text.lines().filter(|l| label_is(l, label)).count()
}

/// Whether a rendered doctor row carries `label` in its label cell.
fn label_is(line: &str, label: &str) -> bool {
    let mut t = line.split_whitespace();
    let _mark = t.next();
    t.next() == Some(label)
}

#[test]
fn catalog_names_all_three_facets() {
    let scratch = scratch_dir("mt-catalog");
    let text = surface(&scratch, &["catalog"]);
    let truth = expected();

    let header = text
        .lines()
        .find(|l| l.contains("catalog entries"))
        .expect("catalog header names its facet");
    assert_eq!(
        leading_number(header.split('—').nth(1).unwrap_or(header)),
        Some(truth.catalog_entries),
        "catalog header counts the embedded entries. Line: {header}"
    );

    let facet_line = text
        .lines()
        .find(|l| l.contains("wired in this build"))
        .expect("catalog names the wired facet under its header");
    assert_eq!(
        leading_number(facet_line),
        Some(truth.wired),
        "catalog's wired count matches the registry facet. Line: {facet_line}"
    );
    let key_seg = facet_line
        .split('·')
        .find(|seg| seg.contains("take a key"))
        .expect("catalog names the key-slot facet");
    assert_eq!(
        leading_number(key_seg),
        Some(truth.cloud_key_slots),
        "catalog's key-slot count matches the doctor's cloud rows. Line: {facet_line}"
    );
}
