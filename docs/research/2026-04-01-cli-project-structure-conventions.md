# Research Report: CLI/Workflow Tool Project Structure Conventions

## Summary

Modern CLI tools universally follow a **config file + runtime directory** split pattern, where a versioned config file (committed to git) serves as the project root marker, and a runtime/cache directory (gitignored) holds generated state. The dominant convention is a single non-dot config file at the root (`Cargo.toml`, `turbo.json`, `Earthfile`) paired with a dot-prefixed runtime directory (`.terraform/`, `.turbo/`, `.prefect/`). YAML dominates workflow/orchestration tools, TOML dominates developer tooling (Rust, Python), and JSON holds in the JavaScript ecosystem.

## Key Findings

---

### 1. Dagger (dagger.io)

| Aspect | Detail |
|--------|--------|
| **Config file** | `dagger.json` (JSON) -- module metadata, SDK, dependencies |
| **Runtime dir** | `sdk/` (vendored SDK, generated) -- no `.dagger/` equivalent |
| **Init command** | `dagger init` -- creates `dagger.json` + language-specific files (`go.mod`, `pyproject.toml`, `package.json`) + `sdk/` dir + starter source |
| **Root detection** | Directory containing `dagger.json` |
| **Secrets** | First-class `dagger.Secret` type -- sourced from env (`env:MY_KEY`), file (`file:~/.secret`), command (`exec:~/script.sh`), external (Vault, 1Password). Scrubbed from logs automatically |
| **Clean command** | None documented |
| **Engine config** | `engine.json` (v0.15+) for logging, security, GC -- separate from project config |

**Notable**: Dagger is unique in using JSON (not YAML/TOML) for project config. The `sdk/` directory is vendored and generated, similar to Go's vendor pattern. No dot-prefixed runtime directory.

- Source: https://docs.dagger.io/reference/configuration/modules
- Source: https://docs.dagger.io/extending/modules

---

### 2. Temporal (temporal.io)

| Aspect | Detail |
|--------|--------|
| **Config file** | None -- all configuration is programmatic (SDK code) |
| **Runtime dir** | None -- state lives in the Temporal Server (Event History) |
| **Init command** | None -- no `temporal init` or project scaffold |
| **Root detection** | None -- relies on language build tools (`go.mod`, `package.json`) |
| **Secrets** | Deferred to external systems (env vars, vaults). Activities access secrets, not workflows (determinism constraint) |
| **Clean command** | None |

**Notable**: Temporal is the outlier -- it provides **zero project structure conventions**. Workers are normal programs in Go/TypeScript/Python/Java. The Temporal Server owns all state. Project structure is entirely determined by the language ecosystem, not Temporal itself.

---

### 3. Prefect (prefect.io)

| Aspect | Detail |
|--------|--------|
| **Config file** | `prefect.yaml` (YAML) or `prefect.toml` (TOML) -- deployments, flows, settings |
| **Runtime dir** | `.prefect/` (dot-prefixed) -- local storage, results, artifacts. Also `~/.prefect/` globally for profiles, database |
| **Init command** | `prefect init` -- creates `prefect.yaml` with deployment templates |
| **Root detection** | Presence of `prefect.yaml`; `prefect deploy` **must** run from root |
| **Secrets** | Prefect Blocks (Secret blocks) stored server-side. `SecretStr` for masked config values. CLI: `prefect block create` |
| **Clean command** | None built-in. `prefect storage delete` for artifacts. Manual `.prefect/` cleanup |

**Notable**: Prefect supports **both YAML and TOML** for config, letting users choose. The `.prefect/` pattern (project-local) + `~/.prefect/` (global) is a common two-tier approach. Secrets are server-managed (Prefect Cloud/Server blocks), not file-based.

- Source: https://docs.prefect.io/latest/concepts/deployments/

---

### 4. n8n (self-hosted)

