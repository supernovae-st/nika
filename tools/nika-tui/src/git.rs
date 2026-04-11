// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Git Integration for TUI — pure CLI implementation (no git2/openssl)
//!
//! Provides git status tracking for:
//! - File browser: Show modified/added/deleted status
//! - Editor gutter: Show line-level changes (+/~/-)
//!
//! Uses `git` CLI instead of git2 crate to avoid 130s C/openssl compile.
//! All operations shell out to `git` (available on all dev machines).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// File status from git perspective
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Clean,
    Modified,
    Added,
    Deleted,
    Renamed,
    Conflicted,
    Ignored,
    Untracked,
}

impl FileStatus {
    pub fn gutter_symbol(&self) -> &'static str {
        match self {
            Self::Clean => " ",
            Self::Modified => "~",
            Self::Added => "+",
            Self::Deleted => "-",
            Self::Renamed => "→",
            Self::Conflicted => "!",
            Self::Ignored => ".",
            Self::Untracked => "?",
        }
    }

    pub fn is_changed(&self) -> bool {
        !matches!(self, Self::Clean | Self::Ignored)
    }
}

/// Line-level change type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    Added,
    Modified,
    Deleted,
}

impl LineChange {
    pub fn gutter_symbol(&self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Modified => "~",
            Self::Deleted => "-",
        }
    }
}

/// Line-level changes for a file
#[derive(Debug, Clone, Default)]
pub struct LineChanges {
    changes: HashMap<usize, LineChange>,
}

impl LineChanges {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, line: usize) -> Option<LineChange> {
        self.changes.get(&line).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    fn add(&mut self, line: usize, change: LineChange) {
        self.changes.insert(line, change);
    }

    pub fn changes_map(&self) -> HashMap<usize, LineChange> {
        self.changes.clone()
    }
}

/// Git status tracker for the TUI (CLI-based, no git2)
pub struct GitStatus {
    root: PathBuf,
    file_statuses: HashMap<PathBuf, FileStatus>,
    line_changes: HashMap<PathBuf, LineChanges>,
}

impl GitStatus {
    /// Try to open a git repository at the given path.
    /// Returns None if not a git repository or git CLI not available.
    pub fn open(path: &Path) -> Option<Self> {
        // `git rev-parse --show-toplevel` to find repo root
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

        let mut status = Self {
            root,
            file_statuses: HashMap::new(),
            line_changes: HashMap::new(),
        };

        status.refresh_file_statuses();
        Some(status)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Refresh file statuses via `git status --porcelain=v1`
    pub fn refresh_file_statuses(&mut self) {
        self.file_statuses.clear();

        let output = match Command::new("git")
            .args(["status", "--porcelain=v1", "-uall"])
            .current_dir(&self.root)
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let xy = &line[..2];
            let path_str = &line[3..];
            let full_path = self.root.join(path_str);

            let status = match xy.trim() {
                "M" | " M" | "MM" => FileStatus::Modified,
                "A" | " A" | "AM" => FileStatus::Added,
                "D" | " D" => FileStatus::Deleted,
                "R" | " R" => FileStatus::Renamed,
                "UU" | "AA" | "DD" => FileStatus::Conflicted,
                "??" => FileStatus::Untracked,
                "!!" => FileStatus::Ignored,
                _ => FileStatus::Modified, // Default to modified for unknown
            };

            self.file_statuses.insert(full_path, status);
        }
    }

    pub fn file_status(&self, path: &Path) -> FileStatus {
        if let Some(status) = self.file_statuses.get(path) {
            return *status;
        }

        if let Ok(rel_path) = path.strip_prefix(&self.root) {
            if let Some(status) = self.file_statuses.get(&self.root.join(rel_path)) {
                return *status;
            }
        }

        FileStatus::Clean
    }

    /// Get line changes for a file (lazy-loads via `git diff`)
    pub fn line_changes(&mut self, path: &Path) -> &LineChanges {
        if !self.line_changes.contains_key(path) {
            let changes = self.compute_line_changes(path);
            self.line_changes.insert(path.to_path_buf(), changes);
        }
        // SAFETY: inserted on the cache-miss branch directly above
        self.line_changes.get(path).expect("inserted on cache miss")
    }

    pub fn invalidate_line_changes(&mut self, path: &Path) {
        self.line_changes.remove(path);
    }

