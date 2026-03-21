# MCP Servers Landscape Research -- March 2026

> Research for Nika `invoke:` verb integration and SuperNovae ecosystem.
> 15 Perplexity searches, 12,220+ servers indexed on PulseMCP alone.

---

## TL;DR

The MCP ecosystem has exploded. **Lifestyle/fun servers are scarce** (mostly Spotify and weather).
**Marketing is emerging** (HubSpot official, Google Ads official, rest lagging).
**Dev tools are mature** (GitHub, Postgres, Puppeteer, Playwright all official).
**Communication is Discord-heavy** (50+ tools), Slack exists via official repo, WhatsApp/Teams/Zoom = gaps.

---

## Marketplaces and Directories

Where to discover MCP servers:

| Directory | URL | Servers Listed | Notes |
|-----------|-----|----------------|-------|
| **PulseMCP** | pulsemcp.com/servers | **12,220+** | Daily updated, largest directory |
| **Smithery.ai** | smithery.ai | **7,600+** | "App Store for AI agents", CLI install |
| **MCP Market** | mcpmarket.com | 100s (daily ranked) | Star-ranked, daily leaderboard |
| **MCP Registry** | github.com/modelcontextprotocol/registry | Official | Community-driven, like npm for MCP |
| **Composio** | composio.dev | **500+** managed | Production-ready, handles OAuth/auth |
| **Glama.ai** | glama.ai | Unknown | Hosted MCP gateway, freemium |
| **FastMCP** | fastmcp.me | Growing | Category-based browser |
| **awesome-mcp-servers** | github.com/punkpeye/awesome-mcp-servers | Curated | Best GitHub list, updated daily |
| **awesome-mcp-servers-2** | github.com/wong2/awesome-mcp-servers | Curated | Alternative curated list |

---

## LIFESTYLE and FUN

### Spotify (Music)

| Server | GitHub | Language | Free/Paid | Key Tools |
|--------|--------|----------|-----------|-----------|
| **igorgarbuz/spotify-mcp** | github.com/igorgarbuz/spotify-mcp | Node.js | FREE (needs Spotify Premium) | Playback control, playlist create/edit, song search, audio features, fusion playlists |
| **DarrelBumika/spotify-mcp-server** | github.com/DarrelBumika/spotify-mcp-server | Node.js | FREE (needs Spotify Premium) | Full account management, search, playlists, playback |
| **vsaez/mcp-spotify-player** | github.com/vsaez/mcp-spotify-player | Node.js | FREE (needs Spotify Premium) | Playback, search, playlists |
| **marcelmarais/spotify-mcp-server** | Listed on Augment Code | Node/TS | FREE (needs Spotify Premium) | Lightweight playback + playlists, Docker support |

**Setup**: All require Spotify Developer App (client ID/secret), `npm install`, OAuth token flow.
**Verdict**: Multiple good options. All need Spotify Premium ($10.99/mo). The servers themselves are free/open-source.

### Weather

| Server | GitHub | API Used | Free/Paid | Highlights |
|--------|--------|----------|-----------|------------|
| **shibing624/weather-forecast-server** | github.com/shibing624/weather-forecast-server | (built-in) | **FREE, no API key** | Best for zero-config |
| **jezweb/weather-mcp-server** | github.com/jezweb/weather-mcp-server | OpenWeatherMap | FREE (OWM free tier) | Forecasts, air quality, location search |
| **isdaniel/mcp_weather_server** | github.com/isdaniel/mcp_weather_server | Open-Meteo | FREE | Supports stdio/HTTP/SSE transport |
| **jpan8866/mcp-weather** | github.com/jpan8866/mcp-weather | NWS (US only) | FREE | US weather alerts by state |
| **caiyunapp/mcp-caiyun-weather** | github.com/caiyunapp/mcp-caiyun-weather | Caiyun | FREE | Hourly/weekly, AQI, life indices |

**Verdict**: Tons of options. `shibing624/weather-forecast-server` is the easiest (no API key).

### Airbnb / Travel

**STATUS: DOES NOT EXIST**
No MCP server found for Airbnb, Booking.com, or any travel platform. Opportunity gap.

### Uber / Lyft (Rides)

