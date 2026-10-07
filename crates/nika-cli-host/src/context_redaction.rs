// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shape-based redaction for outgoing model context. Callers own the read boundary;
//! this pure filter hides recognized credential forms and reports their kinds.

/// Redact obvious secrets before anything leaves: API keys, private key
/// blocks, `password=`/`token=` values. Returns the text and the KINDS
/// found (never a value). An unterminated private-key block hides the rest
/// of the input; this shape-based filter is not a general secret detector.
#[must_use]
pub fn redact(text: &str) -> (String, Vec<String>) {
    let mut kinds = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut private_key_end: Option<String> = None;
    for line in text.lines() {
        // A missing or mismatched footer keeps the remaining material hidden.
        if let Some(end) = &private_key_end {
            if line.contains(end) {
                private_key_end = None;
            }
            continue;
        }
        if let Some((_, rest)) = line.split_once("-----BEGIN ")
            && let Some((label, _)) = rest.split_once("-----")
            && label.contains("PRIVATE KEY")
        {
            let end = format!("-----END {label}-----");
            private_key_end = (!rest.contains(&end)).then_some(end);
            out.push_str("[redacted private key block]\n");
            kinds.push("private key".to_owned());
            continue;
        }
        let mut l = line.to_owned();
        for (marker, kind) in [
            ("sk-", "api key"),
            ("AKIA", "aws key"),
            ("ghp_", "github token"),
            ("xoxb-", "slack token"),
        ] {
            let mut scan = 0;
            let mut kept = 0;
            let mut redacted = String::new();
            while let Some(offset) = l[scan..].find(marker) {
                let i = scan + offset;
                scan = i + marker.len();
                if l[i..].len() < marker.len() + 8
                    || !l[scan..]
                        .chars()
                        .take(8)
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    continue;
                }
                let end = l[i..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                    .map_or(l.len(), |e| i + e);
                // Copy each untouched span once instead of repeatedly shifting
                // the tail of a line containing many credentials.
                redacted.push_str(&l[kept..i]);
                redacted.push_str("[redacted]");
                kept = end;
                scan = end;
            }
            if kept != 0 {
                redacted.push_str(&l[kept..]);
                l = redacted;
                kinds.push(kind.to_owned());
            }
        }
        if l.contains("-----BEGIN") {
            "[redacted private key block]".clone_into(&mut l);
            kinds.push("private key".to_owned());
        }
        for key in ["password", "token", "secret"] {
            let lower = l.to_ascii_lowercase();
            let hit = lower
                .find(&format!("{key}="))
                .or_else(|| lower.find(&format!("{key}: ")));
            if let Some(i) = hit {
                let start = i + key.len() + 1;
                let start = if l[start..].starts_with(' ') {
                    start + 1
                } else {
                    start
                };
                if start < l.len()
                    && !l[start..].trim().is_empty()
                    && !l[start..].starts_with("${{")
                {
                    l.replace_range(start.., "[redacted]");
                    kinds.push(key.to_owned());
                }
            }
        }
        out.push_str(&l);
        out.push('\n');
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    kinds.sort();
    kinds.dedup();
    (out, kinds)
}
