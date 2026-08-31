mod trifecta_rung_tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn console(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    /// A complete lethal trifecta whose ONLY mitigation is a confirm gate
    /// on a bare state edge: the refusal settles `success`, the edge
    /// admits it, the effect fires on 'no'.
    const RUBBER_STAMP: &str = "\
nika: t
permits:
  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }
  net: { http: [\"api.example.com\"] }
  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]
tasks:
  ask:
    invoke:
      tool: \"nika:prompt\"
      args: { mode: \"confirm\", message: \"exfiltrate?\" }
  fetch_page:
    after: { ask: success }
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/data\" }
  leak:
    after: { fetch_page: success }
    with: { body: \"${{ tasks.fetch_page.output }}\" }
    invoke:
      tool: \"nika:write\"
      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }
";

    /// The same file with the answer BOUND and gated on — the shape the
    /// consent lane's own fix teaches.
    const REAL_GATE: &str = "\
nika: t
permits:
  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }
  net: { http: [\"api.example.com\"] }
  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]
tasks:
  ask:
    invoke:
      tool: \"nika:prompt\"
      args: { mode: \"confirm\", message: \"exfiltrate?\" }
  fetch_page:
    with: { go: \"${{ tasks.ask.output }}\" }
    when: ${{ with.go == true }}
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/data\" }
  leak:
    with: { go: \"${{ tasks.ask.output }}\", body: \"${{ tasks.fetch_page.output }}\" }
    when: ${{ with.go == true }}
    invoke:
      tool: \"nika:write\"
      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }
";

    /// MEASURED 2026-08-20 on the shipped 0.111.0 · one file, one card,
    /// two lanes with opposite verdicts about the SAME gate ·
    ///
    /// ```text
    /// ✔ TRIFECTA no lethal trifecta over the declared permits: without a human gate
    /// ✖ CONSENT  [NIKA-SEC-014] task `leak` … the effect fires on 'no'
    /// ```
    ///
    /// A CONTROL run (the same file with the prompt deleted) fires
    /// `NIKA-SEC-009`, so the trifecta is COMPLETE and the tick was
    /// bought entirely by the gate the consent row refutes four lines
    /// lower. The tick is now derived, so it cannot stand there.
    #[test]
    fn the_trifecta_tick_never_stands_on_a_gate_consent_refutes() {
        let out = console(RUBBER_STAMP);
        assert!(
            out.contains("NIKA-SEC-014"),
            "the consent lane must fire on this file:\n{out}"
        );
        assert!(
            !out.contains("✔ TRIFECTA"),
            "the tick cannot stand on a refuted gate:\n{out}"
        );
        assert!(
            out.contains("NIKA-SEC-014 below") || out.contains("refused below"),
            "the lane must hand the reader to the code that owns the repair:\n{out}"
        );
        assert!(
            !out.contains("NIKA-SEC-009"),
            "one defect, one code — consent already owns it:\n{out}"
        );
    }

    /// The guard: a gate that really closes keeps its tick. Reddening
    /// this file would be the cancelled kind of change.
    #[test]
    fn a_gate_that_closes_the_route_keeps_its_tick() {
        let out = console(REAL_GATE);
        assert!(
            !out.contains("NIKA-SEC-014"),
            "the bound-and-gated shape is the consent lane's own fix:\n{out}"
        );
        assert!(
            out.contains("✔ TRIFECTA"),
            "a real gate must still clear the trifecta:\n{out}"
        );
    }

    /// The other silence: a file with no trifecta at all keeps its tick,
    /// whatever the consent lane says elsewhere. A missing leg and a
    /// credited gate are different facts and must render differently.
    #[test]
    fn a_file_with_no_trifecta_keeps_its_tick() {
        let out = console(
            "nika: t\nmodel: mock/echo\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"go?\" }\n",
        );
        assert!(
            out.contains("✔ TRIFECTA"),
            "no legs, no trifecta, no reason to withhold:\n{out}"
        );
    }

    /// The complete trifecta plus a live `lift: taint` on the egress —
    /// AUTH-011 is satisfied (the binding reaches the task), SEC-009 still
    /// refuses. The hatch the author reached for is the other law.
    const LIFT_BESIDE_TRIFECTA: &str = "\
nika: t
permits:
  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }
  net: { http: [\"api.example.com\"] }
  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]
tasks:
  fetch_page:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/data\" }
  leak:
    after: { fetch_page: success }
    with: { body: \"${{ tasks.fetch_page.output }}\" }
    lift:
      - law: taint
        from: with.body
        because: \"first-party queryset, reviewed at authoring\"
    invoke:
      tool: \"nika:write\"
      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }
";

    /// MEASURED on #1065 · a valid `lift: taint` door ticks green beside
    /// the lethal trifecta it looks like it should open. The taint door
    /// is AUTH-011 (permit-parameterization); SEC-009's only door is a
    /// blocking `nika:prompt`. Two readers independently concluded the
    /// hatch was inert because neither line named the other law.
    ///
    /// The LIFT line stays green — AUTH-011 is satisfied — and now names
    /// that this door does not open SEC-009. Deleting the connecting
    /// clause fails this test.
    #[test]
    fn a_valid_lift_door_names_that_it_does_not_open_the_trifecta() {
        let out = console(LIFT_BESIDE_TRIFECTA);
        assert!(
            out.contains("✖ TRIFECTA") && out.contains("NIKA-SEC-009"),
            "the trifecta must still refuse:\n{out}"
        );
        let lift = out
            .lines()
            .find(|l| l.contains("LIFT"))
            .unwrap_or("<LIFT row absent from the report>");
        assert!(
            lift.contains('✔') && !lift.contains("NIKA-AUTH-011"),
            "AUTH-011 is satisfied — the door stays green:\n{lift}\nin:\n{out}"
        );
        assert!(
            lift.contains("SEC-009") || lift.contains("does not open") || lift.contains("prompt"),
            "the connecting clause must sit on the LIFT line so a green \
             tick beside SEC-009 cannot read as an inert hatch:\n{lift}\nin:\n{out}"
        );
    }
}

mod hint_dedup_tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn rendered_as(yaml: &str, path: &str, repair_target: RepairTarget) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            path,
            repair_target,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    fn rendered(yaml: &str, path: &str) -> String {
        let repair_target = if path == "-" {
            RepairTarget::Stdin
        } else {
            RepairTarget::WorkspaceFile
        };
        rendered_as(yaml, path, repair_target)
    }

    #[test]
    fn repeated_hint_code_renders_once_with_site_count_and_next_command() {
        let yaml = r"
nika: repeated
permits:
  exec: [curl]
  net: { http: [example.com] }
tasks:
  first:
    exec: { command: [curl, https://example.com/a] }
  second:
    exec: { command: [curl, https://example.com/b] }
";
        let out = rendered(yaml, "repeated.nika.yaml");
        assert_eq!(
            out.matches("[native-first/001]").count(),
            1,
            "one diagnostic body per stable code:\n{out}"
        );
        assert!(out.contains("2 sites across 2 tasks"), "{out}");
        assert!(out.contains("1 distinct hint across 2 sites"), "{out}");
        // The footer gives the next REAL command. `native-first` is not in
        // the fix ladder, so for this file that command is `nika explain`
        // and never `--fix` — which would change nothing and print this
        // same line again (#1182).
        assert!(out.contains("nika explain"), "{out}");
        assert!(
            !out.contains("--fix"),
            "no repair is machine-applicable here, so none is advised:\n{out}"
        );
    }

    /// #1182 · `--fix` used to be advised on any file carrying a hint,
    /// which is a different set from the files `--fix` can repair. A file
    /// whose findings are all hints got « no machine-applicable repairs »
    /// at the top of a `--fix` run and « run `--fix` » at the bottom of the
    /// same output — a fixed point advertising itself as the exit.
    #[test]
    fn a_file_with_no_typed_rename_is_never_sent_to_fix() {
        let hints_only = "nika: drifty
permits: { exec: [curl], net: { http: [example.com] } }
tasks:
  t:
    exec: { command: [curl, https://example.com/a] }
";
        let out = rendered(hints_only, "drifty.nika.yaml");
        assert!(out.contains("NEXT"), "the footer still speaks:\n{out}");
        assert!(
            !out.contains("--fix"),
            "a hint the ladder cannot touch must not route to `--fix`:\n{out}"
        );

        // The other end, so this cannot pass by never advising anything: a
        // typed rename IS machine-applicable, and keeps its advice.
        let renameable = "nika: fixable
permits: { tools: [nika:read], exec: [curl], net: { http: [example.com] } }
tasks:
  t:
    exec: { command: [curl, https://example.com/a] }
  u:
    invoke: { tool: nika:raed, args: { path: ./x } }
";
        let out = rendered(renameable, "fixable.nika.yaml");
        assert!(
            out.contains("nika check --fix fixable.nika.yaml"),
            "a typed rename still earns the advice:\n{out}"
        );
    }

    #[test]
    fn heterogeneous_advice_keeps_rows_but_footer_counts_distinct_codes() {
        let yaml = r"
nika: heterogeneous
permits: { exec: [date, sha256sum] }
tasks:
  clock:
    exec: { command: [date] }
  digest:
    exec: { command: [sha256sum, input.txt] }
";
        let out = rendered(yaml, "heterogeneous.nika.yaml");
        assert_eq!(
            out.matches("[native-first/006]").count(),
            2,
            "distinct advice variants both render:\n{out}"
        );
        assert!(out.contains("1 distinct hint across 2 sites"), "{out}");
        assert!(!out.contains("2 distinct hints"), "{out}");
    }

    #[test]
    fn next_command_quotes_paths_and_never_offers_fix_for_stdin() {
        // The fixture needs BOTH halves the NEXT line is gated on: a hint
        // (so the line renders at all) and a typed rename (so advising
        // `--fix` is true). `nika:raed` carries the suggestion; `date`
        // carries the native-first hint. This test is about the QUOTING of
        // the path in that advice — it needs the advice to be honest first.
        let yaml = "nika: q
permits: { exec: [date], tools: [nika:read] }
tasks:
  t:
    exec: { command: [date] }
  u:
    invoke: { tool: nika:raed, args: { path: ./x } }
";
        let spaced = rendered(yaml, "my workflow.nika.yaml");
        assert!(
            spaced.contains("nika check --fix 'my workflow.nika.yaml'"),
            "{spaced}"
        );
        let apostrophe = rendered(yaml, "it's ready.nika.yaml");
        assert!(
            apostrophe.contains("nika check --fix 'it'\"'\"'s ready.nika.yaml'"),
            "{apostrophe}"
        );
        let stdin = rendered(yaml, "-");
        assert!(!stdin.contains("--fix -"), "{stdin}");
        assert!(
            stdin.contains("save stdin to a file") && stdin.contains("nika check --fix <file>"),
            "{stdin}"
        );

        let dashed = rendered(yaml, "-workflow.nika.yaml");
        assert!(
            dashed.contains("nika check --fix -- -workflow.nika.yaml"),
            "a positional beginning with '-' needs clap's end-of-options separator:\n{dashed}"
        );

        let cache = rendered_as(
            yaml,
            "/home/operator/.nika/registry/acme/report/1.0.0/workflow.nika.yaml",
            RepairTarget::RegistryArtifact,
        );
        assert!(
            cache.contains("copy the registry artifact into your workspace")
                && cache.contains("nika check --fix <copy>"),
            "{cache}"
        );
        assert!(
            !cache.contains("--fix /home/operator/.nika/registry"),
            "the resolved cache path is never writable guidance:\n{cache}"
        );

        let stream = rendered_as(yaml, "/dev/stdin", RepairTarget::NonRegularSource);
        assert!(
            stream.contains("save or copy this non-regular source")
                && stream.contains("nika check --fix <copy>"),
            "{stream}"
        );
        assert!(
            !stream.contains("--fix /dev/stdin"),
            "a device path is never writable guidance:\n{stream}"
        );
    }
}

mod journey_rung_tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn console(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    /// The trivial voyage, stated honestly: a pure mock-compute workflow
    /// renders the JOURNEY line with its class and counts — never a
    /// dressed-up claim.
    #[test]
    fn a_pure_mock_workflow_states_its_trivial_journey() {
        let out = console(
            "nika: t\nmodel: mock/echo\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        let line = out
            .lines()
            .find(|l| l.contains("JOURNEY"))
            .expect("the JOURNEY rung renders");
        assert!(line.contains("internal"), "the class: {line}");
        assert!(
            line.contains("no secret reaches an external destination"),
            "the honest closure: {line}"
        );
    }

    /// A secret flowing to a cloud endpoint is NAMED on an explicit ⚠
    /// row — advisory (the sanctioned egress stays clean; the SECRETS
    /// lane owns the unsanctioned refusal), with the consent review riding.
    #[test]
    fn a_secret_reaching_a_cloud_endpoint_is_named_with_the_receipt_law() {
        let out = console(
            r#"
    nika: t
    model: openai/gpt-4o-mini
    secrets:
      openai_key:
        source: env
        key: OPENAI_API_KEY
        egress: [{ to: infer }]
    permits: {}
    tasks:
      send:
        infer:
          prompt: "auth ${{ secrets.openai_key }}"
          max_tokens: 10
    "#,
        );
        let row = out
            .lines()
            .find(|l| l.contains("JOURNEY") && l.contains("flows to"))
            .expect("the ⚠ flow row");
        assert!(
            row.contains("secret `openai_key` flows to openai"),
            "the flow is NAMED: {row}"
        );
        assert!(row.contains('⚠'), "advisory warn mark: {row}");
        assert!(
            row.contains("review consent before the run"),
            "the review law rides: {row}"
        );
        // Advisory, never a finding: the sanctioned workflow's verdict
        // stays green and no ✖ rides the JOURNEY rung.
        assert!(
            !out.lines()
                .any(|l| l.contains("JOURNEY") && l.contains('✖')),
            "no blocking row on the rung:\n{out}"
        );
        assert!(out.contains("audited"), "the verdict stays clean:\n{out}");
    }

    /// #1041: sanctioning a secret into `agent:` is consent, not absence.
    /// SECRETS must not say « no declared secret reaches an effect » and
    /// JOURNEY must name the http allowlist the agent can reach.
    #[test]
    fn a_sanctioned_agent_secret_flow_is_visible_on_both_rungs() {
        let out = console(
            r#"
nika: q2-agent-sanctioned
model: mock/echo
secrets:
  TOKEN:
    source: env
    key: BUILD_TOKEN
    egress: [{ to: "agent" }]
permits:
  net:
    http: ["untrusted.example.com", "evil.example.org"]
  tools: [nika:fetch, nika:done]
tasks:
  do_it:
    agent:
      prompt: >-
        Read https://untrusted.example.com/prompt.txt.
        The build token is ${{ secrets.TOKEN }}.
      tools: [nika:fetch, nika:done]
      max_turns: 2
"#,
        );
        let secrets = out
            .lines()
            .find(|line| line.contains("SECRETS"))
            .expect("the SECRETS rung renders");
        assert!(
            !secrets.contains("no declared secret reaches"),
            "a sanctioned flow is still a flow: {secrets}"
        );
        assert!(
            out.lines().any(|l| l.contains("JOURNEY")
                && l.contains("secret `TOKEN`")
                && l.contains("untrusted.example.com")),
            "JOURNEY names the host:\n{out}"
        );
    }

    /// A sanctioned MCP egress is not a leak, but it is still an effect.
    /// Both SECRETS and JOURNEY must say so before suggesting a run.
    #[test]
    fn a_sanctioned_mcp_secret_flow_is_visible_on_both_rungs() {
        let out = console(
            r#"
nika: mcp-secret
secrets:
  api_token:
    source: env
    key: SERVICE_API_TOKEN
    egress:
      - { to: "mcp:service/search" }
      - { to: outputs }
permits:
  tools: ["mcp:service/search"]
tasks:
  search:
    invoke:
      tool: "mcp:service/search"
      args: { token: "${{ secrets.api_token }}", query: "nika" }
outputs:
  result: "${{ tasks.search.output }}"
"#,
        );
        let secrets = out
            .lines()
            .find(|line| line.contains("SECRETS"))
            .expect("the SECRETS rung renders");
        assert!(
            secrets.contains("1 declared-secret flow")
                && !secrets.contains("no declared secret reaches")
        );
        let journey = out
            .lines()
            .find(|line| line.contains("JOURNEY") && line.contains("flows to"))
            .expect("the MCP flow row renders");
        assert!(
            journey.contains("secret `api_token` flows to mcp:service/search"),
            "the exact external sink rides the journey: {journey}"
        );
    }

    /// The journey observes every direct flow, while the IFC finding lane
    /// judges each secret's consent independently. The cleared edge stays
    /// visible without hiding the uncleared edge beside it.
    #[test]
    fn multiple_mcp_secret_flows_do_not_overstate_consent() {
        let out = console(
            r#"
nika: mcp-two-secrets
secrets:
  a_cleared:
    source: env
    key: CLEARED
    egress: [{ to: "mcp:service/send" }]
  z_uncleared: { source: env, key: UNCLEARED }
permits:
  tools: ["mcp:service/send"]
tasks:
  send:
    invoke:
      tool: "mcp:service/send"
      args:
        payload: "${{ secrets.a_cleared }}:${{ secrets.z_uncleared }}"
"#,
        );
        let secret_rows: Vec<&str> = out
            .lines()
            .filter(|line| line.contains("SECRETS"))
            .collect();
        assert_eq!(secret_rows.len(), 1, "one uncleared edge:\n{out}");
        assert!(
            secret_rows[0].contains("secrets.z_uncleared")
                && !secret_rows[0].contains("secrets.a_cleared"),
            "only the uncleared edge refuses:\n{out}"
        );
        assert!(
            out.contains("secret `a_cleared` flows to mcp:service/send")
                && out.contains("secret `z_uncleared` flows to mcp:service/send"),
            "both direct flows remain visible:\n{out}"
        );
    }

    /// The local→cloud flip is READABLE on the human surface (gauntlet
    /// 08-01, Aïcha: the --plain JOURNEY line was byte-identical for
    /// mock and a cloud model while --json knew locus/retention/
    /// training). A cloud endpoint earns its own dim row naming the
    /// provider, the egress fact and the sourced policy words; a pure
    /// mock voyage keeps its single line — the row never becomes noise.
    #[test]
    fn a_cloud_endpoint_earns_its_readable_disclosure_row() {
        let cloud = console(
            "nika: t\nmodel: openai/gpt-4o-mini\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        let row = cloud
            .lines()
            .find(|l| l.contains("cloud endpoint openai"))
            .expect("the cloud disclosure row renders");
        assert!(
            row.contains("task data leaves this machine"),
            "the egress fact is plain: {row}"
        );
        assert!(
            row.contains("retention") && row.contains("training"),
            "the sourced policy words ride the row: {row}"
        );

        let mock = console(
            "nika: t\nmodel: mock/echo\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        assert!(
            !mock.contains("cloud endpoint"),
            "a local voyage never grows the row:\n{mock}"
        );
    }
}

mod energy_tests {
    use crate::check_render::{fmt_scope_totals, fmt_wh};

    /// The display grain is ceiling-honest: rounding is UP, and the
    /// floor of the grain is 0.001 — `0.000` would claim free
    /// inference for a task that does spend.
    #[test]
    fn fmt_wh_never_prints_zero_for_a_positive_bound() {
        assert_eq!(fmt_wh(0.0004), "0.001");
        assert_eq!(fmt_wh(0.004), "0.004");
        assert_eq!(fmt_wh(0.087), "0.087");
        assert_eq!(fmt_wh(2.34), "2.3");
        assert_eq!(fmt_wh(660.1), "660.1");
    }

    /// The scope-total display: one class states the number bare (the
    /// class rides the count line); several classes join, each wearing
    /// its class; nothing measured → no claim at all. (The partition
    /// MATH is `nika_check::energy`'s — these pin the RENDER.)
    #[test]
    fn scope_totals_render_one_claim_per_class() {
        assert_eq!(
            fmt_scope_totals(&[
                ("device".to_owned(), 2.0),
                ("fleet".to_owned(), 4.0),
                ("gpu".to_owned(), 1.5),
            ]),
            "device ≤ 2.0 Wh · fleet ≤ 4.0 Wh · gpu ≤ 1.5 Wh"
        );
        assert_eq!(fmt_scope_totals(&[("gpu".to_owned(), 0.087)]), "≤ 0.087 Wh");
        assert_eq!(fmt_scope_totals(&[]), "");
    }
}

mod models_rung_tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    /// The two-strike class, rendered (audit UX 2026-07-31): the rung
    /// stays GREEN (the provider resolves — the catalog cross-check is
    /// advisory) and the ⚠ rides UNDER the green line, never instead
    /// of it — the user sees « resolves » AND « unheard of » together.
    #[test]
    fn a_catalog_warning_rides_under_the_green_models_line() {
        let yaml = "nika: t\ntasks:\n  probe:\n    infer: { model: anthropic/claude-4-nonexistent, prompt: \"x\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        let audit =
            ModelsAudit::new(Vec::new(), 0, 0).with_catalog_warnings(vec![ModelFinding::new(
                "anthropic/claude-4-nonexistent".to_owned(),
                vec!["probe".to_owned()],
                "matches none of `anthropic`'s known models".to_owned(),
            )]);
        let out = render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &audit,
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        );
        let green = out
            .lines()
            .position(|l| l.contains("MODELS") && l.contains('✔'))
            .expect("the green line stays");
        let warn = out
            .lines()
            .position(|l| l.contains("MODELS") && l.contains('⚠'))
            .expect("the ⚠ rides");
        assert!(warn > green, "the ⚠ rides UNDER the green line:\n{out}");
        // The advisory never dirties the verdict: the report the rung
        // rendered stayed clean (the provider resolves).
        assert!(report.is_clean());
    }
}

mod models_rung_liveness_tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn console(audit: &ModelsAudit) -> String {
        let yaml =
            "nika: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  think:\n    infer: { prompt: \"hi\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            audit,
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    /// B-5's sibling: a resolvable model on a server-backed keyless
    /// engine nuances the green line — « resolves » is never
    /// « reachable » for a server the rung never dialed.
    #[test]
    fn a_local_server_model_carries_the_liveness_nuance() {
        let out = console(&ModelsAudit::new(Vec::new(), 0, 0).with_local_server(1));
        let line = out
            .lines()
            .find(|l| l.contains("MODELS") && l.contains("resolves"))
            .expect("the MODELS green line");
        assert!(
            line.contains("local servers not probed (nika doctor --ping)"),
            "the nuance: {line}"
        );
    }

    /// A cloud-only file keeps the bare green line — the nuance is
    /// earned by a local model, never defaulted.
    #[test]
    fn no_local_server_model_means_no_nuance() {
        let out = console(&ModelsAudit::new(Vec::new(), 0, 0));
        let line = out
            .lines()
            .find(|l| l.contains("MODELS") && l.contains("resolves"))
            .expect("the MODELS green line");
        assert!(
            !line.contains("not probed"),
            "no nuance without a local engine: {line}"
        );
    }
}

/// The PERMITS panel must not assert a property of a body nobody analysed.
///
/// The 2026-07-29 finding — « the green did not mean the leak was gone. It
/// meant nobody looked. » — gated four lanes behind `section_or_skip` and left
/// PERMITS ungated. Measured 2026-08-15 on the shipped 0.108.0 binary: a jq
/// program reaching for the ambient environment printed
/// `✖ CONFORM [NIKA-VAR-005]` and, three rows later, « pure compute » about
/// the same body.
mod audited_line_names_the_blast_radius {
    use super::super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn console(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            true,
        )
    }

    /// Persona 4 · gauntlet g2: cost was on the default card, the named
    /// blast radius lived behind `--infer-permits` / `--json`.
    #[test]
    fn the_audited_line_names_the_declared_grants() {
        let out = console(
            "nika: w\npermits:\n  exec: [\"docker\"]\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out.md\"]\ntasks:\n  t:\n    exec: { command: [\"docker\", \"ps\"] }\n",
        );
        let line = out
            .lines()
            .find(|l| l.contains("audited"))
            .expect("audited card");
        assert!(
            line.contains("permits exec:docker tools:nika:write write:./out.md"),
            "named radius on the default card: {line}"
        );
        assert!(
            !line.contains("permits declared"),
            "the opaque word is gone: {line}"
        );
    }

    #[test]
    fn absent_permits_still_say_none() {
        let out = console(
            "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 1 }\n",
        );
        let line = out
            .lines()
            .find(|l| l.contains("audited"))
            .expect("audited card");
        assert!(line.contains("permits none"), "{line}");
    }
}

mod permits_panel_under_red_conformance {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn console(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    const REFUSED: &str = "nika: w\ntasks:\n  a:\n    invoke:\n      tool: \"nika:jq\"\n      args:\n        input: {}\n        expression: 'env.PATH'\n";
    const JUDGED: &str = "nika: w\ntasks:\n  a:\n    invoke:\n      tool: \"nika:jq\"\n      args:\n        input: {}\n        expression: '.'\n";

    /// A refused body: the panel says it did not judge, and never that the
    /// body is pure.
    #[test]
    fn the_panel_states_that_it_did_not_judge() {
        let out = console(REFUSED);
        assert!(out.contains("NIKA-VAR-005"), "the refusal renders: {out}");
        let line = out
            .lines()
            .find(|l| l.contains("PERMITS"))
            .expect("the PERMITS rung renders");
        assert!(
            line.contains("not judged while conformance fails"),
            "the panel must state that it did not look · got: {line}"
        );
        assert!(
            !out.contains("pure compute"),
            "the certificate claimed « pure compute » about a body it refused:\n{out}"
        );
        assert!(
            !out.contains("nothing escapes"),
            "the certificate claimed « nothing escapes » about a body it refused:\n{out}"
        );
    }

    /// …and the claim is UNCHANGED for a body that really was judged — without
    /// this, silencing the panel everywhere would pass the test above.
    #[test]
    fn a_judged_pure_body_still_states_its_zero() {
        let out = console(JUDGED);
        let line = out
            .lines()
            .find(|l| l.contains("PERMITS"))
            .expect("the PERMITS rung renders");
        assert!(line.contains("zero authority"), "{line}");
        assert!(line.contains("pure compute"), "{line}");
    }
}

mod builtin_contract_code_on_tools_and_args {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::*;

    fn console(yaml: &str) -> String {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        )
    }

    #[test]
    fn tools_and_args_rows_carry_the_json_finding_code() {
        let unknown_tool =
            console("nika: w\ntasks:\n  extract:\n    invoke:\n      tool: nika:ocr\n");
        assert!(
            unknown_tool.contains("[NIKA-BUILTIN-001]")
                && unknown_tool.contains("nika:ocr")
                && unknown_tool.contains("not a canonical builtin"),
            "TOOLS must name the JSON code:\n{unknown_tool}"
        );
        let missing_arg =
            console("nika: w\ntasks:\n  decide:\n    invoke:\n      tool: nika:decide\n");
        assert!(
            missing_arg.contains("[NIKA-BUILTIN-001]")
                && missing_arg.contains("missing required")
                && missing_arg.contains("evidence"),
            "ARGS must name the JSON code:\n{missing_arg}"
        );
    }
}

mod writes_card {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    use crate::check_render::{ModelsAudit, RepairTarget, render};
    use crate::theme::Theme;

    #[test]
    fn writes_card_lists_engine_traces() {
        let yaml =
            "nika: t\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 1 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = nika_check::check(&wf);
        let out = render(
            &report,
            &wf,
            yaml,
            "w.nika.yaml",
            RepairTarget::WorkspaceFile,
            Theme::new(false, false, false),
            &ModelsAudit::new(Vec::new(), 0, 0),
            &nika_schema::ResolvedSkills::default(),
            &[],
            report.is_clean(),
        );
        assert!(out.contains("WRITES"), "B12 WRITES rung missing:\n{out}");
        assert!(
            out.contains(".nika/traces"),
            "B12 engine writes must be on the WRITES card:\n{out}"
        );
    }
}
