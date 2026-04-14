# /dashboard — Open the Nika HQ Mission Control

Opens the local dashboard at `http://localhost:4321`. Starts it if not running.

## What the dashboard shows

- 5 live KPIs (crates, LOC, tests, clippy, hygiene)
- Commit feed with admission highlighting
- 7 shadow zones tracker
- GitNexus code graph evolution at `/graph`
- `Brief Claude` button — copies current state to clipboard as context
- shields.io badges at `/api/badge/*` for Mintlify / nika.sh

## How to run

```bash
# Check if dashboard is already running
curl -sf http://127.0.0.1:4321/api/state >/dev/null 2>&1 && echo "✅ Dashboard LIVE" || echo "❌ Not running"

# Start it (in a separate terminal or background)
cd ../hq/dashboard && pnpm dev

# Open in browser
open http://localhost:4321
```

## Full stack

```bash
# Start GitNexus server for /graph page (separate terminal)
gitnexus serve --port 4747

# Refresh dashboard data manually (auto on post-commit hook)
cd ../hq/dashboard && pnpm run collect

# Export gitnexus graph snapshot
cd ../hq/dashboard && pnpm run snapshot
```

## When to use

- **Session start**: instead of reading MEMORY.md — click "Brief Claude" → paste
- **Mid-session**: check live hygiene, recent commits, shadow zone progress
- **Post-admission**: verify status.mdx auto-regenerated with new numbers
- **Debug**: `/mcp` page (future) to test GitNexus / Linear / nika MCP tools

## Privacy

The dashboard lives in the PRIVATE `nika/hq/dashboard/` (supernovae-hq monorepo). Never mirrored into this public `engine/` submodule. API keys stay in `.env.local` (gitignored).

🦋
