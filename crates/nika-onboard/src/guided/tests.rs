use super::*;

const PLAIN: Theme = Theme::new(false, false, false);

/// The test audit stub — the ladder's SHAPE without the ladder
/// (integration against the real check lives at the composition
/// root and in nika-onboard's own dev-dep ratchets).
fn stub_audit(path: &str) -> Outcome {
    Outcome::ok(format!("✔ audited (stub) ← {path}"))
}

#[test]
fn shell_quote_wraps_only_when_needed() {
    // A kebab path stays bare (the common case · no visual noise);
    // a spaced path is single-quoted so `nika run <it>` survives the
    // copy-paste (a wizard hand-off must not emit a broken command).
    assert_eq!(shell_quote("my-flow.nika.yaml"), "my-flow.nika.yaml");
    assert_eq!(shell_quote("a/b/c.nika.yaml"), "a/b/c.nika.yaml");
    assert_eq!(
        shell_quote("My Cool Flow.nika.yaml"),
        "'My Cool Flow.nika.yaml'"
    );
    // The `'` escape is the total POSIX form (close · escaped · open).
    assert_eq!(shell_quote("it's.nika.yaml"), r"'it'\''s.nika.yaml'");
    // Shell metacharacters (a `;`/`$`/`&` in a pasted name) are quoted.
    assert_eq!(shell_quote("a;rm -rf.yaml"), "'a;rm -rf.yaml'");
}

