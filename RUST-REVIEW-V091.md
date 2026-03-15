# Nika v0.9.1 — Rust Code Review & Best Practices Analysis

**Date:** 2026-02-24
**Reviewer:** Claude (Haiku 4.5 — Rust-Pro Agent)
**Focus:** StableGraph migration, ChatWorkflow design, async patterns, error handling, binding system
**Status:** Pre-implementation review

---

## Executive Summary

The v0.9.1 implementation plan contains **solid architecture with 3 critical issues** that must be addressed before coding:

| Issue | Severity | Category | Impact |
|-------|----------|----------|--------|
| StableGraph with Arc<str> keys | **MEDIUM** | Ownership | Inefficient cloning, memory waste |
| ChatWorkflow struct design | **HIGH** | Lifetimes | Missing Arc/Rc for shared ownership |
| MentionParser binding system | **MEDIUM** | Design | Unclear reference resolution semantics |
| Error handling inconsistency | **LOW** | Patterns | Some functions miss NikaError context |

This review provides **specific code improvements** before you start implementation.

---

## 1. StableGraph Migration (B1.1) — Issues & Improvements

### 1.1 Current Proposed Design

From the plan:

```rust
pub struct Dag {
    graph: petgraph::DiGraph<TaskNode, ()>,
    node_map: HashMap<String, NodeIndex>,
}
```

**Target (v0.9.1):**

```rust
pub struct Dag {
    graph: petgraph::StableGraph<Arc<str>, ()>,
    id_to_node: FxHashMap<Arc<str>, NodeIndex>,
    node_to_id: Vec<Arc<str>>,
}
```

### Issue 1.1A: Arc<str> is Inefficient for Keys

**Problem:** Using `Arc<str>` as keys in a StableGraph causes **unnecessary allocations** and **cloning overhead**.

```rust
// INEFFICIENT - Current plan
graph: petgraph::StableGraph<Arc<str>, ()>
id_to_node: FxHashMap<Arc<str>, NodeIndex>

// Result: Arc clone on every lookup
let idx = id_to_node[&Arc::from(task_id.as_str())];  // ❌ Extra clone
```

**Why it's bad:**
- `Arc<str>` is expensive to clone (atomic operation + memory dereference)
- Task IDs are short strings ("msg-001") - not large blobs needing Arc
- Every `id_to_node.get()` call requires Arc creation

**Recommended Fix:**

```rust
// BETTER APPROACH 1: Store String, borrow as &str
pub struct Dag {
    graph: petgraph::StableGraph<String, ()>,
    id_to_node: FxHashMap<String, NodeIndex>,  // HashMap is fast for String
    node_to_id: Vec<String>,
}

impl Dag {
    pub fn add_task(&mut self, task_id: String) -> NodeIndex {
        let idx = self.graph.add_node(task_id.clone());
        self.id_to_node.insert(task_id, idx);
        idx
    }

    pub fn node_index(&self, id: &str) -> Option<NodeIndex> {
        self.id_to_node.get(id).copied()  // ✅ Borrow with &str
    }
}

// BETTER APPROACH 2: Use Cow for zero-copy when possible
use std::borrow::Cow;

pub struct Dag {
    graph: petgraph::StableGraph<Cow<'static, str>, ()>,
    // Owned for workflow tasks, borrowed for chat messages
}
```

**Preferred: Use String + &str borrows** (simplest, no Arc overhead)

---

### Issue 1.1B: Missing Interior Mutability for Concurrent Access

**Problem:** `Dag` will be accessed from multiple async tasks (execution + TUI).

```rust
// Current design (not thread-safe)
pub struct Dag {
    graph: petgraph::StableGraph<String, ()>,
    id_to_node: FxHashMap<String, NodeIndex>,
}

// Usage from async executor + TUI simultaneously:
let mut executor = executor.clone();
let mut graph = graph.clone();  // ❌ Can't easily clone mutable references
tokio::spawn(async move {
    graph.add_task(...);  // COMPILE ERROR - moved
});
```

**Recommended Fix:**

