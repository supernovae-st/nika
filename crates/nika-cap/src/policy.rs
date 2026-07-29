// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `policy:` block — named workflow law as pure data + the pure judge.
//!
//! Per spec `10-authority.md` · `permits:` bounds capability; `policy:`
//! bounds ORDER and SHAPE (« no shell after an untrusted fetch » · « a
//! human signs before anything irreversible » · « only these providers »).
//! Six families, closed at every level BY THE TYPE (serde
//! `deny_unknown_fields` — an unknown family, rule or value never
//! parses): four hard (`require` · `forbid` · `allow` · `limits` ·
//! judged at check) and two soft (`prefer` · `optimize` · recorded,
//! never judged in v1 — a constraint that cannot be judged must never
//! look judged).
//!
//! The judge ([`policy_violations`]) is pure L0: it reads projected
//! [`PolicySubject`] rows (id · verb · tool · provider pin · direct
//! parents from the ONE derived graph — the caller projects them, this
//! crate never re-walks an AST). Semantics are a rule-for-rule mirror of
//! the spec reference evaluator `conformance/deep_static.py::policy_errors`
//! (proven on the `conformance/tests/core/policy/**` fixtures).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Permits;

/// The closed `policy:` family keys (spec 10 §grammar) — the completion
/// door's vocabulary; serde `deny_unknown_fields` is its enforcing twin
/// (pinned coherent by test, the two can never drift).
pub const POLICY_KEYS: &[&str] = &["require", "forbid", "allow", "limits", "prefer", "optimize"];

/// The human gate: an `invoke:` of this tool (spec 10 · « the pause IS
/// the consent » · exit 4 · resume with the answer).
pub const HUMAN_GATE_TOOL: &str = "nika:prompt";

/// The closed rule keys under one policy family (`None` when `family`
/// names no policy family) — the completion door's second level.
#[must_use]
pub fn policy_child_keys(family: &str) -> Option<&'static [&'static str]> {
    match family {
        "require" => Some(&["human_gate_before"]),
        "forbid" => Some(&["exec_after"]),
        "allow" | "prefer" => Some(&["providers"]),
        "limits" => Some(&["max_tasks"]),
        _ => None,
    }
}

/// The `policy:` block (spec `10-authority.md`) — absent families bind
/// nothing; `policy: {}` is a workflow under no named law.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Policy {
    /// `require:` — hard · judged at check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require: Option<Require>,
    /// `forbid:` — hard · judged at check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbid: Option<Forbid>,
    /// `allow:` — hard · judged at check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<Allow>,
    /// `limits:` — hard · judged at check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    /// `prefer:` — SOFT · recorded, never judged (v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer: Option<Prefer>,
    /// `optimize:` — SOFT · recorded, never judged (v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimize: Option<Objective>,
}

impl Policy {
    /// An empty policy — no law bound.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse from a neutral JSON value — the refusal TEACHES the closed
    /// vocabulary (families · rules · effect classes) so the author never
    /// hunts the spec for the accepted set.
    ///
    /// # Errors
    ///
    /// A human-readable teaching string; the caller wraps it in its own
    /// error type (nika-schema files it as a `NIKA-PARSE`-class refusal).
    pub fn from_value(value: serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value).map_err(|e| {
            format!(
                "`policy:` is refused — {e} · the rule set is closed per minor \
                 (spec 10 §policy): families {} · rules require.human_gate_before \
                 [exec·write·net·tools] · forbid.exec_after [exec·write·net·tools] · \
                 allow.providers · limits.max_tasks (≥ 1) · prefer.providers · \
                 optimize (cost·latency·quality)",
                POLICY_KEYS.join(" · ")
            )
        })
    }

    /// Whether a SOFT family is present (`prefer:` / `optimize:`) — the
    /// hint surface reads this (recorded-not-judged · spec 10).
    #[must_use]
    pub fn has_soft_families(&self) -> bool {
        self.prefer.is_some() || self.optimize.is_some()
    }
}

/// `require:` — every task carrying a listed effect class sits behind a
/// human gate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Require {
    /// `human_gate_before: [<effect-class>…]` — an ancestor `invoke:` of
    /// [`HUMAN_GATE_TOOL`] is the consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_gate_before: Option<Vec<EffectClass>>,
}

