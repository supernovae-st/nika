// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! R1 « the identity » codemod — the fourteen-key envelope's identity
//! block becomes the nine-key one's single key.
//!
//! ```yaml
//! # before                          # after
//! nika: v1                          # Parse one booking into the import shape.
//! workflow:                         nika: booking-parse
//!   id: booking-parse
//!   description: "Parse one booking into the import shape."
//! ```
//!
//! Three source shapes are recognised, all top-level: the block
//! (`workflow:` + indented `id:` / `description:`), the pre-W1 scalar
//! (`workflow: <id>`), and the one-line flow form
//! (`workflow: { id: x, description: y }`); a bare top-level
//! `description:` (the pre-W1 hoist era) is folded the same way. The
//! `nika:` line is REWRITTEN in place (its trailing comment kept) and the
//! description prose is DEMOTED to `#` comment lines directly above it —
//! never dropped, never guessed. Everything else is byte-identical.
//!
//! Equivalence-or-stop, like every rung of the ladder: the migration
//! applies only when the answer is forced. `nika:` already naming
//! something other than the old version literal while `workflow.id` names
//! another thing is TWO names in one file (STOP · only the author knows
//! which one is the identity); a `workflow:` block carrying a key other
//! than `id`/`description` has no destination the parser admits (STOP);
//! an id that is not kebab-case would only move the refusal from one key
//! to another (STOP · the parser's `BadNikaId` teaching then names the
//! grammar); a block without an `id:` has nothing to move (STOP). A
//! document already in the nine-key shape returns [`IdentityOutcome::Clean`]
//! — the transform is idempotent by contract.

/// The outcome of one identity migration pass over one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityOutcome {
    /// Mechanically migrated (the identity moved · the prose demoted).
    Changed(String),
    /// Nothing to migrate: no `workflow:` block, no bare `description:`.
    Clean,
    /// Ambiguous or non-mechanical — each diagnostic names the case.
    Stop(Vec<String>),
}

/// A top-level line: its index and the text after the `key:`.
struct TopLevel<'a> {
    idx: usize,
    key: &'a str,
    rest: &'a str,
}

/// The kebab-case grammar the parser admits for `nika: <id>`
/// (`^[a-z][a-z0-9-]*$` · spec 01 §nika).
fn is_kebab_id(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Strip one layer of matching quotes.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Split `key: rest` on the FIRST colon of an unindented, non-comment line.
fn top_level(idx: usize, line: &str) -> Option<TopLevel<'_>> {
    if line.starts_with(' ')
        || line.starts_with('\t')
        || line.starts_with('#')
        || line.trim().is_empty()
    {
        return None;
    }
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty() || key.contains(' ') || key.contains('"') {
        return None;
    }
    // `key:` must be followed by end, a space, or a flow opener — `a:b`
    // is a scalar, not a mapping key.
    let rest = &line[colon + 1..];
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('{')) {
        return None;
    }
    Some(TopLevel {
        idx,
        key,
        rest: rest.trim_start(),
    })
}

/// The lines of an indented block under `start` (exclusive), up to but
/// not including the next top-level line. Blank lines inside the block
/// belong to it.
fn block_end(lines: &[&str], start: usize) -> usize {
    let mut end = start + 1;
    while end < lines.len() {
        let l = lines[end];
        if l.trim().is_empty() || l.starts_with(' ') || l.starts_with('\t') {
            end += 1;
        } else {
            break;
        }
    }
    // Trailing blank lines are NOT part of the block (they separate it
    // from what follows and stay in the file).
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    end
}

/// The parsed identity block: the id and the description lines (already
/// unquoted / dedented · one string per output comment line).
struct Identity {
    id: Option<String>,
    description: Vec<String>,
}

