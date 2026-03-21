# MCP Servers Catalog -- March 2026

Comprehensive catalog of Model Context Protocol servers, verified against npm registry and community sources.

**Sources**: npm registry (verified), awesome-mcp-servers (83K+ stars), mcpmarket.com, mcp.so, Smithery.ai, PulseMCP, official MCP Registry (registry.modelcontextprotocol.io).

**Legend**: (V) = Verified on npm | (P) = Python/uvx | (R) = Remote/SSE only | (G) = GitHub-only (clone to use)

---

## 1. Search & Web

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Brave Search** (V) | `@modelcontextprotocol/server-brave-search` v0.6.2 | `npx -y @modelcontextprotocol/server-brave-search` | `BRAVE_API_KEY` | Web and local search via Brave Search API |
| **Tavily** (V) | `tavily-mcp` v0.2.18 | `npx -y tavily-mcp` | `TAVILY_API_KEY` | Advanced web search with site inclusions/exclusions |
| **Exa** (V) | `exa-mcp-server` v3.1.9 | `npx -y exa-mcp-server` | `EXA_API_KEY` | Semantic web search and live crawling |
| **Perplexity** (V) | `perplexity-mcp` v0.2.3 | `npx -y perplexity-mcp` | `PERPLEXITY_API_KEY` | AI-powered search and reasoning |
| **Firecrawl** (V) | `firecrawl-mcp` v3.11.0 | `npx -y firecrawl-mcp` | `FIRECRAWL_API_KEY` | Web scraping, batch processing, structured extraction |
| **Fetch** (P) | `mcp-server-fetch` | `uvx mcp-server-fetch` | -- | Web page fetching and conversion to markdown |
| **Context7** (V) | `@upstash/context7-mcp` v2.1.4 | `npx -y @upstash/context7-mcp` | -- | Up-to-date library documentation for vibe-coding |

**Key tools**: `brave_web_search`, `brave_local_search`, `tavily_search`, `exa_search`, `firecrawl_scrape`, `firecrawl_map`, `firecrawl_extract`, `fetch`

---

## 2. Browser Automation

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Playwright** (V) | `@playwright/mcp` v0.0.68 | `npx -y @playwright/mcp@latest` | -- | Cross-browser automation (Chromium, Firefox, WebKit) |
| **Puppeteer** (V) | `@modelcontextprotocol/server-puppeteer` v2025.5.12 | `npx -y @modelcontextprotocol/server-puppeteer` | -- | Chrome/Chromium browser automation |
| **Browserbase** (V) | `@browserbasehq/mcp-server-browserbase` v2.4.3 | `npx -y @browserbasehq/mcp-server-browserbase` | `BROWSERBASE_API_KEY`, `BROWSERBASE_PROJECT_ID` | Cloud browser automation via Stagehand |
| **Chrome DevTools** (V) | `chrome-devtools-mcp` v0.20.3 | `npx -y chrome-devtools-mcp` | -- | Chrome DevTools Protocol integration |

**Key tools**: `browser_navigate`, `browser_screenshot`, `browser_click`, `browser_type`, `browser_evaluate`

---

## 3. AI / Image Generation

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Replicate** (V) | `replicate-mcp` v0.9.0 | `npx -y replicate-mcp` | `REPLICATE_API_TOKEN` | Official MCP for Replicate (Flux, SDXL, etc.) |
| **FAL.ai** (V) | `fal-mcp-server` v1.0.1 | `npx -y fal-mcp-server` | `FAL_KEY` | Image/video/audio generation (Flux, SDXL, TTS) |
| **ComfyUI** (V) | `comfyui-mcp` v0.3.2 | `npx -y comfyui-mcp` | `COMFYUI_URL` | Workflow execution, composition, registry |
| **EverArt** (V) | `@modelcontextprotocol/server-everart` v0.6.2 | `npx -y @modelcontextprotocol/server-everart` | `EVERART_API_KEY` | AI image generation via EverArt API |
| **Stability AI** (V) | `mcp-server-stability` v0.0.1 | `npx -y mcp-server-stability` | `STABILITY_API_KEY` | Image generation/editing via Stable Diffusion |
| **Midjourney** (V) | `midjourney-mcp` v1.0.0 | `npx -y midjourney-mcp` | `MJ_API_KEY` | Midjourney integration via MJ API proxy |

**Key tools**: `generate_image`, `edit_image`, `upscale_image`, `run_model`, `create_prediction`

