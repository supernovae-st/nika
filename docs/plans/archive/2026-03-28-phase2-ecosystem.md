# Implementation Plan: Phase 2 Ecosystem (v0.56-0.60)

### Context and Current State

The Nika project currently sits at v0.50.0 with 10 workspace crates and 8,457+ tests. The registry subsystem (`tools/nika-engine/src/registry/`) has 2,504 lines across 6 files (api.rs, lockfile.rs, mod.rs, operations.rs, resolver.rs, types.rs). The existing `RegistryClient` at `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs` points to `https://registry.supernovae.studio/api/v1` which is down. The CLI already has a full `PkgAction` enum in `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/pkg.rs` with List, Info, Add, Remove, Install, Update, Outdated, and Search subcommands. The MCP server at `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/server.rs` exposes 4 tools (nika_check, nika_list_workflows, nika_schema, nika_error_lookup). The release workflow at `/Users/thibaut/dev/supernovae/nika/.github/workflows/release.yml` already builds 7 targets (macOS arm64/x64, Linux arm64/x64, Windows x64, plus 2 musl/docker) and has Homebrew tap update logic targeting `supernovae-st/homebrew-tap`.

---

## 2.1 Registry & Publishing (v0.56, Week 11-12)

### Part 1: GitHub-Based Registry (Phase 1 of 3-phase rollout)

**Rationale**: The crates.io model analyzed in `/Users/thibaut/dev/supernovae/nika/docs/vision/archive/19-package-registry-design.md` (lines 129-178) shows that a Git-based index is the proven pattern. GitHub raw URLs provide free, fast, global CDN without any infrastructure.

**Repository structure**: Create `supernovae/nika-registry` on GitHub with:

```
nika-registry/
  index.json                          # Master index (array of PackageIndexEntry)
  packages/
    @supernovae/
      blog-generator/
        1.0.0/
          manifest.yaml               # Package manifest (matches types::Manifest)
          blog-generator-1.0.0.tar.gz # Tarball of workflow files
          README.md                   # Package documentation
      seo-audit/
        1.0.0/
          manifest.yaml
          seo-audit-1.0.0.tar.gz
          README.md
    @workflows/
      ...
```

**`index.json` format**: A flat JSON array for client-side filtering:

```json
[
  {
    "name": "@supernovae/blog-generator",
    "version": "1.0.0",
    "description": "Multi-step blog content pipeline",
    "keywords": ["content", "blog", "writing"],
    "type": "workflow",
    "checksum": "sha256:abc123...",
    "updated_at": "2026-04-01T00:00:00Z"
  }
]
```

This maps directly to the existing `SearchResult` struct in `api.rs` (line 181-203).

**CI validation**: A GitHub Action in the registry repo that:
1. Triggers on PR to `main`
2. Downloads the latest `nika` binary from GitHub Releases
3. Runs `nika check` on all `.nika.yaml` files in the changed package directory
4. Validates `manifest.yaml` schema against the `Manifest` type definition in `types.rs`
5. Verifies SHA-256 checksum of tarball matches declared checksum
6. Auto-labels PR with package type (workflow/agent/skill/template/bundle)

**Files to create/modify**:
- NEW: `tools/nika-engine/src/registry/github_backend.rs` -- GitHub raw URL adapter (~180 LOC)
- MODIFY: `tools/nika-engine/src/registry/api.rs` -- Add `RegistryBackend` trait, `GitHubBackend` impl
- MODIFY: `tools/nika-engine/src/registry/mod.rs` -- Re-export new backend

**Implementation details for `github_backend.rs`**:
The key insight from the current `RegistryClient` (api.rs lines 230-260) is that it already uses reqwest with a configurable `base_url` via `NIKA_REGISTRY_URL` env var. The adaptation path is:

1. Introduce a `RegistryBackend` trait with `search()`, `get_package()`, `get_version()`, `download()` methods
2. Implement `GitHubBackend` that constructs raw GitHub URLs: `https://raw.githubusercontent.com/supernovae/nika-registry/main/`
3. For `search()`: Fetch `index.json` once (cache with TTL), filter client-side by query + type
4. For `download()`: Fetch tarball from `packages/@scope/name/version/name-version.tar.gz`
5. Keep existing `RegistryClient` as `ApiBackend` for future Phase 2 API server
6. Select backend based on `NIKA_REGISTRY_URL` -- if it contains `github.com` or `raw.githubusercontent.com`, use `GitHubBackend`; otherwise use `ApiBackend`

