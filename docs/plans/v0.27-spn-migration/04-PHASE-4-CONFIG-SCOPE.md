# Phase 4: Config Scope System

## Overview

**Goal**: Implement three-level config hierarchy (Local → Team → Global).
**Lines**: ~400
**Types**: 5
**Tests**: 6

---

## Design

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  THREE-LEVEL CONFIG HIERARCHY                                                 ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │  RESOLUTION ORDER (innermost wins)                                       │  ║
║  │                                                                          │  ║
║  │  1. LOCAL   ./.nika/local.yaml       ← Wins (user-specific, gitignored) │  ║
║  │      ↑                                                                   │  ║
║  │  2. TEAM    ./nika.yaml              ← Project-shared (committed)       │  ║
║  │      ↑                                                                   │  ║
║  │  3. GLOBAL  ~/.nika/config.yaml      ← User defaults (home directory)   │  ║
║  │                                                                          │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  EXAMPLE:                                                                     ║
║                                                                               ║
║  # Global (~/.nika/config.yaml)         # Team (./nika.yaml)                 ║
║  provider: claude                        provider: openai   ← overrides      ║
║  theme: dark                             mcp:                                ║
║  editor:                                   servers:                          ║
║    indent_size: 4                            neo4j: ...    ← team-shared    ║
║                                                                               ║
║  # Local (./.nika/local.yaml)           # Result (merged)                    ║
║  provider: ollama   ← overrides          provider: ollama                    ║
║  editor:                                 theme: dark                         ║
║    theme: solarized ← overrides          editor:                             ║
║                                            indent_size: 4                    ║
║                                            theme: solarized                  ║
║                                          mcp:                                ║
║                                            servers:                          ║
║                                              neo4j: ...                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Config Scope Enum

```rust
// src/core/config_scope.rs

/// Configuration scope for layered settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigScope {
    /// User-specific, gitignored (./.nika/local.yaml)
    Local,
    /// Project-shared, committed (./nika.yaml)
    Team,
    /// User defaults (~/.nika/config.yaml)
    Global,
}

impl ConfigScope {
    /// Resolution order (innermost first)
    pub const RESOLUTION_ORDER: &'static [Self] = &[
        Self::Local,
        Self::Team,
        Self::Global,
    ];

    /// Get the config file path for this scope
    pub fn path(&self) -> Result<PathBuf, NikaError> {
        match self {
            Self::Local => {
                let cwd = std::env::current_dir()?;
                Ok(cwd.join(".nika").join("local.yaml"))
            }
            Self::Team => {
                let cwd = std::env::current_dir()?;
                Ok(cwd.join("nika.yaml"))
            }
            Self::Global => {
                let home = dirs::home_dir()
                    .ok_or_else(|| NikaError::ConfigError("HOME not found".into()))?;
                Ok(home.join(".nika").join("config.yaml"))
            }
        }
    }

    /// Check if this scope exists
    pub fn exists(&self) -> bool {
        self.path().map(|p| p.exists()).unwrap_or(false)
    }
}
```

---

## ConfigLoader

