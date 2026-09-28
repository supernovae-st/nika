// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The bounded canonical-spelling expansion of a text equality (R4 A5 · C2). Text is compared
//! byte-exact everywhere (jq equality, the candidate, a seat's answer): « livré » typed with a
//! precomposed é never equals a file's « livré » spelled e + U+0301, so a filter selected no row
//! and the run said nothing. Where the host observed, among a compared column's categorical
//! values, a spelling that differs from the stated literal only by Unicode canonical equivalence
//! (NFC), the equality also matches exactly that observed spelling, and the decision records it
//! (`decision.spellings`). This is no normalization at run and no global change of equality: no
//! case folding, no compatibility (NFKC) folding, no accent stripping; a spelling the bounded
//! sample did not show, or a column the observer did not find categorical, stays byte-exact.
use super::grounding;
use crate::CompileOutcome;
use crate::rules::Rule;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

/// The law's name, as the decision records it.
const LAW: &str = "bounded canonical-spelling expansion: observed spellings canonically \
    equivalent (Unicode NFC) to the stated text, each matched exactly";

/// The rule whose text equalities over the source at `path` also match the observed spellings
/// canonically equivalent to their literal. The decision records every expansion.
pub(crate) fn ground(
    mut rule: Rule,
    world: Option<&Value>,
    path: &str,
    out: &mut CompileOutcome,
) -> Rule {
    let row = grounding::row(world, path);
    let canonical = |text: &str| text.nfc().collect::<String>();
    let mut records = Vec::new();
    for (field, literal) in rule.text_equalities() {
        let Some(values) = row
            .and_then(|r| r["values"].get(&field))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let target = canonical(&literal);
        let spellings: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter(|v| *v != literal && canonical(v) == target)
            .map(str::to_owned)
            .collect();
        if spellings.is_empty() {
            continue;
        }
        if let Some(spelled) = rule.with_spellings(&field, &literal, &spellings) {
            rule = spelled;
        }
        records.push(json!({
            "rule": rule.text(), "field": field, "literal": literal, "spellings": spellings,
            "law": LAW, "source": path, "revision": grounding::revision(row),
        }));
    }
    if !records.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["spellings"] = Value::Array(records);
        out.provenance.decision = Some(decision);
    }
    rule
}
