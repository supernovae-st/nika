// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Asset persistence — safe filenames · collision probing · atomic writes
//! (the kernel fs seam's temp+rename) · sha256 provenance · best-effort
//! cleanup on partial failure.
//!
//! Filename grammar: `{stem}-{provider}-{modelslug}-{index}-{sha8}.{ext}`.
//! Every component is sanitized to `[a-z0-9._-]` with no separators, so a
//! filename can never traverse; the only operator-controlled PATH is
//! `output_dir:` itself, which rides the declared `permits.fs` boundary
//! (`NIKA-SEC-004`) exactly like `nika:write`'s `path:`.

use std::path::{Path, PathBuf};

use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use sha2::Digest as _;

use crate::BuiltinFailure;
use crate::permits::{FsAccess, FsBoundary};

use super::args::ImageArgs;
use super::sniff::Sniffed;
use super::types::{C_SAVE, RawImage};

/// One saved asset — everything the output/manifest reports per image.
#[derive(Debug, Clone)]
pub(crate) struct SavedImage {
    pub(crate) index: u32,
    pub(crate) path: String,
    pub(crate) filename: String,
    pub(crate) mime_type: &'static str,
    pub(crate) format: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) variant_id: String,
    pub(crate) seed: Option<i64>,
    pub(crate) revised_prompt: Option<String>,
    pub(crate) warnings: Vec<String>,
}

/// Sanitize a filename component: lowercase `[a-z0-9._-]`, no separators,
/// no leading dot, collapsed dashes, ≤64 chars, never empty.
pub(crate) fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(64));
    let mut last_dash = false;
    for c in raw.chars().flat_map(char::to_lowercase) {
        let mapped = if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
            c
        } else {
            '-'
        };
        if mapped == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(mapped);
        if out.len() >= 64 {
            break;
        }
    }
    let trimmed = out.trim_matches(['-', '.']).to_owned();
    if trimmed.is_empty() {
        "image".to_owned()
    } else {
        trimmed
    }
}

/// The filename stem: `filename_prefix:` wins, else `metadata.page_slug`,
/// else the neutral `image`.
pub(crate) fn stem_of(args: &ImageArgs) -> String {
    let raw = args
        .filename_prefix
        .as_deref()
        .or_else(|| {
            args.metadata
                .get("page_slug")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("image");
    sanitize_component(raw)
}

/// Lowercase hex sha256 of a byte payload.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    crate::data::hex_encode(&sha2::Sha256::digest(bytes))
}

/// Persist one decoded batch. On ANY failure the images already written
/// by THIS call are removed best-effort before the error propagates — no
/// half-batch litter (the manifest is written later, only over a complete
/// batch).
pub(crate) async fn save_all<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &ImageArgs,
    decoded: Vec<(RawImage, Sniffed, Vec<String>)>,
) -> Result<Vec<SavedImage>, BuiltinFailure> {
    let mut written: Vec<String> = Vec::new();
    let result = save_all_inner(fs, boundary, args, decoded, &mut written).await;
    if result.is_err() {
        for path in &written {
            let _ = fs.remove_file(Path::new(path)).await; // best-effort cleanup
        }
    }
    result
}

async fn save_all_inner<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &ImageArgs,
    decoded: Vec<(RawImage, Sniffed, Vec<String>)>,
    written: &mut Vec<String>,
) -> Result<Vec<SavedImage>, BuiltinFailure> {
    let stem = stem_of(args);
    let model_slug = sanitize_component(&args.model);
    let dir = Path::new(&args.output_dir);
    let mut dir_created = false;
    let mut saved = Vec::with_capacity(decoded.len());

    for (index, (image, sniffed, image_warnings)) in decoded.into_iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)] // n ≤ 10 by parse
        let index = index as u32;
        let sha = sha256_hex(&image.bytes);
        let base = format!(
            "{stem}-{}-{model_slug}-{index}-{}",
            args.provider.id(),
            &sha[..8]
        );
        let extension = sniffed.format.extension();

        let path = match resolve_collision_free(fs, boundary, dir, &base, extension, &sha).await? {
            Resolved::Existing(path) => {
                // Identical bytes already on disk (this run's dedupe OR a
                // PRIOR run's asset) — count it saved, write nothing, and
                // never register it for failure cleanup: deleting it on a
                // later image's failure would destroy a prior batch's
                // output and dangle its manifest.
                saved.push(saved_entry(
                    index,
                    &path,
                    sniffed,
                    sha,
                    base,
                    image,
                    image_warnings,
                ));
                continue;
            }
            Resolved::Fresh(path) => path,
        };

        if !dir_created {
            fs.create_dir_all(dir).await.map_err(|e| {
                BuiltinFailure::new(
                    C_SAVE,
                    format!("could not create output_dir `{}`: {e}", args.output_dir),
                )
            })?;
            dir_created = true;
        }
        fs.write(&path, &image.bytes).await.map_err(|e| {
            BuiltinFailure::new(
                C_SAVE,
                format!("write failed for `{}`: {e}", path.display()),
            )
        })?;
        written.push(path.to_string_lossy().into_owned());
        saved.push(saved_entry(
            index,
            &path,
            sniffed,
            sha,
            base,
            image,
            image_warnings,
        ));
    }
    Ok(saved)
}

