use super::*;
use crate::clients_registry::RegistryCoverage;
use crate::output::exit;
use crate::probe::{ClientProbe, ImageProbe, PingState, PricingProbe, ProviderProbe, TtsProbe};
use nika_providers::probe::{ExecutionLocus, ProviderReadiness};

/// The default synthetic readiness — recognized · loopback · the
/// opt-in rungs unmeasured (mirrors `collect_provider_probes`).
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

fn synthetic_probe() -> Probe {
    Probe {
        models: crate::probe::ModelsProbe::default(),
        version: "0.0.0-test".to_owned(),
        config_path: None,
        providers: vec![
            ProviderProbe::new(
                "ollama",
                false,
                false,
                "NIKA_OLLAMA_API_KEY",
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
            ProviderProbe::new(
                "anthropic",
                true,
                true,
                "ANTHROPIC_API_KEY",
                true,
                readiness(true, ExecutionLocus::Cloud),
                "https://api.anthropic.com/v1/messages",
            ),
        ],
        clients: vec![
            ClientProbe {
                id: "cursor".to_owned(),
                path: "~/.cursor/mcp.json".to_owned(),
                present: true,
                current: true,
                stale: false,
            },
            ClientProbe {
                id: "vscode".to_owned(),
                path: "./.vscode/mcp.json".to_owned(),
                present: false,
                current: false,
                stale: false,
            },
        ],
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
    }
}

fn counts() -> EngineCounts {
    EngineCounts {
        builtins: 7,
        locals: 1,
        clouds: 2,
        examples: 3,
        templates: 2,
    }
}

fn plain() -> Theme {
    Theme::new(false, false, false)
}

/// The concierge hands over ONE key per state — the four states each
/// lead with their own move. The 1-workflow-founded state is keyed on
/// the file's VERDICT too (P0-3): the clean gate below is what keeps
/// `run` eligible at all.
#[test]
fn start_moves_key_on_the_workspace_state() {
    let g = |workflows, agents_md| Glance {
        git: true,
        workflows,
        agents_md,
        complete: true,
    };
    let clean = RunGate {
        path: "a.nika.yaml".to_owned(),
        proposable: true,
        priced: false,
    };
    assert!(start_moves(g(0, false), None)[0].0.contains("try 01-hello"));
    assert_eq!(start_moves(g(2, false), None)[0].0, "nika init");
    // FLIP (P0-3 · audit UX 2026-07-30): this line pinned « 1 workflow
    // + AGENTS.md → nika run head » with NO verdict — the finding's
    // exact reproduction. `run` now leads ONLY behind a clean gate;
    // the red and priced arms are pinned by the scratch-dir tests
    // (one_red_workflow_gets_check_never_run · the LOI-3 twins).
    assert_eq!(start_moves(g(1, true), Some(&clean))[0].0, "nika run");
    assert_eq!(start_moves(g(5, true), None)[0].0, "nika welcome --deep");
}

/// A scratch workspace on disk (auto-cleaned) — the run-CTA tests
/// audit REAL files, never synthetic flags.
fn scratch(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("scratch dir");
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).expect("write");
    }
    dir
}

/// The context.rs red fixture: `when:` as a bare string is a
/// conformance finding — the file parses, the ladder refuses it.
const RED_WORKFLOW: &str = "nika: bad\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    after:\n      a: success\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n";

