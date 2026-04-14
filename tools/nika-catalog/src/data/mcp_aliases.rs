// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static MCP server alias catalog — 131 aliases with phf+unicase lookup.
//!
//! Decision B (locked): 11 ex-MCP "providers" from legacy now live here with
//! optional `env_var` and `key_prefix` for secret management.

use phf::phf_map;
use unicase::UniCase;

use crate::types::mcp_alias::{McpAlias, McpPricing};

use McpPricing::{Free, Freemium, Paid};

/// All 131 MCP server aliases.
///
/// Categories (17): anthropic(9), databases(10), search(9), developer(19),
/// productivity(15), ai(8), image(4), audio(2), communication(6), vectordb(6),
/// analytics(8), ecommerce(7), cms(6), devops(7), social(5), lifestyle(5),
/// marketing(4), maps(1).
pub static ALL_MCP_ALIASES: &[McpAlias] = &[
    // ── Anthropic Official (8) ──────────────────────────────────
    McpAlias { name: "filesystem", package: "@modelcontextprotocol/server-filesystem", category: "anthropic", pricing: Free, env_var: Some("FILESYSTEM_ALLOWED_PATHS"), key_prefix: None },
    McpAlias { name: "memory", package: "@modelcontextprotocol/server-memory", category: "anthropic", pricing: Free, env_var: Some("MEMORY_STORAGE_PATH"), key_prefix: None },
    McpAlias { name: "puppeteer", package: "@modelcontextprotocol/server-puppeteer", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "brave-search", package: "@modelcontextprotocol/server-brave-search", category: "anthropic", pricing: Free, env_var: Some("BRAVE_API_KEY"), key_prefix: None },
    McpAlias { name: "google-maps", package: "@modelcontextprotocol/server-google-maps", category: "anthropic", pricing: Free, env_var: Some("GOOGLE_MAPS_API_KEY"), key_prefix: None },
    McpAlias { name: "fetch", package: "@modelcontextprotocol/server-fetch", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "github", package: "@modelcontextprotocol/server-github", category: "anthropic", pricing: Free, env_var: Some("GITHUB_TOKEN"), key_prefix: Some("ghp_") },
    McpAlias { name: "gitlab", package: "@modelcontextprotocol/server-gitlab", category: "anthropic", pricing: Free, env_var: Some("GITLAB_TOKEN"), key_prefix: Some("glpat-") },
    // ── Databases (8) ───────────────────────────────────────────
    McpAlias { name: "neo4j", package: "@johnymontana/neo4j-mcp", category: "databases", pricing: Freemium, env_var: Some("NEO4J_PASSWORD"), key_prefix: None },
    McpAlias { name: "postgres", package: "@modelcontextprotocol/server-postgres", category: "databases", pricing: Free, env_var: Some("POSTGRES_URL"), key_prefix: None },
    McpAlias { name: "mysql", package: "mcp-server-mysql", category: "databases", pricing: Free, env_var: Some("MYSQL_URL"), key_prefix: None },
    McpAlias { name: "sqlite", package: "mcp-server-sqlite", category: "databases", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "mongodb", package: "mongodb-mcp-server", category: "databases", pricing: Free, env_var: Some("MDB_MCP_CONNECTION_STRING"), key_prefix: None },
    McpAlias { name: "redis", package: "redis-mcp", category: "databases", pricing: Free, env_var: Some("REDIS_URL"), key_prefix: None },
    McpAlias { name: "supabase", package: "supabase-mcp", category: "databases", pricing: Freemium, env_var: Some("SUPABASE_URL"), key_prefix: None },
    McpAlias { name: "neon", package: "@neondatabase/mcp-server-neon", category: "databases", pricing: Freemium, env_var: Some("NEON_API_KEY"), key_prefix: None },
    // ── Search & Web (8) ────────────────────────────────────────
    McpAlias { name: "perplexity", package: "perplexity-mcp", category: "search", pricing: Freemium, env_var: Some("PERPLEXITY_API_KEY"), key_prefix: Some("pplx-") },
    McpAlias { name: "firecrawl", package: "firecrawl-mcp", category: "search", pricing: Freemium, env_var: Some("FIRECRAWL_API_KEY"), key_prefix: Some("fc-") },
    McpAlias { name: "brave", package: "@modelcontextprotocol/server-brave-search", category: "search", pricing: Free, env_var: Some("BRAVE_API_KEY"), key_prefix: None },
    McpAlias { name: "exa", package: "exa-mcp-server", category: "search", pricing: Freemium, env_var: Some("EXA_API_KEY"), key_prefix: None },
    McpAlias { name: "tavily", package: "tavily-mcp", category: "search", pricing: Freemium, env_var: Some("TAVILY_API_KEY"), key_prefix: Some("tvly-") },
    McpAlias { name: "serper", package: "mcp-server-serper", category: "search", pricing: Paid, env_var: Some("SERPER_API_KEY"), key_prefix: None },
    McpAlias { name: "searchapi", package: "searchapi-mcp", category: "search", pricing: Paid, env_var: Some("SEARCHAPI_API_KEY"), key_prefix: None },
    McpAlias { name: "bing", package: "bing-mcp", category: "search", pricing: Freemium, env_var: Some("BING_API_KEY"), key_prefix: None },
    // ── Developer Tools (8) ─────────────────────────────────────
    McpAlias { name: "linear", package: "mcp-server-linear", category: "developer", pricing: Paid, env_var: Some("LINEAR_API_KEY"), key_prefix: Some("lin_api_") },
    McpAlias { name: "sentry", package: "@sentry/mcp-server", category: "developer", pricing: Freemium, env_var: Some("SENTRY_AUTH_TOKEN"), key_prefix: None },
    McpAlias { name: "raygun", package: "raygun-mcp", category: "developer", pricing: Freemium, env_var: Some("RAYGUN_API_KEY"), key_prefix: None },
    McpAlias { name: "buildkite", package: "buildkite-mcp", category: "developer", pricing: Paid, env_var: Some("BUILDKITE_API_TOKEN"), key_prefix: None },
    McpAlias { name: "circleci", package: "@circleci/mcp-server-circleci", category: "developer", pricing: Freemium, env_var: Some("CIRCLECI_TOKEN"), key_prefix: None },
    McpAlias { name: "vercel", package: "vercel-mcp", category: "developer", pricing: Freemium, env_var: Some("VERCEL_TOKEN"), key_prefix: None },
    McpAlias { name: "cloudflare", package: "@cloudflare/mcp-server-cloudflare", category: "developer", pricing: Freemium, env_var: Some("CLOUDFLARE_API_TOKEN"), key_prefix: None },
    McpAlias { name: "aws", package: "aws-mcp", category: "developer", pricing: Paid, env_var: Some("AWS_ACCESS_KEY_ID"), key_prefix: None },
    // ── Productivity (8) ────────────────────────────────────────
    McpAlias { name: "slack", package: "@modelcontextprotocol/server-slack", category: "productivity", pricing: Freemium, env_var: Some("SLACK_BOT_TOKEN"), key_prefix: Some("xoxb-") },
    McpAlias { name: "google-drive", package: "@modelcontextprotocol/server-gdrive", category: "productivity", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "notion", package: "@notionhq/notion-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("NOTION_TOKEN"), key_prefix: Some("ntn_") },
    McpAlias { name: "airtable", package: "airtable-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("AIRTABLE_API_KEY"), key_prefix: Some("pat") },
    McpAlias { name: "todoist", package: "todoist-mcp", category: "productivity", pricing: Freemium, env_var: Some("TODOIST_API_TOKEN"), key_prefix: None },
    McpAlias { name: "asana", package: "asana-mcp", category: "productivity", pricing: Freemium, env_var: Some("ASANA_ACCESS_TOKEN"), key_prefix: None },
    McpAlias { name: "trello", package: "trello-mcp", category: "productivity", pricing: Freemium, env_var: Some("TRELLO_API_KEY"), key_prefix: None },
    McpAlias { name: "monday", package: "monday-mcp", category: "productivity", pricing: Freemium, env_var: Some("MONDAY_API_TOKEN"), key_prefix: None },
    // ── AI & Specialized (8) ────────────────────────────────────
    McpAlias { name: "langchain", package: "langchain-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "e2b", package: "@e2b/mcp-server", category: "ai", pricing: Freemium, env_var: Some("E2B_API_KEY"), key_prefix: None },
    McpAlias { name: "sequential-thinking", package: "@modelcontextprotocol/server-sequential-thinking", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "context7", package: "@upstash/context7-mcp", category: "ai", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "21st", package: "@21st-dev/magic", category: "ai", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "supadata", package: "@supadata/mcp", category: "ai", pricing: Freemium, env_var: Some("SUPADATA_API_KEY"), key_prefix: None },
    McpAlias { name: "dataforseo", package: "dataforseo-mcp-server", category: "ai", pricing: Paid, env_var: Some("DATAFORSEO_API_KEY"), key_prefix: None },
    McpAlias { name: "ahrefs", package: "@ahrefs/mcp", category: "ai", pricing: Paid, env_var: Some("AHREFS_API_KEY"), key_prefix: None },
    // ── AI Image Generation (4) ───────────────────────────────────
    McpAlias { name: "replicate", package: "replicate-mcp", category: "image", pricing: Paid, env_var: Some("REPLICATE_API_TOKEN"), key_prefix: Some("r8_") },
    McpAlias { name: "comfyui", package: "comfyui-mcp", category: "image", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "fal", package: "fal-mcp", category: "image", pricing: Paid, env_var: Some("FAL_KEY"), key_prefix: None },
    McpAlias { name: "stability", package: "stability-ai-mcp", category: "image", pricing: Paid, env_var: Some("STABILITY_API_KEY"), key_prefix: Some("sk-") },
    // ── Audio & Voice (2) ───────────────────────────────────────
    McpAlias { name: "elevenlabs", package: "elevenlabs-mcp", category: "audio", pricing: Freemium, env_var: Some("ELEVENLABS_API_KEY"), key_prefix: None },
    McpAlias { name: "deepgram", package: "deepgram-mcp", category: "audio", pricing: Freemium, env_var: Some("DEEPGRAM_API_KEY"), key_prefix: None },
    // ── Communication (6) ───────────────────────────────────────
    McpAlias { name: "discord", package: "discord-mcp", category: "communication", pricing: Free, env_var: Some("DISCORD_BOT_TOKEN"), key_prefix: None },
    McpAlias { name: "telegram", package: "telegram-mcp", category: "communication", pricing: Free, env_var: Some("TELEGRAM_BOT_TOKEN"), key_prefix: None },
    McpAlias { name: "resend", package: "resend-mcp", category: "communication", pricing: Freemium, env_var: Some("RESEND_API_KEY"), key_prefix: Some("re_") },
    McpAlias { name: "sendgrid", package: "sendgrid-mcp", category: "communication", pricing: Freemium, env_var: Some("SENDGRID_API_KEY"), key_prefix: Some("SG.") },
    McpAlias { name: "twilio", package: "twilio-mcp", category: "communication", pricing: Paid, env_var: Some("TWILIO_AUTH_TOKEN"), key_prefix: None },
    McpAlias { name: "intercom", package: "intercom-mcp", category: "communication", pricing: Paid, env_var: Some("INTERCOM_ACCESS_TOKEN"), key_prefix: None },
    // ── Vector DB (6) ───────────────────────────────────────────
    McpAlias { name: "pinecone", package: "@pinecone-database/mcp", category: "vectordb", pricing: Freemium, env_var: Some("PINECONE_API_KEY"), key_prefix: None },
    McpAlias { name: "weaviate", package: "weaviate-mcp", category: "vectordb", pricing: Free, env_var: Some("WEAVIATE_API_KEY"), key_prefix: None },
    McpAlias { name: "qdrant", package: "qdrant-mcp", category: "vectordb", pricing: Free, env_var: Some("QDRANT_API_KEY"), key_prefix: None },
    McpAlias { name: "chroma", package: "chroma-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "milvus", package: "milvus-mcp", category: "vectordb", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "turbopuffer", package: "@turbopuffer/turbopuffer-mcp", category: "vectordb", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Analytics & Monitoring (6) ──────────────────────────────
    McpAlias { name: "posthog", package: "posthog-mcp", category: "analytics", pricing: Freemium, env_var: Some("POSTHOG_API_KEY"), key_prefix: Some("phx_") },
    McpAlias { name: "mixpanel", package: "mixpanel-mcp-server", category: "analytics", pricing: Freemium, env_var: Some("MIXPANEL_TOKEN"), key_prefix: None },
    McpAlias { name: "datadog", package: "@winor30/mcp-server-datadog", category: "analytics", pricing: Paid, env_var: Some("DATADOG_API_KEY"), key_prefix: None },
    McpAlias { name: "grafana", package: "grafana-mcp", category: "analytics", pricing: Freemium, env_var: Some("GRAFANA_API_KEY"), key_prefix: Some("glsa_") },
    McpAlias { name: "prometheus", package: "prometheus-mcp", category: "analytics", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "plausible", package: "plausible-mcp", category: "analytics", pricing: Freemium, env_var: None, key_prefix: None },
    // ── E-commerce & Finance (6) ────────────────────────────────
    McpAlias { name: "stripe", package: "@stripe/mcp", category: "ecommerce", pricing: Paid, env_var: Some("STRIPE_SECRET_KEY"), key_prefix: Some("sk_") },
    McpAlias { name: "shopify", package: "shopify-mcp", category: "ecommerce", pricing: Paid, env_var: Some("SHOPIFY_ACCESS_TOKEN"), key_prefix: Some("shpat_") },
    McpAlias { name: "paypal", package: "@paypal/mcp", category: "ecommerce", pricing: Paid, env_var: Some("PAYPAL_CLIENT_ID"), key_prefix: None },
    McpAlias { name: "polygon", package: "polygon-mcp", category: "ecommerce", pricing: Freemium, env_var: Some("POLYGON_API_KEY"), key_prefix: None },
    McpAlias { name: "coinbase", package: "coinbase-mcp", category: "ecommerce", pricing: Freemium, env_var: Some("COINBASE_API_KEY"), key_prefix: None },
    McpAlias { name: "alpaca", package: "alpaca-mcp", category: "ecommerce", pricing: Freemium, env_var: Some("ALPACA_API_KEY"), key_prefix: None },
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
    McpAlias { name: "fly", package: "fly-mcp", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "railway", package: "@railway/mcp-server", category: "devops", pricing: Freemium, env_var: None, key_prefix: None },
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
    // ── Marketing (4) ───────────────────────────────────────────
    McpAlias { name: "mailchimp", package: "mailchimp-mcp", category: "marketing", pricing: Freemium, env_var: Some("MAILCHIMP_API_KEY"), key_prefix: None },
    McpAlias { name: "google-ads", package: "google-ads-mcp", category: "marketing", pricing: Paid, env_var: Some("GOOGLE_ADS_DEVELOPER_TOKEN"), key_prefix: None },
    McpAlias { name: "meta-ads", package: "meta-ads-mcp", category: "marketing", pricing: Paid, env_var: Some("META_ACCESS_TOKEN"), key_prefix: None },
    McpAlias { name: "semrush", package: "semrush-mcp", category: "marketing", pricing: Paid, env_var: Some("SEMRUSH_API_KEY"), key_prefix: None },
    // ── Maps & Location (1) ─────────────────────────────────────
    McpAlias { name: "mapbox", package: "@mapbox/mcp-server", category: "maps", pricing: Freemium, env_var: Some("MAPBOX_ACCESS_TOKEN"), key_prefix: Some("pk.") },
    // ── Developer Tools (additions) ────────────────────────────
    McpAlias { name: "playwright", package: "@playwright/mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "browserbase", package: "@browserbasehq/mcp", category: "developer", pricing: Freemium, env_var: Some("BROWSERBASE_API_KEY"), key_prefix: None },
    McpAlias { name: "figma", package: "figma-developer-mcp", category: "developer", pricing: Freemium, env_var: Some("FIGMA_API_KEY"), key_prefix: None },
    McpAlias { name: "xcode", package: "xcodebuildmcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "eslint", package: "@eslint/mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "nx", package: "nx-mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "launchdarkly", package: "@launchdarkly/mcp-server", category: "developer", pricing: Freemium, env_var: Some("LD_ACCESS_TOKEN"), key_prefix: None },
    McpAlias { name: "postman", package: "@postman/postman-mcp-server", category: "developer", pricing: Freemium, env_var: None, key_prefix: None },
    McpAlias { name: "bitbucket", package: "@nexus2520/bitbucket-mcp-server", category: "developer", pricing: Freemium, env_var: Some("BITBUCKET_TOKEN"), key_prefix: None },
    // ── Productivity (additions) ───────────────────────────────
    McpAlias { name: "obsidian", package: "obsidian-mcp", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "jira", package: "jira-mcp", category: "productivity", pricing: Paid, env_var: Some("JIRA_API_KEY"), key_prefix: None },
    McpAlias { name: "zendesk", package: "zendesk-mcp", category: "productivity", pricing: Paid, env_var: Some("ZENDESK_API_TOKEN"), key_prefix: None },
    McpAlias { name: "clickup", package: "@taazkareem/clickup-mcp-server", category: "productivity", pricing: Freemium, env_var: Some("CLICKUP_API_KEY"), key_prefix: None },
    McpAlias { name: "n8n", package: "n8n-mcp", category: "productivity", pricing: Freemium, env_var: Some("N8N_API_KEY"), key_prefix: None },
    McpAlias { name: "google-calendar", package: "@cocal/google-calendar-mcp", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "excel", package: "@negokaz/excel-mcp-server", category: "productivity", pricing: Free, env_var: None, key_prefix: None },
    // ── Databases (additions) ──────────────────────────────────
    McpAlias { name: "upstash", package: "@upstash/mcp-server", category: "databases", pricing: Freemium, env_var: Some("UPSTASH_API_KEY"), key_prefix: None },
    McpAlias { name: "turso", package: "mcp-server-turso", category: "databases", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Analytics (additions) ──────────────────────────────────
    McpAlias { name: "axiom", package: "mcp-server-axiom", category: "analytics", pricing: Freemium, env_var: Some("AXIOM_TOKEN"), key_prefix: None },
    // ── E-commerce (additions) ─────────────────────────────────
    McpAlias { name: "salesforce", package: "@salesforce/mcp", category: "ecommerce", pricing: Freemium, env_var: None, key_prefix: None },
    // ── Gap-fill (verified 2026-04-13 — high-download packages) ─
    McpAlias { name: "chrome-devtools", package: "chrome-devtools-mcp", category: "developer", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "pdf", package: "@modelcontextprotocol/server-pdf", category: "anthropic", pricing: Free, env_var: None, key_prefix: None },
    McpAlias { name: "dynatrace", package: "@dynatrace-oss/dynatrace-mcp-server", category: "analytics", pricing: Paid, env_var: Some("DYNATRACE_API_TOKEN"), key_prefix: None },
    McpAlias { name: "apify", package: "@apify/actors-mcp-server", category: "search", pricing: Freemium, env_var: Some("APIFY_TOKEN"), key_prefix: None },
    McpAlias { name: "heroku", package: "@heroku/mcp-server", category: "devops", pricing: Freemium, env_var: Some("HEROKU_API_KEY"), key_prefix: None },
    McpAlias { name: "browserstack", package: "@browserstack/mcp-server", category: "developer", pricing: Paid, env_var: Some("BROWSERSTACK_ACCESS_KEY"), key_prefix: None },
];

// ═══════════════════════════════════════════════════════════════════════════
// phf + unicase index map (case-insensitive lookup → array index)
// ═══════════════════════════════════════════════════════════════════════════

/// Case-insensitive name → index into `ALL_MCP_ALIASES`.
static MCP_INDEX: phf::Map<UniCase<&'static str>, usize> = phf_map! {
    // anthropic (0-7)
    UniCase::ascii("filesystem") => 0, UniCase::ascii("memory") => 1,
    UniCase::ascii("puppeteer") => 2, UniCase::ascii("brave-search") => 3,
    UniCase::ascii("google-maps") => 4, UniCase::ascii("fetch") => 5,
    UniCase::ascii("github") => 6, UniCase::ascii("gitlab") => 7,
    // databases (8-15)
    UniCase::ascii("neo4j") => 8, UniCase::ascii("postgres") => 9,
    UniCase::ascii("mysql") => 10, UniCase::ascii("sqlite") => 11,
    UniCase::ascii("mongodb") => 12, UniCase::ascii("redis") => 13,
    UniCase::ascii("supabase") => 14, UniCase::ascii("neon") => 15,
    // search (16-23)
    UniCase::ascii("perplexity") => 16, UniCase::ascii("firecrawl") => 17,
    UniCase::ascii("brave") => 18, UniCase::ascii("exa") => 19,
    UniCase::ascii("tavily") => 20, UniCase::ascii("serper") => 21,
    UniCase::ascii("searchapi") => 22, UniCase::ascii("bing") => 23,
    // developer (24-31)
    UniCase::ascii("linear") => 24, UniCase::ascii("sentry") => 25,
    UniCase::ascii("raygun") => 26, UniCase::ascii("buildkite") => 27,
    UniCase::ascii("circleci") => 28, UniCase::ascii("vercel") => 29,
    UniCase::ascii("cloudflare") => 30, UniCase::ascii("aws") => 31,
    // productivity (32-39)
    UniCase::ascii("slack") => 32, UniCase::ascii("google-drive") => 33,
    UniCase::ascii("notion") => 34, UniCase::ascii("airtable") => 35,
    UniCase::ascii("todoist") => 36, UniCase::ascii("asana") => 37,
    UniCase::ascii("trello") => 38, UniCase::ascii("monday") => 39,
    // ai (40-47)
    UniCase::ascii("langchain") => 40, UniCase::ascii("e2b") => 41,
    UniCase::ascii("sequential-thinking") => 42, UniCase::ascii("context7") => 43,
    UniCase::ascii("21st") => 44, UniCase::ascii("supadata") => 45,
    UniCase::ascii("dataforseo") => 46, UniCase::ascii("ahrefs") => 47,
    // image (48-51)
    UniCase::ascii("replicate") => 48, UniCase::ascii("comfyui") => 49,
    UniCase::ascii("fal") => 50, UniCase::ascii("stability") => 51,
    // audio (52-53)
    UniCase::ascii("elevenlabs") => 52, UniCase::ascii("deepgram") => 53,
    // communication (54-59)
    UniCase::ascii("discord") => 54, UniCase::ascii("telegram") => 55,
    UniCase::ascii("resend") => 56, UniCase::ascii("sendgrid") => 57,
    UniCase::ascii("twilio") => 58, UniCase::ascii("intercom") => 59,
    // vectordb (60-65)
    UniCase::ascii("pinecone") => 60, UniCase::ascii("weaviate") => 61,
    UniCase::ascii("qdrant") => 62, UniCase::ascii("chroma") => 63,
    UniCase::ascii("milvus") => 64, UniCase::ascii("turbopuffer") => 65,
    // analytics (66-71)
    UniCase::ascii("posthog") => 66, UniCase::ascii("mixpanel") => 67,
    UniCase::ascii("datadog") => 68, UniCase::ascii("grafana") => 69,
    UniCase::ascii("prometheus") => 70, UniCase::ascii("plausible") => 71,
    // ecommerce (72-77)
    UniCase::ascii("stripe") => 72, UniCase::ascii("shopify") => 73,
    UniCase::ascii("paypal") => 74, UniCase::ascii("polygon") => 75,
    UniCase::ascii("coinbase") => 76, UniCase::ascii("alpaca") => 77,
    // cms (78-83)
    UniCase::ascii("wordpress") => 78, UniCase::ascii("contentful") => 79,
    UniCase::ascii("sanity") => 80, UniCase::ascii("strapi") => 81,
    UniCase::ascii("ghost") => 82, UniCase::ascii("hubspot") => 83,
    // devops (84-89)
    UniCase::ascii("docker") => 84, UniCase::ascii("kubernetes") => 85,
    UniCase::ascii("terraform") => 86, UniCase::ascii("pulumi") => 87,
    UniCase::ascii("fly") => 88, UniCase::ascii("railway") => 89,
    // social (90-94)
    UniCase::ascii("twitter") => 90, UniCase::ascii("linkedin") => 91,
    UniCase::ascii("youtube") => 92, UniCase::ascii("tiktok") => 93,
    UniCase::ascii("reddit") => 94,
    // lifestyle (95-99)
    UniCase::ascii("spotify") => 95, UniCase::ascii("openweather") => 96,
    UniCase::ascii("news-api") => 97, UniCase::ascii("recipe") => 98,
    UniCase::ascii("wolfram") => 99,
    // marketing (100-103)
    UniCase::ascii("mailchimp") => 100, UniCase::ascii("google-ads") => 101,
    UniCase::ascii("meta-ads") => 102, UniCase::ascii("semrush") => 103,
    // maps (104)
    UniCase::ascii("mapbox") => 104,
    // developer additions (105-113)
    UniCase::ascii("playwright") => 105, UniCase::ascii("browserbase") => 106,
    UniCase::ascii("figma") => 107, UniCase::ascii("xcode") => 108,
    UniCase::ascii("eslint") => 109, UniCase::ascii("nx") => 110,
    UniCase::ascii("launchdarkly") => 111, UniCase::ascii("postman") => 112,
    UniCase::ascii("bitbucket") => 113,
    // productivity additions (114-120)
    UniCase::ascii("obsidian") => 114, UniCase::ascii("jira") => 115,
    UniCase::ascii("zendesk") => 116, UniCase::ascii("clickup") => 117,
    UniCase::ascii("n8n") => 118, UniCase::ascii("google-calendar") => 119,
    UniCase::ascii("excel") => 120,
    // databases additions (121-122)
    UniCase::ascii("upstash") => 121, UniCase::ascii("turso") => 122,
    // analytics addition (123)
    UniCase::ascii("axiom") => 123,
    // ecommerce addition (124)
    UniCase::ascii("salesforce") => 124,
    // gap-fill (125-130)
    UniCase::ascii("chrome-devtools") => 125, UniCase::ascii("pdf") => 126,
    UniCase::ascii("dynatrace") => 127, UniCase::ascii("apify") => 128,
    UniCase::ascii("heroku") => 129, UniCase::ascii("browserstack") => 130,
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
        assert_eq!(ALL_MCP_ALIASES.len(), 131);
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
