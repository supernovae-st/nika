//! # Global and Local Context (v4.7.1)
//!
//! Context management for workflow execution with compile-time safety.
//!
//! ## Design
//!
//! - `GlobalContext`: Mutable context for agent: tasks (shared history)
//! - `LocalContext`: Immutable snapshot for subagent: tasks (isolated)
//! - Traits enforce compile-time safety via borrow rules
//!
//! ## v4.7.1 Performance Optimization
//!
//! `snapshot()` now uses `Arc<HashMap>` for zero-copy sharing:
//! - Before: O(n) deep clone of all HashMaps
//! - After: O(1) Arc clone (just increment refcount)
//!
//! This is safe because LocalContext only needs read access to the snapshot.
//! Copy-on-write semantics ensure GlobalContext can continue to mutate its
//! data without affecting existing snapshots.

use crate::smart_string::SmartString;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// SECURITY: Environment Variable Protection
// ============================================================================

/// Blocklist of sensitive environment variable patterns
/// These are blocked to prevent exfiltration of secrets via ${env.VAR}
const BLOCKED_ENV_PATTERNS: &[&str] = &[
    // API Keys and tokens
    "API_KEY",
    "APIKEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
    // Cloud provider secrets
    "AWS_",
    "AZURE_",
    "GCP_",
    "GOOGLE_",
    "OPENAI_",
    "ANTHROPIC_",
    "MISTRAL_",
    "HUGGING",
    // Database
    "DATABASE_URL",
    "DB_",
    "MONGO",
    "REDIS_",
    "POSTGRES",
    "MYSQL",
    // SSH/GPG
    "SSH_",
    "GPG_",
    "PRIVATE_KEY",
    // CI/CD
    "CI_",
    "GITHUB_TOKEN",
    "GITLAB_",
    "JENKINS_",
    // Generic sensitive
    "ENCRYPT",
    "DECRYPT",
    "SIGNING",
    "CERT",
];

/// Check if an environment variable name is blocked
fn is_env_blocked(name: &str) -> bool {
    let upper = name.to_uppercase();
    BLOCKED_ENV_PATTERNS
        .iter()
        .any(|pattern| upper.contains(pattern))
}

// ============================================================================
// MESSAGE TYPES
// ============================================================================

/// Role for agent messages (1 byte with Copy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageRole {
    User = 0,
    Assistant = 1,
    System = 2,
}

/// A message in the agent conversation history
///
/// Uses Arc<str> for zero-copy sharing of message content
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: Arc<str>,
}

impl AgentMessage {
    /// Create a new agent message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Arc::from(content.into()),
        }
    }
}

// ============================================================================
// CONTEXT TRAITS
// ============================================================================

/// Read-only access to context data
pub trait ContextReader {
    /// Get a task's output
    fn get_output(&self, task_id: &str) -> Option<&str>;

    /// Get a field from a structured output
    fn get_field(&self, task_id: &str, field: &str) -> Option<String>;

    /// Get an input parameter
    fn get_input(&self, name: &str) -> Option<&str>;

    /// Get an environment variable
    fn get_env(&self, name: &str) -> Option<String>;

    /// Get the agent conversation history
    fn agent_history(&self) -> &[AgentMessage];

    /// Check if we have any conversation history
    fn has_history(&self) -> bool {
        !self.agent_history().is_empty()
    }