```rust
// Use Arc for shared ownership, DashMap for concurrent access
pub struct Dag {
    // Inner struct for actual data
    inner: Arc<DagInner>,
}

struct DagInner {
    graph: RwLock<petgraph::StableGraph<String, ()>>,
    id_to_node: DashMap<String, NodeIndex>,  // Concurrent hash map
    node_to_id: RwLock<Vec<String>>,
}

impl Dag {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DagInner {
                graph: RwLock::new(petgraph::StableGraph::new()),
                id_to_node: DashMap::new(),
                node_to_id: RwLock::new(Vec::new()),
            }),
        }
    }

    pub fn add_task(&self, task_id: String) -> NodeIndex {
        let mut g = self.inner.graph.write();
        let idx = g.add_node(task_id.clone());
        self.inner.id_to_node.insert(task_id, idx);
        idx
    }

    pub fn node_index(&self, id: &str) -> Option<NodeIndex> {
        self.inner.id_to_node.get(id).map(|r| *r)
    }
}

impl Clone for Dag {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),  // Cheap Arc clone
        }
    }
}
```

**Why this matters:**
- Executor runs async tasks in parallel
- TUI reads DAG to render visualization
- Without proper sync, you get race conditions

---

### Issue 1.1C: StableGraph Type Signature Complexity

**Problem:** The plan's type signature is incomplete:

```rust
// Plan shows only this:
graph: petgraph::StableGraph<Arc<str>, ()>

// Missing questions:
// 1. What's stored in nodes? (Task? TaskNode? String?)
// 2. What's stored in edges? (Empty? Edge metadata?)
// 3. How do we handle node removal while keeping indices stable?
```

**Recommended Implementation:**

```rust
// Clear type alias for readability
type TaskGraph = petgraph::StableGraph<TaskNode, EdgeWeight>;

pub struct TaskNode {
    pub id: String,
    pub task: Option<Arc<Task>>,  // Option allows lazy loading
    pub status: TaskStatus,        // Pending/Running/Complete/Failed
}

pub struct EdgeWeight {
    pub source_task: String,
    pub target_task: String,
    pub dependency_type: DependencyType,  // Direct/Conditional/Decompose
}

pub enum DependencyType {
    Direct,
    Conditional(String),  // condition expression
    Decompose,
    ForEach,
}

pub struct Dag {
    graph: TaskGraph,
    id_to_idx: DashMap<String, NodeIndex>,
    idx_to_id: RwLock<Vec<String>>,
}

impl Dag {
    pub fn from_workflow(workflow: &Workflow) -> Result<Self, NikaError> {
        let mut graph = petgraph::StableGraph::new();
        let mut id_to_idx = DashMap::new();
        let mut idx_to_id = Vec::new();

        // 1. Add all tasks as nodes
        for task in &workflow.tasks {
            let node = TaskNode {
                id: task.id.clone(),
                task: Some(Arc::new(task.clone())),
                status: TaskStatus::Pending,
            };
            let idx = graph.add_node(node);
            id_to_idx.insert(task.id.clone(), idx);
            idx_to_id.push(task.id.clone());
        }

        // 2. Add all flows as edges
        for flow in &workflow.flows {
            let source_idx = id_to_idx.get(&flow.source)
                .ok_or_else(|| NikaError::DagError {
                    reason: format!("Unknown source task: {}", flow.source),
                    code: "NIKA-020".into(),
                })?;
            let target_idx = id_to_idx.get(&flow.target)
                .ok_or_else(|| NikaError::DagError {
                    reason: format!("Unknown target task: {}", flow.target),
                    code: "NIKA-020".into(),
                })?;

            graph.add_edge(*source_idx, *target_idx, EdgeWeight {
                source_task: flow.source.clone(),
                target_task: flow.target.clone(),
                dependency_type: DependencyType::Direct,
            });
        }

        // 3. Cycle detection
        if is_cyclic_directed(&graph) {
            return Err(NikaError::DagError {
                reason: "Workflow contains cycles".into(),
                code: "NIKA-021".into(),
            });
        }

        Ok(Self {
            graph,
            id_to_idx,
            idx_to_id: RwLock::new(idx_to_id),
        })
    }

    /// Find all nodes with no dependencies (ready to execute)
    pub fn ready_nodes(&self, completed: &HashSet<String>) -> Vec<String> {
        self.id_to_idx
            .iter()
            .filter(|entry| {
                let idx = *entry.value();
                // Has no incoming edges from incomplete tasks
                self.graph
                    .edges_directed(idx, petgraph::Direction::Incoming)
                    .all(|e| {
                        let source_idx = e.source();
                        let source_id = self.idx_to_id.read()[source_idx.index()].clone();
                        completed.contains(&source_id)
                    })
            })
            .map(|entry| entry.key().clone())
            .collect()
    }
}
```

