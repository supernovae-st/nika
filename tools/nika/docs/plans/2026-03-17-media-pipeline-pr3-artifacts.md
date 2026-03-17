# PR3: Artifact Binary Support + `nika media clean` (v3.1)

> **Branch**: `feat/media-artifacts`
> **Scope**: ArtifactFormat::Binary + `write_binary()` method, template access via `with:` bindings, `nika media clean` CLI, E2E integrity check, MediaCleanup event.
> **Tests**: ~12-18 tests
> **Commits**: ~6-8
> **Depends on**: PR2 merged to main

**Parent**: [Master Plan](./2026-03-17-media-pipeline-master-plan.md) | **Prev**: [PR2](./2026-03-17-media-pipeline-pr2-processor.md)

---

## Tasks

### Task 1: Add Binary variant to ArtifactFormat

**File**: `src/ast/artifact.rs`

> **Source of truth** (verified): Current `ArtifactFormat` has 3 variants: `Text` (default), `Json`, `Yaml`. Also has `extension()` and `Display` impl.

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
```

**Commit**: `feat(ast): add Binary variant to ArtifactFormat`

### Task 2: Add `write_binary()` to ArtifactWriter

**File**: `src/io/writer.rs`

> **Source of truth** (verified): `ArtifactWriter.write()` takes `WriteRequest` which has `content: String`. Binary data cannot flow through the String-based `write()` method. We need a separate `write_binary()` that copies from CAS path using `io::atomic::write_atomic()` with bytes.

Add a new method alongside the existing `write()`:

```rust
use crate::media::MediaRef;

/// Request to write a binary artifact from CAS store
#[derive(Debug, Clone)]
pub struct BinaryWriteRequest {
    /// Task ID that produced this output
    pub task_id: String,
    /// Output path template (may contain `{{var}}` placeholders)
    pub output_path: String,
    /// Source MediaRef from CAS store
    pub media_ref: MediaRef,
    /// Template variables for path resolution
    pub vars: HashMap<String, String>,
}

impl ArtifactWriter {
    // ... existing write() method unchanged ...

    /// Write a binary artifact by copying from CAS store to artifact path
    ///
    /// Unlike `write()` which takes String content, this copies raw bytes
    /// from the CAS store path. Uses io::atomic for crash safety.
    ///
    /// Defense-in-depth Layer 4: verifies destination exists and size matches.
    pub async fn write_binary(&self, request: BinaryWriteRequest) -> Result<WriteResult, NikaError> {
        // 1. Resolve output path template
        let resolver = TemplateResolver::new(&request.vars);
        let resolved_path = resolver.resolve(&request.output_path);

        // 2. Use media extension instead of default "bin"
        let resolved_path = if resolved_path.ends_with(".bin") {
            resolved_path.replace(".bin", &format!(".{}", request.media_ref.extension))
        } else {
            resolved_path
        };

        // 3. Build full artifact path
        let artifact_path = self.base_dir.join(&resolved_path);

        // 4. Security: validate artifact path
        validate_artifact_path(&artifact_path, &self.base_dir)?;

        // 5. Size check against max_size
        if request.media_ref.size_bytes > self.max_size {
            return Err(NikaError::ArtifactSizeExceeded {
                path: artifact_path.display().to_string(),
                size: request.media_ref.size_bytes,
                max: self.max_size,
            });
        }

        // 6. Read source from CAS store
        let source_data = tokio::fs::read(&request.media_ref.path).await.map_err(|e| {
            NikaError::ArtifactWriteError {
                path: request.media_ref.path.display().to_string(),
                reason: format!("failed to read from CAS: {}", e),
            }
        })?;

        // 7. Create parent dirs + atomic write
        if let Some(parent) = artifact_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                NikaError::ArtifactWriteError {
                    path: artifact_path.display().to_string(),
                    reason: format!("failed to create directory: {}", e),
                }
            })?;
        }
        write_atomic(&artifact_path, &source_data).await.map_err(|e| {
            NikaError::ArtifactWriteError {
                path: artifact_path.display().to_string(),
                reason: e.to_string(),
            }
        })?;

        // 8. Layer 4 defense-in-depth: verify destination
        let dest_meta = tokio::fs::metadata(&artifact_path).await.map_err(|e| {
            NikaError::ArtifactWriteError {
                path: artifact_path.display().to_string(),
                reason: format!("destination verification failed: {}", e),
            }
        })?;
        if dest_meta.len() != request.media_ref.size_bytes {
            return Err(NikaError::ArtifactWriteError {
                path: artifact_path.display().to_string(),
                reason: format!(
                    "size mismatch: expected {} bytes, got {} bytes",
                    request.media_ref.size_bytes,
                    dest_meta.len()
                ),
            });
        }

        Ok(WriteResult {
            path: artifact_path,
            size: dest_meta.len(),
            format: OutputFormat::Binary,
        })
    }
}
```

Also add `Binary` to `OutputFormat` (the writer's format enum, defined in `src/ast/output.rs`):

```rust
// src/ast/output.rs -- OutputFormat enum (currently: Text, Json, Yaml, Markdown)
// Add Binary variant:
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

