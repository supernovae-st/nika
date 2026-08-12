// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Server-side rename for task identities (single-file · the W1 map).
//!
//! A task's identity is its map KEY (W1 « the map »), and a rename must
//! move every site that speaks it, atomically ·
//!
//! 1. the declaring key token (`old:` → `new:`),
//! 2. every `after:` entry naming it,
//! 3. every `tasks.<old>` chain inside a `${{ … }}` island — the same
//!    document-level byte-scan discipline (and the same comment
//!    tolerance) as the [`definition`](super::definition) lane, so the
//!    two features agree on what a reference IS.
//!
//! `prepareRename` answers only on those three site shapes; `rename`
//! validates the new name against the task-id grammar
//! (`[a-z][a-z0-9_]*`) and refuses collisions with existing ids — the
//! refusal is a request ERROR carrying the teaching, never a silent
//! no-op edit.

use std::collections::BTreeSet;

use lsp_types::{
    DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, Range, TextDocumentEdit,
    TextEdit, Uri, WorkspaceEdit,
};
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode, parse};

use super::definition::{ident_end, islands, token_range, token_span_contains};
use super::position::LineIndex;

/// The token range + current id under `offset`, when it is a renameable
/// task-identity site (key · `after:` target · `tasks.<id>` island
/// ref). `None` anywhere else — the client greys the gesture out.
#[must_use]
pub fn prepare(text: &str, offset: usize) -> Option<(Range, String)> {
    let wf = parse(text, FileId::new(0), ParseMode::Lenient).ok()?;
    let index = LineIndex::new(text);
    site_at(&wf, text, offset).map(|site| {
        let range = site_lsp_range(&index, &site);
        (range, site.id)
    })
}

/// Rename the task identity under `offset` to `new_name` across every
/// site. `Ok` is the single-document [`WorkspaceEdit`].
///
/// # Errors
///
/// The human-readable refusal the client shows: not an identity site ·
/// unparseable document · same name · grammar violation · collision ·
/// ghost (the id defines no task).
pub fn rename(
    uri: &Uri,
    text: &str,
    offset: usize,
    new_name: &str,
) -> Result<WorkspaceEdit, String> {
    let wf = parse(text, FileId::new(0), ParseMode::Lenient)
        .map_err(|e| format!("rename needs a parseable document: {e}"))?;
    let site = site_at(&wf, text, offset)
        .ok_or_else(|| "not a task identity (key · after: target · tasks.<id> ref)".to_owned())?;
    let old = site.id;
    if new_name == old {
        return Err(format!("`{old}` already is the name"));
    }
    if !valid_task_id(new_name) {
        return Err(format!(
            "`{new_name}` is not a task id — snake_case: [a-z][a-z0-9_]*"
        ));
    }
    let ids: BTreeSet<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    if ids.contains(new_name) {
        return Err(format!("a task named `{new_name}` already exists"));
    }
    if !ids.contains(old.as_str()) {
        return Err(format!(
            "`{old}` names no task in this document — rename the definition, not a ghost"
        ));
    }

    let index = LineIndex::new(text);
    let mut edits: Vec<TextEdit> = Vec::new();
    // 1 · the declaring key token
    for task in &wf.tasks {
        if task.value.id.value == old {
            edits.push(TextEdit {
                range: token_range(&index, task.value.id.span, old.len()),
                new_text: new_name.to_owned(),
            });
        }
        // 2 · every after: entry naming it (the control boundary)
        for (target, _pred) in &task.value.after {
            if target.value == old {
                edits.push(TextEdit {
                    range: token_range(&index, target.span, old.len()),
                    new_text: new_name.to_owned(),
                });
            }
        }
    }
    // 3 · every `tasks.<old>` chain inside an island
    for (start, end) in island_ref_sites(text, &old) {
        edits.push(TextEdit {
            range: Range::new(index.position(start), index.position(end)),
            new_text: new_name.to_owned(),
        });
    }
    edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
    edits.dedup_by(|a, b| a.range == b.range);

    // documentChanges (not the `changes` map): no HashMap in the payload
    // path, and the versioned-document shape every modern client speaks.
    Ok(WorkspaceEdit {
        document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: None,
            },
            edits: edits.into_iter().map(OneOf::Left).collect(),
        }])),
        ..WorkspaceEdit::default()
    })
}

/// One renameable site: the id it names plus its exact byte range.
struct Site {
    id: String,
    start: usize,
    end: usize,
    /// A parser span site (key/dep) carries its span-derived range via
    /// bytes already; island sites too — one shape serves both.
    _kind: SiteKind,
}

enum SiteKind {
    Key,
    Dep,
    IslandRef,
}

fn site_lsp_range(index: &LineIndex, site: &Site) -> Range {
    Range::new(index.position(site.start), index.position(site.end))
}

