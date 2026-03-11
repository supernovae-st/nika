//! MCP server aliases — short names to npm package mappings.
//!
//! This module provides 48 aliases for common MCP servers, allowing users
//! to add servers by short name instead of full npm package path.
//!
//! ## Usage
//!
//! ```bash
//! # Instead of:
//! nika mcp add @neo4j/mcp-server-neo4j
//!
//! # Use:
//! nika mcp add neo4j
//! ```
//!
//! ## Adding New Aliases
//!
//! To add a new alias, add an entry to `MCP_ALIASES` following the pattern:
//! `("short-name", "@org/package-name")`

/// MCP server aliases (48 total).
///
/// Maps short names to npm package names for common MCP servers.
///
/// ## Categories
///
/// - **Anthropic Official (8)**: Core MCP servers from Anthropic
/// - **Databases (8)**: Neo4j, PostgreSQL, MySQL, SQLite, etc.
/// - **Search & Web (8)**: Perplexity, Firecrawl, Brave, Exa, etc.
/// - **Developer Tools (8)**: GitHub, GitLab, Linear, Sentry, etc.
/// - **Productivity (8)**: Slack, Google Drive, Notion, etc.
/// - **AI & Specialized (8)**: Langchain, E2B, Sequential Thinking, etc.
pub static MCP_ALIASES: &[(&str, &str)] = &[
    // ═══════════════════════════════════════════════════════════════════════════
    // ANTHROPIC OFFICIAL (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("filesystem", "@modelcontextprotocol/server-filesystem"),
    ("memory", "@modelcontextprotocol/server-memory"),
    ("puppeteer", "@modelcontextprotocol/server-puppeteer"),
    ("brave-search", "@modelcontextprotocol/server-brave-search"),
    ("google-maps", "@modelcontextprotocol/server-google-maps"),
    ("fetch", "@modelcontextprotocol/server-fetch"),
    ("github", "@modelcontextprotocol/server-github"),
    ("gitlab", "@modelcontextprotocol/server-gitlab"),
    // ═══════════════════════════════════════════════════════════════════════════
    // DATABASES (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("neo4j", "@neo4j/mcp-neo4j"),
    ("postgres", "@modelcontextprotocol/server-postgres"),
    ("mysql", "mcp-server-mysql"),
    ("sqlite", "@anthropic/mcp-server-sqlite"),
    ("mongodb", "mcp-mongodb"),
    ("redis", "mcp-redis"),
    ("supabase", "mcp-supabase"),
    ("neon", "@neondatabase/mcp-server-neon"),
    // ═══════════════════════════════════════════════════════════════════════════
    // SEARCH & WEB (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("perplexity", "perplexity-mcp"),
    ("firecrawl", "firecrawl-mcp"),
    ("brave", "@anthropic/mcp-server-brave-search"),
    ("exa", "exa-mcp-server"),
    ("tavily", "tavily-mcp"),
    ("serper", "serper-mcp"),
    ("searchapi", "searchapi-mcp"),
    ("bing", "bing-mcp"),
    // ═══════════════════════════════════════════════════════════════════════════
    // DEVELOPER TOOLS (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("linear", "mcp-linear"),
    ("sentry", "@modelcontextprotocol/server-sentry"),
    ("raygun", "raygun-mcp"),
    ("buildkite", "buildkite-mcp"),
    ("circleci", "circleci-mcp"),
    ("vercel", "vercel-mcp"),
    ("cloudflare", "cloudflare-mcp"),
    ("aws", "aws-mcp"),
    // ═══════════════════════════════════════════════════════════════════════════
    // PRODUCTIVITY (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("slack", "@anthropic/mcp-server-slack"),
    ("google-drive", "@anthropic/mcp-server-google-drive"),
    ("notion", "notion-mcp"),
    ("airtable", "airtable-mcp"),
    ("todoist", "todoist-mcp"),
    ("asana", "asana-mcp"),
    ("trello", "trello-mcp"),
    ("monday", "monday-mcp"),
    // ═══════════════════════════════════════════════════════════════════════════
    // AI & SPECIALIZED (8)
    // ═══════════════════════════════════════════════════════════════════════════
    ("langchain", "langchain-mcp"),
    ("e2b", "@e2b/mcp-server"),
    ("sequential-thinking", "@modelcontextprotocol/server-sequential-thinking"),
    ("context7", "context7-mcp"),
    ("21st", "21st-mcp"),
    ("supadata", "supadata-mcp"),
    ("dataforseo", "dataforseo-mcp"),
    ("ahrefs", "ahrefs-mcp"),
];

/// Resolve an alias to its full npm package name.
///
/// # Example
///
/// ```
/// use nika::core::mcp_aliases::resolve_alias;
///
/// assert_eq!(resolve_alias("neo4j"), Some("@neo4j/mcp-neo4j"));
/// assert_eq!(resolve_alias("unknown"), None);
/// ```
pub fn resolve_alias(alias: &str) -> Option<&'static str> {
    MCP_ALIASES
        .iter()
        .find(|(a, _)| *a == alias)
        .map(|(_, pkg)| *pkg)
}

