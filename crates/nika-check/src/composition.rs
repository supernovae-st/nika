// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The composition lane (spec `14-composition.md`) — the static half of
//! the ten laws, as four findings:
//!
//! - `NIKA-COMP-001` · a `workflow:` target that is not statically
//!   resolvable (templated · malformed · unpinned registry ref ·
//!   unreadable · unparseable) — law 1.
//! - `NIKA-COMP-002` · the child's effect boundary exceeds the parent's
//!   declared boundary — laws 3/4 (the runtime twin is `NIKA-SEC-004`).
//! - `NIKA-COMP-003` · the static call graph is not acyclic (literal
//!   self-launch · A→B→A) — law 7 (`NIKA-SEC-003` is the run backstop).
//! - `NIKA-COMP-004` · the typed call does not compose (args ⋢ the
//!   child's `vars:` inputs, or the child's `outputs:` ⋢ the parent's
//!   `returns:`) — law 2, judged with the ONE type core
//!   ([`nika_types::types::assignable`]).
//!
//! Two halves, one voice: [`scan_static`] is PURE (runs in every
//! `check()` — a templated target needs no filesystem to refuse);
//! [`scan_resolved`] walks the call graph through an injected reader
//! (the [`nika_schema::resolve_skills`] pattern — this crate stays zero-I/O;
//! the CLI hands `fs::read_to_string`). What one invocation judges: the
//! ROOT's direct calls carry the full law set; the reachable closure is
//! walked for acyclicity; a child's OWN direct calls are that child's
//! check's to judge (each file answers for its own contract).

use std::collections::{BTreeMap, BTreeSet};

use nika_cap::{ExecPermit, Permits};
use nika_types::types::{Field, NikaType, assignable, fits, parse_type};

use super::ByteSpan;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::source::Span;
use nika_schema::types::{OutputDecl, VarDecl, type_expr_display};

/// The static call-graph walk refuses to draw past this many distinct
/// files — acyclicity that cannot be verified is acyclicity refused
/// (fail-closed · the same posture as the runtime depth backstop).
const MAX_GRAPH_FILES: usize = 64;

/// One composition finding (the `NIKA-COMP` namespace · spec 14).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct CompositionFinding {
    /// The calling task id.
    pub task: String,
    /// The `workflow:` target as written.
    pub target: String,
    /// `NIKA-COMP-001` | `NIKA-COMP-002` | `NIKA-COMP-003` | `NIKA-COMP-004`.
    pub code: &'static str,
    /// The human detail (names the exact repair).
    pub detail: String,
    /// The target's source byte range (the LSP diagnostic anchor).
    pub span: ByteSpan,
}

impl CompositionFinding {
    /// The shared human row — every surface prints THIS (one voice).
    #[must_use]
    pub fn row(&self) -> String {
        format!(
            "[{code} · composition] task `{task}` → `{target}` · {detail} · fix: nika explain {code}",
            code = self.code,
            task = self.task,
            target = self.target,
            detail = self.detail,
        )
    }
}

/// Every `invoke: workflow:` call site — `(task id, target)` in
/// declaration order, main verbs AND `on_finally` mini-tasks (the mini
/// parser shares the verb grammar).
fn workflow_calls(wf: &RawWorkflow) -> Vec<(&str, &nika_schema::source::Spanned<String>)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        if let RawAction::Invoke(a) = &task.value.action
            && let RawInvokeTarget::Workflow(w) = &a.target
        {
            out.push((id, w));
        }
    }
    out
}

/// The PURE half — every `workflow:` target refusable without a reader
/// (law 1's textual part). Runs in every `check()`.
pub(super) fn scan_static(wf: &RawWorkflow) -> Vec<CompositionFinding> {
    let mut out = Vec::new();
    for (task, target) in workflow_calls(wf) {
        if let Some(detail) = static_target_defect(&target.value) {
            out.push(CompositionFinding {
                task: task.to_owned(),
                target: target.value.clone(),
                code: "NIKA-COMP-001",
                detail,
                span: byte_span(target.span),
            });
        }
    }
    out
}

/// Why a target string is not statically resolvable — `None` = clean.
fn static_target_defect(target: &str) -> Option<String> {
    if target.contains("${{") {
        return Some(
            "the target is `${{ }}`-templated — a call graph you cannot draw \
             before the run is a call graph you cannot bound (spec 14 law 1); \
             write a literal path or a pinned registry ref"
                .to_owned(),
        );
    }
    if let Some(rest) = target.strip_prefix("registry:") {
        // ⭐ ONE grammar, two readers. This used to be a second parser —
        // `rsplit_once('@')` with no charset rule and no SemVer rule —
        // and it disagreed with the resolver IN BOTH DIRECTIONS:
        // `@nightly` passed HERE and was refused at resolution (a check
        // that says « clean » about a ref the resolver rejects lies at
        // the only moment the author is still reading), while an
        // unpinned ref was refused here and accepted there.
        //
        // The grammar now lives once, at L0 (`nika_vocab::registry_ref`
        // — the check is L0 and the client is L2, so the shared home
        // cannot be the client). What stays HERE is the rule that is
        // genuinely the check's and not the grammar's: a ref must be
        // PINNED. The resolver legitimately reads unpinned refs through
        // its own pin ladder; a workflow may not, because a call graph
        // you cannot bound before the run is one you cannot bound at all
        // (spec 14 law 1).
        match nika_vocab::registry_ref::parse(rest) {
            Err(defect) => {
                return Some(format!(
                    "malformed registry ref — {} (spec 14 §the form)",
                    defect.teaching()
                ));
            }
            Ok(parsed) if parsed.version.is_none() => {
                return Some(
                    "the registry ref carries no `@version` pin — an unpinned ref \
                     resolves to different bodies over time (spec 14 law 1); pin it: \
                     `registry:owner/name@1.2.0`"
                        .to_owned(),
                );
            }
            Ok(_) => {}
        }
    }
    None
}