    /// Get conversation history as a formatted string
    fn format_agent_history(&self) -> String {
        self.agent_history()
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                };
                format!("{}: {}", role, msg.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Write access to context data (only for GlobalContext)
pub trait ContextWriter: ContextReader {
    /// Store a task's output
    fn set_output(&mut self, task_id: &str, output: String);

    /// Store a structured output (for field access)
    fn set_structured_output(&mut self, task_id: &str, value: serde_json::Value);

    /// Add a message to the agent conversation history
    fn add_agent_message(&mut self, role: MessageRole, content: String);
}

// ============================================================================
// GLOBAL CONTEXT
// ============================================================================

/// Global execution context for agent: tasks
///
/// This is the main context shared across the workflow.
/// agent: tasks can READ and WRITE to this context.
///
/// ## Thread Safety
///
/// GlobalContext is !Send + !Sync by design. It should only be used
/// from a single task executor. Use Arc<Mutex<GlobalContext>> if needed
/// for concurrent access (not recommended for performance).
///
/// ## v4.7.1 Copy-on-Write Architecture
///
/// HashMaps are wrapped in `Arc` for efficient `snapshot()`:
/// - `snapshot()` is O(1) - just clones the Arc (refcount increment)
/// - On mutation, we use `Arc::make_mut` for copy-on-write
/// - This avoids deep cloning all entries on every subagent spawn
#[derive(Debug, Clone)]
pub struct GlobalContext {
    /// Outputs from completed tasks (task_id -> output string)
    /// Wrapped in Arc for copy-on-write snapshot()
    outputs: Arc<HashMap<SmartString, Arc<str>>>,

    /// Structured outputs for field access (task_id -> JSON value)
    /// Wrapped in Arc for copy-on-write snapshot()
    structured_outputs: Arc<HashMap<SmartString, serde_json::Value>>,

    /// Main agent conversation history (for context sharing between agent: tasks)
    /// NOT wrapped in Arc - subagents get fresh history anyway
    agent_history: Vec<AgentMessage>,

    /// Input parameters passed to the workflow
    /// Wrapped in Arc for copy-on-write snapshot()
    inputs: Arc<HashMap<String, Arc<str>>>,

    /// Environment variables snapshot
    /// Wrapped in Arc for copy-on-write snapshot()
    env_vars: Arc<HashMap<String, Arc<str>>>,
}

impl Default for GlobalContext {
    fn default() -> Self {
        Self {
            outputs: Arc::new(HashMap::new()),
            structured_outputs: Arc::new(HashMap::new()),
            agent_history: Vec::new(),
            inputs: Arc::new(HashMap::new()),
            env_vars: Arc::new(HashMap::new()),
        }
    }
}

impl GlobalContext {
    /// Create a new global context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with input parameters
    pub fn with_inputs(inputs: HashMap<String, String>) -> Self {
        let inputs: HashMap<String, Arc<str>> =
            inputs.into_iter().map(|(k, v)| (k, Arc::from(v))).collect();
        Self {
            inputs: Arc::new(inputs),
            ..Default::default()
        }
    }

    /// Create a read-only snapshot for isolated execution (v4.7.1 optimized)
    ///
    /// This creates a LocalContext that captures the current state
    /// but cannot modify the GlobalContext.
    ///
    /// ## Performance (v4.7.1)
    ///
    /// This is now O(1) instead of O(n):
    /// - Just clones the Arc pointers (refcount increment)
    /// - No deep copy of HashMap contents
    /// - Copy-on-write: GlobalContext mutations don't affect snapshots
    #[inline]
    pub fn snapshot(&self) -> LocalContext {
        LocalContext {
            // O(1) Arc clone - just refcount increment
            outputs: Arc::clone(&self.outputs),
            structured_outputs: Arc::clone(&self.structured_outputs),
            inputs: Arc::clone(&self.inputs),
            env_vars: Arc::clone(&self.env_vars),
            // Note: agent_history is NOT shared - subagents get fresh history
            local_outputs: HashMap::new(),
            local_structured_outputs: HashMap::new(),
            local_history: Vec::new(),
        }
    }

    /// Store multiple outputs at once (batch operation)
    ///
    /// Uses `Arc::make_mut` for copy-on-write semantics.
    pub fn set_outputs_batch<I, K, V>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        // Copy-on-write: only clones if there are other Arc references
        Arc::make_mut(&mut self.outputs).extend(
            outputs
                .into_iter()
                .map(|(k, v)| (SmartString::from(k.as_ref()), Arc::from(v.into()))),
        );
    }

    /// Store multiple structured outputs at once (batch operation)
    ///
    /// Uses `Arc::make_mut` for copy-on-write semantics.
    pub fn set_structured_outputs_batch<I, K>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, serde_json::Value)>,
        K: AsRef<str>,
    {
        // Copy-on-write: only clones if there are other Arc references
        Arc::make_mut(&mut self.structured_outputs).extend(
            outputs
                .into_iter()
                .map(|(k, v)| (SmartString::from(k.as_ref()), v)),
        );
    }

