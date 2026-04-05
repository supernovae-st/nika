# Secrets Management UX Research

> How popular CLI tools and developer platforms handle API keys from a user experience perspective.
> Researched 2026-04-04. Sources: official documentation of each tool.

---

## Executive Summary

After analyzing 10 tools across 3 categories (deployment platforms, secrets-as-a-service, and password managers), a clear hierarchy of UX simplicity emerges:

**Simplest (1-2 concepts):** Fly.io, Supabase, Stripe CLI, GitHub CLI
**Medium (3-4 concepts):** Vercel, Railway, Infisical
**Complex (5+ concepts):** Doppler, 1Password CLI

The winning pattern is: **`tool secret set KEY=VALUE`** with secrets stored in the cloud and injected as environment variables at runtime. The gold standard for LOCAL-first tools is: **`tool login` (once) then `$env.KEY` everywhere**.

---

## Tool-by-Tool Analysis

### 1. Fly.io Secrets

**Category:** Deployment platform (PaaS)

| Aspect | Detail |
|--------|--------|
| **Set a secret** | `fly secrets set DATABASE_URL=postgres://...` |
| **List secrets** | `fly secrets list` (names only, values never shown) |
| **Remove** | `fly secrets unset MY_SECRET` |
| **Storage** | Encrypted vault on Fly.io servers. API servers can only encrypt, never decrypt. |
| **Sync** | Setting a secret auto-redeploys all Machines (restarts). `--stage` defers deployment. |
| **Concepts to learn** | 2: secrets and Machines |
| **Onboarding** | `fly auth login` (browser OAuth) then `fly secrets set KEY=VALUE` |
| **Read from stdin** | `cat file.txt | fly secrets set MY_SECRET` |
| **Runtime injection** | Environment variables available at boot time via temporary auth token |
| **Security model** | Secrets decrypted on host at boot via temp token. Host loses access when Machine is destroyed. Values never logged. |

**UX Grade: A+** -- The simplest mental model. Set a key=value, it becomes an env var. Done.

**Killer feature:** `--stage` lets you batch secret changes without triggering redeployments until you're ready.

---

### 2. Supabase CLI Secrets

**Category:** Backend-as-a-Service

| Aspect | Detail |
|--------|--------|
| **Set a secret** | `supabase secrets set MY_SECRET=value` |
| **List secrets** | `supabase secrets list` |
| **Remove** | `supabase secrets unset MY_SECRET` |
| **Storage** | Supabase cloud (linked project). Auth token stored in native OS credential store, fallback to `~/.supabase/access-token` plaintext. |
| **Sync** | Secrets are per-project on the Supabase platform. `supabase link` connects local to remote. |
| **Concepts to learn** | 2: secrets and project link |
| **Onboarding** | `supabase login` (access token from dashboard) then `supabase link --project-ref <ref>` |
| **CI/CD** | Skip login with `SUPABASE_ACCESS_TOKEN` env var |

**UX Grade: A** -- Nearly identical to Fly.io. Simple key=value, linked to a project.

---

### 3. Stripe CLI Auth

**Category:** Payment platform

| Aspect | Detail |
|--------|--------|
| **Login** | `stripe login` (opens browser, OAuth) |
| **Storage** | `~/.config/stripe/config.toml` on local machine + restricted keys on Stripe dashboard |
| **Key type** | CLI generates **restricted keys** (not full secret keys) valid for 90 days |
| **Override** | `stripe login --api-key sk_test_...` for inline key |
| **Concepts to learn** | 2: login and restricted keys |
| **Onboarding** | `stripe login` opens browser, authenticates, stores restricted key locally |
| **CI/CD** | `STRIPE_API_KEY` env var |

**UX Grade: A** -- `stripe login` and you're done. Auto-rotating restricted keys is a smart security default.

**Killer feature:** The CLI never uses your full secret key. It generates restricted, time-limited keys automatically.

---

### 4. GitHub CLI (gh) Auth

**Category:** Developer platform

