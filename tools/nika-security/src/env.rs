// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Environment variable validation — blocklist for library injection and key stripping.

use crate::SecurityError;

/// Blocked environment variable names (library injection / privilege escalation / path hijack).
///
/// These variables allow injecting arbitrary shared libraries into child
/// processes or hijacking command resolution, and must never be set from workflow YAML.
const BLOCKED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "LD_AUDIT",
    "LD_PROFILE",
    "PATH",
    // Shell startup injection: sourced before command execution
    "BASH_ENV", // Sourced by bash before `-c` execution
    "ENV",      // Sourced by sh/dash on interactive startup
];

/// Validate environment variables for dangerous names.
///
/// Performs two checks:
/// 1. Rejects env var names that don't match `^[A-Za-z_][A-Za-z0-9_]*$`.
///    This prevents BASH_FUNC injection and other shell metacharacter abuse
///    via crafted env var names (e.g., `BASH_FUNC_x%%`, `FOO=BAR`).
/// 2. Rejects env vars that enable library injection or privilege escalation.
///    Comparison is case-insensitive.
///
/// # Errors
///
/// Returns `BlockedCommand` if a blocked or invalid env var name is found.
pub fn validate_env_vars(vars: &[(String, String)]) -> Result<(), SecurityError> {
    for (key, _) in vars {
        if !is_valid_env_var_name(key) {
            return Err(SecurityError::BlockedCommand {
                command: format!("env: {key}=..."),
                reason: format!(
                    "Invalid environment variable name '{key}': must match [A-Za-z_][A-Za-z0-9_]*"
                ),
            });
        }

        let upper = key.to_uppercase();
        for blocked in BLOCKED_ENV_VARS {
            if upper == *blocked {
                return Err(SecurityError::BlockedCommand {
                    command: format!("env: {key}=..."),
                    reason: format!(
                        "Blocked environment variable '{key}': library injection risk"
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Check if an environment variable name is valid.
///
/// Valid names match `^[A-Za-z_][A-Za-z0-9_]*$` — the POSIX standard for
/// environment variable names. This rejects names containing `%`, `{`, `}`,
/// `(`, `)`, `=`, spaces, etc., which could be used for BASH_FUNC injection.
pub(crate) fn is_valid_env_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First character: must be [A-Za-z_]
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }

    // Remaining characters: must be [A-Za-z0-9_]
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns the list of sensitive env var names that should be stripped
/// from child processes to prevent API key leakage.
pub fn sensitive_env_vars() -> Vec<&'static str> {
    let mut vars: Vec<&'static str> = nika_core::catalogs::KNOWN_PROVIDERS
        .iter()
        .map(|p| p.env_var)
        .collect();

    vars.extend_from_slice(&[
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "AZURE_CLIENT_SECRET",
        "SCALEWAY_SECRET_KEY",
        "DATABASE_URL",
        "REDIS_URL",
        "MONGO_URI",
        "JWT_SECRET",
        "SESSION_SECRET",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "GITLAB_TOKEN",
        "SLACK_TOKEN",
        "SLACK_BOT_TOKEN",
        "SLACK_WEBHOOK_URL",
        "STRIPE_SECRET_KEY",
        "TWILIO_AUTH_TOKEN",
        "SENDGRID_API_KEY",
        "MAILGUN_API_KEY",
        "SENTRY_DSN",
        "DATADOG_API_KEY",
        "PRIVATE_KEY",
        "SECRET_KEY",
        "ENCRYPTION_KEY",
    ]);

    vars.sort();
    vars.dedup();
    vars
}
