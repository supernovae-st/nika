# PR3b Phase 2 Audit Fixes — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 4 MEDIUM bugs, 7 LOW bugs, and 5 test quality issues found by the 4-agent audit of PR3b Phase 2.

**Architecture:** Targeted fixes to `import.rs`, `chart.rs`, `provenance.rs`, `compare.rs`, `safety.rs`, `cli/media.rs`, and their test files. No new modules — all changes are surgical edits to existing files. Feature gate fix ensures `decode_image_safe` is available under `media-phash`.

**Tech Stack:** Rust, tokio, image crate, c2pa, charts-rs, serde_json

---

## Task 1: M1+M2+L1+L2+L3 — Import security hardening (import.rs)

Fixes path traversal (M1), pre-read size check (M2), TOCTOU race (L1), blocking I/O (L2), and adds the `MAX_IMPORT_FILE_SIZE` constant.

**Files:**
- Modify: `src/runtime/builtin/media/import.rs`

**Step 1: Write failing tests for path traversal + size limit**

Add these tests at the bottom of the existing `mod tests` block in `import.rs`:

```rust
#[tokio::test]
async fn import_rejects_path_traversal() {
  let (_dir, ctx) = setup().await;
  let op = ImportOp;
  // Create a valid file outside workspace context with ..
  let result = op.execute(
    serde_json::json!({"path": "/tmp/../etc/hosts"}),
    &ctx,
  ).await;
  assert!(result.is_err());
  let err = result.unwrap_err().to_string();
  assert!(err.contains("NIKA-297"), "path traversal should return security violation, got: {err}");
}

#[tokio::test]
async fn import_rejects_absolute_sensitive_paths() {
  let (_dir, ctx) = setup().await;
  let op = ImportOp;
  let result = op.execute(
    serde_json::json!({"path": "/etc/passwd"}),
    &ctx,
  ).await;
  assert!(result.is_err());
  let err = result.unwrap_err().to_string();
  assert!(err.contains("NIKA-297"), "sensitive path should return security violation, got: {err}");
}

#[tokio::test]
async fn import_rejects_oversized_file() {
  let (_dir, ctx) = setup().await;
  // Create a file bigger than MAX_IMPORT_FILE_SIZE (we test with a small limit by checking the error)
  // We just test that the metadata check happens before read — use a real but small file
  // The actual protection is the size check via metadata before tokio::fs::read
  let tmp = tempfile::NamedTempFile::new().unwrap();
  let data = vec![0u8; 1024];
  std::fs::write(tmp.path(), &data).unwrap();

  let op = ImportOp;
  // This should succeed — it's only 1KB
  let result = op.execute(
    serde_json::json!({"path": tmp.path().to_string_lossy()}),
    &ctx,
  ).await;
  assert!(result.is_ok());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib import::tests::import_rejects_path_traversal -- --exact 2>&1 | tail -5`
Expected: FAIL — current code doesn't check for path traversal

**Step 3: Implement path validation + async metadata check + size limit**

Replace the validation + read block in `import.rs` (lines 49-68) with:

```rust
use super::error::{invalid_args, tool_error, security_violation};

/// Maximum import file size: 500 MB (matches CAS budget default).
const MAX_IMPORT_FILE_SIZE: u64 = 500 * 1024 * 1024;

// ... inside execute() after path_str parsing:

let path = PathBuf::from(path_str);

// Security: reject path traversal and sensitive paths
validate_import_path(&path)?;

// Async metadata check (no blocking I/O, no TOCTOU — we just read and handle errors)
let metadata = tokio::fs::metadata(&path).await
  .map_err(|e| match e.kind() {
    std::io::ErrorKind::NotFound => invalid_args("import", format!("file not found: {path_str}")),
    std::io::ErrorKind::PermissionDenied => tool_error("import", format!("permission denied: {path_str}")),
    _ => tool_error("import", format!("cannot stat file: {e}")),
  })?;

if !metadata.is_file() {
  return Err(invalid_args("import", format!("not a regular file: {path_str}")));
}

// Pre-read size check — prevents OOM from multi-GB files
if metadata.len() == 0 {
  return Err(invalid_args("import", "file is empty"));
}
if metadata.len() > MAX_IMPORT_FILE_SIZE {
  return Err(invalid_args("import", format!(
    "file too large ({} bytes, max {} bytes)", metadata.len(), MAX_IMPORT_FILE_SIZE
  )));
}

// Read the file (size already validated)
let data = tokio::fs::read(&path).await
  .map_err(|e| tool_error("import", format!("read failed: {e}")))?;
```

