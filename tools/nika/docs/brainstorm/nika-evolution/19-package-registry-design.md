# 19 -- Package Registry Design for Nika

> Comprehensive research on package registry design patterns for AI workflow engines.
> Comparison of 7 registries. Architecture recommendation. Manifest and CLI proposals.

**Nika** v0.30.3 -- **NovaNet** v0.20.0 -- Updated 2026-03-15

---

## Summary

Nika already has a solid foundation: `pkg:` URI protocol, `PkgUri` resolver, `RegistryClient` with HTTP API,
lockfile support, and DashMap-cached resolution. The gap is between this local-first skeleton and
a production-grade, npm-like registry that supports 5 package types (skill, workflow, satellite, model, mcp),
content-addressable integrity, and a community marketplace.

This document analyzes 7 existing registries, extracts common patterns, and proposes a concrete
architecture for evolving Nika's registry from its current state to a production system.

---

## 1. Existing Registry Comparison

### 1.1 Comparison Table

```
+------------------+-------------+-----------+----------+-----------+----------+---------+
| Dimension        | npm         | crates.io | HF Hub   | LangChain | CrewAI   | Dify    |
|                  |             |           |          | Hub       | Tools    | Market  |
+==================+=============+===========+==========+===========+==========+=========+
| Language         | JS/TS       | Rust      | Python   | Python    | Python   | Python  |
| Package format   | tarball     | .crate    | Git LFS  | YAML/Py   | Python   | YAML/Py |
| Versioning       | semver      | semver    | Git refs | semver    | semver   | semver  |
| Registry type    | Centralized | Central   | Central  | Central   | Central  | Central |
|                  |             | + alt     | + Git    |           |          |         |
| Dep resolution   | SAT solver  | SAT       | pip/uv   | pip       | pip      | None    |
| Auth model       | Token/OIDC  | GitHub    | Token    | API key   | API key  | OAuth   |
| Content types    | Code        | Code      | Models   | Prompts   | Tools    | Nodes   |
|                  |             |           | Datasets | Chains    | Agents   | Flows   |
|                  |             |           | Spaces   | Tools     | Tasks    | Tools   |
| Integrity        | SHA-512     | SHA-256   | SHA-256  | None      | None     | None    |
| Lockfile         | Yes         | Yes       | No       | No        | No       | No      |
| Namespace        | @scope/name | name      | user/repo| hub/owner | org/name | org/name|
| CLI command      | npm         | cargo     | hf       | langchain | crewai   | dify    |
| Mirror/alt reg   | verdaccio   | alexandrie| clone    | No        | No       | No      |
| Storage backend  | CouchDB+S3  | S3+PG    | Git+LFS  | S3        | S3       | S3      |
| Download format  | .tgz        | .crate    | Git clone| .tar.gz   | .whl     | .zip    |
| Yanking          | Yes         | Yes       | No       | No        | No       | No      |
| Size limit       | ~200MB      | 10MB      | 50GB(LFS)| ~10MB     | ~10MB    | ~50MB   |
+------------------+-------------+-----------+----------+-----------+----------+---------+
```

### 1.2 n8n Community Nodes

n8n takes a distinctive approach: community nodes are standard npm packages with a naming
convention (`n8n-nodes-<name>`) and a specific `package.json` structure. The n8n app discovers
them via npm search with a keyword filter (`n8n-community-node-package`).

Key design decisions:
- **npm as transport**: No custom registry; uses npm directly for hosting and versioning
- **Convention over configuration**: Package name prefix `n8n-nodes-` signals intent
- **`package.json` metadata**: `n8n.nodes` and `n8n.credentials` arrays declare capabilities
- **Node discovery**: n8n queries npm for keyword `n8n-community-node-package`
- **Installation**: `npm install` into the n8n data directory

Relevance to Nika: n8n proves that using an existing registry (npm/crates.io) as transport
is viable for workflow-engine packages. The trade-off is less control over metadata and
discovery UX.

### 1.3 Hugging Face Hub (Deep Dive)

HF Hub is the most relevant model for Nika because it handles heterogeneous content types
(models, datasets, spaces, configs) under a unified registry.

Architecture:
- **Git-based storage**: Every "repo" is a Git repository with LFS for large files
- **Model cards**: `README.md` with YAML frontmatter serves as the manifest
- **Collections**: Curated groups of repos across types
- **Programmatic API**: `huggingface_hub` Python library, REST API, and `hf` CLI
- **Spaces**: Full web apps hosted on the registry (Gradio, Streamlit)
- **Inference API**: Run models directly from the registry endpoint

Why it matters for Nika:
- Nika has 5 package types (skill, workflow, satellite, model, mcp)
- HF Hub handles 3+ content types seamlessly
- Git-based storage means zero custom infrastructure initially
- Model cards (YAML frontmatter) map directly to `manifest.yaml`

### 1.4 LangChain Hub

LangChain Hub (`hub.langchain.com`) is a centralized registry for prompts, chains, and tools.

