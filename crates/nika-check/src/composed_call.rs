// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The composed call: one `invoke: workflow:` call judged through the child
//! it resolves to (spec 14 §The `CallableContract`: a child's contract is
//! derived from its body, never annotated). Two facts cross the wall into
//! the caller's own lanes:
//!
//! - **ingress** (NEP-0002 leg ② · NIK-273). The call's output is untrusted
//!   content when the child, or any workflow it calls, holds a task the
//!   content-flow pass judges a born source inline: an `exec:` · an invoked
//!   `nika:fetch` · an `mcp:*` tool · an `agent:` whose whitelist admits
//!   ingress (the ONE table, [`super::content_flow::classify`]). A workflow
//!   the walk cannot resolve (unreadable · unparseable · a registry or
//!   templated target · a cycle · past the file cap) may hold one: the call
//!   is untrusted then too (fail-closed). The trifecta lane reads this.
//! - **grants** — the entries of the caller's own `permits:` the call uses
//!   or may use. The child runs inside the caller's boundary (laws 3/4: its
//!   effects are judged against `parent ∩ child`, at check and at run), so
//!   these entries are load-bearing even when no task of the caller uses
//!   them: `NIKA-DRIFT-001` never calls them dead. An entry is held when it
//!   admits a need the child closure's inference lists (the needs
//!   `NIKA-COMP-002` judges), and every entry of a category the closure
//!   reaches opaquely is held (an `exec:` child's file and network reach ·
//!   a dynamic program, host or path · an `agent:` whitelist's dispatch · a
//!   harness-servable `agent:`), the same « no claim, never wrong » law the
//!   drift scan applies inside one file. Nothing resolved: every entry.
//!
//! The walk is the composition lane's own ([`super::composition`]: one parse
//! per file, its file cap). Only [`crate::check_composed`] resolves calls; a
//! plain [`crate::check`] reads no child and lists none.

use std::collections::BTreeMap;

use nika_cap::{ExecPermit, FsPermits, NetPermits, Permits};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};

use super::composition::{GraphWalker, normalize_path, resolve_relative, static_target_defect};

/// One `invoke: workflow:` call as a composed check resolved it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ComposedCall {
    /// The calling task's id.
    pub task: String,
    /// The `workflow:` target as written at the call site.
    pub target: String,
    /// Why the call's output is untrusted content: the first task, in the
    /// child or a workflow below it, that brings untrusted content in, or
    /// the workflow the check could not resolve. `None` when nothing below
    /// can (NEP-0002 leg ②).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<String>,
    /// The entries of the caller's own `permits:` this call uses or may use
    /// (spec 14 law 3): never dead while the call exists.
    pub grants: Permits,
}

/// Faces of a closure's reach its inferred needs cannot list exactly.
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
    /// The path below the workflow to the first ingress (targets as written).
    via: Vec<String>,
    /// What that first ingress is (`None`: nothing brings untrusted content in).
    ingress: Option<String>,
    /// Every workflow below resolved.
    resolved: bool,
    /// The union of each workflow's inferred needs (`super::permits_infer`).
    needs: Permits,
    opaque: Opaque,
}

impl Reach {
    fn empty() -> Self {
        Self {
            via: Vec::new(),
            ingress: None,
            resolved: true,
            needs: Permits::new(),
            opaque: Opaque::default(),
        }
    }

    /// A workflow the walk could not resolve: it may bring anything in and
    /// use any grant (fail-closed).
    fn unresolved(why: String) -> Self {
        Self {
            ingress: Some(why),
            resolved: false,
            ..Self::empty()
        }
    }

    /// Fold a called workflow's reach in. The first ingress in task order
    /// wins the witness, reached through `target`.
    fn absorb(&mut self, target: &str, below: Self) {
        if self.ingress.is_none() && below.ingress.is_some() {
            self.via = std::iter::once(target.to_owned())
                .chain(below.via)
                .collect();
            self.ingress = below.ingress;
        }
        self.resolved &= below.resolved;
        self.needs = self.needs.union(&below.needs);
        self.opaque.exec |= below.opaque.exec;
        self.opaque.tools |= below.opaque.tools;
        self.opaque.net |= below.opaque.net;
        self.opaque.fs |= below.opaque.fs;
    }