/// `forbid:` — order law over the derived graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Forbid {
    /// `exec_after: [<effect-class>…]` — no `exec:` task descends from a
    /// task carrying a listed class (any path counts, `after:` included).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec_after: Option<Vec<EffectClass>>,
}

/// `allow:` — the one authority `permits:` does not cover (providers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Allow {
    /// `providers: [<provider>…]` — every `infer:`/`agent:` provider (the
    /// `model:` segment before `/`) must be listed · fail-closed on a
    /// templated or absent model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
}

/// `limits:` — workflow-shape bounds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Limits {
    /// `max_tasks: N` — the workflow declares at most N tasks (≥ 1 ·
    /// validated at parse: a zero-task ceiling forbids the workflow itself).
    #[serde(
        default,
        deserialize_with = "de_max_tasks",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tasks: Option<u32>,
}

/// `prefer:` — SOFT provider preference (ordered) · recorded, never judged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Prefer {
    /// `providers: [<provider>…]` — preference order, inert by design (v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
}

/// `max_tasks` ≥ 1, enforced AT the type (spec 10 · « positive integer »).
fn de_max_tasks<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<u32>::deserialize(deserializer)? {
        Some(0) => Err(serde::de::Error::custom(
            "`limits.max_tasks` must be ≥ 1 — a zero-task ceiling forbids the workflow itself",
        )),
        v => Ok(v),
    }
}

/// One `<effect-class>` (spec 10 · the closed set `exec · write · net ·
/// tools` — the effect vocabulary with `fs` split at its grain of harm:
/// `write`; reads are not gateable in v1). `lowercase` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum EffectClass {
    /// The `exec:` verb.
    Exec,
    /// A file-writing builtin (`nika:write` · `nika:edit`).
    Write,
    /// A URL-reaching builtin (`nika:fetch` · `nika:notify`).
    Net,
    /// The whole `invoke:` surface.
    Tools,
}

impl EffectClass {
    /// The wire/witness name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Write => "write",
            Self::Net => "net",
            Self::Tools => "tools",
        }
    }

    /// The COARSE effect classes ONE task carries (spec 10) — the policy
    /// projection of the builtin classification, mirroring the reference
    /// evaluator's `_task_effect_classes` exactly: `exec` = the `exec:`
    /// verb · `tools` = every `invoke:` · `net` = `nika:fetch`/`nika:notify`
    /// · `write` = `nika:write`/`nika:edit`. The fine-grained boundary
    /// table (`builtin_effect` in nika-schema) answers a DIFFERENT
    /// question (which arg carries the target); the two are pinned
    /// coherent by test, never derived from each other.
    #[must_use]
    pub fn classify(verb: &str, tool: Option<&str>) -> BTreeSet<Self> {
        let mut out = BTreeSet::new();
        if verb == "exec" {
            out.insert(Self::Exec);
        }
        if let Some(tool) = tool {
            out.insert(Self::Tools);
            if matches!(tool, "nika:fetch" | "nika:notify") {
                out.insert(Self::Net);
            }
            if matches!(tool, "nika:write" | "nika:edit") {
                out.insert(Self::Write);
            }
        }
        out
    }
}

/// The SOFT `optimize:` objective (spec 10 · recorded, never judged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Objective {
    /// Minimize spend.
    Cost,
    /// Minimize wall-clock.
    Latency,
    /// Maximize output quality.
    Quality,
}

impl Objective {
    /// The wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cost => "cost",
            Self::Latency => "latency",
            Self::Quality => "quality",
        }
    }
}

/// One hard-rule violation (`NIKA-POLICY-001` on the wire — the caller
/// stamps the code; the diagnostic here names rule + task + witness).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct PolicyViolation {
    /// The violated rule (`require.human_gate_before` · `forbid.exec_after`
    /// · `allow.providers` · `limits.max_tasks`).
    pub rule: &'static str,
    /// The offending task (`None` for workflow-level rules).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The human diagnostic — order rules carry the path (« the path is
    /// the witness »), gate rules the missing ancestor, provider rules
    /// the offending literal.
    pub detail: String,
}

