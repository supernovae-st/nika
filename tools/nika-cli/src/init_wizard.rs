//! Interactive init wizard using cliclack guide-rail pattern
//!
//! Provides a branded, step-by-step setup experience for `nika init`.
//! Auto-detects LLM providers, lets the user choose a mode (project, course,
//! or scaffold), and generates the corresponding files.
//!
//! ## Non-interactive mode
//!
//! When `--yes` is passed, `run_init_wizard(yes: true)` skips all prompts and
//! uses sensible defaults (project mode, plan permission, first detected provider).

use std::env;
use std::fmt;

use cliclack::{self, Theme, ThemeState};
use console::Style;

use nika_engine::error::NikaError;

// ─── Provider detection ─────────────────────────────────────────────────────

/// Known cloud LLM providers and their environment variable keys.
const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        name: "Claude",
        env_var: "ANTHROPIC_API_KEY",
        slug: "claude",
    },
    ProviderInfo {
        name: "OpenAI",
        env_var: "OPENAI_API_KEY",
        slug: "openai",
    },
    ProviderInfo {
        name: "Mistral",
        env_var: "MISTRAL_API_KEY",
        slug: "mistral",
    },
    ProviderInfo {
        name: "Groq",
        env_var: "GROQ_API_KEY",
        slug: "groq",
    },
    ProviderInfo {
        name: "DeepSeek",
        env_var: "DEEPSEEK_API_KEY",
        slug: "deepseek",
    },
    ProviderInfo {
        name: "Gemini",
        env_var: "GEMINI_API_KEY",
        slug: "gemini",
    },
    ProviderInfo {
        name: "xAI",
        env_var: "XAI_API_KEY",
        slug: "xai",
    },
];

/// Static metadata for a cloud LLM provider.
#[derive(Debug, Clone, Copy)]
pub struct ProviderInfo {
    /// Human-readable name (e.g. "Claude")
    pub name: &'static str,
    /// Environment variable that holds the API key
    pub env_var: &'static str,
    /// Short identifier used in config.toml (e.g. "claude")
    pub slug: &'static str,
}

/// Result of scanning the environment for provider API keys.
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub info: &'static ProviderInfo,
    pub available: bool,
}

/// Scan the environment for known LLM provider API keys.
///
/// Returns a list of all providers with their availability status.
pub fn detect_providers() -> Vec<DetectedProvider> {
    PROVIDERS
        .iter()
        .map(|info| {
            let available = env::var(info.env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            DetectedProvider { info, available }
        })
        .collect()
}

/// Return the slug of the first available provider, or `None` if none found.
pub fn first_available_provider(detected: &[DetectedProvider]) -> Option<&'static str> {
    detected.iter().find(|d| d.available).map(|d| d.info.slug)
}

// ─── Init mode ──────────────────────────────────────────────────────────────

/// The three modes available from the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    /// Full project scaffold with .nika/, workflows, context, schemas
    Project,
    /// Interactive 12-level course (44 exercises)
    Course,
    /// Minimal scaffold (.nika/ config only, no example workflows)
    Scaffold,
}

impl fmt::Display for InitMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project => write!(f, "Start a project"),
            Self::Course => write!(f, "Learn Nika (interactive course)"),
            Self::Scaffold => write!(f, "Quick scaffold"),
        }
    }
}

// ─── NikaTheme ──────────────────────────────────────────────────────────────

/// Custom cliclack theme with Nika branding.
///
/// - Magenta guide-rail bar
/// - Butterfly info symbol
/// - Braille spinner characters
pub struct NikaTheme;

impl Theme for NikaTheme {
    /// Magenta bar color for all states (the guide rail).
    fn bar_color(&self, _state: &ThemeState) -> Style {
        Style::new().magenta()
    }

    /// Symbol color matches bar: magenta for active, green for submit,
    /// red for cancel/error.
    fn state_symbol_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active => Style::new().magenta(),
            ThemeState::Submit => Style::new().green(),
            ThemeState::Cancel => Style::new().red(),
            ThemeState::Error(_) => Style::new().red(),
        }
    }

    /// Use butterfly as the info symbol.
    fn info_symbol(&self) -> String {
        "\u{1F98B}".to_string() // butterfly emoji
    }

    /// Braille spinner characters for a distinctive loading animation.
    fn spinner_chars(&self) -> String {
        "\u{2801}\u{2802}\u{2804}\u{2840}\u{2880}\u{2820}\u{2810}\u{2808}".to_string()
    }
}

// ─── Wizard ─────────────────────────────────────────────────────────────────

/// Wizard result passed to the caller to dispatch generation.
#[derive(Debug, Clone)]
pub struct WizardResult {
    /// Which mode the user selected
    pub mode: InitMode,
    /// Permission mode string (e.g. "plan", "accept-edits")
    pub permission: String,
    /// Course destination directory (only set when mode == Course)
    pub course_dest: Option<String>,
    /// Detected providers snapshot
    pub detected_providers: Vec<DetectedProvider>,
    /// Whether to migrate env keys to keychain
    pub migrate_keys: bool,
    /// Whether to run `nika setup editors` after init
    pub setup_editors: bool,
    /// Whether to run `nika setup ai` after init
    pub setup_ai: bool,
}

