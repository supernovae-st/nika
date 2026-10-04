// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika welcome` — the mirror moment (30-seconds surface · first contact).
//!
//! One command, one screen: what Nika IS (the tagline block), what THIS
//! machine already has (the shared [`crate::probe`] engine — the same
//! detection `doctor` diagnoses with · one truth, two voices), what this
//! BINARY carries (counts DERIVED live from the embedded pack/catalog,
//! never hardcoded — the born-stale law), and the three commands to run
//! next (offline first · zero keys).
//!
//! Always offline (no `--ping` here — that stays doctor's opt-in), always
//! exit `0` (a greeting is never a failure — even a bare machine gets
//! routed, not scolded), and PRESENCE-only like everything probe-backed:
//! no secret value exists in this module by construction. The ONE breach
//! of presence-only: with exactly ONE workflow on disk the concierge
//! audits THAT file in-process (parse + check ladder · bounded, never a
//! walk) before any `run` line may be printed (P0-3 · LOI-3). Re-runnable
//! anytime: welcome is a living mirror, not a splash screen.
//!
//! P0-14 rides the binary too (W2): the session context is resolved ONCE
//! through `context_envelope::resolve` at the top of `run` — the surface
//! hands the cwd it can name (never a process-cwd fallback), and a
//! chat-only envelope renders « chat only », never a workspace pretence.

use std::path::Path;

use nika_providers::probe::ExecutionLocus;

use crate::context_envelope::{self, ContextEnvelope, ContextMode, EnvFacts, EvidenceSource};
use crate::display::theme::Theme;
use crate::door::DoorId;
use crate::probe::Probe;
use crate::{output::VerbOutput, probe};
pub use nika_display::front_door::SAMPLE;
use nika_display::front_door::{self, ContextView, EngineCounts, Glance};

/// The ONE next command the first-contact screen promises.
///
/// Derived HERE and nowhere else. The screen used to answer twice —
/// the cascade's `Next:` block keyed on the directory listing, a
/// `start here` menu eleven lines below keyed on the audited state —
/// so a stranger was handed two different first steps (#1196), and
/// `--json` served a third that had never met either fix (#1187).
///
/// `door` is the directory's own answer ([`crate::choice::front_door_next`] ·
/// the cwd key that stopped `nika compile hello hello.nika` being taught into
/// `--force`, gauntlet P15). Everything below it is a VERDICT
/// overruling a listing, in the order the old menu ranked them:
/// P0-3 (a file the ladder has not seen clean is audited, never run),
/// LOI-3 (a priced run always bears its cap), P0-4 (a truncated walk
/// may not presume an empty workspace).
fn next_command(mode: ContextMode, glance: Glance, gate: Option<&RunGate>, door: &str) -> String {
    if mode == ContextMode::ChatOnly {
        // No folder to stand in, so no file claim is honest. The
        // isolated example is the one real answer reachable from here
        // — and it runs exactly what the sample block just showed.
        return format!("{} 01-hello", DoorId::Discover.command());
    }
    match gate {
        // P0-3 — the sole file is red: audit it, never run it.
        Some(g) if !g.proposable => format!("nika check {}", g.path),
        // LOI-3 — a priced run suggestion carries the cap, always.
        Some(g) if g.priced && door.starts_with("nika run") => {
            format!("{door} --max-cost-usd <usd>")
        }
        // P0-4 — a walk that died before it finished cannot hand out a
        // founding CTA that presumes an empty workspace.
        _ if !glance.complete && door.starts_with("nika compile") => {
            "nika welcome --deep".to_owned()
        }
        _ => door.to_owned(),
    }
}

/// The one-file verdict behind the run CTA (P0-3 · LOI-3) — computed
/// ONLY when the workspace carries exactly one workflow (the audit cost
/// is bounded to that file; the multi case keeps a generic CTA).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunGate {
    /// The root-relative path, for the `check <file>` CTA.
    path: String,
    /// The exact file parses AND the check ladder is clean — the ONLY
    /// condition under which welcome may print a `run` line.
    proposable: bool,
    /// At least one resolved task model carries a catalog price (LOI-3:
    /// a priced run suggestion always bears `--max-cost-usd`).
    priced: bool,
}

