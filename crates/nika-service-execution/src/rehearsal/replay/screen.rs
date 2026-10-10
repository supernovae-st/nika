// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a replay trial cannot exercise of a workflow, named before any run, and what each task
//! reads. A model step, a process, a notification, a person, an mcp server and a provider stay
//! outside every room. A fetch is exercised when it GETs an address a capture holds, or one known
//! only at run time: the room then serves an exact capture, or refuses. A crawl, a jq extraction
//! (it would run in this process, with no bound), a request other than GET and an address no
//! capture holds are not exercised. Pure: the workflow is read, nothing is run.

use std::collections::BTreeMap;

use nika_schema::VarDecl;
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{ForEachValue, RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use serde_json::Value;

use super::Captures;

#[cfg(test)]
mod tests;

/// What a replay trial cannot exercise of one workflow, and what each of its tasks reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReplayScreen {
    /// The tasks the trial does not exercise, in the workflow's order, each with what it needs
    /// in plain words: `the model step`, `a process`, `a request other than GET`, `the network`,
    /// `a crawl`, `a jq extraction`, `a notification`, `a person`, `an mcp server` or
    /// `a provider`.
    pub unexercised: Vec<(String, String)>,
    /// Every task, with the tasks it reads, each once: each `tasks.<id>` and each member of a
    /// `group.<name>` in its `with:` values, its arguments and its collection, then its `after:`
    /// producers.
    pub upstream: BTreeMap<String, Vec<String>>,
    /// The tasks whose output is what a `nika:jq` filter kept of what they read, in the
    /// workflow's order.
    pub filters: Vec<String>,
}

impl ReplayScreen {
    /// A screen of these facts (INV-019).
    #[must_use]
    pub fn new(
        unexercised: Vec<(String, String)>,
        upstream: BTreeMap<String, Vec<String>>,
        filters: Vec<String>,
    ) -> Self {
        Self {
            unexercised,
            upstream,
            filters,
        }
    }

    /// Whether the trial does not exercise `task`.
    #[must_use]
    pub fn is_unexercised(&self, task: &str) -> bool {
        self.unexercised.iter().any(|(id, _)| id == task)
    }

    /// Whether `task` reads, directly or through other tasks, one the trial does not exercise:
    /// it runs, if at all, on what the room could not give.
    #[must_use]
    pub fn behind(&self, task: &str) -> bool {
        let (mut seen, mut open): (Vec<&str>, Vec<&str>) = (Vec::new(), vec![task]);
        while let Some(at) = open.pop() {
            for read in self.upstream.get(at).into_iter().flatten() {
                if self.is_unexercised(read) {
                    return true;
                }
                if !seen.contains(&read.as_str()) {
                    seen.push(read);
                    open.push(read);
                }
            }
        }
        false
    }

    /// Whether this screen answers for `task` in a host's own screen: a task the trial does not
    /// exercise, one behind it, or a fetch, which the room replays or refuses.
    #[must_use]
    pub fn screens(&self, task: &RawTask) -> bool {
        let id = task.id.value.as_str();
        self.is_unexercised(id) || self.behind(id) || tool_of(task) == Some("nika:fetch")
    }
}

/// What a trial over `captures` cannot exercise of `workflow`, and what each task reads.
#[must_use]
pub fn screen(workflow: &RawWorkflow, captures: &Captures) -> ReplayScreen {
    let mut screened = ReplayScreen::default();
    for task in &workflow.tasks {
        let (task, id) = (&task.value, task.value.id.value.clone());
        if let Some(need) = need(workflow, task, captures) {
            screened.unexercised.push((id.clone(), need.to_owned()));
        }
        if tool_of(task) == Some("nika:jq") {
            screened.filters.push(id.clone());
        }
        screened.upstream.insert(id, reads(workflow, task));
    }
    screened
}

/// The addresses `workflow` GETs that are known before the run, each once, in the workflow's
/// order: what a host captures for a trial. A crawl, a jq extraction and a request other than
/// GET are left out, as the trial leaves them unexercised.
#[must_use]
pub fn sources(workflow: &RawWorkflow) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for task in &workflow.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        if tool_of(&task.value) != Some("nika:fetch") {
            continue;
        }
        let args = invoke.args.as_ref().map(|args| &args.value);
        let arg = |name: &str| args.and_then(|args| args.get(name));
        let mode = arg("mode").and_then(|mode| known(workflow, mode));
        let method = arg("method").and_then(|method| known(workflow, method));
        let plain =
            arg("traverse").is_none() && arg("jq").is_none() && mode.as_deref() != Some("jq");
        let get = method.is_none_or(|method| method.eq_ignore_ascii_case("GET"));
        let url = arg("url").and_then(|url| known(workflow, url));
        if let Some(url) = url.filter(|_| plain && get)
            && !found.contains(&url)
        {
            found.push(url);
        }
    }
    found
}

