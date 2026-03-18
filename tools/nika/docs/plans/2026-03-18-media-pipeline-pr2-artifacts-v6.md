# PR2: Media Artifacts -- Binary Format + CLI + E2E Integrity

> **Version:** v6.0
> **Date:** 2026-03-18
> **Branch:** `feat/media-artifacts`
> **Baseline:** After PR1 merged (Nika v0.30.5+, 5,261 tests, 36 EventKind variants)
> **Depends on:** PR1 (`feat/media-pipeline`) merged
> **Supersedes:** 2026-03-18-media-pipeline-pr2-artifacts.md (v5.0)
> **Scope:** ArtifactFormat::Binary + `write_binary()` method, template access via `with:` bindings, E2E integrity check, `nika media` CLI, MediaCleanup event.
> **Tests:** ~15 new
> **Commits:** 8

**Parent:** [Master Plan](./2026-03-18-media-pipeline-master-plan.md) | **Prev:** [PR1](./2026-03-18-media-pipeline-pr1-core.md)

---

## Gaps Fixed (v5.0 -> v6.0)

Ten gaps were identified by four parallel analysis agents against the actual post-PR1 codebase. Each gap is addressed inline and summarized here.

| # | Gap | Root Cause | Fix in v6 |
|---|-----|-----------|-----------|
| G1 | `CasEntry` has no `extension` field | CAS filenames are hash-only (no extension). `CasEntry { hash, path, size }` -- three fields. v5 CLI used `entry.extension` which does not exist. | Drop extension column from `nika media list`. Show HASH, SIZE, PATH only. Drop by-extension grouping from `nika media stats`. |
| G2 | `NikaError::InvalidArgument` does not exist | v5 `parse_duration()` used `NikaError::InvalidArgument { arg, reason }` which is not a variant of `NikaError`. | Use `humantime::parse_duration()` crate for `--older-than` parsing. Returns `humantime::DurationError`. Map to `NikaError::ArtifactWriteError` (NIKA-281) with descriptive reason. No new error variant needed. |
| G3 | `RunContext.results` is private | v5 `verify_media_integrity()` accessed `run_context.results.iter()` directly. `results` is `Arc<DashMap<...>>` behind a private field. | Add `pub fn iter_results(&self) -> Vec<(Arc<str>, TaskResult)>` method to `RunContext`. Collects into a Vec to avoid leaking DashMap iterator lifetime. |
| G4 | Stats group-by-extension impossible | `CasEntry` has no extension field (see G1). Grouping by file extension is structurally impossible. | Group by MIME prefix (`image/`, `audio/`, `application/`, `unknown`) is also impossible from CAS alone (no MIME stored). Instead, stats shows only: file count, total size, and shard distribution (2-char hex prefix dirs). |
| G5 | Binary max size logic targets wrong file | v5 showed `with_max_size(BINARY_MAX_SIZE)` in `runner.rs` based on `task.artifact...outputs.iter()`. But `ArtifactSpec` has no `.outputs` method, and the writer is constructed in `artifact_processor.rs` line 105, not runner.rs. | Move the `with_max_size(BINARY_MAX_SIZE)` logic into `artifact_processor.rs` where the `ArtifactWriter` is actually constructed (line 105). Check if any output has `format: binary` there. |
| G6 | `ArtifactSpec` has no `.outputs` method | v5 runner code called `task.artifact.as_ref().map_or(false, \|a\| a.outputs.iter().any(...))`. `ArtifactSpec` is an enum (`Enabled(bool)`, `Single(...)`, `Multiple(...)`) with no `.outputs` method. | Use `matches!()` on the enum variants directly in `artifact_processor.rs`. Define a helper `fn has_binary_format(spec: &ArtifactSpec) -> bool`. |
| G7 | Commit 4 binding verification is redundant | v5 had 6 verification tests for `resolve_path()` media bindings. PR1 already has 11 binding tests in `store/run_context.rs`. Most paths already covered. | Reduce Commit 4 to 2 targeted tests: (1) hash field returns blake3-prefixed string, (2) empty media returns empty array. These cover the two edge cases not fully tested in PR1. |
| G8 | `OutputFormat` missing `Serialize` derive | v5 adds `OutputFormat::Binary` and uses it in `WriteResult.format`. But `OutputFormat` in `src/ast/output.rs` line 126 has `#[derive(Debug, Clone, Deserialize, Default, PartialEq)]` -- no `Serialize`. `WriteResult` does not serialize, so this is not a blocker, but `ArtifactEntry.format` is a `String` field filled by `.to_string()`. Still: add `Serialize` for forward-compatibility. | Add `Serialize` to `OutputFormat`'s derive list. Zero-risk: `#[serde(rename_all = "lowercase")]` already present so serialization maps correctly. |
| G9 | `#[allow(dead_code)]` on CAS methods | `exists()`, `read()`, `list()`, `clean_all()`, `clean_older_than()`, `strip_hash_prefix()` all have `#[allow(dead_code)]` annotations "Used in tests + PR2". | Remove `#[allow(dead_code)]` from all five methods + `strip_hash_prefix()` in Commit 6 when the CLI actually uses them. |
| G10 | Hash prefix mismatch | v5 plan said "plain 64-char hex, no blake3: prefix" but actual code stores `"blake3:af1349..."` (see `store.rs` line 126: `format!("{HASH_PREFIX}{raw_hash}")`). `MediaRef.hash` contains the prefix. `CasEntry.hash` contains the prefix. | Acknowledge: hashes ARE prefixed with `blake3:` everywhere. CLI `list` shows the full prefixed hash. `verify_media_integrity()` uses prefixed hashes. The `strip_hash_prefix()` helper handles lookup. No code change needed -- just correct the plan text. |

### Alignment with PR1 codebase (verified)

