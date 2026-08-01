// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The experience contracts — one versioned state, one deterministic
//! router, one preview shape.
//!
//! The 0.107 train shipped the trust OBJECTS (context envelope, risk
//! grade, data journey, receipts) but every surface still composes its
//! own next step. This module is the missing composition seam: hosts
//! fill an [`ExperienceStateV1`] from facts they can PROVE, the pure
//! [`route`] table folds it into exactly one [`NextActionV1`] (one
//! primary CTA · at most two safe alternatives · the why), and any
//! mutation shows a [`WorkflowPreviewV1`] first. The router never
//! guesses a root, never proposes a run on anything but a clean exact
//! file, and never hides an execution behind a door label — the same
//! laws the concierge already obeys, now computable by every surface
//! from ONE table instead of N hand-rolled ladders.
//!
//! Pure by construction: no I/O, no clock, no host branding — a
//! projection (CLI text, Mermaid, host JSON) renders the OUTPUT of
//! this module, it never re-decides.

use serde::Serialize;

/// Where the session believes work happens — and whether that belief
/// is proof or absence. `ChatOnly` is a normal, first-class state: no
/// root was proven, so nothing may be scanned, written or handed off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextModeV1 {
    /// No proven workspace — design in chat, never touch a disk.
    ChatOnly,
    /// Exactly one proven root.
    Workspace,
    /// Several roots are open and none was explicitly selected.
    MultiRootUnselected,
}

/// The evidence behind the root claim, in trust order. A process cwd
/// is NEVER evidence (the 0.106 audit's inherited-hook-cwd class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextEvidenceV1 {
    /// The host explicitly selected this root for the session.
    HostSelection,
    /// The active editor file proves a candidate root.
    ActiveFile,
    /// The operator named the root in the conversation.
    ExplicitUser,
    /// An attachment exists — it proves ITSELF, never a repository.
    Attachment,
    /// Nothing proves any root.
    None,
}

/// The exact workflow situation the next step must serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStateV1 {
    /// No workflow exists under the proven root.
    Absent,
    /// The inventory could not finish — zero is UNKNOWN, not zero.
    Unknown,
    /// Exactly one workflow, and its last check on THIS content is clean.
    Clean,
    /// Several workflows live under the root — a chooser, never a
    /// silent first-pick.
    Several,
    /// The workflow carries live findings.
    Findings,
    /// The latest run paused on a human decision.
    Paused,
    /// The latest run failed.
    Failed,
}

/// How ready the model lane actually is — recognition is never
/// readiness (CORE-05), and a key present is configured, not proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStateV1 {
    /// No provider is configured at all.
    Unconfigured,
    /// A key or endpoint is configured — nothing was called.
    Configured,
    /// The endpoint answered a liveness probe.
    Reachable,
    /// A real run already succeeded on this machine.
    RunProven,
}

/// The human learning stage — SEPARATE from machine readiness
/// (UX107-08): `guarded` says what the machine has, never what the
/// person understands. Stages only advance on verifiable events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyStageV1 {
    /// Can say what Nika is for.
    Orientation,
    /// Saw input → steps → output and predicted it.
    Previewed,
    /// Holds a clean draft and can tell check from run.
    Checked,
    /// Chose model, locus and cost knowingly.
    Configured,
    /// Asked for and completed a real run.
    Ran,
    /// Opened or verified a trace.
    Traced,
    /// Second success with a changed input.
    Repeated,
    /// Caps, permits, CI and receipts in place.
    Hardened,
}

/// What the person is trying to do right now, when it is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentClassV1 {
    /// A repeated task was described — creation is wanted.
    Create,
    /// Curiosity without a task — show, never own.
    Discover,
    /// Return to existing state.
    Continue,
    /// Nothing stable yet.
    Unknown,
}

/// The versioned session state a host fills from PROVEN facts only —
/// `Unknown`/`None` are honest values, never gaps to guess over.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExperienceStateV1 {
    /// Additive-only schema version (`1`).
    pub schema_version: u32,
    /// Where work happens, per the evidence below.
    pub context_mode: ContextModeV1,
    /// The proof class behind `context_mode`.
    pub evidence: ContextEvidenceV1,
    /// The proven root, when one exists (display path).
    pub root: Option<String>,
    /// The proven root accepts writes (`false` covers read-only AND
    /// unknown — narrow claims only).
    pub writable: bool,
    /// The workspace inventory finished (P0-4: a truncated zero is
    /// `WorkflowStateV1::Unknown`, never `Absent`).
    pub inventory_complete: bool,
    /// The workflow situation under the proven root.
    pub workflow: WorkflowStateV1,
    /// The exact workflow path the state speaks about, when one does.
    pub workflow_path: Option<String>,
    /// Live finding count when `workflow == Findings`.
    pub findings: u32,
    /// The model lane's proven rung.
    pub provider: ProviderStateV1,
    /// Binary and installed kit ride the same train.
    pub versions_coherent: bool,
    /// The person's current learning stage (local, event-proven).
    pub journey: JourneyStageV1,
    /// The declared intent, when the person said one.
    pub intent: IntentClassV1,
}

