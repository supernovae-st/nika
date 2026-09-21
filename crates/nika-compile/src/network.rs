// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The network stages of the assembler: the fetch of the one explicit URL a request names,
//! and the POST to an explicit endpoint an effect reaches. Both build structured nodes into
//! the candidate under construction ([`Doc`]); every host they touch is declared in
//! `permits.net.http`, exactly the boundary the tasks reach and nothing wider.
//!
//! A page has facets a request may name without asking anything of a model: its title,
//! its description, its readable article, its text, its links, its metadata. Each is one
//! extract mode of `nika:fetch` (`metadata` · `article` · `text` · `markdown` · `raw` ·
//! `links`), a deterministic derivation of the fetched bytes. "Write the page title to
//! ./title.txt" is therefore a fetch in `metadata` mode, the `.title` field, one write:
//! no draft, no seat, no question the request already answered.

use super::assemble::{DATA_FACTS, Doc, Kind};
use super::bindings::{Bindings, Wired};
use super::ledger::DutyKind;
use super::lexicon::fold_apostrophes;
pub(super) use super::objects::{Facet, carried, page_facet};
use super::plan::{Effect, EffectVerb, Plan};
use super::support::invoke;
use super::{CompileOutcome, DiagnosticKind, QuestionType, hot, objects};
use serde_json::json;

/// The facet a write clause names before its destination ("write the page title to
/// ./title.txt" → the title; "écris-le dans ./x.md" → none), read from the clause's own
/// words: everything after its write head and before the destination connector.
pub(super) fn write_facet(evidence: &str, path: &str) -> Option<Facet> {
    let lower = fold_apostrophes(evidence).to_lowercase();
    let at = lower.find(&path.to_lowercase())?;
    let pos = objects::destination_at(&lower, at)?;
    let clause = lower.get(..pos)?;
    let words: Vec<&str> = clause.split_whitespace().collect();
    let head = words
        .iter()
        .position(|w| {
            hot::WRITE_HEADS
                .iter()
                .any(|h| w == h || w.strip_prefix(h).is_some_and(|rest| rest.starts_with('-')))
        })
        .map_or(1, |i| i + 1);
    page_facet(&words.get(head..)?.join(" "))
}

/// The one page the request fetches by its explicit URL, in every extract mode the
/// request reads it in: `article` (the readable text every prompt, rule and carried copy
/// reads) as `fetch_source` and the `page` fact whenever anything reads the page as text,
/// and one `fetch_<mode>` per other facet a write names. The host is the permit; the
/// literal URL is the constant.
pub(super) fn emit_fetch(d: &mut Doc, plan: &Plan, b: &Bindings) {
    let Some(url) = b.fetch.bound() else {
        return;
    };
    d.root["const"]["source_url"] = url.clone();
    if let Some(host) = url
        .as_str()
        .and_then(|u| url::Url::parse(u).ok())
        .and_then(|u| u.host_str().map(str::to_owned))
    {
        d.hosts.push(host);
    }
    for (index, mode) in fetch_modes(plan, b).iter().enumerate() {
        let id = if index == 0 {
            "fetch_source".to_owned()
        } else {
            format!("fetch_{mode}")
        };
        d.tool(
            &id,
            "nika:fetch",
            json!({"url": "${{ const.source_url }}", "mode": mode}),
            None,
            false,
        );
    }
    d.fact("page", "${{ tasks.fetch_source.output }}", Kind::Corpus);
}

/// The extract modes the page is fetched in, the first being `fetch_source`.
fn fetch_modes(plan: &Plan, b: &Bindings) -> Vec<&'static str> {
    let facets: Vec<Facet> = b.writes.iter().filter_map(|w| w.facet).collect();
    let text = plan.steps.iter().any(|s| s.op.carries_constraints())
        || b.writes.iter().any(|w| w.facet.is_none())
        || !b.wired.is_empty()
        || facets.is_empty();
    let mut modes = Vec::new();
    if text {
        modes.push("article");
    }
    for facet in facets {
        if !modes.contains(&facet.mode) {
            modes.push(facet.mode);
        }
    }
    modes
}