/// ONE task, projected to exactly what the policy judge reads.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PolicySubject {
    /// The task id.
    pub id: String,
    /// The verb key (`infer` · `exec` · `invoke` · `agent`).
    pub verb: String,
    /// The `invoke:` tool id (`None` for the other verbs).
    pub tool: Option<String>,
    /// The provider pin (`infer:`/`agent:` only).
    pub provider: ProviderPin,
    /// DIRECT upstream subject indices (`E_d ∪ E_c` — the one derived
    /// graph); the transitive closure is computed here.
    pub parents: Vec<usize>,
}

impl PolicySubject {
    /// Project one task (parents attach after construction).
    #[must_use]
    pub fn new(id: String, verb: &str, tool: Option<String>, provider: ProviderPin) -> Self {
        Self {
            id,
            verb: verb.to_owned(),
            tool,
            provider,
            parents: Vec::new(),
        }
    }
}

/// How a task's `model:` resolves statically (spec 10 §allow.providers).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderPin {
    /// Not an `infer:`/`agent:` task — the provider rule never reads it.
    #[default]
    NotApplicable,
    /// A literal model string (task-level, or the workflow default).
    Literal(String),
    /// Templated or absent everywhere — fail-closed under a declared
    /// allowlist (« pin the literal »).
    Undeterminable,
}

/// Judge the hard families over the projected tasks — a rule-for-rule
/// mirror of `deep_static.py::policy_errors` (require → forbid → allow →
/// limits, task order within each). Soft families are never read here.
#[must_use]
pub fn policy_violations(policy: &Policy, tasks: &[PolicySubject]) -> Vec<PolicyViolation> {
    let classes: Vec<BTreeSet<EffectClass>> = tasks
        .iter()
        .map(|t| EffectClass::classify(&t.verb, t.tool.as_deref()))
        .collect();
    let mut out = Vec::new();
    require_human_gate(policy, tasks, &classes, &mut out);
    forbid_exec_after(policy, tasks, &classes, &mut out);
    allow_providers(policy, tasks, &mut out);
    limits_max_tasks(policy, tasks.len(), &mut out);
    out
}

/// Transitive upstream closure of `start` — cycle-safe and bounds-safe
/// (the caller's graph is the analyzer's acyclic derivation, but pure
/// code stays total on any input).
fn ancestors(tasks: &[PolicySubject], start: usize) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<usize> = tasks
        .get(start)
        .map(|t| t.parents.clone())
        .unwrap_or_default();
    while let Some(a) = stack.pop() {
        if let Some(t) = tasks.get(a)
            && seen.insert(a)
        {
            stack.extend(t.parents.iter().copied());
        }
    }
    seen
}

/// Alphabetical class names — the reference evaluator sorts its witness
/// lists (determinism down to the message).
fn sorted_class_names<'c>(classes: impl Iterator<Item = &'c EffectClass>) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = classes.map(|c| c.as_str()).collect();
    names.sort_unstable();
    names
}

/// `require.human_gate_before: [C…]` — every task carrying a listed
/// class has an ancestor [`HUMAN_GATE_TOOL`] invoke.
fn require_human_gate(
    policy: &Policy,
    tasks: &[PolicySubject],
    classes: &[BTreeSet<EffectClass>],
    out: &mut Vec<PolicyViolation>,
) {
    let Some(gated) = policy
        .require
        .as_ref()
        .and_then(|r| r.human_gate_before.as_ref())
    else {
        return;
    };
    let gated: BTreeSet<EffectClass> = gated.iter().copied().collect();
    let gate_ids: BTreeSet<usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tool.as_deref() == Some(HUMAN_GATE_TOOL))
        .map(|(i, _)| i)
        .collect();
    for (i, t) in tasks.iter().enumerate() {
        let hit = sorted_class_names(classes[i].intersection(&gated));
        if !hit.is_empty() && ancestors(tasks, i).intersection(&gate_ids).next().is_none() {
            out.push(PolicyViolation {
                rule: "require.human_gate_before",
                task: Some(t.id.clone()),
                detail: format!(
                    "task '{}' · require.human_gate_before: [{}] — no {HUMAN_GATE_TOOL} \
                     ancestor (the pause IS the consent · 10 §policy)",
                    t.id,
                    hit.join(" · ")
                ),
            });
        }
    }
}