**Note**: Two separate format enums exist:
- `ast::ArtifactFormat` (Text, Json, Yaml) — used in YAML artifact declarations
- `ast::OutputFormat` (Text, Json, Yaml, Markdown) — used by `WriteResult.format`
Both need the `Binary` variant. They are NOT the same type.

**Commit**: `feat(io): add write_binary() method to ArtifactWriter for CAS-to-artifact copy`

### Task 3: Verify template access for media references via `with:` bindings

**NOTE**: Media template access was already implemented in PR2 (Task 7: resolve_path() extension).
This task is about **verifying** it works end-to-end and adding any missing glue.

**Architecture**: Nika templates use 3 passes: `{{with.alias}}`, `{{context.*}}`, `{{inputs.*}}`.
There is NO `{{task.field}}` direct access. Media refs are accessed via `with:` bindings:

```yaml
# How media is accessed in workflows (via existing with: binding system):
tasks:
  save:
    depends_on: [generate]
    with:
      img_path: generate.media[0].path
      img_ext: generate.media[0].extension
      img_hash: generate.media[0].hash
    exec:
      command: "cp {{with.img_path}} output/logo.{{with.img_ext}}"
```

The `with:` binding resolves `generate.media[0].path` via `RunContext.resolve_path()`,
which was extended in PR2 to intercept the `"media"` segment and serialize `TaskResult.media`.

**Verify**:
1. `resolve_path("generate.media[0].path")` returns the CAS file path
2. `resolve_path("generate.media[0].hash")` returns the blake3 hash
3. `resolve_path("generate.media[0].extension")` returns the file extension
4. `resolve_path("generate.media")` returns the full media array
5. `resolve_path("generate.media[0]")` returns the first MediaRef as JSON
6. `resolve_path("text_task.media[0].path")` returns None (no media on text tasks)

**Commit**: `feat(binding): verify media reference template access via with: bindings`

### Task 4: E2E integrity check at workflow end (Layer 5 defense-in-depth)

**File**: `src/runtime/executor/` (workflow completion handler)

At workflow completion, verify all MediaRefs still point to existing CAS files:

```rust
/// Verify all MediaRefs in completed tasks still point to existing CAS files
/// This is Layer 5 of defense-in-depth against silent data loss.
async fn verify_media_integrity(run_context: &RunContext) -> Vec<String> {
    let mut warnings = Vec::new();

    for entry in run_context.results.iter() {
        let task_id = entry.key();
        let result = entry.value();

        for (i, media_ref) in result.media.iter().enumerate() {
            // Check file exists
            if !media_ref.path.exists() {
                warnings.push(format!(
                    "NIKA-253: Task '{}' media[{}] missing: {}",
                    task_id, i, media_ref.path.display()
                ));
                continue;
            }

            // Check file size matches
            if let Ok(meta) = tokio::fs::metadata(&media_ref.path).await {
                if meta.len() != media_ref.size_bytes {
                    warnings.push(format!(
                        "Task '{}' media[{}] size mismatch: expected {}, got {}",
                        task_id, i, media_ref.size_bytes, meta.len()
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

Call this after all tasks complete, before emitting `WorkflowCompleted`:
```rust
// In workflow completion handler:
let integrity_warnings = verify_media_integrity(&run_context).await;
if !integrity_warnings.is_empty() {
    tracing::warn!(
        warnings = integrity_warnings.len(),
        "Media integrity issues found at workflow end"
    );
}
```

**Commit**: `feat(runtime): add E2E media integrity check at workflow completion (Layer 5)`

### Task 5: `nika media` CLI command

**Files**: `src/main.rs` (add variant to `Commands` enum) + `src/cli/media.rs` (NEW handler module)

> **Codebase convention** (verified): CLI follows nested subcommand pattern.
> `Commands::Media { action }` in main.rs dispatches to `cli::media::handle_media_command(action)`.
> Handler returns `Result<(), NikaError>`. All CLI modules live in `src/cli/`.

Add a `media` subcommand to the CLI:

```rust
// In src/main.rs Commands enum:
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Manage media files in the CAS store
    Media {
        #[command(subcommand)]
        action: cli::media::MediaAction,
    },
}

// In src/main.rs dispatch:
// Some(Commands::Media { action }) => cli::media::handle_media_command(action).await,

// In src/cli/media.rs (NEW):
#[derive(Subcommand)]
pub enum MediaAction {
    /// List stored media files
    List,

    /// Show store statistics (count, total size)
    Stats,