/// The chat-only glance: zero walk ran, so zero claim — every stranger
/// arm stays gated behind a COMPLETE scan this view never fakes.
const CHAT_ONLY_GLANCE: Glance = Glance {
    git: false,
    workflows: 0,
    agents_md: false,
    complete: false,
};

/// The root the workspace row names — the RESOLVED root, never the
/// as-typed candidate alone: a subdir candidate resolves up to the git
/// root (the evidence carries the subdir alongside, so the row shows
/// both).
fn root_label(envelope: &ContextEnvelope) -> String {
    let full = match (&envelope.evidence.expanded_from, &envelope.git_root) {
        (Some(_), Some(root)) => root.display().to_string(),
        _ => envelope.display_path.clone(),
    };
    under_home(&full)
}

/// Abbreviate a path under the home directory to `~/…`.
///
/// A path is the one field on this screen whose width belongs to the
/// person. Truncating it would be a lie about where they are, so the
/// only honest shortening is the one the shell itself understands:
/// `~` is lossless — it pastes back — and it keeps the operator's
/// account name off a screen people paste into issues.
fn under_home(path: &str) -> String {
    let Some(home) = probe::home_dir() else {
        return path.to_owned();
    };
    let home = home.to_string_lossy();
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return path.to_owned();
    }
    match path.strip_prefix(home) {
        Some("") => "~".to_owned(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_owned(),
    }
}

/// The `nika welcome` verb. `json` emits the versioned machine projection
/// (`welcome_version: 1` · additive-only, like every machine envelope);
/// the human mirror renders through the ONE colour seam (`Theme` ·
/// semantic never decorative — the same law every other surface obeys).
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let candidate = std::env::current_dir().ok();
    if json {
        return VerbOutput::ok(front_door_json(candidate.as_deref()));
    }
    VerbOutput::ok(crate::display::vocab::sober(
        theme,
        &screen(candidate.as_deref(), theme),
    ))
}

/// The composed first-contact screen — the seat cascade (identity ·
/// the rungs · the ONE `Next:`), then the mirror body (this machine ·
/// this binary · the sample). Rendering the cascade alone made
/// `render_with_context` unreachable and silently dropped the sample
/// block (#1195); rendering the body's old `start here` menu after it
/// put the fork back (#1196). The head promises; the body informs.
fn screen(candidate: Option<&Path>, theme: Theme) -> String {
    let choice = crate::choice::collect();
    let mirror = Mirror::collect(candidate);
    let next = mirror.next(candidate);
    mirror.record_session();
    record_impression(&next);
    format!(
        "{}\n{}",
        choice.render_human_next(theme, &next),
        mirror.render_body(theme)
    )
}

/// The machine front door — the same screen, the same ONE next step.
fn front_door_json(candidate: Option<&Path>) -> String {
    let choice = crate::choice::collect();
    let mirror = Mirror::collect(candidate);
    let next = mirror.next(candidate);
    let mut v = mirror.render_json(&next);
    v["inference_choice"] = choice.welcome_json(&next);
    v.to_string()
}

/// Everything the mirror knows about this session, collected ONCE.
///
/// The envelope-first order (P0-14 binary-side): resolve the session
/// context from the candidate the surface named, then walk and audit
/// the RESOLVED folder. The process cwd is never consulted here:
/// `None` in, chat-only out — no walk, no gate, no workspace claim.
struct Mirror {
    probe: Probe,
    envelope: ContextEnvelope,
    counts: EngineCounts,
    glance: Glance,
    /// P0-3: the ONE file's verdict, audited before any run line may
    /// name it. Only ever `Some` behind a complete walk that saw
    /// exactly one workflow.
    gate: Option<RunGate>,
    ctx: ContextView,
}

