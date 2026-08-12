// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sanctioned secret egress — principled declassification (ADR-092).
//!
//! By default a `secrets.X` reaching an `exec`/`invoke` effect is a
//! BLOCKING [`SecretLeak`](super::secrets::SecretLeak): the engine masks
//! its own output but cannot follow a secret a subprocess or tool
//! re-emits. Legitimate workflows nonetheless MUST send a secret to a
//! sink — a webhook-URL secret to `nika:notify`, an API key to a
//! `nika:fetch`. This module decides whether such an egress was
//! **author-sanctioned**, so a sanctioned one is clean and an
//! unsanctioned one stays a leak (default-deny · statically checkable).
//!
//! ## The lineage
//!
//! Declassification is a studied discipline · this design composes its
//! four load-bearing principles ·
//!
//! - **Non-occlusion** (Sabelfeld & Sands · *Declassification: Dimensions
//!   and Principles* · CSFW 2005) · a declassification must not be a hole
//!   another value can hide inside. The `host_from_self` non-occlusion
//!   guard (§L2) refuses a sanction when a SECOND secret co-occurs in the
//!   same payload (« trusted URL + secret-in-body » laundering).
//! - **The Decentralized Label Model** (Myers & Liskov · SOSP 1997) · the
//!   OWNER of data declassifies it. The `egress:` clause therefore lives
//!   ON the secret it sanctions, co-located, never on the sink.
//! - **Robust declassification** (Zdancewic & Myers · CSFW 2001) · an
//!   attacker who controls data must not control WHAT is released. The
//!   `host:` clause sanctions only a STATIC-LITERAL destination (§L2) —
//!   a templated/`${{ }}`-derived host is injectable, so it does NOT
//!   sanction (it stays the runtime `NIKA-SEC-004` check).
//! - **The confidentiality / integrity split** (Microsoft FIDES · 2025 ·
//!   P-F confidentiality ∧ P-T integrity) and **per-value capabilities**
//!   (the `CaMeL` design · 2025) · confidentiality (who may receive · §L1)
//!   is orthogonal to capability (what the workflow may reach at all ·
//!   §L3 `permits.net`). Both must hold (AND-composed) —
//!   `permits` alone never sanctions an egress (the rejected non-occlusion
//!   hole), and an `egress` clause NARROWS, never widens, `permits`.
//!
//! ## The 3 layers (AND-composed)
//!
//! A secret→sink edge is sanctioned iff ALL three hold ·
//!
//! 1. **L1 confidentiality** · the sink's tool-id (or `exec`) matches a
//!    `to:` entry of THAT secret's `egress:` list. SPECIFIC by design —
//!    a clearance for `nika:fetch` never authorizes `exec`.
//! 2. **L2 integrity** · the matched rule's host clause holds statically ·
//!    a `host:` literal equals the sink's literal destination host; OR
//!    `host_from_self: true` AND the destination arg is exactly
//!    `${{ secrets.<this> }}` AND no other secret co-occurs in the
//!    payload. A sink with no host clause (`{ to }`) clears L2 trivially.
//! 3. **L3 capability** · when a `permits:` block is present AND the host
//!    is statically known, that host must ALSO be in `permits.net.http`
//!    (the intersection). `host_from_self` (host unknown statically)
//!    degrades to the runtime `permits` check.
//!
//! `infer`/`agent` ARE reached here (BUG#3): their `prompt:`/`system:` is a
//! provider-egress sink, sanctioned by a sink-only rule `{ to: "infer" }` /
//! `{ to: "agent" }` (no host clause · the provider endpoint is operator-
//! chosen, not a workflow-controlled URL · L2/L3 vacuous, the `to:` match is
//! the whole sanction · same shape as an `exec` egress). Only OUTPUT taint
//! keeps the carve-out (an infer/agent response is not a verbatim echo ·
//! flow.rs §4).