**Graceful offline fallback**: When network is unavailable, the client should:
1. Return cached `index.json` if available (stored at `~/.nika/cache/registry-index.json`)
2. If no cache, return a clear error: "Registry unavailable. Install packages from local files with `nika pkg add --local <path>`"
3. Never panic or hang -- the 30-second timeout in `RegistryClient::new()` (api.rs line 58) already handles this

### Part 2: `nika pkg publish` Command

**Current gap**: The `PkgAction` enum in `pkg.rs` (line 14) has no `Publish` variant. This is the core new CLI subcommand.

**Add to `PkgAction` enum** (in `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/pkg.rs`):

```rust
/// Publish a package to the registry
Publish {
    /// Directory containing manifest.yaml + workflow files (default: current dir)
    #[arg(default_value = ".")]
    path: PathBuf,
    
    /// Skip validation (not recommended)
    #[arg(long)]
    no_check: bool,
    
    /// Dry run: show what would be published without creating PR
    #[arg(long)]
    dry_run: bool,
}
```

**Publish workflow** (new function `handle_publish()`):

1. **Validate manifest**: Load `manifest.yaml` from the specified directory, deserialize into `Manifest` (types.rs line 37), verify required fields (name, version, description)
2. **Validate workflows**: Run `nika check` on all `.nika.yaml` files found in the directory. Use the engine's validation pipeline directly (not subprocess) since the CLI crate depends on `nika-engine`
3. **Create tarball**: Use the `tar` and `flate2` crates (already available as transitive deps via cargo) to create `name-version.tar.gz` containing manifest.yaml + workflow files + README.md
4. **Calculate checksum**: SHA-256 of the tarball using the `sha2` crate
5. **Generate index entry**: Create the JSON entry for `index.json`
6. **Open PR to registry repo**: Two approaches:
   - **Option A (recommended)**: Shell out to `gh pr create` if `gh` CLI is available. This leverages existing GitHub authentication
   - **Option B (fallback)**: Use the GitHub REST API via reqwest with a personal access token stored in `~/.nika/config.toml` under `[registry]` section

**Files to create/modify**:
- NEW: `tools/nika-engine/src/registry/publish.rs` -- Tarball creation, checksum, index entry generation (~200 LOC)
- MODIFY: `tools/nika-cli/src/pkg.rs` -- Add `Publish` variant and `handle_publish()` handler (~120 LOC)
- MODIFY: `tools/nika-engine/src/registry/mod.rs` -- Export publish module

**Dependencies to add**: `sha2` (for SHA-256), `tar` + `flate2` (for tarball creation). Check if these already exist in the workspace Cargo.lock.

### Part 3: Registry Client Adaptation

**Current state**: `RegistryClient` in api.rs uses `DEFAULT_REGISTRY_URL = "https://registry.supernovae.studio/api/v1"` (line 52). This endpoint is down per the master plan (B3).

**Changes to `api.rs`**:

1. Change `DEFAULT_REGISTRY_URL` to `"https://raw.githubusercontent.com/supernovae/nika-registry/main"` as interim default
2. Add index caching to `~/.nika/cache/`:
   - `registry-index.json` with a `Last-Modified` or `ETag` header for conditional requests
   - Cache TTL: 5 minutes for interactive use, 0 for `--frozen` installs
3. Adapt the `search()` method: Download `index.json` from GitHub, deserialize into `Vec<PackageIndexEntry>`, filter by query + type, return as `SearchResponse`
4. Adapt the `download_and_extract()` method: Construct URL as `{base}/packages/{encoded_name}/{version}/{name}-{version}.tar.gz`, download, verify SHA-256 against index entry, extract to `~/.nika/packages/`

The existing `encode_package_name()` helper in api.rs handles `@scope/name` URL encoding.

**Files to modify**:
- MODIFY: `tools/nika-engine/src/registry/api.rs` -- New default URL, index caching, search adaptation (~100 LOC net change)
- NEW: `tools/nika-engine/src/registry/cache.rs` -- Index cache with TTL (~80 LOC)

### Part 4: Seed Content (20 packages)

