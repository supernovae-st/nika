// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::display::theme::Theme;
use std::collections::BTreeMap;
use std::path::Path;

fn machine(
    ram: Option<u32>,
    seats: Vec<Seat>,
    pulled: bool,
    keys: &[&str],
    harness: bool,
) -> Machine {
    Machine {
        ram,
        seats,
        pulled,
        keys: keys.iter().map(|s| (*s).to_owned()).collect(),
        harness_in_binary: harness,
    }
}

/// Claude Code, signed in, WITH its ACP adapter on PATH — a seat a
/// session can actually start on. The id is the LIVE pin token
/// (`claude-code` · R4) — the retired wrapper id was NIKA-1802 as a
/// pin, and `chosen_access` used to teach it.
fn claude() -> Seat {
    Seat {
        id: "claude-code".to_owned(),
        name: "Claude Code".to_owned(),
        detected: true,
        authenticated: true,
        adapter_present: true,
    }
}

/// What the gauntlet actually met (P1/P4 · 2026-08-25): the person has
/// `claude` and is signed into it, and `claude-agent-acp` — a different
/// npm package, the binary a session spawns — is not installed. The
/// census names the seat by its pin token, so this shape is what a real
/// first-wow machine produces.
fn claude_without_its_adapter() -> Seat {
    Seat {
        adapter_present: false,
        ..claude()
    }
}

#[test]
fn table_parses_and_every_pull_has_one_owner() {
    let table: Table = serde_yaml_bw::from_str(TABLE_YAML).expect("models.yaml parses");
    assert!(!table.gear_one.is_empty());
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    for row in &table.gear_one {
        let (owner, repo) = row.pull.split_once('/').expect("pull is owner/repo");
        let repo = repo.split(':').next().expect("repo");
        if let Some(prev) = owners.insert(repo.to_owned(), owner.to_owned()) {
            assert_eq!(
                prev, owner,
                "model id `{repo}` has two owners: {prev} and {owner}"
            );
        }
    }
}

/// An Anthropic key's first-run suggestion is the provider's catalog default, Claude
/// Sonnet 5.5: a person reaches it without typing a model id.
#[test]
fn an_anthropic_key_suggests_its_catalog_default() {
    let table: Table = serde_yaml_bw::from_str(TABLE_YAML).expect("models.yaml parses");
    let row = (table.keys.iter())
        .find(|row| row.env == "ANTHROPIC_API_KEY")
        .expect("the Anthropic key row");
    let anthropic = nika_catalog::find_provider("anthropic").expect("the anthropic row");
    assert_eq!(row.model, "anthropic/claude-sonnet-5-5");
    assert_eq!(row.model, format!("anthropic/{}", anthropic.default_model));
}

#[test]
fn a_model_id_does_not_appear_under_two_owners_in_s3_surfaces() {
    let surfaces = [
        TABLE_YAML,
        include_str!("../doctor.rs"),
        include_str!("../welcome.rs"),
    ];
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    for text in surfaces {
        for (owner, repo) in regex_lite(text) {
            if let Some(prev) = owners.get(&repo) {
                assert_eq!(
                    prev, &owner,
                    "model `{repo}` claimed by `{prev}` and `{owner}`"
                );
            } else {
                owners.insert(repo, owner);
            }
        }
    }
}

fn regex_lite(text: &str) -> Vec<(String, String)> {
    // owner/repo Hub ids — the two-owner class (Qwen/X vs unsloth/X).
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '/' && c != '-' && c != '_' && c != '.' && c != ':'
        });
        let Some((owner, rest)) = token.split_once('/') else {
            continue;
        };
        if !matches!(
            owner,
            "unsloth" | "Qwen" | "google" | "meta-llama" | "mistralai" | "huggingface"
        ) {
            continue;
        }
        let repo = rest.split(':').next().unwrap_or(rest);
        if repo.is_empty() {
            continue;
        }
        out.push((owner.to_owned(), repo.to_owned()));
    }
    out
}

#[test]
fn authenticated_harness_takes_the_arrow_with_zero_key_zero_download() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    assert_eq!(choice.arrow, "harness");
    assert!(
        choice
            .rungs
            .iter()
            .find(|r| r.id == "harness")
            .is_some_and(|r| r.ready),
        "{choice:?}"
    );
    let dir = tempfile::tempdir().expect("tmp");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    assert!(human.contains("Claude Code"), "{human}");
    assert!(human.contains("nika compile hello hello.nika"), "{human}");
    assert!(!human.contains("xai/grok-4"), "{human}");
}