/// Run the interactive init wizard.
///
/// When `yes` is true, skips all prompts and uses defaults:
/// - Mode: Project
/// - Permission: plan
/// - First detected provider as default
///
/// Returns a `WizardResult` that the caller uses to dispatch generation,
/// or a `NikaError` on failure / Ctrl+C cancellation.
pub fn run_init_wizard(yes: bool) -> Result<WizardResult, NikaError> {
    // Install our custom theme
    cliclack::set_theme(NikaTheme);

    let detected = detect_providers();
    let available_count = detected.iter().filter(|d| d.available).count();

    // ── Non-interactive fast path ───────────────────────────────────────
    if yes {
        return Ok(WizardResult {
            mode: InitMode::Project,
            permission: "plan".to_string(),
            course_dest: None,
            detected_providers: detected,
            migrate_keys: false,
            setup_editors: false,
            setup_ai: false,
        });
    }

    // ── Step 1: Intro ───────────────────────────────────────────────────
    cliclack::intro(format!(
        "{} {} {}",
        Style::new().magenta().bold().apply_to("nika"),
        Style::new()
            .dim()
            .apply_to(format!("v{}", env!("CARGO_PKG_VERSION"))),
        Style::new().dim().apply_to("// semantic workflow engine"),
    ))
    .map_err(NikaError::IoError)?;

    // ── Step 2: Provider detection ──────────────────────────────────────
    let mut status_lines = String::new();
    for d in &detected {
        let (icon, style) = if d.available {
            ("\u{2713}", Style::new().green()) // checkmark
        } else {
            ("\u{2717}", Style::new().red().dim()) // cross
        };
        let label = if d.available {
            format!("{} {} ({})", icon, d.info.name, d.info.env_var)
        } else {
            format!("{} {} (not set)", icon, d.info.name)
        };
        status_lines.push_str(&format!("{}\n", style.apply_to(label)));
    }
    // Trim trailing newline
    let status_lines = status_lines.trim_end();

    let summary = if available_count == 0 {
        "No providers detected. You can still use exec: and fetch: verbs."
    } else if available_count == 1 {
        "1 provider detected."
    } else {
        // We'll format this dynamically below
        ""
    };

    let provider_note = if available_count > 1 {
        format!("{available_count} providers detected.")
    } else {
        summary.to_string()
    };

    cliclack::note(provider_note, status_lines).map_err(NikaError::IoError)?;

    // ── Step 3: Mode selection ──────────────────────────────────────────
    let mode: InitMode = cliclack::select("What do you want to do?")
        .item(
            InitMode::Project,
            "Start a project",
            "Full scaffold with examples",
        )
        .item(
            InitMode::Course,
            "Learn Nika (interactive course)",
            "12 levels, 44 exercises",
        )
        .item(
            InitMode::Scaffold,
            "Quick scaffold",
            "Config only, no examples",
        )
        .interact()
        .map_err(NikaError::IoError)?;

    // ── Step 4: Mode-specific questions ─────────────────────────────────
    let mut permission = "plan".to_string();
    let mut course_dest: Option<String> = None;

    match mode {
        InitMode::Project | InitMode::Scaffold => {
            // Ask permission mode
            permission = cliclack::select("Permission mode for file tools?")
                .item("plan", "Plan", "Review before execute (recommended)")
                .item("accept-edits", "Accept Edits", "Auto-approve file edits")
                .item("deny", "Deny", "Block all file operations")
                .interact()
                .map(|s: &str| s.to_string())
                .map_err(NikaError::IoError)?;
        }
        InitMode::Course => {
            // Ask destination directory
            let dest: String = cliclack::input("Course destination directory?")
                .default_input("./nika-course")
                .interact()
                .map_err(NikaError::IoError)?;
            course_dest = Some(dest);
        }
    }

    // ── Step 5: Migrate keys? ───────────────────────────────────────────
    let migrate_keys = if available_count > 0 {
        cliclack::confirm("Migrate API keys from env vars to system keychain?")
            .initial_value(false)
            .interact()
            .map_err(NikaError::IoError)?
    } else {
        false
    };

    // ── Step 6: Editor & AI setup ──────────────────────────────────────
    let setup_editors = cliclack::confirm("Install editor extension? (VS Code / Cursor)")
        .initial_value(true)
        .interact()
        .map_err(NikaError::IoError)?;

    let setup_ai = cliclack::confirm("Install AI coding rules? (Claude Code, Cursor, Copilot)")
        .initial_value(true)
        .interact()
        .map_err(NikaError::IoError)?;

    // ── Step 7: Outro ───────────────────────────────────────────────────
    // The actual generation happens in the caller after we return.
    // We show the outro there, after the spinner completes.

    Ok(WizardResult {
        mode,
        permission,
        course_dest,
        detected_providers: detected,
        migrate_keys,
        setup_editors,
        setup_ai,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Provider detection ──────────────────────────────────────────────

    #[test]
    fn test_providers_count() {
        assert_eq!(PROVIDERS.len(), 7, "Should detect 7 known providers");
    }

    #[test]
    fn test_provider_slugs_unique() {
        let mut slugs: Vec<&str> = PROVIDERS.iter().map(|p| p.slug).collect();
        let len = slugs.len();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), len, "All provider slugs must be unique");
    }

    #[test]
    fn test_provider_env_vars_unique() {
        let mut vars: Vec<&str> = PROVIDERS.iter().map(|p| p.env_var).collect();
        let len = vars.len();
        vars.sort();
        vars.dedup();
        assert_eq!(vars.len(), len, "All env vars must be unique");
    }

    #[test]
    fn test_detect_providers_returns_all() {
        let detected = detect_providers();
        assert_eq!(
            detected.len(),
            7,
            "detect_providers must return all 7 providers"
        );
    }

    #[test]
    fn test_detect_providers_with_env_set() {
        // Temporarily set one env var
        env::set_var("ANTHROPIC_API_KEY", "sk-test-123");
        let detected = detect_providers();
        let claude = detected.iter().find(|d| d.info.slug == "claude").unwrap();
        assert!(
            claude.available,
            "Claude should be available when env is set"
        );
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_detect_providers_empty_var_not_available() {
        env::set_var("OPENAI_API_KEY", "");
        let detected = detect_providers();
        let openai = detected.iter().find(|d| d.info.slug == "openai").unwrap();
        assert!(
            !openai.available,
            "Empty env var should not count as available"
        );
        env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_first_available_provider_none() {
        // With no env vars set, first_available should be None
        // (unless the test runner has real keys set)
        let detected = detect_providers();
        let none_detected = detected
            .iter()
            .map(|d| DetectedProvider {
                info: d.info,
                available: false,
            })
            .collect::<Vec<_>>();
        assert!(first_available_provider(&none_detected).is_none());
    }

    #[test]
    fn test_first_available_provider_order() {
        // Mock: only Groq and xAI available
        let detected: Vec<DetectedProvider> = PROVIDERS
            .iter()
            .map(|info| DetectedProvider {
                info,
                available: info.slug == "groq" || info.slug == "xai",
            })
            .collect();
        // Groq comes before xAI in PROVIDERS, so it should be first
        assert_eq!(first_available_provider(&detected), Some("groq"));
    }

    // ── InitMode ────────────────────────────────────────────────────────

    #[test]
    fn test_init_mode_display() {
        assert_eq!(InitMode::Project.to_string(), "Start a project");
        assert_eq!(
            InitMode::Course.to_string(),
            "Learn Nika (interactive course)"
        );
        assert_eq!(InitMode::Scaffold.to_string(), "Quick scaffold");
    }

    #[test]
    fn test_init_mode_equality() {
        assert_eq!(InitMode::Project, InitMode::Project);
        assert_ne!(InitMode::Project, InitMode::Course);
    }

    // ── NikaTheme ───────────────────────────────────────────────────────

    #[test]
    fn test_theme_bar_color_is_magenta() {
        let theme = NikaTheme;
        // We cannot directly inspect the Style, but we can verify it does not panic
        // and returns a value for every state.
        let _ = theme.bar_color(&ThemeState::Active);
        let _ = theme.bar_color(&ThemeState::Submit);
        let _ = theme.bar_color(&ThemeState::Cancel);
        let _ = theme.bar_color(&ThemeState::Error("test".to_string()));
    }

    #[test]
    fn test_theme_state_symbol_color_all_states() {
        let theme = NikaTheme;
        let _ = theme.state_symbol_color(&ThemeState::Active);
        let _ = theme.state_symbol_color(&ThemeState::Submit);
        let _ = theme.state_symbol_color(&ThemeState::Cancel);
        let _ = theme.state_symbol_color(&ThemeState::Error("err".to_string()));
    }

    #[test]
    fn test_theme_info_symbol_is_butterfly() {
        let theme = NikaTheme;
        let sym = theme.info_symbol();
        assert!(
            sym.contains('\u{1F98B}'),
            "Info symbol should be the butterfly emoji"
        );
    }

    #[test]
    fn test_theme_spinner_chars_braille() {
        let theme = NikaTheme;
        let chars = theme.spinner_chars();
        // Braille characters are in Unicode block U+2800..U+28FF
        assert!(
            chars
                .chars()
                .all(|c| ('\u{2800}'..='\u{28FF}').contains(&c)),
            "Spinner chars should all be Braille unicode: got {chars:?}"
        );
    }

    // ── Non-interactive path ────────────────────────────────────────────

    #[test]
    fn test_run_init_wizard_yes_defaults() {
        let result = run_init_wizard(true).expect("--yes path should not fail");
        assert_eq!(result.mode, InitMode::Project);
        assert_eq!(result.permission, "plan");
        assert!(result.course_dest.is_none());
        assert!(!result.migrate_keys);
        assert!(!result.setup_editors);
        assert!(!result.setup_ai);
    }

    #[test]
    fn test_run_init_wizard_yes_includes_detected_providers() {
        let result = run_init_wizard(true).expect("--yes path should not fail");
        assert_eq!(
            result.detected_providers.len(),
            7,
            "Should include all 7 provider detection results"
        );
    }
}