| Aspect | Actual PR1 code | PR2 alignment |
|--------|----------------|---------------|
| `ArtifactWriter::new()` | `new(impl Into<PathBuf>, impl Into<String>) -> Self` (line 128, `io/writer.rs`) | All code uses `Self` return, no `.unwrap()` |
| `with_max_size()` | `with_max_size(mut self, max_size: u64) -> Self` (line 137) | Binary writes use same `max_size` field |
| `validate_artifact_path()` | Returns `Result<PathBuf, NikaError>` | Use return value as `full_path` |
| `TemplateResolver::resolve()` | Returns `Result<String, NikaError>` | All call sites use `?` |
| CAS filenames | `{root}/{hash[0..2]}/{hash[2..]}` -- NO extension (line 129, `store.rs`) | Confirmed: no extension in CAS paths |
| Hash format | `blake3:{raw_hash}` -- prefixed (line 126, `store.rs`) | Prefixed everywhere. `strip_hash_prefix()` for lookups |
| `CasEntry` | `{ hash, path, size }` -- three fields, NO extension (line 46, `store.rs`) | CLI uses only these three fields |
| `RunContext.results` | Private field: `Arc<DashMap<Arc<str>, TaskResult, FxBuildHasher>>` (line 176, `run_context.rs`) | New `iter_results()` method for integrity check |
| `ArtifactSpec` | Enum: `Enabled(bool)`, `Single(ArtifactOutput)`, `Multiple(Vec<ArtifactOutput>)` (line 86, `artifact.rs`) | Use match, not `.outputs()` |
| `OutputFormat` | `#[derive(Debug, Clone, Deserialize, Default, PartialEq)]` -- no Serialize (line 126, `output.rs`) | Add `Serialize` derive |
| `ArtifactWriter` construction | In `artifact_processor.rs` line 105, not runner.rs | Binary max size logic in artifact_processor.rs |
| `process_task_artifacts()` | 8 params, builds writer internally (line 55, `artifact_processor.rs`) | Binary dispatch inside `write_single_artifact()` |

---

## Execution Protocol

Every commit follows this sequence:

```
1. Write code
2. cargo check          # Must compile
3. cargo clippy -- -D warnings   # Zero warnings
4. cargo test --lib     # All tests pass (safe, no keychain)
5. git add <specific files>
6. git commit -m "type(scope): description"
```

No commit is made until all three checks pass. If clippy or tests fail, fix before committing.

---

## Dependency Graph

```
C1 (ArtifactFormat::Binary)
 |
 v
C2 (write_binary) -----> C3 (artifact_processor wiring)
                                |
C4 (binding verification)      |
                                v
                          C5 (E2E integrity) -- needs iter_results()
                                |
C6 (CLI: nika media) ----------+
 |
 v
C7 (MediaCleanup event)
 |
 v
C8 (example + all tests)
```

C1 -> C2 -> C3 are strictly sequential (each depends on previous).
C4 is independent (can be done anytime after PR1).
C5 depends on C3 (for iter_results on RunContext).
C6 is independent of C3 but ordered after C5 for logical flow.
C7 depends on C6 (event variant for cleanup).
C8 depends on all (final integration tests).

---

## Error Codes

| Code | Variant | Usage |
|------|---------|-------|
| NIKA-280 | `ArtifactPathError` | Path validation (existing) |
| NIKA-281 | `ArtifactWriteError` | Write failures including duration parse errors (existing) |
| NIKA-282 | `ArtifactSizeExceeded` | Size limit violations (existing) |
| NIKA-283 | `MediaIntegrityWarning` | **NEW** -- CAS file missing or size mismatch at workflow end |
| NIKA-284 | `MediaCleanupFailed` | **NEW** -- GC operation failed (permission denied, etc.) |
| NIKA-285 | `MediaStoreLocked` | **NEW** -- GC blocked by active run (lockfile present) |

### New error variants (added in respective commits)

```rust
// NIKA-283: E2E integrity warning (Commit 5)
#[error("[NIKA-283] Media integrity warning: {message}")]
#[diagnostic(code(nika::media_integrity))]
MediaIntegrityWarning {
    message: String,
},

// NIKA-284: Cleanup failure (Commit 6)
#[error("[NIKA-284] Media cleanup failed: {reason}")]
#[diagnostic(code(nika::media_cleanup))]
MediaCleanupFailed {
    reason: String,
},

// NIKA-285: GC locked by active run (Commit 6)
#[error("[NIKA-285] Media store locked: {reason}")]
#[diagnostic(code(nika::media_store_locked))]
MediaStoreLocked {
    reason: String,
},
```

---

## GC Safety Design

### Problem

`nika media clean --all` could delete CAS files that are still being used by a running workflow. A concurrent `nika run` might reference those files via `MediaRef.path` in `TaskResult.media[]`.

### Solution: Lockfile-based active-run detection

```
.nika/media/store/.lock    # Created by Runner at workflow start, removed at end
```

**Runner side** (in `runner.rs`, workflow start):
1. Create `.nika/media/store/.lock` with PID + timestamp
2. On workflow completion (success or failure): remove lockfile
3. Uses `std::fs::File::create_new()` (O_EXCL) for atomic creation

**CLI GC side** (in `cli/media.rs`):
1. Before any `clean` operation, check if `.lock` exists
2. If exists: read PID, check if process is alive (`kill(pid, 0)`)
3. If alive: print warning, return `NIKA-285` error
4. If stale (process dead): remove stale lockfile, proceed with clean
5. `--force` flag bypasses the lock check (escape hatch)

```rust
/// Check if a workflow run is currently active (lockfile exists with live PID).
fn check_gc_safe(store_root: &Path) -> Result<(), NikaError> {
    let lock_path = store_root.join(".lock");
    if !lock_path.exists() {
        return Ok(());
    }
    // Read PID from lockfile
    let content = std::fs::read_to_string(&lock_path).unwrap_or_default();
    if let Ok(pid) = content.trim().parse::<u32>() {
        // Check if PID is still alive (Unix: kill(pid, 0))
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if alive {
                return Err(NikaError::MediaStoreLocked {
                    reason: format!(
                        "Workflow run (PID {}) is active. Use --force to override.",
                        pid
                    ),
                });
            }
        }
        // Stale lockfile: process is dead, remove it
        let _ = std::fs::remove_file(&lock_path);
    }
    Ok(())
}
```

### Scope limitation

The lockfile mechanism is a best-effort safety net. It does not handle:
- Multiple concurrent workflow runs (only one PID in lockfile)
- Workflows running on different machines sharing the same store