/// `forbid.exec_after: [C…]` — no `exec:` task descends from a task
/// carrying a listed class; the path is the witness.
fn forbid_exec_after(
    policy: &Policy,
    tasks: &[PolicySubject],
    classes: &[BTreeSet<EffectClass>],
    out: &mut Vec<PolicyViolation>,
) {
    let Some(wanted) = policy.forbid.as_ref().and_then(|f| f.exec_after.as_ref()) else {
        return;
    };
    let wanted: BTreeSet<EffectClass> = wanted.iter().copied().collect();
    let wanted_names = sorted_class_names(wanted.iter());
    for (i, t) in tasks.iter().enumerate() {
        if !classes[i].contains(&EffectClass::Exec) {
            continue;
        }
        let mut tainted: Vec<&str> = ancestors(tasks, i)
            .iter()
            .filter(|&&a| !classes[a].is_disjoint(&wanted))
            .map(|&a| tasks[a].id.as_str())
            .collect();
        if tainted.is_empty() {
            continue;
        }
        tainted.sort_unstable();
        out.push(PolicyViolation {
            rule: "forbid.exec_after",
            task: Some(t.id.clone()),
            detail: format!(
                "task '{}' · forbid.exec_after: [{}] — the path is the witness: {} → {} \
                 (order law · 10 §policy)",
                t.id,
                wanted_names.join(" · "),
                tainted.join(" → "),
                t.id
            ),
        });
    }
}

/// `allow.providers: [P…]` — every `infer:`/`agent:` provider is listed;
/// a provider that cannot be determined statically is a violation
/// (fail-closed · « pin the literal »).
fn allow_providers(policy: &Policy, tasks: &[PolicySubject], out: &mut Vec<PolicyViolation>) {
    let Some(providers) = policy.allow.as_ref().and_then(|a| a.providers.as_ref()) else {
        return;
    };
    for t in tasks {
        match &t.provider {
            ProviderPin::NotApplicable => {}
            ProviderPin::Undeterminable => out.push(PolicyViolation {
                rule: "allow.providers",
                task: Some(t.id.clone()),
                detail: format!(
                    "task '{}' · allow.providers — the provider is not statically \
                     determinable (templated or absent model:) · fail-closed: pin the \
                     literal (10 §policy)",
                    t.id
                ),
            }),
            ProviderPin::Literal(model) => {
                let provider = model.split('/').next().unwrap_or_default();
                if !providers.iter().any(|p| p == provider) {
                    out.push(PolicyViolation {
                        rule: "allow.providers",
                        task: Some(t.id.clone()),
                        detail: format!(
                            "task '{}' · allow.providers — '{provider}' is not in [{}] \
                             (10 §policy)",
                            t.id,
                            providers.join(" · ")
                        ),
                    });
                }
            }
        }
    }
}

/// `limits.max_tasks: N` — the workflow declares at most N tasks.
fn limits_max_tasks(policy: &Policy, task_count: usize, out: &mut Vec<PolicyViolation>) {
    let Some(max) = policy.limits.as_ref().and_then(|l| l.max_tasks) else {
        return;
    };
    if u64::try_from(task_count).unwrap_or(u64::MAX) > u64::from(max) {
        out.push(PolicyViolation {
            rule: "limits.max_tasks",
            task: None,
            detail: format!(
                "limits.max_tasks: {max} — the workflow declares {task_count} tasks (10 §policy)"
            ),
        });
    }
}

/// The batch rule's discriminating classes (NEP-0013): the harm triad
/// `exec · write · net` — `tools` is the whole invoke surface, present
/// on every invoke, never discriminative.
const BATCH_CLASSES: [EffectClass; 3] = [EffectClass::Exec, EffectClass::Write, EffectClass::Net];

