//! Git Integration for TUI (v0.21.3)
//!
//! Provides git status tracking for:
//! - File browser: Show modified/added/deleted status
//! - Editor gutter: Show line-level changes (+/~/-)
//!
//! # Architecture
//!
//! ```text
//! GitStatus:
//! ├── repository: git2::Repository
//! ├── file_statuses: HashMap<PathBuf, FileStatus>  (cached)
//! └── line_changes: HashMap<PathBuf, LineChanges>  (lazy-loaded)
//!
//! LineChanges:
//! ├── added: Vec<usize>      (new lines)
//! ├── modified: Vec<usize>   (changed lines)
//! └── deleted: Vec<usize>    (after this line, content was removed)
//! ```

use git2::{DiffOptions, Repository, Status, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// File status from git perspective
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// File is tracked and unmodified
    Clean,
    /// File has modifications (working tree differs from index)
    Modified,
    /// File is new (not in index)
    Added,
    /// File was deleted (in index but not in working tree)
    Deleted,
    /// File was renamed
    Renamed,
    /// File has conflicts
    Conflicted,
    /// File is ignored
    Ignored,
    /// File is untracked
    Untracked,
}

impl FileStatus {
    /// Get the gutter symbol for this status
    pub fn gutter_symbol(&self) -> &'static str {
        match self {
            Self::Clean => " ",
            Self::Modified => "~",
            Self::Added => "+",
            Self::Deleted => "-",
            Self::Renamed => "R",
            Self::Conflicted => "!",
            Self::Ignored => ".",
            Self::Untracked => "?",
        }
    }

    /// Get whether this status indicates changes
    pub fn is_changed(&self) -> bool {
        !matches!(self, Self::Clean | Self::Ignored)
    }
}

/// Line-level change type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    /// Line was added
    Added,
    /// Line was modified
    Modified,
    /// Line(s) were deleted after this line
    Deleted,
}

impl LineChange {
    /// Get the gutter symbol for this change
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
    /// Map of line number to change type
    changes: HashMap<usize, LineChange>,
}

impl LineChanges {
    /// Create empty line changes
    pub fn new() -> Self {
        Self::default()
    }

    /// Get change for a specific line (0-indexed)
    pub fn get(&self, line: usize) -> Option<LineChange> {
        self.changes.get(&line).copied()
    }

    /// Check if any changes exist
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Get number of changes
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Add a change for a line
    fn add(&mut self, line: usize, change: LineChange) {
        self.changes.insert(line, change);
    }

    /// Get a cloned map of all changes (for use in render closures)
    pub fn changes_map(&self) -> HashMap<usize, LineChange> {
        self.changes.clone()
    }
}

/// Git status tracker for the TUI
pub struct GitStatus {
    /// Repository root path
    root: PathBuf,
    /// Cached file statuses
    file_statuses: HashMap<PathBuf, FileStatus>,
    /// Cached line changes (lazy-loaded per file)
    line_changes: HashMap<PathBuf, LineChanges>,
}

impl GitStatus {
    /// Try to open a git repository at the given path
    /// Returns None if not a git repository
    pub fn open(path: &Path) -> Option<Self> {
        let repo = Repository::discover(path).ok()?;
        let root = repo.workdir()?.to_path_buf();

        let mut status = Self {
            root,
            file_statuses: HashMap::new(),
            line_changes: HashMap::new(),
        };

        // Pre-load file statuses
        status.refresh_file_statuses();

        Some(status)
    }

    /// Get the repository root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Refresh all file statuses from git
    pub fn refresh_file_statuses(&mut self) {
        self.file_statuses.clear();

        let repo = match Repository::open(&self.root) {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .include_unmodified(false);

        let statuses = match repo.statuses(Some(&mut opts)) {
            Ok(s) => s,
            Err(_) => return,
        };

        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                let full_path = self.root.join(path);
                let status = Self::convert_status(entry.status());
                self.file_statuses.insert(full_path, status);
            }
        }
    }

    /// Get status for a specific file
    pub fn file_status(&self, path: &Path) -> FileStatus {
        // Try absolute path first
        if let Some(status) = self.file_statuses.get(path) {
            return *status;
        }

        // Try relative to root
        if let Ok(rel_path) = path.strip_prefix(&self.root) {
            if let Some(status) = self.file_statuses.get(&self.root.join(rel_path)) {
                return *status;
            }
        }

        FileStatus::Clean
    }

    /// Get line changes for a file (lazy-loads if not cached)
    pub fn line_changes(&mut self, path: &Path) -> &LineChanges {
        // Check if already cached
        if !self.line_changes.contains_key(path) {
            // Load line changes
            let changes = self.compute_line_changes(path);
            self.line_changes.insert(path.to_path_buf(), changes);
        }

        // Safe to unwrap since we just inserted
        self.line_changes.get(path).unwrap()
    }

    /// Clear line changes cache for a file (call after file is modified)
    pub fn invalidate_line_changes(&mut self, path: &Path) {
        self.line_changes.remove(path);
    }

    /// Compute line changes by diffing file against index
    fn compute_line_changes(&self, path: &Path) -> LineChanges {
        let mut changes = LineChanges::new();

        let repo = match Repository::open(&self.root) {
            Ok(r) => r,
            Err(_) => return changes,
        };

        // Get relative path
        let rel_path = match path.strip_prefix(&self.root) {
            Ok(p) => p,
            Err(_) => return changes,
        };

        // Get the HEAD tree
        let head = match repo.head() {
            Ok(h) => h,
            Err(_) => return changes,
        };

        let tree = match head.peel_to_tree() {
            Ok(t) => t,
            Err(_) => return changes,
        };

        // Setup diff options
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(rel_path);

        // Diff HEAD against working directory
        let diff = match repo.diff_tree_to_workdir(Some(&tree), Some(&mut diff_opts)) {
            Ok(d) => d,
            Err(_) => return changes,
        };

        // Process hunks to get line-level changes
        let _ = diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, hunk| {
                let new_start = hunk.new_start() as usize;
                let new_lines = hunk.new_lines() as usize;
                let old_lines = hunk.old_lines() as usize;

                // Determine change type based on hunk
                if old_lines == 0 {
                    // Pure addition
                    for i in 0..new_lines {
                        changes.add(new_start + i - 1, LineChange::Added);
                    }
                } else if new_lines == 0 {
                    // Pure deletion - mark the line after which deletion occurred
                    if new_start > 0 {
                        changes.add(new_start - 1, LineChange::Deleted);
                    }
                } else {
                    // Modification
                    for i in 0..new_lines {
                        changes.add(new_start + i - 1, LineChange::Modified);
                    }
                }

                true
            }),
            None,
        );

        changes
    }

    /// Convert git2 Status to our FileStatus
    fn convert_status(status: Status) -> FileStatus {
        if status.is_conflicted() {
            FileStatus::Conflicted
        } else if status.is_wt_new() || status.is_index_new() {
            FileStatus::Added
        } else if status.is_wt_deleted() || status.is_index_deleted() {
            FileStatus::Deleted
        } else if status.is_wt_renamed() || status.is_index_renamed() {
            FileStatus::Renamed
        } else if status.is_wt_modified() || status.is_index_modified() {
            FileStatus::Modified
        } else if status.is_ignored() {
            FileStatus::Ignored
        } else if status.is_wt_new() {
            FileStatus::Untracked
        } else {
            FileStatus::Clean
        }
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
        // /tmp is not a git repo
        let status = GitStatus::open(Path::new("/tmp"));
        // This might succeed if /tmp is inside a repo, so just check it doesn't crash
        let _ = status;
    }
}
