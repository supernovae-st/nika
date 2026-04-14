// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static MCP server alias catalog — 102 aliases with phf+unicase lookup.
//!
//! Decision B (locked): 11 ex-MCP "providers" from legacy now live here with
//! optional `env_var` and `key_prefix` for secret management.
//!
//! 2026-04-14: Phase A cleanup — removed 29 broken entries (7 deprecated
//! Anthropic reference servers, 19 non-existent npm packages, 3 zero-download
//! abandoned forks). Phase C (TOML + Distribution enum + xtask verifier) will
//! restore missing coverage via pypi/oci/remote distributions.

use phf::phf_map;
use unicase::UniCase;

use crate::types::mcp_alias::{McpAlias, McpPricing};

use McpPricing::{Free, Freemium, Paid};

/// All 102 MCP server aliases.
///
/// Categories (17): anthropic(3), databases(7), search(7), developer(17),
/// productivity(12), ai(8), image(3), audio(0), communication(5), vectordb(2),
/// analytics(6), ecommerce(6), cms(6), devops(6), social(5), lifestyle(5),
/// marketing(3), maps(1).
pub static ALL_MCP_ALIASES: &[McpAlias] = &[
    // ── Anthropic Official (3) ──────────────────────────────────
    McpAlias { name: "filesystem", package: "@modelcontextprotocol/server-filesystem", category: "anthropic", pricing: Free, env_var: Some("FILESYSTEM_ALLOWED_PATHS"), key_prefix: None },
    McpAlias { name: "memory", package: "@modelcontextprotocol/server-memory", category: "anthropic", pricing: Free, env_var: Some("MEMORY_STORAGE_PATH"), key_prefix: None },
    McpAlias { name: "pdf", package: "@modelcontextprotocol/server-pdf", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    // ── Databases (7) ───────────────────────────────────────────
    McpAlias { name: "neo4j", package: "@johnymontana/neo4j-mcp", category: "databases", pricing: Freemium, env_var: Some("NEO4J_PASSWORD"), key_prefix: None },
    McpAlias { name: "mysql", package: "mcp-server-mysql", category: "databases", pricing: Free, env_var: Some("MYSQL_URL"), key_prefix: None },
    McpAlias { name: "mongodb", package: "mongodb-mcp-server", category: "databases", pricing: Free, env_var: Some("MDB_MCP_CONNECTION_STRING"), key_prefix: None },
    McpAlias { name: "redis", package: "redis-mcp", category: "databases", pricing: Free, env_var: Some("REDIS_URL"), key_prefix: None },
    McpAlias { name: "supabase", package: "supabase-mcp", category: "databases", pricing: Freemium, env_var: Some("SUPABASE_URL"), key_prefix: None },
    McpAlias { name: "upstash", package: "@upstash/mcp-server", category: "databases", pricing: Freemium, env_var: Some("UPSTASH_API_KEY"), key_prefix: None },
    McpAlias { name: "turso", package: "mcp-server-turso", category: "databases", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Search & Web (7) ────────────────────────────────────────
    McpAlias { name: "perplexity", package: "perplexity-mcp", category: "search", pricing: Freemium, env_var: Some("PERPLEXITY_API_KEY"), key_prefix: Some("pplx-") },
    McpAlias { name: "firecrawl", package: "firecrawl-mcp", category: "search", pricing: Freemium, env_var: Some("FIRECRAWL_API_KEY"), key_prefix: Some("fc-") },
    McpAlias { name: "exa", package: "exa-mcp-server", category: "search", pricing: Freemium, env_var: Some("EXA_API_KEY"), key_prefix: None },
    McpAlias { name: "tavily", package: "tavily-mcp", category: "search", pricing: Freemium, env_var: Some("TAVILY_API_KEY"), key_prefix: Some("tvly-") },
    McpAlias { name: "serper", package: "mcp-server-serper", category: "search", pricing: Paid, env_var: Some("SERPER_API_KEY"), key_prefix: None },
    McpAlias { name: "searchapi", package: "searchapi-mcp", category: "search", pricing: Paid, env_var: Some("SEARCHAPI_API_KEY"), key_prefix: None },
    McpAlias { name: "apify", package: "@apify/actors-mcp-server", category: "search", pricing: Freemium, env_var: Some("APIFY_TOKEN"), key_prefix: None },
    // ── Developer Tools (17) ────────────────────────────────────
    McpAlias { name: "linear", package: "mcp-server-linear", category: "developer", pricing: Paid, env_var: Some("LINEAR_API_KEY"), key_prefix: Some("lin_api_") },
    McpAlias { name: "sentry", package: "@sentry/mcp-server", category: "developer", pricing: Freemium, env_var: Some("SENTRY_AUTH_TOKEN"), key_prefix: None },
    McpAlias { name: "circleci", package: "@circleci/mcp-server-circleci", category: "developer", pricing: Freemium, env_var: Some("CIRCLECI_TOKEN"), key_prefix: None },
    McpAlias { name: "vercel", package: "vercel-mcp", category: "developer", pricing: Freemium, env_var: Some("VERCEL_TOKEN"), key_prefix: None },
    McpAlias { name: "cloudflare", package: "@cloudflare/mcp-server-cloudflare", category: "developer", pricing: Freemium, env_var: Some("CLOUDFLARE_API_TOKEN"), key_prefix: None },
    McpAlias { name: "aws", package: "aws-mcp", category: "developer", pricing: Paid, env_var: Some("AWS_ACCESS_KEY_ID"), key_prefix: None },
    McpAlias { name: "playwright", package: "@playwright/mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "browserbase", package: "@browserbasehq/mcp", category: "developer", pricing: Freemium, env_var: Some("BROWSERBASE_API_KEY"), key_prefix: None },
    McpAlias { name: "figma", package: "figma-developer-mcp", category: "developer", pricing: Freemium, env_var: Some("FIGMA_API_KEY"), key_prefix: None },
    McpAlias { name: "xcode", package: "xcodebuildmcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "eslint", package: "@eslint/mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "nx", package: "nx-mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "launchdarkly", package: "@launchdarkly/mcp-server", category: "developer", pricing: Freemium, env_var: Some("LD_ACCESS_TOKEN"), key_prefix: None },
    McpAlias { name: "postman", package: "@postman/postman-mcp-server", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "bitbucket", package: "@nexus2520/bitbucket-mcp-server", category: "developer", pricing: Freemium, env_var: Some("BITBUCKET_TOKEN"), key_prefix: None },
    McpAlias { name: "chrome-devtools", package: "chrome-devtools-mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "browserstack", package: "@browserstack/mcp-server", category: "developer", pricing: Paid, env_var: Some("BROWSERSTACK_ACCESS_KEY"), key_prefix: None },
    // ── Productivity (12) ───────────────────────────────────────
    McpAlias { name: "slack", package: "@modelcontextprotocol/server-slack", category: "productivity", pricing: Freemium, env_var: Some("SLACK_BOT_TOKEN"), key_prefix: Some("xoxb-") },
    McpAlias { name: "google-drive", package: "@modelcontextprotocol/server-gdrive", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "notion", package: "@notionhq/notion-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("NOTION_TOKEN"), key_prefix: Some("ntn_") },
    McpAlias { name: "airtable", package: "airtable-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("AIRTABLE_API_KEY"), key_prefix: Some("pat") },
    McpAlias { name: "monday", package: "@mondaydotcomorg/monday-api-mcp", category: "productivity", pricing: Freemium, env_var: Some("MONDAY_API_TOKEN"), key_prefix: None },
    McpAlias { name: "obsidian", package: "obsidian-mcp", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "jira", package: "jira-mcp", category: "productivity", pricing: Paid, env_var: Some("JIRA_API_KEY"), key_prefix: None },
    McpAlias { name: "zendesk", package: "zendesk-mcp", category: "productivity", pricing: Paid, env_var: Some("ZENDESK_API_TOKEN"), key_prefix: None },
    McpAlias { name: "clickup", package: "@taazkareem/clickup-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("CLICKUP_API_KEY"), key_prefix: None },
    McpAlias { name: "n8n", package: "n8n-mcp", category: "productivity", pricing: Freemium, env_var: Some("N8N_API_KEY"), key_prefix: None },
    McpAlias { name: "google-calendar", package: "@cocal/google-calendar-mcp", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "excel", package: "@negokaz/excel-mcp-server", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    // ── AI & Specialized (8) ────────────────────────────────────
    McpAlias { name: "langchain", package: "langchain-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "e2b", package: "@e2b/mcp-server", category: "ai", pricing: Freemium, env_var: Some("E2B_API_KEY"), key_prefix: None },
    McpAlias { name: "sequential-thinking", package: "@modelcontextprotocol/server-sequential-thinking", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "context7", package: "@upstash/context7-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "21st", package: "@21st-dev/magic", category: "ai", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "supadata", package: "@supadata/mcp", category: "ai", pricing: Freemium, env_var: Some("SUPADATA_API_KEY"), key_prefix: None },
    McpAlias { name: "dataforseo", package: "dataforseo-mcp-server", category: "ai", pricing: Paid, env_var: Some("DATAFORSEO_API_KEY"), key_prefix: None },
    McpAlias { name: "ahrefs", package: "@ahrefs/mcp", category: "ai", pricing: Paid, env_var: Some("AHREFS_API_KEY"), key_prefix: None },
    // ── AI Image Generation (3) ─────────────────────────────────
    McpAlias { name: "replicate", package: "replicate-mcp", category: "image", pricing: Paid, env_var: Some("REPLICATE_API_TOKEN"), key_prefix: Some("r8_") },
    McpAlias { name: "comfyui", package: "comfyui-mcp", category: "image", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "fal", package: "fal-mcp", category: "image", pricing: Paid, env_var: Some("FAL_KEY"), key_prefix: None },
    // ── Communication (5) ───────────────────────────────────────
    McpAlias { name: "discord", package: "discord-mcp", category: "communication", pricing: Free, env_var: Some("DISCORD_BOT_TOKEN"), key_prefix: None },
    McpAlias { name: "telegram", package: "telegram-mcp", category: "communication", pricing: Free, env_var: Some("TELEGRAM_BOT_TOKEN"), key_prefix: None },
    McpAlias { name: "resend", package: "resend-mcp", category: "communication", pricing: Freemium, env_var: Some("RESEND_API_KEY"), key_prefix: Some("re_") },
    McpAlias { name: "sendgrid", package: "sendgrid-mcp", category: "communication", pricing: Freemium, env_var: Some("SENDGRID_API_KEY"), key_prefix: Some("SG.") },
    McpAlias { name: "twilio", package: "twilio-mcp", category: "communication", pricing: Paid, env_var: Some("TWILIO_AUTH_TOKEN"), key_prefix: None },
    // ── Vector DB (2) ───────────────────────────────────────────
    McpAlias { name: "pinecone", package: "@pinecone-database/mcp", category: "vectordb", pricing: Freemium, env_var: Some("PINECONE_API_KEY"), key_prefix: None },
    McpAlias { name: "turbopuffer", package: "@turbopuffer/turbopuffer-mcp", category: "vectordb", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Analytics & Monitoring (6) ──────────────────────────────
    McpAlias { name: "mixpanel", package: "mixpanel-mcp-server", category: "analytics", pricing: Freemium, env_var: Some("MIXPANEL_TOKEN"), key_prefix: None },
    McpAlias { name: "datadog", package: "@winor30/mcp-server-datadog", category: "analytics", pricing: Paid, env_var: Some("DATADOG_API_KEY"), key_prefix: None },
    McpAlias { name: "prometheus", package: "prometheus-mcp", category: "analytics", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "plausible", package: "plausible-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "axiom", package: "mcp-server-axiom", category: "analytics", pricing: Freemium, env_var: Some("AXIOM_TOKEN"), key_prefix: None },
    McpAlias { name: "dynatrace", package: "@dynatrace-oss/dynatrace-mcp-server", category: "analytics", pricing: Paid, env_var: Some("DYNATRACE_API_TOKEN"), key_prefix: None },
    // ── E-commerce & Finance (6) ────────────────────────────────
    McpAlias { name: "stripe", package: "@stripe/mcp", category: "ecommerce", pricing: Paid, env_var: Some("STRIPE_SECRET_KEY"), key_prefix: Some("sk_") },
    McpAlias { name: "shopify", package: "shopify-mcp", category: "ecommerce", pricing: Paid, env_var: Some("SHOPIFY_ACCESS_TOKEN"), key_prefix: Some("shpat_") },
    McpAlias { name: "paypal", package: "@paypal/mcp", category: "ecommerce", pricing: Paid, env_var: Some("PAYPAL_CLIENT_ID"), key_prefix: None },
    McpAlias { name: "polygon", package: "polygon-mcp", category: "ecommerce", pricing: Freemium, env_var: Some("POLYGON_API_KEY"), key_prefix: None },
    McpAlias { name: "alpaca", package: "alpaca-mcp", category: "ecommerce", pricing: Freemium, env_var: Some("ALPACA_API_KEY"), key_prefix: None },
    McpAlias { name: "salesforce", package: "@salesforce/mcp", category: "ecommerce", pricing: Freemium, env_var: None, key_prefix: None },
    // ── CMS & Content (6) ───────────────────────────────────────
    McpAlias { name: "wordpress", package: "wordpress-mcp", category: "cms", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "contentful", package: "@contentful/mcp-server", category: "cms", pricing: Freemium, env_var: Some("CONTENTFUL_MANAGEMENT_ACCESS_TOKEN"), key_prefix: None },
    McpAlias { name: "sanity", package: "@sanity/mcp-server", category: "cms", pricing: Freemium, env_var: Some("SANITY_API_TOKEN"), key_prefix: None },
    McpAlias { name: "strapi", package: "strapi-mcp", category: "cms", pricing: Free, env_var: Some("STRAPI_API_TOKEN"), key_prefix: None },
    McpAlias { name: "ghost", package: "ghost-mcp", category: "cms", pricing: Freemium, env_var: Some("GHOST_ADMIN_API_KEY"), key_prefix: None },
    McpAlias { name: "hubspot", package: "@hubspot/mcp-server", category: "cms", pricing: Freemium, env_var: Some("HUBSPOT_ACCESS_TOKEN"), key_prefix: None },
    // ── Infrastructure & DevOps (6) ─────────────────────────────
    McpAlias { name: "docker", package: "docker-mcp", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "kubernetes", package: "mcp-server-kubernetes", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "terraform", package: "terraform-mcp-server", category: "devops", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "pulumi", package: "@pulumi/mcp-server", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "railway", package: "@railway/mcp-server", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "heroku", package: "@heroku/mcp-server", category: "devops", pricing: Freemium, env_var: Some("HEROKU_API_KEY"), key_prefix: None },
    // ── Social Media (5) ────────────────────────────────────────
    McpAlias { name: "twitter", package: "twitter-mcp", category: "social", pricing: Paid, env_var: Some("TWITTER_BEARER_TOKEN"), key_prefix: None },
    McpAlias { name: "linkedin", package: "linkedin-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "youtube", package: "youtube-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "tiktok", package: "tiktok-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "reddit", package: "reddit-mcp", category: "social", pricing: Free, env_var: None, key_prefix: None },
    // ── Lifestyle (5) ───────────────────────────────────────────
    McpAlias { name: "spotify", package: "spotify-mcp", category: "lifestyle", pricing: Freemium, env_var: Some("SPOTIFY_CLIENT_ID"), key_prefix: None },
    McpAlias { name: "openweather", package: "openweather-mcp", category: "lifestyle", pricing: Freemium, env_var: Some("OPENWEATHER_API_KEY"), key_prefix: None },
    McpAlias { name: "news-api", package: "newsapi-mcp", category: "lifestyle", pricing: Freemium, env_var: Some("NEWS_API_KEY"), key_prefix: None },
    McpAlias { name: "recipe", package: "recipe-mcp", category: "lifestyle", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "wolfram", package: "wolfram-mcp", category: "lifestyle", pricing: Freemium, env_var: Some("WOLFRAM_APP_ID"), key_prefix: None },
    // ── Marketing (3) ───────────────────────────────────────────
    McpAlias { name: "mailchimp", package: "mailchimp-mcp", category: "marketing", pricing: Freemium, env_var: Some("MAILCHIMP_API_KEY"), key_prefix: None },
    McpAlias { name: "google-ads", package: "google-ads-mcp", category: "marketing", pricing: Paid, env_var: Some("GOOGLE_ADS_DEVELOPER_TOKEN"), key_prefix: None },
    McpAlias { name: "meta-ads", package: "meta-ads-mcp", category: "marketing", pricing: Paid, env_var: Some("META_ACCESS_TOKEN"), key_prefix: None },
    // ── Maps & Location (1) ─────────────────────────────────────
    McpAlias { name: "mapbox", package: "@mapbox/mcp-server", category: "maps", pricing: Freemium, env_var: Some("MAPBOX_ACCESS_TOKEN"), key_prefix: Some("pk.") },
];

// ═══════════════════════════════════════════════════════════════════════════
// phf + unicase index map (case-insensitive lookup → array index)
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive name → index into `ALL_MCP_ALIASES`.
static MCP_INDEX: phf::Map<UniCase<&'static str>, usize> = phf_map! {
    // anthropic (0-2)
    UniCase::ascii("filesystem") => 0, UniCase::ascii("memory") => 1,
    UniCase::ascii("pdf") => 2,
    // databases (3-9)
    UniCase::ascii("neo4j") => 3, UniCase::ascii("mysql") => 4,
    UniCase::ascii("mongodb") => 5, UniCase::ascii("redis") => 6,
    UniCase::ascii("supabase") => 7, UniCase::ascii("upstash") => 8,
    UniCase::ascii("turso") => 9,
    // search (10-16)
    UniCase::ascii("perplexity") => 10, UniCase::ascii("firecrawl") => 11,
    UniCase::ascii("exa") => 12, UniCase::ascii("tavily") => 13,
    UniCase::ascii("serper") => 14, UniCase::ascii("searchapi") => 15,
    UniCase::ascii("apify") => 16,
    // developer (17-33)
    UniCase::ascii("linear") => 17, UniCase::ascii("sentry") => 18,
    UniCase::ascii("circleci") => 19, UniCase::ascii("vercel") => 20,
    UniCase::ascii("cloudflare") => 21, UniCase::ascii("aws") => 22,
    UniCase::ascii("playwright") => 23, UniCase::ascii("browserbase") => 24,
    UniCase::ascii("figma") => 25, UniCase::ascii("xcode") => 26,
    UniCase::ascii("eslint") => 27, UniCase::ascii("nx") => 28,
    UniCase::ascii("launchdarkly") => 29, UniCase::ascii("postman") => 30,
    UniCase::ascii("bitbucket") => 31, UniCase::ascii("chrome-devtools") => 32,
    UniCase::ascii("browserstack") => 33,
    // productivity (34-45)
    UniCase::ascii("slack") => 34, UniCase::ascii("google-drive") => 35,
    UniCase::ascii("notion") => 36, UniCase::ascii("airtable") => 37,
    UniCase::ascii("monday") => 38, UniCase::ascii("obsidian") => 39,
    UniCase::ascii("jira") => 40, UniCase::ascii("zendesk") => 41,
    UniCase::ascii("clickup") => 42, UniCase::ascii("n8n") => 43,
    UniCase::ascii("google-calendar") => 44, UniCase::ascii("excel") => 45,
    // ai (46-53)
    UniCase::ascii("langchain") => 46, UniCase::ascii("e2b") => 47,
    UniCase::ascii("sequential-thinking") => 48, UniCase::ascii("context7") => 49,
    UniCase::ascii("21st") => 50, UniCase::ascii("supadata") => 51,
    UniCase::ascii("dataforseo") => 52, UniCase::ascii("ahrefs") => 53,
    // image (54-56)
    UniCase::ascii("replicate") => 54, UniCase::ascii("comfyui") => 55,
    UniCase::ascii("fal") => 56,
    // communication (57-61)
    UniCase::ascii("discord") => 57, UniCase::ascii("telegram") => 58,
    UniCase::ascii("resend") => 59, UniCase::ascii("sendgrid") => 60,
    UniCase::ascii("twilio") => 61,
    // vectordb (62-63)
    UniCase::ascii("pinecone") => 62, UniCase::ascii("turbopuffer") => 63,
    // analytics (64-69)
    UniCase::ascii("mixpanel") => 64, UniCase::ascii("datadog") => 65,
    UniCase::ascii("prometheus") => 66, UniCase::ascii("plausible") => 67,
    UniCase::ascii("axiom") => 68, UniCase::ascii("dynatrace") => 69,
    // ecommerce (70-75)
    UniCase::ascii("stripe") => 70, UniCase::ascii("shopify") => 71,
    UniCase::ascii("paypal") => 72, UniCase::ascii("polygon") => 73,
    UniCase::ascii("alpaca") => 74, UniCase::ascii("salesforce") => 75,
    // cms (76-81)
    UniCase::ascii("wordpress") => 76, UniCase::ascii("contentful") => 77,
    UniCase::ascii("sanity") => 78, UniCase::ascii("strapi") => 79,
    UniCase::ascii("ghost") => 80, UniCase::ascii("hubspot") => 81,
    // devops (82-87)
    UniCase::ascii("docker") => 82, UniCase::ascii("kubernetes") => 83,
    UniCase::ascii("terraform") => 84, UniCase::ascii("pulumi") => 85,
    UniCase::ascii("railway") => 86, UniCase::ascii("heroku") => 87,
    // social (88-92)
    UniCase::ascii("twitter") => 88, UniCase::ascii("linkedin") => 89,
    UniCase::ascii("youtube") => 90, UniCase::ascii("tiktok") => 91,
    UniCase::ascii("reddit") => 92,
    // lifestyle (93-97)
    UniCase::ascii("spotify") => 93, UniCase::ascii("openweather") => 94,
    UniCase::ascii("news-api") => 95, UniCase::ascii("recipe") => 96,
    UniCase::ascii("wolfram") => 97,
    // marketing (98-100)
    UniCase::ascii("mailchimp") => 98, UniCase::ascii("google-ads") => 99,
    UniCase::ascii("meta-ads") => 100,
    // maps (101)
    UniCase::ascii("mapbox") => 101,
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
        assert_eq!(ALL_MCP_ALIASES.len(), 102);
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
        let names = ["neo4j", "slack", "perplexity", "stripe", "filesystem"];
        for name in names {
            assert!(
                find_mcp_alias(name).is_some(),
                "MCP alias `{name}` not found"
            );
        }
    }

    #[test]
    fn case_insensitive_lookup() {
        let cases = ["Neo4j", "NEO4J", "SLACK", "Perplexity"];
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
    fn removed_broken_aliases_not_present() {
        // Phase A cleanup 2026-04-14: these 29 were removed.
        // Any reappearance is a regression.
        let removed = [
            // Deprecated Anthropic reference servers
            "puppeteer", "brave-search", "brave", "google-maps", "fetch",
            "github", "gitlab", "postgres", "neon",
            // Non-existent on npm
            "sqlite", "stability", "deepgram", "intercom", "weaviate",
            "qdrant", "chroma", "milvus", "posthog", "grafana", "coinbase",
            "fly", "asana", "raygun", "buildkite", "bing", "elevenlabs",
            // Zero weekly downloads
            "todoist", "trello", "semrush",
        ];
        for name in removed {
            assert!(
                find_mcp_alias(name).is_none(),
                "broken alias `{name}` must not be re-added without verification"
            );
        }
    }

    #[test]
    fn ex_mcp_providers_have_env_var() {
        let ex_providers = [
            ("neo4j", "NEO4J_PASSWORD"),
            ("slack", "SLACK_BOT_TOKEN"),
            ("perplexity", "PERPLEXITY_API_KEY"),
            ("firecrawl", "FIRECRAWL_API_KEY"),
            ("supadata", "SUPADATA_API_KEY"),
            ("dataforseo", "DATAFORSEO_API_KEY"),
            ("ahrefs", "AHREFS_API_KEY"),
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
        assert!(is_known_mcp_alias("filesystem"));
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