---

## 4. Databases

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **PostgreSQL** (V) | `@modelcontextprotocol/server-postgres` v0.6.2 | `npx -y @modelcontextprotocol/server-postgres postgresql://...` | Connection string as arg | Query and interact with PostgreSQL databases |
| **SQLite** (V) | `mcp-server-sqlite` v0.0.2 | `npx -y mcp-server-sqlite /path/to/db.sqlite` | DB path as arg | SQLite database operations |
| **MySQL** (V) | `mcp-server-mysql` v1.0.42 | `npx -y mcp-server-mysql` | `MYSQL_HOST`, `MYSQL_USER`, `MYSQL_PASSWORD`, `MYSQL_DATABASE` | Read-only MySQL access with caching |
| **MongoDB** (V) | `mongodb-mcp` v1.0.4 | `npx -y mongodb-mcp` | `MONGODB_URI` | Read-only MongoDB querying |
| **Redis** (V) | `@modelcontextprotocol/server-redis` v2025.4.25 | `npx -y @modelcontextprotocol/server-redis redis://localhost:6379` | Redis URL as arg | Redis key-value store operations |
| **Supabase** (V) | `@supabase/mcp-server-supabase` v0.7.0 | `npx -y @supabase/mcp-server-supabase` | `SUPABASE_ACCESS_TOKEN` | Supabase DB, auth, storage, edge functions |
| **Neon** (V) | `mcp-server-neon` v0.0.1 | `npx -y mcp-server-neon` | `NEON_API_KEY` | Serverless Postgres via Neon |
| **Snowflake** (V) | `snowflake-mcp` v1.1.0 | `npx -y snowflake-mcp` | `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, `SNOWFLAKE_PASSWORD` | Snowflake warehouse queries |

**Key tools**: `query`, `list_tables`, `describe_table`, `insert`, `create_database`, `run_sql`

---

## 5. Developer Tools

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **GitHub** (V) | `@modelcontextprotocol/server-github` v2025.4.8 | `npx -y @modelcontextprotocol/server-github` | `GITHUB_PERSONAL_ACCESS_TOKEN` | Repos, PRs, issues, actions, search |
| **GitLab** (V) | `@modelcontextprotocol/server-gitlab` v2025.4.25 | `npx -y @modelcontextprotocol/server-gitlab` | `GITLAB_PERSONAL_ACCESS_TOKEN`, `GITLAB_API_URL` | MRs, pipelines, code review |
| **Linear** (V) | `mcp-server-linear` v1.6.0 | `npx -y mcp-server-linear` | `LINEAR_API_KEY` | Issues, projects, teams, cycles |
| **Jira** (V) | `jira-mcp` v1.0.1 | `npx -y jira-mcp` | `JIRA_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN` | Jira issues and project management |
| **Sentry** (P) | `mcp-server-sentry` | `uvx mcp-server-sentry` | `SENTRY_AUTH_TOKEN` | Error tracking and issue querying |
| **Git** (P) | `mcp-server-git` | `uvx mcp-server-git` | -- | Local git repository operations |
| **Apify** (V) | `@apify/actors-mcp-server` v0.9.12 | `npx -y @apify/actors-mcp-server` | `APIFY_TOKEN` | Run Apify actors for web scraping/automation |

**Key tools**: `create_issue`, `create_pull_request`, `search_repositories`, `get_file_contents`, `push_files`, `git_log`, `git_diff`

---

## 6. Cloud Platforms

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Cloudflare** (V) | `@cloudflare/mcp-server-cloudflare` v0.2.0 | `npx -y @cloudflare/mcp-server-cloudflare` | `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` | Workers, D1, KV, R2, DNS management |
| **AWS** (V) | `@modelcontextprotocol/server-aws-kb-retrieval` v0.6.2 | `npx -y @modelcontextprotocol/server-aws-kb-retrieval` | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION` | AWS Bedrock Knowledge Base retrieval |
| **GCP** (V) | `gcp-mcp` v1.0.2 | `npx -y gcp-mcp` | `GOOGLE_APPLICATION_CREDENTIALS` | Google Cloud Platform resource management |
| **Azure** (V) | `azure-mcp` v0.0.4 | `npx -y azure-mcp` | `AZURE_SUBSCRIPTION_ID`, `AZURE_TENANT_ID` | Azure resource management |
| **Vercel** (V) | `vercel-mcp` v0.0.7 | `npx -y vercel-mcp` | `VERCEL_TOKEN` | Deployments, projects, domains |

