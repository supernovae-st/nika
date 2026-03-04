//! Tree Node Types
//!
//! Defines `TreeNode` and `NodeKind` for the enhanced tree view.
//! Includes SuperNovae ecosystem detection for .nika.yaml, .son, .spn/, .novanet/ files.

use camino::{Utf8Path, Utf8PathBuf};
use std::cmp::Ordering;

/// Unique identifier for a tree node
pub type NodeId = u64;

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
    /// Create a new tree node from a path
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

    // SPN CLI (System Tool) ⚡
    /// ~/.spn/ - Global SPN config (Violet glow)
    SpnFolder,
    /// manifest.toml - System manifest
    SpnManifest,
    /// mcp.yaml - MCP servers config
    SpnMcpConfig,
    /// packages/ - Installed packages
    SpnPackages,
    /// registry.yaml - Package registry
    SpnRegistry,
    /// env - Environment vars (secrets)
    SpnEnv,
    /// state.json - CLI state
    SpnState,

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

        // SPN CLI ⚡
        if name == ".spn" && is_dir {
            return Self::SpnFolder;
        }
        if name == "manifest.toml" {
            // Check if parent is .spn
            if let Some(parent) = path.parent() {
                if parent.file_name() == Some(".spn") {
                    return Self::SpnManifest;
                }
            }
        }
        if name == "mcp.yaml" {
            return Self::SpnMcpConfig;
        }
        if name == "registry.yaml" {
            return Self::SpnRegistry;
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
                "packages" => Self::SpnPackages,
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
            "env" => Self::SpnEnv,
            "state.json" => Self::SpnState,
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
            Self::NikaWorkflow => "✨",    // Gold sparkle
            Self::NikaFolder => "🦋",      // Butterfly
            Self::SonAgent => "🐔",        // Space chicken
            Self::SkillFile => "📜",       // Scroll
            Self::WorkflowsFolder => "⚡", // Workflows
            Self::AgentsFolder => "🐔",    // Agents
            Self::SkillsFolder => "📚",    // Skills

            // SPN CLI ⚡
            Self::SpnFolder => "⚡",    // SPN
            Self::SpnManifest => "📋",  // Manifest
            Self::SpnMcpConfig => "🔌", // MCP
            Self::SpnPackages => "📦",  // Packages
            Self::SpnRegistry => "📦",  // Registry
            Self::SpnEnv => "🔐",       // Secrets
            Self::SpnState => "📊",     // State

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

            // SPN
            Self::SpnFolder => "󰒓",
            Self::SpnManifest => "󰒓",
            Self::SpnMcpConfig => "󱐋",
            Self::SpnPackages => "󰏖",
            Self::SpnRegistry => "󰏖",
            Self::SpnEnv => "󰌆",
            Self::SpnState => "󰘦",

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
                // SPN
                | Self::SpnFolder
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
                | Self::SpnFolder
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
                | Self::SpnPackages
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
        assert!(NodeKind::SpnFolder.is_ecosystem());
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
}