/// The gauntlet's sharpest finding (P1/P4 · 2026-08-25). The screen said
/// `▸ Claude Code · runs on the plan you already pay for · no API key`
/// and `doctor --json` said `ready: true`, `chosen_access:
/// claude-agent-acp` — on a machine where `command -v claude-agent-acp`
/// found nothing. Doctor's own HUMAN harness rows listed only the three
/// adapters that WERE installed, so two instruments in one binary
/// disagreed and the honest one was the quiet one.
///
/// An agent reading that JSON picks the seat and the run cannot start.
/// Detected means "you have the app"; ready means "a session can start",
/// and only the adapter proves the second.
#[test]
fn a_signed_in_app_without_its_acp_adapter_is_not_ready() {
    let choice = collect_from(&machine(
        Some(18),
        vec![claude_without_its_adapter()],
        false,
        &[],
        true,
    ));
    let harness = choice
        .rungs
        .iter()
        .find(|r| r.id == "harness")
        .expect("the harness rung is always rendered");

    assert!(
        !harness.ready,
        "a seat whose adapter is absent cannot start a session: {choice:?}"
    );
    assert_ne!(
        choice.arrow, "harness",
        "the arrow must not point at a path that cannot run: {choice:?}"
    );
    assert!(
        choice.chosen_access.is_none(),
        "no access is chosen when none can serve: {choice:?}"
    );

    // Still SHOWN, and still useful — hiding Claude Code from someone who
    // has it would trade one lie for another.
    assert!(harness.available, "the app is here: {choice:?}");
    let human = render_human_at(&choice, Theme::new(false, false, false), None);
    assert!(human.contains("Claude Code"), "{human}");
    // R4 — the row names the wall (the ACP adapter) and the door
    // (`nika doctor` carries the install line); it no longer teaches
    // the wrapper's npm id, which `run --access` refuses as retired.
    assert!(
        human.contains("needs its ACP adapter") && human.contains("nika doctor"),
        "the row names the gap and the door: {human}"
    );
    assert!(
        !human.contains("claude-agent-acp"),
        "the wrapper id is never taught as the fix: {human}"
    );
    // The seat's own row, not the card: the local rung honestly says
    // « no API key » too (a local model needs none).
    let seat_row = human
        .lines()
        .find(|line| line.contains("Claude Code"))
        .expect("the seat row");
    assert!(
        !seat_row.contains("no API key"),
        "the promise belongs to a seat that can run: {human}"
    );
}

/// The other direction, so the refusal cannot be a blanket one: the same
/// seat WITH its adapter is ready and takes the arrow.
#[test]
fn the_same_seat_with_its_adapter_is_ready_again() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    assert_eq!(choice.arrow, "harness", "{choice:?}");
    assert_eq!(
        choice.chosen_access.as_deref(),
        Some("claude-code"),
        "the chosen access is the LIVE pin token (R4): {choice:?}"
    );
}