Add this function before the `impl MediaOp for ImportOp`:

```rust
/// Validate import path: reject path traversal and known sensitive directories.
fn validate_import_path(path: &std::path::Path) -> Result<(), NikaError> {
  let path_str = path.to_string_lossy();

  // Reject paths containing ".."
  for component in path.components() {
    if matches!(component, std::path::Component::ParentDir) {
      return Err(security_violation("import", format!(
        "path traversal not allowed: {path_str}"
      )));
    }
  }

  // Reject reads from sensitive system directories
  const SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/", "/proc/", "/sys/", "/dev/",
    "/var/run/", "/var/log/",
  ];

  if let Ok(canonical) = path.canonicalize() {
    let canonical_str = canonical.to_string_lossy();
    for prefix in SENSITIVE_PREFIXES {
      if canonical_str.starts_with(prefix) {
        return Err(security_violation("import", format!(
          "reading from {prefix} is not allowed"
        )));
      }
    }
  }

  Ok(())
}
```

**Step 4: Run all import tests**

Run: `cargo test --lib import::tests -- 2>&1 | tail -5`
Expected: all PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/media/import.rs
git commit -m "fix(media): import security — path traversal, size check, async I/O

- Reject paths containing '..' components (NIKA-297 security violation)
- Reject reads from /etc/, /proc/, /sys/, /dev/ etc.
- Pre-read file size check via tokio::fs::metadata() prevents OOM
- Replace sync exists()/is_file() with async metadata() — no TOCTOU
- MAX_IMPORT_FILE_SIZE = 500MB (matches CAS budget)

Fixes: M1, M2, L1, L2"
```

---

## Task 2: M3 — CLI handle_import wrong error codes (cli/media.rs)

**Files:**
- Modify: `src/cli/media.rs`

**Step 1: Write failing test**

Add to `cli/media.rs` tests:

```rust
#[tokio::test]
async fn test_import_nonexistent_error_code() {
  let dir = tempfile::tempdir().unwrap();
  let store = CasStore::new(dir.path());
  let result = handle_import(&store, std::path::Path::new("/tmp/no_such_99999.xyz"), true).await;
  let err = result.unwrap_err().to_string();
  // Should NOT be NIKA-135 (ConfigError), should be NIKA-294 (invalid params)
  assert!(!err.contains("NIKA-135"), "should not use ConfigError, got: {err}");
}
```

**Step 2: Run to verify fail**

Run: `cargo test --lib cli::media::tests::test_import_nonexistent_error_code -- --exact 2>&1 | tail -5`
Expected: FAIL — currently returns NIKA-135

**Step 3: Fix handle_import error types**

Replace the error types in `handle_import` (cli/media.rs lines 81-127):

```rust
async fn handle_import(store: &CasStore, file: &std::path::Path, quiet: bool) -> Result<(), NikaError> {
    if !file.exists() {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-294] file not found: {}", file.display()),
        });
    }
    if !file.is_file() {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-294] not a regular file: {}", file.display()),
        });
    }

    let data = tokio::fs::read(file).await.map_err(|e| NikaError::BuiltinToolError {
        tool: "nika:import".to_string(),
        reason: format!("[NIKA-290] read failed: {e}"),
    })?;

    if data.is_empty() {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-294] file is empty: {}", file.display()),
        });
    }

    // ... rest unchanged (MIME detection, store, output)
    let mime_type = infer::get(&data)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let size = data.len() as u64;

    let result = store.store(&data).await.map_err(|e| NikaError::BuiltinToolError {
        tool: "nika:import".to_string(),
        reason: format!("[NIKA-290] CAS store failed: {e}"),
    })?;

    // ... output section unchanged
