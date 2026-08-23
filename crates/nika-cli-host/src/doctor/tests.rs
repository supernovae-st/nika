use std::path::PathBuf;
use std::time::Duration;

use nika_providers::ProviderRegistry;

use super::*;
use crate::probe::client_probe_any;
use nika_providers::probe::{
    ExecutionLocus, ProviderReadiness, env_present, ping_addr, spawn_ping,
};

/// The sober register — the byte-frozen baseline every pipe reads.
const PLAIN: Theme = Theme::new(false, false, false);

fn readiness(configured: bool, locus: ExecutionLocus) -> ProviderReadiness {
    ProviderReadiness::new(
        true,
        configured,
        None,
        None,
        false,
        locus,
        // Synthetic rows model cloud providers unless the locus says
        // loopback — mirrors Profile::access_class.
        match locus {
            ExecutionLocus::Loopback | ExecutionLocus::Lan => {
                nika_providers::probe::AccessClass::Local
            }
            _ => nika_providers::probe::AccessClass::Api,
        },
    )
}

fn cloud(id: &str, var: &str, present: bool) -> ProviderProbe {
    ProviderProbe::new(
        id,
        true,
        present,
        var,
        id != "deepseek",
        readiness(present, ExecutionLocus::Cloud),
        format!("https://api.{id}.example/v1"),
    )
}
fn local(id: &str) -> ProviderProbe {
    ProviderProbe::new(
        id,
        false,
        false,
        "",
        true,
        readiness(true, ExecutionLocus::Loopback),
        "http://127.0.0.1:11434",
    )
}

#[test]
fn provider_row_separates_recognized_from_configured() {
    // P0-5: « recognized » is never « ready » — the row names the
    // two rungs separately, and reachability stays the opt-in
    // `--ping` lane's word (never implied by a present key).
    let present = provider_finding(&cloud("anthropic", "ANTHROPIC_API_KEY", true));
    assert!(
        present
            .detail
            .contains("— api · recognized · configured (key present)"),
        "the access token leads the ladder (D-2026-08-04-N1): {}",
        present.detail
    );
    assert!(
        !present.detail.contains("reachable"),
        "no ping ran — the word must not appear: {}",
        present.detail
    );
    let unset = provider_finding(&cloud("anthropic", "ANTHROPIC_API_KEY", false));
    assert_eq!(unset.level, Level::Warn);
    assert!(
        unset
            .detail
            .contains("— api · recognized · not configured (ANTHROPIC_API_KEY unset)"),
        "{}",
        unset.detail
    );
    assert_eq!(unset.fix.as_deref(), Some("export ANTHROPIC_API_KEY=…"));
}

#[test]
fn a_cloud_proxy_override_is_never_laundered_as_cloud() {
    // P0-20 on the keyed lane: an operator proxy in front of mistral
    // is NAMED with its locus — never read as « the vendor default ».
    let mut p = cloud("mistral", "MISTRAL_API_KEY", true);
    p.endpoint = "https://proxy.corp.example/v1".to_owned();
    p.readiness.execution_locus = ExecutionLocus::Remote;
    let f = provider_finding(&p);
    assert!(
        f.detail.contains("https://proxy.corp.example/v1"),
        "{}",
        f.detail
    );
    assert!(
        f.detail.contains("(remote — not the vendor default)"),
        "{}",
        f.detail
    );
}

#[test]
fn a_lan_override_is_named_on_the_local_lane() {
    // P0-20: ollama pointed at the GPU box must not render « local »
    // — the endpoint and its locus get their own row.
    let mut ollama = local("ollama");
    ollama.endpoint = "http://gpu.lan:11434".to_owned();
    ollama.readiness.execution_locus = ExecutionLocus::Lan;
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.0.0".to_owned(),
        config_path: None,
        providers: vec![ollama],
        clients: Vec::new(),
        kits: Vec::new(),
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let findings = diagnose(&probe);
    let rows: Vec<_> = findings.iter().filter(|f| f.label == "local").collect();
    assert!(
        rows.iter()
            .any(|f| f.detail.contains("http://gpu.lan:11434")
                && f.detail.contains("(lan — not loopback)")),
        "the override is named: {rows:?}"
    );
    // A loopback ollama earns NO extra row — the exception gets the ink.
    let quiet = Probe {
        providers: vec![local("ollama")],
        ..probe
    };
    let findings = diagnose(&quiet);
    assert_eq!(
        findings.iter().filter(|f| f.label == "local").count(),
        1,
        "loopback stays the one summary line: {findings:?}"
    );
}

#[test]
fn instruction_fallback_cloud_is_named_on_the_health_surface() {
    // deepseek carries a key but no native json_schema — the doctor
    // row says so; a native provider's row stays unannotated.
    let deepseek = cloud("deepseek", "DEEPSEEK_API_KEY", true);
    let f = provider_finding(&deepseek);
    assert!(
        f.detail.contains("instruction + local validation"),
        "{}",
        f.detail
    );
    let openai = cloud("openai", "OPENAI_API_KEY", true);
    let f = provider_finding(&openai);
    assert!(!f.detail.contains("instruction"), "{}", f.detail);
}

#[test]
fn key_present_is_ok_and_exits_zero() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.81.0".to_owned(),
        config_path: Some("~/.nika/config.toml".to_owned()),
        providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", true)],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let f = diagnose(&probe);
    let prov = f
        .iter()
        .find(|f| f.label == "provider")
        .expect("provider line");
    assert_eq!(prov.level, Level::Ok);
    assert!(prov.detail.contains("key present"));
    assert_eq!(exit_code(&f), exit::OK);
}