/// What `task` needs that a trial over `captures` does not exercise, if anything.
fn need(workflow: &RawWorkflow, task: &RawTask, captures: &Captures) -> Option<&'static str> {
    let invoke = match &task.action {
        RawAction::Invoke(invoke) => invoke,
        RawAction::Infer(_) | RawAction::Agent(_) => return Some("the model step"),
        RawAction::Exec(_) => return Some("a process"),
        _ => return None,
    };
    let RawInvokeTarget::Tool(tool) = &invoke.target else {
        return None;
    };
    match tool.value.as_str() {
        "nika:fetch" => fetched(
            workflow,
            invoke.args.as_ref().map(|args| &args.value),
            captures,
        ),
        "nika:notify" => Some("a notification"),
        "nika:prompt" => Some("a person"),
        "nika:image_generate" | "nika:tts_generate" => Some("a provider"),
        mcp if mcp.starts_with("mcp:") => Some("an mcp server"),
        _ => None,
    }
}

/// What a fetch with `args` needs beyond a trial over `captures`, if anything. A GET of an
/// address known only at run time is exercised: the room serves an exact capture, or refuses.
fn fetched(
    workflow: &RawWorkflow,
    args: Option<&Value>,
    captures: &Captures,
) -> Option<&'static str> {
    let arg = |name: &str| args.and_then(|args| args.get(name));
    if arg("traverse").is_some() {
        return Some("a crawl");
    }
    let mode = arg("mode").and_then(|mode| known(workflow, mode));
    if arg("jq").is_some() || mode.as_deref() == Some("jq") {
        return Some("a jq extraction");
    }
    let method = arg("method").and_then(|method| known(workflow, method));
    if method.is_some_and(|method| !method.eq_ignore_ascii_case("GET")) {
        return Some("a request other than GET");
    }
    let url = arg("url").and_then(|url| known(workflow, url))?;
    captures.get(&url).is_none().then_some("the network")
}

/// The text a value holds before the run: a literal, or a `${{ const.NAME }}` whose value is a
/// string. Any other template is known only at run time.
fn known(workflow: &RawWorkflow, value: &Value) -> Option<String> {
    let text = value.as_str()?;
    if !text.contains("${{") {
        return Some(text.to_owned());
    }
    let name = text
        .trim()
        .strip_prefix("${{")?
        .strip_suffix("}}")?
        .trim()
        .strip_prefix("const.")?;
    let (_, declared) = workflow
        .consts
        .iter()
        .find(|(constant, _)| constant.value == name)?;
    let value = match declared {
        VarDecl::Untyped(value) => Some(value),
        VarDecl::Typed { default, .. } => default.as_ref(),
    }?;
    value.as_str().map(str::to_owned)
}

/// The tool `task` invokes, when it invokes one.
fn tool_of(task: &RawTask) -> Option<&str> {
    match &task.action {
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) => Some(tool.value.as_str()),
            RawInvokeTarget::Workflow(_) => None,
        },
        _ => None,
    }
}

/// The tasks `task` reads, each once: every `tasks.<id>`, and every member of a `group.<name>`,
/// in its `with:` values, its arguments and its collection, then its `after:` producers.
fn reads(workflow: &RawWorkflow, task: &RawTask) -> Vec<String> {
    let mut found = Vec::new();
    for (_, value) in &task.with {
        references(&value.value, &mut found);
    }
    if let RawAction::Invoke(invoke) = &task.action
        && let Some(args) = &invoke.args
    {
        references(&args.value, &mut found);
    }
    match task.for_each.as_ref().map(|fan| &fan.value) {
        Some(ForEachValue::Expression(source)) => text_references(source, &mut found),
        Some(ForEachValue::List(list)) => references(list, &mut found),
        _ => {}
    }
    let named = found.into_iter().flat_map(|reference| match reference {
        NamespaceRef::Tasks { id, .. } => vec![id],
        NamespaceRef::Group(name) => members(workflow, &name),
        _ => Vec::new(),
    });
    let after = task
        .after
        .iter()
        .map(|(producer, _)| producer.value.clone());
    let mut read: Vec<String> = Vec::new();
    for id in named.chain(after) {
        if !read.contains(&id) {
            read.push(id);
        }
    }
    read
}

/// Every root reference the template strings of `value` hold, at any depth.
fn references(value: &Value, found: &mut Vec<NamespaceRef>) {
    match value {
        Value::String(text) => text_references(text, found),
        Value::Array(items) => {
            for item in items {
                references(item, found);
            }
        }
        Value::Object(fields) => {
            for field in fields.values() {
                references(field, found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Every root reference the template islands of `text` hold. A text the shared scanner cannot
/// read names none.
fn text_references(text: &str, found: &mut Vec<NamespaceRef>) {
    for island in scan_templates(text).unwrap_or_default() {
        found.extend(expr_refs(&island.expr));
    }
}

/// The tasks that join the group `name`, in the workflow's order.
fn members(workflow: &RawWorkflow, name: &str) -> Vec<String> {
    (workflow.tasks.iter())
        .filter(|task| (task.value.group.as_ref()).is_some_and(|group| group.value == name))
        .map(|task| task.value.id.value.clone())
        .collect()
}