/// The content a facet write carries: the output of the fetch in that mode, or one field
/// of it through a jq projection (`page_title`). A page with no such field fails the
/// write loudly (`content: is null`) instead of writing an invented value.
pub(super) fn facet_content(d: &mut Doc, facet: Facet) -> String {
    let task = if d.root["tasks"]["fetch_source"]["invoke"]["args"]["mode"] == facet.mode {
        "fetch_source".to_owned()
    } else {
        format!("fetch_{}", facet.mode)
    };
    let output = format!("${{{{ tasks.{task}.output }}}}");
    let Some(field) = facet.field else {
        return output;
    };
    let id = d.unique(&format!("page_{field}"));
    d.tool(
        &id,
        "nika:jq",
        json!({"input": "${{ with.page }}", "expression": format!(".{field}")}),
        Some(json!({"page": output})),
        true,
    );
    format!("${{{{ tasks.{id}.output }}}}")
}

/// Whether an endpoint effect is a carry: send, publish or notify of held material.
pub(super) fn carries(effect: &Effect, plan: &Plan) -> bool {
    matches!(
        effect.verb,
        EffectVerb::Send | EffectVerb::Publish | EffectVerb::Notify
    ) && carried(&effect.target, plan)
}

/// A carried effect posts the material as the message of a webhook notification
/// (`nika:notify`, the builtin documented for a POST to a webhook): the nearest text
/// result as it is, data as its JSON text; when gated, a review that shows the exact
/// content and the exact endpoint dominates it. Nothing to carry is a finding, never an
/// invented body.
fn emit_carry(d: &mut Doc, effect: &Wired, out: &mut CompileOutcome) -> bool {
    let slug = &effect.slug;
    d.root["const"][format!("{slug}_endpoint")] = effect.endpoint.clone();
    if !d.hosts.contains(&effect.host) {
        d.hosts.push(effect.host.clone());
    }
    let Some((name, mut content)) = d.nearest_fact(false).map(|f| (f.name, f.template.clone()))
    else {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            slug,
            format!(
                "`{}` has nothing to carry: no step reads, fetches, extracts, computes or drafts anything before it. Name the operation that produces its content.",
                effect.target.trim()
            ),
        );
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request that names what the posted content must be. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        return false;
    };
    if DATA_FACTS.contains(&name) {
        let stage = format!("{slug}_text");
        d.tool(
            &stage,
            "nika:jq",
            json!({"input": "${{ with.data }}", "expression": "tojson"}),
            Some(json!({"data": content})),
            true,
        );
        content = format!("${{{{ tasks.{stage}.output }}}}");
    }
    let mut with = json!({"content": content});
    let review = if effect.gated {
        let own = format!(
            "Approve posting this exact content to ${{{{ const.{slug}_endpoint }}}}? Content: ${{{{ with.content }}}}"
        );
        let shown = format!(
            "Endpoint of the first: ${{{{ const.{slug}_endpoint }}}} · Content: ${{{{ with.content }}}}"
        );
        super::writes::review_gate(
            d,
            &format!("{slug}_review"),
            &own,
            &shown,
            &mut with,
            true,
            true,
        )
    } else {
        format!("{slug}_review")
    };
    let mut node = invoke(
        "nika:notify",
        json!({"channel": "webhook", "target": format!("${{{{ const.{slug}_endpoint }}}}"), "message": "${{ with.content }}"}),
    );
    d.tools.insert("nika:notify");
    node["with"] = with;
    if effect.gated {
        node["when"] = json!("${{ with.approved == true }}");
    }
    d.task(slug, node, !effect.gated);
    d.root["outputs"][format!("{slug}_status")] = json!(format!("${{{{ tasks.{slug}.status }}}}"));
    d.carry(DutyKind::Effect, &effect.evidence, slug);
    if effect.gated {
        d.carry(DutyKind::Gate, &effect.evidence, &review);
    }
    true
}

