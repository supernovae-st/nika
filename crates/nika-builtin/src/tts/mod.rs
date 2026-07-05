// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:tts_generate` — the second Media builtin (stdlib §Audio): text
//! in, a REAL audio file on disk out, through the image family's exact
//! discipline — assets not blobs (bytes never ride outputs), magic-byte
//! authority (the extension follows what the payload IS), boundary-gated
//! atomic saves, a provenance manifest, honest warnings, and the
//! sovereign path first (one `OpenAI`-speech-compatible wire covers
//! `LocalAI` · Kokoro-FastAPI · Speaches · openedai-speech).
//!
//! Keys ride [`TtsKeys`], resolved at the composition root (the ONE
//! sanctioned env boundary — `NIKA_OPENAI_API_KEY`→`OPENAI_API_KEY` ·
//! `NIKA_ELEVENLABS_API_KEY`→`ELEVENLABS_API_KEY` · `NIKA_TTS_LOCAL_URL`
//! plus optional `NIKA_TTS_LOCAL_API_KEY`) and injected via
//! [`crate::BuiltinDispatcher::with_tts_plane`]. Calls ride the SAME
//! provider-plane http seam philosophy as `image_generate` and `infer:`.

pub(crate) mod args;
pub(crate) mod elevenlabs;
pub(crate) mod local;
pub(crate) mod mock;
pub(crate) mod openai;
pub(crate) mod sniff;
pub(crate) mod types;

use std::path::Path;

use nika_kernel::io::clock::ClockDyn;
use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use nika_kernel::io::http::HttpPostDyn;
use nika_kernel::secret::Secret;

use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

use self::args::TtsArgs;
use self::types::{C_NO_AUDIO, C_PROVIDER_UNAVAILABLE, C_SAVE, Provider, ProviderAudio};

use super::image::save::{sanitize_component, sha256_hex};

/// The TTS-plane connection config (composition-root resolved — this
/// crate never reads the environment; the `Secret` type is zeroizing and
/// Debug-redacted).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TtsKeys {
    /// The `OpenAI` API key, when present.
    pub openai: Option<Secret>,
    /// The `ElevenLabs` API key, when present.
    pub elevenlabs: Option<Secret>,
    /// The LOCAL speech server base URL (`NIKA_TTS_LOCAL_URL`) — defaults
    /// to `LocalAI`'s `http://localhost:8080` when unset.
    pub local_base_url: Option<String>,
    /// The optional local-server key (`NIKA_TTS_LOCAL_API_KEY`).
    pub local_api_key: Option<Secret>,
}

impl TtsKeys {
    /// No keys — the mock provider still works (offline CI · first run).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the `OpenAI` key.
    #[must_use]
    pub fn with_openai(mut self, key: Secret) -> Self {
        self.openai = Some(key);
        self
    }

    /// Attach the `ElevenLabs` key.
    #[must_use]
    pub fn with_elevenlabs(mut self, key: Secret) -> Self {
        self.elevenlabs = Some(key);
        self
    }

    /// Point the `local` provider at a specific speech server.
    #[must_use]
    pub fn with_local_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.local_base_url = Some(base_url.into());
        self
    }

    /// Attach the optional local-server key.
    #[must_use]
    pub fn with_local_api_key(mut self, key: Secret) -> Self {
        self.local_api_key = Some(key);
        self
    }
}

/// Run `nika:tts_generate` end-to-end (the wrapper owns the error event).
pub(crate) async fn generate<F, H, C, Em>(
    fs: &F,
    http: Option<&H>,
    keys: &TtsKeys,
    clock: &C,
    emitter: &Em,
    boundary: &FsBoundary,
    raw_args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    match run(fs, http, keys, clock, emitter, boundary, raw_args).await {
        Ok(value) => Ok(value),
        Err(failure) => {
            emitter.emit(
                "tts_generation.error",
                serde_json::json!({ "code": failure.code, "message": failure.message }),
            );
            Err(failure)
        }
    }
}

