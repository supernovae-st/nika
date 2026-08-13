// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The DATA JOURNEY (P0-18 · audit UX 2026-07-30) — where this workflow's
//! data goes, stated BEFORE a token is spent. A pure projection of the
//! facts `check` already computes (permits-shaped effects · resolved
//! models · the IFC taint facts): the journey names the CLASSES, never
//! the values (law 13 — no secret value, no file content, ever).
//!
//! The closure proof is « aucun sink cloud sensible sans reçu/consentement
//! visible »: a secret reaching a cloud destination is NAMED on the
//! journey (advisory — the blocking refusal for the unsanctioned case
//! already lives in the IFC leak lane; a sanctioned egress stays a flow
//! the operator must SEE, receipt-side).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use nika_schema::expression::NamespaceRef;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};

use super::flow::{FlowFacts, action_effect_fields, collect_json_strings, refs_in_str};
use super::permits_fit::{
    BuiltinEffect, ConstStrings, builtin_effect, chart_vl_sibling, judgeable_arg, static_program,
    url_host,
};
use super::walk::static_literal_of;

/// The journey's data classification — derived, never declared (the
/// author cannot talk their way down a class). Conservative by law:
/// `internal` by default, `sensitive` when declared secrets are used or
/// a PII-shaped path is declared, `regulated` when a secret reaches a
/// cloud destination. Never an over-claim: an unknown shape stays DOWN.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DataClassification {
    /// No declared secret in use, no PII-shaped path.
    #[default]
    Internal,
    /// A declared secret is used, or a declared path names a
    /// personal-data class (customers · patients · …).
    Sensitive,
    /// A secret reaches a cloud destination — the receipt law applies.
    Regulated,
}

impl DataClassification {
    /// The wire/display token.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Sensitive => "sensitive",
            Self::Regulated => "regulated",
        }
    }
}

/// Where one model endpoint executes, statically — the catalog's
/// sovereignty facts (deployment tags · the sourced `zdr`), never a
/// ping. `Unknown` is a stance, not a guess: an unrecognized provider
/// is never promoted to `Cloud` (no over-claim) and never excused to
/// `Local`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum EndpointLocus {
    /// Runs on the operator's machine (local deployment tag · mock).
    #[default]
    Local,
    /// A recognized third-party cloud endpoint — data leaves the machine.
    Cloud,
    /// The provider resolved to nothing the catalog knows.
    Unknown,
}

/// One named endpoint of the journey — a path, a host, or a program,
/// with the tasks that touch it. `target` is a CLASS (the declared
/// path/host/program), never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct JourneyEndpoint {
    /// The family (`fs.read` · `fs.write` · `net.http` · `exec`).
    pub kind: &'static str,
    /// The declared path · host · program.
    pub target: String,
    /// The tasks touching it.
    pub tasks: Vec<String>,
}

/// One model endpoint a task resolves to — the provider's identity and
/// its SOURCED data facts (the catalog's vendored policy). An absent
/// policy stays absent (never a fabricated "safe").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct ModelEndpoint {
    /// The task resolving to this model.
    pub task: String,
    /// The resolved `<provider>/<model>` (task override ?? envelope).
    pub model: String,
    /// The provider id (`openai` · `ollama` · …).
    pub provider: String,
    /// The catalog knows this provider.
    pub recognized: bool,
    /// The catalog carries an output price for the model.
    pub priced: bool,
    /// Where the call executes.
    pub locus: EndpointLocus,
    /// The sourced API-payload retention, as the policy states it.
    pub retention: Option<String>,
    /// The sourced training class (`no` · `opt-out` · `split` · …).
    pub trains: Option<String>,
}

/// One declared secret the workflow USES — the NAME only (law 13),
/// the tasks touching it, the cloud destinations it reaches, and the
/// author's declared clearances (the `egress:` receipts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SecretUse {
    /// The `secrets.<name>` — never the value, never the lookup key.
    pub name: String,
    /// The effect tasks referencing it (directly or through the taint).
    pub tasks: Vec<String>,
    /// The cloud destinations the secret reaches (a provider id · a
    /// public host) — the rows the JOURNEY rung warns on.
    pub flows_to: Vec<String>,
    /// The author's declared clearances (`egress.to:` tokens).
    pub consents: Vec<String>,
}

/// One sourced retention fact of a cloud endpoint in the journey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct RetentionFact {
    /// The provider id.
    pub provider: String,
    /// The retention, as the sourced policy states it.
    pub retention: String,
    /// The training class, as the sourced policy states it.
    pub trains: String,
}

