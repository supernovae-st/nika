// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain <file>` — the human story of a workflow (the file form of
//! `explain` · the code form lives in [`super::explain`]).
//!
//! One more RENDERER over the one projection (`load_checked` → `graph::
//! project` — the same seam `inspect` and `graph` read): what this
//! workflow does, the story wave by wave, the cost BEFORE a token is
//! spent (honesty rules: unknown stays unknown, never `$0` · a local
//! model is unpriced compute, never « free »), what it touches, the
//! structural risks, how to run it, and what the flight recorder already
//! holds. Deterministic, offline, zero LLM — a narration derived from
//! facts the checker proved, never a summary something imagined.
//!
//! `--json` emits the versioned machine twin (`explain_version: 1`),
//! reusing the check report's own serialized vocabulary (cost ·
//! requirements · hints · analysis) so agents read ONE dialect across
//! `check --json` and `explain --json`.

use std::fmt::Write as _;
use std::path::Path;

use nika_schema::check::{CheckReport, UnboundedReason};

use crate::verbs::graph::{GraphDoc, Node, project};
use crate::verbs::{VerbOutput, load_checked};

/// Route `explain`'s positional: an existing path or a path-shaped string
/// (`/` · `.yaml`/`.yml` · `-`) narrates the FILE; everything else teaches
/// the CODE (`NIKA-440` · `DAG-003` · bare `440`). A file literally named
/// like a code still routes as a file when it exists on disk — the
/// pathological tie goes to the thing that provably exists.
#[must_use]
pub fn dispatch(query: &str, json: bool) -> VerbOutput {
    let yaml_ext = Path::new(query)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"));
    let file_shaped = query == "-"
        || query.contains('/')
        || query.contains('\\')
        || yaml_ext
        || Path::new(query).exists();
    if file_shaped {
        return run(query, json);
    }
    if json {
        // The code form is prose-by-design (one paragraph, one voice) —
        // refusing beats silently ignoring a flag an agent relied on.
        return VerbOutput::file(
            "--json rides the FILE form (`nika explain <file> --json`); error codes teach in prose"
                .to_owned(),
        );
    }
    super::explain::run(query)
}

/// The `nika explain <file>` verb.
#[must_use]
pub fn run(path: &str, json: bool) -> VerbOutput {
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let description = wf.description.as_ref().map(|d| d.value.clone());
    let permits_declared = wf.permits.is_some();
    if !report.conformance.is_empty() {
        // No valid DAG order → no wave story. Explain stays useful:
        // name the findings and hand over to the fixer, never invent
        // a story the checker refused to prove.
        return dirty(path, description.as_deref(), &report, json);
    }
    let doc = project(&wf, &report);
    let traces = traces_glance(Path::new(".nika").join("traces").as_path());
    if json {
        return VerbOutput::ok(render_json(
            path,
            description.as_deref(),
            &doc,
            &report,
            permits_declared,
            traces.as_ref(),
        ));
    }
    VerbOutput::ok(render_human(
        path,
        description.as_deref(),
        &doc,
        &report,
        permits_declared,
        traces.as_ref(),
    ))
}

/// The findings-first partial for a non-conformant file — explain never
/// narrates a DAG the checker could not order.
fn dirty(path: &str, description: Option<&str>, report: &CheckReport, json: bool) -> VerbOutput {
    if json {
        let v = serde_json::json!({
            "explain_version": 1,
            "file": path,
            "description": description,
            "clean": false,
            "findings": report.conformance.len(),
            "fix": format!("nika check {path}"),
        });
        return VerbOutput::file(v.to_string());
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "this workflow does not check clean yet — {} finding(s):",
        report.conformance.len()
    );
    for c in report.conformance.iter().take(3) {
        let _ = writeln!(s, "  [{}] {}", c.code, c.message);
    }
    if report.conformance.len() > 3 {
        let _ = writeln!(s, "  … +{} more", report.conformance.len() - 3);
    }
    let _ = writeln!(
        s,
        "\nfix first: nika check {path}   # every finding explains itself"
    );
    VerbOutput::file(s)
}