    /// Add multiple messages to the agent conversation history (batch operation)
    pub fn add_agent_messages_batch<I, S>(&mut self, messages: I)
    where
        I: IntoIterator<Item = (MessageRole, S)>,
        S: Into<String>,
    {
        self.agent_history
            .extend(messages.into_iter().map(|(role, content)| AgentMessage {
                role,
                content: Arc::from(content.into()),
            }));
    }

    /// Clear all data from the context (for reuse in memory pool)
    ///
    /// Uses `Arc::make_mut` for copy-on-write semantics.
    pub fn clear(&mut self) {
        // Copy-on-write: only clones if there are other Arc references
        Arc::make_mut(&mut self.outputs).clear();
        Arc::make_mut(&mut self.structured_outputs).clear();
        self.agent_history.clear();
        Arc::make_mut(&mut self.inputs).clear();
        Arc::make_mut(&mut self.env_vars).clear();
    }
}

impl ContextReader for GlobalContext {
    fn get_output(&self, task_id: &str) -> Option<&str> {
        self.outputs.get(task_id).map(|arc| arc.as_ref())
    }

    fn get_field(&self, task_id: &str, field: &str) -> Option<String> {
        self.structured_outputs
            .get(task_id)
            .and_then(|v| v.get(field))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
    }

    fn get_input(&self, name: &str) -> Option<&str> {
        self.inputs.get(name).map(|arc| arc.as_ref())
    }

    fn get_env(&self, name: &str) -> Option<String> {
        // SECURITY: Block access to sensitive environment variables
        if is_env_blocked(name) {
            tracing::warn!(
                "Blocked access to sensitive env var: {} (use explicit secrets)",
                name
            );
            return None;
        }

        self.env_vars
            .get(name)
            .map(|arc| arc.to_string())
            .or_else(|| std::env::var(name).ok())
    }

    fn agent_history(&self) -> &[AgentMessage] {
        &self.agent_history
    }
}

impl ContextWriter for GlobalContext {
    fn set_output(&mut self, task_id: &str, output: String) {
        // Copy-on-write: only clones if there are other Arc references
        Arc::make_mut(&mut self.outputs).insert(SmartString::from(task_id), Arc::from(output));
    }

    fn set_structured_output(&mut self, task_id: &str, value: serde_json::Value) {
        // Copy-on-write: only clones if there are other Arc references
        Arc::make_mut(&mut self.structured_outputs).insert(SmartString::from(task_id), value);
    }

    fn add_agent_message(&mut self, role: MessageRole, content: String) {
        self.agent_history.push(AgentMessage {
            role,
            content: Arc::from(content),
        });
    }
}

// ============================================================================
// LOCAL CONTEXT
// ============================================================================

/// Local context for subagent: tasks (isolated execution)
///
/// This is a snapshot of the GlobalContext at a point in time.
/// subagent: tasks can READ from the snapshot but their writes
/// go to local storage only.
///
/// ## Isolation Guarantees
///
/// - Subagents get a SHARED reference to outputs/inputs (via Arc)
/// - Subagents get EMPTY agent_history (fresh context)
/// - Subagent writes go to local_* fields only
/// - Changes are NOT reflected back to GlobalContext automatically
///
/// ## v4.7.1 Performance
///
/// The snapshot fields are now `Arc<HashMap>` instead of `HashMap`:
/// - O(1) snapshot creation (just Arc refcount increment)
/// - Zero-copy read access to parent context data
/// - Local writes still go to separate HashMap (no Arc overhead)
#[derive(Debug, Clone)]
pub struct LocalContext {
    /// Snapshot of global outputs (read-only, shared via Arc)
    outputs: Arc<HashMap<SmartString, Arc<str>>>,

    /// Snapshot of global structured outputs (read-only, shared via Arc)
    structured_outputs: Arc<HashMap<SmartString, serde_json::Value>>,

    /// Snapshot of inputs (read-only, shared via Arc)
    inputs: Arc<HashMap<String, Arc<str>>>,

