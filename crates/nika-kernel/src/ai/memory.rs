// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Memory traits + types — Cortex kernel hooks.
//!
//! ISP decomposition: `MemoryRemember`, `MemoryRecall`, `MemoryForget`.
//! Super-trait: `MemoryStore` (blanket for all 3).
//! Separate: `EmbeddingProvider` (embedding generation).
//!
//! These hooks land Phase 1 to avoid breaking-change cascades on
//! `#[non_exhaustive]` structs (ROI 6.7x per `POST_AUDIT` decision 3).
//! Business logic lives in `nika-memory` (Phase 9+).

use std::collections::BTreeMap;

// ─── Types descended to nika-error (Phase 0) ────────────────────────
pub use nika_error::memory::{MemoryDirective, MemoryFrameRef, MemoryId, MemoryLevel};

use nika_error::id::TenantId;

/// A memory frame to store.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryFrame {
    /// The content to remember.
    pub content: String,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Searchable tags.
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: BTreeMap<String, String>,
    /// Origin source (e.g., workflow name, task ID).
    pub source: Option<String>,
    /// When the observation was made (Unix epoch nanoseconds).
    ///
    /// Canonical timestamp resolution across the `SuperNovae` galaxy
    /// (cohérent `screen.rs` `captured_at_ns` + cockpit `nano_now()` ·
    /// EC-4 ratchet 2026-05-14 · USER GATE OUI ns canonical).
    pub observed_at: Option<u64>,
    /// Encryption cipher used (v0.95 Cortex — encrypted memory).
    /// Reserved: always `None` until nika-memory crate ships.
    pub cipher: Option<String>,
    /// Provenance chain (v0.95 Cortex — who created this memory).
    /// Reserved: always `None` until nika-memory crate ships.
    pub provenance: Option<String>,
    /// Retention policy tag (v0.95 Cortex — TTL / archival).
    /// Reserved: always `None` until nika-memory crate ships.
    pub retention: Option<String>,
    /// Redacted field paths (v0.95 Cortex — PII scrubbing).
    /// Reserved: always `None` until nika-memory crate ships.
    pub redactions: Option<Vec<String>>,
}

impl MemoryFrame {
    /// Create a new memory frame with content and level.
    #[must_use]
    pub fn new(content: impl Into<String>, level: MemoryLevel) -> Self {
        Self {
            content: content.into(),
            level,
            tags: Vec::new(),
            metadata: BTreeMap::new(),
            source: None,
            observed_at: None,
            cipher: None,
            provenance: None,
            retention: None,
            redactions: None,
        }
    }
}

/// A query to recall memories.
///
/// `tenant` is the mandatory keyspace scope — every query returns only
/// frames stored under the same tenant. The default constructor uses
/// [`TenantId::default_tenant`] for single-tenant deployments; multi-tenant
/// hosts MUST call [`RecallQuery::scoped`] or mutate the field before
/// executing the query.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RecallQuery {
    /// Tenant keyspace — reserved at v0.81 for v0.95 multi-tenant
    /// Cortex (ADR-031). Every recall MUST scope by tenant.
    pub tenant: TenantId,
    /// Text to search for (semantic similarity).
    pub text: String,
    /// Filter by memory levels.
    pub levels: Option<Vec<MemoryLevel>>,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Minimum similarity score (0.0–1.0).
    pub min_score: Option<f32>,
    /// Filter: only return memories observed after this timestamp (Unix ns).
    /// Reserved for `nika-memory-temporal` (v0.95). Cohérent EC-4 ratchet
    /// 2026-05-14 · ns canonical across the `SuperNovae` galaxy.
    pub observed_after: Option<u64>,
    /// Filter: only return memories observed before this timestamp (Unix ns).
    /// Reserved for `nika-memory-temporal` (v0.95). Cohérent EC-4 ratchet
    /// 2026-05-14 · ns canonical across the `SuperNovae` galaxy.
    pub observed_before: Option<u64>,
}

impl RecallQuery {
    /// Create a new recall query scoped to the default tenant.
    ///
    /// Single-tenant deployments can use this and ignore the field;
    /// multi-tenant deployments should prefer [`Self::scoped`].
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self::scoped(TenantId::default_tenant(), text)
    }

    /// Create a new recall query with an explicit tenant scope.
    #[must_use]
    pub fn scoped(tenant: TenantId, text: impl Into<String>) -> Self {
        Self {
            tenant,
            text: text.into(),
            levels: None,
            tags: None,
            limit: None,
            min_score: None,
            observed_after: None,
            observed_before: None,
        }
    }
}