/// One task, one sentence — the plain-words gloss of the verb model.
fn task_line(node: &Node) -> String {
    let mut line = match node.verb {
        "infer" => match &node.model {
            Some(m) => format!("asks {m}"),
            None => "asks the workflow model".to_owned(),
        },
        "exec" => "runs a command".to_owned(),
        "invoke" => match &node.tool {
            Some(t) => format!("calls {t}"),
            None => "calls a tool".to_owned(),
        },
        "agent" => match &node.model {
            Some(m) => format!("runs an agent loop on {m}"),
            None => "runs an agent loop".to_owned(),
        },
        other => other.to_owned(),
    };
    if let Some(fan) = &node.fan_out {
        match fan.count {
            Some(n) => {
                let _ = write!(line, " · ×{n} fan-out");
            }
            None => line.push_str(" · fan-out (count known at run time)"),
        }
    }
    if let Some(when) = &node.when {
        let _ = write!(line, " · only when {when}");
    }
    line
}

/// Why a task's cost has no ceiling — the honesty gloss (never `$0`).
fn unbounded_gloss(task: &str, model: Option<&str>, reason: UnboundedReason) -> String {
    match reason {
        UnboundedReason::NoTokenLimit => {
            format!("{task}: no max_tokens declared — spend has no ceiling")
        }
        UnboundedReason::NoPrice => format!(
            "{task}: {} has no catalog price — unknown stays unknown (never $0)",
            model.unwrap_or("the model")
        ),
        UnboundedReason::UnknownIterations => {
            format!("{task}: fan-out count resolves at run time")
        }
        // The enum is #[non_exhaustive]-shaped by policy: a future reason
        // renders honestly rather than silently bounding.
        _ => format!("{task}: cost not statically boundable"),
    }
}

/// The local providers of THIS build (`requires_key == false`) — derived
/// from the same registry a run composes, never a hardcoded id list.
fn local_provider_ids() -> Vec<String> {
    let registry =
        nika_providers::ProviderRegistry::without_http(crate::verbs::run::config_from_env());
    registry
        .profiles()
        .iter()
        .filter(|p| !p.requires_key && p.id != "mock")
        .map(|p| p.id.to_owned())
        .collect()
}

/// The flight-recorder glance: how many runs the local trace dir holds and
/// the latest journal (names are ISO-timestamped → lexicographic max IS
/// newest). Presence only — no journal is parsed, so no wrong claim.
fn traces_glance(dir: &Path) -> Option<(usize, String)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.ends_with(".ndjson").then_some(name)
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort_unstable();
    let latest = names.last()?.clone();
    Some((names.len(), format!("{}/{latest}", dir.display())))
}

/// The human narration — one section helper per beat, composed here (the
/// 100-line fn cap forced this shape and the shape is better: each beat
/// is independently testable prose).
fn render_human(
    path: &str,
    description: Option<&str>,
    doc: &GraphDoc,
    report: &CheckReport,
    permits_declared: bool,
    traces: Option<&(usize, String)>,
) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} — {}",
        doc.workflow,
        description
            .unwrap_or("(no description yet — one line under `description:` says what it is for)")
    );
    let _ = writeln!(
        s,
        "  {} task(s) · {} wave(s) · checks clean",
        doc.nodes.len(),
        report.waves.len()
    );
    story_section(&mut s, doc, report);
    cost_section(&mut s, report);
    touches_section(&mut s, doc, report, permits_declared);
    risks_section(&mut s, path, report);
    run_section(&mut s, path, report);
    recorder_section(&mut s, traces);
    s
}

/// The story, wave by wave — projection order IS wave order.
fn story_section(s: &mut String, doc: &GraphDoc, report: &CheckReport) {
    let _ = writeln!(s, "\nthe story");
    let wave_sizes: Vec<usize> = report.waves.iter().map(Vec::len).collect();
    let mut cursor = 0usize;
    for (i, &size) in wave_sizes.iter().enumerate() {
        let end = cursor.saturating_add(size).min(doc.nodes.len());
        let members = &doc.nodes[cursor..end];
        cursor = end;
        let label = if members.len() > 1 {
            format!("wave {} — {} in parallel", i + 1, members.len())
        } else {
            format!("wave {}", i + 1)
        };
        let _ = writeln!(s, "  {label}");
        for node in members {
            let _ = writeln!(s, "    {} — {}", node.id, task_line(node));
        }
    }
}