async fn run<F, H, C, Em>(
    fs: &F,
    http: Option<&H>,
    keys: &TtsKeys,
    clock: &C,
    emitter: &Em,
    boundary: &FsBoundary,
    raw_args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    let started = clock.now();
    let mut args = args::parse(raw_args)?;
    let mut warnings = std::mem::take(&mut args.warnings);
    emitter.emit(
        "tts_generation.started",
        serde_json::json!({
            "provider": args.provider.id(), "model": args.model, "voice": args.voice,
            "chars": args.text.chars().count(),
        }),
    );

    let audio = call_provider(http, keys, &args).await?;
    let ProviderAudio {
        bytes,
        cost_usd,
        endpoint_host,
        warnings: adapter_warnings,
    } = audio;
    warnings.extend(adapter_warnings);
    if bytes.is_empty() {
        return Err(BuiltinFailure::new(
            C_NO_AUDIO,
            "the provider returned an empty audio payload",
        ));
    }

    // Magic-byte authority: the payload names its own container.
    let sniffed = sniff::sniff(&bytes)?;
    if let Some(asked) = args.format
        && asked != sniffed.format
        && !warnings.iter().any(|w| w.starts_with("format_mismatch:"))
    {
        warnings.push(format!(
            "format_mismatch: asked {} — the payload is {} (the extension follows the bytes)",
            asked.name(),
            sniffed.format.name()
        ));
    }

    let saved = save_audio(fs, boundary, &args, &bytes, sniffed).await?;
    emitter.emit(
        "tts_generation.saved",
        serde_json::json!({
            "path": saved.path, "sha256_8": &saved.sha256[..8], "size_bytes": saved.size_bytes,
        }),
    );

    let created_at = rfc3339_now(clock);
    let manifest_path = if args.manifest {
        let manifest = manifest_json(
            &args,
            &saved,
            cost_usd,
            &warnings,
            &created_at,
            endpoint_host.as_deref(),
        );
        Some(write_manifest(fs, boundary, &args, &saved, &manifest, emitter).await?)
    } else {
        None
    };

    for warning in &warnings {
        emitter.emit(
            "tts_generation.warning",
            serde_json::json!({ "message": warning }),
        );
    }
    emitter.emit(
        "tts_generation.completed",
        serde_json::json!({
            "size_bytes": saved.size_bytes,
            "duration_ms_audio": saved.duration_ms,
            "cost_usd": cost_usd,
            "duration_ms":
                u64::try_from(clock.elapsed(started).as_millis()).unwrap_or(u64::MAX),
        }),
    );

    Ok(serde_json::json!({
        "provider": args.provider.id(),
        "model": args.model,
        "voice": args.voice,
        "created_at": created_at,
        "endpoint_host": endpoint_host,
        "audio": {
            "path": saved.path,
            "filename": saved.filename,
            "format": saved.format,
            "mime_type": saved.mime_type,
            "size_bytes": saved.size_bytes,
            "sha256": saved.sha256,
            "duration_ms": saved.duration_ms,
        },
        "cost_usd": cost_usd,
        "warnings": warnings,
        "manifest_path": manifest_path,
        "output_dir": args.output_dir,
    }))
}

async fn call_provider<H: HttpPostDyn>(
    http: Option<&H>,
    keys: &TtsKeys,
    args: &TtsArgs,
) -> Result<ProviderAudio, BuiltinFailure> {
    if args.provider == Provider::Mock {
        return Ok(mock::generate(args));
    }
    let http = http.ok_or_else(|| {
        BuiltinFailure::new(
            C_PROVIDER_UNAVAILABLE,
            "the TTS plane is not wired — the composition root did not inject an HTTP client",
        )
    })?;
    match args.provider {
        Provider::Openai => {
            let key = keys
                .openai
                .as_ref()
                .ok_or_else(|| missing_key("openai", "NIKA_OPENAI_API_KEY or OPENAI_API_KEY"))?;
            openai::generate(http, key, args).await
        }
        Provider::Elevenlabs => {
            let key = keys.elevenlabs.as_ref().ok_or_else(|| {
                missing_key(
                    "elevenlabs",
                    "NIKA_ELEVENLABS_API_KEY or ELEVENLABS_API_KEY",
                )
            })?;
            elevenlabs::generate(http, key, args).await
        }
        Provider::Local => {
            let base = keys
                .local_base_url
                .clone()
                .unwrap_or_else(|| local::DEFAULT_BASE_URL.to_owned());
            local::generate(http, &base, keys.local_api_key.as_ref(), args).await
        }
        Provider::Mock => unreachable!("handled above"),
    }
}

