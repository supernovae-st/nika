// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! R5 « the predicates » codemod — `succeeded` → `success` ·
//! `failed` → `failure` in `after:` blocks (spec #118 ·
//! LAW-GRAMMAR-0231 · « Codemod 1:1 mécanique » · `skipped`/`terminal`
//! unchanged).
//!
//! Line-based and structure-aware: the flip touches ONLY the `after:`
//! map's VALUE position — flow style (`after: {a: succeeded}`) and
//! block style (`after:` plus indented `task: predicate` lines) —
//! comments, blank lines, `${{ }}` islands (a `when:` status comparison
//! is the DAG-007 class, never this one) and task KEYS are preserved
//! byte-for-byte, and the transform is IDEMPOTENT (a post-R5 document
//! returns `None`). It is the ONE repair `check --fix` applies when the
//! parser refuses a dead spelling — the old form is repairable, never
//! executable (there is no legacy parser path).
//!
//! Structure guards (the codemod never guesses): an `after:` opener
//! counts only INSIDE `tasks:` and indented under a task key (a task
//! literally named `after` or a top-level key never opens a block), a
//! block entry flips only when the value IS exactly the dead spelling
//! (bare or quoted, trailing comment preserved), and flow braces are
//! counted outside quotes so a `}` inside a string never leaks the
//! flow state into a sibling field.

/// The dead spellings and their R5 respellings (1:1, order-stable).
const DEAD: [(&str, &str); 2] = [("succeeded", "success"), ("failed", "failure")];

/// Apply the R5 predicate codemod. `Some(new)` when at least one dead
/// spelling was respelled, `None` when the document carries none
/// (idempotence by contract).
#[must_use]
pub fn predicates(source: &str) -> Option<String> {
    let mut out: Vec<String> = Vec::with_capacity(source.lines().count());
    let mut changed = false;
    let mut in_tasks = false;
    let mut task_indent: Option<usize> = None;
    let mut after_indent: Option<usize> = None;
    let mut flow_depth = 0_i32;

    for line in source.split('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push(line.to_owned());
            continue;
        }

        if flow_depth > 0 {
            let (new, n) = flip_flow_values(line);
            flow_depth += brace_delta(&new);
            changed |= n;
            out.push(new);
            continue;
        }

        // Top-level section tracking (col-0 keys only).
        if indent == 0 && trimmed.contains(':') {
            in_tasks = trimmed.starts_with("tasks:");
            task_indent = None;
            after_indent = None;
            out.push(line.to_owned());
            continue;
        }
        // A task key sets/resets the field-indent floor (and closes any
        // after: block — sibling fields share the task's indent). The
        // FIRST key line inside `tasks:` is a task key by position.
        if in_tasks && is_key_line(trimmed) && task_indent.is_none_or(|ti| indent <= ti) {
            task_indent = Some(indent);
            after_indent = None;
            out.push(line.to_owned());
            continue;
        }

        // `after:` opener — a task FIELD only (guarded per the module
        // doc): inside tasks:, indented under the task key.
        if in_tasks && task_indent.is_some_and(|ti| indent > ti) && is_after_opener(trimmed) {
            let rest = trimmed["after:".len()..].trim_start();
            if rest.starts_with('{') {
                let (new, n) = flip_flow_values(line);
                changed |= n;
                let depth = brace_delta(&new);
                if depth > 0 {
                    flow_depth = depth;
                }
                after_indent = None;
                out.push(new);
                continue;
            }
            after_indent = Some(indent);
            out.push(line.to_owned());
            continue;
        }

        // Inside an after: block — flip an exact dead-spelling VALUE.
        if let Some(ai) = after_indent {
            if indent <= ai {
                after_indent = None;
                out.push(line.to_owned());
                continue;
            }
            if let Some(new) = flip_block_value(line) {
                changed = true;
                out.push(new);
                continue;
            }
        }
        out.push(line.to_owned());
    }

    changed.then(|| out.join("\n"))
}

/// `after:` (block) or `after: {…` (flow) — the key itself, never a
/// longer word (`aftermath:`) and never a quoted scalar.
fn is_after_opener(trimmed: &str) -> bool {
    trimmed == "after:" || trimmed.starts_with("after: ") || trimmed.starts_with("after:\t")
}

/// A `key:` line (a task key — its value is empty, a flow head, or a
/// trailing comment); the key itself is one non-whitespace token (task
/// ids are `\S+`-shaped by the parser's own id grammar).
fn is_key_line(trimmed: &str) -> bool {
    match trimmed.find(':') {
        Some(i) => {
            let (k, rest) = (&trimmed[..i], trimmed[i + 1..].trim_start());
            !k.is_empty()
                && !k.contains(char::is_whitespace)
                && (rest.is_empty() || rest.starts_with('{') || rest.starts_with('#'))
        }
        None => false,
    }
}

