// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The composed call: one `invoke: workflow:` call judged through the child
//! it resolves to (spec 14 §The `CallableContract`: a child's contract is
//! derived from its body, never annotated). Its facts cross the wall into
//! the caller's own lanes:
//!
//! - **ingress** (NEP-0002 leg ② · NIK-273): the child, or any workflow it
//!   calls, holds a task the content-flow table judges a born source inline
//!   (an `exec:` · an invoked `nika:fetch` · an `mcp:*` tool · an `agent:`
//!   whose whitelist admits ingress · [`super::content_flow::classify`]).
//! - **flows**: where untrusted content entering the closure goes, by the
//!   same content-flow pass the caller runs, once per entry
//!   ([`super::content_flow::Entry`]): content the closure brings in itself
//!   (`born`), the call's arguments (`args`), and the files a tainted writer
//!   left before the call (`files`, read by an `exec:` or a builtin read of
//!   the closure, whatever the path). Each
//!   may reach the call's OUTPUT (the child's `outputs:`), an EGRESS task
//!   (the effect table's egress-capable tasks, the caller's legs deciding
//!   the trifecta), or a FILE (a writer: the channel a later reader reads).
//!   The trifecta lane judges the call as the tasks it stands for: a source
//!   when born content reaches its output or a file, a sink when content
//!   reaches an egress task. A blocking gate inside the closure is never
//!   credited to the caller: the caller's own gate, upstream of the call,
//!   clears it, as it clears the same tasks written inline.
//! - **effect** (NEP-0020 · `NIKA-SEC-014`): a run of the closure reaches
//!   an egress-capable task of the effect table, whatever content reaches
//!   it, on a route that crosses no confirm gate of the closure (the
//!   nearest gate owns its closure). The consent lane judges the call as
//!   that task: a confirm gate in front of it must close every route to it
//!   on a refusal, as in front of the same task written inline. When every
//!   such route crosses a gate of the closure the check cannot decide from
//!   the caller (a `when:` on an input the caller may bind to its answer),
//!   the effect is `effect_conditional`: the advisory, never the refusal.
//! - **grants**: the entries of the caller's own `permits:` the call uses
//!   or may use. The child runs inside the caller's boundary (laws 3/4:
//!   `child ∩ parent` at check and at run), so these entries are
//!   load-bearing even when no task of the caller uses them:
//!   `NIKA-DRIFT-001` never calls them dead. An entry is held when it admits
//!   a need the closure's inference lists (the needs `NIKA-COMP-002`
//!   judges), or when it meets the declared block of a workflow whose reach
//!   in that category is opaque (an `exec:` child's file and network reach ·
//!   a dynamic program, host or path · an `agent:` whitelist's dispatch · a
//!   harness-servable `agent:`), bounded by every declared block above it.
//!
//! A workflow the walk cannot resolve (unreadable · unparseable · a graph
//! that does not analyze · a registry or templated target · a cycle · past
//! the file cap) may do anything: every fact is set (fail-closed) and the
//! call holds the whole block. The walk is the composition lane's own
//! ([`super::composition`]: one parse per file, its file cap). Only
//! [`crate::check_composed`] resolves calls; a plain [`crate::check`] reads
//! no child and lists none.

use std::collections::BTreeMap;

use nika_cap::{ExecPermit, FsPermits, NetPermits, Permits};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};

use super::composition::{GraphWalker, normalize_path, resolve_relative, static_target_defect};
use super::consent::{Sink, closure_effect};
use super::content_flow::{self, ARGS_ORIGIN, Entry, FILES_ORIGIN, Sources};