impl Mirror {
    fn collect(candidate: Option<&Path>) -> Self {
        let facts = EnvFacts {
            evidence: candidate.map_or(EvidenceSource::None, |_| EvidenceSource::ExplicitCwd),
            ..EnvFacts::detect()
        };
        let envelope = context_envelope::resolve(candidate, &facts);
        let probe = probe::collect(false);
        let counts = EngineCounts {
            builtins: nika_builtin::tool_defs().len(),
            locals: probe.providers.iter().filter(|p| !p.requires_key).count(),
            clouds: probe.providers.iter().filter(|p| p.requires_key).count(),
            examples: nika_pack::example_slugs().len(),
            templates: nika_pack::template_names().len(),
        };
        if envelope.mode == ContextMode::ChatOnly {
            return Self {
                probe,
                envelope,
                counts,
                glance: CHAT_ONLY_GLANCE,
                gate: None,
                ctx: ContextView {
                    chat_only: true,
                    ..ContextView::legacy()
                },
            };
        }
        let root = envelope.project_root.clone();
        let (glance, sole) = glance(&root, 4000);
        let gate = sole.as_deref().map(|rel| run_gate(&root, rel));
        let ctx = ContextView {
            chat_only: false,
            root: root_label(&envelope),
            expanded_from: envelope
                .evidence
                .expanded_from
                .as_ref()
                .map(|from| under_home(&from.display().to_string())),
        };
        Self {
            probe,
            envelope,
            counts,
            glance,
            gate,
            ctx,
        }
    }

    /// The screen's ONE next step — the directory's own door, overruled
    /// by whatever verdict this mirror already paid for.
    fn next(&self, candidate: Option<&Path>) -> String {
        let door = crate::choice::front_door_next(candidate);
        next_command(self.envelope.mode, self.glance, self.gate.as_ref(), &door)
    }

    fn render_body(&self, theme: Theme) -> String {
        front_door::render_with_context(
            &machine_view(&self.probe, self.counts),
            self.glance,
            self.counts,
            &self.ctx,
            theme,
        )
    }

    fn render_json(&self, next: &str) -> serde_json::Value {
        let experience =
            experience_block(&self.probe, &self.envelope, self.glance, self.gate.as_ref());
        if self.envelope.mode == ContextMode::ChatOnly {
            return front_door::render_chat_only_json(
                &self.probe.version,
                machine_json(&self.probe),
                self.counts,
                experience,
                next,
            );
        }
        front_door::render_json(
            &self.probe.version,
            machine_json(&self.probe),
            self.glance,
            self.counts,
            experience,
            next,
        )
    }

    fn record_session(&self) {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::ContextResolved,
            crate::metrics::Facts {
                session: Some(match self.envelope.mode {
                    ContextMode::ChatOnly => crate::metrics::Session::ChatOnly,
                    ContextMode::Workspace => crate::metrics::Session::Workspace,
                }),
                flag: (self.envelope.mode == ContextMode::Workspace)
                    .then(|| self.envelope.evidence.expanded_from.is_some()),
                ..crate::metrics::Facts::none()
            },
        );
    }
}

/// The concierge's CTA classes (W8 metrics): found (`init` · `new`) ·
/// go see (`examples` · `--deep`) · keep going (`run` · `check`).
fn cta_class(cmd: &str) -> crate::metrics::Cta {
    if cmd.starts_with("nika init") || cmd.starts_with("nika compile") {
        crate::metrics::Cta::Create
    } else if cmd.starts_with("nika run") || cmd.starts_with("nika check") {
        crate::metrics::Cta::Continue
    } else {
        crate::metrics::Cta::Discover
    }
}

/// The `cta_impression` for the ONE move the mirror shows — the
/// content-free half of the click-through the W8 audit wants measured.
/// It used to fire three times because the screen offered three
/// commands; a metric that counts more CTAs than the screen carries is
/// a metric about a screen nobody sees.
fn record_impression(next: &str) {
    crate::metrics::record_if_enabled(
        crate::metrics::EventKind::CtaImpression,
        crate::metrics::Facts {
            cta: Some(cta_class(next)),
            ..crate::metrics::Facts::none()
        },
    );
    // The hand-off the audit measures: the concierge points at a run
    // (only ever on an audited-clean file — P0-3) → the run is back in
    // human hands.
    if next.starts_with("nika run") {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::HumanRunHandoff,
            crate::metrics::Facts {
                handoff: Some(crate::metrics::Handoff::WelcomeCta),
                ..crate::metrics::Facts::none()
            },
        );
    }
}

