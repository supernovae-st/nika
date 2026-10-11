// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A candidate's edits applied as one revision. Each old passage is found exactly once in the
//! original document and none overlaps another, so the edits never depend on their order: what
//! the author read is what each one replaces.

/// `source` with every `(old, new)` pair applied as one revision. A single pair keeps the one-
/// passage words; with several, a refusal names its edit, counted from 1.
///
/// # Errors
///
/// When no pair is given, an old passage is empty or not found exactly once in `source`, or two
/// old passages overlap.
pub fn edited(source: &str, pairs: &[(String, String)]) -> Result<String, String> {
    let name = |k: usize| {
        if pairs.len() == 1 {
            String::new()
        } else {
            format!("edit {}: ", k + 1)
        }
    };
    let mut spans = Vec::with_capacity(pairs.len());
    for (k, (old, new)) in pairs.iter().enumerate() {
        let found = source.matches(old.as_str()).count();
        let at = (source.find(old.as_str())).filter(|_| !old.is_empty() && found == 1);
        let Some(at) = at else {
            return Err(format!(
                "{}`old` must occur exactly once in the candidate; it occurs {found} times",
                name(k)
            ));
        };
        spans.push((at, at + old.len(), new.as_str(), k + 1));
    }
    if spans.is_empty() {
        return Err("no edit given: `old` and `new`, or `edits`".to_owned());
    }
    spans.sort_unstable_by_key(|span| span.0);
    for pair in spans.windows(2) {
        if let [first, second] = pair
            && second.0 < first.1
        {
            return Err(format!(
                "edits {} and {} overlap: each old passage must stand apart",
                first.3, second.3
            ));
        }
    }
    let mut out = String::with_capacity(source.len());
    let mut last = 0;
    for (start, end, new, _) in spans {
        out.push_str(source.get(last..start).unwrap_or_default());
        out.push_str(new);
        last = end;
    }
    out.push_str(source.get(last..).unwrap_or_default());
    Ok(out)
}

#[cfg(test)]
mod tests;