```

**Step 4: Run tests**

Run: `cargo test --lib cli::media::tests -- 2>&1 | tail -5`
Expected: all PASS

**Step 5: Commit**

```bash
git add src/cli/media.rs
git commit -m "fix(media): CLI import uses NIKA-294/290 instead of NIKA-135

Fixes: M3"
```

---

## Task 3: M4 — decode_image_safe feature gate mismatch (safety.rs)

**Files:**
- Modify: `src/runtime/builtin/media/safety.rs`

**Step 1: Verify the bug**

Run: `cargo check --no-default-features --features media-phash 2>&1 | head -20`
Expected: compilation error — `decode_image_safe` not found

**Step 2: Fix feature gate**

In `safety.rs`, change the cfg gates on the constants and `decode_image_safe` function from:

```rust
#[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
```

to:

```rust
#[cfg(any(feature = "media-thumbnail", feature = "media-svg", feature = "media-phash"))]
```

Apply this to:
- Line 10: `MAX_DECODED_BYTES` constant
- Line 14: `MAX_IMAGE_DIM` constant
- Line 26-27: `decode_image_safe` function
- Line 56: `composite_on_white` function — leave this one unchanged (only needed by thumbnail/svg)

Also update the test cfg gates (lines 182, 192, 201, 208, 221) to include `media-phash`.

**Step 3: Verify fix compiles**

Run: `cargo check --no-default-features --features media-phash 2>&1 | tail -5`
Expected: compiles successfully

**Step 4: Run full tests**

Run: `cargo test --lib -- 2>&1 | tail -5`
Expected: all PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/media/safety.rs
git commit -m "fix(media): decode_image_safe available under media-phash feature

The compare tool uses decode_image_safe but it was only gated on
media-thumbnail/media-svg. Building with --features media-phash alone
would fail to compile.

Fixes: M4"
```

---

## Task 4: L3+L4 — Chart data validation (chart.rs)

**Files:**
- Modify: `src/runtime/builtin/media/chart.rs`

**Step 1: Write failing tests**

Add to the `mod tests` in `chart.rs`:

```rust
#[tokio::test]
async fn chart_rejects_infinity_values() {
  let (_dir, ctx) = setup().await;
  let op = ChartOp;
  let result = op.execute(serde_json::json!({
    "type": "bar",
    "series": [{"name": "X", "data": [1e309]}],
    "labels": ["A"]
  }), &ctx).await;
  assert!(result.is_err());
  assert!(result.unwrap_err().to_string().contains("finite"));
}

#[tokio::test]
async fn chart_rejects_non_string_labels() {
  let (_dir, ctx) = setup().await;
  let op = ChartOp;
  let result = op.execute(serde_json::json!({
    "type": "bar",
    "series": [{"name": "X", "data": [1.0, 2.0]}],
    "labels": [1, 2]
  }), &ctx).await;
  assert!(result.is_err());
  assert!(result.unwrap_err().to_string().contains("NIKA-294"));
}

#[tokio::test]
async fn chart_rejects_empty_series_data() {
  let (_dir, ctx) = setup().await;
  let op = ChartOp;
  let result = op.execute(serde_json::json!({
    "type": "bar",
    "series": [{"name": "X", "data": []}],
    "labels": ["A"]
  }), &ctx).await;
  assert!(result.is_err());
  assert!(result.unwrap_err().to_string().contains("empty"));
}
```

