// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Derived metadata over the embedded corpus — the pack KNOWS its own
//! files (title · verbs · task count), so every consumer (CLI listings ·
//! MCP · docs projections) reads ONE derivation instead of re-parsing.
//! Everything here is derived from the artifact at call time — never
//! stored, never hand-written (born-stale law).

/// What one embedded example says about itself (derived, never stored).
pub struct ExampleMeta {
    /// The full teaching filename (`01-hello.nika.yaml`).
    pub file: String,
    /// The header title/pitch line, cleaned (`Hello world — the
    /// smallest useful Nika workflow`).
    pub title: String,
    /// The verbs the tasks actually use, in the locked order.
    pub verbs: Vec<&'static str>,
    /// How many tasks the workflow carries.
    pub tasks: usize,
}

/// Derive the display metadata from one example body.
#[must_use]
pub fn meta(slug: &str, body: &str) -> ExampleMeta {
    ExampleMeta {
        file: format!("{slug}.nika.yaml"),
        title: title_of(slug, body),
        verbs: verbs_of(body),
        tasks: body
            .lines()
            .filter(|l| l.trim_start().starts_with("- id:"))
            .count(),
    }
}

/// The title out of the file's OWN header comments — foundation files
/// carry `# NN · Title — pitch`, showcase files carry the pitch on the
/// first prose comment after the `# showcase · …` tier line. A file
/// without either degrades to an empty title (never invented prose).
fn title_of(slug: &str, body: &str) -> String {
    let number = slug
        .rsplit('/')
        .next()
        .and_then(|s| s.split('-').next())
        .unwrap_or_default();
    let mut saw_tier_line = false;
    let mut pitch = String::new();
    for line in body.lines().take(12) {
        let Some(comment) = line.strip_prefix('#') else {
            break; // the header block ended — YAML begins
        };
        let text = comment.trim();
        if text.starts_with("SPDX") || text.starts_with("yaml-language-server") {
            continue;
        }
        if text.is_empty() {
            if saw_tier_line && !pitch.is_empty() {
                break; // the pitch paragraph ended
            }
            continue;
        }
        // Foundation: `01 · Hello world — …` (starts with its number).
        if let Some(rest) = text.strip_prefix(number) {
            return rest.trim_start_matches([' ', '·']).trim().to_owned();
        }
        // Showcase: the tier line first (`showcase · T1 starter · …`),
        // then the pitch — which may WRAP across comment lines; join
        // until the header's next beat (a blank comment ends the pitch).
        if text.starts_with("showcase") {
            saw_tier_line = true;
            continue;
        }
        if saw_tier_line {
            pitch.push_str(text);
            pitch.push(' ');
            continue;
        }
        break;
    }
    pitch.trim().trim_end_matches('.').to_owned()
}

/// The verbs a workflow's tasks use — a line scan for the 4 locked verb
/// keys at task-field indentation (`    infer:` …), deduped in the
/// locked presentation order.
fn verbs_of(body: &str) -> Vec<&'static str> {
    let mut found = [false; 4];
    for line in body.lines() {
        let t = line.trim_start();
        for (i, verb) in ["infer:", "exec:", "invoke:", "agent:"].iter().enumerate() {
            if t.starts_with(verb) {
                found[i] = true;
            }
        }
    }
    ["infer", "exec", "invoke", "agent"]
        .into_iter()
        .zip(found)
        .filter_map(|(v, hit)| hit.then_some(v))
        .collect()
}