/// `'{'` minus `'}'` outside quotes — the flow-depth delta of one line.
fn brace_delta(line: &str) -> i32 {
    let mut depth = 0_i32;
    let mut quote = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '{') => depth += 1,
            (None, '}') => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Flip dead-spelling VALUES in flow entries (`key: value` followed by
/// `,` / `}` / end-of-line) on one line. Returns the line and whether
/// it changed.
fn flip_flow_values(line: &str) -> (String, bool) {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut changed = false;
    let mut i = 0_usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        out.push(c);
        i += 1;
        if c != ':' {
            continue;
        }
        // Value head: whitespace, then an optional quote, then the word.
        let ws = i;
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        let (quote, word_start) = match bytes.get(i).map(|b| *b as char) {
            Some(q @ ('\'' | '"')) => (Some(q), i + 1),
            _ => (None, i),
        };
        let mut end = word_start;
        while end < bytes.len() && ((bytes[end] as char).is_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let word = &line[word_start..end];
        let after_word = if quote.is_some() { end + 1 } else { end };
        let closes = quote.is_none_or(|q| line[end..].starts_with(q));
        let terminator_ok = closes
            && matches!(
                line[after_word.min(line.len())..]
                    .trim_start()
                    .chars()
                    .next(),
                None | Some(',' | '}')
            );
        if let Some((_, to)) = DEAD.iter().find(|(dead, _)| *dead == word)
            && terminator_ok
        {
            out.push_str(&line[ws..word_start]);
            out.push_str(to);
            changed = true;
            i = end;
            continue;
        }
        out.push_str(&line[ws..end]);
        i = end;
    }
    (out, changed)
}

/// Flip a block-form entry `key: succeeded  # comment` — only when the
/// value IS exactly the dead spelling (bare or quoted). Returns the
/// rewritten line.
fn flip_block_value(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let value = line[colon + 1..].trim_start();
    for (dead, to) in DEAD {
        for (open, close) in [("", ""), ("'", "'"), ("\"", "\"")] {
            let token = format!("{open}{dead}{close}");
            if let Some(rest) = value.strip_prefix(token.as_str())
                && (rest.is_empty()
                    || rest.starts_with(char::is_whitespace) && rest.trim_start().starts_with('#')
                    || rest.trim().is_empty())
            {
                let head = &line[..=colon];
                let gap = &line[colon + 1..line.len() - value.len()];
                return Some(format!("{head}{gap}{open}{to}{close}{rest}"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_same_line_flips_every_entry_and_keeps_the_shape() {
        let src = "tasks:\n  a:\n    exec: { command: [true] }\n  b:\n    after: { a: succeeded }\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("after: { a: success }"), "{out}");
        assert!(!out.contains("succeeded"), "{out}");
    }

    #[test]
    fn flow_multi_entry_and_failure_flip() {
        let src =
            "tasks:\n  b:\n    after: { a: failed, c: succeeded }\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("after: { a: failure, c: success }"), "{out}");
    }

    #[test]
    fn block_form_flips_and_preserves_comments_and_order() {
        let src = "tasks:\n  b:\n    after:\n      a: succeeded   # the gate\n      c: failed\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("a: success   # the gate"), "{out}");
        assert!(out.contains("c: failure\n"), "{out}");
    }

    #[test]
    fn quoted_values_flip_preserving_the_quotes() {
        let src = "tasks:\n  b:\n    after:\n      a: 'succeeded'\n      c: \"failed\"\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("a: 'success'"), "{out}");
        assert!(out.contains("c: \"failure\""), "{out}");
    }

    #[test]
    fn when_status_comparisons_are_never_touched() {
        // The DAG-007 class is another law's object (the did-you-mean
        // died spec-side · LAW-GRAMMAR-0231) — the codemod is after:-only.
        let src = "tasks:\n  b:\n    with: { s: \"${{ tasks.a.status }}\" }\n    when: ${{ with.s == 'failed' }}\n    exec: { command: [true] }\n";
        assert_eq!(predicates(src), None);
    }

    #[test]
    fn a_task_named_succeeded_keeps_its_key() {
        let src = "tasks:\n  succeeded:\n    exec: { command: [true] }\n  b:\n    after: { succeeded: succeeded }\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("  succeeded:\n"), "{out}");
        assert!(out.contains("after: { succeeded: success }"), "{out}");
    }

    #[test]
    fn a_task_named_after_never_opens_a_block() {
        let src = "tasks:\n  after:\n    exec: { command: [true] }\n    shell: failed\n";
        assert_eq!(predicates(src), None);
    }

    #[test]
    fn prose_and_body_strings_are_never_touched() {
        let src = "tasks:\n  b:\n    after: { a: success }\n    infer: { prompt: \"run when the build failed or succeeded\" }\n";
        assert_eq!(predicates(src), None, "already R5 + prose strings");
    }

    #[test]
    fn idempotence_a_migrated_document_returns_none() {
        let src = "tasks:\n  b:\n    after:\n      a: succeeded\n    exec: { command: [true] }\n";
        let once = predicates(src).expect("changed");
        assert_eq!(predicates(&once), None);
    }

    #[test]
    fn multiline_flow_flips_and_closes() {
        let src = "tasks:\n  b:\n    after: {\n      a: succeeded,\n      c: failed }\n    exec: { command: [true] }\n";
        let out = predicates(src).expect("changed");
        assert!(out.contains("a: success,"), "{out}");
        assert!(out.contains("c: failure }"), "{out}");
        assert!(out.contains("exec: { command: [true] }"), "{out}");
    }

    #[test]
    fn comment_lines_and_block_scalars_are_left_alone() {
        let src = "# after: {a: succeeded}\ntasks:\n  b:\n    after: { a: success }\n    exec:\n      shell: |\n        echo succeeded\n";
        assert_eq!(predicates(src), None);
    }
}