    /// Snapshot of env vars (read-only, shared via Arc)
    env_vars: Arc<HashMap<String, Arc<str>>>,

    /// Local outputs from this subagent (write goes here)
    local_outputs: HashMap<SmartString, Arc<str>>,

    /// Local structured outputs (write goes here)
    local_structured_outputs: HashMap<SmartString, serde_json::Value>,

    /// Local conversation history (starts empty for isolation)
    local_history: Vec<AgentMessage>,
}

impl LocalContext {
    /// Store a task's output in local storage
    pub fn set_local_output(&mut self, task_id: &str, output: String) {
        self.local_outputs
            .insert(SmartString::from(task_id), Arc::from(output));
    }

    /// Store a structured output in local storage
    pub fn set_local_structured_output(&mut self, task_id: &str, value: serde_json::Value) {
        self.local_structured_outputs
            .insert(SmartString::from(task_id), value);
    }

    /// Add a message to the local conversation history
    pub fn add_local_message(&mut self, role: MessageRole, content: String) {
        self.local_history.push(AgentMessage {
            role,
            content: Arc::from(content),
        });
    }

    /// Get local outputs (for bridging back to GlobalContext)
    pub fn local_outputs(&self) -> &HashMap<SmartString, Arc<str>> {
        &self.local_outputs
    }

    /// Get local structured outputs (for bridging back to GlobalContext)
    pub fn local_structured_outputs(&self) -> &HashMap<SmartString, serde_json::Value> {
        &self.local_structured_outputs
    }

    /// Get local history (for debugging/inspection)
    pub fn local_history(&self) -> &[AgentMessage] {
        &self.local_history
    }
}

impl ContextReader for LocalContext {
    fn get_output(&self, task_id: &str) -> Option<&str> {
        // Check local first, then snapshot
        self.local_outputs
            .get(task_id)
            .map(|arc| arc.as_ref())
            .or_else(|| self.outputs.get(task_id).map(|arc| arc.as_ref()))
    }

    fn get_field(&self, task_id: &str, field: &str) -> Option<String> {
        // Check local first, then snapshot
        self.local_structured_outputs
            .get(task_id)
            .and_then(|v| v.get(field))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .or_else(|| {
                self.structured_outputs
                    .get(task_id)
                    .and_then(|v| v.get(field))
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
            })
    }

    fn get_input(&self, name: &str) -> Option<&str> {
        self.inputs.get(name).map(|arc| arc.as_ref())
    }

    fn get_env(&self, name: &str) -> Option<String> {
        // SECURITY: Block access to sensitive environment variables
        if is_env_blocked(name) {
            tracing::warn!(
                "Blocked access to sensitive env var: {} (use explicit secrets)",
                name
            );
            return None;
        }

        self.env_vars
            .get(name)
            .map(|arc| arc.to_string())
            .or_else(|| std::env::var(name).ok())
    }

