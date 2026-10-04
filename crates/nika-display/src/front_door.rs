// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Passive projections of the composed welcome front door.
//!
//! The host owns collection, next-action selection, admission, policy and
//! persistence. This module renders its already selected facts without I/O.

use std::fmt::Write as _;

use crate::theme::{Role, Theme};

mod choice;
/// Pure doctor report presentation over host-prepared cells.
pub mod doctor;
pub use choice::{AcpRuntime, InferenceChoice, Rung};

/// What the current directory already holds — the workspace half of the
/// mirror (the machine half is the probe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glance {
    /// Inside a git repository (any ancestor carries `.git`).
    pub git: bool,
    /// `*.nika` / `*.nika` files under the directory (bounded walk).
    pub workflows: usize,
    /// An `AGENTS.md` sits at the root — the repo's agents are briefed.
    pub agents_md: bool,
    /// The walk finished (P0-4): `false` = the count above is a LOWER
    /// BOUND (budget died · unreadable dir), and zero is UNKNOWN — the
    /// stranger's claims (« no workflows yet » · the sample) are gated
    /// on this flag.
    pub complete: bool,
}

/// Counts DERIVED from the embedded surfaces at call time — never typed by
/// hand, so they cannot drift from the binary that prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCounts {
    pub builtins: usize,
    pub locals: usize,
    pub clouds: usize,
    pub examples: usize,
    pub templates: usize,
}

/// The envelope facts the mirror renders (P0-14 · W2) — the envelope
/// itself stays in the host's `context_envelope`; the mirror eats this small owned
/// view. The legacy render ([`ContextView::legacy`]) is Workspace with an
/// EMPTY root: the workspace row then keeps its historical shape, so the
/// pre-envelope render tests stay byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextView {
    /// The host's already resolved chat-only presentation, never host authority.
    pub chat_only: bool,
    /// The RESOLVED root the workspace row names (display form) — empty
    /// in chat-only and in the legacy render.
    pub root: String,
    /// The subdir the candidate expanded FROM, when it sat below the
    /// root, already abbreviated by the host — displayed without reinterpretation.
    pub expanded_from: Option<String>,
}

impl ContextView {
    /// The pre-envelope render: workspace mode, no root segment.
    #[must_use]
    pub fn legacy() -> Self {
        Self {
            chat_only: false,
            root: String::new(),
            expanded_from: None,
        }
    }
}

/// Host-derived facts for the front door; no collection or decision runs here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineView {
    pub version: String,
    pub clients: Vec<(String, bool)>,
    /// The host has already selected the first editor and the exact command.
    pub wire_hint: Option<String>,
    pub local_providers: usize,
    /// Provider id, user-info-redacted endpoint, and authoritative locus label.
    pub endpoints: Vec<(String, String, String)>,
    pub model_count: usize,
    pub model_size: String,
    pub state_metric: String,
    pub state_cta: &'static str,
    pub drifted_kits: Vec<String>,
    pub wired_facet: String,
}

/// The wired/unwired glyph pair — ✓/✗ with an ASCII column (`+`/`x`),
/// painted Good/Dim: an unwired editor is an opportunity, never a
/// failure (Bad stays the run-verdict red, nothing here earns it).
fn mark(theme: Theme, on: bool) -> String {
    let raw = match (theme.ascii, on) {
        (false, true) => "✓",
        (false, false) => "✗",
        (true, true) => "+",
        (true, false) => "x",
    };
    theme.paint(if on { Role::Good } else { Role::Dim }, raw)
}

/// One client's cell in the editors row (`cursor ✓` · `vscode ✗`).
fn client_cell(theme: Theme, c: &(String, bool)) -> String {
    format!("{} {}", c.0, mark(theme, c.1))
}

/// The six-line taste of the language — shown ONLY when the workspace has
/// zero workflows (the stranger's moment; a workspace with files already
/// knows). The SAME shape as the embedded `01-hello` example, so the
/// START block's `nika try 01-hello` runs exactly what the eye
/// just read — a test pins that the sample checks clean for real.
pub const SAMPLE: &str = r#"nika: hello
model: mock/echo
tasks:
  greet:
    infer: { prompt: "say hello to the operator", max_tokens: 50 }"#;

/// The envelope-aware mirror — the one `run_in` renders with.
#[must_use]
pub fn render_with_context(
    probe: &MachineView,
    glance: Glance,
    counts: EngineCounts,
    ctx: &ContextView,
    theme: Theme,
) -> String {
    let mut s = String::new();
    identity_section(&mut s, probe, theme);
    machine_section(&mut s, probe, glance, ctx, theme);
    binary_section(&mut s, counts, glance, ctx, &probe.wired_facet, theme);
    learn_line(&mut s, theme);
    s
}