| Aspect | Detail |
|--------|--------|
| **Login** | `gh auth login` (interactive: hostname, protocol, browser/token) |
| **Storage** | System credential store (macOS Keychain, Windows Credential Manager). Fallback: plaintext file. |
| **Override** | `GH_TOKEN` env var, or `--with-token` flag |
| **List status** | `gh auth status` (shows stored location) |
| **Switch accounts** | `gh auth switch` |
| **Concepts to learn** | 2: login and token |
| **Onboarding** | `gh auth login` interactive wizard (choose host, auth method, protocol) |
| **Scopes** | Minimum: `repo`, `read:org`, `gist` |

**UX Grade: A** -- The interactive login wizard is the gold standard for first-time setup. Browser-based OAuth means zero typing of tokens.

**Killer feature:** Integrates with system credential store natively (Keychain on macOS). No custom vault needed.

---

### 5. Vercel CLI Env

**Category:** Deployment platform (PaaS)

| Aspect | Detail |
|--------|--------|
| **Add a variable** | `vercel env add SECRET_NAME` (interactive prompt for value + environment) |
| **Add non-interactive** | `vercel env add SECRET_NAME production` then pipe value, or `< file` |
| **List** | `vercel env ls [environment] [gitbranch]` |
| **Remove** | `vercel env rm SECRET_NAME production` |
| **Pull to local** | `vercel env pull .env.local` (downloads Development env vars to file) |
| **Run with env** | `vercel env run -- next dev` (injects env vars without writing to disk) |
| **Storage** | Vercel cloud (per-project, per-environment) |
| **Sync** | `vercel env pull` downloads from cloud. Must re-run after changes. |
| **Concepts to learn** | 3: environments (development/preview/production), variables, git branches |
| **Onboarding** | `vercel login` then `vercel link` then `vercel env add` |
| **Sensitive mode** | `vercel env add API_TOKEN --sensitive` (hidden in dashboard) |

**UX Grade: B+** -- The environment dimension (dev/preview/prod) adds complexity but is genuinely useful. The `env run` command that injects without writing to disk is excellent.

**Killer feature:** `vercel env run -- next dev` injects secrets as env vars for a single command without ever writing them to disk.

---

### 6. Railway Variables

**Category:** Deployment platform (PaaS)

| Aspect | Detail |
|--------|--------|
| **Set a variable** | Via dashboard (no direct `railway variable set` in CLI -- CLI uses `railway variables` to list) |
| **Run with vars** | `railway run <command>` injects project variables as env vars |
| **Shell mode** | `railway shell` opens a subshell with all variables injected |
| **Storage** | Railway cloud (per-service, per-environment) |
| **Variable types** | Service variables, Shared variables, Reference variables (cross-service) |
| **Reference syntax** | `${{ shared.VAR }}`, `${{ SERVICE_NAME.VAR }}` |
| **Sealed variables** | Cannot be un-sealed, not visible in UI/API, not available via `railway run` |
| **Concepts to learn** | 4: service vars, shared vars, reference vars, environments |
| **Onboarding** | `railway login` then `railway link` (select project + service) |
| **Staged changes** | Adding/updating vars creates staged changes that must be deployed |

**UX Grade: B** -- Reference variables across services are powerful but add cognitive load. The "staged changes" concept is borrowed from git and adds friction.

**Killer feature:** Cross-service reference variables (`${{ SERVICE_NAME.VAR }}`) are extremely useful for microservice architectures.

---

### 7. Infisical

**Category:** Secrets-as-a-Service (open source)

| Aspect | Detail |
|--------|--------|
| **Login** | `infisical login` (browser OAuth or machine identity) |
| **Init project** | `infisical init` (select org + project, creates `.infisical.json`) |
| **Set a secret** | `infisical secrets set KEY=value` |
| **Set from file** | `infisical secrets set --file="./.env"` (supports .env and YAML) |
| **Set from path** | `infisical secrets set CERT=@/path/to/cert.pem` |
| **Get specific** | `infisical secrets get KEY --plain --silent` |
| **Delete** | `infisical secrets delete KEY1 KEY2` |
| **Run with secrets** | `infisical run -- npm run dev` |
| **Watch mode** | `infisical run --watch -- npm run dev` (auto-restart on secret change) |
| **Storage** | Infisical cloud (or self-hosted) |
| **Sync** | Real-time via API. Environments: dev, staging, test, prod. |
| **Concepts to learn** | 4: login, project init, environments, paths (folders) |
| **CI/CD** | `INFISICAL_TOKEN` env var (machine identity or service token) |
| **Export** | `infisical export --format=dotenv > .env` |