Key characteristics:
- **Prompt-centric**: Primary content type is prompt templates (not code)
- **Versioning**: Commit-based (not semver), similar to Git
- **SDK integration**: `hub.pull("owner/prompt-name")` returns a runnable object
- **API key auth**: LangSmith API key required for publishing
- **Metadata**: Tags, use cases, language model compatibility
- **Community forks**: Users can fork and modify published prompts

LangChain Hub's main limitation is that it only handles serialized LangChain objects.
It cannot host arbitrary workflow definitions or YAML configs.

### 1.5 CrewAI Tools

CrewAI's approach is code-centric:
- **PyPI distribution**: Tools are Python packages installable via pip
- **`crewai-tools`**: Official toolkit with built-in tools
- **Community contributions**: PR-based to the main `crewai-tools` repository
- **No standalone registry**: No dedicated marketplace or search
- **Agent/Crew sharing**: Via GitHub repos or the CrewAI Enterprise platform

### 1.6 Dify Marketplace

Dify offers a visual marketplace for workflow components:
- **Plugin system**: Plugins extend Dify with new node types
- **Marketplace UI**: Browse and install from the Dify web interface
- **YAML-based plugins**: Plugin definitions are YAML with embedded Python
- **Categories**: Tools, Models, Extensions, Bundles
- **GitHub-backed**: Plugins are GitHub repos; the marketplace indexes them
- **Version pinning**: Each plugin version is a Git tag

### 1.7 Cargo (crates.io) Architecture Deep Dive

crates.io is the gold standard for a Rust-native registry and the most technically relevant
model since Nika is written in Rust.

Architecture:
```
+-----------------------------------------------------------------+
|  CRATES.IO ARCHITECTURE                                          |
+-----------------------------------------------------------------+
|                                                                  |
|  Client: cargo publish                                           |
|    |                                                             |
|    v                                                             |
|  API Server (Rust, Axum)                                         |
|    |-- POST /api/v1/crates/new (with .crate tarball)             |
|    |-- GET /api/v1/crates/{name} (metadata)                      |
|    |-- GET /api/v1/crates/{name}/{version}/download              |
|    |                                                             |
|    v                                                             |
|  Index (Git repository: github.com/rust-lang/crates.io-index)   |
|    |-- Each crate gets a JSON line file                          |
|    |-- Path: ab/cd/abcdef (first 4 chars)                        |
|    |-- Content: one JSON object per version                      |
|    |                                                             |
|    v                                                             |
|  Storage (S3-compatible)                                         |
|    |-- crates/{name}/{name}-{version}.crate (tarball)            |
|    |                                                             |
|    v                                                             |
|  Database (PostgreSQL)                                            |
|    |-- crates, versions, users, teams, keywords, categories      |
|                                                                  |
+-----------------------------------------------------------------+
```

Key design decisions from crates.io:

1. **Git-based index**: The entire package index is a Git repo. Clients clone/pull it for
   fast offline dependency resolution. This avoids hitting the API for every `cargo build`.

2. **`.crate` format**: A tarball containing `Cargo.toml` (manifest) + source files.
   The manifest is extracted and stored separately in the index.

3. **Semver enforcement**: All versions must be valid semver. No `latest` tag --
   the resolver picks the highest compatible version.

4. **Yank (soft delete)**: Published versions cannot be deleted, only "yanked" so they
   are not selected by new builds but existing lockfiles continue to work.

5. **Alternative registries**: The Cargo protocol supports alternative registries via
   `.cargo/config.toml`. Tools like Alexandrie and Kellnr implement the protocol.

6. **Sparse index**: Since Rust 1.68, cargo supports HTTP-based sparse indices
   that fetch only needed metadata, replacing the full Git clone.

---

## 2. Package Types for AI Workflow Ecosystems

### 2.1 Industry Survey

| Framework  | Package Types                                                    |
|------------|------------------------------------------------------------------|
| npm        | library, CLI tool, types, framework, utility                     |
| crates.io  | library, binary, proc-macro, build-script                        |
| HF Hub     | model, dataset, space, config                                    |
| LangChain  | prompt, chain, agent, tool, retriever, memory                    |
| CrewAI     | tool, agent, task, crew                                          |
| Dify       | tool, model-provider, extension, bundle                          |
| n8n        | node, credential, trigger                                        |

### 2.2 Nika Package Types (Validated in Doc 15)

Nika's 5 package types map directly to the ecosystem architecture:

```
+------------+-------------+-------------------+----------------------------+
| Type       | Contents    | File Extension    | Example                    |
+============+=============+===================+============================+
| skill      | .md system  | *.skill.md        | @supernovae/seo-expert     |
|            | prompts     |                   |                            |
+------------+-------------+-------------------+----------------------------+
| workflow   | .nika.yaml  | *.nika.yaml       | @supernovae/page-generator |
|            | DAG defs    |                   |                            |
+------------+-------------+-------------------+----------------------------+
| satellite  | satellite   | satellite.yaml    | @supernovae/vision-analyst |
|            | worker defs | or *.sat.yaml     |                            |
+------------+-------------+-------------------+----------------------------+
| model      | GGUF refs + | model.yaml        | @supernovae/llama3-3b      |
|            | configs     |                   |                            |
+------------+-------------+-------------------+----------------------------+
| mcp        | Server      | mcp.yaml          | @supernovae/novanet        |
|            | configs     |                   |                            |
+------------+-------------+-------------------+----------------------------+
```

