//! # Global and Local Context (v4.6)
//!
//! Context management for workflow execution with compile-time safety.
//!
//! ## Design
//!
//! - `GlobalContext`: Mutable context for agent: tasks (shared history)
//! - `LocalContext`: Immutable snapshot for subagent: tasks (isolated)
//! - Traits enforce compile-time safety via borrow rules

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
#[derive(Debug, Default, Clone)]
pub struct GlobalContext {
    /// Outputs from completed tasks (task_id -> output string)
    outputs: HashMap<SmartString, Arc<str>>,

    /// Structured outputs for field access (task_id -> JSON value)
    structured_outputs: HashMap<SmartString, serde_json::Value>,

    /// Main agent conversation history (for context sharing between agent: tasks)
    agent_history: Vec<AgentMessage>,

    /// Input parameters passed to the workflow
    inputs: HashMap<String, Arc<str>>,

    /// Environment variables snapshot
    env_vars: HashMap<String, Arc<str>>,
}

impl GlobalContext {
    /// Create a new global context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with input parameters
    pub fn with_inputs(inputs: HashMap<String, String>) -> Self {
        let inputs = inputs.into_iter().map(|(k, v)| (k, Arc::from(v))).collect();
        Self {
            inputs,
            ..Default::default()
        }
    }

    /// Create a read-only snapshot for isolated execution
    ///
    /// This creates a LocalContext that captures the current state
    /// but cannot modify the GlobalContext.
    pub fn snapshot(&self) -> LocalContext {
        LocalContext {
            outputs: self.outputs.clone(),
            structured_outputs: self.structured_outputs.clone(),
            inputs: self.inputs.clone(),
            env_vars: self.env_vars.clone(),
            // Note: agent_history is NOT copied - subagents get fresh history
            local_outputs: HashMap::new(),
            local_structured_outputs: HashMap::new(),
            local_history: Vec::new(),
        }
    }

    /// Store multiple outputs at once (batch operation)
    pub fn set_outputs_batch<I, K, V>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        self.outputs.extend(
            outputs
                .into_iter()
                .map(|(k, v)| (SmartString::from(k.as_ref()), Arc::from(v.into()))),
        );
    }

    /// Store multiple structured outputs at once (batch operation)
    pub fn set_structured_outputs_batch<I, K>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, serde_json::Value)>,
        K: AsRef<str>,
    {
        self.structured_outputs.extend(
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
    pub fn clear(&mut self) {
        self.outputs.clear();
        self.structured_outputs.clear();
        self.agent_history.clear();
        self.inputs.clear();
        self.env_vars.clear();
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
        self.outputs
            .insert(SmartString::from(task_id), Arc::from(output));
    }

    fn set_structured_output(&mut self, task_id: &str, value: serde_json::Value) {
        self.structured_outputs
            .insert(SmartString::from(task_id), value);
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
/// - Subagents get a COPY of outputs/inputs (read-only view of main context)
/// - Subagents get EMPTY agent_history (fresh context)
/// - Subagent writes go to local_* fields only
/// - Changes are NOT reflected back to GlobalContext automatically
#[derive(Debug, Clone)]
pub struct LocalContext {
    /// Snapshot of global outputs (read-only)
    outputs: HashMap<SmartString, Arc<str>>,

    /// Snapshot of global structured outputs (read-only)
    structured_outputs: HashMap<SmartString, serde_json::Value>,

    /// Snapshot of inputs (read-only)
    inputs: HashMap<String, Arc<str>>,

    /// Snapshot of env vars (read-only)
    env_vars: HashMap<String, Arc<str>>,

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
