// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The deterministic facts — a Nika question answered from the engine's
//! own authorities before any model is asked: the workflows the snapshot
//! saw, the builtins the catalog ships, the providers this binary drives,
//! the example or template the ONE router names, a workflow's verdict
//! from the ONE facade, a code's teaching from the ONE ladder. Zero
//! tokens, zero invention — and the answer a session without any
//! conversational intelligence still gives.

use std::path::Path;

use crate::snapshot::ProjectSnapshot;

/// The fact an input asks for, when it asks for one.
#[must_use]
pub fn answer(input: &str, snapshot: &ProjectSnapshot, root: &Path) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    if let Some(code) = input
        .split_whitespace()
        .find(|w| w.starts_with("NIKA-"))
        .map(|w| w.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-'))
        && lower.contains("explain")
    {
        let out = nika_cli_host::explain::run(code, nika_cli_host::Theme::new(false, true, false));
        return Some(out.text.trim_end().to_owned());
    }
    if (lower.contains("check") || lower.contains("valid"))
        && let Some(name) = named_workflow(input, snapshot)
    {
        return Some(verdict(root, snapshot, &name));
    }
    if lower.contains("workflow")
        && any(
            &lower,
            &["list", "which", "what", "here", "have", "exist", "show"],
        )
    {
        return Some(snapshot.facts_lines().join("\n"));
    }
    if lower.contains("builtin")
        || (lower.contains("tool") && any(&lower, &["which", "what", "list", "available"]))
    {
        let names = crate::guard::builtin_names();
        return Some(format!(
            "{} builtins this engine ships (`nika catalog --tools` for their arguments):\n  {}",
            names.len(),
            names.join(" · ")
        ));
    }
    if lower.contains("provider")
        || (lower.contains("model")
            && any(&lower, &["which", "what", "list", "available", "support"]))
    {
        let ids: Vec<&str> = nika_providers::CANONICAL_IDS.to_vec();
        return Some(format!(
            "{} providers this binary drives (`nika catalog` for the models · `nika doctor` for this machine's paths):\n  {}",
            ids.len(),
            ids.join(" · ")
        ));
    }
    if any(
        &lower,
        &[
            "example",
            "template",
            "start from",
            "which shape",
            "scaffold",
        ],
    ) {
        return Some(route(input));
    }
    if any(
        &lower,
        &[
            "last run",
            "latest run",
            "previous run",
            "what happened",
            "did it run",
            "the run",
        ],
    ) {
        return Some(last_run(root));
    }
    vocabulary(&lower)
}

/// The newest `.ndjson` under the store by mtime (name tie-break) — the
/// raw file, since the fact reads its lines itself.
fn newest_trace(store: &Path) -> Option<std::path::PathBuf> {
    let mut traces: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(store)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .filter_map(|p| {
            std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| (t, p))
        })
        .collect();
    traces.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    traces.into_iter().next().map(|(_, p)| p)
}

/// The last run, read from its trace (the evidence), never from memory:
/// the workflow, every task's outcome, the settlement.
fn last_run(root: &Path) -> String {
    let store = root.join(".nika").join("traces");
    let Some(trace) = newest_trace(&store) else {
        return "no run yet under this root (no trace in `.nika/traces/`)".to_owned();
    };
    let Ok(text) = std::fs::read_to_string(&trace) else {
        return format!("the latest trace `{}` could not be read", trace.display());
    };
    let mut workflow = String::new();
    let mut tasks: Vec<String> = Vec::new();
    let mut settled: Option<String> = None;
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let field = |key: &str| -> String {
            v.get("fields")
                .and_then(|f| f.as_array())
                .and_then(|rows| {
                    rows.iter()
                        .find(|r| r.get("key").and_then(|k| k.as_str()) == Some(key))
                })
                .and_then(|r| r.get("value"))
                .map(|val| match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        };
        match kind {
            "workflow_started" => workflow = field("workflow"),
            "task_completed" => tasks.push(format!(
                "✔ {} · {} · {} ms",
                field("task"),
                field("note"),
                field("duration_ms")
            )),
            "task_failed" => tasks.push(format!("✖ {} · {}", field("task"), field("error"))),
            "task_skipped" => tasks.push(format!("○ {} · skipped", field("task"))),
            "workflow_completed" => settled = Some("completed".to_owned()),
            "workflow_failed" => settled = Some(format!("failed · {}", field("error"))),
            "workflow_paused" => settled = Some("paused for a human answer".to_owned()),
            _ => {}
        }
    }
    let settled = settled.unwrap_or_else(|| {
        "no settlement line — the run may still be going, or was cut".to_owned()
    });
    format!(
        "last run · `{workflow}` · {settled} · read from `{}`\n  {}",
        trace.display(),
        if tasks.is_empty() {
            "no task line".to_owned()
        } else {
            tasks.join("\n  ")
        }
    )
}