/// The workspace glance — a bounded, dot-dir-skipping walk (depth ≤ 4 ·
/// budget-capped, 4000 entries in production): a greeting must stay
/// instant on a monorepo and must never wander into
/// `node_modules`/`target`. Returns the sole file's root-relative path
/// alongside, exactly when `workflows == 1` (the run CTA's audit target).
/// The walk's truncation flag lands in `Glance::complete` (P0-4) — the
/// budget is a parameter so tests can kill it without staging 4000 files.
fn glance(dir: &Path, walk_budget: usize) -> (Glance, Option<std::path::PathBuf>) {
    let git = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .ancestors()
        .any(|a| a.join(".git").exists());
    let mut budget = walk_budget;
    let mut paths = Vec::new();
    let truncated = probe::collect_workflow_paths(dir, dir, 4, &mut budget, &mut paths);
    paths.sort(); // the walk orders stably; the full-path sort pins it
    let workflows = paths.len();
    let sole = (workflows == 1 && !truncated).then(|| paths.swap_remove(0));
    (
        Glance {
            git,
            workflows,
            agents_md: dir.join("AGENTS.md").exists(),
            complete: !truncated,
        },
        sole,
    )
}

/// Audit ONE file in-process — parse + the check ladder + the catalog
/// price lookup (the same per-file fold `welcome --deep` runs, here
/// bounded to the single workflow a 1-file workspace carries). Anything
/// unreadable or unparseable is RED — never silently runnable.
fn run_gate(root: &Path, rel: &Path) -> RunGate {
    let path = rel.display().to_string();
    let verdict = |proposable, priced| RunGate {
        path: path.clone(),
        proposable,
        priced,
    };
    let Ok(yaml) = std::fs::read_to_string(root.join(rel)) else {
        return verdict(false, false);
    };
    let Ok(wf) = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return verdict(false, false);
    };
    let report = nika_check::check(&wf);
    let priced = report.cost.tasks.iter().any(|t| {
        t.model
            .as_deref()
            .and_then(nika_catalog::find_pricing_for)
            .is_some()
    });
    verdict(report.is_clean(), priced)
}

/// Build passive display facts after all host selection and redaction.
fn machine_view(probe: &Probe, counts: EngineCounts) -> front_door::MachineView {
    let unwired: Vec<&str> = probe
        .clients
        .iter()
        .filter(|client| !client.current)
        .map(|client| client.id.as_str())
        .collect();
    // H7 (audit UX 2026-07-30): never recommend `wire all` — one named
    // host, one consented mutation at a time. The handle takes its own
    // line unconditionally: hanging it off the cell list is what made
    // the row 112 columns wide, and one handle standing behind three
    // gaps reads as one gap, so the count is spoken (gauntlet 08-01).
    let wire_hint = unwired.split_first().map(|(first, rest)| {
        if rest.is_empty() {
            format!("→ nika wire {first}")
        } else {
            format!(
                "{} unwired · one command each → nika wire {first}",
                unwired.len()
            )
        }
    });
    let state = probe::adoption_state(probe);
    front_door::MachineView {
        version: probe.version.clone(),
        clients: probe
            .clients
            .iter()
            .map(|c| (c.id.clone(), c.current))
            .collect(),
        wire_hint,
        local_providers: probe.providers.iter().filter(|p| !p.requires_key).count(),
        endpoints: probe
            .providers
            .iter()
            .filter(|p| {
                !p.requires_key
                    && matches!(
                        p.readiness.execution_locus,
                        ExecutionLocus::Lan | ExecutionLocus::Remote
                    )
            })
            .map(|p| {
                (
                    p.id.clone(),
                    crate::doctor::redact_userinfo(&p.endpoint),
                    p.readiness.execution_locus.label().to_owned(),
                )
            })
            .collect(),
        model_count: probe.models.count,
        model_size: nika_models::store::human_size(probe.models.bytes),
        state_metric: state.metric(probe),
        state_cta: state.cta(),
        drifted_kits: probe
            .kits
            .iter()
            .filter(|k| crate::probe::train_differs(&k.version, &probe.version))
            .map(|k| format!("{} {}", k.client, k.version))
            .collect(),
        wired_facet: nika_providers::wired_facet(counts.locals + counts.clouds, counts.locals),
    }
}

