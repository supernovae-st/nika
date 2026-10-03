// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow in view, four faces over the same bytes: its source, its
//! plan in run order, its graph and its check. Every face reads typed facts
//! its owner computed and handed over ([`Workflow`]); nothing here audits,
//! judges or reads a permit. What the engine functions called here need,
//! and whether they run offline:
//!
//! - `nika_session::review::plan_lines_in_order(source, waves)` and
//!   `gate_tasks(source)` read the source with the engine's own parser:
//!   pure, offline, one parse per call (the caller keeps the result);
//! - `nika_display::dag_art::wire_graph` and `nika_display::wires::render`
//!   draw a caller-supplied graph projection (`dag_art::project` output)
//!   over the check's waves: pure, offline, at most 78 columns; when a
//!   drawing would lie, the waves are listed instead;
//! - the check face shows the layers the check computed
//!   (`nika_display::check_render::VerdictLayers` and its own
//!   `run_ready`), or VALID alone for a proposal judged on its source, and
//!   the findings its owner listed. The owner computes them away from the
//!   display path: the one audit (`nika_cli_host::oracle::audit_source`)
//!   parses and checks purely, and reads which provider keys are present in
//!   the environment for ACCESS READY, never their values, never the
//!   network.
//!
//! The verb hues are syntax in the source face. The plan and the graph are
//! static, so their verb glyphs keep the surrounding ink: a hue there would
//! read as activity nobody observed.

use std::collections::BTreeMap;

use nika_display::check_render::VerdictLayers;
use nika_display::dag_art::{GraphDoc, wire_graph};
use nika_display::theme::{Role, Theme};
use ratatui::text::Span;

use super::cells::{self, Sheet, paint, plain};
use super::{Canvas, Format, Rendered, source};

/// The four faces of one workflow in view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Face {
    /// The bytes, verb-aware.
    Source,
    /// The tasks in run order.
    Plan,
    /// The graph of the tasks.
    Graph,
    /// The layers and every finding.
    Check,
}

impl Face {
    /// Every face, in the order a region's tabs list them.
    pub const ALL: [Self; 4] = [Self::Source, Self::Plan, Self::Graph, Self::Check];

    /// The face in words.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Plan => "plan",
            Self::Graph => "graph",
            Self::Check => "check",
        }
    }
}

/// What the check said about the bytes in view, as its owner projected it.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Verdict<'a> {
    /// The four layers, judged together (after a consent, or by `check`).
    Layers(&'a VerdictLayers),
    /// A proposal judged on its source alone: VALID, the rest judged when
    /// the workflow is saved.
    SourceOnly {
        /// The definition is legal.
        valid: bool,
    },
    /// The layers the check computed over ONE file read alone: what it
    /// imports (children, skills, registry references) was not captured, so
    /// RUN READY is unknown and `unknown` says what was left out. Never a
    /// readiness or a permission of the composed workflow.
    ParentOnly {
        /// The layers, as computed over the file alone.
        layers: &'a VerdictLayers,
        /// What the look did not capture, in the owner's words.
        unknown: &'a str,
    },
    /// The parser refused the bytes (`SchemaError::diagnostic`).
    Refused {
        /// The refusal's code.
        code: &'a str,
        /// The refusal in the engine's words.
        message: &'a str,
    },
    /// Nothing judged the bytes yet.
    NotJudged,
}

/// One finding or hint as its owner listed it (a unified finding, a model
/// or skill row, a hint): shown, never re-derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Finding<'a> {
    /// Its spec code (`NIKA-AUTH-006`), when it has one.
    pub code: Option<&'a str>,
    /// Its class or kind, shown when no code names it.
    pub class: &'a str,
    /// The engine's words.
    pub message: &'a str,
    /// Where it points (a task, a gate), in the owner's words.
    pub place: Option<&'a str>,
}