/// Cost BEFORE a token is spent — the honesty section (FLOOR is named,
/// unknown never renders as $0, local never renders as « free »).
fn cost_section(s: &mut String, report: &CheckReport) {
    let _ = writeln!(s, "\ncost before a token is spent");
    if report.cost.tasks.is_empty() {
        let _ = writeln!(s, "  no inference tasks · $0 model spend");
    } else if report.cost.has_unbounded {
        let _ = writeln!(
            s,
            "  ≥ ${:.4} — a FLOOR, not a ceiling:",
            report.cost.bounded_total_usd
        );
        for t in report
            .cost
            .tasks
            .iter()
            .filter_map(|t| t.unbounded_reason.map(|r| (t, r)))
            .take(4)
        {
            let _ = writeln!(
                s,
                "    {}",
                unbounded_gloss(&t.0.task, t.0.model.as_deref(), t.1)
            );
        }
    } else {
        let _ = writeln!(
            s,
            "  ≤ ${:.4} worst case · ≥ ${:.4} cheapest path",
            report.cost.bounded_total_usd, report.cost.min_path_total_usd
        );
    }
    let locals = local_provider_ids();
    let uses_local = report.requirements.models.iter().any(|m| {
        m.model
            .split_once('/')
            .is_some_and(|(p, _)| locals.iter().any(|l| l == p))
    });
    if uses_local {
        let _ = writeln!(
            s,
            "  local models: your compute · tokens unpriced — not « free »"
        );
    }
}

/// What it touches: models · tools · secret/env NAMES · the permits stance.
fn touches_section(s: &mut String, doc: &GraphDoc, report: &CheckReport, permits_declared: bool) {
    let _ = writeln!(s, "\nwhat it touches");
    if report.requirements.models.is_empty() {
        let _ = writeln!(s, "  models   none (no inference)");
    } else {
        let models: Vec<String> = report
            .requirements
            .models
            .iter()
            .map(|m| format!("{} ({} task(s))", m.model, m.tasks.len()))
            .collect();
        let _ = writeln!(s, "  models   {}", models.join(" · "));
    }
    let tools: Vec<&str> = {
        let mut t: Vec<&str> = doc.nodes.iter().filter_map(|n| n.tool.as_deref()).collect();
        t.sort_unstable();
        t.dedup();
        t
    };
    if !tools.is_empty() {
        let _ = writeln!(s, "  tools    {}", tools.join(" · "));
    }
    let mut needs: Vec<String> = report
        .requirements
        .secrets
        .iter()
        .map(|sec| format!("secrets.{}", sec.name))
        .collect();
    needs.extend(
        report
            .requirements
            .env_reads
            .iter()
            .map(|e| format!("env.{e}")),
    );
    if !needs.is_empty() {
        let _ = writeln!(
            s,
            "  needs    {} (names only — values stay in your environment)",
            needs.join(" · ")
        );
    }
    let _ = writeln!(
        s,
        "  permits  {}",
        if permits_declared {
            "declared boundary (default-deny beyond it)"
        } else {
            "engine floor only — `nika check --infer-permits` prints the tightest boundary"
        }
    );
}

/// Structural risks — only what the checker proved, never speculation.
fn risks_section(s: &mut String, path: &str, report: &CheckReport) {
    let risky = !report.hints.is_empty()
        || report
            .analysis
            .as_ref()
            .is_some_and(|a| !a.blast_radius.is_empty());
    if !risky {
        return;
    }
    let _ = writeln!(s, "\nworth knowing");
    for h in report.hints.iter().take(3) {
        let _ = writeln!(s, "  [{}] {}", h.kind, h.advice);
    }
    if report.hints.len() > 3 {
        let _ = writeln!(
            s,
            "  … +{} more hint(s) → nika check {path}",
            report.hints.len() - 3
        );
    }
    if let Some(a) = report.analysis.as_ref()
        && let Some(b) = a.blast_radius.first()
    {
        let _ = writeln!(
            s,
            "  if {} fails, {} downstream task(s) never run",
            b.task, b.blocks
        );
    }
}

/// Run it — plus the mock rehearsal when the workflow bills real models.
fn run_section(s: &mut String, path: &str, report: &CheckReport) {
    let _ = writeln!(s, "\nrun it");
    let _ = writeln!(s, "  nika run {path}");
    let all_mock = report
        .requirements
        .models
        .iter()
        .all(|m| m.model.starts_with("mock/"));
    if !report.requirements.models.is_empty() && !all_mock {
        let _ = writeln!(
            s,
            "  nika run {path} --model mock/echo   # offline rehearsal · zero keys"
        );
    }
}

