// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candle backend — real local inference (feature `local-infer`).
//!
//! Composes the crate's candle-free control modules into the canonical candle
//! quantized generation loop (research-backed · Context7 `/huggingface/candle`
//! + the 2026-06 arXiv pass):
//!
//! ```text
//! load    gguf_file::Content::read → ModelWeights::from_gguf (Qwen3 family)
//!         tokenizers::Tokenizer::from_file (tokenizer.json — NOT in the GGUF)
//! render  ChatFamily::render(messages)            (template.rs)
//! prefill model.forward(prompt, 0)                (KV-cache lives in the model)
//! decode  model.forward([next], offset) → repeat-penalty (logits.rs) →
//!         LogitsProcessor::sample → StopController (EOS set + stop strings)
//! ```
//!
//! Device: CPU by default; Metal behind the `metal` feature. The whole module
//! runs inside the supervised child process (ADR-091) — a panic here never
//! takes down the engine. Goldens run CPU + greedy + fixed seed ONLY (Metal
//! is validated semantically, never byte-equality — backend reductions are
//! not byte-deterministic).
//!
//! `clear_kv_cache()` between requests is MANDATORY (the #1 correctness bug:
//! reusing a model across prompts without clearing desyncs cache positions →
//! garbage). The mutex below serializes requests for exactly that reason
//! (v1 = one in-flight generation per process · the documented limitation).

use std::path::Path;
use std::sync::Mutex;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen3::ModelWeights as Qwen3;

use crate::backend::BackendDyn;
use crate::error::InferLocalError;
use crate::logits::{apply_repeat_penalty, apply_top_n_sigma};
use crate::protocol::{ChatRequest, ChatResponse, FinishReason, Usage};
use crate::sampling::SamplingConfig;
use crate::stop::StopController;
use crate::template::ChatFamily;

/// Hard ceiling on generated tokens when the request does not cap them —
/// a runaway generation must not spin forever (the resource-floor posture,
/// same spirit as the exec-runner output cap).
const DEFAULT_MAX_TOKENS: u32 = 1024;

/// The candle-backed local backend (Qwen3 GGUF · v1 family scope).
///
/// Owns the loaded model + tokenizer behind a `Mutex`: candle's quantized
/// model is `&mut self` on `forward` (the KV-cache lives inside), so requests
/// are serialized — v1's documented one-in-flight contract, not a perf bug.
pub struct CandleBackend {
    state: Mutex<ModelState>,
    eos_ids: Vec<u32>,
    family: ChatFamily,
    model_id: String,
}

struct ModelState {
    model: Qwen3,
    tokenizer: tokenizers::Tokenizer,
}

impl CandleBackend {
    /// Load a quantized Qwen3-family GGUF + its `tokenizer.json`.
    ///
    /// `device` selection: CPU always works; Metal when the `metal` feature
    /// is enabled and the host supports it (fall back to CPU otherwise).
    ///
    /// # Errors
    ///
    /// [`InferLocalError::ModelNotFound`] when either file is absent;
    /// [`InferLocalError::Backend`] for a GGUF/tokenizer parse failure.
    pub fn load_qwen3(
        gguf_path: &Path,
        tokenizer_path: &Path,
        model_id: impl Into<String>,
    ) -> Result<Self, InferLocalError> {
        let device = Self::pick_device();

        let mut file =
            std::fs::File::open(gguf_path).map_err(|_| InferLocalError::ModelNotFound {
                model: gguf_path.display().to_string(),
            })?;
        let content =
            gguf_file::Content::read(&mut file).map_err(|e| InferLocalError::Backend {
                reason: format!("GGUF parse failed: {e}"),
            })?;
        let model = Qwen3::from_gguf(content, &mut file, &device).map_err(|e| {
            InferLocalError::Backend {
                reason: format!("model load failed: {e}"),
            }
        })?;

        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path).map_err(|e| {
            InferLocalError::Backend {
                reason: format!("tokenizer load failed: {e}"),
            }
        })?;

