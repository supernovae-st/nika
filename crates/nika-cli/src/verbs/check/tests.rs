use super::*;
use nika_cli_host::models_rung::pricing_section;

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

pub(crate) fn parse_wf(yaml: &str) -> RawWorkflow {
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
pub(crate) fn checked_text(name: &str, yaml: &str, ascii: bool) -> String {
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

/// The one-voice guard · every wire code the `--json` lane stamps in
/// `findings[]` must appear in the human lane's text for the SAME file.
/// A finding the wire carries and the terminal hides is the
/// mute-diagnostic class (`✖ findings above` pointing at nothing) — the
/// operator who cannot query JSON is the one who gets no reason.
/// Returns the human text so the caller can assert its rows too.
pub(crate) fn assert_every_wire_code_renders(name: &str, yaml: &str) -> String {
    let dir = std::env::temp_dir().join(format!("nika-cli-onevoice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, yaml).expect("fixture body");
    let path = path.to_str().expect("utf8 path");
    let theme = Theme::new(false, true, false);
    let machine = run(path, true, false, None, theme).text;
    let human = run(path, false, false, None, theme).text;
    let payload: serde_json::Value = serde_json::from_str(&machine).expect("json lane parses");
    let codes: Vec<String> = payload["findings"]
        .as_array()
        .expect("findings[] is an array")
        .iter()
        .filter_map(|f| f["code"].as_str().map(str::to_owned))
        .collect();
    assert!(
        !codes.is_empty(),
        "the fixture must carry at least one wire code: {machine}"
    );
    for code in &codes {
        assert!(
            human.contains(code.as_str()),
            "the wire stamps {code} and the human lane never prints it (mute diagnostic): {human}"
        );
    }
    human
}

/// The order law (spec 10 · NIKA-SEC-015) refused in the wire and said
/// NOTHING in the human lane — measured 2026-08-19 on the published
/// 0.109.2: a beat workflow whose `exec: sleep 3` etiquette gap sat one
/// `after:` control edge downstream of a `nika:fetch` exited rc=2 with
/// every row green and `✖ findings above` pointing at nothing;
/// `check_render.rs` read no `order_findings`. Now an ORDER row, always
/// present like WRITES (a universal static law reads as one), carries
/// the code and the route.
#[test]
fn order_law_renders_its_row_in_the_human_lane() {
    const GAP_AFTER_FETCH: &str = "nika: beat\npermits:\n  exec: [\"sleep\"]\n  tools: [\"nika:fetch\"]\n  net:\n    http: [\"export.arxiv.org\"]\ntasks:\n  pull:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://export.arxiv.org/api/query?search_query=nika\", mode: text } }\n  gap:\n    after: { pull: success }\n    exec: { command: [\"sleep\", \"3\"] }\n";
    // The same file with the gap BESIDE the fetch (no path from a
    // net-effecting task to the shell) · the row is green and says what
    // it looked at — the repair the finding teaches, rendered.
    const GAP_BESIDE_FETCH: &str = "nika: beat\npermits:\n  exec: [\"sleep\"]\n  tools: [\"nika:fetch\"]\n  net:\n    http: [\"export.arxiv.org\"]\ntasks:\n  pull:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://export.arxiv.org/api/query?search_query=nika\", mode: text } }\n  gap:\n    exec: { command: [\"sleep\", \"3\"] }\n";
    let text = assert_every_wire_code_renders("order-gap-after-fetch.nika.yaml", GAP_AFTER_FETCH);
    let row = text
        .lines()
        .find(|l| l.contains("ORDER"))
        .unwrap_or("<ORDER row absent from the report>");
    assert!(
        row.contains("[NIKA-SEC-015]") && row.contains("`gap`") && row.contains("`pull`"),
        "the ORDER row names the code, the sink and the source: `{row}` in: {text}"
    );
    assert!(
        !row.contains("✔"),
        "a refused route is never a green row: {row}"
    );

    let text = checked_text("order-gap-beside-fetch.nika.yaml", GAP_BESIDE_FETCH, true);
    let row = text
        .lines()
        .find(|l| l.contains("ORDER"))
        .unwrap_or("<ORDER row absent from the report>");
    assert!(
        row.contains("no exec: sits downstream of a net-effecting task"),
        "the universal law renders its green like WRITES does: `{row}` in: {text}"
    );
}

/// The authored doors rule 6 (spec 10 · NIKA-AUTH-011) had the same gap:
/// the wire stamped the code, the human lane had no LIFT row to print it
/// in. The row renders only when a task declares `lift:` — a file with
/// no door renders unchanged.
#[test]
fn idle_door_renders_its_row_in_the_human_lane_and_a_doorless_file_has_no_row() {
    // `inputs.p` is declared and lifted, but the task never reads it ·
    // the door guards an empty room.
    const IDLE_DOOR: &str = "nika: doors\ninputs:\n  p: { type: string, default: \"x\" }\npermits:\n  exec: [\"echo\"]\ntasks:\n  say:\n    lift:\n      - { law: taint, from: inputs.p, because: \"reviewed 2026-08-19\" }\n    exec: { command: [\"echo\", \"hello\"] }\n";
    const NO_DOOR: &str = "nika: plain\npermits:\n  exec: [\"echo\"]\ntasks:\n  say:\n    exec: { command: [\"echo\", \"hello\"] }\n";
    let text = assert_every_wire_code_renders("lift-idle-door.nika.yaml", IDLE_DOOR);
    let row = text
        .lines()
        .find(|l| l.contains("LIFT"))
        .unwrap_or("<LIFT row absent from the report>");
    assert!(
        row.contains("[NIKA-AUTH-011]") && row.contains("`say`") && row.contains("fix:"),
        "the LIFT row names the code, the task and the repair: `{row}` in: {text}"
    );

    let text = checked_text("lift-no-door.nika.yaml", NO_DOOR, true);
    assert!(
        !text.lines().any(|l| l.contains("LIFT")),
        "a file with no authored door renders no LIFT row: {text}"
    );
}

/// Same fixture plumbing, full `VerbOutput` (exit-code assertions) —
/// the `--native-strict` posture tests read `.code`.
pub(crate) fn checked_output(name: &str, yaml: &str, native_strict: bool) -> VerbOutput {
    checked_output_profile(name, yaml, native_strict, Profile::Advisory)
}

/// The posture-parameterized twin of [`checked_output`] — the
/// `--profile operational` readiness-gate tests read `.code`.
pub(crate) fn checked_output_profile(
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
        (None, None),
        theme,
    )
}

/// `--json` twin of a fixture already written by [`checked_output`].
pub(crate) fn checked_json(name: &str) -> (VerbOutput, serde_json::Value) {
    let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
    let path = dir.join(name);
    let theme = Theme::new(false, true, false);
    let out = run(path.to_str().expect("utf8 path"), true, false, None, theme);
    let payload = serde_json::from_str(&out.text).expect("json");
    (out, payload)
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
    // #761: cataloged-but-unresolvable is engine-local — no spec code.
    let (json_out, payload) = checked_json("models-azure.nika.yaml");
    assert_eq!(json_out.code, 2, "{}", json_out.text);
    assert_eq!(payload["model_findings"][0]["model"], "azure/gpt-4o");
    assert!(
        payload["model_findings"][0].get("code").is_none(),
        "azure class stays engine-local: {payload:#}"
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
        (None, None),
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
        "nika: h\nmodel: anthropic/claude-sonnet-4-6\npermits:\n  tools: [\"nika:*\"]\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 256 }\n",
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
        "nika: l\nmodel: anthropic/claude-sonnet-4-6\npermits: {}\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 256 }\n",
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
    let code = payload["model_findings"][0]["code"].as_str().unwrap_or("");
    assert!(
        code.contains("NIKA-PROVIDER"),
        "bare id is the FORM law: {payload:#}"
    );
    let row = &payload["pricing"]["models"][0];
    assert!(
        row["input_per_million"].is_null() && row["output_per_million"].is_null(),
        "an unresolvable model is never priced: {row:#}"
    );
}

/// #761: an unknown provider prefix is the same FORM law as a bare id
/// — `check --json` stamps `NIKA-PROVIDER` so the spec harness can match.
#[test]
fn models_rung_stamps_nika_provider_on_an_unknown_prefix() {
    let out = checked_output(
        "models-unknown-prefix.nika.yaml",
        "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"not-a-provider/gpt-4\" }\n",
        false,
    );
    assert_eq!(out.code, 2, "unknown prefix is a finding: {}", out.text);
    let (json_out, payload) = checked_json("models-unknown-prefix.nika.yaml");
    assert_eq!(json_out.code, 2, "{}", json_out.text);
    assert_eq!(
        payload["model_findings"][0]["model"], "not-a-provider/gpt-4",
        "{payload:#}"
    );
    let code = payload["model_findings"][0]["code"].as_str().unwrap_or("");
    assert!(
        code.contains("NIKA-PROVIDER"),
        "unknown prefix is the FORM law: {payload:#}"
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
        "nika: p\nconst:\n  model: \"anthropic/claude-sonnet-4-6\"\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 256, model: \"${{ const.model }}\" }\n",
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
#[cfg(test)]
mod verdict_profiles;

/// #1404 — `check --help` carries its exit contract: CI branches on the
/// code alone, and the FILE/ENVIRONMENT split is spoken (a missing file
/// is 3, a grammar refusal or findings are 2).
#[test]
fn the_check_help_carries_the_exit_contract() {
    use clap::Args as _;
    let help = CheckArgs::augment_args(clap::Command::new("check"))
        .render_long_help()
        .to_string();
    assert!(help.contains("exit codes"), "{help}");
    assert!(
        help.contains("0 the report holds")
            && help.contains("2 the FILE")
            && help.contains("3 the ENVIRONMENT"),
        "the three classes check can exit with: {help}"
    );
    assert!(
        help.contains("never 1 or 4"),
        "the run-only classes are named as such: {help}"
    );
}