impl ExperienceStateV1 {
    /// The honest zero state: chat-only, nothing proven, nothing known.
    #[must_use]
    pub fn chat_only() -> Self {
        Self {
            schema_version: 1,
            context_mode: ContextModeV1::ChatOnly,
            evidence: ContextEvidenceV1::None,
            root: None,
            writable: false,
            inventory_complete: false,
            workflow: WorkflowStateV1::Unknown,
            workflow_path: None,
            findings: 0,
            provider: ProviderStateV1::Unconfigured,
            versions_coherent: true,
            journey: JourneyStageV1::Orientation,
            intent: IntentClassV1::Unknown,
        }
    }
}

/// The router's one answer: a primary action, at most two safe
/// alternatives, and the reason — never a mutation, never a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextActionV1 {
    /// Stable machine id (`choose_project` · `repair_findings` …).
    pub action_id: &'static str,
    /// The human label (EN seed — localization keys off `action_id`).
    pub label: &'static str,
    /// One sentence: why THIS action fits THIS state now.
    pub why_now: &'static str,
    /// A preview must be shown before this action mutates anything.
    pub preview_required: bool,
    /// A human consent gate stands between this action and any effect.
    pub consent_required: bool,
    /// What blocks the richer action this one degrades from.
    pub blocked_by: Vec<&'static str>,
    /// At most two safe exits beside the primary.
    pub alternatives: Vec<&'static str>,
    /// Always `true` from this router: selecting an action runs nothing.
    pub nothing_has_run: bool,
}

impl NextActionV1 {
    fn new(action_id: &'static str, label: &'static str, why_now: &'static str) -> Self {
        Self {
            action_id,
            label,
            why_now,
            preview_required: false,
            consent_required: false,
            blocked_by: Vec::new(),
            alternatives: Vec::new(),
            nothing_has_run: true,
        }
    }

    fn preview_gated(mut self) -> Self {
        self.preview_required = true;
        self.consent_required = true;
        self
    }

    fn blocked(mut self, by: &'static str) -> Self {
        self.blocked_by.push(by);
        self
    }

    fn alt(mut self, alternative: &'static str) -> Self {
        if self.alternatives.len() < 2 {
            self.alternatives.push(alternative);
        }
        self
    }
}

