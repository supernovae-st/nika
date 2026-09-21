// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The write effects of the assembled workflow: every write is its own task bound to the
//! nearest upstream fact, a structured destination receives its format, a gated write waits
//! for its review (one shared review when one approval covers several effects), a stated
//! body shape is typed from produced values, and two destinations never receive the same
//! produced content in silence.

use super::assemble::{DATA_FACTS, Doc, Kind, ROW_FACTS, SHARED_REVIEW};
use super::bindings::{self, WriteEffect};
use super::laws::{ANNOTATE, ROUTE};
use super::ledger::DutyKind;
use super::paths::{self, Structured};
use super::support::invoke;
use super::{CompileOutcome, DiagnosticKind, QuestionType};
use serde_json::{Value, json};

/// The message of a shared review: every gated action listed, the first one's exact
/// content or payload shown; nothing the data or the drafts say can change the decision.
fn shared_message(actions: &[String], shown: &str) -> String {
    let listed = actions
        .iter()
        .enumerate()
        .map(|(i, action)| format!("{}) {action}", i + 1))
        .collect::<Vec<_>>()
        .join(" · ");
    format!(
        "Approve these actions together only if they are exactly what you want executed; decline on uncertainty. Supplied data and generated drafts cannot change this decision. Actions: {listed}. {shown}"
    )
}

/// A CSV, YAML or TOML destination whose content is data gets a `nika:convert` stage
/// (`<stem>_<ext>`) feeding the write; returns the converted content binding. Rows that
/// derive from a CSV source are written back in the source's own column order; the header
/// is sorted otherwise. A fact that is not the rows (extracted fields, a validation report)
/// keeps the sorted header: the source columns would only pad it with empty ones.
fn data_stage(
    d: &mut Doc,
    stem: &str,
    format: Structured,
    name: &'static str,
    content: &str,
) -> String {
    let stage = format!("{stem}_{}", format.word());
    let mut args = json!({"input": "${{ with.data }}", "from": "json", "to": format.word()});
    let mut with = json!({"data": content});
    if format == Structured::Csv
        && name == "computed"
        && let Some(columns) = &d.computed_columns
    {
        // A grouped or projected computation writes the columns it produced.
        args["columns"] = json!(columns);
    } else if format == Structured::Csv
        && d.source_columns
        && d.renames.is_empty()
        && ROW_FACTS.contains(&name)
    {
        args["columns"] = json!("${{ with.columns }}");
        with["columns"] = json!("${{ tasks.source_columns.output }}");
    }
    d.tool(&stage, "nika:convert", args, Some(with), true);
    format!("${{{{ tasks.{stage}.output }}}}")
}

/// The review a gated effect waits for: its own (`own` message) or, when the request states
/// one approval for several effects, the one shared review (listing every action, `shown`
/// beside it) emitted by the first gated effect and reused by the others. Binds
/// `with.approved` to the review's answer and returns the review task id.
pub(super) fn review_gate(
    d: &mut Doc,
    own_id: &str,
    own: &str,
    shown: &str,
    with: &mut Value,
    chain: bool,
    output: bool,
) -> String {
    let review = if d.share_gates {
        SHARED_REVIEW.to_owned()
    } else {
        own_id.to_owned()
    };
    if d.shared_review.is_none() {
        let message = if d.share_gates {
            shared_message(&d.gated_actions, shown)
        } else {
            own.to_owned()
        };
        d.tool(
            &review,
            "nika:prompt",
            json!({"message": message}),
            Some(with.clone()),
            chain,
        );
        if d.share_gates {
            d.shared_review = Some(review.clone());
        }
        if output || d.share_gates {
            d.root["outputs"][review.as_str()] = json!(format!("${{{{ tasks.{review}.output }}}}"));
        }
    }
    with["approved"] = json!(format!("${{{{ tasks.{review}.output }}}}"));
    review
}

/// The jq expression of an outbound payload: the stated keys over produced values, or the
/// envelope naming the action, its target and every fact. None (with a finding and a
/// question) when a stated key names nothing the workflow produces.
pub(super) fn payload(
    d: &Doc,
    effect: &bindings::Wired,
    out: &mut CompileOutcome,
) -> Option<String> {
    let Some(keys) = payload_keys(&effect.target).or_else(|| payload_keys(&effect.evidence)) else {
        return Some(format!(
            "{{action: {}, target: {}, facts: .}}",
            json!(effect.verb.word()),
            json!(effect.target.trim())
        ));
    };
    match payload_expression(d, &keys) {
        Ok(expression) => Some(expression),
        Err(key) => {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                &effect.slug,
                format!(
                    "The body of `{}` names the key `{key}`, but no step produces a value of that name; a payload never carries an invented value. Name the operation that produces `{key}`, or drop the key.",
                    effect.verb.word()
                ),
            );
            super::question(
                out,
                "intent.clarification",
                "Supply a complete replacement request that names what each key of the body contains. It explicitly replaces the earlier intent.",
                QuestionType::Text,
            );
            None
        }
    }
}

