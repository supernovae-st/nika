// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring workspace's knowledge: what a native authoring call may read, versioned and
//! journaled. The stable part is compact — the engine and pack identity, the compiler's laws,
//! the language in one page and the canonical fragments a candidate is built from — and the
//! rest arrives just in time: the contracts of the callables the request may reach (cut from
//! the embedded stdlib page, one section per builtin) and the references recall returns (a
//! canonical skeleton's lean source, a pattern family's row), never the whole shelf. Every
//! piece carries an id and a digest so the receipt can say what the seat actually read.

use serde_json::{Value, json};

/// One reference the seat received, as it was sent.
pub(super) struct Reference {
    pub(super) id: String,
    pub(super) kind: &'static str,
    pub(super) text: String,
}

impl Reference {
    fn new(id: impl Into<String>, kind: &'static str, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            text: text.into(),
        }
    }
    /// The receipt row: id, kind, bytes and digest — never the text again.
    pub(super) fn receipt(&self) -> Value {
        json!({"id": self.id, "kind": self.kind, "bytes": self.text.len(), "sha256": sha256(&self.text)})
    }
}

pub(super) use nika_compile::surface::sha256;

/// The identity every native call is stamped with: the engine, the embedded language pack,
/// the spec pin and the digests of the card and of the engine's output conventions.
pub(super) fn identity() -> Value {
    json!({
        "engine": env!("CARGO_PKG_VERSION"),
        "pack": nika_pack::pack_version(),
        "spec_pin": spec_pin(),
        "card_sha256": sha256(card()),
        "conventions_sha256": sha256(CONVENTIONS),
    })
}

use nika_compile::surface::spec_pin;

/// The path of the stable card in the embedded pack: the laws, the language in one page and the
/// canonical fragments, written from the 0.120 canon (`nika spec --canon`) and the assembler's
/// own emitted shapes, which every check admits; versioned with the pack and by its digest in
/// the receipt.
pub(super) const CARD_PATH: &str = "stdlib/authoring-card-v0.1.md";

/// The card's text; empty only when the pack lacks it (the receipt's digest says so).
pub(super) fn card() -> &'static str {
    nika_pack::doc(CARD_PATH).unwrap_or_default()
}

/// The engine's output conventions sent after the spec's card (the `LINES` law, named shapes).
pub(super) const CONVENTIONS: &str = include_str!("../../assets/native_output_conventions.md");

/// The stdlib sections of the callables a candidate may reach: the builtins named in the
/// references plus the everyday set, each cut from the embedded page at its own heading.
pub(super) fn callables(names: &[String]) -> Vec<Reference> {
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
        let section = rest[..end].trim();
        // Two thousand bytes of contract per builtin is the working set; the page's examples
        // and forward-compat notes beyond that are not what a candidate needs.
        let text: String = section.chars().take(2_000).collect();
        out.push(Reference::new(format!("nika:{name}"), "callable", text));
    }
    out
}

/// The everyday builtins every native call receives, before the ones the references name.
pub(super) const EVERYDAY: &[&str] = &[
    "read", "write", "glob", "jq", "convert", "prompt", "fetch", "notify",
];

/// The references recall returns for the request, expanded just in time: the lean source of
/// every canonical skeleton among the top hits (or covering a hit family) and the row of every
/// hit family, at most `skeletons` sources. Recall orders; nothing here selects.
pub(super) fn references(intent: &str, skeletons: usize) -> Vec<Reference> {
    let hits = crate::retrieve::retrieve(intent, 8);
    let mut out = Vec::new();
    let mut named: Vec<String> = Vec::new();
    for hit in &hits {
        let name = match hit.kind {
            crate::retrieve::HitKind::Skeleton => Some(hit.id.clone()),
            crate::retrieve::HitKind::Family => hit.skeleton.clone(),
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
        if hit.kind == crate::retrieve::HitKind::Family {
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
/// skeleton sources mention, plus the everyday set, deduplicated in that order.
pub(super) fn builtins_of(references: &[Reference]) -> Vec<String> {
    let mut names: Vec<String> = EVERYDAY.iter().map(|s| (*s).to_owned()).collect();
    for reference in references.iter().filter(|r| r.kind == "skeleton") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_is_versioned_and_the_callables_are_cut_from_the_embedded_page() {
        let id = identity();
        assert!(card().contains("# Laws"), "the pack carries the card");
        assert_eq!(id["card_sha256"].as_str().map(str::len), Some(64));
        assert!(!id["pack"].as_str().unwrap_or_default().is_empty());
        let cut = callables(&["read".to_owned(), "jq".to_owned(), "nope".to_owned()]);
        assert_eq!(
            cut.len(),
            2,
            "{:?}",
            cut.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
        assert!(
            cut[0].text.starts_with("### `nika:read`"),
            "{}",
            cut[0].text
        );
        assert!(cut[1].text.len() <= 2_000);
    }

    #[test]
    fn references_expand_the_recalled_skeletons_and_families_and_name_their_builtins() {
        let refs = references(
            "Read ./data/orders.csv, keep the paid rows, total the amounts and write a report to ./out/report.md",
            2,
        );
        let skeletons: Vec<&Reference> = refs.iter().filter(|r| r.kind == "skeleton").collect();
        assert!(
            !skeletons.is_empty() && skeletons.len() <= 2,
            "{}",
            refs.len()
        );
        assert!(
            skeletons[0].text.starts_with("nika:"),
            "{}",
            skeletons[0].text
        );
        assert!(refs.iter().any(|r| r.kind == "family"));
        let names = builtins_of(&refs);
        assert!(names.iter().any(|n| n == "read") && names.iter().any(|n| n == "jq"));
        let row = refs[0].receipt();
        assert_eq!(row["sha256"].as_str().map(str::len), Some(64));
    }

    #[test]
    fn the_output_conventions_state_the_assemblers_line_law_and_leave_unnamed_shapes_open() {
        // The line idiom is the assembler's own law, byte for byte: the seat is told what the
        // deterministic door already writes, and a drift of either fails here.
        assert!(CONVENTIONS.contains(crate::laws::LINES), "{CONVENTIONS}");
        assert!(
            CONVENTIONS.contains(r#"join("\n") + "\n""#),
            "{CONVENTIONS}"
        );
        // Shapes only when the request names them, stated as patterns rather than one request's
        // words; an unnamed shape stays the source's.
        for named in [
            "« la liste des <champs> »",
            "« seulement <champ> »",
            "« par <clé> »",
            "map(.<field>)",
            "from_entries",
        ] {
            assert!(CONVENTIONS.contains(named), "{named}: {CONVENTIONS}");
        }
        assert!(CONVENTIONS.contains("unless the request names another shape"));
        assert!(CONVENTIONS.contains("When the request names no shape, keep the source's shape"));
        // Engine-owned text is receipted by digest beside the spec's card.
        assert_eq!(identity()["conventions_sha256"], json!(sha256(CONVENTIONS)));
    }
}