        // Qwen3's TWO end tokens (the dual-EOS bug guard · stop.rs doc):
        // <|im_end|> ends the assistant turn · <|endoftext|> ends the text.
        let vocab = tokenizer.get_vocab(true);
        let eos_ids: Vec<u32> = ["<|im_end|>", "<|endoftext|>"]
            .iter()
            .filter_map(|t| vocab.get(*t).copied())
            .collect();
        if eos_ids.is_empty() {
            return Err(InferLocalError::Backend {
                reason: "tokenizer has neither <|im_end|> nor <|endoftext|>".to_owned(),
            });
        }

        Ok(Self {
            state: Mutex::new(ModelState { model, tokenizer }),
            eos_ids,
            family: ChatFamily::Qwen,
            model_id: model_id.into(),
        })
    }

    /// CPU, or Metal when compiled in AND available (silent CPU fallback —
    /// availability is a host property, not an error).
    fn pick_device() -> Device {
        #[cfg(feature = "metal")]
        {
            if let Ok(d) = Device::new_metal(0) {
                return d;
            }
        }
        Device::Cpu
    }

    /// Translate the crate's [`SamplingConfig`] into candle's `Sampling`.
    /// Greedy (the agentic default) maps to `ArgMax` — no RNG draw at all.
    fn to_candle_sampling(cfg: &SamplingConfig) -> Sampling {
        if cfg.is_greedy() {
            return Sampling::ArgMax;
        }
        match (cfg.top_k, cfg.top_p) {
            (Some(k), Some(p)) => Sampling::TopKThenTopP {
                k,
                p,
                temperature: cfg.temperature,
            },
            (Some(k), None) => Sampling::TopK {
                k,
                temperature: cfg.temperature,
            },
            (None, Some(p)) => Sampling::TopP {
                p,
                temperature: cfg.temperature,
            },
            (None, None) => Sampling::All {
                temperature: cfg.temperature,
            },
        }
    }

    /// The blocking generation entry: render → tokenize → prefill → decode
    /// loop → detokenize → response. (Runs inline in the dedicated sidecar
    /// process; the Mutex serializes generations — the v1 contract.)
    fn generate_blocking(&self, request: &ChatRequest) -> Result<ChatResponse, InferLocalError> {
        let cfg = SamplingConfig::from_request(request);
        let stop = StopController::new(self.eos_ids.iter().copied(), &request.stop);
        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS) as usize;

        let mut state = self.state.lock().map_err(|_| InferLocalError::Backend {
            reason: "model mutex poisoned (a prior generation panicked)".to_owned(),
        })?;

        // Render the conversation through the family template, tokenize.
        let prompt = self.family.render(&request.messages);
        let encoding = state
            .tokenizer
            .encode(prompt.as_str(), /*add_special_tokens=*/ true)
            .map_err(|e| InferLocalError::Backend {
                reason: format!("tokenize failed: {e}"),
            })?;
        let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
        if prompt_tokens.is_empty() {
            return Err(InferLocalError::InvalidRequest {
                reason: "empty prompt after templating".to_owned(),
            });
        }

        // MANDATORY between requests — stale cache positions produce garbage.
        state.model.clear_kv_cache();

        let mut logits_processor =
            LogitsProcessor::from_sampling(cfg.seed, Self::to_candle_sampling(&cfg));

        let (generated, finish) = decode_loop(
            &mut state,
            &mut logits_processor,
            &cfg,
            &stop,
            &prompt_tokens,
            max_tokens,
        )?;

        let mut text = state
            .tokenizer
            .decode(&generated, /*skip_special_tokens=*/ true)
            .map_err(|e| InferLocalError::Backend {
                reason: format!("decode failed: {e}"),
            })?;
        if stop.hit_stop_string(&text) {
            let keep = stop.trim_stop(&text).len();
            text.truncate(keep); // trim_stop returns a prefix — truncate in place
        }

        let completion = u32::try_from(generated.len()).unwrap_or(u32::MAX);
        let prompt_n = u32::try_from(prompt_tokens.len()).unwrap_or(u32::MAX);
        Ok(ChatResponse::single(
            format!("local-{}", cfg.seed),
            self.model_id.clone(),
            text,
            finish,
            Usage::new(prompt_n, completion),
        ))
    }
}