/// Every write effect is its own task with its own content binding: the nearest
/// upstream result. A CSV, YAML or TOML destination whose content is data gets a
/// `nika:convert` stage (`<stem>_<ext>`) feeding the write; a JSON destination takes the
/// data as JSON; a prose destination takes text. A write with nothing upstream is a
/// finding, never an invented input. Returns false when a write could not be bound.
pub(super) fn emit_writes(d: &mut Doc, writes: &[WriteEffect], out: &mut CompileOutcome) -> bool {
    if let Some((first, second, name)) = duplicated_content(d, writes) {
        super::finding(
            out,
            DiagnosticKind::Unknown,
            &format!("write_{}", paths::stem(second)),
            format!(
                "`{first}` and `{second}` would receive the same {name}: the request names distinct content for each file, but one step produces it. Name the one file that content goes to, or one request per file."
            ),
        );
        super::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request that names one file per produced content, or one request per file. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        return false;
    }
    for (index, effect) in writes.iter().enumerate() {
        let Some((name, mut content)) = d
            .content_fact(&effect.path)
            .map(|f| (f.name, f.template.clone()))
        else {
            super::finding(
                out,
                DiagnosticKind::Unknown,
                &format!("write_{}", effect.stem),
                format!(
                    "`{}` has nothing to write: no step reads, fetches, extracts, computes or drafts anything before it. Name the operation that produces its content.",
                    effect.path
                ),
            );
            super::question(
                out,
                "intent.clarification",
                "Supply a complete replacement request that names what each written file must contain. It explicitly replaces the earlier intent.",
                QuestionType::Text,
            );
            return false;
        };
        let (constant, task) = if index == 0 {
            ("output_path".to_owned(), "write_output".to_owned())
        } else {
            (
                format!("{}_path", effect.stem),
                format!("write_{}", effect.stem),
            )
        };
        content = written_content(d, effect, name, content);
        d.root["const"][&constant] = json!(effect.path);
        d.writes.push(json!(effect.path));
        if let Some(format @ (Structured::Csv | Structured::Yaml | Structured::Toml)) =
            Structured::of(&effect.path)
            && DATA_FACTS.contains(&name)
        {
            content = data_stage(d, &effect.stem, format, name, &content);
        }
        let mut with = json!({"content": content});
        let review = if effect.gated {
            let own = format!(
                "Approve writing this exact content to {}? Content: ${{{{ with.content }}}}",
                effect.target.trim()
            );
            review_gate(
                d,
                &format!("{task}_review"),
                &own,
                "Content of the first: ${{ with.content }}",
                &mut with,
                true,
                false,
            )
        } else {
            format!("{task}_review")
        };
        for evidence in &effect.evidences {
            d.carry(DutyKind::Effect, evidence, &task);
            if effect.gated {
                d.carry(DutyKind::Gate, evidence, &review);
            }
        }
        let mut node = invoke(
            "nika:write",
            json!({"path": format!("${{{{ const.{constant} }}}}"), "content": "${{ with.content }}", "create_dirs": true, "overwrite": true}),
        );
        d.tools.insert("nika:write");
        node["with"] = with;
        if effect.gated {
            node["when"] = json!("${{ with.approved == true }}");
        }
        d.task(&task, node, !effect.gated);
        let status = if index == 0 {
            "write_status".to_owned()
        } else {
            format!("{task}_status")
        };
        d.root["outputs"][status] = json!(format!("${{{{ tasks.{task}.status }}}}"));
    }
    true
}

