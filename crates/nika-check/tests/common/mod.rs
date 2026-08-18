// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// NIKA_SPEC_DIR is a TEST-HARNESS path override (CI checkout layout) ·
// not a secret — the SecretStore rule targets runtime secret lookup.
#![allow(clippy::disallowed_methods)]
// Each integration test binary compiles this module independently and
// uses a subset of it — per-binary dead_code is expected, not rot.
#![allow(dead_code)]

//! Shared conformance-harness plumbing — the spec-checkout resolver,
//! the `expected.json` contract, the engine runner and the
//! runner-protocol matching rule (`conformance/runner-protocol.md`).

use std::path::{Path, PathBuf};

use nika_check::analyze;
use nika_schema::{FileId, ParseMode, SchemaError, SpecCategory, SpecCode, parse};

/// Resolve the nika-spec checkout (env override · sibling default).
pub(crate) fn spec_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    // CARGO_MANIFEST_DIR = …/02-engineering/repos/engine/crates/nika-check
    // the spec checkout  = …/02-engineering/repos/spec
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../spec")
        .canonicalize()
        .expect("nika-spec checkout missing — set NIKA_SPEC_DIR or clone ../spec")
}

/// In the cargo-mutants SANDBOX (tree copied without the `../spec`
/// sibling) the conformance suites cannot resolve their fixtures BY
/// CONSTRUCTION — skip LOUDLY there only; everywhere else the
/// hard-fail stands (the gate must never silently skip in CI).
///
/// Detection is belt-and-braces: the `CARGO_MUTANTS=1` env var (newer
/// cargo-mutants) OR the sandbox path signature — the tree is copied
/// under a `cargo-mutants-*` temp dir, which `CARGO_MANIFEST_DIR`
/// carries (measured empirically: our installed cargo-mutants does NOT
/// set the env var, so the path signature is the one that fires).
pub(crate) fn skip_in_mutants_sandbox() -> bool {
    if std::env::var_os("NIKA_SPEC_DIR").is_some() {
        return false; // an explicit spec dir always wins
    }
    let env_says = std::env::var_os("CARGO_MUTANTS").is_some_and(|v| v == "1");
    let path_says = std::env::var_os("CARGO_MANIFEST_DIR")
        .is_some_and(|d| d.to_string_lossy().contains("cargo-mutants"));
    if env_says || path_says {
        // test-scope diagnostics: the loud-skip message IS the point
        #[allow(clippy::disallowed_macros, clippy::print_stderr)]
        {
            eprintln!(
                "conformance: skipped in the cargo-mutants sandbox (no ../spec sibling by construction)"
            );
        }
        return true;
    }
    false
}

/// One expected-error entry (`code` XOR `namespace` per protocol).
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ExpectedError {
    pub(crate) code: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) category: Option<String>,
}

/// The `expected.json` contract.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct Expected {
    pub(crate) valid: bool,
    #[serde(default)]
    pub(crate) errors: Vec<ExpectedError>,
    #[serde(default)]
    pub(crate) mode: Option<String>,
    #[serde(default)]
    pub(crate) note: Option<String>,
}

impl Expected {
    /// « default: strict · the test default » per runner-protocol.md.
    pub(crate) fn parse_mode(&self) -> ParseMode {
        match self.mode.as_deref() {
            Some("lenient") => ParseMode::Lenient,
            _ => ParseMode::Strict,
        }
    }
}

/// Run parse + analyze · collect every emitted error.
pub(crate) fn run_engine(yaml: &str, mode: ParseMode) -> Vec<SchemaError> {
    match parse(yaml, FileId::new(0), mode) {
        Ok(wf) => match analyze(&wf) {
            Ok(_) => Vec::new(),
            Err(errors) => errors,
        },
        Err(e) => vec![e],
    }
}

/// Protocol matching · exact `code` OR `namespace`-prefix + `category`.
pub(crate) fn matches_expected(emitted: &SchemaError, expected: &ExpectedError) -> bool {
    matches_code(emitted.spec_code(), expected)
}

/// The same protocol over a bare [`SpecCode`] — the check-only surfaces
/// (builtin args · capability escapes) the analyze tier does not emit as
/// `SchemaError`s.
pub(crate) fn matches_code(spec: SpecCode, expected: &ExpectedError) -> bool {
    let code = spec.to_string();
    if let Some(exact) = &expected.code {
        return &code == exact;
    }
    if let Some(namespace) = &expected.namespace {
        if !code.starts_with(&format!("{namespace}-")) {
            return false;
        }
        if let Some(category) = &expected.category {
            return spec.category.as_str() == category;
        }
        return true;
    }
    false
}