/// A memory recall result with similarity score.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MemoryHit {
    /// Memory identifier.
    pub id: MemoryId,
    /// Memory content.
    pub content: String,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Similarity score (0.0–1.0).
    pub score: f32,
    /// Tags.
    pub tags: Vec<String>,
    /// Metadata.
    pub metadata: BTreeMap<String, String>,
    /// When this memory was observed (parity with `MemoryFrame`).
    /// Reserved for nika-memory-temporal ranking (v0.95).
    pub observed_at: Option<u64>,
    /// Source of this memory (parity with `MemoryFrame`).
    /// Reserved for v0.95 Cortex provenance tracking.
    pub source: Option<String>,
}

impl MemoryHit {
    /// Create a new memory hit.
    #[must_use]
    pub fn new(id: MemoryId, content: impl Into<String>, level: MemoryLevel, score: f32) -> Self {
        Self {
            id,
            content: content.into(),
            level,
            score,
            tags: Vec::new(),
            metadata: BTreeMap::new(),
            observed_at: None,
            source: None,
        }
    }
}

// ─── Errors ──────────────────────────────────────────────────────────

/// Memory subsystem errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum MemoryError {
    /// Memory system not available.
    #[error("memory unavailable: {reason}")]
    #[diagnostic(help("{}", nika_error::codes::code_help(nika_error::codes::NIKA_601)))]
    Unavailable {
        /// Why the memory system is unavailable.
        reason: String,
    },

    /// Memory not found.
    #[error("memory not found: {id}")]
    #[diagnostic(help("{}", nika_error::codes::code_help(nika_error::codes::NIKA_602)))]
    NotFound {
        /// The ID that was looked up.
        id: MemoryId,
    },

    /// Embedding generation failed.
    #[error("embedding failed: {reason}")]
    #[diagnostic(help("{}", nika_error::codes::code_help(nika_error::codes::NIKA_603)))]
    EmbeddingFailed {
        /// Why embedding failed.
        reason: String,
    },

    /// Storage backend error.
    #[error("memory storage error: {reason}")]
    #[diagnostic(help("{}", nika_error::codes::code_help(nika_error::codes::NIKA_604)))]
    Storage {
        /// Description of the storage failure.
        reason: String,
    },
}

impl nika_error::traits::NikaErrorCode for MemoryError {
    fn nika_code(&self) -> nika_error::codes::NikaCode {
        match self {
            Self::Unavailable { .. } => nika_error::codes::NIKA_601,
            Self::NotFound { .. } => nika_error::codes::NIKA_602,
            Self::EmbeddingFailed { .. } => nika_error::codes::NIKA_603,
            Self::Storage { .. } => nika_error::codes::NIKA_604,
        }
    }

    /// Whether retrying the same operation may succeed.
    ///
    /// Per Diamond W2.2 (plan §2 v1.1 amendment) :
    /// - `Unavailable` → false · config issue · not transient
    /// - `NotFound`    → false · deterministic miss
    /// - `EmbeddingFailed` → true · provider transient · retry-eligible
    /// - `Storage`     → true · IO / cache layer retriable
    fn is_transient(&self) -> bool {
        matches!(self, Self::EmbeddingFailed { .. } | Self::Storage { .. })
    }
}

// ─── Traits ──────────────────────────────────────────────────────────

/// Store a memory frame.
///
/// Sealed via [`crate::sealed::Sealed`] per ADR-078 (opt-in soft seal ·
/// L1 satellite impls add `impl crate::sealed::Sealed for MySatellite {}`).
#[trait_variant::make(MemoryRememberDyn: Send)]
pub trait MemoryRemember: Send + Sync + crate::sealed::Sealed {
    /// Store a memory frame and return its identifier.
    ///
    /// CANCEL SAFETY: cancel-safe if impl uses atomic insert (single DB
    /// transaction, single fs rename). A dropped future never leaves a
    /// half-committed frame that can be recalled. Impls using multi-step
    /// transactions MUST wrap them in rollback-on-drop guards.
    async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError>;
}