These are acceptable limitations for v0.x. A future PR can upgrade to a proper advisory lock or reference counting scheme.

---

## Reflink-or-Copy Optimization Note

`write_binary()` uses `tokio::fs::copy()` for `BinarySource::CasPath`. On filesystems that support reflinks (APFS on macOS, Btrfs/XFS on Linux), this could be optimized to use `copy_file_range` / `clonefile` for O(1) copy-on-write semantics.

**Not implemented in PR2.** The optimization is deferred because:
1. `tokio::fs::copy()` already delegates to `std::fs::copy()` which uses `sendfile`/`copy_file_range` on Linux
2. macOS `clonefile(2)` requires a separate syscall not exposed by std
3. The performance difference only matters for files >10MB, which are rare in current use cases

**Future PR:** Add `fs_reflink::reflink_or_copy()` (or equivalent) behind a feature flag. Track in backlog.

---

## Commit 1: `feat(ast): add Binary variant to ArtifactFormat and OutputFormat`

### Files

- `src/ast/artifact.rs` (lines 132-165)
- `src/ast/output.rs` (lines 126-141)

### ArtifactFormat (artifact.rs)

Add `Binary` variant to the existing enum (currently: `Text`, `Json`, `Yaml`):

```rust
// src/ast/artifact.rs line 132
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

### OutputFormat (output.rs)

**[G8]** Add `Serialize` derive and `Binary` variant:

```rust
// src/ast/output.rs line 126
// BEFORE: #[derive(Debug, Clone, Deserialize, Default, PartialEq)]
// AFTER:
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
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

Adding `Serialize` is safe: `#[serde(rename_all = "lowercase")]` already present for deserialization, so the same rename rules apply to serialization.

### Tests (in Commit 8)

3 tests: serde roundtrip, extension+display, existing variants unchanged.

---

## Commit 2: `feat(io): add write_binary() to ArtifactWriter`

### File: `src/io/writer.rs`

### New enum: BinarySource

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

### New struct: BinaryWriteRequest

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

### New constant: BINARY_MAX_SIZE

```rust
/// Maximum size for binary artifacts (100 MB).
/// Binary media files (images, audio) are typically larger than text.
/// The artifact processor passes this to `with_max_size()` when writing binary artifacts.
pub const BINARY_MAX_SIZE: u64 = 100 * 1024 * 1024;
```

### New method: write_binary()

`ArtifactWriter::new()` returns `Self` (not `Result`). `write_binary()` uses `self.max_size` -- same field as `write()`.

```rust
impl ArtifactWriter {
    /// Write a binary artifact from a CAS file or raw bytes.
    ///
    /// Unlike `write()` which takes String content and validates JSON,
    /// this writes raw bytes directly. For `BinarySource::CasPath`, uses
    /// async `fs::copy` to avoid loading the full file into memory.
    /// For `BinarySource::Bytes`, uses `write_atomic` for crash safety.
    ///
    /// Size limit uses `self.max_size` (same as `write()`). The artifact processor
    /// calls `with_max_size(BINARY_MAX_SIZE)` when constructing the writer for
    /// binary artifact tasks.
    pub async fn write_binary(&self, request: BinaryWriteRequest) -> Result<WriteResult, NikaError> {
        // 1. Resolve output path template
        let resolver = TemplateResolver::new(&request.task_id, &self.workflow_name)
            .with_vars(request.vars.clone())?;
        let resolved_path = resolver.resolve(&request.output_path)?;

        // 2. Security: validate artifact path (no traversal)
        let full_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;

        // 3. Create parent dirs
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                NikaError::ArtifactWriteError {
                    path: parent.display().to_string(),
                    reason: format!("Failed to create parent directories: {}", e),
                }
            })?;
        }

        // 4. TOCTOU mitigation: re-validate after directory creation
        let final_path = validate_artifact_path(&self.artifact_dir, Path::new(&resolved_path))?;

        // 5. Write based on source type
        let data_size = match &request.source {
            BinarySource::CasPath(cas_path) => {
                let meta = tokio::fs::metadata(cas_path).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: cas_path.display().to_string(),
                        reason: format!("CAS file not found: {}", e),
                    }
                })?;
                let size = meta.len();

                if size > self.max_size {
                    return Err(NikaError::ArtifactSizeExceeded {
                        path: final_path.display().to_string(),
                        size,
                        max_size: self.max_size,
                    });
                }

                // Async copy -- no full load into memory
                // NOTE: Future optimization: reflink/clonefile for APFS/Btrfs (see Reflink note above)
                tokio::fs::copy(cas_path, &final_path).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: final_path.display().to_string(),
                        reason: format!("copy from CAS failed: {}", e),
                    }
                })?;

                size
            }
            BinarySource::Bytes(data) => {
                let size = data.len() as u64;

                if size > self.max_size {
                    return Err(NikaError::ArtifactSizeExceeded {
                        path: final_path.display().to_string(),
                        size,
                        max_size: self.max_size,
                    });
                }

                write_atomic(&final_path, data).await.map_err(|e| {
                    NikaError::ArtifactWriteError {
                        path: final_path.display().to_string(),
                        reason: format!("Atomic write failed: {}", e),
                    }
                })?;

                size
            }
        };

        Ok(WriteResult {
            path: final_path,
            size: data_size,
            format: OutputFormat::Binary,
        })
    }
}
```

### No new fields on ArtifactWriter

The `ArtifactWriter` struct is unchanged. The existing `max_size` field and `with_max_size()` builder are sufficient.

### Tests (in Commit 8)

3 tests: small data CasPath, large data near limit Bytes, over size limit.

---

## Commit 3: `feat(runtime): wire binary format into artifact_processor`

### File: `src/runtime/artifact_processor.rs`

### [G5][G6] Binary format dispatch in `write_single_artifact()`

**Where the writer is constructed** (line 105 of `artifact_processor.rs`):

```rust
// CURRENT (line 99-105):
let max_size = workflow_config
    .map(|c| c.max_size)
    .unwrap_or(crate::ast::artifact::DEFAULT_MAX_ARTIFACT_SIZE);
let writer = ArtifactWriter::new(&artifact_dir, task_id).with_max_size(max_size);
```

**Replace with:**