#[test]
fn unset_key_is_a_warn_with_a_fix_not_a_fail() {
    // An unset cloud key is advisory — exit stays 0 (a local provider is a
    // valid path · doctor cannot know which provider the user needs).
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.81.0".to_owned(),
        config_path: None,
        providers: vec![
            cloud("anthropic", "ANTHROPIC_API_KEY", false),
            local("ollama"),
        ],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let f = diagnose(&probe);
    let prov = f
        .iter()
        .find(|f| f.label == "provider")
        .expect("provider line");
    assert_eq!(prov.level, Level::Warn);
    assert_eq!(
        prov.fix.as_deref(),
        Some("export ANTHROPIC_API_KEY=…"),
        "the exact fix command is printed"
    );
    assert!(
        !f.iter().any(|f| f.level == Level::Fail),
        "a local path exists"
    );
    assert_eq!(exit_code(&f), exit::OK);
}

#[test]
fn never_prints_a_secret_value() {
    // The probe carries only a bool — there is no field a value could ride.
    // Assert the rendered report carries the VAR NAME, never a value.
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.81.0".to_owned(),
        config_path: None,
        providers: vec![cloud("openai", "OPENAI_API_KEY", false)],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let text = render(&diagnose(&probe), true, PLAIN);
    assert!(text.contains("OPENAI_API_KEY"), "names the var: {text}");
    assert!(
        !text.contains("sk-"),
        "no secret-shaped value leaks: {text}"
    );
}

#[test]
fn no_provider_at_all_fails_with_exit_three() {
    // A broken/empty catalog — no cloud key AND no local server: the real
    // "cannot infer" environment error (spec §4 ENV).
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.81.0".to_owned(),
        config_path: None,
        providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", false)],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let f = diagnose(&probe);
    assert!(f.iter().any(|f| f.level == Level::Fail));
    assert_eq!(exit_code(&f), exit::ENV);
}

#[test]
fn local_provider_alone_is_a_usable_path_exit_zero() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.81.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama"), local("vllm")],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let f = diagnose(&probe);
    let loc = f.iter().find(|f| f.label == "local").expect("local line");
    assert_eq!(loc.level, Level::Ok);
    assert!(loc.detail.contains("ollama") && loc.detail.contains("vllm"));
    assert_eq!(exit_code(&f), exit::OK, "a local path is usable");
}

#[test]
fn json_lane_carries_summary_and_findings_verbatim() {
    let findings = vec![
        Finding {
            level: Level::Ok,
            label: "binary".to_owned(),
            detail: "v0.96.0".to_owned(),
            fix: None,
        },
        Finding {
            level: Level::Fail,
            label: "config".to_owned(),
            detail: "broken".to_owned(),
            fix: Some("nika wire claude".to_owned()),
        },
    ];
    let json: serde_json::Value =
        serde_json::from_str(&render_json(&findings, AdoptionState::KeyPresent, &[]))
            .expect("valid JSON");
    assert_eq!(json["summary"]["ok"], 1);
    assert_eq!(json["summary"]["fail"], 1);
    assert_eq!(json["findings"][0]["level"], "ok");
    assert_eq!(json["findings"][1]["fix"], "nika wire claude");
}

/// P0-21 — the machine lane carries the adoption rung next to the
/// findings: agents/CI branch on ONE state token, not on a parse of
/// the flat finding rows.
#[test]
fn doctor_json_serializes_the_adoption_state() {
    let findings = vec![Finding {
        level: Level::Ok,
        label: "binary".to_owned(),
        detail: "v0".to_owned(),
        fix: None,
    }];
    for (state, token) in [
        (AdoptionState::Installed, "installed"),
        (AdoptionState::LocalDetected, "local_detected"),
        (AdoptionState::LocalReachable, "local_reachable"),
        (AdoptionState::KeyPresent, "key_present"),
        (AdoptionState::RealReady, "real_ready"),
    ] {
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&findings, state, &[])).expect("valid JSON");
        assert_eq!(json["adoption_state"], token, "{state:?}");
    }
}

/// H5 — the machine lane gains the per-host runtime receipts
/// ADDITIVELY: summary · findings · `adoption_state` stay verbatim,
/// and each receipt carries the verified-vs-assumed provenance the
/// flat findings never could.
#[test]
fn doctor_json_adds_host_receipts_without_touching_existing_fields() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.96.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: vec![
            ClientProbe {
                id: "hermes".to_owned(),
                path: "~/.hermes/config.yaml".to_owned(),
                present: true,
                current: true,
                stale: false,
            },
            ClientProbe {
                id: "cursor".to_owned(),
                path: "~/.cursor/mcp.json".to_owned(),
                present: true,
                current: true,
                stale: false,
            },
        ],
        kits: vec![KitProbe {
            client: "cursor".to_owned(),
            version: "0.106.0".to_owned(),
        }],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let findings = diagnose(&probe);
    let json: serde_json::Value = serde_json::from_str(&render_json(
        &findings,
        AdoptionState::KeyPresent,
        &crate::probe::capability_receipts(&probe),
    ))
    .expect("valid JSON");
    // The pre-H5 fields are untouched (additive means additive).
    assert_eq!(
        json["summary"]["ok"],
        findings.iter().filter(|f| f.level == Level::Ok).count() as u64
    );
    assert_eq!(json["findings"][0]["label"], "binary");
    assert_eq!(json["adoption_state"], "key_present");
    // The receipts lane: one row per probed host, in probe order.
    let receipts = json["receipts"].as_array().expect("receipts array");
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["host"], "hermes");
    assert_eq!(receipts[0]["capability"], "oracle-only");
    assert_eq!(receipts[0]["repair"], "nika wire hermes");
    assert_eq!(receipts[0]["level_assumed"], false);
    assert!(
        receipts[0]["missing_rails"]
            .as_array()
            .expect("rails")
            .iter()
            .any(|rail| rail == "hooks"),
        "{receipts:?}"
    );
    assert_eq!(receipts[1]["host"], "cursor");
    assert_eq!(receipts[1]["capability"], "guarded");
    assert_eq!(receipts[1]["version"], "0.106.0");
    assert_eq!(receipts[1]["level_assumed"], true);
    assert_eq!(
        receipts[1]["components"]
            .as_array()
            .expect("components")
            .len(),
        3
    );
}