    fn agent_history(&self) -> &[AgentMessage] {
        // Return local history (starts empty for isolation)
        &self.local_history
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_context_output() {
        let mut ctx = GlobalContext::new();
        ctx.set_output("task1", "Hello World".to_string());

        assert_eq!(ctx.get_output("task1"), Some("Hello World"));
        assert_eq!(ctx.get_output("nonexistent"), None);
    }

    #[test]
    fn test_global_context_history() {
        let mut ctx = GlobalContext::new();

        ctx.add_agent_message(MessageRole::User, "What is 2+2?".to_string());
        ctx.add_agent_message(MessageRole::Assistant, "2+2 equals 4.".to_string());

        assert!(ctx.has_history());
        assert_eq!(ctx.agent_history().len(), 2);

        let formatted = ctx.format_agent_history();
        assert!(formatted.contains("User: What is 2+2?"));
        assert!(formatted.contains("Assistant: 2+2 equals 4."));
    }

    #[test]
    fn test_snapshot_creates_local_context() {
        let mut global = GlobalContext::new();
        global.set_output("task1", "output1".to_string());
        global.add_agent_message(MessageRole::User, "Hello".to_string());

        let local = global.snapshot();

        // Local should have the output
        assert_eq!(local.get_output("task1"), Some("output1"));

        // Local should NOT have the history (isolated)
        assert!(!local.has_history());
        assert_eq!(local.agent_history().len(), 0);
    }

    #[test]
    fn test_local_context_isolation() {
        let mut global = GlobalContext::new();
        global.set_output("global_task", "global_output".to_string());

        let mut local = global.snapshot();

        // Write to local
        local.set_local_output("local_task", "local_output".to_string());
        local.add_local_message(MessageRole::User, "Local question".to_string());

        // Local sees both global and local outputs
        assert_eq!(local.get_output("global_task"), Some("global_output"));
        assert_eq!(local.get_output("local_task"), Some("local_output"));

        // Global does NOT see local output
        assert_eq!(global.get_output("local_task"), None);

        // Local has its own history
        assert_eq!(local.agent_history().len(), 1);
        assert_eq!(global.agent_history().len(), 0);
    }

    #[test]
    fn test_local_context_prioritizes_local_output() {
        let mut global = GlobalContext::new();
        global.set_output("task", "global_value".to_string());

        let mut local = global.snapshot();
        local.set_local_output("task", "local_value".to_string());

        // Local value takes precedence
        assert_eq!(local.get_output("task"), Some("local_value"));

        // Global unchanged
        assert_eq!(global.get_output("task"), Some("global_value"));
    }

    #[test]
    fn test_context_inputs() {
        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "src/main.rs".to_string());

        let ctx = GlobalContext::with_inputs(inputs);

        assert_eq!(ctx.get_input("file"), Some("src/main.rs"));
        assert_eq!(ctx.get_input("missing"), None);
    }

    #[test]
    fn test_structured_output_field_access() {
        let mut ctx = GlobalContext::new();
        ctx.set_structured_output(
            "user",
            serde_json::json!({
                "name": "Alice",
                "age": 30
            }),
        );

        assert_eq!(ctx.get_field("user", "name"), Some("Alice".to_string()));
        assert_eq!(ctx.get_field("user", "age"), Some("30".to_string()));
        assert_eq!(ctx.get_field("user", "missing"), None);
    }

    #[test]
    fn test_batch_operations() {
        let mut ctx = GlobalContext::new();

        ctx.set_outputs_batch([("task1", "output1"), ("task2", "output2")]);

        assert_eq!(ctx.get_output("task1"), Some("output1"));
        assert_eq!(ctx.get_output("task2"), Some("output2"));

        ctx.add_agent_messages_batch([(MessageRole::User, "Q1"), (MessageRole::Assistant, "A1")]);

        assert_eq!(ctx.agent_history().len(), 2);
    }

    // ========================================================================
    // v4.7.1 COPY-ON-WRITE TESTS
    // ========================================================================

    #[test]
    fn test_snapshot_is_zero_copy() {
        let mut global = GlobalContext::new();

        // Add many outputs to make deep clone expensive
        for i in 0..100 {
            global.set_output(&format!("task{}", i), format!("output{}", i));
        }

        // Take a snapshot - this should be O(1) now
        let local = global.snapshot();

        // Both should see the same outputs (shared via Arc)
        assert_eq!(local.get_output("task0"), Some("output0"));
        assert_eq!(local.get_output("task99"), Some("output99"));

        // Verify Arc is shared (same pointer)
        assert!(Arc::ptr_eq(&global.outputs, &local.outputs));
    }

    #[test]
    fn test_copy_on_write_after_snapshot() {
        let mut global = GlobalContext::new();
        global.set_output("task1", "original".to_string());

        // Take snapshot
        let local = global.snapshot();

        // Both share the same Arc initially
        assert!(Arc::ptr_eq(&global.outputs, &local.outputs));

        // Mutate global - this triggers copy-on-write
        global.set_output("task2", "new".to_string());

        // Now they should have different Arcs
        assert!(!Arc::ptr_eq(&global.outputs, &local.outputs));

        // Local still sees original data
        assert_eq!(local.get_output("task1"), Some("original"));
        assert_eq!(local.get_output("task2"), None);

        // Global sees new data
        assert_eq!(global.get_output("task1"), Some("original"));
        assert_eq!(global.get_output("task2"), Some("new"));
    }