/// Wrap a candle error into the crate's typed backend error.
fn backend_err(what: &str, e: impl std::fmt::Display) -> InferLocalError {
    InferLocalError::Backend {
        reason: format!("{what}: {e}"),
    }
}

/// Prefill + the one-token-at-a-time decode loop (the canonical candle
/// quantized path: `forward(input, offset)` with the KV-cache inside the
/// model; the offset is the ENTIRE cache contract).
fn decode_loop(
    state: &mut ModelState,
    logits_processor: &mut LogitsProcessor,
    cfg: &SamplingConfig,
    stop: &StopController,
    prompt_tokens: &[u32],
    max_tokens: usize,
) -> Result<(Vec<u32>, FinishReason), InferLocalError> {
    let device = CandleBackend::pick_device();

    // ── prefill: the whole prompt at offset 0 ──
    let input = Tensor::new(prompt_tokens, &device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| backend_err("prefill tensor", e))?;
    let logits = state
        .model
        .forward(&input, 0)
        .and_then(|l| l.squeeze(0))
        .map_err(|e| backend_err("prefill forward", e))?;
    let mut next = logits_processor
        .sample(&logits)
        .map_err(|e| backend_err("sample", e))?;

    // ── decode: one token per step, offset advancing ──
    let mut generated: Vec<u32> = vec![next];
    let has_stops = stop.has_stop_strings();
    for index in 0..max_tokens {
        if stop.is_eos(next) {
            generated.pop(); // never include the EOS token in the text
            return Ok((generated, FinishReason::Stop));
        }
        // Stop-string check on the decoded-so-far text (string-level — safe
        // across tokenizations; only when the request declared stops).
        if has_stops {
            let so_far = state
                .tokenizer
                .decode(&generated, /*skip_special_tokens=*/ true)
                .unwrap_or_default();
            if stop.hit_stop_string(&so_far) {
                return Ok((generated, FinishReason::Stop));
            }
        }
        if index + 1 >= max_tokens {
            break;
        }

        let input = Tensor::new(&[next], &device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| backend_err("decode tensor", e))?;
        let mut logits = state
            .model
            .forward(&input, prompt_tokens.len() + index)
            .and_then(|l| l.squeeze(0))
            .map_err(|e| backend_err("decode forward", e))?;

        // Optional pre-softmax transforms (our native impls · logits.rs):
        // repeat penalty over the recent window, then top-nσ truncation
        // (arXiv:2411.07641 · temperature-invariant noise cut).
        if cfg.repeat_penalty > 1.0 || cfg.top_n_sigma.is_some() {
            let mut raw = logits
                .to_vec1::<f32>()
                .map_err(|e| backend_err("logits to_vec1", e))?;
            if cfg.repeat_penalty > 1.0 {
                let start = generated.len().saturating_sub(cfg.repeat_last_n);
                apply_repeat_penalty(&mut raw, cfg.repeat_penalty, &generated[start..]);
            }
            if let Some(n) = cfg.top_n_sigma {
                apply_top_n_sigma(&mut raw, n);
            }
            logits = Tensor::new(raw, &device).map_err(|e| backend_err("logits rebuild", e))?;
        }

        next = logits_processor
            .sample(&logits)
            .map_err(|e| backend_err("sample", e))?;
        generated.push(next);
    }
    Ok((generated, FinishReason::Length))
}

impl BackendDyn for CandleBackend {
    async fn generate(&self, request: &ChatRequest) -> Result<ChatResponse, InferLocalError> {
        // The forward pass is CPU/GPU-bound — run it off the async thread.
        // The Mutex serializes generations (v1 single-flight contract).
        //
        // SAFETY of the block_in_place alternative rejected: spawn_blocking
        // needs 'static, and self lives in an Arc at the server layer; for
        // the v1 scaffold we run inline — the sidecar process is dedicated
        // to inference, so blocking its runtime is acceptable and documented.
        self.generate_blocking(request)
    }
}