#[test]
fn ready_first_then_scale_rank() {
    let keyed = collect_from(&machine(
        Some(18),
        vec![],
        false,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    assert_eq!(keyed.arrow, "key", "a ready key beats an unready local");
    let both = collect_from(&machine(
        Some(18),
        vec![claude()],
        false,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    assert_eq!(
        both.arrow, "harness",
        "among ready rungs, harness (rank 3) beats key (rank 4)"
    );
    let local = collect_from(&machine(
        Some(18),
        vec![claude()],
        true,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    assert_eq!(local.arrow, "local", "a ready local is the shortest path");
}

#[test]
fn m3_pro_18gb_resolves_standard_not_the_27b() {
    let table = table();
    let tier = resolve_tier(&table, Some(18)).expect("a tier fits 18 GB");
    assert_eq!(tier.tier, "standard");
    assert!(tier.download_gb < 16.5);
}

#[test]
fn doctor_json_names_env_vars_never_values() {
    let choice = collect_from(&machine(
        Some(18),
        vec![claude()],
        false,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    let json = doctor_cascade_json(&choice);
    let dumped = json.to_string();
    assert!(dumped.contains("ANTHROPIC_API_KEY"));
    assert!(!dumped.contains("sk-"));
    assert_eq!(json["nika_cloud_session"], serde_json::Value::Null);
    assert!(json["hardware_ok_for_standard"].as_bool().unwrap());
    assert!(!json["hardware_ok_for_default"].as_bool().unwrap());
    assert_eq!(json["arrow"], "harness");
}

#[test]
fn tty_and_pipe_project_the_same_product() {
    let choice = collect_from(&machine(Some(8), vec![], false, &[], false));
    let dir = tempfile::tempdir().expect("tmp");
    let a = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let b = render_human_at(&choice, Theme::new(false, true, false), Some(dir.path()));
    assert!(a.contains("Keep it as a file"));
    assert!(b.contains("Keep it as a file"));
    assert!(a.contains("Next:"));
    assert_eq!(
        a.lines().filter(|l| l.contains("▸")).count(),
        b.lines().filter(|l| l.contains("▸")).count()
    );
}

/// 0.123 is the open-source reference release, maintained, with no hosted service: the welcome
/// screen and its envelope advertise no Nika Cloud, and a curated open-weight model is never
/// called « ours » (it is Gemma or Qwen, pulled from the Hub) nor « free » (a local model is
/// unpriced: the reason names what it spares, the API key).
#[test]
fn the_welcome_advertises_no_cloud_and_claims_no_model_as_ours() {
    for (ram, pulled) in [(Some(18), false), (Some(18), true), (None, false)] {
        let choice = collect_from(&machine(ram, vec![], pulled, &[], false));
        let dir = tempfile::tempdir().expect("tmp");
        let text = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
        assert!(!text.contains("Cloud"), "{text}");
        // The exact retired phrases, so a word like « yours » never trips it.
        assert!(!text.contains("ours · "), "{text}");
        assert!(!text.contains("· free"), "{text}");
        assert!(text.contains("open weights · no API key · "), "{text}");
        assert!(
            choice.rungs.iter().all(|r| r.id != "cloud"),
            "{:?}",
            choice.rungs
        );
    }
}

/// The local rung's reason, both shapes, verbatim: the price claim is gone and
/// the download still reads where the person decides.
#[test]
fn the_local_reason_names_the_api_key_it_spares_never_a_price() {
    assert_eq!(
        local_reason(false, Some(18), "7.4 GB"),
        "open weights · no API key · 7.4 GB to pull"
    );
    assert_eq!(
        local_reason(true, Some(18), "7.4 GB"),
        "open weights · no API key · runs on this computer · 7.4 GB"
    );
}

#[test]
fn the_welcome_advertises_no_cloud_and_claims_no_model_as_ours_in_the_envelope() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], false));
    let envelope = choice.welcome_json("nika compile hello hello.nika");
    let local = envelope["rungs"]
        .as_array()
        .and_then(|rungs| rungs.iter().find(|r| r["id"] == "local"))
        .expect("the local rung rides the envelope");
    assert_eq!(
        local["reason"], "open weights · no API key · 7.4 GB to pull",
        "{envelope}"
    );
    assert!(!envelope.to_string().contains("· free"), "{envelope}");
}

fn assert_parses(yaml: &str) {
    assert!(
        yaml.contains("\n  greet:\n") || yaml.contains("\n  reply:\n"),
        "a hello task must be indented under tasks"
    );
    let parsed = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    );
    assert!(parsed.is_ok(), "first-wow yaml must parse");
}

#[test]
fn parse_failure_diagnostic_does_not_expose_workflow_body() {
    const SENTINEL: &str = "nika-test-secret-do-not-log";
    let yaml =
        format!("nika: test\nsecrets:\n  token: {SENTINEL}\ntasks:\n  greet:\n    infer: [\n");

    let result = std::panic::catch_unwind(|| assert_parses(&yaml));
    assert!(
        result.is_err(),
        "invalid workflow must fail the parse assertion"
    );
    let payload = match result {
        Ok(()) => return,
        Err(payload) => payload,
    };
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    assert!(
        message.is_some(),
        "parse assertion must carry a string diagnostic"
    );
    let Some(message) = message else {
        return;
    };

    assert!(
        message == "first-wow yaml must parse",
        "parse assertion must use its static diagnostic"
    );
    assert!(
        !message.contains(SENTINEL),
        "parse diagnostics must not expose workflow secrets"
    );
}

#[test]
fn parse_memtotal_reads_linux_proc() {
    let sample = "MemTotal:       18874368 kB\nMemFree:         1024 kB\n";
    assert_eq!(parse_memtotal(sample), Some(18));
    assert_eq!(parse_memtotal("nope\n"), None);
    // A 16 GB box whose kernel reserved a slice still lands on standard.
    let under = "MemTotal:       16333812 kB\n";
    assert_eq!(parse_memtotal(under), Some(16));
}

#[test]
fn bytes_to_gb_rounds_nearest_gib() {
    const GIB: u64 = 1024 * 1024 * 1024;
    assert_eq!(bytes_to_gb(0), None);
    assert_eq!(bytes_to_gb(18 * GIB), Some(18));
    assert_eq!(bytes_to_gb(8 * GIB), Some(8));
    assert_eq!(bytes_to_gb(15 * GIB + GIB / 2), Some(16));
    assert_eq!(bytes_to_gb(15 * GIB + GIB / 2 - 1), Some(15));
}

#[cfg(unix)]
#[test]
fn this_unix_host_reports_ram() {
    let gb = ram_gb();
    assert!(
        gb.is_some_and(|n| (1..1024).contains(&n)),
        "proc or sysconf must see RAM on unix, got {gb:?}"
    );
}

#[test]
fn seven_gb_does_not_claim_lite() {
    let table = table();
    assert!(resolve_tier(&table, Some(7)).is_none());
    assert_eq!(
        resolve_tier(&table, Some(8)).map(|t| t.tier.as_str()),
        Some("lite")
    );
}

#[test]
fn an_empty_model_dir_is_not_ready() {
    let dir = tempfile::tempdir().expect("tmp");
    let repo = dir
        .path()
        .join("unsloth")
        .join("Qwen3-4B-Instruct-2507-GGUF");
    std::fs::create_dir_all(&repo).expect("repo");
    assert!(!dir_has_gguf(&repo), "an empty pull dir is not a model");
    std::fs::write(repo.join("weights.gguf"), b"gguf").expect("weight");
    assert!(dir_has_gguf(&repo));
    std::fs::write(repo.join("empty.gguf"), b"").expect("empty");
    assert!(
        dir_has_gguf(&repo),
        "a real weight still counts beside a zero-byte file"
    );
    let empty_only = dir.path().join("unsloth").join("empty-only");
    std::fs::create_dir_all(&empty_only).expect("empty-only");
    std::fs::write(empty_only.join("empty.gguf"), b"").expect("zero");
    assert!(!dir_has_gguf(&empty_only), "a zero-byte gguf is not ready");
}

#[test]
fn pull_repo_dir_strips_the_quant() {
    let root = Path::new("/tmp/models");
    assert_eq!(
        pull_repo_dir(root, "unsloth/gemma-3-12b-it-GGUF:Q4_K_M").as_deref(),
        Some(Path::new("/tmp/models/unsloth/gemma-3-12b-it-GGUF"))
    );
    assert_eq!(
        pull_repo_dir(root, "nika/gear-one").as_deref(),
        Some(Path::new("/tmp/models/nika/gear-one"))
    );
    assert_eq!(pull_repo_dir(root, "gear-one"), None);
    assert_eq!(pull_repo_dir(root, "unsloth/"), None);
}

#[test]
fn chosen_access_is_the_seat_when_harness_has_the_arrow() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    assert_eq!(choice.chosen_access.as_deref(), Some("claude-code"));
    let json = doctor_cascade_json(&choice);
    assert_eq!(json["chosen_access"], "claude-code");
}

#[test]
fn next_for_a_ready_harness_is_new_hello() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    assert!(human.contains("nika compile hello hello.nika"), "{human}");
    // « this hardware », not « this machine »: the mirror body's
    // `this machine` section is the environment one, and the same two
    // words named three things on one screen (#1196).
    assert!(human.contains("this hardware · 18 GB"), "{human}");
    assert!(!human.contains("this machine"), "{human}");
}

/// [`DOOR_SHAPES`] is a hand-written mirror of [`front_door_next`], and
/// a hand-written mirror is a lie waiting to happen — the concierge's
/// parse ratchet replays it against the live clap tree, so it has to be
/// the function's exact image. Run the real thing on real directories.
#[test]
fn door_shapes_mirror_the_real_door() {
    let placeholder = |door: String| {
        if door.starts_with("nika run ") {
            door.replace("hello.nika", "<file>")
        } else {
            door
        }
    };
    let dir = tempfile::tempdir().expect("tmp");
    let empty = placeholder(front_door_next(Some(dir.path())));
    std::fs::write(dir.path().join("hello.nika"), "nika: hello\n").expect("hello");
    let one = placeholder(front_door_next(Some(dir.path())));
    std::fs::write(dir.path().join("a.nika"), "nika: a\n").expect("a");
    std::fs::remove_file(dir.path().join("hello.nika")).expect("drop hello");
    std::fs::write(dir.path().join("b.nika"), "nika: b\n").expect("b");
    let many = placeholder(front_door_next(Some(dir.path())));
    let measured = [empty, one, many];
    for shape in DOOR_SHAPES {
        assert!(
            measured.iter().any(|m| m == shape),
            "DOOR_SHAPES claims `{shape}`, the door never says it: {measured:?}"
        );
    }
    for m in &measured {
        assert!(
            DOOR_SHAPES.contains(&m.as_str()),
            "the door says `{m}`, DOOR_SHAPES never lists it"
        );
    }
}

#[test]
fn unready_local_next_is_new_hello_not_a_seven_gb_pull() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    assert_eq!(choice.arrow, "local");
    let dir = tempfile::tempdir().expect("tmp");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let after = human.split("Next:").nth(1).expect("Next: block");
    assert!(
        after.contains("nika compile hello hello.nika"),
        "empty machine first door is the file that runs:\n{human}"
    );
    assert!(
        !after.contains("nika model pull"),
        "pull belongs on the rung, not as Next:\n{human}"
    );
    assert!(
        human.contains("to pull"),
        "the local rung still names the download:\n{human}"
    );
}

