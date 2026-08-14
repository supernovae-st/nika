use super::*;

/// `run_many`: every file audits even after an earlier failure (the
/// broken file sits in the MIDDLE), each report keeps its own header,
/// and the worst spec-§4 exit survives.
#[test]
fn run_many_audits_every_file_and_keeps_the_worst_exit() {
    let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let clean =
        "nika: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
    let broken = "nika: bad\ntasks:\n  t:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 10, model: \"mock/echo\" }\n";
    let a = dir.join("many-a.nika.yaml");
    let b = dir.join("many-broken.nika.yaml");
    let c = dir.join("many-c.nika.yaml");
    std::fs::write(&a, clean).expect("fixture a");
    std::fs::write(&b, broken).expect("fixture b");
    std::fs::write(&c, clean).expect("fixture c");

    let paths: Vec<String> = [&a, &b, &c]
        .iter()
        .map(|p| p.to_str().expect("utf8 path").to_owned())
        .collect();
    let out = run_many(
        &paths,
        false,
        Profile::Advisory,
        None,
        Theme::new(false, true, false),
    );

    assert_eq!(out.code, 2, "the broken middle file's exit survives");
    // The report header names its file by BASENAME (`nika check · f`).
    for name in [
        "many-a.nika.yaml",
        "many-broken.nika.yaml",
        "many-c.nika.yaml",
    ] {
        assert!(
            out.text.contains(name),
            "every report present (headers name their file): missing {name}\n{}",
            out.text
        );
    }
    let after = out.text.split_once("many-broken.nika.yaml").map(|s| s.1);
    assert!(
        after.is_some_and(|tail| tail.contains("many-c.nika.yaml")),
        "the file AFTER the failure still audited: {}",
        out.text
    );
}