impl<'a> Finding<'a> {
    /// A finding row.
    #[must_use]
    pub const fn new(
        code: Option<&'a str>,
        class: &'a str,
        message: &'a str,
        place: Option<&'a str>,
    ) -> Self {
        Self {
            code,
            class,
            message,
            place,
        }
    }
}

/// The typed facts a workflow's faces read, each computed by its owner.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Workflow<'a> {
    /// The file name.
    pub name: &'a str,
    /// The bytes, as text.
    pub source: &'a str,
    /// The check's waves (task indices in run order), when a check ran.
    pub waves: Option<&'a [Vec<usize>]>,
    /// The graph projection of the same bytes (`dag_art::project`).
    pub graph: Option<&'a GraphDoc>,
    /// What the check said.
    pub verdict: Verdict<'a>,
    /// The findings, in the owner's order.
    pub findings: &'a [Finding<'a>],
    /// The advisory hints, in the owner's order.
    pub hints: &'a [Finding<'a>],
    /// The semantic identity the check stamped (`workflow_semantic`).
    pub identity: Option<&'a str>,
    /// The risk grade's word (`RiskGrade::as_str`).
    pub risk: Option<&'a str>,
}

impl<'a> Workflow<'a> {
    /// A workflow known only by its name and bytes: nothing judged yet.
    #[must_use]
    pub const fn new(name: &'a str, source: &'a str) -> Self {
        Self {
            name,
            source,
            waves: None,
            graph: None,
            verdict: Verdict::NotJudged,
            findings: &[],
            hints: &[],
            identity: None,
            risk: None,
        }
    }
}

/// Show one face of a workflow.
#[must_use]
pub fn workflow(face: Face, input: &Workflow<'_>, canvas: Canvas) -> Rendered {
    let mut sheet = Sheet::new(canvas, Format::Workflow);
    let dot = cells::sep(canvas.ascii);
    sheet
        .facts
        .push(format!("{}{dot}{}", Format::Workflow.label(), face.label()));
    sheet.facts.push(cells::size_words(input.source.len()));
    if let Some(identity) = input.identity {
        let short: String = cells::clean(identity, canvas.ascii)
            .0
            .chars()
            .take(12)
            .collect();
        sheet.facts.push(format!("identity {short}"));
    }
    match face {
        // The owner hands the bytes over as they are: a protected file is
        // shown through `artifact`, which masks it.
        Face::Source => source::show(&mut sheet, input.source, false),
        Face::Plan => plan(&mut sheet, input),
        Face::Graph => graph(&mut sheet, input),
        Face::Check => check(&mut sheet, input),
    }
    let (title, _) = cells::clean(input.name, canvas.ascii);
    sheet.finish(title)
}

/// One plan line restyled: its number, the verb's glyph, the task id
/// strong, the engine's words for what it does; `None` when the line is
/// not the one expected for this task (then it is shown as written).
fn plan_line(
    line: &str,
    n: usize,
    id: &str,
    verb: &str,
    canvas: Canvas,
) -> Option<Vec<Span<'static>>> {
    let face = line.strip_prefix(&format!("  {}. {id} · ", n + 1))?;
    let theme = Theme::new(false, canvas.ascii, false);
    Some(vec![
        paint(format!("{:>3}. ", n + 1), Role::Dim, canvas.color),
        plain(format!("{} ", theme.verb_glyph_bare(Some(verb)))),
        paint(cells::clean(id, canvas.ascii).0, Role::Strong, canvas.color),
        paint(cells::sep(canvas.ascii), Role::Dim, canvas.color),
        plain(cells::dots(
            &cells::clean(face, canvas.ascii).0,
            canvas.ascii,
        )),
    ])
}

