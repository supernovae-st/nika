// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Readable reasons from findings already selected by their owner; no state is inferred here.
/// The compiler's reasons a human can act on: its machine sentences (the
/// plan's own vocabulary, an unmapped part with nothing after the colon)
/// dropped, duplicates folded, the rest verbatim.
#[must_use]
pub fn human_reasons(reasons: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for reason in reasons {
        let r = reason.trim();
        let machine = r.contains("semantic plan") || r.ends_with(": .") || r.ends_with(':');
        if machine || r.is_empty() {
            continue;
        }
        let said = human_reason(r);
        if kept.contains(&said) {
            continue;
        }
        kept.push(said);
    }
    kept
}

/// One compiler reason in the human's words — the compiler's fidelity
/// grammar is a closed set (« Candidate N is not feasible: … », « dropped
/// the recognized operation `x` (evidence) », « the path `p` is no longer
/// carried … », « the literal `v` is not in the request »); any other line
/// is kept as the compiler said it.
fn human_reason(raw: &str) -> String {
    if let Some(words) = behaviour_words(raw.trim()) {
        return words;
    }
    let r = raw.trim().trim_end_matches('.');
    // A cut answer is the seat's output limit, an internal cause: its command-line advice
    // (`--authoring-max-tokens`) is no gesture a conversation has, and the request is not at
    // fault.
    if r.contains("--authoring-max-tokens") {
        let tokens: String = r
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        let limit = if tokens.is_empty() {
            "its output limit".to_owned()
        } else {
            format!("its {tokens}-token output limit")
        };
        return format!(
            "the model's answer was cut at {limit} before it was complete — an internal limit of this attempt, not a problem with your request"
        );
    }
    let r = match r.find("is not feasible: ") {
        Some(at) if r.starts_with("Candidate ") => &r[at + "is not feasible: ".len()..],
        _ => r,
    };
    let quoted = |s: &str| -> Option<(String, String)> {
        let start = s.find('`')?;
        let end = s[start + 1..].find('`')? + start + 1;
        Some((s[start + 1..end].to_owned(), s[end + 1..].to_owned()))
    };
    if let Some(rest) = r.strip_prefix("dropped the recognized operation ")
        && let Some((op, tail)) = quoted(rest)
    {
        let evidence = tail
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_end_matches(',')
            .trim();
        return if evidence.is_empty() {
            format!("the draft lost the « {op} » step")
        } else {
            format!("the draft lost « {evidence} » (the {op} step)")
        };
    }
    if let Some(rest) = r.strip_prefix("the path ")
        && let Some((path, tail)) = quoted(rest)
        && tail.contains("no longer carried")
    {
        return format!("the draft dropped « {path} »: nothing reads or writes it any more");
    }
    if let Some(rest) = r.strip_prefix("the literal ")
        && let Some((value, tail)) = quoted(rest)
        && tail.contains("not in the request")
    {
        return format!("the draft invented a value (« {value} ») your request never gave");
    }
    r.to_owned()
}

/// A behaviour round's located defects (`{"kind":"behaviour_failed","obligations":[…]}`, what a
/// repair starts from) in words: what a rehearsal of the draft showed against each result the
/// request asks for. A person read that record as raw JSON (2026-10-19). `None` for any other
/// line.
fn behaviour_words(raw: &str) -> Option<String> {
    let record: serde_json::Value = serde_json::from_str(raw).ok()?;
    if record["kind"] != "behaviour_failed" {
        return None;
    }
    let defects: Vec<String> = (record["obligations"].as_array()?.iter())
        .filter_map(|obligation| {
            let requested = obligation["requested"].as_str()?;
            let observed: Vec<&str> = (obligation["observed"].as_array()?.iter())
                .filter_map(serde_json::Value::as_str)
                .collect();
            Some(format!("{} — asked: {requested}", observed.join(" · ")))
        })
        .collect();
    (!defects.is_empty()).then(|| {
        format!(
            "a rehearsal of the draft gave a result your request does not ask for: {}",
            defects.join(" ; ")
        )
    })
}
