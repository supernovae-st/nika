// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model selections an envelope override cannot replace. This scan reads
//! declarations only: no model resolution, child loading or billing claim.
//! The checker wraps these rows as hints; run projects the admitted hints.

use nika_schema::raw::{RawAction, RawWorkflow};

/// `(task id, advice)` in declaration order for explicit model pins and
/// child invocation sites. A site is not a count of executed descendants.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for task in &wf.tasks {
        let id = &task.value.id.value;
        let pin = match &task.value.action {
            RawAction::Infer(a) => a.model.as_ref(),
            RawAction::Agent(a) => a.model.as_ref(),
            _ => None,
        };
        if let Some(model) = pin {
            rows.push((id.clone(), format!(
                "task `{id}` keeps its model pin `{}` — CLI `--model` is envelope-only; \
                 it does not replace this pin, even with `--model mock/echo`. \
                 The override alone does not guarantee an offline run; review task pins and child workflows",
                model.value
            )));
        } else if let RawAction::Invoke(a) = &task.value.action
            && let Some(child) = a.workflow()
        {
            rows.push((
                id.clone(),
                format!(
                    "task `{id}` invokes child workflow `{}` — CLI `--model` is parent-only; \
                 it does not descend into the child, which keeps its own model selection. \
                 Child calls and other effects may remain real with `--model mock/echo`",
                    child.value
                ),
            ));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::scan;
    use nika_schema::{FileId, ParseMode, parse};

    #[test]
    fn pins_and_child_sites_are_named_without_claiming_their_billing() {
        let wf = parse(
            "nika: scope\nmodel: mock/echo\ntasks:\n\
             \x20 inherited:\n    infer: { prompt: hi, max_tokens: 10 }\n\
             \x20 pinned:\n    infer: { model: mock/echo, prompt: hi, max_tokens: 10 }\n\
             \x20 agent:\n    agent: { model: anthropic/claude-sonnet-5, prompt: hi, max_tokens_total: 10 }\n\
             \x20 child:\n    invoke: { workflow: ./child.nika.yaml }\n\
             \x20 child_again:\n    invoke: { workflow: ./child.nika.yaml }\n\
             \x20 tool:\n    invoke: { tool: nika:log, args: { message: hi } }\n\
             \x20 exec:\n    exec: { command: [echo, hi] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture");
        let rows = scan(&wf);
        assert_eq!(
            rows.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["pinned", "agent", "child", "child_again"]
        );
        assert!(rows[0].1.contains("pin `mock/echo`"));
        assert!(rows[1].1.contains("pin `anthropic/claude-sonnet-5`"));
        assert!(!rows[0].1.contains("metered"));
        assert!(!rows[0].1.contains("keeps its own access path"));
        assert!(!rows[0].1.contains("stays live and metered"));
        assert!(rows[2].1.contains("child workflow `./child.nika.yaml`"));
        assert!(rows[2].1.contains("parent-only"));
    }

    #[test]
    fn a_dynamic_pin_is_described_without_resolving_it() {
        let wf = parse(
            "nika: scope\ninputs:\n  seat: { type: string }\ntasks:\n  a:\n    infer: { model: '${{ inputs.seat }}', prompt: hi, max_tokens: 10 }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture");
        let rows = scan(&wf);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1.contains("pin `${{ inputs.seat }}`"));
        assert!(rows[0].1.contains("does not guarantee an offline run"));
    }
}