/// The machine JSON, including host configuration, has one host producer.
fn machine_json(probe: &Probe) -> serde_json::Value {
    let mut machine = probe::environment_json(probe);
    machine["config"] = serde_json::json!(probe.config_path);
    machine
}

/// The human mirror — sections: identity · this machine · this binary ·
/// (the language, first time only) · learn. This legacy entry is the
/// TEST seam: it carries NO envelope view (an empty root), so the
/// pre-envelope render tests stay byte-identical; [`Mirror::render_body`]
/// renders through [`front_door::render_with_context`] with the resolved envelope.
#[cfg(test)]
fn render_human(probe: &Probe, glance: Glance, counts: EngineCounts, theme: Theme) -> String {
    front_door::render_with_context(
        &machine_view(probe, counts),
        glance,
        counts,
        &ContextView::legacy(),
        theme,
    )
}

/// Every command the concierge can ever teach — the parse-ratchet
/// surface. The `nika-cli` unit replays each one against the live clap
/// tree, so a door rename (the 0.107 `examples` → `try` move that
/// welcome kept teaching) breaks a test before it can break a paste.
/// Placeholders (`<file>` · `<usd>`) are the operator's slots; the
/// ratchet fills them.
///
/// Two families, and both are DERIVED, never listed: the ONE next step
/// over every state that can produce a different one, and the dim
/// routes the mirror body hangs off its own facts.
#[must_use]
pub fn taught_start_commands() -> Vec<String> {
    let gates = [
        None,
        Some(RunGate {
            path: "<file>".to_owned(),
            proposable: false,
            priced: false,
        }),
        Some(RunGate {
            path: "<file>".to_owned(),
            proposable: true,
            priced: false,
        }),
        Some(RunGate {
            path: "<file>".to_owned(),
            proposable: true,
            priced: true,
        }),
    ];
    let mut commands: Vec<String> = Vec::new();
    let mut push = |command: String| {
        if !commands.contains(&command) {
            commands.push(command);
        }
    };
    for mode in [ContextMode::ChatOnly, ContextMode::Workspace] {
        for complete in [false, true] {
            let glance = Glance {
                complete,
                ..CHAT_ONLY_GLANCE
            };
            for gate in &gates {
                for door in crate::choice::DOOR_SHAPES {
                    push(next_command(mode, glance, gate.as_ref(), door));
                }
            }
        }
    }
    // The routes that hang off a named gap in the body — each one
    // repairs the fact it sits beside, and each one is a paste.
    for route in [
        "nika init",
        "nika wire cursor",
        "nika model list",
        "nika doctor",
        "nika doctor --ping",
        "nika try 01-hello",
    ] {
        push(route.to_owned());
    }
    commands
}

