//! NewType wrappers for type safety (v5.0)
//!
//! Provides zero-cost abstractions for domain types,
//! preventing type confusion and enabling rich APIs.

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

// ============================================================================
// TASK ID
// ============================================================================

/// Strongly-typed task identifier
///
/// Guarantees:
/// - Non-empty
/// - Valid characters (alphanumeric, dash, underscore)
/// - Maximum 64 characters
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(crate::smart_string::SmartString);

impl TaskId {
    /// Maximum allowed length
    pub const MAX_LENGTH: usize = 64;

    /// Create a new TaskId with validation
    pub fn new(id: impl AsRef<str>) -> Result<Self, TaskIdError> {
        let id = id.as_ref();

        // Validation
        if id.is_empty() {
            return Err(TaskIdError::Empty);
        }
        if id.len() > Self::MAX_LENGTH {
            return Err(TaskIdError::TooLong(id.len()));
        }
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(TaskIdError::InvalidCharacters(id.to_string()));
        }

        Ok(TaskId(crate::smart_string::SmartString::from(id)))
    }

    /// Get as string slice
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Deref for TaskId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaskId {
    type Err = TaskIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TaskId::new(s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TaskIdError {
    #[error("Task ID cannot be empty")]
    Empty,
    #[error("Task ID too long ({0} > {})", TaskId::MAX_LENGTH)]
    TooLong(usize),
    #[error("Task ID contains invalid characters: {0}")]
    InvalidCharacters(String),
}

// ============================================================================
// WORKFLOW NAME
// ============================================================================

/// Strongly-typed workflow name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowName(String);

impl WorkflowName {
    /// Create new workflow name with validation
    pub fn new(name: impl Into<String>) -> Result<Self, WorkflowNameError> {
        let name = name.into();

        if name.is_empty() {
            return Err(WorkflowNameError::Empty);
        }
        if name.len() > 128 {
            return Err(WorkflowNameError::TooLong(name.len()));
        }

        Ok(WorkflowName(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkflowName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowNameError {
    #[error("Workflow name cannot be empty")]
    Empty,
    #[error("Workflow name too long ({0} > 128)")]
    TooLong(usize),
}

// ============================================================================
// PROMPT
// ============================================================================

/// Strongly-typed prompt text
///
/// Guarantees non-empty, sanitized content
#[derive(Debug, Clone)]
pub struct Prompt(String);

impl Prompt {
    /// Maximum prompt length (100KB)
    pub const MAX_LENGTH: usize = 100 * 1024;

    /// Create new prompt with validation
    pub fn new(prompt: impl Into<String>) -> Result<Self, PromptError> {
        let prompt = prompt.into();

        if prompt.trim().is_empty() {
            return Err(PromptError::Empty);
        }
        if prompt.len() > Self::MAX_LENGTH {
            return Err(PromptError::TooLong(prompt.len()));
        }

        // Sanitize: normalize whitespace, remove control characters
        let sanitized = prompt
            .chars()
            .filter(|c| !c.is_control() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        Ok(Prompt(sanitized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get length in bytes
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if prompt is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Append text to prompt
    pub fn append(&mut self, text: &str) -> Result<(), PromptError> {
        let new_len = self.0.len() + text.len();
        if new_len > Self::MAX_LENGTH {
            return Err(PromptError::TooLong(new_len));
        }
        self.0.push_str(text);
        Ok(())
    }
}

impl fmt::Display for Prompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Prompt cannot be empty")]
    Empty,
    #[error("Prompt too long ({0} > {})", Prompt::MAX_LENGTH)]
    TooLong(usize),
}

// ============================================================================
// MODEL NAME
// ============================================================================

/// Strongly-typed model identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelName(Cow<'static, str>);

impl ModelName {
    /// Claude Sonnet 4.5
    pub const CLAUDE_SONNET_4_5: Self = ModelName(Cow::Borrowed("claude-sonnet-4-5"));

    /// Claude Opus 4.5
    pub const CLAUDE_OPUS_4_5: Self = ModelName(Cow::Borrowed("claude-opus-4-5"));

    /// Claude Haiku 4.5
    pub const CLAUDE_HAIKU_4_5: Self = ModelName(Cow::Borrowed("claude-haiku-4-5"));

    /// Create custom model name
    pub fn custom(name: impl Into<String>) -> Result<Self, ModelNameError> {
        let name = name.into();

        if name.is_empty() {
            return Err(ModelNameError::Empty);
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ModelNameError::InvalidCharacters(name));
        }

        Ok(ModelName(Cow::Owned(name)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelNameError {
    #[error("Model name cannot be empty")]
    Empty,
    #[error("Model name contains invalid characters: {0}")]
    InvalidCharacters(String),
}

// ============================================================================
// SHELL COMMAND
// ============================================================================

/// Strongly-typed shell command with safety checks
#[derive(Debug, Clone)]
pub struct ShellCommand {
    command: String,
    is_safe: bool,
}

impl ShellCommand {
    /// Dangerous commands that require explicit approval
    const DANGEROUS_PATTERNS: &'static [&'static str] = &[
        "rm -rf",
        "dd if=",
        "mkfs",
        "format",
        "> /dev/",
        "fork bomb",
        ":()",
    ];

    /// Create new shell command with safety check
    pub fn new(cmd: impl Into<String>) -> Result<Self, ShellCommandError> {
        let command = cmd.into();

        if command.trim().is_empty() {
            return Err(ShellCommandError::Empty);
        }

        let lower = command.to_lowercase();
        let is_dangerous = Self::DANGEROUS_PATTERNS
            .iter()
            .any(|pattern| lower.contains(pattern));

        if is_dangerous {
            return Err(ShellCommandError::Dangerous(command));
        }

        Ok(ShellCommand {
            command,
            is_safe: true,
        })
    }

    /// Create potentially dangerous command (requires explicit unsafe)
    ///
    /// # Safety
    /// Caller must ensure the command is safe to execute. This bypasses
    /// all validation and allows potentially dangerous commands.
    pub unsafe fn new_unchecked(cmd: impl Into<String>) -> Self {
        ShellCommand {
            command: cmd.into(),
            is_safe: false,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.command
    }

    pub fn is_safe(&self) -> bool {
        self.is_safe
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellCommandError {
    #[error("Shell command cannot be empty")]
    Empty,
    #[error("Potentially dangerous command detected: {0}")]
    Dangerous(String),
}

// ============================================================================
// URL
// ============================================================================

/// Strongly-typed URL with validation
#[derive(Debug, Clone)]
pub struct Url {
    url: String,
    scheme: UrlScheme,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UrlScheme {
    Http,
    Https,
    File,
}

impl Url {
    /// Create new URL with validation
    pub fn new(url: impl Into<String>) -> Result<Self, UrlError> {
        let url = url.into();

        // Basic validation
        if url.is_empty() {
            return Err(UrlError::Empty);
        }

        // Parse scheme
        let scheme = if url.starts_with("https://") {
            UrlScheme::Https
        } else if url.starts_with("http://") {
            UrlScheme::Http
        } else if url.starts_with("file://") {
            UrlScheme::File
        } else {
            return Err(UrlError::InvalidScheme(url));
        };

        // Additional validation could go here (e.g., valid host, port)

        Ok(Url { url, scheme })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    pub fn scheme(&self) -> &UrlScheme {
        &self.scheme
    }

    pub fn is_secure(&self) -> bool {
        matches!(self.scheme, UrlScheme::Https | UrlScheme::File)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UrlError {
    #[error("URL cannot be empty")]
    Empty,
    #[error("Invalid URL scheme: {0}")]
    InvalidScheme(String),
}

// ============================================================================
// TOKEN COUNT
// ============================================================================

/// Strongly-typed token count (always non-negative)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TokenCount(u32);

impl TokenCount {
    /// Zero tokens
    pub const ZERO: Self = TokenCount(0);

    /// Create new token count
    pub const fn new(count: u32) -> Self {
        TokenCount(count)
    }

    /// Get as u32
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Add token counts (saturating)
    pub fn saturating_add(self, other: Self) -> Self {
        TokenCount(self.0.saturating_add(other.0))
    }

    /// Subtract token counts (saturating)
    pub fn saturating_sub(self, other: Self) -> Self {
        TokenCount(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for TokenCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for TokenCount {
    fn from(value: u32) -> Self {
        TokenCount(value)
    }
}

impl From<TokenCount> for u32 {
    fn from(tokens: TokenCount) -> Self {
        tokens.0
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_validation() {
        // Valid IDs
        assert!(TaskId::new("task-1").is_ok());
        assert!(TaskId::new("my_task_123").is_ok());
        assert!(TaskId::new("UPPERCASE").is_ok());

        // Invalid IDs
        assert!(TaskId::new("").is_err());
        assert!(TaskId::new("task with spaces").is_err());
        assert!(TaskId::new("task@123").is_err());
        assert!(TaskId::new("x".repeat(65)).is_err());
    }

    #[test]
    fn test_prompt_sanitization() {
        let prompt = Prompt::new("  Multiple   spaces   are   normalized  ").unwrap();
        assert_eq!(prompt.as_str(), "Multiple spaces are normalized");

        let prompt = Prompt::new("Line\nbreaks\nbecome spaces").unwrap();
        assert_eq!(prompt.as_str(), "Line breaks become spaces");
    }

    #[test]
    fn test_shell_command_safety() {
        // Safe commands
        assert!(ShellCommand::new("ls -la").is_ok());
        assert!(ShellCommand::new("echo hello").is_ok());

        // Dangerous commands
        assert!(ShellCommand::new("rm -rf /").is_err());
        assert!(ShellCommand::new("dd if=/dev/zero of=/dev/sda").is_err());

        // Can create dangerous with unsafe
        let cmd = unsafe { ShellCommand::new_unchecked("rm -rf /") };
        assert!(!cmd.is_safe());
    }

    #[test]
    fn test_url_validation() {
        // Valid URLs
        let url = Url::new("https://example.com").unwrap();
        assert!(url.is_secure());

        let url = Url::new("http://localhost:8080").unwrap();
        assert!(!url.is_secure());

        // Invalid URLs
        assert!(Url::new("").is_err());
        assert!(Url::new("not-a-url").is_err());
        assert!(Url::new("ftp://example.com").is_err());
    }

    #[test]
    fn test_token_count_arithmetic() {
        let a = TokenCount::new(100);
        let b = TokenCount::new(50);

        assert_eq!(a.saturating_add(b), TokenCount::new(150));
        assert_eq!(a.saturating_sub(b), TokenCount::new(50));
        assert_eq!(b.saturating_sub(a), TokenCount::ZERO);

        // Saturation
        let max = TokenCount::new(u32::MAX);
        assert_eq!(max.saturating_add(TokenCount::new(1)), max);
    }
}
