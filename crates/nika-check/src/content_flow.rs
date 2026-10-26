// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The content-taint pass (NEP-0002 v2.0 · the INTEGRITY half of the flow
//! lattice — [`super::flow`] tracks secret CONFIDENTIALITY): does untrusted
//! content actually REACH a task's effect surface, and from which ingress?
//! Sources: invoked `nika:fetch` · `mcp:*` (fail-closed) · `agent:` with an
//! ingress whitelist · `exec:`. Propagation mirrors [`super::flow`] with three
//! inversions: `infer:`/`agent:` outputs ARE tainted when their prompt sees
//! it · under a declared `fs.read`, a task that reads a file a tainted
//! WRITER left earlier is tainted (the file channel argv and bindings cannot
//! see · [`super::file_channel`]): an `exec:` whatever the file, a builtin
//! read (nika-cap's read slots) when its path may be one the writer wrote ·
//! recover reads propagate. One sweep = the least fixpoint (the DAG is
//! acyclic).
//!
//! An `invoke: workflow:` call is judged through what the composed check
//! learned of its child closure ([`crate::ComposedCall`] · spec 14 ·
//! NIK-273): content born in the closure may reach the call's output, an
//! egress task in the closure, or a file; the call's arguments and the
//! files an earlier writer left may reach an egress task or a file there.
//! The pass follows one [`Entry`] at a time, so the same table judges a
//! child for each kind of content that can enter it. A call no composed
//! check listed is judged child-blind: its arguments flow to its output,
//! and nothing else.

use std::collections::BTreeMap;

use nika_cap::TaintWitness;

use super::file_channel::{self, Written};
use super::flow::{action_effect_fields, collect_json_strings, refs_in_str};
use crate::ComposedCall;
use nika_schema::expression::NamespaceRef;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::types::OnErrorAction;

/// The origin `inputs.*` reads carry in an [`Entry::Args`] pass.
pub(super) const ARGS_ORIGIN: &str = "<the call's arguments>";
/// The origin the pre-armed file channel carries in an [`Entry::Files`] pass.
pub(super) const FILES_ORIGIN: &str = "<a file written before the call>";

/// Which untrusted content one pass follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// The workflow's own born sources (the root's pass · a child's content).
    Born,
    /// Only what `inputs.*` reads carry: the arguments a caller passes.
    Args,
    /// Only what the file channel carries when a tainted writer ran before
    /// the first task: the files a caller's writers left, at any path.
    Files,
}

/// Where untrusted content is born, beyond the verb tables: the judgments
/// this pass cannot make from the workflow's own bytes.
pub(super) struct Sources<'a> {
    /// `mcp_trusted(server_id)` answers the catalog's `content_trust` mark
    /// (`true` = trusted content).
    pub(super) mcp_trusted: &'a dyn Fn(&str) -> bool,
    /// The `invoke: workflow:` calls a composed check resolved. A
    /// reader-less check lists none.
    pub(super) calls: &'a [ComposedCall],
    /// The content this pass follows.
    pub(super) entry: Entry,
    /// Whether a task reads the file channel. `None`: when the workflow
    /// declares `fs.read` (the root). A child is judged with `Some(true)`:
    /// the trifecta needs the root's `fs.read` (leg ①), and under it the
    /// child's subprocesses and builtin reads read what earlier writers left.
    pub(super) exec_reads_files: Option<bool>,
}

impl Sources<'_> {
    /// The call `task_id` stands for, when it is a listed workflow call.
    fn call(&self, task_id: &str, action: &RawAction) -> Option<&ComposedCall> {
        let RawAction::Invoke(inv) = action else {
            return None;
        };
        if !matches!(inv.target, RawInvokeTarget::Workflow(_)) {
            return None;
        }
        self.calls.iter().find(|c| c.task == task_id)
    }
}

