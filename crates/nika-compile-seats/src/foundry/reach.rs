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
use super::recall::{block_metadata, text_of};

/// The law the coverage account states.
pub const REACH: &str = "every admitted entry the catalogue lists is a candidate: the recalled pack is asked in full, every other entry by its descriptor in the same batch; lexical rank orders, never gates";

/// The first line that marks a descriptor: the entry itself stays in the admitted release.
pub const DESCRIPTOR: &str =
    "[descriptor of an admitted entry; its full text is resolved when it applies]";

/// An admitted entry's descriptor: its title and purpose, then the metadata that keeps it from
/// being misused (version, holes, effects, authority, interfaces, callables, known failures).
#[must_use]
pub fn descriptor(row: &Value) -> String {
    let title = text_of(row, &["title"]);
    let purpose = text_of(row, &["purpose", "need", "intent", "strategy"]);
    let text = format!("{DESCRIPTOR}\n{title} — {purpose}\n{}", block_metadata(row));
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

/// After qualification: each widened entry the seat found applicable, resolved in full from the
/// catalogue; the record's rows say which form each reference was asked and shown in. Returns how
/// many were resolved.
pub fn resolve_applicable(
    pack: &mut AuthoringKnowledge,
    record: &mut Value,
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
    resolved
}

/// The coverage account of one qualification.
#[must_use]
pub fn coverage(
    catalog: Option<&dyn ComponentCatalog>,
    listed: usize,
    widened: usize,
    resolved: usize,
) -> Value {
    match catalog {
        Some(catalog) => json!({
            "law": REACH,
            "complete": true,
            "release": catalog.release().record(),
            "admitted_entries": listed,
            "already_in_pack": listed.saturating_sub(widened),
            "asked_by_descriptor": widened,
            "resolved_in_full": resolved,
        }),
        None => json!({
            "complete": false,
            "why": "no catalogue was lent: only the recalled pack was asked, so an admitted entry sharing no word with the request was never a candidate",
        }),
    }
}
