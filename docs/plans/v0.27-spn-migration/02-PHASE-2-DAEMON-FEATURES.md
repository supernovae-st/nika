# Phase 2: Daemon Features Migration

## Overview

**Goal**: Migrate all daemon features from spn to nika.
**Lines**: ~3,500
**Types**: 44
**Tests**: 21

---

## Architecture Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA DAEMON ARCHITECTURE                                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │                           NikaDaemon                                     │  ║
║  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │  ║
║  │  │MemoryStore  │  │ TraceStore  │  │JobScheduler │  │AgentManager │    │  ║
║  │  │ FxHashMap   │  │   NDJSON    │  │  JoinSet    │  │  depth_lim  │    │  ║
║  │  │ + persist   │  │ + persist   │  │ + Semaphore │  │ + concur    │    │  ║
║  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │  ║
║  │         │                │                │                │            │  ║
║  │         └────────────────┴────────────────┴────────────────┘            │  ║
║  │                                    │                                     │  ║
║  │  ┌─────────────────────────────────┴─────────────────────────────────┐  │  ║
║  │  │                    AutonomyOrchestrator                            │  │  ║
║  │  │  ├── Policy enforcement (AutonomyLevel → ApprovalLevel)           │  │  ║
║  │  │  ├── Decision logging                                              │  │  ║
║  │  │  └── HITL integration (approval workflows)                        │  │  ║
║  │  └───────────────────────────────────────────────────────────────────┘  │  ║
║  │                                    │                                     │  ║
║  │  ┌─────────────────────────────────┴─────────────────────────────────┐  │  ║
║  │  │                    SuggestionAnalyzer                              │  │  ║
║  │  │  ├── Context triggers (file patterns, keywords)                   │  │  ║
║  │  │  ├── Suggestion generation                                         │  │  ║
║  │  │  └── Priority ranking                                              │  │  ║
║  │  └───────────────────────────────────────────────────────────────────┘  │  ║
║  │                                    │                                     │  ║
║  │                           Unix Socket IPC                                │  ║
║  │                        (~/.nika/daemon.sock)                            │  ║
║  └──────────────────────────────────┬──────────────────────────────────────┘  ║
║                                     │                                          ║
║       ┌──────────────┬──────────────┼──────────────┬──────────────┐           ║
║       │              │              │              │              │           ║
║  ┌────▼────┐   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐        ║
║  │  nika   │   │  nika   │   │  nika   │   │   MCP   │   │  IDE    │        ║
║  │   TUI   │   │   CLI   │   │  chat   │   │ Servers │   │ Plugins │        ║
║  └─────────┘   └─────────┘   └─────────┘   └─────────┘   └─────────┘        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Module Breakdown

### 2.1 MemoryStore (`src/store/memory.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/memory.rs` (~440 lines)

```rust
use crate::core::{MemoryKey, MemoryNamespace};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory + persistent storage for agent context.
///
/// Thread-safe via RwLock, persists to JSON on mutation.
pub struct MemoryStore {
    /// In-memory cache
    data: Arc<RwLock<FxHashMap<MemoryKey, MemoryEntry>>>,
    /// Persistence path
    persist_path: PathBuf,
    /// Dirty flag for batch persistence
    dirty: Arc<RwLock<bool>>,
}

/// Single memory entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The value (JSON)
    pub value: serde_json::Value,
    /// When created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When last accessed
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    /// Time-to-live (optional)
    pub ttl: Option<std::time::Duration>,
    /// Access count
    pub access_count: u64,
}

impl MemoryStore {
    /// Create new store with persistence path
    pub fn new(persist_path: PathBuf) -> Self;

    /// Load from disk
    pub async fn load(&self) -> Result<(), NikaError>;

    /// Get a value
    pub async fn get(&self, key: &MemoryKey) -> Option<MemoryEntry>;

    /// Set a value
    pub async fn set(&self, key: MemoryKey, value: serde_json::Value) -> Result<(), NikaError>;

    /// Set with TTL
    pub async fn set_with_ttl(
        &self,
        key: MemoryKey,
        value: serde_json::Value,
        ttl: std::time::Duration,
    ) -> Result<(), NikaError>;

    /// Delete a value
    pub async fn delete(&self, key: &MemoryKey) -> Option<MemoryEntry>;

    /// List keys in namespace
    pub async fn list_namespace(&self, namespace: MemoryNamespace) -> Vec<MemoryKey>;

    /// Clear namespace
    pub async fn clear_namespace(&self, namespace: MemoryNamespace) -> usize;

    /// Expire old entries
    pub async fn expire_ttl(&self) -> usize;

    /// Persist to disk
    pub async fn persist(&self) -> Result<(), NikaError>;

    /// Get stats
    pub async fn stats(&self) -> MemoryStats;
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub entries_by_namespace: FxHashMap<MemoryNamespace, usize>,
    pub total_size_bytes: usize,
}
```