/// What one pass found.
pub(super) struct ContentFlow {
    /// Per task: the origin reaching its EFFECT (the judge's witness). For a
    /// listed workflow call: the origin reaching an egress task of its
    /// closure (the call is a sink exactly when this is set).
    pub(super) witnesses: Vec<TaintWitness>,
    /// Per task: the origin its OUTPUT carries.
    pub(super) outputs: Vec<Option<String>>,
    /// The first task that wrote a file with untrusted content in it, as
    /// `(writer, origin)`: the file channel's arming.
    pub(super) writer: Option<(String, String)>,
}

/// Compute the per-task content-taint witness over `sources`.
pub(super) fn analyze_content_flow(
    wf: &RawWorkflow,
    waves: &[Vec<usize>],
    sources: &Sources<'_>,
) -> ContentFlow {
    let n = wf.tasks.len();
    let id_of: BTreeMap<&str, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();
    let fs_read_declared = sources.exec_reads_files.unwrap_or_else(|| {
        wf.permits
            .as_ref()
            .and_then(|p| p.value.fs.as_ref())
            .is_some_and(|fs| !fs.read.is_empty())
    });
    let follows_born = sources.entry == Entry::Born;
    let inputs_src = (sources.entry == Entry::Args).then(|| ARGS_ORIGIN.to_owned());

    // output[i] / effect[i] = the untrusted origin reaching each (None = clean).
    let mut output: Vec<Option<String>> = vec![None; n];
    let mut effect: Vec<Option<String>> = vec![None; n];
    // Every tainted write so far, in run order (the first arms an `exec:`).
    let mut channel: Vec<Written> = Vec::new();
    if sources.entry == Entry::Files {
        channel.push(Written::anywhere(FILES_ORIGIN.to_owned()));
    }
    let mut writer: Option<(String, String)> = None;

    for wave in waves {
        for &i in wave {
            let task = &wf.tasks[i].value;
            let refs = Refs {
                output: &output,
                id_of: &id_of,
                inputs: inputs_src.as_ref(),
            };

            let (with_src, item_src) = refs.bindings(task);

            let arrived: Option<String> = action_effect_fields(&task.action)
                .into_iter()
                .flat_map(refs_in_str)
                .find_map(|r| refs.source_of(&r, &with_src, item_src.as_ref()));
            let lens = Lens {
                follows_born,
                fs_read_declared,
                channel: &channel,
            };
            let reached = match sources.call(&task.id.value, &task.action) {
                Some(call) => lens.call(&task.id.value, call, arrived),
                None => lens.task(task, arrived, sources.mcp_trusted),
            };
            let (effect_src, mut out_src) = (reached.effect, reached.output);
            if out_src.is_none()
                && let Some(on_error) = &task.on_error
                && let OnErrorAction::Recover(value) = &on_error.value.action
            {
                out_src = collect_json_strings(&value.value)
                    .into_iter()
                    .flat_map(refs_in_str)
                    .find_map(|r| refs.source_of(&r, &with_src, item_src.as_ref()));
            }

            if let Some(written) = reached.written {
                if channel.is_empty() {
                    writer = Some((task.id.value.clone(), written.origin.clone()));
                }
                channel.push(written);
            }
            effect[i] = effect_src;
            output[i] = out_src;
        }
    }

    ContentFlow {
        witnesses: (0..n)
            .map(|i| TaintWitness::new(effect[i].is_some(), effect[i].clone()))
            .collect(),
        outputs: output,
        writer,
    }
}

/// What one task does with the untrusted content reaching it.
struct Reached {
    /// The origin reaching its effect (the judge's witness).
    effect: Option<String>,
    /// The origin its output carries (before a recover read).
    output: Option<String>,
    /// The origin it writes to a file and where (the channel's arming).
    written: Option<Written>,
}

/// The pass's view at one task: which content it follows, and the file
/// channel so far.
struct Lens<'a> {
    follows_born: bool,
    fs_read_declared: bool,
    channel: &'a [Written],
}