/// Parse `workflow: { id: x, description: y }` — one line, no nesting.
fn parse_flow(rest: &str, notes: &mut Vec<String>) -> Option<Identity> {
    let inner = rest.trim().strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains('{') || inner.contains('[') {
        notes.push(
            "`workflow: {…}` carries a nested value — migrate the identity by hand (`nika: <id>` · the description as a `#` comment)"
                .to_owned(),
        );
        return None;
    }
    let mut out = Identity {
        id: None,
        description: Vec::new(),
    };
    for entry in inner.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((k, v)) = entry.split_once(':') else {
            notes.push(format!(
                "`workflow: {{…}}` entry `{entry}` is not `key: value` — migrate the identity by hand"
            ));
            return None;
        };
        match k.trim() {
            "id" => out.id = Some(unquote(v).to_owned()),
            "description" => out.description = vec![unquote(v).to_owned()],
            other => {
                notes.push(format!(
                    "`workflow:` carries `{other}:` — the nine-key envelope has no home for it (only `id` and `description` moved) · migrate by hand"
                ));
                return None;
            }
        }
    }
    Some(out)
}

/// Whether a scalar opens a YAML block scalar (`|` · `>` and their chomping forms).
fn opens_block_scalar(value: &str) -> bool {
    matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+")
}

/// The prose lines of a block scalar whose header sits at `header_idx`
/// (deeper-indented lines that follow · blank lines allowed inside),
/// dedented to their common indent · plus the index of the first line
/// after the scalar.
fn block_scalar_lines(lines: &[&str], header_idx: usize, end: usize) -> (Vec<String>, usize) {
    let header = lines[header_idx];
    let base = header.len() - header.trim_start().len();
    let mut next = header_idx + 1;
    while next < end {
        let candidate = lines[next];
        let indent = candidate.len() - candidate.trim_start().len();
        if candidate.trim().is_empty() || indent > base {
            next += 1;
        } else {
            break;
        }
    }
    let body: Vec<&str> = lines[header_idx + 1..next]
        .iter()
        .copied()
        .filter(|candidate| !candidate.trim().is_empty())
        .collect();
    let min_indent = body
        .iter()
        .map(|candidate| candidate.len() - candidate.trim_start().len())
        .min()
        .unwrap_or(0);
    let prose = body
        .iter()
        .map(|candidate| candidate[min_indent..].trim_end().to_owned())
        .collect();
    (prose, next)
}

/// Parse the indented `workflow:` block · `id:` and `description:` only.
fn parse_block(
    lines: &[&str],
    start: usize,
    end: usize,
    notes: &mut Vec<String>,
) -> Option<Identity> {
    let mut out = Identity {
        id: None,
        description: Vec::new(),
    };
    let mut cursor = start + 1;
    while cursor < end {
        let line = lines[cursor].trim();
        if line.is_empty() || line.starts_with('#') {
            cursor += 1;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            notes.push(format!(
                "`workflow:` block line `{line}` is not `key: value` — migrate the identity by hand"
            ));
            return None;
        };
        let value = value.trim();
        match key.trim() {
            "id" => {
                out.id = Some(unquote(value).to_owned());
                cursor += 1;
            }
            "description" if opens_block_scalar(value) => {
                let (prose, next) = block_scalar_lines(lines, cursor, end);
                out.description = prose;
                cursor = next;
            }
            "description" => {
                out.description = vec![unquote(value).to_owned()];
                cursor += 1;
            }
            other => {
                notes.push(format!(
                    "`workflow:` carries `{other}:` — the nine-key envelope has no home for it (only `id` and `description` moved) · migrate by hand"
                ));
                return None;
            }
        }
    }
    Some(out)
}

/// What the source carries · the identity block, the line ranges to
/// remove, and the `nika:` line (index · value · trailing comment).
struct Collected {
    block: Identity,
    removals: Vec<(usize, usize)>,
    nika_idx: usize,
    nika_value: String,
    nika_comment: Option<String>,
    had_workflow: bool,
}