/// Who nika is — logo · version · the three-line identity.
fn identity_section(s: &mut String, probe: &MachineView, theme: Theme) {
    let _ = writeln!(
        s,
        "{} {} — Intent as Code. The workflow language for AI.",
        theme.logo(),
        theme.paint(Role::Strong, &format!("nika {}", probe.version)),
    );
    let _ = writeln!(
        s,
        "   one file · 4 verbs · one binary · audited BEFORE it runs"
    );
    let _ = writeln!(
        s,
        "   every run records a tamper-evident, hash-chained trace"
    );
    let _ = writeln!(s);
}

/// The hanging indent under `  editors    ` — continuation rows and the
/// wire handle line both sit under the label, never under the margin.
const EDITOR_HANG: &str = "             ";

/// Pack the client cells into rows that stay inside 80 display columns.
///
/// The roster is NOT a constant. Six hosts ship today, the registry
/// already names a seventh, and every addition silently widened this
/// row: on a real machine it measured **112 columns** under a published
/// 0.107.0 — a third of the line hanging off an 80-column terminal. The
/// ratchet that should have caught it was measuring a four-host fixture
/// the binary stopped matching two hosts ago (the same drift its own
/// doc comment records having learned once already, for providers).
///
/// So this measures instead of assuming, and it measures the PLAIN
/// width (`id` + space + one mark glyph) — the painted cell carries
/// zero-width escapes that would make a colour terminal wrap early.
fn editor_rows(probe: &MachineView, theme: Theme) -> Vec<String> {
    let cells: Vec<(usize, String)> = probe
        .clients
        .iter()
        .map(|c| (c.0.chars().count() + 2, client_cell(theme, c)))
        .collect();
    pack_cells(&cells)
}

/// The shared column budget. Eighty is the one terminal width nobody
/// configures, so it is the one every row has to survive.
const LIMIT: usize = 80;

/// Pack `(plain_width, painted)` cells into rows that fit under a
/// 13-column label with a matching hanging indent.
///
/// Every caller here renders a list whose length is DATA, not a
/// constant — the host roster, the drifted kits — and a row that
/// assumes its data is short is a row that breaks on somebody else's
/// machine. The width is taken from the plain text because the painted
/// cell carries zero-width escapes.
fn pack_cells(cells: &[(usize, String)]) -> Vec<String> {
    let hang = EDITOR_HANG.chars().count();
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    let mut used = hang;
    for (plain, painted) in cells {
        if !row.is_empty() && used + 3 + plain > LIMIT {
            rows.push(std::mem::take(&mut row));
            used = hang;
        }
        if !row.is_empty() {
            row.push_str(" · ");
            used += 3;
        }
        row.push_str(painted);
        used += plain;
    }
    // An empty list still owns its label row — the mirror shows an
    // empty cell, it never lets the label vanish.
    if rows.is_empty() || !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// The machine half of the mirror — editors · local · keys · the P0-14
/// session row ([`session_row`]).
fn machine_section(
    s: &mut String,
    probe: &MachineView,
    glance: Glance,
    ctx: &ContextView,
    theme: Theme,
) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this machine"));
    for (i, row) in editor_rows(probe, theme).iter().enumerate() {
        let _ = writeln!(
            s,
            "{}{row}",
            if i == 0 { "  editors    " } else { EDITOR_HANG }
        );
    }
    if let Some(line) = &probe.wire_hint {
        let _ = writeln!(s, "{EDITOR_HANG}{}", theme.paint(Role::Dim, line));
    }
    local_provider_lines(s, probe, theme);
    sovereign_and_keys_lines(s, probe, glance, ctx, theme);
}

/// The keyless rows: the local-provider summary line + the P0-20
/// endpoint rows (an override moves « local » off the box — the engine
/// is NAMED with endpoint + locus, never laundered under « no key
/// needed »; loopback stays silent, the default render keeps its bytes).
fn local_provider_lines(s: &mut String, probe: &MachineView, theme: Theme) {
    if probe.local_providers == 0 {
        let _ = writeln!(s, "  local      no local providers in this build");
    } else {
        // This row lives under « this machine », and it used to read as
        // an inventory: `ollama · lmstudio · llamacpp · localai · vllm`
        // with nothing listening on any of them (gauntlet P2 · B15). The
        // names are what the BINARY supports; the header promises what the
        // MACHINE has. Say the count and the probe, and the row stops
        // claiming a server that is not there — `nika catalog` still
        // names them, where naming them is true.
        let _ = writeln!(
            s,
            "  local      {} keyless engines supported {}",
            probe.local_providers,
            theme.paint(Role::Dim, "· none probed → nika doctor --ping"),
        );
    }
    for (id, endpoint, locus) in &probe.endpoints {
        let _ = writeln!(s, "  endpoint   {id} → {endpoint} ({locus})");
    }
}

