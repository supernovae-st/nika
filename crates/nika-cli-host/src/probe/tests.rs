use super::*;

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
    let dir = base.join(format!("nika-probe-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn manifest_version_reads_the_one_field() {
    let dir = temp_dir("manifest");
    let path = dir.join("plugin.json");
    std::fs::write(&path, r#"{"name":"nika","version":"0.106.0"}"#).expect("fixture");
    assert_eq!(manifest_version(&path).as_deref(), Some("0.106.0"));
    assert_eq!(manifest_version(&dir.join("absent.json")), None);
    std::fs::write(&path, "not json").expect("fixture");
    assert_eq!(manifest_version(&path), None, "unreadable is silence");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn claude_install_rung_is_the_json_of_record() {
    let dir = temp_dir("claude-install");
    std::fs::write(
        dir.join("installed_plugins.json"),
        r#"{"version":2,"plugins":{"nika@nika":[{"scope":"user","version":"0.105.0"}],
                "other@x":[{"version":"9.9.9"}]}}"#,
    )
    .expect("fixture");
    assert_eq!(
        claude_installed_version(&dir).as_deref(),
        Some("0.105.0"),
        "the ACTIVE entry, never another plugin's"
    );
    assert_eq!(claude_installed_version(&temp_dir("claude-empty")), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn codex_cache_picks_the_highest_version_numerically() {
    let dir = temp_dir("codex-cache");
    for d in ["0.9.0", "0.105.0", "0.104.2", "tmp", "0.105"] {
        std::fs::create_dir_all(dir.join(d)).expect("fixture dir");
    }
    // 0.9.0 > 0.105.0 lexically — numeric ordering is the assertion.
    assert_eq!(codex_cache_version(&dir).as_deref(), Some("0.105.0"));
    assert_eq!(codex_cache_version(&dir.join("absent")), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn train_differs_compares_trains_and_never_guesses() {
    assert!(train_differs("0.105.0", "0.106.1"));
    assert!(!train_differs("0.106.0", "0.106.1"), "patch is not a train");
    assert!(
        !train_differs("garbage", "0.106.1"),
        "unparseable = never guess"
    );
    assert!(!train_differs("0.106.0", ""), "either side");
}

#[test]
fn version_key_is_numeric_and_rejects_non_versions() {
    assert!(version_key("0.105.0") > version_key("0.9.0"));
    assert_eq!(
        version_key("0.105"),
        Some((0, 105, 0)),
        "missing patch reads 0"
    );
    assert_eq!(version_key("tmp"), None);
    assert_eq!(version_key("1"), None, "a version needs major.minor");
}

#[test]
fn iso_to_epoch_days_civil_math() {
    assert_eq!(iso_to_epoch_days("1970-01-01"), Some(0));
    assert_eq!(iso_to_epoch_days("1970-01-02"), Some(1));
    // The leap day: 2000-02-29 exists, 03-01 is exactly one later.
    let feb29 = iso_to_epoch_days("2000-02-29").expect("leap day");
    let mar01 = iso_to_epoch_days("2000-03-01").expect("march 1");
    assert_eq!(mar01 - feb29, 1);
    assert!(iso_to_epoch_days("2026-07-07").expect("modern date") > 20_000);
    for bad in ["2026-13-01", "2026-7-07", "garbage", ""] {
        assert_eq!(iso_to_epoch_days(bad), None, "{bad:?}");
    }
}

#[test]
fn snapshot_age_never_reads_the_future_as_stale() {
    let today = iso_to_epoch_days("2026-07-08");
    assert_eq!(snapshot_age_days("2026-07-07", today), Some(1));
    assert_eq!(snapshot_age_days("2026-07-08", today), Some(0));
    // Clock skew: a snapshot "from tomorrow" reads unknowable, never stale.
    assert_eq!(snapshot_age_days("2026-07-09", today), None);
    assert_eq!(snapshot_age_days("garbage", today), None);
    assert_eq!(snapshot_age_days("2026-07-07", None), None);
}

#[test]
fn the_real_probe_fills_pricing_from_the_vendored_snapshot() {
    // Derived counts (born-stale law): non-empty · providers ≥ the
    // cloud set · identity mirrors the catalog accessor.
    let p = pricing_probe();
    assert!(p.rules > 100, "got {}", p.rules);
    assert!(p.providers >= 5, "got {}", p.providers);
    assert_eq!(p.as_of, nika_catalog::pricing_snapshot().as_of);
    assert_eq!(p.sha, nika_catalog::pricing_snapshot().source_sha256_16);
}

// ── The adoption ladder (P0-21 · audit UX 2026-07-30) ──

use nika_providers::probe::{ExecutionLocus, ProviderReadiness};

/// One synthetic machine, rung ZERO: the registry's keyless engines
/// are CATALOG facts (always present, always "configured" — a keyless
/// seed needs nothing), never adoption. Detection needs an operator
/// signal: an override, bytes on disk, or an explicit `--ping`.
fn ladder_probe() -> Probe {
    let readiness = |configured, locus: ExecutionLocus| {
        ProviderReadiness::new(
            true,
            configured,
            None,
            None,
            false,
            locus,
            // Mirrors Profile::access_class on the synthetic machine.
            match locus {
                ExecutionLocus::Loopback | ExecutionLocus::Lan => {
                    nika_providers::probe::AccessClass::Local
                }
                _ => nika_providers::probe::AccessClass::Api,
            },
        )
    };
    Probe {
        version: "0.0.0-test".to_owned(),
        config_path: None,
        providers: vec![
            ProviderProbe::new(
                "ollama",
                false,
                false,
                "",
                true,
                readiness(true, ExecutionLocus::Loopback),
                "http://127.0.0.1:11434",
            ),
            ProviderProbe::new(
                "mistral",
                true,
                false,
                "MISTRAL_API_KEY",
                true,
                readiness(false, ExecutionLocus::Cloud),
                "https://api.mistral.ai/v1/chat/completions",
            ),
        ],
        clients: vec![],
        census: AccessCensus::default(),
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: vec![],
        pricing: PricingProbe::default(),
        retention: RetentionConfig::default(),
        retention_notes: vec![],
        models: ModelsProbe::default(),
        recorded_runs: 0,
        tracked_traces: None,
    }
}

/// One rung per fact — each transition is earned by exactly ONE new
/// observation, and no fact is ever claimed beyond its measurement.
#[test]
fn adoption_ladder_climbs_one_rung_per_fact() {
    let base = ladder_probe();
    // Installed — the catalog's keyless seed is NOT detection.
    assert_eq!(adoption_state(&base), AdoptionState::Installed);

    // KeyPresent — a cloud key is CONFIGURED, never « verified ».
    let mut keyed = base.clone();
    keyed.providers[1].key_present = true;
    keyed.providers[1].readiness.configured = true;
    assert_eq!(adoption_state(&keyed), AdoptionState::KeyPresent);

    // LocalDetected — an override moves the engine off its seed…
    let mut override_lan = base.clone();
    override_lan.providers[0].endpoint = "http://gpu.lan:11434".to_owned();
    override_lan.providers[0].readiness.execution_locus = ExecutionLocus::Lan;
    assert_eq!(adoption_state(&override_lan), AdoptionState::LocalDetected);
    // …or bytes on disk (a pulled GGUF)…
    let mut pulled = base.clone();
    pulled.models.count = 1;
    assert_eq!(adoption_state(&pulled), AdoptionState::LocalDetected);
    // …or a ping that found NOTHING (engaged, honestly unproven).
    let mut dead_ping = base.clone();
    dead_ping.local_pings = vec![(
        "ollama".to_owned(),
        "127.0.0.1:11434".to_owned(),
        PingState::Unreachable,
    )];
    assert_eq!(adoption_state(&dead_ping), AdoptionState::LocalDetected);

    // LocalReachable — ONLY an explicit --ping measurement proves it.
    let mut reachable = base.clone();
    reachable.local_pings = vec![(
        "ollama".to_owned(),
        "127.0.0.1:11434".to_owned(),
        PingState::Reachable(3),
    )];
    assert_eq!(adoption_state(&reachable), AdoptionState::LocalReachable);

    // RealReady — a live path AND runs on record (any lane earns
    // the path: a verified local endpoint, a configured cloud key,
    // or a ready seat · R4).
    let mut real_local = reachable.clone();
    real_local.recorded_runs = 2;
    assert_eq!(adoption_state(&real_local), AdoptionState::RealReady);
    let mut real_cloud = keyed.clone();
    real_cloud.recorded_runs = 1;
    assert_eq!(adoption_state(&real_cloud), AdoptionState::RealReady);
    // Runs ALONE prove nothing about today's path (the journal never
    // records the serving model — mock-vs-real is unknowable, so the
    // rung is not invented: no path today = the honest floor).
    let mut ran_once = base.clone();
    ran_once.recorded_runs = 3;
    assert_eq!(adoption_state(&ran_once), AdoptionState::Installed);
}

/// R4 — the seat rung the pre-census ladder could not see: a
/// signed-in harness seat (the census `seats_ready` lane) earns
/// `SeatReady` between `KeyPresent` and `RealReady`. Mutation pins:
/// emptying the lane drops the machine to the keyless floor, and a
/// seat + a key renders the SEAT (the sovereign order — the
/// operator's own plan outranks the metered key).
#[test]
fn the_seat_rung_is_earned_by_the_census_never_recomputed() {
    let base = ladder_probe();
    let seated = |p: &mut Probe| {
        p.census = AccessCensus::from_parts(
            &[],
            vec![nika_providers::census::SeatFact::new(
                "claude-code",
                vec!["anthropic".to_owned()],
                true,
                true,
                true,
            )],
        );
    };
    // SeatReady — the seat alone earns the rung.
    let mut seated_only = base.clone();
    seated(&mut seated_only);
    assert_eq!(adoption_state(&seated_only), AdoptionState::SeatReady);
    assert_eq!(
        seated_only.census.seat_escape().as_deref(),
        Some("or use a signed-in seat: `--access claude-code`")
    );
    // The seat OUTRANKS a configured cloud key.
    let mut both = seated_only.clone();
    both.providers[1].key_present = true;
    both.providers[1].readiness.configured = true;
    assert_eq!(adoption_state(&both), AdoptionState::SeatReady);
    // A ready seat + runs on record = the top rung (the seat is a
    // live path today).
    let mut real_seat = seated_only.clone();
    real_seat.recorded_runs = 1;
    assert_eq!(adoption_state(&real_seat), AdoptionState::RealReady);
    // The metric names the seat; the line owns its rung.
    let metric = AdoptionState::SeatReady.metric(&seated_only);
    assert!(
        metric.contains("claude-code") && metric.contains("signed in"),
        "{metric}"
    );
}

/// The enum is exhaustive over the ladder and every rung owns its
/// own label, metric and CTA — no two states render the same line.
#[test]
fn every_adoption_rung_renders_its_own_line() {
    let probe = ladder_probe();
    let mut lines = std::collections::BTreeSet::new();
    for state in [
        AdoptionState::Installed,
        AdoptionState::LocalDetected,
        AdoptionState::LocalReachable,
        AdoptionState::KeyPresent,
        AdoptionState::SeatReady,
        AdoptionState::RealReady,
    ] {
        assert!(!state.metric(&probe).is_empty(), "{state:?}");
        assert!(!state.cta().is_empty(), "{state:?}");
        assert!(lines.insert(state.as_str()), "labels are unique");
        assert!(lines.insert(state.cta()), "every rung owns its CTA");
    }
}

// ── Capability levels (P0-9 · audit UX 2026-07-30) + Hermes (H2) ──

/// One synthetic client row (wired or not).
fn client(id: &str, current: bool) -> ClientProbe {
    ClientProbe {
        id: id.to_owned(),
        path: format!("~/.{id}/config"),
        present: true,
        current,
        stale: false,
    }
}

fn versioned_kit(client: &str) -> KitProbe {
    KitProbe {
        client: client.to_owned(),
        version: "0.106.0".to_owned(),
    }
}

#[test]
fn capability_ladder_never_calls_mcp_only_guarded() {
    // The audit's lie: every wired host read as one flat « wired ».
    // MCP alone is the oracle rung — the guard is EARNED by hooks,
    // never implied by the wire.
    let hermes = client("hermes", true);
    assert_eq!(hermes.capability(&[]), CapabilityLevel::OracleOnly);
    // A kit for ANOTHER host does not raise this one.
    assert_eq!(
        hermes.capability(&[versioned_kit("cursor")]),
        CapabilityLevel::OracleOnly
    );
    // Unwired is the floor, kit or no kit.
    assert_eq!(
        client("cursor", false).capability(&[versioned_kit("cursor")]),
        CapabilityLevel::OracleOnly
    );
    // A kit without hooks (the codex kit ships none) = authoring,
    // never guard.
    let codex = client("codex", true);
    assert_eq!(
        codex.capability(&[versioned_kit("codex")]),
        CapabilityLevel::AuthoringEnabled
    );
    // A kit WITH hooks (claude · cursor ship them in the same drop
    // as the manifest) earns the guard.
    let cursor = client("cursor", true);
    assert_eq!(
        cursor.capability(&[versioned_kit("cursor")]),
        CapabilityLevel::Guarded
    );
    let claude = client("claude", true);
    assert_eq!(
        claude.capability(&[versioned_kit("claude")]),
        CapabilityLevel::Guarded
    );
    // Every rung owns a machine token.
    assert_eq!(CapabilityLevel::OracleOnly.as_str(), "oracle-only");
    assert_eq!(
        CapabilityLevel::AuthoringEnabled.as_str(),
        "authoring-enabled"
    );
    assert_eq!(CapabilityLevel::Guarded.as_str(), "guarded");
    assert_eq!(
        CapabilityLevel::FullyIntegrated.as_str(),
        "fully-integrated"
    );
}

// ── The per-host runtime receipt (H5 · audit UX 2026-07-30) ──

#[test]
fn receipt_separates_verified_canaries_from_the_assumed_level() {
    // H5 — hermes wired MCP-only: the receipt says oracle-only,
    // names the missing rails (kit · hooks), prints the repair,
    // and NOTHING is assumed — the wire was parsed, not inferred.
    let hermes = client("hermes", true);
    let r = hermes.capability_receipt(&[]);
    assert_eq!(r.host, "hermes");
    assert_eq!(r.version, None);
    assert_eq!(r.surface, "mcp");
    assert_eq!(r.root.as_deref(), Some(hermes.path.as_str()));
    assert_eq!(r.components, ["mcp"]);
    assert_eq!(r.capability, "oracle-only");
    assert_eq!(r.canaries, ["config-parsed"]);
    assert!(
        r.missing_rails.iter().any(|rail| rail == "hooks"),
        "{:?}",
        r.missing_rails
    );
    assert_eq!(r.repair.as_deref(), Some("nika wire hermes"));
    assert!(!r.level_assumed);

    // cursor wired + kit: the hooks component rides on the STATIC
    // `kit_ships_hooks` table — the receipt must NAME the assumption
    // (canary + level_assumed), never render it as observed.
    let cursor = client("cursor", true);
    let r = cursor.capability_receipt(&[versioned_kit("cursor")]);
    assert_eq!(r.host, "cursor");
    assert_eq!(r.version.as_deref(), Some("0.106.0"));
    assert_eq!(r.surface, "hooks");
    assert_eq!(r.components, ["mcp", "kit", "hooks"]);
    assert_eq!(r.capability, "guarded");
    assert!(
        r.canaries
            .iter()
            .any(|canary| canary == "hooks-assumed-from-kit"),
        "{:?}",
        r.canaries
    );
    assert!(r.missing_rails.is_empty(), "{:?}", r.missing_rails);
    assert_eq!(r.repair, None);
    assert!(r.level_assumed, "the hooks claim is a table lookup");
    // UX107-04: the guard evidence rung is `declared` — a table
    // lookup, never `loaded` or `proven` until a live canary exists.
    assert_eq!(r.guard_evidence.as_deref(), Some("declared"));

    // codex wired + kit without hooks: authoring is EARNED (the
    // manifest is found), the missing guard rail is named, and the
    // level still rests on the static table (hooks absence is a
    // lookup, not an observation).
    let codex = client("codex", true);
    let r = codex.capability_receipt(&[versioned_kit("codex")]);
    assert_eq!(r.components, ["mcp", "kit"]);
    assert_eq!(r.capability, "authoring-enabled");
    assert_eq!(r.missing_rails, ["hooks"]);
    assert!(r.level_assumed);
    assert_eq!(r.guard_evidence, None, "no hooks — no guard rung at all");

    // An unwired host is BELOW the floor: no component, every rail
    // missing, the repair is the wire — and the receipt never lends
    // it the « oracle-only » rung it did not earn.
    let r = client("cursor", false).capability_receipt(&[]);
    assert_eq!(r.components, Vec::<String>::new());
    assert_eq!(r.surface, "none");
    assert_eq!(r.capability, "unwired");
    assert_eq!(r.missing_rails, ["mcp", "kit", "hooks"]);
    assert_eq!(r.repair.as_deref(), Some("nika wire cursor"));
    assert!(!r.level_assumed);
}

#[test]
fn hermes_yaml_probe_recognizes_the_wire_contract() {
    // H2 — Hermes is wireable (`nika wire hermes`) and proven, but
    // its config is YAML: the JSON probe cannot see it. Recognition
    // mirrors patch_hermes: a `nika:` server whose command line
    // names the binary.
    let dir = temp_dir("hermes");
    let path = dir.join("config.yaml");
    std::fs::write(
        &path,
        "mcp_servers:\n  nika:\n    command: nika\n    args: [mcp]\n    timeout: 120\n",
    )
    .expect("fixture");
    let p = hermes_probe(&path);
    assert!(p.present && p.current && !p.stale, "{p:?}");
    assert_eq!(p.id, "hermes");
    // A foreign config is present but NOT wired — never a guess.
    std::fs::write(
        &path,
        "# my hermes\nmodel: hermes-4\nmcp_servers:\n  other: { command: x }\n",
    )
    .expect("fixture");
    let p = hermes_probe(&path);
    assert!(p.present && !p.current, "{p:?}");
    // Absent is silence.
    let p = hermes_probe(&dir.join("absent.yaml"));
    assert!(!p.present && !p.current, "{p:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn collect_offline_never_pings_and_never_binds_a_value() {
    // The default (welcome + doctor sans --ping) is fully offline:
    // zero ping observations, and the probe carries booleans/names
    // only — no field can hold a key VALUE by construction.
    let probe = collect(false);
    assert!(probe.local_pings.is_empty(), "offline = no ping rows");
    assert!(!probe.version.is_empty());
    assert!(
        probe.providers.iter().all(|p| !p.id.is_empty()),
        "provider rows are ids, not values"
    );
}

// ── The client registry (H6 · Q1 2026-07-31 — the binary consumes
// the vendored nika-plugins matrix) ──

#[test]
fn the_real_probe_derives_registry_coverage_from_the_vendored_matrix() {
    let probe = collect(false);
    let cov = &probe.clients_registry;
    assert!(
        cov.declared >= 27,
        "the matrix is 27+ clients, got {}",
        cov.declared
    );
    // Every wireable client is probed or honestly declared — and
    // the probed rows are a SUBSET of what the matrix wires (a
    // row the matrix drops leaves the probe list with it).
    assert_eq!(
        cov.probed + cov.declared_not_probed.len(),
        cov.wireable,
        "{cov:?}"
    );
    assert!(cov.probed >= 6, "the 6 known mechanisms, got {cov:?}");
    for client in &probe.clients {
        assert!(
            clients_registry::registry_wires(&client.id),
            "probed {} is not matrix-claimed",
            client.id
        );
        assert!(
            !cov.declared_not_probed.contains(&client.id),
            "{} cannot be probed AND declared-not-probed",
            client.id
        );
    }
    // The kit lanes ride the same derivation.
    for kit in &probe.kits {
        assert!(
            clients_registry::registry_ships_kit(&kit.client),
            "kit {} is not a matrix class-A wire row",
            kit.client
        );
    }
}

#[test]
fn environment_json_carries_the_registry_lane() {
    let probe = collect(false);
    let env = environment_json(&probe);
    let lane = &env["clients_registry"];
    assert_eq!(
        lane["declared"].as_u64(),
        Some(probe.clients_registry.declared as u64)
    );
    assert_eq!(
        lane["probed"].as_u64(),
        Some(probe.clients_registry.probed as u64)
    );
    assert!(
        lane["declared_not_probed"].is_array(),
        "the not-probed are named, machine-readable: {lane}"
    );
}
