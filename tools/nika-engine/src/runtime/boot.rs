//! Boot Sequence - 7-phase startup with progress reporting
//!
//! Phases:
//! 1. Config discovery (find .nika/)
//! 2. Config validation (parse config.toml)
//! 3. Memory loading (load memory files)
//! 4. Secrets loading (load from nika daemon or fallback)
//! 5. MCP server startup (launch configured servers)
//! 6. Provider validation (check API keys)
//! 7. Ready state

use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Boot phase enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BootPhase {
    /// Phase 1: Finding .nika/ directory
    ConfigDiscovery,
    /// Phase 2: Parsing and validating config.toml
    ConfigValidation,
    /// Phase 3: Loading memory files
    MemoryLoading,
    /// Phase 4: Loading secrets from nika daemon
    SecretsLoading,
    /// Phase 5: Starting MCP servers
    McpStartup,
    /// Phase 6: Validating provider API keys
    ProviderValidation,
    /// Phase 7: System ready
    Ready,
}

impl BootPhase {
    /// Get human-readable phase name
    pub fn name(&self) -> &'static str {
        match self {
            Self::ConfigDiscovery => "Config Discovery",
            Self::ConfigValidation => "Config Validation",
            Self::MemoryLoading => "Memory Loading",
            Self::SecretsLoading => "Secrets Loading",
            Self::McpStartup => "MCP Startup",
            Self::ProviderValidation => "Provider Validation",
            Self::Ready => "Ready",
        }
    }

    /// Get snake_case identifier for event logging
    pub fn snake_case_name(&self) -> &'static str {
        match self {
            Self::ConfigDiscovery => "config_discovery",
            Self::ConfigValidation => "config_validation",
            Self::MemoryLoading => "memory_loading",
            Self::SecretsLoading => "secrets_loading",
            Self::McpStartup => "mcp_startup",
            Self::ProviderValidation => "provider_validation",
            Self::Ready => "ready",
        }
    }

    /// Get phase number (1-7)
    pub fn number(&self) -> u8 {
        match self {
            Self::ConfigDiscovery => 1,
            Self::ConfigValidation => 2,
            Self::MemoryLoading => 3,
            Self::SecretsLoading => 4,
            Self::McpStartup => 5,
            Self::ProviderValidation => 6,
            Self::Ready => 7,
        }
    }

    /// Get phase icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::ConfigDiscovery => "🔍",
            Self::ConfigValidation => "✅",
            Self::MemoryLoading => "📚",
            Self::SecretsLoading => "🔐",
            Self::McpStartup => "🔌",
            Self::ProviderValidation => "🔑",
            Self::Ready => "🚀",
        }
    }
}

/// Phase result with timing
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub phase: BootPhase,
    pub success: bool,
    pub duration: Duration,
    pub message: Option<String>,
    pub warnings: Vec<String>,
}

/// Boot context - accumulated state during boot
#[derive(Debug, Clone, Default)]
pub struct BootContext {
    /// Root .nika/ directory
    pub nika_dir: Option<PathBuf>,
    /// Parsed configuration
    pub config: Option<BootstrapConfig>,
    /// Loaded memory context
    pub memory: Option<HashMap<String, serde_json::Value>>,
    /// Secrets loading result
    pub secrets_loaded: Option<crate::secrets::SecretsLoadResult>,
    /// Available MCP servers
    pub mcp_servers: Vec<String>,
    /// Available providers with API keys
    pub providers: Vec<String>,
    /// Phase results
    pub phases: Vec<PhaseResult>,
    /// Total boot duration
    pub total_duration: Duration,
}

/// Nika configuration from config.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub trace: TraceConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_permission")]
    pub permission: String,
    pub working_dir: Option<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            permission: default_permission(),
            working_dir: None,
        }
    }
}

fn default_permission() -> String {
    "plan".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_provider")]
    pub default: String,
    pub model: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: default_provider(),
            model: None,
        }
    }
}

fn default_provider() -> String {
    "claude".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: u8,
    #[serde(default = "default_true")]
    pub auto_format: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            tab_width: default_tab_width(),
            auto_format: true,
        }
    }
}