/// One declared clearance — a secret's `egress:` rule, projected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct JourneyConsent {
    /// The secret the clearance belongs to.
    pub secret: String,
    /// The sanctioned sink (`infer` · `nika:fetch` · `exec` · …).
    pub to: String,
    /// The literal host clause, when the rule carries one.
    pub host: Option<String>,
}

/// The whole journey — additive on [`crate::CheckReport`] (the
/// `report_version` stays 1). Serializable: the `--json` surface carries
/// it verbatim, the console renders the JOURNEY rung off it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DataJourney {
    /// The derived classification.
    pub classification: DataClassification,
    /// Where data is READ (declared static `fs.read` effects).
    pub sources: Vec<JourneyEndpoint>,
    /// Where data GOES (`fs.write` · `exec` programs · `net.http` hosts).
    pub destinations: Vec<JourneyEndpoint>,
    /// The model endpoint each infer/agent task resolves to.
    pub model_endpoints: Vec<ModelEndpoint>,
    /// The static write paths declared in the body.
    pub writes: Vec<String>,
    /// The sourced retention facts of the cloud endpoints in play.
    pub trace_retention: Vec<RetentionFact>,
    /// The declared secrets the body uses — names, never values.
    pub secrets_used: Vec<SecretUse>,
    /// The author's declared clearances (the `egress:` receipts).
    pub consents: Vec<JourneyConsent>,
}

/// The PII-shaped path tokens — a CLOSED list, matched on path segments
/// (a segment STARTING with a token: `customers.csv` matches,
/// `custom.rs` does not). Conservative by law: an unmatched shape keeps
/// the `internal` class; the journey never upgrades on a guess.
const PII_PATH_TOKENS: &[&str] = &[
    "pii", "personal", "customer", "patient", "passport", "ssn", "identity", "medical", "gdpr",
    "rgpd",
];

