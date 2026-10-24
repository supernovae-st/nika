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
    assert!(
        !human.contains("no API key"),
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
/// called « ours » (it is Gemma or Qwen, pulled from the Hub).
#[test]
fn the_welcome_advertises_no_cloud_and_claims_no_model_as_ours() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], false));
    let dir = tempfile::tempdir().expect("tmp");
    let text = render_human_at(&choice, Theme::new(false, false, false), Some(dir.path()));
    assert!(!text.contains("Cloud"), "{text}");
    assert!(!text.contains("ours"), "{text}");
    assert!(text.contains("open weights · free"), "{text}");
    assert!(
        choice.rungs.iter().all(|r| r.id != "cloud"),
        "{:?}",
        choice.rungs
    );
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
            "7adc0da1e1d5cb2fcdff33471a5388602e218d9b5d2ae5845bb7abef54ba5130",
        ),
        (
            "bare8/welcome",
            2,
            "a5d6dfc05d8df52434a6268e603a2a8defbd5fabc1bf4964f7174fac3f359ff5",
        ),
        (
            "bare8/human",
            10,
            "990b483c1ce81af7e316de501599884dc4543da7bbe8f2cbf887014de8461a1a",
        ),
        (
            "bare8/welcome",
            2,
            "8fd62862899d8c6f5214890b418f25ea69682d39fb0c3844715446b32b71af52",
        ),
        (
            "bare8/human",
            10,
            "f54532f28340d16025e00db6fd446eb51c8f4da4ce3f618dcfa6fb4b78fc4074",
        ),
        (
            "bare8/welcome",
            2,
            "3d5536bdc631a31896554c48a227090e4b4245aea85e4b0505035d4df2d94205",
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
            "b31128b43ce795d5e7ce4ecae26d4a020771c195a0c5f6a73b88ad7fd691c429",
        ),
        (
            "unknown_ram/welcome",
            2,
            "1c749691b596dbaf4976a70468ca790052405f2d6b6160485b70bfe64f893cac",
        ),
        (
            "unknown_ram/human",
            10,
            "f3c80ba6bf883ab3f57b190d7fb3cfb49391450894c88cadf9aebbf7144a6c70",
        ),
        (
            "unknown_ram/welcome",
            2,
            "d69451232610ad921c7b05fda1ddbedccbaf24589877755eefcd474070e0c033",
        ),
        (
            "unknown_ram/human",
            10,
            "49965d85f922e55321a3412f78bd1827a1c9a0962b7f9a867242b2a562a1505b",
        ),
        (
            "unknown_ram/welcome",
            2,
            "a5dbee2efb4d9c4b9e663ab9ffe04d879425e3532e9cf75e06bd0e95397582f9",
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
            "c7e6d3571b397ae4d6b6a5f1df8082ec334b10b1bd0c8cfaec013894eeb419a7",
        ),
        (
            "seat18/welcome",
            2,
            "ff58de77bde5e71a2621769a1c7ae502118089192f2448ffccb3e3cf09b7041c",
        ),
        (
            "seat18/human",
            10,
            "59675aa39e54ba8708bdb107ac3d1846949f05fca242f1e164d87dc8dcd6d032",
        ),
        (
            "seat18/welcome",
            2,
            "98396ed5b6e895cc5f8f215cde75c6fa64433981d14d20ed377a59e9a91590e4",
        ),
        (
            "seat18/human",
            10,
            "3fc5970241fdbce681e26e89930f975897583a4f8f20d1ae6fa06e66a8b55b41",
        ),
        (
            "seat18/welcome",
            2,
            "26e5a30a26ea5c38cc18ea447100818422e192f48832aecd75ecd6c825056132",
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
            "d9d255a75445f47431a7573ef2702874f847a709fa608b9403bac10b03712047",
        ),
        (
            "no_adapter/welcome",
            2,
            "b2aa567b1c0c242f2ca18c3f4b05a97c6d00ab763d3c489c5048d014b22ee9ab",
        ),
        (
            "no_adapter/human",
            10,
            "5e682acde880aee0d7d9e14755f3b5904b30bd43436859135f9c6e5308af334c",
        ),
        (
            "no_adapter/welcome",
            2,
            "d6bab3f3c55b591b9ef034e0d8b8465cfbc6340336bc80627aa9db07daedffbd",
        ),
        (
            "no_adapter/human",
            10,
            "7f8ea1b6302430977a61755037901b6211f9e4d23be41ad45d05ecc1a429f7c2",
        ),
        (
            "no_adapter/welcome",
            2,
            "0dca5b30943fbd629c4391444ada1668aab8cb274cfcd4088304096535e8af4b",
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
            "fbe22e90b03115045d9bdba537223767e87a0800e51744d80e11553417501d0a",
        ),
        (
            "keys64/welcome",
            2,
            "7832f0148b74736986e8269a9fe6c3104c5178b46c66bdd5dea5d3918f85d8a2",
        ),
        (
            "keys64/human",
            10,
            "48719e8630c1ca9f496b45eabe934a4580ee4da3d7b3a84afdef804e99160c7f",
        ),
        (
            "keys64/welcome",
            2,
            "d211dd9289c7c3bb5e971e8a52e285e449aa9d0aff02b44021f8a9b1c57d6934",
        ),
        (
            "keys64/human",
            10,
            "077cf5dece78438b6ccf9e24eccc909ab96bee39f9f0c79ca625c561f45461fd",
        ),
        (
            "keys64/welcome",
            2,
            "b21f13e73e47c945bdb98519d313bf72629ffe7ab94d570ad000e503e679090e",
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
            "c36c83191c8d37c85a30545e3e0813937febfc784fbb7bf4a4b56c83eac1bd1d",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "35dc5ce9a468ab1e443e035b1c7e71c7155bd65c3d300287c468b979ca297556",
        ),
        (
            "seat_pulled32/human",
            10,
            "0d9f8512f7128610d2e26d32b3965eecd702c9d6ff23b02d50e63a2ce5b1a4cc",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "60cc55e45bc5fc7ef7aa0aea28d3b930d26df19bbae158014b682a0c6fa1bac9",
        ),
        (
            "seat_pulled32/human",
            10,
            "7d7d38c30435538398199f98cdc8947da6a97ae18ebed35ae3f7566d01b6099b",
        ),
        (
            "seat_pulled32/welcome",
            2,
            "657e66c174df7fd4666e421a4679be67c4319d786786ca058c5f1357d2114c8d",
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
