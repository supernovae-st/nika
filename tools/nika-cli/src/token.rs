//! `nika token` — manage serve API tokens for multi-tenant auth.
//!
//! CLI-only token management (no HTTP routes — AD-6 security decision).
//! Tokens are stored as BLAKE3 hashes in SQLite. The raw token is printed
//! ONCE on creation — it cannot be retrieved later.

use clap::Subcommand;
use colored::Colorize;
use std::io::IsTerminal;
use std::path::PathBuf;

use nika_engine::display::{tree_connector, StatusIcon};
use nika_engine::error::NikaError;

/// Token management actions.
#[derive(Subcommand)]
pub enum TokenAction {
    /// Create a new API token
    Add {
        /// Human-readable name for this token (e.g. "ci-deploy", "staging")
        name: String,
        /// Token role (operator, viewer, admin)
        #[arg(long, default_value = "operator")]
        role: String,
        /// Scope glob pattern — restricts which workflows this token can access.
        /// Use "*" for all, "project-a/*" for a subtree, comma-separate for multiple.
        #[arg(long, default_value = "*")]
        scope: String,
        /// Expiry duration (e.g. "30d", "12h", "never")
        #[arg(long)]
        expires: Option<String>,
        /// SQLite database path (default: .nika/serve.db)
        #[arg(long, value_name = "PATH")]
        db: Option<String>,
    },

    /// List all active tokens (names and metadata, never raw tokens)
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// SQLite database path
        #[arg(long, value_name = "PATH")]
        db: Option<String>,
    },

    /// Revoke a token by name (soft-delete — immediate effect after cache TTL)
    Revoke {
        /// Token name to revoke
        name: String,
        /// SQLite database path
        #[arg(long, value_name = "PATH")]
        db: Option<String>,
    },
}

fn resolve_db_path(db: Option<&str>) -> PathBuf {
    if let Some(path) = db {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("NIKA_SERVE_DB") {
        return PathBuf::from(path);
    }
    // Default: .nika/serve.db relative to project root
    PathBuf::from(".nika/serve.db")
}

pub async fn run(action: TokenAction) -> Result<(), NikaError> {
    match action {
        TokenAction::Add {
            name,
            role,
            scope,
            expires,
            db,
        } => cmd_add(&name, &role, &scope, expires.as_deref(), db.as_deref()).await,
        TokenAction::List { json, db } => cmd_list(json, db.as_deref()).await,
        TokenAction::Revoke { name, db } => cmd_revoke(&name, db.as_deref()).await,
    }
}

async fn cmd_add(
    name: &str,
    role_str: &str,
    scope: &str,
    expires: Option<&str>,
    db: Option<&str>,
) -> Result<(), NikaError> {
    // Validate name
    if name.is_empty() || name.len() > 64 {
        return Err(NikaError::ConfigError {
            reason: "Token name must be 1-64 characters".into(),
        });
    }

    let role = nika_storage::Role::parse(role_str);
    let db_path = resolve_db_path(db);

    let storage = nika_storage::Storage::open(&db_path).map_err(|e| NikaError::ConfigError {
        reason: format!("Failed to open database: {e}"),
    })?;

    // Check for duplicate name
    if let Ok(Some(_)) = storage.get_token_by_name(name).await {
        return Err(NikaError::ConfigError {
            reason: format!("Token name '{name}' already exists. Use a different name."),
        });
    }

    // Generate token
    let raw_token = generate_token();
    let hash = hash_token(&raw_token);
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let expires_at = match expires {
        Some("never") | None => None,
        Some(duration_str) => {
            let duration = parse_duration(duration_str)?;
            Some((chrono::Utc::now() + duration).to_rfc3339())
        }
    };

    let entry = nika_storage::TokenEntry {
        id,
        name: name.to_string(),
        token_hash: hash.to_vec(),
        role,
        scope: scope.to_string(),
        created_at: now,
        expires_at,
        last_used_at: None,
        revoked: false,
    };

    storage
        .insert_token(entry)
        .await
        .map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to store token: {e}"),
        })?;

    let is_tty = std::io::stdout().is_terminal();
    if is_tty {
        eprintln!();
        eprintln!("  {} Token '{}' created", StatusIcon::Ok, name.cyan());
        eprintln!();
        eprintln!(
            "  {}",
            "Save this token now — it cannot be retrieved later:".yellow()
        );
        eprintln!();
    }
    // Print raw token to stdout (safe for piping)
    println!("{raw_token}");

    if is_tty {
        eprintln!();
        eprintln!(
            "  {} Use with: {} {}",
            tree_connector(false),
            "Authorization:".dimmed(),
            format!("Bearer {}", &raw_token[..10]).dimmed()
        );
        eprintln!();
    }

    Ok(())
}