**UX Grade: B+** -- The `infisical run` command is the cleanest pattern. The `--watch` flag for auto-reload during dev is genuinely innovative. The `@path/to/file` syntax for setting from files is elegant.

**Killer features:**
- `--watch` auto-restarts when secrets change remotely
- `set KEY=@/path/to/file` loads value from file
- `--file="./.env"` bulk imports from .env files

---

### 8. Doppler

**Category:** Secrets-as-a-Service (commercial)

| Aspect | Detail |
|--------|--------|
| **Login** | `doppler login` (browser OAuth, per-workplace) |
| **Setup project** | `doppler setup` (interactive: choose project + config) |
| **Run with secrets** | `doppler run -- npm run dev` |
| **Run with flags** | `doppler run -p PROJECT -c CONFIG -- command` |
| **Storage** | Doppler cloud. Local mapping in `~/.doppler/.doppler.yaml` |
| **Config hierarchy** | Workplace > Project > Config (dev, staging, production) |
| **Sync** | Real-time via API. CLI resolves project+config by current directory. |
| **Concepts to learn** | 5: workplace, project, config, environment, scope (directory mapping) |
| **Onboarding** | `doppler login` then `doppler setup` (interactive per directory) |
| **Troubleshoot** | `doppler configure --scope $(pwd)` shows what config applies |
| **Reset** | `doppler configure reset` clears all local mappings |

**UX Grade: B-** -- The directory-scoped configuration is clever but creates surprising behavior when moving directories. 5 concepts is too many for "just give me my API key."

**Killer feature:** Directory-scoped resolution. Run `doppler setup` once in a directory, and `doppler run` always knows which project/config to use. No config files in the project.

**Pain point:** Moving a directory breaks the mapping (stored by absolute path in `~/.doppler/.doppler.yaml`).

---

### 9. 1Password CLI

**Category:** Password manager

| Aspect | Detail |
|--------|--------|
| **Login** | Integrate with 1Password desktop app, then any `op` command prompts auth (Touch ID / Windows Hello) |
| **Read a secret** | `op read "op://vault/item/field"` |
| **Inject into env** | `op run --env-file=.env -- npm run dev` |
| **Inject into config** | `op inject -i config.tpl -o config.json` |
| **Secret references** | `op://vault-name/item-name/field-name` URI syntax |
| **Shell plugins** | Auto-authenticate AWS, GitHub, etc. via `op plugin run` |
| **Storage** | 1Password vault (encrypted, synced across devices) |
| **Concepts to learn** | 5-6: vaults, items, fields, secret references (URI syntax), shell plugins, service accounts |
| **Onboarding** | Install CLI, enable desktop app integration, authenticate via biometric |
| **CI/CD** | Service Accounts with vault-scoped access |

**UX Grade: B-** -- The `op://vault/item/field` URI syntax is powerful but requires understanding 1Password's data model. The desktop app integration with Touch ID is beautiful UX, but the mental model is heavy for "I just need my OpenAI key."

**Killer feature:** Touch ID / biometric authentication. `op run` never writes secrets to disk.

---

### 10. Cursor / Windsurf (AI Code Editors)

**Category:** AI IDE

| Aspect | Detail |
|--------|--------|
| **Set API key** | Settings > Models > Enter API key in GUI field |
| **Storage** | Cursor stores in VS Code's `SecretStorage` API (OS Keychain on macOS, libsecret on Linux, Credential Manager on Windows) |
| **Concepts to learn** | 1: paste your key in settings |
| **Onboarding** | Open settings, paste key, done |
| **Sync** | No sync -- per-device only |

**UX Grade: A+ for simplicity, D for security** -- The simplest possible UX (paste in GUI), but no sync, no rotation, no team sharing.

---

## Pattern Taxonomy

### Pattern 1: "Login + Run" (Best overall)
```
tool login          # Once, opens browser
tool run -- cmd     # Injects secrets as env vars
```
**Used by:** Doppler, Infisical, Vercel (`env run`), Railway (`run`)
**Pros:** Zero files on disk, always fresh secrets, works in CI with token env var
**Cons:** Requires network access, latency on each run

