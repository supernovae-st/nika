// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika model serve` — the sovereign local-inference launch surface
//! (ADR-091 · ADR-093).
//!
//! FOREGROUND by design: the process the operator (or a workflow's `exec`
//! task, or the future L3 supervisor) starts, watches and stops. The v1
//! contract is deliberately narrow — one loopback `tiny_http` server over
//! one loaded GGUF, one generation at a time, Ctrl-C ends it. Process
//! supervision (spawn/health/restart) is the runtime layer's job per
//! ADR-091 §isolation; this verb is the rung below it.
//!
//! The served endpoint is reached over the EXISTING `OpenAiCompat` wire
//! (zero new dispatch seam): point any keyless local profile's base URL at
//! it (`NIKA_LLAMACPP_BASE_URL=http://127.0.0.1:<port>`) and a workflow's
//! `model: llamacpp/<id>` lands on the sidecar.
//!
//! The whole surface is feature-gated (`local-infer` · off by default —
//! the default binary links zero inference deps): the SUBCOMMAND always
//! exists so the tree teaches it, the default BODY refuses with the build
//! recipe (exit `3` · environment class).

// The healthy path prints a banner then serves for the process' whole
// life — output DURING a blocking serve cannot be deferred through
// `VerbOutput`. The same sanctioned exemption `run` and `main.rs` carry.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::path::Path;
// `PathBuf` rides only the feature/test axes (`ServePlan` · fixtures).
#[cfg(any(feature = "local-infer", test))]
use std::path::PathBuf;

use crate::verbs::{VerbOutput, exit};

/// The default loopback port (`nika mcp` owns 8123; this stays clear of
/// the 5 external local servers' defaults: 11434 · 1234 · 8080 · 8000).
pub const DEFAULT_PORT: u16 = 8712;

/// Serve `gguf` (Qwen3 family · v1 loader scope) in the foreground on
/// loopback `port`. `tokenizer` defaults to `tokenizer.json` beside the
/// GGUF (the layout a Hugging Face repo download produces); `model_id`
/// (the id responses report) defaults to the GGUF file stem.
///
/// Returns ONLY on refusal (missing file · unreadable model · bind
/// failure · a build without the feature) — a healthy server runs until
/// the process ends (Ctrl-C). Refusals are environment-class (exit `3`).
#[must_use]
pub fn serve(
    gguf: &Path,
    tokenizer: Option<&Path>,
    port: u16,
    model_id: Option<&str>,
) -> VerbOutput {
    #[cfg(feature = "local-infer")]
    {
        serve_local(gguf, tokenizer, port, model_id)
    }
    #[cfg(not(feature = "local-infer"))]
    {
        let _ = (gguf, tokenizer, port, model_id);
        feature_stub()
    }
}

/// The resolved boot inputs: every path verified present, the id named.
#[cfg(any(feature = "local-infer", test))]
#[derive(Debug, PartialEq, Eq)]
struct ServePlan {
    gguf: PathBuf,
    tokenizer: PathBuf,
    model_id: String,
}

/// Resolve + verify the boot inputs BEFORE any load: a wrong path must
/// refuse with the exact fix, not a parse failure deep in the loader.
#[cfg(any(feature = "local-infer", test))]
fn plan(
    gguf: &Path,
    tokenizer: Option<&Path>,
    model_id: Option<&str>,
) -> Result<ServePlan, VerbOutput> {
    if !gguf.is_file() {
        return Err(VerbOutput {
            text: format!(
                "model serve: no model file at {}\n  fix: point --model at a \
                 Qwen3-family .gguf (download one from the Hugging Face Hub \
                 — its tokenizer.json goes beside it)\n",
                gguf.display()
            ),
            code: exit::ENV,
        });
    }
    let tokenizer =
        tokenizer.map_or_else(|| gguf.with_file_name("tokenizer.json"), Path::to_path_buf);
    if !tokenizer.is_file() {
        return Err(VerbOutput {
            text: format!(
                "model serve: no tokenizer at {}\n  fix: download the model \
                 repo's tokenizer.json beside the GGUF, or name one with \
                 --tokenizer <path>\n",
                tokenizer.display()
            ),
            code: exit::ENV,
        });
    }
    Ok(ServePlan {
        model_id: model_id.map_or_else(|| derive_model_id(gguf), str::to_owned),
        gguf: gguf.to_path_buf(),
        tokenizer,
    })
}

