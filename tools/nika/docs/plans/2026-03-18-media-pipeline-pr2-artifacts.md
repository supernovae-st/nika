# PR2: Media Artifacts — Binary Format + CLI + E2E Integrity

> **Version:** v4.1
> **Date:** 2026-03-18
> **Branch:** `feat/media-artifacts`
> **Baseline:** After PR1 merged (Nika v0.30.5+, 5,261 tests, 36 EventKind variants)
> **Depends on:** PR1 (`feat/media-pipeline`) merged
> **Supersedes:** 2026-03-17-media-pipeline-pr3-artifacts.md
> **Scope:** ArtifactFormat::Binary + `write_binary()` method, template access via `with:` bindings, E2E integrity check, `nika media` CLI, MediaCleanup event.
> **Tests:** ~15 new
> **Commits:** 8

**Parent:** [Master Plan](./2026-03-18-media-pipeline-master-plan.md) | **Prev:** [PR1](./2026-03-17-media-pipeline-pr1-extraction.md)

---

## Bugs Fixed (v4.0 -> v4.1)

| # | Bug | Fix |
|---|-----|-----|
| B1 | `TemplateResolver::new()` signature wrong -- took `&vars` directly | Use `TemplateResolver::new(&task_id, &workflow_name).with_vars(vars.clone())?` |
| B2 | `validate_artifact_path()` arg order reversed | Correct: `validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))` |
| B3 | `self.base_dir` used everywhere instead of `self.artifact_dir` | Replace all `self.base_dir` with `self.artifact_dir` |
| B4 | `BinaryWriteRequest.data: Vec<u8>` forces full copy into memory | Replace with `source: BinarySource` enum; `write_binary()` does async CAS copy |
| B5 | `MediaRef.size` field name inconsistent | Standardize to `size_bytes` everywhere |
| B6 | `CasStore::list()` returns tuple `(String, String, u64, PathBuf)` | Use `CasEntry` struct fields: `entry.hash`, `entry.extension`, `entry.size_bytes`, `entry.path` |
| B7 | Dead checksum computation in `write_binary()` -- computed but never stored | Remove dead checksum OR add `checksum: Option<String>` to `WriteResult` |
| B8 | `handle_media_command()` has no access to EventLog | Accept `event_log: Option<&EventLog>` parameter |
| B9 | `test_write_binary_over_size_limit` uses fake 200-byte data, never tests real limit | Use `ArtifactWriter::new(dir, name).with_max_size(1024)` for real test |

---

## Commit 1: `feat(ast): add Binary variant to ArtifactFormat and OutputFormat`

### Files

- `src/ast/artifact.rs`
- `src/ast/output.rs` (if separate enum exists)

### ArtifactFormat

Add `Binary` variant to the existing enum (currently: `Text`, `Json`, `Yaml`):

```rust
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactFormat {
    #[default]
    Text,
    Json,
    Yaml,
    /// Binary format -- copies from CAS store to artifact path
    Binary,
}

impl ArtifactFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Binary => "bin",  // Default; actual ext comes from MediaRef
        }
    }
}

impl std::fmt::Display for ArtifactFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Yaml => write!(f, "yaml"),
            Self::Binary => write!(f, "binary"),
        }
    }
}
```

### OutputFormat

Add `Binary` variant to `OutputFormat` (currently: `Text`, `Json`, `Yaml`, `Markdown`).
Two separate enums exist -- both need the `Binary` variant:

- `ast::ArtifactFormat` -- used in YAML artifact declarations
- `ast::OutputFormat` -- used by `WriteResult.format`

```rust
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Yaml,
    Markdown,
    /// Binary format -- used for media artifact writes
    Binary,
}
```

Update serde `rename_all = "lowercase"` handles serialization for both enums automatically.

---

## Commit 2: `feat(io): add write_binary() to ArtifactWriter + align size limits`

### File: `src/io/writer.rs`

#### New enum: BinarySource

```rust
/// Source of binary data for a binary artifact write.
///
/// Prefer `CasPath` when the data is already in the CAS store (zero-copy via
/// async file copy). Use `Bytes` only for synthetic/test data not in CAS.
#[derive(Debug, Clone)]
pub enum BinarySource {
    /// Path to an existing CAS file -- write_binary() does async fs::copy
    CasPath(PathBuf),
    /// Raw bytes in memory (for tests or generated data)
    Bytes(Vec<u8>),
}
```

#### New struct: BinaryWriteRequest