```rust
// [G5] Binary max size logic lives HERE (artifact_processor.rs), not runner.rs
let max_size = if has_binary_format(artifact_spec) {
    crate::io::writer::BINARY_MAX_SIZE
} else {
    workflow_config
        .map(|c| c.max_size)
        .unwrap_or(crate::ast::artifact::DEFAULT_MAX_ARTIFACT_SIZE)
};
let writer = ArtifactWriter::new(&artifact_dir, task_id).with_max_size(max_size);
```

### [G6] Helper function: `has_binary_format()`

```rust
/// Check if any artifact output in the spec uses Binary format.
/// [G6] ArtifactSpec is an enum, not a struct with .outputs() method.
fn has_binary_format(spec: &ArtifactSpec) -> bool {
    match spec {
        ArtifactSpec::Enabled(_) => false,
        ArtifactSpec::Single(output) => {
            matches!(output.format, Some(ArtifactFormat::Binary))
        }
        ArtifactSpec::Multiple(outputs) => {
            outputs.iter().any(|o| matches!(o.format, Some(ArtifactFormat::Binary)))
        }
    }
}
```

### Binary dispatch in `write_single_artifact()` (line ~253)

**Current code** (line ~253-257):

```rust
let output_format = match format {
    ArtifactFormat::Text => OutputFormat::Text,
    ArtifactFormat::Json => OutputFormat::Json,
    ArtifactFormat::Yaml => OutputFormat::Text,
};
```

**Replace with early return for Binary before the text path:**

```rust
// [G5/G6] Binary format: dispatch to write_binary() instead of text path
if matches!(format, ArtifactFormat::Binary) {
    return write_binary_artifact(task_id, output_spec, writer, bindings, datastore).await;
}

let output_format = match format {
    ArtifactFormat::Text => OutputFormat::Text,
    ArtifactFormat::Json => OutputFormat::Json,
    ArtifactFormat::Yaml => OutputFormat::Text,
    ArtifactFormat::Binary => unreachable!("handled above"),
};
```

### New function: `write_binary_artifact()`

```rust
/// Write a binary artifact from CAS store.
///
/// Resolves the CAS path from `source:` field which maps to a with: binding
/// (e.g., `source: img_path` where `with: { img_path: generate.media[0].path }`).
async fn write_binary_artifact(
    task_id: &str,
    output_spec: &ArtifactOutput,
    writer: &ArtifactWriter,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<WriteResult, NikaError> {
    // The source binding must resolve to a CAS file path
    let cas_path = if let Some(ref source_alias) = output_spec.source {
        let value = bindings
            .get(source_alias)
            .or_else(|| datastore.get_output(source_alias).map(|arc| (*arc).clone()))
            .ok_or_else(|| NikaError::ArtifactWriteError {
                path: output_spec.path.clone(),
                reason: format!(
                    "Binary artifact source '{}' not found in bindings or task outputs",
                    source_alias
                ),
            })?;
        match value {
            serde_json::Value::String(s) => PathBuf::from(s),
            other => {
                return Err(NikaError::ArtifactWriteError {
                    path: output_spec.path.clone(),
                    reason: format!(
                        "Binary artifact source must be a string path, got: {}",
                        other
                    ),
                });
            }
        }
    } else {
        return Err(NikaError::ArtifactWriteError {
            path: output_spec.path.clone(),
            reason: "Binary artifact requires a 'source:' field pointing to a CAS path binding"
                .into(),
        });
    };

    // Pre-resolve {{with.*}} in the artifact path
    let resolved_path =
        resolve_artifact_path_bindings(&output_spec.path, "", bindings, datastore);

    let request = BinaryWriteRequest {
        task_id: task_id.to_string(),
        output_path: resolved_path,
        source: BinarySource::CasPath(cas_path),
        mime_type: String::new(), // MIME type not needed for file copy
        vars: HashMap::new(),
    };

    writer.write_binary(request).await
}
```

**Required imports at top of `artifact_processor.rs`:**

```rust
use crate::io::writer::{BinarySource, BinaryWriteRequest};
```

### Tests (in Commit 8)

Covered by `write_binary()` tests and E2E integration.

---

## Commit 4: `test(binding): verify media template edge cases via with: bindings`

### Scope

**[G7]** PR1 already has 11 binding tests in `store/run_context.rs`. This commit adds only 2 targeted edge-case tests that are NOT covered by PR1.

### File: `src/store/run_context.rs` (tests section)

### Test 1: Hash field returns blake3-prefixed string

**[G10]** Verifies that `resolve_path("generate.media[0].hash")` returns the full prefixed hash (`"blake3:..."`) -- not a stripped 64-char hex.

```rust
#[test]
fn media_binding_hash_returns_prefixed_blake3() {
    // [G10] Hashes are stored with blake3: prefix
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 1024,
            path: PathBuf::from("/tmp/store/af/1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"),
            extension: "png".to_string(),
            created_by: "generate".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("generate".into(), result);

    let resolved = context.resolve_path("generate.media[0].hash");
    let hash_str = resolved.unwrap();
    // Must include blake3: prefix (total length: 7 + 64 = 71)
    assert!(hash_str.as_str().unwrap().starts_with("blake3:"),
        "hash should have blake3: prefix, got: {}", hash_str);
    assert_eq!(hash_str.as_str().unwrap().len(), 71);
}
```

### Test 2: Empty media returns empty array (regression guard)

```rust
#[test]
fn media_binding_empty_media_returns_empty_array() {
    // Text-only task: media[] is empty, indexed access returns None
    let result = TaskResult::success(json!({"text": "ok"}), Duration::from_secs(1));
    let context = RunContext::new();
    context.insert("text_task".into(), result);

    // Array itself should be empty
    let resolved = context.resolve_path("text_task.media");
    assert_eq!(resolved.unwrap().as_array().unwrap().len(), 0);

    // Indexed access on empty array returns None (not panic)
    assert!(context.resolve_path("text_task.media[0].path").is_none());
    assert!(context.resolve_path("text_task.media[0].hash").is_none());
}
```

---

## Commit 5: `feat(runtime): E2E integrity check at workflow end (Layer 5)`

### Files