**TDD Tests**:
```rust
#[tokio::test]
async fn test_memory_store_crud() {
    let store = MemoryStore::new(temp_path());
    let key = MemoryKey::preference("theme");

    // Create
    store.set(key.clone(), json!("dark")).await.unwrap();

    // Read
    let entry = store.get(&key).await.unwrap();
    assert_eq!(entry.value, json!("dark"));

    // Update
    store.set(key.clone(), json!("light")).await.unwrap();
    let entry = store.get(&key).await.unwrap();
    assert_eq!(entry.value, json!("light"));

    // Delete
    let deleted = store.delete(&key).await;
    assert!(deleted.is_some());
    assert!(store.get(&key).await.is_none());
}

#[tokio::test]
async fn test_memory_ttl_expiration() {
    let store = MemoryStore::new(temp_path());
    let key = MemoryKey::new(MemoryNamespace::Analytics, "session");

    store.set_with_ttl(key.clone(), json!({}), Duration::from_millis(10)).await.unwrap();
    assert!(store.get(&key).await.is_some());

    tokio::time::sleep(Duration::from_millis(15)).await;
    let expired = store.expire_ttl().await;
    assert_eq!(expired, 1);
    assert!(store.get(&key).await.is_none());
}

#[tokio::test]
async fn test_memory_persistence() {
    let path = temp_path();

    // Create and persist
    {
        let store = MemoryStore::new(path.clone());
        store.set(MemoryKey::preference("key"), json!("value")).await.unwrap();
        store.persist().await.unwrap();
    }

    // Reload
    {
        let store = MemoryStore::new(path);
        store.load().await.unwrap();
        let entry = store.get(&MemoryKey::preference("key")).await.unwrap();
        assert_eq!(entry.value, json!("value"));
    }
}
```

---

### 2.2 TraceStore (`src/store/trace_store.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/traces.rs` (~479 lines)

```rust
use crate::core::{TraceId, TraceMetadata, TraceStepKind};
use std::path::PathBuf;

/// Storage for LLM reasoning traces.
///
/// Persists as NDJSON for streaming writes.
pub struct TraceStore {
    /// Directory for trace files
    trace_dir: PathBuf,
    /// In-memory index of trace metadata
    index: Arc<RwLock<FxHashMap<TraceId, TraceMetadata>>>,
}

/// Single step in a reasoning trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    /// Step index
    pub index: usize,
    /// Kind of step
    pub kind: TraceStepKind,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Content
    pub content: String,
    /// Token count (if applicable)
    pub tokens: Option<u64>,
    /// Duration (if applicable)
    pub duration_ms: Option<u64>,
    /// Associated tool call (if ToolCall/ToolResult)
    pub tool_name: Option<String>,
}

/// Complete reasoning trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    /// Metadata
    pub metadata: TraceMetadata,
    /// Steps in order
    pub steps: Vec<TraceStep>,
}

impl TraceStore {
    /// Create new store
    pub fn new(trace_dir: PathBuf) -> Self;

    /// Start a new trace
    pub async fn start_trace(&self, workflow_id: Option<String>, task_id: Option<String>) -> TraceId;

    /// Add step to trace
    pub async fn add_step(&self, trace_id: &TraceId, step: TraceStep) -> Result<(), NikaError>;

    /// End trace
    pub async fn end_trace(&self, trace_id: &TraceId) -> Result<(), NikaError>;

    /// Get trace by ID
    pub async fn get_trace(&self, trace_id: &TraceId) -> Option<ReasoningTrace>;

    /// List recent traces
    pub async fn list_recent(&self, limit: usize) -> Vec<TraceMetadata>;

    /// Delete trace
    pub async fn delete_trace(&self, trace_id: &TraceId) -> Result<(), NikaError>;

    /// Cleanup old traces
    pub async fn cleanup_older_than(&self, days: u32) -> usize;
}
```