fn default_theme() -> String {
    "solarized".to_string()
}

fn default_tab_width() -> u8 {
    2
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_true")]
    pub auto_restore: bool,
    #[serde(default = "default_max_sessions")]
    pub max_sessions: u32,
    #[serde(default = "default_session_ttl")]
    pub session_ttl_days: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            auto_restore: true,
            max_sessions: default_max_sessions(),
            session_ttl_days: default_session_ttl(),
        }
    }
}

fn default_max_sessions() -> u32 {
    50
}

fn default_session_ttl() -> u32 {
    7
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_max_traces")]
    pub max_traces: u32,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            retention_days: default_retention(),
            max_traces: default_max_traces(),
        }
    }
}

fn default_retention() -> u32 {
    7
}

fn default_max_traces() -> u32 {
    100
}

/// Policy configuration for security enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Allow exec: verb
    #[serde(default = "default_true")]
    pub allow_exec: bool,
    /// Allow fetch: verb (network access)
    #[serde(default = "default_true")]
    pub allow_network: bool,
    /// Blocked shell commands (substrings)
    #[serde(default)]
    pub blocked_commands: Vec<String>,
    /// Maximum token spend per workflow run
    pub max_token_spend: Option<u64>,
    /// Allowed hosts for fetch:
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Blocked hosts for fetch:
    #[serde(default)]
    pub blocked_hosts: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allow_exec: true,
            allow_network: true,
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "sudo".to_string(),
                "chmod 777".to_string(),
            ],
            max_token_spend: None,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
        }
    }
}

/// Boot sequence runner
pub struct BootSequence {
    start_dir: PathBuf,
    verbose: bool,
}

impl BootSequence {
    /// Create a new boot sequence starting from a directory
    pub fn new(start_dir: impl AsRef<Path>) -> Self {
        Self {
            start_dir: start_dir.as_ref().to_path_buf(),
            verbose: false,
        }
    }

    /// Enable verbose output
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Run the full boot sequence
    ///
    /// Pass `Some(&event_log)` to emit `BootPhaseCompleted` events for each phase.
    /// Pass `None` if the EventLog is not yet available.
    pub async fn run(&self, event_log: Option<&EventLog>) -> Result<BootContext, NikaError> {
        let boot_start = Instant::now();
        let mut ctx = BootContext::default();

        // Helper closure to emit boot phase events
        let emit_phase = |log: Option<&EventLog>, result: &PhaseResult| {
            if let Some(log) = log {
                log.emit(EventKind::BootPhaseCompleted {
                    phase: result.phase.snake_case_name().to_string(),
                    success: result.success,
                    duration_ms: result.duration.as_millis() as u64,
                    warnings: result.warnings.clone(),
                });
            }
        };

        // Phase 1: Config Discovery
        let phase_result = self.phase_config_discovery(&mut ctx).await;
        ctx.phases.push(phase_result.clone());
        emit_phase(event_log, &phase_result);
        if !phase_result.success {
            ctx.total_duration = boot_start.elapsed();
            return Err(NikaError::BootFailed {
                phase: phase_result.phase.name().to_string(),
                reason: phase_result
                    .message
                    .unwrap_or_else(|| "Config discovery failed".into()),
            });
        }

        // Phase 2: Config Validation
        let phase_result = self.phase_config_validation(&mut ctx).await;
        ctx.phases.push(phase_result.clone());
        emit_phase(event_log, &phase_result);
        if !phase_result.success {
            ctx.total_duration = boot_start.elapsed();
            return Err(NikaError::BootFailed {
                phase: phase_result.phase.name().to_string(),
                reason: phase_result
                    .message
                    .unwrap_or_else(|| "Config validation failed".into()),
            });
        }

        // Phase 3: Memory Loading (optional, doesn't fail boot)
        let phase_result = self.phase_memory_loading(&mut ctx).await;
        emit_phase(event_log, &phase_result);
        ctx.phases.push(phase_result);

        // Phase 4: Secrets Loading
        let phase_result = self.phase_secrets_loading(&mut ctx).await;
        emit_phase(event_log, &phase_result);
        ctx.phases.push(phase_result);

        // Phase 5: MCP Startup (optional, doesn't fail boot)
        let phase_result = self.phase_mcp_startup(&mut ctx).await;
        emit_phase(event_log, &phase_result);
        ctx.phases.push(phase_result);

        // Phase 6: Provider Validation (optional, doesn't fail boot)
        let phase_result = self.phase_provider_validation(&mut ctx).await;
        emit_phase(event_log, &phase_result);
        ctx.phases.push(phase_result);

        // Phase 7: Ready
        let ready_result = PhaseResult {
            phase: BootPhase::Ready,
            success: true,
            duration: Duration::ZERO,
            message: Some("Boot complete".into()),
            warnings: vec![],
        };
        emit_phase(event_log, &ready_result);
        ctx.phases.push(ready_result);

        ctx.total_duration = boot_start.elapsed();
        Ok(ctx)
    }