/// One `invoke: workflow:` call as a composed check resolved it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ComposedCall {
    /// The calling task's id.
    pub task: String,
    /// The `workflow:` target as written at the call site.
    pub target: String,
    /// The first task, in the child or a workflow below it, that brings
    /// untrusted content in, or the workflow the check could not resolve.
    /// `None` when nothing below can (NEP-0002 leg ②).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<String>,
    /// Where the untrusted content the closure brings in goes.
    #[serde(default, skip_serializing_if = "CallFlow::is_empty")]
    pub born: CallFlow,
    /// Where the call's arguments go. Its `output` is informative: the
    /// trifecta carries an argument's taint to the call's output whatever
    /// it says (the child-blind reading).
    #[serde(default, skip_serializing_if = "CallFlow::is_empty")]
    pub args: CallFlow,
    /// Where a file written with untrusted content before the call goes,
    /// once an `exec:` or a builtin read of the closure reads it. Its `file`
    /// is never set: the channel is already open.
    #[serde(default, skip_serializing_if = "CallFlow::is_empty")]
    pub files: CallFlow,
    /// The first task, in the child or a workflow below it, that a run of
    /// the call reaches with an effect (an egress-capable task of the effect
    /// table, whatever content reaches it) on a route that crosses no
    /// confirm gate of the closure, or the workflow the check could not
    /// resolve. `None` when nothing below can act, or only behind the
    /// closure's own confirm gate: a refused confirm in front of the call
    /// then triggers no effect (NEP-0020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    /// Every route to `effect` crosses a gate of the closure the check
    /// cannot decide from the caller (a `when:` on an input the caller may
    /// bind to its answer): whether a refusal fires it is unproven, so the
    /// consent lane advises instead of refusing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub effect_conditional: bool,
    /// The entries of the caller's own `permits:` this call uses or may use
    /// (spec 14 law 3): never dead while the call exists.
    pub grants: Permits,
}

impl ComposedCall {
    /// A call no check resolved: it may bring anything in and send anything
    /// out (fail-closed), and it holds no grant claim. `why` is the witness
    /// every fact carries.
    #[must_use]
    pub fn unresolved(task: String, target: String, why: &str) -> Self {
        Self {
            task,
            target,
            ingress: Some(why.to_owned()),
            born: CallFlow::everywhere(why),
            args: CallFlow::everywhere(why),
            files: CallFlow {
                file: None,
                ..CallFlow::everywhere(why)
            },
            effect: Some(why.to_owned()),
            effect_conditional: false,
            grants: Permits::new(),
        }
    }
}

/// Where one kind of untrusted content entering a call goes in its child
/// closure. Each value is the witness: the path down to the task it
/// reaches.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct CallFlow {
    /// It reaches the call's output (an entry of the child's `outputs:`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// It reaches an egress-capable task of the closure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<String>,
    /// It reaches a task that writes a file (the channel an `exec:` reads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl CallFlow {
    /// Whether the content reaches nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_none() && self.egress.is_none() && self.file.is_none()
    }

    fn everywhere(why: &str) -> Self {
        Self {
            output: Some(why.to_owned()),
            egress: Some(why.to_owned()),
            file: Some(why.to_owned()),
        }
    }
}

/// A witness below a call: the targets walked, then what was found.
#[derive(Debug, Clone)]
struct Witness {
    via: Vec<String>,
    what: String,
}

impl Witness {
    fn here(what: String) -> Self {
        Self {
            via: Vec::new(),
            what,
        }
    }

    /// The same witness, one call further down (through `target`).
    fn under(&self, target: &str) -> Self {
        Self {
            via: std::iter::once(target.to_owned())
                .chain(self.via.iter().cloned())
                .collect(),
            what: self.what.clone(),
        }
    }

    /// The witness a caller reads, starting at its own call's target.
    fn render(&self, target: &str) -> String {
        let path: Vec<String> = std::iter::once(target)
            .chain(self.via.iter().map(String::as_str))
            .map(|t| format!("`{t}`"))
            .collect();
        format!("{} · {}", path.join(" → "), self.what)
    }
}

/// [`CallFlow`] before rendering.
#[derive(Debug, Clone, Default)]
struct Flow {
    output: Option<Witness>,
    egress: Option<Witness>,
    file: Option<Witness>,
}

impl Flow {
    fn everywhere(why: &str) -> Self {
        let w = Some(Witness::here(why.to_owned()));
        Self {
            output: w.clone(),
            egress: w.clone(),
            file: w,
        }
    }

    fn render(&self, target: &str) -> CallFlow {
        CallFlow {
            output: self.output.as_ref().map(|w| w.render(target)),
            egress: self.egress.as_ref().map(|w| w.render(target)),
            file: self.file.as_ref().map(|w| w.render(target)),
        }
    }
}

/// Faces of a workflow's reach its inferred needs cannot list exactly.
#[derive(Debug, Clone, Copy, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent category flags, one per permits face; a bitmask would hide which face"
)]
struct Opaque {
    exec: bool,
    tools: bool,
    net: bool,
    fs: bool,
}

