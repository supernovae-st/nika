// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The references an authoring seat reads beside its card: what the embedded recall returns for
//! a request (a canonical skeleton's lean source, a pattern family's row), the stdlib contracts
//! of the callables they name, each rendered once and receipted by digest, never the whole
//! shelf. Owned with the seats since 2026-10-07 (the ADR-146 descent); the seats' doors keep
//! these names at their historical paths.

use nika_compile::{HitKind, surface::sha256};
use nika_compile_reader::plan::Plan;
use serde_json::{Value, json};

pub(crate) mod trims;

/// One reference the seat received, as it was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The reference id (`skeleton:<name>`, `family:<id>`, `nika:<builtin>`, a pack row id).
    pub id: String,
    /// What it is: `skeleton`, `family`, `callable`, `pattern`, `block`, `example`, `skill`.
    pub kind: &'static str,
    /// The text the seat reads.
    pub text: String,
}

impl Reference {
    /// One reference.
    #[must_use]
    pub fn new(id: impl Into<String>, kind: &'static str, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            text: text.into(),
        }
    }
    /// The receipt row: id, kind, bytes and digest — never the text again.
    #[must_use]
    pub fn receipt(&self) -> Value {
        json!({"id": self.id, "kind": self.kind, "bytes": self.text.len(), "sha256": sha256(&self.text)})
    }
}

/// What a feed's dates are, which the stdlib pages show only by example: the engine normalizes
/// every feed date to RFC 3339 with an explicit offset (`nika-extract`'s feed mode), whatever
/// format the feed used. Stated here until the extract-modes page states it (amended 2026-10-10:
/// an author filtered `published` as RSS text and kept nothing; an item dated only by its
/// revision carries `updated`).
const FEED_DATES: &str = "Feed dates (the feed's `updated`, each item's `published`, and an item's `updated` when the source dates a revision of its own) are RFC 3339 instants with an explicit offset (`2026-05-22T10:00:00+00:00`), whatever format the feed itself used (an RSS `pubDate`, Atom, JSON Feed). An Atom entry or a JSON Feed item may carry only `updated`, and an item without a date carries neither: read an item's date as `published // updated`. Compare them as instants: jq's `fromdateiso8601` reads them, and `nika:date` computes a bound (`op: now`, then `op: format` with `%Y-%m-%d`, or `op: subtract` with an ISO 8601 `duration`).";

/// What a JSON API needs from `nika:fetch`, which its section states only as the default mode
/// and a table row (amended 2026-10-11: an author fetched a JSON API with the default `markdown`
/// and parsed no JSON).
const JSON_API: &str = "A JSON API answers JSON: fetch it with `mode: jq` and a `jq:` expression (`.` for the whole response), which returns a JSON value, or with `mode: raw`, which returns the body text for `nika:jq` to read with `fromjson`. The default `markdown` returns a Markdown string for HTML pages, never a JSON value.";

/// How `nika:date` shifts an instant, which its section does not state (amended 2026-10-11: a day
/// added across a DST change measured 25 hours).
const DATE_SPANS: &str = "`op: add` and `op: subtract` take `tz:` too (IANA, default UTC) and apply the ISO 8601 `duration` in that zone's calendar: `P1D` is the same local time on the next calendar day, 23 or 25 hours later across a DST change, while `PT24H` is exactly 24 hours.";

/// The law every `nika:jq` program meets, which its section does not state (amended 2026-10-11: a
/// program emitting a stream of values failed).
const JQ_ONE_VALUE: &str = "A program must emit exactly one value: a stream (`.[] | select(…)`) fails `NIKA-BUILTIN-JQ-001`, so wrap it in `[ … ]`; one emitting nothing fails too, and `([ … ] + [null])[0]` gives its first value or null.";