    #[test]
    fn test_multiple_snapshots_share_data() {
        let mut global = GlobalContext::new();
        global.set_output("shared", "data".to_string());

        // Take multiple snapshots
        let local1 = global.snapshot();
        let local2 = global.snapshot();
        let local3 = global.snapshot();

        // All share the same Arc
        assert!(Arc::ptr_eq(&global.outputs, &local1.outputs));
        assert!(Arc::ptr_eq(&local1.outputs, &local2.outputs));
        assert!(Arc::ptr_eq(&local2.outputs, &local3.outputs));

        // All see the same data
        assert_eq!(local1.get_output("shared"), Some("data"));
        assert_eq!(local2.get_output("shared"), Some("data"));
        assert_eq!(local3.get_output("shared"), Some("data"));
    }

    #[test]
    fn test_clear_with_active_snapshots() {
        let mut global = GlobalContext::new();
        global.set_output("task1", "value1".to_string());

        // Take snapshot
        let local = global.snapshot();

        // Clear global - should copy-on-write
        global.clear();

        // Global is empty
        assert_eq!(global.get_output("task1"), None);

        // Local still has original data
        assert_eq!(local.get_output("task1"), Some("value1"));
    }

    // ========================================================================
    // SECURITY TESTS - Environment Variable Blocking
    // ========================================================================

    #[test]
    fn test_env_blocks_api_keys() {
        assert!(is_env_blocked("OPENAI_API_KEY"));
        assert!(is_env_blocked("ANTHROPIC_API_KEY"));
        assert!(is_env_blocked("AWS_SECRET_ACCESS_KEY"));
        assert!(is_env_blocked("MISTRAL_API_KEY"));
    }

    #[test]
    fn test_env_blocks_tokens_and_secrets() {
        assert!(is_env_blocked("GITHUB_TOKEN"));
        assert!(is_env_blocked("MY_SECRET"));
        assert!(is_env_blocked("DATABASE_PASSWORD"));
        assert!(is_env_blocked("AUTH_TOKEN"));
    }

    #[test]
    fn test_env_blocks_cloud_credentials() {
        assert!(is_env_blocked("AWS_ACCESS_KEY_ID"));
        assert!(is_env_blocked("AZURE_SUBSCRIPTION_ID"));
        assert!(is_env_blocked("GCP_PROJECT_ID"));
        assert!(is_env_blocked("GOOGLE_APPLICATION_CREDENTIALS"));
    }

    #[test]
    fn test_env_blocks_database_urls() {
        assert!(is_env_blocked("DATABASE_URL"));
        assert!(is_env_blocked("MONGODB_URI"));
        assert!(is_env_blocked("REDIS_URL"));
        assert!(is_env_blocked("POSTGRES_PASSWORD"));
    }

    #[test]
    fn test_env_allows_safe_vars() {
        assert!(!is_env_blocked("HOME"));
        assert!(!is_env_blocked("USER"));
        assert!(!is_env_blocked("PATH"));
        assert!(!is_env_blocked("PWD"));
        assert!(!is_env_blocked("SHELL"));
        assert!(!is_env_blocked("TERM"));
        assert!(!is_env_blocked("LANG"));
        assert!(!is_env_blocked("EDITOR"));
    }

    #[test]
    fn test_env_case_insensitive() {
        assert!(is_env_blocked("openai_api_key"));
        assert!(is_env_blocked("OpenAI_API_KEY"));
        assert!(is_env_blocked("OPENAI_API_KEY"));
    }

    #[test]
    fn test_context_blocks_sensitive_env() {
        let ctx = GlobalContext::new();

        // Should return None for blocked vars
        assert!(ctx.get_env("OPENAI_API_KEY").is_none());
        assert!(ctx.get_env("AWS_SECRET_ACCESS_KEY").is_none());

        // Should allow safe vars (if they exist in the environment)
        // We just check it doesn't panic and might return Some or None
        let _ = ctx.get_env("HOME");
        let _ = ctx.get_env("PATH");
    }
}