**TDD Tests**:
```rust
#[tokio::test]
async fn test_trace_lifecycle() {
    let store = TraceStore::new(temp_dir());

    // Start
    let trace_id = store.start_trace(Some("wf-1".into()), Some("task-1".into())).await;

    // Add steps
    store.add_step(&trace_id, TraceStep {
        index: 0,
        kind: TraceStepKind::Observation,
        content: "Analyzing input...".into(),
        ..Default::default()
    }).await.unwrap();

    store.add_step(&trace_id, TraceStep {
        index: 1,
        kind: TraceStepKind::Reasoning,
        content: "Deciding on approach...".into(),
        ..Default::default()
    }).await.unwrap();

    // End
    store.end_trace(&trace_id).await.unwrap();

    // Retrieve
    let trace = store.get_trace(&trace_id).await.unwrap();
    assert_eq!(trace.steps.len(), 2);
    assert!(trace.metadata.ended_at.is_some());
}

#[tokio::test]
async fn test_trace_ndjson_format() {
    let store = TraceStore::new(temp_dir());
    let trace_id = store.start_trace(None, None).await;

    // Verify NDJSON file exists
    let trace_path = store.trace_path(&trace_id);
    assert!(trace_path.exists());

    // Read raw file, verify NDJSON format
    let content = std::fs::read_to_string(&trace_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    // First line should be parseable as JSON
    let _: TraceMetadata = serde_json::from_str(lines[0]).unwrap();
}
```

---

### 2.3 JobScheduler (`src/jobs/scheduler.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/jobs.rs` (~500 lines)

```rust
use tokio::task::JoinSet;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

/// Background job scheduler with concurrency control.
///
/// Uses tokio JoinSet + Semaphore for managed concurrency.
pub struct JobScheduler {
    /// Active jobs
    jobs: Arc<RwLock<FxHashMap<JobId, JobHandle>>>,
    /// Concurrency limiter
    semaphore: Arc<Semaphore>,
    /// Cancellation token for graceful shutdown
    cancel_token: CancellationToken,
    /// Job store (SQLite)
    store: Arc<JobStore>,
    /// Max concurrent jobs
    max_concurrent: usize,
}

/// Job identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

/// Job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub workflow_path: PathBuf,
    pub inputs: serde_json::Value,
    pub priority: u8,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: JobStatus,
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Job state with output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobState {
    pub job: Job,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub logs: Vec<String>,
}

impl JobScheduler {
    /// Create new scheduler
    pub fn new(store: Arc<JobStore>, max_concurrent: usize) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(FxHashMap::default())),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            cancel_token: CancellationToken::new(),
            store,
            max_concurrent,
        }
    }

    /// Submit a job
    pub async fn submit(&self, job: Job) -> Result<JobId, NikaError>;

    /// Cancel a job
    pub async fn cancel(&self, job_id: &JobId) -> Result<(), NikaError>;

    /// Get job state
    pub async fn get_state(&self, job_id: &JobId) -> Option<JobState>;

    /// List jobs by status
    pub async fn list_jobs(&self, status: Option<JobStatus>) -> Vec<JobState>;

    /// Start the scheduler (background task)
    pub fn start(&self) -> JoinHandle<()>;

    /// Graceful shutdown
    pub async fn shutdown(&self);

    /// Wait for job completion
    pub async fn wait_for(&self, job_id: &JobId) -> JobState;
}
```

**Tokio Patterns (from Context7 research)**:

```rust
impl JobScheduler {
    pub fn start(&self) -> JoinHandle<()> {
        let jobs = self.jobs.clone();
        let semaphore = self.semaphore.clone();
        let cancel = self.cancel_token.clone();
        let store = self.store.clone();

        tokio::spawn(async move {
            let mut join_set = JoinSet::new();

            loop {
                tokio::select! {
                    // Check cancellation
                    _ = cancel.cancelled() => {
                        // Drain remaining jobs
                        while let Some(result) = join_set.join_next().await {
                            handle_job_result(result, &store).await;
                        }
                        break;
                    }

                    // Process next queued job
                    Some(job) = get_next_queued(&store) => {
                        // Acquire semaphore permit
                        let permit = semaphore.clone().acquire_owned().await.unwrap();

                        // Spawn job
                        join_set.spawn(async move {
                            let result = run_job(job).await;
                            drop(permit); // Release permit
                            result
                        });
                    }

                    // Collect completed jobs
                    Some(result) = join_set.join_next() => {
                        handle_job_result(result, &store).await;
                    }
                }
            }
        })
    }
}
```

**TDD Tests**:
```rust
#[tokio::test]
async fn test_job_scheduling() {
    let store = Arc::new(JobStore::in_memory());
    let scheduler = JobScheduler::new(store.clone(), 2);

    let job = Job {
        id: JobId("job-1".into()),
        workflow_path: "test.nika.yaml".into(),
        inputs: json!({}),
        priority: 5,
        created_at: Utc::now(),
        status: JobStatus::Queued,
    };

    let job_id = scheduler.submit(job).await.unwrap();

    // Should be queued
    let state = scheduler.get_state(&job_id).await.unwrap();
    assert_eq!(state.job.status, JobStatus::Queued);
}

#[tokio::test]
async fn test_concurrency_limit() {
    let store = Arc::new(JobStore::in_memory());
    let scheduler = JobScheduler::new(store, 2); // Max 2 concurrent

    // Submit 5 jobs
    for i in 0..5 {
        scheduler.submit(slow_job(i)).await.unwrap();
    }

    // Only 2 should be running
    let running = scheduler.list_jobs(Some(JobStatus::Running)).await;
    assert!(running.len() <= 2);
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let store = Arc::new(JobStore::in_memory());
    let scheduler = JobScheduler::new(store, 4);

    let handle = scheduler.start();

    // Submit some jobs
    for i in 0..3 {
        scheduler.submit(quick_job(i)).await.unwrap();
    }

    // Graceful shutdown
    scheduler.shutdown().await;
    handle.await.unwrap();

    // All jobs should be complete or cancelled
    let queued = scheduler.list_jobs(Some(JobStatus::Queued)).await;
    assert!(queued.is_empty());
}
```

---

### 2.4 AgentManager (`src/runtime/agent_manager.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/agents.rs` (~648 lines)

```rust
use crate::core::{AgentId, AgentRole, AgentState, AgentStatus, DelegatedTask};

/// Multi-agent coordinator with depth limiting.
pub struct AgentManager {
    /// Active agents
    agents: Arc<RwLock<FxHashMap<AgentId, AgentHandle>>>,
    /// Max concurrent agents
    max_concurrent: usize,
    /// Max depth (prevents infinite recursion)
    max_depth: usize,
    /// Semaphore for concurrency
    semaphore: Arc<Semaphore>,
}

/// Agent handle for management
struct AgentHandle {
    pub status: AgentStatus,
    pub cancel: CancellationToken,
    pub join: JoinHandle<Result<serde_json::Value, NikaError>>,
}

impl AgentManager {
    /// Create new manager
    pub fn new(max_concurrent: usize, max_depth: usize) -> Self;

    /// Spawn an agent
    pub async fn spawn(
        &self,
        prompt: String,
        role: AgentRole,
        parent_id: Option<AgentId>,
        mcp_clients: Vec<Arc<McpClient>>,
    ) -> Result<AgentId, NikaError>;

    /// Get agent status
    pub async fn get_status(&self, id: &AgentId) -> Option<AgentStatus>;

    /// List all agents
    pub async fn list_agents(&self) -> Vec<AgentStatus>;

    /// Cancel agent
    pub async fn cancel(&self, id: &AgentId) -> Result<(), NikaError>;

    /// Wait for agent completion
    pub async fn wait_for(&self, id: &AgentId) -> Result<serde_json::Value, NikaError>;

    /// Delegate task to agent
    pub async fn delegate(&self, task: DelegatedTask) -> Result<AgentId, NikaError>;

    /// Get depth of agent
    fn get_depth(&self, id: &AgentId) -> usize;

    /// Check if spawn allowed (depth limit)
    fn can_spawn(&self, parent_id: Option<&AgentId>) -> bool;
}
```

