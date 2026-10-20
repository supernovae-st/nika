// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source-only hold on an `mcp:` step, and the way it lifts: the project registers the
//! server, approved its pins, and the pins hold the tool. An `invoke:` step's literal arguments
//! are then judged against the pinned input schema; an argument a template builds is wired, its
//! value known only when the run makes it. No server is reached: a pin is what an operator
//! approved, read from the project's files. Any other answer keeps the hold, in words that say
//! what to do. Permits stay the only authority over what a step may do: judging grants nothing.
//!
//! Each verdict is recorded on `decision.mcp` (`[{task, tools: [{tool, verdict, reason, …}]}]`),
//! the typed record a reader can key on; the hold's diagnostic carries the same instruction in
//! words. `approved_at` is the lockfile's approval stamp, not a schema identity: the run still
//! verifies the live server against its pins.

use std::collections::BTreeMap;

use nika_compile::surface::{UNJUDGED_DEPENDENCY, parse, ready_by_law, state_check_refusals};
use nika_compile::{CompileOutcome, CompileStatus, DiagnosticKind};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use serde_json::{Map, Value, json};

use super::Rehearse;

/// What the project says about one `mcp:<server>/<tool>`, read from its registry and its approved
/// pins, never by reaching the server.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PinnedTool {
    /// Registered and approved, and the pins hold the tool.
    Pinned {
        /// The tool's `inputSchema`, as approved.
        input_schema: Value,
        /// The tool's `outputSchema`, as approved, when the pins hold one.
        output_schema: Option<Value>,
        /// When the server's pin set was approved (the lockfile's own stamp).
        approved_at: String,
    },
    /// The project's registry does not name the server.
    Unregistered,
    /// The server is registered, and no pin set was approved for it.
    Unapproved,
    /// The registry names the server with another command, arguments or URL than the one its
    /// pins were approved for: the approval does not carry over.
    Repointed,
    /// The approved pins do not hold the tool.
    NotPinned {
        /// The tools the approved pins hold.
        pinned: Vec<String>,
    },
    /// This host reads no pins.
    Unoffered,
}

/// One concrete `mcp:` tool a held task names.
struct Call {
    server: String,
    tool: String,
    /// The `invoke:` arguments (an empty object when none is written); `None` for an agent's
    /// tool, whose arguments its model writes at run time.
    args: Option<Value>,
}

/// What one call came to.
enum Verdict {
    Judged {
        wired: Vec<String>,
        approved_at: Option<String>,
    },
    Held(String),
    /// The host reads no pins: the task stays exactly as the core left it.
    Unoffered,
}

