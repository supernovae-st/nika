// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tree Node Types
//!
//! Defines `TreeNode` and `NodeKind` for the enhanced tree view.
//! Includes SuperNovae ecosystem detection for .nika.yaml, .son, .nika/, .novanet/ files.
//!
//! ## Features
//! - Recursive tree building with `build_tree()`
//! - Git status detection via `git status --porcelain`
//! - Permission error handling (graceful degradation)
//! - Edge case handling (empty dirs, no .nika folder)

use camino::{Utf8Path, Utf8PathBuf};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::process::Command;

/// Unique identifier for a tree node
pub type NodeId = u64;

/// Maximum depth for tree recursion (prevents runaway expansion)
pub const MAX_TREE_DEPTH: usize = 10;

/// Directories that should NOT have their children loaded during tree building.
/// These are shown in the tree but collapsed by default and never auto-expanded.
/// This prevents multi-second startup delays from traversing thousands of files.
const HEAVY_DIRECTORIES: &[&str] = &[
    "target",       // Rust build artifacts (can be 10k+ files)
    "node_modules", // npm packages (can be 100k+ files)
    ".git",         // Git internals
    "dist",         // Build outputs
    "build",        // Build outputs
    ".cargo",       // Cargo cache
    "__pycache__",  // Python cache
    ".venv",        // Python virtual env
    "venv",         // Python virtual env
    ".next",        // Next.js build
    ".nuxt",        // Nuxt.js build
];

/// Git status cache - maps relative paths to their status
pub type GitStatusCache = FxHashMap<String, GitStatus>;

/// Build git status cache from `git status --porcelain`
///
/// Returns a map of relative file paths to their git status.
/// Returns empty map if:
/// - Not in a git repository
/// - Git command fails
/// - Permission issues
pub fn build_git_status_cache(root: &Utf8Path) -> GitStatusCache {
    let mut cache = FxHashMap::default();

    // Run git status --porcelain from the root directory
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root.as_std_path())
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return cache, // Not a git repo or git not available
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }

        // Format: XY <filename>
        // X = index status, Y = worktree status
        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].trim();

        // Handle renamed files (format: "R  old -> new")
        let path = if path.contains(" -> ") {
            path.split(" -> ").last().unwrap_or(path)
        } else {
            path
        };

        let status = GitStatus::from_status_chars(index_status, worktree_status);

        // Skip clean files (they don't need to be in the cache)
        if status != GitStatus::Clean {
            cache.insert(path.to_string(), status);
        }
    }

    cache
}

/// Represents a single node in the file tree
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Unique identifier
    pub id: NodeId,
    /// Display name (file/folder name)
    pub name: String,
    /// Full path to the file/folder
    pub path: Utf8PathBuf,
    /// Node type for icon/color selection
    pub kind: NodeKind,
    /// Git status (if available)
    pub git_status: Option<GitStatus>,
    /// Children (lazily loaded for directories)
    pub children: Vec<TreeNode>,
    /// Is this node expanded? (only relevant for directories)
    pub expanded: bool,
    /// Depth in the tree (0 = root)
    pub depth: usize,
}

impl TreeNode {
    /// Create a new tree node from a path (without children)
    ///
    /// Use `build_tree()` for recursive tree building with children.
    pub fn from_path(path: &Utf8Path, depth: usize) -> Self {
        let name = path.file_name().unwrap_or(path.as_str()).to_string();
        let kind = NodeKind::from_path(path);
        let id = Self::generate_id(path);

        Self {
            id,
            name,
            path: path.to_path_buf(),
            kind,
            git_status: None,
            children: Vec::new(),
            expanded: false,
            depth,
        }
    }