/// What one workflow, and everything it calls, brings to a caller.
#[derive(Debug, Clone)]
struct Reach {
    /// The first ingress in task order (`None`: nothing brings content in).
    ingress: Option<Witness>,
    born: Flow,
    args: Flow,
    files: Flow,
    /// The effect a run reaches from the entry ([`ComposedCall::effect`]).
    effect: Option<Witness>,
    /// Only behind a gate the check cannot decide from a caller.
    effect_conditional: bool,
    /// Every workflow below resolved.
    resolved: bool,
    /// The union of each workflow's inferred needs (`super::permits_infer`).
    needs: Permits,
    /// The boundary the opaque faces reach: each workflow's declared block in
    /// its opaque categories, met with every declared block above it.
    opaque: Permits,
}

impl Reach {
    /// A workflow the walk could not resolve: it may bring anything in, send
    /// anything out and use any grant (fail-closed).
    fn unresolved(why: &str) -> Self {
        Self {
            ingress: Some(Witness::here(why.to_owned())),
            born: Flow::everywhere(why),
            args: Flow::everywhere(why),
            files: Flow {
                file: None,
                ..Flow::everywhere(why)
            },
            effect: Some(Witness::here(why.to_owned())),
            effect_conditional: false,
            resolved: false,
            needs: Permits::new(),
            opaque: Permits::new(),
        }
    }

    /// The call a caller's content-flow pass reads (the flows' presence).
    fn as_call(&self, task: &str, target: &str) -> ComposedCall {
        ComposedCall {
            task: task.to_owned(),
            target: target.to_owned(),
            ingress: self.ingress.as_ref().map(|w| w.render(target)),
            born: self.born.render(target),
            args: self.args.render(target),
            files: self.files.render(target),
            effect: self.effect.as_ref().map(|w| w.render(target)),
            effect_conditional: self.effect_conditional,
            grants: Permits::new(),
        }
    }
}

/// Resolve every direct `invoke: workflow:` call of `wf` through `walker`.
pub(super) fn resolve(
    wf: &RawWorkflow,
    root: &str,
    walker: &mut GraphWalker<'_>,
) -> Vec<ComposedCall> {
    let caller = wf.permits.as_ref().map(|p| &p.value);
    let mut memo = BTreeMap::new();
    let mut out = Vec::new();
    for task in &wf.tasks {
        let Some(target) = workflow_target(&task.value.action) else {
            continue;
        };
        let mut stack = vec![normalize_path(root)];
        let reach = reach_of_target(root, target, walker, &mut memo, &mut stack);
        let mut call = reach.as_call(&task.value.id.value, target);
        call.grants = held_grants(caller, &reach);
        out.push(call);
    }
    out
}

/// The `workflow:` target of a child call, else `None`.
fn workflow_target(action: &RawAction) -> Option<&str> {
    let RawAction::Invoke(invoke) = action else {
        return None;
    };
    if let RawInvokeTarget::Workflow(target) = &invoke.target {
        Some(target.value.as_str())
    } else {
        None
    }
}

/// The reach of the workflow `target` names, written in the file `from`.
fn reach_of_target(
    from: &str,
    target: &str,
    walker: &mut GraphWalker<'_>,
    memo: &mut BTreeMap<String, Reach>,
    stack: &mut Vec<String>,
) -> Reach {
    if let Some(defect) = static_target_defect(target) {
        return Reach::unresolved(&format!(
            "cannot be resolved before the run ({defect}) · untrusted until it is (fail-closed)"
        ));
    }
    reach_of_file(&resolve_relative(from, target), walker, memo, stack)
}