### 2.3 Package Type Rationale

**skill** -- System prompts are the most common shareable artifact. They are:
- Small (typically < 50KB)
- Version-sensitive (prompt engineering evolves)
- Composable (workflows merge skills from multiple packages)
- Already supported via `pkg:` URI in Nika

**workflow** -- Complete DAG definitions that can be run directly or composed via `include:`.
These are the primary "application" unit.

**satellite** -- Specialized worker definitions (v0.33+ Shaka orchestration). Each satellite
specifies model, capabilities, tools. This is novel -- no existing registry supports this concept.

**model** -- References to GGUF models with configuration (quantization, context length,
capabilities). Not the model weights themselves (those live on HuggingFace or local disk),
but the metadata and runtime configuration.

**mcp** -- MCP server configurations. Pre-configured server definitions (command, args, env vars)
that can be shared across teams. Similar to n8n's credential concept.

### 2.4 Cross-Ecosystem Comparison

| Nika Type    | npm Equivalent  | HF Equivalent  | LangChain Equivalent |
|-------------|-----------------|----------------|----------------------|
| skill       | @types/         | Config/README  | Prompt template      |
| workflow    | Full package    | Space          | Chain                |
| satellite   | N/A (novel)     | N/A            | Agent definition     |
| model       | N/A             | Model card     | N/A                  |
| mcp         | N/A             | N/A            | Tool definition      |

---

## 3. Registry Architecture

### 3.1 Architecture Comparison

```
+------------------+---------------+--------------------+------------------+
| Approach         | Pros          | Cons               | Used By          |
+==================+===============+====================+==================+
| Centralized      | Simple UX     | Single point of    | npm, crates.io,  |
| (single server)  | Fast search   | failure, hosting   | PyPI, LangChain  |
|                  | Easy auth     | cost, trust model  | Hub              |
+------------------+---------------+--------------------+------------------+
| Federated        | No single     | Complex resolution | Cargo alt regs,  |
| (alt registries) | point of      | Priority rules     | Verdaccio,       |
|                  | failure       | Auth fragmentation | JFrog Artifactory|
+------------------+---------------+--------------------+------------------+
| Git-based        | Zero infra    | No search          | Go modules,      |
| (repos as pkgs)  | Free hosting  | No analytics       | Deno (early),    |
|                  | Familiar UX   | Auth per-host      | HF Hub           |
+------------------+---------------+--------------------+------------------+
| Hybrid           | Best of both  | More complex       | Go modules +     |
| (git + index)    | Progressive   | implementation     | pkg.go.dev,      |
|                  | adoption      |                    | Cargo sparse     |
+------------------+---------------+--------------------+------------------+
```

### 3.2 Recommended Architecture for Nika

**Phase 1 (Now -- v0.31): Git-Based with Static Index**

Use GitHub repositories as package sources with a static JSON index:

```
supernovae/nika-registry (GitHub repo)
|-- index.json                   # Package metadata index
|-- packages/
|   |-- @supernovae/
|   |   |-- seo-expert/
|   |   |   |-- 1.0.0.tar.gz    # Package tarball
|   |   |   |-- 1.0.0.json      # Version metadata
|   |   |   |-- 1.1.0.tar.gz
|   |   |   +-- 1.1.0.json
|   |   +-- page-generator/
|   |       +-- ...
|   +-- @community/
|       +-- ...
+-- schemas/
    +-- manifest.schema.json     # JSON Schema for validation
```

Why Git-based first:
- Zero infrastructure cost (GitHub handles hosting, CDN, auth)
- Nika already has `RegistryClient` pointing at `registry.supernovae.studio`
- GitHub releases as package sources (tarballs via release assets)
- Familiar workflow for contributors (PR to publish)
- Migration to custom server later is non-breaking (same API shape)

**Phase 2 (v0.32-0.33): Lightweight API Server**

Deploy a thin API server in front of the Git index:

```
Client (nika add @supernovae/seo-expert)
  |
  v
API Server (registry.supernovae.studio)
  |-- GET /api/v1/packages/:name          --> Read from index
  |-- GET /api/v1/packages/:name/:version --> Read from index
  |-- GET /api/v1/search?q=...            --> SQLite FTS search
  |-- POST /api/v1/packages               --> Write to index + S3
  |
  v
Storage
  |-- Index: SQLite (metadata) + Git repo (source of truth)
  |-- Packages: S3/R2 (Cloudflare R2 for cost efficiency)
```

Why this progression:
- Phase 1 validates the package format and CLI with zero infra cost
- Phase 2 adds search, analytics, and publishing UX when there is demand
- Cloudflare R2 has zero egress cost (important for package downloads)
- SQLite eliminates the need for a database server

**Phase 3 (Future): Federation + OCI**

