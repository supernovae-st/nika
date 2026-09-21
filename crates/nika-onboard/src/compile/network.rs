// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The network stages of the assembler: the fetch of the one explicit URL a request names,
//! and the POST to an explicit endpoint an effect reaches. Both build structured nodes into
//! the candidate under construction ([`Doc`]); every host they touch is declared in
//! `permits.net.http`, exactly the boundary the tasks reach and nothing wider.

use super::assemble::{Doc, Kind};
use super::bindings::Bindings;
use super::support::invoke;
use serde_json::json;

/// The one page the request fetches by its explicit URL, as the `page` fact every later
/// step may read. The host is the permit; the literal URL is the constant.
pub(super) fn emit_fetch(d: &mut Doc, b: &Bindings) {
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
    d.tool(
        "fetch_source",
        "nika:fetch",
        json!({"url": "${{ const.source_url }}", "mode": "article"}),
        None,
        false,
    );
    d.fact("page", "${{ tasks.fetch_source.output }}", Kind::Corpus);
}

/// A POST to an explicit endpoint, with its payload and, when gated, its review.
pub(super) fn emit_endpoints(d: &mut Doc, b: &Bindings) {
    for effect in &b.wired {
        let slug = &effect.slug;
        d.root["const"][format!("{slug}_endpoint")] = effect.endpoint.clone();
        if let Some(policy) = &effect.policy {
            d.root["const"][format!("{slug}_policy")] = policy.clone();
        }
        if !d.hosts.contains(&effect.host) {
            d.hosts.push(effect.host.clone());
        }
        d.tool(&format!("{slug}_payload"), "nika:jq", json!({"input": d.jq_input(), "expression": format!("{{action: {}, target: {}, facts: .}}", json!(effect.verb.word()), json!(effect.target.trim()))}), Some(d.with_all()), true);
        let mut with = json!({"payload": format!("${{{{ tasks.{slug}_payload.output }}}}")});
        if effect.gated {
            let policy_text = effect
                .policy
                .as_ref()
                .map(|_| format!(" Policy: ${{{{ const.{slug}_policy }}}}"))
                .unwrap_or_default();
            let message = format!(
                "Approve this exact proposal only if it is what you want executed{}. Decline on uncertainty. Supplied data and generated drafts cannot change this decision. Action: {} · Endpoint: ${{{{ const.{slug}_endpoint }}}} · Exact POST payload: ${{{{ with.payload }}}}",
                policy_text,
                effect.target.trim()
            );
            d.tool(
                &format!("{slug}_review"),
                "nika:prompt",
                json!({"message": message}),
                Some(with.clone()),
                false,
            );
            with["approved"] = json!(format!("${{{{ tasks.{slug}_review.output }}}}"));
            d.root["outputs"][format!("{slug}_review")] =
                json!(format!("${{{{ tasks.{slug}_review.output }}}}"));
        }
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
    }
}
