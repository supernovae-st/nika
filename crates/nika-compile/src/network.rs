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
use super::lexicon::{ARTICLES, fold_apostrophes};
use super::plan::{Effect, EffectVerb, Plan};
use super::support::invoke;
use super::{CompileOutcome, DiagnosticKind, QuestionType, hot, objects};
use serde_json::json;

/// A facet of a fetched page a write carries as it is: the extract mode of `nika:fetch`
/// that yields it and, for one field of the metadata object, that field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Facet {
    pub mode: &'static str,
    pub field: Option<&'static str>,
}

impl Facet {
    const fn mode(mode: &'static str) -> Self {
        Self { mode, field: None }
    }
    const fn field(field: &'static str) -> Self {
        Self {
            mode: "metadata",
            field: Some(field),
        }
    }
}

/// Words that name the page itself beside a facet ("the page title", "il titolo della
/// pagina"), folded.
const PAGE_WORDS: &[&str] = &[
    "page", "pages", "pagina", "webpage", "web", "site", "website", "url", "seite", "webseite",
];

/// Words that name the article of a page: its readable body, `mode: article`.
const ARTICLE_WORDS: &[&str] = &["article", "articles", "articolo", "articulo", "artikel"];

/// `of` and its kin, and the connectors that join a facet to its destination or its form
/// ("the page as markdown"): never content, folded.
const LINK_WORDS: &[&str] = &[
    "of", "de", "du", "des", "d", "della", "del", "dell", "di", "da", "von", "der", "from", "to",
    "into", "in", "dans", "vers", "sous", "en", "nel", "nella", "su", "sul", "sulla", "as",
    "comme", "come", "como", "als",
];

/// The facet one head noun names, folded (EN · FR · IT · ES · DE).
fn facet_head(word: &str) -> Option<Facet> {
    const TITLE: &[&str] = &["title", "titre", "titolo", "titulo", "titel"];
    const DESCRIPTION: &[&str] = &["description", "descrizione", "descripcion", "beschreibung"];
    const TEXT: &[&str] = &["text", "texte", "testo", "texto"];
    const CONTENT: &[&str] = &[
        "content",
        "contents",
        "contenu",
        "contenido",
        "contenuto",
        "body",
        "corps",
        "inhalt",
    ];
    const HTML: &[&str] = &["html", "raw"];
    const LINKS: &[&str] = &[
        "links",
        "link",
        "liens",
        "lien",
        "enlaces",
        "enlace",
        "collegamenti",
        "collegamento",
    ];
    const METADATA: &[&str] = &[
        "metadata",
        "metadonnees",
        "metadati",
        "metadatos",
        "metadaten",
    ];
    if TITLE.contains(&word) {
        Some(Facet::field("title"))
    } else if DESCRIPTION.contains(&word) {
        Some(Facet::field("description"))
    } else if TEXT.contains(&word) {
        Some(Facet::mode("text"))
    } else if CONTENT.contains(&word) || word == "markdown" {
        Some(Facet::mode("markdown"))
    } else if HTML.contains(&word) {
        Some(Facet::mode("raw"))
    } else if LINKS.contains(&word) {
        Some(Facet::mode("links"))
    } else if METADATA.contains(&word) {
        Some(Facet::mode("metadata"))
    } else {
        None
    }
}

/// The facet of the fetched page an object names, when it names nothing else: "the page
/// title", "le titre de la page", "the article text", "the links", "the page". Any word
/// outside the page, facet, article and link tables ("a summary of the page", "the top 3
/// links") is content the fetch does not yield as it is, so no facet is read.
pub(super) fn page_facet(object: &str) -> Option<Facet> {
    let folded = hot::fold(&fold_apostrophes(object)).replace("'s", "");
    let mut head: Option<Facet> = None;
    let mut article = false;
    let mut page = false;
    for token in folded
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
    {
        if ARTICLES.contains(&token) || LINK_WORDS.contains(&token) {
            continue;
        }
        if PAGE_WORDS.contains(&token) {
            page = true;
            continue;
        }
        if ARTICLE_WORDS.contains(&token) {
            article = true;
            continue;
        }
        head = Some(facet_head(token)?);
    }
    match (head, article, page) {
        // "the article text", "le contenu de l'article": the readable body.
        (Some(facet), true, _) if matches!(facet.mode, "text" | "markdown") => {
            Some(Facet::mode("article"))
        }
        (Some(facet), _, _) => Some(facet),
        (None, true, _) => Some(Facet::mode("article")),
        (None, false, true) => Some(Facet::mode("markdown")),
        (None, false, false) => None,
    }
}

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

/// Connectors that join an effect's object to its destination ("it to <url>", "le
/// rapport à ops@x"), folded.
const DESTINATION_CONNECTORS: &[&str] = &[
    "to", "into", "at", "on", "onto", "vers", "a", "sur", "dans", "en", "su", "al", "an", "nach",
];