**STATUS: DOES NOT EXIST**
No MCP server found. No public APIs make this difficult anyway.

### DoorDash / UberEats (Food)

**STATUS: DOES NOT EXIST**
No MCP server found.

### Recipe / Cooking

**STATUS: DOES NOT EXIST**
No specific MCP server found. Could wrap Spoonacular API or similar.

### Fitness / Health

**STATUS: DOES NOT EXIST**
No MCP server found. Could wrap Fitbit/Garmin/Apple Health APIs.

### News

**STATUS: NOT FOUND SPECIFICALLY**
No dedicated news MCP server found, but Brave Search / Exa / Tavily MCP servers can fetch news.

---

## MARKETING and GROWTH

### HubSpot (CRM) -- OFFICIAL + Community

| Server | GitHub | Type | Free/Paid |
|--------|--------|------|-----------|
| **HubSpot Official MCP** | developers.hubspot.com/mcp | **OFFICIAL** | FREE (HubSpot account required) |
| **peakmojo/mcp-hubspot** | github.com/peakmojo/mcp-hubspot | Community | FREE |
| **shinzo-labs/hubspot-mcp** | github.com/shinzo-labs/hubspot-mcp | Community | FREE |
| **SanketSKasar/hubspot-mcp-server** | github.com/sanketskasar/hubspot-mcp-server | Community | FREE |
| **CDataSoftware/hubspot-mcp-server-by-cdata** | github.com/CDataSoftware/hubspot-mcp-server-by-cdata | Community | FREE (read-only) |

**Tools**: Contact/company/deal management, search, batch ops, activity history, engagements.
**Install (official)**: OAuth via Developer Platform app.
**Install (community)**: `npx -y @smithery/cli@latest install mcp-hubspot --client claude`

### Google Ads -- OFFICIAL

| Server | Source | Type | Free/Paid |
|--------|--------|------|-----------|
| **Google Ads MCP** | developers.google.com/google-ads/api/docs/developer-toolkit/mcp-server | **OFFICIAL** | FREE (Google Ads account required) |
| **FlowHunt hosted** | flowhunt.io/hosted-mcp-servers/google-ads/ | Hosted | Paid (FlowHunt pricing) |

**Tools**: `list_accessible_customers`, `search` (GAQL queries), campaign performance, budget updates, search terms, budget pacing.
**Verdict**: Official Google server is the clear winner. Free to use, just needs a Google Ads account.

### Amazon Ads

| Server | Type | Free/Paid |
|--------|------|-----------|
| **Amazon Ads MCP** | OFFICIAL (open beta) | FREE |

**Tools**: Campaign creation, ad groups, ads, Amazon Marketing Cloud queries.

### Mailchimp

**STATUS: NO DEDICATED MCP SERVER FOUND**
No official or well-known community MCP server. Major gap considering Mailchimp's market share.

### Buffer / Hootsuite (Social Scheduling)

**STATUS: DOES NOT EXIST**
No MCP servers found for any social media scheduling platform.

### Semrush / Ahrefs / Moz (SEO)

**STATUS: DOES NOT EXIST**
No MCP servers found for SEO tools. Huge opportunity.

### Canva (Design)

**STATUS: DOES NOT EXIST**
No MCP server found. Related: MeiGen-AI-Design-MCP exists for AI image generation (1,500+ prompts).

### Meta / Facebook Ads

**STATUS: NOT FOUND**
No specific MCP server confirmed.

---

## COMMUNICATION

### Slack

| Server | Source | Type | Free/Paid |
|--------|--------|------|-----------|
| **@modelcontextprotocol/server-slack** | Official MCP repo | **OFFICIAL** | FREE |

**Tools**: Channel listing, message posting, message reading, thread replies.
**Install**: `npx -y @modelcontextprotocol/server-slack` with `SLACK_BOT_TOKEN` and `SLACK_TEAM_ID` env vars.

### Discord