use nika_schema::raw::{RawAction, RawInvokeAction};
use nika_schema::types::{EgressRule, Permits};

use super::permits_fit::{BuiltinEffect, builtin_effect, literal_arg, url_host};

/// The set of `${{ secrets.X }}` islands referenced inside a string —
/// reused to enforce the `host_from_self` non-occlusion guard (no OTHER
/// secret may co-occur with the self-URL in the same payload).
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};

/// Whether the secret named `secret` may flow into `action`'s effect — the
/// L1∧L2∧L3 declassification decision. `egress` is THAT secret's rule list
/// (empty = default-deny = never sanctioned). `permits` is the workflow's
/// declared boundary (`None` = no boundary declared → L3 is vacuous).
///
/// Returns `true` only when some rule sanctions the edge under all three
/// layers; `false` keeps it a leak.
#[must_use]
pub(super) fn is_sanctioned(
    secret: &str,
    egress: &[EgressRule],
    action: &RawAction,
    permits: Option<&Permits>,
) -> bool {
    let sink = sink_id(action);
    egress
        .iter()
        .filter(|rule| rule.to == sink) // L1 · the SPECIFIC sink
        .any(|rule| integrity_ok(secret, rule, action, permits))
}

/// WHY an unsanctioned edge refused — one reason per refused edge,
/// computed beside `is_sanctioned` so the finding's fix names the
/// LAYER that actually failed (the 2026-07-29 audit · run 4: every
/// refused edge used to teach « add `egress:` » even when the clause
/// was already declared, and the real missing layer — sink · host ·
/// capability — went unnamed).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum LeakReason {
    /// No `egress:` at all on the secret — the full sanction teach.
    NoEgress,
    /// Egress exists, but no rule names THIS sink (`to:`) — the author
    /// already declassified and must ADD the sink to the list.
    SinkNotCleared {
        /// The sink the `to:` list misses.
        sink: String,
    },
    /// A rule matches the sink but the sink's literal destination host
    /// is not the rule's (robust declassification — a host clears only
    /// itself).
    HostMismatch {
        /// The host the rule clears.
        declared: String,
        /// The sink's actual literal destination.
        actual: String,
    },
    /// A rule matches sink + host, but the host is not in
    /// `permits.net.http` — the capability layer is the missing one
    /// (the egress narrows, never widens, permits).
    CapabilityMissing {
        /// The host the boundary refuses.
        host: String,
    },
    /// The destination is derived (`${{ }}`-built) — a sanction needs a
    /// static-literal destination, never an injectable one.
    DerivedDestination,
    /// A `host_from_self` rule whose shape is broken: the destination is
    /// not exactly the lone secret, or another secret co-occurs in the
    /// payload (the non-occlusion guard).
    SelfShapeBroken,
}

/// Compute the refusal reason for one refused edge — the caller asks
/// only on edges `is_sanctioned` already refused, so the analysis
/// reads the SAME rules with the SAME primitives (one seam, no drift).
/// A full-clear match under a refused edge is a caller bug (`debug_assert`
/// pins it in dev; production falls back to the full-sanction teach,
/// the safe default).
pub(super) fn leak_reason(
    egress: &[EgressRule],
    action: &RawAction,
    permits: Option<&Permits>,
) -> LeakReason {
    if egress.is_empty() {
        return LeakReason::NoEgress;
    }
    let sink = sink_id(action).to_owned();
    let matching: Vec<&EgressRule> = egress.iter().filter(|rule| rule.to == sink).collect();
    if matching.is_empty() {
        return LeakReason::SinkNotCleared { sink };
    }
    for rule in matching {
        if rule.host_from_self {
            // A matching self-URL rule that refused can only have failed
            // the non-occlusion shape (L2/L3 degrade to the runtime arm).
            return LeakReason::SelfShapeBroken;
        }
        let Some(host) = rule.host.as_deref() else {
            // A host-less matching rule clears L2/L3 vacuously — the edge
            // would be sanctioned, so it cannot be in this path.
            continue;
        };
        let Some(dest) = literal_dest_host(action) else {
            return LeakReason::DerivedDestination;
        };
        if dest != host {
            return LeakReason::HostMismatch {
                declared: host.to_owned(),
                actual: dest,
            };
        }
        if !host_within_permits(permits, host) {
            return LeakReason::CapabilityMissing {
                host: host.to_owned(),
            };
        }
    }
    debug_assert!(false, "leak_reason reached over a sanctioned edge");
    LeakReason::NoEgress
}