async fn cmd_list(json: bool, db: Option<&str>) -> Result<(), NikaError> {
    let db_path = resolve_db_path(db);
    let storage = nika_storage::Storage::open(&db_path).map_err(|e| NikaError::ConfigError {
        reason: format!("Failed to open database: {e}"),
    })?;

    let tokens = storage
        .list_tokens()
        .await
        .map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to list tokens: {e}"),
        })?;

    if json {
        #[derive(serde::Serialize)]
        struct TokenInfo {
            name: String,
            role: String,
            scope: String,
            created_at: String,
            expires_at: Option<String>,
            last_used_at: Option<String>,
        }

        let infos: Vec<TokenInfo> = tokens
            .iter()
            .map(|t| TokenInfo {
                name: t.name.clone(),
                role: t.role.as_str().to_string(),
                scope: t.scope.clone(),
                created_at: t.created_at.clone(),
                expires_at: t.expires_at.clone(),
                last_used_at: t.last_used_at.clone(),
            })
            .collect();

        println!("{}", serde_json::to_string_pretty(&infos).unwrap());
        return Ok(());
    }

    if tokens.is_empty() {
        eprintln!();
        eprintln!(
            "  {} No tokens configured. Create one with: {}",
            StatusIcon::Warn,
            "nika token add <name>".cyan()
        );
        eprintln!();
        return Ok(());
    }

    eprintln!();
    eprintln!("  {} {} active token(s)", StatusIcon::Info, tokens.len());
    eprintln!();

    for (i, token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        let connector = tree_connector(is_last);
        let role_display = match token.role {
            nika_storage::Role::Admin => "admin".red().to_string(),
            nika_storage::Role::Viewer => "viewer".blue().to_string(),
            nika_storage::Role::Operator => "operator".green().to_string(),
        };

        let last_used = token.last_used_at.as_deref().unwrap_or("never");

        let scope_display = if token.scope == "*" {
            String::new()
        } else {
            format!("  scope: {}", token.scope.yellow())
        };

        eprintln!(
            "  {} {}  {}{}  last used: {}",
            connector,
            token.name.cyan().bold(),
            role_display,
            scope_display,
            last_used.dimmed()
        );
    }

    eprintln!();
    Ok(())
}

async fn cmd_revoke(name: &str, db: Option<&str>) -> Result<(), NikaError> {
    let db_path = resolve_db_path(db);
    let storage = nika_storage::Storage::open(&db_path).map_err(|e| NikaError::ConfigError {
        reason: format!("Failed to open database: {e}"),
    })?;

    // Find by name
    let entry = storage
        .get_token_by_name(name)
        .await
        .map_err(|e| NikaError::ConfigError {
            reason: format!("Database error: {e}"),
        })?
        .ok_or_else(|| NikaError::ConfigError {
            reason: format!("Token '{name}' not found"),
        })?;

    if entry.revoked {
        return Err(NikaError::ConfigError {
            reason: format!("Token '{name}' is already revoked"),
        });
    }

    storage
        .revoke_token(&entry.id)
        .await
        .map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to revoke token: {e}"),
        })?;

    // Always return the same response shape (prevent enumeration — AD-10)
    eprintln!();
    eprintln!("  {} Token '{}' revoked", StatusIcon::Ok, name.cyan());
    eprintln!(
        "  {} Active requests may continue for up to 60s (cache TTL)",
        StatusIcon::Info
    );
    eprintln!();

    Ok(())
}

/// Generate a cryptographically secure 192-bit API token with nk_ prefix.
fn generate_token() -> String {
    let mut bytes = [0u8; 24]; // 192 bits
    getrandom::fill(&mut bytes).expect("getrandom failed");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("nk_{hex}")
}

/// Hash a raw token with BLAKE3 for storage.
fn hash_token(raw: &str) -> [u8; 32] {
    *blake3::hash(raw.as_bytes()).as_bytes()
}

/// Parse a human-readable duration like "30d", "12h", "7d".
fn parse_duration(s: &str) -> Result<chrono::Duration, NikaError> {
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = num.parse().map_err(|_| NikaError::ConfigError {
        reason: format!("Invalid duration: '{s}'. Use e.g. '30d', '12h', '1y'"),
    })?;

    match unit {
        "h" => Ok(chrono::Duration::hours(n)),
        "d" => Ok(chrono::Duration::days(n)),
        "w" => Ok(chrono::Duration::weeks(n)),
        "y" => Ok(chrono::Duration::days(n * 365)),
        _ => Err(NikaError::ConfigError {
            reason: format!("Unknown duration unit '{unit}'. Use h/d/w/y"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_days() {
        let d = parse_duration("30d").unwrap();
        assert_eq!(d.num_days(), 30);
    }

    #[test]
    fn parse_duration_hours() {
        let d = parse_duration("12h").unwrap();
        assert_eq!(d.num_hours(), 12);
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("30x").is_err());
    }

    #[test]
    fn generate_token_has_prefix() {
        let token = generate_token();
        assert!(token.starts_with("nk_"));
        assert_eq!(token.len(), 51); // nk_ + 48 hex
    }

    #[test]
    fn hash_deterministic() {
        let h1 = hash_token("test");
        let h2 = hash_token("test");
        assert_eq!(h1, h2);
    }
}