**Key tools**: `deploy`, `list_workers`, `create_kv_namespace`, `query_d1`, `manage_dns`, `list_deployments`

---

## 7. Communication

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Slack** (V) | `@modelcontextprotocol/server-slack` v2025.4.25 | `npx -y @modelcontextprotocol/server-slack` | `SLACK_BOT_TOKEN`, `SLACK_TEAM_ID` | Messages, channels, threads, reactions, search |
| **Discord** (V) | `mcp-server-discord` v1.2.8 | `npx -y mcp-server-discord` | `DISCORD_BOT_TOKEN` | Server/channel management, messaging |
| **Telegram** (V) | `mcp-server-telegram` v0.0.1 | `npx -y mcp-server-telegram` | `TELEGRAM_BOT_TOKEN` | Send/receive messages via Telegram Bot API |
| **Resend** (V) | `resend-mcp` v2.2.0 | `npx -y resend-mcp` | `RESEND_API_KEY` | Transactional email sending |
| **SendGrid** (V) | `sendgrid-mcp` v1.0.4 | `npx -y sendgrid-mcp` | `SENDGRID_API_KEY` | Email marketing, templates, automations, analytics |

**Key tools**: `send_message`, `list_channels`, `search_messages`, `send_email`, `create_template`

---

## 8. File Storage & Notes

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Filesystem** (V) | `@modelcontextprotocol/server-filesystem` v2026.1.14 | `npx -y @modelcontextprotocol/server-filesystem /path/to/dir` | Allowed dirs as args | Local file system read/write access |
| **Google Drive** (V) | `@modelcontextprotocol/server-gdrive` v2025.1.14 | `npx -y @modelcontextprotocol/server-gdrive` | OAuth credentials file | Search and read Google Drive files |
| **Notion** (V) | `@notionhq/notion-mcp-server` v2.2.1 | `npx -y @notionhq/notion-mcp-server` | `NOTION_TOKEN` | Official Notion API: pages, databases, blocks |
| **Obsidian** (V) | `obsidian-mcp` v1.0.6 | `npx -y obsidian-mcp` | `OBSIDIAN_VAULT_PATH` | AI assistant interaction with Obsidian vaults |

**Key tools**: `read_file`, `write_file`, `list_directory`, `search_files`, `create_page`, `query_database`, `search_notes`

---

## 9. Knowledge / RAG / Vector DBs

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Pinecone** (V) | `mcp-server-pinecone` v0.0.1 | `npx -y mcp-server-pinecone` | `PINECONE_API_KEY`, `PINECONE_ENVIRONMENT` | Vector similarity search |
| **Qdrant** (V) | `mcp-server-qdrant` v0.0.1 | `npx -y mcp-server-qdrant` | `QDRANT_URL`, `QDRANT_API_KEY` | Vector database operations |
| **Weaviate** (V) | `mcp-server-weaviate` v0.0.1 | `npx -y mcp-server-weaviate` | `WEAVIATE_URL`, `WEAVIATE_API_KEY` | Vector search and schema management |
| **Chroma** (V) | `mcp-server-chroma` v0.0.1 | `npx -y mcp-server-chroma` | `CHROMA_URL` | Embeddings-based vector store |
| **Memory** (V) | `@modelcontextprotocol/server-memory` v2026.1.26 | `npx -y @modelcontextprotocol/server-memory` | -- | Knowledge graph-based persistent memory for LLMs |

**Key tools**: `upsert_vector`, `query_vectors`, `delete_vectors`, `create_collection`, `store_memory`, `retrieve_memory`

---

## 10. Analytics & Observability

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Plausible** (V) | `plausible-mcp` v0.1.1 | `npx -y plausible-mcp` | `PLAUSIBLE_API_KEY`, `PLAUSIBLE_SITE_ID` | Privacy-friendly web analytics queries |
| **Axiom** (V) | `axiom-mcp` v2.35.0 | `npx -y axiom-mcp` | `AXIOM_TOKEN`, `AXIOM_DATASET` | Log search and analysis |
| PostHog | -- | -- | -- | No verified npm package (Python SDK only) |
| Mixpanel | -- | -- | -- | No verified npm package |

**Key tools**: `query_analytics`, `get_stats`, `search_logs`, `create_dashboard`

---