// ── The pricing snapshot line (Cost Intelligence 2026-07-08) ──

fn pricing(age_days: Option<u32>) -> PricingProbe {
    PricingProbe {
        as_of: "2026-07-07".to_owned(),
        sha: "d31a39603aa5419d".to_owned(),
        rules: 606,
        providers: 10,
        age_days,
    }
}

#[test]
fn pricing_line_names_the_snapshot_identity() {
    let f = pricing_finding(&pricing(Some(3)));
    assert_eq!(f.level, Level::Ok);
    assert!(f.detail.contains("606 models"), "{}", f.detail);
    assert!(f.detail.contains("10 providers"), "{}", f.detail);
    assert!(f.detail.contains("2026-07-07"), "{}", f.detail);
    assert!(f.detail.contains("d31a39603aa5419d"), "{}", f.detail);
    assert!(
        f.detail.contains("list rates"),
        "the public-catalog basis is named — private/proxy deals are \
             not reflected and the line must say so: {}",
        f.detail
    );
    assert!(f.fix.is_none());
}

#[test]
fn stale_pricing_snapshot_warns_with_the_upgrade_fix() {
    // The staleness gap no surveyed tool closes: past the threshold
    // the line flips ⚠ and prints the exact remedy.
    let f = pricing_finding(&pricing(Some(PRICING_STALE_DAYS + 1)));
    assert_eq!(f.level, Level::Warn);
    assert!(f.detail.contains("days old"), "{}", f.detail);
    assert!(
        f.fix.as_deref().is_some_and(|x| x.contains("upgrade nika")),
        "{:?}",
        f.fix
    );
    // AT the threshold stays green (stale means PAST it).
    assert_eq!(
        pricing_finding(&pricing(Some(PRICING_STALE_DAYS))).level,
        Level::Ok
    );
    // An uncomputable age never guesses stale.
    assert_eq!(pricing_finding(&pricing(None)).level, Level::Ok);
}

/// Each severity prints a DISTINCT glyph — a `Default::default()` mutant
/// (the null char `'\0'`) would erase the level cue the operator scans for.
/// The sidecar row is a BUILD fact: present exactly when this binary
/// carries the `local-infer` feature, absent otherwise — the default
/// doctor stays byte-identical.
#[test]
fn sidecar_row_tracks_the_build_feature() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.99.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let sidecar = diagnose(&probe).into_iter().find(|f| f.label == "sidecar");
    if cfg!(feature = "local-infer") {
        let row = sidecar.expect("built with local-infer — the row must appear");
        assert_eq!(row.level, Level::Ok);
        assert!(row.detail.contains("nika model serve"), "{}", row.detail);
    } else {
        assert!(sidecar.is_none(), "default build carries no sidecar row");
    }
}

/// The models row (issue #146 · the sidecar's dir+count half): pulled
/// models list on ANY build; an empty store rows only on a sidecar
/// build (teaching pull) — the default doctor stays byte-identical.
#[test]
fn models_row_reports_the_dir_and_count_once_pulled() {
    let with_models = ModelsProbe {
        root: Some("/home/x/.nika/models".to_owned()),
        count: 2,
        bytes: 3 * 1024 * 1024 * 1024,
    };
    let rows = models_finding(&with_models);
    assert_eq!(rows.len(), 1, "pulled models row on EVERY build");
    assert_eq!(rows[0].level, Level::Ok);
    assert_eq!(rows[0].label, "models");
    assert!(
        rows[0].detail.contains("/home/x/.nika/models"),
        "{}",
        rows[0].detail
    );
    assert!(rows[0].detail.contains("2 GGUF"), "{}", rows[0].detail);
    assert!(rows[0].detail.contains("3.0 GiB"), "{}", rows[0].detail);
    assert!(
        rows[0].detail.contains("nika model list"),
        "{}",
        rows[0].detail
    );
}

#[test]
fn empty_models_store_teaches_pull_only_on_a_sidecar_build() {
    let rows = models_finding(&ModelsProbe::default());
    if cfg!(feature = "local-infer") {
        assert_eq!(rows.len(), 1, "sidecar build with nothing to serve warns");
        assert_eq!(rows[0].level, Level::Warn);
        assert!(
            rows[0]
                .fix
                .as_deref()
                .is_some_and(|f| f.contains("nika model pull")),
            "{rows:?}"
        );
    } else {
        assert!(rows.is_empty(), "default build + zero pulls = no row");
    }
}

#[test]
fn level_glyphs_are_distinct() {
    assert_eq!(Level::Ok.glyph(), '✔');
    assert_eq!(Level::Warn.glyph(), '⚠');
    assert_eq!(Level::Fail.glyph(), '✖');
}

#[test]
fn the_ascii_theme_folds_every_doctor_glyph() {
    // LANG-04 (gauntlet G-B): `--plain` promises ASCII glyph twins,
    // and doctor's ✔/⚠/· column shipped raw Unicode for a train while
    // the concierge held the contract. The verb composes the same
    // `vocab::sober` seam welcome rides — this pins the fold on a
    // full findings render, not one glyph.
    let wired = |id: &str| ClientProbe {
        id: id.to_owned(),
        path: format!("~/.{id}/cfg"),
        present: true,
        current: true,
        stale: false,
    };
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.106.0".to_owned(),
        config_path: None,
        providers: vec![],
        clients: vec![wired("hermes"), wired("cursor")],
        kits: vec![KitProbe {
            client: "cursor".to_owned(),
            version: "0.106.0".to_owned(),
        }],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let ascii_theme = Theme::new(false, true, false);
    let folded =
        crate::display::vocab::sober(ascii_theme, &render(&diagnose(&probe), true, ascii_theme));
    assert!(
        folded.is_ascii(),
        "ascii doctor render must carry zero Unicode: {folded}"
    );
    assert!(folded.contains("guard-declared"), "{folded}");
}