```rust
/// Request to write a binary artifact from CAS or raw bytes
#[derive(Debug, Clone)]
pub struct BinaryWriteRequest {
    /// Task ID that produced this output
    pub task_id: String,
    /// Output path template (may contain `{{var}}` placeholders)
    pub output_path: String,
    /// Binary data source -- CAS path (async copy) or raw bytes
    pub source: BinarySource,
    /// MIME type of the data (e.g., "image/png")
    pub mime_type: String,
    /// Template variables for path resolution
    pub vars: HashMap<String, String>,
}
```

#### New method: write_binary()

```rust
impl ArtifactWriter {
    /// Write a binary artifact from a CAS file or raw bytes.
    ///
    /// Unlike `write()` which takes String content and validates JSON,
    /// this writes raw bytes directly. For `BinarySource::CasPath`, uses
    /// async `fs::copy` to avoid loading the full file into memory.
    /// For `BinarySource::Bytes`, uses `write_atomic` for crash safety.
    ///
    /// Uses BINARY_MAX_SIZE (100MB) instead of DEFAULT_MAX_SIZE (10MB).
    pub async fn write_binary(&self, request: BinaryWriteRequest) -> Result<WriteResult, NikaError> {
        // 1. Resolve output path template
        let resolver = TemplateResolver::new(&request.task_id, &self.workflow_name)
            .with_vars(request.vars.clone())?;
        let resolved_path = resolver.resolve(&request.output_path);

        // 2. Build full artifact path
        let artifact_path = self.artifact_dir.join(&resolved_path);

        // 3. Security: validate artifact path (no traversal)
        validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;

        // 4. Create parent dirs
        if let Some(parent) = artifact_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                NikaError::ArtifactWriteError {
                    path: artifact_path.display().to_string(),
                    reason: format!("failed to create directory: {}", e),
                }
            })?;
        }

        // 5. Write based on source type
        let data_size = match &request.source {
            BinarySource::CasPath(cas_path) => {
                // Get size before copy for limit check
                let meta = tokio::fs::metadata(cas_path).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: cas_path.display().to_string(),
                        reason: format!("CAS file not found: {}", e),
                    }
                })?;
                let size = meta.len();

                // Size check against BINARY_MAX_SIZE
                if size > self.max_binary_size() {
                    return Err(NikaError::ArtifactSizeExceeded {
                        path: artifact_path.display().to_string(),
                        size,
                        max_size: self.max_binary_size(),
                    });
                }

                // Async copy -- no full load into memory
                tokio::fs::copy(cas_path, &artifact_path).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: artifact_path.display().to_string(),
                        reason: format!("copy from CAS failed: {}", e),
                    }
                })?;

                size
            }
            BinarySource::Bytes(data) => {
                let size = data.len() as u64;

                // Size check against BINARY_MAX_SIZE
                if size > self.max_binary_size() {
                    return Err(NikaError::ArtifactSizeExceeded {
                        path: artifact_path.display().to_string(),
                        size,
                        max_size: self.max_binary_size(),
                    });
                }

                // Atomic write for crash safety
                write_atomic(&artifact_path, data).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: artifact_path.display().to_string(),
                        reason: e.to_string(),
                    }
                })?;

                size
            }
        };

        Ok(WriteResult {
            path: artifact_path,
            size: data_size,
            format: OutputFormat::Binary,
        })
    }
}
```

#### Size limit constants + configurable max

```rust
/// Maximum size for text/json/yaml artifacts (10 MB)
const DEFAULT_MAX_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum size for binary artifacts (100 MB)
/// Binary media files (images, audio) are typically larger than text.
const BINARY_MAX_SIZE: u64 = 100 * 1024 * 1024;

impl ArtifactWriter {
    /// Create a new ArtifactWriter with default limits.
    pub fn new(artifact_dir: &str, workflow_name: &str) -> Result<Self, NikaError> { ... }

    /// Override the binary max size (for testing).
    pub fn with_max_size(mut self, max: u64) -> Self {
        self.binary_max_size_override = Some(max);
        self
    }

    fn max_binary_size(&self) -> u64 {
        self.binary_max_size_override.unwrap_or(BINARY_MAX_SIZE)
    }
}
```

- `write()` continues to use `DEFAULT_MAX_SIZE` (10 MB) for text artifacts
- `write_binary()` uses `BINARY_MAX_SIZE` (100 MB) for binary artifacts, configurable via `with_max_size()`
- Document the distinction in both constants' doc comments

---

## Commit 3: `feat(binding): verify media template access via with: bindings`