---

## 2. ChatWorkflow Struct Design — Ownership & Lifetime Issues

### 2.1 Current Proposed Design (from plan)

```rust
pub struct ChatWorkflow {
    pub workflow: Workflow,
    pub dag: Dag,
    pub store: RunContext,
    pub log: EventLog,
    pub message_counter: u32,
}

impl ChatWorkflow {
    pub fn new(session_id: &str) -> Self {
        Self {
            workflow: Workflow { ... },
            dag: Dag::new(),
            store: RunContext::new(),
            log: EventLog::new(),
            message_counter: 0,
        }
    }
}
```

### Issue 2.1A: Ownership Mismatch — ChatAgent + ChatWorkflow

**Problem:** `ChatAgent` will hold both TUI state AND `ChatWorkflow`:

```rust
// From the plan (Task 1.3):
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: ChatWorkflow,  // NEW - but where's the lifetime?
}

// This means:
// 1. ChatAgent owns ChatWorkflow
// 2. TUI owns ChatAgent
// 3. Executor needs to borrow ChatWorkflow from ChatAgent
// 4. Multiple borrows = compiler error

async fn execute_message(agent: &mut ChatAgent, prompt: &str) -> Result<String, NikaError> {
    agent.workflow.add_task(...);  // ✅ Mutable borrow
    // ...
    agent.provider.infer(prompt).await?;  // ✅ Shared borrow
}

// Problem: Can't do this when multiple tasks reference agent
let agent = Arc::new(Mutex::new(agent));  // Now what?
```

**Recommended Fix — Use Arc<Mutex<T>> for shared mutable state:**

```rust
pub struct ChatSession {
    /// Shared workflow state (Mutex for interior mutability)
    pub workflow: Arc<Mutex<ChatWorkflow>>,

    /// TUI layer (immutable)
    pub ui_state: ChatUIState,
}

pub struct ChatWorkflow {
    pub dag: Dag,
    pub store: RunContext,
    pub log: EventLog,
    pub message_counter: u32,
    // NO workflow: Workflow (store in DAG instead)
}

pub struct ChatUIState {
    pub history: Vec<ChatMessage>,
    pub provider: RigProvider,
    pub session_id: String,
}

impl ChatSession {
    pub fn new(session_id: String) -> Self {
        Self {
            workflow: Arc::new(Mutex::new(ChatWorkflow {
                dag: Dag::new(),
                store: RunContext::new(),
                log: EventLog::new(),
                message_counter: 0,
            })),
            ui_state: ChatUIState {
                history: Vec::new(),
                provider: RigProvider::auto().unwrap(),
                session_id,
            },
        }
    }

    /// Used by executor (can hold mutable lock)
    pub async fn add_message_and_execute(
        &self,
        prompt: &str,
    ) -> Result<String, NikaError> {
        let mut wf = self.workflow.lock().await;  // ✅ Acquire lock once

        // All operations on &mut wf
        let task_id = format!("msg-{:03}", wf.message_counter + 1);
        wf.message_counter += 1;

        // Build task
        let task = Task {
            id: task_id.clone(),
            action: TaskAction::Infer(InferParams {
                prompt: prompt.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        wf.dag.add_task(task_id.clone());
        wf.log.emit(EventKind::TaskStarted {
            task_id: task_id.clone().into(),
            action_type: "infer".into(),
        });

        // Execute
        let result = self.ui_state.provider.infer(prompt, None).await?;

        // Store
        wf.store.set(task_id.clone(), serde_json::json!({
            "output": result.clone(),
        }));

        wf.log.emit(EventKind::TaskCompleted {
            task_id: task_id.into(),
            output: result.clone().into(),
        });

        Ok(result)
    }

    /// Used by TUI rendering (read-only)
    pub fn dag_snapshot(&self) -> DagSnapshot {
        let wf = self.workflow.try_lock()
            .expect("DAG not being modified");
        wf.dag.snapshot()  // Snapshot for rendering
    }
}
```