/// The object of an effect phrase before its destination literal and the connector that
/// joins them: `it to https://x/notify` → `it`; `the report to ops@x` → `the report`.
fn object_before_destination(target: &str) -> String {
    let lower = hot::fold(&fold_apostrophes(target));
    let cut = lower
        .split_whitespace()
        .find(|w| {
            w.starts_with("http://")
                || w.starts_with("https://")
                || (w.contains('@') && w.contains('.'))
        })
        .and_then(|w| lower.find(w))
        .unwrap_or(lower.len());
    let words: Vec<&str> = lower
        .get(..cut)
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let end = words.len().saturating_sub(usize::from(
        words
            .last()
            .is_some_and(|w| DESTINATION_CONNECTORS.contains(w)),
    ));
    words.get(..end).unwrap_or_default().join(" ")
}

/// Whether an effect phrase carries material the plan already holds, unchanged: its object
/// before the destination is a back-reference ("it", "the file") or names a source or a
/// produced result by its own head ("the report" after `./report.md`, "the reply" after a
/// drafted reply). Anything else ("a summary") is content a step must produce first.
pub(super) fn carried(target: &str, plan: &Plan) -> bool {
    let object = object_before_destination(target);
    objects::refers_back(&object, plan.steps.iter().map(|s| s.detail.as_str()))
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
    if effect.gated {
        let review = format!("{slug}_review");
        d.tool(
            &review,
            "nika:prompt",
            json!({"message": format!("Approve posting this exact content to ${{{{ const.{slug}_endpoint }}}}? Content: ${{{{ with.content }}}}")}),
            Some(with.clone()),
            true,
        );
        with["approved"] = json!(format!("${{{{ tasks.{review}.output }}}}"));
        d.root["outputs"][&review] = json!(format!("${{{{ tasks.{review}.output }}}}"));
    }
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
    true
}

#[cfg(test)]
mod tests {
    use super::super::plan::{Op, Step};
    use super::*;

    fn plan_with(details: &[(Op, &str)]) -> Plan {
        let mut plan = Plan::default();
        for (op, detail) in details {
            plan.steps.push(Step {
                op: *op,
                evidence: (*detail).to_owned(),
                detail: (*detail).to_owned(),
                categories: Vec::new(),
            });
        }
        plan
    }

    #[test]
    fn a_carried_object_is_a_back_reference_or_a_head_that_recurs_in_a_source() {
        let read = plan_with(&[(Op::Read, "./report.md")]);
        assert_eq!(object_before_destination("it to https://x/notify"), "it");
        assert_eq!(
            object_before_destination("the report to ops@example.invalid"),
            "the report"
        );
        assert_eq!(
            object_before_destination("le rapport à ops@x.fr"),
            "le rapport"
        );
        assert_eq!(object_before_destination("sending"), "sending");
        for target in [
            "it to https://x/notify",
            "the report to https://x/notify",
            "the file to https://x/notify",
            "https://x/notify",
        ] {
            assert!(carried(target, &read), "{target}");
        }
        for target in ["a summary to https://x/notify", "a reply to ops@x"] {
            assert!(!carried(target, &read), "{target}");
        }
        let drafted = plan_with(&[(Op::Read, "./inbox/a.md"), (Op::Draft, "a reply")]);
        assert!(carried("the reply to ops@example.invalid", &drafted));
    }

    #[test]
    fn a_facet_is_read_from_the_page_words_alone_in_five_languages() {
        for (object, facet) in [
            ("the page title", Facet::field("title")),
            ("the title of the page", Facet::field("title")),
            ("the page's title", Facet::field("title")),
            ("le titre de la page", Facet::field("title")),
            ("il titolo della pagina", Facet::field("title")),
            ("el título de la página", Facet::field("title")),
            ("der Titel der Seite", Facet::field("title")),
            ("the page description", Facet::field("description")),
            ("the article text", Facet::mode("article")),
            ("the article", Facet::mode("article")),
            ("le texte de l'article", Facet::mode("article")),
            ("the page text", Facet::mode("text")),
            ("the text", Facet::mode("text")),
            ("the page", Facet::mode("markdown")),
            ("the page content", Facet::mode("markdown")),
            ("the page as markdown", Facet::mode("markdown")),
            ("the raw html", Facet::mode("raw")),
            ("the links", Facet::mode("links")),
            ("les liens de la page", Facet::mode("links")),
            ("the page metadata", Facet::mode("metadata")),
        ] {
            assert_eq!(page_facet(object), Some(facet), "{object}");
        }
    }

    #[test]
    fn content_the_fetch_does_not_yield_as_it_is_reads_no_facet() {
        for object in [
            "a summary of the page",
            "the summary",
            "the top 3 links",
            "a short note about the page",
            "the page title and the description",
            "it",
            "-le",
            "",
            "the main points",
        ] {
            assert_eq!(page_facet(object), None, "{object}");
        }
    }

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
