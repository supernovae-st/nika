// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W1 « the map » migration — the machine-applicable repair for the dead
//! envelope forms (`NIKA-PARSE-020..023`).
//!
//! Line-based and structure-aware: comments, blank lines and source order
//! are preserved byte-for-byte outside the three transformed shapes. The
//! transform is IDEMPOTENT (a migrated document returns `None`), and it is
//! the ONE repair `check --fix` applies when the parser refuses an old-map
//! document — the old form is repairable, never executable (there is no
//! legacy parser path).
//!
//! Transforms (top-level only — nothing else is touched):
//! 1. `workflow: <scalar>`            → `workflow:` + `  id: <scalar>`
//! 2. top-level `description: <text>` → hoisted under the workflow object
//! 3. `tasks:` sequence `  - id: X`   → map key `  X:` (the two-space list
//!    marker becomes the key's indent, so task bodies never re-indent)
//!
//! A task whose `id:` is NOT the item's first line is deliberately not
//! handled — the parser's teaching names the file and a human decides
//! (never guess; the conformance suite pins the refusal).

/// Apply the W1 migration. `Some(new)` when the document changed,
/// `None` when it is already in the new form (idempotence by contract).
#[must_use]
pub(crate) fn w1(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.split('\n').collect();

    // pass 1 · locate the top-level `description:` (to hoist) and the
    // top-level `workflow:` line (scalar or already-object).
    let mut desc_line: Option<usize> = None;
    let mut desc_text: Option<&str> = None;
    let mut wf_line: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        if desc_line.is_none()
            && let Some(rest) = l.strip_prefix("description: ")
        {
            desc_line = Some(i);
            desc_text = Some(rest.trim_end());
        }
        if wf_line.is_none() && (l.starts_with("workflow:") || *l == "workflow:") {
            wf_line = Some(i);
        }
    }

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let mut in_tasks = false;
    let mut changed = false;
    for (i, l) in lines.iter().enumerate() {
        if Some(i) == desc_line {
            changed = true; // hoisted under workflow (emitted there)
            continue;
        }
        if Some(i) == wf_line {
            if let Some(id) = scalar_workflow_id(l) {
                out.push("workflow:".to_owned());
                out.push(format!("  id: {id}"));
                if let Some(d) = desc_text {
                    out.push(format!("  description: {d}"));
                }
                changed = true;
                continue;
            }
            // already an object — still hoist a stray top-level description
            out.push((*l).to_owned());
            if let Some(d) = desc_text {
                out.push(format!("  description: {d}"));
                changed = true;
            }
            continue;
        }
        // track which top-level section we are in (col-0 keys)
        if !l.starts_with(' ') && !l.starts_with('#') && l.contains(':') {
            in_tasks = l.starts_with("tasks:");
        }
        if in_tasks && let Some(rewritten) = task_item_to_key(l) {
            out.push(rewritten);
            changed = true;
            continue;
        }
        out.push((*l).to_owned());
    }
    changed.then(|| out.join("\n"))
}

/// `workflow: some-id` (optional trailing comment) → the id, comment kept
/// by the caller's rewrite of the line pair. `workflow:` alone (object
/// head) returns `None`.
fn scalar_workflow_id(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("workflow: ")?;
    let rest = rest.trim_end();
    if rest.is_empty() {
        return None;
    }
    // keep any trailing comment attached to the id line
    let token_end = rest.find(" #").unwrap_or(rest.len());
    let token = rest[..token_end].trim_end();
    let ok = token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(rest)
}

/// `  - id: name` (optional trailing comment) → `  name:` (+ comment).
/// The body lines that follow at indent 4 stay untouched: the list marker
/// column becomes the key's indent, so alignment is preserved.
fn task_item_to_key(line: &str) -> Option<String> {
    let rest = line.strip_prefix("  - id: ")?;
    let rest = rest.trim_end();
    let token = match rest.find('#') {
        Some(idx) => rest[..idx].trim_end(),
        None => rest,
    };
    // everything after the token (its original spacing + comment) rides along
    let comment = &rest[token.len()..];
    let ok = !token.is_empty()
        && token.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    ok.then(|| format!("  {token}:{comment}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "# banner\nnika: v1\nworkflow: demo-flow\ndescription: A demo\nmodel: mock/echo\n\ntasks:\n  # fetch first\n  - id: fetch\n    invoke:\n      tool: nika:fetch\n      args: { url: x }\n\n  - id: summarize\n    depends_on: [fetch]\n    with:\n      doc: ${{ tasks.fetch.output }}\n    infer:\n      prompt: go\n";

    #[test]
    fn migrates_all_three_shapes_and_preserves_comments() {
        let new = w1(OLD).expect("changes");
        assert!(new.contains("workflow:\n  id: demo-flow\n  description: A demo"));
        assert!(new.contains("  # fetch first\n  fetch:\n    invoke:"));
        assert!(new.contains("  summarize:\n    depends_on: [fetch]"));
        assert!(new.contains("# banner"), "comments preserved");
        assert!(!new.contains("- id:"));
    }

    #[test]
    fn idempotent_by_contract() {
        let once = w1(OLD).expect("changes");
        assert!(w1(&once).is_none(), "migrated form must return None");
    }

    #[test]
    fn verb_named_ids_survive_the_trap() {
        // the census trap: task ids shadowing verb names — the KEY is the
        // identity, the inner verb key is untouched
        let old = "nika: v1\nworkflow: t\ntasks:\n  - id: invoke\n    exec:\n      command: [\"true\"]\n  - id: agent\n    infer:\n      prompt: hi\n";
        let new = w1(old).expect("changes");
        assert!(new.contains("  invoke:\n    exec:"));
        assert!(new.contains("  agent:\n    infer:"));
    }

    #[test]
    fn trailing_comments_ride_along() {
        let old = "nika: v1\nworkflow: t  # SLOT: kebab id\ntasks:\n  - id: probe  # the one task\n    exec:\n      command: [\"true\"]\n";
        let new = w1(old).expect("changes");
        assert!(new.contains("  id: t  # SLOT: kebab id"));
        assert!(new.contains("  probe:  # the one task"));
    }

    #[test]
    fn already_new_form_untouched() {
        let doc = "nika: v1\nworkflow:\n  id: t\ntasks:\n  probe:\n    exec:\n      command: [\"true\"]\n";
        assert!(w1(doc).is_none());
    }

    #[test]
    fn id_not_first_line_is_left_for_a_human() {
        // deliberate refusal: the id is not the item's first line — the
        // transform does not fire on that item (the parser teaches).
        let old = "nika: v1\nworkflow: t\ntasks:\n  - depends_on: []\n    id: probe\n";
        let new = w1(old).expect("workflow line still migrates");
        assert!(new.contains("    id: probe"), "ambiguous item untouched");
    }
}