/// The sink id of an effect-carrying action — the value an `egress.to:`
/// must equal. `exec` for shells, the tool id for an invoke, and `infer` /
/// `agent` for the provider-egress sink (BUG#3 · a prompt-only egress
/// sanctioned by `{ to: "infer" }` / `{ to: "agent" }`).
fn sink_id(action: &RawAction) -> &str {
    match action {
        RawAction::Exec(_) => "exec",
        RawAction::Invoke(a) => match &a.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => t.value.as_str(),
            // A secret flowing into a child workflow's args is an egress
            // to THAT child — the rule names the target as written.
            nika_schema::raw::RawInvokeTarget::Workflow(w) => w.value.as_str(),
        },
        RawAction::Infer(_) => "infer",
        RawAction::Agent(_) => "agent",
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// L2 (integrity) ∧ L3 (capability) for ONE matched rule.
///
/// - A rule with no host clause (`{ to }`) clears L2/L3 — the sink has no
///   statically-addressable host (e.g. `exec`, or a non-webhook notify).
/// - A `host:` literal must equal the sink's LITERAL destination host
///   (robust declass · a templated host does not sanction) AND, under a
///   `permits:` block, be in `permits.net.http` (L3 intersection).
/// - `host_from_self: true` sanctions only the direct-secret-URL shape
///   with the non-occlusion guard; L3 degrades to the runtime check.
fn integrity_ok(
    secret: &str,
    rule: &EgressRule,
    action: &RawAction,
    permits: Option<&Permits>,
) -> bool {
    if rule.host_from_self {
        return self_url_ok(secret, action);
    }
    let Some(host) = rule.host.as_deref() else {
        // No host clause · the sink carries no addressable host. L2/L3
        // vacuous — L1 (the `to:` match) is the whole sanction.
        return true;
    };
    // A host clause names a network sink. The action's LITERAL destination
    // host must equal it (robust declass · author-controlled, not derived).
    let Some(dest) = literal_dest_host(action) else {
        return false; // templated/derived destination → not sanctioned
    };
    if dest != host {
        return false; // the rule clears a different host
    }
    // L3 · under a declared boundary, the host must ALSO be permitted
    // (intersection · the egress narrows, never widens, permits.net).
    host_within_permits(permits, host)
}

/// L2 for `host_from_self` · the destination arg is EXACTLY
/// `${{ secrets.<this> }}` (not concatenated with other refs/literals) AND
/// no OTHER secret co-occurs anywhere in the effect payload (non-occlusion).
fn self_url_ok(secret: &str, action: &RawAction) -> bool {
    let RawAction::Invoke(a) = action else {
        // `host_from_self` is a network-tool notion (the secret is a URL);
        // an exec « self-URL » is not a modeled shape → not sanctioned.
        return false;
    };
    let Some(BuiltinEffect::Net { url_arg }) = builtin_effect(a) else {
        return false; // not a classified net sink → no self-URL semantics
    };
    // (a) the destination arg is the secret DIRECTLY · exactly one ref,
    //     `secrets.<this>`, and nothing else (no concatenation).
    let Some(dest) = arg_raw(a, url_arg) else {
        return false;
    };
    if !is_exact_secret_ref(dest, secret) {
        return false;
    }
    // (b) NO other secret co-occurs in the whole effect payload — a second
    //     secret in the body would ride out under the trusted self-URL.
    !any_other_secret_in_payload(a, url_arg, secret)
}

/// The raw (un-resolved) string value of `args.<key>` — unlike
/// [`literal_arg`], this KEEPS a `${{ }}` value (the self-URL check needs
/// to see the island, not reject it as dynamic).
fn arg_raw<'a>(a: &'a RawInvokeAction, key: &str) -> Option<&'a str> {
    a.args.as_ref()?.value.get(key)?.as_str()
}