**Key improvements:**
- Arc allows shared ownership (TUI + Executor + Renderer)
- Mutex serializes access to DAG
- No lifetime parameters needed
- TUI state separate from execution state

---

### Issue 2.1B: Missing Context Management

**Problem:** ChatWorkflow has no way to load context files (brand.md, etc.):

```rust
// Plan shows: pub struct ChatWorkflow { workflow: Workflow, ... }
// But Workflow might have: context: ContextSpec

// How do we resolve {{context.files.brand}}?
// Answer in plan: ???
```

**Recommended Fix:**

```rust
pub struct ChatWorkflow {
    pub dag: Dag,
    pub store: RunContext,
    pub log: EventLog,
    pub message_counter: u32,
    pub context_store: ContextStore,  // NEW - from v0.9.1 Sprint 2
}

pub struct ContextStore {
    files: DashMap<String, Value>,
    loaded_at: DashMap<String, Instant>,
}

impl ContextStore {
    pub async fn load_from_spec(spec: &ContextSpec) -> Result<Self, NikaError> {
        let mut store = Self {
            files: DashMap::new(),
            loaded_at: DashMap::new(),
        };

        for (alias, path_or_glob) in &spec.files {
            match path_or_glob {
                PathOrGlob::Path(path) => {
                    let value = ContextLoader::load_file(path).await?;
                    store.files.insert(alias.clone(), value);
                    store.loaded_at.insert(alias.clone(), Instant::now());
                }
                PathOrGlob::Glob(pattern) => {
                    let values = ContextLoader::load_glob(pattern).await?;
                    store.files.insert(alias.clone(), serde_json::json!(values));
                    store.loaded_at.insert(alias.clone(), Instant::now());
                }
            }
        }

        Ok(store)
    }

    pub fn resolve(&self, path: &str) -> Option<Value> {
        // "brand.tagline" → files["brand"]["tagline"]
        let parts: Vec<&str> = path.split('.').collect();
        let root = parts.get(0)?;
        let value = self.files.get(*root)?.value().clone();

        // Navigate nested path
        let mut current = value;
        for key in &parts[1..] {
            current = current.get(key)?.clone();
        }

        Some(current)
    }
}
```

---

## 3. MentionParser & Binding System — Design Clarity

### 3.1 Current Proposed Design

From the plan (B4.2):

```rust
// @mention syntax for references
@1              // Reference message 1
@last           // Reference previous message
@prev           // Alternative for previous
@all            // Reference all messages
```

### Issue 3.1A: Ambiguous Semantics

**Problem:** The plan doesn't specify what "@1" means:

```rust
// Question 1: Is @1 a message ID or a task ID?
> @1 summarize the findings
// Does this mean:
// a) msg-001 (message counter)?
// b) msg-1 (1-indexed)?
// c) The 1st task in DAG order?

// Question 2: How does @1 resolve in binding context?
// Is it {{use.msg_001}} or a special syntax?

// Question 3: What about multiple references?
> Based on @1 and @2, combine findings
// Are these sequential edges or both sourced from same node?
```

**Recommended Fix — Explicit Design:**

