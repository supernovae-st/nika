//! Media tool execution context.
//!
//! Provides `MediaToolContext` (shared per workflow run) with:
//! - CAS store access
//! - Media budget enforcement
//! - `ComputePool` (rayon, isolated from tokio)
//! - `WorkingMemoryBudget` (transient buffer limits)
//! - Cancellation support

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::error::tool_error;
use super::error::MediaToolError;
use crate::{CasStore, MediaBudget};

/// Shared context for all media tool operations within a workflow run.
pub struct MediaToolContext {
    /// CAS store for reading/writing binary media.
    pub cas: CasStore,
    /// Per-run budget enforcement (500 MB default).
    pub budget: Arc<MediaBudget>,
    /// CPU-bound compute pool (rayon, 4 threads, isolated from tokio).
    pub compute: Arc<ComputePool>,
    /// Transient working memory budget (512 MB default).
    pub working_memory: Arc<WorkingMemoryBudget>,
    /// Workflow cancellation token.
    pub cancel: CancellationToken,
    /// Project working directory for path confinement (import, read).
    /// When set, file-based tools reject paths outside this directory.
    pub working_dir: Option<std::path::PathBuf>,
}

impl MediaToolContext {
    /// Create a new context with the given CAS store.
    ///
    /// Returns an error if the compute pool cannot be created.
    pub fn new(cas: CasStore) -> Result<Self, MediaToolError> {
        Ok(Self {
            cas,
            budget: Arc::new(MediaBudget::new()),
            compute: Arc::new(ComputePool::new()?),
            working_memory: Arc::new(WorkingMemoryBudget::new()),
            cancel: CancellationToken::new(),
            working_dir: None,
        })
    }

    /// Set the project working directory for path confinement.
    ///
    /// When set, `nika:import` rejects paths that resolve outside this directory.
    pub fn with_working_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Read media data from CAS by hash.
    pub async fn read_media(&self, hash: &str) -> Result<Vec<u8>, MediaToolError> {
        self.cas.read(hash).await.map_err(|e| e.into())
    }

    /// Store binary data in CAS and charge the budget.
    pub async fn store_media(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<crate::store::StoreResult, MediaToolError> {
        // Charge budget first
        self.budget
            .check_and_add(data.len() as u64, task_id)
            .map_err(|e| -> MediaToolError { e.into() })?;

        // Store in CAS
        match self.cas.store(data).await {
            Ok(result) => Ok(result),
            Err(e) => {
                // Rollback budget on CAS failure
                self.budget.rollback(data.len() as u64);
                Err(e.into())
            }
        }
    }

    /// Check if the workflow has been cancelled.
    pub fn check_cancelled(&self) -> Result<(), MediaToolError> {
        if self.cancel.is_cancelled() {
            Err(tool_error("media", "workflow cancelled"))
        } else {
            Ok(())
        }
    }
}

/// Dedicated rayon ThreadPool for CPU-bound media operations.
///
/// Isolates media compute (resize, encode, optimize) from the tokio runtime.
/// Uses `min(num_cpus, 4)` threads named `nika-media-N`.
///
/// Communication: rayon thread → tokio via oneshot channel.
pub struct ComputePool {
    pool: rayon::ThreadPool,
}

impl ComputePool {
    /// Create a new compute pool.
    pub fn new() -> Result<Self, MediaToolError> {
        Ok(Self {
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(num_cpus().min(4))
                .thread_name(|idx| format!("nika-media-{idx}"))
                .panic_handler(|info| {
                    // Log the panic for debugging, then absorb — the oneshot channel
                    // receiver gets RecvError which is mapped to a MediaToolError.
                    tracing::error!("media compute thread panicked: {info:?}");
                })
                .build()
                .map_err(|e| {
                    tool_error(
                        "compute_pool",
                        format!("Failed to create media compute pool: {e}"),
                    )
                })?,
        })
    }

    /// Execute a CPU-bound closure on the rayon pool.
    ///
    /// Bridges rayon → tokio via a oneshot channel.
    /// If the closure panics, returns an error instead of crashing.
    pub async fn compute<F, T>(&self, f: F) -> Result<T, MediaToolError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pool.spawn(move || {
            let _ = tx.send(f());
        });
        rx.await
            .map_err(|_| tool_error("compute", "task panicked on rayon thread"))
    }
}

/// Get the number of available CPUs.
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}

/// Budget for transient working memory (in-flight decode buffers, etc.).
///
/// Prevents OOM from multiple concurrent image decodes.
/// Default: 512 MB. Uses atomic CAS for lock-free concurrent access.
#[derive(Debug)]
pub struct WorkingMemoryBudget {
    used: AtomicUsize,
    max_bytes: usize,
}