/// The plan face: the engine's plan lines in run order, a wave header
/// where the waves change, the verb glyphs, the human gates in amber.
fn plan(sheet: &mut Sheet, input: &Workflow<'_>) {
    let canvas = sheet.body.canvas();
    let waves = input.waves.unwrap_or(&[]);
    let lines = nika_session::review::plan_lines_in_order(input.source, waves);
    let nodes: Vec<(&str, &str, &str)> = input
        .graph
        .map(|g| {
            g.nodes
                .iter()
                .map(|n| (n.id.as_str(), n.verb, n.kind))
                .collect()
        })
        .unwrap_or_default();
    let gates = nika_session::review::gate_tasks(input.source);
    let dot = cells::sep(canvas.ascii);
    let mut starts = BTreeMap::new();
    let mut at = 0;
    for (k, wave) in waves.iter().enumerate() {
        starts.insert(at, (k + 1, wave.len()));
        at += wave.len();
    }
    for (n, line) in lines.iter().enumerate() {
        if waves.len() > 1
            && let Some((k, size)) = starts.get(&n)
        {
            let beside = if *size > 1 {
                format!("{dot}{size} side by side")
            } else {
                String::new()
            };
            let header = paint(format!("wave {k}{beside}"), Role::Dim, canvas.color);
            if !sheet.body.push(vec![header], false) {
                break;
            }
        }
        let styled = nodes.get(n).and_then(|(id, verb, kind)| {
            let mut spans = plan_line(line, n, id, verb, canvas)?;
            if gates.iter().any(|g| g == id) {
                spans.push(paint(format!("{dot}human gate"), Role::Warn, canvas.color));
            }
            if *kind == "finally" {
                spans.push(paint(format!("{dot}cleanup"), Role::Dim, canvas.color));
            }
            Some(spans)
        });
        let verbatim = || {
            vec![plain(cells::dots(
                &cells::clean(line, canvas.ascii).0,
                canvas.ascii,
            ))]
        };
        let spans = styled.unwrap_or_else(verbatim);
        if !sheet.body.push(spans, false) {
            break;
        }
    }
    let order = match (input.waves, waves.is_empty()) {
        (Some(_), false) => "run order from the check",
        (Some(_), true) => "file order: the check found no run order",
        (None, _) => "file order: no check ran",
    };
    let counted = [
        cells::count(nodes.len().max(lines.len()), "task"),
        cells::count(waves.len(), "wave"),
        cells::count(gates.len(), "human gate"),
    ];
    sheet.facts.push(counted.join(dot));
    sheet.facts.push(order.to_owned());
}

/// Whether `c` may belong to a task id.
fn id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
}

/// The id-shaped word starting at byte `from` of `line`.
fn word_at(line: &str, from: usize) -> &str {
    let rest = &line[from..];
    &rest[..rest.find(|c: char| !id_char(c)).unwrap_or(rest.len())]
}

/// One line of a drawing restyled: a node (its glyph and its id) and a
/// known id in the default ink, the rails and the words around them dim.
fn restyle(line: &str, ids: &BTreeMap<&str, &str>, canvas: Canvas) -> Vec<Span<'static>> {
    let theme = Theme::new(false, canvas.ascii, false);
    let mut out = Vec::new();
    let mut chrome = String::new();
    let mut at = 0;
    let flush = |chrome: &mut String, out: &mut Vec<Span<'static>>| {
        if !chrome.is_empty() {
            out.push(paint(std::mem::take(chrome), Role::Dim, canvas.color));
        }
    };
    while let Some(c) = line[at..].chars().next() {
        let after = at + c.len_utf8();
        if line[after..].starts_with(' ') {
            let word = word_at(line, after + 1);
            let glyph = |verb: &&str| theme.verb_glyph_bare(Some(*verb)).starts_with(c);
            if !word.is_empty() && ids.get(word).is_some_and(glyph) {
                flush(&mut chrome, &mut out);
                out.push(plain(format!("{c} {word}")));
                at = after + 1 + word.len();
                continue;
            }
        }
        let starts_word = line[..at].chars().next_back().is_none_or(|b| !id_char(b));
        let word = word_at(line, at);
        if starts_word && !word.is_empty() && ids.contains_key(word) {
            flush(&mut chrome, &mut out);
            out.push(plain(word.to_owned()));
            at += word.len();
            continue;
        }
        chrome.push(c);
        at = after;
    }
    flush(&mut chrome, &mut out);
    out
}

