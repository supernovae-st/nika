// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The four workflow faces over real audits. Each test stands in for the
//! owner: it runs the one audit on the fixture bytes and hands the typed
//! facts over, as the Session will; the view itself never audits.

#![allow(clippy::expect_used)]

use nika_cli_host::oracle::{Audit, AuditOptions, audit_source};
use nika_display::check_render::VerdictLayers;
use nika_display::dag_art::{GraphDoc, project};
use ratatui::style::Color;
use ratatui::text::Line;

use super::cells;
use super::{Canvas, Face, Finding, Rendered, Verdict, Workflow, workflow};

/// Four tasks, three waves (two side by side), a human gate, no finding.
const CLEAN: &str = r#"nika: view-clean
permits:
  fs: { read: ["./notes/**"], write: ["./out/summary.md", "./out/digest.md"] }
  tools: ["nika:read", "nika:write", "nika:prompt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/brief.md" }
  approve:
    with: { brief: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Publish ${{ with.brief }}?" }
  save:
    with: { text: "${{ tasks.read_notes.output }}", go: "${{ tasks.approve.output }}" }
    when: "${{ with.go == true }}"
    invoke:
      tool: "nika:write"
      args: { path: "./out/summary.md", content: "${{ with.text }}" }
  digest:
    with: { text: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/digest.md", content: "${{ with.text }}" }
"#;

/// A diamond the wires can draw truthfully.
const DIAMOND: &str = r#"nika: view-diamond
permits:
  fs: { read: ["./notes/brief.md"], write: ["./out/a.md", "./out/b.md"] }
  tools: ["nika:read", "nika:write", "nika:log"]
tasks:
  source:
    invoke: { tool: "nika:read", args: { path: "./notes/brief.md" } }
  left:
    with: { t: "${{ tasks.source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/a.md", content: "${{ with.t }}" } }
  right:
    with: { t: "${{ tasks.source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/b.md", content: "${{ with.t }}" } }
  join:
    with: { a: "${{ tasks.left.output }}", b: "${{ tasks.right.output }}" }
    invoke: { tool: "nika:log", args: { message: "done ${{ with.a }} ${{ with.b }}" } }
"#;

/// A confirm gate whose answer the effect never reads: NIKA-SEC-014.
const CONSENT: &str = r#"nika: view-consent
permits:
  fs: { read: ["./notes/**"], write: ["./out/summary.md"] }
  tools: ["nika:read", "nika:write", "nika:prompt"]