### Scope

Ensure these `with:` bindings resolve correctly end-to-end. The resolution path goes through `resolve_path()` from PR1, but needs verification tests.

### Bindings to verify

| Binding | Resolves to |
|---------|-------------|
| `with: { img: generate.media[0].path }` | CAS file path |
| `with: { hash: generate.media[0].hash }` | blake3 hash string |
| `with: { mime: generate.media[0].mime_type }` | MIME type string |
| `with: { count: generate.media.length }` | Media count (number) |

### How it works

The `with:` binding system resolves `generate.media[0].path` via `RunContext.resolve_path()`, which was extended in PR1 to intercept the `"media"` segment and serialize `TaskResult.media` for JSONPath traversal.

### Verification tests

Write tests confirming:

1. `resolve_path("generate.media[0].path")` returns the CAS file path string
2. `resolve_path("generate.media[0].hash")` returns the blake3 hash string
3. `resolve_path("generate.media[0].mime_type")` returns the MIME type string
4. `resolve_path("generate.media.length")` or equivalent returns the count
5. Empty media (text-only tasks) returns empty array, indexed access returns None
6. Out-of-bounds index returns None gracefully

---

## Commit 4: `feat(runtime): E2E integrity check at workflow end (Layer 5)`

### File: `src/runtime/` (runner.rs or workflow completion handler)

### Behavior

After all tasks complete, before emitting `WorkflowCompleted`:

1. Collect all `MediaRef`s from all `TaskResult`s
2. For each: verify CAS file exists, verify size matches
3. If any mismatch: **log warning** (don't fail workflow -- media may have been cleaned)
4. Emit summary if any media was processed

### Implementation

```rust
/// Verify all MediaRefs in completed tasks still point to existing CAS files.
/// This is Layer 5 of defense-in-depth against silent data loss.
/// Warnings only -- does not fail the workflow.
async fn verify_media_integrity(run_context: &RunContext) -> Vec<String> {
    let mut warnings = Vec::new();

    for entry in run_context.results.iter() {
        let task_id = entry.key();
        let result = entry.value();

        for (i, media_ref) in result.media.iter().enumerate() {
            // Single metadata() call -- avoids TOCTOU between exists check and size check
            match tokio::fs::metadata(&media_ref.path).await {
                Ok(meta) => {
                    if meta.len() != media_ref.size_bytes {
                        warnings.push(format!(
                            "Task '{}' media[{}] size mismatch: expected {}, got {}",
                            task_id, i, media_ref.size_bytes, meta.len()
                        ));
                    }
                }
                Err(_) => {
                    warnings.push(format!(
                        "NIKA-253: Task '{}' media[{}] missing: {}",
                        task_id, i, media_ref.path.display()
                    ));
                }
            }
        }
    }

    if !warnings.is_empty() {
        tracing::warn!(
            count = warnings.len(),
            "Media integrity check found issues"
        );
        for w in &warnings {
            tracing::warn!("{}", w);
        }
    }

    warnings
}
```

Call site in workflow completion handler:

```rust
// After all tasks complete, before WorkflowCompleted event:
let integrity_warnings = verify_media_integrity(&run_context).await;
if !integrity_warnings.is_empty() {
    tracing::warn!(
        warnings = integrity_warnings.len(),
        "Media integrity issues found at workflow end"
    );
}
```

---

## Commit 5: `feat(cli): add nika media subcommand (list/stats/clean)`

### Files

- `src/cli/media.rs` (NEW)
- `src/main.rs` (add `Commands::Media` variant)

### Subcommand structure

```rust
// src/main.rs -- Commands enum
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Manage media files in the CAS store
    Media {
        #[command(subcommand)]
        action: cli::media::MediaAction,
    },
}

// Dispatch:
// Some(Commands::Media { action }) => {
//     cli::media::handle_media_command(action, event_log.as_ref()).await
// },
```

```rust
// src/cli/media.rs
#[derive(Subcommand)]
pub enum MediaAction {
    /// List stored media files
    List,

    /// Show store statistics (count, total size, by-type breakdown)
    Stats,

    /// Clean media files from the store
    Clean {
        /// Remove files older than this duration (e.g., "7d", "24h", "30m")
        #[arg(long)]
        older_than: Option<String>,

        /// Remove all stored media files
        #[arg(long)]
        all: bool,

        /// Dry run -- show what would be removed without deleting
        #[arg(long, short = 'n')]
        dry_run: bool,
    },
}
```

### Implementation

```rust
pub async fn handle_media_command(
    action: MediaAction,
    event_log: Option<&EventLog>,
) -> Result<(), NikaError> {
    let store = CasStore::workspace_default();

    match action {
        MediaAction::List => {
            let entries = store.list().await?;
            if entries.is_empty() {
                println!("No media files stored.");
                return Ok(());
            }
            println!("{:<64}  {:<6}  {:>10}  {}", "HASH", "EXT", "SIZE", "PATH");
            for entry in &entries {
                println!(
                    "{:<64}  {:<6}  {:>10}  {}",
                    entry.hash, entry.extension,
                    format_bytes(entry.size_bytes),
                    entry.path.display()
                );
            }
            println!(
                "\n{} files, {} total",
                entries.len(),
                format_bytes(entries.iter().map(|e| e.size_bytes).sum::<u64>())
            );
        }
        MediaAction::Stats => {
            let entries = store.list().await?;
            let total_size: u64 = entries.iter().map(|e| e.size_bytes).sum();

            // By-type breakdown
            let mut by_ext: std::collections::HashMap<String, (u32, u64)> =
                std::collections::HashMap::new();
            for entry in &entries {
                let bucket = by_ext.entry(entry.extension.clone()).or_insert((0, 0));
                bucket.0 += 1;
                bucket.1 += entry.size_bytes;
            }

            println!("Media store: .nika/media/store/");
            println!("Files:       {}", entries.len());
            println!("Total size:  {}", format_bytes(total_size));
            if !by_ext.is_empty() {
                println!("\nBy type:");
                let mut sorted: Vec<_> = by_ext.into_iter().collect();
                sorted.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
                for (ext, (count, size)) in sorted {
                    println!("  .{:<8} {} files, {}", ext, count, format_bytes(size));
                }
            }
        }
        MediaAction::Clean { older_than, all, dry_run } => {
            if all {
                if dry_run {
                    let entries = store.list().await?;
                    let total: u64 = entries.iter().map(|e| e.size_bytes).sum();
                    println!(
                        "Would remove {} files ({})",
                        entries.len(),
                        format_bytes(total)
                    );
                } else {
                    let (removed, freed) = store.clean_all().await?;
                    // Emit MediaCleanup event (see Commit 6)
                    if let Some(log) = event_log {
                        log.emit(EventKind::MediaCleanup {
                            removed_count: removed,
                            freed_bytes: freed,
                            policy: "all".into(),
                        });
                    }
                    println!(
                        "Removed {} files, freed {}",
                        removed,
                        format_bytes(freed)
                    );
                }
            } else if let Some(duration_str) = older_than {
                let duration = parse_duration(&duration_str)?;
                if dry_run {
                    println!("Would remove files older than {}", duration_str);
                } else {
                    let (removed, freed) = store.clean_older_than(duration).await?;
                    // Emit MediaCleanup event
                    if let Some(log) = event_log {
                        log.emit(EventKind::MediaCleanup {
                            removed_count: removed,
                            freed_bytes: freed,
                            policy: format!("older_than_{}", duration_str),
                        });
                    }
                    println!(
                        "Removed {} files, freed {}",
                        removed,
                        format_bytes(freed)
                    );
                }
            } else {
                println!("Specify --all or --older-than <duration>");
            }
        }
    }
    Ok(())
}
```

### Helper functions

```rust
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn parse_duration(s: &str) -> Result<std::time::Duration, NikaError> {
    let s = s.trim();
    if s.len() < 2 {
        return Err(NikaError::InvalidArgument {
            arg: "duration".into(),
            reason: format!("too short: '{}'", s),
        });
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().map_err(|_| NikaError::InvalidArgument {
        arg: "duration".into(),
        reason: format!("invalid number: '{}'", num_str),
    })?;
    let multiplier: u64 = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return Err(NikaError::InvalidArgument {
            arg: "duration".into(),
            reason: format!("unknown unit: '{}' (expected s/m/h/d)", unit),
        }),
    };
    // Use checked_mul to prevent arithmetic overflow on huge values
    let secs = num.checked_mul(multiplier).ok_or_else(|| NikaError::InvalidArgument {
        arg: "duration".into(),
        reason: format!("duration too large: '{}' overflows", s),
    })?;
    Ok(std::time::Duration::from_secs(secs))
}
```

### CAS store methods used (from PR1)

- `CasStore::workspace_default()` -- creates store at `.nika/media/store/`
- `CasStore::list()` -- returns `Vec<CasEntry>` where `CasEntry { hash, extension, size_bytes, path }`
- `CasStore::clean_all()` -- returns `(u32, u64)` (removed count, freed bytes)
- `CasStore::clean_older_than(duration)` -- returns `(u32, u64)`

---

## Commit 6: `feat(event): add MediaCleanup event (36 -> 37)`

### File: `src/event/log.rs`

### New variant

```rust
// ═══════════════════════════════════════════
// MEDIA EVENTS
// ═══════════════════════════════════════════
/// Media cleanup was performed (via CLI or automatic policy)
MediaCleanup {
    removed_count: u32,
    freed_bytes: u64,
    policy: String,              // "all" | "older_than_7d" | "manual"
},
```

### No task_id

This is a store-level operation, not task-scoped. Add to the `None` branch of `task_id()`:

```rust
| Self::MediaCleanup { .. } => None,
```

### Update variant count test

Update the `count_all_variants` guard test from 36 to 37:

```rust
// MEDIA (5) -- PR1: 4, PR2: 1
"MediaExtracted",
"MediaProcessed",
"MediaStored",
"MediaStoreFailed",
"MediaCleanup",          // NEW in PR2
```

```rust
assert_eq!(variants.len(), 37,
    "EventKind variant count changed! Update this test and task_id() match.");
```

### Emit in clean command handler

Event emission is now done inline in `handle_media_command()` (see Commit 5) via the
`event_log: Option<&EventLog>` parameter.

---

## Commit 7: `docs(examples): add binary workflow example`

### File: `examples/media-pipeline.nika.yaml`

```yaml
# Media pipeline example: invoke MCP tool, auto-capture media, save as binary artifact
name: media-pipeline
schema: "@0.12"

tasks:
  generate:
    invoke:
      tool: image_gen
      input:
        prompt: "A butterfly logo on dark background"
    # Media auto-captured from tool output into TaskResult.media[]

  save:
    depends_on: [generate]
    with:
      img_path: generate.media[0].path
      img_ext: generate.media[0].extension
      img_hash: generate.media[0].hash
      img_mime: generate.media[0].mime_type
    exec:
      command: "cp {{with.img_path}} output/logo.{{with.img_ext}}"
    artifact:
      path: "output/logo.{{with.img_ext}}"
      format: binary
```

**Notes:**
- Uses `with:` bindings (not direct `{{generate.media[0].path}}`) -- consistent with Nika's 3-pass template architecture
- The `with:` block resolves via `RunContext.resolve_path()` which handles the `media` segment
- Shows all four media binding types: path, extension, hash, mime_type

---

## Commit 8: `test(artifacts): comprehensive tests (~15 tests)`

### Test inventory

#### ArtifactFormat / OutputFormat (3 tests)

```rust
#[test]
fn test_artifact_format_binary_serde_roundtrip() {
    // JSON roundtrip
    let format: ArtifactFormat = serde_json::from_str("\"binary\"").unwrap();
    assert_eq!(format, ArtifactFormat::Binary);
    let json = serde_json::to_string(&ArtifactFormat::Binary).unwrap();
    assert_eq!(json, "\"binary\"");

    // YAML roundtrip
    let format: ArtifactFormat = serde_yaml::from_str("binary").unwrap();
    assert_eq!(format, ArtifactFormat::Binary);
}

#[test]
fn test_artifact_format_binary_extension_and_display() {
    assert_eq!(ArtifactFormat::Binary.extension(), "bin");
    assert_eq!(ArtifactFormat::Binary.to_string(), "binary");
}

#[test]
fn test_artifact_format_existing_variants_unchanged() {
    // Regression: existing formats must not change
    assert_eq!(ArtifactFormat::Text.extension(), "txt");
    assert_eq!(ArtifactFormat::Json.extension(), "json");
    assert_eq!(ArtifactFormat::Yaml.extension(), "yaml");
}
```

#### write_binary() (3 tests)

```rust
#[tokio::test]
async fn test_write_binary_small_data() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();

    // Write source data to a temp CAS file
    let cas_file = dir.path().join("cas_source.png");
    let data = b"fake png data";
    tokio::fs::write(&cas_file, data).await.unwrap();

    let writer = ArtifactWriter::new(art_dir.to_str().unwrap(), "test-workflow").unwrap();
    let request = BinaryWriteRequest {
        task_id: "gen".to_string(),
        output_path: "output/logo.png".to_string(),
        source: BinarySource::CasPath(cas_file),
        mime_type: "image/png".to_string(),
        vars: HashMap::new(),
    };

    let result = writer.write_binary(request).await.unwrap();
    assert!(result.path.exists());
    assert_eq!(result.size, data.len() as u64);
    assert_eq!(result.format, OutputFormat::Binary);

    // Verify content matches
    let written = tokio::fs::read(&result.path).await.unwrap();
    assert_eq!(written, data);
}

#[tokio::test]
async fn test_write_binary_large_data_near_limit() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();

    // Just under limit -- use Bytes source for simplicity
    let data = vec![0xFFu8; 1024 * 1024]; // 1 MB
    let writer = ArtifactWriter::new(art_dir.to_str().unwrap(), "test").unwrap();
    let request = BinaryWriteRequest {
        task_id: "t1".to_string(),
        output_path: "big.bin".to_string(),
        source: BinarySource::Bytes(data),
        mime_type: "application/octet-stream".to_string(),
        vars: HashMap::new(),
    };

    let result = writer.write_binary(request).await.unwrap();
    assert_eq!(result.size, 1024 * 1024);
}

#[tokio::test]
async fn test_write_binary_over_size_limit() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();

    // Create writer with a low max size to test the limit for real
    let writer = ArtifactWriter::new(art_dir.to_str().unwrap(), "test")
        .unwrap()
        .with_max_size(1024); // 1 KB limit

    let data = vec![0u8; 2048]; // 2 KB -- exceeds 1 KB limit
    let request = BinaryWriteRequest {
        task_id: "t1".to_string(),
        output_path: "huge.bin".to_string(),
        source: BinarySource::Bytes(data),
        mime_type: "image/png".to_string(),
        vars: HashMap::new(),
    };

    let result = writer.write_binary(request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, NikaError::ArtifactSizeExceeded { .. }));
}
```

#### Media template resolution via with: bindings (2 tests)

```rust
#[test]
fn test_media_template_resolution_via_with_bindings() {
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "abc123".to_string() + &"0".repeat(58),
            mime_type: "image/png".to_string(),
            media_type: MediaType::Image,
            size_bytes: 1024,
            path: PathBuf::from(".nika/media/store/ab/c123.png"),
            extension: "png".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("generate".into(), result);

    // CAS path
    let resolved = context.resolve_path("generate.media[0].path");
    assert_eq!(resolved.unwrap().as_str(), Some(".nika/media/store/ab/c123.png"));

    // blake3 hash
    let resolved = context.resolve_path("generate.media[0].hash");
    assert!(resolved.is_some());

    // MIME type
    let resolved = context.resolve_path("generate.media[0].mime_type");
    assert_eq!(resolved.unwrap().as_str(), Some("image/png"));

    // Full media array
    let resolved = context.resolve_path("generate.media");
    assert!(resolved.is_some());
    assert!(resolved.unwrap().is_array());
}

#[test]
fn test_media_template_resolution_empty_media() {
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1));
    let context = RunContext::new();
    context.insert("text_task".into(), result);

    // No media -> index returns None
    let resolved = context.resolve_path("text_task.media[0].path");
    assert!(resolved.is_none());

    // Media array itself should return empty array
    let resolved = context.resolve_path("text_task.media");
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().as_array().unwrap().len(), 0);
}
```

#### E2E integrity check (2 tests)

```rust
#[tokio::test]
async fn test_e2e_integrity_all_media_present() {
    let dir = tempfile::tempdir().unwrap();
    let cas_path = dir.path().join("test.png");
    tokio::fs::write(&cas_path, b"valid data").await.unwrap();

    let result = TaskResult::success(json!({}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: CasStore::hash(b"valid data"),
            mime_type: "image/png".to_string(),
            media_type: MediaType::Image,
            size_bytes: 10,
            path: cas_path,
            extension: "png".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert!(warnings.is_empty());
}

#[tokio::test]
async fn test_e2e_integrity_missing_cas_file_warns() {
    let result = TaskResult::success(json!({}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "nonexistent".to_string() + &"0".repeat(53),
            mime_type: "image/png".to_string(),
            media_type: MediaType::Image,
            size_bytes: 100,
            path: PathBuf::from("/tmp/does-not-exist-nika-test.png"),
            extension: "png".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("NIKA-253"));
}
```

#### CLI: media commands (3 tests)

```rust
#[tokio::test]
async fn test_media_list_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("empty_store"));
    let entries = store.list().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_media_stats_with_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    store.store(b"img1", &CasStore::hash(b"img1"), "png").await.unwrap();
    store.store(b"img2", &CasStore::hash(b"img2"), "jpg").await.unwrap();

    let entries = store.list().await.unwrap();
    assert_eq!(entries.len(), 2);

    let total_size: u64 = entries.iter().map(|e| e.size_bytes).sum();
    assert!(total_size > 0);
}

#[tokio::test]
async fn test_media_clean_all_removes_everything() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    store.store(b"img1", &CasStore::hash(b"img1"), "png").await.unwrap();
    store.store(b"img2", &CasStore::hash(b"img2"), "jpg").await.unwrap();
    store.store(b"img3", &CasStore::hash(b"img3"), "gif").await.unwrap();

    let (removed, _freed) = store.clean_all().await.unwrap();
    assert_eq!(removed, 3);
    assert!(store.list().await.unwrap().is_empty());
}
```

#### Event: MediaCleanup (2 tests)

```rust
#[test]
fn test_media_cleanup_event_serde_roundtrip() {
    let event = EventKind::MediaCleanup {
        removed_count: 3,
        freed_bytes: 4096,
        policy: "older_than_7d".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"media_cleanup\""));
    assert!(json.contains("\"removed_count\":3"));
    let parsed: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, event);
}

#[test]
fn test_media_cleanup_event_has_no_task_id() {
    let event = EventKind::MediaCleanup {
        removed_count: 5,
        freed_bytes: 1024 * 1024,
        policy: "all".into(),
    };
    assert_eq!(event.task_id(), None); // Store-level, not task-level
}
```

#### Variant count (1 test)

```rust
#[test]
fn count_all_variants() {
    let variants = [
        // WORKFLOW (6)
        "WorkflowStarted", "WorkflowCompleted", "WorkflowFailed",
        "WorkflowAborted", "WorkflowPaused", "WorkflowResumed",
        // TASK (4)
        "TaskScheduled", "TaskStarted", "TaskCompleted", "TaskFailed",
        // FINE-GRAINED (3)
        "TemplateResolved", "ProviderCalled", "ProviderResponded",
        // CONTEXT (1)
        "ContextAssembled",
        // MCP (5)
        "McpInvoke", "McpResponse", "McpConnected", "McpError", "McpRetry",
        // AGENT (4)
        "AgentStart", "AgentTurn", "AgentComplete", "AgentSpawned",
        // GUARDRAIL (3)
        "GuardrailPassed", "GuardrailFailed", "GuardrailEscalation",
        // BUILTIN (2)
        "Log", "Custom",
        // ARTIFACT (2)
        "ArtifactWritten", "ArtifactFailed",
        // STRUCTURED OUTPUT (2)
        "StructuredOutputAttempt", "StructuredOutputSuccess",
        // MEDIA (5) -- PR1: 4, PR2: 1
        "MediaExtracted", "MediaProcessed", "MediaStored", "MediaStoreFailed",
        "MediaCleanup",
    ];
    assert_eq!(variants.len(), 37,
        "EventKind variant count changed! Update this test and task_id() match.");
}
```

### Test summary: 15 tests total

| Category | Count | Tests |
|----------|-------|-------|
| ArtifactFormat::Binary serde + display | 3 | roundtrip, extension+display, existing unchanged |
| write_binary() | 3 | small data (CasPath), large data near limit (Bytes), over size limit (with_max_size) |
| Media template resolution | 2 | with bindings, empty media |
| E2E integrity check | 2 | all present (pass), missing CAS file (warning) |
| CLI media commands | 3 | list empty, stats with entries, clean --all |
| MediaCleanup event | 2 | serde roundtrip, no task_id |

Plus the updated `count_all_variants` guard test (37).

---

## Commit Sequence

```
1. feat(ast): add Binary variant to ArtifactFormat and OutputFormat
2. feat(io): add write_binary() to ArtifactWriter + align size limits
3. feat(binding): verify media template access via with: bindings
4. feat(runtime): E2E integrity check at workflow end (Layer 5)
5. feat(cli): add nika media subcommand (list/stats/clean)
6. feat(event): add MediaCleanup event (36 -> 37)
7. docs(examples): add binary workflow example
8. test(artifacts): comprehensive tests (~15 tests)
```

---

## Verification Checklist

- [ ] `cargo test` -- all tests pass
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] `ArtifactFormat::Binary` serializes to `"binary"` in JSON and YAML
- [ ] `OutputFormat::Binary` serializes to `"binary"`
- [ ] `write_binary()` writes via async CAS copy (CasPath) or atomic bytes
- [ ] `write_binary()` uses `BINARY_MAX_SIZE` (100 MB), not `DEFAULT_MAX_SIZE` (10 MB)
- [ ] `write_binary()` over size limit returns `ArtifactSizeExceeded` (tested via `with_max_size`)
- [ ] `with: { img: generate.media[0].path }` + `{{with.img}}` resolves CAS path
- [ ] `with: { hash: generate.media[0].hash }` + `{{with.hash}}` resolves blake3 hash
- [ ] `with: { mime: generate.media[0].mime_type }` + `{{with.mime}}` resolves MIME
- [ ] Empty media (text-only tasks) returns empty array, index returns None
- [ ] Layer 5: E2E integrity warns on missing CAS file (does not fail workflow)
- [ ] Layer 5: E2E integrity passes when all media present
- [ ] `nika media list` shows stored files with hash, ext, size_bytes, path (via CasEntry)
- [ ] `nika media stats` shows count, total size, by-type breakdown
- [ ] `nika media clean --all` removes all files
- [ ] `nika media clean --older-than 7d` removes old files
- [ ] `nika media clean --all --dry-run` shows count + size without deleting
- [ ] `MediaCleanup` event emitted after clean operations (via event_log param)
- [ ] `MediaCleanup` event has `task_id() -> None` (store-level, not task-level)
- [ ] `MediaCleanup` serde roundtrip correct (`"type":"media_cleanup"`)
- [ ] Event variant count guard test updated: 37
- [ ] Existing text-only artifact workflows unchanged (regression)
- [ ] Merge to main, delete `feat/media-artifacts` branch

