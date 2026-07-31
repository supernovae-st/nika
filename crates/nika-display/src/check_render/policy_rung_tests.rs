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
        // No MODELS/SKILLS findings in this harness — the one verdict
        // reduces to the report's own cleanliness.
        report.is_clean(),
    )
}

/// 2a · the console speaks the findings[] voice: an `endorsement.*`
/// finding prints NIKA-SEC-013 on the POLICY rung — the rung used to
/// stamp NIKA-POLICY-001 on EVERY policy row while the wire spoke
/// SEC-013 (measured live broken).
#[test]
fn a_solo_count_row_prints_sec_013_on_console() {
    let out = console(
        "nika: v1\nworkflow:\n  id: t\npolicy:\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"two?\", default: false }\n  act:\n    after: { second: success }\n    exec: { command: [\"echo\", \"shipped\"] }\n",
    );
    let row = out
        .lines()
        .find(|l| l.contains("POLICY") && l.contains("solo_count"))
        .expect("the POLICY finding row");
    assert!(
        row.contains("[NIKA-SEC-013]"),
        "the console speaks the wire code: {row}"
    );
    assert!(
        !out.contains("[NIKA-POLICY-001]"),
        "no stale policy-lane code on an endorsement row:\n{out}"
    );
}

/// The mapping is prefix-exact, not a blanket rename: a plain
/// policy-lane rule keeps NIKA-POLICY-001 on the console too.
#[test]
fn a_limits_row_keeps_policy_001_on_console() {
    let out = console(
        "nika: v1\nworkflow:\n  id: t\npolicy:\n  limits: { max_tasks: 1 }\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n  b:\n    infer: { prompt: \"y\" }\n",
    );
    let row = out
        .lines()
        .find(|l| l.contains("POLICY") && l.contains('['))
        .expect("the POLICY finding row");
    assert!(row.contains("[NIKA-POLICY-001]"), "the lane code: {row}");
}