impl Lens<'_> {
    /// An inline task, through the ONE table ([`classify`]).
    fn task(
        &self,
        task: &nika_schema::raw::RawTask,
        arrived: Option<String>,
        mcp_trusted: &dyn Fn(&str) -> bool,
    ) -> Reached {
        let effect = arrived.or_else(|| self.file_read(&task.action));
        // Output taint: born-source wins; else effect flows out (inversion).
        let (born, writes) = classify(&task.action, mcp_trusted);
        let output = if born && self.follows_born {
            Some(task.id.value.clone())
        } else {
            effect.clone()
        };
        // The file channel arms on ARRIVED taint (`effect`), never on born
        // output — the full story and its mutation-proven pin:
        // `trifecta::tests::two_execs_with_no_flow_between_them_pass`.
        let written = effect.clone().filter(|_| writes).map(|origin| Written {
            origin,
            spots: file_channel::written_spots(&task.action),
        });
        Reached {
            effect,
            output,
            written,
        }
    }

    /// The origin a task reads through the file channel: an `exec:` reads
    /// what the first tainted writer left, a builtin read what a writer of
    /// a path it reads left.
    fn file_read(&self, action: &RawAction) -> Option<String> {
        if !self.fs_read_declared {
            return None;
        }
        if matches!(action, RawAction::Exec(_)) {
            return self.channel.first().map(|w| w.origin.clone());
        }
        file_channel::first_met(self.channel, &file_channel::read_spots(action)).cloned()
    }

    /// A listed workflow call, through what its closure does with each
    /// entry: the call's arguments (`arrived`), the channel an earlier
    /// writer armed (when a task of the closure reads files, whatever the
    /// path: the closure's reads are not compared across the call), and what
    /// the closure brings in itself (the call's own id is that origin).
    fn call(&self, task_id: &str, call: &ComposedCall, arrived: Option<String>) -> Reached {
        let file = self
            .channel
            .first()
            .filter(|_| self.fs_read_declared && !call.files.is_empty())
            .map(|w| w.origin.clone());
        let own = || Some(task_id.to_owned());
        let output = if self.follows_born && call.born.output.is_some() {
            own()
        } else {
            // Arguments pass through (the child-blind reading).
            arrived
                .clone()
                .or_else(|| file.clone().filter(|_| call.files.output.is_some()))
        };
        // Arrived content names its origin first (the caller's own task),
        // then what the closure brings in itself.
        let effect = arrived
            .clone()
            .filter(|_| call.args.egress.is_some())
            .or_else(|| file.filter(|_| call.files.egress.is_some()))
            .or_else(|| own().filter(|_| self.follows_born && call.born.egress.is_some()));
        let written = if self.follows_born && call.born.file.is_some() {
            own()
        } else {
            arrived.filter(|_| call.args.file.is_some())
        }
        .map(Written::anywhere);
        Reached {
            effect,
            output,
            written,
        }
    }
}

/// The origin a workflow `outputs:` entry carries after `flow`, as
/// `(output name, origin)`: the first entry, in declaration order, that
/// reads a tainted task (or, in an [`Entry::Args`] pass, an input).
pub(super) fn output_origin(
    wf: &RawWorkflow,
    flow: &ContentFlow,
    entry: Entry,
) -> Option<(String, String)> {
    let id_of: BTreeMap<&str, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();
    let inputs = (entry == Entry::Args).then(|| ARGS_ORIGIN.to_owned());
    let refs = Refs {
        output: &flow.outputs,
        id_of: &id_of,
        inputs: inputs.as_ref(),
    };
    wf.outputs.iter().find_map(|(name, decl)| {
        refs_in_str(decl.value().value.as_str())
            .into_iter()
            .find_map(|r| refs.source_of(&r, &BTreeMap::new(), None))
            .map(|origin| (name.value.clone(), origin))
    })
}

/// What a reference can read: task outputs so far, and the inputs' origin.
struct Refs<'a> {
    output: &'a [Option<String>],
    id_of: &'a BTreeMap<&'a str, usize>,
    inputs: Option<&'a String>,
}

