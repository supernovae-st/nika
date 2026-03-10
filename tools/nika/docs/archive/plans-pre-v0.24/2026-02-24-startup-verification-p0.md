# Startup Verification P0 Implementation Plan

**Version:** v0.8.4
**Date:** 2026-02-24
**Status:** In Progress

## Overview

Implement 5 HIGH PRIORITY startup verification features to ensure robust TUI initialization.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  STARTUP SEQUENCE (run_unified)                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. verify_startup_environment()  ← NEW                                     │
│     ├── ensure_directories()      [P0-1] .nika/, sessions/, traces/         │
│     ├── verify_schema()           [P0-2] schema file readable               │
│     ├── load_config_graceful()    [P0-3] config.toml with fallback          │
│     └── verify_project_access()   [P0-5] home directory readable            │
│                                                                             │
│  2. init_mcp_clients()            (existing)                                │
│                                                                             │
│  3. spawn_provider_verification() (existing)                                │
│     └── with_timeout(5s)          [P0-4] fallback UI on timeout             │
│                                                                             │
│  4. spawn_mcp_verification()      (existing)                                │
│                                                                             │
│  5. init_terminal()               (existing)                                │
│                                                                             │
│  6. Main event loop               (existing)                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Implementation Tasks

### P0-1: Directory Creation (~1h)

**Goal:** Ensure `.nika/`, `.nika/sessions/`, `.nika/traces/` exist before TUI starts.

**Files to modify:**
- `src/tui/startup.rs` (NEW) - Startup verification module
- `src/tui/mod.rs` - Add module export
- `src/tui/app.rs` - Call `ensure_directories()` in `run_unified()`

**Implementation:**

```rust
// src/tui/startup.rs
use std::path::PathBuf;
use std::fs;
use crate::error::{NikaError, Result};

/// Directories required for Nika TUI operation
pub const NIKA_DIR: &str = ".nika";
pub const SESSIONS_DIR: &str = "sessions";
pub const TRACES_DIR: &str = "traces";

/// Ensure all required directories exist, creating them if needed
pub fn ensure_directories() -> Result<DirectoryReport> {
    let cwd = std::env::current_dir().map_err(|e| NikaError::StartupError {
        phase: "directory_check".into(),
        reason: format!("Cannot access current directory: {}", e),
    })?;

    let nika_dir = cwd.join(NIKA_DIR);
    let sessions_dir = nika_dir.join(SESSIONS_DIR);
    let traces_dir = nika_dir.join(TRACES_DIR);

    let mut report = DirectoryReport::default();

    // Create directories (idempotent)
    for (path, name) in [
        (&nika_dir, "nika"),
        (&sessions_dir, "sessions"),
        (&traces_dir, "traces"),
    ] {
        match fs::create_dir_all(path) {
            Ok(_) => report.created.push(name.to_string()),
            Err(e) => report.errors.push(format!("{}: {}", name, e)),
        }
    }

    report.nika_dir = Some(nika_dir);
    Ok(report)
}

#[derive(Debug, Default)]
pub struct DirectoryReport {
    pub nika_dir: Option<PathBuf>,
    pub created: Vec<String>,
    pub errors: Vec<String>,
}

impl DirectoryReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}
```

**Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_directories_creates_all() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        let report = ensure_directories().unwrap();

        assert!(report.is_ok());
        assert!(temp.path().join(".nika").exists());
        assert!(temp.path().join(".nika/sessions").exists());
        assert!(temp.path().join(".nika/traces").exists());
    }

    #[test]
    fn test_ensure_directories_idempotent() {
        let temp = TempDir::new().unwrap();
        std::env::set_current_dir(&temp).unwrap();

        let report1 = ensure_directories().unwrap();
        let report2 = ensure_directories().unwrap();

        assert!(report1.is_ok());
        assert!(report2.is_ok());
    }
}
```

---

### P0-2: Schema File Validation (~1h)

**Goal:** Verify `schemas/nika-workflow.schema.json` is readable before TUI starts.

**Files to modify:**
- `src/tui/startup.rs` - Add `verify_schema()` function
- `src/ast/schema_validator.rs` - Add `schema_path()` method

**Implementation:**

```rust
// In src/tui/startup.rs
use crate::ast::schema_validator::WorkflowSchemaValidator;