Support alternative registries and OCI artifacts:

```toml
# .nika/config.toml
[registries]
default = "https://registry.supernovae.studio"
company = "https://nika-registry.company.internal"

[registries.company]
priority = 1  # Check company registry first
auth = "token"
```

OCI artifacts for model packages:
- Model configs as OCI manifests
- GGUF weight references as OCI blob references
- Compatible with existing container registries (GHCR, ECR, etc.)

### 3.3 Package Format

**Tarball contents:**

```
package-name-1.0.0.tar.gz
|-- manifest.yaml              # Package manifest (required)
|-- README.md                  # Documentation (optional)
|-- LICENSE                    # License file (optional)
|-- skills/                    # For skill packages
|   |-- main.skill.md
|   +-- helpers/
|       +-- seo.skill.md
|-- workflows/                 # For workflow packages
|   +-- main.nika.yaml
|-- satellites/                # For satellite packages
|   +-- vision-analyst.sat.yaml
|-- models/                    # For model packages
|   +-- model.yaml
+-- mcp/                       # For mcp packages
    +-- server.yaml
```

**Integrity:**

```yaml
# In the index, each version entry includes:
checksum: "sha256:a1b2c3d4e5f6..."
size: 4096
```

The client verifies the checksum after download, before extraction.
This is already partially implemented in Nika's `LockEntry.checksum` field.

### 3.4 Versioning Strategy

**SemVer for all package types.** This is consistent with:
- Nika's own versioning (strict semver, forever 0.x.x)
- Existing `PkgUri` parser (already validates semver)
- Existing `Lockfile` with version pinning
- Existing `resolver.rs` using the `semver` crate for comparison

Version constraint syntax (npm-compatible):

```yaml
dependencies:
  "@supernovae/seo-expert": "^1.0.0"    # >=1.0.0 <2.0.0
  "@supernovae/page-gen": "~2.1.0"      # >=2.1.0 <2.2.0
  "@supernovae/vision": ">=1.0.0"       # Any >=1.0.0
  "@community/translator": "1.2.3"      # Exact version
```

### 3.5 Dependency Resolution

Nika packages are primarily YAML/markdown, not compiled code. This dramatically simplifies
dependency resolution compared to npm or cargo:

- **No diamond dependency problem**: Skills and workflows are loaded at runtime, not linked
- **No binary compatibility concerns**: YAML is always forward-compatible
- **Flat resolution sufficient**: No need for SAT solver; latest compatible version wins
- **Schema version gating**: `schema: nika/workflow@0.10` ensures runtime compatibility

Algorithm:

```
1. Parse nika.yaml dependencies
2. For each dependency:
   a. Check lockfile for pinned version
   b. If not locked, resolve constraint against index
   c. Check if version already installed locally
   d. Download if needed, verify checksum
   e. Load manifest, check for transitive deps
3. Repeat for transitive dependencies (max depth: 5)
4. Write nika.lock with resolved versions
```

### 3.6 Authentication and Publishing

**Phase 1 (Git-based):**
- Publishing = PR to the registry repo
- Automated CI validates manifest.yaml schema
- Maintainers review and merge
- GitHub Actions builds the tarball and updates index.json

**Phase 2 (API server):**
- `nika login` -- authenticate via GitHub OAuth or API token
- `nika publish` -- pack and upload the tarball
- Automated validation:
  - manifest.yaml schema compliance
  - File size limits (10MB for skills/workflows, 1MB for satellites/mcp)
  - Name availability check
  - License field required
  - README.md recommended

**Token storage:**
- Stored in OS keychain via the existing `secrets` module
- Token name: `nika-registry-token`
- Falls back to `NIKA_REGISTRY_TOKEN` env var (CI/CD)

---

## 4. Package Manifest Format Proposal

### 4.1 Unified `manifest.yaml`

The manifest extends the existing `Manifest` struct in `src/registry/types.rs`:

```yaml
# ================================================================
# MANIFEST.YAML -- Nika Package Manifest
# Schema: nika/manifest@1.0
# ================================================================

# Required fields
name: "@supernovae/seo-expert"
version: "1.2.0"
type: skill                       # skill | workflow | satellite | model | mcp
schema: "nika/manifest@1.0"

# Metadata
description: "SEO optimization skill with keyword research and content scoring"
license: "MIT"
repository: "https://github.com/supernovae/nika-skills"
homepage: "https://supernovae.studio/packages/seo-expert"
authors:
  - "SuperNovae Studio <hello@supernovae.studio>"

# Discovery
keywords:
  - seo
  - content
  - optimization
  - marketing
categories:
  - content-generation
  - marketing

# Nika compatibility
nika:
  min_version: "0.27.0"           # Minimum Nika version required
  schema_version: "0.10"          # Minimum workflow schema version

# Package contents (varies by type)
# -- For skills:
skills:
  seo-expert:
    path: "skills/seo-expert.skill.md"
    description: "Full SEO optimization skill"
  keyword-research:
    path: "skills/keyword-research.skill.md"
    description: "Focused keyword research skill"

# -- For workflows:
workflows:
  page-generator:
    path: "workflows/page-gen.nika.yaml"
    description: "Generate SEO-optimized pages"
    inputs:
      entity_key: { type: string, required: true }
      locale: { type: string, default: "en-US" }

# -- For satellites:
satellites:
  vision-analyst:
    path: "satellites/vision-analyst.sat.yaml"
    accepts: ["image/png", "image/jpeg"]
    produces: ["application/json", "text/markdown"]
    capabilities: ["vision", "ocr"]

# -- For models:
models:
  llama3-3b:
    source: "huggingface://meta-llama/Llama-3.2-3B-Instruct-GGUF"
    quantization: "Q4_K_M"
    context_length: 131072
    capabilities: ["text-generation", "reasoning"]

# -- For mcp:
servers:
  novanet:
    command: "novanet-mcp"
    args: ["--stdio"]
    env:
      NEO4J_URI: "bolt://localhost:7687"
    required_secrets: ["neo4j"]

# Dependencies
dependencies:
  "@supernovae/core-skills": "^1.0.0"
  "@supernovae/brand-voice": "^2.0.0"

# Dev dependencies (not installed by consumers)
dev_dependencies:
  "@supernovae/test-workflows": "^1.0.0"

# Files to include in the tarball (default: all non-hidden files)
include:
  - "skills/**"
  - "README.md"
  - "LICENSE"

# Files to exclude from the tarball
exclude:
  - "tests/**"
  - ".env"
  - "*.test.yaml"
```

### 4.2 Manifest Schema Evolution

The current `Manifest` struct in `types.rs` needs these additions:

```
Current fields (keep):           New fields (add):
  name                             type: PackageType enum
  version                          schema: String
  description                      homepage: Option<String>
  authors                          keywords: Vec<String>
  license                          categories: Vec<String>
  repository                       nika: NikaCompat
  skills                           workflows: HashMap<String, WorkflowEntry>
  dependencies                     satellites: HashMap<String, SatelliteEntry>
                                   models: HashMap<String, ModelEntry>
                                   servers: HashMap<String, McpEntry>
                                   dev_dependencies: Option<HashMap<String, String>>
                                   include: Option<Vec<String>>
                                   exclude: Option<Vec<String>>
```

