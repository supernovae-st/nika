// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secret-leak reporting — a thin reader over the IFC flow facts.
//!
//! Per spec `04-variables.md` §secrets · the engine masks its OWN output
//! (logs · traces · journal), but it CANNOT follow a `secrets.X` that the
//! author routes into a subprocess or tool which re-emits it into captured
//! output. The detection is the [`flow`](super::flow) IFC engine (ADR-092);
//! this module turns its taint facts into the two reportable findings ·
//!
//! - [`SecretLeak`] — a secret reaches an `exec`/`invoke` EFFECT (directly,
//!   via a `with:` alias, via a `for_each` item, or transitively through a
//!   tainted upstream output). The full taint chain is carried.
//! - [`SecretEgress`] — a secret reaches the workflow `outputs:` (it leaves
//!   the run as the return value · the literal exfiltration case).
//!
//! `infer`/`agent` prompts are NOT sinks — a secret in a prompt is
//! provider-bound by design (ADR-092 carve-out).

use super::flow::FlowFacts;
use crate::raw::{RawAction, RawWorkflow};

/// A secret that escapes the masking boundary into an effect.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SecretLeak {
    /// The task whose effect can re-emit the secret.
    pub task: String,
    /// The originating secret name (`secrets.<name>`).
    pub secret: String,
    /// The effect surface (`exec` · `invoke`).
    pub sink: &'static str,
    /// The full taint chain (`secrets.x → with.t → ...`) for diagnostics.
    pub trace: String,
}

/// A secret that leaves the run as a workflow return value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SecretEgress {
    /// The `outputs:` entry name carrying the secret.
    pub output: String,
    /// The originating secret name.
    pub secret: String,
    /// The full taint chain for diagnostics.
    pub trace: String,
}

/// Report every effect-leak from the precomputed flow facts — main verbs
/// AND `on_finally` cleanup verbs (the trace's `on_finally @ <task>` hop
/// distinguishes the cleanup case).
#[must_use]
pub(super) fn scan_leaks(wf: &RawWorkflow, flow: &FlowFacts) -> Vec<SecretLeak> {
    let mut leaks = Vec::new();
    for (idx, task) in wf.tasks.iter().enumerate() {
        if let Some(trace) = flow.effect_taint(idx) {
            leaks.push(SecretLeak {
                task: task.value.id.value.clone(),
                secret: trace.secret.clone(),
                sink: match task.value.action {
                    RawAction::Exec(_) => "exec",
                    RawAction::Invoke(_) => "invoke",
                    // flow only taints exec/invoke effects (infer/agent are
                    // provider-bound) — this arm is unreachable in practice.
                    RawAction::Infer(_) | RawAction::Agent(_) => "effect",
                },
                trace: trace.render(),
            });
        }
        if let Some((trace, sink)) = flow.finally_effect_taint(idx) {
            leaks.push(SecretLeak {
                task: task.value.id.value.clone(),
                secret: trace.secret.clone(),
                sink,
                trace: trace.render(),
            });
        }
    }
    leaks
}

/// Report every workflow-output egress from the precomputed flow facts.
#[must_use]
pub(super) fn scan_egresses(flow: &FlowFacts) -> Vec<SecretEgress> {
    flow.egresses()
        .into_iter()
        .map(|(output, trace)| SecretEgress {
            output: output.to_owned(),
            secret: trace.secret.clone(),
            trace: trace.render(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::analyze;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn leaks_of(yaml: &str) -> Vec<SecretLeak> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let a = analyze(&wf).expect("analyze");
        let flow = super::super::flow::analyze_flow(&wf, &a.topo_waves);
        scan_leaks(&wf, &flow)
    }

    fn egresses_of(yaml: &str) -> Vec<SecretEgress> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let a = analyze(&wf).expect("analyze");
        let flow = super::super::flow::analyze_flow(&wf, &a.topo_waves);
        scan_egresses(&flow)
    }

    const SECRETS: &str = "\
secrets:
  api_key:
    source: vault
    key: prod/key
";

    #[test]
    fn secret_into_exec_command_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    exec: {{ command: \"curl -H 'Auth: ${{{{ secrets.api_key }}}}' x\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].secret, "api_key");
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_exec_env_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    exec:\n      command: \"printenv\"\n      env:\n        TOKEN: \"${{{{ secrets.api_key }}}}\"\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_invoke_args_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: leak\n{SECRETS}tasks:\n  - id: t\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "invoke");
    }

    #[test]
    fn secret_into_infer_prompt_is_not_a_leak() {
        let yaml = format!(
            "nika: v1\nworkflow: ok\n{SECRETS}tasks:\n  - id: t\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n"
        );
        assert!(leaks_of(&yaml).is_empty(), "provider-bound by design");
    }

    #[test]
    fn no_secrets_declared_no_scan() {
        let yaml =
            "nika: v1\nworkflow: none\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n";
        assert!(leaks_of(yaml).is_empty());
    }

    #[test]
    fn with_aliased_secret_now_leaks_with_a_trace() {
        // The review's false negative — fixed by the IFC engine.
        let yaml = format!(
            "nika: v1\nworkflow: w\n{SECRETS}tasks:\n  - id: t\n    with: {{ tok: \"${{{{ secrets.api_key }}}}\" }}\n    exec: {{ command: \"curl -H ${{{{ with.tok }}}}\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert!(l[0].trace.contains("with.tok"), "trace: {}", l[0].trace);
    }

    #[test]
    fn secret_into_on_finally_cleanup_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow: w\n{SECRETS}tasks:\n  - id: t\n    exec: {{ command: \"echo build\" }}\n    on_finally:\n      - invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1, "the cleanup leak is reported");
        assert_eq!(l[0].sink, "invoke");
        assert!(l[0].trace.contains("on_finally"), "trace: {}", l[0].trace);
    }

    #[test]
    fn secret_egress_into_outputs_is_reported() {
        let yaml = format!(
            "nika: v1\nworkflow: w\n{SECRETS}tasks:\n  - id: a\n    exec: {{ command: \"echo ${{{{ secrets.api_key }}}}\" }}\noutputs:\n  leaked: ${{{{ tasks.a.output }}}}\n"
        );
        let e = egresses_of(&yaml);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].output, "leaked");
        assert_eq!(e[0].secret, "api_key");
    }

    #[test]
    fn literal_prose_mentioning_secret_is_not_a_leak() {
        // No ${{ }} island → no reference → no leak (the prose false positive).
        let yaml = format!(
            "nika: v1\nworkflow: w\n{SECRETS}tasks:\n  - id: t\n    exec: {{ command: \"echo 'set secrets.api_key in vault'\" }}\n"
        );
        assert!(
            leaks_of(&yaml).is_empty(),
            "prose mention is not a reference"
        );
    }
}