tasks:
  read_notes:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/brief.md" }
  approve:
    with: { brief: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Publish ${{ with.brief }}?" }
  save:
    with: { text: "${{ tasks.read_notes.output }}" }
    after: { approve: success }
    invoke:
      tool: "nika:write"
      args: { path: "./out/summary.md", content: "${{ with.text }}" }
"#;

/// A dependency cycle: NIKA-DAG-001, no run order.
const CYCLE: &str = r#"nika: view-cycle
tasks:
  first:
    with: { x: "${{ tasks.second.output }}" }
    invoke:
      tool: "nika:log"
      args: { message: "${{ with.x }}" }
  second:
    with: { y: "${{ tasks.first.output }}" }
    invoke:
      tool: "nika:log"
      args: { message: "${{ with.y }}" }
"#;

fn audit(source: &str) -> Audit {
    audit_source(source, "view.nika", None, None, AuditOptions::default())
        .expect("the fixture parses")
}

/// What the owner hands over for one source: the audit, its projection,
/// and where each finding and hint points, in the owner's words (a task by
/// its id, nothing for a workflow-level hint, whose task reads `-`).
struct Owner {
    audit: Audit,
    doc: GraphDoc,
    finding_places: Vec<Option<String>>,
    hint_places: Vec<Option<String>>,
}

impl Owner {
    fn new(source: &str) -> Self {
        let audit = audit(source);
        let doc = project(&audit.wf, &audit.report);
        let finding_places = audit
            .report
            .findings
            .iter()
            .map(|f| f.task.as_ref().map(|task| format!("task {task}")))
            .collect();
        let hint_places = audit
            .report
            .hints
            .iter()
            .map(|h| (h.task != "-").then(|| format!("task {}", h.task)))
            .collect();
        Self {
            audit,
            doc,
            finding_places,
            hint_places,
        }
    }

    /// Show `face` of `source` as the owner would hand it over after a check.
    fn show(&self, face: Face, name: &str, source: &str, canvas: Canvas) -> Rendered {
        let report = &self.audit.report;
        let findings: Vec<Finding<'_>> = report
            .findings
            .iter()
            .zip(&self.finding_places)
            .map(|(f, place)| Finding::new(f.code.as_deref(), f.kind, &f.message, place.as_deref()))
            .collect();
        let hints: Vec<Finding<'_>> = report
            .hints
            .iter()
            .zip(&self.hint_places)
            .map(|(h, place)| Finding::new(h.code, h.kind, &h.advice, place.as_deref()))
            .collect();
        let mut input = Workflow::new(name, source);
        input.waves = Some(&report.waves);
        input.graph = Some(&self.doc);
        input.verdict = Verdict::Layers(&self.audit.verdict.layers);
        input.findings = &findings;
        input.hints = &hints;
        input.identity = report.workflow_semantic.as_deref();
        input.risk = Some(self.audit.verdict.grade.as_str());
        workflow(face, &input, canvas)
    }
}

/// One face of one source, audited once for this call.
fn shown(face: Face, name: &str, source: &str, canvas: Canvas) -> Rendered {
    Owner::new(source).show(face, name, source, canvas)
}

fn text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn texts(rendered: &Rendered) -> Vec<String> {
    rendered.lines.iter().map(text).collect()
}

#[test]
fn the_plan_reads_in_run_order_by_wave_with_its_gate() {
    let plan = shown(
        Face::Plan,
        "clean.nika",
        CLEAN,
        Canvas::new(80, true, false),
    );
    assert_eq!(
        texts(&plan),
        [
            "wave 1",
            "  1. @ read_notes - reads a file",
            "wave 2 - 2 side by side",
            "  2. @ approve - asks a human - human gate",
            "  3. @ digest - writes a file",
            "wave 3",
            "  4. @ save - writes a file",
        ]
    );
    assert!(
        plan.facts
            .contains(&"4 tasks - 3 waves - 1 human gate".to_owned()),
        "{:?}",
        plan.facts
    );
    assert!(plan.facts.contains(&"run order from the check".to_owned()));
}

#[test]
fn a_static_plan_and_graph_keep_the_verbs_in_the_surrounding_ink() {
    for face in [Face::Plan, Face::Graph] {
        let rendered = shown(face, "diamond.nika", DIAMOND, Canvas::new(80, false, true));
        for line in &rendered.lines {
            for span in &line.spans {
                assert!(
                    !matches!(
                        span.style.fg,
                        Some(
                            Color::LightBlue
                                | Color::LightYellow
                                | Color::LightCyan
                                | Color::LightMagenta
                        )
                    ),
                    "{face:?}: a verb hue on a static face: {span:?}"
                );
            }
        }
    }
}

#[test]
fn the_graph_draws_a_diamond_and_lists_what_it_cannot_draw_truthfully() {
    let diamond = shown(
        Face::Graph,
        "diamond.nika",
        DIAMOND,
        Canvas::new(80, true, false),
    );
    let art = texts(&diamond).join("\n");
    for node in ["@ source", "@ left", "@ right", "@ join"] {
        assert!(art.contains(node), "{node} missing from:\n{art}");
    }
    assert!(art.is_ascii(), "{art}");
    assert!(
        diamond
            .facts
            .contains(&"4 tasks - 4 edges - 3 waves".to_owned()),
        "{:?}",
        diamond.facts
    );
    let clean = shown(
        Face::Graph,
        "clean.nika",
        CLEAN,
        Canvas::new(80, true, false),
    );
    let listed = texts(&clean);
    assert_eq!(listed[0], "  wave 1 - @ read_notes");
    assert!(
        clean.facts.iter().any(|f| f.starts_with("listed by wave")),
        "{:?}",
        clean.facts
    );
}

#[test]
fn the_check_names_the_four_layers_and_every_code() {
    let clean = texts(&shown(
        Face::Check,
        "clean.nika",
        CLEAN,
        Canvas::new(100, true, false),
    ));
    assert!(clean[0].starts_with("ok VALID"), "{clean:?}");
    assert!(clean[1].starts_with("-  ACCESS READY n/a"), "{clean:?}");
    let has = |start: &str| clean.iter().any(|l| l.starts_with(start));
    assert!(has("ok CAPACITY FIT the seats"), "{clean:?}");
    assert!(
        has("ok RUN READY    nothing known blocks a run"),
        "{clean:?}"
    );
    assert!(clean.iter().any(|l| l.starts_with("hints - ")), "{clean:?}");
    let consent = shown(
        Face::Check,
        "consent.nika",
        CONSENT,
        Canvas::new(100, true, false),
    );
    let rows = texts(&consent);
    assert!(rows[0].starts_with("X  VALID"), "{rows:?}");
    assert!(
        rows.iter().any(|l| l.starts_with("X  NIKA-SEC-014 - ")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|l| l == "X  RUN READY    not ready: a layer above says why"),
        "{rows:?}"
    );
    assert!(
        rows.join(" ").contains("(task save)"),
        "the finding points at its task in the owner's words: {rows:?}"
    );
    assert!(
        consent.facts.contains(&"1 finding".to_owned()),
        "{:?}",
        consent.facts
    );
}

#[test]
fn run_ready_says_blocked_by_its_blockers_or_points_above() {
    let blocked = VerdictLayers::new(
        true,
        Some(true),
        Vec::new(),
        true,
        vec!["ceiling · the run needs --max-cost-usd".to_owned()],
    );
    let above = VerdictLayers::new(true, Some(true), Vec::new(), false, Vec::new());
    let row = |layers: &VerdictLayers, ascii: bool| {
        let mut input = Workflow::new("x.nika", CLEAN);
        input.verdict = Verdict::Layers(layers);
        texts(&workflow(
            Face::Check,
            &input,
            Canvas::new(80, ascii, false),
        ))
    };
    let rows = row(&blocked, true);
    let at = rows
        .iter()
        .position(|l| l == "X  RUN READY    blocked by")
        .expect("the blocked row");
    assert_eq!(
        rows[at + 1],
        format!("{}ceiling - the run needs --max-cost-usd", " ".repeat(16)),
        "the blocker hangs under its layer, in the ASCII column"
    );
    assert!(
        row(&blocked, false)
            .iter()
            .any(|l| l.ends_with("ceiling · the run needs --max-cost-usd")),
        "the Unicode column keeps the engine's dot"
    );
    let rows = row(&above, true);
    assert!(
        rows.iter()
            .any(|l| l == "X  RUN READY    not ready: a layer above says why"),
        "{rows:?}"
    );
}

/// Layers computed over one file read alone never stand for the composed
/// workflow: RUN READY is unknown even when every layer holds, and the rows
/// say what was not captured.
#[test]
fn a_file_judged_alone_never_claims_run_ready() {
    let ready = VerdictLayers::new(true, Some(true), Vec::new(), true, Vec::new());
    let mut input = Workflow::new("parent.nika", CLEAN);
    input.verdict = Verdict::ParentOnly {
        layers: &ready,
        unknown: "not captured: child workflows, skills, registry references",
    };
    let rows = texts(&workflow(Face::Check, &input, Canvas::new(80, true, false)));
    assert_eq!(
        rows[0],
        "-  IMPORTS      not captured: child workflows, skills, registry references"
    );
    assert!(rows.iter().any(|l| l.starts_with("ok VALID")), "{rows:?}");
    let run = rows
        .iter()
        .find(|l| l.contains("RUN READY"))
        .expect("a RUN READY row");
    assert!(run.starts_with("-  RUN READY    unknown"), "{run}");
    assert!(
        !rows
            .iter()
            .any(|l| l.contains("nothing known blocks a run")),
        "{rows:?}"
    );
}

#[test]
fn a_proposal_is_judged_on_its_source_alone() {
    let mut input = Workflow::new("proposal.nika", CLEAN);
    input.verdict = Verdict::SourceOnly { valid: true };
    let rows = texts(&workflow(Face::Check, &input, Canvas::new(80, true, false)));
    assert!(rows[0].starts_with("ok VALID"), "{rows:?}");
    for (row, name) in rows[1..4]
        .iter()
        .zip(["ACCESS READY", "CAPACITY FIT", "RUN READY"])
    {
        assert_eq!(
            row,
            &format!("-  {name:<13}judged when the workflow is saved")
        );
    }
}

#[test]
fn refused_and_unjudged_bytes_say_so_on_every_face() {
    let mut refused = Workflow::new("bad.nika", "nika: x\nbogus: 1\n");
    refused.verdict = Verdict::Refused {
        code: "NIKA-PARSE-005",
        message: "unknown key bogus",
    };
    let check = texts(&workflow(
        Face::Check,
        &refused,
        Canvas::new(80, true, false),
    ));
    assert!(check[0].starts_with("X  VALID"), "{check:?}");
    assert!(
        check[1].starts_with("X  NIKA-PARSE-005 - unknown key bogus"),
        "{check:?}"
    );
    let graph = texts(&workflow(
        Face::Graph,
        &refused,
        Canvas::new(80, true, false),
    ));
    assert_eq!(
        graph,
        ["no graph: the parser refused the bytes (see the check face)"]
    );
    let unjudged = Workflow::new("new.nika", CLEAN);
    let check = texts(&workflow(
        Face::Check,
        &unjudged,
        Canvas::new(80, true, false),
    ));
    assert!(
        check[..4]
            .iter()
            .all(|l| l.starts_with("-  ") && l.ends_with("nothing checked these bytes yet")),
        "{check:?}"
    );
    let plan = workflow(Face::Plan, &unjudged, Canvas::new(80, true, false));
    assert!(plan.facts.contains(&"file order: no check ran".to_owned()));
}

#[test]
fn a_cycle_has_no_graph_and_its_plan_keeps_file_order() {
    let graph = shown(
        Face::Graph,
        "cycle.nika",
        CYCLE,
        Canvas::new(80, true, false),
    );
    assert_eq!(
        texts(&graph),
        ["no graph: the check found no valid run order"]
    );
    let plan = shown(
        Face::Plan,
        "cycle.nika",
        CYCLE,
        Canvas::new(80, true, false),
    );
    assert!(
        plan.facts
            .contains(&"file order: the check found no run order".to_owned()),
        "{:?}",
        plan.facts
    );
    let check = texts(&shown(
        Face::Check,
        "cycle.nika",
        CYCLE,
        Canvas::new(100, true, false),
    ));
    assert!(
        check.iter().any(|l| l.contains("NIKA-DAG-001")),
        "{check:?}"
    );
}

#[test]
fn every_face_fits_every_width_and_keeps_its_meaning_without_colour() {
    for source in [CLEAN, DIAMOND, CONSENT, CYCLE] {
        let owner = Owner::new(source);
        for face in Face::ALL {
            for width in [1_u16, 12, 20, 40, 80, 120] {
                for (ascii, color) in [(true, false), (false, false), (false, true)] {
                    let rendered = owner.show(
                        face,
                        "fixture.nika",
                        source,
                        Canvas::new(width, ascii, color),
                    );
                    for line in rendered
                        .lines
                        .iter()
                        .chain(rendered.head(Canvas::new(width, ascii, color)).iter())
                    {
                        let row = text(line);
                        assert!(
                            cells::width(&row) <= usize::from(width),
                            "{face:?} @ {width}: {row}"
                        );
                        assert!(
                            !row.chars().any(char::is_control),
                            "{face:?}: a control char in {row:?}"
                        );
                        if !color {
                            assert!(
                                line.spans.iter().all(|s| s.style.fg.is_none()),
                                "{face:?}: a hue without colour"
                            );
                        }
                    }
                    if ascii {
                        let all = texts(&rendered).join("\n");
                        assert!(all.is_ascii(), "{face:?}: not ASCII:\n{all}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_source_face_shows_every_byte_it_is_given_within_its_bounds() {
    let source = shown(
        Face::Source,
        "clean.nika",
        CLEAN,
        Canvas::new(120, true, false),
    );
    let lines = texts(&source);
    assert_eq!(lines.len(), CLEAN.lines().count());
    for (row, original) in lines.iter().zip(CLEAN.lines()) {
        assert!(row.ends_with(original), "{row} != {original}");
    }
}
