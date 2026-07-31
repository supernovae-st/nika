use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

use super::*;

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
        "nika: v1\nworkflow:\n  id: t\nmodel: mock/echo\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
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
nika: v1
workflow:
  id: t
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
