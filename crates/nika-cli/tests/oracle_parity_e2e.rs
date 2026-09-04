// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The rig spawns the REAL binary (the CLI door and the MCP door over
// stdio); the kernel's ShellExecutor is the engine's seam, not a test's.
#![allow(clippy::disallowed_types)]
#![cfg(unix)]

//! ADR-124 · the oracle law, proven on the real binary: for the same
//! source, the direct facade (`nika_cli_host::oracle`), the CLI's
//! machine projection (`nika check --json`) and the MCP tool's semantic
//! result (`nika_check` over the real stdio transport) carry the SAME
//! verdict. Formatting may differ per door; the semantic keys may not.
//!
//! Six fixtures: a clean file · a boundary finding · a hallucinated
//! model · a templated default that judges as one · a capacity cap · a
//! native-first exec under strict. Keyless (`mock/echo` · a dead key
//! aimed at a closed port · a stub seat on PATH): the rig never dials.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const FAKE_CODEX: &str = r#"#!/bin/sh
case "$1" in
  --version) echo "codex-cli 0.0.0-stub"; exit 0 ;;
  *) echo "stub seat" >&2; exit 1 ;;
esac
"#;

const CLEAN: &str =
    "nika: parity\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n";
const BOUNDARY: &str = "nika: parity\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
const HALLUCINATED: &str = "nika: parity\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n";
const TEMPLATED: &str = "nika: parity\ninputs:\n  m: { type: string, default: \"azure/gpt-4o\" }\nmodel: \"${{ inputs.m }}\"\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n";
const CAPACITY: &str = "nika: parity\nmodel: openai/gpt-5.2\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 999999999 }\n";
const NATIVE: &str = "nika: parity\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  grab:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nika-one-door-w3-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["bin", "home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        for bin in ["codex", "codex-acp"] {
            let path = root.join("bin").join(bin);
            let mut f = std::fs::File::create(&path).expect("fake bin");
            f.write_all(FAKE_CODEX.as_bytes()).expect("fake body");
            let mut perm = std::fs::metadata(&path).expect("meta").permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&path, perm).expect("chmod");
        }
        Self { root }
    }

    fn command(&self, args: &[&str]) -> Command {
        let path = format!("{}:/usr/bin:/bin", self.root.join("bin").display());
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
        cmd.args(args)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("TERM", "dumb")
            .env("OPENAI_API_KEY", "sk-dead-key-never-accepted")
            .env("NIKA_OPENAI_BASE_URL", "http://127.0.0.1:9/v1")
            .current_dir(self.root.join("work"));
        cmd
    }

    /// The CLI door: `nika check <file> --json [flags]` → (exit, object).
    fn cli(&self, name: &str, source: &str, flags: &[&str]) -> (i32, Value) {
        std::fs::write(self.root.join("work").join(name), source).expect("workflow");
        let mut args = vec!["check", name, "--json"];
        args.extend_from_slice(flags);
        let out = self.command(&args).output().expect("binary runs");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let value: Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| panic!("check --json is one object ({e}):\n{stdout}"));
        (out.status.code().unwrap_or(-1), value)
    }

    /// The MCP door over the real stdio transport: initialize · the
    /// initialized notification · one `tools/call` → (isError, text).
    fn mcp(&self, source: &str, native_strict: bool) -> (bool, String) {
        let mut child = self
            .command(&["mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the oracle serves stdio");
        let frames = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "nika_check", "arguments": { "workflow": source, "native_strict": native_strict } }
            }),
        ];
        {
            let mut stdin = child.stdin.take().expect("stdin");
            for frame in &frames {
                writeln!(stdin, "{frame}").expect("frame written");
            }
        }
        let out = child.wait_with_output().expect("the oracle exits on EOF");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let reply = stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["id"] == 2)
            .unwrap_or_else(|| panic!("a reply to the tools/call frame:\n{stdout}"));
        let result = &reply["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("one text block: {reply}"))
            .to_owned();
        (result["isError"] == true, text)
    }
}