/// What Nika calls the words another tool taught — answered from the
/// language itself, no model asked (the rival-tool persona's first wall).
const VOCABULARY: &[(&str, &str)] = &[
    (
        "node",
        "a `task` (a map entry under `tasks:` · one verb each) · the edges are `with:` bindings (data) and `after:` (order), never a canvas",
    ),
    (
        "step",
        "a `task` under `tasks:` (a map, never a list) · order is the DAG the bindings draw, not the file order",
    ),
    (
        "job",
        "a `task`, or a whole workflow file invoked as a child (`invoke: { workflow: ./child.nika.yaml }`)",
    ),
    (
        "trigger",
        "there is no trigger inside a workflow · a run is started by `nika run`, by `nika serve` (the resident firer) or by an armed cadence in the project file (`nika arm`)",
    ),
    (
        "cron",
        "an armed cadence in the project file (`nika arm` · `nika serve` fires it) · a workflow itself carries no schedule",
    ),
    (
        "schedule",
        "an armed cadence in the project file (`nika arm` · `nika serve` fires it) · a workflow itself carries no schedule",
    ),
    (
        "webhook",
        "`nika serve` is the resident door (authenticated loopback HTTP) · a workflow itself listens to nothing",
    ),
    (
        "secret",
        "`secrets:` at the envelope — store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}` · reaches an effect only through an `egress:` door",
    ),
    (
        "credential",
        "`secrets:` at the envelope — store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}`",
    ),
    (
        "action",
        "a builtin `nika:<name>` under `invoke:` (28 ship · `nika catalog --tools`) or an `mcp:<server>/<tool>` (`nika wire` adds a server)",
    ),
    (
        "plugin",
        "a builtin `nika:<name>` under `invoke:` or an MCP server (`mcp:<server>/<tool>` · `nika wire`)",
    ),
    (
        "integration",
        "an MCP server (`mcp:<server>/<tool>` under `invoke:` · `nika wire` adds one) or a builtin `nika:<name>`",
    ),
    (
        "connection",
        "an MCP server (`mcp:<server>/<tool>` under `invoke:` · `nika wire` adds one)",
    ),
    (
        "variable",
        "`inputs:` (caller-supplied · `--var k=v`) or `const:` (baked in the file) · read as `${{ inputs.x }}` / `${{ const.x }}` · `vars:` and `env:` are dead forms",
    ),
    (
        "environment variable",
        "a secret reference (`secrets: { NAME: { source: env, key: NAME } }`) · the environment is never read directly (`env:` is a dead form)",
    ),
    (
        "output",
        "`outputs:` at the envelope (`${{ tasks.x.output }}`) · a file lands through `nika:write` under a `permits.fs.write` grant",
    ),
    (
        "artifact",
        "a file landed by `nika:write` under a `permits.fs.write` grant · the run's evidence is the trace under `.nika/traces/`",
    ),
    (
        "loop",
        "`for_each: { items: … , max_parallel, fail_fast }` on a task · `${{ item }}` and `${{ index }}` inside",
    ),
    (
        "condition",
        "`when: \"${{ … }}\"` on a task (a CEL boolean) · `after: { x: failure }` routes on an outcome",
    ),
    (
        "retry",
        "`retry: { max_attempts, backoff_ms, backoff_strategy, jitter, on_codes }` on a task",
    ),
    (
        "timeout",
        "`timeout: \"30s\"` on a task (a duration string · max 24h)",
    ),
    (
        "approval",
        "a human gate · `invoke: { tool: \"nika:prompt\" }` pauses the run (exit 4) and `--resume <trace> --answer <task>=<value>` continues it",
    ),
    (
        "human",
        "a human gate · `invoke: { tool: \"nika:prompt\" }` pauses the run (exit 4) and `--resume` continues it",
    ),
    (
        "pipeline",
        "a workflow · one `.nika.yaml` file · nine envelope keys · `tasks:` a map · four verbs",
    ),
    (
        "function",
        "a `task` with one verb (`infer` · `exec` · `invoke` · `agent`) · a reusable one is a child workflow under `invoke: { workflow: … }`",
    ),
    (
        "permits",
        "the declared boundary: what the file may read (`fs.read`) · write (`fs.write`) · reach (`net.http`) · run (`exec`) · call (`tools`) · see (`env`) · absent = zero authority · a run refuses anything outside it · `nika check --infer-permits` writes the tightest block the body needs",
    ),
    (
        "inputs",
        "what the caller supplies at run time (`--var name=value`) · typed · a `default:` makes one a deployment knob · read as `${{ inputs.name }}`",
    ),
    (
        "const",
        "values baked in the file · read as `${{ const.name }}` · never a secret",
    ),
    (
        "secrets",
        "store references only (`{ source: env, key: NAME }`), never a value · read as `${{ secrets.NAME }}` · reaches an effect only through an `egress:` door",
    ),
    (
        "tasks",
        "the work: a map keyed by task id, one verb each (`infer` · `exec` · `invoke` · `agent`) · the order is the DAG the `with:` bindings and `after:` edges draw",
    ),
    (
        "outputs",
        "what the workflow returns (`name: ${{ tasks.x.output }}`) · the only place a task's output is read outside `with:`",
    ),
    (
        "model",
        "the default seat for every `infer` · `<provider>/<name>` · `mock/echo` rehearses offline · a task may name its own",
    ),
    (
        "with",
        "a task's bindings · `with: { name: \"${{ tasks.x.output }}\" }` IS the data edge · read inside the task as `${{ with.name }}`",
    ),
    (
        "after",
        "an order edge without data · `after: { x: success }` (or `failure` · `skipped` · `terminal` · `unwind`)",
    ),
    (
        "infer",
        "the verb for one model call · `prompt` (required) · `system` · `model` · `temperature` · `max_tokens` · `schema` for structured output",
    ),
    (
        "exec",
        "the verb for a process · `command: [\"prog\", \"arg\"]` (argv · no shell) or `shell: \"…\"` (the explicit door) · needs `permits.exec`",
    ),
    (
        "invoke",
        "the verb for a builtin (`nika:<name>`) · an MCP tool (`mcp:<server>/<tool>`) · or a child workflow (`workflow: ./x.nika.yaml`)",
    ),
    (
        "agent",
        "the verb for a governed multi-turn loop · `prompt` · `tools: [globs · default-deny]` · `max_turns` · `max_tokens_total`",
    ),
];