/// `run_many` on all-clean files exits OK — the concatenation never
/// invents a failure.
#[test]
fn run_many_is_clean_when_every_file_is() {
    let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let clean =
        "nika: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
    let a = dir.join("clean-a.nika.yaml");
    let b = dir.join("clean-b.nika.yaml");
    std::fs::write(&a, clean).expect("fixture a");
    std::fs::write(&b, clean).expect("fixture b");
    let paths: Vec<String> = [&a, &b]
        .iter()
        .map(|p| p.to_str().expect("utf8 path").to_owned())
        .collect();
    let out = run_many(
        &paths,
        false,
        Profile::Advisory,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn missing_read_files_flags_static_literal_and_var_default() {
    let dir = std::env::temp_dir().join(format!("nika-lint-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap_or(());
    let present = dir.join("present.txt");
    std::fs::write(&present, "x").expect("fixture");
    let yaml = format!(
        "nika: w\nconst:\n  src: \"{missing}\"\ntasks:\n  a:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ const.src }}}}\" }}\n  b:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{present}\" }}\n  c:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ tasks.a.output }}}}\" }}\n",
        missing = dir.join("missing.txt").display(),
        present = present.display(),
    );
    let wf = parse_wf(&yaml);
    let flagged: Vec<(String, String)> = nika_check::static_read_paths(&wf)
        .into_iter()
        .filter(|(_, p)| !std::path::Path::new(p).exists())
        .collect();
    // `a` via var default → flagged · `b` exists → silent ·
    // `c` dynamic (task ref) → the lint never guesses.
    assert_eq!(flagged.len(), 1, "{flagged:?}");
    assert_eq!(flagged[0].0, "a");
    let _ = std::fs::remove_file(&present);
}

#[test]
fn pricing_section_rates_known_null_unknown() {
    let wf = parse_wf(
        "nika: priced\nmodel: anthropic/claude-opus-4-5\ntasks:\n  think:\n    infer:\n      prompt: hi\n  odd:\n    infer:\n      model: custom/never-heard-of-it\n      prompt: hi\n",
    );
    let report = nika_check::check(&wf);
    let section = pricing_section(&report, &unresolvable_models(&report, &wf).findings);
    let models = section["models"].as_array().expect("array");
    assert_eq!(models.len(), 2, "one row per requirements model");
    let by_model = |name: &str| {
        models
            .iter()
            .find(|m| m["model"] == name)
            .expect("a row per requirements model")
            .clone()
    };
    let priced = by_model("anthropic/claude-opus-4-5");
    assert!((priced["input_per_million"].as_f64().expect("rate") - 5.0).abs() < 1e-9);
    assert!((priced["output_per_million"].as_f64().expect("rate") - 25.0).abs() < 1e-9);
    // UNKNOWN renders null — a missing price must look missing,
    // never $0.00 (the silent-zero anti-pattern).
    let unknown = by_model("custom/never-heard-of-it");
    assert!(unknown["input_per_million"].is_null());
    assert!(unknown["output_per_million"].is_null());
}

fn parse_wf(yaml: &str) -> RawWorkflow {
    nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses")
}

/// The mute-diagnostic regression the battery re-run caught: with NO
/// `permits:` block, a floor escape (SSRF-parity pass · permits-
/// independent) exited rc=2 while the PERMITS panel printed only the
/// informational line — `✖ findings above` pointed at nothing. The
/// panel must render the escape. F-O8 rider: the ABSENT block now
/// also speaks — the tool escape rides NIKA-AUTH-006 next to the
/// floor's NIKA-SEC-005.
#[test]
fn floor_escape_renders_without_a_permits_block() {
    let wf = parse_wf(
        "nika: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/x\" } }\n",
    );
    let report = nika_check::check(&wf);
    assert!(
        !report.capability_escapes.is_empty(),
        "the floor pass fires without permits"
    );
    let theme = Theme::new(false, true, false);
    let mut out = String::new();
    permits(&mut out, &report, &wf, theme);
    assert!(out.contains("SSRF floor"), "escape must render: {out}");
    assert!(
        out.contains("NIKA-SEC-005"),
        "the wire code names it: {out}"
    );
    // A2 (agent battery 2026-07-11): the code LEADS the row in
    // bracket position — `[NIKA-SEC-005 · net]` — so the PERMITS
    // panel is explainable like every CONFORM row (`nika explain`).
    assert!(
        out.contains("[NIKA-SEC-005 · net]"),
        "the code leads the row: {out}"
    );
    // F-O8 « absent = zero authority » + NEP-0003 law 1: the literal
    // URL is a NET escape under the absent block — NIKA-AUTH-006 rides
    // next to the floor code.
    assert!(
        out.contains("[NIKA-AUTH-006 · net]"),
        "the absent boundary speaks its own code: {out}"
    );
    // …and a public-host fetch without permits is NOT the
    // informational case anymore: the net escape (AUTH-006 · the
    // literal URL is statically judged) is the row (the old
    // « no boundary declared » mute is retired).
    let undeclared = parse_wf(
        "nika: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
    );
    let undeclared_report = nika_check::check(&undeclared);
    let mut undeclared_out = String::new();
    permits(&mut undeclared_out, &undeclared_report, &undeclared, theme);
    assert!(
        undeclared_out.contains("[NIKA-AUTH-006 · net]"),
        "absent + a literal url = the AUTH-006 net row: {undeclared_out}"
    );
    // …while the TRUE clean case (pure compute · zero authority
    // assumed) renders the F-O8 informational line.
    let clean = parse_wf(
        "nika: w\nmodel: mock/echo\ntasks:\n  probe:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
    );
    let clean_report = nika_check::check(&clean);
    let mut clean_out = String::new();
    permits(&mut clean_out, &clean_report, &clean, theme);
    assert!(clean_out.contains("zero authority"), "{clean_out}");
}

/// The #395 admitting direction, through the CLI render: the battery
/// local-watch repro (`permits.net.http: ["127.0.0.1"]` + a literal
/// fetch to it) is GREEN — no NIKA-SEC-005, no dead-grant flag — and
/// the panel TEACHES the clearing with the informational line.
#[test]
fn permitted_loopback_literal_renders_green_with_the_teaching_line() {
    let wf = parse_wf(
        "nika: local-watch\npermits:\n  net: { http: [\"127.0.0.1\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/price.json\" } }\n",
    );
    let report = nika_check::check(&wf);
    assert!(
        report.capability_escapes.is_empty(),
        "the exact literal declassifies: {:?}",
        report.capability_escapes
    );
    let theme = Theme::new(false, true, false);
    let mut out = String::new();
    permits(&mut out, &report, &wf, theme);
    assert!(
        out.contains("literal + const: args fit the boundary"),
        "green panel: {out}"
    );
    assert!(
        out.contains("exact loopback literal") && out.contains("`127.0.0.1`"),
        "the teaching line renders: {out}"
    );
    // …and a boundary with no loopback literal renders NO such line.
    let plain = parse_wf(
        "nika: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
    );
    let plain_report = nika_check::check(&plain);
    let mut plain_out = String::new();
    permits(&mut plain_out, &plain_report, &plain, theme);
    assert!(
        !plain_out.contains("exact loopback literal"),
        "no loopback grant → no line: {plain_out}"
    );
}

/// A `required: true` input with no `default:` is what the operator MUST
/// pass — `check` should NAME it, so a bare `run` does not surprise them
/// with NIKA-VAR-001.
#[test]
fn required_input_without_default_is_listed() {
    let wf = parse_wf(
        "nika: needs-input\nmodel: mock/echo\ninputs:\n  text:\n    type: string\n    required: true\ntasks:\n  a:\n    infer: { prompt: \"${{ inputs.text }}\" }\n",
    );
    assert_eq!(required_inputs(&wf), vec!["text"]);
}

/// Untyped (the value IS the default) · typed-with-default · typed-optional
/// — none block a bare `run`, so none are listed.
#[test]
fn defaulted_or_optional_inputs_are_not_listed() {
    let wf = parse_wf(
        "nika: ok\nmodel: mock/echo\ninputs:\n  b:\n    type: string\n    default: \"d\"\n  c:\n    type: string\n    required: false\nconst:\n  a: \"has default\"\ntasks:\n  t:\n    infer: { prompt: \"${{ const.a }} ${{ inputs.b }} ${{ inputs.c }}\" }\n",
    );
    assert!(
        required_inputs(&wf).is_empty(),
        "{:?}",
        required_inputs(&wf)
    );
}

/// Write a fixture + run the human `check` render over it (ascii/no-colour
/// so the assertions pin glyphs/text, not ANSI). The render path is what
/// the operator reads — these tests pin its exact words.
///
/// NOTE (W8 · P1 « --ascii réellement ASCII », audit 2026-07-30): the
/// ascii register now folds chrome punctuation to its ASCII twins at
/// the verb boundary (`vocab::sober`), so ascii-mode assertions pin
/// `-`/`--`/`X`, never `·`/`—`/`✖` — the unicode pins live in the
/// `checked_text(..., false)` calls, unchanged.
fn checked_text(name: &str, yaml: &str, ascii: bool) -> String {
    // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
    // matrix · a dev double-run) share the OS tmpdir, and a fixed
    // name let them stomp each other's fixtures mid-read (flaked
    // live 2026-07-10 — the same fixed-temp-name class as the
    // check-expect mktemp collision, #376).
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, yaml).expect("fixture body");
    let theme = Theme::new(false, ascii, false);
    run(path.to_str().expect("utf8 path"), false, false, None, theme).text
}

/// Same fixture plumbing, full `VerbOutput` (exit-code assertions) —
/// the `--native-strict` posture tests read `.code`.
fn checked_output(name: &str, yaml: &str, native_strict: bool) -> VerbOutput {
    checked_output_profile(name, yaml, native_strict, Profile::Advisory)
}

/// The posture-parameterized twin of [`checked_output`] — the
/// `--profile operational` readiness-gate tests read `.code`.
fn checked_output_profile(
    name: &str,
    yaml: &str,
    native_strict: bool,
    profile: Profile,
) -> VerbOutput {
    // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
    // matrix · a dev double-run) share the OS tmpdir, and a fixed
    // name let them stomp each other's fixtures mid-read (flaked
    // live 2026-07-10 — the same fixed-temp-name class as the
    // check-expect mktemp collision, #376).
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, yaml).expect("fixture body");
    let theme = Theme::new(false, true, false);
    run_with_profile(
        path.to_str().expect("utf8 path"),
        false,
        native_strict,
        profile,
        None,
        theme,
    )
}

/// #320 repro 1: a CATALOGED-but-unresolvable provider (`azure/…` —
/// the vendor listing knows it, the resolver does not) must be a
/// finding, exit 2 — never a green audit that dies at run.
#[test]
fn models_rung_reds_a_cataloged_but_unresolvable_provider() {
    let out = checked_output(
        "models-azure.nika.yaml",
        "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n",
        false,
    );
    assert_eq!(
        out.code, 2,
        "unresolvable provider is a finding: {}",
        out.text
    );
    assert!(
        out.text.contains("MODELS") && out.text.contains("`azure`"),
        "the rung names the provider: {}",
        out.text
    );
}

/// P0-11 (UX audit 2026-07-30): the human footer re-decided the
/// verdict on its own narrow criterion (`report.is_clean()` — MODELS
/// and SKILLS outside it), so one file printed `✖ MODELS …` THEN
/// `✔ audited · …` while the exit said 2 and `--json` said
/// `clean: false`. Three surfaces, two answers. The verdict is
/// computed ONCE (the `clean` fold at the top of `run`) and every
/// surface renders THAT — the footer's job is to show it, never to
/// re-derive it.
#[test]
fn the_footer_never_contradicts_the_exit_code() {
    let out = checked_output(
        "footer-verdict.nika.yaml",
        "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n",
        false,
    );
    assert_eq!(out.code, 2, "the MODELS finding fails: {}", out.text);
    assert!(out.text.contains("MODELS"), "the rung speaks: {}", out.text);
    assert!(
        !out.text.contains("audited"),
        "no audited card over a failing verdict (exit 2 · clean: false): {}",
        out.text
    );
    assert!(
        out.text.contains("findings above"),
        "the footer renders THE verdict: {}",
        out.text
    );
}

/// P0-6 (UX audit 2026-07-30): unbounded autonomy must never wear
/// the green audited card — and under the operational profile it
/// BLOCKS readiness (exit 2). The fixture is the audit's own:
/// `tools: ["nika:*"]` + an agent loop at `max_turns: 100` with no
/// `max_tokens_total` — zero findings, zero escapes, and (before
/// this fix) `✔ audited · est unbounded` painted `Role::Good` at
/// exit 0.
#[test]
fn operational_profile_folds_unbounded_risk_into_the_verdict() {
    let yaml = "nika: loop\nmodel: mock/echo\npermits:\n  tools: [\"nika:*\"]\ntasks:\n  loop:\n    agent: { prompt: \"go\", tools: [\"nika:read\"], max_turns: 100 }\n";
    // Advisory (default): the exit stays 0 — but the card must tell
    // the truth (no green audited line over unbounded rope).
    let advisory = checked_output("risk-advisory.nika.yaml", yaml, false);
    assert_eq!(
        advisory.code, 0,
        "the advisory posture does not gate: {}",
        advisory.text
    );
    assert!(
        !advisory.text.contains("ok audited"),
        "no green audited card over unbounded autonomy: {}",
        advisory.text
    );
    assert!(
        advisory.text.contains("risk unbounded"),
        "the card NAMES the grade: {}",
        advisory.text
    );
    // Operational: the SAME file fails readiness — grade ≥ High
    // folds into the exit-2 verdict, and the refusal line says why.
    let operational = checked_output_profile(
        "risk-operational.nika.yaml",
        yaml,
        false,
        Profile::Operational,
    );
    assert_eq!(
        operational.code, 2,
        "the operational profile blocks grade ≥ High: {}",
        operational.text
    );
    assert!(
        !operational.text.contains("ok audited"),
        "no green audited card under the gate: {}",
        operational.text
    );
    assert!(
        operational.text.contains("X operational - risk unbounded"),
        "the refusal names the grade and the posture: {}",
        operational.text
    );
    // The JSON twin agrees — the one verdict on every surface: exit
    // 2 · `operational_clean: false` · the grade on the payload.
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    let path = dir.join("risk-operational.nika.yaml");
    let out = run_with_profile(
        path.to_str().expect("utf8 path"),
        true,
        false,
        Profile::Operational,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 2, "the machine surface agrees: {}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], true, "spec-clean stays true: {payload:#}");
    assert_eq!(payload["risk_grade"], "unbounded", "{payload:#}");
    assert_eq!(payload["operational_clean"], false, "{payload:#}");
}

/// The advisory card over bounded-but-broad authority is honest too:
/// a glob grant (`tools: nika:*`) with every token capped is High —
/// not green, not blocking by default — and a narrow boundary keeps
/// the green card with the grade named.
#[test]
fn the_card_names_the_grade_on_every_rung() {
    let high = checked_output(
        "risk-high.nika.yaml",
        "nika: h\nmodel: anthropic/claude-sonnet-4-6\npermits:\n  tools: [\"nika:*\"]\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        false,
    );
    assert_eq!(high.code, 0, "advisory: {}", high.text);
    assert!(
        !high.text.contains("ok audited") && high.text.contains("risk high"),
        "broad-but-bounded is named, not greened: {}",
        high.text
    );
    let low = checked_output(
        "risk-low.nika.yaml",
        "nika: l\nmodel: anthropic/claude-sonnet-4-6\npermits: {}\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
        false,
    );
    assert_eq!(low.code, 0, "{}", low.text);
    assert!(
        low.text.contains("ok audited") && low.text.contains("risk low"),
        "pure compute keeps the green card, grade named: {}",
        low.text
    );
}

/// #320 repro 2: a BARE model id (no `<provider>/` prefix) reds the
/// rung AND must never wear a conjured price in the pricing section.
#[test]
fn models_rung_reds_a_bare_model_id_and_never_conjures_a_price() {
    let out = checked_output(
        "models-bare.nika.yaml",
        "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n",
        false,
    );
    assert_eq!(out.code, 2, "bare id is a finding: {}", out.text);
    assert!(
        out.text.contains("bare model id"),
        "teaches the contract: {}",
        out.text
    );
    // The JSON surface: models_resolve false · clean false · the
    // pricing row is NULL (unpriced beats conjured — the $0.0001
    // fuzzy-match hole from the live evidence).
    // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
    // matrix · a dev double-run) share the OS tmpdir, and a fixed
    // name let them stomp each other's fixtures mid-read (flaked
    // live 2026-07-10 — the same fixed-temp-name class as the
    // check-expect mktemp collision, #376).
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    let path = dir.join("models-bare.nika.yaml");
    let theme = Theme::new(false, true, false);
    let out = run(path.to_str().expect("utf8 path"), true, false, None, theme);
    assert_eq!(out.code, 2);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], false);
    assert_eq!(payload["models_resolve"], false);
    assert_eq!(
        payload["model_findings"][0]["model"], "gpt-5-turbo",
        "{payload:#}"
    );
    let row = &payload["pricing"]["models"][0];
    assert!(
        row["input_per_million"].is_null() && row["output_per_million"].is_null(),
        "an unresolvable model is never priced: {row:#}"
    );
}

/// The happy path: every model resolvable → the rung is one green
/// line and the audit verdict is untouched.
#[test]
fn models_rung_is_green_when_every_model_resolves() {
    let out = checked_output(
        "models-green.nika.yaml",
        "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        false,
    );
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text.contains("MODELS") && out.text.contains("1 model resolves"),
        "the green rung is visible: {}",
        out.text
    );
}

/// The parameterization pin (found 2026-07-29 rendering the
/// conformance parity through the reference harness, sharpened
/// 2026-07-30 with the shared resolver): a TEMPLATED `model:` is a
/// run-time fact, but its DECLARED DEFAULT is not — the rung judges
/// the default through `nika_check::static_literal_of` (spec 08 §H8
/// « one workflow, any backend » · the spec's own fixture
/// `stdlib/providers/005-valid-parameterized-model`), and the green
/// line names the via-default judgement.
#[test]
fn models_rung_judges_a_templated_models_declared_default() {
    // A bare-literal const (spec 01 §const), read through `${{ }}`
    // at the task — the parameterization pattern in its simplest
    // canonical form.
    let out = checked_output(
        "models-param.nika.yaml",
        "nika: p\nconst:\n  model: \"anthropic/claude-sonnet-4-6\"\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 10, model: \"${{ const.model }}\" }\n",
        false,
    );
    assert_eq!(
        out.code, 0,
        "a resolvable declared default is not a finding: {}",
        out.text
    );
    assert!(
        !out.text.contains("bare model id"),
        "the raw template is never read as an id: {}",
        out.text
    );
    assert!(
        out.text.contains("via declared default"),
        "the green names WHAT resolved (the default, not the run-time value): {}",
        out.text
    );
    // The teeth stay on what IS statically decidable.
    let literal = checked_output(
        "models-param-teeth.nika.yaml",
        "nika: p\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n",
        false,
    );
    assert_eq!(
        literal.code, 2,
        "a LITERAL bare id still reds: {}",
        literal.text
    );
    assert!(literal.text.contains("bare model id"), "{}", literal.text);
}

/// The sharper half of the same pin: `${{ const.model }}` whose
/// const declares a BAD id is a finding — the skip-everything fix
/// (69c402333) let a refusable declared default sail through green.
/// The fixture uses the TYPED constant form (`{ type, value }` ·
/// spec 01 §const normative discriminator) so both resolver arms
/// are exercised across this test pair.
#[test]
fn models_rung_reds_a_templated_models_refusable_default() {
    let out = checked_output(
        "models-param-bad.nika.yaml",
        "nika: p\nconst:\n  model: { type: string, value: \"gpt-5-turbo\" }\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 10, model: \"${{ const.model }}\" }\n",
        false,
    );
    assert_eq!(
        out.code, 2,
        "a refusable declared default is a finding: {}",
        out.text
    );
    assert!(
        out.text.contains("declared default `gpt-5-turbo`"),
        "the finding names BOTH halves (template + judged default): {}",
        out.text
    );
    assert!(
        out.text.contains("${{ const.model }}"),
        "the row shows the model as written: {}",
        out.text
    );
}

/// A `{ type, default }` const is a BARE LITERAL OBJECT per the
/// spec 01 §const normative discriminator (typed constants carry
/// `value:`, not `default:`) — so `${{ const.model }}` over it
/// resolves to an object, not a string, and the rung makes NO
/// claim. Pinned because the spec's own fixture
/// (`stdlib/providers/005-valid-parameterized-model`) writes this
/// exact shape: the resolver must never « helpfully » read the
/// object's `default` key — analysis never guesses.
#[test]
fn models_rung_never_guesses_inside_a_literal_object_const() {
    let out = checked_output(
        "models-param-object.nika.yaml",
        "nika: p\nconst:\n  model: { type: string, default: \"gpt-5-turbo\" }\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 10, model: \"${{ const.model }}\" }\n",
        false,
    );
    assert_eq!(
        out.code, 0,
        "a literal-object const is not judgeable — no claim, no finding: {}",
        out.text
    );
    assert!(
        out.text.contains("run-time model") && out.text.contains("unjudged"),
        "the no-claim posture is named: {}",
        out.text
    );
}

/// A templated model with NO static default is UNJUDGED, and the
/// headline says so — measured 2026-07-30 before this fix: the same
/// file printed `✔ MODELS 1 model resolves in this binary` while the
/// rung had skipped its only model wholesale (nothing resolved ·
/// nobody looked — the false-green class, MODELS edition).
#[test]
fn models_rung_makes_no_claim_over_a_defaultless_run_time_model() {
    let out = checked_output(
        "models-param-runtime.nika.yaml",
        "nika: p\ninputs:\n  model: { type: string, required: true }\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 10, model: \"${{ inputs.model }}\" }\n",
        false,
    );
    assert_eq!(out.code, 0, "no claim is not a finding: {}", out.text);
    assert!(
        !out.text.contains("resolves in this binary"),
        "an unjudged model is never counted as resolving: {}",
        out.text
    );
    assert!(
        out.text.contains("run-time model") && out.text.contains("unjudged"),
        "the no-claim posture is named: {}",
        out.text
    );
}

/// `--json --native-strict`: the payload's `native_strict_clean` and
/// the exit code must agree (the review-swarm untested-branch gap).
#[test]
fn native_strict_json_payload_agrees_with_the_exit_code() {
    // net.http rides along: post-D1 the exec URL is a net USE —
    // undeclared it would be a PERMITS escape, not a hint-only file.
    let helper = "nika: helper\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
    // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
    // matrix · a dev double-run) share the OS tmpdir, and a fixed
    // name let them stomp each other's fixtures mid-read (flaked
    // live 2026-07-10 — the same fixed-temp-name class as the
    // check-expect mktemp collision, #376).
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("native-strict-json.nika.yaml");
    std::fs::write(&path, helper).expect("fixture body");
    let theme = Theme::new(false, true, false);
    let out = run(path.to_str().expect("utf8 path"), true, true, None, theme);
    assert_eq!(
        out.code, 2,
        "strict hint-only workflow exits FILE: {}",
        out.text
    );
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(
        payload["clean"],
        serde_json::json!(true),
        "spec-clean stays true"
    );
    assert_eq!(
        payload["native_strict_clean"],
        serde_json::json!(false),
        "the strict verdict rides the payload: {payload:#}"
    );
}

/// `--native-strict` promotes native-first hints to failure: the SAME
/// spec-valid workflow exits 0 by default and 2 under strict, with the
/// strict verdict naming the count; a natively-written twin stays exit
/// 0 under strict.
#[test]
fn native_strict_fails_on_native_first_hints_only() {
    // net.http rides along: post-D1 the exec URL is a net USE —
    // undeclared it would be a PERMITS escape, not a hint-only file.
    let helper = "nika: helper\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
    let default_run = checked_output("native-default.nika.yaml", helper, false);
    assert_eq!(
        default_run.code, 0,
        "advisory by default: {}",
        default_run.text
    );
    assert!(
        default_run.text.contains("[native-first]"),
        "{}",
        default_run.text
    );

    let strict = checked_output("native-strict.nika.yaml", helper, true);
    assert_eq!(
        strict.code, 2,
        "strict promotes to failure: {}",
        strict.text
    );
    assert!(
        strict.text.contains("native-strict - 1 native-first hint"),
        "{}",
        strict.text
    );

    let native_twin = "nika: native\npermits: { tools: [\"nika:fetch\"], net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://acme.test\" } }\n";
    let twin = checked_output("native-twin.nika.yaml", native_twin, true);
    assert_eq!(twin.code, 0, "the native twin passes strict: {}", twin.text);
    assert!(!twin.text.contains("native-strict ·"), "{}", twin.text);
}

/// The strict refusal must not offer a remedy that does not work.
///
/// It used to read "replace them or record them in the exec ledger",
/// and the second half was false: the gate judges the SHAPE of the
/// subprocess, so a ledgered `.py` wrapper fails exactly as hard as
/// an un-ledgered one. A reader who took the offer wrote a ledger,
/// re-ran, and met the identical red — the diagnostic spent a cycle
/// and returned nothing. This pins the honest form: name the builtin
/// as the remedy, and say what the ledger is actually for.
#[test]
fn the_strict_refusal_does_not_sell_the_ledger_as_an_escape() {
    // One line, like every other fixture here. A backslash-continued
    // string reads better but defeats the fn-length ratchet: its
    // literal stripper is line-local, so the YAML braces inside the
    // continuation count as code and the reported length runs to the
    // end of the module. Measured: this 24-line test reported as 212.
    // net.http rides along: post-D1 the exec URL is a net USE —
    // undeclared it would be a PERMITS escape, not a hint-only file.
    let ledgered = "# EXEC LEDGER ·\n# | task | command | why no native path | unlock |\n# | crawl | curl | legacy auth | nika:fetch oauth |\nnika: ledgered\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
    let out = checked_output("ledgered.nika.yaml", ledgered, true);
    assert_eq!(
        out.code, 2,
        "a ledger does not clear the strict gate: {}",
        out.text
    );
    assert!(
        !out.text.contains("or record them in the exec ledger"),
        "the refusal still offers the ledger as an alternative: {}",
        out.text
    );
    assert!(
        out.text.contains("does not clear this gate"),
        "the refusal must say what the ledger is NOT: {}",
        out.text
    );
}

/// The COST section names a DISTINCT reason per unbounded task — a deleted
/// match arm collapses one of these into the bare `unbounded` fallback, so
/// each exact phrase pins its arm: `NoTokenLimit` · `NoPrice` · `UnknownIterations`.
#[test]
fn cost_section_names_each_unbounded_reason() {
    let text = checked_text(
        "cost-reasons.nika.yaml",
        "nika: cost-reasons\ninputs:\n  items: { type: { array: string }, required: true }\ntasks:\n  a:\n    infer: { prompt: \"hi\", model: \"anthropic/claude-opus-4-20250514\" }\n  b:\n    infer: { prompt: \"hi\", model: \"ollama/llama3.1\", max_tokens: 50 }\n  c:\n    for_each: { items: \"${{ inputs.items }}\" }\n    infer: { prompt: \"x\", model: \"anthropic/claude-opus-4-20250514\", max_tokens: 10 }\n",
        true,
    );
    assert!(text.contains("no max_tokens declared"), "{text}");
    assert!(
        text.contains("no catalog price (local/unknown model)"),
        "{text}"
    );
    assert!(
        text.contains("for_each over an expression (unknown count)"),
        "{text}"
    );
}

/// `mark()` paints the verdict glyph on EVERY clean section — not just the
/// one literal verdict line. A mutated mark (returns `""` / `"xyzzy"`)
/// strips the section glyphs (count drops) or injects a placeholder.
#[test]
fn clean_report_marks_every_section() {
    let text = checked_text(
        "clean-one.nika.yaml",
        "nika: clean-one\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        false,
    );
    let ticks = text.matches('✔').count();
    assert!(
        ticks >= 5,
        "every clean section carries ✔ (got {ticks}): {text}"
    );
    assert!(
        !text.contains("xyzzy"),
        "mark never emits a placeholder: {text}"
    );
}

/// The clean verdict is the audited CARD line: tasks · waves ·
/// permits state · the cost CEILING · the hint count — with full
/// ASCII parity (`ok audited` · `<=`).
///
/// The ceiling is the point. This line used to read `est ≥$X` over
/// the cheapest-path total, which bounds nothing from below: every
/// task in that total is priced at its own token cap, so a run bills
/// under it routinely (measured: $0.000242 against `≥$0.0305`). It
/// also contradicted the COST section three lines above, which says
/// `≤N tk` per task and labels its range "worst-case output ceiling".
///
/// `est out ≤` is the second narrowing (2026-07-29). The COST section
/// says "worst-case OUTPUT ceiling · prompts, exec + mcp unpriced";
/// the card said `est ≤$X` flat, which reads as the bill. F7 measured
/// 328x on the commonest first workflow (fetch a 3.2 MB document,
/// summarise it: $2.4563 of input priced at $0.0075), so `out` is the
/// word that keeps the quoted line from meaning the whole meter.
#[test]
fn clean_verdict_is_the_audited_card_line() {
    let yaml = "nika: card\nmodel: mock/echo\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"echo\", \"bye\"] }\n";
    let text = checked_text("audited-card.nika.yaml", yaml, false);
    assert!(
            text.contains(
                "✔ audited · 2 tasks · 2 waves · permits declared · est out ≤$0.0000 · 0 hints · risk supervised"
            ),
            "the audited card line: {text}"
        );
    assert!(
        !text.contains("est ≥"),
        "the card must not claim a floor it cannot hold: {text}"
    );
    let ascii = checked_text("audited-card-ascii.nika.yaml", yaml, true);
    assert!(
        ascii.contains("ok audited") && ascii.contains("est out <=$0.0000"),
        "ascii parity (ok · <=): {ascii}"
    );
    assert!(
        !ascii.contains('≤'),
        "no unicode leaks into --ascii: {ascii}"
    );
    // Hint pluralization: 0 hints here (the boundary is declared).
    assert!(
        text.contains("0 hints") && !text.contains("0 hint·"),
        "{text}"
    );
}

/// The report must not teach a form the engine refuses.
///
/// A painted literal is an ARGUMENT to `writeln!`, not part of its
/// format string, so `{{}}` inside one is not unescaped — it reaches
/// the terminal doubled. The zero-authority PERMITS line shipped that
/// way, and `permits: {{}}` is refused by YAML itself (a mapping used
/// as a key), so one line taught an unparseable form while the HINT
/// one row below printed the right one: two lines of the same output
/// disagreeing.
///
/// The kit already carries this law for what IT teaches
/// (`the_kit_never_teaches_a_form_the_engine_refuses` in
/// `nika-onboard`). The engine's own diagnostics were outside it.
/// This closes that half, and closes the CLASS rather than the
/// instance: no doubled brace anywhere in a rendered report.
#[test]
fn the_report_never_teaches_a_doubled_brace() {
    let pure = "nika: pure\ntasks:\n  j:\n    invoke:\n      tool: \"nika:jq\"\n      args:\n        expr: \".n\"\n        input: { n: 1 }\n";
    for ascii in [false, true] {
        let text = checked_text("doubled-brace.nika.yaml", pure, ascii);
        assert!(
            text.contains("`permits: {}` states it"),
            "the zero-authority line names the form YAML accepts (ascii={ascii}): {text}"
        );
        assert!(
            !text.contains("{{") && !text.contains("}}"),
            "no doubled brace reaches the terminal (ascii={ascii}): {text}"
        );
    }
}

/// When conformance FAILS there is no valid DAG, so PLAN announces the skip
/// (gated on `!conformance.is_empty()`) — a deleted `!` would suppress the
/// line and leave the operator wondering where the plan went.
#[test]
fn plan_prints_wave_membership_with_verbs_and_targets() {
    let text = checked_text(
        "plan-membership.nika.yaml",
        "nika: w\nmodel: anthropic/claude-sonnet-5\ntasks:\n  think:\n    infer: { prompt: hi }\n  after:\n    after:\n      think: success\n    exec:\n      command: [\"echo\", \"x\"]\n",
        true,
    );
    assert!(text.contains("wave 1"), "membership renders: {text}");
    assert!(
        text.contains("think (infer - anthropic/claude-sonnet-5)"),
        "the envelope model resolves into the plan line: {text}"
    );
    assert!(
        text.contains("after (exec - echo)"),
        "argv[0] names the exec: {text}"
    );
}

#[test]
fn plan_announces_the_skip_when_conformance_fails() {
    let text = checked_text(
        "plan-skip.nika.yaml",
        "nika: bad-ref\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.nope }}\"] }\n",
        true,
    );
    assert!(
        text.contains("(skipped -- no valid DAG order while conformance fails)"),
        "{text}"
    );
}

/// PLAN was not the only DAG-gated lane — it was the only one that
/// SAID so. SECRETS and GATES read the topological waves; POLICY and
/// TRIFECTA are wrapped in an explicit `if conformance.is_empty()`
/// in `nika-check`. All four rendered `✔` on a workflow whose order
/// could not be computed, which is the false-green class at its
/// purest: the green did not mean the lane found nothing, it meant
/// nobody looked.
///
/// The fixture is the measurement (2026-07-29). A secret piped
/// straight into `exec curl` is a real `SECRETS` finding; adding ONE
/// task that depends on a name which does not exist turned that
/// finding into `✔ SECRETS no information-flow escapes`. Both halves
/// are asserted here so a regression cannot pass by making the lane
/// silent in both directions.
#[test]
fn dag_gated_lanes_announce_the_skip_instead_of_a_verdict() {
    const LEAK: &str = "nika: leak\nsecrets:\n  key: { source: env, key: K }\npermits: { exec: [\"curl\"], net: { http: [\"x.example.com\"] }, fs: { read: [\"data/**\"] } }\ntasks:\n  send:\n    with: { k: \"${{ secrets.key }}\" }\n    exec: { command: [\"curl\", \"-d\", \"${{ with.k }}\", \"https://x.example.com\"] }\n";
    let analyzable = checked_text("lanes-analyzable.nika.yaml", LEAK, false);
    assert!(
        analyzable.contains("leak into exec (task `send`)"),
        "the lane really does find this leak when the DAG resolves: {analyzable}"
    );

    // The SAME body plus one task depending on a name that does not exist.
    let broken = format!(
        "{LEAK}  ghost:\n    with: {{ z: \"${{{{ tasks.nope.output }}}}\" }}\n    exec: {{ command: [\"curl\", \"${{{{ with.z }}}}\"] }}\n"
    );
    let text = checked_text("lanes-skip.nika.yaml", &broken, false);
    for lane in ["SECRETS", "GATES", "TRIFECTA"] {
        // The placeholder makes ONE assert cover both failure shapes:
        // the lane vanished, or it printed a verdict it never computed.
        let line = text
            .lines()
            .find(|l| l.contains(lane))
            .unwrap_or("<lane absent from the report>");
        assert!(
            line.contains("(skipped — no valid DAG order while conformance fails)"),
            "{lane} must announce the skip, never a verdict it did not \
                 compute — got `{line}` in: {text}"
        );
        assert!(
            !line.contains('✔'),
            "{lane} must not carry a verdict glyph while skipped: {line}"
        );
    }
}

/// The FAILING verdict had no ASCII twin. Every section row goes
/// through `mark()`, which swaps `✖` for `X ` under `--ascii`; the
/// last line of a red report carried a hardcoded `✖`, so the one row
/// a terminal without unicode most needs to read was the one row it
/// could not. The clean card was already covered
/// (`clean_verdict_is_the_audited_card_line`); this is its red twin.
#[test]
fn the_failing_verdict_has_an_ascii_twin() {
    const BAD: &str =
        "nika: typo\ntasks:\n  t:\n    invoke: { tool: \"nika:raed\", args: { path: \"x\" } }\n";
    let uni = checked_text("verdict-unicode.nika.yaml", BAD, false);
    assert!(uni.contains("✖ findings above"), "{uni}");
    let ascii = checked_text("verdict-ascii.nika.yaml", BAD, true);
    assert!(
        ascii.contains("findings above") && !ascii.contains('✖'),
        "the failing verdict speaks ascii too: {ascii}"
    );
}

/// NIKA-DRIFT-001: a declared-but-unused envelope entry is an
/// advisory HINT — rendered code-first (the bracket voice), counted
/// in the audited card line, and the exit stays GREEN (dead
/// declarations are smell, not failure).
#[test]
fn unused_declaration_is_hinted_and_the_exit_stays_green() {
    let out = checked_output(
        "drift-unused.nika.yaml",
        "nika: w\nconst:\n  ghost: \"x\"\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        false,
    );
    assert_eq!(out.code, 0, "a drift hint never fails: {}", out.text);
    assert!(
        out.text.contains("[NIKA-DRIFT-001 - drift]"),
        "code-first bracket voice: {}",
        out.text
    );
    assert!(out.text.contains("`const.ghost`"), "{}", out.text);
    assert!(
        out.text.contains("audited") && out.text.contains("hint"),
        "the card line still renders: {}",
        out.text
    );
}

/// The machine projection law: `--json` carries the drift hint with
/// its code, `clean` stays true, and the exit stays 0.
#[test]
fn drift_hint_rides_the_json_projection() {
    // Per-PROCESS dir (the check-expect mktemp collision class, #376).
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("drift-json.nika.yaml");
    std::fs::write(
            &path,
            "nika: w\nconst:\n  ghost: \"x\"\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        )
        .expect("fixture body");
    let out = run(
        path.to_str().expect("utf8 path"),
        true,
        false,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], true, "{payload:#}");
    let hints = payload["hints"].as_array().expect("hints array");
    let drift = hints
        .iter()
        .find(|h| h["kind"] == "drift")
        .expect("the drift hint rides the machine surface");
    assert_eq!(drift["code"], "NIKA-DRIFT-001", "{drift:#}");
    assert!(
        drift["advice"]
            .as_str()
            .expect("advice")
            .contains("`const.ghost`"),
        "{drift:#}"
    );
}

/// The no-duplication law: an UNDECLARED reference is the hard
/// lane's (`NIKA-VAR-001`) — the drift code must not also fire for
/// it (the two codes never name the same site).
#[test]
fn unresolved_reference_never_also_drifts() {
    let out = checked_output(
        "drift-no-dup.nika.yaml",
        "nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.ghost }}\"] }\n",
        false,
    );
    assert_eq!(out.code, 2, "the hard lane fails: {}", out.text);
    assert!(out.text.contains("NIKA-VAR-001"), "{}", out.text);
    assert!(
        !out.text.contains("NIKA-DRIFT-001"),
        "no drift duplication: {}",
        out.text
    );
}

/// The honesty law, applied to marketing: the six lines the welcome
/// stranger reads must BE a checkable workflow, not pseudo-yaml.
/// (Lives cli-side since ADR-110: SAMPLE is the host member's, `check::run` is ours.)
#[test]
fn the_welcome_sample_is_a_real_workflow_that_checks_clean() {
    let path = std::env::temp_dir().join(format!(
        "nika-welcome-sample-{}.nika.yaml",
        std::process::id()
    ));
    std::fs::write(&path, format!("{}\n", crate::verbs::welcome::SAMPLE)).expect("sample written");
    let out = crate::verbs::check::run(
        path.to_str().expect("utf8"),
        false,
        false,
        None,
        crate::Theme::new(false, false, false),
    );
    std::fs::remove_file(&path).ok();
    assert_eq!(
        out.code,
        crate::verbs::exit::OK,
        "the welcome sample must check clean:\n{}",
        out.text
    );
}

#[test]
fn access_plan_rows_narrate_the_machine_paths() {
    // mock: keyless, compiled in — deterministic on EVERY machine (the
    // env-independent fixture class this advisory section must test on).
    let wf = parse_wf("nika: a\ntasks:\n  t:\n    infer: { prompt: hi, model: \"mock/echo\" }\n");
    let rows = models_rung::access_plan_rows(&nika_check::check(&wf));
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(row["model"], "mock/echo");
    assert_eq!(row["resolved"], true);
    assert_eq!(row["access"], "mock");
    assert_eq!(row["chosen"], "mock");
    assert_eq!(row["billing"], "local");
    assert_eq!(row["pinned"], false);

    // ollama: keyless local — `configured` holds on every machine, so
    // the chosen class is deterministic (liveness is the RUN's business).
    let wf =
        parse_wf("nika: b\ntasks:\n  t:\n    infer: { prompt: hi, model: \"ollama/llama3.2\" }\n");
    let rows = models_rung::access_plan_rows(&nika_check::check(&wf));
    assert_eq!(rows[0]["resolved"], true);
    assert_eq!(rows[0]["chosen"], "local");
    assert_eq!(rows[0]["billing"], "local");

    // A templated `model:` is not a static fact — never judged here.
    let wf = parse_wf(
        "nika: c\nconst:\n  m: { default: \"mock/echo\" }\ntasks:\n  t:\n    infer: { prompt: hi, model: \"${{ const.m }}\" }\n",
    );
    assert!(
        models_rung::access_plan_rows(&nika_check::check(&wf)).is_empty(),
        "a templated model must stay unjudged"
    );
}

#[test]
fn a_catalog_warning_speaks_exactly_once_per_model() {
    // The duplicated advisory block (pre-2026-08-05) doubled every row —
    // one model with a catalog miss must yield ONE warning.
    let wf = parse_wf(
        "nika: w\ntasks:\n  t:\n    infer: { prompt: hi, model: \"anthropic/claude-never-heard-of-it\" }\n",
    );
    let report = nika_check::check(&wf);
    let audit = unresolvable_models(&report, &wf);
    assert_eq!(
        audit.catalog_warnings.len(),
        1,
        "one miss, one warning — never a doubled row"
    );
}

/// #774 · the determinism pin. `check --infer-permits` is a STATIC
/// audit — it has no filesystem. The issue's darwin-vs-ubuntu skew
/// traced to two binaries that only LOOKED like one release (no build
/// stamp), but the audit's own contract is that the machine's disk
/// cannot enter the output. Same workflow, one literal `nika:read`
/// path arg — once with the named file on disk, once with it absent
/// (the bare-cwd half of the repro): the bytes must be identical.
#[test]
fn infer_permits_output_cannot_depend_on_the_disk() {
    let dir = std::env::temp_dir().join(format!("nika-i774-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let target = dir.join("news.json");
    let path = dir.join("pin.nika.yaml");
    std::fs::write(
        &path,
        format!(
            "nika: pin\ntasks:\n  read:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{}\" }}\n",
            target.display()
        ),
    )
    .expect("fixture body");
    let wf = path.to_str().expect("utf8 path");

    std::fs::write(&target, "{}").expect("referenced file present");
    let present = run_infer_permits(wf, false);
    let present_json = run_infer_permits(wf, true);
    std::fs::remove_file(&target).expect("referenced file absent");
    let absent = run_infer_permits(wf, false);
    let absent_json = run_infer_permits(wf, true);

    assert_eq!(present.code, 0, "{}", present.text);
    // The fixture really names the read path — the pin is not vacuous.
    assert!(present.text.contains("news.json"), "{}", present.text);
    assert_eq!(
        present.text, absent.text,
        "the plain bytes are filesystem-independent"
    );
    assert_eq!(
        present_json.text, absent_json.text,
        "the json bytes are filesystem-independent"
    );
}

/// #774 · the provenance pair rides every `--json` report: the bare
/// crate version under the run journal's `engine_version` key and the
/// compile-time commit stamp beside it — additive siblings, the
/// `report_version: 1` contract untouched.
#[test]
fn check_json_carries_the_build_provenance_pair() {
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("provenance.nika.yaml");
    std::fs::write(
        &path,
        "nika: w\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
    )
    .expect("fixture body");
    let out = run(
        path.to_str().expect("utf8 path"),
        true,
        false,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["engine_version"], env!("CARGO_PKG_VERSION"));
    let sha = payload["build_sha"].as_str().expect("build_sha string");
    assert!(!sha.is_empty(), "a stamp always rides: {payload:#}");
    assert_eq!(
        sha,
        env!("NIKA_BUILD_SHA"),
        "the payload IS the compile-time stamp: {payload:#}"
    );
}

/// #774 · the `--version` stamp contract: the bare version stays the
/// FIRST whitespace token (the harness probe · the nix smoke · the
/// kit's version handshake all parse it positionally) and a known
/// commit stamps `(<sha>[-dirty])` after it; `unknown` leaves the long
/// form byte-identical to the bare version.
#[test]
fn the_long_version_keeps_the_bare_version_first() {
    let long = env!("NIKA_VERSION_LONG");
    let first = long.split_whitespace().next().expect("a version token");
    assert_eq!(first, env!("CARGO_PKG_VERSION"), "{long}");
    let sha = env!("NIKA_BUILD_SHA");
    assert!(!sha.is_empty(), "build.rs always emits a stamp");
    if sha == "unknown" {
        assert_eq!(long, env!("CARGO_PKG_VERSION"), "{long}");
    } else {
        assert_eq!(long, format!("{} ({sha})", env!("CARGO_PKG_VERSION")));
    }
}