**Source**: The 115 showcase workflows are spread across 7 files in `tools/nika-init/src/`:
- `course/showcase_builtin.rs` (15 workflows)
- `course/showcase_llm.rs` (20 workflows)
- `course/showcase_exec.rs` (20 workflows)
- `showcase_patterns.rs` (15 workflows)
- `showcase_advanced.rs` (15 workflows)
- `showcase_infra.rs` (15 workflows)
- `showcase_fetch.rs` (15 workflows)

**Selection criteria for 20 seed packages**:

| Category | Count | Examples |
|----------|-------|---------|
| Content | 5 | blog-generator, social-media-calendar, newsletter-builder, product-description, content-pipeline |
| SEO | 3 | seo-audit, keyword-research, competitor-analysis |
| Media | 3 | image-caption, audio-transcribe, media-pipeline |
| Research | 3 | parallel-research, hn-summarizer, trend-analyzer |
| System | 3 | api-monitor, standup-summary, bug-triager |
| Patterns | 3 | multi-provider, dag-diamond, structured-output |

**Each package contains**:
- `manifest.yaml` -- Matches `Manifest` struct from types.rs
- `workflow.nika.yaml` -- The actual workflow (extracted from showcase source)
- `README.md` -- Description, usage example, expected output

**Creation script**: A Rust binary or `nika showcase export-packages` command that:
1. Iterates over selected showcase workflows
2. Generates `manifest.yaml` with proper scope (`@supernovae/`), version `1.0.0`, description, keywords
3. Writes the workflow YAML (already stored as string constants in the showcase modules)
4. Creates README from the showcase description
5. Runs `nika check` on each
6. Creates tarballs
7. Generates `index.json`

**Target validation**: `nika pkg search "blog"` should return 3+ results. `nika pkg search --type workflow` should return all 20.

### Part 5: Security Scanning on Install

**Existing security infrastructure**: The codebase already has robust security:
- Command blocklist in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/security.rs` (lines 28-59) with NFKC unicode normalization
- SSRF protection in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/policy.rs` (lines 15-50) blocking private IP ranges and cloud metadata endpoints
- Path traversal protection in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/io/security.rs`

**New: Pre-install scanning** (`tools/nika-engine/src/registry/scanner.rs`):

Before extracting a downloaded package, scan the YAML workflow files for:

1. **Exec blocklist** (reuse `runtime/security.rs` BLOCKLIST): Scan all `exec:` command strings for dangerous patterns. The existing blocklist at security.rs:28 covers rm -rf, pipe-to-shell, eval, mkfifo, nc -e, sudo, etc.
2. **SSRF patterns** (reuse `runtime/policy.rs` is_ssrf_blocked): Scan all `fetch:` URLs for private IP ranges (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16)
3. **Env var exfiltration**: Detect patterns like `exec: echo $API_KEY | curl`, or fetch URLs containing `${{env.}}` references
4. **Base64 payloads**: Flag base64-encoded strings longer than 500 characters in any field (potential obfuscated payloads)
5. **Suspicious tool invocations**: Scan `invoke:` for known shell escape tools

**Trust levels** (displayed in `nika pkg search` and `nika pkg info` output):

| Level | Badge | Criteria |
|-------|-------|----------|
| `builtin` | `[VERIFIED]` green | Published by @supernovae scope, in seed content |
| `trusted` | `[TRUSTED]` blue | Community-vetted, 2+ maintainer reviews, passes all scans |
| `community` | `[COMMUNITY]` yellow | Passes automated scans, no manual review |
| `untrusted` | `[WARNING]` red | Fails scan, user must pass `--allow-untrusted` |

**Implementation**:
- NEW: `tools/nika-engine/src/registry/scanner.rs` (~250 LOC)
- MODIFY: `tools/nika-cli/src/pkg.rs` -- Integrate scan before extract in `PkgAction::Add` handler

---

## 2.2 Community & Content (v0.57, Week 13)

### Part 6: Workflow Metadata Standard (WORKFLOW.md)

**Context**: The agentskills.io ecosystem (researched in `/Users/thibaut/dev/supernovae/nika/docs/research/2026-03-23-vercel-skills-deep-dive.md`) uses YAML frontmatter in markdown files for multi-agent discovery.

**WORKFLOW.md format** (placed in package root):

```markdown
---
name: blog-generator
description: Multi-step blog content pipeline with SEO optimization
version: 1.0.0
license: MIT
keywords: [content, blog, writing, seo]
nika_schema: "nika/workflow@0.12"
verbs: [infer, fetch]
providers: [anthropic, groq]
agent_compatible: true
---

