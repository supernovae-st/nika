// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP server aliases -- short names to npm package mappings.
//!
//! This module provides 113 aliases for common MCP servers, allowing users
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
//! To add a new alias, add an `McpAlias` entry to `MCP_ALIASES` with
//! the appropriate `category` and `pricing`.

/// Pricing model for an MCP server's underlying service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpPricing {
    /// Fully free / open source (no API key needed or free forever)
    Free,
    /// Free tier available, paid for higher usage
    Freemium,
    /// Requires paid subscription / API key with charges
    Paid,
}

/// A known MCP server alias with metadata.
#[derive(Debug, Clone, Copy)]
pub struct McpAlias {
    /// Short alias name (e.g., "replicate")
    pub name: &'static str,
    /// npm package name (e.g., "replicate-mcp")
    pub package: &'static str,
    /// Category for grouping in CLI display
    pub category: &'static str,
    /// Pricing model of the underlying service
    pub pricing: McpPricing,
}

/// MCP server aliases (114 total).
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
/// - **AI Image/Media Generation (6)**: Replicate, ComfyUI, FAL, etc.
/// - **Communication (6)**: Discord, Telegram, Resend, etc.
/// - **Knowledge & Vector DB (6)**: Pinecone, Weaviate, Qdrant, etc.
/// - **Analytics & Monitoring (6)**: PostHog, Mixpanel, Datadog, etc.
/// - **E-commerce & Finance (6)**: Stripe, Shopify, PayPal, etc.
/// - **CMS & Content (6)**: WordPress, Contentful, Sanity, etc.
/// - **Infrastructure & DevOps (6)**: Docker, Kubernetes, Terraform, etc.
/// - **Social Media (6)**: Twitter, LinkedIn, YouTube, etc.
/// - **Lifestyle (8)**: Spotify, Airbnb, Uber, OpenWeather, etc.
/// - **Marketing (6)**: Mailchimp, Google Ads, Meta Ads, etc.
/// - **Maps & Location (3)**: Mapbox, HERE, IPInfo
pub static MCP_ALIASES: &[McpAlias] = &[
    // =============================================================================
    // ANTHROPIC OFFICIAL (8)
    // =============================================================================
    McpAlias {
        name: "filesystem",
        package: "@modelcontextprotocol/server-filesystem",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "memory",
        package: "@modelcontextprotocol/server-memory",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "puppeteer",
        package: "@modelcontextprotocol/server-puppeteer",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "brave-search",
        package: "@modelcontextprotocol/server-brave-search",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "google-maps",
        package: "@modelcontextprotocol/server-google-maps",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "fetch",
        package: "@modelcontextprotocol/server-fetch",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "github",
        package: "@modelcontextprotocol/server-github",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "gitlab",
        package: "@modelcontextprotocol/server-gitlab",
        category: "anthropic",
        pricing: McpPricing::Free,
    },
    // =============================================================================
    // DATABASES (8)
    // =============================================================================
    McpAlias {
        name: "neo4j",
        package: "@neo4j/mcp-neo4j",
        category: "databases",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "postgres",
        package: "@modelcontextprotocol/server-postgres",
        category: "databases",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "mysql",
        package: "mcp-server-mysql",
        category: "databases",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "sqlite",
        package: "@anthropic/mcp-server-sqlite",
        category: "databases",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "mongodb",
        package: "mcp-mongodb",
        category: "databases",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "redis",
        package: "mcp-redis",
        category: "databases",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "supabase",
        package: "mcp-supabase",
        category: "databases",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "neon",
        package: "@neondatabase/mcp-server-neon",
        category: "databases",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // SEARCH & WEB (8)
    // =============================================================================
    McpAlias {
        name: "perplexity",
        package: "perplexity-mcp",
        category: "search",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "firecrawl",
        package: "firecrawl-mcp",
        category: "search",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "brave",
        package: "@anthropic/mcp-server-brave-search",
        category: "search",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "exa",
        package: "exa-mcp-server",
        category: "search",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "tavily",
        package: "tavily-mcp",
        category: "search",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "serper",
        package: "serper-mcp",
        category: "search",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "searchapi",
        package: "searchapi-mcp",
        category: "search",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "bing",
        package: "bing-mcp",
        category: "search",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // DEVELOPER TOOLS (8)
    // =============================================================================
    McpAlias {
        name: "linear",
        package: "mcp-linear",
        category: "developer",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "sentry",
        package: "@modelcontextprotocol/server-sentry",
        category: "developer",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "raygun",
        package: "raygun-mcp",
        category: "developer",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "buildkite",
        package: "buildkite-mcp",
        category: "developer",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "circleci",
        package: "circleci-mcp",
        category: "developer",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "vercel",
        package: "vercel-mcp",
        category: "developer",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "cloudflare",
        package: "cloudflare-mcp",
        category: "developer",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "aws",
        package: "aws-mcp",
        category: "developer",
        pricing: McpPricing::Paid,
    },
    // =============================================================================
    // PRODUCTIVITY (8)
    // =============================================================================
    McpAlias {
        name: "slack",
        package: "@anthropic/mcp-server-slack",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "google-drive",
        package: "@anthropic/mcp-server-google-drive",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "notion",
        package: "notion-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "airtable",
        package: "airtable-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "todoist",
        package: "todoist-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "asana",
        package: "asana-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "trello",
        package: "trello-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "monday",
        package: "monday-mcp",
        category: "productivity",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // AI & SPECIALIZED (8)
    // =============================================================================
    McpAlias {
        name: "langchain",
        package: "langchain-mcp",
        category: "ai",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "e2b",
        package: "@e2b/mcp-server",
        category: "ai",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "sequential-thinking",
        package: "@modelcontextprotocol/server-sequential-thinking",
        category: "ai",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "context7",
        package: "context7-mcp",
        category: "ai",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "21st",
        package: "21st-mcp",
        category: "ai",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "supadata",
        package: "supadata-mcp",
        category: "ai",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "dataforseo",
        package: "dataforseo-mcp",
        category: "ai",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "ahrefs",
        package: "ahrefs-mcp",
        category: "ai",
        pricing: McpPricing::Paid,
    },
    // =============================================================================
    // AI IMAGE/MEDIA GENERATION (6)
    // =============================================================================
    McpAlias {
        name: "replicate",
        package: "replicate-mcp",
        category: "image",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "comfyui",
        package: "comfyui-mcp-server",
        category: "image",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "fal",
        package: "fal-mcp",
        category: "image",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "stability",
        package: "stability-ai-mcp",
        category: "image",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "elevenlabs",
        package: "elevenlabs-mcp",
        category: "image",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "deepgram",
        package: "deepgram-mcp",
        category: "image",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // COMMUNICATION (6)
    // =============================================================================
    McpAlias {
        name: "discord",
        package: "discord-mcp",
        category: "communication",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "telegram",
        package: "telegram-mcp",
        category: "communication",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "resend",
        package: "resend-mcp",
        category: "communication",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "sendgrid",
        package: "sendgrid-mcp",
        category: "communication",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "twilio",
        package: "twilio-mcp",
        category: "communication",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "intercom",
        package: "intercom-mcp",
        category: "communication",
        pricing: McpPricing::Paid,
    },
    // =============================================================================
    // KNOWLEDGE & VECTOR DB (6)
    // =============================================================================
    McpAlias {
        name: "pinecone",
        package: "pinecone-mcp",
        category: "vectordb",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "weaviate",
        package: "weaviate-mcp",
        category: "vectordb",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "qdrant",
        package: "qdrant-mcp",
        category: "vectordb",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "chroma",
        package: "chroma-mcp",
        category: "vectordb",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "milvus",
        package: "milvus-mcp",
        category: "vectordb",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "turbopuffer",
        package: "turbopuffer-mcp",
        category: "vectordb",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // ANALYTICS & MONITORING (6)
    // =============================================================================
    McpAlias {
        name: "posthog",
        package: "posthog-mcp",
        category: "analytics",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "mixpanel",
        package: "mixpanel-mcp",
        category: "analytics",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "datadog",
        package: "datadog-mcp",
        category: "analytics",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "grafana",
        package: "grafana-mcp",
        category: "analytics",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "prometheus",
        package: "prometheus-mcp",
        category: "analytics",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "plausible",
        package: "plausible-mcp",
        category: "analytics",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // E-COMMERCE & FINANCE (6)
    // =============================================================================
    McpAlias {
        name: "stripe",
        package: "stripe-mcp",
        category: "ecommerce",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "shopify",
        package: "shopify-mcp",
        category: "ecommerce",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "paypal",
        package: "paypal-mcp",
        category: "ecommerce",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "polygon",
        package: "polygon-mcp",
        category: "ecommerce",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "coinbase",
        package: "coinbase-mcp",
        category: "ecommerce",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "alpaca",
        package: "alpaca-mcp",
        category: "ecommerce",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // CMS & CONTENT (6)
    // =============================================================================
    McpAlias {
        name: "wordpress",
        package: "wordpress-mcp",
        category: "cms",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "contentful",
        package: "contentful-mcp",
        category: "cms",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "sanity",
        package: "sanity-mcp",
        category: "cms",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "strapi",
        package: "strapi-mcp",
        category: "cms",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "ghost",
        package: "ghost-mcp",
        category: "cms",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "hubspot",
        package: "hubspot-mcp",
        category: "cms",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // INFRASTRUCTURE & DEVOPS (6)
    // =============================================================================
    McpAlias {
        name: "docker",
        package: "docker-mcp",
        category: "devops",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "kubernetes",
        package: "kubernetes-mcp",
        category: "devops",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "terraform",
        package: "terraform-mcp",
        category: "devops",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "pulumi",
        package: "pulumi-mcp",
        category: "devops",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "fly",
        package: "fly-mcp",
        category: "devops",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "railway",
        package: "railway-mcp",
        category: "devops",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // SOCIAL MEDIA (6)
    // =============================================================================
    McpAlias {
        name: "twitter",
        package: "twitter-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "linkedin",
        package: "linkedin-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "youtube",
        package: "youtube-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "tiktok",
        package: "tiktok-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "reddit",
        package: "reddit-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "mastodon",
        package: "mastodon-mcp",
        category: "social",
        pricing: McpPricing::Free,
    },
    // =============================================================================
    // LIFESTYLE (8)
    // =============================================================================
    McpAlias {
        name: "spotify",
        package: "spotify-mcp",
        category: "lifestyle",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "airbnb",
        package: "airbnb-mcp",
        category: "lifestyle",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "uber",
        package: "uber-mcp",
        category: "lifestyle",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "openweather",
        package: "openweather-mcp",
        category: "lifestyle",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "news-api",
        package: "newsapi-mcp",
        category: "lifestyle",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "recipe",
        package: "recipe-mcp",
        category: "lifestyle",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "imdb",
        package: "imdb-mcp",
        category: "lifestyle",
        pricing: McpPricing::Free,
    },
    McpAlias {
        name: "wolfram",
        package: "wolfram-mcp",
        category: "lifestyle",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // MARKETING (6)
    // =============================================================================
    McpAlias {
        name: "mailchimp",
        package: "mailchimp-mcp",
        category: "marketing",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "google-ads",
        package: "google-ads-mcp",
        category: "marketing",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "meta-ads",
        package: "meta-ads-mcp",
        category: "marketing",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "buffer",
        package: "buffer-mcp",
        category: "marketing",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "semrush",
        package: "semrush-mcp",
        category: "marketing",
        pricing: McpPricing::Paid,
    },
    McpAlias {
        name: "canva",
        package: "canva-mcp",
        category: "marketing",
        pricing: McpPricing::Freemium,
    },
    // =============================================================================
    // MAPS & LOCATION (3)
    // =============================================================================
    McpAlias {
        name: "mapbox",
        package: "mapbox-mcp",
        category: "maps",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "here",
        package: "here-mcp",
        category: "maps",
        pricing: McpPricing::Freemium,
    },
    McpAlias {
        name: "ipinfo",
        package: "ipinfo-mcp",
        category: "maps",
        pricing: McpPricing::Freemium,
    },
];

/// All category keys, in display order.
pub static CATEGORIES: &[(&str, &str)] = &[
    ("anthropic", "Anthropic Official"),
    ("databases", "Databases"),
    ("search", "Search & Web"),
    ("developer", "Developer Tools"),
    ("productivity", "Productivity"),
    ("ai", "AI & Specialized"),
    ("image", "AI Image/Media"),
    ("communication", "Communication"),
    ("vectordb", "Vector DB"),
    ("analytics", "Analytics & Monitoring"),
    ("ecommerce", "E-commerce & Finance"),
    ("cms", "CMS & Content"),
    ("devops", "Infrastructure & DevOps"),
    ("social", "Social Media"),
    ("lifestyle", "Lifestyle"),
    ("marketing", "Marketing"),
    ("maps", "Maps & Location"),
];

/// Resolve an alias to its full npm package name.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::mcp_aliases::resolve_alias;
///
/// assert_eq!(resolve_alias("neo4j"), Some("@neo4j/mcp-neo4j"));
/// assert_eq!(resolve_alias("unknown"), None);
/// ```
pub fn resolve_alias(alias: &str) -> Option<&'static str> {
    let lower = alias.to_lowercase();
    MCP_ALIASES
        .iter()
        .find(|a| a.name.to_lowercase() == lower)
        .map(|a| a.package)
}

/// Check if a string is a known alias.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::mcp_aliases::is_alias;
///
/// assert!(is_alias("neo4j"));
/// assert!(!is_alias("@neo4j/mcp-neo4j"));
/// ```
pub fn is_alias(name: &str) -> bool {
    let lower = name.to_lowercase();
    MCP_ALIASES.iter().any(|a| a.name.to_lowercase() == lower)
}

/// List all available alias names.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::mcp_aliases::list_aliases;
///
/// let aliases = list_aliases();
/// assert!(aliases.len() >= 113);
/// ```
pub fn list_aliases() -> Vec<&'static str> {
    MCP_ALIASES.iter().map(|a| a.name).collect()
}

/// Get all aliases in a specific category.
///
/// Filters `MCP_ALIASES` by the `.category` field.
pub fn aliases_by_category(category: &str) -> Vec<&'static McpAlias> {
    MCP_ALIASES
        .iter()
        .filter(|a| a.category == category)
        .collect()
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
/// use nika_core::catalogs::mcp_aliases::resolve_name;
///
/// // Alias -> package
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

/// Return a display label for a pricing tier.
pub fn pricing_label(pricing: McpPricing) -> &'static str {
    match pricing {
        McpPricing::Free => "FREE",
        McpPricing::Freemium => "FREEMIUM",
        McpPricing::Paid => "PAID",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_aliases_count() {
        assert_eq!(MCP_ALIASES.len(), 113);
    }

    #[test]
    fn test_resolve_alias() {
        assert_eq!(resolve_alias("neo4j"), Some("@neo4j/mcp-neo4j"));
        assert_eq!(
            resolve_alias("github"),
            Some("@modelcontextprotocol/server-github")
        );
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
        assert_eq!(aliases.len(), 113);
        assert!(aliases.contains(&"neo4j"));
        assert!(aliases.contains(&"github"));
        assert!(aliases.contains(&"perplexity"));
    }

    #[test]
    fn test_aliases_by_category() {
        let anthropic = aliases_by_category("anthropic");
        assert_eq!(anthropic.len(), 8);
        assert!(anthropic.iter().any(|a| a.name == "filesystem"));
        assert!(anthropic.iter().any(|a| a.name == "github"));

        let databases = aliases_by_category("databases");
        assert_eq!(databases.len(), 8);
        assert!(databases.iter().any(|a| a.name == "neo4j"));
        assert!(databases.iter().any(|a| a.name == "postgres"));

        let unknown = aliases_by_category("unknown");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_aliases_by_category_all_17() {
        let expected = [
            ("anthropic", 8),
            ("databases", 8),
            ("search", 8),
            ("developer", 8),
            ("productivity", 8),
            ("ai", 8),
            ("image", 6),
            ("communication", 6),
            ("vectordb", 6),
            ("analytics", 6),
            ("ecommerce", 6),
            ("cms", 6),
            ("devops", 6),
            ("social", 6),
            ("lifestyle", 8),
            ("marketing", 6),
            ("maps", 3),
        ];
        for (cat, count) in expected {
            let aliases = aliases_by_category(cat);
            assert_eq!(
                aliases.len(),
                count,
                "category '{}' should have {} aliases",
                cat,
                count
            );
        }
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
        for alias in MCP_ALIASES {
            assert!(!alias.name.is_empty(), "Alias name should not be empty");
            assert!(!alias.package.is_empty(), "Package should not be empty");
            assert!(
                !alias.name.contains('/'),
                "Alias '{}' should not contain '/'",
                alias.name
            );
            assert!(
                !alias.name.starts_with('@'),
                "Alias '{}' should not start with '@'",
                alias.name
            );
        }
    }

    #[test]
    fn test_no_duplicate_aliases() {
        let aliases: Vec<_> = MCP_ALIASES.iter().map(|a| a.name).collect();
        let unique: std::collections::HashSet<_> = aliases.iter().collect();
        assert_eq!(
            aliases.len(),
            unique.len(),
            "Duplicate aliases found in MCP_ALIASES"
        );
    }

    #[test]
    fn test_all_categories_covered() {
        // Every alias must use a known category key
        let known_cats: Vec<&str> = CATEGORIES.iter().map(|(k, _)| *k).collect();
        for alias in MCP_ALIASES {
            assert!(
                known_cats.contains(&alias.category),
                "Alias '{}' uses unknown category '{}'",
                alias.name,
                alias.category
            );
        }
    }

    #[test]
    fn test_pricing_label() {
        assert_eq!(pricing_label(McpPricing::Free), "FREE");
        assert_eq!(pricing_label(McpPricing::Freemium), "FREEMIUM");
        assert_eq!(pricing_label(McpPricing::Paid), "PAID");
    }

    #[test]
    fn test_categories_count() {
        assert_eq!(CATEGORIES.len(), 17);
    }

    #[test]
    fn test_struct_fields_accessible() {
        let neo4j = MCP_ALIASES.iter().find(|a| a.name == "neo4j").unwrap();
        assert_eq!(neo4j.package, "@neo4j/mcp-neo4j");
        assert_eq!(neo4j.category, "databases");
        assert_eq!(neo4j.pricing, McpPricing::Freemium);
    }
}