#[test]
fn doctor_names_each_host_capability_level() {
    // P0-9 — the flat « wired at … » line claimed a host parity
    // that does not exist: an oracle-only host and a guarded one
    // now read as the two DIFFERENT rungs they are.
    let wired = |id: &str| ClientProbe {
        id: id.to_owned(),
        path: format!("~/.{id}/cfg"),
        present: true,
        current: true,
        stale: false,
    };
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.106.0".to_owned(),
        config_path: None,
        providers: vec![],
        clients: vec![wired("hermes"), wired("cursor")],
        kits: vec![KitProbe {
            client: "cursor".to_owned(),
            version: "0.106.0".to_owned(),
        }],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let text = render(&diagnose(&probe), true, PLAIN);
    assert!(
        text.contains("hermes · Nika MCP oracle wired · oracle-only (mcp · no hooks)"),
        "{text}"
    );
    // UX107-04: a table-declared guard never borrows the proven word —
    // the line says `guard-declared … unproven`, and the bare `guarded`
    // token is reserved for a live allow+deny canary (none exists yet).
    assert!(
        text.contains("cursor · Nika MCP oracle wired · guard-declared (kit ships hooks · unproven in session)"),
        "{text}"
    );
    assert!(
        !text.contains("· guarded ("),
        "no assumed guard may render the proven word: {text}"
    );
}

#[test]
fn client_probe_reports_stale_wiring_with_a_wire_fix() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.90.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: vec![ClientProbe {
            id: "cursor".to_owned(),
            path: "~/.cursor/mcp.json".to_owned(),
            present: true,
            current: false,
            stale: true,
        }],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let text = render(&diagnose(&probe), true, PLAIN);
    assert!(text.contains("stale MCP args"), "{text}");
    assert!(text.contains("fix: nika wire cursor"), "{text}");
}

// ── The kit↔binary handshake (the drift surface CI alone carried) ──

fn kit(client: &str, version: &str) -> KitProbe {
    KitProbe {
        client: client.to_owned(),
        version: version.to_owned(),
    }
}

#[test]
fn kit_on_the_binary_train_is_ok_and_patch_drift_is_not_a_finding() {
    // 0.106.0 kit vs 0.106.1 binary — same train, patch releases
    // ship binary-only, the row stays green with no fix.
    let f = kit_finding(&kit("codex", "0.106.0"), "0.106.1");
    assert_eq!(f.level, Level::Ok);
    assert_eq!(f.label, "kit");
    assert!(f.detail.contains("on the binary's train"), "{}", f.detail);
    assert!(f.fix.is_none());
}

#[test]
fn lagging_kit_names_the_refresh_for_its_own_client() {
    // Codex climbs ONE rung; Claude Code climbs TWO — the fix is
    // per-client, never generic when the client is known.
    let f = kit_finding(&kit("codex", "0.104.0"), "0.106.1");
    assert_eq!(f.level, Level::Warn);
    assert!(
        f.detail.contains("lags the binary (0.106.1)"),
        "{}",
        f.detail
    );
    assert_eq!(
        f.fix.as_deref(),
        Some("codex plugin marketplace upgrade nika")
    );

    let f = kit_finding(&kit("claude", "0.104.0"), "0.106.1");
    assert_eq!(
        f.fix.as_deref(),
        Some("claude plugin marketplace update nika, then claude plugin update nika@nika"),
        "both rungs are named — the half-climbed ladder is the proven trap"
    );

    let f = kit_finding(&kit("cursor", "0.104.0"), "0.106.1");
    assert!(
        f.fix
            .as_deref()
            .is_some_and(|x| x.contains("update-mirrors.sh")),
        "{:?}",
        f.fix
    );

    let f = kit_finding(&kit("someclient", "0.104.0"), "0.106.1");
    assert!(
        f.fix.as_deref().is_some_and(|x| x.contains("marketplace")),
        "unknown client still gets a generic refresh: {:?}",
        f.fix
    );
}

#[test]
fn kit_ahead_of_the_binary_names_the_binary_upgrade() {
    let f = kit_finding(&kit("claude", "0.108.0"), "0.106.1");
    assert_eq!(f.level, Level::Warn);
    assert!(f.detail.contains("rides ahead"), "{}", f.detail);
    assert_eq!(f.fix.as_deref(), Some("brew upgrade nika"));
}

#[test]
fn unparseable_kit_version_warns_without_guessing_a_train() {
    let f = kit_finding(&kit("codex", "garbage"), "0.106.1");
    assert_eq!(f.level, Level::Warn);
    assert!(f.detail.contains("unparseable"), "{}", f.detail);
    assert!(f.fix.is_none(), "no fix can be honest without a train");
}

#[test]
fn diagnose_carries_one_kit_row_per_found_surface() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.106.1".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: vec![],
        kits: vec![kit("codex", "0.106.0"), kit("claude", "0.104.0")],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let findings = diagnose(&probe);
    let kits: Vec<_> = findings.iter().filter(|f| f.label == "kit").collect();
    assert_eq!(kits.len(), 2, "one row per found kit, absence is silence");
    assert_eq!(kits[0].level, Level::Ok);
    assert_eq!(kits[1].level, Level::Warn);
}

#[test]
fn major_minor_parses_trains_and_rejects_junk() {
    assert_eq!(major_minor("0.106.1"), Some((0, 106)));
    assert_eq!(
        major_minor("1.0.0-rc.2"),
        Some((1, 0)),
        "an rc rides its release's train — the tag lives in the patch slot"
    );
    assert_eq!(major_minor("garbage"), None);
    assert_eq!(major_minor(""), None);
    assert_eq!(major_minor("7"), None, "a train needs both components");
}