fn missing_key(provider: &str, vars: &str) -> BuiltinFailure {
    BuiltinFailure::new(
        C_PROVIDER_UNAVAILABLE,
        format!("no {provider} API key configured — set {vars}"),
    )
}

/// One saved audio asset (the audio analogue of `SavedImage`).
#[derive(Debug)]
struct SavedAudio {
    path: String,
    filename: String,
    format: &'static str,
    mime_type: &'static str,
    size_bytes: u64,
    sha256: String,
    duration_ms: Option<u64>,
}

/// Boundary-gated atomic save — `{stem}-{provider}-{model}-{sha8}.{ext}`
/// (idempotent by content hash: a re-run with identical bytes lands on
/// the same name and never rewrites).
async fn save_audio<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    boundary: &FsBoundary,
    args: &TtsArgs,
    bytes: &[u8],
    sniffed: sniff::Sniffed,
) -> Result<SavedAudio, BuiltinFailure> {
    let stem = args
        .filename_prefix
        .as_deref()
        .map(sanitize_component)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            args.metadata
                .get("page_slug")
                .and_then(serde_json::Value::as_str)
                .map(sanitize_component)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "speech".to_owned());
    let sha256 = sha256_hex(bytes);
    let filename = format!(
        "{stem}-{}-{}-{}.{}",
        args.provider.id(),
        sanitize_component(&args.model),
        &sha256[..8],
        sniffed.format.extension()
    );
    let path = Path::new(&args.output_dir).join(&filename);
    let path_str = path.to_string_lossy().into_owned();
    boundary.enforce(fs, &path_str, FsAccess::Write).await?;
    if !fs.exists(&path).await {
        fs.write(&path, bytes).await.map_err(|e| {
            BuiltinFailure::new(C_SAVE, format!("audio write failed for `{path_str}`: {e}"))
        })?;
    }
    Ok(SavedAudio {
        path: path_str,
        filename,
        format: sniffed.format.name(),
        mime_type: sniffed.format.mime(),
        size_bytes: bytes.len() as u64,
        sha256,
        duration_ms: sniffed.duration_ms,
    })
}

fn manifest_json(
    args: &TtsArgs,
    saved: &SavedAudio,
    cost_usd: Option<f64>,
    warnings: &[String],
    created_at: &str,
    endpoint_host: Option<&str>,
) -> serde_json::Value {
    let request = serde_json::json!({
        "provider": args.provider.id(),
        "model": args.model,
        "voice": args.voice,
        "text": args.text,
        "format": args.format.map(types::AudioFormat::name),
        "speed": args.speed_explicit.then(|| args.speed_f64()),
        "timeout_ms": u64::try_from(args.timeout.as_millis()).unwrap_or(u64::MAX),
    });
    serde_json::json!({
        "manifest_version": 1,
        "nika_version": env!("CARGO_PKG_VERSION"),
        "created_at": created_at,
        "tool": "nika:tts_generate",
        "provider": args.provider.id(),
        "model": args.model,
        "voice": args.voice,
        "endpoint_host": endpoint_host,
        "request": request,
        "input_hash": sha256_hex(request.to_string().as_bytes()),
        "audio": {
            "path": saved.path,
            "filename": saved.filename,
            "format": saved.format,
            "mime_type": saved.mime_type,
            "size_bytes": saved.size_bytes,
            "sha256": saved.sha256,
            "duration_ms": saved.duration_ms,
        },
        "cost_usd": cost_usd,
        "warnings": warnings,
        "metadata": args.metadata,
    })
}