- `src/store/run_context.rs` -- add `iter_results()` method
- `src/runtime/runner.rs` or `src/runtime/integrity.rs` (NEW) -- integrity check function
- `src/error.rs` -- add NIKA-283 variant

### [G3] New method on RunContext: `iter_results()`

```rust
// src/store/run_context.rs -- add to impl RunContext
/// Iterate over all task results.
///
/// Returns a snapshot Vec to avoid leaking DashMap iterator lifetime.
/// Used by E2E integrity check at workflow end.
pub fn iter_results(&self) -> Vec<(Arc<str>, TaskResult)> {
    self.results
        .iter()
        .map(|entry| (Arc::clone(entry.key()), entry.value().clone()))
        .collect()
}
```

### NIKA-283 error variant

Add to `src/error.rs` in the artifact errors section (after NIKA-282):

```rust
/// Media integrity warning at workflow end
#[error("[NIKA-283] Media integrity warning: {message}")]
#[diagnostic(code(nika::media_integrity))]
MediaIntegrityWarning {
    message: String,
},
```

And in the `code()` match:

```rust
Self::MediaIntegrityWarning { .. } => "NIKA-283",
```

### Integrity check function

```rust
// src/runtime/integrity.rs (NEW file)
//! Layer 5: E2E media integrity verification at workflow end.
//!
//! After all tasks complete, verifies that all MediaRef entries still
//! point to existing CAS files with correct sizes. Warnings only --
//! does not fail the workflow (media may have been cleaned externally).

use crate::store::RunContext;

/// Verify all MediaRefs in completed tasks still point to existing CAS files.
/// Returns warning strings for any mismatches (empty = all good).
///
/// [G3] Uses `run_context.iter_results()` instead of accessing private `results` field.
pub async fn verify_media_integrity(run_context: &RunContext) -> Vec<String> {
    let mut warnings = Vec::new();

    for (task_id, result) in run_context.iter_results() {
        for (i, media_ref) in result.media.iter().enumerate() {
            match tokio::fs::metadata(&media_ref.path).await {
                Ok(meta) => {
                    if meta.len() != media_ref.size_bytes {
                        warnings.push(format!(
                            "NIKA-283: Task '{}' media[{}] size mismatch: expected {}, got {}",
                            task_id, i, media_ref.size_bytes, meta.len()
                        ));
                    }
                }
                Err(_) => {
                    warnings.push(format!(
                        "NIKA-283: Task '{}' media[{}] missing: {}",
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

### Wire into runner.rs

Add to `src/runtime/mod.rs`:

```rust
pub mod integrity;
```

Call site in workflow completion handler (runner.rs, after all tasks complete, before `WorkflowCompleted` event):

```rust
// Layer 5: E2E media integrity check (warnings only)
let integrity_warnings = integrity::verify_media_integrity(&run_context).await;
if !integrity_warnings.is_empty() {
    tracing::warn!(
        warnings = integrity_warnings.len(),
        "Media integrity issues found at workflow end"
    );
}
```

### Tests (in Commit 8)

2 tests: all present (pass), missing CAS file (warning with NIKA-283).

---

## Commit 6: `feat(cli): add nika media subcommand (list/stats/clean)`

### Files

- `src/cli/media.rs` (NEW)
- `src/main.rs` (add `Commands::Media` variant)
- `src/media/store.rs` (remove `#[allow(dead_code)]` from CLI-used methods)
- `src/error.rs` (add NIKA-284, NIKA-285 variants)
- `Cargo.toml` (add `humantime` dependency)

### [G9] Remove dead_code annotations

In `src/media/store.rs`, remove `#[allow(dead_code)]` from:
- `exists()` (line 198)
- `read()` (line 209)
- `list()` (line 233)
- `clean_all()` (line 263)
- `clean_older_than()` (line 283)
- `strip_hash_prefix()` (line 315)
- `CleanResult` struct (line 59)

### [G2] Duration parsing with humantime

Add to `Cargo.toml`:

```toml
humantime = "2.1"
```

This replaces the hand-rolled `parse_duration()` from v5 that used non-existent `NikaError::InvalidArgument`. `humantime` supports rich formats: `"7d"`, `"24h"`, `"30m"`, `"2h30m"`, `"7days"`, etc.

### NIKA-284, NIKA-285 error variants

Add to `src/error.rs`:

```rust
/// Media cleanup operation failed
#[error("[NIKA-284] Media cleanup failed: {reason}")]
#[diagnostic(code(nika::media_cleanup))]
MediaCleanupFailed {
    reason: String,
},

/// Media store is locked by an active workflow run
#[error("[NIKA-285] Media store locked: {reason}")]
#[diagnostic(code(nika::media_store_locked))]
MediaStoreLocked {
    reason: String,
},
```

### Subcommand structure

```rust
// src/main.rs -- Commands enum (add variant)
/// Manage media files in the CAS store
Media {
    #[command(subcommand)]
    action: cli::media::MediaAction,
},
```

Dispatch:

```rust
Some(Commands::Media { action }) => {
    cli::media::handle_media_command(action).await
},
```

### Implementation: `src/cli/media.rs`