/// The reach of one resolved workflow file, memoized per file id.
fn reach_of_file(
    id: &str,
    walker: &mut GraphWalker<'_>,
    memo: &mut BTreeMap<String, Reach>,
    stack: &mut Vec<String>,
) -> Reach {
    let id = normalize_path(id);
    if let Some(known) = memo.get(&id) {
        return known.clone();
    }
    if stack.contains(&id) {
        // Not memoized: whether a file closes a cycle depends on the path.
        return Reach::unresolved(
            "closes a cycle in the call graph (spec 14 law 7) · untrusted (fail-closed)",
        );
    }
    let child = match walker.load(&id) {
        Ok(child) => child.clone(),
        Err(detail) => {
            let reach = Reach::unresolved(&format!(
                "{detail} · untrusted until it resolves (fail-closed)"
            ));
            memo.insert(id, reach.clone());
            return reach;
        }
    };
    stack.push(id.clone());
    let declared = child.permits.as_ref().map(|p| &p.value);
    let mut reach = own_reach(&child, declared);
    let mut nested: BTreeMap<String, (String, Reach)> = BTreeMap::new();
    // What each task does for the consent law, and its witness.
    let mut acts: Vec<(Sink, Option<Witness>)> = Vec::with_capacity(child.tasks.len());
    for task in &child.tasks {
        let task_id = &task.value.id.value;
        if let Some(target) = workflow_target(&task.value.action) {
            let below = reach_of_target(&id, target, walker, memo, stack);
            if reach.ingress.is_none() {
                reach.ingress = below.ingress.as_ref().map(|w| w.under(target));
            }
            let sink = Sink::of_closure(below.effect.is_some(), below.effect_conditional);
            acts.push((sink, below.effect.as_ref().map(|w| w.under(target))));
            reach.resolved &= below.resolved;
            reach.needs = reach.needs.union(&below.needs);
            // A grandchild runs inside this child's boundary.
            let bound = below.opaque.intersect(declared.unwrap_or(&Permits::new()));
            reach.opaque = reach.opaque.union(&bound);
            nested.insert(task_id.clone(), (target.to_owned(), below));
        } else {
            if reach.ingress.is_none() {
                reach.ingress = born_ingress(task_id, &task.value.action).map(Witness::here);
            }
            acts.push(if crate::trifecta::egress_capable(&task.value.action) {
                let what = egress_what(task_id, &task.value.action);
                (Sink::Proven, Some(Witness::here(what)))
            } else {
                (Sink::Inert, None)
            });
        }
    }
    stack.pop();
    match crate::analyzer::analyze(&child) {
        Ok(analyzed) => {
            let sinks = acts.iter().map(|(sink, _)| *sink).collect();
            if let Some((n, proven)) = closure_effect(&child, &analyzed.edges, sinks) {
                reach.effect = acts.get_mut(n).and_then(|(_, witness)| witness.take());
                reach.effect_conditional = !proven;
            }
            let judged = Judged {
                wf: &child,
                waves: &analyzed.topo_waves,
                nested: &nested,
            };
            reach.born = judged.flow(Entry::Born);
            reach.args = judged.flow(Entry::Args);
            reach.files = judged.flow(Entry::Files);
        }
        Err(_) => {
            reach = Reach::unresolved(
                "its task graph does not analyze (its own `nika check` names why) · \
                 untrusted (fail-closed)",
            );
        }
    }
    memo.insert(id, reach.clone());
    reach
}

/// One analyzed child and the reach of each call it makes.
struct Judged<'a> {
    wf: &'a RawWorkflow,
    waves: &'a [Vec<usize>],
    nested: &'a BTreeMap<String, (String, Reach)>,
}

impl Judged<'_> {
    /// Where the content `entry` names goes in this child.
    fn flow(&self, entry: Entry) -> Flow {
        let calls: Vec<ComposedCall> = self
            .nested
            .iter()
            .map(|(task, (target, reach))| reach.as_call(task, target))
            .collect();
        // The catalog's MCP trust mark has no reader yet: every server is
        // untrusted, as in the trifecta lane itself.
        let mcp_trusted = |_: &str| false;
        let sources = Sources {
            mcp_trusted: &mcp_trusted,
            calls: &calls,
            entry,
            exec_reads_files: Some(true),
        };
        let flow = content_flow::analyze_content_flow(self.wf, self.waves, &sources);
        let output = content_flow::output_origin(self.wf, &flow, entry)
            .map(|(name, origin)| self.origin_witness(&name, &origin));
        let egress = self.wf.tasks.iter().enumerate().find_map(|(i, task)| {
            let tainted = flow.witnesses.get(i)?;
            if !tainted.tainted {
                return None;
            }
            let id = task.value.id.value.as_str();
            match self.nested.get(id) {
                // A listed call's witness is set exactly when it is a sink.
                Some((target, below)) => {
                    let born_here = tainted.source.as_deref() == Some(id);
                    [&below.born, &below.args, &below.files]
                        .into_iter()
                        .skip(usize::from(!born_here))
                        .find_map(|f| f.egress.as_ref())
                        .map(|w| w.under(target))
                }
                None => crate::trifecta::egress_capable(&task.value.action)
                    .then(|| Witness::here(egress_what(id, &task.value.action))),
            }
        });
        let file = flow
            .writer
            .as_ref()
            .map(|(writer, _)| self.writer_witness(writer));
        Flow {
            output,
            egress,
            file,
        }
    }

    /// The witness of the origin an output entry carries.
    fn origin_witness(&self, name: &str, origin: &str) -> Witness {
        if origin == ARGS_ORIGIN {
            return Witness::here(format!("output `{name}` carries the call's arguments"));
        }
        if origin == FILES_ORIGIN {
            return Witness::here(format!(
                "output `{name}` carries a file read after an earlier write"
            ));
        }
        if let Some((target, below)) = self.nested.get(origin)
            && let Some(w) = &below.born.output
        {
            return w.under(target);
        }
        self.wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == origin)
            .and_then(|t| born_ingress(origin, &t.value.action))
            .map_or_else(
                || Witness::here(format!("output `{name}` carries untrusted content")),
                Witness::here,
            )
    }

    /// The witness of the task that armed the file channel.
    fn writer_witness(&self, writer: &str) -> Witness {
        if let Some((target, below)) = self.nested.get(writer)
            && let Some(w) = below.born.file.as_ref().or(below.args.file.as_ref())
        {
            return w.under(target);
        }
        let what = match self.wf.tasks.iter().find(|t| t.value.id.value == writer) {
            Some(t) => match &t.value.action {
                RawAction::Exec(_) => format!("task `{writer}` is an `exec:` that may write files"),
                RawAction::Invoke(inv) => match inv.tool() {
                    Some(tool) => format!("task `{writer}` writes a file through `{}`", tool.value),
                    None => format!("task `{writer}` writes a file"),
                },
                _ => format!("task `{writer}` writes a file"),
            },
            None => format!("task `{writer}` writes a file"),
        };
        Witness::here(what)
    }
}