/// The tail of the machine section (post-provider rows).
fn sovereign_and_keys_lines(
    s: &mut String,
    probe: &MachineView,
    glance: Glance,
    ctx: &ContextView,
    theme: Theme,
) {
    // The sovereign lane — ONLY when bytes are on disk (a mirror line
    // must carry information, never a lecture; zero models = silence).
    if probe.model_count > 0 {
        let _ = writeln!(
            s,
            "  models     {} pulled · {} on disk {}",
            probe.model_count,
            probe.model_size,
            theme.paint(Role::Dim, "· nika model list"),
        );
    }
    // P0-21 — the adoption rung replaces the raw key ratio: ONE state,
    // its own metric, its own CTA (the same classifier doctor --json
    // serializes — one truth, two voices).
    let _ = writeln!(
        s,
        "  state      {} {}",
        probe.state_metric,
        theme.paint(Role::Dim, &format!("— {}", probe.state_cta)),
    );
    // The plugin-kit lane — ONLY on train drift (an aligned or absent
    // kit is silence; the same carry-information-never-lecture law as
    // the models row · the per-client fix lives in doctor).
    let drifted = &probe.drifted_kits;
    if !drifted.is_empty() {
        // Same width law as the editors roster: the drifted list is
        // data, not a constant. Three drifted kits plus the binary
        // version plus the handle measured 100 columns on a normal
        // machine — the verdict and the handle take the hanging line.
        let cells: Vec<(usize, String)> = drifted
            .iter()
            .map(|d| (d.chars().count(), d.clone()))
            .collect();
        for (i, row) in pack_cells(&cells).iter().enumerate() {
            let _ = writeln!(
                s,
                "{}{row}",
                if i == 0 { "  kits       " } else { EDITOR_HANG }
            );
        }
        let _ = writeln!(
            s,
            "{EDITOR_HANG}{}",
            theme.paint(
                Role::Dim,
                &format!("vs binary {} · fixes → nika doctor", probe.version),
            ),
        );
    }
    // `session_row` closes the section with its own blank line — a
    // second one here rendered two, and the screen is meant to be the
    // short one (#1196).
    session_row(s, glance, ctx, theme);
}

/// The P0-14 row closing the machine section: chat-only SAYS « chat
/// only » and claims nothing (no git bit, no count, no agents row —
/// there is no workspace here); workspace names the RESOLVED root and
/// traces the subdir expansion (displayed, never silent).
fn session_row(s: &mut String, glance: Glance, ctx: &ContextView, theme: Theme) {
    if ctx.chat_only {
        let _ = writeln!(s, "  session    chat only — no reliable project detected");
    } else {
        // The root is a PATH — the one field in this whole screen
        // whose width belongs to the person, not to us. A normal
        // checkout rendered this row at 150 columns. So the facts
        // move under the label when the two cannot share a line;
        // the path is never truncated (a half-path is a lie about
        // where you are).
        let facts = workspace_facts(glance, theme);
        let plain = workspace_facts(glance, Theme::new(false, false, false));
        let root_w = ctx.root.chars().count();
        if ctx.root.is_empty() {
            let _ = writeln!(s, "  workspace  {facts}");
        } else if EDITOR_HANG.chars().count() + root_w + 3 + plain.chars().count() <= LIMIT {
            let _ = writeln!(s, "  workspace  {} · {facts}", ctx.root);
        } else {
            let _ = writeln!(s, "  workspace  {}", ctx.root);
            let _ = writeln!(s, "{EDITOR_HANG}{facts}");
        }
        // The expansion trace carries a SECOND path — the subdir
        // the person actually stood in. Two unbounded paths never
        // share a line (this one rendered at 161 columns beside the
        // facts); it earns its own, and its own `~`.
        if let Some(from) = &ctx.expanded_from {
            let _ = writeln!(
                s,
                "{EDITOR_HANG}{}",
                theme.paint(Role::Dim, &format!("from {from}"),)
            );
        }
    }
    let _ = writeln!(s);
}

/// The workspace row's facts — git bit · workflow count · agents brief
/// · the subdir expansion when there was one. Built twice per render
/// (painted for the eye, plain to MEASURE), because a painted string
/// carries escapes that lie about its width.
fn workspace_facts(glance: Glance, theme: Theme) -> String {
    let mut s = String::new();
    {
        let s = &mut s;
        let _ = write!(
            s,
            "git {} · {} · agents {}",
            mark(theme, glance.git),
            match (glance.workflows, glance.complete) {
                // P0-4: « no workflows yet » is a claim only a COMPLETE scan
                // may make; a truncated walk renders the honest lower bound.
                (0, true) => "no workflows yet".to_owned(),
                (0, false) => "0 found · scan partial".to_owned(),
                (1, true) => "1 workflow".to_owned(),
                (n, true) => format!("{n} workflows"),
                (n, false) => format!("{n}+ found · scan partial"),
            },
            if glance.agents_md {
                format!("briefed {} (AGENTS.md)", mark(theme, true))
            } else {
                format!("not briefed {}", theme.paint(Role::Dim, "→ nika init"))
            }
        );
    }
    s
}

