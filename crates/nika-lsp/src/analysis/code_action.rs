// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `textDocument/codeAction` — the `check --fix` repair engine, projected.
//!
//! ONE fix engine, two surfaces: the CLI's `nika check --fix` splices the
//! checker's typed rename suggestions (`offending`/`suggestion` pairs);
//! this module projects the SAME pairs as LSP quickfixes, so every editor
//! (VS Code · Cursor · zed · helix · neovim) gets the one-click repair the
//! terminal already had. No second brain: the suggestions come from the
//! identical `parse` + `check` ladder the diagnostics run.
//!
//! Discipline (mirrors `--fix`):
//! - **typed did-you-mean only** — a finding without a `suggestion` offers
//!   nothing (silence beats a wrong guess).
//! - **unique-token only** — the edit targets the offending token's ONE
//!   occurrence (identifier-boundary checked); an ambiguous token offers
//!   nothing, exactly like the CLI skip-with-a-note.
//! - **pure** — `(text, range) -> actions`; the server shell owns I/O.

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, DocumentChanges, OneOf,
    OptionalVersionedTextDocumentIdentifier, Range, TextDocumentEdit, TextEdit, Uri, WorkspaceEdit,
};
use nika_schema::{FileId, ParseMode, SchemaError};

use super::position::LineIndex;

/// One machine-applicable rename: the offending token and its repair.
struct Rename {
    offending: String,
    suggestion: String,
    kind: &'static str,
}

/// Compute the quickfixes for `range` — every typed rename whose offending
/// token sits (uniquely) on a line the range touches.
#[must_use]
pub fn code_actions(
    uri: &Uri,
    text: &str,
    index: &LineIndex,
    range: Range,
) -> Vec<CodeActionOrCommand> {
    renames(text)
        .into_iter()
        .filter_map(|r| to_action(uri, text, index, range, &r))
        .collect()
}

/// The typed rename set for this document — the exact pairs `--fix`
/// splices, in the same precedence (parse-fatal first, then the report).
fn renames(text: &str) -> Vec<Rename> {
    match nika_schema::parse(text, FileId::new(0), ParseMode::Strict) {
        Err(SchemaError::UnknownField {
            field,
            suggestion: Some(to),
            ..
        }) => vec![Rename {
            offending: field,
            suggestion: to,
            kind: "field",
        }],
        Err(_) => Vec::new(),
        Ok(wf) => {
            let report = nika_check::check(&wf);
            let mut out = Vec::new();
            for v in &report.conformance {
                if let (Some(o), Some(s)) = (&v.offending, &v.suggestion) {
                    out.push(Rename {
                        offending: o.clone(),
                        suggestion: s.clone(),
                        kind: "reference",
                    });
                }
            }
            for t in &report.unknown_tools {
                if let Some(s) = &t.suggestion {
                    out.push(Rename {
                        offending: t.tool.clone(),
                        suggestion: s.clone(),
                        kind: "tool",
                    });
                }
            }
            for a in &report.unknown_args {
                if let Some(s) = &a.suggestion {
                    out.push(Rename {
                        offending: a.arg.clone(),
                        suggestion: s.clone(),
                        kind: "arg",
                    });
                }
            }
            out
        }
    }
}

/// Project one rename to a quickfix — `None` when the token is absent,
/// ambiguous, or off the requested range.
fn to_action(
    uri: &Uri,
    text: &str,
    index: &LineIndex,
    range: Range,
    rename: &Rename,
) -> Option<CodeActionOrCommand> {
    let start = find_unique_token(text, &rename.offending)?;
    let edit_range = byte_range(index, start, start + rename.offending.len());
    if edit_range.start.line > range.end.line || edit_range.end.line < range.start.line {
        return None; // the lightbulb is elsewhere
    }
    let edit = TextEdit {
        range: edit_range,
        new_text: rename.suggestion.clone(),
    };
    // `document_changes` (not the Uri-keyed map): the house bans HashMap
    // and a Uri key is a mutable-key-type lint — the versioned-edit shape
    // is also what modern clients prefer.
    let doc_edit = TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: None,
        },
        edits: vec![OneOf::Left(edit)],
    };
    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!(
            "Rename {} `{}` to `{}`",
            rename.kind, rename.offending, rename.suggestion
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        is_preferred: Some(true),
        edit: Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(vec![doc_edit])),
            ..WorkspaceEdit::default()
        }),
        ..CodeAction::default()
    }))
}