/// One workflow's own needs and opaque reach, its calls excluded.
fn own_reach(wf: &RawWorkflow, declared: Option<&Permits>) -> Reach {
    let inferred = super::permits_infer::infer(wf);
    let mut opaque = Opaque {
        exec: inferred.partial.exec,
        tools: false,
        net: inferred.partial.net,
        fs: inferred.partial.fs,
    };
    for task in &wf.tasks {
        match &task.value.action {
            // A subprocess's file and network reach are opaque to statics
            // (its OS jail is built from the boundary it runs under).
            RawAction::Exec(_) => {
                opaque.net = true;
                opaque.fs = true;
            }
            // A whitelist dispatches dynamically (glob ⊆ glob is undecidable).
            RawAction::Agent(a) if !a.tools.is_empty() => {
                opaque.tools = true;
                opaque.net = true;
                opaque.fs = true;
            }
            // A harness can serve it, and judges its asks against exec + fs.
            RawAction::Agent(a) if a.schema.is_none() && a.schema_file.is_none() => {
                opaque.exec = true;
                opaque.fs = true;
            }
            _ => {}
        }
    }
    Reach {
        ingress: None,
        born: Flow::default(),
        args: Flow::default(),
        files: Flow::default(),
        effect: None,
        effect_conditional: false,
        resolved: true,
        needs: inferred.permits,
        opaque: declared.map_or_else(Permits::new, |d| faces_of(d, opaque)),
    }
}

/// The categories of `declared` an opaque face reaches; an absent block is
/// the declared zero (spec 14 law 4), so it reaches nothing.
fn faces_of(declared: &Permits, faces: Opaque) -> Permits {
    let mut bound = Permits::new();
    if faces.exec {
        bound.exec.clone_from(&declared.exec);
    }
    if faces.tools {
        bound.tools.clone_from(&declared.tools);
    }
    if faces.net {
        bound.net.clone_from(&declared.net);
    }
    if faces.fs {
        bound.fs.clone_from(&declared.fs);
    }
    bound
}

/// The witness of a task the content-flow table judges a born
/// untrusted-content source inline (the SAME predicate), else `None`.
fn born_ingress(task_id: &str, action: &RawAction) -> Option<String> {
    // The catalog's MCP trust mark has no reader yet: every server is
    // untrusted, as in the trifecta lane itself.
    if !content_flow::classify(action, &|_: &str| false).0 {
        return None;
    }
    Some(match action {
        RawAction::Exec(_) => {
            format!("task `{task_id}` is an `exec:` (its output is untrusted content)")
        }
        RawAction::Invoke(invoke) => match invoke.tool() {
            Some(tool) => format!("task `{task_id}` invokes `{}`", tool.value),
            None => format!("task `{task_id}` calls a workflow"),
        },
        RawAction::Agent(_) => {
            format!("task `{task_id}` is an `agent:` whose `tools:` admit a fetch or MCP tool")
        }
        _ => format!("task `{task_id}` brings untrusted content in"),
    })
}