    /// Clean media files
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

Implementation in `src/cli/media.rs`:

```rust
pub async fn handle_media_command(action: MediaAction) -> Result<(), NikaError> {
    let store = CasStore::workspace_default();

    match action {
        MediaAction::List => {
            let entries = store.list().await?;
            if entries.is_empty() {
                println!("No media files stored.");
                return Ok(());
            }
            println!("{:<64}  {:<6}  {:>10}  {}", "HASH", "EXT", "SIZE", "PATH");
            for (hash, ext, size, path) in &entries {
                println!(
                    "{:<64}  {:<6}  {:>10}  {}",
                    hash, ext,
                    format_bytes(*size),
                    path.display()
                );
            }
            println!("\n{} files, {} total", entries.len(), format_bytes(
                entries.iter().map(|(_, _, s, _)| s).sum::<u64>()
            ));
        }
        MediaAction::Stats => {
            let entries = store.list().await?;
            let total_size: u64 = entries.iter().map(|(_, _, s, _)| s).sum();
            println!("Media store: .nika/media/store/");
            println!("Files:       {}", entries.len());
            println!("Total size:  {}", format_bytes(total_size));
        }
        MediaAction::Clean { older_than, all, dry_run } => {
            let policy = if all { "all" } else { "older_than" };

            if all {
                if dry_run {
                    let entries = store.list().await?;
                    let total: u64 = entries.iter().map(|(_, _, s, _)| s).sum();
                    println!("Would remove {} files ({})", entries.len(), format_bytes(total));
                } else {
                    let (removed, freed) = store.clean_all().await?;
                    // Emit MediaCleanup event
                    println!("Removed {} files, freed {}", removed, format_bytes(freed));
                }
            } else if let Some(duration_str) = older_than {
                let duration = parse_duration(&duration_str)?;
                if dry_run {
                    println!("Would remove files older than {}", duration_str);
                } else {
                    let (removed, freed) = store.clean_older_than(duration).await?;
                    println!("Removed {} files, freed {}", removed, format_bytes(freed));
                }
            } else {
                println!("Specify --all or --older-than <duration>");
            }
        }
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 { return format!("{} B", bytes); }
    if bytes < 1024 * 1024 { return format!("{:.1} KB", bytes as f64 / 1024.0); }
    if bytes < 1024 * 1024 * 1024 { return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)); }
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
    match unit {
        "s" => Ok(std::time::Duration::from_secs(num)),
        "m" => Ok(std::time::Duration::from_secs(num * 60)),
        "h" => Ok(std::time::Duration::from_secs(num * 3600)),
        "d" => Ok(std::time::Duration::from_secs(num * 86400)),
        _ => Err(NikaError::InvalidArgument {
            arg: "duration".into(),
            reason: format!("unknown unit: '{}' (expected s/m/h/d)", unit),
        }),
    }
}
```

**Commit**: `feat(cli): add nika media list/stats/clean commands`

### Task 6: Add MediaCleanup telemetry event

**File**: `src/event/log.rs`

Add final media event variant:

```rust
/// Media cleanup was performed (via CLI or automatic policy)
MediaCleanup {
    removed_count: u32,
    freed_bytes: u64,
    policy: String,              // "all"|"older_than_7d"|"manual"
},
```

This variant does NOT have `task_id` (it's a store-level operation, not task-level).
Add to the `None` branch of `task_id()`:
```rust
| Self::MediaCleanup { .. } => None,
```

Update variant count test (36 from PR2 -> 37 after PR3).

Emit in the clean command handler:
```rust
// After clean operation:
self.event_log.emit(EventKind::MediaCleanup {
    removed_count: removed,
    freed_bytes: freed,
    policy: if all { "all".into() } else { format!("older_than_{}", duration_str) },
});
```

**Commit**: `feat(event): add MediaCleanup telemetry event`

### Task 7: YAML workflow example with binary artifact

Add to cookbook or test fixtures:

```yaml
# Example: Generate image and save as artifact
name: image-generation
schema: "@0.12"

tasks:
  generate:
    invoke:
      tool: image_gen
      input:
        prompt: "A butterfly logo"
    # Media auto-captured from tool output into TaskResult.media[]

  save:
    depends_on: [generate]
    with:
      img_path: generate.media[0].path
      img_ext: generate.media[0].extension
    exec:
      command: "cp {{with.img_path}} output/logo.{{with.img_ext}}"
    artifact:
      path: "output/logo.{{with.img_ext}}"
      format: binary
```

> **NOTE**: Uses `with:` bindings (not direct `{{generate.media[0].path}}`).
> This is consistent with Nika's 3-pass template architecture.
> The `with:` block resolves via `RunContext.resolve_path()` which handles the `media` segment.

**Commit**: `docs: add image generation workflow example`

### Task 8: Tests

#### ArtifactFormat tests:

```rust
#[test]
fn test_artifact_format_binary_serde_json() {
    let format: ArtifactFormat = serde_json::from_str("\"binary\"").unwrap();
    assert_eq!(format, ArtifactFormat::Binary);

    let json = serde_json::to_string(&ArtifactFormat::Binary).unwrap();
    assert_eq!(json, "\"binary\"");
}

#[test]
fn test_artifact_format_binary_serde_yaml() {
    let format: ArtifactFormat = serde_yaml::from_str("binary").unwrap();
    assert_eq!(format, ArtifactFormat::Binary);
}

#[test]
fn test_artifact_format_binary_extension() {
    assert_eq!(ArtifactFormat::Binary.extension(), "bin");
}

#[test]
fn test_artifact_format_binary_display() {
    assert_eq!(ArtifactFormat::Binary.to_string(), "binary");
}

#[test]
fn test_artifact_format_existing_unchanged() {
    // Regression: existing formats must not change
    assert_eq!(ArtifactFormat::Text.extension(), "txt");
    assert_eq!(ArtifactFormat::Json.extension(), "json");
    assert_eq!(ArtifactFormat::Yaml.extension(), "yaml");
}
```

#### write_binary() tests:

```rust
#[tokio::test]
async fn test_write_binary_copies_from_cas() {
    let dir = tempfile::tempdir().unwrap();
    let cas_dir = dir.path().join("cas");
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&cas_dir).await.unwrap();
    tokio::fs::create_dir_all(&art_dir).await.unwrap();

    // Write a fake CAS file
    let data = b"fake png data";
    let cas_path = cas_dir.join("test.png");
    tokio::fs::write(&cas_path, data).await.unwrap();

    let media_ref = MediaRef {
        hash: "abc123".to_string() + &"0".repeat(58),
        mime_type: "image/png".to_string(),
        media_type: MediaType::Image,
        size_bytes: data.len() as u64,
        path: cas_path,
        extension: "png".to_string(),
    };

    let writer = ArtifactWriter::new(art_dir.to_str().unwrap(), "test-workflow").unwrap();
    let request = BinaryWriteRequest {
        task_id: "gen".to_string(),
        output_path: "output/logo.png".to_string(),
        media_ref,
        vars: HashMap::new(),
    };

    let result = writer.write_binary(request).await.unwrap();
    assert!(result.path.exists());
    assert_eq!(result.size, data.len() as u64);

    // Verify content matches
    let written = tokio::fs::read(&result.path).await.unwrap();
    assert_eq!(written, data);
}

#[tokio::test]
async fn test_write_binary_size_mismatch_detected() {
    // Create CAS file with wrong size in MediaRef
    let dir = tempfile::tempdir().unwrap();
    let cas_path = dir.path().join("test.png");
    tokio::fs::write(&cas_path, b"short").await.unwrap();

    let media_ref = MediaRef {
        hash: "abc".to_string() + &"0".repeat(61),
        mime_type: "image/png".to_string(),
        media_type: MediaType::Image,
        size_bytes: 9999, // Wrong size!
        path: cas_path,
        extension: "png".to_string(),
    };

    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();
    let writer = ArtifactWriter::new(art_dir.to_str().unwrap(), "test").unwrap();
    let request = BinaryWriteRequest {
        task_id: "t1".to_string(),
        output_path: "out.png".to_string(),
        media_ref,
        vars: HashMap::new(),
    };

    let result = writer.write_binary(request).await;
    assert!(result.is_err()); // Size mismatch caught by Layer 4
}
```

#### Template access tests (via resolve_path, not resolve_media_path):

```rust
#[test]
fn test_media_resolve_path() {
    // Build a TaskResult with media
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "abc123".to_string() + &"0".repeat(58),
            mime_type: "image/png".to_string(),
            media_type: MediaType::Image,
            size_bytes: 1024,
            path: PathBuf::from(".nika/media/store/ab/c123.png"),
            extension: "png".to_string(),
        }]);

    // Verify media is accessible via resolve_path (used by with: bindings)
    let context = RunContext::new();
    context.store("gen_image".into(), result);

    // Full media array
    let resolved = context.resolve_path("gen_image.media");
    assert!(resolved.is_some());
    assert!(resolved.unwrap().is_array());

    // Indexed access
    let resolved = context.resolve_path("gen_image.media[0].hash");
    assert!(resolved.is_some());

    let resolved = context.resolve_path("gen_image.media[0].path");
    assert_eq!(resolved.unwrap().as_str(), Some(".nika/media/store/ab/c123.png"));

    let resolved = context.resolve_path("gen_image.media[0].extension");
    assert_eq!(resolved.unwrap().as_str(), Some("png"));
}

#[test]
fn test_media_resolve_path_empty_media() {
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1));
    let context = RunContext::new();
    context.store("text_task".into(), result);

    // No media -> media array is empty, index returns None
    let resolved = context.resolve_path("text_task.media[0].path");
    assert!(resolved.is_none());

    // But media array itself should return empty array
    let resolved = context.resolve_path("text_task.media");
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().as_array().unwrap().len(), 0);
}
```

#### E2E integrity test:

```rust
#[tokio::test]
async fn test_e2e_integrity_check_passes() {
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
    context.store("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert!(warnings.is_empty());
}

#[tokio::test]
async fn test_e2e_integrity_check_detects_missing() {
    let result = TaskResult::success(json!({}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "nonexistent".to_string() + &"0".repeat(53),
            mime_type: "image/png".to_string(),
            media_type: MediaType::Image,
            size_bytes: 100,
            path: PathBuf::from("/tmp/does-not-exist.png"),
            extension: "png".to_string(),
        }]);

    let context = RunContext::new();
    context.store("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("NIKA-253"));
}
```

#### CLI tests:

```rust
#[tokio::test]
async fn test_media_list_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("empty_store"));
    let entries = store.list().await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_media_clean_all() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("store"));
    store.store(b"img1", &CasStore::hash(b"img1"), "png").await.unwrap();
    store.store(b"img2", &CasStore::hash(b"img2"), "jpg").await.unwrap();
    store.store(b"img3", &CasStore::hash(b"img3"), "gif").await.unwrap();

    let (removed, _) = store.clean_all().await.unwrap();
    assert_eq!(removed, 3);
    assert!(store.list().await.unwrap().is_empty());
}

#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("7d").unwrap(), Duration::from_secs(7 * 86400));
    assert_eq!(parse_duration("24h").unwrap(), Duration::from_secs(24 * 3600));
    assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(30 * 60));
    assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
}

#[test]
fn test_parse_duration_invalid() {
    assert!(parse_duration("x").is_err());
    assert!(parse_duration("7x").is_err());
    assert!(parse_duration("").is_err());
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(1048576), "1.0 MB");
    assert_eq!(format_bytes(1073741824), "1.0 GB");
}
```

#### MediaCleanup event test:

```rust
#[test]
fn test_media_cleanup_event_has_no_task_id() {
    let event = EventKind::MediaCleanup {
        removed_count: 5,
        freed_bytes: 1024 * 1024,
        policy: "all".into(),
    };
    assert_eq!(event.task_id(), None); // Store-level, not task-level
}

#[test]
fn test_media_cleanup_event_serde() {
    let event = EventKind::MediaCleanup {
        removed_count: 3,
        freed_bytes: 4096,
        policy: "older_than_7d".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"media_cleanup\""));
    assert!(json.contains("\"removed_count\":3"));
}
```

**Commits**:
- `test(ast): add ArtifactFormat::Binary serde + display + regression tests`
- `test(io): add write_binary() CAS-to-artifact copy tests`
- `test(binding): add media template resolution tests`
- `test(runtime): add E2E media integrity check tests`
- `test(cli): add nika media command + duration parsing tests`
- `test(event): add MediaCleanup event tests`

---

## Verification Checklist

- [ ] `cargo test` -- all tests pass
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] `ArtifactFormat::Binary` serializes to `"binary"` in JSON and YAML
- [ ] `write_binary()` copies file from CAS to artifact path correctly
- [ ] Layer 4: destination size verification catches mismatches
- [ ] Layer 5: E2E integrity check detects missing CAS files at workflow end
- [ ] `with: { img: generate.media[0].path }` + `{{with.img}}` resolves correctly
- [ ] `with: { h: generate.media[0].hash }` + `{{with.h}}` resolves correctly
- [ ] `with: { ext: generate.media[0].extension }` + `{{with.ext}}` resolves correctly
- [ ] Empty media (text-only tasks) returns empty array, index returns None
- [ ] `nika media list` shows stored files
- [ ] `nika media stats` shows count + total size
- [ ] `nika media clean --all` removes all files
- [ ] `nika media clean --older-than 7d` removes old files
- [ ] `nika media clean --all --dry-run` shows count without deleting
- [ ] `MediaCleanup` event emitted after clean operations
- [ ] `MediaCleanup` event has `task_id() -> None` (store-level, not task-level)
- [ ] Existing text-only artifact workflows unchanged (regression)
- [ ] Final event variant count = 37 (32 original + 5 media events across 3 PRs)
- [ ] Merge to main, delete `feat/media-artifacts` branch

---

## Commit Sequence

```
1. feat(ast): add Binary variant to ArtifactFormat
2. feat(io): add write_binary() method to ArtifactWriter
3. feat(binding): verify media reference template access via with: bindings
4. feat(runtime): add E2E media integrity check at workflow completion
5. feat(cli): add nika media list/stats/clean commands
6. feat(event): add MediaCleanup telemetry event
7. test: add ArtifactFormat + write_binary + template + e2e + cli + event tests
8. docs: add image generation workflow example
```

---

## End State

After all 3 PRs are merged, Nika has:

```
MCP Server returns image
  -> rmcp_adapter extracts ALL content types (PR1)
  -> MediaExtracted event emitted (PR1)
  -> MediaProcessor decodes base64 (Layer 2: decode verification) (PR2)
  -> MIME detected via magic bytes + cross-validated vs server (PR2)
  -> blake3 hash computed, CAS store with atomic write (PR2)
  -> Read-back verification (Layer 3: re-hash stored file) (PR2)
  -> MediaProcessed + MediaStored events emitted (PR2)
  -> MediaRef stored in TaskResult.media[] (PR2)
  -> with: bindings resolve media refs via resolve_path() (PR3 verifies)
  -> ArtifactFormat::Binary + write_binary() copies from CAS (PR3)
  -> Layer 4: destination size verification after copy (PR3)
  -> Layer 5: E2E integrity check at workflow end (PR3)
  -> `nika media clean` manages lifecycle + MediaCleanup event (PR3)
```

**5 defense-in-depth layers. 5 new EventKind variants (37 total). Zero breaking changes. Fully backward compatible.**

---

## Corrections from v1 + v2

| Item | v1/v2 | v3 (correct) |
|------|-------|-------------|
| ArtifactWriter binary handling | Assumed String content works | **Fixed**: new `write_binary()` method + `BinaryWriteRequest` for `&[u8]` path |
| Template resolution | `{{task.media[0].path}}` direct access | **Fixed**: via `with:` bindings + `resolve_path()` media interceptor (PR2) |
| OutputFormat vs ArtifactFormat | Single type assumed | **Fixed**: `Binary` added to BOTH `ArtifactFormat` and `OutputFormat` (two separate enums) |
| Artifact copy verification | Not mentioned | **Added**: Layer 4 destination size check after copy |
| E2E integrity check | Not mentioned | **Added**: Layer 5 verify all MediaRefs at workflow end |
| MediaCleanup event | Missing from PR3 | **Added**: with serde, task_id() -> None, and tests |
| parse_duration error handling | Used `/* error */` placeholder | **Fixed**: proper `NikaError::InvalidArgument` |
| Dry run for clean | Only showed count | **Fixed**: also shows total size that would be freed |
| Event variant count | Not tracked | **Tracked**: 32 -> 33 (PR1) -> 36 (PR2) -> 37 (PR3) |
| Error codes in E2E | NIKA-252 | **Shifted** to NIKA-253 (250 taken by ContextLoadError) |
| YAML example | `{{generate.media[0].path}}` | **Fixed**: `with:` bindings + `{{with.img_path}}` |
| CLI file location | `src/commands/media.rs` or `src/main.rs` | **Fixed**: `src/cli/media.rs` with `handle_media_command()` (matches codebase convention) |
| CLI handler return type | `Result<()>` | **Fixed**: `Result<(), NikaError>` (matches all other CLI handlers) |
| CLI dispatch | Not shown | **Added**: `Some(Commands::Media { action }) => cli::media::handle_media_command(action).await` |
