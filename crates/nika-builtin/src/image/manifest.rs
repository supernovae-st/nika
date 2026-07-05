// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The asset manifest — one JSON document per `image_generate` batch that
//! answers « where does this asset come from? » forever: resolved request
//! (never secrets — keys live a layer away by construction), provider +
//! model, prompt, per-image dimensions/hashes/paths, usage, warnings and
//! the caller's `metadata:` provenance fields.
//!
//! Run/workflow/task identities are honestly ABSENT: no run context
//! reaches builtins today (the dispatcher holds no `RunId` seam) — wiring
//! one is on the media roadmap, and lying with empty strings is worse
//! than omitting the fields.

use std::path::Path;

use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};

use crate::BuiltinFailure;
use crate::permits::{FsAccess, FsBoundary};

use super::args::ImageArgs;
use super::save::{SavedImage, sanitize_component, sha256_hex, stem_of};
use super::types::{C_SAVE, Usage};

/// The manifest document version — bump on breaking shape changes.
const MANIFEST_VERSION: u32 = 1;

/// Build the manifest JSON for a completed batch.
#[allow(clippy::too_many_arguments)] // a pure projection of the pipeline's products
pub(crate) fn build(
    args: &ImageArgs,
    saved: &[SavedImage],
    usage: Usage,
    warnings: &[String],
    created_at: &str,
    revised_prompt: Option<&str>,
    provider_text: Option<&str>,
    endpoint_host: Option<&str>,
) -> serde_json::Value {
    let request = request_echo(args);
    let input_hash = sha256_hex(request.to_string().as_bytes());
    serde_json::json!({
        "manifest_version": MANIFEST_VERSION,
        "nika_version": env!("CARGO_PKG_VERSION"),
        "created_at": created_at,
        "provider": args.provider.id(),
        "model": args.model,
        "mode": "generate",
        "endpoint_host": endpoint_host,
        "prompt": args.prompt,
        "revised_prompt": revised_prompt,
        "provider_text": provider_text,
        "request": request,
        "input_hash": input_hash,
        "images": saved.iter().map(image_entry).collect::<Vec<_>>(),
        "usage": usage.to_json(),
        "warnings": warnings,
        "metadata": args.metadata,
    })
}

/// The resolved-request echo — deterministic key order (a fixed literal),
/// safe by construction: the builtin never SEES a credential, so no
/// credential can land here. `provider_options` is echoed verbatim — it
/// is CALLER-owned data (a caller who writes a secret into e.g.
/// `provider_options.user` chose to put it in their own manifest; the
/// engine's keys live a composition layer away and cannot appear).
fn request_echo(args: &ImageArgs) -> serde_json::Value {
    serde_json::json!({
        "provider": args.provider.id(),
        "model": args.model,
        "prompt": args.prompt,
        "n": args.n,
        "size": match args.size {
            super::types::SizeSpec::Auto => "auto".to_owned(),
            super::types::SizeSpec::Exact { width, height } => format!("{width}x{height}"),
            super::types::SizeSpec::Aspect(ratio) => ratio.name().to_owned(),
        },
        "quality": args.quality.name(),
        "format": args.format.name(),
        "compression": args.compression,
        "background": args.background.name(),
        "seed": args.seed,
        "provider_options": args.provider_options,
        "timeout_ms": u64::try_from(args.timeout.as_millis()).unwrap_or(u64::MAX),
    })
}

fn image_entry(image: &SavedImage) -> serde_json::Value {
    serde_json::json!({
        "index": image.index,
        "path": image.path,
        "filename": image.filename,
        "mime_type": image.mime_type,
        "format": image.format,
        "width": image.width,
        "height": image.height,
        "size_bytes": image.size_bytes,
        "sha256": image.sha256,
        "variant_id": image.variant_id,
        "seed": image.seed,
        "revised_prompt": image.revised_prompt,
        "warnings": image.warnings,
    })
}