```rust
pub enum MentionRef {
    /// Absolute message number (1-indexed): @1, @42
    Absolute(u32),

    /// Relative to current message: @prev, @last
    Relative(RelativeRef),

    /// Range: @1-3 (combine messages 1-3)
    Range(u32, u32),

    /// All messages: @*
    All,
}

pub enum RelativeRef {
    Previous,
    Last,
    All,
}

pub struct MentionParser;

impl MentionParser {
    /// Parse "@1", "@prev", "@1-3", "@*" from user input
    pub fn parse(input: &str) -> Result<Vec<MentionRef>, ParseError> {
        // Regex: @(\d+)(?:-(\d+))?|@(prev|last|all|\*)
        let re = regex::Regex::new(
            r"@(?:(\d+)(?:-(\d+))?|([a-z*]+))"
        ).unwrap();

        let mut refs = Vec::new();
        for cap in re.captures_iter(input) {
            match (cap.get(1), cap.get(2), cap.get(3)) {
                (Some(start), None, None) => {
                    refs.push(MentionRef::Absolute(start.as_str().parse()?));
                }
                (Some(start), Some(end), None) => {
                    let s = start.as_str().parse()?;
                    let e = end.as_str().parse()?;
                    if s > e {
                        return Err(ParseError::InvalidRange(s, e));
                    }
                    refs.push(MentionRef::Range(s, e));
                }
                (None, None, Some(keyword)) => {
                    match keyword.as_str() {
                        "prev" | "previous" => refs.push(MentionRef::Relative(RelativeRef::Previous)),
                        "last" => refs.push(MentionRef::Relative(RelativeRef::Last)),
                        "all" => refs.push(MentionRef::Relative(RelativeRef::All)),
                        "*" => refs.push(MentionRef::All),
                        _ => return Err(ParseError::UnknownRef(keyword.to_string())),
                    }
                }
                _ => return Err(ParseError::InvalidSyntax),
            }
        }

        Ok(refs)
    }
}

// Resolution in ChatDAG
impl ChatDAG {
    pub fn resolve_mention(
        &self,
        mention: &MentionRef,
    ) -> Result<Vec<String>, NikaError> {
        match mention {
            MentionRef::Absolute(n) => {
                let id = format!("msg-{:03}", n);
                if self.tasks.contains_key(&id) {
                    Ok(vec![id])
                } else {
                    Err(NikaError::BindingError {
                        alias: format!("@{}", n),
                        reason: "Message not found".into(),
                    })
                }
            }
            MentionRef::Relative(RelativeRef::Previous) => {
                if let Some(last) = self.tasks.keys().last() {
                    Ok(vec![last.clone()])
                } else {
                    Err(NikaError::BindingError {
                        alias: "@prev".into(),
                        reason: "No previous message".into(),
                    })
                }
            }
            MentionRef::All => {
                Ok(self.tasks.keys().cloned().collect())
            }
            MentionRef::Range(start, end) => {
                let ids: Vec<String> = (*start..=*end)
                    .map(|n| format!("msg-{:03}", n))
                    .collect();

                let all_found = ids.iter().all(|id| self.tasks.contains_key(id));
                if all_found {
                    Ok(ids)
                } else {
                    Err(NikaError::BindingError {
                        alias: format!("@{}-{}", start, end),
                        reason: "Some messages in range not found".into(),
                    })
                }
            }
            MentionRef::Relative(RelativeRef::Last) => {
                // Same as Previous for now
                if let Some(last) = self.tasks.keys().last() {
                    Ok(vec![last.clone()])
                } else {
                    Err(NikaError::BindingError {
                        alias: "@last".into(),
                        reason: "No previous message".into(),
                    })
                }
            }
            MentionRef::Relative(RelativeRef::All) => {
                Ok(self.tasks.keys().cloned().collect())
            }
        }
    }
}
```

---

### 3.1B: Fork Syntax (`//`) Needs Clarification

**Problem:** The plan mentions:

```yaml
# B4.3 Fork Syntax — // prefix for parallel tasks

> // Research QR trends
> // Analyze competitor products
```

**Issue:** What does a "fork" mean in DAG execution?

```rust
// Option A: Parallel branches (both run from last message)
msg-001
├─ msg-002 (research)
└─ msg-003 (competitor)
// Then merge on next message?

// Option B: Sequential but marked as independent
msg-001 → msg-002 → msg-003 (linear)
// But msg-002 and msg-003 marked for parallel execution?

// Option C: Conditional branching
// "If you want research OR competitor analysis"
```

**Recommended clarification:**