/// The RESOLVED half — walks the call graph through the injected reader.
/// `root` is the parent workflow's own id (the path the caller loaded it
/// from); relative targets resolve against each file's own directory.
/// Returns the FULL lane (static defects included — one superset, so the
/// caller replaces, never merges).
pub(super) fn scan_resolved(
    wf: &RawWorkflow,
    root: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Vec<CompositionFinding> {
    let mut out = scan_static(wf);
    let statically_bad: Vec<String> = out.iter().map(|f| f.target.clone()).collect();
    let parent_env = BTreeMap::new();
    let parent_permits = wf.permits.as_ref().map(|s| &s.value);
    let mut walker = GraphWalker {
        read,
        parsed: BTreeMap::new(),
        file_cap_hit: false,
    };
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        let RawAction::Invoke(a) = &task.value.action else {
            continue;
        };
        let RawInvokeTarget::Workflow(w) = &a.target else {
            continue;
        };
        if statically_bad.contains(&w.value) {
            continue; // COMP-001 already said it — one voice
        }
        judge_direct_call(
            &JudgeCtx {
                task_id: id,
                target: w,
                args: a.args.as_ref().map(|s| &s.value),
                returns: task.value.returns.as_ref(),
                parent_env: &parent_env,
                parent_permits,
                root,
            },
            &mut walker,
            &mut out,
        );
    }
    out
}

/// Everything one direct call is judged against (laws 1-resolve · 2 ·
/// 3/4 · 7) — bundled so the judge stays under the fn-length cap.
struct JudgeCtx<'a> {
    task_id: &'a str,
    target: &'a nika_schema::source::Spanned<String>,
    args: Option<&'a serde_json::Value>,
    returns: Option<&'a nika_schema::source::Spanned<serde_json::Value>>,
    parent_env: &'a BTreeMap<String, NikaType>,
    parent_permits: Option<&'a Permits>,
    root: &'a str,
}

/// Parse-once cache + reader for the call-graph walk.
struct GraphWalker<'a> {
    read: &'a mut dyn FnMut(&str) -> Result<String, String>,
    parsed: BTreeMap<String, RawWorkflow>,
    file_cap_hit: bool,
}

impl GraphWalker<'_> {
    /// Resolve + parse one file id (memoized). `Err(detail)` = COMP-001.
    fn load(&mut self, id: &str) -> Result<&RawWorkflow, String> {
        if !self.parsed.contains_key(id) {
            if self.parsed.len() >= MAX_GRAPH_FILES {
                self.file_cap_hit = true;
                return Err(format!(
                    "the static call graph exceeds {MAX_GRAPH_FILES} files — \
                     acyclicity that cannot be verified is refused (fail-closed)"
                ));
            }
            let text = (self.read)(id).map_err(|e| format!("cannot read `{id}`: {e}"))?;
            let parsed = nika_schema::parse(
                &text,
                nika_schema::source::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .map_err(|err| format!("`{id}` is not a parseable workflow: {err}"))?;
            self.parsed.insert(id.to_owned(), parsed);
        }
        Ok(&self.parsed[id])
    }
}

/// One priced call — the child's ceiling plus the calling task's own
/// multipliers (the [`super::cost::CostCeiling::fold_composed`] input).
pub(super) struct PricedCall {
    /// The calling task's id.
    pub task: String,
    /// The child target as written.
    pub target: String,
    /// The child's ceiling (its own composed half folded already).
    pub child: super::cost::CostCeiling,
    /// The `for_each` multiplier — `None` when the count is not statically
    /// known (the unbounded arm, same law as [`super::cost::ceiling`]).
    pub iterations: Option<u64>,
    /// The retry multiplier (`max_attempts` · first-try is 1).
    pub attempts: u64,
    /// A `when:` gate — the cheapest path never calls.
    pub gated: bool,
}

