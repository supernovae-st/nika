// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The screen a candidate passes before any room exists. Every task is read, whether or not it
//! could run: a verb other than `invoke:`, a nested workflow, a tool a template names, a tool
//! outside the surface a rehearsal runs, a jq or convert step and any `extract:` (no data bound
//! is established for them), and a `secrets:` block are refused, each in its own words. Every
//! path a literal or a constant names must stay in the room, and a read must name an observed
//! input or one of the candidate's own outputs. A path known only at run time passes: the room
//! itself confines it. Last, the amplifying value forms described in `arguments` are refused
//! before evaluation, independently of the room's write budget.

use nika_compile_cognition::rehearse::Refusal;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use nika_schema::{FileId, ParseMode, VarDecl};
use nika_service_execution::ADMITTED_TOOLS;
use serde_json::Value;

/// A refusal before any room: its type, and its words.
#[derive(Debug)]
pub(super) struct Refused {
    pub(super) refusal: Refusal,
    pub(super) reason: String,
}

impl Refused {
    pub(super) fn new(refusal: Refusal, reason: impl Into<String>) -> Self {
        Self {
            refusal,
            reason: reason.into(),
        }
    }
}

/// One file the candidate writes at a path known before the run.
#[derive(Debug)]
pub(super) struct Output {
    /// The path as the candidate writes it.
    pub(super) path: String,
    /// The same file, as the room spells it.
    pub(super) at: String,
    /// The tasks that write it, each with whether a `when:` guards it.
    pub(super) writers: Vec<(String, bool)>,
}

/// What the screen established before any room: the inputs to copy (as the caller names them,
/// and as the room spells them), the outputs known before the run and every task, each in the
/// candidate's order.
#[derive(Debug)]
pub(super) struct Screened {
    pub(super) inputs: Vec<(String, String)>,
    pub(super) outputs: Vec<Output>,
    pub(super) tasks: Vec<String>,
}

/// Screen `candidate` over the observed `inputs`.
pub(super) fn screen(candidate: &str, inputs: &[String]) -> Result<Screened, Refused> {
    let workflow =
        nika_schema::parse(candidate, FileId::new(0), ParseMode::Strict).map_err(|error| {
            Refused::new(
                Refusal::Admission,
                format!("the admission door cannot read the candidate: {error}"),
            )
        })?;
    if !workflow.secrets.is_empty() {
        return Err(Refused::new(
            Refusal::Effect,
            "the candidate declares a secret, and a rehearsal resolves none",
        ));
    }
    let inputs = inputs
        .iter()
        .map(|input| Ok((input.clone(), room_path(input, "the observed input")?)))
        .collect::<Result<Vec<_>, Refused>>()?;
    let mut screened = Screened {
        inputs,
        outputs: Vec::new(),
        tasks: Vec::new(),
    };
    let mut reads = Vec::new();
    for task in &workflow.tasks {
        if let Some(read) = screen_task(&workflow, &task.value, &mut screened.outputs)? {
            reads.push(read);
        }
        screened.tasks.push(task.value.id.value.clone());
    }
    for (path, at) in reads {
        let observed = screened.inputs.iter().any(|(_, input)| *input == at);
        if !observed && !screened.outputs.iter().any(|output| output.at == at) {
            return Err(Refused::new(
                Refusal::Confinement,
                format!("{path} is read but not observed: the room holds the observed inputs only"),
            ));
        }
    }
    super::arguments::evaluated(&workflow)?;
    Ok(screened)
}