/// Verify schema file is readable
pub fn verify_schema() -> Result<SchemaReport> {
    let mut report = SchemaReport::default();

    // Try to create validator (loads schema internally)
    match WorkflowSchemaValidator::new() {
        Ok(validator) => {
            report.schema_loaded = true;
            report.schema_path = validator.schema_path();
        }
        Err(e) => {
            report.error = Some(format!("Schema load failed: {}", e));
        }
    }

    Ok(report)
}

#[derive(Debug, Default)]
pub struct SchemaReport {
    pub schema_loaded: bool,
    pub schema_path: Option<PathBuf>,
    pub error: Option<String>,
}

impl SchemaReport {
    pub fn is_ok(&self) -> bool {
        self.schema_loaded && self.error.is_none()
    }
}
```

---

### P0-3: Config File Validation (~1h)

**Goal:** Load `.nika/config.toml` with graceful fallback to defaults.

**Files to modify:**
- `src/tui/startup.rs` - Add `load_config_graceful()` function
- `src/tui/config.rs` - Ensure `TuiConfig::load()` returns defaults on error

**Implementation:**

```rust
// In src/tui/startup.rs
use crate::tui::config::TuiConfig;

/// Load config with graceful fallback to defaults
pub fn load_config_graceful() -> ConfigReport {
    let mut report = ConfigReport::default();

    match TuiConfig::load() {
        Ok(config) => {
            report.loaded = true;
            report.config = config;
            report.source = ConfigSource::File;
        }
        Err(e) => {
            tracing::warn!("Config load failed, using defaults: {}", e);
            report.loaded = true;
            report.config = TuiConfig::default();
            report.source = ConfigSource::Default;
            report.warning = Some(format!("Using defaults: {}", e));
        }
    }

    report
}

#[derive(Debug, Default)]
pub struct ConfigReport {
    pub loaded: bool,
    pub config: TuiConfig,
    pub source: ConfigSource,
    pub warning: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    #[default]
    Default,
    File,
}
```

---

### P0-4: Provider Timeout Fallback (~2h)

**Goal:** Show fallback UI if ALL providers fail to verify within 5s.

**Files to modify:**
- `src/tui/app.rs` - Add timeout wrapper around provider verification
- `src/tui/widgets/provider_selector.rs` - Add "No providers available" state

**Implementation:**

```rust
// In src/tui/app.rs
const PROVIDER_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn provider verification with timeout fallback
fn spawn_provider_verification_with_timeout(&mut self) {
    let tx = self.stream_tx.clone();
    let cache = Arc::clone(&self.verification_cache);

    tokio::spawn(async move {
        let start = Instant::now();

        // Wait for at least one provider to verify
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let has_verified = {
                let cache = cache.lock();
                cache.has_any_verified_provider()
            };

            if has_verified {
                break;
            }

            if start.elapsed() > PROVIDER_VERIFICATION_TIMEOUT {
                // Send timeout event
                let _ = tx.send(StreamChunk::ProviderVerificationTimeout);
                break;
            }
        }
    });

    // Also spawn individual verifications
    self.spawn_provider_verification();
}
```

**New StreamChunk variant:**
```rust
// In src/provider/rig.rs
pub enum StreamChunk {
    // ... existing variants
    ProviderVerificationTimeout,
}
```

**UI handling in chat.rs:**
```rust
// Show warning banner if no providers verified after timeout
if self.all_providers_failed() {
    // Render warning: "No LLM providers configured. Press Cmd+P to configure."
}
```

---

### P0-5: Home Directory Access (~1h)

**Goal:** Verify project root is readable before creating HomeView.

**Files to modify:**
- `src/tui/startup.rs` - Add `verify_project_access()` function

**Implementation:**

```rust
// In src/tui/startup.rs

/// Verify project directory is accessible
pub fn verify_project_access() -> Result<ProjectReport> {
    let mut report = ProjectReport::default();

    let cwd = std::env::current_dir().map_err(|e| NikaError::StartupError {
        phase: "project_access".into(),
        reason: format!("Cannot access current directory: {}", e),
    })?;

    report.project_dir = Some(cwd.clone());

    // Check if we can read the directory
    match std::fs::read_dir(&cwd) {
        Ok(entries) => {
            report.readable = true;
            report.file_count = entries.count();
        }
        Err(e) => {
            report.error = Some(format!("Cannot read project directory: {}", e));
        }
    }

    // Check for .nika.yaml files
    report.workflow_count = glob::glob("**/*.nika.yaml")
        .map(|paths| paths.filter_map(Result::ok).count())
        .unwrap_or(0);

    Ok(report)
}

