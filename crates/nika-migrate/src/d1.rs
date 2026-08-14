// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! D1 « the split » codemod (#572) — the pre-0.103 string `command:`
//! (an IMPLICIT shell) becomes `shell:` verbatim or the argv flow form.
//! Split out of `lib.rs` at the 1500-file wall (ADR-023).

// ── D1 « the split » (#572) ───────────────────────────────────────────
// The pre-0.103 string `command:` — an IMPLICIT shell — becomes
// `shell:` VERBATIM (the same decoded string reaches /bin/sh -c, so
// semantics are byte-identical), or the argv flow form when every token
// is provably inert (no character a shell could reinterpret). The
// parser's `D1StringCommand` refusal is what the fix ladder matches;
// this codemod is the repair it names.

/// The D1 repair outcome (equivalence-or-stop — the W2 shape).
pub enum D1Outcome {
    /// Mechanically migrated (semantics preserved by construction).
    Changed(String),
    /// Nothing mechanical — each note names the case and the route.
    Stop(Vec<String>),
}

/// The D1 codemod (#572): every string `command:` inside an `exec:`
/// block migrates — the block/quoted/templated forms rename the KEY to
/// `shell:` (verbatim value, identical semantics), a bare string of
/// provably-inert tokens becomes the argv flow form (the safer reading
/// the grammar prefers). `command:` keys OUTSIDE an exec block (an
/// `invoke:` arg named `command`, say) are never touched — the parser
/// only refuses the exec one, the codemod keeps the same scope.
#[must_use]
pub fn d1(source: &str) -> D1Outcome {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut stops: Vec<String> = Vec::new();
    let mut exec_col: Option<usize> = None; // key column of the open `exec:` block
    for (ix, l) in lines.iter().enumerate() {
        let l = *l;
        let trimmed = l.trim_start();
        // A dedent closes the open exec block (blank/comment lines never do).
        if let Some(e) = exec_col
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && key_col(l) <= e
        {
            exec_col = None;
        }
        if exec_col.is_none() && exec_key_col(l).is_some() {
            exec_col = exec_key_col(l);
            // The flow form carries the command on the exec line itself:
            // `exec: { command: "ls" }` — rename the key in place when
            // the value is a scalar (a `[` sequence is already legal).
            if let Some(rewritten) = rewrite_flow_exec_line(l) {
                out.push(rewritten);
                changed = true;
            } else {
                out.push((*l).to_owned());
            }
            continue;
        }
        if exec_col.is_some()
            && let Some((after, indent)) = command_value(l)
        {
            match rewrite_command_value(after, indent, lines.get(ix + 1).copied()) {
                CommandRewrite::Leave => {}
                CommandRewrite::Shell(newline) | CommandRewrite::Argv(newline) => {
                    out.push(newline);
                    changed = true;
                    continue;
                }
                CommandRewrite::Stop(note) => {
                    stops.push(note);
                }
            }
        }
        out.push((*l).to_owned());
    }
    if !stops.is_empty() {
        return D1Outcome::Stop(stops);
    }
    if !changed {
        return D1Outcome::Stop(vec![
            "[D1] no string `command:` found under an `exec:` block — nothing mechanical"
                .to_owned(),
        ]);
    }
    D1Outcome::Changed(out.join("\n"))
}

/// One line's rewrite verdict.
enum CommandRewrite {
    /// Not a string command (a legal argv form) — leave the line.
    Leave,
    /// Rename the key to `shell:` (verbatim value — semantics identical).
    Shell(String),
    /// Rewrite to the argv flow form (provably inert tokens only).
    Argv(String),
    /// A human decides — the note names the case.
    Stop(String),
}

/// The column of a line's KEY, riding past an optional `- ` list marker
/// (a block sequence item's key sits two columns right of the dash).
fn key_col(line: &str) -> usize {
    let trimmed = line.trim_start();
    let lead = line.len() - trimmed.len();
    lead + usize::from(trimmed.starts_with("- ")) * 2
}

/// The column of an `exec:` key when this line opens one (`exec:` ·
/// `- exec:`), `None` otherwise.
fn exec_key_col(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let key = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    (key == "exec:" || key.starts_with("exec: {")).then(|| key_col(line))
}

/// The value text after a `command:` key (and the key's own column),
/// when this line IS one. `None` for comments/keys merely CONTAINING
/// the word.
fn command_value(line: &str) -> Option<(&str, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let key = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let rest = key.strip_prefix("command:")?;
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('#') {
        return Some((rest.trim_start(), key_col(line)));
    }
    None
}