### Pattern 2: "Set + Inject" (Simplest for platforms)
```
tool secrets set KEY=VALUE
# Secrets auto-available as env vars at runtime
```
**Used by:** Fly.io, Supabase
**Pros:** One concept (key=value), secrets stored where app runs
**Cons:** Only works for deployed apps, not local dev

### Pattern 3: "Pull + .env" (Vercel model)
```
tool env pull .env.local    # Download secrets to file
# Framework reads .env.local automatically
```
**Used by:** Vercel (`env pull`), Railway (via `railway run`)
**Pros:** Works with any framework's .env support
**Cons:** Secrets written to disk, must re-pull after changes, risk of committing .env

### Pattern 4: "Reference URI" (1Password model)
```
op://vault/item/field       # Secret reference in config
op run --env-file=.env -- cmd   # Resolves at runtime
```
**Used by:** 1Password CLI
**Pros:** Config files can be committed (no secrets in them), powerful templating
**Cons:** Requires understanding URI syntax, heavy mental model

### Pattern 5: "Env Var Override" (Universal escape hatch)
```
export TOOL_TOKEN=xxx       # For CI/CD
export API_KEY=xxx          # Direct usage
```
**Used by:** ALL tools as fallback. GitHub CLI (`GH_TOKEN`), Stripe (`STRIPE_API_KEY`), Infisical (`INFISICAL_TOKEN`), Supabase (`SUPABASE_ACCESS_TOKEN`)
**Pros:** Universal, zero learning curve, works everywhere
**Cons:** No encryption, visible in process list, easy to leak in logs

---

## Onboarding Complexity Ranking

| Rank | Tool | Steps to First Secret | Concepts |
|------|------|----------------------|----------|
| 1 | Fly.io | 2 (`fly auth login` + `fly secrets set K=V`) | 2 |
| 2 | Supabase | 3 (`login` + `link` + `secrets set K=V`) | 2 |
| 3 | Stripe CLI | 1 (`stripe login`) | 2 |
| 4 | GitHub CLI | 1 (`gh auth login`) | 2 |
| 5 | Infisical | 3 (`login` + `init` + `secrets set K=V`) | 4 |
| 6 | Vercel | 3 (`login` + `link` + `env add`) | 3 |
| 7 | Railway | 3 (`login` + `link` + set via dashboard) | 4 |
| 8 | Doppler | 2 (`login` + `setup`) | 5 |
| 9 | 1Password | 3 (install app + enable integration + `op read`) | 5-6 |

---

## Storage Location Comparison

| Tool | Where Stored | Encryption | Sync |
|------|-------------|------------|------|
| Fly.io | Cloud vault | Yes (server-side) | Auto (redeploy) |
| Supabase | Cloud + OS keychain | OS keychain for token | Per-project |
| Stripe | `~/.config/stripe/config.toml` | No (plaintext TOML) | None |
| GitHub | OS credential store, fallback plaintext | OS-level | None |
| Vercel | Cloud | Yes (server-side) | `env pull` manual |
| Railway | Cloud | Yes (server-side) | `railway run` |
| Infisical | Cloud (or self-hosted) | Yes (E2E encrypted) | `infisical run` |
| Doppler | Cloud | Yes (server-side) | `doppler run` |
| 1Password | 1Password vault | Yes (E2E encrypted) | Across devices |
| Cursor | OS keychain (VS Code SecretStorage) | OS-level | None |

---

## Key Insights for Nika

### What the best tools do RIGHT

1. **`tool login` is a one-time ceremony.** Every tool uses browser-based OAuth or a token prompt. Nobody asks users to manually edit config files.

2. **Env vars are the universal interface.** Every tool injects secrets as environment variables. Not custom config, not special APIs -- just env vars.

3. **The `run` command is the killer feature.** `doppler run -- cmd`, `infisical run -- cmd`, `vercel env run -- cmd` -- inject secrets for the duration of one command, nothing written to disk.

4. **CI/CD always uses env var override.** `TOOL_TOKEN=xxx tool run` -- every tool supports this as the headless/CI path.

5. **Secrets are never readable.** `fly secrets list` shows names only. `vercel env ls` shows names only. The value is gone forever after setting.

6. **Simple tools use 2 concepts.** Login + set. That's it.

### What tools get WRONG