#[derive(Debug, Default)]
pub struct ProjectReport {
    pub project_dir: Option<PathBuf>,
    pub readable: bool,
    pub file_count: usize,
    pub workflow_count: usize,
    pub error: Option<String>,
}

impl ProjectReport {
    pub fn is_ok(&self) -> bool {
        self.readable && self.error.is_none()
    }
}
```

---

## Integration: StartupReport

```rust
// In src/tui/startup.rs

/// Combined startup verification report
#[derive(Debug, Default)]
pub struct StartupReport {
    pub directories: DirectoryReport,
    pub schema: SchemaReport,
    pub config: ConfigReport,
    pub project: ProjectReport,
    pub started_at: Instant,
    pub duration: Duration,
}

impl StartupReport {
    pub fn is_ok(&self) -> bool {
        self.directories.is_ok()
            && self.schema.is_ok()
            && self.project.is_ok()
        // config always "ok" due to fallback
    }

    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(w) = &self.config.warning {
            warnings.push(w.clone());
        }

        if !self.directories.errors.is_empty() {
            warnings.extend(self.directories.errors.clone());
        }

        warnings
    }
}

/// Run all startup verifications
pub fn verify_startup() -> Result<StartupReport> {
    let started_at = Instant::now();

    let directories = ensure_directories()?;
    let schema = verify_schema()?;
    let config = load_config_graceful();
    let project = verify_project_access()?;

    Ok(StartupReport {
        directories,
        schema,
        config,
        project,
        started_at,
        duration: started_at.elapsed(),
    })
}
```

---

## App Integration

```rust
// In src/tui/app.rs, run_unified()

pub async fn run_unified(&mut self) -> Result<()> {
    // === NEW: Startup verification ===
    let startup_report = startup::verify_startup()?;

    if !startup_report.is_ok() {
        return Err(NikaError::StartupError {
            phase: "verification".into(),
            reason: "Startup verification failed".into(),
        });
    }

    // Log warnings
    for warning in startup_report.warnings() {
        tracing::warn!("Startup warning: {}", warning);
    }

    // Apply loaded config
    self.apply_config(startup_report.config.config);

    // === Existing code ===
    self.init_mcp_clients().await?;
    self.spawn_provider_verification_with_timeout(); // Modified
    self.spawn_mcp_verification();

    // ... rest of run_unified
}
```

---

## Test Plan

| Test | File | Description |
|------|------|-------------|
| `test_ensure_directories_creates_all` | startup.rs | Creates .nika/, sessions/, traces/ |
| `test_ensure_directories_idempotent` | startup.rs | Safe to call multiple times |
| `test_verify_schema_success` | startup.rs | Schema file loads correctly |
| `test_verify_schema_missing` | startup.rs | Graceful error on missing schema |
| `test_load_config_graceful_defaults` | startup.rs | Returns defaults on missing config |
| `test_load_config_graceful_file` | startup.rs | Loads from file when present |
| `test_verify_project_access_readable` | startup.rs | Reports project as readable |
| `test_verify_project_access_workflow_count` | startup.rs | Counts .nika.yaml files |
| `test_startup_report_is_ok` | startup.rs | Combined report validation |
| `test_provider_timeout_fallback` | app.rs | UI shows warning after 5s timeout |

---

## Execution Order

1. **Create `src/tui/startup.rs`** with all types and functions
2. **Add module to `src/tui/mod.rs`**
3. **Add tests** (TDD - write first, run to fail)
4. **Implement functions** (make tests pass)
5. **Integrate in `app.rs`** (`run_unified()`)
6. **Add P0-4 timeout** (StreamChunk + UI handling)
7. **Run full test suite**
8. **Manual testing** with TUI

---

## Success Criteria

- [ ] All 10 tests pass
- [ ] TUI starts without crash on fresh project (no .nika/)
- [ ] Missing schema shows clear error message
- [ ] Missing config uses defaults with warning
- [ ] Provider timeout shows "No providers" banner
- [ ] Home view shows workflow count or "No workflows found"