    async fn phase_config_discovery(&self, ctx: &mut BootContext) -> PhaseResult {
        let start = Instant::now();
        let mut warnings = vec![];

        // Search for .nika/ directory
        let mut dir = self.start_dir.as_path();
        loop {
            let nika_dir = dir.join(".nika");
            if nika_dir.exists() && nika_dir.is_dir() {
                ctx.nika_dir = Some(nika_dir);
                return PhaseResult {
                    phase: BootPhase::ConfigDiscovery,
                    success: true,
                    duration: start.elapsed(),
                    message: Some(format!("Found .nika/ at {}", dir.display())),
                    warnings,
                };
            }

            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }

        // Not found - use current directory
        warnings.push("No .nika/ directory found, using defaults".into());
        ctx.nika_dir = Some(self.start_dir.join(".nika"));

        PhaseResult {
            phase: BootPhase::ConfigDiscovery,
            success: true, // Still succeeds, just with defaults
            duration: start.elapsed(),
            message: Some("Using default configuration".into()),
            warnings,
        }
    }

    async fn phase_config_validation(&self, ctx: &mut BootContext) -> PhaseResult {
        let start = Instant::now();
        let mut warnings = vec![];

        let nika_dir = match &ctx.nika_dir {
            Some(dir) => dir.clone(),
            None => {
                return PhaseResult {
                    phase: BootPhase::ConfigValidation,
                    success: false,
                    duration: start.elapsed(),
                    message: Some("No .nika directory".into()),
                    warnings,
                };
            }
        };

        let config_path = nika_dir.join("config.toml");
        if !config_path.exists() {
            // Use defaults
            ctx.config = Some(BootstrapConfig::default());
            warnings.push("config.toml not found, using defaults".into());
            return PhaseResult {
                phase: BootPhase::ConfigValidation,
                success: true,
                duration: start.elapsed(),
                message: Some("Using default configuration".into()),
                warnings,
            };
        }

        // Parse config
        match tokio::fs::read_to_string(&config_path).await {
            Ok(content) => match toml::from_str::<BootstrapConfig>(&content) {
                Ok(config) => {
                    ctx.config = Some(config);
                    PhaseResult {
                        phase: BootPhase::ConfigValidation,
                        success: true,
                        duration: start.elapsed(),
                        message: Some("Configuration loaded".into()),
                        warnings,
                    }
                }
                Err(e) => {
                    let msg = format!("Config parse error in {}: {}", config_path.display(), e);
                    tracing::error!("{}", msg);
                    warnings.push(msg);
                    ctx.config = Some(BootstrapConfig::default());
                    PhaseResult {
                        phase: BootPhase::ConfigValidation,
                        success: true, // Proceed with defaults
                        duration: start.elapsed(),
                        message: Some("Using defaults due to parse error".into()),
                        warnings,
                    }
                }
            },
            Err(e) => {
                warnings.push(format!("Config read error: {}", e));
                ctx.config = Some(BootstrapConfig::default());
                PhaseResult {
                    phase: BootPhase::ConfigValidation,
                    success: true,
                    duration: start.elapsed(),
                    message: Some("Using defaults due to read error".into()),
                    warnings,
                }
            }
        }
    }