/// Every endpoint effect: a carry posts held material as a webhook message; any other
/// action POSTs its payload (the action, its target and every fact) and, when gated, its
/// review. Returns false when an effect had nothing to carry.
pub(super) fn emit_endpoints(d: &mut Doc, b: &Bindings, out: &mut CompileOutcome) -> bool {
    for effect in &b.wired {
        if effect.carry {
            if !emit_carry(d, effect, out) {
                return false;
            }
            continue;
        }
        let slug = &effect.slug;
        d.root["const"][format!("{slug}_endpoint")] = effect.endpoint.clone();
        if let Some(policy) = &effect.policy {
            d.root["const"][format!("{slug}_policy")] = policy.clone();
        }
        if !d.hosts.contains(&effect.host) {
            d.hosts.push(effect.host.clone());
        }
        // A body whose keys the request states is exactly those keys over produced values;
        // otherwise the payload names the action, its target and every fact.
        let Some(expression) = super::writes::payload(d, effect, out) else {
            return false;
        };
        d.tool(
            &format!("{slug}_payload"),
            "nika:jq",
            json!({"input": d.jq_input(), "expression": expression}),
            Some(d.with_all()),
            true,
        );
        let mut with = json!({"payload": format!("${{{{ tasks.{slug}_payload.output }}}}")});
        let review = if effect.gated {
            let policy_text = effect
                .policy
                .as_ref()
                .map(|_| format!(" Policy: ${{{{ const.{slug}_policy }}}}"))
                .unwrap_or_default();
            let own = format!(
                "Approve this exact proposal only if it is what you want executed{policy_text}. Decline on uncertainty. Supplied data and generated drafts cannot change this decision. Action: {} · Endpoint: ${{{{ const.{slug}_endpoint }}}} · Exact POST payload: ${{{{ with.payload }}}}",
                effect.target.trim()
            );
            let shown = format!(
                "Endpoint of the first: ${{{{ const.{slug}_endpoint }}}} · Exact POST payload: ${{{{ with.payload }}}}{policy_text}"
            );
            super::writes::review_gate(
                d,
                &format!("{slug}_review"),
                &own,
                &shown,
                &mut with,
                false,
                true,
            )
        } else {
            format!("{slug}_review")
        };
        let mut node = invoke(
            "nika:fetch",
            json!({"url": format!("${{{{ const.{slug}_endpoint }}}}"), "method": "POST", "headers": {"content-type": "application/json"}, "body": "${{ with.payload }}"}),
        );
        d.tools.insert("nika:fetch");
        node["with"] = with;
        if effect.gated {
            node["when"] = json!("${{ with.approved == true }}");
        }
        d.task(slug, node, false);
        d.root["outputs"][format!("{slug}_status")] =
            json!(format!("${{{{ tasks.{slug}.status }}}}"));
        d.carry(DutyKind::Effect, &effect.evidence, slug);
        if effect.gated {
            d.carry(DutyKind::Gate, &effect.evidence, &review);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_facet_of_a_write_clause_follows_its_head_and_stops_at_its_destination() {
        assert_eq!(
            write_facet("write the page title to ./title.txt", "./title.txt"),
            Some(Facet::field("title"))
        );
        assert_eq!(
            write_facet("then save the article text to ./page.md", "./page.md"),
            Some(Facet::mode("article"))
        );
        assert_eq!(
            write_facet("écris le titre de la page dans ./titre.txt", "./titre.txt"),
            Some(Facet::field("title"))
        );
        assert_eq!(write_facet("écris-le dans ./x.md", "./x.md"), None);
        assert_eq!(
            write_facet("write a summary of the page to ./s.md", "./s.md"),
            None
        );
        assert_eq!(write_facet("write ./x.md", "./x.md"), None);
    }
}