```rust
pub enum MessageType {
    Sequential,    // Default: depends on previous message
    Parallel,      // //, depends on same parent as previous
    Conditional,   // /if condition
    Loop,          // /repeat
}

pub struct ChatMessage {
    pub id: String,
    pub content: String,
    pub role: Role,
    pub msg_type: MessageType,
}

impl ChatDAG {
    pub fn add_message_parsed(&mut self, input: &str) -> Result<ChatMessage, ParseError> {
        let (msg_type, content) = if input.starts_with("//") {
            (MessageType::Parallel, &input[2..].trim())
        } else if input.starts_with("/if ") {
            (MessageType::Conditional, &input[4..].trim())
        } else {
            (MessageType::Sequential, input)
        };

        let id = format!("msg-{:03}", self.tasks.len() + 1);
        let msg = ChatMessage {
            id: id.clone(),
            content: content.to_string(),
            role: Role::User,
            msg_type,
        };

        match msg_type {
            MessageType::Sequential => {
                if let Some(parent) = self.tasks.keys().last() {
                    self.dag.add_edge(parent, &id);
                }
            }
            MessageType::Parallel => {
                if let Some(parent) = self.tasks.keys().last() {
                    // Add edge from grandparent (same as parent's parent)
                    if let Some(grandparent_idx) = self.dag.predecessors(parent).next() {
                        self.dag.add_edge(&grandparent_idx, &id);
                    }
                }
            }
            _ => {}
        }

        self.tasks.insert(id.clone(), msg.clone());
        Ok(msg)
    }
}
```

---

## 4. Error Handling — Consistency & Context

### 4.1 Current Issues

From the implementation plan, error handling is scattered:

```rust
// B1.1 - Dag
pub fn from_workflow(workflow: &Workflow) -> Result<Self, NikaError> {
    // No examples given
}

// B1.3 - Context Resolver
pub async fn load_file(path: &Path) -> Result<Value, NikaError> {
    // No error code specification
}

// B4.1 - ChatDAG
pub fn add_message(&mut self, content: &str) -> ChatTask {
    // Returns ChatTask, no error handling!
}
```

### Issue 4.1A: Missing Error Codes

**Problem:** Not all error paths use NikaError codes:

```rust
// BAD - Missing error context
pub fn resolve_mention(&self, mention: &str) -> Result<String, NikaError> {
    self.tasks.get(mention)
        .ok_or_else(|| NikaError::Other("mention not found".into()))
        // ❌ No code, no context
}

// GOOD - Full NikaError with code
pub fn resolve_mention(&self, mention: &str) -> Result<String, NikaError> {
    self.tasks.get(mention)
        .ok_or_else(|| NikaError::BindingError {
            alias: mention.to_string(),
            reason: format!("Message {} not found in DAG", mention),
            // Add code if not in NikaError
            code: Some("NIKA-041"),  // Binding error
        })
}
```

### Recommended: Error Code Registry for v0.9.1

```rust
// src/error.rs — Add these codes
pub enum ErrorCode {
    // Context errors (NIKA-200-209)
    ContextFileNotFound = 200,
    ContextParseError = 201,
    ContextGlobExpansionFailed = 202,

    // DAG/StableGraph errors (NIKA-210-219)
    DagTaskNotFound = 210,
    DagEdgeCreationFailed = 211,
    DagCycleDetected = 212,
    DagStableIndexError = 213,

    // Chat mention/binding errors (NIKA-220-229)
    MentionParseError = 220,
    MentionResolutionFailed = 221,
    MentionOutOfRange = 222,

    // Agent/Skill loading errors (NIKA-230-239)
    AgentNotFound = 230,
    SkillNotFound = 231,
    AgentParseError = 232,
}

#[derive(Error, Debug)]
#[error("[NIKA-{code}] {message}")]
pub struct NikaError {
    pub code: u16,
    pub message: String,
    pub context: Option<String>,  // NEW - additional context
}

impl NikaError {
    pub fn context_file_not_found(path: &Path) -> Self {
        Self {
            code: 200,
            message: format!("Context file not found: {}", path.display()),
            context: Some(format!("Check path and permissions: {}", path.display())),
        }
    }

    pub fn dag_task_not_found(task_id: &str) -> Self {
        Self {
            code: 210,
            message: format!("Task '{}' not found in DAG", task_id),
            context: Some("Available tasks: [list them]".into()),
        }
    }

    pub fn mention_parse_error(input: &str, reason: &str) -> Self {
        Self {
            code: 220,
            message: format!("Failed to parse mention '@': {}", reason),
            context: Some("Valid formats: @1, @prev, @1-5, @*".into()),
        }
    }
}
```