/// The waves listed, one row each, when a drawing would lie.
fn listing(doc: &GraphDoc, waves: &[Vec<usize>], canvas: Canvas) -> Vec<String> {
    let theme = Theme::new(false, canvas.ascii, false);
    let dot = cells::sep(canvas.ascii);
    let mut cursor = 0;
    waves
        .iter()
        .enumerate()
        .map(|(i, wave)| {
            let nodes = doc
                .nodes
                .get(cursor..cursor + wave.len())
                .unwrap_or_default();
            cursor += wave.len();
            let names: Vec<String> = nodes
                .iter()
                .map(|n| format!("{} {}", theme.verb_glyph_bare(Some(n.verb)), n.id))
                .collect();
            format!("  wave {}{dot}{}", i + 1, names.join(dot))
        })
        .collect()
}

/// The graph face: the engine's drawing of the caller's projection,
/// restyled, or the waves listed, or why there is neither.
fn graph(sheet: &mut Sheet, input: &Workflow<'_>) {
    let canvas = sheet.body.canvas();
    let quiet = |sheet: &mut Sheet, why: &str, role: Role| {
        sheet.body.push(vec![paint(why, role, canvas.color)], false);
    };
    let (Some(doc), Some(waves)) = (input.graph, input.waves) else {
        let why = match input.verdict {
            Verdict::Refused { .. } => {
                "no graph: the parser refused the bytes (see the check face)"
            }
            _ => "no graph yet: it needs the projection and the check's run order",
        };
        quiet(sheet, why, Role::Dim);
        return;
    };
    if waves.is_empty() {
        quiet(
            sheet,
            "no graph: the check found no valid run order",
            Role::Warn,
        );
        return;
    }
    if waves.iter().map(Vec::len).sum::<usize>() > doc.nodes.len() {
        quiet(
            sheet,
            "no graph: the projection and the waves disagree",
            Role::Warn,
        );
        return;
    }
    let theme = Theme::new(false, canvas.ascii, false);
    let drawn = nika_display::wires::render(&wire_graph(doc, waves), theme);
    let lines: Vec<String> = match &drawn {
        Some(art) => art.lines().map(str::to_owned).collect(),
        None => listing(doc, waves, canvas),
    };
    let ids: BTreeMap<&str, &str> = doc.nodes.iter().map(|n| (n.id.as_str(), n.verb)).collect();
    let limit = canvas.limits.line_bytes;
    for line in &lines {
        let cut = line.len() > limit;
        let line = &line[..cells::floor_boundary(line, limit)];
        if !sheet.body.push(restyle(line, &ids, canvas), cut) {
            break;
        }
    }
    let counted = [
        cells::count(doc.nodes.len(), "task"),
        cells::count(doc.edges.len(), "edge"),
        cells::count(waves.len(), "wave"),
    ];
    sheet.facts.push(counted.join(cells::sep(canvas.ascii)));
    if drawn.is_none() {
        sheet
            .facts
            .push("listed by wave: a drawing here would cross, skip or crowd its wires".to_owned());
    }
}

/// The mark of a layer's answer, two cells wide in both glyph columns.
fn mark(answer: Option<bool>, ascii: bool) -> String {
    let glyph = match (answer, ascii) {
        (Some(true), false) => "✔",
        (Some(true), true) => "ok",
        (Some(false), false) => "✖",
        (Some(false), true) => "X",
        (None, false) => "○",
        (None, true) => "-",
    };
    cells::cell(glyph, 2, false, ascii)
}