```rust
//! CLI subcommand: `nika media {list,stats,clean}`
//!
//! Manages the Content-Addressable Storage (CAS) for media files.
//! [G1] CasEntry has only { hash, path, size } -- no extension field.
//! [G4] Stats cannot group by extension or MIME (not stored in CAS).
//! [G10] Hashes are blake3-prefixed: "blake3:af1349..."

use std::path::{Path, PathBuf};

use clap::Subcommand;

use crate::error::NikaError;
use crate::media::store::CasStore;

#[derive(Subcommand)]
pub enum MediaAction {
    /// List stored media files
    List,

    /// Show store statistics (count, total size)
    Stats,

    /// Clean media files from the store
    Clean {
        /// Remove files older than this duration (e.g., "7d", "24h", "2h30m")
        #[arg(long)]
        older_than: Option<String>,

        /// Remove all stored media files
        #[arg(long)]
        all: bool,

        /// Dry run -- show what would be removed without deleting
        #[arg(long, short = 'n')]
        dry_run: bool,

        /// Force clean even if a workflow run is active
        #[arg(long)]
        force: bool,
    },
}

/// Handle nika media subcommand.
/// No event_log parameter -- CLI commands are not workflow runs.
pub async fn handle_media_command(action: MediaAction) -> Result<(), NikaError> {
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store = CasStore::workspace_default(&workspace_root);
    let store_root = workspace_root.join(".nika").join("media").join("store");

    match action {
        MediaAction::List => {
            let entries = store.list();
            if entries.is_empty() {
                println!("No media files stored.");
                return Ok(());
            }
            // [G1] CasEntry has only hash, path, size -- no extension column
            // [G10] Hash is blake3-prefixed
            println!("{:<72}  {:>10}  {}", "HASH", "SIZE", "PATH");
            for entry in &entries {
                println!(
                    "{:<72}  {:>10}  {}",
                    entry.hash,
                    format_bytes(entry.size),
                    entry.path.display()
                );
            }
            println!(
                "\n{} files, {} total",
                entries.len(),
                format_bytes(entries.iter().map(|e| e.size).sum::<u64>())
            );
        }
        MediaAction::Stats => {
            let entries = store.list();
            let total_size: u64 = entries.iter().map(|e| e.size).sum();

            // [G4] Cannot group by extension or MIME -- CasEntry only has hash, path, size.
            // Show shard distribution instead (2-char hex prefix directories).
            let mut by_shard: std::collections::HashMap<String, (u32, u64)> =
                std::collections::HashMap::new();
            for entry in &entries {
                // Extract shard from hash: "blake3:af1349..." -> "af"
                let shard = entry.hash
                    .strip_prefix("blake3:")
                    .and_then(|h| h.get(..2))
                    .unwrap_or("??")
                    .to_string();
                let bucket = by_shard.entry(shard).or_insert((0, 0));
                bucket.0 += 1;
                bucket.1 += entry.size;
            }

            println!("Media store: .nika/media/store/");
            println!("Files:       {}", entries.len());
            println!("Total size:  {}", format_bytes(total_size));
            if !by_shard.is_empty() {
                println!("Shards:      {}", by_shard.len());
            }
        }
        MediaAction::Clean { older_than, all, dry_run, force } => {
            // GC safety: check for active workflow run (lockfile)
            if !force {
                check_gc_safe(&store_root)?;
            }

            if all {
                if dry_run {
                    let entries = store.list();
                    let total: u64 = entries.iter().map(|e| e.size).sum();
                    println!(
                        "Would remove {} files ({})",
                        entries.len(),
                        format_bytes(total)
                    );
                } else {
                    let result = store.clean_all();
                    println!(
                        "Removed {} files, freed {}",
                        result.removed,
                        format_bytes(result.bytes_freed)
                    );
                }
            } else if let Some(duration_str) = older_than {
                // [G2] Use humantime crate instead of hand-rolled parse_duration()
                let duration = humantime::parse_duration(&duration_str)
                    .map_err(|e| NikaError::ArtifactWriteError {
                        path: "".into(),
                        reason: format!("Invalid duration '{}': {}", duration_str, e),
                    })?;
                if dry_run {
                    println!("Would remove files older than {}", duration_str);
                } else {
                    let result = store.clean_older_than(duration);
                    println!(
                        "Removed {} files, freed {}",
                        result.removed,
                        format_bytes(result.bytes_freed)
                    );
                }
            } else {
                println!("Specify --all or --older-than <duration>");
            }
        }
    }
    Ok(())
}

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

/// Check if a workflow run is currently active (lockfile with live PID).
/// Returns Ok(()) if safe to proceed, or NIKA-285 if locked.
fn check_gc_safe(store_root: &Path) -> Result<(), NikaError> {
    let lock_path = store_root.join(".lock");
    if !lock_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&lock_path).unwrap_or_default();
    if let Ok(pid) = content.trim().parse::<i32>() {
        #[cfg(unix)]
        {
            // kill(pid, 0) checks if process exists without signaling
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if alive {
                return Err(NikaError::MediaStoreLocked {
                    reason: format!(
                        "Workflow run (PID {}) is active. Use --force to override.",
                        pid
                    ),
                });
            }
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, assume locked if lockfile exists
            return Err(NikaError::MediaStoreLocked {
                reason: format!(
                    "Lockfile exists (PID {}). Use --force to override.",
                    pid
                ),
            });
        }
        // Stale lockfile: process is dead, clean up
        let _ = std::fs::remove_file(&lock_path);
    }
    Ok(())
}
```

### Tests (in Commit 8)

3 tests: list empty, stats with entries, clean --all.

---

## Commit 7: `feat(event): add MediaCleanup event (36 -> 37)`

### File: `src/event/log.rs`

### New variant

Add after `MediaStoreFailed` in the MEDIA EVENTS section:

```rust
// ═══════════════════════════════════════════
// MEDIA EVENTS
// ═══════════════════════════════════════════
// ... existing: MediaExtracted, MediaProcessed, MediaStored, MediaStoreFailed ...

/// Media cleanup was performed (via automatic policy during workflow execution)
MediaCleanup {
    removed_count: u32,
    freed_bytes: u64,
    /// Policy that triggered cleanup: "all" | "older_than_7d" | "auto"
    policy: String,
},
```

### No task_id

This is a store-level operation, not task-scoped. Add to the `None` branch of `task_id()`:

```rust
| Self::MediaCleanup { .. } => None,
```

### When MediaCleanup is emitted

`MediaCleanup` is **NOT** emitted by CLI commands. It exists for future workflow-triggered automatic cleanup policies (e.g., "clean CAS files older than 7d after workflow completes"). The event variant is defined now so the variant count is stable for PR2.

### Update variant count test

Update the `all_36_variants()` helper and `count_all_variants` guard test:

```rust
// Add to all_36_variants() -> all_37_variants():
EventKind::MediaCleanup {
    removed_count: 5,
    freed_bytes: 1024,
    policy: "all".to_string(),
},
```

```rust
assert_eq!(
    variants.len(),
    37,
    "EventKind should have exactly 37 variants"
);
```

### Update module doc comment

Change line 6 of `src/event/log.rs`:
```
// BEFORE: 36 variants across 11 categories
// AFTER: 37 variants across 11 categories
```