fn next_block(human: &str) -> &str {
    human.split("Next:").nth(1).expect("Next: block")
}

#[test]
fn next_after_hello_exists_is_run_not_new() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join("hello.nika"), "nika: hello\n").expect("seed");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let after = next_block(&human);
    assert!(
        after.contains("nika run hello.nika"),
        "a file already here is the next door:\n{human}"
    );
    assert!(
        !after.contains("nika compile hello hello.nika"),
        "new hello after a file exists is the --force dead end:\n{human}"
    );
}

#[test]
fn next_after_hello_with_harness_is_run_not_a_pin() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join("hello.nika"), "nika: hello\n").expect("seed");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let after = next_block(&human);
    assert!(
        after.contains("nika run hello.nika"),
        "a file already here is the next door:\n{human}"
    );
    assert!(
        !after.contains("--access harness"),
        "harness pin on an unmodeled agent was NIKA-INFER-001:\n{human}"
    );
    assert!(
        !after.contains("claude-agent-acp"),
        "seat ids are NIKA-1802 as --access pins:\n{human}"
    );
    assert!(!after.contains("nika compile hello hello.nika"), "{human}");
}

#[test]
fn next_prefers_hello_when_other_workflows_sit_beside_it() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join("hello.nika"), "nika: hello\n").expect("hello");
    std::fs::write(dir.path().join("other.nika"), "nika: other\n").expect("other");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let after = next_block(&human);
    assert!(
        after.contains("nika run hello.nika"),
        "first-wow stays the one Next when it is here:\n{human}"
    );
}