/// Read the identity block(s) out of the top-level lines · `Err(notes)`
/// on a STOP · `Ok(None)` when there is nothing to migrate (Clean).
fn collect(lines: &[&str], tops: &[TopLevel<'_>]) -> Result<Option<Collected>, Vec<String>> {
    let nika = tops.iter().find(|top| top.key == "nika");
    let workflow = tops.iter().find(|top| top.key == "workflow");
    let bare_description = tops.iter().find(|top| top.key == "description");
    if workflow.is_none() && bare_description.is_none() {
        return Ok(None);
    }
    let mut notes = Vec::new();
    let mut block = Identity {
        id: None,
        description: Vec::new(),
    };
    let mut removals: Vec<(usize, usize)> = Vec::new();

    if let Some(wf) = workflow {
        let parsed = if wf.rest.starts_with('{') {
            removals.push((wf.idx, wf.idx + 1));
            parse_flow(wf.rest, &mut notes)
        } else if wf.rest.is_empty() || wf.rest.starts_with('#') {
            let end = block_end(lines, wf.idx);
            removals.push((wf.idx, end));
            parse_block(lines, wf.idx, end, &mut notes)
        } else {
            // the pre-W1 scalar · `workflow: <id>` (a trailing comment allowed)
            let scalar = wf.rest.split('#').next().unwrap_or("").trim();
            removals.push((wf.idx, wf.idx + 1));
            Some(Identity {
                id: Some(unquote(scalar).to_owned()),
                description: Vec::new(),
            })
        };
        block = parsed.ok_or_else(|| notes.clone())?;
    }

    if let Some(bare) = bare_description {
        if !block.description.is_empty() {
            notes.push(
                "two descriptions (a bare top-level `description:` AND `workflow.description`) — keep one by hand as the `#` comment above `nika:`"
                    .to_owned(),
            );
            return Err(notes);
        }
        let value = bare.rest.split('#').next().unwrap_or("").trim();
        if opens_block_scalar(value) {
            let end = block_end(lines, bare.idx);
            let (prose, _) = block_scalar_lines(lines, bare.idx, end);
            block.description = prose;
            removals.push((bare.idx, end));
        } else {
            block.description = vec![unquote(value).to_owned()];
            removals.push((bare.idx, bare.idx + 1));
        }
    }

    let Some(nika) = nika else {
        notes.push(
            "no top-level `nika:` line — the identity has no key to move onto · write `nika: <id>` by hand"
                .to_owned(),
        );
        return Err(notes);
    };
    let nika_value = unquote(nika.rest.split('#').next().unwrap_or("").trim()).to_owned();
    let nika_comment = nika.rest.find('#').map(|at| nika.rest[at..].to_owned());
    Ok(Some(Collected {
        block,
        removals,
        nika_idx: nika.idx,
        nika_value,
        nika_comment,
        had_workflow: workflow.is_some(),
    }))
}

/// The one forced answer for the id · or the STOP that names why not.
fn resolve_id(collected: &mut Collected) -> Result<String, Vec<String>> {
    let id = if let Some(id) = collected.block.id.take() {
        id
    } else {
        // A bare description with an already-named `nika:` · the name
        // stands · only the prose moves.
        let named = !collected.had_workflow
            && !collected.nika_value.is_empty()
            && !is_version_literal(&collected.nika_value);
        if !named {
            return Err(vec![
                "the `workflow:` block carries no `id:` — there is nothing to move onto `nika:` · choose the name by hand"
                    .to_owned(),
            ]);
        }
        collected.nika_value.clone()
    };
    if !is_kebab_id(&id) {
        return Err(vec![format!(
            "`workflow.id` `{id}` is not a kebab-case name (`^[a-z][a-z0-9-]*$`) — moving it onto `nika:` would only move the refusal · choose the name by hand"
        )]);
    }
    if !(is_version_literal(&collected.nika_value) || collected.nika_value == id) {
        return Err(vec![format!(
            "`nika:` already names `{}` while `workflow.id` names `{id}` — one file, two names · keep one by hand",
            collected.nika_value
        )]);
    }
    Ok(id)
}

/// Rebuild the document · byte-identical outside the touched lines.
fn render(lines: &[&str], collected: &Collected, id: &str) -> String {
    let removed = |idx: usize| {
        collected
            .removals
            .iter()
            .any(|(start, end)| idx >= *start && idx < *end)
    };
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + collected.block.description.len());
    for (idx, line) in lines.iter().enumerate() {
        if removed(idx) {
            continue;
        }
        if idx == collected.nika_idx {
            for prose in &collected.block.description {
                if prose.is_empty() {
                    out.push("#".to_owned());
                } else {
                    out.push(format!("# {prose}"));
                }
            }
            match &collected.nika_comment {
                Some(comment) => out.push(format!("nika: {id}  {comment}")),
                None => out.push(format!("nika: {id}")),
            }
            continue;
        }
        out.push((*line).to_owned());
    }
    out.join("\n")
}