/// P0-3 (audit UX 2026-07-30) — one RED workflow, agents briefed:
/// the concierge must NEVER carry a `nika run` CTA (head or dim)
/// for a file the ladder has not seen clean; the head is
/// `nika check <the exact file>`. Human and JSON both eat from
/// `start_moves`, and the JSON projection is asserted too.
#[test]
fn one_red_workflow_gets_check_never_run() {
    let dir = scratch(&[("AGENTS.md", "x"), ("bad.nika.yaml", RED_WORKFLOW)]);
    let (g, sole) = glance(dir.path(), 4000);
    assert_eq!(g.workflows, 1, "the scratch holds exactly one file");
    assert!(g.agents_md);
    let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
    assert!(
        !gate.as_ref().expect("a gate for the sole file").proposable,
        "the `when: maybe` fixture is RED for real"
    );
    let moves = start_moves(g, gate.as_ref());
    assert_eq!(
        moves[0].0, "nika check bad.nika.yaml",
        "a red file is audited, never run: {moves:?}"
    );
    for (cmd, _) in &moves {
        assert!(
            !cmd.starts_with("nika run"),
            "no run CTA while the exact file is red: {moves:?}"
        );
    }
    let raw = render_json(
        &synthetic_probe(),
        g,
        gate.as_ref(),
        counts(),
        serde_json::Value::Null,
    );
    let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(v["start"][0], "nika check bad.nika.yaml", "{raw}");
    assert!(
        !raw.contains("nika run"),
        "the JSON mirror carries no run CTA on a red file: {raw}"
    );
}