    /// Build a complete file tree recursively from a root path
    ///
    /// This is the primary way to create a populated tree. Features:
    /// - Recursively populates children up to `max_depth`
    /// - Applies git status from cache
    /// - Handles permission errors gracefully (skips unreadable dirs)
    /// - Handles edge cases (empty dirs, missing dirs)
    ///
    /// # Arguments
    /// - `root`: The root directory path
    /// - `git_cache`: Optional git status cache (use `build_git_status_cache()`)
    /// - `max_depth`: Maximum recursion depth (default: `MAX_TREE_DEPTH`)
    ///
    /// # Example
    /// ```ignore
    /// use camino::Utf8Path;
    /// let root = Utf8Path::new(".");
    /// let git_cache = build_git_status_cache(root);
    /// let tree = TreeNode::build_tree(root, Some(&git_cache), None);
    /// ```
    pub fn build_tree(
        root: &Utf8Path,
        git_cache: Option<&GitStatusCache>,
        max_depth: Option<usize>,
    ) -> Self {
        let max_depth = max_depth.unwrap_or(MAX_TREE_DEPTH);
        Self::build_tree_recursive(root, root, git_cache, 0, max_depth)
    }

    /// Internal recursive tree builder
    fn build_tree_recursive(
        root: &Utf8Path,
        path: &Utf8Path,
        git_cache: Option<&GitStatusCache>,
        depth: usize,
        max_depth: usize,
    ) -> Self {
        let mut node = Self::from_path(path, depth);

        // Apply git status from cache
        if let Some(cache) = git_cache {
            // Get relative path from root for git status lookup
            if let Ok(rel_path) = path.strip_prefix(root) {
                if let Some(&status) = cache.get(rel_path.as_str()) {
                    node.git_status = Some(status);
                }
            }
        }

        // If directory and under max depth, populate children
        // Skip loading children for heavy directories (target/, node_modules/, etc.)
        // These directories can contain thousands of files and cause multi-second startup delays.
        // The directories are still shown in the tree but collapsed with no children loaded.
        let is_heavy_dir = HEAVY_DIRECTORIES.contains(&node.name.as_str());
        if node.kind.is_directory() && depth < max_depth && !is_heavy_dir {
            node.children = Self::load_children(root, path, git_cache, depth, max_depth);
            node.sort_children();
        }

        node
    }

    /// Load children of a directory with error handling
    ///
    /// Returns empty vec if:
    /// - Path doesn't exist
    /// - Permission denied
    /// - Not a directory
    fn load_children(
        root: &Utf8Path,
        parent: &Utf8Path,
        git_cache: Option<&GitStatusCache>,
        parent_depth: usize,
        max_depth: usize,
    ) -> Vec<TreeNode> {
        let child_depth = parent_depth + 1;

        // Try to read directory - gracefully handle errors
        let entries = match std::fs::read_dir(parent.as_std_path()) {
            Ok(entries) => entries,
            Err(e) => {
                // Log permission errors for debugging (in debug builds)
                #[cfg(debug_assertions)]
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    tracing::debug!("Permission denied reading: {}", parent);
                }
                // Suppress unused variable warning in release builds
                let _ = e;
                return Vec::new();
            }
        };

        let mut children = Vec::new();

        for entry in entries.flatten() {
            // Security: Skip symlinks to prevent traversal outside project root
            if let Ok(ft) = entry.file_type() {
                if ft.is_symlink() {
                    continue;
                }
            }

            // Convert to UTF-8 path (skip non-UTF-8 paths)
            let entry_path = match Utf8PathBuf::try_from(entry.path()) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let child =
                Self::build_tree_recursive(root, &entry_path, git_cache, child_depth, max_depth);
            children.push(child);
        }