/// The byte offset of `token`'s single identifier-bounded occurrence —
/// `None` on zero or several (the `--fix` unique-by-the-gate discipline).
fn find_unique_token(text: &str, token: &str) -> Option<usize> {
    if token.is_empty() {
        return None;
    }
    let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
    let mut found: Option<usize> = None;
    let mut from = 0;
    while let Some(pos) = text[from..].find(token) {
        let at = from + pos;
        let before = text[..at].chars().next_back();
        let after = text[at + token.len()..].chars().next();
        if boundary(before) && boundary(after) {
            if found.is_some() {
                return None; // ambiguous — never guess
            }
            found = Some(at);
        }
        from = at + token.len();
    }
    found
}

/// A byte span as an LSP range through the document's line index.
fn byte_range(index: &LineIndex, start: usize, end: usize) -> Range {
    Range {
        start: index.position(start),
        end: index.position(end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: Range = Range {
        start: lsp_types::Position {
            line: 0,
            character: 0,
        },
        end: lsp_types::Position {
            line: 999,
            character: 0,
        },
    };

    fn uri() -> Uri {
        "file:///wf.nika.yaml".parse().expect("static uri")
    }

    fn actions(text: &str) -> Vec<CodeActionOrCommand> {
        code_actions(&uri(), text, &LineIndex::new(text), WHOLE)
    }

    fn first_edit(action: &CodeActionOrCommand) -> &TextEdit {
        let CodeActionOrCommand::CodeAction(a) = action else {
            panic!("quickfix expected");
        };
        let Some(DocumentChanges::Edits(edits)) =
            a.edit.as_ref().unwrap().document_changes.as_ref()
        else {
            panic!("document_changes expected");
        };
        let OneOf::Left(edit) = &edits[0].edits[0] else {
            panic!("plain edit expected");
        };
        edit
    }

    /// The one-voice proof: the REAL checker's typed suggestion becomes a
    /// quickfix whose edit slices exactly the offending token.
    #[test]
    fn an_unknown_dependency_offers_the_did_you_mean_rename() {
        let text = "nika: t\ntasks:\n  gather:\n    exec: { command: [\"true\"] }\n  think:\n    after: { gathr: success }\n    exec: { command: [\"true\"] }\n";
        let acts = actions(text);
        assert_eq!(acts.len(), 1, "one typed rename");
        let edit = first_edit(&acts[0]);
        assert_eq!(edit.new_text, "gather");
        let line = text.lines().nth(edit.range.start.line as usize).unwrap();
        let s = edit.range.start.character as usize;
        let e = edit.range.end.character as usize;
        assert_eq!(&line[s..e], "gathr", "the edit slices the offending token");
    }

    /// A finding with no suggestion offers NOTHING (silence beats a guess).
    #[test]
    fn no_suggestion_means_no_action() {
        let text =
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"${{ tasks.zzzzzz.output }}\"] }\n";
        // `zzzzzz` is nowhere near any declared name — no did-you-mean.
        assert!(actions(text).is_empty());
    }

    /// An ambiguous token (two identifier-bounded occurrences) is skipped —
    /// the CLI's unique-by-the-gate discipline, mirrored.
    #[test]
    fn ambiguous_tokens_are_never_touched() {
        assert_eq!(find_unique_token("a gathr b gathr", "gathr"), None);
        assert_eq!(find_unique_token("x gathr y", "gathr"), Some(2));
        assert_eq!(
            find_unique_token("gathrer gathr", "gathr"),
            Some("gathrer ".len()),
            "substring hits are not occurrences"
        );
    }

    /// The lightbulb only lights where the finding lives.
    #[test]
    fn actions_off_the_requested_range_stay_silent() {
        let text = "nika: t\ntasks:\n  gather:\n    exec: { command: [\"true\"] }\n  think:\n    after: { gathr: success }\n    exec: { command: [\"true\"] }\n";
        let top = Range {
            start: lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: lsp_types::Position {
                line: 1,
                character: 0,
            },
        };
        assert!(code_actions(&uri(), text, &LineIndex::new(text), top).is_empty());
    }
}