/// The check-only HARD-invalidating surface codes (builtin arg-contract
/// violations · capability escapes) — so a fixture is verdicted against the
/// real `nika check` surface, not just the narrow `analyze` tier (the gap
/// that let `nika:write`-without-`content` / `nika:jq` wrong-arg pass). Only
/// invalidating findings are returned (NOT advisory surfaces — hints ·
/// gate/schema findings — which fire on valid workflows and must not flip a
/// VALID fixture to invalid).
pub(crate) fn check_extra(yaml: &str, mode: ParseMode, dir: &Path) -> Vec<SpecCode> {
    match parse(yaml, FileId::new(0), mode) {
        Ok(wf) => {
            // The COMPOSED lane (spec 14): a fixture's `workflow:` targets
            // resolve against the fixture directory (sibling files) — the
            // same reader shape the CLI injects.
            let root = dir.join("input.yaml").to_string_lossy().into_owned();
            let mut codes = nika_check::check_composed(&wf, &root, &mut |p| {
                std::fs::read_to_string(p).map_err(|e| e.to_string())
            })
            .extra_conformance_codes();
            codes.extend(skills_codes(&wf, dir));
            codes
        }
        // A parse error surfaces through `run_engine` (analyze) as a SchemaError.
        Err(_) => Vec::new(),
    }
}

/// The SKILLS lane (spec 02 §Agent Skills · check≡run): the same
/// `nika_schema::resolve_skills` the CLI's `check` runs beside
/// `check_composed`, with the reader rooted at the fixture directory —
/// the file-relative resolution the spec names. Its findings carry the
/// four codes of the table: AUTH-006 (absent block) · SEC-004 (declared
/// boundary that does not admit the path) · AGENT-003 (missing) ·
/// AGENT-004 (malformed). Until 2026-08-18 the harness never ran this
/// lane, so the CLI refused verbs-shape/023-025 while the suite reported
/// « engine accepted » and the spec pin could not advance.
pub(crate) fn skills_codes(wf: &nika_schema::raw::RawWorkflow, dir: &Path) -> Vec<SpecCode> {
    let resolved = nika_schema::resolve_skills(wf, &mut |p| {
        std::fs::read_to_string(dir.join(p)).map_err(|e| e.to_string())
    });
    resolved
        .findings
        .iter()
        .map(|f| match f.code {
            "NIKA-AUTH-006" => SpecCode::new("AUTH", 6, SpecCategory::SecurityError),
            "NIKA-SEC-004" => SpecCode::new("SEC", 4, SpecCategory::SecurityError),
            "NIKA-AGENT-004" => SpecCode::new("AGENT", 4, SpecCategory::ValidationError),
            // AGENT-003 and any future code the lane mints: the missing
            // file is the default shape (a new code lands here loudly —
            // the fixture names it and the arm below would mislabel it).
            _ => SpecCode::new("AGENT", 3, SpecCategory::ValidationError),
        })
        .collect()
}

/// The CORE-tier check-only codes (`policy:` surface of spec 10 · F-O8's
/// AUTH namespace of NEP-0003 · and spec 14's COMP namespace — law 3/4's
/// « absent child = ∅ » is judged in core/authority/006): the lane lives
/// in `check()` (it reads the derived graph — and the composed read for
/// the child files), and the reference oracle judges these on every tier.
/// Never the deep-only builtin/permits-fit classes, which stay deep
/// concerns.
pub(crate) fn check_core_codes(yaml: &str, mode: ParseMode, dir: &Path) -> Vec<SpecCode> {
    match parse(yaml, FileId::new(0), mode) {
        Ok(wf) => {
            let root = dir.join("input.yaml").to_string_lossy().into_owned();
            let mut codes = nika_check::check_composed(&wf, &root, &mut |p| {
                std::fs::read_to_string(p).map_err(|e| e.to_string())
            })
            .extra_conformance_codes();
            codes.extend(skills_codes(&wf, dir));
            codes
                .into_iter()
                .filter(|c| {
                    matches!(c.namespace, "POLICY" | "AUTH" | "COMP")
                    // 02 §Agent Skills · the skills lane is Core-visible:
                    // its fixtures live in core/verbs-shape (023-026) and
                    // core/authority/030, and the reference oracle judges
                    // it on every tier (deep_static.agent_skills_errors).
                    // Its SEC-004 (a declared boundary that does not admit
                    // the path) stays the deep tier's, like every SEC-004.
                    || c.namespace == "AGENT"
                    // LAW-TEMPORAL-0435 · entropy: none × a live structural
                    // randomness source (NIKA-PARSE-028) is Core-visible:
                    // its fixture is core/envelope/028 and the reference
                    // oracle judges it on every tier · the CLI's RUN rung
                    // refused it all along, the harness never counted it.
                    || (c.namespace == "PARSE" && c.num == 28)
                    // NEP-0006 · the data-as-code sink (NIKA-SEC-008) is a
                    // Core-visible law: the reference oracle judges it on
                    // every tier, so the core verdict must too — the REST
                    // of the SEC namespace (004 escape · 009 trifecta)
                    // stays the deep tier's ground.
                    || (c.namespace == "SEC" && c.num == 8)
                    // NEP-0008 law 5 · the floor-parity dead grant
                    // (NIKA-SEC-005) is Core-visible for the same reason:
                    // the reference oracle judges the net boundary on
                    // every tier (deep_static.py net_egress_boundary_errors),
                    // and the law lives in the authority suite's home tier
                    // (core/authority/026) — a fetch-side SEC-005 emitted
                    // at run stays the deep/runtime ground as before.
                    || (c.namespace == "SEC" && c.num == 5)
                    // NEP-0020 · the affirmative-consent law (NIKA-SEC-014)
                    // is Core-visible for the same reason: the reference
                    // oracle judges it on every tier (deep_static.py
                    // consent_errors) and its fixtures live in
                    // core/policy/ — the human-gate family's home tier.
                    || (c.namespace == "SEC" && c.num == 14)
                    // The unconditional order law (NIKA-SEC-015) is
                    // Core-visible by construction: it is what SURVIVED
                    // the `policy:` death, its fixtures live in
                    // core/order/, and a law no block can disable
                    // belongs to every tier or to none.
                    || (c.namespace == "SEC" && c.num == 15)
                })
                .collect()
        }
        // A parse error (fixture 009's closed-set refusal) surfaces
        // through `run_engine` (analyze) as a SchemaError.
        Err(_) => Vec::new(),
    }
}

