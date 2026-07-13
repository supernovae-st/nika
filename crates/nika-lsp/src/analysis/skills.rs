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

use std::fs;
use std::path::{Path, PathBuf};

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
    let mut items = Vec::new();
    let mut queue = vec![(root.clone(), 0usize)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = queue.pop() {
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
                    queue.push((path, depth + 1));
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
                .strip_prefix(&root)
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

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        fs::write(p, text).expect("write");
    }

    #[test]
    fn valid_skills_surface_with_their_frontmatter_voice() {
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