# Blog Generator

Generate SEO-optimized blog posts from a topic or outline.

## Usage

```bash
nika run blog-generator.nika.yaml
```
```

**Key design decisions**:
- Frontmatter fields are a superset of `manifest.yaml` fields from `types::Manifest`
- `agent_compatible: true` signals that this workflow can be invoked as an MCP tool or via `nika:run`
- `verbs` and `providers` enable filtering in search/discovery
- Compatible with Vercel skills.sh symlink pattern: `ln -s WORKFLOW.md .well-known/agent-skill.md`

**Symlink pattern for multi-agent discovery**:
When `nika pkg publish` creates a package, it also generates:
- `.well-known/agent-skill.md` -- Symlink to WORKFLOW.md
- `.well-known/ai-plugin.json` -- OpenAI plugin-compatible metadata

**Files to create/modify**:
- NEW: `tools/nika-engine/src/registry/workflow_md.rs` -- WORKFLOW.md parser/generator (~120 LOC)
- MODIFY: `tools/nika-engine/src/registry/publish.rs` -- Generate WORKFLOW.md during publish

### Part 7: `nika new --ai "description"`

**Current state**: `nika new` in `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/new_cmd.rs` (73 lines) supports flag-based generation (name, verb, provider) and templates via `nika_engine::new::NewWorkflowConfig`. There is no AI-powered generation.

**New flag** added to the CLI argument parser (in the main `nika` binary crate):

```rust
/// Generate workflow from natural language description using AI
#[arg(long)]
ai: Option<String>,
```

**Generation pipeline**:

1. **Construct system prompt**: Include the Nika schema reference (the same `SCHEMA_REF` from `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/server.rs` lines 271-289) plus examples from 3-5 showcase workflows
2. **Call the current provider**: Use the user's configured provider (from `nika setup` or env vars). The engine already has full inference capability via the `infer:` verb executor
3. **Parse the response**: Extract the YAML code block from the LLM response
4. **Validate**: Run the engine's validation pipeline (`nika check` equivalent) on the generated YAML
5. **If validation fails**: Include the error messages in a follow-up prompt asking the LLM to fix the issues. Retry up to 2 times
6. **Output**: Write the validated `.nika.yaml` file and display a success message

**Mock mode for testing**: When `NIKA_PROVIDER=mock` or no provider is configured:
- Return a pre-built template based on keyword matching in the description
- Map keywords like "blog" to the blog-generator template, "fetch" to the simple-fetch template
- This ensures `nika new --ai "create a blog generator"` works without an API key

**Optional Nika-Brain integration**: When the fine-tuned model is available (Phase 2.3):
- Check for `NIKA_BRAIN_ENDPOINT` env var
- If set, route `nika new --ai` to the Nika-Brain model endpoint instead of the general provider
- The fine-tuned model will produce higher-quality Nika-specific YAML

**Files to create/modify**:
- NEW: `tools/nika-engine/src/new/ai_generate.rs` -- AI generation pipeline (~200 LOC)
- MODIFY: `tools/nika-engine/src/new/mod.rs` -- Export ai_generate
- MODIFY: `tools/nika-cli/src/new_cmd.rs` -- Add `--ai` flag handling
- MODIFY: main binary CLI argument definitions -- Add `--ai` flag to `new` subcommand

### Part 8: Course Gamification

**Current state**: The course system has 12 levels and 226 exercises stored in `tools/nika-init/src/course/`. Progress tracking exists in `tools/nika-init/src/course/progress.rs`. The `nika course status` command exists in `tools/nika-cli/src/course.rs`.

**Additions**:

1. **Constellation progress map** (ASCII art in terminal):
   - Each completed level lights up a "star" in a constellation pattern
   - The course's 12 levels map to zodiac-themed constellations (aligned with Nika's space/navigation branding)
   - Already partially referenced in the master plan (S3)

2. **Badge system**:
   - Stored in `~/.nika/profile.yaml`
   - Badges: "First Workflow" (complete level 1), "DAG Master" (complete DAG exercises), "Agent Tamer" (complete agent exercises), "Speed Runner" (complete 5 exercises in one session), "Contributor" (publish a package)
   - Display in `nika course status` output

3. **Showcase contribution incentive**:
   - `nika showcase submit` command that packages a local workflow into a PR to the registry
   - Completing the submit workflow awards the "Contributor" badge
   - Displayed in `nika course status` as a progress item

**Files to create/modify**:
- NEW: `tools/nika-init/src/course/badges.rs` -- Badge definitions and award logic (~120 LOC)
- MODIFY: `tools/nika-init/src/course/progress.rs` -- Add badge tracking to progress state
- MODIFY: `tools/nika-cli/src/course.rs` -- Enhanced status display with constellation and badges (~80 LOC)

---

## 2.3 Integration & Distribution (v0.58-0.60, Week 14-16)

### Part 9: Telegram Webhook Trigger

**Current daemon architecture**: The daemon at `/Users/thibaut/dev/supernovae/nika/tools/nika-daemon/` uses Unix socket IPC with length-prefixed JSON protocol (protocol.rs). It has 5 services: secrets, jobs, cache, watch, and install.

**New service**: `tools/nika-daemon/src/services/telegram.rs`

**Architecture**:
1. The daemon starts an HTTP server (using `axum` or `warp`, already available as transitive deps) on a configurable port (default: 8443)
2. The HTTP server receives Telegram webhook POST requests
3. Each incoming message is parsed: extract chat_id, user_id, message text
4. Message text is matched against configured workflows:
   - Exact match: `/run blog-generator` triggers `blog-generator.nika.yaml`
   - NLP match (optional): If `nika new --ai` is available, parse intent from message
5. Matched workflow is executed via the engine (the daemon currently does not depend on nika-engine, so this requires either adding the dependency or shelling out to `nika run`)
6. Result is sent back to the user via the Telegram Bot API

**Configuration** (`.nika/telegram.yaml`):

```yaml
bot_token: "${TELEGRAM_BOT_TOKEN}"
webhook_port: 8443
allowed_users: [123456789, 987654321]    # Telegram user IDs
rate_limit: 10                            # Max requests per minute per user
max_message_size: 4096                    # Bytes
workflows:
  blog: blog-generator.nika.yaml
  seo: seo-audit.nika.yaml
  research: parallel-research.nika.yaml