/// The twin that must NOT regress: one CLEAN unpriced workflow
/// (mock/echo) keeps the bare `nika run` head — and the wording
/// never calls an unpriced model « free » (LOI-3: unknown stays
/// unknown, it is merely uncapped by law's absence of a price).
#[test]
fn one_clean_mock_workflow_keeps_the_run_cta_uncapped() {
    let dir = scratch(&[
        ("AGENTS.md", "x"),
        (
            "good.nika.yaml",
            "nika: good\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        ),
    ]);
    let (g, sole) = glance(dir.path(), 4000);
    assert_eq!(g.workflows, 1);
    let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
    let gate = gate.expect("a gate for the sole file");
    assert!(
        gate.proposable && !gate.priced,
        "mock/echo: clean, unpriced"
    );
    let moves = start_moves(g, Some(&gate));
    assert_eq!(
        moves[0].0, "nika run",
        "clean + unpriced → run leads: {moves:?}"
    );
    for (cmd, _) in &moves {
        assert!(
            !cmd.contains("--max-cost-usd"),
            "an unpriced model carries no cap placeholder: {moves:?}"
        );
    }
    let text = render_human(&synthetic_probe(), g, Some(&gate), counts(), plain());
    assert!(
        !text.contains("free"),
        "unpriced is never « free »:\n{text}"
    );
}

/// LOI-3 — one CLEAN workflow on a PRICED model (openai/*): the run
/// line the concierge prints always carries the spend cap, with an
/// explicit placeholder the operator fills.
#[test]
fn one_clean_priced_workflow_carries_the_loi3_cap() {
    let dir = scratch(&[
        ("AGENTS.md", "x"),
        (
            "priced.nika.yaml",
            "nika: priced\nmodel: openai/gpt-4o-mini\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        ),
    ]);
    let (g, sole) = glance(dir.path(), 4000);
    assert_eq!(g.workflows, 1);
    let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
    let gate = gate.expect("a gate for the sole file");
    assert!(gate.proposable && gate.priced, "openai/*: clean, priced");
    let moves = start_moves(g, Some(&gate));
    assert!(
        moves[0].0.starts_with("nika run"),
        "a clean priced file still runs head-first: {moves:?}"
    );
    assert!(
        moves[0].0.contains("--max-cost-usd"),
        "LOI-3: a priced run CTA carries the cap: {moves:?}"
    );
}

/// The mirror speaks the sovereign lane ONLY when bytes are on disk
/// (a pulled model must be visible — the machine section IS "what
/// this machine already has"; zero models = zero line, never a
/// lecture).
#[test]
fn mirror_shows_pulled_models_and_stays_silent_at_zero() {
    let glance = Glance {
        git: true,
        workflows: 0,
        agents_md: false,
        complete: true,
    };
    let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
    assert!(
        !silent.contains("  models"),
        "zero models = zero line:\n{silent}"
    );

    let mut probe = synthetic_probe();
    probe.models.count = 2;
    probe.models.bytes = 211 * 1024 * 1024;
    let shown = render_human(&probe, glance, None, counts(), plain());
    assert!(
        shown.contains("models     2 pulled · 211.0 MiB on disk"),
        "the sovereign lane is in the mirror:\n{shown}"
    );
    assert!(shown.contains("nika model list"), "{shown}");
}

/// The endpoint lane obeys the same law as the kit lane: loopback
/// everywhere is silence (the byte-identical default) — only an
/// override-pointed engine (`NIKA_OLLAMA_BASE_URL` · `OLLAMA_HOST`) earns
/// a line, NAMED with its endpoint and locus (P0-20: « local » is a
/// protocol, never a topology).
#[test]
fn mirror_names_an_endpoint_override_and_stays_silent_on_loopback() {
    let glance = Glance {
        git: true,
        workflows: 0,
        agents_md: false,
        complete: true,
    };
    let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
    assert!(
        !silent.contains("  endpoint"),
        "loopback = no endpoint line:\n{silent}"
    );

    let mut probe = synthetic_probe();
    probe.providers[0].endpoint = "http://gpu.lan:11434".to_owned();
    probe.providers[0].readiness.execution_locus = ExecutionLocus::Lan;
    let shown = render_human(&probe, glance, None, counts(), plain());
    assert!(
        shown.contains("ollama → http://gpu.lan:11434 (lan)"),
        "the override is NAMED next to the local row:\n{shown}"
    );
    assert!(
        shown.contains("· no key needed"),
        "the keyless truth stays — only the topology claim is fixed:\n{shown}"
    );
}

/// The kit lane obeys the same law: an aligned (or absent) plugin
/// kit is silence — only TRAIN drift earns a line, and the line
/// routes to doctor (the per-client fix lives there, not here).
#[test]
fn mirror_names_kit_drift_and_stays_silent_when_aligned() {
    let glance = Glance {
        git: true,
        workflows: 0,
        agents_md: false,
        complete: true,
    };
    let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
    assert!(!silent.contains("  kits"), "no kits = no line:\n{silent}");

    let mut probe = synthetic_probe();
    probe.kits = vec![
        crate::probe::KitProbe {
            client: "codex".to_owned(),
            version: "0.0.7".to_owned(), // same 0.0 train as the binary
        },
        crate::probe::KitProbe {
            client: "claude".to_owned(),
            version: "0.105.0".to_owned(), // another train — drift
        },
    ];
    let shown = render_human(&probe, glance, None, counts(), plain());
    // The verdict moved to the hanging line when the drifted list grew
    // past the right edge (three kits + the version + the handle
    // measured 100 columns), so the two halves are asserted apart.
    assert!(
        shown.contains("kits       claude 0.105.0"),
        "only the DRIFTED kit is named, the aligned one is silent:\n{shown}"
    );
    assert!(
        shown.contains("vs binary 0.0.0-test"),
        "the drift is measured against THIS binary:\n{shown}"
    );
    assert!(!shown.contains("codex"), "aligned kit stays out:\n{shown}");
    assert!(shown.contains("fixes → nika doctor"), "{shown}");
}

/// P0-21 (audit UX 2026-07-30) — the mirror greets with the adoption
/// rung: each state carries its OWN metric and its OWN CTA, and the
/// line never claims more than the probe measured.
#[test]
fn the_mirror_greets_each_adoption_rung_with_its_own_cta() {
    let glance = Glance {
        git: true,
        workflows: 2,
        agents_md: true,
        complete: true,
    };
    // KeyPresent — the stock synthetic machine (anthropic keyed).
    let keyed = render_human(&synthetic_probe(), glance, None, counts(), plain());
    assert!(
        keyed.contains("state      key present · 1 of 2 clouds configured — ready for a real run"),
        "KeyPresent renders its own line:\n{keyed}"
    );

    // Installed — strip every engagement fact: no key, no ping, no
    // pulled model, no journal. The catalog's keyless seeds do NOT
    // count as detection.
    let mut bare = synthetic_probe();
    bare.providers[2].key_present = false;
    bare.providers[2].readiness.configured = false;
    let installed = render_human(&bare, glance, None, counts(), plain());
    assert!(
        installed.contains("state      installed · no inference path — proof → nika try 01-hello"),
        "Installed routes to the offline proof:\n{installed}"
    );

    // LocalDetected — an override moves ollama off its loopback seed.
    let mut lan = bare.clone();
    lan.providers[0].endpoint = "http://gpu.lan:11434".to_owned();
    lan.providers[0].readiness.execution_locus = ExecutionLocus::Lan;
    let detected = render_human(&lan, glance, None, counts(), plain());
    assert!(
        detected
            .contains("state      ollama detected · unproven — start it, then nika doctor --ping"),
        "LocalDetected names the engine and the ping hand-off:\n{detected}"
    );

    // LocalReachable — ONLY a --ping measurement earns « reachable ».
    let mut pinged = bare.clone();
    pinged.local_pings = vec![(
        "ollama".to_owned(),
        "127.0.0.1:11434".to_owned(),
        PingState::Reachable(3),
    )];
    let reachable = render_human(&pinged, glance, None, counts(), plain());
    assert!(
        reachable.contains("state      local reachable · ollama (3ms) — point a run at it"),
        "LocalReachable carries the measured round-trip:\n{reachable}"
    );

    // RealReady — a live path AND runs on record. The wording says
    // « path configured »: the journal proves runs, never the model.
    let mut real = synthetic_probe();
    real.recorded_runs = 2;
    let ready = render_human(&real, glance, None, counts(), plain());
    assert!(
        ready.contains("state      real-ready · 2 runs on record · path configured — nika run"),
        "RealReady claims the record, never « a real model answered »:\n{ready}"
    );
    // Every rung's line is its own — the five renders differ.
    let lines: std::collections::BTreeSet<String> = [keyed, installed, detected, reachable, ready]
        .iter()
        .map(|t| {
            t.lines()
                .find(|l| l.contains("  state"))
                .expect("a state line")
                .to_owned()
        })
        .collect();
    assert_eq!(lines.len(), 5, "five rungs, five distinct lines");
}

#[test]
fn human_mirror_carries_the_four_sections_and_no_key_names() {
    let text = render_human(
        &synthetic_probe(),
        Glance {
            git: true,
            workflows: 2,
            agents_md: false,
            complete: true,
        },
        None,
        counts(),
        plain(),
    );
    for needle in [
        "Intent as Code",
        "this machine",
        "this binary",
        "start here",
        "hash-chained",
        "cursor ✓",
        "vscode ✗",
        "nika wire",
        // P0-21: the raw key ratio is REPLACED by the adoption rung —
        // this synthetic machine sits at KeyPresent (anthropic keyed,
        // 1 of 2 clouds), with that rung's own metric and CTA.
        "state      key present · 1 of 2 clouds configured",
        "ready for a real run",
        "not briefed → nika init",
        // 2 workflows + unbriefed → the ONE key is the founding wizard
        // (adds only); the stranger's mock/echo line belongs to the
        // 0-workflow state (pinned in start_moves' own test below).
        "brief agents · adds only",
        "learn: nika.sh",
        "github.com/supernovae-st/nika",
    ] {
        assert!(text.contains(needle), "missing `{needle}`:\n{text}");
    }
    // PRESENT-NOT-PRINTED, one step further: welcome never even names
    // the env VARS — that is doctor's fix surface, not the mirror's.
    assert!(
        !text.contains("API_KEY"),
        "welcome must not name key variables:\n{text}"
    );
    // P0-21: the raw ratio row is gone — the state line carries the
    // counts inside its own metric now.
    assert!(
        !text.contains("cloud keys present"),
        "the raw key ratio is replaced by the adoption rung:\n{text}"
    );
    // A workspace that already HAS workflows skips the language taste
    // (progressive disclosure — the sample is the stranger's moment).
    assert!(
        !text.contains("a whole workflow is one file"),
        "2 workflows → no sample:\n{text}"
    );
}

/// A probe shaped like the REAL shipped catalog (5 locals · 10 clouds ·
/// every client the registry names) — the 80-column gate must hold on
/// the widest TRUE rows, not on a slim synthetic (the first cut passed
/// on a 1-local probe while the real machine rendered 102 columns; the
/// second passed on four hand-written clients while the real machine
/// rendered 112). Counts that can drift are derived here, not typed.
fn shipped_shape_probe() -> Probe {
    let local = |id: &str| {
        ProviderProbe::new(
            id,
            false,
            false,
            "",
            true,
            readiness(true, ExecutionLocus::Loopback),
            "http://127.0.0.1:1",
        )
    };
    let cloud = |id: &str| {
        ProviderProbe::new(
            id,
            true,
            false,
            "",
            true,
            readiness(false, ExecutionLocus::Cloud),
            format!("https://api.{id}.example/v1"),
        )
    };
    let client = |id: &str| ClientProbe {
        id: id.to_owned(),
        path: String::new(),
        present: false,
        current: false,
        stale: false,
    };
    Probe {
        models: crate::probe::ModelsProbe::default(),
        version: "0.98.0".to_owned(),
        config_path: None,
        providers: ["ollama", "lmstudio", "llamacpp", "localai", "vllm"]
            .into_iter()
            .map(local)
            .chain(
                [
                    "mistral",
                    "anthropic",
                    "openai",
                    "gemini",
                    "deepseek",
                    "xai",
                    "groq",
                    "openrouter",
                    "huggingface",
                    "nvidia",
                ]
                .into_iter()
                .map(cloud),
            )
            .collect(),
        // DERIVED from the registry, never listed by hand. The hand-
        // written four here were two short of what the binary probes,
        // so the 80-column ratchet was measuring a machine that does
        // not exist — and missed a row rendering 112 columns under a
        // published release. The roster grows; the fixture follows it
        // now, and this drift class cannot come back a third time.
        clients: crate::clients_registry::PROBE_MECHANISMS
            .iter()
            .copied()
            .map(client)
            .collect(),
        kits: vec![],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
    }
}

#[test]
fn the_stranger_sees_the_language_and_it_fits_eighty_columns() {
    // Zero workflows = the first-contact moment: the mirror SHOWS a
    // whole workflow (the abstract tagline made concrete) — and every
    // line of the whole render stays ≤80 display columns (the one
    // terminal width nobody configures), measured on the REAL
    // catalog shape.
    let text = render_human(
        &shipped_shape_probe(),
        Glance {
            git: false,
            workflows: 0,
            agents_md: false,
            complete: true,
        },
        None,
        EngineCounts {
            builtins: 25,
            locals: 5,
            clouds: 10,
            examples: 28,
            templates: 9,
        },
        plain(),
    );
    assert!(
        text.contains("a whole workflow is one file"),
        "0 workflows → the sample shows:\n{text}"
    );
    // the sample teaches the LIVE envelope — a welcome screen showing
    // `nika: v1` would hand a stranger a file its own binary refuses
    assert!(text.contains("nika: hello"), "{text}");
    assert!(text.contains("infer:"), "{text}");
    for line in text.lines() {
        assert!(
            line.chars().count() <= 80,
            "line exceeds 80 cols ({}): `{line}`",
            line.chars().count()
        );
    }
}

#[test]
fn ascii_theme_swaps_every_glyph() {
    // CI logs and legacy terminals get a first-class column: no 🦋,
    // no ✓/✗ — the [nika] mark and +/x, same meaning (colour law:
    // meaning never lives in glyph loss either).
    let text = render_human(
        &synthetic_probe(),
        Glance {
            git: true,
            workflows: 1,
            agents_md: true,
            complete: true,
        },
        None,
        counts(),
        Theme::new(false, true, false),
    );
    assert!(text.contains("[nika]"), "{text}");
    assert!(text.contains("cursor +"), "{text}");
    assert!(text.contains("vscode x"), "{text}");
    for glyph in ['🦋', '✓', '✗'] {
        assert!(
            !text.contains(glyph),
            "unicode {glyph} leaked into --ascii:\n{text}"
        );
    }
}

/// The experience block folds the concierge's own proven facts into
/// the router and ships state+action beside the mirror — the FIRST
/// consumer of `route()`. The three load-bearing arcs: a clean sole
/// workflow opens (never runs), kit drift outranks everything, and a
/// chat-only session discovers without a scan.
#[test]
fn the_experience_block_routes_from_the_concierge_facts() {
    // The same pure seam run_in rides: a named candidate resolves to a
    // workspace envelope, `None` resolves chat-only.
    let facts = EnvFacts {
        evidence: EvidenceSource::ExplicitCwd,
        ..EnvFacts::default()
    };
    let dir = std::env::temp_dir();
    let workspace = context_envelope::resolve(Some(dir.as_path()), &facts);
    assert_eq!(workspace.mode, ContextMode::Workspace);
    let chat = context_envelope::resolve(None, &EnvFacts::default());
    assert_eq!(chat.mode, ContextMode::ChatOnly);
    let clean_gate = RunGate {
        path: "wf.nika.yaml".to_owned(),
        proposable: true,
        priced: false,
    };
    let one_clean = Glance {
        git: true,
        workflows: 1,
        agents_md: true,
        complete: true,
    };
    let block = experience_block(&synthetic_probe(), &workspace, one_clean, Some(&clean_gate));
    assert_eq!(block["state"]["schema_version"], 1);
    assert_eq!(block["state"]["workflow"], "clean");
    assert_eq!(block["action"]["action_id"], "open_workflow");
    assert_eq!(block["action"]["nothing_has_run"], true);

    // Kit drift degrades every richer move (UX107-02) — but a PATCH is
    // not a train (probe::train_differs, the same law the human line
    // obeys): a 0.107.0 kit under a 0.107.x binary stays coherent.
    let mut drifted = synthetic_probe();
    drifted.version = "0.107.0".to_owned();
    drifted.kits.push(crate::probe::KitProbe {
        client: "cursor".to_owned(),
        version: "0.106.0".to_owned(),
    });
    let block = experience_block(&drifted, &workspace, one_clean, Some(&clean_gate));
    assert_eq!(block["state"]["versions_coherent"], false);
    assert_eq!(block["action"]["action_id"], "align_versions");

    let mut patch = synthetic_probe();
    patch.version = "0.107.1".to_owned();
    patch.kits.push(crate::probe::KitProbe {
        client: "cursor".to_owned(),
        version: "0.107.0".to_owned(),
    });
    let block = experience_block(&patch, &workspace, one_clean, Some(&clean_gate));
    assert_eq!(
        block["state"]["versions_coherent"], true,
        "a patch bump is not a train drift — the router must not degrade on it"
    );
    assert_eq!(block["action"]["action_id"], "open_workflow");

    // A TRUNCATED walk is UNKNOWN at every count — the one file it saw
    // was never audited (no gate exists behind an incomplete walk), so
    // the JSON contract must not claim `clean` while the text render
    // says « scan partial » (refuter pass, pre-ship).
    let truncated = Glance {
        complete: false,
        ..one_clean
    };
    let block = experience_block(&synthetic_probe(), &workspace, truncated, None);
    assert_eq!(block["state"]["workflow"], "unknown");
    assert_eq!(block["action"]["action_id"], "deep_scan");

    // Chat-only: no root claim, discovery, nothing scanned.
    let block = experience_block(&synthetic_probe(), &chat, CHAT_ONLY_GLANCE, None);
    assert_eq!(block["state"]["context_mode"], "chat_only");
    assert_eq!(block["state"]["root"], serde_json::Value::Null);
    assert_eq!(block["action"]["action_id"], "discover_example");
}

#[test]
fn json_mirror_is_versioned_additive_and_value_free() {
    let raw = render_json(
        &synthetic_probe(),
        Glance {
            git: false,
            workflows: 0,
            agents_md: true,
            complete: true,
        },
        None,
        counts(),
        serde_json::Value::Null,
    );
    let v: serde_json::Value = serde_json::from_str(&raw).expect("welcome --json parses");
    assert_eq!(v["welcome_version"], 1);
    assert_eq!(v["machine"]["cloud_keys_present"], 1);
    assert_eq!(v["machine"]["cloud_keys_total"], 2);
    assert_eq!(v["machine"]["clients"][0]["wired"], true);
    assert_eq!(v["workspace"]["workflows"], 0);
    assert_eq!(v["workspace"]["inventory_complete"], true);
    assert_eq!(v["engine"]["verbs"], 4);
    assert_eq!(v["start"].as_array().map(Vec::len), Some(3));
    assert!(
        !raw.contains("API_KEY") && !raw.contains("key_present"),
        "the JSON mirror carries counts, never per-key facts: {raw}"
    );
}

/// P0-4 (audit UX 2026-07-30): a TRUNCATED inventory that found zero
/// files is an UNKNOWN, never « no workflows yet » — the stranger's
/// claims (the zero line · the language sample · the JSON mirror) are
/// all gated on a COMPLETE scan, and a partial scan says so instead.
#[test]
fn a_partial_scan_never_renders_the_strangers_zero() {
    let g = Glance {
        git: true,
        workflows: 0,
        agents_md: false,
        complete: false,
    };
    let text = render_human(&synthetic_probe(), g, None, counts(), plain());
    assert!(
        !text.contains("no workflows yet"),
        "a partial scan is unknown, never « zero »:\n{text}"
    );
    assert!(
        text.contains("scan partial"),
        "the partial scan says so:\n{text}"
    );
    assert!(
        !text.contains("a whole workflow is one file"),
        "the sample is the COMPLETE stranger's moment only:\n{text}"
    );
    let raw = render_json(
        &synthetic_probe(),
        g,
        None,
        counts(),
        serde_json::Value::Null,
    );
    let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(
        v["workspace"]["inventory_complete"], false,
        "the machine mirror carries the scan's completeness: {raw}"
    );
}

/// The glance itself propagates the walk's truncation: an injected
/// tiny budget that dies before the workflow's directory yields
/// `workflows: 0` WITH `complete: false` — the finding's exact
/// reproduction, now indistinguishable-proof.
#[test]
fn glance_marks_a_budget_killed_scan_incomplete() {
    let dir = tempfile::tempdir().expect("scratch");
    for i in 0..8 {
        std::fs::write(dir.path().join(format!("noise-{i}.txt")), "x").expect("write");
    }
    std::fs::create_dir(dir.path().join("z")).expect("mkdir");
    std::fs::write(dir.path().join("z/flow.nika.yaml"), "x").expect("write");
    let (g, sole) = glance(dir.path(), 3); // dies in the noise
    assert_eq!(g.workflows, 0, "the workflow was never reached");
    assert!(!g.complete, "a killed scan is partial, never « zero »");
    assert!(sole.is_none());
    let (g, _) = glance(dir.path(), 4000);
    assert_eq!(g.workflows, 1);
    assert!(g.complete, "a full scan is complete");
}

#[test]
fn glance_counts_workflows_skips_heavy_dirs_and_sees_git() {
    let tmp = std::env::temp_dir().join(format!("nika-welcome-glance-{}", std::process::id()));
    let nested = tmp.join("flows");
    let heavy = tmp.join("node_modules");
    std::fs::create_dir_all(&nested).expect("mkdir");
    std::fs::create_dir_all(&heavy).expect("mkdir");
    std::fs::create_dir_all(tmp.join(".git")).expect("mkdir");
    std::fs::write(tmp.join("a.nika.yaml"), "x").expect("write");
    std::fs::write(nested.join("b.nika.yml"), "x").expect("write");
    std::fs::write(heavy.join("c.nika.yaml"), "x").expect("write");
    std::fs::write(tmp.join("AGENTS.md"), "x").expect("write");
    let (g, sole) = glance(&tmp, 4000);
    std::fs::remove_dir_all(&tmp).ok();
    assert!(g.git, "sees the .git ancestor");
    assert_eq!(g.workflows, 2, "counts a.nika.yaml + flows/b.nika.yml only");
    assert!(g.agents_md);
    assert!(sole.is_none(), "two files → no sole audit target");
}

/// P0-14 binary-side (W2): candidate `None` → chat-only. The mirror
/// SAYS it and makes zero workspace claim — no workspace row, no
/// inventory count, no agents line — and routes to the two doors the
/// spec allows (an isolated example · choosing a project).
#[test]
fn chat_only_says_so_and_claims_no_workspace() {
    let out = run_in(false, plain(), None);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(
        out.text
            .contains("chat only — no reliable project detected"),
        "chat-only says so:\n{}",
        out.text
    );
    assert!(
        !out.text.contains("  workspace"),
        "no workspace row in chat-only:\n{}",
        out.text
    );
    assert!(
        !out.text.contains("workflows") && !out.text.contains("agents"),
        "no workspace inventory claim in chat-only:\n{}",
        out.text
    );
    assert!(
        out.text.contains("isolated") && out.text.contains("choose a project"),
        "the two chat-only doors are named:\n{}",
        out.text
    );
    let json = run_in(true, plain(), None);
    assert_eq!(json.code, exit::OK, "{}", json.text);
    assert!(
        json.text.contains("\"chat_only\""),
        "the machine mirror carries the mode: {}",
        json.text
    );
    assert!(
        !json.text.contains("\"workspace\""),
        "no workspace projection in chat-only: {}",
        json.text
    );
}

/// P0-14 binary-side (W2): a candidate sitting in a SUBDIR resolves
/// UP to the git root — the workspace row names the resolved root AND
/// traces the expansion (`expanded_from`), never a silent rewrite.
#[test]
fn workspace_names_the_root_and_traces_the_subdir_expansion() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::create_dir(dir.path().join(".git")).expect("mkdir .git");
    let deep = dir.path().join("crates").join("nika-cli");
    std::fs::create_dir_all(&deep).expect("mkdir deep");
    let out = run_in(false, plain(), Some(&deep));
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let root = dir.path().canonicalize().expect("canon root");
    let deep_canon = deep.canonicalize().expect("canon deep");
    let row = out
        .text
        .lines()
        .skip_while(|l| !l.contains("  workspace"))
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    // The facts drop under the label when the path cannot share its
    // line (a normal checkout rendered 150 columns), so the block is
    // the unit here — both claims must be present and adjacent.
    assert!(
        row.contains(&root.display().to_string()),
        "the row names the resolved root: {row}"
    );
    assert!(
        row.contains(&format!("from {}", deep_canon.display())),
        "the subdir→root expansion is traced: {row}"
    );
    // The rule, stated honestly: a row may only pass the right edge
    // because of ONE indivisible token — a path nobody may truncate.
    // Drop the longest token and what remains must fit.
    for line in row.lines() {
        let widest = line.split_whitespace().map(str::len).max().unwrap_or(0);
        assert!(
            line.chars().count().saturating_sub(widest) <= 80,
            "only one unbreakable token may pass the right edge ({}): `{line}`",
            line.chars().count()
        );
    }
}

#[test]
fn welcome_is_always_a_success() {
    // A greeting is never a failure — even on a bare machine the verb
    // routes (doctor owns the gate semantics, welcome never gates).
    let out = run(false, plain());
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(out.text.contains("start here"), "{}", out.text);
    let json = run(true, plain());
    assert_eq!(json.code, exit::OK);
    assert!(json.text.contains("\"welcome_version\":1"), "{}", json.text);
}

#[test]
fn the_editor_roster_wraps_instead_of_running_off_the_terminal() {
    // The roster grows. A published 0.107.0 rendered this row at 112
    // columns on a normal machine — the cells alone, before any wire
    // handle — because the row assumed six ids would fit where four
    // once did. Growth must cost a line, never the right edge.
    let text = render_human(
        &shipped_shape_probe(),
        Glance {
            git: true,
            workflows: 0,
            agents_md: false,
            complete: true,
        },
        None,
        EngineCounts {
            builtins: 25,
            locals: 5,
            clouds: 10,
            examples: 28,
            templates: 9,
        },
        plain(),
    );
    let shown: Vec<&str> = text
        .lines()
        .filter(|l| l.contains(" ✓") || l.contains(" ✗"))
        .collect();
    for id in crate::clients_registry::PROBE_MECHANISMS {
        assert!(
            shown.iter().any(|l| l.contains(id)),
            "every registry host stays visible after wrapping — {id} vanished:\n{text}"
        );
    }
    // The handle names ONE host and speaks the count of the rest.
    assert!(
        text.contains("unwired · one command each → nika wire"),
        "several gaps must say how many:\n{text}"
    );
}