/// Whether `text` is EXACTLY one `${{ secrets.<secret> }}` island and
/// nothing else — the « direct secret » shape (no concatenation). Literal
/// text on either side (`${{ secrets.x }}/extra`) breaks the match.
fn is_exact_secret_ref(text: &str, secret: &str) -> bool {
    let Ok(islands) = scan_templates(text) else {
        return false;
    };
    // exactly one island, and nothing but whitespace outside its span.
    let [island] = islands.as_slice() else {
        return false;
    };
    if !text[..island.start].trim().is_empty() || !text[island.end..].trim().is_empty() {
        return false; // literal text outside the island → concatenation
    }
    let refs = expr_refs(&island.expr);
    matches!(
        refs.as_slice(),
        [NamespaceRef::Secrets(name)] if name == secret
    )
}

/// Whether any secret OTHER than `self_secret` is referenced anywhere in
/// the invoke payload EXCEPT the self-URL arg — the non-occlusion guard.
fn any_other_secret_in_payload(a: &RawInvokeAction, url_arg: &str, self_secret: &str) -> bool {
    let Some(args) = a.args.as_ref() else {
        return false;
    };
    let mut found = false;
    walk_other_secret(&args.value, url_arg, self_secret, true, &mut found);
    found
}

/// Recurse a JSON value collecting whether a non-self secret appears.
/// `at_top` marks the top-level object so the `url_arg` key (the self-URL)
/// is excluded exactly once.
fn walk_other_secret(
    value: &serde_json::Value,
    url_arg: &str,
    self_secret: &str,
    at_top: bool,
    found: &mut bool,
) {
    match value {
        serde_json::Value::String(s) => {
            if string_has_other_secret(s, self_secret) {
                *found = true;
            }
        }
        serde_json::Value::Array(items) => {
            for it in items {
                walk_other_secret(it, url_arg, self_secret, false, found);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                // skip the self-URL arg at the top level (its own secret is
                // the sanctioned one · only OTHER occurrences occlude)
                if at_top && k == url_arg {
                    continue;
                }
                walk_other_secret(v, url_arg, self_secret, false, found);
            }
        }
        _ => {}
    }
}

/// Whether a string references a secret other than `self_secret`.
fn string_has_other_secret(text: &str, self_secret: &str) -> bool {
    let Ok(islands) = scan_templates(text) else {
        return false;
    };
    islands
        .iter()
        .flat_map(|i| expr_refs(&i.expr))
        .any(|r| matches!(r, NamespaceRef::Secrets(name) if name != self_secret))
}

/// The LITERAL destination host of an effect, when it is a classified net
/// sink with a literal URL arg — reuses the permits-fit extraction so the
/// declass host check and the permits host check read the SAME signature.
/// `None` for a non-net sink OR a templated/derived destination.
fn literal_dest_host(action: &RawAction) -> Option<String> {
    let RawAction::Invoke(a) = action else {
        return None; // host clauses are a network-tool notion
    };
    let BuiltinEffect::Net { url_arg } = builtin_effect(a)? else {
        return None;
    };
    literal_arg(a, url_arg).as_deref().and_then(url_host)
}