/// Project the journey from the parsed workflow + the IFC facts (the
/// taint the flow engine already computed — the projection never
/// re-derives propagation). Total: a half-broken workflow still states
/// its declared journey (the direct-reference half needs no DAG order).
pub(crate) fn collect(wf: &RawWorkflow, flow: &FlowFacts) -> DataJourney {
    let consts = ConstStrings::of(wf);
    let envelope = wf.model.as_ref().map(|m| m.value.clone());
    let declared: BTreeSet<&str> = wf.secrets.iter().map(|(n, _)| n.value.as_str()).collect();
    // (kind, target) → tasks — the endpoint tables, BTree-ordered.
    let mut endpoints: BTreeMap<(&'static str, String), BTreeSet<String>> = BTreeMap::new();
    let mut model_endpoints = Vec::new();
    // task → the cloud destinations it reaches (provider ids · public hosts).
    let mut task_cloud: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // secret name → the tasks using it.
    let mut secret_tasks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (idx, task) in wf.tasks.iter().enumerate() {
        let t = &task.value;
        let id = t.id.value.clone();
        let mut cloud = BTreeSet::new();
        collect_action_effects(&consts, &id, &t.action, &mut endpoints, &mut cloud);
        if let Some(ep) = model_endpoint_of(wf, t, envelope.as_deref()) {
            if ep.locus == EndpointLocus::Cloud {
                cloud.insert(ep.provider.clone());
            }
            model_endpoints.push(ep);
        }
        if !cloud.is_empty() {
            task_cloud.insert(id.clone(), cloud);
        }
        for name in secret_names_of_task(idx, t, flow, &declared) {
            secret_tasks.entry(name).or_default().insert(id.clone());
        }
    }

    let sources = endpoints_of(&endpoints, "fs.read");
    let destinations: Vec<JourneyEndpoint> = ["fs.write", "net.http", "exec"]
        .into_iter()
        .flat_map(|kind| endpoints_of(&endpoints, kind))
        .collect();
    let writes: Vec<String> = destinations
        .iter()
        .filter(|d| d.kind == "fs.write")
        .map(|d| d.target.clone())
        .collect();
    let secrets_used = secret_uses(wf, secret_tasks, &task_cloud);
    let consents = consents_of(wf);
    let trace_retention = retention_facts(&model_endpoints);
    let classification = classify(&sources, &writes, &secrets_used);
    DataJourney {
        classification,
        sources,
        destinations,
        model_endpoints,
        writes,
        trace_retention,
        secrets_used,
        consents,
    }
}

/// Fold one action's STATIC effect signature into the endpoint tables —
/// the same classification the boundary lanes read ([`builtin_effect`] ·
/// [`static_program`]), so the journey and the permits never disagree on
/// what a task touches. Dynamic effects pin NOTHING here: the escape and
/// inference lanes own that story, a projection never guesses.
fn collect_action_effects(
    consts: &ConstStrings,
    id: &str,
    action: &RawAction,
    endpoints: &mut BTreeMap<(&'static str, String), BTreeSet<String>>,
    cloud: &mut BTreeSet<String>,
) {
    let mut touch = |kind: &'static str, target: String| {
        endpoints
            .entry((kind, target))
            .or_default()
            .insert(id.to_owned());
    };
    match action {
        RawAction::Exec(a) => {
            // argv[0] literal only — a shell string has no single static
            // program (the boundary lanes refuse it by FORM).
            if let Some(p) = static_program(&a.command) {
                touch("exec", p.to_owned());
            }
        }
        RawAction::Invoke(a) => {
            if !matches!(a.target, RawInvokeTarget::Tool(_)) {
                return; // a child workflow's boundary: the composition lane's
            }
            match builtin_effect(a) {
                Some(BuiltinEffect::Net { url_arg }) => {
                    if let Some(host) = judgeable_arg(consts, a, url_arg)
                        .as_deref()
                        .and_then(url_host)
                    {
                        // A floor-blocked literal (loopback · private) is not
                        // a cloud sink — data to it stays off the wire.
                        if !nika_types::net::host_is_blocked(&host) {
                            cloud.insert(host.clone());
                        }
                        touch("net.http", host);
                    }
                }
                Some(BuiltinEffect::Fs {
                    path_arg,
                    reads,
                    writes,
                    recursive,
                }) => {
                    if let Some(path) = judgeable_arg(consts, a, path_arg) {
                        let entry = if recursive {
                            format!("{path}/**")
                        } else {
                            path
                        };
                        if reads {
                            touch("fs.read", entry.clone());
                        }
                        if writes {
                            touch("fs.write", entry);
                        }
                    }
                }
                None => {}
            }
            // The chart vega sibling is a second gated write of the same task.
            if let Some(vl) = chart_vl_sibling(a) {
                touch("fs.write", vl);
            }
        }
        // infer/agent effects are the model endpoint — collected by the caller.
        _ => {}
    }
}

/// The endpoint one task's model resolves to — the catalog's identity +
/// sourced sovereignty facts, statically (never a ping). A templated
/// `model:` resolves only through a declared static literal; anything
/// else yields NO endpoint (the MODELS rung owns the unjudged claim).
fn model_endpoint_of(
    wf: &RawWorkflow,
    task: &RawTask,
    envelope: Option<&str>,
) -> Option<ModelEndpoint> {
    let declared = match &task.action {
        RawAction::Infer(a) => a.model.as_ref().map(|m| m.value.as_str()),
        RawAction::Agent(a) => a.model.as_ref().map(|m| m.value.as_str()),
        _ => None,
    };
    let declared = declared.or(envelope)?;
    let model: String = if declared.contains("${{") {
        static_literal_of(wf, declared)?.as_str()?.to_owned()
    } else {
        declared.to_owned()
    };
    let provider = model
        .split_once('/')
        .map_or(model.as_str(), |(p, _)| p)
        .to_owned();
    let known = nika_catalog::find_provider(&provider);
    let locus = known.map_or(EndpointLocus::Unknown, locus_of);
    let priced = nika_catalog::find_pricing_for(&model).is_some();
    Some(ModelEndpoint {
        task: task.id.value.clone(),
        model,
        provider,
        recognized: known.is_some(),
        priced,
        locus,
        retention: known
            .and_then(|p| p.data_policy)
            .map(|d| d.retention.to_owned()),
        trains: known
            .and_then(|p| p.data_policy)
            .map(|d| d.trains.to_owned()),
    })
}

/// The catalog's sovereignty facts → the execution locus: a local
/// deployment tag, or a sourced `zdr: local` (the mock's structural
/// « never leaves the process »), is LOCAL; any other recognized
/// provider is CLOUD. Unrecognized stays UNKNOWN — never promoted,
/// never excused.
fn locus_of(p: &nika_catalog::Provider) -> EndpointLocus {
    if p.tags.contains(&nika_catalog::Tag::Local) || p.data_policy.is_some_and(|d| d.zdr == "local")
    {
        EndpointLocus::Local
    } else {
        EndpointLocus::Cloud
    }
}

/// Every DECLARED secret name one task touches — the direct references
/// across its effect surface (main verb · `with:` · `for_each` ·
/// cleanups), plus the IFC propagation fact (a with-alias or tainted
/// upstream output) when a valid DAG order computed one. NAMES only —
/// law 13.
fn secret_names_of_task(
    idx: usize,
    task: &RawTask,
    flow: &FlowFacts,
    declared: &BTreeSet<&str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut scan = |text: &str| {
        for r in refs_in_str(text) {
            if let NamespaceRef::Secrets(name) = r
                && declared.contains(name.as_str())
            {
                out.insert(name);
            }
        }
    };
    for text in action_effect_fields(&task.action) {
        scan(text);
    }
    for (_, v) in &task.with {
        for text in collect_json_strings(&v.value) {
            scan(text);
        }
    }
    if let Some(fe) = &task.for_each
        && let nika_schema::raw::ForEachValue::Expression(src) = &fe.value
    {
        scan(src);
    }
    // The propagation facts (valid order only — empty facts, never wrong).
    if let Some(trace) = flow.effect_taint(idx) {
        out.insert(trace.secret.clone());
    }
    out
}

/// The endpoints of one family, folded out of the shared table.
fn endpoints_of(
    endpoints: &BTreeMap<(&'static str, String), BTreeSet<String>>,
    kind: &'static str,
) -> Vec<JourneyEndpoint> {
    endpoints
        .iter()
        .filter(|((k, _), _)| *k == kind)
        .map(|((k, target), tasks)| JourneyEndpoint {
            kind: k,
            target: target.clone(),
            tasks: tasks.iter().cloned().collect(),
        })
        .collect()
}

/// The used secrets with their cloud flows — a secret's `flows_to` is
/// the union of the cloud destinations of every task touching it.
fn secret_uses(
    wf: &RawWorkflow,
    secret_tasks: BTreeMap<String, BTreeSet<String>>,
    task_cloud: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<SecretUse> {
    secret_tasks
        .into_iter()
        .map(|(name, tasks)| {
            let flows_to: BTreeSet<String> = tasks
                .iter()
                .filter_map(|t| task_cloud.get(t))
                .flatten()
                .cloned()
                .collect();
            let consents = wf
                .secrets
                .iter()
                .find(|(n, _)| n.value == name)
                .map(|(_, decl)| decl.value.egress.iter().map(|r| r.to.clone()).collect())
                .unwrap_or_default();
            SecretUse {
                name,
                tasks: tasks.into_iter().collect(),
                flows_to: flows_to.into_iter().collect(),
                consents,
            }
        })
        .collect()
}

/// The declared clearances, projected — one row per `egress:` rule.
fn consents_of(wf: &RawWorkflow) -> Vec<JourneyConsent> {
    let mut out = Vec::new();
    for (name, decl) in &wf.secrets {
        for rule in &decl.value.egress {
            out.push(JourneyConsent {
                secret: name.value.clone(),
                to: rule.to.clone(),
                host: rule.host.clone(),
            });
        }
    }
    out
}

/// The sourced retention facts of the CLOUD endpoints in play — one row
/// per provider, deduped. An absent sourced policy contributes NOTHING
/// (absence of the fact, never a fabricated fact).
fn retention_facts(endpoints: &[ModelEndpoint]) -> Vec<RetentionFact> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for ep in endpoints {
        if ep.locus != EndpointLocus::Cloud || !seen.insert(ep.provider.clone()) {
            continue;
        }
        if let (Some(retention), Some(trains)) = (&ep.retention, &ep.trains) {
            out.push(RetentionFact {
                provider: ep.provider.clone(),
                retention: retention.clone(),
                trains: trains.clone(),
            });
        }
    }
    out
}

/// The derived class — conservative: `regulated` needs a secret reaching
/// a cloud sink, `sensitive` needs a used secret or a PII-shaped
/// declared path, everything else stays `internal`.
fn classify(
    sources: &[JourneyEndpoint],
    writes: &[String],
    secrets_used: &[SecretUse],
) -> DataClassification {
    if secrets_used.iter().any(|s| !s.flows_to.is_empty()) {
        return DataClassification::Regulated;
    }
    if !secrets_used.is_empty()
        || sources.iter().any(|s| pii_shaped(&s.target))
        || writes.iter().any(|w| pii_shaped(w))
    {
        return DataClassification::Sensitive;
    }
    DataClassification::Internal
}

/// A declared path naming a personal-data class (segment-prefix match
/// over the closed [`PII_PATH_TOKENS`] list).
fn pii_shaped(path: &str) -> bool {
    path.split(|c: char| !c.is_ascii_alphanumeric()).any(|seg| {
        let seg = seg.to_ascii_lowercase();
        PII_PATH_TOKENS.iter().any(|t| seg.starts_with(t))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn journey_of(yaml: &str) -> DataJourney {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        crate::check(&wf).data_journey
    }

    /// The flagship shape: a read + a fetch + a secret-bearing prompt to a
    /// cloud model + a write. The journey names the source, the
    /// destinations, the endpoint, and the secret — by NAME.
    #[test]
    fn the_journey_names_source_destination_endpoint_and_secret() {
        let j = journey_of(
            r#"
nika: journey-probe
model: openai/gpt-4o-mini
secrets:
  openai_key:
    source: env
    key: OPENAI_API_KEY
    egress: [{ to: infer }]
permits:
  fs: { read: ["data/input.csv"], write: ["out/summary.md"] }
  net: { http: ["api.example.com"] }
tasks:
  read_in:
    invoke: { tool: "nika:read", args: { path: "data/input.csv" } }
  fetch_ctx:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.com/ctx" } }
  summarize:
    after: { read_in: success, fetch_ctx: success }
    infer:
      prompt: "Summarize ${{ tasks.read_in.output }} · auth ${{ secrets.openai_key }}"
      max_tokens: 100
  save:
    after: { summarize: success }
    invoke:
      tool: "nika:write"
      args: { path: "out/summary.md", content: "${{ tasks.summarize.output }}" }
"#,
        );
        // The source: the declared static read, attributed to its task.
        assert!(
            j.sources.iter().any(|s| s.kind == "fs.read"
                && s.target == "data/input.csv"
                && s.tasks == ["read_in"]),
            "the read is a named source: {:?}",
            j.sources
        );
        // The destinations: the write path and the fetched host.
        assert!(
            j.destinations
                .iter()
                .any(|d| d.kind == "fs.write" && d.target == "out/summary.md"),
            "the write is a named destination: {:?}",
            j.destinations
        );
        assert!(
            j.destinations.iter().any(|d| d.kind == "net.http"
                && d.target == "api.example.com"
                && d.tasks == ["fetch_ctx"]),
            "the host is a named destination: {:?}",
            j.destinations
        );
        // The writes list carries the static write path.
        assert_eq!(j.writes, ["out/summary.md"], "{:?}", j.writes);
        // The endpoint: recognized, priced, CLOUD.
        let ep = j
            .model_endpoints
            .iter()
            .find(|e| e.task == "summarize")
            .expect("the infer task's endpoint");
        assert_eq!(ep.provider, "openai");
        assert!(ep.recognized, "openai is a catalog provider");
        assert!(ep.priced, "gpt-4o-mini is catalog-priced");
        assert_eq!(ep.locus, EndpointLocus::Cloud);
        // The secret: named, attributed, its cloud flow visible — the
        // VALUE never appears (the canary test owns that proof).
        let s = j
            .secrets_used
            .iter()
            .find(|s| s.name == "openai_key")
            .expect("the used secret is named");
        assert_eq!(s.tasks, ["summarize"], "{s:?}");
        assert!(
            s.flows_to.iter().any(|d| d == "openai"),
            "the secret flows to the provider endpoint: {s:?}"
        );
        // The author's clearance is on the receipt.
        assert!(
            j.consents
                .iter()
                .any(|c| c.secret == "openai_key" && c.to == "infer"),
            "the declared egress is a consent: {:?}",
            j.consents
        );
        // A secret reaching a cloud sink: regulated.
        assert_eq!(j.classification, DataClassification::Regulated);
    }

    /// A pure mock-compute workflow: the honest trivial journey — nothing
    /// leaves the machine, and the journey SAYS the trivial truth instead
    /// of dressing it up.
    #[test]
    fn a_pure_mock_workflow_states_the_trivial_journey() {
        let j = journey_of(
            "nika: mock-pure\nmodel: mock/echo\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        assert_eq!(j.classification, DataClassification::Internal);
        assert!(j.sources.is_empty(), "{:?}", j.sources);
        assert!(j.destinations.is_empty(), "{:?}", j.destinations);
        assert!(j.writes.is_empty(), "{:?}", j.writes);
        assert!(j.secrets_used.is_empty(), "{:?}", j.secrets_used);
        let ep = j
            .model_endpoints
            .iter()
            .find(|e| e.task == "think")
            .expect("the mock endpoint is still named");
        assert_eq!(ep.locus, EndpointLocus::Local, "mock never egresses");
        assert!(
            j.trace_retention.is_empty(),
            "no cloud endpoint → no retention fact: {:?}",
            j.trace_retention
        );
    }

    /// Law 13: the journey names CLASSES, never values. A canary value
    /// planted in the prompt and in the secret's lookup key must NOT
    /// survive serialization — only the secret's NAME may.
    #[test]
    fn no_secret_value_ever_leaks_into_the_journey() {
        let yaml = r#"
nika: canary
model: openai/gpt-4o-mini
secrets:
  canary_named_secret:
    source: env
    key: CANARY_LOOKUP_KEY_NEVER_SHOWN
    egress: [{ to: infer }]
permits: {}
tasks:
  send:
    infer:
      prompt: "use sk-canary-VALUE-never-shown via ${{ secrets.canary_named_secret }}"
      max_tokens: 10
"#;
        let j = journey_of(yaml);
        let json = serde_json::to_string(&j).expect("the journey serializes");
        assert!(json.contains("canary_named_secret"), "the NAME rides");
        assert!(
            !json.contains("sk-canary-VALUE-never-shown"),
            "the prompt VALUE never rides: {json}"
        );
        assert!(
            !json.contains("CANARY_LOOKUP_KEY_NEVER_SHOWN"),
            "the lookup key never rides: {json}"
        );
    }

    /// A PII-shaped declared path upgrades the class to sensitive — no
    /// secret needed (the data itself is the sensitive class).
    #[test]
    fn a_pii_shaped_path_marks_the_journey_sensitive() {
        let j = journey_of(
            "nika: pii\nmodel: mock/echo\npermits:\n  fs: { read: [\"data/customers.csv\"] }\ntasks:\n  load:\n    invoke: { tool: \"nika:read\", args: { path: \"data/customers.csv\" } }\n",
        );
        assert_eq!(
            j.classification,
            DataClassification::Sensitive,
            "customers.csv is a personal-data class"
        );
    }

    /// The test above proves ONE token of ten. Every declared token must
    /// mark a path, or the list is decoration for the nine nobody exercises.
    #[test]
    fn every_declared_pii_token_marks_a_path() {
        for token in PII_PATH_TOKENS {
            assert!(
                pii_shaped(&format!("data/{token}-export.csv")),
                "the declared token {token:?} classifies nothing"
            );
            // Segment-prefix, per the doc: a token INSIDE a word is not a match.
            assert!(
                !pii_shaped(&format!("data/x{token}.csv")),
                "{token:?} matched mid-segment · the rule says segment-prefix"
            );
        }
        // Conservative by law: an unmatched shape stays internal.
        assert!(!pii_shaped("data/build-cache.json"));
        assert!(!pii_shaped("custom.rs"), "custom != customer");
    }

    /// Iterating proves what is there; only a named floor catches a removal.
    #[test]
    fn the_pii_floor_never_loses_a_token() {
        const FLOOR: &[&str] = &[
            "pii", "personal", "customer", "patient", "passport", "ssn", "identity", "medical",
            "gdpr", "rgpd",
        ];
        for token in FLOOR {
            assert!(
                PII_PATH_TOKENS.contains(token),
                "{token:?} left the list · paths naming it stop reading as sensitive"
            );
        }
    }

    /// The wire shape: lowercase classification + locus, the field riding
    /// the report's `--json` surface as `data_journey` (additive —
    /// `report_version` untouched).
    #[test]
    fn the_report_serializes_the_journey_on_the_wire() {
        let wf = parse(
            "nika: wire\nmodel: openai/gpt-4o-mini\npermits: {}\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 5 }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parses");
        let report = crate::check(&wf);
        let json = serde_json::to_value(&report).expect("the report serializes");
        let dj = &json["data_journey"];
        assert_eq!(dj["classification"], "internal");
        assert_eq!(dj["model_endpoints"][0]["locus"], "cloud");
        assert_eq!(dj["model_endpoints"][0]["provider"], "openai");
        assert_eq!(json["report_version"], 1, "additive — no version bump");
    }
}