```rust
// src/core/config_loader.rs

use crate::core::ConfigScope;

/// Unified configuration with scope tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NikaConfig {
    /// Active provider
    #[serde(default)]
    pub provider: Option<String>,

    /// Model for native inference
    #[serde(default)]
    pub model: Option<String>,

    /// Editor settings
    #[serde(default)]
    pub editor: EditorConfig,

    /// MCP configuration
    #[serde(default)]
    pub mcp: Option<McpConfig>,

    /// Autonomy settings
    #[serde(default)]
    pub autonomy: AutonomyConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorConfig {
    pub theme: Option<String>,
    pub indent_size: Option<u8>,
    pub auto_format: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutonomyConfig {
    pub level: Option<AutonomyLevel>,
    pub approval_timeout_secs: Option<u64>,
}

/// Config loader with scope resolution.
pub struct ConfigLoader {
    /// Cached configs by scope
    cache: FxHashMap<ConfigScope, NikaConfig>,
}

impl ConfigLoader {
    /// Create new loader
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
        }
    }

    /// Load config from specific scope
    pub fn load_scope(&mut self, scope: ConfigScope) -> Result<Option<NikaConfig>, NikaError> {
        let path = scope.path()?;

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let config: NikaConfig = serde_yaml::from_str(&content)?;
        self.cache.insert(scope, config.clone());

        Ok(Some(config))
    }

    /// Load all scopes and merge
    pub fn load_merged(&mut self) -> Result<NikaConfig, NikaError> {
        let mut merged = NikaConfig::default();

        // Load in reverse order (global first, local last)
        for scope in ConfigScope::RESOLUTION_ORDER.iter().rev() {
            if let Some(config) = self.load_scope(*scope)? {
                merged = merged.merge(&config);
            }
        }

        Ok(merged)
    }

    /// Get a specific setting with scope tracking
    pub fn get_with_scope<T, F>(&self, getter: F) -> Option<(T, ConfigScope)>
    where
        F: Fn(&NikaConfig) -> Option<T>,
    {
        for scope in ConfigScope::RESOLUTION_ORDER.iter() {
            if let Some(config) = self.cache.get(scope) {
                if let Some(value) = getter(config) {
                    return Some((value, *scope));
                }
            }
        }
        None
    }
}

impl NikaConfig {
    /// Merge with another config (other takes precedence)
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            provider: other.provider.clone().or_else(|| self.provider.clone()),
            model: other.model.clone().or_else(|| self.model.clone()),
            editor: EditorConfig {
                theme: other.editor.theme.clone().or_else(|| self.editor.theme.clone()),
                indent_size: other.editor.indent_size.or(self.editor.indent_size),
                auto_format: other.editor.auto_format.or(self.editor.auto_format),
            },
            mcp: other.mcp.clone().or_else(|| self.mcp.clone()),
            autonomy: AutonomyConfig {
                level: other.autonomy.level.or(self.autonomy.level),
                approval_timeout_secs: other.autonomy.approval_timeout_secs
                    .or(self.autonomy.approval_timeout_secs),
            },
        }
    }
}
```

---

## CLI Integration

### `nika config` Command

```rust
// src/commands/config.rs

#[derive(Parser)]
pub struct ConfigCmd {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Subcommand)]
enum ConfigSubcommand {
    /// Show effective config
    Show,
    /// Show where a setting comes from
    Where { key: String },
    /// Set a value in a scope
    Set {
        key: String,
        value: String,
        #[arg(long, default_value = "local")]
        scope: ConfigScope,
    },
    /// Get a value
    Get { key: String },
    /// List all config files
    Files,
}

impl ConfigCmd {
    pub fn run(&self) -> Result<(), NikaError> {
        match &self.command {
            ConfigSubcommand::Show => {
                let mut loader = ConfigLoader::new();
                let config = loader.load_merged()?;
                println!("{}", serde_yaml::to_string(&config)?);
            }

            ConfigSubcommand::Where { key } => {
                let mut loader = ConfigLoader::new();
                loader.load_merged()?;

                let (value, scope) = match key.as_str() {
                    "provider" => loader.get_with_scope(|c| c.provider.clone()),
                    "model" => loader.get_with_scope(|c| c.model.clone()),
                    "editor.theme" => loader.get_with_scope(|c| c.editor.theme.clone()),
                    _ => return Err(NikaError::ConfigError(format!("Unknown key: {}", key))),
                }.ok_or_else(|| NikaError::ConfigError("Key not set".into()))?;

                println!("{} = {} (from {:?})", key, value, scope);
            }

            ConfigSubcommand::Files => {
                for scope in ConfigScope::RESOLUTION_ORDER {
                    let path = scope.path()?;
                    let exists = if path.exists() { "✓" } else { "✗" };
                    println!("[{}] {:?}: {}", exists, scope, path.display());
                }
            }

            _ => todo!(),
        }
        Ok(())
    }
}
```

---

## File Structure

```
src/core/
├── mod.rs              # Add exports
├── config_scope.rs     # 🆕 ConfigScope enum
└── config_loader.rs    # 🆕 ConfigLoader, NikaConfig

src/commands/
└── config.rs           # 🆕 nika config command
```

---

## Config File Examples

### Global (`~/.nika/config.yaml`)

```yaml
# User-wide defaults
provider: claude
model: claude-sonnet-4-6

editor:
  theme: dark
  indent_size: 2
  auto_format: true

autonomy:
  level: assisted
  approval_timeout_secs: 300
```

### Team (`./nika.yaml`)

```yaml
# Project-wide settings (committed to git)
provider: openai  # Override for this project

mcp:
  servers:
    neo4j:
      command: npx
      args: ["-y", "@neo4j/mcp-neo4j"]
      env:
        NEO4J_URI: bolt://localhost:7687
        NEO4J_USERNAME: neo4j
        NEO4J_PASSWORD: ${spn:neo4j}
    perplexity:
      command: npx
      args: ["-y", "@anthropic/mcp-server-perplexity"]
      env:
        PERPLEXITY_API_KEY: ${spn:perplexity}
```