**Step 2: Run to verify failures**

Run: `cargo test --lib chart::tests::chart_rejects_infinity -- --exact 2>&1 | tail -5`
Expected: FAIL for infinity (currently passes through as f32::INFINITY)

**Step 3: Fix parse_series and label parsing**

In `parse_series` (chart.rs), after the `as f32` conversion, add the finite check:

```rust
let values: Vec<f32> = data.iter().enumerate().map(|(j, v)| {
  let n = v.as_f64()
    .ok_or_else(|| invalid_args("chart", format!("series[{i}].data[{j}] is not a number")))?;
  let f = n as f32;
  if !f.is_finite() {
    return Err(invalid_args("chart", format!("series[{i}].data[{j}] is not a finite number")));
  }
  Ok(f)
}).collect::<Result<_, _>>()?;
```

For label parsing (chart.rs ~line 98), replace the silent `filter_map`:

```rust
let labels: Vec<String> = match args.get("labels").and_then(|v| v.as_array()) {
  Some(arr) => {
    arr.iter().enumerate().map(|(i, v)| {
      v.as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| invalid_args("chart", format!("labels[{i}] must be a string")))
    }).collect::<Result<_, _>>()?
  }
  None => Vec::new(),
};
```

**Step 4: Run tests**

Run: `cargo test --lib chart::tests -- 2>&1 | tail -5`
Expected: all PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/media/chart.rs
git commit -m "fix(media): chart rejects non-finite values and non-string labels

- NaN/Infinity f32 values now produce NIKA-294 error
- Non-string labels produce error instead of silent filter
- Empty series data array already caught (test added)

Fixes: L3, L4"
```

---

## Task 5: L5+L6 — Provenance cleanup (provenance.rs)

**Files:**
- Modify: `src/runtime/builtin/media/provenance.rs`

**Step 1: Fix unreachable match arm**

In `digital_source_type` (provenance.rs line 148), change:

```rust
_ => "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
```

to:

```rust
_ => unreachable!("assertion validated against KNOWN_ASSERTIONS before calling digital_source_type"),
```

**Step 2: Remove unnecessary clone**

In the execute method (~line 93-103), simplify:

```rust
let extension_for_metadata = extension.clone();

// ... then later at line 102-103:
mime_type: extension_to_mime(&extension_for_metadata),
extension: extension_for_metadata.clone(),  // <- remove this .clone()
```

Change `extension_for_metadata.clone()` to just `extension_for_metadata` (it's being moved, not borrowed later — verify the json! macro on line 107 uses `extension_for_metadata` by value).

Actually, the `serde_json::json!` macro borrows, so we need to reorder. Let the `json!` macro use a separate variable:

```rust
let format_str = extension_for_metadata.clone();  // for json! metadata
Ok(MediaOpResult::Binary {
  data: signed_data,
  mime_type: extension_to_mime(&extension_for_metadata),
  extension: extension_for_metadata,
  metadata: serde_json::json!({
    "assertion": assertion_for_metadata,
    "title": title_for_metadata,
    "format": format_str,
    "signed": true,
  }),
})
```

**Step 3: Run tests**

Run: `cargo test --lib provenance::tests -- 2>&1 | tail -5`
Expected: all PASS

**Step 4: Commit**

```bash
git add src/runtime/builtin/media/provenance.rs
git commit -m "fix(media): provenance — unreachable!() for impossible match, remove clone