/// Where a decoded image lands (boundary-gated per candidate).
enum Resolved {
    /// A free name — write the bytes here (and register for cleanup).
    Fresh(PathBuf),
    /// IDENTICAL bytes already live at this path (idempotent re-run, at
    /// the base name OR a probed suffix) — skip the write entirely; the
    /// file belongs to a prior batch and must survive THIS call's
    /// failure cleanup.
    Existing(PathBuf),
}

/// Pick a collision-free target path (boundary-gated per candidate).
async fn resolve_collision_free<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    boundary: &FsBoundary,
    dir: &Path,
    base: &str,
    extension: &str,
    sha: &str,
) -> Result<Resolved, BuiltinFailure> {
    for attempt in 0u32..100 {
        let filename = if attempt == 0 {
            format!("{base}.{extension}")
        } else {
            format!("{base}-{}.{extension}", attempt + 1)
        };
        let candidate = dir.join(filename);
        let candidate_str = candidate.to_string_lossy().into_owned();
        // The capability boundary gates EVERY candidate before any I/O.
        boundary
            .enforce(fs, &candidate_str, FsAccess::Write)
            .await?;
        if !fs.exists(&candidate).await {
            return Ok(Resolved::Fresh(candidate));
        }
        // Occupied: identical content = idempotent re-run; different
        // content (a truncated-sha collision) = probe the next suffix.
        if let Ok(existing) = fs.read(&candidate).await
            && sha256_hex(&existing) == sha
        {
            return Ok(Resolved::Existing(candidate));
        }
    }
    Err(BuiltinFailure::new(
        C_SAVE,
        format!("could not find a collision-free name for `{base}.{extension}` after 100 tries"),
    ))
}