fn vocabulary(lower: &str) -> Option<String> {
    let asks = any(
        lower,
        &[
            "what do you call",
            "what is the word",
            "is there a",
            "how do i",
            "equivalent",
            "instead of",
            "in nika",
            "nika word",
            "nika term",
            "do you have",
            "what is",
            "what are",
            "what does",
            "meaning of",
            "explain",
        ],
    );
    if !asks {
        return None;
    }
    let mut lines: Vec<String> = VOCABULARY
        .iter()
        .filter(|(word, _)| {
            let w = format!(" {word}");
            lower.contains(&w) || lower.starts_with(word)
        })
        .map(|(word, meaning)| format!("{word} → {meaning}"))
        .collect();
    if lines.is_empty() {
        return None;
    }
    lines.truncate(4);
    Some(format!(
        "what Nika calls it:\n  {}\n  (exact shapes: ask for the schema · `nika spec --canon`)",
        lines.join("\n  ")
    ))
}

fn any(lower: &str, words: &[&str]) -> bool {
    words.iter().any(|w| lower.contains(w))
}

/// The workflow the input names, when the snapshot holds exactly one match.
fn named_workflow(input: &str, snapshot: &ProjectSnapshot) -> Option<String> {
    input
        .split(|c: char| {
            c.is_whitespace() || c == '`' || c == '"' || c == '\'' || c == ',' || c == '?'
        })
        .filter(|t| !t.is_empty())
        .find_map(|t| snapshot.find(t).map(|w| w.path.clone()))
}