/// What an egress-capable task is, for a witness.
fn egress_what(task_id: &str, action: &RawAction) -> String {
    match action {
        RawAction::Exec(_) => {
            format!("task `{task_id}` is an `exec:` (a program reaches anywhere)")
        }
        RawAction::Invoke(invoke) => match invoke.tool() {
            Some(tool) => format!("task `{task_id}` invokes `{}`", tool.value),
            None => format!("task `{task_id}` calls a workflow"),
        },
        RawAction::Agent(_) => {
            format!("task `{task_id}` is an `agent:` whose `tools:` admit an egress tool")
        }
        _ => format!("task `{task_id}` sends it out"),
    }
}

/// The caller's own entries a closure with `reach` uses or may use.
fn held_grants(caller: Option<&Permits>, reach: &Reach) -> Permits {
    let Some(caller) = caller else {
        return Permits::new();
    };
    if !reach.resolved {
        return caller.clone();
    }
    let (needs, bound) = (&reach.needs, &reach.opaque);
    let mut held = Permits::new();
    held.exec = match &caller.exec {
        Some(ExecPermit::Any) if needs.allows_exec() || bound.allows_exec() => {
            Some(ExecPermit::Any)
        }
        Some(ExecPermit::Programs(list)) => {
            let kept: Vec<String> = list
                .iter()
                .filter(|p| needs.allows_program(p) || bound.allows_program(p))
                .cloned()
                .collect();
            (!kept.is_empty()).then_some(ExecPermit::Programs(kept))
        }
        _ => None,
    };
    held.tools = caller.tools.as_ref().and_then(|globs| {
        let kept: Vec<String> = globs
            .iter()
            .filter(|g| {
                needs
                    .tools
                    .iter()
                    .flatten()
                    .any(|t| nika_cap::glob_matches(g, t))
                    || meets(&single_tool(g), bound, |p| p.tools.as_ref().map(Vec::len))
            })
            .cloned()
            .collect();
        (!kept.is_empty()).then_some(kept)
    });
    held.net = caller.net.as_ref().and_then(|net| {
        let kept: Vec<String> = net
            .http
            .iter()
            .filter(|entry| {
                needs_host(needs, entry)
                    || meets(&single_host(entry), bound, |p| {
                        p.net.as_ref().map(|n| n.http.len())
                    })
            })
            .cloned()
            .collect();
        (!kept.is_empty()).then(|| NetPermits::new(kept))
    });
    held.fs = caller.fs.as_ref().and_then(|fs| {
        let read = held_paths(&fs.read, bound, needs, false);
        let write = held_paths(&fs.write, bound, needs, true);
        (!read.is_empty() || !write.is_empty()).then(|| FsPermits::new(read, write))
    });
    held
}

/// Whether the single-entry boundary `one` meets `bound` in the category
/// `size` measures: the child's run boundary (`child ∩ parent`, the same
/// meet) would keep something of it.
fn meets(one: &Permits, bound: &Permits, size: impl Fn(&Permits) -> Option<usize>) -> bool {
    size(&one.intersect(bound)).is_some_and(|n| n > 0)
}

fn single_tool(glob: &str) -> Permits {
    let mut one = Permits::new();
    one.tools = Some(vec![glob.to_owned()]);
    one
}

fn single_host(entry: &str) -> Permits {
    let mut one = Permits::new();
    one.net = Some(NetPermits::new(vec![entry.to_owned()]));
    one
}

fn single_path(entry: &str, write: bool) -> Permits {
    let mut one = Permits::new();
    one.fs = Some(if write {
        FsPermits::new(Vec::new(), vec![entry.to_owned()])
    } else {
        FsPermits::new(vec![entry.to_owned()], Vec::new())
    });
    one
}

/// Does the single host entry `entry` admit a host the closure needs?
fn needs_host(needs: &Permits, entry: &str) -> bool {
    needs
        .net
        .iter()
        .flat_map(|n| &n.http)
        .any(|host| single_host(entry).allows_host(host))
}

/// The path entries (one direction) that admit a path the closure needs or
/// meet the opaque bound.
fn held_paths(entries: &[String], bound: &Permits, needs: &Permits, write: bool) -> Vec<String> {
    let needed: &[String] = match (&needs.fs, write) {
        (Some(fs), false) => &fs.read,
        (Some(fs), true) => &fs.write,
        (None, _) => &[],
    };
    let side = |p: &Permits| {
        p.fs.as_ref()
            .map(|fs| if write { fs.write.len() } else { fs.read.len() })
    };
    entries
        .iter()
        .filter(|entry| {
            let one = single_path(entry, write);
            needed.iter().any(|path| one.allows_path(path, write)) || meets(&one, bound, side)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
