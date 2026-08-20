// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The agent-run contract, pinned against the KIT SOURCE (RAMS-17 ·
//! operator 2026-07-31): the friction was written, not technical. The
//! prose predated `nika guard`; these tests keep the kit's teaching
//! aligned with the structure that actually judges (guard at the hook
//! · the cap at the flag · the gate at the pause).
//!
//! The kit lives in THIS repo (`.agents/plugins/nika` — the marketplace
//! repo is a mirror), so a `--lib` test can read it hermetically. A
//! path move fails loudly here before it silently un-pins the law.

use std::path::PathBuf;

/// The kit source root, resolved from this crate's manifest — never
/// the mirror.
fn kit() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.agents/plugins/nika")
}

fn read(rel: &str) -> String {
    let path = kit().join(rel);
    let text = std::fs::read_to_string(&path);
    assert!(
        text.is_ok(),
        "kit surface `{rel}` must exist at {path:?}: {text:?}"
    );
    text.unwrap_or_default()
}

/// Every prose surface of the kit that teaches the run posture.
const TEACHING_SURFACES: &[&str] = &[
    "scripts/session-context.sh",
    "rules/nika-delegation.mdc",
    "agents/nika-author.md",
    "agents/nika-migrator.md",
    "agents/nika-debugger.md",
    "README.md",
];

/// LAW (acceptance 1): the ownerless prohibitions are gone. No surface
/// says « running is the human's move » (the pre-guard law) and no
/// surface says « never run the workflow » / « never executes them »
/// without its reason. `commands/doctor.md` keeps « the human's move »
/// for FIXES (credentials · env dies with the process) — that clause
/// is about fixes, not running, and stays.
#[test]
fn no_ownerless_run_prohibition_survives_in_the_kit() {
    for rel in TEACHING_SURFACES {
        let text = read(rel).to_ascii_lowercase();
        assert!(
            !text.contains("running is the human's move"),
            "{rel}: the pre-guard law is back"
        );
        assert!(
            !text.contains("never run the workflow"),
            "{rel}: a bare prohibition with no reason is back"
        );
        assert!(
            !text.contains("never executes them"),
            "{rel}: a bare prohibition with no reason is back"
        );
    }
    // The kept clause keeps its OWN ground: the doctor's fixes stay the
    // human's (this pin makes the exemption deliberate, not forgotten).
    assert!(
        read("commands/doctor.md").contains("human's move"),
        "doctor's fixes remain the human's — credentials and env die with the agent process"
    );
}

/// LAW (acceptance 2): the injected session context teaches the three
/// laws — run-under-ceiling on ask · the gate never answered for the
/// human · cost before the gesture — and still teaches the check-first
/// floor. One map, every client, every session.
#[test]
fn the_injected_map_teaches_the_three_laws() {
    let ctx = read("scripts/session-context.sh");
    // L1 · the agent may run, capped, when asked — guard judges.
    assert!(
        ctx.contains("When the human asks you to run it, run it yourself with --max-cost-usd"),
        "L1 (run under ceiling on ask) missing from the injected map"
    );
    assert!(
        ctx.contains("announce the ceiling first"),
        "L1 (ceiling announced BEFORE) missing"
    );
    assert!(
        ctx.contains("nika guard judges every run at the hook"),
        "L1 (guard is the judge) missing"
    );
    // L2 · the human gate is theirs alone.
    assert!(
        ctx.contains("NEVER answer a human gate for them"),
        "L2 (gate never answered) missing"
    );
    assert!(
        ctx.contains("surface the gate") && ctx.contains("question in the conversation verbatim"),
        "L2 (the question goes back to the chat, verbatim) missing"
    );
    assert!(
        ctx.contains("--resume <trace> --answer <task>=<their answer>"),
        "L2 (resume carries THEIR answer) missing"
    );
    // L3 · cost honesty before the gesture.
    assert!(
        ctx.contains("a local model is unpriced, never free"),
        "L3 (unpriced, never free) missing"
    );
    // The floor the laws stand on.
    assert!(
        ctx.contains("nika check <file> must pass before any run"),
        "the check-first floor eroded"
    );
}

/// LAW (the hidden-channel ban): the oracle stays read-only — the map
/// lists no `nika_run` MCP tool, and the delegation rule pins the
/// paused-gate posture. The agent launches in its OWN tool-use,
/// visible and interruptible, never through the oracle.
#[test]
fn the_oracle_gains_no_run_tool_and_the_gate_stays_human() {
    let ctx = read("scripts/session-context.sh");
    assert!(
        !ctx.contains("nika_run"),
        "the MCP oracle must never grow a run tool"
    );
    let rule = read("rules/nika-delegation.mdc");
    assert!(
        rule.contains("never pre-fill `--answer`"),
        "the delegation rule must pin the gate posture"
    );
    assert!(
        rule.contains("Never a") && rule.contains("spontaneous run"),
        "the delegation rule must ban the spontaneous run"
    );
}

/// An author joins the workspace that already exists before reaching for
/// the embedded shelf. Discovery is not validation: the skill must keep
/// `nika check` as the oracle after it names a local candidate.
#[test]
fn authoring_reads_local_workflows_before_creating_another() {
    let skill = read("skills/nika-authoring/SKILL.md");
    let list = skill.find("nika list").expect("local discovery is taught");
    let explain = skill[list..]
        .find("nika explain")
        .map(|offset| list + offset)
        .expect("the local candidate is narrated");
    let shelf = skill
        .find("nika try")
        .expect("the embedded shelf stays taught");
    assert!(list < explain, "discovery comes before narration");
    assert!(explain < shelf, "the workspace is read before the shelf");
    assert!(
        skill.contains("lists candidates; it does not certify them"),
        "list must never be mistaken for the oracle"
    );
}

/// The authoring map must describe the callable set the CEL evaluator
/// actually serves. A smaller remembered subset makes valid conditions look
/// forbidden and sends authors toward unnecessary glue.
#[test]
fn authoring_names_the_served_cel_callables() {
    let skill = read("skills/nika-authoring/SKILL.md");
    for callable in [
        "size()",
        "has()",
        ".contains()",
        ".startsWith()",
        ".endsWith()",
    ] {
        assert!(
            skill.contains(callable),
            "CEL callable `{callable}` missing"
        );
    }
    assert!(
        !skill.contains("size()` is the only function"),
        "the retired one-function claim returned"
    );
}