/// The deterministic decision table — ONE primary action per state,
/// most-specific first. Order IS the contract:
///
/// 1. an incoherent train degrades every richer CTA (UX107-02);
/// 2. an unselected multi-root must be resolved before any scan;
/// 3. chat-only routes to choosing a project or discovering isolated;
/// 4. a paused run outranks everything under a proven root (J-18);
/// 5. a failed run is understood before anything new (J-17 · CORE-12);
/// 6. findings are repaired — never run, never badged ready (J-16);
/// 7. an unknown inventory tells the truth before any founding CTA;
/// 8. a clean workflow opens (the run stays a human ask behind its
///    own check-gated door — this router never hands a run);
/// 9. no workflow + create intent previews the draft (write-gated on
///    a writable root);
/// 10. everything else discovers safely.
#[must_use]
pub fn route(state: &ExperienceStateV1) -> NextActionV1 {
    if !state.versions_coherent {
        return NextActionV1::new(
            "align_versions",
            "Align Nika versions",
            "The binary and an installed kit ride different trains — richer moves would teach a stale language.",
        )
        .blocked("version_drift")
        .alt("discover_example");
    }
    if state.context_mode == ContextModeV1::MultiRootUnselected {
        return NextActionV1::new(
            "select_root",
            "Choose the project root",
            "Several roots are open and none is selected — nothing is scanned or written until one is.",
        )
        .alt("discover_example");
    }
    if state.context_mode == ContextModeV1::ChatOnly {
        return match state.intent {
            IntentClassV1::Create => NextActionV1::new(
                "choose_project",
                "Choose a project",
                "A repeated task wants a home — connect the folder Nika should work in (nothing is scanned before that).",
            )
            .alt("discover_example")
            .alt("preview_in_chat"),
            _ => NextActionV1::new(
                "discover_example",
                "Discover Nika with a safe example",
                "No project is attached — explore an isolated example without touching any file.",
            )
            .alt("choose_project"),
        };
    }
    match state.workflow {
        // Consent-gated: the reply IS what resumes execution of the
        // remaining tasks — the one arm most analogous to a run, so it
        // carries the same explicit human gate as every mutating door
        // (a refuter pass caught it shipping ungated).
        WorkflowStateV1::Paused => {
            let mut action = NextActionV1::new(
                "answer_pending_decision",
                "Answer the pending decision",
                "A run is paused on a question only a human may answer — your reply resumes the remaining tasks.",
            )
            .alt("understand_latest_failure");
            action.consent_required = true;
            action
        }
        WorkflowStateV1::Failed => NextActionV1::new(
            "understand_latest_failure",
            "Understand the latest failure",
            "The newest trace failed — cause, blocked tasks and the smallest repair come before anything new runs.",
        )
        .alt("repair_findings"),
        WorkflowStateV1::Findings => NextActionV1::new(
            "repair_findings",
            "Repair this workflow",
            "Live findings block the file — nothing has been executed, and no run exists until the exact file is clean.",
        )
        .preview_gated()
        .alt("understand_findings"),
        WorkflowStateV1::Unknown => NextActionV1::new(
            "deep_scan",
            "See the workspace truth",
            "The inventory could not finish — zero is unknown here, so the deep scan speaks before any founding move.",
        )
        .alt("discover_example"),
        WorkflowStateV1::Clean => NextActionV1::new(
            "open_workflow",
            "Open your workflow",
            "One clean workflow lives here — open it; running stays your explicit ask behind its own check gate.",
        )
        .alt("check_workflow")
        .alt("discover_example"),
        WorkflowStateV1::Several => NextActionV1::new(
            "open_project_workflows",
            "Open Nika in this project",
            "Several workflows live here — the whole-workspace view chooses by goal, never by first file.",
        )
        .alt("discover_example"),
        WorkflowStateV1::Absent => match state.intent {
            IntentClassV1::Create if state.writable => NextActionV1::new(
                "preview_draft",
                "See the proposed workflow",
                "Your repeated task routes to a plan — the preview shows inputs, steps and effects before any file exists.",
            )
            .preview_gated()
            .alt("discover_example"),
            IntentClassV1::Create => NextActionV1::new(
                "preview_in_chat",
                "Preview the plan in chat",
                "This root is not writable — the plan renders here, and you choose a writable destination for the draft.",
            )
            .blocked("root_read_only")
            .alt("choose_project"),
            _ => NextActionV1::new(
                "discover_example",
                "Discover Nika with a safe example",
                "No workflow exists here yet — see one work in isolation, then decide what this project needs.",
            )
            .alt("preview_draft"),
        },
    }
}

/// The cost lane a preview may speak — priced ceiling, local
/// resources, or honestly unknown. `Unpriced` is NEVER worded free.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PreviewCostV1 {
    /// A cataloged price with its worst-case output ceiling.
    Ceiling {
        /// Worst-case output spend in USD.
        usd: f64,
    },
    /// Local compute: unpriced, pays in time and machine resources.
    Unpriced,
    /// No basis to state a bound.
    Unknown,
}

/// The one preview every mutating door shows first — business words,
/// exact effects, honest cost, and the standing fact that nothing ran.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowPreviewV1 {
    /// Additive-only schema version (`1`).
    pub schema_version: u32,
    /// The outcome in the person's words.
    pub goal: String,
    /// What flows in.
    pub inputs: Vec<String>,
    /// The business steps, in order (verbs come later).
    pub steps: Vec<String>,
    /// The useful artifact out.
    pub output: String,
    /// Exact paths read.
    pub reads: Vec<String>,
    /// Exact paths written.
    pub writes: Vec<String>,
    /// Network destinations, when any.
    pub network: Vec<String>,
    /// The cost lane.
    pub cost: PreviewCostV1,
    /// Standing truth: rendering a preview executes nothing.
    pub nothing_has_run: bool,
}

impl WorkflowPreviewV1 {
    /// The Mermaid digest for rich chat surfaces — the SAME semantic
    /// content as the ASCII twin (business labels lead, effects ride a
    /// legend), projected as a `flowchart LR`. Labels are quoted and
    /// double quotes stripped so a step name can never break out of
    /// its node.
    #[must_use]
    pub fn render_mermaid(&self) -> String {
        use std::fmt::Write as _;
        let clean = |s: &str| s.replace('"', "'");
        let mut m = String::from("flowchart LR\n");
        let _ = writeln!(m, "    in[\"{}\"]", clean(&self.inputs.join(", ")));
        let mut prev = "in".to_owned();
        for (i, step) in self.steps.iter().enumerate() {
            let id = format!("s{}", i + 1);
            let _ = writeln!(m, "    {id}[\"{}\"]", clean(step));
            let _ = writeln!(m, "    {prev} --> {id}");
            prev = id;
        }
        let _ = writeln!(m, "    out[\"{}\"]", clean(&self.output));
        let _ = writeln!(m, "    {prev} --> out");
        m
    }