---

## End State

After PR1 + PR2 are merged, Nika has the complete media pipeline:

```
MCP Server returns image
  -> rmcp_adapter extracts ALL content types (PR1)
  -> MediaExtracted event emitted (PR1)
  -> MediaProcessor decodes base64 (Layer 2: decode verification) (PR1)
  -> MIME detected via magic bytes + cross-validated vs server (PR1)
  -> blake3 hash computed, CAS store with atomic write (PR1)
  -> Read-back verification (Layer 3: re-hash stored file) (PR1)
  -> MediaProcessed + MediaStored events emitted (PR1)
  -> MediaRef stored in TaskResult.media[] (PR1)
  -> with: bindings resolve media refs via resolve_path() (PR2 verifies)
  -> ArtifactFormat::Binary + write_binary() for binary artifacts (PR2)
  -> Layer 5: E2E integrity check at workflow end (PR2)
  -> nika media list/stats/clean manages lifecycle (PR2)
  -> MediaCleanup event emitted on clean operations (PR2)
```

**5 defense-in-depth layers. 5 new EventKind variants (37 total). Zero breaking changes. Fully backward compatible.**

---

## Changes from Superseded Plan

| Item | Old (PR3 in v3.1) | New (PR2 in v4.1) |
|------|-------------------|-------------------|
| Branch name | `feat/media-artifacts` | `feat/media-artifacts` (unchanged) |
| Depends on | "PR2 merged" (processor) | "PR1 merged" (combined extraction+processing) |
| Baseline tests | Not specified | 5,261 tests |
| Baseline events | 36 (PR2 added 3) | 36 (PR1 added 4) |
| BinaryWriteRequest.data | `data: Vec<u8>` | `source: BinarySource` enum (CasPath / Bytes) |
| Size limit | Single `max_size` | `DEFAULT_MAX_SIZE` (10 MB text) + `BINARY_MAX_SIZE` (100 MB binary) |
| Size limit testing | Untestable (100 MB constant) | `with_max_size()` builder for real limit tests |
| Checksum | Computed then discarded | Removed dead code (no checksum on WriteResult) |
| TemplateResolver | `TemplateResolver::new(&vars)` | `TemplateResolver::new(&task_id, &name).with_vars(vars)?` |
| validate_artifact_path | Args reversed | `validate_artifact_path(&self.artifact_dir, Path::new(&resolved))` |
| ArtifactWriter field | `self.base_dir` | `self.artifact_dir` everywhere |
| CasStore::list() return | `Vec<(String, String, u64, PathBuf)>` tuple | `Vec<CasEntry>` struct |
| handle_media_command() | No EventLog access | `event_log: Option<&EventLog>` parameter |
| Commit count | ~6-8 | 8 (explicit) |
| Test count | ~12-18 | ~15 (explicit inventory) |
| CLI stats | Count + total size only | Count + total size + by-type breakdown |