| Aspect | Detail |
|--------|--------|
| **Config file** | None (env vars only). `.env` file for Docker/npm setups |
| **Runtime dir** | `~/.n8n/` (global, home dir) -- `database.sqlite`, encrypted credentials, execution history. Override with `N8N_USER_FOLDER` |
| **Init command** | None -- first `npx n8n` or `docker run` creates `~/.n8n/` and prompts for owner setup |
| **Root detection** | `N8N_USER_FOLDER` env var or defaults to `~/.n8n`. No project-local detection |
| **Secrets** | Credentials encrypted in DB using `N8N_ENCRYPTION_KEY` env var. External vault integration (AWS Secrets Manager, HashiCorp Vault) |
| **Clean command** | None. Manual DB queries to prune executions |

**Notable**: n8n is entirely server-oriented, not project-oriented. There is no "n8n project" -- it is an instance with a global state directory. All config is env-var-driven, no config files. This is the server-first model, contrasting with code-first tools.

---

### 5. Pulumi

| Aspect | Detail |
|--------|--------|
| **Config file** | `Pulumi.yaml` (YAML) -- project name, runtime, description. `Pulumi.<stack>.yaml` for per-stack config |
| **Runtime dir** | `.pulumi/` (dot-prefixed) -- stack state (checkpoints, history), per-stack subdirs. Gitignored |
| **Init command** | `pulumi new` -- creates `Pulumi.yaml`, `Pulumi.<stack>.yaml`, language entrypoint (`index.ts`, `__main__.py`) |
| **Root detection** | Walks up to find `Pulumi.yaml` (capital P required, `.yaml` or `.yml`) |
| **Secrets** | `pulumi config set --secret <key> <value>` -- encrypts in `Pulumi.<stack>.yaml`. State encryption at rest in `.pulumi/stacks/`. Per-stack encryption keys |
| **Clean command** | None. `pulumi stack rm` deletes stack + state. Manual `.pulumi/` removal |

**Notable**: Pulumi's `Pulumi.yaml` + `.pulumi/` is the **cleanest example** of the config/runtime split pattern. Per-stack config files (`Pulumi.dev.yaml`, `Pulumi.prod.yaml`) is an elegant multi-environment pattern. Secrets are first-class -- encrypted inline in the stack config, not in a separate vault.

- Source: https://www.pulumi.com/docs/concepts/projects/

---

### 6. Turborepo

| Aspect | Detail |
|--------|--------|
| **Config file** | `turbo.json` (JSON) -- task pipeline, caching, global dependencies, env var config |
| **Runtime dir** | `.turbo/` (dot-prefixed) -- local cache, logs, `turbo-trace.json`. Also `node_modules/.cache/turbo/` for global cache |
| **Init command** | `npx create-turbo@latest` -- creates monorepo scaffold: `turbo.json`, `package.json`, example apps/packages |
| **Root detection** | Walks up from workspace to find `turbo.json`, `pnpm-workspace.yaml`, `yarn.lock`, or `package.json` with `workspaces` |
| **Secrets** | `env` key in `turbo.json` with modes (`strict`, `infer`). `--env-mode=strict` for enforced hashing. Sensitive vars marked non-cacheable |
| **Clean command** | `turbo prune --scope=<pkg>` -- generates a minimal monorepo subset (for Docker). No cache clean command. Manual `rm -rf .turbo` |

**Notable**: `turbo prune` is **not** cleanup -- it is a monorepo slicing tool for deployment. The dual cache location (`.turbo/` + `node_modules/.cache/turbo/`) is unusual. Turborepo's `turbo.json` also serves as the monorepo task orchestration config, not just project metadata.

- Source: https://turbo.build/repo/docs/reference/configuration

---

### 7. Mise (mise-en-place)

| Aspect | Detail |
|--------|--------|
| **Config file** | `mise.toml` (TOML, preferred) or `.mise.toml` (legacy dotfile). Also `.mise/config.toml` for directory-based grouping |
| **Runtime dir** | `.mise/` contains config + tasks, but also `~/.local/share/mise/` for global installs. `mise.local.toml` for gitignored local overrides |
| **Init command** | `mise generate config` or `mise use <tool>` -- creates `mise.toml`. No formal `mise init` |
| **Root detection** | Walks up from CWD, finds nearest `mise.toml` (or variants). Configs merge recursively (child overrides parent) |
| **Secrets** | `mise.local.toml` (gitignored) for private settings. No dedicated secrets manager |
| **Clean command** | None built-in. User-defined tasks in `mise.toml` |