/// The stdlib sections of the callables a candidate may reach: the builtins named in the
/// references plus the everyday set, each cut from the embedded page at its own heading;
/// `nika:fetch` with what each mode returns (`fetch_outputs`), `nika:date` with how it shifts an
/// instant and `nika:jq` with its one-value law.
#[must_use]
pub fn callables(names: &[String]) -> Vec<Reference> {
    let Some(page) = nika_pack::doc("stdlib/builtins-v0.1.md") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in names {
        let Some(section) = stdlib_section(page, name) else {
            continue;
        };
        let text = match name.as_str() {
            "fetch" => fetch_outputs(section),
            "date" => format!("{section}\n\n{DATE_SPANS}"),
            "jq" => format!("{section}\n\n{JQ_ONE_VALUE}"),
            _ => section.to_owned(),
        };
        out.push(Reference::new(format!("nika:{name}"), "callable", text));
    }
    out
}

/// One builtin's whole section of the stdlib page, from the heading that names `nika:<name>` to
/// the next heading of level 3 or above, whichever comes first: the last builtin of a chapter
/// stops before the next chapter's `##` heading, and an argument table cut mid-way reads complete
/// and is not.
fn stdlib_section<'a>(page: &'a str, name: &str) -> Option<&'a str> {
    let heading = format!("### `nika:{name}`");
    let start = page.find(&heading)?;
    let rest = &page[start..];
    let tail = &rest[heading.len()..];
    let end = [tail.find("\n### "), tail.find("\n## ")]
        .into_iter()
        .flatten()
        .min()
        .map_or(rest.len(), |at| at + heading.len());
    Some(rest[..end].trim())
}

/// The `nika:fetch` contract with what each `mode:` returns, which its section only points to:
/// the extract-modes page's table, what a JSON API needs, its whole feed section, then the feed
/// dates' format.
fn fetch_outputs(section: &str) -> String {
    let Some(page) = nika_pack::doc("stdlib/extract-modes-v0.1.md") else {
        return format!("{section}\n\n{JSON_API}\n\n{FEED_DATES}");
    };
    let cut = |start: &str, end: &str| {
        let at = page.find(start)?;
        let stop = page[at..].find(end).map_or(page.len(), |len| at + len);
        Some(page[at..stop].trim())
    };
    let table = cut("| Mode | Output type | Use case |", "\n\n").unwrap_or_default();
    let feed = cut("### `feed`", "\n---").unwrap_or_default();
    format!(
        "{section}\n\n#### What each `mode:` returns\n\n{table}\n\n{JSON_API}\n\n{feed}\n\n{FEED_DATES}"
    )
}

/// The everyday builtins every native call receives, before the ones the references name.
pub const EVERYDAY: &[&str] = &[
    "read", "write", "glob", "jq", "convert", "prompt", "fetch", "notify", "date",
];

/// The references recall returns for the request, expanded just in time: the lean source of
/// every canonical skeleton among the top hits (or covering a hit family) and the row of every
/// hit family, at most `skeletons` sources. Recall orders; nothing here selects.
#[must_use]
pub fn references(intent: &str, skeletons: usize) -> Vec<Reference> {
    let hits = nika_compile::retrieve(intent, 8);
    let mut out = Vec::new();
    let mut named: Vec<String> = Vec::new();
    for hit in &hits {
        let name = match hit.kind {
            HitKind::Skeleton => Some(hit.id.clone()),
            HitKind::Family => hit.skeleton.clone(),
            _ => None,
        };
        if let Some(name) = name
            && !named.contains(&name)
            && named.len() < skeletons
            && let Some(source) = nika_pack::template(&name)
        {
            named.push(name.clone());
            out.push(Reference::new(
                format!("skeleton:{name}"),
                "skeleton",
                nika_pack::lean(source),
            ));
        }
        if hit.kind == HitKind::Family {
            out.push(Reference::new(
                format!("family:{}", hit.id),
                "family",
                format!(
                    "{} · {} · signature {} · patterns {}",
                    hit.id,
                    hit.title,
                    hit.signature.as_deref().unwrap_or("-"),
                    hit.patterns.join(", ")
                ),
            ));
        }
    }
    out
}

/// The builtins the references use, for the callable contracts: every `nika:<name>` the
/// skeleton sources and the knowledge pack's blocks and examples mention, plus the everyday set,
/// deduplicated in that order.
#[must_use]
pub fn builtins_of(references: &[Reference]) -> Vec<String> {
    named_after(EVERYDAY, references)
}

