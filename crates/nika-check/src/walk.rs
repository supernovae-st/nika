// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Walking the workflow's text surfaces — the traversal half of the hint
//! lane, split out of `hints.rs` under the ADR-023 1,500-LOC ceiling.
//!
//! These functions answer structural questions by VISITING the workflow
//! (which outputs are referenced anywhere · which strings live inside a
//! `for_each` island · what an action's text fields are · whether an arg
//! resolves to a literal). They emit no hints and hold no policy, which
//! is exactly why they belong beside the hint lane rather than inside it:
//! a traversal is reusable, a hint is a judgement.

use std::collections::BTreeSet;

use nika_schema::expression::{bare_task_refs, scan_templates, task_output_paths};
use nika_schema::raw::{RawAction, RawWorkflow};

// The static-value resolver descended to the analysis substrate
// (`static_ref.rs`) with the thinking-seat law (2026-08-25 · the 15k
// wall). Re-exported here so the in-crate lanes keep their historical
// `walk::` / `crate::` call sites — one resolver, every lane, no drift.
pub(crate) use nika_check_analyzer::{bare_static_ref, static_literal_of};

/// Task ids whose output is referenced ANYWHERE (any `tasks.X.output…`
/// chain in any island, or an envelope `outputs:` entry).
pub(crate) fn consumed_outputs(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut consumed = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, _) in task_output_paths(&island.expr) {
                    consumed.insert(target);
                }
            }
        }
    });
    consumed
}

/// `(output name, task id)` for every `outputs:` binding that
/// references a BARE task envelope (`tasks.X` — no field hop). Scoped
/// to `outputs:` deliberately: a bare envelope in a gate or a prompt is
/// legitimate plumbing; bound into the workflow's public contract it is
/// the golden-drift trap.
pub(crate) fn envelope_bound_outputs(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    for (name, decl) in &wf.outputs {
        if let Ok(islands) = scan_templates(&decl.value().value) {
            for island in islands {
                for id in bare_task_refs(&island.expr) {
                    bound.push((name.value.clone(), id));
                }
            }
        }
    }
    bound
}

/// Task ids referenced with a DEEP path (`tasks.X.output.field…`).
pub(crate) fn deeply_referenced(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut deep = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, path) in task_output_paths(&island.expr) {
                    if !path.is_empty() {
                        deep.insert(target);
                    }
                }
            }
        }
    });
    deep
}

/// Visit every expression-bearing text in the workflow (the same surface
/// the dataflow typer walks: verbs · `when:` · `with:` · `for_each` ·
/// `on_finally` · envelope `outputs:`).
fn for_each_island_text(wf: &RawWorkflow, visit: &mut dyn FnMut(&str)) {
    for task in &wf.tasks {
        let t = &task.value;
        for text in crate::flow::action_effect_fields(&t.action) {
            visit(text);
        }
        if let Some(when) = &t.when
            && let Some(expr) = when.value.as_expr()
        {
            visit(expr);
        }
        if let Some(f) = &t.for_each
            && let nika_schema::raw::ForEachValue::Expression(src) = &f.value
        {
            visit(src);
        }
        for (_, v) in &t.with {
            for text in crate::flow::collect_json_strings(&v.value) {
                visit(text);
            }
        }
    }
    for (_, decl) in &wf.outputs {
        visit(&decl.value().value);
    }
}

/// Resolve an invoke arg to a STATIC string when it is a plain literal
/// or a bare authority ref [`static_literal_of`] resolves to a string
/// literal — the shapes a scaffold ships. Anything dynamic (task refs ·
/// concatenations) resolves to `None`: analysis never guesses.
fn static_string_arg(wf: &RawWorkflow, value: &serde_json::Value) -> Option<String> {
    let s = value.as_str()?;
    let trimmed = s.trim();
    if !trimmed.contains("${{") {
        return Some(trimmed.to_owned());
    }
    static_literal_of(wf, trimmed)?.as_str().map(str::to_owned)
}