impl WorkingMemoryBudget {
    /// Default transient memory budget: 512 MB.
    pub const DEFAULT_MAX: usize = 512 * 1024 * 1024;

    /// Create with default budget.
    pub fn new() -> Self {
        Self {
            used: AtomicUsize::new(0),
            max_bytes: Self::DEFAULT_MAX,
        }
    }

    /// Create with custom budget.
    pub fn with_max(max_bytes: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            max_bytes,
        }
    }

    /// Try to acquire `size` bytes. Returns a guard that releases on drop.
    pub fn acquire(&self, size: usize) -> Result<WorkingMemoryGuard<'_>, MediaToolError> {
        let result = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                let new_total = current + size;
                if new_total > self.max_bytes {
                    None
                } else {
                    Some(new_total)
                }
            });

        match result {
            Ok(_) => Ok(WorkingMemoryGuard { budget: self, size }),
            Err(current) => Err(tool_error(
                "memory",
                format!(
                    "working memory exhausted ({} + {} > {} limit)",
                    current, size, self.max_bytes
                ),
            )),
        }
    }

    /// Current bytes in use.
    pub fn current(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

impl Default for WorkingMemoryBudget {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that releases working memory on drop.
#[derive(Debug)]
pub struct WorkingMemoryGuard<'a> {
    budget: &'a WorkingMemoryBudget,
    size: usize,
}

impl<'a> Drop for WorkingMemoryGuard<'a> {
    fn drop(&mut self) {
        let _ = self
            .budget
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_sub(self.size))
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════
    // COMPUTE POOL TESTS
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn compute_pool_executes_on_rayon_thread() {
        let pool = ComputePool::new().unwrap();
        let thread_name = pool
            .compute(|| {
                std::thread::current()
                    .name()
                    .unwrap_or("unknown")
                    .to_string()
            })
            .await
            .unwrap();
        assert!(
            thread_name.starts_with("nika-media"),
            "expected nika-media thread, got: {thread_name}"
        );
    }

    #[tokio::test]
    async fn compute_pool_returns_result() {
        let pool = ComputePool::new().unwrap();
        let result = pool.compute(|| 2 + 2).await.unwrap();
        assert_eq!(result, 4);
    }

    #[tokio::test]
    async fn compute_pool_handles_panic() {
        let pool = ComputePool::new().unwrap();
        let result: Result<i32, _> = pool
            .compute(|| {
                panic!("intentional test panic");
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("panicked"));
    }

    // ═══════════════════════════════════════════
    // WORKING MEMORY BUDGET TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn working_memory_acquire_release() {
        let budget = WorkingMemoryBudget::with_max(1024);
        assert_eq!(budget.current(), 0);

        {
            let _guard = budget.acquire(100).unwrap();
            assert_eq!(budget.current(), 100);
        }
        // Guard dropped, memory released
        assert_eq!(budget.current(), 0);
    }

    #[test]
    fn working_memory_blocks_when_full() {
        let budget = WorkingMemoryBudget::with_max(512);

        let _guard = budget.acquire(512).unwrap();
        assert_eq!(budget.current(), 512);

        // Second acquire should fail
        let result = budget.acquire(1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("working memory exhausted"));
    }

    #[test]
    fn working_memory_multiple_guards() {
        let budget = WorkingMemoryBudget::with_max(300);

        let g1 = budget.acquire(100).unwrap();
        let g2 = budget.acquire(100).unwrap();
        assert_eq!(budget.current(), 200);

        drop(g1);
        assert_eq!(budget.current(), 100);

        drop(g2);
        assert_eq!(budget.current(), 0);
    }

    // ═══════════════════════════════════════════
    // MEDIA TOOL CONTEXT TESTS
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn context_check_cancelled_ok() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = MediaToolContext::new(CasStore::new(dir.path())).unwrap();
        assert!(ctx.check_cancelled().is_ok());
    }

    #[tokio::test]
    async fn context_check_cancelled_err() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = MediaToolContext::new(CasStore::new(dir.path())).unwrap();
        ctx.cancel.cancel();
        assert!(ctx.check_cancelled().is_err());
    }

    #[tokio::test]
    async fn context_store_charges_budget() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = MediaToolContext::new(CasStore::new(dir.path())).unwrap();
        let data = b"test media data";
        let result = ctx.store_media(data, "test_task").await;
        assert!(result.is_ok());
        assert_eq!(ctx.budget.current_bytes(), data.len() as u64);
    }

    #[tokio::test]
    async fn context_read_missing_hash() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = MediaToolContext::new(CasStore::new(dir.path())).unwrap();
        let result = ctx
            .read_media("blake3:0000000000000000000000000000000000000000000000000000000000000000")
            .await;
        assert!(result.is_err());
    }
}