/// The flight recorder — what already happened here, and that it is
/// provable. The full path prints ONCE (the read command); verify names
/// the same file with « it » — three repetitions of a 45-char path was
/// the wall-of-text tell the 80-column read caught.
fn recorder_section(s: &mut String, traces: Option<&(usize, String)>) {
    match traces {
        Some((n, latest)) => {
            let _ = writeln!(
                s,
                "\nflight recorder\n  {n} run(s) in .nika/traces · latest:\n  \
                 nika trace show {latest}\n  \
                 nika trace verify <same file>   # prove the hash chain"
            );
        }
        None => {
            let _ = writeln!(
                s,
                "\nflight recorder\n  no runs recorded here yet — every run writes a \
                 tamper-evident, hash-chained trace to .nika/traces/"
            );
        }
    }
}

/// The versioned machine twin — reuses the report's own serialized
/// vocabulary so `check --json` and `explain --json` speak one dialect.
fn render_json(
    path: &str,
    description: Option<&str>,
    doc: &GraphDoc,
    report: &CheckReport,
    permits_declared: bool,
    traces: Option<&(usize, String)>,
) -> String {
    let wave_sizes: Vec<usize> = report.waves.iter().map(Vec::len).collect();
    let mut waves: Vec<Vec<&str>> = Vec::with_capacity(wave_sizes.len());
    let mut cursor = 0usize;
    for &size in &wave_sizes {
        let end = cursor.saturating_add(size).min(doc.nodes.len());
        waves.push(
            doc.nodes[cursor..end]
                .iter()
                .map(|n| n.id.as_str())
                .collect(),
        );
        cursor = end;
    }
    let tasks: Vec<serde_json::Value> = doc
        .nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "verb": n.verb,
                "story": task_line(n),
                "model": n.model,
                "tool": n.tool,
                "when": n.when,
            })
        })
        .collect();
    serde_json::json!({
        "explain_version": 1,
        "file": path,
        "workflow": doc.workflow,
        "description": description,
        "clean": true,
        "tasks": tasks,
        "waves": waves,
        "cost": report.cost,
        "requirements": report.requirements,
        "permits_declared": permits_declared,
        "hints": report.hints,
        "analysis": report.analysis,
        "traces": traces.map(|(n, latest)| serde_json::json!({"count": n, "latest": latest})),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    fn tmp(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nika-explain-file-{}-{name}.nika.yaml",
            std::process::id(),
        ));
        std::fs::write(&path, content).expect("fixture written");
        path
    }

    const DIAMOND: &str = "nika: v1\nworkflow: brief-factory\ndescription: fetch, summarize twice, join\n\nmodel: mock/echo\n\ntasks:\n  - id: root\n    infer: { prompt: \"r\", max_tokens: 10 }\n  - id: left\n    depends_on: [root]\n    infer: { prompt: \"l\", max_tokens: 10 }\n  - id: right\n    depends_on: [root]\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: join\n    depends_on: [left, right]\n    infer: { prompt: \"j\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.join.output }}\n";

    #[test]
    fn narrates_the_diamond_with_cost_and_handoff() {
        let path = tmp("diamond", DIAMOND);
        let out = run(path.to_str().expect("utf8"), false);
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        for needle in [
            "brief-factory — fetch, summarize twice, join",
            "4 task(s) · 3 wave(s) · checks clean",
            "the story",
            "wave 2 — 2 in parallel",
            "asks mock/echo",
            "cost before a token is spent",
            "what it touches",
            "mock/echo (4 task(s))",
            "run it",
            "nika run",
            "flight recorder",
        ] {
            assert!(
                out.text.contains(needle),
                "missing `{needle}`:\n{}",
                out.text
            );
        }
        // The default model IS mock — no redundant mock-rehearsal line.
        assert!(
            !out.text.contains("offline rehearsal"),
            "mock workflows need no mock hint:\n{}",
            out.text
        );
        // If root fails, everything downstream is named.
        assert!(
            out.text
                .contains("if root fails, 3 downstream task(s) never run"),
            "{}",
            out.text
        );
    }

    #[test]
    fn unbounded_cost_reads_as_a_floor_never_zero() {
        // qwen has no max_tokens → NoTokenLimit; the narration must say
        // FLOOR and must never render a fake $0 ceiling.
        let path = tmp(
            "floor",
            "nika: v1\nworkflow: floor-story\ntasks:\n  - id: think\n    infer: { prompt: \"x\" }\n",
        );
        let out = run(path.to_str().expect("utf8"), false);
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("FLOOR"), "{}", out.text);
        assert!(
            out.text.contains("no max_tokens declared"),
            "names the reason:\n{}",
            out.text
        );
        assert!(
            !out.text.contains("≤ $"),
            "an unbounded workflow never shows a ceiling:\n{}",
            out.text
        );
    }

    #[test]
    fn json_twin_is_versioned_and_speaks_the_report_dialect() {
        let path = tmp("json", DIAMOND);
        let out = run(path.to_str().expect("utf8"), true);
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::OK, "{}", out.text);
        let v: serde_json::Value = serde_json::from_str(&out.text).expect("parses");
        assert_eq!(v["explain_version"], 1);
        assert_eq!(v["workflow"], "brief-factory");
        assert_eq!(v["clean"], true);
        assert_eq!(v["waves"].as_array().map(Vec::len), Some(3));
        assert_eq!(v["waves"][1].as_array().map(Vec::len), Some(2));
        assert_eq!(v["tasks"][0]["story"], "asks mock/echo");
        // The report's own vocabulary rides through (one dialect).
        assert!(v["cost"]["bounded_total_usd"].is_number(), "{}", out.text);
        assert!(v["requirements"]["models"].is_array(), "{}", out.text);
    }

    #[test]
    fn a_dirty_file_gets_findings_first_never_a_story() {
        // `when:` as a bare string is a conformance finding — explain
        // must refuse to narrate and hand over to check (exit 2).
        let path = tmp(
            "dirty",
            "nika: v1\nworkflow: dirty\ntasks:\n  - id: a\n    exec: { command: \"echo x\" }\n  - id: b\n    depends_on: [a]\n    when: maybe\n    exec: { command: \"echo y\" }\n",
        );
        let out = run(path.to_str().expect("utf8"), false);
        std::fs::remove_file(&path).ok();
        assert_eq!(out.code, exit::FILE, "{}", out.text);
        assert!(out.text.contains("does not check clean"), "{}", out.text);
        assert!(out.text.contains("fix first: nika check"), "{}", out.text);
        assert!(!out.text.contains("the story"), "{}", out.text);
    }

    #[test]
    fn traces_glance_finds_the_lexicographically_latest_journal() {
        let dir = std::env::temp_dir().join(format!("nika-explain-traces-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("2026-07-01T10-00-00Z-aaaa.ndjson"), "x").expect("write");
        std::fs::write(dir.join("2026-07-08T09-49-09Z-9c3f.ndjson"), "x").expect("write");
        std::fs::write(dir.join("notes.txt"), "x").expect("write");
        let glance = traces_glance(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let (n, latest) = glance.expect("two journals found");
        assert_eq!(n, 2);
        assert!(
            latest.ends_with("2026-07-08T09-49-09Z-9c3f.ndjson"),
            "{latest}"
        );
    }

    /// `explain` routes codes to the teacher and paths to the narrator —
    /// and the tie (a string that exists on disk) goes to the file.
    #[test]
    fn dispatch_routes_codes_and_files() {
        // Codes: registry + spec + bare forms stay the teaching surface.
        for code in ["NIKA-440", "440", "DAG-003"] {
            let out = dispatch(code, false);
            assert_eq!(out.code, exit::OK, "{code}: {}", out.text);
        }
        // A path-shaped query routes to the file narrator — missing file
        // = the loader's own error, never a "unknown code" 404.
        let out = dispatch("no/such/dir/flow.nika.yaml", false);
        assert!(
            !out.text.contains("unknown code"),
            "paths never 404 as codes: {}",
            out.text
        );
        // The code form refuses --json loudly instead of ignoring it.
        let out = dispatch("NIKA-440", true);
        assert_eq!(out.code, exit::FILE);
        assert!(out.text.contains("--json"), "{}", out.text);
    }

    #[test]
    fn unbounded_glosses_never_say_zero() {
        assert!(unbounded_gloss("t", Some("x/y"), UnboundedReason::NoPrice).contains("never $0"));
        assert!(unbounded_gloss("t", None, UnboundedReason::NoTokenLimit).contains("no ceiling"));
        assert!(
            unbounded_gloss("t", None, UnboundedReason::UnknownIterations).contains("run time")
        );
    }
}