/// The composition COST walk (spec 14 · the 2026-07-29 finding): every
/// direct `workflow:` call contributes the CHILD's ceiling — recursively,
/// so a parent whose child alone explains `≤$X` stops printing `$0 model
/// spend`. Returns one [`PricedCall`] per resolvable call; an unresolvable
/// child contributes nothing (its `NIKA-COMP-001` finding already owns
/// the verdict — never a double report). Cycle-safe and file-capped like
/// the judgment walk: a cyclic branch contributes nothing (`NIKA-COMP-003`
/// owns the cycle), and past [`MAX_GRAPH_FILES`] the walk stops rather
/// than hang.
pub(super) fn price_resolved(
    wf: &RawWorkflow,
    root: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Vec<PricedCall> {
    let mut walker = PriceWalker {
        read,
        ceilings: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };
    let mut out = Vec::new();
    for task in &wf.tasks {
        let RawAction::Invoke(a) = &task.value.action else {
            continue;
        };
        let RawInvokeTarget::Workflow(w) = &a.target else {
            continue;
        };
        if w.value.starts_with("registry:") {
            continue; // no filesystem child today (law 1's own arm)
        }
        // The calling task's OWN multipliers (the per-task law of
        // `cost::ceiling`, applied across the wall): fan-out count · retry
        // attempts · gate.
        let iterations = match task.value.for_each.as_ref().map(|f| &f.value) {
            None => Some(1),
            Some(nika_schema::raw::ForEachValue::List(arr)) => {
                Some(arr.as_array().map_or(1, Vec::len) as u64)
            }
            Some(nika_schema::raw::ForEachValue::Expression(expr)) => {
                super::cost::static_vars_array_len(wf, expr)
            }
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown for_each form: {other:?}"),
        };
        let attempts = task
            .value
            .retry
            .as_ref()
            .map_or(1, |r| u64::from(r.value.max_attempts.max(1)));
        let id = resolve_relative(root, &w.value);
        if let Some(child) = walker.ceiling_of(&id) {
            out.push(PricedCall {
                task: task.value.id.value.clone(),
                target: w.value.clone(),
                child,
                iterations,
                attempts,
                gated: task.value.when.is_some(),
            });
        }
    }
    out
}

/// The memoized, cycle-guarded ceiling walker (parse-once per file id).
struct PriceWalker<'a> {
    read: &'a mut dyn FnMut(&str) -> Result<String, String>,
    /// `None` = the file did not load/parse (COMP-001 owns that verdict).
    ceilings: BTreeMap<String, Option<super::cost::CostCeiling>>,
    /// The in-progress stack — a re-entered id is a cycle: the branch
    /// contributes nothing and COMP-003 names it, the walk never hangs.
    visiting: BTreeSet<String>,
}

impl PriceWalker<'_> {
    fn ceiling_of(&mut self, id: &str) -> Option<super::cost::CostCeiling> {
        if let Some(c) = self.ceilings.get(id) {
            return c.clone();
        }
        if self.ceilings.len() >= MAX_GRAPH_FILES || !self.visiting.insert(id.to_owned()) {
            return None; // over the verified-graph cap (fail-closed) · a cycle
        }
        let parsed = (self.read)(id).ok().and_then(|text| {
            nika_schema::parse(
                &text,
                nika_schema::source::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .ok()
        });
        let ceiling = parsed.map(|wf| {
            let mut ceiling = super::cost::ceiling(&wf);
            // The child's OWN composed half folds in (the recursion is the
            // topological pass — the call graph is static and acyclic when
            // clean, so this terminates and a child is priced once).
            for task in &wf.tasks {
                let RawAction::Invoke(a) = &task.value.action else {
                    continue;
                };
                let RawInvokeTarget::Workflow(w) = &a.target else {
                    continue;
                };
                if w.value.starts_with("registry:") {
                    continue;
                }
                let child_id = resolve_relative(id, &w.value);
                if let Some(child) = self.ceiling_of(&child_id) {
                    ceiling.min_path_total_usd += child.min_path_total_usd;
                    ceiling.bounded_total_usd += child.bounded_total_usd;
                    ceiling.has_unbounded |= child.has_unbounded;
                }
            }
            ceiling
        });
        self.visiting.remove(id);
        self.ceilings.insert(id.to_owned(), ceiling.clone());
        ceiling
    }
}

/// Judge ONE direct call: resolve the child (COMP-001), then laws 2
/// (COMP-004), 3/4 (COMP-002) and 7 (COMP-003 over the closure).
fn judge_direct_call(
    cx: &JudgeCtx<'_>,
    walker: &mut GraphWalker<'_>,
    out: &mut Vec<CompositionFinding>,
) {
    let mut push = |code: &'static str, detail: String| {
        out.push(CompositionFinding {
            task: cx.task_id.to_owned(),
            target: cx.target.value.clone(),
            code,
            detail,
            span: byte_span(cx.target.span),
        });
    };
    if cx.target.value.starts_with("registry:") {
        // A pinned registry ref is statically WELL-FORMED (law 1's
        // grammar) but resolves through the registry lane, not the
        // filesystem — its contract checks land with that lane.
        return;
    }
    let child_id = resolve_relative(cx.root, &cx.target.value);
    // Law 7 — the closure walk (self-launch is the 1-cycle).
    let mut stack = vec![normalize_path(cx.root)];
    if let Some(cycle) = find_cycle(&child_id, &mut stack, walker) {
        push(
            "NIKA-COMP-003",
            format!("the static call graph is not acyclic: {cycle} (spec 14 law 7)"),
        );
        return; // a cyclic child has no well-founded contract to judge
    }
    let child = match walker.load(&child_id) {
        Ok(c) => c.clone(),
        Err(detail) => {
            push("NIKA-COMP-001", detail);
            return;
        }
    };
    // Law 2 — the typed call (args ⋢ inputs · outputs ⋢ returns).
    for detail in typed_call_defects(cx.args, cx.returns, &child, cx.parent_env) {
        push("NIKA-COMP-004", detail);
    }
    // Laws 3/4 — effect containment (child ⊆ parent). F-O8 « absent =
    // zero authority »: an absent parent block IS the empty boundary.
    // NEP-0003 retires the draft-inferred reading (ex LAW-AUTH-0306): an
    // absent CHILD block is the DECLARED zero — the parent's grants never
    // flow down implicitly. The judged formula (the Python oracle's twin):
    // child NEEDS − (parent ∩ child-declared) — the inference only ever
    // computes NEEDS, never the judged boundary itself.
    let zero = Permits::new();
    let parent = cx.parent_permits.unwrap_or(&zero);
    let child_declared = child.permits.as_ref().map_or(&zero, |s| &s.value);
    let meet = parent.intersect(child_declared);
    let child_needs = super::permits_infer::infer(&child).permits;
    for detail in boundary_violations(&child_needs, &meet) {
        push("NIKA-COMP-002", detail);
    }
}