fn saved_entry(
    index: u32,
    path: &Path,
    sniffed: Sniffed,
    sha: String,
    variant_id: String,
    image: RawImage,
    warnings: Vec<String>,
) -> SavedImage {
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    SavedImage {
        index,
        path: path.to_string_lossy().into_owned(),
        filename,
        mime_type: sniffed.format.mime(),
        format: sniffed.format.name(),
        width: sniffed.width,
        height: sniffed.height,
        size_bytes: image.bytes.len() as u64,
        sha256: sha,
        variant_id,
        seed: image.seed,
        revised_prompt: image.revised_prompt,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use nika_fs::TokioFs;
    use proptest::prelude::*;

    use super::super::args;
    use super::super::sniff::sniff;
    use super::super::types::ImageFormat;
    use super::*;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("nika-image-save-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("scratch root");
        root
    }

    fn parsed(dir: &Path, patch: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "mock", "prompt": "save test",
            "output_dir": dir.to_string_lossy(),
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(p) = patch {
            for (k, v) in p {
                map.insert(k, v);
            }
        }
        args::parse(&map).expect("valid")
    }

    /// A tiny REAL png via the mock renderer (so `sniff()` has honest input).
    fn tiny_png(seed: i64) -> (RawImage, Sniffed, Vec<String>) {
        let args = parsed(
            Path::new("/tmp"),
            serde_json::json!({ "size": "24x16", "seed": seed }),
        );
        let batch = super::super::mock::generate(&args, &[]).expect("mock");
        let image = batch.images.into_iter().next().expect("one image");
        let sniffed = sniff(&image.bytes).expect("sniffs");
        (image, sniffed, Vec::new())
    }

    #[test]
    fn sanitize_component_flattens_hostile_input() {
        assert_eq!(sanitize_component("Hello World!"), "hello-world");
        assert_eq!(sanitize_component("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_component("a/b\\c"), "a-b-c");
        assert_eq!(sanitize_component(".hidden"), "hidden");
        assert_eq!(
            sanitize_component("--- ---"),
            "image",
            "all-junk → fallback"
        );
        assert_eq!(sanitize_component(""), "image");
        assert_eq!(
            sanitize_component("Gemini-3.1-Flash-Image"),
            "gemini-3.1-flash-image"
        );
        assert!(sanitize_component(&"x".repeat(300)).len() <= 64);
    }

    proptest! {
        /// The sanitized component can never traverse or hide: no
        /// separators, no leading dot, never empty, bounded length.
        #[test]
        fn sanitize_component_is_traversal_free(raw in "\\PC*") {
            let out = sanitize_component(&raw);
            prop_assert!(!out.contains('/'));
            prop_assert!(!out.contains('\\'));
            prop_assert!(!out.starts_with('.'));
            prop_assert!(!out.is_empty());
            prop_assert!(out.len() <= 64);
            prop_assert!(out.chars().all(|c| c.is_ascii_lowercase()
                || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')));
        }
    }

    #[tokio::test]
    async fn saves_with_the_documented_filename_grammar_and_sha() {
        let root = scratch();
        let args = parsed(&root, serde_json::json!({ "filename_prefix": "OG Hero!" }));
        let (image, sniffed, _) = tiny_png(1);
        let expected_sha = sha256_hex(&image.bytes);
        let saved = save_all(
            &TokioFs,
            &FsBoundary::unbounded(),
            &args,
            vec![(image, sniffed, Vec::new())],
        )
        .await
        .expect("saves");
        assert_eq!(saved.len(), 1);
        let entry = &saved[0];
        assert_eq!(
            entry.filename,
            format!("og-hero-mock-mock-image-1-0-{}.png", &expected_sha[..8])
        );
        assert_eq!(entry.sha256, expected_sha);
        assert_eq!((entry.width, entry.height), (24, 16));
        assert_eq!(entry.mime_type, "image/png");
        let on_disk = std::fs::read(&entry.path).expect("file exists");
        assert_eq!(sha256_hex(&on_disk), expected_sha, "bytes round-trip");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn stem_falls_back_to_metadata_page_slug() {
        let root = scratch();
        let args = parsed(
            &root,
            serde_json::json!({ "metadata": { "page_slug": "QR Menu Café" } }),
        );
        assert_eq!(stem_of(&args), "qr-menu-caf");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn identical_rerun_is_idempotent_and_different_content_probes() {
        let root = scratch();
        let args = parsed(&root, serde_json::json!({}));
        let (image, sniffed, _) = tiny_png(1);
        let bytes = image.bytes.clone();
        let first = save_all(
            &TokioFs,
            &FsBoundary::unbounded(),
            &args,
            vec![(image, sniffed, Vec::new())],
        )
        .await
        .expect("first save");
        // Re-run with identical bytes → same path, no duplicate file.
        let rerun_image = RawImage {
            bytes: bytes.clone(),
            revised_prompt: None,
            seed: None,
        };
        let rerun = save_all(
            &TokioFs,
            &FsBoundary::unbounded(),
            &args,
            vec![(rerun_image, sniffed, Vec::new())],
        )
        .await
        .expect("idempotent rerun");
        assert_eq!(rerun[0].path, first[0].path);
        let files: Vec<_> = std::fs::read_dir(&root).expect("dir").collect();
        assert_eq!(files.len(), 1, "no duplicate");
        // A DIFFERENT payload squatting the exact target name → -2 probe.
        std::fs::write(&first[0].path, b"squatter").expect("squat");
        let probe_image = RawImage {
            bytes,
            revised_prompt: None,
            seed: None,
        };
        let probed = save_all(
            &TokioFs,
            &FsBoundary::unbounded(),
            &args,
            vec![(probe_image, sniffed, Vec::new())],
        )
        .await
        .expect("probed save");
        assert!(
            probed[0].filename.ends_with("-2.png"),
            "{}",
            probed[0].filename
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn boundary_escape_is_refused_and_nothing_is_written() {
        let root = scratch();
        // Boundary admits ONLY <root>/allowed/** — output_dir points outside.
        let boundary = FsBoundary::declared(vec![], vec![format!("{}/allowed/**", root.display())]);
        let outside = root.join("outside");
        let args = parsed(&outside, serde_json::json!({}));
        let (image, sniffed, _) = tiny_png(2);
        let err = save_all(
            &TokioFs,
            &boundary,
            &args,
            vec![(image, sniffed, Vec::new())],
        )
        .await
        .expect_err("refused");
        assert_eq!(err.code, "NIKA-SEC-004");
        assert!(
            !outside.exists(),
            "no dir, no file — the gate is before I/O"
        );
        // …and the same batch INSIDE the boundary lands.
        let inside = root.join("allowed");
        let args = parsed(&inside, serde_json::json!({}));
        let (image, sniffed, _) = tiny_png(2);
        let saved = save_all(
            &TokioFs,
            &boundary,
            &args,
            vec![(image, sniffed, Vec::new())],
        )
        .await
        .expect("inside saves");
        assert!(Path::new(&saved[0].path).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn partial_failure_cleans_up_the_written_prefix() {
        let root = scratch();
        let dir = root.join("out");
        let (first, sniffed1, _) = tiny_png(3);
        let (second, sniffed2, _) = tiny_png(4);
        // Admit exactly the FIRST image's final path — the second write is
        // refused by the boundary, and the first file must be cleaned up.
        let args = parsed(&dir, serde_json::json!({}));
        let stem = stem_of(&args);
        let first_name = format!(
            "{stem}-mock-mock-image-1-0-{}.png",
            &sha256_hex(&first.bytes)[..8]
        );
        let boundary = FsBoundary::declared(
            vec![],
            vec![dir.join(&first_name).to_string_lossy().into_owned()],
        );
        let err = save_all(
            &TokioFs,
            &boundary,
            &args,
            vec![
                (first, sniffed1, Vec::new()),
                (second, sniffed2, Vec::new()),
            ],
        )
        .await
        .expect_err("second write refused");
        assert_eq!(err.code, "NIKA-SEC-004");
        assert!(
            !dir.join(&first_name).exists(),
            "the already-written first image was cleaned up"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn prior_batch_asset_at_a_probed_name_survives_a_later_failure() {
        // The P1 regression (review swarm 2026-07-05): an IDENTICAL file
        // found at a PROBED name is a PRIOR batch's asset — it must never
        // be re-registered for this call's failure cleanup, or a later
        // image's failure destroys the earlier batch's output (and
        // dangles its manifest).
        let root = scratch();
        let dir = root.join("out");
        let args = parsed(&dir, serde_json::json!({}));
        let (image, sniffed, _) = tiny_png(9);
        let bytes = image.bytes.clone();
        // Run 1: squat the BASE name with different bytes so run 2 probes.
        let stem = stem_of(&args);
        let base_name = format!(
            "{stem}-mock-mock-image-1-0-{}.png",
            &sha256_hex(&bytes)[..8]
        );
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join(&base_name), b"squatter").expect("squat");
        let first = save_all(
            &TokioFs,
            &FsBoundary::unbounded(),
            &args,
            vec![(image, sniffed, Vec::new())],
        )
        .await
        .expect("probed save");
        assert!(
            first[0].filename.ends_with("-2.png"),
            "{}",
            first[0].filename
        );
        let probed_path = first[0].path.clone();
        // Run 2: image 1 = the SAME bytes (identical at the probed name ·
        // a prior asset) · image 2's write is refused by the boundary.
        let (second, sniffed2, _) = tiny_png(10);
        let boundary = FsBoundary::declared(
            vec![],
            vec![
                dir.join(&base_name).to_string_lossy().into_owned(),
                probed_path.clone(),
            ],
        );
        let rerun_image = RawImage {
            bytes,
            revised_prompt: None,
            seed: None,
        };
        let err = save_all(
            &TokioFs,
            &boundary,
            &args,
            vec![
                (rerun_image, sniffed, Vec::new()),
                (second, sniffed2, Vec::new()),
            ],
        )
        .await
        .expect_err("second write refused");
        assert_eq!(err.code, "NIKA-SEC-004");
        assert!(
            std::path::Path::new(&probed_path).exists(),
            "the PRIOR batch's asset must survive this call's cleanup"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sha256_matches_a_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn extension_follows_the_actual_sniffed_format() {
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
    }
}