/// The ONE facade's verdict on a workflow the snapshot holds.
fn verdict(root: &Path, snapshot: &ProjectSnapshot, rel: &str) -> String {
    let path = root.join(rel);
    let Ok(source) = std::fs::read_to_string(&path) else {
        return format!("`{rel}` could not be read");
    };
    let base = path.parent().map(Path::to_path_buf);
    let mut read = |p: &str| std::fs::read_to_string(p).map_err(|e| e.to_string());
    match nika_cli_host::oracle::audit_source(
        &source,
        &path.display().to_string(),
        Some(&mut read),
        base.as_deref(),
        nika_cli_host::oracle::AuditOptions::default(),
    ) {
        Ok(audit) => {
            let v = &audit.verdict;
            let mut lines = vec![format!(
                "`{rel}` · {} · valid {} · access ready {} · capacity fit {} · run ready {} · grade {} (authority and spend, not danger)",
                if v.clean { "clean" } else { "findings" },
                tick(Some(v.layers.valid)),
                tick(v.layers.access_ready),
                tick(Some(v.layers.capacity_fit)),
                tick(v.layers.run_ready()),
                v.grade.as_str()
            )];
            for f in audit.report.findings.iter().take(6) {
                lines.push(format!(
                    "  · {} · {}",
                    f.code.as_deref().unwrap_or("-"),
                    f.message
                ));
            }
            for b in &v.layers.blockers {
                lines.push(format!("  · {b}"));
            }
            for h in audit.report.hints.iter().take(3) {
                lines.push(format!("  · hint · {} · {}", h.kind, h.advice));
            }
            if audit.report.findings.len() > 6 {
                lines.push(format!(
                    "  · … {} more (`nika check {rel}`)",
                    audit.report.findings.len() - 6
                ));
            }
            let _ = snapshot;
            lines.join("\n")
        }
        Err(e) => format!("`{rel}` does not parse: {}", e.diagnostic()),
    }
}

fn tick(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "✔",
        Some(false) => "✖",
        None => "○",
    }
}

