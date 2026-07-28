// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the ADR-092 static ladder, rendered (spec §2).
//!
//! The human surface: grep-stable section keywords (CONFORM/PLAN/COST/
//! SECRETS/TYPES/TOOLS/SCHEMA/GATES/PERMITS/HINT) through the ONE colour seam
//! (`display::theme` · semantic-only). The machine surface (`--json`):
//! the full [`CheckReport`] + a `clean` flag, NEVER coloured — the
//! contract bytes are the contract. Check is INFALLIBLE past parse
//! (rustc model): every defect lands in the report, one round-trip.

/// The `check` arm's routing: single file = the pre-variadic path,
/// byte-identical (every existing consumer — hooks · agents · CI — sees
/// exactly what it saw before); several files fan out through
/// [`run_many`]. The machine modes stay one-file-per-call —
/// `report_version: 1` and the inferred boundary are per-file contracts —
/// so `--json`/`--infer-permits` with several files refuse with a teach
/// line at exit 3 (the INVOCATION is wrong, no file was judged), and
/// stdin (`-`) cannot join a multi-file audit.
pub struct CheckFlags {
    pub json: bool,
    pub infer_permits: bool,
    pub native_strict: bool,
}

#[must_use]
pub fn dispatch(
    files: &[String],
    flags: &CheckFlags,
    fix: bool,
    model: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let CheckFlags {
        json,
        infer_permits,
        native_strict,
    } = *flags;
    if fix {
        // The repair loop rewrites a file: stdin has nothing to rewrite,
        // --json's report_version is a single immutable audit, several
        // files would interleave rewrites with one summary, and
        // --infer-permits is a different output entirely.
        if json || infer_permits {
            return crate::verbs::fix::refuse(
                "--fix pairs with the plain audit only (not --json / --infer-permits)",
            );
        }
        return match files {
            [file] if file != "-" => crate::verbs::fix::run(file, native_strict, model, theme),
            [_] => {
                crate::verbs::fix::refuse("stdin (`-`) has no file to rewrite — name a real path")
            }
            _ => crate::verbs::fix::refuse(
                "one file per repair loop — loop the files, one --fix per call",
            ),
        };
    }
    if let [file] = files {
        if infer_permits {
            run_infer_permits(file, json)
        } else {
            run(file, json, native_strict, model, theme)
        }
    } else if json || infer_permits {
        VerbOutput {
            text: "check: --json and --infer-permits report ONE file per call \
                   (report_version 1 is a per-file contract)\n  fix: loop the \
                   files, one check per call\n"
                .to_owned(),
            code: crate::verbs::exit::ENV,
        }
    } else if files.iter().any(|f| f == "-") {
        VerbOutput {
            text: "check: stdin (`-`) cannot join a multi-file audit\n  fix: \
                   pipe one call per stream, or name the files\n"
                .to_owned(),
            code: crate::verbs::exit::ENV,
        }
    } else {
        run_many(files, native_strict, model, theme)
    }
}

use std::fmt::Write as _;

use nika_check::CheckReport;
use nika_check::infer_permits;
#[cfg(test)]
use nika_schema::raw::RawWorkflow;

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, load_checked, load_checked_with_source};

mod drift;
mod models_rung;
use models_rung::{ModelFinding, pricing_section, unresolvable_models};

mod render;
use render::render;
#[cfg(test)]
use render::{permits, required_inputs};

