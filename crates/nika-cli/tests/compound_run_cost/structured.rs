// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Generated grounded shape, exercised through the retained real TUI/Run seam.
//! HTTP bodies are fixtures: this is not provider or billing qualification.
use super::*;

const GROUNDED: &str = include_str!("../fixtures/run_cost_grounded.nika");
fn bound() -> u32 {
    1 + u32::from(nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET)
}
fn reply(root: &Path, call: u32, anchor: &str) {
    let body = serde_json::json!({"body":"Grounded summary", "facts_used":[
        {"claim":"The source mentions alpha", "anchor":anchor}
    ]});
    std::fs::write(root.join(format!("reply-{call}.txt")), body.to_string()).unwrap();
}
fn fail_without_write(root: &Path, calls_expected: usize) {
    assert_eq!(calls(root), calls_expected);
    assert!(!root.join("output.txt").exists());
    let events = std::fs::read_to_string(root.join("events.ndjson")).unwrap();
    assert!(events.contains("task_failed"), "{events}");
}

fn check_binary(root: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["check", "one.nika", "--json", "--access", "api"])
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root)
        .env("NIKA_KEYCHAIN", "off")
        .env("OPENAI_API_KEY", "fixture-not-a-key")
        .env(
            "NIKA_OPENAI_BASE_URL",
            "https://api.scaleway.ai/example-project/v1/chat/completions",
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn actual_check_projects_fresh_choice_without_claiming_runtime_admission() {
    let source = GROUNDED
        .replace(MODEL, "openai/gpt-oss-120b")
        .replace("max_tokens: 32", "max_tokens: 1200");
    let root = new_root(&source);
    let checked = check_binary(root.path());
    assert_eq!(checked["verdicts"]["valid"], true, "{checked}");
    assert_eq!(checked["verdicts"]["access_ready"], true, "{checked}");
    assert_eq!(checked["verdicts"]["run_ready"], false, "{checked}");
    assert_eq!(checked["judged"]["runtime_admission"], false, "{checked}");
    assert!(
        checked["verdicts"]["blockers"]
            .to_string()
            .contains("at most 3 requests")
    );
    assert_unspent(root.path());
}

#[test]
fn local_assert_only_check_and_run_still_succeed_without_money_choice() {
    let root = new_root(
        "nika: local\npermits: { tools: ['nika:assert'] }\ntasks:\n  ok:\n    invoke: { tool: 'nika:assert', args: { condition: true } }\n",
    );
    let checked = check_binary(root.path());
    assert_eq!(checked["verdicts"]["run_ready"], true, "{checked}");
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["run", "one.nika", "--json"])
        .current_dir(root.path())
        .env_clear()
        .env("HOME", root.path())
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !root
            .path()
            .join(".nika/inference-cost-observations.ndjson")
            .exists()
    );
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn grounded_schema_reasks_share_one_finite_choice_and_write_only_after_assert() {
    let root = new_root(GROUNDED);
    // Earlier replies are non-JSON. Only the last allowed reply validates.
    reply(root.path(), bound(), "alpha café");
    std::fs::write(
        root.path().join("attempt-extra"),
        "must refuse without HTTP",
    )
    .unwrap();
    let mut p = spawn(root.path());
    ask(&mut p, bound());
    assert_unspent(root.path());
    answer(&mut p, true);
    assert_eq!(calls(root.path()), bound() as usize);
    assert_eq!(
        std::fs::read_to_string(root.path().join("output.txt")).unwrap(),
        "Grounded summary"
    );
    assert_observation(root.path(), u64::from(bound()), u64::from(bound()));
    let sent = wire(root.path());
    assert!(sent.iter().all(|r| r["max_tokens"] == 32));
    assert!(sent[0]["messages"].to_string().contains("facts_used"));
    assert!(
        sent.last().unwrap()["messages"].as_array().unwrap().len()
            > sent[0]["messages"].as_array().unwrap().len()
    );
    leave(&mut p);
}

#[test]
fn exhausted_schema_budget_and_failed_anchor_never_write_or_fall_back() {
    for bad_anchor in [false, true] {
        let root = new_root(GROUNDED);
        if bad_anchor {
            reply(root.path(), 1, "not in the source");
        } else {
            std::fs::write(root.path().join("attempt-extra"), "no fourth request").unwrap();
        }
        let mut p = spawn(root.path());
        ask(&mut p, bound());
        answer(&mut p, true);
        let count = if bad_anchor { 1 } else { bound() };
        fail_without_write(root.path(), count as usize);
        assert_observation(root.path(), u64::from(bound()), u64::from(count));
        leave(&mut p);
    }
}

#[test]
fn structured_transport_uncertainty_stops_but_complete_unknown_answers_stay_bounded() {
    for uncertain in [true, false] {
        let root = new_root(GROUNDED);
        let expected = if uncertain { 1 } else { bound() };
        if uncertain {
            std::fs::write(root.path().join("uncertain-1"), "possibly billed").unwrap();
        } else {
            for call in 1..=bound() {
                std::fs::write(
                    root.path().join(format!("missing-usage-{call}")),
                    "usage absent",
                )
                .unwrap();
            }
            std::fs::write(root.path().join("attempt-extra"), "no fourth request").unwrap();
        }
        let mut p = spawn(root.path());
        ask(&mut p, bound());
        answer(&mut p, true);
        fail_without_write(root.path(), expected as usize);
        let observation = observation(root.path());
        assert_eq!(observation["observation"]["unknown_calls"], expected);
        assert!(
            observation["observation"]["unknown_attempts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|attempt| attempt["estimated_nano_usd"].is_null())
        );
        leave(&mut p);
    }
}

#[test]
fn grounded_declined_or_stale_source_and_read_bytes_do_not_dispatch() {
    for change in ["decline", "source", "read"] {
        let root = new_root(GROUNDED);
        let mut p = spawn(root.path());
        ask(&mut p, bound());
        if change == "decline" {
            answer(&mut p, false);
        } else {
            let (path, bytes) = if change == "source" {
                ("one.nika", format!("{GROUNDED}\n# changed after review\n"))
            } else {
                ("input.txt", "changed source bytes".into())
            };
            std::fs::write(root.path().join(path), bytes).unwrap();
            p.send("yes\r").unwrap();
            p.expect("changed").unwrap();
            p.expect("nika ›").unwrap();
        }
        assert_unspent(root.path());
        leave(&mut p);
    }
}

#[test]
fn zero_ceiling_and_missing_write_permit_refuse_before_choice_or_transport() {
    let root = new_root(GROUNDED);
    std::fs::write(
        root.path().join("zero-ceiling"),
        "explicit invocation constraint",
    )
    .unwrap();
    let mut p = spawn(root.path());
    p.send("run one.nika\r").unwrap();
    p.expect("zero").unwrap();
    p.expect("ceiling").unwrap();
    p.expect("environment").unwrap();
    assert_unspent(root.path());
    leave(&mut p);

    let root = new_root(&GROUNDED.replace("  - nika:write\n", ""));
    let mut p = spawn(root.path());
    p.send("run one.nika\r").unwrap();
    p.expect("findings").unwrap();
    p.expect("started").unwrap();
    assert_unspent(root.path());
    leave(&mut p);
}