/// One layer row: mark, name, reason; `extra` rows hang under the reason.
fn layer(
    sheet: &mut Sheet,
    answer: Option<bool>,
    name: &str,
    reason: &str,
    tone: Role,
    extra: &[String],
) {
    let canvas = sheet.body.canvas();
    let lead = [
        paint(
            format!("{} ", mark(answer, canvas.ascii)),
            tone,
            canvas.color,
        ),
        paint(format!("{name:<13}"), Role::Strong, canvas.color),
    ];
    let hang = [plain(" ".repeat(16))];
    sheet
        .body
        .wrap(&[paint(reason, tone, canvas.color)], &lead, &hang, false);
    for row in extra {
        let row = cells::dots(&cells::clean(row, canvas.ascii).0, canvas.ascii);
        sheet
            .body
            .wrap(&[paint(row, Role::Dim, canvas.color)], &hang, &hang, false);
    }
}

/// The tone of a layer's answer: green, the failure's tone, or dim.
fn tone(answer: Option<bool>, failed: Role) -> Role {
    match answer {
        Some(true) => Role::Good,
        Some(false) => failed,
        None => Role::Dim,
    }
}

/// The four layers as the check computed them, each with why.
fn layers(sheet: &mut Sheet, layers: &VerdictLayers, run_unknown: Option<&str>) {
    let valid = if layers.valid {
        "the definition is legal"
    } else {
        "the definition breaks a rule: the findings below say which"
    };
    layer(
        sheet,
        Some(layers.valid),
        "VALID",
        valid,
        tone(Some(layers.valid), Role::Bad),
        &[],
    );
    let access = match (layers.access_ready, layers.access_moot) {
        (Some(true), _) => "every model has a ready path on this machine",
        (Some(false), _) => "a model has no ready path on this machine",
        (None, true) => "n/a: no infer or agent task, nothing dials",
        (None, false) => "not judged here: the model arrives at run time and admission judges it",
    };
    let access_tone = tone(layers.access_ready, Role::Bad);
    layer(
        sheet,
        layers.access_ready,
        "ACCESS READY",
        access,
        access_tone,
        &layers.access_lines,
    );
    let capacity = if layers.capacity_fit {
        "the seats can satisfy the declarations"
    } else {
        "a seat cannot satisfy a declaration: the findings below say which"
    };
    let capacity_tone = tone(Some(layers.capacity_fit), Role::Bad);
    layer(
        sheet,
        Some(layers.capacity_fit),
        "CAPACITY FIT",
        capacity,
        capacity_tone,
        &[],
    );
    if let Some(why) = run_unknown {
        unjudged(sheet, &["RUN READY"], why);
        return;
    }
    let run = layers.run_ready();
    let reason = match run {
        Some(true) => "nothing known blocks a run",
        Some(false) if layers.blockers.is_empty() => "not ready: a layer above says why",
        Some(false) => "blocked by",
        None => "unknown until admission judges the model",
    };
    layer(
        sheet,
        run,
        "RUN READY",
        reason,
        tone(run, Role::Warn),
        &layers.blockers,
    );
}

/// One finding row: its code (or class) in red, the engine's words, and
/// where it points, under a hanging indent. `advisory` rows are dim.
fn finding(sheet: &mut Sheet, row: &Finding<'_>, advisory: bool) -> bool {
    let canvas = sheet.body.canvas();
    let dot = cells::sep(canvas.ascii);
    let (lead, head) = if advisory {
        (plain("  "), Role::Dim)
    } else {
        (
            paint(
                format!("{} ", mark(Some(false), canvas.ascii)),
                Role::Bad,
                canvas.color,
            ),
            Role::Bad,
        )
    };
    let name = row.code.unwrap_or(row.class);
    let words = cells::dots(&cells::clean(row.message, canvas.ascii).0, canvas.ascii);
    let words = if advisory {
        paint(words, Role::Dim, canvas.color)
    } else {
        plain(words)
    };
    let mut spans = vec![
        paint(cells::clean(name, canvas.ascii).0, head, canvas.color),
        paint(dot, Role::Dim, canvas.color),
        words,
    ];
    if let Some(place) = row.place {
        let place = cells::dots(&cells::clean(place, canvas.ascii).0, canvas.ascii);
        spans.push(paint(format!(" ({place})"), Role::Dim, canvas.color));
    }
    sheet.body.wrap(&spans, &[lead], &[plain("   ")], false)
}

