// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-providers — the LLM provider layer (L1.5 service)
//!
//! The shared provider layer both `nika-verb-infer` and `nika-verb-agent`
//! depend on (D-2026-05-22-N17 — providers live BELOW the verbs so no
//! verb→verb sideways dep ever forms).
//!
//! ## Shape
//!
//! - **Contract** — implements the L0.5 `nika_kernel::ai::provider` ISP
//!   traits (`ProviderInferDyn` · `ProviderStreamDyn` · `ProviderMeta`).
//! - **Transport** — rides the kernel http seam (`HttpPostDyn` · generic
//!   `Arc<H>`, the wiring layer injects `nika-http`'s `ReqwestHttp`). This
//!   crate owns **zero** TLS/connection code: one reqwest site in the
//!   workspace, provider calls inherit the effect floor.
//! - **Profiles** — the canonical 14 (8 cloud · 5 local · 1 mock per
//!   `nika-spec/canon.yaml` · D-2026-06-10-N2) over **three wire formats**:
//!   Anthropic Messages · OpenAI-compat Chat Completions (12 of the 14) ·
//!   Gemini `generateContent`.
//! - **Keys** — kernel [`Secret`](nika_kernel::secret::Secret)
//!   (zeroize-on-drop · redacted `Debug`), resolved from the env ladder
//!   `NIKA_<ID>_API_KEY` → conventional var, or injected explicitly.
//!   Never logged · never serialized.
//!
//! ## Example (mock · zero key · zero network)
//!
//! ```
//! use nika_kernel::ai::provider::{InferRequest, Message, Role};
//! use nika_providers::{ProviderRegistry, ProvidersConfig};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = ProviderRegistry::without_http(ProvidersConfig::default());
//! let provider = registry.resolve("mock/echo")?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod census;
#[cfg(test)]
mod parity_tests;
pub mod probe;
pub mod profile;
pub mod registry;
pub mod resolve_access;
mod sse;
#[cfg(test)]
mod test_support;
pub mod wire;

pub use census::{AccessCensus, AccessPath, SeatFact};
pub use probe::{KeyAuth, classify_http_status, classify_key_value};
pub use profile::{
    CANONICAL_IDS, PREFIX_REFUSAL_CODE, Profile, ResolveRefusal, WireFormat, canonical_provider,
    catalog_warning, resolve_refusal, seed, server_backed_local,
};
pub use registry::{NoHttp, ProviderRegistry, ProvidersConfig, ResolvedProvider};
#[cfg(feature = "access-harness")]
pub use resolve_access::first_ready_infer_harness;
pub use resolve_access::{
    AccessCandidate, AccessRefusal, PinRefusal, access_plan_map, candidates_for,
    first_ready_harness, provider_of, refuse_pin, refuse_pin_for_verbs, resolve_access,
};