## 11. E-commerce & Payments

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Stripe** (V) | `@stripe/agent-toolkit` v0.9.0 | `npx -y @stripe/agent-toolkit` | `STRIPE_SECRET_KEY` | Official Stripe agent toolkit (payments, customers, invoices) |
| **Stripe** (V) | `mcp-server-stripe` v0.0.1 | `npx -y mcp-server-stripe` | `STRIPE_SECRET_KEY` | Community MCP wrapper for Stripe API |
| **Shopify** (V) | `mcp-server-shopify` v1.0.2 | `npx -y mcp-server-shopify` | `SHOPIFY_ACCESS_TOKEN`, `SHOPIFY_STORE_URL` | Products, orders, inventory management |
| **PayPal** (V) | `mcp-server-paypal` v0.0.1 | `npx -y mcp-server-paypal` | `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET` | PayPal payments integration |

**Key tools**: `create_payment`, `list_products`, `create_order`, `manage_customers`, `create_invoice`

---

## 12. Social Media

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Twitter/X** (V) | `mcp-server-twitter` v0.0.1 | `npx -y mcp-server-twitter` | `TWITTER_BEARER_TOKEN` | Twitter/X API integration |
| **LinkedIn** (V) | `linkedin-mcp` v1.0.1 | `npx -y linkedin-mcp` | `LINKEDIN_ACCESS_TOKEN` | LinkedIn profile and posting |
| **Instagram** (V) | `instagram-mcp` v1.1.7 | `npx -y instagram-mcp` | `RAPIDAPI_KEY` | Instagram API via RapidAPI |
| **YouTube** (V) | `youtube-mcp` v0.1.2 | `npx -y youtube-mcp` | `YOUTUBE_API_KEY` | YouTube video data and transcripts |

**Key tools**: `post_tweet`, `search_tweets`, `get_profile`, `post_update`, `get_video_info`, `get_transcript`

---

## 13. Maps & Location

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Google Maps** (V) | `@modelcontextprotocol/server-google-maps` v0.6.2 | `npx -y @modelcontextprotocol/server-google-maps` | `GOOGLE_MAPS_API_KEY` | Geocoding, places, directions, distance matrix |
| **Mapbox** (V) | `@mapbox/mcp-server` v0.9.0 | `npx -y @mapbox/mcp-server` | `MAPBOX_ACCESS_TOKEN` | Official Mapbox MCP: maps, geocoding, routing |

**Key tools**: `geocode`, `reverse_geocode`, `search_places`, `get_directions`, `distance_matrix`

---

## 14. Finance & Crypto

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Alpha Vantage** (V) | `alpha-vantage-mcp` v0.1.0 | `npx -y alpha-vantage-mcp` | `ALPHA_VANTAGE_API_KEY` | Stock market data, forex, crypto prices |
| **Polygon** (V) | `polygon-mcp` v1.0.0 | `npx -y polygon-mcp` | `POLYGON_API_KEY` | Onchain tools for Polygon blockchain |

**Key tools**: `get_stock_quote`, `get_time_series`, `get_crypto_price`, `get_forex_rate`

---

## 15. Productivity

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Google Calendar** (V) | `google-calendar-mcp` v1.0.9 | `npx -y google-calendar-mcp` | OAuth credentials | Calendar events CRUD |
| **Todoist** (V) | `todoist-mcp` v1.3.0 | `npx -y todoist-mcp` | `TODOIST_API_TOKEN` | Task management via Todoist API |

**Key tools**: `list_events`, `create_event`, `add_task`, `complete_task`, `list_projects`

---

## 16. Media Processing

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **FFmpeg** (V) | `ffmpeg-mcp` v0.0.3 | `npx -y ffmpeg-mcp` | -- (requires ffmpeg installed) | Video/audio processing via FFmpeg |
| **Sharp** (V) | `sharp-mcp` v0.2.6 | `npx -y sharp-mcp` | -- | Image session management and processing |

**Key tools**: `convert_video`, `extract_audio`, `resize_image`, `compress_image`, `get_metadata`

---

## 17. Code & Infrastructure

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Docker** (V) | `mcp-server-docker` v1.0.0 | `npx -y mcp-server-docker` | `DOCKER_HOST` (optional) | Execute commands in Docker containers |
| **Kubernetes** (V) | `mcp-server-kubernetes` v3.3.0 | `npx -y mcp-server-kubernetes` | `KUBECONFIG` | Cluster management via kubectl |
| Terraform | -- | -- | -- | No verified npm package (GitHub only: hashicorp/terraform-mcp-server) |