### Local (`./.nika/local.yaml`)

```yaml
# User-specific overrides (gitignored)
provider: ollama  # I'm testing locally

editor:
  theme: solarized  # My preference

autonomy:
  level: full  # Trust me, I know what I'm doing
```

---

## TDD Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scope_resolution_order() {
        assert_eq!(
            ConfigScope::RESOLUTION_ORDER,
            &[ConfigScope::Local, ConfigScope::Team, ConfigScope::Global]
        );
    }

    #[test]
    fn test_config_merge() {
        let global = NikaConfig {
            provider: Some("claude".into()),
            model: Some("claude-sonnet".into()),
            editor: EditorConfig {
                theme: Some("dark".into()),
                indent_size: Some(4),
                auto_format: Some(true),
            },
            ..Default::default()
        };

        let local = NikaConfig {
            provider: Some("ollama".into()),  // Override
            editor: EditorConfig {
                theme: Some("solarized".into()),  // Override
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = global.merge(&local);

        assert_eq!(merged.provider, Some("ollama".into()));  // Local wins
        assert_eq!(merged.model, Some("claude-sonnet".into()));  // From global
        assert_eq!(merged.editor.theme, Some("solarized".into()));  // Local wins
        assert_eq!(merged.editor.indent_size, Some(4));  // From global
    }

    #[test]
    fn test_scope_path() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        let local_path = ConfigScope::Local.path().unwrap();
        assert!(local_path.ends_with(".nika/local.yaml"));

        let team_path = ConfigScope::Team.path().unwrap();
        assert!(team_path.ends_with("nika.yaml"));
    }

    #[test]
    fn test_loader_merged() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        // Create team config
        std::fs::write(
            temp.path().join("nika.yaml"),
            "provider: openai\neditor:\n  theme: light"
        ).unwrap();

        // Create local config
        std::fs::create_dir_all(temp.path().join(".nika")).unwrap();
        std::fs::write(
            temp.path().join(".nika/local.yaml"),
            "editor:\n  theme: dark"
        ).unwrap();

        let mut loader = ConfigLoader::new();
        let config = loader.load_merged().unwrap();

        assert_eq!(config.provider, Some("openai".into()));  // From team
        assert_eq!(config.editor.theme, Some("dark".into()));  // Local overrides
    }

    #[test]
    fn test_get_with_scope() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        // Create team config only
        std::fs::write(
            temp.path().join("nika.yaml"),
            "provider: openai"
        ).unwrap();

        let mut loader = ConfigLoader::new();
        loader.load_merged().unwrap();

        let (value, scope) = loader.get_with_scope(|c| c.provider.clone()).unwrap();
        assert_eq!(value, "openai");
        assert_eq!(scope, ConfigScope::Team);
    }

    #[test]
    fn test_scope_exists() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        assert!(!ConfigScope::Local.exists());
        assert!(!ConfigScope::Team.exists());

        std::fs::write(temp.path().join("nika.yaml"), "").unwrap();
        assert!(ConfigScope::Team.exists());
    }
}
```

---

## Integration Points

### Provider Resolution

```rust
// In RigProvider::auto()
impl RigProvider {
    pub fn auto() -> Option<Self> {
        let loader = ConfigLoader::new();
        let config = loader.load_merged().ok()?;

        // Use configured provider if set
        if let Some(provider) = &config.provider {
            return Self::from_name(provider);
        }

        // Otherwise, detect from environment
        Self::detect_from_env()
    }
}
```

### TUI Settings

```rust
// In TUI initialization
impl App {
    pub fn new() -> Result<Self, NikaError> {
        let loader = ConfigLoader::new();
        let config = loader.load_merged()?;

        let theme = config.editor.theme
            .map(|t| Theme::from_name(&t))
            .unwrap_or(Theme::Dark);

        Ok(Self {
            theme,
            // ...
        })
    }
}
```

---

## Estimated Effort

| Task | Hours |
|------|-------|
| ConfigScope enum | 0.5 |
| ConfigLoader | 2 |
| NikaConfig struct | 1 |
| CLI command | 1.5 |
| Tests | 1 |
| Integration | 1 |
| **Total** | **~7 hours** |

---

## Validation Checklist

- [ ] ConfigScope paths resolve correctly
- [ ] Merge precedence works (local > team > global)
- [ ] `nika config show` displays merged config
- [ ] `nika config where` shows correct scope
- [ ] `nika config files` lists all paths
- [ ] Provider auto-detection uses config
- [ ] TUI respects editor config
- [ ] Tests pass
- [ ] Documentation updated