/// What this binary carries — derived counts, and the six-line taste of
/// the language itself (first contact only — chat-only included: the
/// isolated example IS the sample's run).
fn binary_section(
    s: &mut String,
    counts: EngineCounts,
    glance: Glance,
    ctx: &ContextView,
    wired_facet: &str,
    theme: Theme,
) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this binary"));
    let _ = writeln!(
        s,
        "  4 verbs · {} builtins · {} providers · {} examples · {} templates",
        counts.builtins,
        counts.locals + counts.clouds,
        counts.examples,
        counts.templates
    );
    // #1398 — the facet's split on its own line (the card keeps its
    // eighty columns): the same sentence the catalog header and the
    // check refusal print, so the three surfaces cannot disagree.
    let _ = writeln!(s, "  {wired_facet}");
    let _ = writeln!(s);
    // The stranger's moment is gated on a COMPLETE zero (P0-4) — a
    // partial scan cannot know the workspace is empty. Chat-only holds
    // no scan at all: the sample rides as the isolated example instead.
    if ctx.chat_only || (glance.workflows == 0 && glance.complete) {
        let _ = writeln!(
            s,
            "{}",
            theme.paint(Role::Strong, "a whole workflow is one file")
        );
        for line in SAMPLE.lines() {
            let _ = writeln!(s, "  {line}");
        }
        let _ = writeln!(s);
    }
}

/// Where to learn more — the tail of the body.
///
/// This used to sit under a `start here` menu of three commands. The
/// cascade above already carries the ONE next step, and that menu was
/// precisely what the first-wow cascade replaced: keeping both put the
/// fork back in front of a stranger (#1196).
fn learn_line(s: &mut String, theme: Theme) {
    let _ = writeln!(
        s,
        "{}",
        theme.paint(
            Role::Dim,
            &format!(
                "learn: {} · docs: {} · ⭐ {}",
                theme.link("https://nika.sh", "nika.sh"),
                theme.link("https://docs.nika.sh", "docs.nika.sh"),
                theme.link(
                    "https://github.com/supernovae-st/nika",
                    "github.com/supernovae-st/nika"
                ),
            )
        )
    );
}

/// The versioned machine mirror — additive-only (`welcome_version: 1`).
/// Names and booleans and counts, by construction: nothing in the probe
/// carries a value a secret could ride.
#[must_use]
pub fn render_json(
    version: &str,
    machine: serde_json::Value,
    glance: Glance,
    counts: EngineCounts,
    experience: serde_json::Value,
    next: &str,
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "welcome_version": 1,
        "version": version,
        "workspace": {
            "git": glance.git,
            "workflows": glance.workflows,
            "agents_md": glance.agents_md,
            "inventory_complete": glance.complete,
        },
        "engine": {
            "verbs": 4,
            "builtins": counts.builtins,
            "local_providers": counts.locals,
            "cloud_providers": counts.clouds,
            "examples": counts.examples,
            "templates": counts.templates,
        },
        // ONE next step, the same string the `Next:` block prints. It
        // was a three-command array that had met neither the cascade
        // nor the cwd key — an agent reading `start[0]` on 0.115 was
        // handed the retired door (#1187). `start` stays an array so a
        // consumer indexing it still works; it just tells the truth now.
        "next": next,
        "start": [next],
    });
    v["machine"] = machine;
    v["experience"] = experience;
    v
}

/// The chat-only machine mirror — the SAME versioned envelope, minus
/// every workspace claim (there is none to make), plus the mode and the
/// two doors the spec allows. Additive against `welcome_version: 1`: the
/// `context` key is the chat-only signal.
#[must_use]
pub fn render_chat_only_json(
    version: &str,
    machine: serde_json::Value,
    counts: EngineCounts,
    experience: serde_json::Value,
    next: &str,
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "welcome_version": 1,
        "version": version,
        "context": { "mode": "chat_only" },
        "engine": {
            "verbs": 4,
            "builtins": counts.builtins,
            "local_providers": counts.locals,
            "cloud_providers": counts.clouds,
            "examples": counts.examples,
            "templates": counts.templates,
        },
        // ONE source with the rendered text (the W8 law): the machine
        // mirror is handed the same string the `Next:` block prints,
        // never a hand-kept twin — two lists of the same moves drift.
        "next": next,
        "start": [next],
    });
    v["machine"] = machine;
    v["experience"] = experience;
    v
}

#[cfg(test)]
mod tests;
