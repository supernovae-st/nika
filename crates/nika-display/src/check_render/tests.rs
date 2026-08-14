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
            line.contains("no secret reaches a cloud destination"),
            "the honest closure: {line}"
        );
    }

    /// A secret flowing to a cloud endpoint is NAMED on an explicit ⚠
    /// row — advisory (the sanctioned egress stays clean; the SECRETS
    /// lane owns the unsanctioned refusal), with the receipt law riding.
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
            row.contains("read it before the run"),
            "the receipt law rides: {row}"
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