/// One fixture's verdict against its `expected.json` (None = conformant).
///
/// `deep` selects the conformance TIER (spec `07-conformance.md` §Levels):
/// - `false` · the **Core** tier (`tests/core/`) — parse · validate · DAG ·
///   variables · errors · the `analyze()` contract a minimal engine implements.
/// - `true` · the **Deep-static** tier (`tests/deep/`) — Core PLUS the
///   builtin-arg contracts + capability-boundary fit the fuller `check()`
///   adds (`nika:write` without `content` · `nika:jq` wrong-arg · a body
///   outside `permits:`). The deep tier verdicts against the real
///   `nika check` surface; the core tier must not (a builtin-arg defect is a
///   deep concern, not a Core-rules one).
pub(crate) fn fixture_verdict(dir: &Path, deep: bool) -> Option<String> {
    let yaml = std::fs::read_to_string(dir.join("input.yaml")).expect("read input.yaml");
    let expected_raw =
        std::fs::read_to_string(dir.join("expected.json")).expect("read expected.json");
    let expected: Expected = serde_json::from_str(&expected_raw).expect("parse expected.json");
    let mode = expected.parse_mode();
    // The Core tier is `analyze()` (rich SchemaErrors) PLUS the policy
    // surface (spec 10 files its fixtures under core/); the Deep tier
    // adds every check-only invalidating surface (builtin args ·
    // capability escapes · policy included via `extra_conformance_codes`).
    let emitted = run_engine(&yaml, mode);
    let extra = if deep {
        check_extra(&yaml, mode, dir)
    } else {
        check_core_codes(&yaml, mode, dir)
    };

    if expected.valid {
        if !emitted.is_empty() || !extra.is_empty() {
            return Some(format!(
                "expected VALID · engine emitted {} analyze error(s) + {} check-only ·\n{}{}",
                emitted.len(),
                extra.len(),
                render(&emitted),
                render_codes(&extra),
            ));
        }
    } else if emitted.is_empty() && extra.is_empty() {
        return Some(format!(
            "expected INVALID ({}) · engine accepted{}",
            render_expected(&expected.errors),
            expected
                .note
                .as_deref()
                .map(|n| format!("\n  note · {n}"))
                .unwrap_or_default(),
        ));
    } else {
        let any_match = emitted
            .iter()
            .any(|e| expected.errors.iter().any(|x| matches_expected(e, x)))
            || extra
                .iter()
                .any(|c| expected.errors.iter().any(|x| matches_code(*c, x)));
        if !any_match {
            return Some(format!(
                "expected one of [{}] · engine emitted ·\n{}{}",
                render_expected(&expected.errors),
                render(&emitted),
                render_codes(&extra),
            ));
        }
    }
    None
}

/// Render the check-only surface codes for a failure diagnostic.
fn render_codes(codes: &[SpecCode]) -> String {
    if codes.is_empty() {
        return String::new();
    }
    let list = codes
        .iter()
        .map(|c| format!("  {} [{}] · (check-only surface)", c, c.category.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n{list}")
}

/// Collect every fixture dir (any depth · a dir containing `input.yaml`).
pub(crate) fn fixture_dirs(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if dir.join("input.yaml").is_file() {
            out.push(dir.to_path_buf());
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            }
        }
    }
    let mut dirs = Vec::new();
    walk(root, &mut dirs);
    dirs.sort();
    assert!(
        !dirs.is_empty(),
        "zero fixtures found under {} — the conformance gate must never be empty",
        root.display()
    );
    dirs
}

/// Render emitted errors with their spec codes for diagnosis.
pub(crate) fn render(errors: &[SchemaError]) -> String {
    errors
        .iter()
        .map(|e| {
            let spec = e.spec_code();
            format!("  {} [{}] · {e}", spec, spec.category.as_str())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render the expected entries compactly.
pub(crate) fn render_expected(expected: &[ExpectedError]) -> String {
    expected
        .iter()
        .map(|x| {
            let id = x
                .code
                .clone()
                .or_else(|| x.namespace.clone())
                .unwrap_or_else(|| "<any>".to_owned());
            match &x.category {
                Some(category) => format!("{id}+{category}"),
                None => id,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