**Notable**: Mise has the **most flexible config hierarchy** of any tool surveyed: 7+ config file locations with defined precedence. The `mise.toml` vs `.mise/config.toml` choice lets users pick between root-level simplicity and directory-based grouping. The `.mise/tasks/` convention for file-based task definitions is elegant.

Config precedence (highest first):
1. `mise.local.toml` (gitignored)
2. `mise.toml`
3. `mise/config.toml`
4. `.mise/config.toml`
5. `.config/mise.toml`
6. `.config/mise/config.toml`
7. `.config/mise/conf.d/*.toml`

- Source: https://mise.jdx.dev/configuration.html

---

### 8. Taskfile (go-task)

| Aspect | Detail |
|--------|--------|
| **Config file** | `Taskfile.yml` (YAML, capital T preferred). Also `taskfile.yml`, `Taskfile.yaml`, `Taskfile.dist.yml` |
| **Runtime dir** | `.task/` (dot-prefixed) -- caching, state, timestamps for up-to-date checks |
| **Init command** | `task init` -- creates minimal `Taskfile.yml` with `version: '3'` and sample `hello` task |
| **Root detection** | Walks up from CWD to find first Taskfile (`Taskfile.yml` etc.). Also checks `$HOME/Taskfile.yml` for global tasks |
| **Secrets** | Shell env vars via `{{.ENV_VAR}}` templating. No built-in secret manager |
| **Clean command** | None built-in. Convention: user defines `clean` task |

**Notable**: The `Taskfile.dist.yml` pattern is smart -- a committed template that users copy to `Taskfile.yml` for local customization (like `.env.example` -> `.env`). The `.task/` runtime dir for up-to-date checking timestamps is minimal and well-scoped. go-task's `includes` system with `flatten: true` is a clean way to compose task files across a monorepo.

- Source: https://taskfile.dev/usage/

---

### 9. Just (casey/just)

| Aspect | Detail |
|--------|--------|
| **Config file** | `justfile` (no extension, lowercase preferred) or `Justfile` (capitalized). Custom make-like syntax, not YAML/TOML |
| **Runtime dir** | None. Just is stateless -- no cache, no runtime directory |
| **Init command** | `just --init` -- creates a sample `justfile` |
| **Root detection** | Walks up from CWD until finding first `justfile`/`Justfile`. `--ceiling` flag limits search depth. `set fallback` continues to parent justfiles if recipe missing |
| **Secrets** | Shell env var expansion (`$HOME`). `[private]` attribute hides recipes from listing |
| **Clean command** | None -- Just is purely a command runner with zero state |

**Notable**: Just is the **purest** command runner -- zero state, zero runtime directory, zero cache. The `set fallback` mechanism for cascading justfiles up the directory tree is unique. The custom syntax (not YAML/TOML/JSON) is a deliberate choice for ergonomics -- closer to Makefile but without Make's pitfalls.

- Source: https://github.com/casey/just

---

### 10. Earthly

| Aspect | Detail |
|--------|--------|
| **Config file** | `Earthfile` (no extension, capital E). Dockerfile-like syntax with `VERSION`, targets, `FROM`, `RUN`, `SAVE ARTIFACT` |
| **Runtime dir** | `~/.earthly/` (global, home dir) -- `config.yml` for engine config, build cache. No project-local runtime dir |
| **Init command** | `earthly init` -- creates a basic `Earthfile` template |
| **Root detection** | Walks up from CWD to find `Earthfile`. Supports Earthfiles at any level in monorepos |
| **Secrets** | `--secret` and `--secret-file` CLI flags. Secrets injected as env vars in `RUN` commands. Not cached in layers |
| **Clean command** | None documented. `.earthlyignore` for excluding files from builds |