/// The experience block riding `welcome --json` (additive against
/// `welcome_version: 1`) — the FIRST consumer of the router: the same
/// facts the concierge already proves (envelope mode · glance · the
/// one-file gate · kit drift) fold into an
/// [`ExperienceStateV1`](crate::experience::ExperienceStateV1), and
/// `route()` answers with the one next action. MCP and the plugin read
/// THIS object next — one truth, many projections, never a second
/// ladder. Honest floors of this projection, stated not hidden:
/// `findings` is 0 (the gate carries a verdict, not a count),
/// paused/failed traces are the deep lens's knowledge (not folded
/// yet — only their COUNT rides `action.nothing_has_run`, #1585),
/// `journey` has no local persistence (always `orientation`), and
/// `intent` is unknowable at a greeting.
fn experience_block(
    probe: &Probe,
    envelope: &ContextEnvelope,
    glance: Glance,
    gate: Option<&RunGate>,
) -> serde_json::Value {
    use crate::experience::{
        ContextEvidenceV1, ContextModeV1, ExperienceStateV1, WorkflowStateV1, route,
    };
    let chat_only = envelope.mode == ContextMode::ChatOnly;
    // The multi-root door. Several roots are open and none was named:
    // the envelope has always known it, the router has always had the
    // `select_root` arm for it — but `context_mode` only ever spoke
    // two words, so the arm was unreachable and the predicate sat
    // behind a dead-code exemption. One wire closes both, and every
    // claim below narrows: an unchosen root is not a root, so nothing
    // is named, scanned or called writable until the person picks.
    let unselected = !chat_only && envelope.requires_explicit_root();
    let evidence = if chat_only {
        ContextEvidenceV1::None
    } else {
        match envelope.evidence.source {
            EvidenceSource::HostSelection => ContextEvidenceV1::HostSelection,
            EvidenceSource::ActiveFile => ContextEvidenceV1::ActiveFile,
            // The CLI's cwd IS the operator's explicit choice (they
            // stood there when they called).
            EvidenceSource::ExplicitCwd => ContextEvidenceV1::ExplicitUser,
            EvidenceSource::None => ContextEvidenceV1::None,
        }
    };
    let workflow = if chat_only || unselected {
        WorkflowStateV1::Unknown
    } else {
        // P0-4 applies to EVERY count, not just zero: a truncated walk
        // that saw one file knows nothing about the rest — and the one
        // it saw was never audited (`glance` only hands a sole path,
        // and therefore a gate, behind a COMPLETE walk). A `Clean` here
        // would have the JSON contract claim an audit the text render
        // refuses in the same breath (« 1+ found · scan partial ») —
        // the two-surfaces-one-evidence law, caught by the refuter pass
        // before it ever shipped.
        match (glance.workflows, gate) {
            (_, _) if !glance.complete => WorkflowStateV1::Unknown,
            (0, _) => WorkflowStateV1::Absent,
            (1, Some(g)) if !g.proposable => WorkflowStateV1::Findings,
            (1, Some(_)) => WorkflowStateV1::Clean,
            // A complete walk that saw one file always carries its
            // gate; without one the state is unknown, never clean.
            (1, None) => WorkflowStateV1::Unknown,
            (_, _) => WorkflowStateV1::Several,
        }
    };
    let root = (!chat_only && !unselected).then(|| envelope.project_root.display().to_string());
    // ONE writability truth: the envelope already probed it (mode bits
    // AND the mirror-safe rules) — recomputing it here would be a
    // second answer to the same question.
    let writable = !chat_only && !unselected && envelope.writable;
    let configured = probe.providers.iter().any(|p| !p.requires_key)
        || probe
            .providers
            .iter()
            .any(|p| p.requires_key && p.key_present);
    let state = ExperienceStateV1 {
        context_mode: if chat_only {
            ContextModeV1::ChatOnly
        } else if unselected {
            ContextModeV1::MultiRootUnselected
        } else {
            ContextModeV1::Workspace
        },
        evidence,
        root,
        writable,
        inventory_complete: !chat_only && !unselected && glance.complete,
        workflow,
        workflow_path: gate.map(|g| g.path.clone()),
        provider: if configured {
            crate::experience::ProviderStateV1::Configured
        } else {
            crate::experience::ProviderStateV1::Unconfigured
        },
        // ONE drift law with the rendered line (probe::train_differs):
        // a PATCH is not a train. String equality here would have made
        // the router cry `align_versions` the day a 0.107.1 binary met
        // a 0.107.0 kit — degrading every richer CTA on the very
        // release this wave is cutting, while the human line stayed
        // silent. Two definitions of drift in one file is one too many.
        versions_coherent: !probe
            .kits
            .iter()
            .any(|k| probe::train_differs(&k.version, &probe.version)),
        ..ExperienceStateV1::chat_only()
    };
    // #1585 — the journal's word, never the router's constant.
    let action = route(&state).with_recorded_runs(probe.recorded_runs);
    serde_json::json!({ "state": state, "action": action })
}

#[cfg(test)]
mod tests;
