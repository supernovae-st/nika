// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The agent `skills:` list — the workspace's own `SKILL.md` files
//! offered as paths (02-verbs §Agent Skills: entries are explicit
//! static file paths; `nika_schema::parse_skill` is the validity
//! judge). This is the one lane that reads the DISK: the project root
//! is derived from the document's own directory (up to the `.git`
//! marker), the walk is bounded (depth · dir count · file size · item
//! count), and every failure degrades to silence — a completion lane
//! must never take the server down.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::parse_skill;

/// The project root for a document: the nearest ancestor carrying a
/// `.git` (walking at most `MAX_ASCENT` levels), else the document's
/// own directory — relative skill paths resolve from where the run is
/// launched, and the repo root is that place in practice.
const MAX_ASCENT: usize = 8;
/// BFS bounds — a completion must stay instant on a monorepo.
const MAX_DIRS: usize = 512;
const MAX_DEPTH: usize = 6;
const MAX_ITEMS: usize = 50;
/// A `SKILL.md` past this size is not a skill card — skip, never read.
const MAX_FILE_BYTES: u64 = 64 * 1024;
/// Completion fires per KEYSTROKE inside `skills: ["` — without a memo
/// every keypress replays up to `MAX_DIRS` `read_dir` calls. Three
/// seconds covers a typing burst; a freshly authored SKILL.md shows up
/// on the next pause.
const WALK_TTL: Duration = Duration::from_secs(3);

/// The one-slot memo (root → items). A `Mutex`, not a `RwLock`: the server
/// loop is sync single-threaded — this exists for the keystroke burst,
/// not for contention. A poisoned lock degrades to a fresh walk.
static WALK_MEMO: Mutex<Option<(PathBuf, Instant, Vec<CompletionItem>)>> = Mutex::new(None);

pub(super) fn project_root(doc_dir: &Path) -> PathBuf {
    let mut cur = doc_dir.to_path_buf();
    for _ in 0..MAX_ASCENT {
        if cur.join(".git").exists() {
            return cur;
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => break,
        }
    }
    doc_dir.to_path_buf()
}

/// Every valid `SKILL.md` under the document's project root — the path
/// (root-relative, `/`-separated) is the label the author inserts; the
/// frontmatter name/description ride as the detail. Unparseable or
/// oversized files stay OUT: offering a path `parse_skill` would
/// reject at compose time teaches a failure.
pub(super) fn skill_items(doc_dir: &Path) -> Vec<CompletionItem> {
    let root = project_root(doc_dir);
    if let Ok(guard) = WALK_MEMO.lock()
        && let Some((cached_root, at, items)) = guard.as_ref()
        && *cached_root == root
        && at.elapsed() < WALK_TTL
    {
        return items.clone();
    }
    let items = walk_skills(&root);
    if let Ok(mut guard) = WALK_MEMO.lock() {
        *guard = Some((root, Instant::now(), items.clone()));
    }
    items
}

fn walk_skills(root: &Path) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    // A true breadth-first queue: when the dir budget bites on a huge
    // tree, the shallow (conventional) homes have already been seen —
    // a DFS would let one deep subtree starve `.agents/` at the root.
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut visited = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        if visited >= MAX_DIRS || items.len() >= MAX_ITEMS {
            break;
        }
        visited += 1;
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                // Hidden trees and dependency/build caches never carry
                // the author's skills — EXCEPT `.agents`, the very home
                // `nika init` scaffolds (`.agents/skills/…/SKILL.md`).
                if depth < MAX_DEPTH
                    && (!name.starts_with('.') || name == ".agents")
                    && name != "node_modules"
                    && name != "target"
                {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            if name != "SKILL.md" || items.len() >= MAX_ITEMS {
                continue;
            }
            if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_FILE_BYTES) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(doc) = parse_skill(&text) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            items.push(CompletionItem {
                label: rel,
                kind: Some(CompletionItemKind::FILE),
                detail: Some(format!("{} — {}", doc.name, doc.description)),
                ..CompletionItem::default()
            });
        }
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk memo is ONE slot — parallel tests hitting different
    /// roots would interleave writes and read each other's walks.
    /// Every test touching `skill_items` serializes on this.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(p, text).expect("write");
    }

    #[test]
    fn valid_skills_surface_with_their_frontmatter_voice() {
        let _serial = SERIAL.lock().expect("serial");
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).expect("git marker");
        write(
            root,
            ".agents/skills/review/SKILL.md",
            "---\nname: code-review\ndescription: review the diff\n---\nbody\n",
        );
        write(
            root,
            "docs/skills/broken/SKILL.md",
            "no frontmatter at all\n",
        );
        write(
            root,
            "node_modules/x/SKILL.md",
            "---\nname: ghost\ndescription: d\n---\n",
        );
        let items = skill_items(&root.join("workflows"));
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![".agents/skills/review/SKILL.md"],
            "valid in · broken out · node_modules never walked: {labels:?}"
        );
        assert_eq!(
            items[0].detail.as_deref(),
            Some("code-review — review the diff"),
            "the frontmatter is the detail voice"
        );
    }

    #[test]
    fn the_memo_serves_the_burst_and_a_new_root_invalidates() {
        let _serial = SERIAL.lock().expect("serial");
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).expect("git marker");
        write(
            root,
            ".agents/skills/one/SKILL.md",
            "---\nname: one\ndescription: d\n---\n",
        );
        let first = skill_items(root);
        assert_eq!(first.len(), 1);
        // Delete the card — within the TTL the memo still answers (one
        // walk per burst, not per keystroke). This is the assertion the
        // memo EXISTS; freshness returns on the next pause.
        fs::remove_file(root.join(".agents/skills/one/SKILL.md")).expect("rm");
        let second = skill_items(root);
        assert_eq!(second.len(), 1, "the burst is served from the memo");
        // A DIFFERENT root never reads a stale memo.
        let other = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(other.path().join(".git")).expect("git marker");
        assert!(
            skill_items(other.path()).is_empty(),
            "a new root walks fresh"
        );
    }

    #[test]
    fn the_root_is_the_git_ancestor_of_the_document() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).expect("git marker");
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).expect("nested");
        assert_eq!(project_root(&nested), root);
        // No marker anywhere → the document's own directory.
        let bare = tempfile::tempdir().expect("tempdir");
        assert_eq!(project_root(bare.path()), bare.path());
    }
}