async fn write_manifest<F: FsReadDyn + FsWriteDyn, Em: Emitter>(
    fs: &F,
    boundary: &FsBoundary,
    args: &TtsArgs,
    saved: &SavedAudio,
    manifest: &serde_json::Value,
    emitter: &Em,
) -> Result<String, BuiltinFailure> {
    let filename = format!(
        "{}-{}.manifest.json",
        saved
            .filename
            .trim_end_matches(&format!(".{}", saved.format)),
        &sha256_hex(saved.sha256.as_bytes())[..8]
    );
    let path = Path::new(&args.output_dir).join(&filename);
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
    emitter.emit(
        "tts_generation.manifest_written",
        serde_json::json!({ "path": path_str }),
    );
    Ok(path_str)
}

fn rfc3339_now<C: ClockDyn>(clock: &C) -> String {
    jiff::Timestamp::try_from(clock.system_now())
        .map_or_else(|_| "1970-01-01T00:00:00Z".to_owned(), |ts| ts.to_string())
}

#[cfg(test)]
mod tests {
    use nika_fs::TokioFs;
    use nika_kernel_mock::{MockClock, MockHttp};

    use crate::NullEmitter;

    use super::*;

    fn scratch() -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("nika-tts-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("scratch");
        root
    }

    fn args_for(dir: &std::path::Path) -> Args {
        let serde_json::Value::Object(map) = serde_json::json!({
            "provider": "mock",
            "text": "bonjour le monde, ceci est un test",
            "output_dir": dir.to_string_lossy(),
            "metadata": { "page_slug": "accueil" },
        }) else {
            unreachable!()
        };
        map
    }

    #[tokio::test]
    async fn mock_pipeline_saves_real_wav_with_manifest_and_is_idempotent() {
        let root = scratch();
        let fs = TokioFs;
        let clock = MockClock::default();
        let emitter = NullEmitter;
        let boundary = FsBoundary::unbounded();
        let keys = TtsKeys::new();

        let out = generate::<_, MockHttp, _, _>(
            &fs,
            None,
            &keys,
            &clock,
            &emitter,
            &boundary,
            &args_for(&root),
        )
        .await
        .expect("mock synthesis");
        let path = out["audio"]["path"].as_str().expect("path").to_owned();
        assert!(path.contains("accueil-mock-mock-tts-1-"), "{path}");
        assert!(
            std::path::Path::new(&path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        );
        let bytes = std::fs::read(&path).expect("file exists");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(
            out["audio"]["sha256"].as_str().map(str::len),
            Some(64),
            "full sha in output"
        );
        assert!(out["audio"]["duration_ms"].as_u64().expect("wav math") > 300);
        assert!(out["manifest_path"].as_str().is_some());
        assert!(out.get("audio").and_then(|a| a.get("b64")).is_none());

        // Idempotent re-run: same bytes → same name → no new files.
        let before = std::fs::read_dir(&root).expect("dir").count();
        let _ = generate::<_, MockHttp, _, _>(
            &fs,
            None,
            &keys,
            &clock,
            &emitter,
            &boundary,
            &args_for(&root),
        )
        .await
        .expect("re-run");
        assert_eq!(std::fs::read_dir(&root).expect("dir").count(), before);
    }

    #[tokio::test]
    async fn unwired_plane_and_missing_keys_fail_002() {
        let root = scratch();
        let fs = TokioFs;
        let clock = MockClock::default();
        let emitter = NullEmitter;
        let boundary = FsBoundary::unbounded();

        let serde_json::Value::Object(cloud) = serde_json::json!({
            "provider": "openai", "text": "x", "output_dir": root.to_string_lossy(),
        }) else {
            unreachable!()
        };
        let err = generate::<_, MockHttp, _, _>(
            &fs,
            None,
            &TtsKeys::new(),
            &clock,
            &emitter,
            &boundary,
            &cloud,
        )
        .await
        .expect_err("unwired");
        assert!(err.code.ends_with("-002"), "{}", err.code);

        let http = MockHttp::new();
        let err = generate(
            &fs,
            Some(&http),
            &TtsKeys::new(),
            &clock,
            &emitter,
            &boundary,
            &cloud,
        )
        .await
        .expect_err("no key");
        assert!(err.message.contains("OPENAI_API_KEY"), "{}", err.message);
    }
}