---

## 5. Async Patterns — Multiple Issues

### 5.1 Current Proposed Code (from plan)

```rust
// B1.5 - Integration Tests
pub async fn test_context_loading() {
    let ctx = ContextLoader::load_file("brand.md").await?;
    // ...
}

// B4.4 - Unified Executor
pub async fn execute(&mut self, task: &ChatTask, executor: &TaskExecutor) -> Result<String> {
    let result = executor.execute(&workflow_task, &ctx).await?;
    // ...
}
```

### Issue 5.1A: Missing Timeout Protection on Async Operations

**Problem:** Long-running file I/O has no timeout:

```rust
// UNSAFE - Can hang forever on slow disk
pub async fn load_file(path: &Path) -> Result<Value, NikaError> {
    let content = tokio::fs::read_to_string(path).await?;
    // ❌ No timeout, can block TUI
}

// SAFE - With timeout
pub async fn load_file(path: &Path, timeout_secs: u64) -> Result<Value, NikaError> {
    tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::fs::read_to_string(path),
    )
    .await
    .map_err(|_| NikaError {
        code: 200,
        message: format!("Timeout reading {}", path.display()),
        context: None,
    })?
    .map_err(|e| NikaError {
        code: 200,
        message: format!("Failed to read {}: {}", path.display(), e),
        context: None,
    })
}
```

### Issue 5.1B: Ownership in Async Closures

**Problem:** Moving values into tokio::spawn can cause issues:

```rust
// From B4.4 Unified Executor
pub async fn execute(&mut self, task: &ChatTask, executor: &TaskExecutor) -> Result<String> {
    let result = executor.execute(&workflow_task, &ctx).await?;
    // ...
}

// If we spawn this as a background task:
tokio::spawn(async move {
    // `self`, `executor`, `task` all moved here
    // Can't use them in caller anymore
    // TUI can't update while task runs
});
```

**Recommended fix — Separate background execution from sync API:**

```rust
pub struct ExecutionHandle {
    rx: tokio::sync::watch::Receiver<ExecutionState>,
    task_handle: tokio::task::JoinHandle<Result<String, NikaError>>,
}

pub enum ExecutionState {
    Pending,
    Running { progress: u32 },
    Streaming { content: String },
    Complete { output: String },
    Failed { error: String },
}

impl ChatSession {
    /// Spawn background execution, return handle for polling
    pub fn execute_message_background(
        &self,
        prompt: &str,
    ) -> ExecutionHandle {
        let workflow = Arc::clone(&self.workflow);
        let provider = self.ui_state.provider.clone();
        let task_id = format!("msg-{:03}", /* counter */);

        let (tx, rx) = tokio::sync::watch::channel(ExecutionState::Pending);

        let handle = tokio::spawn(async move {
            tx.send(ExecutionState::Running { progress: 0 }).ok();

            let mut wf = workflow.lock().await;
            let result = provider.infer(prompt, None).await?;

            wf.store.set(task_id.clone(), serde_json::json!({
                "output": result.clone(),
            }));

            tx.send(ExecutionState::Complete { output: result.clone() }).ok();
            Ok(result)
        });

        ExecutionHandle { rx, task_handle: handle }
    }

    /// Poll for execution state (called by TUI render loop)
    pub async fn poll_execution(handle: &mut ExecutionHandle) -> Option<ExecutionState> {
        handle.rx.borrow().clone()
    }
}
```

---

## 6. Summary of Key Recommendations

### Must-Fix Before Coding (CRITICAL)

| Item | Issue | Fix | Effort |
|------|-------|-----|--------|
| StableGraph key type | Arc<str> inefficient | Use String + &str borrows | 2 hours |
| ChatWorkflow ownership | Missing Arc<Mutex<>> | Add shared mutable state | 3 hours |
| Error codes | Scattered/incomplete | Create ErrorCode registry | 1 hour |

### Should-Fix (HIGH)

| Item | Issue | Fix | Effort |
|------|-------|-----|--------|
| Thread-safety | No RwLock/DashMap | Add interior mutability | 2 hours |
| Mention parsing | Ambiguous semantics | Define MentionRef enum | 1 hour |
| Timeout handling | Missing on async ops | Add timeout() wrappers | 1 hour |