**Depth Limiting Logic**:
```rust
impl AgentManager {
    fn get_depth(&self, agents: &FxHashMap<AgentId, AgentHandle>, id: &AgentId) -> usize {
        let handle = match agents.get(id) {
            Some(h) => h,
            None => return 0,
        };

        match &handle.status.parent_id {
            Some(parent) => 1 + self.get_depth(agents, parent),
            None => 0,
        }
    }

    fn can_spawn(&self, parent_id: Option<&AgentId>) -> bool {
        let agents = self.agents.blocking_read();

        match parent_id {
            None => true, // Root agent always allowed
            Some(pid) => {
                let parent_depth = self.get_depth(&agents, pid);
                parent_depth < self.max_depth
            }
        }
    }
}
```

**TDD Tests**:
```rust
#[tokio::test]
async fn test_agent_spawn_and_complete() {
    let manager = AgentManager::new(10, 3);

    let agent_id = manager.spawn(
        "Test task".into(),
        AgentRole::Worker,
        None,
        vec![],
    ).await.unwrap();

    let status = manager.get_status(&agent_id).await.unwrap();
    assert_eq!(status.role, AgentRole::Worker);
    assert_eq!(status.depth, 0);
}

#[tokio::test]
async fn test_depth_limiting() {
    let manager = AgentManager::new(10, 2); // Max depth 2

    // Root agent (depth 0)
    let root = manager.spawn("Root".into(), AgentRole::Supervisor, None, vec![]).await.unwrap();

    // Child agent (depth 1)
    let child = manager.spawn("Child".into(), AgentRole::Worker, Some(root.clone()), vec![]).await.unwrap();

    // Grandchild agent (depth 2) - should succeed
    let grandchild = manager.spawn("Grandchild".into(), AgentRole::Worker, Some(child.clone()), vec![]).await.unwrap();

    // Great-grandchild (depth 3) - should fail
    let result = manager.spawn("Too deep".into(), AgentRole::Worker, Some(grandchild), vec![]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrency_limit() {
    let manager = AgentManager::new(3, 10); // Max 3 concurrent

    // Spawn 5 agents
    let mut ids = vec![];
    for i in 0..5 {
        ids.push(manager.spawn(format!("Agent {}", i), AgentRole::Worker, None, vec![]).await.unwrap());
    }

    // Only 3 should be working
    let working: Vec<_> = manager.list_agents().await
        .into_iter()
        .filter(|s| s.state == AgentState::Working)
        .collect();
    assert!(working.len() <= 3);
}
```

---

### 2.5 AutonomyOrchestrator (`src/runtime/autonomy_orchestrator.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/autonomy.rs` (~1017 lines)