And line 6 of `src/event/mod.rs`:
```
// BEFORE: 36 variants across 11 categories
// AFTER: 37 variants across 11 categories
```

### Tests (in Commit 8)

2 tests: serde roundtrip, no task_id.

---

## Commit 8: `docs(examples): add binary workflow example + tests (~15 tests)`

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
      command: "echo Saving {{with.img_hash}}"
    artifact:
      path: "output/logo.{{with.img_ext}}"
      format: binary
      source: img_path
```

**Notes:**
- Uses `with:` bindings (not direct `{{generate.media[0].path}}`) -- consistent with Nika's 3-pass template architecture
- `source: img_path` tells the artifact processor which binding holds the CAS file path
- Shows all four media binding types: path, extension, hash, mime_type

### Test inventory: 15 tests

#### ArtifactFormat / OutputFormat (3 tests)

```rust
#[test]
fn artifact_format_binary_serde_roundtrip() {
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
fn artifact_format_binary_extension_and_display() {
    assert_eq!(ArtifactFormat::Binary.extension(), "bin");
    assert_eq!(ArtifactFormat::Binary.to_string(), "binary");
}

#[test]
fn artifact_format_existing_variants_unchanged() {
    // Regression: existing formats must not change
    assert_eq!(ArtifactFormat::Text.extension(), "txt");
    assert_eq!(ArtifactFormat::Json.extension(), "json");
    assert_eq!(ArtifactFormat::Yaml.extension(), "yaml");
}
```

#### write_binary() (3 tests)

```rust
#[tokio::test]
async fn write_binary_small_data_from_cas() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();
    let canonical_dir = art_dir.canonicalize().unwrap();

    // Write source data to a temp CAS file
    let cas_file = dir.path().join("cas_source.png");
    let data = b"fake png data";
    tokio::fs::write(&cas_file, data).await.unwrap();

    // ArtifactWriter::new() returns Self -- no .unwrap()
    let writer = ArtifactWriter::new(canonical_dir, "test-workflow");
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
async fn write_binary_large_data_near_limit_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();
    let canonical_dir = art_dir.canonicalize().unwrap();

    // Just under limit -- use Bytes source for simplicity
    let data = vec![0xFFu8; 1024 * 1024]; // 1 MB
    let writer = ArtifactWriter::new(canonical_dir, "test");
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
async fn write_binary_over_size_limit_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let art_dir = dir.path().join("artifacts");
    tokio::fs::create_dir_all(&art_dir).await.unwrap();
    let canonical_dir = art_dir.canonicalize().unwrap();

    // with_max_size sets self.max_size for both write() and write_binary()
    let writer = ArtifactWriter::new(canonical_dir, "test")
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

#### Media template resolution via with: bindings (2 tests -- from Commit 4)

Already defined in Commit 4 section above:
1. `media_binding_hash_returns_prefixed_blake3`
2. `media_binding_empty_media_returns_empty_array`

#### E2E integrity check (2 tests)

```rust
#[tokio::test]
async fn e2e_integrity_all_media_present() {
    let dir = tempfile::tempdir().unwrap();
    let cas_path = dir.path().join("test.png");
    tokio::fs::write(&cas_path, b"valid data").await.unwrap();

    let result = TaskResult::success(json!({}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: format!("blake3:{}", blake3::hash(b"valid data").to_hex()),
            mime_type: "image/png".to_string(),
            size_bytes: 10,
            path: cas_path,
            extension: "png".to_string(),
            created_by: "t1".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert!(warnings.is_empty());
}

#[tokio::test]
async fn e2e_integrity_missing_cas_file_warns() {
    let result = TaskResult::success(json!({}), Duration::from_secs(1))
        .with_media(vec![MediaRef {
            hash: "blake3:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 100,
            path: PathBuf::from("/tmp/does-not-exist-nika-test.png"),
            extension: "png".to_string(),
            created_by: "t1".to_string(),
        }]);

    let context = RunContext::new();
    context.insert("t1".into(), result);

    let warnings = verify_media_integrity(&context).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("NIKA-283"));
}
```

#### CLI media commands (3 tests)

```rust
#[tokio::test]
async fn media_list_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path().join("empty_store"));
    let entries = store.list();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn media_stats_with_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // CasStore::store() takes only data (no extension param)
    store.store(b"image one data").await.unwrap();
    store.store(b"image two data").await.unwrap();

    let entries = store.list();
    assert_eq!(entries.len(), 2);

    // [G1] CasEntry has { hash, path, size } -- verify fields exist
    let total_size: u64 = entries.iter().map(|e| e.size).sum();
    assert!(total_size > 0);
    // [G10] All hashes should have blake3: prefix
    assert!(entries.iter().all(|e| e.hash.starts_with("blake3:")));
}

#[tokio::test]
async fn media_clean_all_removes_everything() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());
    store.store(b"data one").await.unwrap();
    store.store(b"data two").await.unwrap();
    store.store(b"data three").await.unwrap();

    let result = store.clean_all();
    assert_eq!(result.removed, 3);
    assert!(store.list().is_empty());
}
```

#### Event: MediaCleanup (2 tests)

```rust
#[test]
fn media_cleanup_event_serde_roundtrip() {
    let event = EventKind::MediaCleanup {
        removed_count: 3,
        freed_bytes: 4096,
        policy: "older_than_7d".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"removed_count\":3"));
    let parsed: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, event);
}

#[test]
fn media_cleanup_event_has_no_task_id() {
    let event = EventKind::MediaCleanup {
        removed_count: 5,
        freed_bytes: 1024 * 1024,
        policy: "all".into(),
    };
    assert_eq!(event.task_id(), None); // Store-level, not task-level
}
```

### Test summary: 15 tests total

| Category | Count | Tests |
|----------|-------|-------|
| ArtifactFormat::Binary serde + display | 3 | roundtrip, extension+display, existing unchanged |
| write_binary() | 3 | small CasPath, large Bytes near limit, over size limit |
| Media template resolution [G7] | 2 | blake3-prefixed hash, empty media array |
| E2E integrity check | 2 | all present, missing CAS file warning |
| CLI media commands | 3 | list empty, stats with entries, clean --all |
| MediaCleanup event | 2 | serde roundtrip, no task_id |

Plus the updated `count_all_variants` guard test (36 -> 37).

---

## Commit Sequence

```
1. feat(ast): add Binary variant to ArtifactFormat and OutputFormat
     Files: src/ast/artifact.rs, src/ast/output.rs
     Tests: 0 (tests in Commit 8)

2. feat(io): add write_binary() to ArtifactWriter
     Files: src/io/writer.rs
     Tests: 0 (tests in Commit 8)

3. feat(runtime): wire binary format into artifact_processor
     Files: src/runtime/artifact_processor.rs
     Tests: 0 (tests in Commit 8)

4. test(binding): verify media template edge cases via with: bindings
     Files: src/store/run_context.rs
     Tests: 2

5. feat(runtime): E2E integrity check at workflow end (Layer 5)
     Files: src/store/run_context.rs, src/runtime/integrity.rs (NEW),
            src/runtime/mod.rs, src/runtime/runner.rs, src/error.rs
     Tests: 0 (tests in Commit 8)

6. feat(cli): add nika media subcommand (list/stats/clean)
     Files: src/cli/media.rs (NEW), src/main.rs, src/media/store.rs,
            src/error.rs, Cargo.toml
     Tests: 0 (tests in Commit 8)

7. feat(event): add MediaCleanup event (36 -> 37)
     Files: src/event/log.rs, src/event/mod.rs
     Tests: 0 (tests in Commit 8)

8. test(media): add binary workflow example + all PR2 tests (~15 tests)
     Files: examples/media-pipeline.nika.yaml,
            src/ast/artifact.rs (tests section),
            src/io/writer.rs (tests section),
            src/store/run_context.rs (tests section),
            src/runtime/integrity.rs (tests section),
            src/cli/media.rs (tests section or separate test file),
            src/event/log.rs (tests section)
     Tests: 15
```

---

## Verification Checklist

### Compilation & Quality
- [ ] `cargo check` -- compiles
- [ ] `cargo clippy -- -D warnings` -- zero warnings
- [ ] `cargo test --lib` -- all tests pass (no keychain popup)

### Gap Fixes
- [ ] **[G1]** CLI `list` shows HASH, SIZE, PATH -- no extension column
- [ ] **[G2]** `--older-than` uses `humantime::parse_duration()` -- no `NikaError::InvalidArgument`
- [ ] **[G3]** `verify_media_integrity()` uses `run_context.iter_results()` -- no private field access
- [ ] **[G4]** `stats` shows count + total size + shard count -- no by-extension breakdown
- [ ] **[G5]** `with_max_size(BINARY_MAX_SIZE)` is in `artifact_processor.rs` line ~105 -- not runner.rs
- [ ] **[G6]** `has_binary_format()` uses `match` on `ArtifactSpec` enum -- no `.outputs()` call
- [ ] **[G7]** Only 2 binding tests (blake3 prefix, empty media) -- not 6 redundant tests
- [ ] **[G8]** `OutputFormat` has `Serialize` derive
- [ ] **[G9]** `#[allow(dead_code)]` removed from `exists()`, `read()`, `list()`, `clean_all()`, `clean_older_than()`, `strip_hash_prefix()`, `CleanResult`
- [ ] **[G10]** All hashes shown with `blake3:` prefix -- plan text corrected

### Functional
- [ ] `ArtifactFormat::Binary` serializes to `"binary"` in JSON and YAML
- [ ] `OutputFormat::Binary` serializes to `"binary"`
- [ ] `write_binary()` writes via async CAS copy (CasPath) or atomic bytes
- [ ] `write_binary()` over size limit returns `ArtifactSizeExceeded`
- [ ] `has_binary_format()` correctly detects Binary in Single and Multiple specs
- [ ] `write_binary_artifact()` requires `source:` field
- [ ] `with: { img: generate.media[0].hash }` resolves blake3-prefixed hash
- [ ] Empty media returns empty array, indexed access returns None
- [ ] Layer 5: E2E integrity warns (NIKA-283) on missing CAS file
- [ ] Layer 5: E2E integrity passes when all media present
- [ ] `nika media list` shows hash, size, path
- [ ] `nika media stats` shows count, total size, shards
- [ ] `nika media clean --all` removes all files
- [ ] `nika media clean --older-than 7d` uses humantime parsing
- [ ] `nika media clean --all --dry-run` shows count + size without deleting
- [ ] `nika media clean` checks lockfile (GC safety)
- [ ] `nika media clean --force` bypasses lockfile check
- [ ] `MediaCleanup` event has `task_id() -> None`
- [ ] `MediaCleanup` serde roundtrip correct
- [ ] Event variant count guard test: 37
- [ ] NIKA-283, NIKA-284, NIKA-285 error variants added
- [ ] `humantime` dependency added to Cargo.toml
- [ ] `iter_results()` method on RunContext
- [ ] Existing text-only artifact workflows unchanged (regression)

---

## End State

After PR1 + PR2 are merged, Nika has the complete media pipeline:

```
MCP Server returns image
  -> rmcp_adapter extracts ALL content types (PR1)
  -> MediaExtracted event emitted (PR1)
  -> MediaProcessor decodes base64 (Layer 2) (PR1)
  -> MIME detected via magic bytes + cross-validated (PR1)
  -> blake3 hash computed, CAS store with atomic write (PR1)
  -> Read-back verification (Layer 3) (PR1)
  -> MediaProcessed + MediaStored events emitted (PR1)
  -> MediaRef stored in TaskResult.media[] (PR1)
  -> with: bindings resolve media refs via resolve_path() (PR1, verified in PR2)
  -> ArtifactFormat::Binary + write_binary() for binary artifacts (PR2)
  -> artifact_processor dispatches Binary format to write_binary() (PR2)
  -> Layer 5: E2E integrity check at workflow end (PR2)
  -> nika media list/stats/clean manages lifecycle (PR2)
  -> GC safety via lockfile + PID check (PR2)
  -> MediaCleanup event defined for future auto-cleanup (PR2)
```

**5 defense-in-depth layers. 5 new EventKind variants (37 total). 3 new error codes (NIKA-283..285). Zero breaking changes. Fully backward compatible.**

---
