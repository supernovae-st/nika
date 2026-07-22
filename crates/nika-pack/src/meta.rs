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
        tasks: task_key_count(body),
    }
}

/// Count the task map keys (W1 « the map »): bare `name:` block keys at
/// indent 2 inside the top-level `tasks:` section — the same boundary
/// shape every surface anchors on since the list form died.
fn task_key_count(body: &str) -> usize {
    let mut in_tasks = false;
    let mut n = 0;
    for line in body.lines() {
        if !line.starts_with(' ') && !line.starts_with('#') && line.contains(':') {
            in_tasks = line.starts_with("tasks:");
            continue;
        }
        if !in_tasks {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent != 2 {
            continue;
        }
        let head = line.trim_start().split('#').next().unwrap_or("").trim_end();
        if let Some(name) = head.strip_suffix(':')
            && !name.is_empty()
            && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            n += 1;
        }
    }
    n
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
/// locked presentation order. The top-level `permits:` block is SKIPPED:
/// its own `exec:`/`tools:` keys are grants, not task verbs (latent
/// until F-O8 put a `permits:` block in every pack file).
fn verbs_of(body: &str) -> Vec<&'static str> {
    let mut found = [false; 4];
    let mut in_permits = false;
    for line in body.lines() {
        if !line.starts_with(' ') && !line.starts_with('#') && line.contains(':') {
            in_permits = line.starts_with("permits:");
            continue;
        }
        if in_permits {
            continue;
        }
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