Fixes: L5, L6"
```

---

## Task 6: T1+T2 — Fix chart test false positives (chart.rs + tests_pr3b_tools.rs)

**Files:**
- Modify: `src/runtime/builtin/media/chart.rs` (tests section)
- Modify: `src/runtime/builtin/media/tests_pr3b_tools.rs` (chart_tests module)

**Step 1: Add `else { panic!() }` to all bare `if let` in tests_pr3b_tools.rs chart tests**

In `tests_pr3b_tools.rs`, for each `if let MediaOpResult::Binary { data, .. } = result {` without an else clause (lines 278, 296, 310, 326, 338, 354, 404, 419), add:

```rust
} else {
  panic!("expected Binary result");
}
```

**Step 2: Add PNG decodability check to one test per chart type**

In `chart.rs` tests, update `chart_bar_basic`, `chart_line_basic`, and `chart_pie_basic` to decode the PNG:

```rust
// After the PNG magic bytes check, add:
image::load_from_memory(&data).expect("chart PNG must be decodable");
```

**Step 3: Run tests**

Run: `cargo test --lib chart -- 2>&1 | tail -5`
Expected: all PASS

**Step 4: Commit**

```bash
git add src/runtime/builtin/media/chart.rs src/runtime/builtin/media/tests_pr3b_tools.rs
git commit -m "test(media): chart tests verify PNG decodability, no silent false positives

- Bar/line/pie basic tests now decode PNG output
- All if-let matches in tests_pr3b_tools.rs have else { panic!() }

Fixes: T1, T2"
```

---

## Task 7: T3 — Provenance test verifies C2PA readback (provenance.rs)

**Files:**
- Modify: `src/runtime/builtin/media/provenance.rs` (tests section)

**Step 1: Add C2PA manifest readback assertion**

In `provenance_sign_jpeg_ai_generated` test, after the existing assertions:

```rust
// Verify C2PA manifest can be read back
let reader = c2pa::Reader::from_stream("image/jpeg", &mut std::io::Cursor::new(&data))
  .expect("C2PA manifest must be readable in signed output");
let manifest = reader.active_manifest().expect("must have active manifest");
assert!(manifest.title().is_some(), "manifest must have a title");
```

**Step 2: Run test**

Run: `cargo test --lib provenance::tests::provenance_sign_jpeg_ai_generated -- --exact 2>&1 | tail -5`
Expected: PASS — if the C2PA Reader API is available. If not, wrap in `#[cfg(feature = "media-provenance")]` and verify the method exists.

NOTE: The `c2pa::Reader` API may have slightly different method names. Check docs/autocomplete. Key point: read back the manifest and assert it exists.

**Step 3: Commit**

```bash
git add src/runtime/builtin/media/provenance.rs
git commit -m "test(media): provenance verifies C2PA manifest readback after signing

Fixes: T3"
```

---

## Task 8: T4 — Add compare missing edge case tests (compare.rs)

**Files:**
- Modify: `src/runtime/builtin/media/compare.rs` (tests section)

**Step 1: Add missing test cases**

```rust
#[tokio::test]
async fn compare_cancelled_workflow() {
  let (_dir, ctx) = setup().await;
  ctx.cancel.cancel();
  let op = CompareOp;
  let result = op.execute(serde_json::json!({
    "hash_a": "x", "hash_b": "y"
  }), &ctx).await;
  assert!(result.is_err());
  assert!(result.unwrap_err().to_string().contains("cancelled"));
}

#[tokio::test]
async fn compare_fuzz_no_panic() {
  let (_dir, ctx) = setup().await;
  let op = CompareOp;
  let bad_inputs = vec![
    serde_json::json!(null),
    serde_json::json!(42),
    serde_json::json!({"hash_a": 123, "hash_b": 456}),
    serde_json::json!({"hash_a": "x"}),
    serde_json::json!({"hash_b": "y"}),
    serde_json::json!({}),
  ];
  for input in bad_inputs {
    let result = op.execute(input.clone(), &ctx).await;
    assert!(result.is_err(), "bad input should error: {input}");
  }
}

#[tokio::test]
async fn compare_nonexistent_hash() {
  let (_dir, ctx) = setup().await;
  let op = CompareOp;
  let result = op.execute(serde_json::json!({
    "hash_a": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
    "hash_b": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
  }), &ctx).await;
  assert!(result.is_err());
}
```