/// A section header: its name strong and its count dim.
fn section(sheet: &mut Sheet, name: &str, count: usize, note: &str) {
    let canvas = sheet.body.canvas();
    let dot = cells::sep(canvas.ascii);
    sheet.body.push(Vec::new(), false);
    sheet.body.push(
        vec![
            paint(name.to_owned(), Role::Strong, canvas.color),
            paint(format!("{dot}{count}{note}"), Role::Dim, canvas.color),
        ],
        false,
    );
}

/// Rows for layers nothing judged, each saying why.
fn unjudged(sheet: &mut Sheet, names: &[&str], reason: &str) {
    for name in names {
        layer(sheet, None, name, reason, Role::Dim, &[]);
    }
}

/// The layer rows of a verdict, as its owner projected it.
fn verdict_rows(sheet: &mut Sheet, verdict: Verdict<'_>) {
    let later = ["ACCESS READY", "CAPACITY FIT", "RUN READY"];
    match verdict {
        Verdict::Layers(computed) => layers(sheet, computed, None),
        Verdict::ParentOnly {
            layers: computed,
            unknown,
        } => {
            unjudged(sheet, &["IMPORTS"], unknown);
            layers(
                sheet,
                computed,
                Some("unknown: what this file imports was not captured"),
            );
        }
        Verdict::SourceOnly { valid } => {
            let reason = if valid {
                "the definition is legal"
            } else {
                "the definition breaks a rule: the findings below say which"
            };
            let hue = tone(Some(valid), Role::Bad);
            layer(sheet, Some(valid), "VALID", reason, hue, &[]);
            unjudged(sheet, &later, "judged when the workflow is saved");
        }
        Verdict::Refused { code, message } => {
            let refusal = "the parser refused the bytes";
            layer(sheet, Some(false), "VALID", refusal, Role::Bad, &[]);
            finding(
                sheet,
                &Finding::new(Some(code), "parse", message, None),
                false,
            );
            unjudged(sheet, &later, "not judged: the bytes do not parse");
        }
        Verdict::NotJudged => {
            let all = ["VALID", "ACCESS READY", "CAPACITY FIT", "RUN READY"];
            unjudged(sheet, &all, "not judged: nothing checked these bytes yet");
        }
    }
}

/// The findings, then the advisory hints, each under its header.
fn listed(sheet: &mut Sheet, input: &Workflow<'_>) {
    for (rows, name, note, advisory) in [
        (input.findings, "findings", "", false),
        (input.hints, "hints", " advisory, never a refusal", true),
    ] {
        if rows.is_empty() {
            continue;
        }
        section(sheet, name, rows.len(), note);
        for row in rows {
            if !finding(sheet, row, advisory) {
                break;
            }
        }
    }
}

/// The check face: the layers, the findings with their codes, the hints,
/// and the reminder that a static check ran nothing.
fn check(sheet: &mut Sheet, input: &Workflow<'_>) {
    let canvas = sheet.body.canvas();
    let dot = cells::sep(canvas.ascii);
    verdict_rows(sheet, input.verdict);
    listed(sheet, input);
    sheet
        .facts
        .push(cells::count(input.findings.len(), "finding"));
    if let Some(risk) = input.risk {
        let risk = cells::clean(risk, canvas.ascii).0;
        sheet.facts.push(format!("risk {risk}"));
    }
    sheet.body.push(Vec::new(), false);
    let footer = format!("a static check{dot}nothing ran, and a green check grants no run");
    sheet
        .body
        .push(vec![paint(footer, Role::Dim, canvas.color)], false);
}