        children
    }

    /// Generate a unique ID from a path (using xxhash)
    fn generate_id(path: &Utf8Path) -> NodeId {
        use xxhash_rust::xxh3::xxh3_64;
        xxh3_64(path.as_str().as_bytes())
    }

    /// Check if this node is a directory
    pub fn is_directory(&self) -> bool {
        self.kind.is_directory()
    }

    /// Check if this node is hidden by default
    pub fn is_hidden(&self) -> bool {
        self.kind.is_hidden() || self.name.starts_with('.')
    }

    /// Check if this is an ecosystem file (gets premium treatment)
    pub fn is_ecosystem(&self) -> bool {
        self.kind.is_ecosystem()
    }

    /// Sort children: directories first, then by name (case-insensitive)
    pub fn sort_children(&mut self) {
        self.children
            .sort_by(|a, b| match (a.is_directory(), b.is_directory()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
    }

    /// Check if this directory has any children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Count total nodes in tree (including self)
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    /// Check if a .nika folder exists in this tree
    pub fn has_nika_folder(&self) -> bool {
        if self.kind == NodeKind::NikaFolder {
            return true;
        }
        self.children.iter().any(|c| c.has_nika_folder())
    }

    /// Check if any .nika.yaml workflows exist in this tree
    pub fn has_workflows(&self) -> bool {
        if self.kind == NodeKind::NikaWorkflow {
            return true;
        }
        self.children.iter().any(|c| c.has_workflows())
    }

    /// Find a node by path in the tree
    pub fn find_by_path(&self, target: &Utf8Path) -> Option<&TreeNode> {
        if self.path == target {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_path(target) {
                return Some(found);
            }
        }
        None
    }

    /// Get all files with a specific git status
    pub fn files_with_status(&self, status: GitStatus) -> Vec<&TreeNode> {
        let mut result = Vec::new();
        self.collect_by_status(status, &mut result);
        result
    }

    fn collect_by_status<'a>(&'a self, status: GitStatus, result: &mut Vec<&'a TreeNode>) {
        if self.git_status == Some(status) {
            result.push(self);
        }
        for child in &self.children {
            child.collect_by_status(status, result);
        }
    }
}

/// Git status for a file/folder
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    /// File has been modified
    Modified,
    /// File has been added/staged
    Added,
    /// File has been deleted
    Deleted,
    /// File is untracked
    Untracked,
    /// File has conflicts
    Conflict,
    /// File is ignored
    Ignored,
    /// Clean (no changes)
    Clean,
}

impl GitStatus {
    /// Parse git status from `git status --porcelain` XY format
    ///
    /// The XY format uses two characters:
    /// - X: status in the index (staging area)
    /// - Y: status in the work tree
    ///
    /// Common patterns:
    /// - `M ` or ` M` = Modified
    /// - `A ` = Added (staged)
    /// - `D ` or ` D` = Deleted
    /// - `??` = Untracked
    /// - `UU`, `AA`, `DD` = Conflict
    /// - `!!` = Ignored
    #[must_use]
    pub fn from_status_chars(x: char, y: char) -> Self {
        match (x, y) {
            // Untracked
            ('?', '?') => Self::Untracked,
            // Ignored
            ('!', '!') => Self::Ignored,
            // Conflicts (various combinations)
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => Self::Conflict,
            // Added
            ('A', _) => Self::Added,
            // Deleted
            ('D', _) | (_, 'D') => Self::Deleted,
            // Modified (in index or work tree)
            ('M', _) | (_, 'M') | ('R', _) | ('C', _) => Self::Modified,
            // Clean (no changes)
            _ => Self::Clean,
        }
    }
}