    /// The witness a caller reads, starting at its own call's target.
    fn witness(&self, target: &str) -> Option<String> {
        let what = self.ingress.as_deref()?;
        let path: Vec<String> = std::iter::once(target)
            .chain(self.via.iter().map(String::as_str))
            .map(|t| format!("`{t}`"))
            .collect();
        Some(format!("{} · {what}", path.join(" → ")))
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
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        let RawInvokeTarget::Workflow(target) = &invoke.target else {
            continue;
        };
        let mut stack = vec![normalize_path(root)];
        let reach = reach_of_target(root, &target.value, walker, &mut memo, &mut stack);
        out.push(ComposedCall {
            task: task.value.id.value.clone(),
            target: target.value.clone(),
            ingress: reach.witness(&target.value),
            grants: held_grants(caller, &reach),
        });
    }
    out
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
        return Reach::unresolved(format!(
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
            "closes a cycle in the call graph (spec 14 law 7) · untrusted (fail-closed)".to_owned(),
        );
    }
    let child = match walker.load(&id) {
        Ok(child) => child.clone(),
        Err(detail) => {
            let reach = Reach::unresolved(format!(
                "{detail} · untrusted until it resolves (fail-closed)"
            ));
            memo.insert(id, reach.clone());
            return reach;
        }
    };
    stack.push(id.clone());
    let mut reach = own_reach(&child);
    for task in &child.tasks {
        let action = &task.value.action;
        if let RawAction::Invoke(invoke) = action
            && let RawInvokeTarget::Workflow(target) = &invoke.target
        {
            let below = reach_of_target(&id, &target.value, walker, memo, stack);
            reach.absorb(&target.value, below);
        } else if reach.ingress.is_none() {
            reach.ingress = born_ingress(&task.value.id.value, action);
        }
    }
    stack.pop();
    memo.insert(id, reach.clone());
    reach
}

/// One workflow's own needs and opaque faces, its calls excluded.
fn own_reach(wf: &RawWorkflow) -> Reach {
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
        needs: inferred.permits,
        opaque,
        ..Reach::empty()
    }
}

/// The witness of a task the content-flow table judges a born
/// untrusted-content source inline (the SAME predicate), else `None`.
fn born_ingress(task_id: &str, action: &RawAction) -> Option<String> {
    // The catalog's MCP trust mark has no reader yet: every server is
    // untrusted, as in the trifecta lane itself.
    let mcp_trusted = |_: &str| false;
    // A nested call is folded by the walk, never classified here.
    let no_call = |_: &str| false;
    let sources = super::content_flow::Sources {
        mcp_trusted: &mcp_trusted,
        untrusted_call: &no_call,
    };
    if !super::content_flow::classify(task_id, action, &sources).0 {
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

/// The caller's own entries a closure with `reach` uses or may use.
fn held_grants(caller: Option<&Permits>, reach: &Reach) -> Permits {
    let Some(caller) = caller else {
        return Permits::new();
    };
    if !reach.resolved {
        return caller.clone();
    }
    let (needs, opaque) = (&reach.needs, reach.opaque);
    let mut held = Permits::new();
    held.exec = match &caller.exec {
        Some(ExecPermit::Any) if opaque.exec || needs.allows_exec() => Some(ExecPermit::Any),
        Some(ExecPermit::Programs(list)) => {
            let kept: Vec<String> = list
                .iter()
                .filter(|p| opaque.exec || needs.allows_program(p))
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
                opaque.tools
                    || needs
                        .tools
                        .iter()
                        .flatten()
                        .any(|t| nika_cap::glob_matches(g, t))
            })
            .cloned()
            .collect();
        (!kept.is_empty()).then_some(kept)
    });
    held.net = caller.net.as_ref().and_then(|net| {
        let kept: Vec<String> = net
            .http
            .iter()
            .filter(|entry| opaque.net || needs_host(needs, entry))
            .cloned()
            .collect();
        (!kept.is_empty()).then(|| NetPermits::new(kept))
    });
    held.fs = caller.fs.as_ref().and_then(|fs| {
        let read = held_paths(&fs.read, opaque.fs, needs, false);
        let write = held_paths(&fs.write, opaque.fs, needs, true);
        (!read.is_empty() || !write.is_empty()).then(|| FsPermits::new(read, write))
    });
    held
}