/// Write the manifest next to the assets (boundary-gated · atomic via the
/// kernel fs seam). Named after the batch content hash so re-runs with
/// identical output land on the same manifest.
pub(crate) async fn write<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &ImageArgs,
    saved: &[SavedImage],
    manifest: &serde_json::Value,
) -> Result<String, BuiltinFailure> {
    let batch_hash = {
        let joined: String = saved.iter().map(|s| s.sha256.as_str()).collect();
        sha256_hex(joined.as_bytes())
    };
    let filename = format!(
        "{}-{}-{}-{}.manifest.json",
        stem_of(args),
        args.provider.id(),
        sanitize_component(&args.model),
        &batch_hash[..8]
    );
    let path = Path::new(&args.output_dir).join(filename);
    let path_str = path.to_string_lossy().into_owned();
    boundary.enforce(fs, &path_str, FsAccess::Write).await?;
    let pretty = serde_json::to_vec_pretty(manifest)
        .map_err(|e| BuiltinFailure::new(C_SAVE, format!("manifest serialization failed: {e}")))?;
    fs.write(&path, &pretty).await.map_err(|e| {
        BuiltinFailure::new(
            C_SAVE,
            format!("manifest write failed for `{path_str}`: {e}"),
        )
    })?;
    Ok(path_str)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use nika_fs::TokioFs;

    use super::super::args;
    use super::super::types::SizeSpec;
    use super::*;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> std::path::PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nika-image-manifest-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("scratch root");
        root
    }

    fn parsed(dir: &std::path::Path) -> ImageArgs {
        let serde_json::Value::Object(map) = serde_json::json!({
            "provider": "mock", "prompt": "manifest test",
            "output_dir": dir.to_string_lossy(),
            "metadata": { "campaign": "spring", "page_slug": "qr-menu" },
            "seed": 7,
        }) else {
            unreachable!()
        };
        args::parse(&map).expect("valid")
    }

    fn saved_fixture() -> SavedImage {
        SavedImage {
            index: 0,
            path: "/tmp/x/qr-menu-mock-mock-image-1-0-abcd1234.png".into(),
            filename: "qr-menu-mock-mock-image-1-0-abcd1234.png".into(),
            mime_type: "image/png",
            format: "png",
            width: 256,
            height: 256,
            size_bytes: 1234,
            sha256: "ab".repeat(32),
            variant_id: "qr-menu-mock-mock-image-1-0-abcd1234".into(),
            seed: Some(7),
            revised_prompt: None,
            warnings: vec![],
        }
    }

    #[test]
    fn manifest_carries_full_provenance_and_never_a_secret_shape() {
        let root = scratch();
        let args = parsed(&root);
        let manifest = build(
            &args,
            &[saved_fixture()],
            Usage {
                input_tokens: Some(10),
                output_tokens: Some(1120),
                total_tokens: Some(1130),
                thoughts_tokens: None,
            },
            &["seed_best_effort: x".to_owned()],
            "2026-07-05T12:00:00Z",
            None,
            None,
            Some("api.openai.com"),
        );
        assert_eq!(manifest["manifest_version"], 1);
        assert_eq!(manifest["nika_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["provider"], "mock");
        assert_eq!(manifest["prompt"], "manifest test");
        assert_eq!(manifest["metadata"]["campaign"], "spring");
        assert_eq!(manifest["images"][0]["sha256"], "ab".repeat(32));
        assert_eq!(manifest["images"][0]["width"], 256);
        assert_eq!(manifest["usage"]["total_tokens"], 1130);
        assert_eq!(manifest["request"]["n"], 1);
        assert_eq!(manifest["request"]["timeout_ms"], 180_000);
        // The serialized document carries no credential-shaped content —
        // there is no key input anywhere in the build path.
        let text = manifest.to_string();
        for needle in ["api_key", "authorization", "bearer", "x-goog-api-key"] {
            assert!(
                !text.to_lowercase().contains(needle),
                "manifest must not carry `{needle}`"
            );
        }
        // Run identity is honestly absent, not empty-stringed.
        assert!(manifest.get("run_id").is_none());
        assert!(manifest.get("workflow").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn input_hash_is_deterministic_and_request_sensitive() {
        let root = scratch();
        let args = parsed(&root);
        let a = build(&args, &[], Usage::default(), &[], "t", None, None, None);
        let b = build(&args, &[], Usage::default(), &[], "t", None, None, None);
        assert_eq!(a["input_hash"], b["input_hash"], "same request → same hash");
        let mut other = parsed(&root);
        other.prompt = "different".into();
        let c = build(&other, &[], Usage::default(), &[], "t", None, None, None);
        assert_ne!(a["input_hash"], c["input_hash"], "prompt changes the hash");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn request_echo_renders_every_size_spec() {
        let root = scratch();
        let mut args = parsed(&root);
        args.size = SizeSpec::Auto;
        assert_eq!(request_echo(&args)["size"], "auto");
        args.size = SizeSpec::Exact {
            width: 1536,
            height: 800,
        };
        assert_eq!(request_echo(&args)["size"], "1536x800");
        args.size = SizeSpec::Aspect(super::super::types::AspectRatio::Wide);
        assert_eq!(request_echo(&args)["size"], "16:9");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn manifest_writes_atomically_and_is_boundary_gated() {
        let root = scratch();
        let args = parsed(&root);
        let saved = vec![saved_fixture()];
        let manifest = build(&args, &saved, Usage::default(), &[], "t", None, None, None);
        let path = write(&TokioFs, &FsBoundary::unbounded(), &args, &saved, &manifest)
            .await
            .expect("writes");
        assert!(path.ends_with(".manifest.json"), "{path}");
        assert!(
            path.contains("qr-menu-mock-"),
            "stem from page_slug: {path}"
        );
        let read_back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("on disk")).expect("valid json");
        assert_eq!(read_back["manifest_version"], 1);
        // An out-of-boundary manifest target is refused before I/O.
        let boundary = FsBoundary::declared(vec![], vec!["/nowhere/**".to_owned()]);
        let err = write(&TokioFs, &boundary, &args, &saved, &manifest)
            .await
            .expect_err("refused");
        assert_eq!(err.code, "NIKA-SEC-004");
        let _ = std::fs::remove_dir_all(&root);
    }
}