/// F-P4 (NEP-0013 law 3) — the HETEROGENEOUS BATCH: ONE prompt whose yes
/// unleashes actions of TWO OR MORE effect classes is the
/// consent-fatigue machine (one question, many consequences). Judged
/// where the gate law itself is declared (`require.human_gate_before` —
/// the lane that already reads the gate's ancestry): each prompt's
/// descendant closure must carry AT MOST ONE class among
/// `exec · write · net` (`tools` is the whole invoke surface — present
/// on every invoke, never discriminative). The walk never traverses
/// THROUGH another prompt: the nearest gate owns what it re-asks for.
/// Homogeneous batches (same class ×N) stay legal — the runtime dedups
/// identical content to one ticket. The wire code is `NIKA-SEC-010`
/// (the findings fold maps the `approval.*` rules there).
#[must_use]
pub fn approval_batch_violations(policy: &Policy, tasks: &[PolicySubject]) -> Vec<PolicyViolation> {
    if policy
        .require
        .as_ref()
        .and_then(|r| r.human_gate_before.as_ref())
        .is_none()
    {
        return Vec::new();
    }
    let classes: Vec<BTreeSet<EffectClass>> = tasks
        .iter()
        .map(|t| EffectClass::classify(&t.verb, t.tool.as_deref()))
        .collect();
    // children[p] = the direct downstreams of p (the parents map reversed).
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];
    for (i, t) in tasks.iter().enumerate() {
        for &p in &t.parents {
            if let Some(list) = children.get_mut(p) {
                list.push(i);
            }
        }
    }
    let mut out = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        if t.tool.as_deref() != Some(HUMAN_GATE_TOOL) {
            continue;
        }
        let mut covered: BTreeSet<EffectClass> = BTreeSet::new();
        let mut witness: Vec<&str> = Vec::new();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut queue: Vec<usize> = vec![i];
        while let Some(at) = queue.pop() {
            for &next in children.get(at).into_iter().flatten() {
                if !seen.insert(next) {
                    continue;
                }
                let Some(subject) = tasks.get(next) else {
                    continue;
                };
                if subject.tool.as_deref() == Some(HUMAN_GATE_TOOL) {
                    continue; // another gate — it owns its own closure
                }
                let unleashed: BTreeSet<EffectClass> = classes
                    .get(next)
                    .map(|c| {
                        c.intersection(&BTreeSet::from(BATCH_CLASSES))
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                if !unleashed.is_empty() {
                    covered.extend(unleashed);
                    witness.push(subject.id.as_str());
                }
                queue.push(next);
            }
        }
        if covered.len() >= 2 {
            let names = sorted_class_names(covered.iter());
            witness.sort_unstable();
            out.push(PolicyViolation {
                rule: "approval.heterogeneous_batch",
                task: Some(t.id.clone()),
                detail: format!(
                    "task '{}' · approval.heterogeneous_batch — one prompt gates ONE effect \
                     class: this yes unleashes [{}] (tasks: {}) · split the gate per class \
                     (NEP-0013 law 3 · the anti-fatigue law · NIKA-SEC-010)",
                    t.id,
                    names.join(" · "),
                    witness.join(" · ")
                ),
            });
        }
    }
    out
}

/// The certificate's AUTHORITY projection (spec 10 §the certificate
/// names its effects) — a projection, never a judge: the check ladder
/// stays the one truth, this field exists so a certificate consumer
/// never re-derives the boundary story.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CertEffects {
    /// Whether the file declares a `permits:` block.
    pub boundary_declared: bool,
    /// The inferred TIGHTEST boundary the body statically needs — the
    /// same object `nika check --infer-permits` prints.
    pub needed: Permits,
    /// Count of required-outside-permitted violations (0 in any clean
    /// report).
    pub escapes: usize,
}