/// Every `nika:read` whose `path` arg resolves STATICALLY — the pure
/// half of the missing-input lint (V-arc F1 2026-07-09): the analyzer
/// names the (task · path) pairs, the CALLER decides what existence
/// means on its side of the I/O boundary (the CLI checks the local
/// filesystem; a server might check an artifact store).
#[must_use]
pub fn static_read_paths(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        if invoke.tool().map(|t| t.value.as_str()) != Some("nika:read") {
            continue;
        }
        let Some(args) = &invoke.args else { continue };
        let Some(path_val) = args.value.get("path") else {
            continue;
        };
        if let Some(path) = static_string_arg(wf, path_val) {
            out.push((task.value.id.value.clone(), path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn workflow(task: &str) -> Result<RawWorkflow, String> {
        parse(
            &format!("nika: references\ntasks:\n  consumer:\n{task}\n"),
            FileId::new(0),
            ParseMode::Strict,
        )
        .map_err(|errors| format!("{errors:?}"))
    }

    #[test]
    fn output_references_cover_every_action_text_surface() -> Result<(), String> {
        for task in [
            "    exec: { shell: 'echo ${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' }",
            "    exec: { command: ['echo', '${{ tasks.shallow.output }}', '${{ tasks.deep.output.x }}'] }",
            "    exec: { command: ['cat'], stdin: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' }",
            "    exec: { command: ['echo'], env: { X: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' } }",
            "    infer: { prompt: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' }",
            "    infer: { prompt: 'plain', system: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' }",
            "    agent: { prompt: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}', tools: [] }",
            "    agent: { prompt: 'plain', system: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}', tools: [] }",
            "    invoke: { tool: 'nika:read', args: { path: '${{ tasks.shallow.output }}', binary: '${{ tasks.deep.output.x }}' } }",
            "    invoke: { tool: 'mcp:local/tool', args: { nested: [null, 1, true, { text: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' }] } }",
            "    invoke: { workflow: './child.nika.yaml', args: { text: '${{ tasks.shallow.output }} ${{ tasks.deep.output.x }}' } }",
        ] {
            let wf = workflow(task)?;
            assert_eq!(
                consumed_outputs(&wf),
                BTreeSet::from(["shallow".to_owned(), "deep".to_owned()]),
                "{task}"
            );
            assert_eq!(
                deeply_referenced(&wf),
                BTreeSet::from(["deep".to_owned()]),
                "{task}"
            );
        }
        Ok(())
    }

    #[test]
    fn output_references_keep_workflow_surfaces_and_ignore_nonvalues() -> Result<(), String> {
        let wf = workflow(
            "    invoke: { tool: 'nika:log' }\n\
             \x20   with:\n\
             \x20     nested: [null, false, 42, { text: '${{ tasks.nested.output.value }}' }]\n\
             \x20     '${{ tasks.key.output }}': '${{ tasks.envelope }}'\n\
             \x20   when: '${{ tasks.gate.output.ok }}'\n\
             \x20   for_each: { items: '${{ tasks.items.output }}' }\n\
             outputs:\n  public: '${{ tasks.public.output }}'\n  again: '${{ tasks.nested.output.value }}'",
        )?;
        assert_eq!(
            consumed_outputs(&wf),
            ["nested", "gate", "items", "public"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );
        assert_eq!(
            deeply_referenced(&wf),
            BTreeSet::from(["nested".to_owned(), "gate".to_owned()])
        );
        Ok(())
    }

    #[test]
    fn output_references_ignore_literals_and_bare_envelopes() -> Result<(), String> {
        let wf = workflow(
            "    invoke: { tool: 'mcp:local/tool', args: { text: 'tasks.literal.output', bare: '${{ tasks.envelope }}', empty: [], scalar: null } }",
        )?;
        assert!(consumed_outputs(&wf).is_empty());
        assert!(deeply_referenced(&wf).is_empty());
        Ok(())
    }
}