#[test]
fn cursor_probe_accepts_workspace_config_from_extension() {
    let dir = temp_dir("cursor-workspace");
    let global = dir.join("home").join(".cursor").join("mcp.json");
    let workspace = dir.join("repo").join(".cursor").join("mcp.json");
    std::fs::create_dir_all(workspace.parent().expect("parent")).expect("mkdir");
    std::fs::write(
        &workspace,
        r#"{"mcpServers":{"nika":{"command":"nika","args":["mcp"]}}}"#,
    )
    .expect("fixture");

    let paths = vec![global, workspace.clone()];
    let probe = client_probe_any("cursor", &paths, &["mcpServers", "nika"]);
    assert!(probe.current);
    assert_eq!(probe.path, workspace.display().to_string());
    let _ = std::fs::remove_dir_all(dir);
}

/// `env_present` is a PRESENCE check (set + non-empty) — read against real
/// vars so no racy `set_var` is needed. PATH is always set + non-empty
/// (kills the `-> false` constant + the `!is_empty` negation); a name
/// nothing sets is absent (kills the `-> true` constant).
#[test]
fn env_present_reflects_the_real_environment() {
    assert!(env_present("PATH"), "PATH is set + non-empty");
    assert!(!env_present("NIKA_CLI_DEFINITELY_UNSET_VARIABLE_XYZZY"));
}

#[test]
fn the_real_catalog_has_no_fail_and_renders() {
    // The wired run() over the canonical catalog: local providers exist, so
    // there is never a hard Fail (exit 0) even with no keys in the test env.
    // ping=false — the default run stays fully offline, in tests too.
    let out = run(false, false, true, PLAIN);
    assert_eq!(out.code, exit::OK, "the catalog always offers a path");
    assert!(out.text.contains("binary"), "renders the binary line");
    // The LLM test backend stays hidden (no `mock — key` provider row);
    // the IMAGE line's `mock ready` is operator-facing truth (the image
    // mock is a documented, always-available provider).
    assert!(
        !out.text.contains("mock —"),
        "the LLM test backend is hidden"
    );
    assert!(out.text.contains("image"), "the image plane renders");
    assert!(out.text.contains("tts"), "the tts plane renders");
}

/// The report opens on the ONE verdict line — level counts first,
/// sections unchanged below. The glyph tracks the WORST level (✔
/// with warns · ✖ only on a fail).
#[test]
fn render_opens_with_the_verdict_count_line() {
    let ok = Finding {
        level: Level::Ok,
        label: "binary".to_owned(),
        detail: "v0 · self-contained".to_owned(),
        fix: None,
    };
    let warn = Finding {
        level: Level::Warn,
        label: "provider".to_owned(),
        detail: "x — KEY unset".to_owned(),
        fix: Some("export KEY=…".to_owned()),
    };
    let text = render(&[ok.clone(), ok.clone(), warn.clone()], true, PLAIN);
    let first = text.lines().next().expect("verdict line");
    assert_eq!(first, "✔ 2 ok · 1 warn · 0 fail");
    assert!(text.contains("binary"), "sections unchanged: {text}");

    let fail = Finding {
        level: Level::Fail,
        label: "providers".to_owned(),
        detail: "no path".to_owned(),
        fix: None,
    };
    let red = render(&[ok, warn, fail], true, PLAIN);
    assert!(
        red.starts_with("✖ 1 ok · 1 warn · 1 fail"),
        "a fail flips the verdict glyph: {red}"
    );
}

#[allow(clippy::disallowed_methods)]
fn temp_dir(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
        || {
            std::env::current_dir()
                .expect("current dir")
                .join("target")
                .join("tmp")
        },
        PathBuf::from,
    );
    let dir = base.join(format!("nika-doctor-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn ping_addr_extracts_authority_and_defaults_ports() {
    assert_eq!(
        ping_addr("http://127.0.0.1:11434/v1/chat/completions").as_deref(),
        Some("127.0.0.1:11434")
    );
    assert_eq!(
        ping_addr("https://example.test/v1").as_deref(),
        Some("example.test:443")
    );
    assert_eq!(ping_addr("http://host/v1").as_deref(), Some("host:80"));
    assert_eq!(ping_addr("ftp://x"), None);
    assert_eq!(ping_addr("http://"), None);
}

/// The single-probe composition the parallel collector applies per
/// surface — kept here so the probe contract stays directly tested.
fn ping_once(addr: &str, timeout: Duration) -> PingState {
    match spawn_ping(addr, timeout).recv_timeout(timeout) {
        Ok(Some(ms)) => PingState::Reachable(ms),
        _ => PingState::Unreachable,
    }
}

#[test]
fn userinfo_never_prints_and_never_dials() {
    // F5: an operator embedding basic-auth in a local URL must not
    // see it echoed (CI logs) nor dialed as part of the host.
    assert_eq!(
        redact_userinfo("http://user:s3cret@tts.lan:8080/v1"),
        "http://***@tts.lan:8080/v1"
    );
    assert_eq!(
        redact_userinfo("http://tts.lan:8080"),
        "http://tts.lan:8080"
    );
    assert_eq!(redact_userinfo("no-scheme"), "no-scheme");
    assert_eq!(
        ping_addr("http://user:s3cret@tts.lan:8080/v1").as_deref(),
        Some("tts.lan:8080")
    );
}

#[test]
fn effective_base_url_reaches_the_ping() {
    use nika_providers::ProvidersConfig;
    // The anti-doctor fix: --ping probes the OVERRIDDEN url, not the
    // seed — the registry answers the override when one is present.
    let config =
        ProvidersConfig::new().with_base_url("ollama", "http://10.9.9.9:7777/v1/chat/completions");
    let reg = ProviderRegistry::without_http(config);
    assert_eq!(
        reg.effective_base_url("ollama"),
        Some("http://10.9.9.9:7777/v1/chat/completions")
    );
    let reg2 = ProviderRegistry::without_http(ProvidersConfig::new());
    assert!(
        reg2.effective_base_url("ollama")
            .is_some_and(|u| u.contains("127.0.0.1:11434"))
    );
    assert_eq!(reg2.effective_base_url("nope"), None);
}

#[test]
fn tcp_ping_reachable_and_unreachable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    assert!(matches!(
        ping_once(&addr, Duration::from_millis(300)),
        PingState::Reachable(_)
    ));
    // Bind then drop → the port is free again: nothing listens on it.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").to_string()
    };
    assert_eq!(
        ping_once(&closed, Duration::from_millis(300)),
        PingState::Unreachable
    );
    assert_eq!(
        ping_once("not-an-addr", Duration::from_millis(300)),
        PingState::Unreachable
    );
}