/// Screen one task. A write at a known path joins `outputs`; a read at a known path is returned.
fn screen_task(
    workflow: &RawWorkflow,
    task: &RawTask,
    outputs: &mut Vec<Output>,
) -> Result<Option<(String, String)>, Refused> {
    let id = task.id.value.as_str();
    if !task.extract.is_empty() {
        return Err(bounded(id, "extracts with jq"));
    }
    let invoke = match &task.action {
        RawAction::Invoke(invoke) => invoke,
        RawAction::Exec(_) => return Err(effect(id, "exec", "a rehearsal spawns no process")),
        RawAction::Infer(_) | RawAction::Agent(_) => {
            return Err(effect(id, "a provider", "a rehearsal calls no model"));
        }
        _ => {
            return Err(Refused::new(
                Refusal::Surface,
                format!("task {id} uses a verb unknown to a rehearsal"),
            ));
        }
    };
    let tool = match &invoke.target {
        RawInvokeTarget::Workflow(_) => {
            return Err(effect(
                id,
                "a nested workflow",
                "a rehearsal starts no nested run",
            ));
        }
        RawInvokeTarget::Tool(tool) => tool.value.as_str(),
    };
    admitted(id, tool)?;
    if tool != "nika:read" && tool != "nika:write" {
        return Ok(None);
    }
    let Some(path) = invoke
        .args
        .as_ref()
        .and_then(|args| args.value.get("path"))
        .and_then(|path| known(workflow, path))
    else {
        return Ok(None);
    };
    let at = room_path(&path, &format!("task {id} names"))?;
    if tool == "nika:read" {
        return Ok(Some((path, at)));
    }
    let writer = (id.to_owned(), task.when.is_some());
    match outputs.iter_mut().find(|output| output.at == at) {
        Some(output) => output.writers.push(writer),
        None => outputs.push(Output {
            path,
            at,
            writers: vec![writer],
        }),
    }
    Ok(None)
}

/// Whether `tool` is one a rehearsal runs, refused in the words of what it reaches for.
fn admitted(id: &str, tool: &str) -> Result<(), Refused> {
    if tool.contains("${{") {
        return Err(Refused::new(
            Refusal::Surface,
            format!("task {id} names its tool by a template: a tool unknown before the run"),
        ));
    }
    match tool {
        "nika:jq" | "nika:convert" => Err(bounded(id, &format!("runs {tool}"))),
        "nika:fetch" => Err(effect(id, "the network", "a rehearsal opens no socket")),
        "nika:notify" => Err(effect(
            id,
            "the network",
            "a notification reaches beyond files",
        )),
        "nika:image_generate" | "nika:tts_generate" => {
            Err(effect(id, "a provider", "a rehearsal calls no provider"))
        }
        "nika:prompt" => Err(effect(id, "a person", "a rehearsal answers no gate")),
        mcp if mcp.starts_with("mcp:") => {
            Err(effect(id, "an mcp server", "a rehearsal reaches none"))
        }
        surface if ADMITTED_TOOLS.contains(&surface) => Ok(()),
        other => Err(Refused::new(
            Refusal::Surface,
            format!("task {id} runs {other}, a tool unknown to the surface a rehearsal runs"),
        )),
    }
}

/// The path a `path:` argument names before the run: a literal, or a `${{ const.NAME }}` whose
/// value is a string. Any other template is known only at run time.
fn known(workflow: &RawWorkflow, path: &Value) -> Option<String> {
    let text = path.as_str()?;
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

/// The room's spelling of `path`, `.` and empty components dropped. An absolute path, a `..` or
/// no name at all is refused before any room.
pub(super) fn room_path(path: &str, what: &str) -> Result<String, Refused> {
    let outside = |why: &str| {
        Refused::new(
            Refusal::Confinement,
            format!("{what} {path}: {why}, outside the room"),
        )
    };
    if path.starts_with('/') {
        return Err(outside("an absolute path"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(outside("a path that climbs")),
            name => parts.push(name),
        }
    }
    if parts.is_empty() {
        return Err(outside("no file"));
    }
    Ok(parts.join("/"))
}

/// A refusal of what reaches beyond the room's files.
fn effect(id: &str, what: &str, why: &str) -> Refused {
    Refused::new(Refusal::Effect, format!("task {id} needs {what}: {why}"))
}

/// A refusal of a data step with no established bound.
fn bounded(id: &str, what: &str) -> Refused {
    Refused::new(
        Refusal::DataBounds,
        format!("task {id} {what}, outside any bounded subset a rehearsal vouches for"),
    )
}