/// Resolve the renameable site under `offset` — key first (the
/// declaration outranks a same-byte ref), then `after:` targets,
/// then island refs.
fn site_at(wf: &RawWorkflow, text: &str, offset: usize) -> Option<Site> {
    let off = u32::try_from(offset).unwrap_or(u32::MAX);
    for task in &wf.tasks {
        let id = &task.value.id;
        if token_span_contains(id.span, id.value.len(), off) {
            let start = id.span.start.0 as usize;
            return Some(Site {
                id: id.value.clone(),
                start,
                end: start + id.value.len(),
                _kind: SiteKind::Key,
            });
        }
        for (target, _pred) in &task.value.after {
            if token_span_contains(target.span, target.value.len(), off) {
                let start = target.span.start.0 as usize;
                return Some(Site {
                    id: target.value.clone(),
                    start,
                    end: start + target.value.len(),
                    _kind: SiteKind::Dep,
                });
            }
        }
    }
    island_ref_at(text, offset)
}

/// The `tasks.<id>` island ref under `offset`, with the id's byte range
/// (cursor anywhere from the `tasks.` keyword through the id).
fn island_ref_at(text: &str, offset: usize) -> Option<Site> {
    let bytes = text.as_bytes();
    for (island_start, island_end) in islands(text) {
        let inner = text.get(island_start..island_end)?;
        let mut search_from = 0usize;
        while let Some(rel) = inner.get(search_from..)?.find("tasks.") {
            let kw_at = island_start + search_from + rel;
            let mid_ident = kw_at > 0
                && bytes
                    .get(kw_at - 1)
                    .is_some_and(|b| *b == b'_' || b.is_ascii_alphanumeric());
            if !mid_ident {
                let id_start = kw_at + "tasks.".len();
                let id_end = ident_end(bytes, id_start);
                if id_end > id_start && (kw_at..id_end).contains(&offset) {
                    return text.get(id_start..id_end).map(|id| Site {
                        id: id.to_owned(),
                        start: id_start,
                        end: id_end,
                        _kind: SiteKind::IslandRef,
                    });
                }
            }
            search_from = (search_from + rel + "tasks.".len()).min(inner.len());
        }
    }
    None
}

/// Every `tasks.<old>` id byte range across every island — the edit set
/// for site shape 3.
fn island_ref_sites(text: &str, old: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (island_start, island_end) in islands(text) {
        let Some(inner) = text.get(island_start..island_end) else {
            continue;
        };
        let mut search_from = 0usize;
        while let Some(rel) = inner.get(search_from..).and_then(|s| s.find("tasks.")) {
            let kw_at = island_start + search_from + rel;
            let mid_ident = kw_at > 0
                && bytes
                    .get(kw_at - 1)
                    .is_some_and(|b| *b == b'_' || b.is_ascii_alphanumeric());
            if !mid_ident {
                let id_start = kw_at + "tasks.".len();
                let id_end = ident_end(bytes, id_start);
                if id_end > id_start && text.get(id_start..id_end) == Some(old) {
                    out.push((id_start, id_end));
                }
            }
            search_from = (search_from + rel + "tasks.".len()).min(inner.len());
        }
    }
    out
}