```rust
use crate::core::{AutonomyLevel, ApprovalLevel, Decision, DecisionOutcome, RiskLevel};

/// Human-in-the-loop orchestrator with policy enforcement.
pub struct AutonomyOrchestrator {
    /// Current autonomy level
    level: Arc<RwLock<AutonomyLevel>>,
    /// Policy rules
    policy: AutonomyPolicy,
    /// Decision history
    decisions: Arc<RwLock<Vec<Decision>>>,
    /// Pending approvals
    pending: Arc<RwLock<FxHashMap<String, PendingApproval>>>,
    /// State
    state: Arc<RwLock<OrchestratorState>>,
    /// Stats
    stats: Arc<RwLock<OrchestratorStats>>,
}

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyPolicy {
    /// Actions that always require approval
    pub always_approve: Vec<String>,
    /// Actions that never require approval
    pub never_approve: Vec<String>,
    /// Risk mappings
    pub risk_map: FxHashMap<String, RiskLevel>,
    /// Timeout for approvals
    pub approval_timeout: Duration,
}

/// Orchestrator state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorState {
    Idle,
    Processing,
    WaitingForApproval,
    Executing,
}

/// Pending approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub action: String,
    pub risk: RiskLevel,
    pub context: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Stats for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorStats {
    pub decisions_total: u64,
    pub auto_approved: u64,
    pub user_approved: u64,
    pub denied: u64,
    pub timed_out: u64,
}

impl AutonomyOrchestrator {
    /// Create new orchestrator
    pub fn new(level: AutonomyLevel, policy: AutonomyPolicy) -> Self;

    /// Request approval for an action
    pub async fn request_approval(
        &self,
        action: String,
        risk: RiskLevel,
        context: serde_json::Value,
    ) -> Result<Decision, NikaError>;

    /// Process approval (from user)
    pub async fn process_approval(
        &self,
        id: &str,
        approved: bool,
        reasoning: Option<String>,
    ) -> Result<(), NikaError>;

    /// Get pending approvals
    pub async fn get_pending(&self) -> Vec<PendingApproval>;

    /// Set autonomy level
    pub async fn set_level(&self, level: AutonomyLevel);

    /// Get current level
    pub async fn get_level(&self) -> AutonomyLevel;

    /// Get stats
    pub async fn get_stats(&self) -> OrchestratorStats;

    /// Internal: determine approval requirement
    fn determine_approval(&self, action: &str, risk: RiskLevel) -> ApprovalLevel;
}
```

**Approval Flow**:
```rust
impl AutonomyOrchestrator {
    pub async fn request_approval(
        &self,
        action: String,
        risk: RiskLevel,
        context: serde_json::Value,
    ) -> Result<Decision, NikaError> {
        let level = *self.level.read().await;
        let approval_needed = self.determine_approval(&action, risk);

        match approval_needed {
            ApprovalLevel::None => {
                // Auto-approve
                let decision = Decision {
                    id: generate_id(),
                    action,
                    outcome: DecisionOutcome::Approved,
                    timestamp: Utc::now(),
                    reasoning: Some("Auto-approved per policy".into()),
                };
                self.record_decision(decision.clone()).await;
                Ok(decision)
            }

            ApprovalLevel::Silent => {
                // Log but don't prompt
                let decision = Decision {
                    id: generate_id(),
                    action,
                    outcome: DecisionOutcome::Approved,
                    timestamp: Utc::now(),
                    reasoning: Some("Silent approval".into()),
                };
                self.record_decision(decision.clone()).await;
                Ok(decision)
            }

            ApprovalLevel::Explicit | ApprovalLevel::MultiStep => {
                // Wait for user approval
                let pending = PendingApproval {
                    id: generate_id(),
                    action: action.clone(),
                    risk,
                    context,
                    created_at: Utc::now(),
                    expires_at: Utc::now() + self.policy.approval_timeout,
                };

                self.pending.write().await.insert(pending.id.clone(), pending.clone());

                // Wait with timeout
                let result = self.wait_for_approval(&pending.id).await;

                match result {
                    Ok(decision) => Ok(decision),
                    Err(_) => {
                        // Timed out
                        let decision = Decision {
                            id: pending.id,
                            action,
                            outcome: DecisionOutcome::TimedOut,
                            timestamp: Utc::now(),
                            reasoning: Some("Approval timed out".into()),
                        };
                        self.record_decision(decision.clone()).await;
                        Ok(decision)
                    }
                }
            }
        }
    }
}
```

---

### 2.6 SuggestionAnalyzer (`src/runtime/suggestion_analyzer.rs`)

**Source**: `supernovae-cli/crates/spn/src/daemon/proactive.rs` (~907 lines)