    /// Compute line changes via `git diff --unified=0`
    fn compute_line_changes(&self, path: &Path) -> LineChanges {
        let mut changes = LineChanges::new();

        let rel_path = match path.strip_prefix(&self.root) {
            Ok(p) => p,
            Err(_) => return changes,
        };

        let output = match Command::new("git")
            .args([
                "diff",
                "--unified=0",
                "--no-color",
                "--",
                &rel_path.to_string_lossy(),
            ])
            .current_dir(&self.root)
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return changes,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse unified diff hunks: @@ -old,count +new,count @@
        for line in stdout.lines() {
            if !line.starts_with("@@") {
                continue;
            }

            // Parse: @@ -X[,N] +Y[,M] @@
            if let Some((old_info, new_info)) = parse_hunk_header(line) {
                let (new_start, new_count) = new_info;
                let (_old_start, old_count) = old_info;

                if old_count == 0 {
                    // Pure addition
                    for i in 0..new_count {
                        changes.add(new_start + i - 1, LineChange::Added);
                    }
                } else if new_count == 0 {
                    // Pure deletion
                    if new_start > 0 {
                        changes.add(new_start - 1, LineChange::Deleted);
                    }
                } else {
                    // Modification
                    for i in 0..new_count {
                        changes.add(new_start + i - 1, LineChange::Modified);
                    }
                }
            }
        }

        changes
    }
}

/// Parse a unified diff hunk header: `@@ -X[,N] +Y[,M] @@`
/// Returns ((old_start, old_count), (new_start, new_count))
fn parse_hunk_header(line: &str) -> Option<((usize, usize), (usize, usize))> {
    // Find the ranges between @@ markers
    let line = line.strip_prefix("@@ ")?;
    let end = line.find(" @@")?;
    let ranges = &line[..end];

    let mut parts = ranges.split_whitespace();

    let old_range = parts.next()?.strip_prefix('-')?;
    let new_range = parts.next()?.strip_prefix('+')?;

    let old = parse_range(old_range);
    let new = parse_range(new_range);

    Some((old, new))
}

fn parse_range(s: &str) -> (usize, usize) {
    if let Some((start, count)) = s.split_once(',') {
        (start.parse().unwrap_or(1), count.parse().unwrap_or(1))
    } else {
        (s.parse().unwrap_or(1), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_status_gutter_symbol() {
        assert_eq!(FileStatus::Clean.gutter_symbol(), " ");
        assert_eq!(FileStatus::Modified.gutter_symbol(), "~");
        assert_eq!(FileStatus::Added.gutter_symbol(), "+");
        assert_eq!(FileStatus::Deleted.gutter_symbol(), "-");
    }

    #[test]
    fn test_file_status_is_changed() {
        assert!(!FileStatus::Clean.is_changed());
        assert!(FileStatus::Modified.is_changed());
        assert!(FileStatus::Added.is_changed());
        assert!(!FileStatus::Ignored.is_changed());
    }

    #[test]
    fn test_line_change_gutter_symbol() {
        assert_eq!(LineChange::Added.gutter_symbol(), "+");
        assert_eq!(LineChange::Modified.gutter_symbol(), "~");
        assert_eq!(LineChange::Deleted.gutter_symbol(), "-");
    }

    #[test]
    fn test_line_changes_empty() {
        let changes = LineChanges::new();
        assert!(changes.is_empty());
        assert_eq!(changes.len(), 0);
        assert_eq!(changes.get(0), None);
    }

    #[test]
    fn test_line_changes_add() {
        let mut changes = LineChanges::new();
        changes.add(5, LineChange::Modified);
        changes.add(10, LineChange::Added);

        assert!(!changes.is_empty());
        assert_eq!(changes.len(), 2);
        assert_eq!(changes.get(5), Some(LineChange::Modified));
        assert_eq!(changes.get(10), Some(LineChange::Added));
        assert_eq!(changes.get(7), None);
    }

    #[test]
    fn test_git_status_open_non_repo() {
        let status = GitStatus::open(Path::new("/tmp"));
        let _ = status;
    }

    #[test]
    fn parse_hunk_header_basic() {
        let result = parse_hunk_header("@@ -1,3 +1,4 @@");
        assert_eq!(result, Some(((1, 3), (1, 4))));
    }

    #[test]
    fn parse_hunk_header_single_line() {
        let result = parse_hunk_header("@@ -5 +5,2 @@");
        assert_eq!(result, Some(((5, 1), (5, 2))));
    }

    #[test]
    fn parse_hunk_header_deletion() {
        let result = parse_hunk_header("@@ -10,3 +9,0 @@");
        assert_eq!(result, Some(((10, 3), (9, 0))));
    }

    #[test]
    fn parse_hunk_header_addition() {
        let result = parse_hunk_header("@@ -5,0 +6,3 @@");
        assert_eq!(result, Some(((5, 0), (6, 3))));
    }

    #[test]
    fn parse_range_with_count() {
        assert_eq!(parse_range("10,3"), (10, 3));
    }

    #[test]
    fn parse_range_single() {
        assert_eq!(parse_range("5"), (5, 1));
    }
}
