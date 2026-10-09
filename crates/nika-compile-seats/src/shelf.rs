// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The references an authoring seat reads beside its card: what the embedded recall returns for
//! a request (a canonical skeleton's lean source, a pattern family's row), the stdlib contracts
//! of the callables they name, each rendered once and receipted by digest, never the whole
//! shelf. Owned with the seats since 2026-10-07 (the ADR-146 descent); the seats' doors keep
//! these names at their historical paths.

use nika_compile::{HitKind, surface::sha256};
use serde_json::{Value, json};

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

/// The stdlib sections of the callables a candidate may reach: the builtins named in the
/// references plus the everyday set, each cut from the embedded page at its own heading.
#[must_use]
pub fn callables(names: &[String]) -> Vec<Reference> {
    let Some(page) = nika_pack::doc("stdlib/builtins-v0.1.md") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in names {
        let heading = format!("### `nika:{name}`");
        let Some(start) = page.find(&heading) else {
            continue;
        };
        let rest = &page[start..];
        let end = rest[heading.len()..]
            .find("\n### ")
            .or_else(|| rest[heading.len()..].find("\n## "))
            .map_or(rest.len(), |at| at + heading.len());
        // The whole section: an argument table cut mid-way reads complete and is not.
        let section = rest[..end].trim();
        out.push(Reference::new(format!("nika:{name}"), "callable", section));
    }
    out
}

/// The everyday builtins every native call receives, before the ones the references name.
pub const EVERYDAY: &[&str] = &[
    "read", "write", "glob", "jq", "convert", "prompt", "fetch", "notify",
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
    let mut names: Vec<String> = EVERYDAY.iter().map(|s| (*s).to_owned()).collect();
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