/// The R1 identity migration. See the module doc for the contract.
#[must_use]
pub fn identity(source: &str) -> IdentityOutcome {
    let lines: Vec<&str> = source.split('\n').collect();
    let tops: Vec<TopLevel<'_>> = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| top_level(idx, line))
        .collect();
    let mut collected = match collect(&lines, &tops) {
        Ok(Some(collected)) => collected,
        Ok(None) => return IdentityOutcome::Clean,
        Err(notes) => return IdentityOutcome::Stop(notes),
    };
    let id = match resolve_id(&mut collected) {
        Ok(id) => id,
        Err(notes) => return IdentityOutcome::Stop(notes),
    };
    IdentityOutcome::Changed(render(&lines, &collected, &id))
}

/// The old era's `nika:` values were version literals (`v1` · `v2` · a
/// quoted `"1"`) — those are safe to overwrite; anything else is a name.
fn is_version_literal(s: &str) -> bool {
    let s = s.trim();
    s.is_empty()
        || (s.starts_with('v')
            && s[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
            && s.len() > 1)
        || s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(src: &str) -> String {
        match identity(src) {
            IdentityOutcome::Changed(s) => s,
            other => panic!("expected Changed · got {other:?}"),
        }
    }

    fn stop(src: &str) -> Vec<String> {
        match identity(src) {
            IdentityOutcome::Stop(n) => n,
            other => panic!("expected Stop · got {other:?}"),
        }
    }

    #[test]
    fn the_block_form_moves_the_id_and_demotes_the_prose() {
        let src = "nika: v1\nworkflow:\n  id: hello\n  description: \"says hi\"\n\ntasks:\n  t:\n    exec: { shell: \"true\" }\n";
        let out = changed(src);
        assert_eq!(
            out,
            "# says hi\nnika: hello\n\ntasks:\n  t:\n    exec: { shell: \"true\" }\n"
        );
        assert_eq!(
            identity(&out),
            IdentityOutcome::Clean,
            "idempotent · a migrated document is Clean"
        );
    }

    #[test]
    fn a_block_without_description_leaves_no_comment() {
        let src = "nika: v1\nworkflow:\n  id: hello\ntasks:\n  t:\n    exec: { shell: \"true\" }\n";
        assert_eq!(
            changed(src),
            "nika: hello\ntasks:\n  t:\n    exec: { shell: \"true\" }\n"
        );
    }

    #[test]
    fn the_scalar_and_flow_forms_are_recognised() {
        let scalar = "nika: v1\nworkflow: hello\ntasks: {}\n";
        assert_eq!(changed(scalar), "nika: hello\ntasks: {}\n");
        let flow = "nika: v1\nworkflow: { id: hello, description: \"says hi\" }\ntasks: {}\n";
        assert_eq!(changed(flow), "# says hi\nnika: hello\ntasks: {}\n");
    }

    #[test]
    fn a_block_scalar_description_becomes_one_comment_per_line() {
        let src = "nika: v1\nworkflow:\n  id: hello\n  description: |\n    first line\n    second line\ntasks: {}\n";
        assert_eq!(
            changed(src),
            "# first line\n# second line\nnika: hello\ntasks: {}\n"
        );
    }

    #[test]
    fn a_bare_top_level_description_folds_the_same_way() {
        let src = "nika: v1\nworkflow: hello\ndescription: \"the pre-W1 hoist era\"\ntasks: {}\n";
        assert_eq!(
            changed(src),
            "# the pre-W1 hoist era\nnika: hello\ntasks: {}\n"
        );
        // an already-named `nika:` with only a bare description · the name stands
        let named = "nika: hello\ndescription: prose\ntasks: {}\n";
        assert_eq!(changed(named), "# prose\nnika: hello\ntasks: {}\n");
    }

    #[test]
    fn everything_else_is_byte_identical_including_comments_and_the_modeline() {
        let src = "# SPDX-License-Identifier: Apache-2.0\n# yaml-language-server: $schema=https://nika.sh/spec/v1/workflow.schema.json\n#\n# a header comment\nnika: v1  # the old marker\nworkflow:\n  id: hello\n  # a comment inside the block\n  description: 'says hi'\n\nmodel: mock/echo\n\ntasks:\n  t:\n    exec: { shell: \"true\" }\n";
        let out = changed(src);
        assert_eq!(
            out,
            "# SPDX-License-Identifier: Apache-2.0\n# yaml-language-server: $schema=https://nika.sh/spec/v1/workflow.schema.json\n#\n# a header comment\n# says hi\nnika: hello  # the old marker\n\nmodel: mock/echo\n\ntasks:\n  t:\n    exec: { shell: \"true\" }\n"
        );
    }

    #[test]
    fn a_nine_key_document_is_clean() {
        assert_eq!(identity("nika: hello\ntasks: {}\n"), IdentityOutcome::Clean);
        assert_eq!(
            identity("# prose\nnika: hello\nmodel: mock/echo\ntasks: {}\n"),
            IdentityOutcome::Clean
        );
    }

    #[test]
    fn two_names_in_one_file_stop() {
        let notes = stop("nika: other-name\nworkflow:\n  id: hello\ntasks: {}\n");
        assert!(notes[0].contains("two names"), "{notes:?}");
    }

    #[test]
    fn a_foreign_key_in_the_block_stops() {
        let notes = stop("nika: v1\nworkflow:\n  id: hello\n  version: 3\ntasks: {}\n");
        assert!(notes[0].contains("`version:`"), "{notes:?}");
    }

    #[test]
    fn a_non_kebab_id_stops_instead_of_moving_the_refusal() {
        let notes = stop("nika: v1\nworkflow:\n  id: My_Flow\ntasks: {}\n");
        assert!(notes[0].contains("kebab-case"), "{notes:?}");
    }

    #[test]
    fn a_block_without_an_id_stops() {
        let notes = stop("nika: v1\nworkflow:\n  description: prose only\ntasks: {}\n");
        assert!(notes[0].contains("no `id:`"), "{notes:?}");
    }

    #[test]
    fn a_missing_nika_line_stops() {
        let notes = stop("workflow:\n  id: hello\ntasks: {}\n");
        assert!(notes[0].contains("no top-level `nika:`"), "{notes:?}");
    }

    #[test]
    fn a_nested_flow_value_stops() {
        let notes = stop("nika: v1\nworkflow: { id: hello, meta: { a: 1 } }\ntasks: {}\n");
        assert!(notes[0].contains("nested"), "{notes:?}");
    }

    #[test]
    fn a_task_level_workflow_key_is_not_the_envelope() {
        // `workflow:` indented (an invoke child call) is not the identity block
        let src = "nika: hello\ntasks:\n  child:\n    invoke:\n      workflow: ./other.nika.yaml\n";
        assert_eq!(identity(src), IdentityOutcome::Clean);
    }
}