/// Recall memories by similarity.
///
/// Sealed via [`crate::sealed::Sealed`] per ADR-078 Option D · the L0.5
/// satellite contract keeps `&self + Send + Sync` (zero W3 cascade). The
/// L2 orchestrator pool (`nika-memory::RecallPool`, W10) owns `&mut self`
/// access internally — Restate stateful-vs-stateless split.
///
/// SYNC BOUND: explicit. Satellites SHOULD use `parking_lot::Mutex`,
/// `DashMap`, or `AtomicU64` for stats/cache state. Hot-path contention
/// is bounded by per-satellite sharding at the L2 pool boundary
/// (1M+ BGE-M3 cosine calls/sec is mitigated by fan-out, not by
/// flipping the trait to `&mut self` per Ryhl 2026 « Sync bound nobody
/// asked for »).
#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: Send + Sync + crate::sealed::Sealed {
    /// Search memories and return ranked results.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only similarity search.
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}

/// Forget a memory by its identifier.
///
/// Sealed via [`crate::sealed::Sealed`] per ADR-078.
#[trait_variant::make(MemoryForgetDyn: Send)]
pub trait MemoryForget: Send + Sync + crate::sealed::Sealed {
    /// Remove a memory from the store.
    ///
    /// CANCEL SAFETY: cancel-safe — idempotent delete. Retrying on a
    /// missing id is a no-op.
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}

// ─── Cortex lifecycle reservation (Wave 4A R5, ADR-033 amendment) ────
//
// Cognitive consolidation + retention pruning are the two background
// duties the v0.95 nika-memory orchestrator drives. Reserving them on
// the MemoryStore surface at v0.81 (with default no-op impls) avoids
// a breaking change every external Store impl would have to absorb
// when Cortex lands.

/// Scope selector for a consolidation pass.
///
/// Consolidation collapses short-term frames into long-term summaries
/// (episodic → semantic, procedural → reflective). The v0.95 orchestrator
/// passes a `ConsolidationScope` to tell the store which slice of the
/// keyspace to process (single tenant, single run, single level, or all).
///
/// Defaults to the full-sweep scope.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ConsolidationScope {
    /// Optional tenant filter. `None` = every tenant.
    pub tenant: Option<nika_error::id::TenantId>,
    /// Optional run/workflow filter. `None` = every run.
    pub run: Option<nika_error::id::RunId>,
    /// Optional level filter. `None` = every level.
    pub level: Option<MemoryLevel>,
}

/// Result of a consolidation pass.
///
/// The v0.95 impl returns how many frames were processed + how many new
/// summaries emerged. The v0.81 default impl returns an empty report so
/// existing `MemoryStore` impls keep passing without migration.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ConsolidationReport {
    /// Frames visited by this pass.
    pub frames_processed: u64,
    /// New consolidated summaries emitted.
    pub summaries_emitted: u64,
    /// Whether the pass completed fully (vs was truncated by budget).
    pub completed: bool,
}

impl ConsolidationReport {
    /// An empty, fully-completed report — used as the v0.81 default.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            frames_processed: 0,
            summaries_emitted: 0,
            completed: true,
        }
    }
}

/// Retention pruning policy.
///
/// Cortex pruning removes frames that fall below a retention threshold
/// (e.g., FSRS stability below X, age above Y, unaccessed for Z).
/// Defaults to "no-op" — impls override with their retention model.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PrunePolicy {
    /// Tenant scope — None = every tenant.
    pub tenant: Option<nika_error::id::TenantId>,
    /// Maximum age in seconds before a frame becomes prune-eligible.
    /// None = no age threshold.
    pub max_age_secs: Option<u64>,
    /// Minimum retention score (0.0-1.0) to KEEP. None = no threshold.
    pub min_retention: Option<f32>,
    /// Dry-run flag — compute what would be pruned without deleting.
    pub dry_run: bool,
}

/// Result of a prune pass.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PruneReport {
    /// Frames actually removed (0 in dry-run).
    pub frames_pruned: u64,
    /// Frames that matched the policy (equal to `frames_pruned` except in dry-run).
    pub frames_matched: u64,
    /// Whether the pass completed fully.
    pub completed: bool,
}

impl PruneReport {
    /// An empty, fully-completed report — used as the v0.81 default.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            frames_pruned: 0,
            frames_matched: 0,
            completed: true,
        }
    }
}