/// Rewrite one block-form `command:` line per its value's shape
/// (`next` is the following line — the empty-value lookahead).
fn rewrite_command_value(after: &str, indent: usize, next: Option<&str>) -> CommandRewrite {
    let pad = " ".repeat(indent);
    // An empty value rides the FOLLOWING line: a DEEPER block-sequence
    // item is the legal argv form; a mapping or a null is not the
    // implicit-shell string at all — a human decides, never guess.
    if after.is_empty() {
        let seq = next.is_some_and(|n| {
            let t = n.trim_start();
            t.starts_with("- ") && n.len() - t.len() > indent
        });
        return if seq {
            CommandRewrite::Leave
        } else {
            CommandRewrite::Stop(
                "[D1] `command:` with no scalar value (a mapping · null) is not the implicit-shell form — rewrite it by hand".to_owned(),
            )
        };
    }
    // A flow mapping (`command: {x: 1}`) is not the string form either.
    if after.starts_with('{') {
        return CommandRewrite::Stop(
            "[D1] `command:` carries a flow mapping — not the implicit-shell string; rewrite it by hand".to_owned(),
        );
    }
    // A flow sequence (`command: ["prog", …]`) is the legal argv form.
    if after.starts_with('[') {
        return CommandRewrite::Leave;
    }
    // A block scalar (`command: |` · `>` × chomping/indent indicators ·
    // an optional trailing comment): the content lines follow the key —
    // rename the key, the block rides untouched.
    let head = after.split(" #").next().unwrap_or(after).trim_end();
    let mut chars = head.chars();
    if matches!(chars.next(), Some('|' | '>')) && chars.all(|c| matches!(c, '+' | '-' | '1'..='9'))
    {
        return CommandRewrite::Shell(format!("{pad}shell: {after}"));
    }
    // A quoted scalar keeps its quotes — the key rename is the whole
    // repair (the decoded string is the one the implicit shell ran).
    if after.starts_with('"') || after.starts_with('\'') {
        return CommandRewrite::Shell(format!("{pad}shell: {after}"));
    }
    // A bare scalar: a templated island re-splits under the shell's
    // rules — the verbatim `shell:` rename preserves the old semantics
    // exactly; never guess the author's quoting intent.
    let (value, comment) = match after.split_once(" #") {
        Some((v, c)) => (v.trim_end(), format!(" #{c}")),
        None => (after.trim_end(), String::new()),
    };
    if value.contains("${{") {
        return CommandRewrite::Shell(format!("{pad}shell: {after}"));
    }
    let tokens: Vec<&str> = value.split_whitespace().collect();
    if !tokens.is_empty() && tokens.iter().all(|t| inert_token(t)) {
        let argv = tokens
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ");
        return CommandRewrite::Argv(format!("{pad}command: [{argv}]{comment}"));
    }
    // Shell-meta characters (pipe · redirect · glob · expansion …): the
    // old form WAS a shell line — name it, never reinterpret it.
    CommandRewrite::Shell(format!("{pad}shell: {after}"))
}

/// A token no shell can reinterpret: letters · digits · `_ . , / + : @ % -`
/// only — no expansions (`$` `` ` `` `~`), no globs (`* ? [ ] { }`), no
/// operators (`| & ; < > ( )`), no quoting/escaping (`" ' \`), no `=`
/// (a leading `VAR=val` would be an ASSIGNMENT under the shell — an
/// argv element never is). Conservative by design: when in doubt the
/// value rides `shell:` verbatim.
fn inert_token(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '.' | ',' | '/' | '+' | ':' | '@' | '%' | '-')
        })
}