**Notable**: Earthly follows the Dockerfile mental model -- `Earthfile` is both config and build script. The global `~/.earthly/` for cache (not project-local) is unusual. Earthly's `SAVE ARTIFACT` verb is analogous to Nika's `artifact:` block. The `VERSION 0.8` directive at the top of Earthfile mirrors schema versioning.

- Source: https://docs.earthly.dev/docs/earthfile

---

## Cross-Tool Comparison Matrix

| Tool | Config File | Format | Runtime Dir | Init Command | Root Detection | Secrets | Clean |
|------|-------------|--------|-------------|--------------|----------------|---------|-------|
| **Dagger** | `dagger.json` | JSON | `sdk/` (vendored) | `dagger init` | `dagger.json` | `dagger.Secret` type | No |
| **Temporal** | None (code) | -- | None (server) | None | Language tools | External vaults | No |
| **Prefect** | `prefect.yaml` | YAML/TOML | `.prefect/` | `prefect init` | `prefect.yaml` | Prefect Blocks | No |
| **n8n** | None (env) | -- | `~/.n8n/` | First run | `N8N_USER_FOLDER` | Encrypted DB | No |
| **Pulumi** | `Pulumi.yaml` | YAML | `.pulumi/` | `pulumi new` | `Pulumi.yaml` | `--secret` in stack yaml | `stack rm` |
| **Turborepo** | `turbo.json` | JSON | `.turbo/` | `create-turbo` | `turbo.json` + workspace markers | `env` in turbo.json | `prune` (subset, not clean) |
| **Mise** | `mise.toml` | TOML | `~/.local/share/mise/` | `mise use` | `mise.toml` (hierarchy) | `mise.local.toml` | No |
| **Taskfile** | `Taskfile.yml` | YAML | `.task/` | `task init` | `Taskfile.yml` | Shell env vars | No |
| **Just** | `justfile` | Custom | None | `just --init` | `justfile` | Shell env vars | No |
| **Earthly** | `Earthfile` | Dockerfile-like | `~/.earthly/` | `earthly init` | `Earthfile` | `--secret` flag | No |

---

## Pattern Analysis

### Config File Naming Conventions

Three dominant patterns emerge:

1. **`ToolName.ext`** (capitalized, with extension) -- `Cargo.toml`, `Pulumi.yaml`, `Taskfile.yml`
2. **`tool.ext`** (lowercase, with extension) -- `turbo.json`, `mise.toml`, `dagger.json`, `prefect.yaml`
3. **No extension** -- `justfile`, `Earthfile`, `Dockerfile`, `Makefile`

The no-extension pattern comes from Make heritage. Modern tools overwhelmingly use standard formats (YAML, TOML, JSON) with explicit extensions.

### Runtime Directory Conventions

| Pattern | Examples | When Used |
|---------|----------|-----------|
| `.tool/` (project-local, dot-prefixed) | `.prefect/`, `.pulumi/`, `.turbo/`, `.task/`, `.terraform/` | Most common. Hidden, gitignored, project-scoped |
| `tool/` (project-local, visible) | `target/`, `build/`, `sdk/` | Build outputs the user inspects |
| `~/.tool/` (global, home dir) | `~/.n8n/`, `~/.earthly/`, `~/.cargo/` | Server instances, global caches |
| None | Just, Temporal | Stateless tools or server-managed state |

**Winner**: `.tool/` (dot-prefixed, project-local) is the dominant convention for runtime state.

### Init Command Patterns

Every tool with a config file offers an init command. The convention is:
- `tool init` (most common): Dagger, Prefect, Taskfile, Earthly
- `tool new` (Pulumi): creates project + stack interactively
- `tool --init` (Just): flag-style, creates single file
- No init (Temporal, n8n): server-oriented tools

Init commands universally create **the minimal viable config** -- one file, one example, ready to run.

### Project Root Detection

**Universal pattern**: Walk up from CWD until finding the config file marker.