### 4.3 Type-Specific Entries

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageType {
    #[serde(rename = "skill")]
    Skill,
    #[serde(rename = "workflow")]
    Workflow,
    #[serde(rename = "satellite")]
    Satellite,
    #[serde(rename = "model")]
    Model,
    #[serde(rename = "mcp")]
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEntry {
    pub path: String,
    pub description: Option<String>,
    pub inputs: Option<HashMap<String, InputSpec>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteEntry {
    pub path: String,
    pub accepts: Vec<String>,
    pub produces: Vec<String>,
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub source: String,
    pub quantization: Option<String>,
    pub context_length: Option<u64>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEntry {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub required_secrets: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NikaCompat {
    pub min_version: Option<String>,
    pub schema_version: Option<String>,
}
```

---

## 5. CLI Commands Proposal

### 5.1 Current State

Nika already has these package-related commands in `src/cli/pkg.rs`:

```
nika pkg list          # List installed
nika pkg info <name>   # Show package info
nika pkg add <name>    # Install package
nika pkg remove <name> # Uninstall
nika pkg install       # Install from nika.yaml
nika pkg update        # Update packages
nika pkg outdated      # Check for updates
nika pkg search <q>    # Search registry
```

And from v0.30.3 unified CLI (doc 15):

```
nika add <name>        # Shorthand for nika pkg add
nika remove <name>     # Shorthand for nika pkg remove
nika list              # Shorthand for nika pkg list
nika update            # Shorthand for nika pkg update
```

### 5.2 Proposed CLI Evolution

**Package Consumer Commands (keep + extend):**

```bash
# ================================================================
# DISCOVERY
# ================================================================

nika search <query>                     # Search registry
nika search <query> --type satellite    # Filter by type
nika search <query> --category seo      # Filter by category
nika info @supernovae/seo-expert        # Package details + versions

# ================================================================
# INSTALLATION
# ================================================================

nika add @supernovae/seo-expert         # Add latest to nika.yaml
nika add @supernovae/seo-expert@1.2.0   # Add specific version
nika add @supernovae/seo-expert --dev   # Add as dev dependency
nika remove @supernovae/seo-expert      # Remove from nika.yaml
nika install                            # Install all from nika.yaml
nika install --frozen                   # Use exact versions from nika.lock
nika update                             # Update all to latest compatible
nika update @supernovae/seo-expert      # Update specific package
nika outdated                           # Show outdated packages
nika list                               # List installed packages
nika list --type workflow               # Filter by type

# ================================================================
# USAGE
# ================================================================

nika run @supernovae/page-generator     # Run an installed workflow directly
nika run @supernovae/page-generator -- --locale fr-FR  # With inputs
```

**Package Author Commands (new):**

```bash
# ================================================================
# AUTHORING
# ================================================================

nika init                               # Create nika.yaml in current dir
nika init --type skill                  # Create skill package scaffold
nika init --type workflow               # Create workflow package scaffold

# ================================================================
# PUBLISHING
# ================================================================

nika login                              # Authenticate with registry
nika login --token <token>              # Non-interactive login
nika whoami                             # Show authenticated user
nika logout                             # Remove stored credentials

nika pack                               # Create tarball (dry run)
nika pack --list                        # Show what would be included
nika publish                            # Pack + upload to registry
nika publish --dry-run                  # Validate without uploading
nika publish --tag beta                 # Publish with dist-tag

nika yank @supernovae/seo-expert@1.0.0  # Yank a published version
nika owner add <user>                   # Add co-owner to package
nika owner list                         # List package owners

# ================================================================
# VALIDATION
# ================================================================

nika check manifest.yaml                # Validate manifest schema
nika check --strict                     # + dependency resolution check
```

### 5.3 Command Implementation Priority

```
+------+-----------------------------+------------------+--------------+
| Wave | Command                     | Complexity       | Status       |
+======+=============================+==================+==============+
|  1   | nika search                 | Low (HTTP GET)   | Existing     |
|  1   | nika add / remove / install | Medium           | Existing     |
|  1   | nika list / info / outdated | Low              | Existing     |
|  1   | nika init (scaffold)        | Low              | New          |
+------+-----------------------------+------------------+--------------+
|  2   | nika pack                   | Medium (tar.gz)  | New          |
|  2   | nika publish                | Medium (upload)  | New          |
|  2   | nika login / logout         | Low (keychain)   | New          |
|  2   | nika check manifest.yaml    | Low (schema val) | New          |
+------+-----------------------------+------------------+--------------+
|  3   | nika yank                   | Low (API call)   | New          |
|  3   | nika owner add/list         | Low (API call)   | New          |
|  3   | nika run @pkg/workflow       | Medium           | New          |
+------+-----------------------------+------------------+--------------+
```

---

## 6. Rust Ecosystem Specifics

### 6.1 Can We Reuse the Cargo Registry Protocol?

The Cargo registry protocol (RFC 2789) defines:
- **Index format**: JSON lines in a Git repo or sparse HTTP index
- **Download endpoint**: `GET /api/v1/crates/{name}/{version}/download`
- **Publish endpoint**: `PUT /api/v1/crates/new`
- **Search endpoint**: `GET /api/v1/crates?q={query}`

**Verdict: Partially reusable.**

The index format and download mechanism could be adopted, but the publish and metadata
format are too Cargo-specific (Cargo.toml, dependency features, build targets).

Better approach: **Inspired by Cargo, customized for Nika.**

| Cargo concept      | Nika adaptation                                |
|--------------------|------------------------------------------------|
| `.crate` tarball   | `.nika.tar.gz` tarball                         |
| `Cargo.toml`       | `manifest.yaml`                                |
| `Cargo.lock`       | `nika.lock` (already implemented)              |
| Git index          | Git index (Phase 1) or sparse HTTP (Phase 2)   |
| `cargo publish`    | `nika publish`                                 |
| Features           | Package types (skill, workflow, satellite, ...) |
| Yank               | `nika yank` (same concept)                     |
| Alt registries     | Same `.nika/config.toml` mechanism              |

### 6.2 Alternative Rust Registries

**Alexandrie** (github.com/Hirevo/alexandrie):
- Cargo-compatible registry server
- Supports SQLite, MySQL, PostgreSQL backends
- Has git and sparse index modes
- Good reference for building a Rust registry server

**Kellnr** (github.com/kellnr/kellnr):
- Private crate registry
- Web UI with search
- Docker deployment
- Simpler than Alexandrie

**Neither is directly useful** for Nika since they implement the Cargo protocol specifically.
However, their code is useful as reference for:
- Tarball creation/extraction
- Index management
- Version resolution

### 6.3 OCI Artifacts

OCI (Open Container Initiative) artifacts are an emerging standard for distributing
non-container content through container registries.

Relevance:
- **Model packages**: GGUF model references could be OCI manifests
- **Existing infrastructure**: GHCR, ECR, GCR all support OCI artifacts
- **Tooling**: `oras` CLI for pushing/pulling OCI artifacts
- **Content addressing**: SHA-256 digests built into the protocol

**Recommendation**: Consider OCI for model packages only (Phase 3), not for
skills/workflows/satellites where tarball is simpler.

---

## 7. Security Considerations

### 7.1 Supply Chain Security

```
+----------------------------+------------------------------------------+
| Threat                     | Mitigation                               |
+============================+==========================================+
| Package tampering          | SHA-256 checksums in lockfile and index   |
|                            | Verify checksum after download, before    |
|                            | extraction                               |
+----------------------------+------------------------------------------+
| Typosquatting              | Scoped namespaces (@scope/name)          |
|                            | Reserved scope @supernovae for official   |
|                            | Name similarity check on publish         |
+----------------------------+------------------------------------------+
| Malicious skills           | Skills are .md files (no code execution)  |
|                            | Workflows validated against JSON Schema  |
|                            | exec: defaults to shell:false            |
+----------------------------+------------------------------------------+
| Credential leakage         | .env excluded from tarball by default     |
|                            | Secrets use ${spn:name} references       |
|                            | Pre-publish scan for common patterns     |
+----------------------------+------------------------------------------+
| Dependency confusion       | Scoped packages prevent confusion        |
|                            | Priority rules for alt registries        |
|                            | Lockfile pins exact versions             |
+----------------------------+------------------------------------------+
| Account takeover           | GitHub OAuth (delegated auth)            |
|                            | 2FA requirement for publishing           |
|                            | Package signing (Phase 3)                |
+----------------------------+------------------------------------------+
| Abandoned packages         | Yank mechanism (soft delete)             |
|                            | Activity indicators in search results    |
|                            | Ownership transfer process               |
+----------------------------+------------------------------------------+
```

### 7.2 Package Signing (Phase 3)

Long-term, packages should be signed:

```yaml
# In index.json per-version entry:
signature:
  algorithm: "ed25519"
  public_key: "supernovae-signing-key-2026"
  value: "base64-encoded-signature..."
```

- Sign with Ed25519 (fast, small signatures)
- Public keys distributed in a separate "trust root" file
- Sigstore integration as an alternative (keyless signing)

### 7.3 Content Validation

Before accepting a publish:

1. **Schema validation**: `manifest.yaml` conforms to `nika/manifest@1.0` schema
2. **File scanning**: No executables, no `.env`, no common secret patterns
3. **Size limits**: Skills <1MB, workflows <5MB, satellites <1MB, mcp <500KB
4. **Path safety**: No `..`, no absolute paths, no symlinks outside package
5. **License presence**: License field required (SPDX identifier)
6. **Name validation**: Same rules as existing `PkgUri.validate_identifier()`

---

## 8. Implementation Roadmap

### 8.1 Phase 1: Foundation (v0.31)

**Goal:** Validate package format with real packages, zero new infrastructure.

Tasks:
1. Extend `Manifest` struct with `type`, `keywords`, `categories`, `nika` fields
2. Create `nika/manifest@1.0` JSON Schema for validation
3. Implement `nika init --type <type>` scaffold command
4. Implement `nika pack` tarball creation (tar.gz with manifest.yaml)
5. Create 5-10 official `@supernovae/*` packages (existing skills/workflows)
6. Host tarballs as GitHub release assets on `supernovae/nika-registry`
7. Point existing `RegistryClient` at GitHub releases API
8. Add `nika check manifest.yaml` validation command
9. Update `nika add` to handle all 5 package types
10. Write tests: packing, unpacking, manifest validation, type-specific resolution

**Migration from current state:**
- Existing `manifest.yaml` format is forward-compatible (new fields are optional)
- Existing `PkgUri` parser unchanged (works for all types)
- Existing `RegistryClient` gets a GitHub releases adapter

### 8.2 Phase 2: API Server (v0.32-0.33)

**Goal:** Searchable registry with publishing support.

Tasks:
1. Deploy lightweight API server (Rust + Axum + SQLite)
2. Implement publish endpoint with validation
3. Add full-text search (SQLite FTS5)
4. Implement `nika login` / `nika publish` commands
5. Build simple web UI for browsing (optional)
6. Add download analytics
7. Implement yank functionality
8. Add webhook notifications (package published, yanked)

### 8.3 Phase 3: Ecosystem (v0.31+)

**Goal:** Community growth and enterprise features.

Tasks:
1. Alternative registry support (`.nika/config.toml`)
2. Package signing (Ed25519 or Sigstore)
3. OCI artifacts for model packages
4. Organization teams and permissions
5. Usage analytics and quality scores
6. Package deprecation and successor recommendations
7. Collections/curations (like HF Hub collections)

---

## 9. Comparison with Current Implementation

### 9.1 What Exists (Strengths)

| Component | File | Status |
|-----------|------|--------|
| `PkgUri` parser | `src/ast/pkg_resolver.rs` | Solid, well-tested (22 tests) |
| `Manifest` struct | `src/registry/types.rs` | Good base, needs extension |
| `RegistryClient` | `src/registry/api.rs` | Complete HTTP client |
| `Lockfile` | `src/registry/lockfile.rs` | Works, has checksum field |
| `Resolver` with cache | `src/registry/resolver.rs` | DashMap cache, semver sort |
| CLI commands | `src/cli/pkg.rs` | 8 subcommands implemented |
| Package types inference | `src/cli/pkg.rs` | Type from scope prefix |
| `SourceRegistry` | `src/source/registry.rs` | Multi-file tracking (LSP) |

### 9.2 What Needs Work (Gaps)

| Gap | Priority | Effort |
|-----|----------|--------|
| `Manifest` needs `type`, `keywords`, `categories` | High | Low |
| No `nika pack` (tarball creation) | High | Medium |
| No `nika publish` (upload) | Medium | Medium |
| No `nika init --type` (scaffold) | High | Low |
| No `nika login` (auth) | Medium | Low |
| Registry server does not exist yet | Medium | High |
| No manifest schema validation | High | Low |
| No pre-publish content scanning | Low | Medium |
| No satellite/model/mcp type support in resolver | High | Medium |
| `infer_package_type()` uses scope prefix heuristic | Low | Low |

### 9.3 Files That Need Changes

```
src/registry/types.rs          -- Extend Manifest struct
src/registry/api.rs            -- Add GitHub releases adapter
src/registry/resolver.rs       -- Support all 5 package types
src/registry/mod.rs            -- Re-export new types
src/cli/pkg.rs                 -- Add init, pack, publish commands
src/ast/pkg_resolver.rs        -- Validate package type in URI
schemas/manifest.schema.json   -- New file: JSON Schema
tests/contracts/pkg_contracts.rs -- Extend contract tests
```

---

## 10. Design Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Registry model | Git-based (Phase 1) then API server (Phase 2) | Zero infra cost initially, progressive complexity |
| Package format | `.tar.gz` with `manifest.yaml` | Simple, well-understood, works with any HTTP server |
| Versioning | Strict semver | Consistent with Nika and Cargo ecosystems |
| Namespace | `@scope/name` (npm-style) | Already implemented in `PkgUri`, familiar to developers |
| Dep resolution | Flat (latest compatible) | YAML packages have no binary compat concerns |
| Integrity | SHA-256 checksum in lockfile + index | Already partially implemented in `LockEntry` |
| Auth | GitHub OAuth then API tokens | Leverages existing identity, zero password management |
| Package types | 5 types (skill, workflow, satellite, model, mcp) | Validated in doc 15 ecosystem coherence |
| Index format | Static JSON (Phase 1) then SQLite FTS (Phase 2) | Simple to start, searchable when needed |
| Model storage | Reference only (HuggingFace/GGUF paths) | Models are too large for a package registry |

---

## Sources

| Source | Type | Contribution |
|--------|------|--------------|
| npm registry architecture | Training data | Scoped packages, lockfile, tarball format |
| crates.io source code | Training data | Git index, .crate format, yank, sparse index |
| Hugging Face Hub docs | Training data | Multi-type registry, Git LFS, model cards |
| LangChain Hub | Training data | Prompt registry patterns, versioning |
| CrewAI documentation | Training data | Tool/agent marketplace patterns |
| Dify marketplace | Training data | Plugin system, YAML-based extensions |
| n8n community nodes | Training data | npm-as-transport pattern |
| Cargo RFC 2789 | Training data | Registry protocol specification |
| Alexandrie/Kellnr | Training data | Alternative Rust registry patterns |
| OCI artifacts spec | Training data | Non-container artifact distribution |
| Nika `src/registry/` | Codebase | Current implementation (6 files, ~1200 LOC) |
| Nika `src/ast/pkg_resolver.rs` | Codebase | PkgUri parser (534 lines, 22 tests) |
| Nika `src/cli/pkg.rs` | Codebase | CLI commands (583 lines, 8 subcommands) |
| Nika doc 15 (ecosystem coherence) | Codebase | 5 package types, nika.yaml manifest |

## Methodology

- **Codebase analysis**: Read all 8 registry-related source files in Nika
- **Training data synthesis**: Compared 7 registry architectures from general knowledge
- **Existing brainstorm context**: Built on decisions validated in docs 12-17
- **Progressive complexity**: Designed phases that start simple and scale

## Confidence Level

**High** for:
- Package format design (well-established patterns)
- CLI command design (follows npm/cargo conventions)
- Phase 1 architecture (Git-based, proven approach)
- Manifest schema (extends existing Manifest struct)

**Medium** for:
- Phase 2 API server specifics (depends on scale requirements)
- OCI artifacts for models (emerging standard, may shift)
- Federation protocol (needs real multi-registry use cases)

**Low** for:
- Community adoption predictions (depends on marketing and content)
- Timeline estimates (depends on team bandwidth and priorities)

---

## Open Questions

1. **Should `nika add` replace `nika pkg add`?** Currently both exist. Recommend deprecating
   `nika pkg` subgroup in favor of top-level `nika add/remove/install/search`.

2. **Should satellites be packages or inline definitions?** Currently proposed as both
   (inline in workflow YAML + distributable as packages). Need to validate this with real
   use cases.

3. **Registry domain**: `registry.supernovae.studio` or `registry.nika.dev` or `nika.sh`?
   This affects branding and is a business decision.

4. **Should model packages include weight files?** Current proposal is reference-only
   (pointers to HuggingFace). But users may want self-contained packages for air-gapped
   environments. Consider OCI blobs for this use case.

5. **Multi-type packages**: Should a single package contain multiple types (e.g., a workflow
   that bundles its satellites and skills)? npm allows this; crates.io does not. The manifest
   schema above supports multi-type but the resolver needs to handle it.

---

<div align="center">

[<-- 18 MCP Multi-Modal Ecosystem](./18-mcp-multimodal-ecosystem-march2026.md) -- [Index](./00-README.md)

</div>
