// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native-first hints — `exec:` with a probable native path
//! (the `native-first/*` preference ruleset · spec 03 §native-first).
//!
//! An agent authoring a workflow reaches for the shell because the
//! shell always works — and `nika check` stayed silent about the
//! builtin that would have made the task portable, auditable and
//! sandboxed. This pass names the native path. Empirical trigger: a
//! generated site-asset workflow shipped FOUR avoidable `exec` tasks
//! (curl crawl · node upload helper · shell jq · curl image API) and
//! checked clean.
//!
//! Five deterministic rules, each keyed on LITERAL command fragments
//! (`RawCommand::argv_program` / leading shell token / fragments — a
//! templated head makes no claim):
//!
//! - `native-first/001 exec-http` — `curl`/`wget`/`xh`/`http(s)` (or an
//!   interpreter one-liner around `fetch(`/`axios`/`http.request`) →
//!   `nika:fetch` (uploads: `multipart:` · crawls: `traverse:`).
//! - `native-first/002 exec-file` — `cat`/`tee`/`cp`/`mv`/`mkdir`/
//!   `touch`/`head`/`tail`/`ls` → `nika:read`/`nika:write`/`nika:glob`.
//! - `native-first/003 exec-data` — `jq`/`sed`/`awk` → `nika:jq` (or an
//!   `output:` binding — the same jq engine, zero subprocess).
//! - `native-first/004 exec-media` — an image/speech provider endpoint
//!   in the command → `nika:image_generate`/`nika:tts_generate`.
//! - `native-first/005 exec-helper` — an interpreter running a script
//!   file → inventory the helper (HTTP→fetch · files→read/write ·
//!   JSON→jq · product APIs→an MCP server) and keep only a genuine
//!   subprocess, recorded in the exec ledger.
//!
//! ADVISORY like every hint (`is_clean` ignores them) — the strict
//! posture is the CLI's: `nika check --native-strict` fails on any
//! `native-first` hint. One hint per task (the most specific rule
//! wins: helper ≻ media ≻ http ≻ file ≻ data); `nika run …` nested
//! invocations are never flagged (the sanctioned composition path).

use crate::raw::{RawAction, RawCommand, RawWorkflow};

use super::hints::Hint;

/// The hint kind (agents route on it · `hints.rs` kind registry).
const KIND: &str = "native-first";

/// HTTP client programs (001).
const HTTP_PROGRAMS: [&str; 5] = ["curl", "wget", "xh", "http", "https"];
/// File-op programs (002).
const FILE_PROGRAMS: [&str; 9] = [
    "cat", "tee", "cp", "mv", "mkdir", "touch", "head", "tail", "ls",
];
/// Data-transform programs (003).
const DATA_PROGRAMS: [&str; 3] = ["jq", "sed", "awk"];
/// Script interpreters (005 · and the 001 one-liner carve-in).
const INTERPRETERS: [&str; 10] = [
    "node", "python", "python3", "bash", "sh", "zsh", "deno", "bun", "ruby", "perl",
];
/// Script-file suffixes an interpreter head promotes to 005.
const SCRIPT_SUFFIXES: [&str; 8] = [".mjs", ".cjs", ".js", ".ts", ".py", ".sh", ".rb", ".pl"];
/// Media provider endpoint markers (004) — endpoint fragments, not
/// bare words (a prompt mentioning "tts" must not fire).
const MEDIA_MARKERS: [&str; 4] = [
    "images/generations",
    "images/edits",
    "/v1/audio/speech",
    "api.elevenlabs.io",
];
/// HTTP one-liner markers inside interpreter code (001 carve-in).
const HTTP_CODE_MARKERS: [&str; 4] = ["fetch(", "axios", "http.request", "https.request"];

/// Scan every `exec:` task (and `on_finally` cleanups) for a native
/// path — at most one hint per task.
pub(super) fn scan(wf: &RawWorkflow) -> Vec<Hint> {
    let mut hints = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        push_native_first(&task.value.action, id, &mut hints);
        for cleanup in &task.value.on_finally {
            push_native_first(&cleanup.value.action, id, &mut hints);
        }
    }
    hints
}

fn push_native_first(action: &RawAction, id: &str, hints: &mut Vec<Hint>) {
    let RawAction::Exec(exec) = action else {
        return;
    };
    if let Some(advice) = native_advice(&exec.command) {
        hints.push(Hint {
            kind: KIND,
            task: id.to_owned(),
            advice,
        });
    }
}

