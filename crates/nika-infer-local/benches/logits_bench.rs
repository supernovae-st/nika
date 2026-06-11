// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 7 BENCHMARKS · criterion · the decode-loop hot path.
//!
//! These transforms run ONCE PER GENERATED TOKEN, on a vocab-sized slice
//! (Qwen3 vocab = 151,936) — their cost is pure overhead added to every
//! decode step, so the envelope matters. Reference point: llguidance
//! computes a full grammar token-mask in ~50µs at 128k vocab; our simple
//! transforms must sit WELL under that.
//!
//! Targets (CPU · vocab 151,936):
//! - `min_p`           · single pass + max scan        · target <300µs
//! - `top_n_sigma`     · two stat passes + mask pass   · target <500µs
//! - `repeat_penalty`  · 64-token window, O(window)    · target <1µs
//! - `token_mask`      · single pass                   · target <300µs
//!
//! Model tok/s itself is e2e-gated (the real-GGUF candle test prints its
//! timing) — a criterion harness cannot run the model in CI.

use criterion::{Criterion, criterion_group, criterion_main};
use nika_infer_local::{apply_min_p, apply_repeat_penalty, apply_token_mask, apply_top_n_sigma};
use std::hint::black_box;

/// Qwen3's actual vocabulary size — the realistic slice length.
const VOCAB: usize = 151_936;

/// Deterministic pseudo-logits (no rand dep): a hash-spread over [-10, 10)
/// with a distinct peak so truncation transforms have an informative region.
#[allow(clippy::cast_precision_loss)] // bench fixture · exact values irrelevant
fn synthetic_logits() -> Vec<f32> {
    let mut v: Vec<f32> = (0..VOCAB)
        .map(|i| {
            let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            // (h >> 40) is a 24-bit value → fits f32's mantissa exactly.
            ((h >> 40) as u32 as f32 / 16_777_216.0) * 20.0 - 10.0
        })
        .collect();
    v[42] = 14.0; // the peak
    v
}

/// Softmax-ish probabilities (normalized positives) for the post-softmax ops.
fn synthetic_probs() -> Vec<f32> {
    let logits = synthetic_logits();
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|l| (l - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.into_iter().map(|e| e / sum).collect()
}

fn bench_logits(c: &mut Criterion) {
    let logits = synthetic_logits();
    let probs = synthetic_probs();
    let vocab_u32 = u32::try_from(VOCAB).unwrap_or(u32::MAX);
    let recent: Vec<u32> = (0..64u32).map(|i| (i * 1009) % vocab_u32).collect();
    let mask: Vec<bool> = (0..VOCAB).map(|i| i % 7 != 0).collect();

    c.bench_function("min_p_vocab152k", |b| {
        b.iter_batched(
            || probs.clone(),
            |mut p| black_box(apply_min_p(&mut p, 0.05)),
            criterion::BatchSize::LargeInput,
        );
    });

    c.bench_function("top_n_sigma_vocab152k", |b| {
        b.iter_batched(
            || logits.clone(),
            |mut l| black_box(apply_top_n_sigma(&mut l, 2.0)),
            criterion::BatchSize::LargeInput,
        );
    });

    c.bench_function("repeat_penalty_64win_vocab152k", |b| {
        b.iter_batched(
            || logits.clone(),
            |mut l| {
                apply_repeat_penalty(&mut l, 1.1, &recent);
                // black_box the whole slice — black_box(l[0]) would let the
                // compiler skip the other 63 penalized writes (review P3).
                black_box(&l);
            },
            criterion::BatchSize::LargeInput,
        );
    });

    c.bench_function("token_mask_vocab152k", |b| {
        b.iter_batched(
            || probs.clone(),
            |mut p| black_box(apply_token_mask(&mut p, &mask)),
            criterion::BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, bench_logits);
criterion_main!(benches);