/// Judge every `mcp:` step `out`'s candidate still holds against `host`'s pins, and record each
/// task's verdict on `decision.mcp`. A task whose every tool resolves (registered, approved,
/// pinned, its literal arguments meeting the pinned input schema) loses its hold; any other keeps
/// it, in words naming what to do. A host whose runs make no MCP call keeps even a judged step
/// held, saying so (the record's `held`). READY then follows the core's own law. A host that reads
/// no pins, a refused outcome, a child workflow, a skill or a tool named by a template change
/// nothing.
pub fn judged_tools(host: &dyn Rehearse, out: &mut CompileOutcome) {
    if out.status == CompileStatus::Refused {
        return;
    }
    let Some(wf) = out.candidate.as_deref().and_then(|c| parse(c).ok()) else {
        return;
    };
    let mut records = Vec::new();
    let mut lifted = Vec::new();
    let mut reasons: BTreeMap<String, String> = BTreeMap::new();
    for (task, calls) in held_steps(out, &wf) {
        let verdicts: Vec<(String, Verdict)> = (calls.iter())
            .map(|call| {
                (
                    format!("mcp:{}/{}", call.server, call.tool),
                    judge(host, call),
                )
            })
            .collect();
        if verdicts
            .iter()
            .any(|(_, v)| matches!(v, Verdict::Unoffered))
        {
            continue;
        }
        let tools: Vec<Value> = verdicts.iter().map(|(tool, v)| record(tool, v)).collect();
        let held: Vec<&str> = (verdicts.iter())
            .filter_map(|(_, v)| match v {
                Verdict::Held(reason) => Some(reason.as_str()),
                _ => None,
            })
            .collect();
        let mut entry = json!({"task": task, "tools": tools, "output": "untyped"});
        if !held.is_empty() {
            reasons.insert(task.clone(), held.join(" · "));
        } else if host.runs_mcp_steps() {
            lifted.push(task.clone());
        } else {
            // Judged, and still held: the runs this compile answers for make no MCP call.
            let reason = unrun_here(&calls);
            entry["held"] = json!(reason);
            reasons.insert(task.clone(), reason);
        }
        records.push(entry);
    }
    if records.is_empty() {
        return;
    }
    (out.diagnostics).retain(|d| d.message != UNJUDGED_DEPENDENCY || !lifted.contains(&d.target));
    for diagnostic in &mut out.diagnostics {
        if diagnostic.message == UNJUDGED_DEPENDENCY
            && let Some(reason) = reasons.get(&diagnostic.target)
        {
            diagnostic.message.clone_from(reason);
        }
    }
    let clean = (out.check_preview.as_ref()).is_some_and(|preview| preview.report.is_clean());
    if !lifted.is_empty() && ready_by_law(out, clean) {
        out.status = CompileStatus::Ready;
    } else if !clean {
        // Judged holds may leave Check's own refusals as a reason the candidate waits, alone or
        // beside a hold that remains: they are stated once, as the core states them, so no
        // remaining cause hides another.
        let report = (out.check_preview.as_ref()).map(|preview| preview.report.clone());
        if let Some(report) = report {
            state_check_refusals(out, &report);
        }
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["mcp"] = Value::Array(records);
    out.provenance.decision = Some(decision);
}

/// Why a step its pins judge stays held where the runs make no MCP call: no READY promises a run
/// this server cannot make.
fn unrun_here(calls: &[Call]) -> String {
    let tools: Vec<String> = (calls.iter())
        .map(|call| format!("`mcp:{}/{}`", call.server, call.tool))
        .collect();
    format!(
        "{} judged on the approved pins, but this server runs no MCP step: the step stays held \
         here",
        tools.join(", ")
    )
}

/// The tasks of `wf` that still carry the core's source-only hold and name only concrete `mcp:`
/// tools: an `invoke:` of one, or an agent without skills whose `mcp:` tools are all concrete
/// or a server's glob. A child workflow, a skill or a templated tool name keeps its hold.
fn held_steps(out: &CompileOutcome, wf: &RawWorkflow) -> Vec<(String, Vec<Call>)> {
    let held = |id: &str| {
        (out.diagnostics.iter()).any(|d| {
            d.kind == DiagnosticKind::Unknown && d.target == id && d.message == UNJUDGED_DEPENDENCY
        })
    };
    (wf.tasks.iter())
        .filter(|t| held(&t.value.id.value))
        .filter_map(|t| {
            let calls = match &t.value.action {
                RawAction::Invoke(invoke) => match &invoke.target {
                    RawInvokeTarget::Tool(tool) => {
                        let args = (invoke.args.as_ref())
                            .map_or_else(|| Value::Object(Map::new()), |args| args.value.clone());
                        vec![call(&tool.value, Some(args))?]
                    }
                    RawInvokeTarget::Workflow(_) => return None,
                },
                RawAction::Agent(agent) if agent.skills.is_empty() => (agent.tools.iter())
                    .filter(|tool| tool.value.starts_with("mcp:"))
                    .map(|tool| call(&tool.value, None))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            };
            // Never lift vacuously: a task the hold names but that calls no tool stays held.
            (!calls.is_empty()).then(|| (t.value.id.value.clone(), calls))
        })
        .collect()
}

/// `mcp:<server>/<tool>` as a call, unless a template names it (a tool unknown before the run).
fn call(name: &str, args: Option<Value>) -> Option<Call> {
    if name.contains("${{") {
        return None;
    }
    let (server, tool) = name.strip_prefix("mcp:")?.split_once('/')?;
    (!server.is_empty() && !tool.is_empty()).then(|| Call {
        server: server.to_owned(),
        tool: tool.to_owned(),
        args,
    })
}

/// One call against the host's pins. An agent's glob (`mcp:<server>/*`) names no tool: pins
/// that do not hold `*` prove the server approved, which is all a glob asks.
fn judge(host: &dyn Rehearse, call: &Call) -> Verdict {
    let Call { server, tool, args } = call;
    let name = format!("mcp:{server}/{tool}");
    match host.pinned(server, tool) {
        PinnedTool::Pinned {
            input_schema,
            approved_at,
            ..
        } => match args
            .as_ref()
            .map(|args| argument_errors(&input_schema, args))
        {
            None => Verdict::Judged {
                wired: Vec::new(),
                approved_at: Some(approved_at),
            },
            Some(Ok((wired, errors))) if errors.is_empty() => Verdict::Judged {
                wired,
                approved_at: Some(approved_at),
            },
            Some(Ok((_, errors))) => Verdict::Held(format!(
                "`{name}` arguments do not meet its pinned input schema: {}",
                errors.join("; ")
            )),
            Some(Err(why)) => Verdict::Held(format!(
                "the pinned input schema of `{name}` does not compile ({why}): approve \
                 `{server}` again with `nika mcp approve {server}`"
            )),
        },
        PinnedTool::NotPinned { .. } if tool == "*" => Verdict::Judged {
            wired: Vec::new(),
            approved_at: None,
        },
        PinnedTool::NotPinned { pinned } => Verdict::Held(format!(
            "`{tool}` is not among the tools MCP server `{server}` was approved with (pinned: \
             {}): if the server serves it now, approve it again with `nika mcp approve {server}`",
            pinned.join(", ")
        )),
        PinnedTool::Unapproved => Verdict::Held(format!(
            "MCP server `{server}` is registered but not approved: `nika mcp approve {server}` \
             pins the tools it serves now; verify again after"
        )),
        PinnedTool::Repointed => Verdict::Held(format!(
            "MCP server `{server}` now names another command, arguments or URL than the one its \
             pins were approved for: `nika mcp approve {server}` reviews the tools it serves now; \
             verify again after"
        )),
        PinnedTool::Unregistered => Verdict::Held(format!(
            "MCP server `{server}` is not in the project's registry (`.nika/mcp_servers.json`): \
             declare it, approve it with `nika mcp approve {server}`, then verify again"
        )),
        PinnedTool::Unoffered => Verdict::Unoffered,
    }
}

/// `args` against `schema`: the JSON Pointer of every argument a template builds (wired, the
/// run's to judge), and every error elsewhere, each at its path.
fn argument_errors(schema: &Value, args: &Value) -> Result<(Vec<String>, Vec<String>), String> {
    let validator = jsonschema::validator_for(schema).map_err(|e| e.to_string())?;
    let mut wired = Vec::new();
    templated(args, "", &mut wired);
    let errors = (validator.iter_errors(args))
        .filter(|e| !wired.iter().any(|path| path == e.instance_path().as_str()))
        .map(|e| format!("args{}: {e}", e.instance_path()))
        .collect();
    Ok((wired, errors))
}

/// Push the JSON Pointer of every string leaf of `value` a template builds (`${{ … }}`).
fn templated(value: &Value, at: &str, wired: &mut Vec<String>) {
    match value {
        Value::String(text) if text.contains("${{") => wired.push(at.to_owned()),
        Value::Array(items) => (items.iter().enumerate())
            .for_each(|(index, item)| templated(item, &format!("{at}/{index}"), wired)),
        Value::Object(members) => members.iter().for_each(|(key, member)| {
            let key = key.replace('~', "~0").replace('/', "~1");
            templated(member, &format!("{at}/{key}"), wired);
        }),
        _ => {}
    }
}

/// One call's entry on `decision.mcp`.
fn record(tool: &str, verdict: &Verdict) -> Value {
    match verdict {
        Verdict::Judged { wired, approved_at } => {
            json!({"tool": tool, "verdict": "judged", "wired": wired, "approved_at": approved_at})
        }
        Verdict::Held(reason) => {
            json!({"tool": tool, "verdict": "held", "reason": reason, "approved_at": null})
        }
        Verdict::Unoffered => json!({"tool": tool, "verdict": "unoffered"}),
    }
}
