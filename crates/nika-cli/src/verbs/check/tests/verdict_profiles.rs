// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

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

#[test]
fn json_keeps_each_native_hint_site_with_its_stable_code() {
    let yaml = "nika: sites\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  first:\n    exec: { command: [\"curl\", \"https://acme.test/a\"] }\n  second:\n    exec: { command: [\"curl\", \"https://acme.test/b\"] }\n";
    let dir = std::env::temp_dir().join(format!("nika-cli-hint-sites-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("sites.nika.yaml");
    std::fs::write(&path, yaml).expect("fixture body");
    let out = run(
        path.to_str().expect("utf8 path"),
        true,
        false,
        None,
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 0, "advisory sites stay clean: {}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    let sites: Vec<(&str, &str)> = payload["hints"]
        .as_array()
        .expect("hints")
        .iter()
        .filter(|hint| hint["code"] == "native-first/001")
        .map(|hint| {
            (
                hint["task"].as_str().expect("task"),
                hint["code"].as_str().expect("code"),
            )
        })
        .collect();
    assert_eq!(
        sites,
        vec![
            ("first", "native-first/001"),
            ("second", "native-first/001")
        ],
        "machine output retains per-site findings: {payload:#}"
    );
}

/// `--json` always carries `paid_ready`. Hints never fail `clean` or
/// the exit code — the field is the question an agent reads after
/// `--native-strict` is green.
#[test]
fn json_payload_names_paid_ready_without_failing_clean() {
    let judge = "nika: w\nmodel: mock/echo\ntasks:\n  judge:\n    infer:\n      prompt: |\n        Read the note and assign a belt.\n      max_tokens: 32\noutputs:\n  r: ${{ tasks.judge.output }}\n";
    let dir = std::env::temp_dir().join(format!("nika-cli-paidready-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("paid-ready.nika.yaml");
    std::fs::write(&path, judge).expect("fixture body");
    let theme = Theme::new(false, true, false);
    let out = run(path.to_str().expect("utf8 path"), true, false, None, theme);
    assert_eq!(
        out.code, 0,
        "paid-run hints stay advisory on the CLI: {}",
        out.text
    );
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
    assert_eq!(payload["clean"], serde_json::json!(true), "{payload:#}");
    assert_eq!(
        payload["paid_ready"],
        serde_json::json!(false),
        "{payload:#}"
    );
    let kinds: Vec<&str> = payload["paid_blockers"]
        .as_array()
        .expect("paid_blockers")
        .iter()
        .filter_map(|r| r["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"infer-as-law"), "{payload:#}");
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
        default_run.text.contains("[native-first/001]"),
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
                "✔ audited · 2 tasks · 2 waves · permits exec:echo · est out ≤$0.0000 · 0 hints · risk supervised"
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
    for lane in ["SECRETS", "GATES", "TRIFECTA", "ORDER"] {
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

/// The typed engine identity rides every `--json` report while the
/// independent `report_version` schema contract remains untouched.
#[test]
fn check_json_carries_the_typed_engine_identity() {
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
    let identity = nika_runtime::engine_identity();
    assert_eq!(payload["engine_version"], identity.engine_version());
    let sha = payload["build_sha"].as_str().expect("build_sha string");
    assert!(!sha.is_empty(), "a stamp always rides: {payload:#}");
    assert_eq!(
        sha,
        identity.build_sha(),
        "the payload IS the compile-time stamp: {payload:#}"
    );
    assert_eq!(payload["spec_sha"], identity.spec_sha());
    assert_eq!(payload["api_version"], identity.api_version());
    assert_eq!(payload["report_version"], nika_check::REPORT_VERSION);
    assert_eq!(payload["engineVersion"], identity.engine_version());
    assert_eq!(payload["buildSha"], identity.build_sha());
    assert_eq!(payload["specSha"], identity.spec_sha());
    assert_eq!(
        payload["machineProtocolVersion"],
        identity.machine_protocol_version()
    );
    assert_eq!(
        payload["snapshotFormatVersion"],
        identity.snapshot_format_version()
    );
    assert_eq!(
        payload["checkReportVersion"],
        identity.check_report_version()
    );
    assert_eq!(
        payload["eventFormatVersion"],
        identity.event_format_version()
    );
    assert_eq!(
        payload["traceFormatVersion"],
        identity.trace_format_version()
    );
    assert_eq!(
        payload["supportedCapabilities"],
        serde_json::json!(identity.supported_capabilities())
    );
    assert!(
        payload.get("execution_snapshot").is_none(),
        "ordinary --json must never carry snapshot bytes: {payload:#}"
    );
    assert_eq!(
        nika_runtime::MACHINE_SNAPSHOT_FORMAT_VERSION,
        nika_execution::SNAPSHOT_FORMAT_VERSION,
        "the sibling L3 format clocks must stay in parity"
    );
}

#[test]
fn snapshot_export_is_explicit_machine_only_and_round_trips() {
    let dir = std::env::temp_dir().join(format!("nika-cli-snapshot-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("snapshot-export.nika.yaml");
    std::fs::write(
        &path,
        "nika: snapshot-export\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n",
    )
    .expect("fixture body");

    let out = run_snapshot_export(
        path.to_str().expect("utf8 path"),
        Theme::new(false, true, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
    let payload: serde_json::Value = serde_json::from_str(&out.text).expect("machine json");
    let encoded = payload["execution_snapshot"]
        .as_str()
        .expect("opt-in snapshot string");
    let snapshot = nika_execution::ExecutionSnapshot::decode(encoded).expect("decode export");
    let root = snapshot.root().to_owned();
    let admitted = nika_execution::ExecutionService::default()
        .readmit_snapshot(snapshot)
        .expect("readmit export");

    assert_eq!(root, "snapshot-export.nika.yaml");
    assert_eq!(admitted.snapshot().root(), root);
    assert_eq!(
        payload["snapshotFormatVersion"],
        nika_execution::SNAPSHOT_FORMAT_VERSION
    );
}

/// #774 · the `--version` stamp contract: the bare version stays the
/// FIRST whitespace token (the harness probe · the nix smoke · the
/// kit's version handshake all parse it positionally) and a known
/// commit stamps `(<sha>[-dirty])` after it; `unknown` leaves the long
/// form byte-identical to the bare version.
#[test]
fn the_long_version_keeps_the_bare_version_first() {
    let identity = nika_runtime::engine_identity();
    let long = identity.version_long();
    let first = long.split_whitespace().next().expect("a version token");
    assert_eq!(first, identity.engine_version(), "{long}");
    let sha = identity.build_sha();
    assert!(!sha.is_empty(), "build.rs always emits a stamp");
    if sha == "unknown" {
        assert_eq!(long, identity.engine_version(), "{long}");
    } else {
        assert_eq!(long, format!("{} ({sha})", identity.engine_version()));
    }
}

/// The naming note fires on the ACCIDENT and stays silent on the
/// deliberate — and it never touches the verdict.
///
/// The accidental shape is a copy: `bar.nika.yaml` still carrying
/// `nika: foo`, so every trace and journal event says `foo` while the
/// file says `bar`. The deliberate shapes are the ordering prefix (the
/// numbered teaching path) and plain agreement — 62 of this house's 80
/// workflow files agree, and all 18 divergences are the prefix.
#[test]
fn the_naming_note_fires_on_a_copy_and_not_on_an_ordering_prefix() {
    let wf = parse_wf(
        "nika: foo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
    );
    let theme = Theme::new(false, true, false);

    let mut accident = String::new();
    naming_note(&mut accident, theme, "bar.nika.yaml", &wf);
    assert!(accident.contains("`bar`"), "the file: {accident}");
    assert!(accident.contains("`foo`"), "the name: {accident}");

    for deliberate in ["01-foo.nika.yaml", "17_foo.nika.yaml", "foo.nika.yaml"] {
        let mut out = String::new();
        naming_note(&mut out, theme, deliberate, &wf);
        assert!(out.is_empty(), "{deliberate} must stay silent: {out}");
    }

    // A path that is not a workflow filename at all says nothing.
    let mut other = String::new();
    naming_note(&mut other, theme, "-", &wf);
    assert!(other.is_empty(), "stdin has no stem: {other}");
}