/// L3 · whether `host` is within the declared `permits.net.http` (or there
/// is no boundary to intersect). The egress narrows permits; it cannot
/// widen it, so a host absent from a declared `net` list is NOT sanctioned.
fn host_within_permits(permits: Option<&Permits>, host: &str) -> bool {
    // No boundary declared → L3 vacuous. Otherwise defer to the ONE canonical
    // host matcher (`Permits::allows_host` → `nika_types::net::host_glob_matches`,
    // case-insensitive) — the third local copy is gone, so nothing to drift
    // (spec §6 Step 4b · the divergence this crate extraction set out to remove).
    permits.is_none_or(|p| p.allows_host(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    /// Parse a workflow and return (secret egress list, the one task's
    /// action, permits) for the named secret + task — the declass inputs.
    fn parts(
        yaml: &str,
        secret: &str,
        task: &str,
    ) -> (Vec<EgressRule>, RawAction, Option<Permits>) {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let egress = wf
            .secrets
            .iter()
            .find(|(n, _)| n.value == secret)
            .map(|(_, s)| s.value.egress.clone())
            .expect("secret");
        let action = wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == task)
            .map(|t| t.value.action.clone())
            .expect("task");
        let permits = wf.permits.map(|p| p.value);
        (egress, action, permits)
    }

    fn sanctioned(yaml: &str, secret: &str, task: &str) -> bool {
        let (egress, action, permits) = parts(yaml, secret, task);
        is_sanctioned(secret, &egress, &action, permits.as_ref())
    }

    const HOOK: &str = "\
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
";

    #[test]
    fn empty_egress_never_sanctions() {
        let y = "nika: w\nsecrets:\n  k:\n    source: env\n    key: K\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: webhook, target: \"${{ secrets.k }}\", message: \"x\" } }\n";
        assert!(!sanctioned(y, "k", "t"), "default-deny");
    }

    #[test]
    fn self_url_direct_secret_is_sanctioned() {
        let y = format!(
            "nika: w\n{HOOK}tasks:\n  t:\n    invoke: {{ tool: \"nika:notify\", args: {{ channel: webhook, target: \"${{{{ secrets.hook }}}}\", message: \"hi\" }} }}\n"
        );
        assert!(sanctioned(&y, "hook", "t"));
    }

    #[test]
    fn self_url_concatenated_is_not_sanctioned() {
        // the secret is part of a larger string — not the direct URL.
        let y = format!(
            "nika: w\n{HOOK}tasks:\n  t:\n    invoke: {{ tool: \"nika:notify\", args: {{ channel: webhook, target: \"${{{{ secrets.hook }}}}/extra\", message: \"hi\" }} }}\n"
        );
        assert!(
            !sanctioned(&y, "hook", "t"),
            "concatenation breaks the self-URL"
        );
    }

    #[test]
    fn self_url_with_second_secret_in_body_is_occluded() {
        let y = "\
nika: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
  leaked:
    source: env
    key: OTHER
tasks:
  t:
    invoke:
      tool: \"nika:notify\"
      args:
        channel: webhook
        target: \"${{ secrets.hook }}\"
        message: \"token ${{ secrets.leaked }}\"
";
        assert!(!sanctioned(y, "hook", "t"), "non-occlusion guard fires");
    }

    #[test]
    fn cross_tool_laundering_is_not_sanctioned() {
        // egress cleared nika:fetch, but the secret is used in exec.
        let y = "\
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
    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"api.x.com\"] }
";
        assert!(!sanctioned(y, "k", "t"), "fetch clearance ≠ exec clearance");
    }

    #[test]
    fn literal_host_match_is_sanctioned() {
        let y = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  t:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.stripe.com/v1/charges\" } }
";
        assert!(sanctioned(y, "k", "t"));
    }

    #[test]
    fn literal_host_mismatch_is_not_sanctioned() {
        let y = "\
nika: w
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  t:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://evil.example.com/x\" } }
";
        assert!(!sanctioned(y, "k", "t"), "a cleared host ≠ every host");
    }

    #[test]
    fn derived_destination_with_host_clause_is_not_sanctioned() {
        // the url is templated — robust declass refuses (injectable).
        let y = "\
nika: w
const: { ep: \"api.stripe.com\" }
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
tasks:
  t:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://${{ const.ep }}/v1/x\", headers: { Authorization: \"${{ secrets.k }}\" } } }
";
        assert!(
            !sanctioned(y, "k", "t"),
            "templated host is not author-fixed"
        );
    }

    #[test]
    fn permits_intersection_blocks_unlisted_host() {
        // egress cleared the host, but permits.net does NOT list it (L3).
        let y = "\
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
    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.stripe.com/v1/x\", headers: { Authorization: \"${{ secrets.k }}\" } } }
";
        assert!(
            !sanctioned(y, "k", "t"),
            "egress narrows permits · it cannot widen them"
        );
    }

    #[test]
    fn permits_intersection_allows_listed_host() {
        let y = "\
nika: w
permits:
  net: { http: [\"api.stripe.com\"] }
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
    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.stripe.com/v1/x\", headers: { Authorization: \"${{ secrets.k }}\" } } }
";
        assert!(sanctioned(y, "k", "t"), "both layers agree on the host");
    }

    #[test]
    fn wrong_tool_id_does_not_match_l1() {
        // egress is for nika:notify but the sink is nika:fetch.
        let y = format!(
            "nika: w\n{HOOK}tasks:\n  t:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"${{{{ secrets.hook }}}}\" }} }}\n"
        );
        assert!(!sanctioned(&y, "hook", "t"), "L1 sink id must match");
    }

    #[test]
    fn host_within_permits_exact_subdomain_and_vacuous() {
        use nika_schema::types::NetPermits;
        let mut boundary = Permits::new();
        boundary.net = Some(NetPermits::new(vec![
            "api.x.com".into(),
            "*.github.com".into(),
        ]));
        let p = Some(&boundary);
        // Exact host in the allowlist is within; an unrelated host is not.
        assert!(host_within_permits(p, "api.x.com"), "exact host is within");
        assert!(
            !host_within_permits(p, "evil.com"),
            "a host absent from the net allowlist is NOT sanctioned"
        );
        // `*.suffix` matches a deeper subdomain AND the bare suffix.
        assert!(
            host_within_permits(p, "api.github.com"),
            "a subdomain is within via the `*.` wildcard"
        );
        assert!(
            host_within_permits(p, "github.com"),
            "the bare suffix is within via the `*.` wildcard"
        );
        // Suffix-attack: neither the suffix nor a subdomain → not within.
        assert!(
            !host_within_permits(p, "github.com.evil.com"),
            "an unrelated domain does not match the `*.` wildcard"
        );
        // Host comparison is case-insensitive (DNS is · the canonical matcher folds).
        assert!(
            host_within_permits(p, "API.GitHub.com"),
            "host match is case-insensitive"
        );
        // No declared net boundary → L3 vacuous (permits do not narrow the host).
        assert!(
            host_within_permits(None, "anything.example"),
            "no boundary declared → vacuously within"
        );
    }

    #[test]
    fn self_url_with_second_secret_nested_in_an_array_is_occluded() {
        // The non-occlusion guard must descend into JSON ARRAYS: a second
        // secret hidden inside an array-valued arg would ride out under the
        // trusted self-URL. Deleting the `Value::Array` walk arm lets it
        // through (wrongly sanctioned) — this asserts it stays a leak.
        let y = "\
nika: w
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
  leaked:
    source: env
    key: OTHER
tasks:
  t:
    invoke:
      tool: \"nika:notify\"
      args:
        channel: webhook
        target: \"${{ secrets.hook }}\"
        attachments:
          - \"token ${{ secrets.leaked }}\"
";
        assert!(
            !sanctioned(y, "hook", "t"),
            "a second secret nested in an array still occludes the self-URL"
        );
    }
}
