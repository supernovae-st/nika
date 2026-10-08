// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Whole-catalog reach (R3 · A1): the lexical pack is a shortlist, never the eligibility gate.
//!
//! Every admitted entry the pack does not already hold is put to the same decision seat, in the
//! same batch, by its descriptor — what it is for, its version, holes, effects and interfaces,
//! never its code. The seat's answer then decides the form the author reads:
//! - an entry it finds applicable joins the pack in full, resolved from the admitted release;
//! - one it cannot judge (NONE, a failed call) stays a descriptor, a hypothesis to explore;
//! - one it judges unrelated stays out, its answer kept in the record. That is a judgment, never
//!   a proven incompatibility: binding and Check prove incompatibilities.
//!
//! Without a seat the descriptors are shown as they are. The coverage account says how many
//! admitted entries were asked, and how. Only a catalogue that lists the release's entries makes
//! the pass complete; without one the record says that only the recalled pack was asked.

use std::collections::BTreeSet;

use nika_compile::{AuthoringKnowledge, KnowledgeReference};
use serde_json::{Value, json};

use super::component::ComponentCatalog;
use super::entry::{role_line, sources_line};
use super::recall::{block_metadata, text_of};

/// The law the coverage account states.
pub const REACH: &str = "every admitted entry the catalogue lists is a candidate: the recalled pack is asked in full, every other entry by its descriptor in the same batch; lexical rank orders, never gates";

/// The first line that marks a descriptor: the entry itself stays in the admitted release.
pub const DESCRIPTOR: &str =
    "[descriptor of an admitted entry; its full text is resolved when it applies]";

/// The fields that say what an entry is for, whatever its kind: a block's or pattern's purpose, a
/// family's need, an example's intent, a counterexample's difference and why it is wrong, a
/// callable's description, a construct's what and when, a skill's scope, a diagnostic's failure
/// and teaching, a repair's symptom and strategy, a skeleton's how, a facet's values.
const PURPOSE: [&str; 16] = [
    "purpose",
    "need",
    "intent",
    "difference",
    "why_wrong",
    "description",
    "what",
    "when",
    "when_not",
    "scope",
    "failure",
    "teach",
    "symptom",
    "strategy",
    "how",
    "values",
];

/// An admitted entry's descriptor: its role, title and purpose, then the metadata that keeps it
/// from being misused (version, holes, effects, authority, interfaces, callables, known
/// failures) and the sources it derives from.
#[must_use]
pub fn descriptor(row: &Value) -> String {
    let title = text_of(row, &["title"]);
    let purpose = text_of(row, &PURPOSE);
    let role = role_line(row)
        .map(|line| format!("{line}\n"))
        .unwrap_or_default();
    let sources = sources_line(row).unwrap_or_default();
    let text = format!(
        "{DESCRIPTOR}\n{role}{title} — {purpose}\n{}{sources}",
        block_metadata(row)
    );
    text.trim_end().to_owned()
}

/// The entries of `catalog` the pack does not hold, appended to it as descriptors. Returns the
/// ids appended and how many entries the catalogue listed.
pub fn widen(
    pack: &mut AuthoringKnowledge,
    catalog: &dyn ComponentCatalog,
) -> (Vec<String>, usize) {
    let held: BTreeSet<String> = pack.references.iter().map(|r| r.id.clone()).collect();
    let entries = catalog.entries();
    let mut widened = Vec::new();
    for row in &entries {
        let (Some(id), Some(kind)) = (row["id"].as_str(), row["kind"].as_str()) else {
            continue;
        };
        if held.contains(id) || widened.iter().any(|seen| seen == id) {
            continue;
        }
        pack.references.push(KnowledgeReference {
            kind: kind.to_owned(),
            id: id.to_owned(),
            text: descriptor(row),
        });
        widened.push(id.to_owned());
    }
    (widened, entries.len())
}

/// After qualification: each entry `catalog` widened that the seat found applicable, resolved in
/// full from it. Returns how many were resolved.
pub fn resolve_applicable(
    pack: &mut AuthoringKnowledge,
    record: &Value,
    widened: &[String],
    catalog: &dyn ComponentCatalog,
) -> usize {
    let applies: BTreeSet<String> = (record["references"].as_array().into_iter().flatten())
        .filter(|row| row["verdict"] == "applies")
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect();
    let mut resolved = 0;
    for reference in &mut pack.references {
        if widened.contains(&reference.id)
            && applies.contains(&reference.id)
            && let Some(full) = catalog.reference(&reference.id)
        {
            *reference = full;
            resolved += 1;
        }
    }
    resolved
}

/// The record's rows marked with the form each reference was asked in (`widened`: by its
/// descriptor) and, unless discarded, shown in.
pub fn annotate(pack: &AuthoringKnowledge, record: &mut Value, widened: &[String]) {
    let shown: BTreeSet<&str> = pack
        .references
        .iter()
        .filter(|r| !r.text.starts_with(DESCRIPTOR))
        .map(|r| r.id.as_str())
        .collect();
    for row in record["references"].as_array_mut().into_iter().flatten() {
        let id = row["id"].as_str().unwrap_or_default().to_owned();
        let asked = if widened.contains(&id) {
            "descriptor"
        } else {
            "full"
        };
        row["asked"] = json!(asked);
        if row["verdict"] != "unrelated" {
            row["shown"] = json!(if shown.contains(id.as_str()) {
                "full"
            } else {
                "descriptor"
            });
        }
    }
}

/// What one catalogue's pass covered: its release, how many entries it lists, how many the pack
/// already held, how many were asked by descriptor and how many were resolved in full.
#[must_use]
pub fn account(
    catalog: &dyn ComponentCatalog,
    listed: usize,
    widened: usize,
    resolved: usize,
) -> Value {
    json!({
        "release": catalog.release().record(),
        "admitted_entries": listed,
        "already_in_pack": listed.saturating_sub(widened),
        "asked_by_descriptor": widened,
        "resolved_in_full": resolved,
    })
}

/// The coverage account of one qualification: complete only when a release catalogue was lent.
#[must_use]
pub fn coverage(
    catalog: Option<&dyn ComponentCatalog>,
    listed: usize,
    widened: usize,
    resolved: usize,
) -> Value {
    match catalog {
        Some(catalog) => {
            let mut account = account(catalog, listed, widened, resolved);
            account["law"] = json!(REACH);
            account["complete"] = json!(true);
            account
        }
        None => json!({
            "complete": false,
            "why": "no catalogue was lent: only the recalled pack was asked, so an admitted entry sharing no word with the request was never a candidate",
        }),
    }
}