/// The ONE router's answer for an authoring intent.
fn route(input: &str) -> String {
    match nika_onboard::routing::route_query(input) {
        nika_onboard::routing::RoutedEntry::Example(slug) => {
            format!(
                "the example `{slug}` fits — read it with `nika try {slug}`, own it with `nika new {slug} <file>`"
            )
        }
        nika_onboard::routing::RoutedEntry::Skeleton(name) => {
            format!(
                "the template `{name}` fits — `nika new {name} <file>` lays it down with its SLOT lines"
            )
        }
        nika_onboard::routing::RoutedEntry::Clarify(options) => format!(
            "closest shapes: {} — name one, or say more about the job",
            options.join(" · ")
        ),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("alpha.nika.yaml"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("a");
        std::fs::write(
            dir.path().join("curl.nika.yaml"),
            "nika: curl\nmodel: mock/echo\npermits: { exec: [\"curl\"], net: { http: [\"example.com\"] } }\ntasks:\n  fetch:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n",
        )
        .expect("c");
        dir
    }

    /// The facts answer without a model: the workflows, the builtins, the
    /// providers, a verdict, a code, a shape — and stay silent otherwise.
    #[test]
    fn the_facts_answer_from_the_engine_and_stay_silent_otherwise() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let root = dir.path();
        assert!(
            answer("what workflows are here?", &snap, root)
                .expect("workflows")
                .contains("alpha.nika.yaml")
        );
        let builtins = answer("which builtins exist?", &snap, root).expect("builtins");
        assert!(
            builtins.contains("nika:read") && builtins.contains("nika:jq"),
            "{builtins}"
        );
        let providers = answer("which providers are supported?", &snap, root).expect("providers");
        assert!(
            providers.contains("mistral") && providers.contains("ollama"),
            "{providers}"
        );
        let verdict = answer("is alpha valid? check it", &snap, root).expect("verdict");
        assert!(
            verdict.contains("`alpha.nika.yaml` · clean") && verdict.contains("valid ✔"),
            "{verdict}"
        );
        assert!(
            verdict.contains("· grade ") && verdict.contains("(authority and spend, not danger)"),
            "the grade is named for what it is: {verdict}"
        );
        assert!(!verdict.contains("risk "), "never the word risk: {verdict}");
        let hinted = answer("is curl valid?", &snap, root).expect("verdict with hints");
        assert!(
            hinted.contains("· hint ·") && hinted.contains("nika:fetch"),
            "the report's hints ride the verdict fact: {hinted}"
        );
        let explain = answer("explain NIKA-AUTH-006", &snap, root).expect("explain");
        assert!(explain.contains("NIKA-AUTH-006"), "{explain}");
        let shape = answer(
            "which example fetches a url and summarizes it?",
            &snap,
            root,
        )
        .expect("shape");
        assert!(
            shape.contains("nika new") || shape.contains("closest shapes"),
            "{shape}"
        );
        assert!(
            answer("write me a poem about the sea", &snap, root).is_none(),
            "not a fact"
        );
        let vocab = answer(
            "what do you call a trigger here? and a secret?",
            &snap,
            root,
        )
        .expect("vocabulary");
        assert!(
            vocab.contains("trigger →")
                && vocab.contains("secret →")
                && vocab.contains("`secrets:`"),
            "{vocab}"
        );
        assert!(
            answer("is there a node concept?", &snap, root)
                .is_some_and(|v| v.contains("node → a `task`"))
        );
        let none_yet = answer("what happened in the last run?", &snap, root).expect("a fact");
        assert!(none_yet.contains("no run yet"), "{none_yet}");
        let store = root.join(".nika").join("traces");
        std::fs::create_dir_all(&store).expect("store");
        std::fs::write(
            store.join("2026-09-03T00-00-00Z-abcd.ndjson"),
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"digest\"}]}\n{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"note\",\"value\":\"invoke · nika:read\"},{\"key\":\"duration_ms\",\"value\":2}]}\n{\"kind\":\"task_failed\",\"fields\":[{\"key\":\"task\",\"value\":\"sum\"},{\"key\":\"error\",\"value\":\"NIKA-INFER-001 · no seat\"}]}\n{\"kind\":\"workflow_failed\",\"fields\":[{\"key\":\"error\",\"value\":\"task sum failed\"}]}\n",
        )
        .expect("trace");
        let last = answer("what happened in the last run?", &snap, root).expect("a fact");
        assert!(
            last.contains("last run · `digest` · failed · task sum failed")
                && last.contains("✔ read · invoke · nika:read · 2 ms")
                && last.contains("✖ sum · NIKA-INFER-001"),
            "read from the trace, never from memory: {last}"
        );
        let permits = answer("what is permits?", &snap, root).expect("the language's own word");
        assert!(
            permits.contains("permits →") && permits.contains("boundary"),
            "{permits}"
        );
        assert!(
            answer("tell me about the sea", &snap, root).is_none(),
            "a foreign word alone is not a question"
        );
    }
}