    /// The strict-ASCII projection (CLI `--plain` and transcripts):
    /// input, steps, output, then the four effect lines — no glyph a
    /// legacy terminal cannot carry.
    #[must_use]
    pub fn render_ascii(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(s, "goal    {}", self.goal);
        let _ = writeln!(s, "input   {}", self.inputs.join(", "));
        for (i, step) in self.steps.iter().enumerate() {
            let _ = writeln!(s, "step {}  {}", i + 1, step);
        }
        let _ = writeln!(s, "output  {}", self.output);
        let _ = writeln!(s, "reads   {}", join_or_none(&self.reads));
        let _ = writeln!(s, "writes  {}", join_or_none(&self.writes));
        let _ = writeln!(s, "network {}", join_or_none(&self.network));
        let cost = match &self.cost {
            PreviewCostV1::Ceiling { usd } => format!("ceiling <= ${usd:.2} (output worst-case)"),
            PreviewCostV1::Unpriced => "unpriced (local compute: time + machine)".to_owned(),
            PreviewCostV1::Unknown => "unknown".to_owned(),
        };
        let _ = writeln!(s, "cost    {cost}");
        let _ = writeln!(s, "state   nothing has run");
        s
    }
}

fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_owned()
    } else {
        items.join(", ")
    }
}

/// The context receipt — WHERE the agent works and how that belief was
/// earned, in one short line. Shown before the first scan, write or
/// handoff; a changed root mints a new receipt (never a silent carry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextReceiptV1 {
    /// Additive-only schema version (`1`).
    pub schema_version: u32,
    /// The proof class behind the root claim.
    pub source: ContextEvidenceV1,
    /// The proven root, when one exists.
    pub root: Option<String>,
    /// Writes are allowed under the root (`false` covers read-only AND
    /// unknown).
    pub writable: bool,
    /// The remote/container identity, when execution is not local.
    pub remote: Option<String>,
    /// Facts the receipt could NOT verify — named, never silently
    /// upgraded to claims.
    pub assumptions: Vec<String>,
}

impl ContextReceiptV1 {
    /// The one-line human rendering: root · locus · write scope, or the
    /// honest chat-only sentence when nothing is proven.
    #[must_use]
    pub fn render_line(&self) -> String {
        match &self.root {
            None => "Working context: no project attached — chat only, nothing is read or written."
                .to_owned(),
            Some(root) => {
                let locus = self.remote.as_deref().unwrap_or("local");
                let scope = if self.writable {
                    "writes allowed"
                } else {
                    "read-only"
                };
                format!("Working in {root} \u{b7} {locus} \u{b7} {scope}")
            }
        }
    }
}

/// How long a party retains data — `Unknown` is a first-class honest
/// value, never rounded to a comforting adjective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionV1 {
    /// A sourced, dated policy statement.
    Known(String),
    /// No verifiable policy — say so.
    Unknown,
}

/// The privacy receipt — what would leave the machine, to whom, and
/// what is knowably retained. Rendered BEFORE any cloud handoff; the
/// copy never says « private » or « secure » on top of an `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrivacyReceiptV1 {
    /// Additive-only schema version (`1`).
    pub schema_version: u32,
    /// The provider the data would travel to.
    pub provider: String,
    /// The model id, when one is chosen.
    pub model: Option<String>,
    /// The classes of data that would be sent (`prompt` ·
    /// `selected_source_files` …), never raw contents.
    pub data_classes: Vec<String>,
    /// What is explicitly excluded from the egress.
    pub excluded: Vec<String>,
    /// The network destinations.
    pub destinations: Vec<String>,
    /// The provider's retention, as far as it is verifiable.
    pub retention: RetentionV1,
}

/// The authority receipt — what the workflow's `permits:` actually
/// grant. An absent permit is ZERO authority: the `denied` lane names
/// what was refused, so a refusal is visible, never a silent gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorityReceiptV1 {
    /// Additive-only schema version (`1`).
    pub schema_version: u32,
    /// Paths the permits allow reading.
    pub reads: Vec<String>,
    /// Paths the permits allow writing.
    pub writes: Vec<String>,
    /// Network destinations the permits allow.
    pub network: Vec<String>,
    /// Effect classes the permits allow.
    pub effects: Vec<String>,
    /// What was requested or implied but NOT granted.
    pub denied: Vec<String>,
    /// Where the authority came from (`workflow.permits` — the one
    /// source; hosts never mint authority).
    pub source: String,
}

#[cfg(test)]
mod tests;