| Tool | Walk-up? | Fallback |
|------|----------|----------|
| Cargo | Yes (Cargo.toml) | None |
| Just | Yes (justfile) | `--ceiling` limit, `set fallback` cascading |
| Mise | Yes (mise.toml) | Recursive merge (child overrides parent) |
| Taskfile | Yes (Taskfile.yml) | `$HOME/Taskfile.yml` for global tasks |
| Pulumi | Yes (Pulumi.yaml) | None |

Just's `set fallback` is the most sophisticated -- it allows recipes from parent justfiles to cascade down, enabling monorepo-wide defaults with per-project overrides.

### Secrets Handling Patterns

Four distinct approaches:

1. **Type-safe secrets** (Dagger): First-class `Secret` type in the SDK, multiple providers (env, file, vault), auto-scrubbed from logs
2. **Encrypted inline** (Pulumi): `pulumi config set --secret` encrypts values directly in stack YAML files
3. **Server-managed** (Prefect, n8n): Secrets stored in the orchestration server (Prefect Blocks, n8n encrypted DB)
4. **Env vars only** (Just, Taskfile, Mise): No secrets management -- deferred to shell environment

**Best practice**: Dagger and Pulumi represent the gold standard. Dagger for runtime secret injection, Pulumi for encrypted-at-rest in version-controlled files.

---

## Config Format Analysis: TOML vs YAML vs JSON (2025-2026)

### Adoption by Domain

| Domain | Dominant Format | Examples |
|--------|----------------|----------|
| **Developer tooling (Rust/Python)** | TOML | Cargo.toml, pyproject.toml, mise.toml |
| **Workflow/orchestration** | YAML | Prefect, Kubernetes, GitHub Actions, Nika |
| **JavaScript ecosystem** | JSON | turbo.json, dagger.json, package.json, tsconfig.json |
| **Build systems** | Custom DSL | Earthfile, justfile, Makefile, BUILD (Bazel) |

### Format Comparison

| Criterion | TOML | YAML | JSON |
|-----------|------|------|------|
| **Human editing** | Best -- no whitespace traps, explicit syntax | Good -- readable but indentation-sensitive | Worst -- verbose, no comments |
| **Machine generation** | Good | Good | Best -- universal parsing |
| **Comments** | Yes | Yes | No (spec) |
| **Deep nesting** | Awkward (table headers) | Natural | Natural |
| **Schema validation** | Growing (taplo) | Mature (JSON Schema via YAML) | Native JSON Schema |
| **Error messages** | Clear (line-based) | Confusing (indentation) | Clear |
| **Multiline strings** | Excellent (triple quotes) | Excellent (block scalars) | Poor (no native support) |

### Trend

TOML is **gaining ground** for config files in 2025-2026, especially in Rust and Python ecosystems. YAML remains dominant for **workflow definitions** where deep nesting and complex data structures are needed. JSON is declining for human-edited config but remains standard for machine-generated config.