/// The flow form `exec: { command: "…" }` — the key rename happens
/// INSIDE the line. Only a scalar value is the D1 case (a `[` argv
/// sequence is legal already); anything else leaves the line alone.
fn rewrite_flow_exec_line(line: &str) -> Option<String> {
    let open = line.find("exec: {")?;
    let rest = &line[open + "exec: {".len()..];
    let cmd_at = rest.find("command:")?;
    let value = rest[cmd_at + "command:".len()..].trim_start();
    if value.starts_with('[') {
        return None; // already argv
    }
    let mut rewritten = String::with_capacity(line.len());
    rewritten.push_str(&line[..open + "exec: {".len() + cmd_at]);
    rewritten.push_str("shell:");
    rewritten.push_str(&rest[cmd_at + "command:".len()..]);
    Some(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── D1 « the split » (#572) ───────────────────────────────────────

    /// The issue's exact ask: the 0.102 string command is the finding
    /// whose repair IS mechanical.
    #[test]
    fn d1_bare_inert_tokens_become_argv() {
        let old = "nika: v1\nworkflow: t\ntasks:\n  a:\n    exec:\n      command: cargo build --release\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("an inert bare string migrates to argv")
        };
        assert_eq!(
            new,
            "nika: v1\nworkflow: t\ntasks:\n  a:\n    exec:\n      command: [\"cargo\", \"build\", \"--release\"]\n"
        );
        assert!(
            matches!(d1(&new), D1Outcome::Stop(_)),
            "idempotent — no string command left: {new}"
        );
    }

    #[test]
    fn d1_shell_meta_renames_the_key_verbatim() {
        // Pipes/redirects/globs/expansions: the old form WAS a shell
        // line — name it, never reinterpret it.
        for value in [
            "echo a | grep b",
            "ls > /tmp/x",
            "echo $HOME",
            "rm -r *.log",
            "FOO=bar make",
            "echo ${{ vars.x }}",
        ] {
            let old = format!("workflow: t\ntasks:\n  a:\n    exec:\n      command: {value}\n");
            let D1Outcome::Changed(new) = d1(&old) else {
                panic!("{value} renames to shell:")
            };
            assert!(
                new.contains(&format!("      shell: {value}\n")),
                "{value} verbatim under shell:: {new}"
            );
        }
    }

    #[test]
    fn d1_quoted_and_block_forms_rename_verbatim() {
        // A quoted scalar: the decoded string is the one the implicit
        // shell ran — the key rename is the whole repair.
        let old = "workflow: t\ntasks:\n  a:\n    exec:\n      command: \"echo a | grep b\"\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("quoted renames")
        };
        assert!(new.contains("      shell: \"echo a | grep b\"\n"), "{new}");
        // A block scalar: the content rides the renamed key untouched.
        let old = "workflow: t\ntasks:\n  a:\n    exec:\n      command: |\n        echo one\n        echo two\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("block scalar renames")
        };
        assert_eq!(
            new,
            "workflow: t\ntasks:\n  a:\n    exec:\n      shell: |\n        echo one\n        echo two\n"
        );
    }

    #[test]
    fn d1_keeps_the_trailing_comment() {
        let old = "workflow: t\ntasks:\n  a:\n    exec:\n      command: make build  # the build\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("migrates")
        };
        assert!(
            new.contains("command: [\"make\", \"build\"] # the build\n"),
            "{new}"
        );
    }

    #[test]
    fn d1_never_touches_a_command_outside_exec() {
        // An invoke arg NAMED `command` is data, not the dead form.
        let old = "workflow: t\ntasks:\n  a:\n    invoke:\n      tool: \"mcp:sh\"\n      args:\n        command: echo hi\n";
        assert!(
            matches!(d1(old), D1Outcome::Stop(_)),
            "the invoke arg stays put"
        );
        // And a legal argv command under exec is left alone.
        let legal = "workflow: t\ntasks:\n  a:\n    exec:\n      command: [\"echo\", \"hi\"]\n";
        assert!(matches!(d1(legal), D1Outcome::Stop(_)), "already legal");
    }

    #[test]
    fn d1_flow_exec_renames_inside_the_line() {
        let old = "workflow: t\ntasks:\n  a:\n    exec: { command: \"ls -la\" }\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("flow renames")
        };
        assert!(new.contains("exec: { shell: \"ls -la\" }"), "{new}");
        // A flow argv is already legal — no D1 there.
        let legal = "workflow: t\ntasks:\n  a:\n    exec: { command: [\"ls\"] }\n";
        assert!(matches!(d1(legal), D1Outcome::Stop(_)));
    }

    #[test]
    fn d1_reaches_a_cleanup_task_too() {
        // A cleanup is an ordinary task joined by `after: { a: unwind }`
        // — D1 scopes by the `exec:` block, never by the task's role,
        // so it reaches this one at the same indent as any other.
        let old = "nika: t\ntasks:\n  a:\n    exec:\n      command: [\"make\"]\n  a_cleanup:\n    after: { a: unwind }\n    exec:\n      command: docker stop db\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("the cleanup migrates")
        };
        assert!(
            new.contains("      command: [\"docker\", \"stop\", \"db\"]\n"),
            "{new}"
        );
        assert!(new.contains("      command: [\"make\"]\n"), "{new}");
    }

    #[test]
    fn d1_several_string_commands_migrate_in_one_pass() {
        let old = "workflow: t\ntasks:\n  a:\n    exec:\n      command: cargo build\n  b:\n    exec:\n      command: echo done | tee log\n";
        let D1Outcome::Changed(new) = d1(old) else {
            panic!("both migrate")
        };
        assert!(new.contains("command: [\"cargo\", \"build\"]\n"), "{new}");
        assert!(new.contains("shell: echo done | tee log\n"), "{new}");
    }

    #[test]
    fn d1_a_mapping_or_null_value_stops_honestly() {
        // The parser's D1 arm refuses ANY non-sequence — a flow mapping
        // or a null is not the implicit-shell string, and no codemod
        // should guess what the author meant.
        let old = "workflow: t\ntasks:\n  a:\n    exec:\n      command: {x: 1}\n";
        let D1Outcome::Stop(notes) = d1(old) else {
            panic!("a flow mapping is not mechanical")
        };
        assert!(notes[0].contains("flow mapping"), "{notes:?}");
        let null = "workflow: t\ntasks:\n  a:\n    exec:\n      command:\n";
        let D1Outcome::Stop(notes) = d1(null) else {
            panic!("a null value is not mechanical")
        };
        assert!(notes[0].contains("no scalar value"), "{notes:?}");
        // …but the empty value followed by a DEEPER sequence is the
        // legal block-argv form — left alone, nothing to do.
        let legal =
            "workflow: t\ntasks:\n  a:\n    exec:\n      command:\n        - echo\n        - hi\n";
        let D1Outcome::Stop(notes) = d1(legal) else {
            panic!("nothing to migrate")
        };
        assert!(notes[0].contains("nothing mechanical"), "{notes:?}");
    }
}