/// The rule ladder — most specific first, one verdict.
fn native_advice(command: &RawCommand) -> Option<String> {
    let head = command_head(command)?;
    if head == "nika" {
        return None; // nested `nika run …` — the sanctioned composition
    }
    let fragments = command.text_fragments();
    let has_marker = |markers: &[&str]| {
        fragments
            .iter()
            .any(|f| markers.iter().any(|m| f.contains(m)))
    };
    let is_interpreter = INTERPRETERS.contains(&head.as_str());

    // 005 · interpreter + script file — the helper umbrella.
    if is_interpreter
        && fragments
            .iter()
            .any(|f| !f.contains("${{") && SCRIPT_SUFFIXES.iter().any(|suffix| f.ends_with(suffix)))
    {
        return Some(format!(
            "native-first/005 · `{head}` runs a helper script — inventory it: \
             HTTP calls → `nika:fetch` (uploads: `multipart:` · crawls: `traverse:`) · \
             file I/O → `nika:read`/`nika:write` · JSON shaping → `nika:jq` · \
             a product API → wrap it as an MCP server (`mcp:<server>/<tool>`); \
             keep `exec:` only for a genuine subprocess and record it in the exec ledger"
        ));
    }
    // 004 · a media provider endpoint in the command.
    if has_marker(&MEDIA_MARKERS) {
        return Some(format!(
            "native-first/004 · `{head}` calls a media provider endpoint — \
             `nika:image_generate`/`nika:tts_generate` cover generation natively \
             (provider-portable · provenance manifest · permits.fs-gated saves)"
        ));
    }
    // 001 · an HTTP client program, or an interpreter one-liner around one.
    if HTTP_PROGRAMS.contains(&head.as_str()) || (is_interpreter && has_marker(&HTTP_CODE_MARKERS))
    {
        return Some(format!(
            "native-first/001 · `{head}` is an HTTP fetch — `nika:fetch` covers \
             GET/POST + extraction (SSRF-defended · permits-gated); uploads take \
             `multipart:` · crawls take `traverse:` (builtins-v0.1.md §nika:fetch)"
        ));
    }
    // 002 · file plumbing.
    if FILE_PROGRAMS.contains(&head.as_str()) {
        return Some(format!(
            "native-first/002 · `{head}` is file plumbing — `nika:read`/`nika:write` \
             (`create_dirs: true` replaces mkdir) / `nika:glob` cover it inside the \
             permits.fs boundary (no subprocess)"
        ));
    }
    // 003 · data transforms.
    if DATA_PROGRAMS.contains(&head.as_str()) {
        return Some(format!(
            "native-first/003 · `{head}` reshapes data — `nika:jq` (or an `output:` \
             jq binding) covers JSON in-process · `nika:edit` covers in-place \
             literal file edits (one data language · no quoting traps)"
        ));
    }
    None
}