/// Cortex lifecycle operations — consolidation + prune.
///
/// Reserved at v0.81 as a STANDALONE trait (not a super-trait of
/// `MemoryStore`) with default no-op implementations. Impls opt in
/// per-Store when they're ready; v0.95 nika-memory overrides both
/// methods with FSRS + retention-threshold logic per ADR-004. ADR-033
/// amendment captures the full semantics.
///
/// Why standalone (not `MemoryStore` super-trait): keeping this trait
/// separate means external `MemoryStore` impls aren't forced to add an
/// `impl MemoryLifecycle` until they actually want the behaviour — no
/// migration burden for crates that only care about remember/recall/
/// forget. v0.95 Cortex will use `T: MemoryStore + MemoryLifecycle` as
/// its bound, making the opt-in explicit at the consumer boundary.
pub trait MemoryLifecycle: Send + Sync {
    /// Run a consolidation pass on the given scope.
    ///
    /// Default: no-op (returns an empty report). v0.95 overrides with
    /// FSRS consolidation against the frame graph.
    ///
    /// CANCEL SAFETY: cancel-safe — consolidation impls MUST checkpoint
    /// per-frame progress so a dropped future loses at most one frame's
    /// worth of work.
    fn consolidate(
        &self,
        _scope: ConsolidationScope,
    ) -> impl core::future::Future<Output = Result<ConsolidationReport, MemoryError>> + Send {
        async { Ok(ConsolidationReport::empty()) }
    }

    /// Run a prune pass under the given policy.
    ///
    /// Default: no-op (returns an empty report). v0.95 overrides with
    /// retention-threshold pruning + tombstoning.
    ///
    /// CANCEL SAFETY: cancel-safe — prune impls MUST use tombstone-then-
    /// garbage-collect so a dropped future leaves live frames untouched.
    fn prune(
        &self,
        _policy: PrunePolicy,
    ) -> impl core::future::Future<Output = Result<PruneReport, MemoryError>> + Send {
        async { Ok(PruneReport::empty()) }
    }
}

/// Full memory store — blanket super-trait (unchanged from v0.81.0).
pub trait MemoryStore: MemoryRemember + MemoryRecall + MemoryForget {}
impl<T: MemoryRemember + MemoryRecall + MemoryForget> MemoryStore for T {}

/// Embedding vector generation.
#[trait_variant::make(EmbeddingProviderDyn: Send)]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    ///
    /// CANCEL SAFETY: cancel-safe — embeddings are deterministic over
    /// (model, text) and produce no side effects. Drops abort the inference
    /// call without leaking state.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;

    /// Dimensionality of the embedding vectors.
    fn dimension(&self) -> usize;
}