#[test]
fn local_line_hands_off_to_ping_and_ping_lines_render() {
    let base = Probe {
        models: ModelsProbe::default(),
        version: "0.0.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: Vec::new(),
        kits: Vec::new(),
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let findings = diagnose(&base);
    let local = findings
        .iter()
        .find(|f| f.label == "local")
        .expect("local line");
    assert!(
        local.fix.as_deref().is_some_and(|f| f.contains("--ping")),
        "unpinged run must hand off to --ping"
    );

    let pinged = Probe {
        models: ModelsProbe::default(),
        local_pings: vec![
            (
                "ollama".to_owned(),
                "127.0.0.1:11434".to_owned(),
                PingState::Reachable(3),
            ),
            (
                "vllm".to_owned(),
                "127.0.0.1:8000".to_owned(),
                PingState::Unreachable,
            ),
        ],
        ..base
    };
    let findings = diagnose(&pinged);
    let pings: Vec<_> = findings.iter().filter(|f| f.label == "ping").collect();
    assert_eq!(pings.len(), 2);
    assert!(pings[0].detail.contains("listening on 127.0.0.1:11434"));
    assert_eq!(pings[0].level, Level::Ok);
    assert!(pings[1].detail.contains("nothing listening"));
    assert_eq!(pings[1].level, Level::Warn);
    let local = findings
        .iter()
        .find(|f| f.label == "local")
        .expect("local line");
    assert!(local.fix.is_none(), "pinged run drops the hand-off");
}

/// The trace-leak signal (the doctor half of init's `.gitignore`
/// guarantee): a repo that TRACKS its run journals gets ONE loud warn
/// naming the count and the exact remedy; zero tracked — or an
/// unobserved surface (no git · no repo) — is silence, never a guess.
#[test]
fn tracked_trace_journals_warn_with_the_untrack_remedy() {
    let rows = tracked_traces_finding(Some(2));
    assert_eq!(rows.len(), 1, "one row, never a pile");
    let f = &rows[0];
    assert_eq!(f.level, Level::Warn, "advisory — the env works");
    assert_eq!(f.label, "traces");
    assert!(f.detail.contains("2 run journals"), "{}", f.detail);
    assert!(f.detail.contains("tracked by git"), "{}", f.detail);
    let fix = f.fix.as_deref().expect("the remedy is printed");
    assert!(
        fix.contains("git rm") && fix.contains("--cached") && fix.contains(".nika/traces"),
        "the untrack command, copy-paste ready: {fix}"
    );
    assert!(
        tracked_traces_finding(Some(0)).is_empty(),
        "nothing tracked — nothing to say"
    );
    assert!(
        tracked_traces_finding(None).is_empty(),
        "unobserved is silence"
    );
}

/// Through the full diagnose lane: the row rides next to the retention
/// one, a warn never moves the exit code, and the calm render may NOT
/// fold it (the fold classes are the healthy machine's three — a leak
/// earned its row).
#[test]
fn diagnose_surfaces_the_tracked_traces_row_without_failing() {
    let probe = Probe {
        models: ModelsProbe::default(),
        version: "0.0.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        clients: vec![],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 1,
        tracked_traces: Some(1),
    };
    let findings = diagnose(&probe);
    let rows: Vec<_> = findings
        .iter()
        .filter(|f| f.label == "traces" && f.detail.contains("tracked by git"))
        .collect();
    assert_eq!(rows.len(), 1, "the leak row rides diagnose: {findings:?}");
    assert_eq!(rows[0].level, Level::Warn);
    assert_eq!(exit_code(&findings), exit::OK, "a warn never fails the env");
    let calm = render(&findings, false, PLAIN);
    assert!(calm.contains("tracked by git"), "never folded: {calm}");
}

/// Every label `diagnose` can emit fits STRICTLY inside the fixed
/// [`LABEL_COL`] cell — the grid never shears (nextest school).
#[test]
fn every_label_fits_the_fixed_column() {
    for label in [
        "binary",
        "config",
        "lsp",
        "mcp",
        "sidecar",
        "traces",
        "agent",
        "kit",
        "registry",
        "provider",
        "providers",
        "local",
        "ping",
        "image",
        "tts",
        "pricing",
    ] {
        assert!(
            label.len() <= LABEL_COL,
            "label `{label}` ({}) exceeds LABEL_COL ({LABEL_COL})",
            label.len()
        );
    }
}

/// Item-3 wiring: on the linked register an https target inside a
/// detail line rides the OSC-8 wrapper (text unchanged); the sober
/// register — every pipe — keeps its exact bytes, zero escapes.
#[test]
fn linked_register_wraps_https_targets_and_sober_stays_frozen() {
    let f = vec![Finding {
        level: Level::Ok,
        label: "pricing".to_owned(),
        detail: "snapshot · see https://docs.nika.sh/errors for codes".to_owned(),
        fix: None,
    }];
    let sober = render(&f, true, PLAIN);
    assert!(
        !sober.contains('\x1b'),
        "sober register is escape-free: {sober:?}"
    );
    let mut linked = PLAIN;
    linked.links = true;
    let out = render(&f, true, linked);
    assert!(
        out.contains(
            "\x1b]8;;https://docs.nika.sh/errors\x1b\\https://docs.nika.sh/errors\x1b]8;;\x1b\\"
        ),
        "https target rides OSC-8: {out:?}"
    );
}

// ── The registry coverage row (H6 · Q1 2026-07-31) ──

/// The row renders the derived counts and NAMES the wireable
/// clients doctor cannot probe — a declared-not-probed client is
/// listed, never silently dropped; an unparsed registry is a loud
/// warning, never a silent zero.
#[test]
fn registry_finding_names_the_declared_not_probed() {
    let cov = RegistryCoverage {
        declared: 31,
        wireable: 15,
        probed: 6,
        wire_pending: 2,
        declared_not_probed: vec!["cline".to_owned(), "codex".to_owned()],
    };
    let f = registry_finding(&cov);
    assert_eq!(f.level, Level::Ok);
    assert_eq!(f.label, "registry");
    assert!(f.detail.contains("31 declared"), "{}", f.detail);
    assert!(f.detail.contains("15 wireable"), "{}", f.detail);
    assert!(f.detail.contains("6 probed"), "{}", f.detail);
    assert!(f.detail.contains("declared-not-probed"), "{}", f.detail);
    assert!(f.detail.contains("cline"), "named: {}", f.detail);
    assert!(f.detail.contains("codex"), "named: {}", f.detail);
    assert!(f.detail.contains("wire-pending"), "{}", f.detail);
    assert!(f.fix.is_none(), "no repair: the gap is engine-side");

    let quiet = registry_finding(&RegistryCoverage {
        declared: 31,
        wireable: 6,
        probed: 6,
        wire_pending: 0,
        declared_not_probed: vec![],
    });
    assert!(
        !quiet.detail.contains("declared-not-probed"),
        "no gap, no gap line: {}",
        quiet.detail
    );
    assert!(
        !quiet.detail.contains("wire-pending"),
        "no pending, no pending line: {}",
        quiet.detail
    );

    let broken = registry_finding(&RegistryCoverage::default());
    assert_eq!(broken.level, Level::Warn);
    assert!(broken.detail.contains("unavailable"), "{}", broken.detail);
}

/// `diagnose` emits EXACTLY ONE registry row, after the client/kit
/// lanes, with the counts the probe derived.
#[test]
fn diagnose_emits_one_registry_coverage_row() {
    let base = Probe {
        models: ModelsProbe::default(),
        version: "0.0.0".to_owned(),
        config_path: None,
        providers: vec![],
        clients: Vec::new(),
        kits: Vec::new(),
        clients_registry: RegistryCoverage {
            declared: 31,
            wireable: 15,
            probed: 6,
            wire_pending: 0,
            declared_not_probed: vec!["cline".to_owned()],
        },
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    };
    let findings = diagnose(&base);
    let rows: Vec<_> = findings.iter().filter(|f| f.label == "registry").collect();
    assert_eq!(rows.len(), 1, "one coverage row: {findings:?}");
    assert!(rows[0].detail.contains("31 declared"), "{}", rows[0].detail);
}

/// B-8b (the 2026-07-31 gauntlet): a healthy keyless machine printed
/// 13+ ⚠ rows and the alarm glyph taught the user to ignore it. The
/// calm default folds the three advisory classes into ONE line that
/// names the counts and the unfolding flag — the verdict line keeps
/// counting the truth, and any OTHER warn keeps its row.
#[test]
fn the_calm_default_folds_the_healthy_machines_notes_into_one_line() {
    let ok = Finding {
        level: Level::Ok,
        label: "binary".to_owned(),
        detail: "v0.107.0 · self-contained".to_owned(),
        fix: None,
    };
    let agent = |id: &str| Finding {
        level: Level::Warn,
        label: "agent".to_owned(),
        detail: format!("{id} not wired"),
        fix: Some(format!("nika wire {id}")),
    };
    let provider = |id: &str, var: &str| Finding {
        level: Level::Warn,
        label: "provider".to_owned(),
        detail: format!("{id} — recognized · not configured ({var} unset)"),
        fix: Some(format!("export {var}=…")),
    };
    let config = Finding {
        level: Level::Warn,
        label: "config".to_owned(),
        detail: "none — built-in defaults".to_owned(),
        fix: None,
    };
    let findings = vec![
        ok,
        config,
        agent("cursor"),
        agent("vscode"),
        provider("mistral", "MISTRAL_API_KEY"),
        provider("openai", "OPENAI_API_KEY"),
    ];
    let calm = render(&findings, false, PLAIN);
    let first = calm.lines().next().expect("verdict line");
    assert_eq!(first, "✔ 1 ok · 5 warn · 0 fail", "the truth still counts");
    assert!(
        !calm.lines().any(|l| l.contains("⚠")),
        "no alarm glyph on a healthy machine: {calm}"
    );
    let fold = calm
        .lines()
        .find(|l| l.contains("advisory"))
        .expect("the ONE calm line");
    assert!(
        fold.contains("2 agents unwired")
            && fold.contains("2 providers unconfigured")
            && fold.contains("config defaults")
            && fold.contains("--verbose"),
        "the fold names every class and the door: {fold}"
    );
    // The verbose lane unfolds each note — nothing is lost.
    let loud = render(&findings, true, PLAIN);
    assert!(loud.contains("cursor not wired"), "{loud}");
    assert!(loud.contains("MISTRAL_API_KEY unset"), "{loud}");
    assert!(loud.contains("built-in defaults"), "{loud}");
    assert!(
        !loud.lines().any(|l| l.contains("advisory")),
        "no fold line under --verbose: {loud}"
    );
}

/// A Warn OUTSIDE the three healthy classes never folds — a dead ping
/// earned its row.
#[test]
fn an_unlisted_warn_never_folds() {
    let ping = Finding {
        level: Level::Warn,
        label: "ping".to_owned(),
        detail: "ollama — nothing listening on 127.0.0.1:11434".to_owned(),
        fix: None,
    };
    let calm = render(&[ping], false, PLAIN);
    assert!(
        calm.contains("nothing listening on 127.0.0.1:11434"),
        "{calm}"
    );
    assert!(
        !calm.lines().any(|l| l.contains("advisory")),
        "no fold line without a foldable class: {calm}"
    );
}

/// The sandbox row (#891): a confined backend is Ok and names its
/// mechanism — the Linux row carries the landlock id AND the allowlist
/// residual, never a full-strength claim (#822 P3 · #893 owed).
#[test]
fn a_confined_backend_reads_ok_with_its_mechanism_named() {
    let seatbelt = sandbox_finding(&crate::probe::SandboxProbe {
        backend: "seatbelt",
        confined: true,
    });
    assert_eq!(seatbelt.level, Level::Ok);
    assert!(seatbelt.detail.contains("seatbelt"), "{seatbelt:?}");

    let landlock = sandbox_finding(&crate::probe::SandboxProbe {
        backend: "landlock",
        confined: true,
    });
    assert_eq!(landlock.level, Level::Ok);
    assert!(landlock.detail.contains("bubblewrap"), "{landlock:?}");
    assert!(
        landlock.detail.contains("allowlist = follow-on"),
        "the residual is named, never greenwashed: {landlock:?}"
    );
}

/// A noop backend WARNS with the per-OS fix (#891 · #822 P1) — never a
/// green "sandboxed" over an unconfined spawn.
#[test]
fn a_noop_backend_warns_with_the_exact_fix() {
    let finding = sandbox_finding(&crate::probe::SandboxProbe {
        backend: "noop",
        confined: false,
    });
    assert_eq!(finding.level, Level::Warn);
    assert!(finding.detail.contains("UNCONFINED"), "{finding:?}");
    let fix = finding.fix.expect("a noop carries the printed fix");
    if cfg!(target_os = "linux") {
        assert!(fix.contains("bubblewrap"), "{fix}");
    } else {
        assert!(fix.contains("sandbox-exec"), "{fix}");
    }
}

#[test]
fn serve_row_is_silent_without_a_token_file() {
    let dir = temp_dir("serve-absent");
    assert!(serve_http_door(&dir).is_empty());
}

#[cfg(unix)]
#[test]
fn serve_row_fails_when_the_token_is_world_readable() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("serve-mode");
    let nika = dir.join(".nika");
    std::fs::create_dir(&nika).expect("nika dir");
    let token = nika.join("serve.token");
    std::fs::write(&token, "x".repeat(40)).expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).expect("mode");
    let rows = serve_http_door(&dir);
    assert_eq!(rows[0].level, Level::Fail);
    assert!(
        rows[0]
            .fix
            .as_deref()
            .is_some_and(|f| f.contains("chmod 600"))
    );
    assert!(!format!("{rows:?}").contains("xxxx"));
}