1. **Too many concepts.** Doppler's workplace > project > config > environment hierarchy is powerful but overwhelming for someone who just wants their OpenAI key.

2. **Directory-scoped magic.** Doppler's `~/.doppler/.doppler.yaml` breaks when you move directories. Invisible state is debugging hell.

3. **Requiring a web dashboard.** Railway requires the dashboard to set variables -- the CLI can only read them.

4. **No local-only mode.** Doppler, Infisical, and Railway all require network access to use secrets. No offline fallback.

5. **Staged changes.** Railway's "changes must be deployed" adds unnecessary friction for secret management.

### The SIMPLEST possible UX for a CLI tool

Based on this research, the minimum viable secrets UX is:

```
# First time (once ever)
nika setup                          # Interactive wizard: paste keys, stored encrypted locally

# Daily use (zero friction)
nika run workflow.nika.yaml         # Keys auto-resolved from $env.API_KEY
                                    # Resolution: env var > daemon > vault > error

# CI/CD (universal)
ANTHROPIC_API_KEY=xxx nika run ...  # Env var override, nothing to configure

# Check status
nika provider list                  # Shows which keys are configured and valid
```

**Concepts needed: 2** -- provider (which AI service) and key (the secret). That's it.

### Recommendations for Nika (ranked by impact)

1. **Keep the current model.** Nika's resolution chain (env vars > daemon IPC > NikaVault > error) is already best-in-class. It's simpler than Doppler and more secure than Stripe.

2. **`nika keys set` is the right command.** It's equivalent to `fly secrets set` but scoped to the domain (AI providers). One command, one concept.

3. **Never show key values.** Like Fly.io -- show that a key exists, never its value.

4. **The `nika setup` wizard is crucial.** Like `gh auth login`, the interactive first-time experience should be beautiful. Walk through providers one by one.

5. **Always support env var override.** `ANTHROPIC_API_KEY=xxx nika run ...` must always work. This is the CI/CD path.

6. **Consider `nika run --inject` mode.** Like `vercel env run` or `doppler run` -- inject keys as env vars into a subprocess without writing to disk. Useful for running non-Nika tools with the same keys.

7. **`nika doctor` should validate keys.** Like `nika provider list` already does -- show green/red status for each configured provider.

---

## Sources

1. [Vercel CLI env docs](https://vercel.com/docs/cli/env) -- Commands, environments, pull, run
2. [Railway Variables guide](https://docs.railway.com/guides/variables) -- Service vars, shared vars, reference vars, sealed vars
3. [Doppler Install CLI](https://docs.doppler.com/docs/install-cli) -- Login, setup, run, directory scoping
4. [Doppler Secrets Setup Guide](https://docs.doppler.com/docs/secrets-setup-guide) -- Config resolution, troubleshooting
5. [Infisical CLI secrets](https://infisical.com/docs/cli/commands/secrets) -- CRUD, file import, path syntax
6. [Infisical CLI run](https://infisical.com/docs/cli/commands/run) -- Inject, watch, flags
7. [1Password CLI get started](https://developer.1password.com/docs/cli/get-started/) -- Install, app integration, biometric
8. [1Password CLI load secrets](https://developer.1password.com/docs/cli/secrets-scripts/) -- op run, op read, op inject, shell plugins
9. [Fly.io Secrets](https://fly.io/docs/apps/secrets/) -- Architecture, set/list/unset, vault design
10. [Supabase CLI Reference](https://supabase.com/docs/reference/cli/supabase-secrets) -- secrets set/list/unset, credential storage
11. [Stripe CLI keys](https://docs.stripe.com/stripe-cli/keys) -- Restricted keys, `~/.config/stripe/config.toml`
12. [GitHub CLI auth login](https://cli.github.com/manual/gh_auth_login) -- System credential store, fallback, env vars

## Methodology

- Tools analyzed: 10
- Documentation pages scraped: 15+
- Categories covered: Deployment platforms (Fly, Vercel, Railway, Supabase), Secrets-as-a-Service (Doppler, Infisical), Password managers (1Password), Developer CLIs (Stripe, GitHub, Cursor)
- Focus: User experience, not security architecture

## Confidence Level

**High** -- All information sourced from official documentation. Patterns are well-established and unlikely to change significantly.