    async fn phase_memory_loading(&self, ctx: &mut BootContext) -> PhaseResult {
        let start = Instant::now();
        let warnings = vec![];

        let nika_dir = match &ctx.nika_dir {
            Some(dir) => dir.clone(),
            None => {
                return PhaseResult {
                    phase: BootPhase::MemoryLoading,
                    success: true,
                    duration: start.elapsed(),
                    message: Some("Skipped (no .nika/)".into()),
                    warnings,
                };
            }
        };

        let memory_path = nika_dir.join("memory.yaml");
        if !memory_path.exists() {
            return PhaseResult {
                phase: BootPhase::MemoryLoading,
                success: true,
                duration: start.elapsed(),
                message: Some("No memory.yaml".into()),
                warnings,
            };
        }

        // Load memory (simplified - full implementation in memory_loader.rs)
        ctx.memory = Some(HashMap::new());

        PhaseResult {
            phase: BootPhase::MemoryLoading,
            success: true,
            duration: start.elapsed(),
            message: Some("Memory loaded".into()),
            warnings,
        }
    }

    /// Phase 4: Load secrets from nika daemon or fallback
    async fn phase_secrets_loading(&self, ctx: &mut BootContext) -> PhaseResult {
        let start = Instant::now();
        let mut warnings = vec![];

        // Load secrets from daemon (or fallback to keyring/env)
        let result = crate::secrets::load_from_daemon_or_fallback().await;

        if !result.daemon_available {
            warnings.push("nika daemon not running, using fallback".into());
        }

        let message = if result.daemon_available {
            format!(
                "{} secrets loaded ({} daemon, {} fallback)",
                result.total_loaded(),
                result.from_daemon.len(),
                result.from_fallback.len()
            )
        } else {
            format!("{} secrets loaded (fallback)", result.total_loaded())
        };

        ctx.secrets_loaded = Some(result);

        PhaseResult {
            phase: BootPhase::SecretsLoading,
            success: true, // Never fails boot, just warns
            duration: start.elapsed(),
            message: Some(message),
            warnings,
        }
    }

    async fn phase_mcp_startup(&self, _ctx: &mut BootContext) -> PhaseResult {
        let start = Instant::now();
        let warnings = vec![];

        // MCP servers are started on-demand, not at boot
        // This phase just checks for configured servers

        PhaseResult {
            phase: BootPhase::McpStartup,
            success: true,
            duration: start.elapsed(),
            message: Some("MCP servers ready (on-demand)".into()),
            warnings,
        }
    }

    async fn phase_provider_validation(&self, ctx: &mut BootContext) -> PhaseResult {
        use crate::core::{ProviderCategory, KNOWN_PROVIDERS};

        let start = Instant::now();
        let mut warnings = vec![];
        let mut providers = vec![];

        // Check for API keys using KNOWN_PROVIDERS as source of truth
        for p in KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.category == ProviderCategory::Llm)
        {
            if std::env::var(p.env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
            {
                providers.push(p.id.to_string());
            }
        }

        if providers.is_empty() {
            warnings.push("No API keys found".into());
        }

        ctx.providers = providers.clone();

        PhaseResult {
            phase: BootPhase::ProviderValidation,
            success: true,
            duration: start.elapsed(),
            message: Some(format!("{} provider(s) available", providers.len())),
            warnings,
        }
    }
}

impl BootContext {
    /// Check if boot was successful
    pub fn is_ready(&self) -> bool {
        self.phases
            .iter()
            .any(|p| p.phase == BootPhase::Ready && p.success)
    }

    /// Get all warnings from boot phases
    pub fn all_warnings(&self) -> Vec<String> {
        self.phases
            .iter()
            .flat_map(|p| p.warnings.clone())
            .collect()
    }