```rust
/// Proactive suggestion system based on context analysis.
pub struct SuggestionAnalyzer {
    /// Registered triggers
    triggers: Arc<RwLock<Vec<ContextTrigger>>>,
    /// Active suggestions
    suggestions: Arc<RwLock<Vec<ProactiveSuggestion>>>,
    /// Configuration
    config: SuggestionConfig,
}

/// Context trigger definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTrigger {
    pub id: String,
    pub condition: TriggerCondition,
    pub suggestion_template: String,
    pub category: SuggestionCategory,
    pub priority: SuggestionPriority,
}

/// Trigger condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerCondition {
    /// File pattern match
    FilePattern { pattern: String },
    /// Keyword in content
    Keyword { keywords: Vec<String> },
    /// Error pattern
    ErrorPattern { pattern: String },
    /// Time-based
    Schedule { cron: String },
    /// Combined conditions
    All { conditions: Vec<TriggerCondition> },
    Any { conditions: Vec<TriggerCondition> },
}

/// Suggestion category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    CodeQuality,
    Performance,
    Security,
    Documentation,
    Testing,
    Refactoring,
}

/// Suggestion priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// Generated suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveSuggestion {
    pub id: SuggestionId,
    pub trigger_id: String,
    pub category: SuggestionCategory,
    pub priority: SuggestionPriority,
    pub title: String,
    pub description: String,
    pub actions: Vec<SuggestedAction>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub source: SuggestionSource,
}

impl SuggestionAnalyzer {
    /// Create new analyzer
    pub fn new(config: SuggestionConfig) -> Self;

    /// Register a trigger
    pub async fn register_trigger(&self, trigger: ContextTrigger);

    /// Analyze context and generate suggestions
    pub async fn analyze(&self, context: &AnalysisContext) -> Vec<ProactiveSuggestion>;

    /// Get active suggestions
    pub async fn get_suggestions(&self) -> Vec<ProactiveSuggestion>;

    /// Dismiss suggestion
    pub async fn dismiss(&self, id: &SuggestionId);

    /// Act on suggestion
    pub async fn act(&self, id: &SuggestionId) -> Result<(), NikaError>;
}
```

---

## File Structure (Final)

```
src/
├── core/
│   ├── autonomy.rs        # Phase 1
│   ├── agent_types.rs     # Phase 1
│   ├── memory_types.rs    # Phase 1
│   └── trace_types.rs     # Phase 1
├── store/
│   ├── mod.rs
│   ├── data.rs            # ✅ Existing DataStore
│   ├── memory.rs          # 🆕 MemoryStore
│   └── trace_store.rs     # 🆕 TraceStore
├── runtime/
│   ├── agent_manager.rs   # 🆕 AgentManager
│   ├── autonomy_orchestrator.rs # 🆕 AutonomyOrchestrator
│   └── suggestion_analyzer.rs   # 🆕 SuggestionAnalyzer
├── jobs/
│   ├── mod.rs             # 🆕 Re-exports
│   ├── scheduler.rs       # 🆕 JobScheduler
│   └── store.rs           # 🆕 JobStore (SQLite)
└── daemon/
    ├── mod.rs             # 🆕 Daemon entry
    └── server.rs          # 🆕 Unix socket server
```

---

## Dependencies

Add to Cargo.toml:

```toml
[dependencies]
# Already have
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }

# New
tokio-util = { version = "0.7", features = ["rt"] }  # CancellationToken
rustc-hash = "2"  # FxHashMap
rusqlite = { version = "0.32", features = ["bundled"] }  # JobStore
```

---

## Estimated Effort

| Module | Lines | Hours |
|--------|-------|-------|
| MemoryStore | 440 | 4 |
| TraceStore | 479 | 4 |
| JobScheduler | 500 | 6 |
| AgentManager | 648 | 5 |
| AutonomyOrchestrator | 1017 | 8 |
| SuggestionAnalyzer | 907 | 6 |
| Tests | - | 8 |
| Integration | - | 4 |
| **Total** | **~3,991** | **~45 hours** |

---

## Validation Checklist

- [ ] All tests pass
- [ ] Zero clippy warnings
- [ ] Memory safety (no data races)
- [ ] Graceful shutdown works
- [ ] Persistence survives restart
- [ ] Depth limiting prevents infinite recursion
- [ ] Concurrency limits enforced
- [ ] HITL approval flow works
- [ ] Documentation complete