/// Node type classification for icon/color selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    // ═══════════════════════════════════════════════════════════════════
    // SUPERNOVAE ECOSYSTEM (Premium treatment with glow animations)
    // ═══════════════════════════════════════════════════════════════════

    // NIKA (Body - Workflow Engine) 🦋
    /// .nika.yaml - Workflow file (Gold glow)
    NikaWorkflow,
    /// .nika/ - Project config folder (Teal glow)
    NikaFolder,
    /// .son - Agent definition file (Purple glow)
    SonAgent,
    /// .skill.md - Skill file (Cyan glow)
    SkillFile,
    /// workflows/ - Workflows folder
    WorkflowsFolder,
    /// agents/ - Agents folder
    AgentsFolder,
    /// skills/ - Skills folder
    SkillsFolder,

    // NOVANET (Brain - Knowledge Graph) 🧠
    /// .novanet/ - NovaNet config (Sky glow)
    NovanetFolder,
    /// brain/ - Schema YAML
    BrainFolder,
    /// models/ - Node/Arc classes
    ModelsFolder,
    /// seed/ - Cypher seeds
    SeedFolder,

    // CLAUDE CODE DX
    /// .claude/ - Claude Code config
    ClaudeFolder,
    /// CLAUDE.md - Claude context file
    ClaudeMd,

    // ═══════════════════════════════════════════════════════════════════
    // STANDARD DIRECTORIES
    // ═══════════════════════════════════════════════════════════════════
    /// Generic directory
    Directory,
    /// src/ - Source code
    SrcFolder,
    /// tests/ - Test files
    TestsFolder,
    /// docs/ - Documentation
    DocsFolder,
    /// examples/ - Examples
    ExamplesFolder,
    /// benches/ - Benchmarks
    BenchesFolder,

    // ═══════════════════════════════════════════════════════════════════
    // COMMON FILES
    // ═══════════════════════════════════════════════════════════════════
    /// Generic file
    File,
    /// .md - Markdown
    Markdown,
    /// .yaml/.yml - YAML
    Yaml,
    /// .toml - TOML config
    Toml,
    /// .json - JSON
    Json,
    /// .rs - Rust source
    Rust,
    /// .ts/.tsx - TypeScript
    TypeScript,
    /// .js/.jsx - JavaScript
    JavaScript,
    /// README.md
    Readme,
    /// CHANGELOG.md
    Changelog,
    /// ROADMAP.md
    Roadmap,
    /// Cargo.toml
    CargoToml,
    /// package.json
    PackageJson,

    // ═══════════════════════════════════════════════════════════════════
    // HIDDEN (dimmed, collapsed by default)
    // ═══════════════════════════════════════════════════════════════════
    /// .git/
    GitFolder,
    /// node_modules/
    NodeModules,
    /// target/
    TargetFolder,
}

impl NodeKind {
    /// Detect node kind from path
    pub fn from_path(path: &Utf8Path) -> Self {
        let name = path.file_name().unwrap_or("");
        let is_dir = path.is_dir();

        // ═══════════════════════════════════════════════════════════════════
        // SUPERNOVAE ECOSYSTEM DETECTION (Premium treatment)
        // ═══════════════════════════════════════════════════════════════════

        // NIKA (Body) 🦋
        if name.ends_with(".nika.yaml") {
            return Self::NikaWorkflow;
        }
        if name.ends_with(".son") {
            return Self::SonAgent;
        }
        if name.ends_with(".skill.md") {
            return Self::SkillFile;
        }
        if name == ".nika" && is_dir {
            return Self::NikaFolder;
        }

        // NOVANET (Brain) 🧠
        if name == ".novanet" && is_dir {
            return Self::NovanetFolder;
        }
        if name == "brain" && is_dir {
            return Self::BrainFolder;
        }

        // CLAUDE CODE DX
        if name == ".claude" && is_dir {
            return Self::ClaudeFolder;
        }
        if name == "CLAUDE.md" {
            return Self::ClaudeMd;
        }

        // ═══════════════════════════════════════════════════════════════════
        // SPECIAL FOLDERS
        // ═══════════════════════════════════════════════════════════════════
        if is_dir {
            return match name {
                // Ecosystem folders
                "workflows" => Self::WorkflowsFolder,
                "agents" => Self::AgentsFolder,
                "skills" => Self::SkillsFolder,
                "packages" => Self::Directory, // Generic packages folder
                "models" => Self::ModelsFolder,
                "seed" => Self::SeedFolder,

                // Standard folders
                "src" => Self::SrcFolder,
                "tests" => Self::TestsFolder,
                "docs" => Self::DocsFolder,
                "examples" => Self::ExamplesFolder,
                "benches" => Self::BenchesFolder,

                // Hidden folders
                ".git" => Self::GitFolder,
                "node_modules" => Self::NodeModules,
                "target" => Self::TargetFolder,

                _ => Self::Directory,
            };
        }

        // ═══════════════════════════════════════════════════════════════════
        // SPECIAL FILES
        // ═══════════════════════════════════════════════════════════════════
        match name {
            "README.md" => Self::Readme,
            "CHANGELOG.md" => Self::Changelog,
            "ROADMAP.md" => Self::Roadmap,
            "Cargo.toml" => Self::CargoToml,
            "package.json" => Self::PackageJson,
            _ => {
                // By extension
                let ext = path.extension().unwrap_or("");
                match ext {
                    "md" => Self::Markdown,
                    "yaml" | "yml" => Self::Yaml,
                    "toml" => Self::Toml,
                    "json" => Self::Json,
                    "rs" => Self::Rust,
                    "ts" | "tsx" => Self::TypeScript,
                    "js" | "jsx" => Self::JavaScript,
                    _ => Self::File,
                }
            }
        }
    }