/// DFS the child's reachable `workflow:` closure looking for a cycle.
/// Returns the human cycle path when found. Registry refs and templated
/// targets are not walked (the former pin immutable bodies; the latter
/// are already COMP-001).
fn find_cycle(
    child_id: &str,
    stack: &mut Vec<String>,
    walker: &mut GraphWalker<'_>,
) -> Option<String> {
    let id = normalize_path(child_id);
    if stack.contains(&id) {
        let mut path = stack.clone();
        path.push(id);
        return Some(path.join(" → "));
    }
    let Ok(child) = walker.load(&id) else {
        return None; // unreadable — COMP-001's voice, not a cycle
    };
    let targets: Vec<String> = workflow_calls(child)
        .into_iter()
        .map(|(_, t)| t.value.clone())
        .filter(|t| !t.starts_with("registry:") && !t.contains("${{"))
        .collect();
    stack.push(id.clone());
    for t in targets {
        let next = resolve_relative(&id, &t);
        if let Some(cycle) = find_cycle(&next, stack, walker) {
            stack.pop();
            return Some(cycle);
        }
    }
    stack.pop();
    None
}

/// Law 2 defects — the parent's `args:` against the child's `vars:`
/// (the child's own `--var` refusal law, judged statically) and the
/// child's `outputs:` against the parent's `returns:` (one type core).
fn typed_call_defects(
    args: Option<&serde_json::Value>,
    returns: Option<&nika_schema::source::Spanned<serde_json::Value>>,
    child: &RawWorkflow,
    parent_env: &BTreeMap<String, NikaType>,
) -> Vec<String> {
    let mut out = Vec::new();
    let empty = serde_json::Map::new();
    let arg_map = args
        .and_then(serde_json::Value::as_object)
        .unwrap_or(&empty);
    let declared: BTreeMap<&str, &VarDecl> = child
        .inputs
        .iter()
        .map(|(n, d)| (n.value.as_str(), d))
        .collect();
    for key in arg_map.keys() {
        if !declared.contains_key(key.as_str()) {
            out.push(format!(
                "arg `{key}` is not a declared child input — the child refuses \
                 unknown vars (spec 14 law 2 · the `--var` law, statically)"
            ));
        }
    }
    // The declared TypeExpr renders for the findings; the fit itself is
    // judged by the one type core. There is no named env on either side
    // any more — a type expression is self-contained.
    let child_named = BTreeMap::new();
    let child_type_names = BTreeSet::new();
    for (name, decl) in &declared {
        let VarDecl::Typed {
            r#type,
            required,
            default,
            ..
        } = decl
        else {
            continue;
        };
        let display = type_expr_display(&r#type.value);
        match arg_map.get(*name) {
            None if *required && default.is_none() => out.push(format!(
                "required child input `{name}` ({display}) is not supplied by `args:` \
                 (spec 14 law 2)"
            )),
            Some(v) if is_literal(v) => {
                if let Ok(ty) = parse_type(&r#type.value, &child_type_names, name)
                    && !fits(v, &ty, &child_named)
                {
                    out.push(format!(
                        "arg `{name}` does not fit the child's declared `{display}` input \
                         (spec 14 law 2 · args ⋢ inputs)"
                    ));
                }
            }
            _ => {}
        }
    }
    if let Some(ret) = returns {
        let names: std::collections::BTreeSet<String> = parent_env.keys().cloned().collect();
        if let Ok(want) = nika_types::types::parse_type(&ret.value, &names, "returns")
            && !assignable(&child_outputs_type(child), &want, parent_env)
        {
            out.push(
                "the child's `outputs:` do not fit the parent's `returns:` \
                 (spec 14 law 2 · outputs ⋢ returns · one type core)"
                    .to_owned(),
            );
        }
    }
    out
}

/// The child's composed output type — an OBJECT of its `outputs:`
/// entries (`{name: declared-or-Unknown}` · closed). This IS the value
/// shape the runtime hands the parent task (the child `RunOutcome`
/// outputs map), so the static judgment and the run agree. A declared
/// `type:` is parsed as a self-contained `TypeExpr` (R3b) — a broken
/// expression degrades to `Unknown` (gradual · its refusal is the
/// child's own check).
fn child_outputs_type(child: &RawWorkflow) -> NikaType {
    let child_type_names = BTreeSet::new();
    let fields: BTreeMap<String, Field> = child
        .outputs
        .iter()
        .map(|(name, decl)| {
            let ty = match decl {
                OutputDecl::Typed {
                    r#type: Some(t), ..
                } => parse_type(&t.value, &child_type_names, &name.value)
                    .unwrap_or(NikaType::Unknown),
                OutputDecl::Typed { r#type: None, .. } | OutputDecl::Untyped(_) => {
                    NikaType::Unknown
                }
            };
            (name.value.clone(), Field::new(ty, false))
        })
        .collect();
    NikaType::Object {
        fields,
        additional: false,
    }
}

/// A JSON arg value with no `${{ }}` island anywhere — statically
/// judgeable. A templated value is the run's to render (gradual).
fn is_literal(v: &serde_json::Value) -> bool {
    !serde_json::to_string(v).unwrap_or_default().contains("${{")
}

/// Laws 3/4 — every concrete child-boundary entry the parent's declared
/// boundary does not admit. The child side is CONCRETE (declared globs
/// or inferred literal effects); the parent side judges with the same
/// `allows_*` predicates the escape scan and the runtime use — one
/// containment vocabulary, check≡run.
fn boundary_violations(child: &Permits, parent: &Permits) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(fs) = &child.fs {
        for p in &fs.read {
            if !parent.allows_path(p, false) {
                out.push(format!(
                    "child fs read `{p}` is outside the parent boundary \
                     (spec 14 law 4 · child ⊆ parent)"
                ));
            }
        }
        for p in &fs.write {
            if !parent.allows_path(p, true) {
                out.push(format!(
                    "child fs write `{p}` is outside the parent boundary \
                     (spec 14 law 4 · child ⊆ parent)"
                ));
            }
        }
    }
    if let Some(net) = &child.net {
        for h in &net.http {
            if !parent.allows_host(h) {
                out.push(format!(
                    "child net host `{h}` is outside the parent boundary \
                     (spec 14 law 4 · child ⊆ parent)"
                ));
            }
        }
    }
    out.extend(exec_violations(child.exec.as_ref(), parent));
    if let Some(tools) = &child.tools {
        for tool in tools {
            if !parent.allows_tool(tool) {
                out.push(format!(
                    "child tool `{tool}` is outside the parent boundary \
                     (spec 14 law 3 · zero implicit authority)"
                ));
            }
        }
    }
    out
}

/// The `exec:` axis of the containment law (closed tri-state).
fn exec_violations(child_exec: Option<&ExecPermit>, parent: &Permits) -> Vec<String> {
    match child_exec {
        None | Some(ExecPermit::No) => Vec::new(),
        Some(ExecPermit::Any) => {
            if matches!(parent.exec, Some(ExecPermit::Any)) {
                Vec::new()
            } else {
                vec![
                    "child declares `exec: true` (any program) but the parent \
                     boundary does not (spec 14 law 3 · a child never gains a \
                     capability the parent lacks)"
                        .to_owned(),
                ]
            }
        }
        Some(ExecPermit::Programs(list)) => list
            .iter()
            .filter(|p| !parent.allows_program(p))
            .map(|p| {
                format!(
                    "child exec program `{p}` is outside the parent boundary \
                     (spec 14 law 3)"
                )
            })
            .collect(),
        // `#[non_exhaustive]` — an exec form this checker does not know
        // cannot be proven contained: refuse loudly (fail-closed).
        Some(other) => vec![format!(
            "child declares an exec form this checker cannot bound ({other:?}) \
             — containment unprovable, refused (spec 14 law 3 · fail-closed)"
        )],
    }
}

/// The report-shaped byte range of a source span.
fn byte_span(s: Span) -> ByteSpan {
    ByteSpan::new(s.start.0, s.end.0)
}

/// Lexically resolve `target` against the DIRECTORY of `base` (both as
/// the caller names them) — pure, no filesystem, `..`/`.` normalized.
fn resolve_relative(base: &str, target: &str) -> String {
    if target.starts_with('/') {
        return normalize_path(target);
    }
    let dir = match base.rfind('/') {
        Some(i) => &base[..i],
        None => "",
    };
    if dir.is_empty() {
        normalize_path(target)
    } else {
        normalize_path(&format!("{dir}/{target}"))
    }
}

/// Lexical `.`/`..` normalization (never touches the filesystem — the
/// SAME id two spellings of one path collapse to, so the cycle walk
/// cannot be dodged with `./a/../a/child.yaml`).
fn normalize_path(p: &str) -> String {
    let absolute = p.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|s| *s != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    const CHILD_OK: &str = "\
nika: child
inputs:
  url: { type: string, required: true }
permits:
  exec: [\"echo\"]
tasks:
  fetch:
    exec: { command: [\"echo\", \"${{ inputs.url }}\"] }
outputs:
  report: { value: \"${{ tasks.fetch.output }}\", type: string }
";

    fn parent_yaml(target: &str, args: &str) -> String {
        format!(
            "nika: parent\ntasks:\n  audit:\n    invoke:\n      workflow: \"{target}\"\n      args: {args}\n"
        )
    }

    #[test]
    fn templated_target_is_comp_001_purely() {
        let wf = parse(&parent_yaml("./x-${{ inputs.env }}.nika.yaml", "{}"));
        let f = scan_static(&wf);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "NIKA-COMP-001");
        assert!(f[0].detail.contains("templated"), "{}", f[0].detail);
    }

    #[test]
    fn unpinned_registry_ref_is_comp_001() {
        let wf = parse(&parent_yaml("registry:acme/audit", "{}"));
        let f = scan_static(&wf);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].detail.contains("@version"), "{}", f[0].detail);
        // …and the pinned form is clean.
        let wf = parse(&parent_yaml("registry:acme/audit@1.2.0", "{}"));
        assert!(scan_static(&wf).is_empty());
    }

    #[test]
    fn unreadable_child_is_comp_001_resolved() {
        let wf = parse(&parent_yaml("./ghost.nika.yaml", "{}"));
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| {
            Err("No such file or directory (os error 2)".to_owned())
        });
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "NIKA-COMP-001");
        assert!(f[0].detail.contains("cannot read"), "{}", f[0].detail);
    }

    #[test]
    fn typed_call_judges_args_and_returns() {
        // unknown arg + missing required + literal misfit + returns misfit
        let yaml = "\
nika: parent
permits:
  exec: [\"echo\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { bogus: 1 }
    returns: integer
";
        let wf = parse(yaml);
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |p| {
            assert_eq!(p, "child.nika.yaml", "resolved against the parent dir");
            Ok(CHILD_OK.to_owned())
        });
        let codes: Vec<&str> = f.iter().map(|x| x.code).collect();
        assert_eq!(
            codes,
            vec!["NIKA-COMP-004", "NIKA-COMP-004", "NIKA-COMP-004"],
            "{f:?}"
        );
        assert!(f[0].detail.contains("`bogus`"), "{}", f[0].detail);
        assert!(f[1].detail.contains("`url`"), "{}", f[1].detail);
        assert!(f[2].detail.contains("outputs"), "{}", f[2].detail);
    }

    #[test]
    fn a_fitting_typed_call_is_clean() {
        let yaml = "\
nika: parent
permits:
  exec: [\"echo\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"https://example.com\" }
    returns: { object: { report: string } }
";
        let wf = parse(yaml);
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(CHILD_OK.to_owned()));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn literal_arg_misfit_is_comp_004_and_templated_is_gradual() {
        let yaml = "\
nika: parent
const:
  u: \"x\"
permits:
  exec: [\"echo\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: 42 }
  audit2:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"${{ const.u }}\" }
";
        let wf = parse(yaml);
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(CHILD_OK.to_owned()));
        assert_eq!(f.len(), 1, "templated arg is the run's to render: {f:?}");
        assert_eq!(f[0].task, "audit");
        assert!(f[0].detail.contains("`url`"), "{}", f[0].detail);
    }

    #[test]
    fn self_launch_is_comp_003() {
        let yaml = parent_yaml("./parent.nika.yaml", "{}");
        let wf = parse(&yaml);
        let f = scan_resolved(&wf, "./parent.nika.yaml", &mut |_| Ok(yaml.clone()));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "NIKA-COMP-003");
        assert!(f[0].detail.contains("→"), "{}", f[0].detail);
    }

    #[test]
    fn two_file_cycle_is_comp_003() {
        let a = parent_yaml("./b.nika.yaml", "{}");
        let b = parent_yaml("./a.nika.yaml", "{}");
        let wf = parse(&a);
        let f = scan_resolved(&wf, "a.nika.yaml", &mut |p| match p {
            "b.nika.yaml" => Ok(b.clone()),
            "a.nika.yaml" => Ok(a.clone()),
            other => Err(format!("unexpected read `{other}`")),
        });
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "NIKA-COMP-003");
    }

    #[test]
    fn acyclic_chain_is_clean_and_reads_are_memoized() {
        let a = parent_yaml("./b.nika.yaml", "{}");
        let b = parent_yaml("./c.nika.yaml", "{}");
        let mut reads = 0usize;
        let wf = parse(&a);
        let f = scan_resolved(&wf, "a.nika.yaml", &mut |p| {
            reads += 1;
            match p {
                "b.nika.yaml" => Ok(b.clone()),
                "c.nika.yaml" => Ok(CHILD_OK.to_owned()),
                other => Err(format!("unexpected read `{other}`")),
            }
        });
        assert!(f.is_empty(), "{f:?}");
        assert_eq!(reads, 2, "each file read once (memoized)");
    }

    #[test]
    fn effect_containment_is_comp_002() {
        // Parent declares a narrow boundary; the child (no declared
        // permits) INFERS an exec effect — outside the parent's wall.
        let parent = "\
nika: parent
permits:
  net:
    http: [\"api.example.com\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"https://api.example.com/x\" }
";
        let wf = parse(parent);
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(CHILD_OK.to_owned()));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "NIKA-COMP-002");
        assert!(f[0].detail.contains("exec"), "{}", f[0].detail);
    }

    #[test]
    fn contained_child_is_clean_under_declared_parent() {
        let parent = "\
nika: parent
permits:
  exec: [\"echo\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"https://example.com\" }
";
        let child = "\
nika: child
inputs:
  url: { type: string, required: true }
permits:
  exec: [\"echo\"]
tasks:
  fetch:
    exec: { command: [\"echo\", \"${{ inputs.url }}\"] }
";
        let wf = parse(parent);
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(child.to_owned()));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn undeclared_parent_boundary_is_zero_authority() {
        // F-O8 « absent = zero authority »: no parent permits ⇒ the parent
        // boundary is ∅ (not « no wall »), so the child's concrete exec
        // need exceeds it — containment refuses (law 3's ∩ cuts to zero).
        let wf = parse(&parent_yaml("./child.nika.yaml", "{}"));
        let child = "\
nika: child
tasks:
  go:
    exec: { command: [\"rm\", \"-rf\", \"x\"] }
";
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(child.to_owned()));
        assert!(
            f.iter().any(|x| x.code == "NIKA-COMP-002"),
            "absent parent boundary = ∅ → the child's need escapes it: {f:?}"
        );
    }

    #[test]
    fn normalization_makes_dodged_spellings_one_id() {
        assert_eq!(normalize_path("./a/../a/x.yaml"), "a/x.yaml");
        assert_eq!(normalize_path("a/x.yaml"), "a/x.yaml");
        assert_eq!(resolve_relative("dir/p.yaml", "./c.yaml"), "dir/c.yaml");
        assert_eq!(resolve_relative("dir/p.yaml", "../c.yaml"), "c.yaml");
        assert_eq!(resolve_relative("p.yaml", "c.yaml"), "c.yaml");
        assert_eq!(resolve_relative("dir/p.yaml", "/abs/c.yaml"), "/abs/c.yaml");
    }

    /// The emitted⊆registered ratchet, check-only tier: every code this
    /// lane (and its runtime twin) stamps as a STRING must exist in the
    /// vendored canon registry — the same law `spec_code.rs` pins for
    /// `SchemaError` variants.
    #[test]
    fn every_composition_code_is_registered_in_the_canon() {
        let registered: std::collections::BTreeSet<String> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code.to_string())
            .collect();
        for code in [
            "NIKA-COMP-001",
            "NIKA-COMP-002",
            "NIKA-COMP-003",
            "NIKA-COMP-004",
            "NIKA-SEC-003", // the run-recursion backstop the dispatch stamps
        ] {
            assert!(
                registered.contains(code),
                "`{code}` is not in the canon registry (spec/05-errors.md SSOT)"
            );
        }
    }

    #[test]
    fn finding_row_is_one_voice() {
        let f = CompositionFinding {
            task: "audit".to_owned(),
            target: "./c.yaml".to_owned(),
            code: "NIKA-COMP-001",
            detail: "cannot read `./c.yaml`".to_owned(),
            span: ByteSpan::new(0, 0),
        };
        let row = f.row();
        assert!(row.starts_with("[NIKA-COMP-001 · composition] task `audit`"));
        assert!(row.ends_with("fix: nika explain NIKA-COMP-001"), "{row}");
    }

    // ─── the composition COST half (the 2026-07-29 finding) ──────────

    /// A child with a priced infer task (the fixture's floor comes from
    /// its OWN check — the assertion is catalog-move-proof).
    const CHILD_PRICED: &str = "\
nika: child
tasks:
  spend:
    infer: { prompt: hi, max_tokens: 1000000, model: \"anthropic/claude-sonnet-5\" }
outputs:
  said: { value: \"${{ tasks.spend.output }}\", type: string }
";

    fn parent_calling(target: &str) -> RawWorkflow {
        parse(&format!(
            "nika: parent\ntasks:\n  call:\n    invoke:\n      workflow: \"{target}\"\n"
        ))
    }

    #[test]
    fn a_priced_child_folds_into_the_parent_envelope() {
        let wf = parent_calling("./child.nika.yaml");
        let child_floor = crate::check(&parse(CHILD_PRICED)).cost.min_path_total_usd;
        assert!(child_floor > 0.0, "the fixture prices");
        let report =
            crate::check_composed(
                &wf,
                "parent.nika.yaml",
                &mut |_| Ok(CHILD_PRICED.to_owned()),
            );
        assert_eq!(report.cost.composed.len(), 1, "{:?}", report.cost.composed);
        assert_eq!(report.cost.composed[0].task, "call");
        assert_eq!(report.cost.composed[0].target, "./child.nika.yaml");
        assert!(
            (report.cost.min_path_total_usd - child_floor).abs() < 1e-12,
            "the parent's floor IS the child's: {} vs {child_floor}",
            report.cost.min_path_total_usd
        );
        assert!(
            !report.cost.has_unbounded,
            "a fully-bounded child bounds the parent too"
        );
        // …and the reader-less `check` stays child-blind BY DESIGN.
        let pure = crate::check(&wf);
        assert!(pure.cost.composed.is_empty());
        assert_eq!(pure.cost.min_path_total_usd, 0.0);
    }

    #[test]
    fn a_grandchild_folds_through_the_child() {
        let middle = "\
nika: middle
tasks:
  call:
    invoke:
      workflow: \"./leaf.nika.yaml\"
";
        let wf = parent_calling("./middle.nika.yaml");
        let leaf_floor = crate::check(&parse(CHILD_PRICED)).cost.min_path_total_usd;
        let report = crate::check_composed(&wf, "parent.nika.yaml", &mut |p| {
            Ok(match p {
                "middle.nika.yaml" => middle.to_owned(),
                "leaf.nika.yaml" => CHILD_PRICED.to_owned(),
                other => panic!("unexpected read: {other}"),
            })
        });
        assert!(
            (report.cost.min_path_total_usd - leaf_floor).abs() < 1e-12,
            "the grandchild's floor reaches the parent through the child: {}",
            report.cost.min_path_total_usd
        );
    }

    /// The amplification law (§7b's untested row, closed): a call task's
    /// OWN multipliers apply across the wall — `for_each` N calls are
    /// always made (floor ×N) and every retry attempt can re-run the whole
    /// child (ceiling ×N×attempts). A gated call's cheapest path is zero.
    #[test]
    fn the_calling_tasks_multipliers_scale_the_child() {
        let wf = parse(
            "nika: parent\ntasks:\n  call:\n    for_each: { items: [\"a\", \"b\"] }\n    retry: { max_attempts: 3 }\n    invoke:\n      workflow: \"./child.nika.yaml\"\n",
        );
        let child = crate::check(&parse(CHILD_PRICED)).cost;
        let report =
            crate::check_composed(
                &wf,
                "parent.nika.yaml",
                &mut |_| Ok(CHILD_PRICED.to_owned()),
            );
        assert!(
            (report.cost.min_path_total_usd - 2.0 * child.min_path_total_usd).abs() < 1e-12,
            "2 fanned-out calls, first-try each: {} vs {}",
            report.cost.min_path_total_usd,
            2.0 * child.min_path_total_usd
        );
        assert!(
            (report.cost.bounded_total_usd - 6.0 * child.bounded_total_usd).abs() < 1e-9,
            "2 calls × 3 attempts at worst: {} vs {}",
            report.cost.bounded_total_usd,
            6.0 * child.bounded_total_usd
        );
    }

    /// A gated call contributes zero to the cheapest path; a `for_each`
    /// over an unknown-count source makes the parent unbounded.
    #[test]
    fn a_gated_call_floors_at_zero_and_an_unknown_fanout_unbounds() {
        let gated = parse(
            "nika: parent\ntasks:\n  call:\n    when: ${{ inputs.go == \"yes\" }}\n    invoke:\n      workflow: \"./child.nika.yaml\"\n",
        );
        let child = crate::check(&parse(CHILD_PRICED)).cost;
        let report = crate::check_composed(&gated, "parent.nika.yaml", &mut |_| {
            Ok(CHILD_PRICED.to_owned())
        });
        assert_eq!(
            report.cost.min_path_total_usd, 0.0,
            "gates closed ⇒ the cheapest path never calls"
        );
        assert!(
            (report.cost.bounded_total_usd - child.bounded_total_usd).abs() < 1e-9,
            "gates open ⇒ one full child at worst"
        );

        let fanned = parse(
            "nika: parent\ntasks:\n  call:\n    for_each: { items: \"${{ tasks.seed.output }}\" }\n    invoke:\n      workflow: \"./child.nika.yaml\"\n  seed:\n    exec: { command: [\"echo\", \"[]\"] }\n",
        );
        let report = crate::check_composed(&fanned, "parent.nika.yaml", &mut |_| {
            Ok(CHILD_PRICED.to_owned())
        });
        assert!(
            report.cost.has_unbounded,
            "an unknown iteration count makes the call's spend unbounded"
        );
    }

    #[test]
    fn an_unbounded_child_propagates_the_warning() {
        let child = "\
nika: child
tasks:
  spend:
    infer: { prompt: hi, model: \"anthropic/claude-sonnet-5\" }
";
        let wf = parent_calling("./child.nika.yaml");
        let report = crate::check_composed(&wf, "parent.nika.yaml", &mut |_| Ok(child.to_owned()));
        assert!(
            report.cost.has_unbounded,
            "no max_tokens in the child ⇒ the parent's total is no ceiling"
        );
    }

    #[test]
    fn an_unreadable_child_contributes_nothing_and_comp_001_owns_it() {
        let wf = parent_calling("./ghost.nika.yaml");
        let report = crate::check_composed(&wf, "parent.nika.yaml", &mut |_| {
            Err("No such file or directory (os error 2)".to_owned())
        });
        assert!(report.cost.composed.is_empty(), "no double report");
        assert!(
            report.composition.iter().any(|f| f.code == "NIKA-COMP-001"),
            "the one voice: {:?}",
            report.composition
        );
    }

    #[test]
    fn a_cyclic_call_graph_neither_hangs_nor_contributes() {
        let a = "\
nika: a
tasks:
  call:
    invoke:
      workflow: \"./b.nika.yaml\"
";
        let b = "\
nika: b
tasks:
  call:
    invoke:
      workflow: \"./a.nika.yaml\"
";
        let report = crate::check_composed(&parse(a), "a.nika.yaml", &mut |p| {
            Ok(match p {
                "a.nika.yaml" => a.to_owned(),
                "b.nika.yaml" => b.to_owned(),
                other => panic!("unexpected read: {other}"),
            })
        });
        assert!(
            report.cost.composed.iter().all(|c| !c.has_unbounded),
            "no phantom unbounded spend through a cycle: {:?}",
            report.cost.composed
        );
        assert!(
            report.cost.min_path_total_usd == 0.0 && report.cost.bounded_total_usd == 0.0,
            "a cyclic branch adds zero spend (COMP-003 owns the verdict): {}",
            report.cost.min_path_total_usd
        );
        assert!(
            report.composition.iter().any(|f| f.code == "NIKA-COMP-003"),
            "the cycle is named: {:?}",
            report.composition
        );
    }
}