/// [`builtins_of`] for a request the reader read as `plan` (amended 2026-10-10): a build with
/// the prompt trims (`unstable-prompt-trims`, off by default) narrows the everyday set to what
/// the plan's typed operations and effects reach (`trims::reached`); any other build keeps it.
#[must_use]
pub fn builtins_for(references: &[Reference], plan: &Plan) -> Vec<String> {
    if trims::ON {
        named_after(&trims::reached(plan), references)
    } else {
        builtins_of(references)
    }
}

/// `everyday`, then every builtin the references name that it lacks.
fn named_after(everyday: &[&str], references: &[Reference]) -> Vec<String> {
    let mut names: Vec<String> = everyday.iter().map(|s| (*s).to_owned()).collect();
    let sources = references
        .iter()
        .filter(|r| matches!(r.kind, "skeleton" | "block" | "example"));
    for reference in sources {
        for token in reference
            .text
            .split(|c: char| c == '"' || c.is_whitespace())
        {
            if let Some(name) = token.strip_prefix("nika:")
                && !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !names.iter().any(|n| n == name)
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// The callable contracts, then the references recalled for the request, as every door that
/// reads them renders them after its own instructions (the native card, the Plan's).
#[must_use]
pub fn rendered(references: &[Reference], callables: &[Reference]) -> String {
    let mut text = String::from("\n\n# Callable contracts (the stdlib page, cut)\n");
    for callable in callables {
        text.push_str(&callable.text);
        text.push_str("\n\n");
    }
    text.push_str("\n# References recalled for this request (priors, never prisons)\n");
    for reference in references {
        text.push_str("## ");
        text.push_str(&reference.id);
        text.push('\n');
        if reference.kind == "skeleton" {
            text.push_str("```yaml\n");
            text.push_str(&reference.text);
            text.push_str("\n```\n\n");
        } else {
            text.push_str(&reference.text);
            text.push_str("\n\n");
        }
    }
    text
}

/// The embedded pack's canonical templates as a catalogue for whole-catalog reach
/// ([`crate::foundry::reach`]): every template an entry (`skeleton:<name>`, its header's summary
/// and description as its purpose), resolved in full as its lean source. The recall above shows
/// a few in full; through this catalogue no template is out of reach. Knowledge to read, never a
/// component to expand: a template holds `<SLOT>` markers, not producer holes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Templates;

impl Templates {
    /// A template's header summary: the `# TEMPLATE · <name> · <summary>` line's summary, then
    /// the description paragraph that follows it.
    fn summary(source: &str) -> String {
        let mut lines = source.lines().map(str::trim);
        let heading = lines.find(|line| line.starts_with("# TEMPLATE ·"));
        let summary = heading
            .and_then(|line| line.rsplit(" · ").next())
            .unwrap_or_default();
        let description = lines
            .map(|line| line.trim_start_matches('#').trim())
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        format!("{summary}. {description}")
    }
}

impl crate::foundry::ComponentCatalog for Templates {
    fn release(&self) -> crate::foundry::Release {
        let names = nika_pack::template_names();
        let mut all = String::new();
        for name in &names {
            all.push_str(name);
            all.push('\n');
            all.push_str(nika_pack::template(name).unwrap_or_default());
        }
        let version = format!("embedded-pack {}", nika_pack::pack_version());
        crate::foundry::Release::new(version, sha256(&all), "embedded-templates")
    }

    fn resolve(
        &self,
        reference: &crate::foundry::ComponentRef,
    ) -> Result<crate::foundry::Component, crate::foundry::Unresolved> {
        reference.block_name()?;
        Err(crate::foundry::Unresolved::Unknown(reference.id.clone()))
    }

    fn entries(&self) -> Vec<Value> {
        (nika_pack::template_names().iter())
            .filter_map(|name| {
                let source = nika_pack::template(name)?;
                Some(json!({
                    "id": format!("skeleton:{name}"),
                    "kind": "skeleton",
                    "title": name,
                    "purpose": Self::summary(source),
                }))
            })
            .collect()
    }

    fn reference(&self, id: &str) -> Option<nika_compile::KnowledgeReference> {
        let source = nika_pack::template(id.strip_prefix("skeleton:")?)?;
        Some(nika_compile::KnowledgeReference {
            kind: "skeleton".to_owned(),
            id: id.to_owned(),
            text: nika_pack::lean(source).to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DATE_SPANS, EVERYDAY, JQ_ONE_VALUE, JSON_API, callables, stdlib_section};

    /// The `nika:fetch` contract carries what each mode returns and the feed dates' format: an
    /// author reading it knows a feed item's `published` is an RFC 3339 instant, never the
    /// feed's own text; `nika:date` is among the everyday contracts that compute a bound.
    #[test]
    fn the_fetch_contract_carries_each_mode_s_output_and_the_feed_dates() {
        let cut = callables(&["fetch".to_owned(), "date".to_owned()]);
        let fetch = &cut[0].text;
        assert!(fetch.starts_with("### `nika:fetch`"), "{fetch}");
        assert!(
            fetch.contains("| `feed` | object (parsed feed) |"),
            "{fetch}"
        );
        assert!(fetch.contains("\"published\": "), "{fetch}");
        assert!(fetch.contains("`published // updated`"), "{fetch}");
        assert!(
            fetch.contains(
                "RFC 3339 instants with an explicit offset (`2026-05-22T10:00:00+00:00`)"
            ),
            "{fetch}"
        );
        assert!(
            cut[1].text.starts_with("### `nika:date`"),
            "{}",
            cut[1].text
        );
        assert!(EVERYDAY.contains(&"date"));
    }

    /// The `nika:date`, `nika:jq` and `nika:fetch` contracts carry what runs measured: a day
    /// shifted in a zone's calendar, one value per program, and the mode a JSON API needs.
    #[test]
    fn the_date_jq_and_fetch_contracts_carry_what_runs_measured() {
        let cut = callables(&["date".to_owned(), "jq".to_owned(), "fetch".to_owned()]);
        let ids: Vec<&str> = cut.iter().map(|reference| reference.id.as_str()).collect();
        assert_eq!(ids, ["nika:date", "nika:jq", "nika:fetch"]);
        assert!(
            cut[0].text.starts_with("### `nika:date`"),
            "{}",
            cut[0].text
        );
        assert!(cut[0].text.ends_with(DATE_SPANS), "{}", cut[0].text);
        assert!(cut[1].text.ends_with(JQ_ONE_VALUE), "{}", cut[1].text);
        let fetch = &cut[2].text;
        let table = fetch.find("| `raw` |").zip(fetch.find(JSON_API));
        assert!(table.is_some_and(|(row, note)| row < note), "{fetch}");
    }

    /// Every builtin's section on the whole embedded page is its own and nothing more: after its
    /// heading no heading of level 1 to 3 follows outside a code block, and its code fences close,
    /// so a cut never ends inside an example. The last builtin of a chapter (`nika:notify` before
    /// `## Introspection builtins (2)`, and four more) once carried that chapter's heading.
    #[test]
    fn every_builtin_section_stops_before_the_next_heading_and_closes_its_fences() {
        let page = nika_pack::doc("stdlib/builtins-v0.1.md").expect("the embedded stdlib page");
        let names: Vec<&str> = page
            .lines()
            .filter_map(|line| line.strip_prefix("### `nika:")?.split('`').next())
            .collect();
        assert!(names.len() >= 20, "{names:?}");
        for name in names {
            let section = stdlib_section(page, name).expect("a listed heading has its section");
            let mut fenced = false;
            for line in section.lines().skip(1) {
                if line.trim_start().starts_with("```") {
                    fenced = !fenced;
                    continue;
                }
                let hashes = line.len() - line.trim_start_matches('#').len();
                let heading = (1..=3).contains(&hashes) && line[hashes..].starts_with(' ');
                assert!(fenced || !heading, "nika:{name} runs into {line:?}");
            }
            assert!(!fenced, "nika:{name} ends inside a code block");
        }
        let notify = callables(&["notify".to_owned()]);
        assert!(
            !notify[0].text.contains("## Introspection builtins"),
            "{}",
            notify[0].text
        );
    }
}
