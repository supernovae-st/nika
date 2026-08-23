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

fn claude() -> Seat {
    Seat {
        id: "claude-agent-acp".to_owned(),
        name: "Claude Code".to_owned(),
        detected: true,
        authenticated: true,
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
    let human = choice.render_human(Theme::new(false, false, false));
    assert!(human.contains("Claude Code"), "{human}");
    assert!(human.contains("nika new hello"), "{human}");
    assert!(!human.contains("xai/grok-4"), "{human}");
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
fn stamp_writes_the_cascade_model() {
    let body = "nika: hello\nmodel: ollama/llama3.2:3b\ntasks: {}\n";
    let stamped = stamp_body(body, "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M");
    assert!(stamped.contains("model: 'unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M'"));
    assert!(!stamped.contains("ollama/"));
}

#[test]
fn skeletons_name_only_the_cascade_alias_or_are_stamped() {
    let dir = pack_template_dir();
    assert!(dir.is_dir(), "pack templates: {}", dir.display());
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let want = runnable_stamp_model(&choice);
    assert!(
        nika_providers::resolve_refusal(&want).is_none(),
        "stamp must be a runnable provider/model, got {want}"
    );
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).expect("templates") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("negative"))
        {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        scanned += 1;
        let stamped = stamp_body(&body, &want);
        let model = stamped
            .lines()
            .find(|l| l.starts_with("model: "))
            .map(|l| l.trim_start_matches("model: ").trim().trim_matches('\''));
        assert_eq!(
            model,
            Some(want.as_str()),
            "{} kept a model the cascade did not stamp",
            path.display()
        );
        assert!(
            !stamped
                .lines()
                .any(|l| l.starts_with("model: ") && l.contains("unsloth/")),
            "{} stamped a Hub id: {stamped}",
            path.display()
        );
    }
    assert!(scanned >= 8, "scanned {scanned} skeletons");
}

#[test]
fn stamp_falls_back_to_mock_when_the_cascade_id_is_not_runnable() {
    let local = collect_from(&machine(Some(18), vec![], false, &[], true));
    assert!(
        nika_providers::resolve_refusal(&local.chosen_model).is_some(),
        "local chosen_model is a Hub pull id: {}",
        local.chosen_model
    );
    assert_eq!(runnable_stamp_model(&local), "mock/echo");

    let harness = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    assert!(harness.chosen_model.starts_with("harness/"));
    assert_eq!(runnable_stamp_model(&harness), "mock/echo");

    let keyed = collect_from(&machine(
        Some(18),
        vec![],
        false,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    assert_eq!(runnable_stamp_model(&keyed), keyed.chosen_model);
    assert!(nika_providers::resolve_refusal(&keyed.chosen_model).is_none());
}

#[test]
fn tty_and_pipe_project_the_same_product() {
    let choice = collect_from(&machine(Some(8), vec![], false, &[], false));
    let a = choice.render_human(Theme::new(false, false, false));
    let b = choice.render_human(Theme::new(false, true, false));
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
        yaml.contains("\n  reply:\n"),
        "task `reply:` must be indented under tasks: — a `\\` continuation ate the YAML:\n{yaml}"
    );
    let parsed = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    );
    assert!(
        parsed.is_ok(),
        "first-wow yaml must parse: {parsed:?}\n{yaml}"
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
fn harness_first_wow_is_agent_without_vendor_model() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let yaml = first_wow_yaml(&choice);
    assert!(yaml.contains("agent:"), "{yaml}");
    assert!(!yaml.contains("infer:"), "{yaml}");
    assert!(!yaml.contains("ollama/"), "{yaml}");
    assert!(!yaml.contains("xai/grok-4"), "{yaml}");
    assert!(!yaml.contains("harness/claude"), "{yaml}");
    assert!(yaml.contains("yaml-language-server"), "{yaml}");
    assert_parses(&yaml);
}

#[test]
fn key_first_wow_is_infer_with_the_cascade_model() {
    let choice = collect_from(&machine(
        Some(18),
        vec![],
        false,
        &["ANTHROPIC_API_KEY"],
        true,
    ));
    let yaml = first_wow_yaml(&choice);
    assert!(yaml.contains("infer:"), "{yaml}");
    assert!(yaml.contains("anthropic/"), "{yaml}");
    assert!(!yaml.contains("agent:"), "{yaml}");
    assert!(!yaml.contains("ollama/"), "{yaml}");
    assert_parses(&yaml);
}

#[test]
fn local_unready_first_wow_runs_on_mock_and_names_the_pull() {
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    assert_eq!(choice.arrow, "local");
    let yaml = first_wow_yaml(&choice);
    assert!(yaml.contains("infer:"), "{yaml}");
    assert!(yaml.contains("model: mock/echo"), "{yaml}");
    assert!(
        yaml.contains(&format!("nika model pull {}", choice.local_pull)),
        "{yaml}"
    );
    assert!(
        !yaml
            .lines()
            .any(|l| l.starts_with("model: ") && l.contains("unsloth/")),
        "Hub ids are pull targets, not model: values:\n{yaml}"
    );
    assert!(!yaml.contains("agent:"), "{yaml}");
    assert_parses(&yaml);
    let dir = tempfile::tempdir().expect("tmp");
    let dest = dir.path().join("hello.nika.yaml");
    let out = write_first_wow_from(&dest, false, &choice);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("nika run"), "{}", out.text);
}

#[test]
fn write_first_wow_refuses_without_force() {
    let dir = tempfile::tempdir().expect("tmp");
    let dest = dir.path().join("hello.nika.yaml");
    std::fs::write(&dest, "stale\n").expect("seed");
    let choice = collect_from(&machine(Some(18), vec![], false, &[], true));
    let out = write_first_wow_from(&dest, false, &choice);
    assert_ne!(out.code, 0);
    assert!(out.text.contains("--force"), "{}", out.text);
    assert_eq!(std::fs::read_to_string(&dest).expect("kept"), "stale\n");
}

#[test]
fn write_first_wow_lands_a_file_and_names_run() {
    let dir = tempfile::tempdir().expect("tmp");
    let dest = dir.path().join("hello.nika.yaml");
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let out = write_first_wow_from(&dest, false, &choice);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(dest.is_file());
    assert!(out.text.contains("wrote"), "{}", out.text);
    assert!(
        out.text.contains("nika run") && out.text.contains("--access claude-agent-acp"),
        "{}",
        out.text
    );
    let body = std::fs::read_to_string(&dest).expect("body");
    assert!(body.contains("agent:"));
    assert!(!body.contains("xai/grok-4"));
    assert!(!body.contains("ollama/"));
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
fn first_wow_slug_is_not_the_lesson_pack() {
    assert!(!is_first_wow(Some("01-hello"), None));
    assert!(!is_first_wow(Some("chain"), Some("hello.nika.yaml")));
    assert_eq!(first_wow_dest(None), FIRST_WOW_DEST);
    assert_eq!(first_wow_dest(Some("hello")), FIRST_WOW_DEST);
}

#[test]
fn chosen_access_is_the_seat_when_harness_has_the_arrow() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    assert_eq!(choice.chosen_access.as_deref(), Some("claude-agent-acp"));
    let json = choice.doctor_cascade_json();
    assert_eq!(json["chosen_access"], "claude-agent-acp");
}

#[test]
fn next_for_a_ready_harness_is_new_hello() {
    let choice = collect_from(&machine(Some(18), vec![claude()], false, &[], true));
    let human = choice.render_human(Theme::new(false, false, false));
    assert!(human.contains("nika new hello"), "{human}");
    assert!(human.contains("this machine · 18 GB"), "{human}");
}