/// The task-id grammar (spec 03 §id · `snake_case` · CEL-safe).
fn valid_task_id(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// The W2 fixture — `fetch` is spoken at FIVE sites: its key, two
    /// `after:` entries (digest's + save's) and two `${{ tasks.fetch }}`
    /// islands (both inside `with:` binding values, the boundary form).
    const DOC: &str = "nika: w\ntasks:\n  fetch:\n    exec: { command: [\"curl\"] }\n  digest:\n    after: { fetch: success }\n    with:\n      article: \"${{ tasks.fetch.output }}\"\n    infer: { prompt: \"sum ${{ with.article }}\", max_tokens: 10 }\n  save:\n    after: { digest: success, fetch: terminal }\n    with:\n      doc: \"${{ tasks.digest.output }}\"\n      raw: \"${{ tasks.fetch.output }}\"\n    exec: { command: [\"tee\", \"${{ with.doc }}\", \"${{ with.raw }}\"] }\n";

    fn uri() -> Uri {
        Uri::from_str("file:///w.nika.yaml").expect("uri")
    }

    fn edits(we: &WorkspaceEdit) -> Vec<TextEdit> {
        let Some(DocumentChanges::Edits(docs)) = we.document_changes.as_ref() else {
            panic!("documentChanges edits");
        };
        docs[0]
            .edits
            .iter()
            .map(|e| match e {
                OneOf::Left(t) => t.clone(),
                OneOf::Right(a) => a.text_edit.clone(),
            })
            .collect()
    }

    fn apply(text: &str, mut es: Vec<TextEdit>) -> String {
        // apply bottom-up so earlier byte offsets stay valid
        let index = LineIndex::new(text);
        es.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
        let mut out = text.to_owned();
        for e in es {
            let s = index.offset(e.range.start);
            let t = index.offset(e.range.end);
            out.replace_range(s..t, &e.new_text);
        }
        out
    }

    #[test]
    fn rename_from_the_key_moves_every_site() {
        // rename `fetch` → `pull` from its declaring key: the key, TWO
        // after: entries, TWO island refs — five edits, one truth.
        let at = DOC.find("\n  fetch:").expect("key") + 3;
        let we = rename(&uri(), DOC, at, "pull").expect("renames");
        let es = edits(&we);
        assert_eq!(es.len(), 5, "key + 2 after entries + 2 island refs: {es:?}");
        let after = apply(DOC, es);
        assert!(!after.contains("fetch"), "no site left behind: {after}");
        assert!(after.contains("\n  pull:"), "the key moved");
        assert!(
            after.contains("after: { pull: success }"),
            "digest's control entry moved"
        );
        assert!(
            after.contains("after: { digest: success, pull: terminal }"),
            "save's control entry moved (the predicate untouched)"
        );
        assert!(
            after.contains("${{ tasks.pull.output }}"),
            "island refs moved"
        );
        // the renamed document still parses — the edit set is coherent
        assert!(
            nika_schema::parse(&after, FileId::new(0), ParseMode::Strict).is_ok(),
            "renamed doc parses strict: {after}"
        );
    }

    #[test]
    fn rename_from_an_after_entry_and_from_an_island_ref_agree_with_the_key() {
        let from_key = {
            let at = DOC.find("\n  fetch:").expect("key") + 3;
            apply(DOC, edits(&rename(&uri(), DOC, at, "pull").expect("ok")))
        };
        let from_after = {
            let at = DOC.find("{ fetch: success }").expect("after entry") + 2;
            apply(DOC, edits(&rename(&uri(), DOC, at, "pull").expect("ok")))
        };
        let from_ref = {
            let at = DOC.find("tasks.fetch").expect("ref") + "tasks.".len();
            apply(DOC, edits(&rename(&uri(), DOC, at, "pull").expect("ok")))
        };
        assert_eq!(from_key, from_after, "after: site → same document");
        assert_eq!(from_key, from_ref, "island site → same document");
    }

    #[test]
    fn refusals_teach_grammar_collision_and_ghost() {
        let at = DOC.find("\n  fetch:").expect("key") + 3;
        assert!(
            rename(&uri(), DOC, at, "Bad-Name")
                .expect_err("grammar")
                .contains("snake_case"),
            "grammar refusal teaches the shape"
        );
        assert!(
            rename(&uri(), DOC, at, "digest")
                .expect_err("collision")
                .contains("already exists"),
            "collision refusal names the conflict"
        );
        // a ghost after: site — rename refuses (rename the definition,
        // not a ghost)
        let ghost_doc = "nika: w\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n";
        let at = ghost_doc.find("ghost").expect("ghost") + 1;
        assert!(
            rename(&uri(), ghost_doc, at, "real")
                .expect_err("ghost")
                .contains("ghost"),
            "ghost refusal names it"
        );
    }

    #[test]
    fn prepare_answers_on_the_three_sites_and_nowhere_else() {
        let index = LineIndex::new(DOC);
        // key site
        let key_at = DOC.find("\n  digest:").expect("key") + 3;
        let (range, id) = prepare(DOC, key_at).expect("key prepares");
        assert_eq!(id, "digest");
        assert_eq!(
            index.offset(range.start),
            key_at,
            "range anchors the key token"
        );
        // after: entry site
        let dep_at = DOC
            .find("{ digest: success, fetch: terminal }")
            .expect("after")
            + 2;
        assert_eq!(prepare(DOC, dep_at).expect("after prepares").1, "digest");
        // island site
        let ref_at = DOC.find("tasks.digest").expect("ref") + "tasks.".len();
        assert_eq!(prepare(DOC, ref_at).expect("ref prepares").1, "digest");
        // a verb keyword is NOT a renameable site
        let verb_at = DOC.find("infer:").expect("verb");
        assert!(prepare(DOC, verb_at).is_none(), "verb keyword refuses");
        // the workflow name is NOT a task identity
        let wf_at = DOC.find("nika: w").expect("wf name") + "nika: ".len();
        assert!(prepare(DOC, wf_at).is_none(), "the `nika:` name refuses");
    }

    #[test]
    fn verb_named_task_renames_only_identity_sites() {
        // a task NAMED `invoke` (the census trap): the verb KEY of another
        // task must not be touched — only identity sites move.
        let doc = "nika: w\ntasks:\n  invoke:\n    exec: { command: [\"x\"] }\n  b:\n    after: { invoke: success }\n    with:\n      path: \"${{ tasks.invoke.output }}\"\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"${{ with.path }}\" }\n";
        let at = doc.find("\n  invoke:").expect("key") + 3;
        let we = rename(&uri(), doc, at, "caller").expect("renames");
        let after = apply(doc, edits(&we));
        assert!(after.contains("\n  caller:"), "identity key moved");
        assert!(
            after.contains("after: { caller: success }"),
            "control entry moved"
        );
        assert!(after.contains("tasks.caller.output"), "ref moved");
        assert!(
            after.contains("\n    invoke:\n      tool:"),
            "task b's VERB key untouched: {after}"
        );
    }

    #[test]
    fn same_name_refuses_and_unparseable_refuses() {
        let at = DOC.find("\n  fetch:").expect("key") + 3;
        assert!(rename(&uri(), DOC, at, "fetch").is_err(), "no-op refused");
        assert!(
            rename(&uri(), "nika: v1\nworkflow: [bad\n", 0, "x").is_err(),
            "unparseable refused with a message"
        );
    }
}