#[cfg(unix)]
#[test]
fn serve_row_ok_names_tls_as_the_proxy_and_hides_the_secret() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("serve-ok");
    let nika = dir.join(".nika");
    std::fs::create_dir(&nika).expect("nika dir");
    let token = nika.join("serve.token");
    std::fs::write(&token, "s3cret-token-value-never-printed!!").expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
    let rows = serve_http_door(&dir);
    assert_eq!(rows[0].level, Level::Ok);
    assert!(rows[0].detail.contains("does not terminate TLS"));
    assert!(!rows[0].detail.contains("s3cret"));
}

#[cfg(unix)]
#[test]
fn serve_row_fails_when_the_token_is_too_short_and_does_not_echo_it() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("serve-short");
    let nika = dir.join(".nika");
    std::fs::create_dir(&nika).expect("nika dir");
    let token = nika.join("serve.token");
    let secret = "too-short-secret";
    std::fs::write(&token, secret).expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
    let rows = serve_http_door(&dir);
    assert_eq!(rows[0].level, Level::Fail);
    assert!(
        rows[0]
            .fix
            .as_deref()
            .is_some_and(|f| f.contains("openssl rand -hex 24"))
    );
    assert!(!format!("{rows:?}").contains(secret));
}

#[cfg(unix)]
#[test]
fn serve_row_fails_when_the_token_is_a_symlink() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("serve-symlink");
    let nika = dir.join(".nika");
    std::fs::create_dir(&nika).expect("nika dir");
    let target = dir.join("real.token");
    std::fs::write(&target, "a".repeat(32)).expect("target");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).expect("mode");
    std::os::unix::fs::symlink(&target, nika.join("serve.token")).expect("symlink");
    let rows = serve_http_door(&dir);
    assert_eq!(rows[0].level, Level::Fail);
    assert!(rows[0].detail.contains("symlink"));
    assert!(
        rows[0]
            .fix
            .as_deref()
            .is_some_and(|f| f.contains("openssl rand -hex 24"))
    );
}

#[cfg(feature = "access-harness")]
#[test]
fn doctor_lists_every_agentic_cli_runtime() {
    let findings = super::harness_findings();
    assert_eq!(findings.len(), 5, "{findings:?}");
    let text: String = findings
        .iter()
        .map(|f| format!("{} {}", f.label, f.detail))
        .collect::<Vec<_>>()
        .join("\n");
    for token in [
        "claude-code",
        "codex",
        "gemini-cli",
        "kimi-code",
        "qwen-code",
    ] {
        assert!(text.contains(token), "missing {token} in:\n{text}");
    }
    assert!(
        findings.iter().all(|f| f.label == "runtime"),
        "ACP runtimes must not reuse the MCP-wire `agent` label: {text}"
    );
    assert!(
        !text.contains("Nika MCP oracle"),
        "runtime rows must not market MCP wire: {text}"
    );
}
