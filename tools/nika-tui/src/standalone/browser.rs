// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! File browser entry for standalone TUI mode.

use std::path::{Path, PathBuf};

/// Entry in the file browser
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    /// Full path to the file
    pub path: PathBuf,
    /// Display name (relative to project root)
    pub display_name: String,
    /// Is this a directory?
    pub is_dir: bool,
    /// Is this expanded? (for directories)
    pub expanded: bool,
    /// Indentation level
    pub depth: usize,
}

impl BrowserEntry {
    pub fn new(path: PathBuf, root: &Path) -> Self {
        let display_name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let is_dir = path.is_dir();
        Self {
            path,
            display_name,
            is_dir,
            expanded: false,
            depth: 0,
        }
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_entry_new_file() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path.clone(), &root);

        assert_eq!(entry.path, path);
        assert_eq!(entry.display_name, "examples/test.nika.yaml");
        assert!(!entry.is_dir);
        assert!(!entry.expanded);
        assert_eq!(entry.depth, 0);
    }

    #[test]
    fn test_browser_entry_with_depth() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root).with_depth(2);

        assert_eq!(entry.depth, 2);
    }

    #[test]
    fn test_browser_entry_depth_chain() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root).with_depth(1).with_depth(3);

        assert_eq!(entry.depth, 3);
    }

    #[test]
    fn test_browser_entry_strip_prefix() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/deep/nested/workflow.nika.yaml");
        let entry = BrowserEntry::new(path, &root);

        assert_eq!(entry.display_name, "deep/nested/workflow.nika.yaml");
    }

    #[test]
    fn test_browser_entry_fallback_display_name() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/other/path/test.nika.yaml");
        let entry = BrowserEntry::new(path.clone(), &root);

        // When path is not under root, display_name should be the full path
        assert_eq!(entry.display_name, path.display().to_string());
    }

    #[test]
    fn test_browser_entry_clone() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/test.nika.yaml");
        let entry1 = BrowserEntry::new(path, &root).with_depth(1);
        let entry2 = entry1.clone();

        assert_eq!(entry1.path, entry2.path);
        assert_eq!(entry1.display_name, entry2.display_name);
        assert_eq!(entry1.is_dir, entry2.is_dir);
        assert_eq!(entry1.depth, entry2.depth);
    }

    #[test]
    fn test_browser_entry_is_dir_false() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root);

        assert!(!entry.is_dir);
    }
}