/// Batch embed texts by calling [`EmbeddingProvider::embed`] sequentially.
///
/// Sequential fallback for providers that lack native batch embedding.
/// Override-capable providers (v0.95 hnsw, autodesc) should call their
/// backend's batch API directly instead.
///
/// # Errors
///
/// Returns [`MemoryError`] if any individual embed call fails.
///
/// CANCEL SAFETY: cancel-safe. Cancelling mid-batch abandons the remaining
/// per-element embeddings; already-completed results are dropped with the
/// future. Retrying from scratch is always safe (embeddings are pure).
pub async fn embed_batch_sequential(
    provider: &(impl EmbeddingProvider + ?Sized),
    texts: &[&str],
) -> Result<Vec<Vec<f32>>, MemoryError> {
    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        results.push(provider.embed(text).await?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // MemoryId, MemoryLevel, MemoryDirective, MemoryFrameRef tests
    // are in nika-error::memory::tests (Phase 0 descent).

    #[test]
    fn memory_frame_new_defaults() {
        let frame = MemoryFrame::new("test content", MemoryLevel::Working);
        assert_eq!(frame.content, "test content");
        assert_eq!(frame.level, MemoryLevel::Working);
        assert!(frame.tags.is_empty());
        assert!(frame.metadata.is_empty());
        assert!(frame.source.is_none());
        assert!(frame.observed_at.is_none());
    }

    #[test]
    fn memory_frame_new_cortex_reserved_fields_default_none() {
        let frame = MemoryFrame::new("test", MemoryLevel::Working);
        assert!(frame.cipher.is_none(), "cipher should default to None");
        assert!(
            frame.provenance.is_none(),
            "provenance should default to None"
        );
        assert!(
            frame.retention.is_none(),
            "retention should default to None"
        );
        assert!(
            frame.redactions.is_none(),
            "redactions should default to None"
        );
    }

    #[test]
    fn recall_query_new_defaults() {
        let query = RecallQuery::new("search term");
        assert_eq!(query.text, "search term");
        assert!(query.levels.is_none());
        assert!(query.tags.is_none());
        assert!(query.limit.is_none());
        assert!(query.min_score.is_none());
        assert!(query.observed_after.is_none());
        assert!(query.observed_before.is_none());
    }

    #[test]
    fn memory_hit_new() {
        let hit = MemoryHit::new(
            MemoryId::new(Uuid::from_u128(1)),
            "content",
            MemoryLevel::Semantic,
            0.95,
        );
        assert_eq!(hit.score, 0.95);
        assert_eq!(hit.level, MemoryLevel::Semantic);
        assert!(hit.observed_at.is_none());
        assert!(hit.source.is_none());
    }

    #[test]
    fn memory_error_display() {
        let err = MemoryError::Unavailable {
            reason: "not configured".into(),
        };
        assert!(err.to_string().contains("unavailable"));

        let err = MemoryError::NotFound {
            id: MemoryId::new(Uuid::from_u128(5)),
        };
        assert!(err.to_string().contains("not found"));
    }

    /// Test-only embedding provider returning zero vectors.
    struct ZeroEmbedder;
    impl EmbeddingProvider for ZeroEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MemoryError> {
            Ok(vec![0.0; 3])
        }
        fn dimension(&self) -> usize {
            3
        }
    }

    #[tokio::test]
    async fn embed_batch_sequential_calls_embed_per_text() {
        let embedder = ZeroEmbedder;
        let results = embed_batch_sequential(&embedder, &["hello", "world"])
            .await
            .expect("embed_batch should succeed");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 3);
        assert_eq!(results[1].len(), 3);
    }

    #[tokio::test]
    async fn embed_batch_sequential_empty_returns_empty() {
        let embedder = ZeroEmbedder;
        let results = embed_batch_sequential(&embedder, &[])
            .await
            .expect("empty batch should succeed");
        assert!(results.is_empty());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn memory_types_send_sync() {
        _assert_send_sync::<MemoryId>();
        _assert_send_sync::<MemoryFrame>();
        _assert_send_sync::<RecallQuery>();
        _assert_send_sync::<MemoryHit>();
    }

    // ── R5 lifecycle reservation ─────────────────────────────────────

    #[test]
    fn consolidation_report_empty_is_completed() {
        let r = ConsolidationReport::empty();
        assert_eq!(r.frames_processed, 0);
        assert_eq!(r.summaries_emitted, 0);
        assert!(r.completed);
    }

    #[test]
    fn prune_report_empty_is_completed() {
        let r = PruneReport::empty();
        assert_eq!(r.frames_pruned, 0);
        assert_eq!(r.frames_matched, 0);
        assert!(r.completed);
    }

    #[test]
    fn consolidation_scope_default_has_full_sweep() {
        let s = ConsolidationScope::default();
        assert!(s.tenant.is_none());
        assert!(s.run.is_none());
        assert!(s.level.is_none());
    }

    #[test]
    fn prune_policy_default_is_no_op() {
        let p = PrunePolicy::default();
        assert!(p.tenant.is_none());
        assert!(p.max_age_secs.is_none());
        assert!(p.min_retention.is_none());
        assert!(!p.dry_run);
    }

    /// A minimal implementor that relies on the default consolidate/prune
    /// methods — proves the defaults compile + return empty reports.
    struct NullLifecycleStore;
    impl MemoryLifecycle for NullLifecycleStore {}

    #[tokio::test]
    async fn memory_lifecycle_default_consolidate_is_noop() {
        let s = NullLifecycleStore;
        let r = s
            .consolidate(ConsolidationScope::default())
            .await
            .expect("default consolidate should Ok");
        assert_eq!(r.frames_processed, 0);
        assert!(r.completed);
    }

    #[tokio::test]
    async fn memory_lifecycle_default_prune_is_noop() {
        let s = NullLifecycleStore;
        let r = s
            .prune(PrunePolicy::default())
            .await
            .expect("default prune should Ok");
        assert_eq!(r.frames_pruned, 0);
        assert!(r.completed);
    }

    #[test]
    fn lifecycle_types_send_sync() {
        _assert_send_sync::<ConsolidationScope>();
        _assert_send_sync::<ConsolidationReport>();
        _assert_send_sync::<PrunePolicy>();
        _assert_send_sync::<PruneReport>();
    }

    // ─── MemoryError → NIKA-60X cross-mapping (Diamond W2.2) ───────────

    mod error_code_mapping {
        use super::*;
        use nika_error::codes;
        use nika_error::traits::NikaErrorCode;

        #[test]
        fn unavailable_maps_to_nika_601() {
            let e = MemoryError::Unavailable {
                reason: "backend down".into(),
            };
            assert_eq!(e.nika_code(), codes::NIKA_601);
            assert_eq!(e.nika_code().category, codes::Category::Memory);
        }

        #[test]
        fn not_found_maps_to_nika_602() {
            let e = MemoryError::NotFound {
                id: MemoryId::nil(),
            };
            assert_eq!(e.nika_code(), codes::NIKA_602);
            assert_eq!(e.nika_code().category, codes::Category::Memory);
        }

        #[test]
        fn embedding_failed_maps_to_nika_603() {
            let e = MemoryError::EmbeddingFailed {
                reason: "rate limit".into(),
            };
            assert_eq!(e.nika_code(), codes::NIKA_603);
            assert_eq!(e.nika_code().category, codes::Category::Memory);
        }

        #[test]
        fn storage_maps_to_nika_604() {
            let e = MemoryError::Storage {
                reason: "disk full".into(),
            };
            assert_eq!(e.nika_code(), codes::NIKA_604);
            assert_eq!(e.nika_code().category, codes::Category::Memory);
        }

        #[test]
        fn transience_unavailable_and_not_found_are_non_transient() {
            let unavail = MemoryError::Unavailable { reason: "x".into() };
            let nf = MemoryError::NotFound {
                id: MemoryId::nil(),
            };
            assert!(
                !unavail.is_transient(),
                "unavailable must NOT be transient · config issue"
            );
            assert!(
                !nf.is_transient(),
                "not-found must NOT be transient · deterministic miss"
            );
        }

        #[test]
        fn transience_embedding_and_storage_are_transient() {
            let emb = MemoryError::EmbeddingFailed {
                reason: "timeout".into(),
            };
            let st = MemoryError::Storage {
                reason: "io".into(),
            };
            assert!(
                emb.is_transient(),
                "embedding failure IS transient · retry-eligible"
            );
            assert!(
                st.is_transient(),
                "storage IS transient · retry after backoff"
            );
        }

        #[test]
        fn fingerprint_differs_per_variant() {
            let a = MemoryError::Unavailable { reason: "x".into() }.fingerprint();
            let b = MemoryError::NotFound {
                id: MemoryId::nil(),
            }
            .fingerprint();
            let c = MemoryError::EmbeddingFailed { reason: "x".into() }.fingerprint();
            let d = MemoryError::Storage { reason: "x".into() }.fingerprint();
            // Default fingerprint hashes nika_code().num · 4 distinct nums → 4 distinct hashes
            // Pairwise inequality avoids HashSet dep per kernel disallowed-types policy.
            assert_ne!(a, b, "601 vs 602 must differ");
            assert_ne!(a, c, "601 vs 603 must differ");
            assert_ne!(a, d, "601 vs 604 must differ");
            assert_ne!(b, c, "602 vs 603 must differ");
            assert_ne!(b, d, "602 vs 604 must differ");
            assert_ne!(c, d, "603 vs 604 must differ");
        }

        #[test]
        fn miette_help_resolves_per_variant() {
            use miette::Diagnostic;
            for e in [
                MemoryError::Unavailable { reason: "x".into() },
                MemoryError::NotFound {
                    id: MemoryId::nil(),
                },
                MemoryError::EmbeddingFailed { reason: "x".into() },
                MemoryError::Storage { reason: "x".into() },
            ] {
                let help = e.help().map(|h| h.to_string());
                assert!(help.is_some(), "{e} must carry miette help");
                let txt = help.expect("checked above");
                assert!(!txt.is_empty(), "help must be non-empty for {e}");
                assert!(
                    !txt.contains("Unknown"),
                    "{e} must resolve to specific help"
                );
            }
        }
    }
}