/// The JSON object a dirty MCP verdict carries after its header line.
fn mcp_object(text: &str) -> Value {
    let start = text
        .find('{')
        .unwrap_or_else(|| panic!("a JSON object rides the verdict: {text}"));
    serde_json::from_str(&text[start..]).unwrap_or_else(|e| panic!("valid JSON ({e}): {text}"))
}

/// The direct door: the facade in-process, default options.
fn facade(source: &str, native_strict: bool) -> Value {
    let audit = nika_cli_host::oracle::audit_source(
        source,
        "parity.nika.yaml",
        None,
        None,
        nika_cli_host::oracle::AuditOptions::default(),
    )
    .expect("the fixture parses");
    let obj = nika_cli_host::oracle::audit_json(
        &audit.wf,
        &audit.report,
        &audit.skills,
        &audit.verdict,
        nika_cli_host::oracle::Lanes::new(native_strict, false),
    )
    .expect("serializes");
    Value::Object(obj)
}

fn sorted_strings(values: Option<&Vec<Value>>, key: &str) -> Vec<String> {
    let mut out: Vec<String> = values
        .into_iter()
        .flatten()
        .filter_map(|v| v[key].as_str().map(str::to_owned))
        .collect();
    out.sort();
    out
}

/// The semantic keys two doors on the SAME machine must agree on.
fn semantic(v: &Value) -> Value {
    json!({
        "clean": v["clean"],
        "models_resolve": v["models_resolve"],
        "risk_grade": v["risk_grade"],
        "verdicts": v["verdicts"],
        "model_findings": v["model_findings"],
        "models_catalog_warnings": v["models_catalog_warnings"],
        "findings": sorted_strings(v["findings"].as_array(), "code"),
        "hints": sorted_strings(v["hints"].as_array(), "kind"),
        "native_strict_clean": v["native_strict_clean"],
    })
}

/// The subset that never reads this machine's paths — what the
/// in-process facade (a different env) must still agree on.
fn semantic_env_free(v: &Value) -> Value {
    json!({
        "clean": v["clean"],
        "models_resolve": v["models_resolve"],
        "risk_grade": v["risk_grade"],
        "valid": v["verdicts"]["valid"],
        "capacity_fit": v["verdicts"]["capacity_fit"],
        "model_findings": v["model_findings"],
        "findings": sorted_strings(v["findings"].as_array(), "code"),
        "native_strict_clean": v["native_strict_clean"],
    })
}

/// One dirty fixture through the three doors.
fn dirty_parity(rig: &Rig, name: &str, source: &str, expect_code: Option<&str>) {
    let (code, cli) = rig.cli(&format!("{name}.nika.yaml"), source, &[]);
    assert_eq!(code, 2, "{name}: the CLI refuses\n{cli:#}");
    assert_eq!(cli["clean"], false, "{name}: {cli:#}");
    let (is_error, text) = rig.mcp(source, false);
    assert!(is_error, "{name}: the oracle refuses: {text}");
    let mcp = mcp_object(&text);
    assert_eq!(
        semantic(&cli),
        semantic(&mcp),
        "{name}: CLI --json and the MCP tool disagree\nCLI: {cli:#}\nMCP: {mcp:#}"
    );
    assert_eq!(
        semantic_env_free(&cli),
        semantic_env_free(&facade(source, false)),
        "{name}: the facade and the CLI disagree"
    );
    // The two doors say what they judged: the CLI read the filesystem,
    // the oracle had none.
    assert_eq!(cli["judged"]["composition"], true, "{cli:#}");
    assert_eq!(mcp["judged"]["composition"], false, "{mcp:#}");
    assert!(
        mcp["next_actions"].as_array().is_some(),
        "the oracle's own decoration rides beside the shared keys: {mcp:#}"
    );
    if let Some(code) = expect_code {
        assert!(
            semantic(&cli)["findings"]
                .as_array()
                .is_some_and(|c| c.iter().any(|v| v == code)),
            "{name}: the finding {code} is the cause: {cli:#}"
        );
    }
}