**For workflow engines specifically** (Nika's category), YAML remains the clear winner due to:
- Natural representation of task DAGs with nesting
- Block scalar support for inline prompts/scripts
- Existing mental model from Kubernetes, GitHub Actions, Docker Compose
- Schema validation via JSON Schema (YAML is a JSON superset)

---

## Best Practices: Config vs Runtime Directory Split

### The Universal Pattern

```
project/
  Config.ext          # Versioned, committed, human-edited
  Config.lock         # Versioned, committed, machine-generated (optional)
  .tool/              # Gitignored, machine-managed runtime state
    cache/
    state/
    traces/
```

### Rules Observed Across All Tools

1. **Config file is the root marker** -- tools walk up to find it
2. **Config file is singular** -- one file, not a directory of configs (exceptions: Mise's hierarchy, Pulumi's per-stack files)
3. **Runtime dir is dot-prefixed** -- `.tool/` convention for hidden directories
4. **Runtime dir is gitignored** -- always. No exceptions across all tools surveyed
5. **Config file uses a standard format** -- YAML, TOML, or JSON. Custom DSLs (Just, Earthly) are the exception
6. **Init creates the minimal config** -- one file, one example, ready to run
7. **Clean commands are rare** -- only 1/10 tools has anything resembling cleanup. Users are expected to `rm -rf .tool/`
8. **Secrets never live in the runtime dir** -- they are either env vars, encrypted in config, or server-managed

### The Cargo Model (Gold Standard)

```
project/
  Cargo.toml          # Project manifest (versioned)
  Cargo.lock          # Dependency lockfile (versioned for binaries)
  src/                # Source code
  target/             # Build artifacts (gitignored, can be rm -rf'd safely)
```

Why it works:
- `Cargo.toml` is human-readable, self-documenting, and the sole root marker
- `target/` is entirely disposable -- `cargo clean` removes it
- `Cargo.lock` is the reproducibility guarantee (machine-generated but versioned)
- No hidden directories, no global state pollution

---

## Recommendations for Nika

Based on this research, Nika's current patterns map well to industry conventions:

| Nika Pattern | Industry Equivalent | Assessment |
|--------------|---------------------|------------|
| `.nika.yaml` config | `Taskfile.yml`, `prefect.yaml` | Standard -- YAML for workflow definitions |
| `.nika/` runtime dir | `.prefect/`, `.pulumi/`, `.task/` | Standard -- dot-prefixed, gitignored |
| `nika run` | `task run`, `prefect deploy` | Standard |
| `nika check` | `cargo check`, `pulumi preview` | Standard -- validate without executing |
| `nika init` | `task init`, `pulumi new`, `dagger init` | Standard |
| NikaVault (encrypted) | Pulumi `--secret`, Dagger `dagger.Secret` | Above average -- encrypted at rest |
| `schema: "nika/workflow@0.12"` | Earthly `VERSION 0.8`, Taskfile `version: '3'` | Standard -- version in config |

Areas where Nika could consider improvements:
- **`nika clean`**: No tool surveyed has a good clean command, but `cargo clean` is loved. A `nika clean` that removes `.nika/traces/` and cache would be differentiated
- **Lockfile pattern**: Pulumi's `Pulumi.<stack>.yaml` for per-environment config is worth studying for multi-environment workflow deployment
- **Mise-style hierarchy**: For monorepos, a cascading config resolution (child overrides parent) could be powerful

---

## Sources

1. Dagger Docs -- Module Configuration (https://docs.dagger.io/reference/configuration/modules)
2. Dagger Docs -- Module Initialization (https://docs.dagger.io/extending/modules)
3. Prefect Docs -- Deployments (https://docs.prefect.io/latest/concepts/deployments/)
4. Pulumi Docs -- Projects (https://www.pulumi.com/docs/concepts/projects/)
5. Turborepo Docs -- Configuration (https://turbo.build/repo/docs/reference/configuration)
6. Mise Docs -- Configuration (https://mise.jdx.dev/configuration.html)
7. Taskfile Docs -- Usage (https://taskfile.dev/usage/)
8. Just README (https://github.com/casey/just)
9. Earthly Docs -- Earthfile Reference (https://docs.earthly.dev/docs/earthfile)
10. n8n Docs -- Self-hosting (https://docs.n8n.io/hosting/)

## Methodology

- Tools used: Perplexity AI (sonar-pro) for web search, cross-referenced across multiple queries
- 10 tools analyzed, 12 search queries executed
- Time period covered: 2024-2026 (focus on latest stable versions)
- All findings verified against official documentation where citations were available

## Confidence Level

**High** -- All core findings (config files, runtime dirs, init commands) are well-documented by the tools themselves. Medium confidence on some clean/prune details where documentation was sparse.

## Further Research Suggestions

- Deep dive into Mise's config hierarchy for Nika monorepo support
- Study Pulumi's per-stack config pattern for multi-environment workflow deployment
- Investigate Dagger's `dagger.Secret` type system for potential Nika secret improvements
- Research how Bazel and Nx handle project root detection in very large monorepos
- Compare `nika doctor` with similar diagnostic commands across tools (`cargo doctor`, `brew doctor`)