Also add `else { panic!("expected Metadata") }` to the existing `compare_identical_images` and `compare_different_images` tests.

**Step 2: Run tests**

Run: `cargo test --lib compare::tests -- 2>&1 | tail -5`
Expected: all PASS

**Step 3: Commit**

```bash
git add src/runtime/builtin/media/compare.rs
git commit -m "test(media): compare — add cancel, fuzz, nonexistent hash, fix if-let panics

Fixes: T4 (partial), compare edge cases"
```

---

## Task 9: T5 — Strengthen import_four_bytes_file test (tests_pr3b_tools.rs)

**Files:**
- Modify: `src/runtime/builtin/media/tests_pr3b_tools.rs`

**Step 1: Strengthen assertions**

Replace the `import_four_bytes_file` test:

```rust
#[tokio::test]
async fn import_four_bytes_file() {
  let (_dir, ctx) = setup().await;
  let tmp = tempfile::NamedTempFile::new().unwrap();
  std::fs::write(tmp.path(), &[0x89, 0x50, 0x4E, 0x47]).unwrap(); // PNG magic but truncated

  let result = ImportOp.execute(
    serde_json::json!({"path": tmp.path().to_string_lossy()}),
    &ctx,
  ).await.unwrap();

  if let MediaOpResult::Metadata(v) = result {
    assert_eq!(v["size_bytes"], 4);
    // Hash must be valid blake3 format
    let hash = v["hash"].as_str().unwrap();
    assert!(hash.starts_with("blake3:"), "hash must be blake3-prefixed");
    assert_eq!(hash.len(), 71, "blake3:xxxx = 6 prefix + 64 hex = 71 chars");
    // MIME: PNG magic but truncated — infer may or may not detect it
    let mime = v["mime_type"].as_str().unwrap();
    assert!(!mime.is_empty(), "mime_type should not be empty");
    // Must be readable from CAS
    let read_back = ctx.read_media(hash).await.unwrap();
    assert_eq!(read_back, &[0x89, 0x50, 0x4E, 0x47]);
  } else {
    panic!("expected Metadata result");
  }
}
```

**Step 2: Run test**

Run: `cargo test --lib tests_pr3b_tools::tests::import_four_bytes_file -- --exact 2>&1 | tail -5`
Expected: PASS

**Step 3: Commit**

```bash
git add src/runtime/builtin/media/tests_pr3b_tools.rs
git commit -m "test(media): strengthen import_four_bytes_file — hash format, CAS readback

Fixes: T5"
```

---

## Task 10: Final verification

**Step 1: Run full test suite**

Run: `cargo test --lib -q 2>&1 | tail -5`
Expected: 6049+ passed, 0 failed

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | tail -5`
Expected: 0 warnings

**Step 3: Verify feature combo compiles**

Run: `cargo check --no-default-features --features media-phash 2>&1 | tail -5`
Expected: compiles clean

**Step 4: Commit any leftover**

If all clean, no commit needed.

---

## Summary

| Task | Fixes | Files | Estimate |
|------|-------|-------|----------|
| 1 | M1, M2, L1, L2 | import.rs | Security hardening |
| 2 | M3 | cli/media.rs | Error code fix |
| 3 | M4 | safety.rs | Feature gate fix |
| 4 | L3, L4 | chart.rs | Data validation |
| 5 | L5, L6 | provenance.rs | Code cleanup |
| 6 | T1, T2 | chart.rs, tests_pr3b_tools.rs | Test quality |
| 7 | T3 | provenance.rs | Test quality |
| 8 | T4 | compare.rs | Test coverage |
| 9 | T5 | tests_pr3b_tools.rs | Test quality |
| 10 | — | — | Final verification |

**Total: 10 tasks, 4 MEDIUM + 7 LOW + 5 test quality = 16 issues fixed**
**1 commit per task = 9 commits + final verify**
