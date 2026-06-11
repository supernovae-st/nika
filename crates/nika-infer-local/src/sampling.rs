// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sampling policy — `ChatRequest` knobs → a backend-agnostic plan.
//!
//! candle-free on purpose: this is the *decision* (greedy vs which truncation,
//! seed, penalties), the candle binding translates it to candle's `Sampling`
//! enum + `LogitsProcessor::from_sampling(seed, …)`. Keeping the policy here
//! means it is unit-tested in the default build, independent of `local-infer`.
//!
//! Defaults follow the research (ADR-091): **greedy (`ArgMax`) is the default**
//! for agentic / tool-calling determinism — sampling diversity helps creative
//! generation, not structured workflow steps. `min_p` (arXiv:2407.01082) is a
//! quality tweak applied in the binding's `sample_f` (candle has no min-p
//! variant); off by default. `seed` is always concrete so output is
//! reproducible for golden tests even when the request omits it.

use crate::protocol::ChatRequest;

/// Default seed when a request omits one — fixed, so unset ⇒ deterministic.
pub const DEFAULT_SEED: u64 = 0;
/// Repeat-penalty look-back window (candle/llama.cpp convention).
pub const DEFAULT_REPEAT_LAST_N: usize = 64;

/// A resolved sampling plan, backend-agnostic.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplingConfig {
    /// 0.0 ⇒ greedy (`ArgMax`). >0 ⇒ stochastic with this temperature.
    pub temperature: f64,
    /// Nucleus cutoff, if any.
    pub top_p: Option<f64>,
    /// Top-k cutoff, if any.
    pub top_k: Option<usize>,
    /// min-p floor (`prob >= min_p * max_prob`); applied in `sample_f`. Off = None.
    pub min_p: Option<f32>,
    /// Top-nσ truncation (arXiv:2411.07641): keep logits within `n·σ` of the
    /// max, PRE-softmax — temperature-invariant survivor set. Off = None.
    pub top_n_sigma: Option<f32>,
    /// RNG seed — always concrete (determinism).
    pub seed: u64,
    /// Repeat penalty (1.0 = off; 1.1 typical).
    pub repeat_penalty: f32,
    /// How many recent tokens the penalty looks back over.
    pub repeat_last_n: usize,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            temperature: 0.0, // greedy — the agentic default
            top_p: None,
            top_k: None,
            min_p: None,
            top_n_sigma: None,
            seed: DEFAULT_SEED,
            repeat_penalty: 1.0, // off by default — least surprise
            repeat_last_n: DEFAULT_REPEAT_LAST_N,
        }
    }
}

impl SamplingConfig {
    /// Resolve a request's sampling knobs into a plan, applying the defaults.
    ///
    /// Penalty fields (`repeat_penalty` / `repeat_last_n`) are NOT in the
    /// `OpenAI` request subset — they always take the defaults here and are
    /// configured per-family by the backend. If a request knob is ever added
    /// for them, wire it EXPLICITLY (the `..default()` spread below would
    /// silently mask it).
    #[must_use]
    pub fn from_request(req: &ChatRequest) -> Self {
        Self {
            temperature: req.temperature.map_or(0.0, f64::from),
            top_p: req.top_p.map(f64::from),
            top_k: None, // not in the OpenAI subset we serve; backend default
            min_p: None,
            seed: req.seed.unwrap_or(DEFAULT_SEED),
            ..Self::default()
        }
    }

    /// Greedy when temperature is non-positive — deterministic, no RNG draw.
    /// This is the path golden tests run on (seed becomes irrelevant).
    #[must_use]
    pub fn is_greedy(&self) -> bool {
        self.temperature <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Message, Role};

    fn req_with(temp: Option<f32>, top_p: Option<f32>, seed: Option<u64>) -> ChatRequest {
        ChatRequest {
            model: "m".to_owned(),
            messages: vec![Message::new(Role::User, "x")],
            temperature: temp,
            top_p,
            seed,
            ..Default::default()
        }
    }

    #[test]
    fn unset_temperature_is_greedy() {
        let c = SamplingConfig::from_request(&req_with(None, None, None));
        assert!(c.is_greedy(), "agentic default must be greedy");
        assert_eq!(c.seed, DEFAULT_SEED, "unset seed is deterministic");
    }

    #[test]
    fn zero_temperature_is_greedy() {
        let c = SamplingConfig::from_request(&req_with(Some(0.0), None, None));
        assert!(c.is_greedy());
    }

    #[test]
    fn positive_temperature_samples() {
        let c = SamplingConfig::from_request(&req_with(Some(0.7), Some(0.9), Some(42)));
        assert!(!c.is_greedy());
        // f32 request knobs widen to f64 — compare with a tolerance, never `==`.
        assert!((c.temperature - 0.7).abs() < 1e-6);
        assert!(c.top_p.is_some_and(|p| (p - 0.9).abs() < 1e-6));
        assert_eq!(c.seed, 42);
    }

    #[test]
    fn default_penalty_is_off() {
        // Repetition penalty off by default — opt-in, no silent output shaping.
        assert!((SamplingConfig::default().repeat_penalty - 1.0).abs() < f32::EPSILON);
    }
}