/// Does the single host entry `entry` admit a host the closure needs?
fn needs_host(needs: &Permits, entry: &str) -> bool {
    needs.net.iter().flat_map(|n| &n.http).any(|host| {
        let mut one = Permits::new();
        one.net = Some(NetPermits::new(vec![entry.to_owned()]));
        one.allows_host(host)
    })
}

/// The path entries (one direction) that admit a path the closure needs.
fn held_paths(entries: &[String], opaque: bool, needs: &Permits, write: bool) -> Vec<String> {
    let needed: &[String] = match (&needs.fs, write) {
        (Some(fs), false) => &fs.read,
        (Some(fs), true) => &fs.write,
        (None, _) => &[],
    };
    entries
        .iter()
        .filter(|entry| {
            opaque
                || needed.iter().any(|path| {
                    let mut one = Permits::new();
                    one.fs = Some(if write {
                        FsPermits::new(Vec::new(), vec![(*entry).clone()])
                    } else {
                        FsPermits::new(vec![(*entry).clone()], Vec::new())
                    });
                    one.allows_path(path, write)
                })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FETCH: &str = "nika: leaf\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://docs.example.com/page\" } }\noutputs:\n  text: { value: \"${{ tasks.page.output }}\", type: string }\n";
    const GIT: &str = "nika: log\npermits: { exec: [\"git\"], fs: { read: [\"./repo/**\"] } }\ntasks:\n  log:\n    exec: { command: [\"git\", \"-C\", \"./repo\", \"log\"] }\noutputs:\n  text: { value: \"${{ tasks.log.output }}\", type: string }\n";
    const PURE: &str = "nika: pure\nmodel: mock/echo\npermits: {}\ntasks:\n  brief:\n    infer: { prompt: \"hi\", max_tokens: 8 }\noutputs:\n  text: { value: \"${{ tasks.brief.output }}\", type: string }\n";

    /// A caller holding a wide boundary and calling `target` once.
    fn caller(target: &str) -> String {
        format!(
            "nika: caller\npermits:\n  exec: [\"git\", \"jq\"]\n  tools: [\"nika:fetch\", \"nika:read\", \"nika:glob\"]\n  net: {{ http: [\"docs.example.com\", \"hooks.example.com\"] }}\n  fs: {{ read: [\"./repo/**\", \"./notes/**\"], write: [\"./out/**\"] }}\ntasks:\n  call:\n    invoke: {{ workflow: \"{target}\" }}\n"
        )
    }

    fn calls(target: &str, files: &[(&str, &str)]) -> Vec<ComposedCall> {
        let wf = nika_schema::parse(
            &caller(target),
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        crate::check_composed(&wf, "caller.nika", &mut |path| {
            files
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, text)| (*text).to_owned())
                .ok_or_else(|| format!("no such file: {path}"))
        })
        .composed_calls
    }

    fn only(calls: &[ComposedCall]) -> &ComposedCall {
        assert_eq!(calls.len(), 1, "{calls:?}");
        &calls[0]
    }

    #[test]
    fn a_fetching_child_is_ingress_and_holds_exactly_its_fetch_grants() {
        let found = calls("./leaf.nika", &[("leaf.nika", FETCH)]);
        let call = only(&found);
        assert_eq!(call.task, "call");
        assert_eq!(call.target, "./leaf.nika");
        assert_eq!(
            call.ingress.as_deref(),
            Some("`./leaf.nika` · task `page` invokes `nika:fetch`")
        );
        assert_eq!(call.grants.tools, Some(vec!["nika:fetch".to_owned()]));
        assert_eq!(
            call.grants.net,
            Some(NetPermits::new(vec!["docs.example.com".to_owned()]))
        );
        assert_eq!(call.grants.exec, None, "the child runs no program");
        assert_eq!(call.grants.fs, None, "the child touches no file");
    }

    #[test]
    fn an_exec_child_is_ingress_and_its_subprocess_holds_every_fs_and_net_entry() {
        let found = calls("./log.nika", &[("log.nika", GIT)]);
        let call = only(&found);
        assert_eq!(
            call.ingress.as_deref(),
            Some("`./log.nika` · task `log` is an `exec:` (its output is untrusted content)")
        );
        assert_eq!(
            call.grants.exec,
            Some(ExecPermit::Programs(vec!["git".to_owned()])),
            "only the program the child runs, `jq` stays judged"
        );
        assert_eq!(
            call.grants.fs,
            Some(FsPermits::new(
                vec!["./repo/**".to_owned(), "./notes/**".to_owned()],
                vec!["./out/**".to_owned()],
            )),
            "a subprocess's file reach is opaque: no fs claim"
        );
        assert_eq!(
            call.grants.net.as_ref().map(|n| n.http.len()),
            Some(2),
            "nor a network claim"
        );
        assert_eq!(call.grants.tools, None);
    }

    #[test]
    fn a_pure_child_brings_nothing_in_and_holds_nothing() {
        let found = calls("./pure.nika", &[("pure.nika", PURE)]);
        let call = only(&found);
        assert_eq!(call.ingress, None);
        assert_eq!(call.grants, Permits::new());
    }

    #[test]
    fn the_witness_walks_down_to_a_grandchild() {
        let middle = "nika: middle\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  inner:\n    invoke: { workflow: \"./deep/leaf.nika\" }\n";
        let found = calls(
            "./middle.nika",
            &[("middle.nika", middle), ("deep/leaf.nika", FETCH)],
        );
        let call = only(&found);
        assert_eq!(
            call.ingress.as_deref(),
            Some("`./middle.nika` → `./deep/leaf.nika` · task `page` invokes `nika:fetch`")
        );
        assert_eq!(
            call.grants.tools,
            Some(vec!["nika:fetch".to_owned()]),
            "the grandchild's need is held through the middle"
        );
    }

    #[test]
    fn an_unresolvable_child_is_ingress_and_holds_the_whole_block() {
        let found = calls("./ghost.nika", &[]);
        let call = only(&found);
        let why = call.ingress.as_deref().expect("fail-closed");
        assert!(
            why.starts_with("`./ghost.nika` · cannot read `ghost.nika`"),
            "{why}"
        );
        assert!(why.ends_with("(fail-closed)"), "{why}");
        let wide = nika_schema::parse(
            &caller("./ghost.nika"),
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        assert_eq!(
            Some(&call.grants),
            wide.permits.as_ref().map(|p| &p.value),
            "any grant may carry an unread child"
        );

        let registry = calls("registry:acme/pages@1.0.0", &[]);
        let why = only(&registry).ingress.as_deref().expect("fail-closed");
        assert!(why.contains("cannot be resolved before the run"), "{why}");
    }

    #[test]
    fn a_cycle_is_ingress_and_the_walk_terminates() {
        let a = "nika: a\npermits: {}\ntasks:\n  back:\n    invoke: { workflow: \"./b.nika\" }\n";
        let b = "nika: b\npermits: {}\ntasks:\n  again:\n    invoke: { workflow: \"./a.nika\" }\n";
        let found = calls("./a.nika", &[("a.nika", a), ("b.nika", b)]);
        let why = only(&found).ingress.as_deref().expect("fail-closed");
        assert!(why.contains("closes a cycle"), "{why}");
    }

    #[test]
    fn an_agent_whitelist_holds_its_tools_and_every_net_and_fs_entry() {
        let agent = "nika: browse\nmodel: mock/echo\npermits: { tools: [\"nika:fetch\"], net: { http: [\"docs.example.com\"] } }\ntasks:\n  look:\n    agent: { prompt: \"read the page\", tools: [\"nika:fetch\"] }\noutputs:\n  text: { value: \"${{ tasks.look.output }}\", type: string }\n";
        let found = calls("./browse.nika", &[("browse.nika", agent)]);
        let call = only(&found);
        assert!(
            call.ingress
                .as_deref()
                .is_some_and(|w| w.contains("task `look` is an `agent:`")),
            "{call:?}"
        );
        assert_eq!(
            call.grants.tools.as_ref().map(Vec::len),
            Some(3),
            "a whitelist dispatches dynamically: no tools claim"
        );
        assert!(call.grants.net.is_some() && call.grants.fs.is_some());
    }

    #[test]
    fn a_reader_less_check_resolves_no_call() {
        let wf = nika_schema::parse(
            &caller("./leaf.nika"),
            nika_schema::source::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        assert!(crate::check(&wf).composed_calls.is_empty());
    }
}