/// The `nika check <file>` verb. `native_strict` promotes the advisory
/// `native-first` hints to failures (exit 2) — the agent/CI posture:
/// spec-validity is unchanged, but an `exec:` with a probable native
/// path no longer sails through silently.
#[must_use]
pub fn run(
    path: &str,
    json: bool,
    native_strict: bool,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let (source, wf, report) = match load_checked_with_source(path) {
        Ok(triple) => triple,
        // Parse-fatal + `--json` (#331's papercut): the machine mode
        // stays machine-parseable — ONE JSON error object on stdout
        // (parse_fatal + a findings[] row shaped like the report's own),
        // never the plain-text refusal an agent's json parse chokes on.
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    // `--model m` previews the RUN override's static envelope (#342): the
    // report is recomputed with `m` as the effective envelope default —
    // the same substitution the run's budget preflight prices, so what
    // check shows IS what run will refuse or allow.
    let (wf, report) = match model_override {
        Some(m) => {
            let wf = crate::verbs::with_model_override(&wf, m);
            let report = nika_check::check(&wf);
            (wf, report)
        }
        None => (wf, report),
    };
    // The declared-vs-used drift family (NIKA-DRIFT-001 · drift.rs) —
    // advisory in both projections, never an exit-code input.
    let drift_hints = drift::scan(&wf);
    let native_hints = report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .count();
    // The MODELS rung (#320): the ladder validated TOOLS but not MODELS —
    // the exact asymmetry a hallucinating agent hits. A `model:` this
    // binary cannot resolve is a FINDING (exit 2), never a green audit.
    let model_findings = unresolvable_models(&report);
    // SKILLS rung (#473 · MODELS pattern): a bad SKILL.md is a FINDING.
    let skills = super::resolve_workflow_skills(&wf);
    let clean = report.is_clean() && model_findings.is_empty() && skills.findings.is_empty();
    let strict_clean = clean && (!native_strict || native_hints == 0);

    if json {
        return json_verdict(
            &report,
            &model_findings,
            &skills,
            &drift_hints,
            clean,
            strict_clean,
            native_strict,
        );
    }

    let mut text = render(
        &report,
        &wf,
        &source,
        path,
        theme,
        &model_findings,
        &skills,
        &drift_hints,
    );
    if native_strict && report.is_clean() && native_hints > 0 {
        let hint_word = if native_hints == 1 { "hint" } else { "hints" };
        let _ = writeln!(
            text,
            " {}",
            theme.paint(
                Role::Bad,
                // NOT "or record them in the exec ledger". Measured: a
                // ledgered `.py` wrapper still fails this gate, because
                // the gate judges the SHAPE of the subprocess, not
                // whether it was written down. Offering the ledger as
                // an alternative sends the reader to write one, re-run,
                // and meet the identical red — a diagnostic that costs
                // a cycle and buys nothing. The ledger is for the
                // reviewer; only replacing the call clears this line.
                &format!(
                    "✖ native-strict · {native_hints} native-first {hint_word} above — \
                     replace each one with the builtin its hint names \
                     (the exec ledger documents intent for a reviewer; \
                     it does not clear this gate)"
                ),
            )
        );
    }
    if strict_clean {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// The parse-fatal machine verdict (#331): a file the parser refuses
/// never reaches the report, but a `--json` consumer still gets JSON —
/// `parse_fatal: true`, `clean: false`, and ONE findings[] row carrying
/// the spec code the plain-text voice prints (`PARSE ✗ [CODE] …`). The
/// exit code (2 file · 3 env) rides unchanged.
fn parse_fatal_json(out: &VerbOutput) -> VerbOutput {
    let text = out.text.trim();
    // The plain voice is `PARSE ✗  [NIKA-…] message` — recover the code;
    // an env-class refusal (unreadable file) has no code and stays codeless.
    let code = text
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(code, _)| code.to_owned());
    let message = text.split_once("] ").map_or(text, |(_, m)| m).to_owned();
    let mut finding = serde_json::json!({
        "kind": "parse",
        "gate": "PARSE",
        "severity": "error",
        "message": message,
    });
    if let Some(c) = &code {
        finding["code"] = serde_json::json!(c);
        finding["docs_url"] = serde_json::json!(format!("{}/{c}", nika_check::ERROR_DOCS_BASE));
    }
    let payload = serde_json::json!({
        "report_version": nika_check::REPORT_VERSION,
        "parse_fatal": true,
        "clean": false,
        "findings": [finding],
    });
    VerbOutput {
        text: format!("{payload:#}"),
        code: out.code,
    }
}

/// The `--json` verdict: the full report + the machine keys (`clean` ·
/// `models_resolve` · `model_findings[]` · `skills_resolve` ·
/// `skill_findings[]` · `pricing` · the strict flag) — never coloured,
/// the contract bytes are the contract. The drift rows (NIKA-DRIFT-001)
/// append to `hints[]` in the report's row shape plus their `code`.
fn json_verdict(
    report: &CheckReport,
    model_findings: &[ModelFinding],
    skills: &nika_schema::ResolvedSkills,
    drift_hints: &[String],
    clean: bool,
    strict_clean: bool,
    native_strict: bool,
) -> VerbOutput {
    let mut payload = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => return VerbOutput::env(format!("cannot serialize report: {e}")),
    };
    if let Some(obj) = payload.as_object_mut() {
        if let Some(hints) = obj
            .get_mut("hints")
            .and_then(serde_json::Value::as_array_mut)
        {
            for advice in drift_hints {
                hints.push(serde_json::json!({"kind": "drift", "task": "-", "advice": advice, "code": drift::DRIFT_CODE}));
            }
        }
        obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
        obj.insert(
            "models_resolve".to_owned(),
            serde_json::Value::Bool(model_findings.is_empty()),
        );
        if !model_findings.is_empty() {
            obj.insert(
                "model_findings".to_owned(),
                serde_json::Value::Array(
                    model_findings
                        .iter()
                        .map(|f| {
                            serde_json::json!({
                                "model": f.model,
                                "tasks": f.tasks,
                                "why": f.why,
                            })
                        })
                        .collect(),
                ),
            );
        }
        skills.extend_check_json(obj);
        obj.insert(
            "pricing".to_owned(),
            pricing_section(report, model_findings),
        );
        if native_strict {
            obj.insert(
                "native_strict_clean".to_owned(),
                serde_json::Value::Bool(strict_clean),
            );
        }
    }
    let text = format!("{payload:#}");
    if strict_clean {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// Several files through the same per-file ladder — the pre-commit / CI
/// shape (`nika check a.nika.yaml b.nika.yaml`). Each file gets the FULL
/// [`run`] report (its header names the file), every file still audits
/// after an earlier failure (no stop-at-first — the hook UX law), and the
/// worst spec-§4 exit survives (3 environment > 2 findings). The machine
/// modes stay one-file-per-call — `report_version: 1` is a per-file
/// contract — so `main` refuses `--json`/`--infer-permits` upstream
/// before this is reached.
#[must_use]
pub fn run_many(
    paths: &[String],
    native_strict: bool,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let mut texts = Vec::with_capacity(paths.len());
    let mut worst = crate::verbs::exit::OK;
    for path in paths {
        let out = run(path, false, native_strict, model_override, theme);
        texts.push(out.text);
        worst = worst.max(out.code);
    }
    VerbOutput {
        text: texts.join("\n"),
        code: worst,
    }
}

/// `nika check --infer-permits` — write the boundary FOR the operator.
#[must_use]
pub fn run_infer_permits(path: &str, json: bool) -> VerbOutput {
    let (wf, _report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let inferred = infer_permits(&wf);
    if json {
        let payload = serde_json::json!({
            "permits_yaml": inferred.to_yaml(),
            "notes": inferred.notes,
        });
        return VerbOutput::ok(format!("{payload:#}"));
    }
    let mut text = inferred.to_yaml();
    if !inferred.notes.is_empty() {
        text.push_str("\n# review — effects too dynamic to pin statically:\n");
        for note in &inferred.notes {
            let _ = writeln!(text, "#   · {note}");
        }
    }
    VerbOutput::ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `run_many`: every file audits even after an earlier failure (the
    /// broken file sits in the MIDDLE), each report keeps its own header,
    /// and the worst spec-§4 exit survives.
    #[test]
    fn run_many_audits_every_file_and_keeps_the_worst_exit() {
        let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let clean = "nika: v1\nworkflow:\n  id: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
        let broken = "nika: v1\nworkflow:\n  id: bad\ntasks:\n  t:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 10, model: \"mock/echo\" }\n";
        let a = dir.join("many-a.nika.yaml");
        let b = dir.join("many-broken.nika.yaml");
        let c = dir.join("many-c.nika.yaml");
        std::fs::write(&a, clean).expect("fixture a");
        std::fs::write(&b, broken).expect("fixture b");
        std::fs::write(&c, clean).expect("fixture c");

        let paths: Vec<String> = [&a, &b, &c]
            .iter()
            .map(|p| p.to_str().expect("utf8 path").to_owned())
            .collect();
        let out = run_many(&paths, false, None, Theme::new(false, true, false));

        assert_eq!(out.code, 2, "the broken middle file's exit survives");
        // The report header names its file by BASENAME (`nika check · f`).
        for name in [
            "many-a.nika.yaml",
            "many-broken.nika.yaml",
            "many-c.nika.yaml",
        ] {
            assert!(
                out.text.contains(name),
                "every report present (headers name their file): missing {name}\n{}",
                out.text
            );
        }
        let after = out.text.split_once("many-broken.nika.yaml").map(|s| s.1);
        assert!(
            after.is_some_and(|tail| tail.contains("many-c.nika.yaml")),
            "the file AFTER the failure still audited: {}",
            out.text
        );
    }

    /// `run_many` on all-clean files exits OK — the concatenation never
    /// invents a failure.
    #[test]
    fn run_many_is_clean_when_every_file_is() {
        let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let clean = "nika: v1\nworkflow:\n  id: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
        let a = dir.join("clean-a.nika.yaml");
        let b = dir.join("clean-b.nika.yaml");
        std::fs::write(&a, clean).expect("fixture a");
        std::fs::write(&b, clean).expect("fixture b");
        let paths: Vec<String> = [&a, &b]
            .iter()
            .map(|p| p.to_str().expect("utf8 path").to_owned())
            .collect();
        let out = run_many(&paths, false, None, Theme::new(false, true, false));
        assert_eq!(out.code, 0, "{}", out.text);
    }

    #[test]
    fn missing_read_files_flags_static_literal_and_var_default() {
        let dir = std::env::temp_dir().join(format!("nika-lint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or(());
        let present = dir.join("present.txt");
        std::fs::write(&present, "x").expect("fixture");
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\nconst:\n  src: \"{missing}\"\ntasks:\n  a:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ const.src }}}}\" }}\n  b:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{present}\" }}\n  c:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ tasks.a.output }}}}\" }}\n",
            missing = dir.join("missing.txt").display(),
            present = present.display(),
        );
        let wf = parse_wf(&yaml);
        let flagged: Vec<(String, String)> = nika_check::static_read_paths(&wf)
            .into_iter()
            .filter(|(_, p)| !std::path::Path::new(p).exists())
            .collect();
        // `a` via var default → flagged · `b` exists → silent ·
        // `c` dynamic (task ref) → the lint never guesses.
        assert_eq!(flagged.len(), 1, "{flagged:?}");
        assert_eq!(flagged[0].0, "a");
        let _ = std::fs::remove_file(&present);
    }

    #[test]
    fn pricing_section_rates_known_null_unknown() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: priced\nmodel: anthropic/claude-opus-4-5\ntasks:\n  think:\n    infer:\n      prompt: hi\n  odd:\n    infer:\n      model: custom/never-heard-of-it\n      prompt: hi\n",
        );
        let report = nika_check::check(&wf);
        let section = pricing_section(&report, &unresolvable_models(&report));
        let models = section["models"].as_array().expect("array");
        assert_eq!(models.len(), 2, "one row per requirements model");
        let by_model = |name: &str| {
            models
                .iter()
                .find(|m| m["model"] == name)
                .expect("a row per requirements model")
                .clone()
        };
        let priced = by_model("anthropic/claude-opus-4-5");
        assert!((priced["input_per_million"].as_f64().expect("rate") - 5.0).abs() < 1e-9);
        assert!((priced["output_per_million"].as_f64().expect("rate") - 25.0).abs() < 1e-9);
        // UNKNOWN renders null — a missing price must look missing,
        // never $0.00 (the silent-zero anti-pattern).
        let unknown = by_model("custom/never-heard-of-it");
        assert!(unknown["input_per_million"].is_null());
        assert!(unknown["output_per_million"].is_null());
    }

    fn parse_wf(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// The mute-diagnostic regression the battery re-run caught: with NO
    /// `permits:` block, a floor escape (SSRF-parity pass · permits-
    /// independent) exited rc=2 while the PERMITS panel printed only the
    /// informational line — `✖ findings above` pointed at nothing. The
    /// panel must render the escape. F-O8 rider: the ABSENT block now
    /// also speaks — the tool escape rides NIKA-AUTH-006 next to the
    /// floor's NIKA-SEC-005.
    #[test]
    fn floor_escape_renders_without_a_permits_block() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/x\" } }\n",
        );
        let report = nika_check::check(&wf);
        assert!(
            !report.capability_escapes.is_empty(),
            "the floor pass fires without permits"
        );
        let theme = Theme::new(false, true, false);
        let mut out = String::new();
        permits(&mut out, &report, &wf, theme);
        assert!(out.contains("SSRF floor"), "escape must render: {out}");
        assert!(
            out.contains("NIKA-SEC-005"),
            "the wire code names it: {out}"
        );
        // A2 (agent battery 2026-07-11): the code LEADS the row in
        // bracket position — `[NIKA-SEC-005 · net]` — so the PERMITS
        // panel is explainable like every CONFORM row (`nika explain`).
        assert!(
            out.contains("[NIKA-SEC-005 · net]"),
            "the code leads the row: {out}"
        );
        // F-O8 « absent = zero authority » + NEP-0003 law 1: the literal
        // URL is a NET escape under the absent block — NIKA-AUTH-006 rides
        // next to the floor code.
        assert!(
            out.contains("[NIKA-AUTH-006 · net]"),
            "the absent boundary speaks its own code: {out}"
        );
        // …and a public-host fetch without permits is NOT the
        // informational case anymore: the net escape (AUTH-006 · the
        // literal URL is statically judged) is the row (the old
        // « no boundary declared » mute is retired).
        let undeclared = parse_wf(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        let undeclared_report = nika_check::check(&undeclared);
        let mut undeclared_out = String::new();
        permits(&mut undeclared_out, &undeclared_report, &undeclared, theme);
        assert!(
            undeclared_out.contains("[NIKA-AUTH-006 · net]"),
            "absent + a literal url = the AUTH-006 net row: {undeclared_out}"
        );
        // …while the TRUE clean case (pure compute · zero authority
        // assumed) renders the F-O8 informational line.
        let clean = parse_wf(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  probe:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        let clean_report = nika_check::check(&clean);
        let mut clean_out = String::new();
        permits(&mut clean_out, &clean_report, &clean, theme);
        assert!(clean_out.contains("zero authority"), "{clean_out}");
    }

    /// The #395 admitting direction, through the CLI render: the battery
    /// local-watch repro (`permits.net.http: ["127.0.0.1"]` + a literal
    /// fetch to it) is GREEN — no NIKA-SEC-005, no dead-grant flag — and
    /// the panel TEACHES the clearing with the informational line.
    #[test]
    fn permitted_loopback_literal_renders_green_with_the_teaching_line() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: local-watch\npermits:\n  net: { http: [\"127.0.0.1\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/price.json\" } }\n",
        );
        let report = nika_check::check(&wf);
        assert!(
            report.capability_escapes.is_empty(),
            "the exact literal declassifies: {:?}",
            report.capability_escapes
        );
        let theme = Theme::new(false, true, false);
        let mut out = String::new();
        permits(&mut out, &report, &wf, theme);
        assert!(
            out.contains("body fits the declared boundary"),
            "green panel: {out}"
        );
        assert!(
            out.contains("exact loopback literal") && out.contains("`127.0.0.1`"),
            "the teaching line renders: {out}"
        );
        // …and a boundary with no loopback literal renders NO such line.
        let plain = parse_wf(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        let plain_report = nika_check::check(&plain);
        let mut plain_out = String::new();
        permits(&mut plain_out, &plain_report, &plain, theme);
        assert!(
            !plain_out.contains("exact loopback literal"),
            "no loopback grant → no line: {plain_out}"
        );
    }

    /// A `required: true` input with no `default:` is what the operator MUST
    /// pass — `check` should NAME it, so a bare `run` does not surprise them
    /// with NIKA-VAR-001.
    #[test]
    fn required_input_without_default_is_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: needs-input\nmodel: mock/echo\ninputs:\n  text:\n    type: string\n    required: true\ntasks:\n  a:\n    infer: { prompt: \"${{ inputs.text }}\" }\n",
        );
        assert_eq!(required_inputs(&wf), vec!["text"]);
    }

    /// Untyped (the value IS the default) · typed-with-default · typed-optional
    /// — none block a bare `run`, so none are listed.
    #[test]
    fn defaulted_or_optional_inputs_are_not_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: ok\nmodel: mock/echo\ninputs:\n  b:\n    type: string\n    default: \"d\"\n  c:\n    type: string\n    required: false\nconst:\n  a: \"has default\"\ntasks:\n  t:\n    infer: { prompt: \"${{ const.a }} ${{ inputs.b }} ${{ inputs.c }}\" }\n",
        );
        assert!(
            required_inputs(&wf).is_empty(),
            "{:?}",
            required_inputs(&wf)
        );
    }

    /// Write a fixture + run the human `check` render over it (ascii/no-colour
    /// so the assertions pin glyphs/text, not ANSI). The render path is what
    /// the operator reads — these tests pin its exact words.
    fn checked_text(name: &str, yaml: &str, ascii: bool) -> String {
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture body");
        let theme = Theme::new(false, ascii, false);
        run(path.to_str().expect("utf8 path"), false, false, None, theme).text
    }

    /// Same fixture plumbing, full `VerbOutput` (exit-code assertions) —
    /// the `--native-strict` posture tests read `.code`.
    fn checked_output(name: &str, yaml: &str, native_strict: bool) -> VerbOutput {
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture body");
        let theme = Theme::new(false, true, false);
        run(
            path.to_str().expect("utf8 path"),
            false,
            native_strict,
            None,
            theme,
        )
    }

    /// #320 repro 1: a CATALOGED-but-unresolvable provider (`azure/…` —
    /// the vendor listing knows it, the resolver does not) must be a
    /// finding, exit 2 — never a green audit that dies at run.
    #[test]
    fn models_rung_reds_a_cataloged_but_unresolvable_provider() {
        let out = checked_output(
            "models-azure.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n",
            false,
        );
        assert_eq!(
            out.code, 2,
            "unresolvable provider is a finding: {}",
            out.text
        );
        assert!(
            out.text.contains("MODELS") && out.text.contains("`azure`"),
            "the rung names the provider: {}",
            out.text
        );
    }

    /// #320 repro 2: a BARE model id (no `<provider>/` prefix) reds the
    /// rung AND must never wear a conjured price in the pricing section.
    #[test]
    fn models_rung_reds_a_bare_model_id_and_never_conjures_a_price() {
        let out = checked_output(
            "models-bare.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n",
            false,
        );
        assert_eq!(out.code, 2, "bare id is a finding: {}", out.text);
        assert!(
            out.text.contains("bare model id"),
            "teaches the contract: {}",
            out.text
        );
        // The JSON surface: models_resolve false · clean false · the
        // pricing row is NULL (unpriced beats conjured — the $0.0001
        // fuzzy-match hole from the live evidence).
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        let path = dir.join("models-bare.nika.yaml");
        let theme = Theme::new(false, true, false);
        let out = run(path.to_str().expect("utf8 path"), true, false, None, theme);
        assert_eq!(out.code, 2);
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
        assert_eq!(payload["clean"], false);
        assert_eq!(payload["models_resolve"], false);
        assert_eq!(
            payload["model_findings"][0]["model"], "gpt-5-turbo",
            "{payload:#}"
        );
        let row = &payload["pricing"]["models"][0];
        assert!(
            row["input_per_million"].is_null() && row["output_per_million"].is_null(),
            "an unresolvable model is never priced: {row:#}"
        );
    }

    /// The happy path: every model resolvable → the rung is one green
    /// line and the audit verdict is untouched.
    #[test]
    fn models_rung_is_green_when_every_model_resolves() {
        let out = checked_output(
            "models-green.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
            false,
        );
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.text.contains("MODELS") && out.text.contains("1 model resolves"),
            "the green rung is visible: {}",
            out.text
        );
    }

    /// `--json --native-strict`: the payload's `native_strict_clean` and
    /// the exit code must agree (the review-swarm untested-branch gap).
    #[test]
    fn native_strict_json_payload_agrees_with_the_exit_code() {
        let helper = "nika: v1\nworkflow:\n  id: helper\npermits: { exec: [\"curl\"] }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("native-strict-json.nika.yaml");
        std::fs::write(&path, helper).expect("fixture body");
        let theme = Theme::new(false, true, false);
        let out = run(path.to_str().expect("utf8 path"), true, true, None, theme);
        assert_eq!(
            out.code, 2,
            "strict hint-only workflow exits FILE: {}",
            out.text
        );
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
        assert_eq!(
            payload["clean"],
            serde_json::json!(true),
            "spec-clean stays true"
        );
        assert_eq!(
            payload["native_strict_clean"],
            serde_json::json!(false),
            "the strict verdict rides the payload: {payload:#}"
        );
    }

    /// `--native-strict` promotes native-first hints to failure: the SAME
    /// spec-valid workflow exits 0 by default and 2 under strict, with the
    /// strict verdict naming the count; a natively-written twin stays exit
    /// 0 under strict.
    #[test]
    fn native_strict_fails_on_native_first_hints_only() {
        let helper = "nika: v1\nworkflow:\n  id: helper\npermits: { exec: [\"curl\"] }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
        let default_run = checked_output("native-default.nika.yaml", helper, false);
        assert_eq!(
            default_run.code, 0,
            "advisory by default: {}",
            default_run.text
        );
        assert!(
            default_run.text.contains("[native-first]"),
            "{}",
            default_run.text
        );

        let strict = checked_output("native-strict.nika.yaml", helper, true);
        assert_eq!(
            strict.code, 2,
            "strict promotes to failure: {}",
            strict.text
        );
        assert!(
            strict.text.contains("native-strict · 1 native-first hint"),
            "{}",
            strict.text
        );

        let native_twin = "nika: v1\nworkflow:\n  id: native\npermits: { tools: [\"nika:fetch\"], net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://acme.test\" } }\n";
        let twin = checked_output("native-twin.nika.yaml", native_twin, true);
        assert_eq!(twin.code, 0, "the native twin passes strict: {}", twin.text);
        assert!(!twin.text.contains("native-strict ·"), "{}", twin.text);
    }

    /// The strict refusal must not offer a remedy that does not work.
    ///
    /// It used to read "replace them or record them in the exec ledger",
    /// and the second half was false: the gate judges the SHAPE of the
    /// subprocess, so a ledgered `.py` wrapper fails exactly as hard as
    /// an un-ledgered one. A reader who took the offer wrote a ledger,
    /// re-ran, and met the identical red — the diagnostic spent a cycle
    /// and returned nothing. This pins the honest form: name the builtin
    /// as the remedy, and say what the ledger is actually for.
    #[test]
    fn the_strict_refusal_does_not_sell_the_ledger_as_an_escape() {
        // One line, like every other fixture here. A backslash-continued
        // string reads better but defeats the fn-length ratchet: its
        // literal stripper is line-local, so the YAML braces inside the
        // continuation count as code and the reported length runs to the
        // end of the module. Measured: this 24-line test reported as 212.
        let ledgered = "# EXEC LEDGER ·\n# | task | command | why no native path | unlock |\n# | crawl | curl | legacy auth | nika:fetch oauth |\nnika: v1\nworkflow:\n  id: ledgered\npermits: { exec: [\"curl\"] }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
        let out = checked_output("ledgered.nika.yaml", ledgered, true);
        assert_eq!(
            out.code, 2,
            "a ledger does not clear the strict gate: {}",
            out.text
        );
        assert!(
            !out.text.contains("or record them in the exec ledger"),
            "the refusal still offers the ledger as an alternative: {}",
            out.text
        );
        assert!(
            out.text.contains("does not clear this gate"),
            "the refusal must say what the ledger is NOT: {}",
            out.text
        );
    }

    /// The COST section names a DISTINCT reason per unbounded task — a deleted
    /// match arm collapses one of these into the bare `unbounded` fallback, so
    /// each exact phrase pins its arm: `NoTokenLimit` · `NoPrice` · `UnknownIterations`.
    #[test]
    fn cost_section_names_each_unbounded_reason() {
        let text = checked_text(
            "cost-reasons.nika.yaml",
            "nika: v1\nworkflow:\n  id: cost-reasons\ninputs:\n  items: { type: { array: string }, required: true }\ntasks:\n  a:\n    infer: { prompt: \"hi\", model: \"anthropic/claude-opus-4-20250514\" }\n  b:\n    infer: { prompt: \"hi\", model: \"ollama/llama3.1\", max_tokens: 50 }\n  c:\n    for_each: \"${{ inputs.items }}\"\n    infer: { prompt: \"x\", model: \"anthropic/claude-opus-4-20250514\", max_tokens: 10 }\n",
            true,
        );
        assert!(text.contains("no max_tokens declared"), "{text}");
        assert!(
            text.contains("no catalog price (local/unknown model)"),
            "{text}"
        );
        assert!(
            text.contains("for_each over an expression (unknown count)"),
            "{text}"
        );
    }

    /// `mark()` paints the verdict glyph on EVERY clean section — not just the
    /// one literal verdict line. A mutated mark (returns `""` / `"xyzzy"`)
    /// strips the section glyphs (count drops) or injects a placeholder.
    #[test]
    fn clean_report_marks_every_section() {
        let text = checked_text(
            "clean-one.nika.yaml",
            "nika: v1\nworkflow:\n  id: clean-one\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
            false,
        );
        let ticks = text.matches('✔').count();
        assert!(
            ticks >= 5,
            "every clean section carries ✔ (got {ticks}): {text}"
        );
        assert!(
            !text.contains("xyzzy"),
            "mark never emits a placeholder: {text}"
        );
    }

    /// The clean verdict is the audited CARD line: tasks · waves ·
    /// permits state · the cost floor · the hint count — with full
    /// ASCII parity (`ok audited` · `>=`).
    #[test]
    fn clean_verdict_is_the_audited_card_line() {
        let yaml = "nika: v1\nworkflow:\n  id: card\nmodel: mock/echo\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"echo\", \"bye\"] }\n";
        let text = checked_text("audited-card.nika.yaml", yaml, false);
        assert!(
            text.contains(
                "✔ audited · 2 tasks · 2 waves · permits declared · est ≥$0.0000 · 0 hints"
            ),
            "the audited card line: {text}"
        );
        let ascii = checked_text("audited-card-ascii.nika.yaml", yaml, true);
        assert!(
            ascii.contains("ok audited") && ascii.contains("est >=$0.0000"),
            "ascii parity (ok · >=): {ascii}"
        );
        assert!(
            !ascii.contains('≥'),
            "no unicode leaks into --ascii: {ascii}"
        );
        // Hint pluralization: 0 hints here (the boundary is declared).
        assert!(
            text.contains("0 hints") && !text.contains("0 hint·"),
            "{text}"
        );
    }

    /// When conformance FAILS there is no valid DAG, so PLAN announces the skip
    /// (gated on `!conformance.is_empty()`) — a deleted `!` would suppress the
    /// line and leave the operator wondering where the plan went.
    #[test]
    fn plan_prints_wave_membership_with_verbs_and_targets() {
        let text = checked_text(
            "plan-membership.nika.yaml",
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-5\ntasks:\n  think:\n    infer: { prompt: hi }\n  after:\n    after:\n      think: success\n    exec:\n      command: [\"echo\", \"x\"]\n",
            true,
        );
        assert!(text.contains("wave 1"), "membership renders: {text}");
        assert!(
            text.contains("think (infer · anthropic/claude-sonnet-5)"),
            "the envelope model resolves into the plan line: {text}"
        );
        assert!(
            text.contains("after (exec · echo)"),
            "argv[0] names the exec: {text}"
        );
    }

    #[test]
    fn plan_announces_the_skip_when_conformance_fails() {
        let text = checked_text(
            "plan-skip.nika.yaml",
            "nika: v1\nworkflow:\n  id: bad-ref\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ vars.nope }}\"] }\n",
            true,
        );
        assert!(
            text.contains("(skipped — no valid DAG order while conformance fails)"),
            "{text}"
        );
    }

    /// NIKA-DRIFT-001: a declared-but-unused envelope entry is an
    /// advisory HINT — rendered code-first (the bracket voice), counted
    /// in the audited card line, and the exit stays GREEN (dead
    /// declarations are smell, not failure).
    #[test]
    fn unused_declaration_is_hinted_and_the_exit_stays_green() {
        let out = checked_output(
            "drift-unused.nika.yaml",
            "nika: v1\nworkflow:\n  id: w\nconst:\n  ghost: \"x\"\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
            false,
        );
        assert_eq!(out.code, 0, "a drift hint never fails: {}", out.text);
        assert!(
            out.text.contains("[NIKA-DRIFT-001 · drift]"),
            "code-first bracket voice: {}",
            out.text
        );
        assert!(out.text.contains("`const.ghost`"), "{}", out.text);
        assert!(
            out.text.contains("audited") && out.text.contains("hint"),
            "the card line still renders: {}",
            out.text
        );
    }

    /// The machine projection law: `--json` carries the drift hint with
    /// its code, `clean` stays true, and the exit stays 0.
    #[test]
    fn drift_hint_rides_the_json_projection() {
        // Per-PROCESS dir (the check-expect mktemp collision class, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("drift-json.nika.yaml");
        std::fs::write(
            &path,
            "nika: v1\nworkflow:\n  id: w\nconst:\n  ghost: \"x\"\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        )
        .expect("fixture body");
        let out = run(
            path.to_str().expect("utf8 path"),
            true,
            false,
            None,
            Theme::new(false, true, false),
        );
        assert_eq!(out.code, 0, "{}", out.text);
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
        assert_eq!(payload["clean"], true, "{payload:#}");
        let hints = payload["hints"].as_array().expect("hints array");
        let drift = hints
            .iter()
            .find(|h| h["kind"] == "drift")
            .expect("the drift hint rides the machine surface");
        assert_eq!(drift["code"], "NIKA-DRIFT-001", "{drift:#}");
        assert!(
            drift["advice"]
                .as_str()
                .expect("advice")
                .contains("`const.ghost`"),
            "{drift:#}"
        );
    }

    /// The no-duplication law: an UNDECLARED reference is the hard
    /// lane's (`NIKA-VAR-001`) — the drift code must not also fire for
    /// it (the two codes never name the same site).
    #[test]
    fn unresolved_reference_never_also_drifts() {
        let out = checked_output(
            "drift-no-dup.nika.yaml",
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.ghost }}\"] }\n",
            false,
        );
        assert_eq!(out.code, 2, "the hard lane fails: {}", out.text);
        assert!(out.text.contains("NIKA-VAR-001"), "{}", out.text);
        assert!(
            !out.text.contains("NIKA-DRIFT-001"),
            "no drift duplication: {}",
            out.text
        );
    }
}