/// A unique EMPTY dir per test — the collision-aware default reads
/// the base, so shared temp dirs would leak state between tests.
fn fresh_base(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-wiz-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn dest(tag: &str) -> String {
    std::env::temp_dir()
        .join(format!("nika-new-{}-{tag}.nika.yaml", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn exact_template_name_stays_the_fast_path() {
    let d = dest("exact");
    let out = run("chain", Some(&d), true);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(!out.text.contains("routed"), "{}", out.text);
    std::fs::remove_file(&d).ok();
}

/// The ladder's second rung: an example slug (or filename) lands
/// VERBATIM at dest (minus the one self-referential pack path, which
/// re-points to the owned file) — and the default dest flattens any
/// tiering to the basename.
#[test]
fn example_sources_land_verbatim_through_new() {
    let dir = std::env::temp_dir().join(format!("nika-new-example-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let dest = dir.join("mine.nika.yaml");
    let dest_s = dest.to_string_lossy().into_owned();

    let out = run("01-hello", Some(&dest_s), false);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(out.text.contains("example `01-hello`"), "{}", out.text);
    assert!(out.text.contains("nika check"), "{}", out.text);
    // Verbatim MINUS the pack's self-referential path: the `# Run ·`
    // comment inside the OWNED copy must name the owned file — pasting
    // the pack path exited 3 in the user's workspace (gauntlet 08-01).
    let landed = std::fs::read_to_string(&dest).expect("written");
    assert_eq!(
        landed,
        nika_pack::example("01-hello")
            .expect("embedded")
            .replace("examples/01-hello.nika.yaml", &dest_s),
        "verbatim, self-reference re-pointed to the owned dest"
    );
    assert!(
        !landed.contains("examples/01-hello.nika.yaml"),
        "no taught command may name the pack-only path: {landed}"
    );
    // Filename form resolves too; overwrite refuses. FLIP
    // (2026-07-31): this used to probe `showcase/t1-price-watch` —
    // the pack dropped the invented showcase tier (re-vendored flat,
    // "indexed by what it teaches") and the assert only stayed green
    // because the OLD router silently mis-routed the dead slug to a
    // template (exit 0 — the P0-1 bug class). It now probes a slug
    // that EXISTS and asserts the verbatim landing, like 01-hello.
    assert_eq!(
        run("01-hello.nika.yaml", Some(&dest_s), true).code,
        codes::OK
    );
    assert_eq!(run("01-hello", Some(&dest_s), false).code, codes::ENV);
    let show = run("price-watch", Some(&dest_s), true);
    assert_eq!(show.code, codes::OK, "{}", show.text);
    assert_eq!(
        std::fs::read_to_string(&dest).expect("written"),
        nika_pack::example("price-watch")
            .expect("embedded")
            .replace("examples/price-watch.nika.yaml", &dest_s),
        "verbatim, self-reference re-pointed — a flat-corpus example"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A cadence clause in a routed intent gets the schedule note — half
/// of « Chaque lundi, analyser… » used to vanish silently (gauntlet
/// 08-01, Camille): the file owns the WORK, cron/CI owns WHEN, and
/// the dropped half is NAMED. A cadence-free intent stays untouched,
/// and a non-OK outcome never grows a note.
#[test]
fn a_routed_cadence_intent_carries_the_schedule_note() {
    let dir = std::env::temp_dir().join(format!("nika-cadence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let dest = dir.join("lundi.nika.yaml");
    let dest_s = dest.to_string_lossy().into_owned();

    let out = run(
        "Chaque lundi, analyser les tickets support et produire les priorités",
        Some(&dest_s),
        false,
    );
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(
        out.text.contains("is a schedule") && out.text.contains("chaque lundi"),
        "the dropped cadence half is named: {}",
        out.text
    );
    assert!(
        out.text.contains("cron"),
        "the note points at the trigger owner: {}",
        out.text
    );

    // No cadence — no note (the note never becomes noise).
    let dest2 = dir.join("plain.nika.yaml");
    let out = run(
        "analyser les tickets support et produire les priorités",
        Some(&dest2.to_string_lossy()),
        false,
    );
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(!out.text.contains("is a schedule"), "{}", out.text);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The wire contract: `nika new '?'` (NO dest) names the set —
/// and a discovery query answered is a SUCCESS (exit 0 · it used to
/// reuse the unknown-template error, so the documented command read
/// as a failure). The `embedded set:` line survives verbatim (the
/// editor probes regex exactly that grammar).
#[test]
fn discovery_query_lists_the_set_without_a_dest() {
    let out = run("?", None, false);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(out.text.contains("embedded set:"), "{}", out.text);
    assert!(
        out.text
            .contains("routes across jobs · lessons · skeletons"),
        "the discovery surface must name the whole intent-routing corpus: {}",
        out.text
    );
    assert!(
        !out.text.contains("closest skeleton"),
        "the router is broader than the skeleton set: {}",
        out.text
    );
    for name in nika_pack::template_names() {
        assert!(out.text.contains(&name), "lists `{name}`: {}", out.text);
    }
}

/// The listing derives its taglines from the template bodies' own
/// `# TEMPLATE` headers — no second prose source to drift.
#[test]
fn discovery_taglines_come_from_the_bodies() {
    let body = nika_pack::template("chain").expect("chain embedded");
    let tag = tagline("chain", body);
    assert!(!tag.is_empty(), "chain header carries a tagline");
    assert!(
        run("?", None, false).text.contains(&tag),
        "the listing shows it"
    );
}

/// A REAL template with no dest can't be instantiated — ask for one
/// (don't silently no-op or panic).
#[test]
fn known_template_without_dest_asks_for_a_path() {
    let out = run("chain", None, false);
    assert_eq!(out.code, codes::ENV, "{}", out.text);
    assert!(out.text.contains("pass a destination"), "{}", out.text);
}

#[test]
fn parallel_intent_routes_to_fanout() {
    let d = dest("par");
    let out = run("summarize every item in parallel", Some(&d), true);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(
        out.text.contains("routed intent → template `fanout`"),
        "{}",
        out.text
    );
    assert!(
        !out.text.contains("is a schedule"),
        "per-item fan-out must not be described as cadence: {}",
        out.text
    );
    // Own-corpus by construction: the instantiated file IS the
    // embedded template verbatim.
    let written = std::fs::read_to_string(&d).expect("file written");
    assert_eq!(Some(written.as_str()), nika_pack::template("fanout"));
    std::fs::remove_file(&d).ok();
}

#[test]
fn an_agentic_research_intent_routes_to_the_job_then_the_skeleton() {
    // `nika new "<words>"` reads the WHOLE catalog, and the catalog now
    // answers with the research JOB — a complete lesson, not a file of
    // slots — because the corpus regained the sentence each entry wrote
    // about itself. The skeleton door still answers `agent-loop`: that
    // door's corpus IS the skeleton set, so the two answers are the two
    // questions, not a drift.
    let d = dest("agent");
    let out = run(
        "an autonomous budgeted agent that researches a topic",
        Some(&d),
        true,
    );
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(out.text.contains("deep-research-brief"), "{}", out.text);
    assert!(
        matches!(
            crate::intent::route_skeletons("an autonomous budgeted agent that researches a topic"),
            crate::intent::RoutingOutcome::Routed { ref template, .. } if template == "agent-loop"
        ),
        "the skeleton door still lands the loop"
    );
    std::fs::remove_file(&d).ok();
}

#[test]
fn approval_intent_routes_to_a_gated_template() {
    let d = dest("gate");
    let out = run(
        "verify then wait for human approval before deploy",
        Some(&d),
        true,
    );
    assert_eq!(out.code, codes::OK, "{}", out.text);
    assert!(
        out.text.contains("`human-gated-ship`") || out.text.contains("`gate-and-act`"),
        "a gated template must win: {}",
        out.text
    );
    std::fs::remove_file(&d).ok();
}

#[test]
fn zero_evidence_intent_keeps_the_wire_contract_error() {
    // Gibberish shares no term with any template — the honest unknown
    // (exit 2) still names the set on the `embedded set:` wire line.
    let out = run("zzzz qqqq xxxx", Some(&dest("zero")), true);
    assert_eq!(out.code, codes::FILE, "{}", out.text);
    assert!(out.text.contains("embedded set:"), "{}", out.text);
}

// NOTE · the bare-`nika new`-in-a-pipe contract (fail fast naming
// `--from`) is pinned at the BINARY plane (bin_smoke) — is_terminal
// inside `cargo test` reflects the invoking terminal, so an
// in-process assert would flip between a laptop run and CI.

#[test]
fn dispatch_with_a_template_is_the_flag_path_unchanged() {
    let d = dest("dispatch");
    let out = dispatch(Some("chain"), Some(&d), true, PLAIN, &stub_audit);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    std::fs::remove_file(&d).ok();
}

// ─── The wizard's pure parts ─────────────────────────────────────

#[test]
fn resolve_template_covers_the_four_rungs() {
    // FLIP (P0-1 · 2026-07-31): was `resolve_template_covers_the_three_rungs`
    // — the third rung asserted a SILENT `chain` fallback on zero-evidence
    // intent (`("chain", false)` indistinguishable from the announced Enter
    // default). The fallback is now a distinct, SAID rung carrying the
    // closest candidates; only the empty answer defaults quietly.
    assert!(matches!(resolve_template(""), WizardRoute::Default));
    assert!(matches!(resolve_template("fanout"), WizardRoute::Exact(ref n) if n == "fanout"));
    assert!(
        matches!(resolve_template("summarize every item in parallel"), WizardRoute::Routed(ref n) if n == "fanout")
    );
    // Zero evidence → the SAID fallback with nobody to name.
    let candidates = match resolve_template("zzzz qqqq") {
        WizardRoute::Fallback { candidates } => Some(candidates),
        _ => None,
    }
    .expect("zero evidence is the said fallback, not a silent chain");
    assert!(candidates.is_empty(), "{candidates:?}");
    // Below the margin → the fallback NAMES the coin-flip pair.
    let candidates =
        match resolve_template("Chaque lundi compare three competitors and write a French brief") {
            WizardRoute::Fallback { candidates } => Some(candidates),
            _ => None,
        }
        .expect("a coin-flip falls back, said");
    assert!(
        candidates.contains(&"media-asset-pack".to_owned()),
        "{candidates:?}"
    );
}

#[test]
fn the_ollama_note_drops_local_under_an_endpoint_override() {
    // P0-20 · « local » is a TOPOLOGY claim the menu cannot make
    // when an override (NIKA_OLLAMA_BASE_URL · OLLAMA_HOST) may
    // point the engine at a LAN box.
    assert!(ollama_note_for(false).contains("local"));
    let overridden = ollama_note_for(true);
    assert!(!overridden.contains("local"), "{overridden}");
    assert!(overridden.contains("custom endpoint"), "{overridden}");
    assert!(
        overridden.contains("zero key"),
        "the protocol truth stays: {overridden}"
    );
}

#[test]
fn the_model_menu_derives_from_the_catalog_local_first() {
    let menu = model_menu();
    assert!(menu.len() >= 2, "catalog carries the menu providers");
    assert!(
        menu[0].0.starts_with("ollama/"),
        "local first (presentation order): {menu:?}"
    );
    assert!(menu[1].0.starts_with("mock/"), "offline second: {menu:?}");
    // Every entry is a full provider/model wire id from the catalog.
    assert!(menu.iter().all(|(m, _)| m.contains('/')), "{menu:?}");
}

/// P0-8 · a non-empty pick that is neither a menu number nor a
/// `provider/model` is SAID and re-asked — it used to become the
/// offline mock SILENTLY (« gpt » → mock/echo without a word).
#[test]
fn ask_model_reasks_an_unrecognized_pick() {
    let mut input = std::io::Cursor::new(b"gpt\n\n".to_vec());
    let mut out = Vec::new();
    let model = ask_model(&mut input, &mut out, PLAIN)
        .expect("io ok")
        .expect("not cancelled");
    assert!(
        model.starts_with("mock/"),
        "Enter after the re-ask: {model}"
    );
    let shown = String::from_utf8(out).expect("utf8");
    assert!(shown.contains("unrecognized"), "the typo is said: {shown}");
}

#[test]
fn resolve_model_accepts_only_a_menu_number_or_a_wire_id() {
    // FLIP (P0-8 · 2026-07-31): resolve_model is now honest — Option,
    // with the ASK LOOP owning the Enter default (default_model) and
    // the re-ask. « 99 » and « gpt » used to pin the SILENT mock
    // fallback.
    let menu = model_menu();
    assert!(
        default_model(&menu).starts_with("mock/"),
        "Enter must never fail"
    );
    assert_eq!(
        resolve_model("1", &menu).as_deref(),
        Some(menu[0].0.as_str())
    );
    assert_eq!(
        resolve_model("ollama/llama3.2:3b", &menu).as_deref(),
        Some("ollama/llama3.2:3b")
    );
    // A number off the menu or a word without `/` is unrecognized —
    // said + re-asked by the loop, never a silent mock.
    assert_eq!(resolve_model("99", &menu), None);
    assert_eq!(resolve_model("gpt", &menu), None);
    assert_eq!(
        resolve_model("", &menu),
        None,
        "Enter is the loop's default"
    );
}

#[test]
fn workflow_id_is_a_kebab_of_the_file_name() {
    assert_eq!(workflow_id("my-first.nika.yaml"), "my-first");
    assert_eq!(workflow_id("dir/Sub/PR Review.nika.yaml"), "pr-review");
    assert_eq!(workflow_id(".nika.yaml"), "my-first");
}

#[test]
fn yaml_scalar_keeps_plain_bare_and_single_quotes_the_rest() {
    assert_eq!(yaml_scalar("mock/echo"), "mock/echo");
    assert_eq!(yaml_scalar("summarize"), "summarize");
    // Space · colon · backslash · quote → single-quoted, literal.
    assert_eq!(
        yaml_scalar("save to C:\\Users\\me"),
        "'save to C:\\Users\\me'"
    );
    assert_eq!(yaml_scalar("foo/bar: baz"), "'foo/bar: baz'");
    assert_eq!(yaml_scalar("it's a test"), "'it''s a test'");
    assert_eq!(yaml_scalar(""), "''");
}

#[test]
fn stamped_file_survives_a_hostile_model_string() {
    // The rust-pro HIGH: a YAML-significant model pick reached the
    // scalar unescaped -> the fresh scaffold failed its OWN check
    // under a green. Every stamp must round-trip through the REAL
    // parser+check clean. The intent no longer rides into the file at
    // all (there is no description slot), so the hostile-INTENT half
    // of this battery has no destination left to defend.
    let body = nika_pack::template("chain").expect("embedded");
    for model in ["mock/echo", "foo/bar: baz"] {
        let stamped = stamp(body, "hostile", Some(model));
        let parsed = nika_schema::parse(
            &stamped,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        assert!(parsed.is_ok(), "model={model:?} must parse: {parsed:?}");
        // The check ladder must not choke either (a dirty audit is
        // fine; a PARSE error at this point is the bug).
        let wf = parsed.expect("asserted ok above");
        let _ = nika_check::check(&wf);
    }
}

#[test]
fn every_embedded_template_audits_clean_or_is_a_documented_gap() {
    // The own-corpus law (#261): every embedded skeleton a fresh
    // scaffold can produce MUST audit clean — a red ladder on a first
    // scaffold is the self-contradiction the wizard exists to avoid.
    // This ratchet was MISSING (pack-integrity only hashes text), so
    // `api-upload-and-create` shipped in #257 failing its OWN
    // SECRETS-egress check, unnoticed, until a user-sim caught it.
    //
    // KNOWN GAP — an operator design call, NOT a template typo: the
    // ADR-092 flow model taints an authenticated `invoke`'s OUTPUT (a
    // secret in a fetch auth-header taints the response, exactly as a
    // secret in the body would), with no `infer`/`agent`-style prompt
    // exception and no output-declassification construct. So
    // EMPTY since 2026-07-10: the one former gap
    // (`api-upload-and-create` — a secret-authed response piped to
    // `outputs:` had NO sanctioned path) resolved via the
    // output-declassification this ratchet's note called for:
    // `egress: [{ to: "outputs" }]` (spec 01-envelope §egress · the
    // owner declassifies the workflow boundary itself). Every template
    // now passes its own audit; a dirty one fails this ratchet unless
    // a genuine flow-model design gap is documented here.
    const KNOWN_GAP: &[&str] = &[];
    let mut clean = 0_usize;
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("template embedded");
        let parsed = nika_schema::parse(
            body,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        );
        assert!(parsed.is_ok(), "{name}: template must parse: {parsed:?}");
        let wf = parsed.expect("asserted ok above");
        let is_gap = KNOWN_GAP.contains(&name.as_str());
        if nika_check::check(&wf).is_clean() {
            assert!(
                !is_gap,
                "{name}: now audits CLEAN — remove it from KNOWN_GAP, the design gap is resolved"
            );
            clean += 1;
        } else {
            assert!(
                is_gap,
                "{name}: a fresh scaffold FAILS its own `nika check` (own-corpus law · #261) — \
                     fix the template, or (if a genuine flow-model design gap) document it in KNOWN_GAP"
            );
        }
    }
    assert!(clean >= 8, "expected >= 8 clean templates, got {clean}");
}

#[test]
fn stamp_fills_exactly_the_two_known_slots() {
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("embedded");
        let stamped = stamp(body, "field-demo", Some("mock/echo"));
        assert!(stamped.contains("nika: field-demo"), "{name}: id stamped");
        assert!(
            !stamped.contains("-template "),
            "{name}: no template id remnant"
        );
        if body.lines().any(|l| l.starts_with("model: ")) {
            assert!(
                stamped.contains("model: mock/echo"),
                "{name}: model stamped"
            );
        }
    }
}

/// The whole conversation over an injected cursor: three Enters =
/// the golden path (chain · default name · offline mock).
#[test]
fn read_wizard_three_enters_is_the_golden_path() {
    let base = fresh_base("golden");
    let mut input = std::io::Cursor::new(b"\n\n\n".to_vec());
    let mut out = Vec::new();
    let w = read_wizard(
        &mut input,
        &mut out,
        base.to_str().expect("utf8"),
        None,
        PLAIN,
    )
    .expect("io ok")
    .expect("not cancelled");
    assert_eq!(w.template, "chain");
    assert_eq!(w.dest, "my-first.nika.yaml");
    assert!(w.model.as_deref().is_some_and(|m| m.starts_with("mock/")));
    let shown = String::from_utf8(out).expect("utf8");
    assert!(shown.contains("template `chain`"), "{shown}");
    assert!(shown.contains("ollama/"), "menu shows local first: {shown}");
    std::fs::remove_dir_all(&base).ok();
}

/// A skeleton without a top-level `model:` gets NO model question —
/// asking would promise a stamp the file doesn't carry. Two answers
/// complete the flow and the conversation says where models live.
#[test]
fn read_wizard_skips_the_model_question_when_the_template_takes_none() {
    let base = fresh_base("permodel");
    // gate-and-act carries no top-level model line (exact name = rung 1).
    let mut input = std::io::Cursor::new(b"gate-and-act\n\n".to_vec());
    let mut out = Vec::new();
    let w = read_wizard(
        &mut input,
        &mut out,
        base.to_str().expect("utf8"),
        None,
        PLAIN,
    )
    .expect("io ok")
    .expect("not cancelled");
    assert_eq!(w.template, "gate-and-act");
    assert_eq!(w.model, None, "no model harvested");
    let shown = String::from_utf8(out).expect("utf8");
    assert!(
        shown.contains("models are per-task in this skeleton"),
        "{shown}"
    );
    assert!(
        !shown.contains("a number, or any provider/model"),
        "the question must not fire: {shown}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// The default file name walks past collisions — a wizard re-run must
/// never dead-end on « exists » AFTER every question was answered.
#[test]
fn wizard_default_dest_walks_past_collisions() {
    let base = fresh_base("collide");
    let b = base.to_str().expect("utf8");
    assert_eq!(wizard_default_dest(b), "my-first.nika.yaml");
    std::fs::write(base.join("my-first.nika.yaml"), "x").expect("seed");
    assert_eq!(wizard_default_dest(b), "my-second.nika.yaml");
    std::fs::write(base.join("my-second.nika.yaml"), "x").expect("seed");
    std::fs::write(base.join("my-third.nika.yaml"), "x").expect("seed");
    assert_eq!(wizard_default_dest(b), "my-4.nika.yaml");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn template_takes_model_matches_every_body() {
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).expect("embedded");
        let has = body.lines().any(|l| l.starts_with("model: "));
        assert_eq!(template_takes_model(body), has, "{name}");
    }
    // The split is real: both kinds exist in the embedded set.
    let kinds: Vec<bool> = nika_pack::template_names()
        .iter()
        .map(|n| template_takes_model(nika_pack::template(n).expect("embedded")))
        .collect();
    assert!(kinds.contains(&true) && kinds.contains(&false));
}

/// An intent answer routes; EOF mid-flow cancels instead of looping.
#[test]
fn read_wizard_routes_and_cancels_honestly() {
    let base = fresh_base("routes");
    let mut input = std::io::Cursor::new(b"process every item in parallel\nbatch\n2\n".to_vec());
    let mut out = Vec::new();
    let w = read_wizard(
        &mut input,
        &mut out,
        base.to_str().expect("utf8"),
        None,
        PLAIN,
    )
    .expect("io ok")
    .expect("not cancelled");
    assert_eq!(w.template, "fanout");
    assert_eq!(w.dest, "batch.nika.yaml", "the suffix is appended");

    let mut eof = std::io::Cursor::new(Vec::new());
    let mut out2 = Vec::new();
    assert!(
        read_wizard(
            &mut eof,
            &mut out2,
            base.to_str().expect("utf8"),
            None,
            PLAIN
        )
        .expect("io ok")
        .is_none(),
        "EOF = cancelled"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// Answering everything and THEN hitting an existing typed dest is
/// the one wizard dead-end left by design (the refuse-overwrite law
/// on a HUMAN-chosen name) — pin the honest ENV exit + the --force
/// override (rust-pro e2e review finding #4).
#[test]
fn wizard_io_refuses_a_typed_existing_dest_and_force_overrides() {
    let base = fresh_base("refuse");
    std::fs::write(base.join("my-first.nika.yaml"), "taken").expect("seed");

    let mut input = std::io::Cursor::new(b"\nmy-first\n\n".to_vec());
    let mut out = Vec::new();
    let v = wizard_io(
        base.to_str().expect("utf8"),
        None,
        false,
        PLAIN,
        &mut input,
        &mut out,
        &stub_audit,
    );
    assert_eq!(v.code, codes::ENV, "{}", v.text);
    assert!(
        v.text.contains("--force"),
        "teaches the override: {}",
        v.text
    );
    assert_eq!(
        std::fs::read_to_string(base.join("my-first.nika.yaml")).expect("read"),
        "taken",
        "refused = untouched"
    );

    let mut input2 = std::io::Cursor::new(b"\nmy-first\n\n".to_vec());
    let mut out2 = Vec::new();
    let v2 = wizard_io(
        base.to_str().expect("utf8"),
        None,
        true,
        PLAIN,
        &mut input2,
        &mut out2,
        &stub_audit,
    );
    assert_eq!(v2.code, codes::OK, "{}", v2.text);
    assert!(
        std::fs::read_to_string(base.join("my-first.nika.yaml"))
            .expect("read")
            .contains("nika: my-first"),
        "--force overwrote with the stamped template"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// End-to-end over injected io: the file lands stamped + checkable.
#[test]
fn wizard_io_materializes_a_stamped_file() {
    let dir = std::env::temp_dir().join(format!("nika-wizard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut input = std::io::Cursor::new(b"\nfirst\n\n".to_vec());
    let mut out = Vec::new();
    let v = wizard_io(
        dir.to_str().expect("utf8"),
        None,
        true,
        PLAIN,
        &mut input,
        &mut out,
        &stub_audit,
    );
    assert_eq!(v.code, codes::OK, "{}", v.text);
    assert!(v.text.contains("scriptable form"), "{}", v.text);
    // The wow contract: the wizard SHOWS the audit ladder — the file
    // arrives already checked, not with a suggestion to check.
    assert!(v.text.contains("audited"), "the ladder ran: {}", v.text);
    let written = std::fs::read_to_string(dir.join("first.nika.yaml")).expect("file written");
    assert!(written.contains("nika: first"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn boilerplate_and_stopwords_do_not_route() {
    // Regression: the router indexed the `# SLOT:` scaffolding prose +
    // accepted any score > 0, so envelope/stopword queries spuriously
    // routed to a scaffold. They carry no SIGNAL → clarify, never route.
    for garbage in [
        "the",
        "workflow",
        "template",
        "slot",
        "fill the lines",
        "a workflow",
    ] {
        assert!(
            !matches!(crate::intent::route(garbage), RoutingOutcome::Routed { .. }),
            "`{garbage}` must not route · got {:?}",
            crate::intent::route(garbage)
        );
    }
    // …but a real intent still routes on its signal terms.
    assert!(matches!(
        crate::intent::route("scrape a website and summarize"),
        RoutingOutcome::Routed { .. }
    ));
    assert!(matches!(
        crate::intent::route("review and approve before deploy"),
        RoutingOutcome::Routed { .. }
    ));
}

/// P0-10 · the non-interactive door: below the confidence bar,
/// `run` writes NOTHING and the honest error names the candidates.
#[test]
fn a_below_the_bar_intent_writes_nothing_and_names_the_candidates() {
    let d = dest("clarify");
    let out = run(
        "Chaque lundi compare three competitors and write a French brief",
        Some(&d),
        true,
    );
    assert_eq!(out.code, codes::FILE, "{}", out.text);
    assert!(!Path::new(&d).exists(), "below the bar = no write");
    assert!(out.text.contains("competitor-radar"), "{}", out.text);
    assert!(out.text.contains("website-brief"), "{}", out.text);
}

/// P0-10 · the routed file is a DRAFT: the message says so, never
/// « ready », and hands over to `nika check` before any run.
#[test]
fn a_confident_route_lands_a_draft_that_hands_over_to_check() {
    let d = dest("draft");
    let out = run("summarize every item in parallel", Some(&d), true);
    assert_eq!(out.code, codes::OK, "{}", out.text);
    let lower = out.text.to_ascii_lowercase();
    assert!(lower.contains("draft"), "{}", out.text);
    assert!(!lower.contains("ready"), "{}", out.text);
    assert!(out.text.contains("nika check"), "{}", out.text);
    std::fs::remove_file(&d).ok();
}

/// Gauntlet 2026-08-01 (Priya + Marco · the teach-a-command-that-breaks
/// class): every taught line WORKS pasted back in the taught context.
/// A skeleton clarify-hint carries its <dest>; an example lands bare;
/// a spaced intent is re-echoed shell-quoted.
#[test]
fn taught_lines_survive_the_paste_back() {
    // A skeleton-first clarify teaches the dest.
    let out = run(
        "check disk space, restart the stuck service, notify the team",
        None,
        false,
    );
    assert_eq!(out.code, codes::FILE, "{}", out.text);
    let rest = out
        .text
        .split("take one by name (`")
        .nth(1)
        .expect("the clarify hint is present");
    let taught = rest.split('`').next().unwrap_or("");
    let name = taught
        .trim_start_matches("nika new ")
        .split_whitespace()
        .next()
        .unwrap_or("");
    if nika_pack::template(name).is_some() {
        assert!(
            taught.ends_with("<dest>.nika.yaml"),
            "a skeleton hint must teach its destination: {taught}"
        );
    } else {
        assert!(
            nika_pack::example(name).is_some(),
            "the taught name must exist: {taught}"
        );
    }

    // The dest-missing hint re-echoes a spaced intent QUOTED. The probe
    // must route CONFIDENTLY to a TEMPLATE (a skeleton is what demands a
    // <dest>; a routed example lands bare): `summarize … csv` stopped
    // routing when the spec snippets landed (snippets/think competes on
    // it, and csv now belongs to the csv-chart-report example), so the
    // probe leans on `docker`, which only the docker-report skeleton
    // carries.
    let out = run("report on my running docker containers", None, false);
    assert_eq!(out.code, codes::ENV, "{}", out.text);
    assert!(
        out.text
            .contains("nika new 'report on my running docker containers' <dest>.nika.yaml"),
        "the taught paste-back is ONE shell argument: {}",
        out.text
    );
}

/// The keyless lane is named on the recipes that need a seat, and ONLY
/// on those (first-run review 2026-08-03: the scaffold taught `nika run`
/// as its next step, and on a machine with no model wired that is the
/// first thing the reader tries and the first thing that fails). Both
/// directions matter — an exec-only recipe offered a `--model` hint
/// would be teaching noise, which is how a courtesy line stops being
/// read at all.
#[test]
fn the_keyless_lane_is_named_only_where_a_seat_is_needed() {
    assert!(needs_a_seat(
        "tasks:\n  ask:\n    infer:\n      prompt: hi\n"
    ));
    assert!(needs_a_seat(
        "tasks:\n  loop:\n    agent:\n      prompt: go\n"
    ));
    // Inline forms count too — the head of the line is the signal.
    assert!(needs_a_seat("    infer: { prompt: \"x\" }\n"));

    // exec/invoke recipes reach no provider.
    assert!(!needs_a_seat(
        "tasks:\n  greet:\n    exec:\n      command: [echo, hi]\n"
    ));
    assert!(!needs_a_seat(
        "tasks:\n  read:\n    invoke:\n      tool: \"nika:read\"\n"
    ));
    // Prose about infer is not an infer task (the comment lane).
    assert!(!needs_a_seat(
        "# infer: this comment mentions the verb\ntasks:\n  t:\n    exec:\n      command: [echo]\n"
    ));
}