impl CertEffects {
    /// Assemble the projection (invariant #19 — constructor on
    /// `#[non_exhaustive]`).
    #[must_use]
    pub fn new(boundary_declared: bool, needed: Permits, escapes: usize) -> Self {
        Self {
            boundary_declared,
            needed,
            escapes,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    fn subject(id: &str, verb: &str, tool: Option<&str>) -> PolicySubject {
        PolicySubject::new(id.to_owned(), verb, tool.map(str::to_owned), {
            ProviderPin::NotApplicable
        })
    }

    // ── the closed set at the type level ────────────────────────────

    #[test]
    fn full_policy_block_parses() {
        let p = Policy::from_value(json!({
            "require": { "human_gate_before": ["exec", "write"] },
            "forbid":  { "exec_after": ["net"] },
            "allow":   { "providers": ["ollama", "mistral"] },
            "limits":  { "max_tasks": 50 },
            "prefer":  { "providers": ["ollama"] },
            "optimize": "cost",
        }))
        .expect("the spec's own example parses");
        assert_eq!(
            p.require.unwrap().human_gate_before,
            Some(vec![EffectClass::Exec, EffectClass::Write])
        );
        assert_eq!(p.forbid.unwrap().exec_after, Some(vec![EffectClass::Net]));
        assert_eq!(p.limits.unwrap().max_tasks, Some(50));
        assert_eq!(p.optimize, Some(Objective::Cost));
    }

    #[test]
    fn unknown_family_rule_and_value_are_refused_with_teaching() {
        // family (fixture 009's sibling)
        let e = Policy::from_value(json!({ "deny": {} })).expect_err("unknown family");
        assert!(e.contains("deny") && e.contains("require"), "{e}");
        // rule (fixture 009 exactly: write_after is not a v1 rule)
        let e = Policy::from_value(json!({ "forbid": { "write_after": ["net"] } }))
            .expect_err("unknown rule");
        assert!(e.contains("write_after") && e.contains("exec_after"), "{e}");
        // effect-class value outside the closed set
        let e = Policy::from_value(json!({ "forbid": { "exec_after": ["read"] } }))
            .expect_err("unknown class");
        assert!(e.contains("closed per minor"), "{e}");
        // objective outside the closed set
        let e = Policy::from_value(json!({ "optimize": "speed" })).expect_err("unknown objective");
        assert!(e.contains("cost·latency·quality"), "{e}");
    }

    #[test]
    fn max_tasks_zero_is_refused_at_parse() {
        let e = Policy::from_value(json!({ "limits": { "max_tasks": 0 } })).expect_err("zero");
        assert!(e.contains("must be ≥ 1"), "{e}");
        // and a negative never fits u32
        assert!(Policy::from_value(json!({ "limits": { "max_tasks": -1 } })).is_err());
    }

    #[test]
    fn keyset_door_and_serde_cannot_drift() {
        // Every family the door offers parses; a non-offered family is
        // refused — POLICY_KEYS/policy_child_keys and deny_unknown_fields
        // are the SAME closed set.
        for family in POLICY_KEYS {
            let value = if *family == "optimize" {
                json!({ "optimize": "cost" })
            } else {
                let rule = policy_child_keys(family).expect("every mapping family has rules")[0];
                let inner = if rule == "max_tasks" {
                    json!({ rule: 1 })
                } else {
                    json!({ rule: [] })
                };
                json!({ *family: inner })
            };
            Policy::from_value(value).expect("door-offered key parses");
        }
        assert!(policy_child_keys("ghost").is_none());
        assert!(Policy::from_value(json!({ "ghost": {} })).is_err());
    }

    #[test]
    fn soft_families_are_detected() {
        assert!(!Policy::new().has_soft_families());
        let p = Policy::from_value(json!({ "prefer": { "providers": ["ollama"] } })).unwrap();
        assert!(p.has_soft_families());
        let p = Policy::from_value(json!({ "optimize": "latency" })).unwrap();
        assert!(p.has_soft_families());
    }

    // ── the coarse class table (one voice with deep_static.py) ──────

    #[test]
    fn classify_mirrors_the_reference_table() {
        use EffectClass::{Exec, Net, Tools, Write};
        assert_eq!(EffectClass::classify("exec", None), BTreeSet::from([Exec]));
        assert_eq!(EffectClass::classify("infer", None), BTreeSet::new());
        assert_eq!(EffectClass::classify("agent", None), BTreeSet::new());
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:jq")),
            BTreeSet::from([Tools])
        );
        // net rides fetch AND notify (the COARSE table — unconditional,
        // unlike the fine-grained webhook-only boundary classification)
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:fetch")),
            BTreeSet::from([Net, Tools])
        );
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:notify")),
            BTreeSet::from([Net, Tools])
        );
        // write is exactly write/edit (media builtins stay out of the
        // v1 coarse set — the reference evaluator's exact members)
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:write")),
            BTreeSet::from([Write, Tools])
        );
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:edit")),
            BTreeSet::from([Write, Tools])
        );
        assert_eq!(
            EffectClass::classify("invoke", Some("nika:image_generate")),
            BTreeSet::from([Tools])
        );
    }

    // ── the four hard rules (fixture mirrors) ───────────────────────

    #[test]
    fn human_gate_satisfied_and_missing() {
        let policy = Policy::from_value(json!({
            "require": { "human_gate_before": ["exec"] }
        }))
        .unwrap();
        // fixture 002: ungated exec
        let v = policy_violations(&policy, &[subject("act", "exec", None)]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "require.human_gate_before");
        assert_eq!(v[0].task.as_deref(), Some("act"));
        assert!(
            v[0].detail
                .contains("no nika:prompt ancestor (the pause IS the consent · 10 §policy)"),
            "{}",
            v[0].detail
        );
        // fixture 001: exec behind an ancestor nika:prompt
        let mut human = subject("human", "invoke", Some("nika:prompt"));
        human.parents = vec![];
        let mut act = subject("act", "exec", None);
        act.parents = vec![0];
        assert!(policy_violations(&policy, &[human, act]).is_empty());
    }

    #[test]
    fn the_gate_must_be_an_ancestor_not_a_sibling() {
        let policy = Policy::from_value(json!({
            "require": { "human_gate_before": ["exec"] }
        }))
        .unwrap();
        // a prompt task EXISTS but the exec does not descend from it
        let human = subject("human", "invoke", Some("nika:prompt"));
        let act = subject("act", "exec", None); // no parents
        let v = policy_violations(&policy, &[human, act]);
        assert_eq!(v.len(), 1, "presence is not consent — ancestry is");
    }

    #[test]
    fn exec_after_net_violation_carries_the_path_witness() {
        let policy = Policy::from_value(json!({
            "forbid": { "exec_after": ["net"] }
        }))
        .unwrap();
        // fixture 003: act consumes fetch_page's output
        let fetch = subject("fetch_page", "invoke", Some("nika:fetch"));
        let mut act = subject("act", "exec", None);
        act.parents = vec![0];
        let v = policy_violations(&policy, &[fetch, act]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "forbid.exec_after");
        assert!(
            v[0].detail
                .contains("the path is the witness: fetch_page → act"),
            "{}",
            v[0].detail
        );
        // fixture 004: independent exec stays clean
        let fetch = subject("fetch_page", "invoke", Some("nika:fetch"));
        let act = subject("act", "exec", None);
        assert!(policy_violations(&policy, &[fetch, act]).is_empty());
    }

    #[test]
    fn exec_after_reads_transitive_ancestry() {
        let policy = Policy::from_value(json!({
            "forbid": { "exec_after": ["net"] }
        }))
        .unwrap();
        // fetch → summarize → deploy (the spec's own 3-hop example)
        let fetch = subject("fetch", "invoke", Some("nika:fetch"));
        let mut mid = subject("summarize", "infer", None);
        mid.parents = vec![0];
        let mut deploy = subject("deploy", "exec", None);
        deploy.parents = vec![1];
        let v = policy_violations(&policy, &[fetch, mid, deploy]);
        assert_eq!(v.len(), 1);
        assert!(
            v[0].detail.contains("fetch → deploy"),
            "tainted ancestors only (sorted) + the task: {}",
            v[0].detail
        );
    }

    #[test]
    fn providers_allowlist_violation_and_clean() {
        let policy = Policy::from_value(json!({
            "allow": { "providers": ["ollama", "mistral"] }
        }))
        .unwrap();
        // fixture 005
        let mut s = subject("s", "infer", None);
        s.provider = ProviderPin::Literal("openai/gpt-4o".to_owned());
        let v = policy_violations(&policy, &[s]);
        assert_eq!(v.len(), 1);
        assert!(
            v[0].detail
                .contains("'openai' is not in [ollama · mistral]"),
            "{}",
            v[0].detail
        );
        // fixture 006
        let mut s = subject("s", "infer", None);
        s.provider = ProviderPin::Literal("ollama/llama3.2".to_owned());
        assert!(policy_violations(&policy, &[s]).is_empty());
    }

    #[test]
    fn templated_or_absent_model_fails_closed() {
        let policy = Policy::from_value(json!({
            "allow": { "providers": ["ollama"] }
        }))
        .unwrap();
        // fixture 010: the lane pins Undeterminable for a templated model
        let mut s = subject("s", "infer", None);
        s.provider = ProviderPin::Undeterminable;
        let v = policy_violations(&policy, &[s]);
        assert_eq!(v.len(), 1);
        assert!(v[0].detail.contains("fail-closed: pin the literal"));
        // a non-infer task is never read by the rule
        let v = policy_violations(&policy, &[subject("e", "exec", None)]);
        assert!(v.is_empty());
    }

    #[test]
    fn max_tasks_exceeded_and_within() {
        let policy = Policy::from_value(json!({ "limits": { "max_tasks": 2 } })).unwrap();
        // fixture 007: 3 > 2
        let tasks = [
            subject("a", "infer", None),
            subject("b", "infer", None),
            subject("c", "infer", None),
        ];
        let v = policy_violations(&policy, &tasks);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "limits.max_tasks");
        assert_eq!(v[0].task, None, "workflow-level · no task witness");
        assert!(
            v[0].detail
                .contains("limits.max_tasks: 2 — the workflow declares 3 tasks"),
            "{}",
            v[0].detail
        );
        assert!(policy_violations(&policy, &tasks[..2]).is_empty());
    }

    #[test]
    fn soft_families_are_inert_in_the_judge() {
        // fixture 008: prefer/optimize present · non-preferred provider · clean
        let policy = Policy::from_value(json!({
            "prefer": { "providers": ["ollama"] },
            "optimize": "cost",
        }))
        .unwrap();
        let mut s = subject("s", "infer", None);
        s.provider = ProviderPin::Literal("openai/gpt-4o".to_owned());
        assert!(policy_violations(&policy, &[s]).is_empty());
    }

    #[test]
    fn empty_policy_judges_nothing() {
        let v = policy_violations(&Policy::new(), &[subject("a", "exec", None)]);
        assert!(v.is_empty());
    }

    #[test]
    fn ancestors_survive_cycles_and_bad_indices() {
        // pure totality: a cyclic or corrupt projection never hangs/panics
        let mut a = subject("a", "exec", None);
        a.parents = vec![1, 99];
        let mut b = subject("b", "invoke", Some("nika:fetch"));
        b.parents = vec![0];
        let policy = Policy::from_value(json!({ "forbid": { "exec_after": ["net"] } })).unwrap();
        let v = policy_violations(&policy, &[a, b]);
        assert_eq!(v.len(), 1, "the reachable tainted ancestor still reports");
    }

    #[test]
    fn cert_effects_serializes_the_spec_shape() {
        let e = CertEffects::new(true, Permits::new(), 0);
        let json = serde_json::to_value(&e).expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({ "boundary_declared": true, "needed": {}, "escapes": 0 })
        );
    }

    #[test]
    fn the_wire_names_are_pinned_variant_by_variant() {
        // Both accessors call themselves the WIRE name, and EffectClass adds
        // "witness": these strings land in the serialized form and in the
        // permit witness (F-O6), so they are recorded evidence rather than
        // display text. cargo-mutants replaced each with "" and with
        // "xyzzy" and nothing failed, which means a mutant could rename the
        // effect class inside the very record that proves what a run did.
        // Pinned variant by variant, because a match arm is only as honest
        // as the assertion naming it.
        assert_eq!(EffectClass::Exec.as_str(), "exec");
        assert_eq!(EffectClass::Write.as_str(), "write");
        assert_eq!(EffectClass::Net.as_str(), "net");
        assert_eq!(EffectClass::Tools.as_str(), "tools");

        assert_eq!(Objective::Cost.as_str(), "cost");
        assert_eq!(Objective::Latency.as_str(), "latency");
        assert_eq!(Objective::Quality.as_str(), "quality");
    }
}