/// The id responses report when `--model-id` is absent: the GGUF file
/// stem (`qwen3-4b-instruct-q4_k_m.gguf` → `qwen3-4b-instruct-q4_k_m`).
#[cfg(any(feature = "local-infer", test))]
fn derive_model_id(gguf: &Path) -> String {
    gguf.file_stem()
        .map_or_else(|| "local".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The default build's honest refusal: name the feature and the build
/// recipe — never pretend the lane is one flag away at runtime.
#[cfg(any(not(feature = "local-infer"), test))]
fn feature_stub() -> VerbOutput {
    VerbOutput {
        text: "model serve: this binary was built without local inference \
               (the default build links zero inference dependencies)\n  \
               fix: build from source with the feature — cargo build -p \
               nika-cli --features local-infer   (Apple GPU: --features \
               metal)\n"
            .to_owned(),
        code: exit::ENV,
    }
}

/// Load the GGUF, bind loopback, announce, serve until the process ends.
#[cfg(feature = "local-infer")]
fn serve_local(
    gguf: &Path,
    tokenizer: Option<&Path>,
    port: u16,
    model_id: Option<&str>,
) -> VerbOutput {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    let plan = match plan(gguf, tokenizer, model_id) {
        Ok(plan) => plan,
        Err(refusal) => return refusal,
    };
    eprintln!(
        "nika model serve · loading {} (a first load reads the whole file)",
        plan.gguf.display()
    );
    let backend = match nika_infer_local::CandleBackend::load_qwen3(
        &plan.gguf,
        &plan.tokenizer,
        &plan.model_id,
    ) {
        Ok(backend) => backend,
        Err(err) => {
            return VerbOutput {
                text: format!(
                    "model serve: {err}\n  fix: the v1 loader serves \
                         Qwen3-family GGUFs with their tokenizer.json — \
                         check both files\n"
                ),
                code: exit::ENV,
            };
        }
    };
    // Loopback ONLY (ADR-093: no TLS · no auth · the operator owns the
    // port) — widening the bind is the supervisor era's decision, not v1's.
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let handle = match nika_infer_local::serve(Arc::new(backend), addr) {
        Ok(handle) => handle,
        Err(err) => {
            return VerbOutput {
                text: format!(
                    "model serve: {err}\n  fix: another process may own the \
                     port — pick one with --port <n>\n"
                ),
                code: exit::ENV,
            };
        }
    };
    let bound = handle.addr();
    eprintln!(
        "nika model serve · http://{bound}/v1/chat/completions · {} · \
         loopback only · one generation at a time · Ctrl-C stops",
        plan.model_id
    );
    eprintln!(
        "  reach it from a workflow: export NIKA_LLAMACPP_BASE_URL=http://{bound} \
         then model: llamacpp/{}",
        plan.model_id
    );
    // The handle must outlive the park (dropping it stops the server).
    // Ctrl-C ends the foreground process — the v1 lifecycle contract.
    let _serving = handle;
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The doctor tests' exact fixture idiom (CARGO_TARGET_TMPDIR is unset
    // for lib unit tests — fall back beside the build dir).
    #[allow(clippy::disallowed_methods)]
    fn temp_dir(name: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
            || {
                std::env::current_dir()
                    .expect("current dir")
                    .join("target")
                    .join("tmp")
            },
            PathBuf::from,
        );
        let dir = base.join(format!("nika-model-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn plan_refuses_a_missing_gguf_with_the_fix() {
        let dir = temp_dir("missing-gguf");
        let refusal = plan(&dir.join("nope.gguf"), None, None).expect_err("must refuse");
        assert_eq!(refusal.code, exit::ENV);
        assert!(refusal.text.contains("no model file"), "{}", refusal.text);
        assert!(refusal.text.contains("--model"), "{}", refusal.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plan_defaults_to_the_sibling_tokenizer_and_the_file_stem_id() {
        let dir = temp_dir("sibling");
        let gguf = dir.join("qwen3-4b-instruct-q4_k_m.gguf");
        std::fs::write(&gguf, b"stub").expect("fixture");
        std::fs::write(dir.join("tokenizer.json"), b"{}").expect("fixture");
        let plan = plan(&gguf, None, None).expect("resolves");
        assert_eq!(plan.tokenizer, dir.join("tokenizer.json"));
        assert_eq!(plan.model_id, "qwen3-4b-instruct-q4_k_m");
        assert_eq!(plan.gguf, gguf);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plan_refuses_a_missing_tokenizer_naming_both_fixes() {
        let dir = temp_dir("no-tokenizer");
        let gguf = dir.join("model.gguf");
        std::fs::write(&gguf, b"stub").expect("fixture");
        let refusal = plan(&gguf, None, None).expect_err("must refuse");
        assert_eq!(refusal.code, exit::ENV);
        assert!(refusal.text.contains("tokenizer.json"), "{}", refusal.text);
        assert!(refusal.text.contains("--tokenizer"), "{}", refusal.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plan_honors_explicit_tokenizer_and_model_id() {
        let dir = temp_dir("explicit");
        let gguf = dir.join("weights.gguf");
        let tok = dir.join("tok.json");
        std::fs::write(&gguf, b"stub").expect("fixture");
        std::fs::write(&tok, b"{}").expect("fixture");
        let resolved = plan(&gguf, Some(&tok), Some("qwen3")).expect("resolves");
        assert_eq!(resolved.tokenizer, tok);
        assert_eq!(resolved.model_id, "qwen3");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_stub_names_the_feature_and_exits_environment() {
        let out = feature_stub();
        assert_eq!(out.code, exit::ENV);
        assert!(out.text.contains("local-infer"), "{}", out.text);
        assert!(out.text.contains("cargo build"), "{}", out.text);
    }

    /// The REAL serve path refuses a missing model file cleanly — the
    /// documented no-fixture e2e (a live-model boot is a manual step).
    #[cfg(feature = "local-infer")]
    #[test]
    fn serve_refuses_a_missing_model_before_binding_anything() {
        let dir = temp_dir("serve-missing");
        let out = serve(&dir.join("absent.gguf"), None, DEFAULT_PORT, None);
        assert_eq!(out.code, exit::ENV);
        assert!(out.text.contains("no model file"), "{}", out.text);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A present-but-invalid GGUF exercises the loader's typed refusal
    /// through the real feature-on path (no model fixture in the repo).
    #[cfg(feature = "local-infer")]
    #[test]
    fn serve_refuses_a_garbage_gguf_with_the_loader_error() {
        let dir = temp_dir("serve-garbage");
        let gguf = dir.join("junk.gguf");
        std::fs::write(&gguf, b"not a gguf").expect("fixture");
        std::fs::write(dir.join("tokenizer.json"), b"{}").expect("fixture");
        let out = serve(&gguf, None, DEFAULT_PORT, None);
        assert_eq!(out.code, exit::ENV);
        assert!(out.text.contains("Qwen3-family"), "{}", out.text);
        let _ = std::fs::remove_dir_all(dir);
    }
}