/// A clean file is clean on the three doors, with the same grade.
#[test]
fn a_clean_file_is_clean_on_the_three_doors() {
    let rig = Rig::new("clean");
    let (code, cli) = rig.cli("clean.nika.yaml", CLEAN, &[]);
    assert_eq!(code, 0, "{cli:#}");
    assert_eq!(cli["clean"], true, "{cli:#}");
    let (is_error, text) = rig.mcp(CLEAN, true);
    assert!(!is_error, "{text}");
    assert!(text.contains("✔ clean"), "{text}");
    let grade = cli["risk_grade"].as_str().expect("grade");
    assert!(
        text.to_lowercase()
            .contains(&format!("risk {}", grade.to_lowercase())),
        "the oracle names the CLI's grade `{grade}`: {text}"
    );
    let direct = facade(CLEAN, false);
    assert_eq!(semantic_env_free(&cli), semantic_env_free(&direct));
    assert_eq!(direct["judged"]["composition"], false);
}

/// A boundary finding (an effect with no grant) is the same finding on
/// the three doors.
#[test]
fn a_boundary_finding_is_the_same_finding_on_the_three_doors() {
    dirty_parity(
        &Rig::new("boundary"),
        "boundary",
        BOUNDARY,
        Some("NIKA-AUTH-006"),
    );
}

/// A provider this binary cannot drive is a MODELS finding on every
/// door — the oracle no longer carries its own cross-check.
#[test]
fn a_hallucinated_model_is_a_finding_on_the_three_doors() {
    let rig = Rig::new("hallucinated");
    dirty_parity(&rig, "hallucinated", HALLUCINATED, None);
    let (_, cli) = rig.cli("hallucinated.nika.yaml", HALLUCINATED, &[]);
    assert_eq!(cli["models_resolve"], false, "{cli:#}");
    assert!(
        cli["model_findings"]
            .as_array()
            .is_some_and(|r| r.iter().any(|f| f["model"] == "azure/gpt-4o")),
        "{cli:#}"
    );
}

/// A templated `model:` judges as its declared default on every door —
/// the law the oracle's old cross-check did not know.
#[test]
fn a_templated_default_judges_as_its_default_on_the_three_doors() {
    let rig = Rig::new("templated");
    dirty_parity(&rig, "templated", TEMPLATED, None);
    let (_, text) = rig.mcp(TEMPLATED, false);
    let mcp = mcp_object(&text);
    assert!(
        mcp["model_findings"]
            .as_array()
            .is_some_and(|r| r.iter().any(|f| f["why"]
                .as_str()
                .is_some_and(|w| w.contains("declared default")))),
        "the oracle judges the declared default: {mcp:#}"
    );
}

/// A capacity cap is red on the three doors, and `verdicts` says which
/// layer failed on both machine lanes.
#[test]
fn a_capacity_cap_is_red_on_the_three_doors() {
    let rig = Rig::new("capacity");
    dirty_parity(&rig, "capacity", CAPACITY, None);
    let (_, text) = rig.mcp(CAPACITY, false);
    let mcp = mcp_object(&text);
    assert_eq!(mcp["verdicts"]["capacity_fit"], false, "{mcp:#}");
    assert_eq!(mcp["verdicts"]["valid"], true, "{mcp:#}");
}

/// A native-first exec is clean by default and red under strict — on
/// the three doors, with the same lane key.
#[test]
fn the_native_strict_lane_folds_the_same_hint_on_the_three_doors() {
    let rig = Rig::new("native");
    let (code, cli) = rig.cli("native.nika.yaml", NATIVE, &[]);
    assert_eq!(code, 0, "advisory: clean\n{cli:#}");
    assert_eq!(cli["clean"], true, "{cli:#}");
    let (code, strict) = rig.cli("native.nika.yaml", NATIVE, &["--native-strict"]);
    assert_eq!(code, 2, "strict: red\n{strict:#}");
    assert_eq!(strict["native_strict_clean"], false, "{strict:#}");
    let (is_error, text) = rig.mcp(NATIVE, true);
    assert!(is_error, "the oracle's strict default refuses: {text}");
    assert!(text.contains("native-first"), "{text}");
    let (is_error, text) = rig.mcp(NATIVE, false);
    assert!(!is_error, "advisory on the oracle: {text}");
    let direct = facade(NATIVE, true);
    assert_eq!(direct["native_strict_clean"], false, "{direct:#}");
    assert_eq!(semantic_env_free(&strict), semantic_env_free(&direct));
}