/// Check if a string is a known alias.
///
/// # Example
///
/// ```
/// use nika::core::mcp_aliases::is_alias;
///
/// assert!(is_alias("neo4j"));
/// assert!(!is_alias("@neo4j/mcp-neo4j"));
/// ```
pub fn is_alias(name: &str) -> bool {
    MCP_ALIASES.iter().any(|(a, _)| *a == name)
}

/// List all available aliases.
///
/// # Example
///
/// ```
/// use nika::core::mcp_aliases::list_aliases;
///
/// let aliases = list_aliases();
/// assert!(aliases.len() >= 48);
/// ```
pub fn list_aliases() -> Vec<&'static str> {
    MCP_ALIASES.iter().map(|(a, _)| *a).collect()
}

/// Get all aliases in a specific category.
///
/// Categories are determined by index ranges in MCP_ALIASES.
pub fn aliases_by_category(category: &str) -> Vec<(&'static str, &'static str)> {
    let (start, end) = match category {
        "anthropic" => (0, 8),
        "databases" => (8, 16),
        "search" => (16, 24),
        "developer" => (24, 32),
        "productivity" => (32, 40),
        "ai" => (40, 48),
        _ => return vec![],
    };

    MCP_ALIASES[start..end.min(MCP_ALIASES.len())].to_vec()
}

/// Resolve a name that might be an alias or a full package name.
///
/// If the name is an alias, returns the package name.
/// If the name is already a package name (starts with @ or contains /), returns as-is.
/// Otherwise, returns None.
///
/// # Example
///
/// ```
/// use nika::core::mcp_aliases::resolve_name;
///
/// // Alias → package
/// assert_eq!(resolve_name("neo4j"), Some("@neo4j/mcp-neo4j".to_string()));
///
/// // Already a package name
/// assert_eq!(resolve_name("@custom/server"), Some("@custom/server".to_string()));
///
/// // Unknown
/// assert_eq!(resolve_name("unknown"), None);
/// ```
pub fn resolve_name(name: &str) -> Option<String> {
    // If it's already a package name, return as-is
    if name.starts_with('@') || name.contains('/') {
        return Some(name.to_string());
    }

    // Try to resolve as alias
    resolve_alias(name).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_aliases_count() {
        assert_eq!(MCP_ALIASES.len(), 48);
    }

    #[test]
    fn test_resolve_alias() {
        assert_eq!(resolve_alias("neo4j"), Some("@neo4j/mcp-neo4j"));
        assert_eq!(resolve_alias("github"), Some("@modelcontextprotocol/server-github"));
        assert_eq!(resolve_alias("perplexity"), Some("perplexity-mcp"));
        assert_eq!(resolve_alias("slack"), Some("@anthropic/mcp-server-slack"));
        assert_eq!(resolve_alias("nonexistent"), None);
    }

    #[test]
    fn test_is_alias() {
        assert!(is_alias("neo4j"));
        assert!(is_alias("github"));
        assert!(is_alias("perplexity"));
        assert!(!is_alias("@neo4j/mcp-neo4j"));
        assert!(!is_alias("unknown"));
    }

    #[test]
    fn test_list_aliases() {
        let aliases = list_aliases();
        assert_eq!(aliases.len(), 48);
        assert!(aliases.contains(&"neo4j"));
        assert!(aliases.contains(&"github"));
        assert!(aliases.contains(&"perplexity"));
    }

    #[test]
    fn test_aliases_by_category() {
        let anthropic = aliases_by_category("anthropic");
        assert_eq!(anthropic.len(), 8);
        assert!(anthropic.iter().any(|(a, _)| *a == "filesystem"));
        assert!(anthropic.iter().any(|(a, _)| *a == "github"));

        let databases = aliases_by_category("databases");
        assert_eq!(databases.len(), 8);
        assert!(databases.iter().any(|(a, _)| *a == "neo4j"));
        assert!(databases.iter().any(|(a, _)| *a == "postgres"));

        let unknown = aliases_by_category("unknown");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_resolve_name() {
        // Alias resolution
        assert_eq!(resolve_name("neo4j"), Some("@neo4j/mcp-neo4j".to_string()));

        // Package name passthrough
        assert_eq!(
            resolve_name("@custom/server"),
            Some("@custom/server".to_string())
        );
        assert_eq!(
            resolve_name("some-org/server"),
            Some("some-org/server".to_string())
        );

        // Unknown
        assert_eq!(resolve_name("unknown-thing"), None);
    }

    #[test]
    fn test_all_aliases_have_valid_packages() {
        for (alias, package) in MCP_ALIASES {
            assert!(!alias.is_empty(), "Alias should not be empty");
            assert!(!package.is_empty(), "Package should not be empty");
            assert!(
                !alias.contains('/'),
                "Alias '{}' should not contain '/'",
                alias
            );
            assert!(
                !alias.starts_with('@'),
                "Alias '{}' should not start with '@'",
                alias
            );
        }
    }

    #[test]
    fn test_no_duplicate_aliases() {
        let aliases: Vec<_> = MCP_ALIASES.iter().map(|(a, _)| *a).collect();
        let unique: std::collections::HashSet<_> = aliases.iter().collect();
        assert_eq!(
            aliases.len(),
            unique.len(),
            "Duplicate aliases found in MCP_ALIASES"
        );
    }
}