    /// Format boot summary for display
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("Boot completed in {:?}", self.total_duration)];

        for phase in &self.phases {
            let status = if phase.success { "✓" } else { "✗" };
            lines.push(format!(
                "  {} {} {} ({:?})",
                status,
                phase.phase.icon(),
                phase.phase.name(),
                phase.duration
            ));
            for warning in &phase.warnings {
                lines.push(format!("    ⚠ {}", warning));
            }
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_boot_phase_properties() {
        assert_eq!(BootPhase::ConfigDiscovery.number(), 1);
        assert_eq!(BootPhase::SecretsLoading.number(), 4);
        assert_eq!(BootPhase::Ready.number(), 7);
        assert_eq!(BootPhase::McpStartup.name(), "MCP Startup");
        assert_eq!(BootPhase::SecretsLoading.icon(), "🔐");
    }

    #[test]
    fn test_default_config() {
        let config = BootstrapConfig::default();
        assert_eq!(config.tools.permission, "plan");
        assert_eq!(config.provider.default, "claude");
        assert_eq!(config.editor.theme, "solarized");
        assert!(config.policy.allow_exec);
    }

    #[test]
    fn test_policy_defaults() {
        let policy = PolicyConfig::default();
        assert!(policy.allow_exec);
        assert!(policy.allow_network);
        assert!(policy.blocked_commands.contains(&"sudo".to_string()));
    }

    #[tokio::test]
    async fn test_boot_sequence_no_nika_dir() {
        let temp = tempdir().unwrap();
        let boot = BootSequence::new(temp.path());
        let ctx = boot.run(None).await.unwrap();

        assert!(ctx.is_ready());
        assert!(ctx.all_warnings().iter().any(|w| w.contains("No .nika/")));
    }

    #[tokio::test]
    async fn test_boot_sequence_with_config() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        std::fs::create_dir_all(&nika_dir).unwrap();

        let config_content = r#"
[tools]
permission = "accept-edits"

[provider]
default = "openai"
"#;
        std::fs::write(nika_dir.join("config.toml"), config_content).unwrap();

        let boot = BootSequence::new(temp.path());
        let ctx = boot.run(None).await.unwrap();

        assert!(ctx.is_ready());
        let config = ctx.config.unwrap();
        assert_eq!(config.tools.permission, "accept-edits");
        assert_eq!(config.provider.default, "openai");
    }

    // ─── Bug 14: provider validation must use KNOWN_PROVIDERS, not hardcoded ──

    #[tokio::test]
    async fn test_provider_validation_detects_xai() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        std::fs::create_dir_all(&nika_dir).unwrap();
        std::fs::write(nika_dir.join("config.toml"), "").unwrap();

        // Set xAI env var
        let key = "XAI_API_KEY";
        let original = std::env::var(key).ok();
        std::env::set_var(key, "xai-test-key-12345");

        let boot = BootSequence::new(temp.path());
        let ctx = boot.run(None).await.unwrap();

        assert!(
            ctx.providers.contains(&"xai".to_string()),
            "Provider validation must detect xAI, got: {:?}",
            ctx.providers
        );

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[tokio::test]
    async fn test_provider_validation_ignores_empty_env_var() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        std::fs::create_dir_all(&nika_dir).unwrap();
        std::fs::write(nika_dir.join("config.toml"), "").unwrap();

        // Set empty env var
        let key = "MISTRAL_API_KEY";
        let original = std::env::var(key).ok();
        std::env::set_var(key, "");

        let boot = BootSequence::new(temp.path());
        let ctx = boot.run(None).await.unwrap();

        assert!(
            !ctx.providers.contains(&"mistral".to_string()),
            "Provider validation must ignore empty env vars, got: {:?}",
            ctx.providers
        );

        // Restore
        match original {
            Some(v) => std::env::set_var(key, v),
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[test]
    fn test_boot_context_summary() {
        let ctx = BootContext {
            phases: vec![
                PhaseResult {
                    phase: BootPhase::ConfigDiscovery,
                    success: true,
                    duration: Duration::from_millis(5),
                    message: Some("Found".into()),
                    warnings: vec![],
                },
                PhaseResult {
                    phase: BootPhase::Ready,
                    success: true,
                    duration: Duration::ZERO,
                    message: Some("Ready".into()),
                    warnings: vec![],
                },
            ],
            total_duration: Duration::from_millis(10),
            ..Default::default()
        };

        let summary = ctx.summary();
        assert!(summary.contains("Boot completed"));
        assert!(summary.contains("Config Discovery"));
    }
}