/// The literal program head: `argv[0]` for the argv form, the first
/// non-assignment token of a shell string. Basename'd (`/usr/bin/curl`
/// counts as `curl`); a templated head (`${{ … }}`) makes no claim.
fn command_head(command: &RawCommand) -> Option<String> {
    let raw = match command {
        RawCommand::Argv(_) => command.argv_program()?.to_owned(),
        RawCommand::Shell(s) => {
            let mut tokens = s.value.split_whitespace();
            // `FOO=bar cmd …` — skip leading env assignments (a `=`
            // before any `/` is an assignment, not a path).
            loop {
                let token = tokens.next()?;
                let is_assignment = token
                    .split_once('=')
                    .is_some_and(|(name, _)| !name.contains('/') && !name.is_empty());
                if !is_assignment {
                    break token.to_owned();
                }
            }
        }
    };
    if raw.contains("${{") {
        return None; // templated — runtime business, no static claim
    }
    let head = raw.rsplit('/').next().unwrap_or(&raw);
    (!head.is_empty()).then(|| head.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn hints_of(yaml: &str) -> Vec<Hint> {
        scan(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn exec_wf(command_yaml: &str) -> String {
        format!(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: {{ command: {command_yaml} }}\n"
        )
    }

    fn sole_native_hint(yaml: &str) -> Hint {
        let hints = hints_of(yaml);
        let native: Vec<&Hint> = hints.iter().filter(|h| h.kind == "native-first").collect();
        assert_eq!(native.len(), 1, "exactly one per task: {hints:?}");
        native[0].clone()
    }

    #[test]
    fn fires_on_curl_naming_fetch_001() {
        let h = sole_native_hint(&exec_wf(
            "\"curl -s https://api.test/items -H 'x-api-key: k'\"",
        ));
        assert_eq!(h.task, "t");
        assert!(h.advice.contains("native-first/001"), "{h:?}");
        assert!(h.advice.contains("nika:fetch"), "{h:?}");
    }

    #[test]
    fn fires_on_argv_wget_and_pathed_program() {
        let h = sole_native_hint(&exec_wf("[\"/usr/bin/wget\", \"https://x.test\"]"));
        assert!(h.advice.contains("native-first/001"), "{h:?}");
    }

    #[test]
    fn fires_on_node_helper_script_005_not_001() {
        // An interpreter + script file is the HELPER class even when the
        // script name suggests HTTP — the umbrella advice owns it.
        let h = sole_native_hint(&exec_wf(
            "[\"node\", \"workflows/site/bin/crawl-and-upload.mjs\", \"--url\", \"${{ vars.site }}\"]",
        ));
        assert!(h.advice.contains("native-first/005"), "{h:?}");
        assert!(h.advice.contains("exec ledger"), "{h:?}");
    }

    #[test]
    fn fires_on_python_inline_fetch_as_001() {
        let h = sole_native_hint(&exec_wf(
            "\"python3 -c 'import urllib; fetch(\\\"https://x.test\\\")'\"",
        ));
        assert!(h.advice.contains("native-first/001"), "{h:?}");
    }

    #[test]
    fn fires_on_media_endpoint_004_over_http_001() {
        let h = sole_native_hint(&exec_wf(
            "\"curl -X POST https://api.openai.com/v1/images/generations -d @body.json\"",
        ));
        assert!(h.advice.contains("native-first/004"), "{h:?}");
        assert!(h.advice.contains("nika:image_generate"), "{h:?}");
    }

    #[test]
    fn fires_on_file_and_data_programs() {
        let file = sole_native_hint(&exec_wf("\"cat out/manifest.json\""));
        assert!(file.advice.contains("native-first/002"), "{file:?}");
        let data = sole_native_hint(&exec_wf(
            "\"jq '.items | map(.name)' out/crawl.json > out/names.json\"",
        ));
        assert!(data.advice.contains("native-first/003"), "{data:?}");
        let env_prefixed = sole_native_hint(&exec_wf("\"LC_ALL=C sed s/a/b/ in.txt\""));
        assert!(
            env_prefixed.advice.contains("native-first/003"),
            "assignments are skipped to the real head: {env_prefixed:?}"
        );
    }

    #[test]
    fn silent_on_genuine_subprocesses() {
        for command in [
            "\"cargo test --workspace --lib\"",
            "\"git commit -m 'x'\"",
            "[\"qrt\", \"product\", \"create\", \"--json\"]",
            "\"make release\"",
            "\"nika run subroutine.nika.yaml\"",
            "\"${{ vars.tool }} --flag\"",
        ] {
            let hints = hints_of(&exec_wf(command));
            assert!(
                !hints.iter().any(|h| h.kind == "native-first"),
                "{command} must stay silent: {hints:?}"
            );
        }
    }

    #[test]
    fn silent_on_interpreter_without_script_or_http() {
        // A bare interpreter computation is a legitimate subprocess.
        let hints = hints_of(&exec_wf("\"python3 -c 'print(6*7)'\""));
        assert!(!hints.iter().any(|h| h.kind == "native-first"), "{hints:?}");
    }

    #[test]
    fn on_finally_cleanups_are_scanned_too() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"make build\" }\n    on_finally:\n      - exec: { command: \"curl -X POST https://hooks.test/done\" }\n";
        let hints = hints_of(yaml);
        assert!(
            hints
                .iter()
                .any(|h| h.kind == "native-first" && h.task == "t"),
            "{hints:?}"
        );
    }

    #[test]
    fn the_site_asset_regression_fixture_yields_all_four_hints() {
        // The genericized empirical trigger: a site-asset workflow whose
        // four exec tasks are all natively expressible. Spec-VALID (it
        // parses + checks) yet every task earns its native-first hint.
        let yaml = r#"nika: v1
workflow: site-asset
model: mock/echo
tasks:
  - id: crawl_site
    exec: { command: "curl -s https://acme.test -o out/site.html" }
  - id: upload_background
    depends_on: [crawl_site]
    exec:
      command: ["node", "workflows/site/bin/helper.mjs", "upload", "--file", "out/bg.png"]
  - id: render_background
    depends_on: [crawl_site]
    exec: { command: "curl -X POST https://api.openai.com/v1/images/generations" }
  - id: write_manifest
    depends_on: [upload_background, render_background]
    exec: { command: "jq -n '{done: true}' > out/manifest.json" }
"#;
        let hints = hints_of(yaml);
        let by_task: Vec<(&str, &str)> = hints
            .iter()
            .filter(|h| h.kind == "native-first")
            .map(|h| {
                let rule = h
                    .advice
                    .split_once(" · ")
                    .map(|(rule, _)| rule)
                    .unwrap_or_default();
                (h.task.as_str(), rule)
            })
            .collect();
        assert_eq!(
            by_task,
            vec![
                ("crawl_site", "native-first/001"),
                ("upload_background", "native-first/005"),
                ("render_background", "native-first/004"),
                ("write_manifest", "native-first/003"),
            ],
            "{hints:?}"
        );
    }
}