impl Refs<'_> {
    /// The origin each `with:` binding carries, and the `for_each` item's.
    fn bindings<'t>(
        &self,
        task: &'t nika_schema::raw::RawTask,
    ) -> (BTreeMap<&'t str, String>, Option<String>) {
        let mut with_src: BTreeMap<&str, String> = BTreeMap::new();
        for (key, value) in &task.with {
            for r in collect_json_strings(&value.value)
                .into_iter()
                .flat_map(refs_in_str)
            {
                if let Some(src) = self.source_of(&r, &with_src, None) {
                    with_src.insert(key.value.as_str(), src);
                    break;
                }
            }
        }
        let item_src: Option<String> = task.for_each.as_ref().and_then(|f| match &f.value {
            nika_schema::raw::ForEachValue::Expression(src) => refs_in_str(src)
                .into_iter()
                .find_map(|r| self.source_of(&r, &with_src, None)),
            nika_schema::raw::ForEachValue::List(_) => None,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown for_each form: {other:?}"),
        });
        (with_src, item_src)
    }

    fn source_of(
        &self,
        r: &NamespaceRef,
        with_src: &BTreeMap<&str, String>,
        item_src: Option<&String>,
    ) -> Option<String> {
        match r {
            NamespaceRef::Tasks { id, .. } => self
                .id_of
                .get(id.as_str())
                .and_then(|&u| self.output.get(u).cloned().flatten()),
            NamespaceRef::With(key) => with_src.get(key.as_str()).cloned(),
            NamespaceRef::Item => item_src.cloned(),
            NamespaceRef::Inputs(_) => self.inputs.cloned(),
            _ => None,
        }
    }
}

/// The invoke-side classifications, ONE effect-table walk: `(born_ingress,
/// writes_fs)` — the module doc's source table, and the writer clause.
/// The ingress predicates are nika-cap's shared ones (check≡run — the
/// runtime's F-O1 label reads the same law).
///
/// `exec:` is born-ingress (F2 · 2026-07-30 · operator decision A, run 5):
/// a subprocess is a trust boundary — its stdout is content the workflow
/// did not author (a curled page · a git log with hostile subjects · a
/// compromised binary), exactly the class `nika:fetch` is judged for.
/// Until this row read `(false, true)` the SAME flow drew SEC-009 over
/// `nika:fetch` and a ✔ over `exec curl` — a perverse incentive to leave
/// the native path (measured live). Every exec workflow's posture moves
/// with this row: its outputs taint downstream effects like any other
/// ingress, and the trifecta's leg ② counts the exec permit as an
/// ingress channel (`nika_cap::trifecta` — the grant twin of this law).
///
/// An `invoke: workflow:` call reads `(false, false)` here: the table
/// judges one task's own bytes, and a call's are its child's. The composed
/// check folds the child closure through this same table
/// ([`crate::ComposedCall`] · NIK-273), and [`analyze_content_flow`] reads
/// what it found.
pub(super) fn classify(action: &RawAction, mcp_trusted: &dyn Fn(&str) -> bool) -> (bool, bool) {
    match action {
        RawAction::Exec(_) => (true, true),
        RawAction::Agent(a) => (
            a.tools
                .iter()
                .any(|t| nika_cap::tool_grant_admits_ingress(t.value.as_str())),
            false,
        ),
        RawAction::Invoke(inv) => {
            let Some(tool) = inv.tool() else {
                return (false, false); // a child call — folded by the composed check
            };
            let id = tool.value.as_str();
            if let Some(server) = id.strip_prefix("mcp:") {
                // Server content untrusted UNLESS the catalog marks it
                // (absent/unknown = untrusted) · server writes are opaque.
                let trusted = mcp_trusted(server.split('/').next().unwrap_or(server));
                return (nika_cap::invoke_tool_is_ingress(id, trusted), true);
            }
            let args = inv.args.as_ref().map(|a| &a.value);
            let writes = match nika_cap::builtin_effect(id, args) {
                Some(nika_cap::BuiltinEffect::Fs { writes, .. }) => writes,
                _ => false,
            };
            (nika_cap::invoke_tool_is_ingress(id, false), writes)
        }
        _ => (false, false),
    }
}