    /// Get the icon for this node kind (emoji fallback)
    pub fn icon(&self) -> &'static str {
        match self {
            // ═══════════════════════════════════════════════════════════════
            // SUPERNOVAE ECOSYSTEM (Premium icons)
            // ═══════════════════════════════════════════════════════════════

            // NIKA 🦋
            Self::NikaWorkflow => "🦋",    // Butterfly - Nika mascot
            Self::NikaFolder => "📁",      // Folder with dot
            Self::SonAgent => "🐔",        // Space chicken
            Self::SkillFile => "📜",       // Scroll
            Self::WorkflowsFolder => "⚡", // Workflows
            Self::AgentsFolder => "🐔",    // Agents
            Self::SkillsFolder => "📚",    // Skills

            // NOVANET 🧠
            Self::NovanetFolder => "🧠", // Brain
            Self::BrainFolder => "🧠",   // Brain
            Self::ModelsFolder => "📐",  // Models
            Self::SeedFolder => "🌱",    // Seed

            // CLAUDE CODE DX
            Self::ClaudeFolder => "🤖", // Claude
            Self::ClaudeMd => "🤖",     // Claude

            // ═══════════════════════════════════════════════════════════════
            // STANDARD DIRECTORIES
            // ═══════════════════════════════════════════════════════════════
            Self::Directory => "📂",
            Self::SrcFolder => "📂",
            Self::TestsFolder => "🧪",
            Self::DocsFolder => "📚",
            Self::ExamplesFolder => "💡",
            Self::BenchesFolder => "📊",

            // ═══════════════════════════════════════════════════════════════
            // COMMON FILES
            // ═══════════════════════════════════════════════════════════════
            Self::Readme => "📖",
            Self::Changelog => "📋",
            Self::Roadmap => "🗺️",
            Self::CargoToml => "⚙️",
            Self::PackageJson => "📦",
            Self::Markdown => "📄",
            Self::Yaml => "📄",
            Self::Toml => "⚙️",
            Self::Json => "📄",
            Self::Rust => "🦀",
            Self::TypeScript => "📄",
            Self::JavaScript => "📄",
            Self::File => "📄",

            // ═══════════════════════════════════════════════════════════════
            // HIDDEN
            // ═══════════════════════════════════════════════════════════════
            Self::GitFolder => "📂",
            Self::NodeModules => "📦",
            Self::TargetFolder => "📂",
        }
    }

    /// Get the NerdFont icon for this node kind
    pub fn nerd_icon(&self) -> &'static str {
        match self {
            // NIKA
            Self::NikaWorkflow => "󰙨",
            Self::NikaFolder => "󱂵",
            Self::SonAgent => "󰚩",
            Self::SkillFile => "󰛨",
            Self::WorkflowsFolder => "󰙨",
            Self::AgentsFolder => "󰚩",
            Self::SkillsFolder => "󰛨",

            // NOVANET
            Self::NovanetFolder => "󰠗",
            Self::BrainFolder => "󰠗",
            Self::ModelsFolder => "󰠗",
            Self::SeedFolder => "󰆼",

            // CLAUDE
            Self::ClaudeFolder => "󰚩",
            Self::ClaudeMd => "󰚩",

            // DIRECTORIES
            Self::Directory => "",
            Self::SrcFolder => "",
            Self::TestsFolder => "󰙨",
            Self::DocsFolder => "󰈙",
            Self::ExamplesFolder => "󰉋",
            Self::BenchesFolder => "󰙨",

            // FILES
            Self::Readme => "󰍔",
            Self::Changelog => "󰋽",
            Self::Roadmap => "󰙨",
            Self::CargoToml => "",
            Self::PackageJson => "󰏖",
            Self::Markdown => "󰍔",
            Self::Yaml => "󰈙",
            Self::Toml => "",
            Self::Json => "󰘦",
            Self::Rust => "󱘗",
            Self::TypeScript => "󰛦",
            Self::JavaScript => "󰛦",
            Self::File => "󰈙",

            // HIDDEN
            Self::GitFolder => "",
            Self::NodeModules => "󰎙",
            Self::TargetFolder => "",
        }
    }

    /// Returns true if this node should get glow animation on hover
    pub fn is_ecosystem(&self) -> bool {
        matches!(
            self,
            // NIKA
            Self::NikaWorkflow
                | Self::NikaFolder
                | Self::SonAgent
                | Self::SkillFile
                // NOVANET
                | Self::NovanetFolder
                // CLAUDE
                | Self::ClaudeFolder
        )
    }

    /// Returns true if this node should be hidden by default
    pub fn is_hidden(&self) -> bool {
        matches!(
            self,
            Self::GitFolder | Self::NodeModules | Self::TargetFolder
        )
    }

    /// Returns true if this is a directory type
    pub fn is_directory(&self) -> bool {
        matches!(
            self,
            Self::NikaFolder
                | Self::NovanetFolder
                | Self::ClaudeFolder
                | Self::Directory
                | Self::SrcFolder
                | Self::TestsFolder
                | Self::DocsFolder
                | Self::ExamplesFolder
                | Self::BenchesFolder
                | Self::WorkflowsFolder
                | Self::AgentsFolder
                | Self::SkillsFolder
                | Self::BrainFolder
                | Self::ModelsFolder
                | Self::SeedFolder
                | Self::GitFolder
                | Self::NodeModules
                | Self::TargetFolder
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_kind_from_path_nika_workflow() {
        let path = Utf8Path::new("workflows/deploy.nika.yaml");
        assert_eq!(NodeKind::from_path(path), NodeKind::NikaWorkflow);
    }

    #[test]
    fn test_node_kind_from_path_son_agent() {
        let path = Utf8Path::new("agents/researcher.son");
        assert_eq!(NodeKind::from_path(path), NodeKind::SonAgent);
    }

    #[test]
    fn test_node_kind_from_path_skill_file() {
        let path = Utf8Path::new("skills/writing.skill.md");
        assert_eq!(NodeKind::from_path(path), NodeKind::SkillFile);
    }

    #[test]
    fn test_node_kind_from_path_rust_file() {
        let path = Utf8Path::new("src/main.rs");
        assert_eq!(NodeKind::from_path(path), NodeKind::Rust);
    }

    #[test]
    fn test_node_kind_from_path_readme() {
        let path = Utf8Path::new("README.md");
        assert_eq!(NodeKind::from_path(path), NodeKind::Readme);
    }

    #[test]
    fn test_node_kind_from_path_claude_md() {
        let path = Utf8Path::new("CLAUDE.md");
        assert_eq!(NodeKind::from_path(path), NodeKind::ClaudeMd);
    }

    #[test]
    fn test_node_kind_is_ecosystem() {
        assert!(NodeKind::NikaWorkflow.is_ecosystem());
        assert!(NodeKind::SonAgent.is_ecosystem());
        assert!(NodeKind::NikaFolder.is_ecosystem());
        assert!(NodeKind::NovanetFolder.is_ecosystem());
        assert!(!NodeKind::Rust.is_ecosystem());
        assert!(!NodeKind::Directory.is_ecosystem());
    }

    #[test]
    fn test_node_kind_is_hidden() {
        assert!(NodeKind::GitFolder.is_hidden());
        assert!(NodeKind::NodeModules.is_hidden());
        assert!(NodeKind::TargetFolder.is_hidden());
        assert!(!NodeKind::SrcFolder.is_hidden());
        assert!(!NodeKind::NikaWorkflow.is_hidden());
    }

    #[test]
    fn test_node_kind_is_directory() {
        assert!(NodeKind::Directory.is_directory());
        assert!(NodeKind::NikaFolder.is_directory());
        assert!(NodeKind::SrcFolder.is_directory());
        assert!(!NodeKind::NikaWorkflow.is_directory());
        assert!(!NodeKind::Rust.is_directory());
    }

    #[test]
    fn test_tree_node_from_path() {
        let path = Utf8Path::new("workflows/deploy.nika.yaml");
        let node = TreeNode::from_path(path, 1);

        assert_eq!(node.name, "deploy.nika.yaml");
        assert_eq!(node.kind, NodeKind::NikaWorkflow);
        assert_eq!(node.depth, 1);
        assert!(!node.expanded);
        assert!(node.is_ecosystem());
    }

    #[test]
    fn test_build_tree_creates_children() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create test structure
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("workflow.nika.yaml"), "workflow: test").unwrap();

        let root_utf8 = Utf8Path::from_path(root).unwrap();
        let tree = TreeNode::build_tree(root_utf8, None, None);

        // Root should have children
        assert!(tree.has_children());
        assert!(tree.node_count() >= 3); // root + src + main.rs + workflow.nika.yaml

        // Should find workflow file
        assert!(tree.has_workflows());
    }

    #[test]
    fn test_build_tree_respects_max_depth() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create deep nested structure
        let deep_path = root.join("a/b/c/d/e/f/g");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("deep.txt"), "deep").unwrap();

        let root_utf8 = Utf8Path::from_path(root).unwrap();

        // With max_depth=2, shouldn't go past a/b
        let tree = TreeNode::build_tree(root_utf8, None, Some(2));
        let total_depth = count_max_depth(&tree, 0);
        assert!(total_depth <= 2, "Max depth exceeded: {}", total_depth);
    }

    fn count_max_depth(node: &TreeNode, current: usize) -> usize {
        let mut max = current;
        for child in &node.children {
            max = max.max(count_max_depth(child, current + 1));
        }
        max
    }

    #[test]
    fn test_build_tree_handles_empty_dir() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();
        let root_utf8 = Utf8Path::from_path(root).unwrap();

        // Empty directory should build without panic
        let tree = TreeNode::build_tree(root_utf8, None, None);
        assert_eq!(tree.children.len(), 0);
        assert!(!tree.has_children());
    }

    #[test]
    fn test_build_tree_with_git_status() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();
        let root_utf8 = Utf8Path::from_path(root).unwrap();

        // Create a git status cache manually
        let mut cache = GitStatusCache::default();
        cache.insert("file.txt".to_string(), GitStatus::Modified);
        cache.insert("new.txt".to_string(), GitStatus::Added);

        // Build tree (won't match since temp dir is different, but tests the code path)
        let tree = TreeNode::build_tree(root_utf8, Some(&cache), None);
        assert!(tree.children.is_empty()); // Empty temp dir
    }

    #[test]
    fn test_has_nika_folder() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        // No .nika folder initially
        let root_utf8 = Utf8Path::from_path(root).unwrap();
        let tree = TreeNode::build_tree(root_utf8, None, None);
        assert!(!tree.has_nika_folder());

        // Add .nika folder
        fs::create_dir(root.join(".nika")).unwrap();
        let tree = TreeNode::build_tree(root_utf8, None, None);
        assert!(tree.has_nika_folder());
    }

    #[test]
    fn test_find_by_path() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let root_utf8 = Utf8Path::from_path(root).unwrap();
        let tree = TreeNode::build_tree(root_utf8, None, None);

        // Find existing path
        let src_path = root_utf8.join("src");
        let found = tree.find_by_path(&src_path);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "src");

        // Find non-existent path
        let missing_path = root_utf8.join("nonexistent");
        assert!(tree.find_by_path(&missing_path).is_none());
    }

    #[test]
    fn test_files_with_status() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        fs::write(root.join("file1.txt"), "content").unwrap();
        fs::write(root.join("file2.txt"), "content").unwrap();

        let root_utf8 = Utf8Path::from_path(root).unwrap();

        // Create cache with one modified file (use RELATIVE path like git does)
        let mut cache = GitStatusCache::default();
        cache.insert("file1.txt".to_string(), GitStatus::Modified);

        let tree = TreeNode::build_tree(root_utf8, Some(&cache), None);
        let modified_files = tree.files_with_status(GitStatus::Modified);

        assert_eq!(modified_files.len(), 1);
        assert!(modified_files[0].path.ends_with("file1.txt"));
    }

    #[test]
    fn test_git_status_parsing() {
        // Test the GitStatus parsing logic directly
        assert_eq!(GitStatus::from_status_chars('M', ' '), GitStatus::Modified);
        assert_eq!(GitStatus::from_status_chars(' ', 'M'), GitStatus::Modified);
        assert_eq!(GitStatus::from_status_chars('A', ' '), GitStatus::Added);
        assert_eq!(GitStatus::from_status_chars('?', '?'), GitStatus::Untracked);
        assert_eq!(GitStatus::from_status_chars('D', ' '), GitStatus::Deleted);
        assert_eq!(GitStatus::from_status_chars(' ', ' '), GitStatus::Clean);
    }

    #[test]
    fn test_heavy_directories_skipped() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create heavy directories with nested content
        let target_dir = root.join("target/debug/build/some-crate");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(target_dir.join("output.txt"), "build artifact").unwrap();

        let node_modules = root.join("node_modules/react/lib");
        fs::create_dir_all(&node_modules).unwrap();
        fs::write(node_modules.join("index.js"), "module.exports = {}").unwrap();

        let git_dir = root.join(".git/objects/pack");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("pack.idx"), "git data").unwrap();

        // Also create a normal src directory for comparison
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

        let root_utf8 = Utf8Path::from_path(root).unwrap();
        let tree = TreeNode::build_tree(root_utf8, None, None);

        // Heavy directories should exist but have NO children loaded
        let target = tree.find_by_path(&Utf8PathBuf::from(root.join("target").to_str().unwrap()));
        assert!(target.is_some(), "target directory should exist in tree");
        assert!(
            target.unwrap().children.is_empty(),
            "target should have no children (skipped)"
        );

        let node_mod = tree.find_by_path(&Utf8PathBuf::from(
            root.join("node_modules").to_str().unwrap(),
        ));
        assert!(node_mod.is_some(), "node_modules should exist in tree");
        assert!(
            node_mod.unwrap().children.is_empty(),
            "node_modules should have no children (skipped)"
        );

        let git = tree.find_by_path(&Utf8PathBuf::from(root.join(".git").to_str().unwrap()));
        assert!(git.is_some(), ".git should exist in tree");
        assert!(
            git.unwrap().children.is_empty(),
            ".git should have no children (skipped)"
        );

        // Normal src directory SHOULD have children
        let src = tree.find_by_path(&Utf8PathBuf::from(root.join("src").to_str().unwrap()));
        assert!(src.is_some(), "src should exist in tree");
        assert!(
            !src.unwrap().children.is_empty(),
            "src should have children"
        );
    }
}
