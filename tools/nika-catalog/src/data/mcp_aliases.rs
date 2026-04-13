// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static MCP server alias catalog — 113 aliases with phf+unicase lookup.
//!
//! Decision B (locked): 11 ex-MCP "providers" from legacy now live here with
//! optional `env_var` and `key_prefix` for secret management.

use phf::phf_map;
use unicase::UniCase;

use crate::types::mcp_alias::{McpAlias, McpPricing};

use McpPricing::{Free, Freemium, Paid};

/// All 113 MCP server aliases.
///
/// Categories (17): anthropic(8), databases(8), search(8), developer(8),
/// productivity(8), ai(8), image(6), communication(6), vectordb(6),
/// analytics(6), ecommerce(6), cms(6), devops(6), social(6), lifestyle(8),
/// marketing(6), maps(3).
pub static ALL_MCP_ALIASES: &[McpAlias] = &[
    // ── Anthropic Official (8) ──────────────────────────────────
    McpAlias { name: "filesystem", package: "@modelcontextprotocol/server-filesystem", category: "anthropic", pricing: Free, env_var: Some("FILESYSTEM_ALLOWED_PATHS"), key_prefix: None },
    McpAlias { name: "memory", package: "@modelcontextprotocol/server-memory", category: "anthropic", pricing: Free, env_var: Some("MEMORY_STORAGE_PATH"), key_prefix: None },
    McpAlias { name: "puppeteer", package: "@modelcontextprotocol/server-puppeteer", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "brave-search", package: "@modelcontextprotocol/server-brave-search", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "google-maps", package: "@modelcontextprotocol/server-google-maps", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "fetch", package: "@modelcontextprotocol/server-fetch", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "github", package: "@modelcontextprotocol/server-github", category: "anthropic", pricing: Free, env_var: Some("GITHUB_TOKEN"), key_prefix: Some("ghp_") },
    McpAlias { name: "gitlab", package: "@modelcontextprotocol/server-gitlab", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    // ── Databases (8) ───────────────────────────────────────────
    McpAlias { name: "neo4j", package: "@neo4j/mcp-neo4j", category: "databases", pricing: Freemium, env_var: Some("NEO4J_PASSWORD"), key_prefix: None },
    McpAlias { name: "postgres", package: "@modelcontextprotocol/server-postgres", category: "databases", pricing: Free, env_var: Some("POSTGRES_URL"), key_prefix: None },
    McpAlias { name: "mysql", package: "mcp-server-mysql", category: "databases", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "sqlite", package: "@anthropic/mcp-server-sqlite", category: "databases", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "mongodb", package: "mcp-mongodb", category: "databases", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "redis", package: "mcp-redis", category: "databases", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "supabase", package: "mcp-supabase", category: "databases", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "neon", package: "@neondatabase/mcp-server-neon", category: "databases", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Search & Web (8) ────────────────────────────────────────
    McpAlias { name: "perplexity", package: "perplexity-mcp", category: "search", pricing: Freemium, env_var: Some("PERPLEXITY_API_KEY"), key_prefix: Some("pplx-") },
    McpAlias { name: "firecrawl", package: "firecrawl-mcp", category: "search", pricing: Freemium, env_var: Some("FIRECRAWL_API_KEY"), key_prefix: Some("fc-") },
    McpAlias { name: "brave", package: "@anthropic/mcp-server-brave-search", category: "search", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "exa", package: "exa-mcp-server", category: "search", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "tavily", package: "tavily-mcp", category: "search", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "serper", package: "serper-mcp", category: "search", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "searchapi", package: "searchapi-mcp", category: "search", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "bing", package: "bing-mcp", category: "search", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Developer Tools (8) ─────────────────────────────────────
    McpAlias { name: "linear", package: "mcp-linear", category: "developer", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "sentry", package: "@modelcontextprotocol/server-sentry", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "raygun", package: "raygun-mcp", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "buildkite", package: "buildkite-mcp", category: "developer", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "circleci", package: "circleci-mcp", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "vercel", package: "vercel-mcp", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "cloudflare", package: "cloudflare-mcp", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "aws", package: "aws-mcp", category: "developer", pricing: Paid, env_var: None, key_prefix: None },
    // ── Productivity (8) ────────────────────────────────────────
    McpAlias { name: "slack", package: "@anthropic/mcp-server-slack", category: "productivity", pricing: Freemium, env_var: Some("SLACK_BOT_TOKEN"), key_prefix: Some("xoxb-") },
    McpAlias { name: "google-drive", package: "@anthropic/mcp-server-google-drive", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "notion", package: "notion-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "airtable", package: "airtable-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "todoist", package: "todoist-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "asana", package: "asana-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "trello", package: "trello-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "monday", package: "monday-mcp", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    // ── AI & Specialized (8) ────────────────────────────────────
    McpAlias { name: "langchain", package: "langchain-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "e2b", package: "@e2b/mcp-server", category: "ai", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "sequential-thinking", package: "@modelcontextprotocol/server-sequential-thinking", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "context7", package: "context7-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "21st", package: "21st-mcp", category: "ai", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "supadata", package: "supadata-mcp", category: "ai", pricing: Freemium, env_var: Some("SUPADATA_API_KEY"), key_prefix: None },
    McpAlias { name: "dataforseo", package: "dataforseo-mcp", category: "ai", pricing: Paid, env_var: Some("DATAFORSEO_API_KEY"), key_prefix: None },
    McpAlias { name: "ahrefs", package: "ahrefs-mcp", category: "ai", pricing: Paid, env_var: Some("AHREFS_API_KEY"), key_prefix: None },
    // ── AI Image/Media (6) ──────────────────────────────────────
    McpAlias { name: "replicate", package: "replicate-mcp", category: "image", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "comfyui", package: "comfyui-mcp-server", category: "image", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "fal", package: "fal-mcp", category: "image", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "stability", package: "stability-ai-mcp", category: "image", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "elevenlabs", package: "elevenlabs-mcp", category: "image", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "deepgram", package: "deepgram-mcp", category: "image", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Communication (6) ───────────────────────────────────────
    McpAlias { name: "discord", package: "discord-mcp", category: "communication", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "telegram", package: "telegram-mcp", category: "communication", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "resend", package: "resend-mcp", category: "communication", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "sendgrid", package: "sendgrid-mcp", category: "communication", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "twilio", package: "twilio-mcp", category: "communication", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "intercom", package: "intercom-mcp", category: "communication", pricing: Paid, env_var: None, key_prefix: None },
    // ── Vector DB (6) ───────────────────────────────────────────
    McpAlias { name: "pinecone", package: "pinecone-mcp", category: "vectordb", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "weaviate", package: "weaviate-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "qdrant", package: "qdrant-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "chroma", package: "chroma-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "milvus", package: "milvus-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "turbopuffer", package: "turbopuffer-mcp", category: "vectordb", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Analytics & Monitoring (6) ──────────────────────────────
    McpAlias { name: "posthog", package: "posthog-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "mixpanel", package: "mixpanel-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "datadog", package: "datadog-mcp", category: "analytics", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "grafana", package: "grafana-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "prometheus", package: "prometheus-mcp", category: "analytics", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "plausible", package: "plausible-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    // ── E-commerce & Finance (6) ────────────────────────────────
    McpAlias { name: "stripe", package: "stripe-mcp", category: "ecommerce", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "shopify", package: "shopify-mcp", category: "ecommerce", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "paypal", package: "paypal-mcp", category: "ecommerce", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "polygon", package: "polygon-mcp", category: "ecommerce", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "coinbase", package: "coinbase-mcp", category: "ecommerce", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "alpaca", package: "alpaca-mcp", category: "ecommerce", pricing: Freemium, env_var: None, key_prefix: None },
    // ── CMS & Content (6) ───────────────────────────────────────
    McpAlias { name: "wordpress", package: "wordpress-mcp", category: "cms", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "contentful", package: "contentful-mcp", category: "cms", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "sanity", package: "sanity-mcp", category: "cms", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "strapi", package: "strapi-mcp", category: "cms", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "ghost", package: "ghost-mcp", category: "cms", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "hubspot", package: "hubspot-mcp", category: "cms", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Infrastructure & DevOps (6) ─────────────────────────────
    McpAlias { name: "docker", package: "docker-mcp", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "kubernetes", package: "kubernetes-mcp", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "terraform", package: "terraform-mcp", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "pulumi", package: "pulumi-mcp", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "fly", package: "fly-mcp", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "railway", package: "railway-mcp", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Social Media (6) ────────────────────────────────────────
    McpAlias { name: "twitter", package: "twitter-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "linkedin", package: "linkedin-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "youtube", package: "youtube-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "tiktok", package: "tiktok-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "reddit", package: "reddit-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "mastodon", package: "mastodon-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    // ── Lifestyle (8) ───────────────────────────────────────────
    McpAlias { name: "spotify", package: "spotify-mcp", category: "lifestyle", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "airbnb", package: "airbnb-mcp", category: "lifestyle", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "uber", package: "uber-mcp", category: "lifestyle", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "openweather", package: "openweather-mcp", category: "lifestyle", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "news-api", package: "newsapi-mcp", category: "lifestyle", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "recipe", package: "recipe-mcp", category: "lifestyle", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "imdb", package: "imdb-mcp", category: "lifestyle", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "wolfram", package: "wolfram-mcp", category: "lifestyle", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Marketing (6) ───────────────────────────────────────────
    McpAlias { name: "mailchimp", package: "mailchimp-mcp", category: "marketing", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "google-ads", package: "google-ads-mcp", category: "marketing", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "meta-ads", package: "meta-ads-mcp", category: "marketing", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "buffer", package: "buffer-mcp", category: "marketing", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "semrush", package: "semrush-mcp", category: "marketing", pricing: Paid, env_var: None, key_prefix: None },
    McpAlias { name: "canva", package: "canva-mcp", category: "marketing", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Maps & Location (3) ─────────────────────────────────────
    McpAlias { name: "mapbox", package: "mapbox-mcp", category: "maps", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "here", package: "here-mcp", category: "maps", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "ipinfo", package: "ipinfo-mcp", category: "maps", pricing: Freemium, env_var: None, key_prefix: None },
];

// ═══════════════════════════════════════════════════════════════════════════
// phf + unicase index map (case-insensitive lookup → array index)
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive name → index into `ALL_MCP_ALIASES`.
static MCP_INDEX: phf::Map<UniCase<&'static str>, usize> = phf_map! {
    UniCase::ascii("filesystem") => 0, UniCase::ascii("memory") => 1,
    UniCase::ascii("puppeteer") => 2, UniCase::ascii("brave-search") => 3,
    UniCase::ascii("google-maps") => 4, UniCase::ascii("fetch") => 5,
    UniCase::ascii("github") => 6, UniCase::ascii("gitlab") => 7,
    UniCase::ascii("neo4j") => 8, UniCase::ascii("postgres") => 9,
    UniCase::ascii("mysql") => 10, UniCase::ascii("sqlite") => 11,
    UniCase::ascii("mongodb") => 12, UniCase::ascii("redis") => 13,
    UniCase::ascii("supabase") => 14, UniCase::ascii("neon") => 15,
    UniCase::ascii("perplexity") => 16, UniCase::ascii("firecrawl") => 17,
    UniCase::ascii("brave") => 18, UniCase::ascii("exa") => 19,
    UniCase::ascii("tavily") => 20, UniCase::ascii("serper") => 21,
    UniCase::ascii("searchapi") => 22, UniCase::ascii("bing") => 23,
    UniCase::ascii("linear") => 24, UniCase::ascii("sentry") => 25,
    UniCase::ascii("raygun") => 26, UniCase::ascii("buildkite") => 27,
    UniCase::ascii("circleci") => 28, UniCase::ascii("vercel") => 29,
    UniCase::ascii("cloudflare") => 30, UniCase::ascii("aws") => 31,
    UniCase::ascii("slack") => 32, UniCase::ascii("google-drive") => 33,
    UniCase::ascii("notion") => 34, UniCase::ascii("airtable") => 35,
    UniCase::ascii("todoist") => 36, UniCase::ascii("asana") => 37,
    UniCase::ascii("trello") => 38, UniCase::ascii("monday") => 39,
    UniCase::ascii("langchain") => 40, UniCase::ascii("e2b") => 41,
    UniCase::ascii("sequential-thinking") => 42, UniCase::ascii("context7") => 43,
    UniCase::ascii("21st") => 44, UniCase::ascii("supadata") => 45,
    UniCase::ascii("dataforseo") => 46, UniCase::ascii("ahrefs") => 47,
    UniCase::ascii("replicate") => 48, UniCase::ascii("comfyui") => 49,
    UniCase::ascii("fal") => 50, UniCase::ascii("stability") => 51,
    UniCase::ascii("elevenlabs") => 52, UniCase::ascii("deepgram") => 53,
    UniCase::ascii("discord") => 54, UniCase::ascii("telegram") => 55,
    UniCase::ascii("resend") => 56, UniCase::ascii("sendgrid") => 57,
    UniCase::ascii("twilio") => 58, UniCase::ascii("intercom") => 59,
    UniCase::ascii("pinecone") => 60, UniCase::ascii("weaviate") => 61,
    UniCase::ascii("qdrant") => 62, UniCase::ascii("chroma") => 63,
    UniCase::ascii("milvus") => 64, UniCase::ascii("turbopuffer") => 65,
    UniCase::ascii("posthog") => 66, UniCase::ascii("mixpanel") => 67,
    UniCase::ascii("datadog") => 68, UniCase::ascii("grafana") => 69,
    UniCase::ascii("prometheus") => 70, UniCase::ascii("plausible") => 71,
    UniCase::ascii("stripe") => 72, UniCase::ascii("shopify") => 73,
    UniCase::ascii("paypal") => 74, UniCase::ascii("polygon") => 75,
    UniCase::ascii("coinbase") => 76, UniCase::ascii("alpaca") => 77,
    UniCase::ascii("wordpress") => 78, UniCase::ascii("contentful") => 79,
    UniCase::ascii("sanity") => 80, UniCase::ascii("strapi") => 81,
    UniCase::ascii("ghost") => 82, UniCase::ascii("hubspot") => 83,
    UniCase::ascii("docker") => 84, UniCase::ascii("kubernetes") => 85,
    UniCase::ascii("terraform") => 86, UniCase::ascii("pulumi") => 87,
    UniCase::ascii("fly") => 88, UniCase::ascii("railway") => 89,
    UniCase::ascii("twitter") => 90, UniCase::ascii("linkedin") => 91,
    UniCase::ascii("youtube") => 92, UniCase::ascii("tiktok") => 93,
    UniCase::ascii("reddit") => 94, UniCase::ascii("mastodon") => 95,
    UniCase::ascii("spotify") => 96, UniCase::ascii("airbnb") => 97,
    UniCase::ascii("uber") => 98, UniCase::ascii("openweather") => 99,
    UniCase::ascii("news-api") => 100, UniCase::ascii("recipe") => 101,
    UniCase::ascii("imdb") => 102, UniCase::ascii("wolfram") => 103,
    UniCase::ascii("mailchimp") => 104, UniCase::ascii("google-ads") => 105,
    UniCase::ascii("meta-ads") => 106, UniCase::ascii("buffer") => 107,
    UniCase::ascii("semrush") => 108, UniCase::ascii("canva") => 109,
    UniCase::ascii("mapbox") => 110, UniCase::ascii("here") => 111,
    UniCase::ascii("ipinfo") => 112,
};

/// Find an MCP alias by name (case-insensitive, O(1) via phf).
#[must_use]
pub fn find_mcp_alias(name: &str) -> Option<&'static McpAlias> {
    MCP_INDEX
        .get(&UniCase::ascii(name))
        .map(|&i| &ALL_MCP_ALIASES[i])
}

/// Check if a name is a known MCP alias.
#[must_use]
pub fn is_known_mcp_alias(name: &str) -> bool {
    MCP_INDEX.contains_key(&UniCase::ascii(name))
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn mcp_alias_count() {
        assert_eq!(ALL_MCP_ALIASES.len(), 113);
    }

    #[test]
    fn phf_index_count_matches_array() {
        assert_eq!(MCP_INDEX.len(), ALL_MCP_ALIASES.len());
    }

    #[test]
    fn phf_indices_valid() {
        for (name, &idx) in &MCP_INDEX {
            assert!(
                idx < ALL_MCP_ALIASES.len(),
                "index {idx} for `{name}` out of bounds"
            );
            assert_eq!(
                ALL_MCP_ALIASES[idx].name.to_lowercase(),
                name.as_ref().to_lowercase(),
                "index {idx} points to wrong alias"
            );
        }
    }

    #[test]
    fn find_known_aliases() {
        let names = ["neo4j", "github", "slack", "perplexity", "stripe"];
        for name in names {
            assert!(
                find_mcp_alias(name).is_some(),
                "MCP alias `{name}` not found"
            );
        }
    }

    #[test]
    fn case_insensitive_lookup() {
        let cases = ["Neo4j", "NEO4J", "GitHub", "SLACK", "Perplexity"];
        for name in cases {
            assert!(
                find_mcp_alias(name).is_some(),
                "case-insensitive lookup failed for `{name}`"
            );
        }
    }

    #[test]
    fn unknown_returns_none() {
        assert!(find_mcp_alias("nonexistent").is_none());
        assert!(find_mcp_alias("").is_none());
    }

    #[test]
    fn ex_mcp_providers_have_env_var() {
        let ex_providers = [
            ("neo4j", "NEO4J_PASSWORD"),
            ("github", "GITHUB_TOKEN"),
            ("slack", "SLACK_BOT_TOKEN"),
            ("perplexity", "PERPLEXITY_API_KEY"),
            ("firecrawl", "FIRECRAWL_API_KEY"),
            ("supadata", "SUPADATA_API_KEY"),
            ("dataforseo", "DATAFORSEO_API_KEY"),
            ("ahrefs", "AHREFS_API_KEY"),
            ("postgres", "POSTGRES_URL"),
            ("filesystem", "FILESYSTEM_ALLOWED_PATHS"),
            ("memory", "MEMORY_STORAGE_PATH"),
        ];
        for (name, expected_env) in ex_providers {
            let alias = find_mcp_alias(name).unwrap_or_else(|| panic!("ex-MCP `{name}` not found"));
            assert_eq!(
                alias.env_var,
                Some(expected_env),
                "ex-MCP `{name}` should have env_var `{expected_env}`"
            );
        }
    }

    #[test]
    fn no_duplicate_names() {
        let mut seen = std::collections::HashSet::new();
        for alias in ALL_MCP_ALIASES {
            assert!(
                seen.insert(alias.name),
                "duplicate MCP alias: `{}`",
                alias.name
            );
        }
    }

    #[test]
    fn is_known_returns_true_for_known() {
        assert!(is_known_mcp_alias("neo4j"));
        assert!(is_known_mcp_alias("github"));
    }

    #[test]
    fn is_known_returns_false_for_unknown() {
        assert!(!is_known_mcp_alias("nonexistent"));
        assert!(!is_known_mcp_alias(""));
    }

    #[test]
    fn all_have_non_empty_fields() {
        for alias in ALL_MCP_ALIASES {
            assert!(!alias.name.is_empty());
            assert!(!alias.package.is_empty());
            assert!(!alias.category.is_empty());
        }
    }
}