```

**Security**:
- User whitelist (allowed_users) -- reject messages from unknown users
- Rate limiting per user (token bucket)
- Message size limit
- No shell injection: workflow names are matched against a fixed map, never interpolated into commands
- The webhook secret is verified using Telegram's `X-Telegram-Bot-Api-Secret-Token` header

**Files to create/modify**:
- NEW: `tools/nika-daemon/src/services/telegram.rs` -- Webhook handler, message parser, workflow matcher (~300 LOC)
- MODIFY: `tools/nika-daemon/src/services/mod.rs` -- Register telegram service
- MODIFY: `tools/nika-daemon/src/server.rs` -- Start HTTP listener for telegram
- MODIFY: `tools/nika-daemon/Cargo.toml` -- Add `axum` (for HTTP server), `reqwest` (for Telegram API calls)

**Design decision**: The daemon currently depends only on `nika-core` to stay lightweight (see daemon lib.rs line 16-17: "This crate depends on nika-core only... does NOT depend on nika-engine"). For the Telegram trigger, two options:
- **Option A**: Shell out to `nika run <workflow>` (preserves lightweight daemon, simpler)
- **Option B**: Add `nika-engine` dependency (in-process execution, faster, but increases daemon size)
- **Recommendation**: Option A. The daemon spawns `nika run` as a child process, captures stdout/stderr, and sends the result back via Telegram. This keeps the daemon lightweight and the execution boundary clean.

### Part 10: MCP Server Expansion

**Current state**: 4 tools in `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/server.rs` (414 lines):
1. `nika_check` -- Validate a workflow file
2. `nika_list_workflows` -- List .nika.yaml files in project
3. `nika_schema` -- Get schema reference
4. `nika_error_lookup` -- Look up error codes

**New tools to add** (target: 7 total):

5. **`nika_run`** -- Execute a workflow and return the result
   - Params: `path: String` (path to .nika.yaml), `dry_run: bool` (optional, default false), `timeout_secs: u64` (optional, default 120)
   - Implementation: Shell out to `nika run <path>` (same pattern as nika_check), capture stdout
   - Security: Same path validation as nika_check (validate_workflow_path), plus timeout enforcement (already in server.rs line 79-86)

6. **`nika_list_packages`** -- Search installed and registry packages
   - Params: `query: Option<String>`, `installed_only: bool` (default false)
   - Implementation: For installed packages, use `registry::list_installed()`. For registry search, use the GitHub backend
   - Returns: JSON list of {name, version, description, installed: bool}

7. **`nika_generate`** -- Natural language to YAML generation
   - Params: `description: String`, `provider: Option<String>`
   - Implementation: Calls the same pipeline as `nika new --ai` (Part 7)
   - Returns: The generated YAML string or validation errors
   - NOTE: This tool requires an LLM provider to be configured. If none is available, return a helpful error message

**Files to modify**:
- MODIFY: `tools/nika-mcp/src/server.rs` -- Add 3 new tools with params structs and handlers (~200 LOC)

### Part 11: Fine-Tuning Data Pipeline

**Research basis**: `/Users/thibaut/dev/supernovae/nika/docs/research/fine-tuning-for-workflow-orchestration.md` (lines 163-241) details the full pipeline. The mega-stack brainstorm at `/Users/thibaut/dev/supernovae/nika/docs/research/2026-03-27-mega-stack-brainstorm.md` (lines 145-178) specifies costs and teacher model selection.

**Pipeline as a Nika workflow** (meta-opportunity -- the pipeline itself is a `.nika.yaml`):

**Phase 1: Seed Collection** (~350 examples)
- 115 showcase workflows (from nika-init showcase modules)
- 226 course exercises (from nika-init course modules)
- Documented examples from vision docs

**Phase 2: Taxonomy Generation** (GLAN approach)
- Generate 500 taxonomy skeletons: 5 verbs x N domains x 5 complexity tiers
- Domains: content, seo, research, data, system, media, code, translation, monitoring, automation

**Phase 3: Synthetic Generation** (5,000-10,000 examples)
- Teacher models (legally safe for training -- per brainstorm line 172-174):
  - Llama 3.1 405B (Meta license permits)
  - Qwen 2.5 72B (Apache 2.0)
  - DeepSeek V3 (MIT)
- Do NOT use Claude/GPT outputs for training (ToS prohibit distillation)
- Input format: Natural language description
- Output format: Valid `.nika.yaml` workflow
- Cost estimate: ~$175 in API calls (per brainstorm line 159)

**Phase 4: Validation (nika check as automatic reward)**
- Run `nika check --strict` on each generated workflow
- 4-stage filter: schema validation, DAG validation, template validation, MCP connectivity
- Discard any example that fails
- Expected yield: ~65% pass rate = ~3,250-6,500 valid from 5,000-10,000 raw

**Phase 5: Evol-Instruct complexity escalation** (per fine-tuning doc lines 217-226)
- 4 epochs of evolution:
  1. Add constraints (retry, timeout, fail_fast)
  2. Increase depth (more DAG nodes, deeper dependencies)
  3. Add concreteness (specific URLs, real API patterns)
  4. Complicate (for_each, structured output, multi-provider)

**Phase 6: Preference pairs for DPO/SimPO**
- For each valid workflow, generate a deliberately incorrect variant (per fine-tuning doc lines 228-240)
- Common mistake patterns: missing $ prefix, wrong template syntax, wrong extension, wrong separator
- Pairs: (correct, incorrect) for preference optimization

**Phase 7: Output**
- Format: ShareGPT-style JSON conversations (per fine-tuning doc lines 198-215)
- Publish to HuggingFace dataset: `supernovae/nika-workflows`
- Training script: LLaMA-Factory config for QLoRA rank 64 on Qwen3-8B

**Files to create**:
- NEW: `tools/nika-engine/src/training/mod.rs` -- Training data export module (~100 LOC)
- NEW: `tools/nika-engine/src/training/export.rs` -- Convert showcases/exercises to ShareGPT format (~150 LOC)
- NEW: `tools/nika-engine/src/training/evolve.rs` -- Evol-Instruct complexity mutations (~200 LOC)
- NEW: `training/generate-data.nika.yaml` -- Meta-workflow for synthetic generation
- NEW: `training/README.md` -- Pipeline documentation

### Part 12: Homebrew Tap + GitHub Releases

**Current state**: The release workflow at `.github/workflows/release.yml` (lines 577-599) ALREADY handles:
- Building 7 target binaries (macOS arm64/x64, Linux arm64/x64/musl, Windows x64)
- Updating the Homebrew formula at `supernovae-st/homebrew-tap` via `mislav/bump-homebrew-formula-action`
- Publishing to npm (`@supernovae/nika`), crates.io, VS Code Marketplace, and Docker

**What needs verification/fixes for Phase 2**:

1. **HOMEBREW_TAP_TOKEN secret**: Verify it is set and valid. The release workflow checks this at line 585: `HAS_HOMEBREW_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN != '' }}`
2. **Tap repository**: Verify `supernovae-st/homebrew-tap` exists and has a formula file
3. **Formula correctness**: The formula should install to `bin/nika` with proper man page, completions
4. **Test**: `brew install supernovae-st/tap/nika && nika --version` on macOS arm64 and x64

**Additional distribution work**:
- Verify the existing release pipeline end-to-end with a dry run (`workflow_dispatch` with `dry_run: true`)
- Add shell completions to the release artifacts (bash, zsh, fish) -- generate via `clap_complete`
- Add man page generation via `clap_mangen`

**Files to modify**:
- POSSIBLY MODIFY: `.github/workflows/release.yml` -- Add completion/man page generation steps (~30 LOC)
- VERIFY: `supernovae-st/homebrew-tap` repository and formula

---

## Part 13: Tests (20-30 across all parts)

| Part | Test File | Tests | Type |
|------|-----------|-------|------|
| 1: GitHub Backend | `registry/github_backend.rs` | 4 | Unit: URL construction, index parsing, cache TTL, offline fallback |
| 2: Publish | `registry/publish.rs` | 4 | Unit: tarball creation, checksum calculation, manifest validation, index entry |
| 3: Client Adaptation | `registry/api.rs` | 3 | Unit: backend selection, search filtering, download URL construction |
| 4: Seed Content | Integration test | 2 | Integration: `nika pkg search "blog"` returns 3+, all 20 packages pass `nika check` |
| 5: Security Scanner | `registry/scanner.rs` | 5 | Unit: exec blocklist, SSRF detection, env exfil, base64 detection, trust levels |
| 6: WORKFLOW.md | `registry/workflow_md.rs` | 2 | Unit: frontmatter parsing, generation roundtrip |
| 7: AI Generate | `new/ai_generate.rs` | 3 | Unit: system prompt construction, mock mode, validation retry loop |
| 8: Badges | `course/badges.rs` | 2 | Unit: badge award conditions, persistence |
| 9: Telegram | `services/telegram.rs` | 3 | Unit: message parsing, user whitelist, rate limiting |
| 10: MCP Expansion | `mcp/server.rs` | 3 | Unit/async: nika_run params, nika_list_packages, nika_generate |
| 11: Fine-tuning | `training/export.rs` | 2 | Unit: ShareGPT format, Evol-Instruct mutations |
| **Total** | | **33** | |

---

## Part 14: Timeline

```
Week 11 (v0.56.0-alpha)
  Day 1-2: GitHub backend implementation (github_backend.rs, cache.rs)
  Day 3-4: Registry client adaptation (api.rs changes, backend trait)
  Day 5: Security scanner (scanner.rs)