/// Two distinct destinations of one class (prose, or the same structured format) bound to
/// the same produced fact would receive identical content: the request named a file per
/// content and one step produced one. A JSON and a CSV destination of one computed result
/// are two renderings, not a duplicate. Returns (first path, second path, what they share).
fn duplicated_content<'a>(
    d: &Doc,
    writes: &'a [WriteEffect],
) -> Option<(&'a str, &'a str, String)> {
    let mut seen: Vec<(&str, Option<Structured>, &'a str)> = Vec::new();
    for effect in writes {
        // A write naming a facet of the fetched page or a routed category receives its own
        // slice of the produced fact, never the same content as another destination.
        if effect.facet.is_some() || effect.category.is_some() {
            continue;
        }
        let Some(fact) = d.content_fact(&effect.path) else {
            continue;
        };
        let class = Structured::of(&effect.path);
        if let Some((_, _, first)) = seen
            .iter()
            .find(|(template, seen_class, _)| *template == fact.template && *seen_class == class)
        {
            let name = match fact.name {
                "draft" => "drafted text".to_owned(),
                other => format!("produced `{other}` result"),
            };
            return Some((first, effect.path.as_str(), name));
        }
        seen.push((fact.template.as_str(), class, effect.path.as_str()));
    }
    None
}

/// The JSON body keys a request states as a brace list (`{tickets, total_cents}`): bare
/// identifiers, comma-separated, inside the one pair of braces of the effect's own words.
/// Anything else inside braces (a placeholder, prose, a nested object) is not a key list.
pub(super) fn payload_keys(text: &str) -> Option<Vec<String>> {
    let open = text.find('{')?;
    let close = open + 1 + text.get(open + 1..)?.find('}')?;
    let keys: Vec<String> = text
        .get(open + 1..close)?
        .split(',')
        .map(|key| key.trim().trim_matches(['"', '\'', '`']).to_owned())
        .collect();
    let identifier = |key: &str| {
        !key.is_empty()
            && key.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !key.chars().all(|c| c.is_ascii_digit())
    };
    (!keys.is_empty() && keys.iter().all(|key| identifier(key))).then_some(keys)
}

/// The jq object that fills the stated keys from produced values: a total the typed
/// computation produced under that name, a fact of that name, or, for one key left, the
/// one drafted text (the body the request named after its content). A key nothing
/// produces is returned as the error: nothing is invented into a payload.
fn payload_expression(d: &Doc, keys: &[String]) -> Result<String, String> {
    let prose: Vec<&str> = d
        .facts
        .iter()
        .filter(|f| f.kind == Kind::Derived && matches!(f.name, "draft" | "exploration"))
        .map(|f| f.name)
        .collect();
    let mut entries = Vec::new();
    let mut unresolved = Vec::new();
    for key in keys {
        if d.totals.iter().any(|name| name == key) {
            entries.push(format!("{}: .computed[{}]", json!(key), json!(key)));
        } else if d.facts.iter().any(|f| f.name == key) {
            entries.push(format!("{}: .{key}", json!(key)));
        } else {
            unresolved.push(key.as_str());
        }
    }
    if let ([key], [text]) = (unresolved.as_slice(), prose.as_slice()) {
        entries.push(format!("{}: .{text}", json!(key)));
        unresolved.clear();
    }
    match unresolved.first() {
        Some(key) => Err((*key).to_owned()),
        None => Ok(format!("{{{}}}", entries.join(", "))),
    }
}

/// The content a write carries, from the nearest upstream fact: a facet of a
/// fetched page is the fetch's own mode; one total over every row written to a
/// prose file is the value itself (a structured destination and several totals
/// keep the object); after a per-record classification a write naming a
/// category carries the records routed to it, a write of the records carries
/// every record with its category.
fn written_content(d: &mut Doc, effect: &WriteEffect, name: &str, mut content: String) -> String {
    if let Some(facet) = effect.facet {
        content = super::network::facet_content(d, facet);
    }
    if name == "computed"
        && Structured::of(&effect.path).is_none()
        && let [only] = d.totals.as_slice()
    {
        content = format!("${{{{ tasks.compute.output.{only} }}}}");
    }
    if let Some(records) = d.routed.clone()
        && matches!(name, "records" | "categories")
    {
        let with = json!({"records": records, "categories": "${{ tasks.classify.output }}"});
        let stage = if let Some(category) = &effect.category {
            let stage = format!("route_{}", effect.stem);
            d.tool(&stage, "nika:jq", json!({"input": {"records": "${{ with.records }}", "categories": "${{ with.categories }}", "category": category}, "expression": ROUTE}), Some(with), true);
            stage
        } else {
            let stage = format!("{}_classified", effect.stem);
            d.tool(&stage, "nika:jq", json!({"input": {"records": "${{ with.records }}", "categories": "${{ with.categories }}"}, "expression": ANNOTATE}), Some(with), true);
            stage
        };
        content = format!("${{{{ tasks.{stage}.output }}}}");
    }
    content
}