### Nice-to-Have (MEDIUM)

| Item | Issue | Fix | Effort |
|------|-------|-----|--------|
| Background execution | Blocks TUI | Use ExecutionHandle pattern | 2 hours |
| Context loading | No context in DAG | Wire ContextStore | 1 hour |
| Fork syntax | Undefined behavior | Implement parallel edges | 1 hour |

---

## 7. Recommended Coding Order for v0.9.1

**Sprint 1 order (adjusted for dependencies):**

1. **Error codes** (1 hour) — Needed by everything else
2. **StableGraph + thread-safety** (6 hours) — Foundation for DAG
3. **MentionParser** (2 hours) — Needed for Chat
4. **ChatWorkflow + Arc<Mutex>** (4 hours) — Integration point
5. **Context loading** (3 hours) — Feature completion
6. **Tests & integration** (4 hours)

**Total effort:** ~20 hours (adjusted from 30 hours in plan due to better upfront design)

---

## 8. Anti-Patterns to Avoid

### ❌ Don't:

```rust
// 1. Expose internal Arc<str> keys
pub fn node_ids(&self) -> Vec<Arc<str>> {
    self.id_to_node.keys().cloned().collect()  // Forces Arc clone
}
// ✅ Do: Return &str
pub fn node_ids(&self) -> Vec<&str> {
    self.id_to_node.keys().map(|s| s.as_str()).collect()
}

// 2. Use &mut on async-shared data
pub async fn add_task(&mut self, id: String) {
    // Can't be called from multiple concurrent tasks!
}
// ✅ Do: Use &self with interior mutability
pub async fn add_task(&self, id: String) {
    // Works with Arc<Mutex<>>
}

// 3. Block in async code
pub async fn load_context() -> Result<Value> {
    let content = std::fs::read_to_string("file.md")?;  // Blocks executor!
}
// ✅ Do: Use tokio::fs
pub async fn load_context() -> Result<Value> {
    let content = tokio::fs::read_to_string("file.md").await?;
}

// 4. Clone Arc<str> unnecessarily
let key = Arc::from("msg-001");
let key2 = key.clone();  // Atomic operation
// ✅ Do: Borrow references
let key = "msg-001";
let key_borrowed = key;  // Copy
```

---

## 9. Testing Strategy for v0.9.1

### Unit Tests (Inline in each module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_parser_absolute() {
        let refs = MentionParser::parse("Based on @1 and @2")
            .unwrap();
        assert_eq!(refs.len(), 2);
        assert!(matches!(refs[0], MentionRef::Absolute(1)));
    }

    #[test]
    fn test_chatdag_resolve_mention_not_found() {
        let dag = ChatDAG::new();
        let err = dag.resolve_mention(&MentionRef::Absolute(42))
            .unwrap_err();
        assert_eq!(err.code, 221);  // MentionResolutionFailed
    }

    #[tokio::test]
    async fn test_context_loader_with_timeout() {
        let result = ContextLoader::load_file(
            Path::new("nonexistent.md"),
            1,  // 1 second timeout
        ).await;
        assert!(result.is_err());
    }
}
```

### Integration Tests (`tests/`)

```rust
// tests/v091_integration.rs
#[tokio::test]
async fn test_chat_workflow_with_mentions() {
    let session = ChatSession::new("test".into());

    session.add_message_and_execute("Hello").await.unwrap();
    session.add_message_and_execute("@1 summarize").await.unwrap();

    let dag_snap = session.dag_snapshot();
    assert_eq!(dag_snap.nodes, 2);
    assert_eq!(dag_snap.edges, vec![("msg-001", "msg-002")]);
}
```

---

## 10. References & Related Documents

- **Current Dag:** `/src/dag/flow_graph.rs`
- **Existing TaskExecutor:** `/src/runtime/executor.rs`
- **Error handling:** `/src/error.rs`
- **ContextLoader (v0.9.1):** `docs/plans/v0.9.1/2026-02-24-memory-and-agents-design.md`

---

**End of Review**

Questions? File issues or reference this document in code comments.
