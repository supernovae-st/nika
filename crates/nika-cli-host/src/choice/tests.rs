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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), None);
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
    let json = choice.doctor_cascade_json();
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
    let a = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
    let b = choice.render_human_at(Theme::new(false, true, false), Some(dir.path()));
    assert!(a.contains("Local first"));
    assert!(b.contains("Local first"));
    assert!(a.contains("Next:"));
    assert_eq!(
        a.lines().filter(|l| l.contains("▸")).count(),
        b.lines().filter(|l| l.contains("▸")).count()
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
    let json = choice.doctor_cascade_json();
    assert_eq!(json["chosen_access"], "claude-code");
}

#[test]
fn next_for_a_ready_harness_is_new_hello() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let dir = tempfile::tempdir().expect("tmp");
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
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
    let human = choice.render_human_at(Theme::new(false, false, false), Some(dir.path()));
    let after = next_block(&human);
    assert!(
        after.contains("nika run"),
        "several files → the lazy door, not another scaffold:\n{human}"
    );
    assert!(!after.contains("nika compile hello hello.nika"), "{human}");
}