**Key tools**: `docker_exec`, `docker_build`, `list_pods`, `apply_manifest`, `get_deployment`, `scale_deployment`

---

## 18. CMS & Content

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **WordPress** (V) | `wordpress-mcp` v1.0.2 | `npx -y wordpress-mcp` | `WORDPRESS_URL`, `WORDPRESS_USERNAME`, `WORDPRESS_PASSWORD` | WordPress posts, pages, media management |
| **Sanity** (R) | Remote MCP at `https://mcp.sanity.io` | `npx mcp-remote https://mcp.sanity.io/sse` | `SANITY_TOKEN` | Sanity CMS content management (remote only) |

**Key tools**: `create_post`, `update_post`, `upload_media`, `get_content`, `publish`

---

## 19. Data Warehouses

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Snowflake** (V) | `snowflake-mcp` v1.1.0 | `npx -y snowflake-mcp` | `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, `SNOWFLAKE_PASSWORD` | Snowflake SQL queries |
| BigQuery | -- | -- | -- | Python only (GitHub: bq-mcp-server) |
| Databricks | -- | -- | -- | Python only (GitHub: databricks-mcp) |

---

## 20. Workflow Automation & Integration

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **n8n** (V) | `n8n-mcp` v2.40.0 | `npx -y n8n-mcp` | `N8N_API_KEY`, `N8N_URL` | n8n workflow automation integration |
| **Pipedream** (V) | `@pipedream/mcp` v0.0.1 | `npx -y @pipedream/mcp` | `PIPEDREAM_API_KEY` | Connect 2,500+ APIs with 8,000+ prebuilt tools |
| **Composio** (V) | `@composio/mcp` v1.0.9 | `npx -y @composio/mcp` | `COMPOSIO_API_KEY` | 250+ app integrations with managed OAuth |
| **MCP Remote** (V) | `mcp-remote` v0.1.38 | `npx -y mcp-remote <SSE_URL>` | -- | Proxy to connect local clients to remote MCP servers |

---

## 21. Core / Utility Servers

| Server | Package | Install | Env Vars | Description |
|--------|---------|---------|----------|-------------|
| **Sequential Thinking** (V) | `@modelcontextprotocol/server-sequential-thinking` v2025.12.18 | `npx -y @modelcontextprotocol/server-sequential-thinking` | -- | Step-by-step reasoning and problem solving |
| **Memory** (V) | `@modelcontextprotocol/server-memory` v2026.1.26 | `npx -y @modelcontextprotocol/server-memory` | -- | Knowledge graph-based persistent memory |
| **Everything** (V) | `@modelcontextprotocol/server-everything` v2026.1.26 | `npx -y @modelcontextprotocol/server-everything` | -- | Reference server exercising all MCP features |
| **Time** (P) | `mcp-server-time` | `uvx mcp-server-time` | -- | Current time and timezone conversions |
| **Fetch** (P) | `mcp-server-fetch` | `uvx mcp-server-fetch` | -- | HTTP requests and webpage-to-markdown |

---

## Python-Only Servers (uvx)

These servers are published on PyPI and run via `uvx` (Python):

| Server | Package | Install |
|--------|---------|---------|
| Fetch | `mcp-server-fetch` | `uvx mcp-server-fetch` |
| Git | `mcp-server-git` | `uvx mcp-server-git` |
| Sentry | `mcp-server-sentry` | `uvx mcp-server-sentry --auth-token=...` |
| Time | `mcp-server-time` | `uvx mcp-server-time` |
| SQLite (alt) | `mcp-server-sqlite` | `uvx mcp-server-sqlite --db-path /path/to/db` |

---

## Quick Reference: Top 20 Must-Have Servers for Nika

Priority servers for AI workflow automation (`invoke:` verb):

| Priority | Server | Why |
|----------|--------|-----|
| 1 | Firecrawl | Web scraping/extraction for RAG pipelines |
| 2 | Brave Search | Web search for agent research |
| 3 | GitHub | Repo management, PR automation |
| 4 | Playwright | Browser automation for testing/scraping |
| 5 | PostgreSQL / Supabase | Database operations |
| 6 | Slack | Team notifications and messaging |
| 7 | Memory | Persistent context across workflows |
| 8 | Sequential Thinking | Complex reasoning chains |
| 9 | Filesystem | Local file operations |
| 10 | Notion | Knowledge base management |
| 11 | Linear | Issue tracking integration |
| 12 | Replicate / FAL | Image/video generation |
| 13 | Context7 | Library docs lookup |
| 14 | n8n / Pipedream | Workflow orchestration |
| 15 | Redis | Caching and state management |
| 16 | Docker | Container management |
| 17 | Kubernetes | Cluster operations |
| 18 | Stripe | Payment automation |
| 19 | Cloudflare | Edge deployment |
| 20 | Resend | Email notifications |

---

## MCP Server Directories & Registries

| Directory | URL | Description |
|-----------|-----|-------------|
| **Official MCP Registry** | registry.modelcontextprotocol.io | Anthropic's official server registry |
| **awesome-mcp-servers** | github.com/punkpeye/awesome-mcp-servers (83K+ stars) | Largest curated list |
| **best-of-mcp-servers** | github.com/tolkonepiu/best-of-mcp-servers | 450 servers, 34 categories, ranked by quality |
| **mcp.so** | mcp.so | Community directory with search |
| **Smithery** | smithery.ai | Registry with CLI installer and hosted servers |
| **PulseMCP** | pulsemcp.com | Popularity tracking, 207+ servers |
| **mcpmarket.com** | mcpmarket.com/leaderboards | Top 100 by GitHub stars |
| **mcpservers.org** | mcpservers.org | Curated collection |
| **MCP Server Finder** | mcpserverfinder.com | Large categorized collection |

---

## Claude Desktop Config Example

```json
{
  "mcpServers": {
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-brave-search"],
      "env": { "BRAVE_API_KEY": "your-key" }
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "your-pat" }
    },
    "firecrawl": {
      "command": "npx",
      "args": ["-y", "firecrawl-mcp"],
      "env": { "FIRECRAWL_API_KEY": "your-key" }
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/you/projects"]
    },
    "memory": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"]
    },
    "sequential-thinking": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"]
    },
    "postgres": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-postgres", "postgresql://localhost/mydb"]
    },
    "supabase": {
      "command": "npx",
      "args": ["-y", "@supabase/mcp-server-supabase"],
      "env": { "SUPABASE_ACCESS_TOKEN": "your-token" }
    },
    "slack": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-slack"],
      "env": { "SLACK_BOT_TOKEN": "xoxb-your-token", "SLACK_TEAM_ID": "T0123456" }
    },
    "notion": {
      "command": "npx",
      "args": ["-y", "@notionhq/notion-mcp-server"],
      "env": { "NOTION_TOKEN": "your-token" }
    },
    "playwright": {
      "command": "npx",
      "args": ["-y", "@playwright/mcp@latest"]
    },
    "replicate": {
      "command": "npx",
      "args": ["-y", "replicate-mcp"],
      "env": { "REPLICATE_API_TOKEN": "your-token" }
    },
    "cloudflare": {
      "command": "npx",
      "args": ["-y", "@cloudflare/mcp-server-cloudflare"],
      "env": { "CLOUDFLARE_API_TOKEN": "your-token", "CLOUDFLARE_ACCOUNT_ID": "your-id" }
    },
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"]
    },
    "linear": {
      "command": "npx",
      "args": ["-y", "mcp-server-linear"],
      "env": { "LINEAR_API_KEY": "your-key" }
    },
    "google-maps": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-google-maps"],
      "env": { "GOOGLE_MAPS_API_KEY": "your-key" }
    },
    "redis": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-redis", "redis://localhost:6379"]
    },
    "stripe": {
      "command": "npx",
      "args": ["-y", "@stripe/agent-toolkit"],
      "env": { "STRIPE_SECRET_KEY": "sk_your_key" }
    },
    "docker": {
      "command": "npx",
      "args": ["-y", "mcp-server-docker"]
    },
    "kubernetes": {
      "command": "npx",
      "args": ["-y", "mcp-server-kubernetes"]
    }
  }
}
```

---

## Methodology

- **npm registry**: All packages marked (V) were verified via `npm view <package>` on 2026-03-21
- **Perplexity search**: 10 queries across best-of lists, official repos, category-specific searches
- **Community sources**: awesome-mcp-servers (83K stars), mcpmarket.com, mcp.so, Smithery, PulseMCP
- **Total servers cataloged**: 85+
- **Confidence**: HIGH for verified packages; MEDIUM for env vars (some inferred from conventions)

## Last Updated

2026-03-21