#[test]
fn next_with_two_non_hello_files_is_bare_run() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join("a.nika"), "nika: a\n").expect("a");
    std::fs::write(dir.path().join("b.nika"), "nika: b\n").expect("b");
    let human = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    let after = next_block(&human);
    assert!(
        after.contains("nika run"),
        "several files → the lazy door, not another scaffold:\n{human}"
    );
    assert!(!after.contains("nika compile hello hello.nika"), "{human}");
}

/// The seat-cascade byte oracle retains the pre-move captures for human,
/// welcome and cascade output. The six doctor captures include the additive
/// `summary.unknown` and `scope.mode` fields, recorded by the renderer after
/// that intentional change. Doctor cases compose the machine report with
/// this cascade exactly as `doctor --json` does (`v["cascade"] = …`).
mod cascade_oracle {
    use super::*;
    use crate::doctor::{Finding, Level};
    use crate::probe::AdoptionState;
    use nika_providers::census::AccessCensus;
    use nika_providers::probe::{AccessClass, ExecutionLocus, ProviderProbe, ProviderReadiness};
    use sha2::{Digest as _, Sha256};

    fn themes() -> [(&'static str, Theme); 5] {
        [
            ("plain", Theme::new(false, false, false)),
            ("color", Theme::new(true, false, false)),
            ("ascii", Theme::new(false, true, false)),
            ("color_ascii", Theme::new(true, true, false)),
            ("animate", Theme::new(true, false, true)),
        ]
    }

    fn machines() -> Vec<(&'static str, Machine)> {
        vec![
            ("bare8", machine(Some(8), vec![], false, &[], false)),
            ("unknown_ram", machine(None, vec![], false, &[], false)),
            (
                "seat18",
                machine(Some(18), vec![claude()], false, &[], true),
            ),
            (
                "no_adapter",
                machine(
                    Some(18),
                    vec![claude_without_its_adapter()],
                    false,
                    &["ANTHROPIC_API_KEY"],
                    true,
                ),
            ),
            (
                "keys64",
                machine(
                    Some(64),
                    vec![],
                    true,
                    &["MISTRAL_API_KEY", "OPENAI_API_KEY"],
                    false,
                ),
            ),
            (
                "seat_pulled32",
                machine(Some(32), vec![claude()], true, &["XAI_API_KEY"], false),
            ),
        ]
    }

    fn findings() -> Vec<Finding> {
        let row = |level, label: &str, detail: &str, fix: Option<&str>| Finding {
            level,
            label: label.to_owned(),
            detail: detail.to_owned(),
            fix: fix.map(str::to_owned),
        };
        vec![
            row(Level::Ok, "binary", "v0.0.0-tëst ✨", None),
            row(
                Level::Warn,
                "keys",
                "a \"quoted\" \\ detail",
                Some("export X=1"),
            ),
            row(
                Level::Fail,
                "config",
                "broken\nsecond line",
                Some("nika wire claude"),
            ),
        ]
    }

    fn census() -> AccessCensus {
        let probe = |id: &str, var: &str, present: bool, locus| {
            ProviderProbe::new(
                id,
                !var.is_empty(),
                present,
                var,
                true,
                ProviderReadiness::new(
                    true,
                    present || var.is_empty(),
                    None,
                    None,
                    false,
                    locus,
                    if var.is_empty() {
                        AccessClass::Local
                    } else {
                        AccessClass::Api
                    },
                ),
                format!("https://{id}.example/v1"),
            )
        };
        AccessCensus::from_parts(
            &[
                probe("mistral", "MISTRAL_API_KEY", true, ExecutionLocus::Cloud),
                probe("deepseek", "DEEPSEEK_API_KEY", false, ExecutionLocus::Cloud),
                probe("ollama", "", false, ExecutionLocus::Loopback),
            ],
            vec![],
        )
    }

    pub(super) fn cases(
        cascade: &dyn Fn(&InferenceChoice) -> serde_json::Value,
    ) -> Vec<(String, String)> {
        let nexts = [
            ("hello", "nika compile hello hello.nika"),
            ("quoted", "nika run \"é.nika\" --var x=\\\\"),
            ("try", "nika try 01-hello"),
        ];
        let states = [
            ("installed", AdoptionState::Installed),
            ("local_detected", AdoptionState::LocalDetected),
            ("local_reachable", AdoptionState::LocalReachable),
            ("key_present", AdoptionState::KeyPresent),
            ("seat_ready", AdoptionState::SeatReady),
            ("real_ready", AdoptionState::RealReady),
        ];
        let mut out = Vec::new();
        for (m, machine) in machines() {
            let mut choice = collect_from(&machine);
            if m == "no_adapter" {
                // The live choice: the Anthropic key's catalog default, no ready harness.
                assert_eq!(choice.arrow, "key");
                assert_eq!(choice.chosen_model, "anthropic/claude-sonnet-5-5");
                assert!(
                    choice
                        .rungs
                        .iter()
                        .any(|rung| rung.id == "harness" && !rung.ready)
                );
                // The pre-move captures were recorded when that default was
                // claude-sonnet-4-6: the renderer is compared on that same input.
                choice.chosen_model = "anthropic/claude-sonnet-4-6".to_owned();
            }
            for (n, next) in nexts {
                for (t, theme) in themes() {
                    let text = choice.render_human_next(theme, next);
                    let sober = crate::display::vocab::sober(theme, &text);
                    out.push((format!("{m}/human/{n}/{t}/raw"), text));
                    out.push((format!("{m}/human/{n}/{t}/sober"), sober));
                }
                let v = choice.welcome_json(next);
                out.push((format!("{m}/welcome/{n}/compact"), v.to_string()));
                out.push((format!("{m}/welcome/{n}/pretty"), format!("{v:#}")));
            }
            let v = cascade(&choice);
            out.push((format!("{m}/cascade/compact"), v.to_string()));
            out.push((format!("{m}/cascade/pretty"), format!("{v:#}")));
            for (s, state) in states {
                for (c, census) in [("empty", AccessCensus::default()), ("paths", census())] {
                    let raw = crate::doctor::render_json(&findings(), state, &[], &census);
                    let mut v: serde_json::Value =
                        serde_json::from_str(&raw).expect("doctor JSON parses");
                    v["cascade"] = cascade(&choice);
                    out.push((format!("{m}/doctor/{s}/{c}/raw"), raw));
                    out.push((format!("{m}/doctor/{s}/{c}/cascade"), format!("{v:#}")));
                }
            }
        }
        out
    }

    /// `(group, cases, sha256)` per leading two name segments, in order.
    pub(super) fn groups(cases: &[(String, String)]) -> Vec<(String, usize, String)> {
        let mut out: Vec<(String, usize, Sha256)> = Vec::new();
        for (name, text) in cases {
            let key = name.splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
            if out.last().is_none_or(|(k, _, _)| *k != key) {
                out.push((key, 0, Sha256::new()));
            }
            if let Some((_, n, hasher)) = out.last_mut() {
                *n += 1;
                hasher.update(name.as_bytes());
                hasher.update([0]);
                hasher.update(text.len().to_string().as_bytes());
                hasher.update([0]);
                hasher.update(text.as_bytes());
            }
        }
        out.into_iter()
            .map(|(k, n, h)| (k, n, format!("{:x}", h.finalize())))
            .collect()
    }

    /// `(group, cases, sha256)` recorded captures; the test never recomputes its expectations.
    const EXPECTED: &[(&str, usize, &str)] = &[
        (
            "bare8/human",
            10,
            "8cc5369fa7849f9ec3344d0bcce7ec340fc087775b7960a13ad48e0d8ccfeaf5",
        ),
        (
            "bare8/welcome",
            2,
            "591ec96c2c648573e6c5d14dca1161962a3a47d03931908cd2ad7e57dfc7e8bb",
        ),
        (
            "bare8/human",
            10,
            "1bb4d6fbda254293b5e3593e9c09225403c8343c570bb60e525b35fb2734de0d",
        ),
        (
            "bare8/welcome",
            2,
            "41bad4edb2ea1f98309c1ef154a29eab7547150426515d660a2f6d432c1f9aa7",
        ),
        (
            "bare8/human",
            10,
            "56d33174f628fde16695649f60ffbcc27112faa617a3944149dd86eb67cbeb86",
        ),
        (
            "bare8/welcome",
            2,
            "8c9d62eb55ded34999dacb10c2782b503c5cfb990329b79cf49d056a83c1c3df",
        ),
        (
            "bare8/cascade",
            2,
            "d8927ab11f59c7a1e53f89c3d86de0768954107b57f00add9dbf9e3015314621",
        ),
        (
            "bare8/doctor",
            24,
            "14128d8f33f5e8b1b8931d54a8a7c9ce3f0e1b84c8f986a9b69206c81aefe4df",
        ),
        (
            "unknown_ram/human",
            10,
            "4a65f09b552a47890203b05714977563b79c827b8731cfd90c6b6e6151dfddd9",
        ),
        (
            "unknown_ram/welcome",
            2,
            "e84a821f492edb3fd9f2f7cbd7cf4002be9a0d191cc713b682c9fc03edaf2372",
        ),
        (
            "unknown_ram/human",
            10,
            "9d90f3892a064d5b3d72da95ccba673dc08a2b288320d736e5e7267e8a981cbb",
        ),
        (
            "unknown_ram/welcome",
            2,
            "57365b883116d917ea3a6485a0ee04ac8884f162886f72b3bb47ac8a765c3e35",
        ),
        (
            "unknown_ram/human",
            10,
            "fff0fd9bb96f65cb6bfc15a1f8d747f45e8076ff0bf6decbeb8b0230ca0a977b",
        ),
        (
            "unknown_ram/welcome",
            2,
            "faabda73dd8dc1fc7706e17d9664bc42a8d491e91babb0f61e4cbe705125fde2",
        ),
        (
            "unknown_ram/cascade",
            2,
            "5bbfe309ac624c30b3fdff3e3bd8f63a4dade305dfdba0acc29b1a093b570e9c",
        ),
        (
            "unknown_ram/doctor",
            24,
            "e5a89e9c5fc962ebf588cc7466bd4333f377aaafe63a6a82e136592bcf62196b",
        ),
        (
            "seat18/human",
            10,
            "5dbb0d4099443090b33246e935811961a0c9f1ab1fb919e3a917b6c36c01b721",
        ),
        (
            "seat18/welcome",
            2,
            "9bfaae6f386bc9888ca8cd5dea01b7482410dd102e0a574d5179edc8a28f37cb",
        ),
        (
            "seat18/human",
            10,
            "96953899bd016d76e73254d096a67ee7381b13f23ee54ceeec962a8546ea4124",
        ),
        (
            "seat18/welcome",
            2,
            "a8001d0f4074f94fe158d6ed94c64e3de5f372c2444b49c4c8907bb16eae92c8",
        ),
        (
            "seat18/human",
            10,
            "258be398f91d9dda4879bb755f61c89cda39b22e0b271fb1db3f23d9a4a16618",
        ),
        (
            "seat18/welcome",
            2,
            "171713fd431eadabba9bb4f8123887bc3a5e332f1ed6ed584eb4cef5e77c10d3",
        ),
        (
            "seat18/cascade",
            2,
            "8b19e621fd6cd5c11d9782ef8737a0b3021b06ed75a48baa3f52f25861233e92",
        ),
        (
            "seat18/doctor",
            24,
            "6e5b40f5ae7c1c67e54c4538e28c2f6a789e487510b18d32152618e939f9cb4b",
        ),
        (
            "no_adapter/human",
            10,
            "f72410006a9cec295930fcd70342e5bb0f7eb0f13a011ae3acdc9cb7a5b8f978",
        ),
        (
            "no_adapter/welcome",
            2,
            "158b32ce6d3471022a7d9e53fd22e9895359c1dee48d3e8875049fc7808153a8",
        ),
        (
            "no_adapter/human",
            10,
            "a0557e3128492b68a6182438a2b0d179e97fbf55b35ab0d9830626e723124f37",
        ),
        (
            "no_adapter/welcome",
            2,
            "c95b80244808cdad02a70f7dd02f6840806c3a573cf15a1bdd48f0a91f9d0598",
        ),
        (
            "no_adapter/human",
            10,
            "c98bc2e9312ace014bdfd276c786aaff4d0d0f7558d3ffeab9391c36f9b210e0",
        ),
        (
            "no_adapter/welcome",
            2,
            "2f837dd40dfeab79e1317a4badca9cb69425565b50defb3528539cbcaf67ec44",
        ),
        (
            "no_adapter/cascade",
            2,
            "4e2aa9d9f60b54872d68cbd18e6fe8262ed9dade5fd54b8b31caa0989ed0b3e9",
        ),
        (
            "no_adapter/doctor",
            24,
            "2eda7f61bb1ac10ec9cb6728a7b4a7cba5d187e4bc13d08ce2fb4d29c5500174",
        ),
        (
            "keys64/human",
            10,
            "5982df07a458d1f69914301864a79c270a98913c7a1e8651365ace765aa78b1c",
        ),
        (
            "keys64/welcome",
            2,
            "af6843f2a3fc5834dec4af1a3bd61dd5ced5b912a86dfde01434c9abd96cf223",
        ),
        (
            "keys64/human",
            10,
            "df7639b9a11c322550f676b15357e63a6ecf620837c74eb04cd5e49d2d522e2a",
        ),
        (
            "keys64/welcome",
            2,
            "6ed9cee80f233203ec6070763b6771b7c931408c401ea75c519204710a872002",
        ),
        (
            "keys64/human",
            10,
            "503a965b3dcc105d070c5daba62e5a3273ba3f5de6d5a3841ee225fde2e4be2d",
        ),
        (
            "keys64/welcome",
            2,
            "ef00a10b983905ff61ce042ded2541ffb2f1f90961784ef10ac00430c844b08e",
        ),
        (
            "keys64/cascade",
            2,
            "d203166e929c4ab00441f29e19de1858c26911249c264565506a6316732d83dd",
        ),
        (
            "keys64/doctor",
            24,
            "280384b16eedc8c4c95ab68e3d67218e9de46bb2e7285fabbff77dd9494a4f77",
        ),
        (
            "seat_pulled32/human",
            10,
            "28cd6c817aa43af86a1ee4590b16ecd0448e556d66fe16337fd49344c8639272",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "09d8e1d1bc9aae2528d0e0446942fcc77de2765ed4901818ab81dae06857dcfa",
        ),
        (
            "seat_pulled32/human",
            10,
            "85c577114439927b1d759388fe58a6eff34f261dc068f6eb90a7e6ec0b351ea8",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "505c733cf36967c4e903dd11caf1ee8307504c75f18185e5eb6d6689e226e601",
        ),
        (
            "seat_pulled32/human",
            10,
            "c81d17841aa21ada1b3b90063e9b7486d4fa09cb0e530585de3ff53987504032",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "2bcad6a577811e51df2980a68efa4dcca7fd26c75860f0bc48e082fd444834c8",
        ),
        (
            "seat_pulled32/cascade",
            2,
            "da4d432a390739b0393eeb12a09644bbc75b61ff6025687789791a0d50ff168d",
        ),
        (
            "seat_pulled32/doctor",
            24,
            "b7a4dceba169a014b58f8a84358f1ac621722f879d2ffac5faebd34ac13d39d4",
        ),
    ];

    #[test]
    fn the_cascade_renders_the_pre_move_bytes() {
        let cases = cases(&doctor_cascade_json);
        let want: Vec<(String, usize, String)> = EXPECTED
            .iter()
            .map(|(g, n, s)| ((*g).to_owned(), *n, (*s).to_owned()))
            .collect();
        assert_eq!(groups(&cases), want);
    }
}
