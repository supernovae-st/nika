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
//! (the [`crate::resolve_skills`] pattern — this crate stays zero-I/O;
//! the CLI hands `fs::read_to_string`). What one invocation judges: the
//! ROOT's direct calls carry the full law set; the reachable closure is
//! walked for acyclicity; a child's OWN direct calls are that child's
//! check's to judge (each file answers for its own contract).

use std::collections::BTreeMap;

use nika_cap::{ExecPermit, Permits};
use nika_types::types::{Field, NikaType, Primitive, assignable};

use super::ByteSpan;
use crate::analyzer;
use crate::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use crate::source::Span;
use crate::types::{OutputDecl, VarDecl, VarType};

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
fn workflow_calls(wf: &RawWorkflow) -> Vec<(&str, &crate::source::Spanned<String>)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        if let RawAction::Invoke(a) = &task.value.action
            && let RawInvokeTarget::Workflow(w) = &a.target
        {
            out.push((id, w));
        }
        for mini in &task.value.on_finally {
            if let RawAction::Invoke(a) = &mini.value.action
                && let RawInvokeTarget::Workflow(w) = &a.target
            {
                out.push((id, w));
            }
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
        // Pinned grammar: owner/name@version (each part non-empty).
        let Some((path, version)) = rest.rsplit_once('@') else {
            return Some(
                "the registry ref carries no `@version` pin — an unpinned ref \
                 resolves to different bodies over time (spec 14 law 1); pin it: \
                 `registry:owner/name@1.2.0`"
                    .to_owned(),
            );
        };
        let parts: Vec<&str> = path.split('/').collect();
        if version.is_empty() || parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
            return Some(
                "malformed registry ref — the pinned form is \
                 `registry:owner/name@version` (spec 14 §the form)"
                    .to_owned(),
            );
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
    let parent_env = analyzer::named_types(wf);
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
    target: &'a crate::source::Spanned<String>,
    args: Option<&'a serde_json::Value>,
    returns: Option<&'a crate::source::Spanned<serde_json::Value>>,
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
            let parsed = crate::parse(
                &text,
                crate::source::FileId::new(0),
                crate::ParseMode::Strict,
            )
            .map_err(|err| format!("`{id}` is not a parseable workflow: {err}"))?;
            self.parsed.insert(id.to_owned(), parsed);
        }
        Ok(&self.parsed[id])
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
    // Laws 3/4 — effect containment (child ⊆ parent declared).
    if let Some(parent) = cx.parent_permits {
        let child_boundary = child.permits.as_ref().map_or_else(
            || super::permits_infer::infer(&child).permits,
            |s| s.value.clone(),
        );
        for detail in boundary_violations(&child_boundary, parent) {
            push("NIKA-COMP-002", detail);
        }
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
    returns: Option<&crate::source::Spanned<serde_json::Value>>,
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
        let t = *r#type; // `r#` idents cannot appear inline in format strings
        match arg_map.get(*name) {
            None if *required && default.is_none() => out.push(format!(
                "required child input `{name}` ({t}) is not supplied by `args:` \
                 (spec 14 law 2)"
            )),
            Some(v) if is_literal(v) && !literal_fits(v, t) => out.push(format!(
                "arg `{name}` does not fit the child's declared `{t}` input \
                 (spec 14 law 2 · args ⋢ inputs)"
            )),
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
/// outputs map), so the static judgment and the run agree.
fn child_outputs_type(child: &RawWorkflow) -> NikaType {
    let fields: BTreeMap<String, Field> = child
        .outputs
        .iter()
        .map(|(name, decl)| {
            let ty = match decl {
                OutputDecl::Typed {
                    r#type: Some(t), ..
                } => vartype_to_nikatype(*t),
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

/// The closed 6-value `vars:`/`outputs:` vocabulary, lowered to the one
/// type core (spec 09).
fn vartype_to_nikatype(t: VarType) -> NikaType {
    match t {
        VarType::String => NikaType::Prim(Primitive::String),
        VarType::Number => NikaType::Prim(Primitive::Number),
        VarType::Integer => NikaType::Prim(Primitive::Integer),
        VarType::Boolean => NikaType::Prim(Primitive::Bool),
        VarType::Array => NikaType::Array(Box::new(NikaType::Unknown)),
        VarType::Object => NikaType::Map(Box::new(NikaType::Unknown)),
    }
}

/// A JSON arg value with no `${{ }}` island anywhere — statically
/// judgeable. A templated value is the run's to render (gradual).
fn is_literal(v: &serde_json::Value) -> bool {
    !serde_json::to_string(v).unwrap_or_default().contains("${{")
}

/// Whether a LITERAL arg value fits a declared [`VarType`] — the static
/// twin of the runtime's input validation (same closed vocabulary).
fn literal_fits(v: &serde_json::Value, t: VarType) -> bool {
    match t {
        VarType::String => v.is_string(),
        VarType::Number => v.is_number(),
        VarType::Integer => v.is_i64() || v.is_u64(),
        VarType::Boolean => v.is_boolean(),
        VarType::Array => v.is_array(),
        VarType::Object => v.is_object(),
    }
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
        crate::parse(
            yaml,
            crate::source::FileId::new(0),
            crate::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    const CHILD_OK: &str = "\
nika: v1
workflow:
  id: child
inputs:
  url: { type: string, required: true }
tasks:
  fetch:
    exec: { command: [\"echo\", \"${{ inputs.url }}\"] }
outputs:
  report: { value: \"${{ tasks.fetch.output }}\", type: string }
";

    fn parent_yaml(target: &str, args: &str) -> String {
        format!(
            "nika: v1\nworkflow:\n  id: parent\ntasks:\n  audit:\n    invoke:\n      workflow: \"{target}\"\n      args: {args}\n"
        )
    }

    #[test]
    fn templated_target_is_comp_001_purely() {
        let wf = parse(&parent_yaml("./x-${{ vars.env }}.nika.yaml", "{}"));
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
nika: v1
workflow:
  id: parent
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
nika: v1
workflow:
  id: parent
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
nika: v1
workflow:
  id: parent
const:
  u: \"x\"
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
nika: v1
workflow:
  id: parent
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
nika: v1
workflow:
  id: parent
permits:
  exec: [\"echo\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"https://example.com\" }
";
        let child = "\
nika: v1
workflow:
  id: child
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
    fn undeclared_parent_boundary_is_no_wall() {
        // No parent permits ⇒ no containment wall (the parent's own
        // authority is unconstrained · law 3's ∩ has nothing to cut).
        let wf = parse(&parent_yaml("./child.nika.yaml", "{}"));
        let child = "\
nika: v1
workflow:
  id: child
tasks:
  go:
    exec: { command: [\"rm\", \"-rf\", \"x\"] }
";
        let f = scan_resolved(&wf, "parent.nika.yaml", &mut |_| Ok(child.to_owned()));
        assert!(
            !f.iter().any(|x| x.code == "NIKA-COMP-002"),
            "no declared parent boundary → no containment finding: {f:?}"
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
}