| Server | GitHub | npm Package | Tools Count |
|--------|--------|-------------|-------------|
| **@scarecr0w12/discord-mcp** | npm | `@scarecr0w12/discord-mcp` | **50+** tools |
| **sashathelambo-discord-mcp** | LobeHub listing | -- | **87** tools |
| **barryyip0625/mcp-discord** | mcpservers.org | npm install | Send, list servers, login |
| **hanweg/mcp-discord** | github.com/hanweg/mcp-discord | pip (Python) | Send/read, roles, channels |
| **SaseQ/discord-mcp** | github.com/SaseQ/discord-mcp | Java (JDA) | Full Discord API |

**Verdict**: Rich ecosystem. `@scarecr0w12/discord-mcp` via npm is the easiest install. All need a Discord bot token.

### Telegram

**STATUS: LIKELY EXISTS (not confirmed in search)**
Check Smithery.ai or PulseMCP directory. Community servers probably exist.

### WhatsApp

**STATUS: DOES NOT EXIST AS MCP**
Could be built on top of Twilio WhatsApp API (paid, per-message pricing from July 2025).

### Microsoft Teams

**STATUS: NOT CONFIRMED**
No specific MCP server found in searches.

### Zoom

**STATUS: NOT CONFIRMED**
No specific MCP server found in searches.

### Signal

**STATUS: DOES NOT EXIST**
Signal's encryption model makes this very difficult.

---

## MUST-HAVE DEV TOOLS

### Tier 1: Official / Battle-Tested

| Server | npm Package / Install | Type | Free/Paid | What It Does |
|--------|-----------------------|------|-----------|--------------|
| **GitHub** | `npx -y @modelcontextprotocol/server-github` | OFFICIAL | FREE | Repos, PRs, issues, code search, workflows |
| **PostgreSQL** | `npx -y @modelcontextprotocol/server-postgres` | OFFICIAL | FREE | Schema inspection, queries (works with Neon) |
| **Filesystem** | `npx -y @modelcontextprotocol/server-filesystem` | OFFICIAL | FREE | Secure file read/write/search |
| **Git** | `uvx mcp-server-git` (Python) | OFFICIAL | FREE | Repo operations, diff, log, blame |
| **Fetch** | `npx -y @modelcontextprotocol/server-fetch` | OFFICIAL | FREE | HTTP requests, web content fetching |
| **Puppeteer** | `npx -y @modelcontextprotocol/server-puppeteer` | OFFICIAL | FREE | Browser automation, screenshots |
| **Slack** | `npx -y @modelcontextprotocol/server-slack` | OFFICIAL | FREE | Channel messages, threads |
| **Playwright** | Microsoft official | OFFICIAL | FREE | Browser automation (accessibility-first) |

### Tier 2: Essential Third-Party (Free)

| Server | GitHub / Install | Free/Paid | What It Does |
|--------|------------------|-----------|--------------|
| **Firecrawl** | firecrawl MCP | FREE tier ($16/mo paid) | Web scraping, extraction, crawling |
| **Context7** | context7 MCP | FREE | Real-time library documentation |
| **Brave Search** | brave-search MCP | FREE (API key needed) | Web search for AI |
| **Exa** | exa-ai/exa-mcp | FREE tier (paid for volume) | Neural search, embeddings |
| **Tavily** | tavily MCP | FREE tier (1000 req/mo) | AI-optimized search |
| **Supabase** | supabase-community/supabase-mcp-server | FREE tier ($25/mo pro) | DB, auth, edge functions |

### Tier 3: Specialized Dev Tools

| Server | Type | Free/Paid | What It Does |
|--------|------|-----------|--------------|
| **Sentry** | getsentry/sentry-mcp-server | FREE (5K errors/mo) | Error monitoring, debugging |
| **Linear** | linear-app/linear-mcp-server | FREE (<10 users) | Issue tracking, sprints |
| **Notion** | notion-devs/notion-mcp-server | FREE (personal) | Docs, databases, workspace |
| **Stripe** | stripe/mcp-server-stripe | FREE SDK (tx fees apply) | Payments, billing, subscriptions |
| **Docker** | Community | FREE | Container management |
| **Kubernetes** | Community | FREE | Cluster management |
| **Cloudflare** | Omedia/mcp-servers (reference) | FREE | Workers/D1/R2/KV |

### Platforms WITHOUT MCP Servers (Gaps)