Week 12 (v0.56.0)
  Day 1-2: nika pkg publish command (publish.rs, pkg.rs)
  Day 3-4: Seed 20 packages, create nika-registry repo, CI validation
  Day 5: Trust levels, tests, v0.56 release

Week 13 (v0.57.0)
  Day 1: WORKFLOW.md standard (workflow_md.rs)
  Day 2-3: nika new --ai (ai_generate.rs, new_cmd.rs)
  Day 4: Course gamification (badges.rs, constellation)
  Day 5: Tests, v0.57 release

Week 14 (v0.58.0)
  Day 1-3: Telegram webhook trigger (telegram.rs, daemon integration)
  Day 4-5: MCP server expansion (3 new tools)

Week 15 (v0.59.0)
  Day 1-3: Fine-tuning data pipeline (training modules, seed extraction)
  Day 4-5: Evol-Instruct mutations, preference pairs

Week 16 (v0.60.0)
  Day 1-2: Homebrew tap verification, distribution polish
  Day 3-4: End-to-end integration testing across all parts
  Day 5: v0.60 release, HuggingFace dataset publish
```

---

## Part 15: File Summary with LOC Estimates

### New Files

| # | File | LOC | Part |
|---|------|-----|------|
| 1 | `tools/nika-engine/src/registry/github_backend.rs` | ~180 | Registry GitHub backend |
| 2 | `tools/nika-engine/src/registry/cache.rs` | ~80 | Index cache with TTL |
| 3 | `tools/nika-engine/src/registry/publish.rs` | ~200 | Tarball creation, checksums |
| 4 | `tools/nika-engine/src/registry/scanner.rs` | ~250 | Pre-install security scanning |
| 5 | `tools/nika-engine/src/registry/workflow_md.rs` | ~120 | WORKFLOW.md parser/generator |
| 6 | `tools/nika-engine/src/new/ai_generate.rs` | ~200 | AI workflow generation |
| 7 | `tools/nika-init/src/course/badges.rs` | ~120 | Badge system |
| 8 | `tools/nika-daemon/src/services/telegram.rs` | ~300 | Telegram webhook handler |
| 9 | `tools/nika-engine/src/training/mod.rs` | ~100 | Training data module |
| 10 | `tools/nika-engine/src/training/export.rs` | ~150 | ShareGPT export |
| 11 | `tools/nika-engine/src/training/evolve.rs` | ~200 | Evol-Instruct mutations |
| 12 | `training/generate-data.nika.yaml` | ~80 | Meta-workflow for data gen |
| | **Subtotal new** | **~1,980** | |

### Modified Files

| # | File | Delta LOC | Part |
|---|------|-----------|------|
| 1 | `tools/nika-engine/src/registry/api.rs` | +100 | Backend trait, GitHub URLs |
| 2 | `tools/nika-engine/src/registry/mod.rs` | +20 | Re-exports |
| 3 | `tools/nika-cli/src/pkg.rs` | +120 | Publish variant + handler |
| 4 | `tools/nika-cli/src/new_cmd.rs` | +30 | --ai flag handling |
| 5 | `tools/nika-engine/src/new/mod.rs` | +10 | Export ai_generate |
| 6 | `tools/nika-cli/src/course.rs` | +80 | Constellation display, badges |
| 7 | `tools/nika-init/src/course/progress.rs` | +40 | Badge tracking |
| 8 | `tools/nika-mcp/src/server.rs` | +200 | 3 new MCP tools |
| 9 | `tools/nika-daemon/src/services/mod.rs` | +5 | Register telegram |
| 10 | `tools/nika-daemon/src/server.rs` | +30 | HTTP listener start |
| 11 | `tools/nika-daemon/Cargo.toml` | +5 | axum dependency |
| 12 | `.github/workflows/release.yml` | +30 | Completions, man pages |
| | **Subtotal modified** | **~670** | |

### External (Non-Rust)

| # | Item | Est. Work |
|---|------|-----------|
| 1 | `supernovae/nika-registry` repo | 20 package dirs + CI action |
| 2 | `training/README.md` | Pipeline documentation |
| 3 | HuggingFace dataset | `supernovae/nika-workflows` |

**Grand total**: ~2,650 LOC of Rust + 20 seed packages + 1 new GitHub repo + 1 HuggingFace dataset

---

### Critical Files for Implementation

- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs` -- Core registry client that must be adapted from the dead API endpoint to GitHub raw URLs; the RegistryBackend trait and GitHubBackend live here
- `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/pkg.rs` -- CLI entry point for all package commands; must add Publish variant and integrate security scanner before extract
- `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/server.rs` -- MCP server that expands from 4 to 7 tools; the nika_run and nika_generate tools are the primary AI-agent interface
- `/Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/services/mod.rs` -- Service registry for the daemon; the Telegram webhook service registers here alongside existing secrets/jobs/cache/watch
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/security.rs` -- Existing command blocklist and NFKC normalization that the new package scanner must reuse for consistency