| Platform | Category | Notes |
|----------|----------|-------|
| **Vercel** | Deployment | No MCP server found |
| **Netlify** | Deployment | No MCP server found |
| **PlanetScale** | Database (MySQL) | No MCP server found |
| **Turso** | Database (SQLite edge) | No MCP server (but Turso has great SDKs) |
| **Neon** | Database (Postgres) | Use official Postgres MCP with Neon connection string |
| **Upstash** | Redis serverless | No MCP server found (Redis MCP exists generically) |
| **Val Town** | Serverless functions | No MCP server found |
| **Deno Deploy** | Runtime/deploy | No MCP server found |
| **Bun** | Runtime | No MCP server found |

---

## PRICING SUMMARY

| Tier | Cost | Examples |
|------|------|---------|
| **Completely Free** | $0 | GitHub MCP, Postgres MCP, Filesystem, Git, Puppeteer, Playwright, Discord, Weather |
| **Free + API Key** | $0 (free tier) | Brave Search, Tavily (1K req/mo), Exa, OpenWeatherMap |
| **Freemium** | $0-16/mo | Firecrawl ($16/mo), Supabase ($25/mo pro), Sentry ($26/mo) |
| **Account Required** | Varies | Spotify (Premium $10.99/mo), Google Ads (ad spend), HubSpot (CRM plan), Stripe (tx fees) |
| **Hosted/Managed** | $20+/mo | Composio, FlowHunt, Glama.ai |

---

## OPPORTUNITIES (Servers That Should Exist)

High-value MCP servers nobody has built yet:

1. **Mailchimp** -- Email marketing (huge user base, good API)
2. **Airbnb** -- Travel search/booking
3. **Semrush/Ahrefs** -- SEO analysis
4. **Canva** -- Design generation/editing
5. **Buffer/Hootsuite** -- Social scheduling
6. **WhatsApp** (via Twilio) -- Messaging
7. **Vercel** -- Deployment management
8. **Turso** -- SQLite edge database
9. **Upstash** -- Redis serverless
10. **Recipe API** (Spoonacular) -- Cooking/nutrition

---

## RECOMMENDED STACK FOR NIKA

For Nika's `invoke:` verb, prioritize integrating with:

### Must-Have (install now)
1. `@modelcontextprotocol/server-github` -- code workflow
2. `@modelcontextprotocol/server-filesystem` -- file ops
3. `@modelcontextprotocol/server-fetch` -- HTTP
4. Context7 MCP -- documentation
5. Brave Search or Tavily -- web search

### High Value (install when needed)
6. `@modelcontextprotocol/server-postgres` -- database
7. Firecrawl MCP -- scraping
8. `@modelcontextprotocol/server-slack` -- team comms
9. Playwright MCP -- browser automation
10. Supabase MCP -- backend

### Fun/Lifestyle (for demos and user delight)
11. Spotify MCP (igorgarbuz/spotify-mcp) -- music
12. Weather MCP (shibing624) -- zero-config weather
13. Discord MCP (@scarecr0w12/discord-mcp) -- community

---

## METHODOLOGY

- **Tools used**: Perplexity AI (sonar model), 15 searches
- **Sources analyzed**: 80+ URLs across GitHub, blogs, official docs, YouTube
- **Time period**: Data current as of March 21, 2026
- **Confidence**: HIGH for dev tools, MEDIUM for marketing, LOW for lifestyle gaps

## Sources

1. github.com/modelcontextprotocol/servers -- Official MCP server repo
2. github.com/punkpeye/awesome-mcp-servers -- Curated community list
3. pulsemcp.com/servers -- 12,220+ server directory
4. smithery.ai -- 7,600+ MCP marketplace
5. developers.hubspot.com/mcp -- Official HubSpot MCP
6. developers.google.com/google-ads/api/docs/developer-toolkit/mcp-server -- Official Google Ads MCP
7. mcpmarket.com -- Daily ranked MCP directory
8. github.com/modelcontextprotocol/registry -- Official MCP registry
9. firecrawl.dev/blog/best-mcp-servers-for-developers -- Firecrawl's dev picks
10. stackgen.com/blog/the-10-best-mcp-servers-for-platform-engineers-in-2026 -- Platform eng picks
