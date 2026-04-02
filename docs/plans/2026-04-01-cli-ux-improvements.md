# CLI UX Improvements — Error Messages & Guidance

> **For Claude:** Handoff for improving nika CLI user experience.
> Working directory: `/Users/thibaut/dev/supernovae/nika`

---

## Problem

The CLI error messages are hostile. When a user makes a typo or gets the subcommand order wrong, they get a generic clap error that doesn't help:

```
$ nika set provider openai
error: unrecognized subcommand 'openai'

Usage: nika provider [OPTIONS] <COMMAND>

For more information, try '--help'.
```

The user meant `nika provider set openai`. The CLI should:
1. Detect the likely intent
2. Suggest the correct command
3. Be friendly, not robotic

## What to fix

### 1. "Did you mean?" suggestions on typos

When a subcommand is close to a valid one (Levenshtein distance ≤ 2), suggest it:

```
$ nika set provider openai
error: unknown subcommand 'set'

  Did you mean?  nika provider set openai

For all commands: nika help
```

```
$ nika porider list
error: unknown subcommand 'porider'

  Did you mean?  nika provider list

For all commands: nika help
```

**Implementation:** Use clap's built-in `suggest` feature or add a post-parse fuzzy matcher. Look at how `git` does "Did you mean?" suggestions.

### 2. Better error context for common mistakes

```
$ nika run
error: missing workflow file

  Usage:  nika run <workflow.nika.yaml>

  Try:    nika run my-workflow.nika.yaml
          nika ui                          (interactive file picker)
          ls *.nika.yaml                   (list workflows in current dir)
```

```
$ nika serve
error: NIKA_SERVE_TOKEN is required

  Set a token (min 16 chars):
    export NIKA_SERVE_TOKEN=$(openssl rand -hex 32)
    nika serve

  Or create a .env file:
    echo "NIKA_SERVE_TOKEN=$(openssl rand -hex 32)" > .nika/.env
```

### 3. Friendlier first-run experience

When nika detects no API keys and the user tries to run a workflow:

```
$ nika run hello.nika.yaml
  ⚠ No LLM providers configured

  This workflow needs 'openai'. Let's set it up:

    nika provider set openai

  Or use mock provider (no API key needed):
    nika run hello.nika.yaml --provider mock

  Full setup wizard:
    nika setup
```

### 4. nika serve startup banner

Currently `nika serve` starts with no output (just a blinking cursor). Should show:

```
$ nika serve

  🦋 Nika Serve v0.58.1

  ├── Listening    http://0.0.0.0:3000
  ├── Workflows    /opt/nika/nk-jungo/workflows/ (4 files)
  ├── Executor     embedded
  ├── Max jobs     6 concurrent
  ├── Timeout      600s per job
  ├── Auth         Bearer token (64 chars)
  └── Providers    openai ✓  xai ✓  gemini ✗

  Ready. Waiting for requests...

  Ctrl+C to stop
```

### 5. nika provider set — guide the user

Currently `nika provider set openai` just shows a bare prompt. Should guide:

```
$ nika provider set openai

  🔑 OpenAI API Key

  Get your key at: https://platform.openai.com/api-keys
  Paste it below (starts with sk-):

  Key: sk-proj-****

  Testing connection... ✓ gpt-4.1-mini responded in 340ms

  ✓ OpenAI configured and working
```

### 6. Colored, structured error output

All errors should use consistent formatting:

```
Error (red):     ✗ Something went wrong
Warning (yellow): ⚠ Something might be wrong
Success (green):  ✓ Something worked
Info (blue):      ℹ Something to know
Hint (gray):      → Try this instead
```

## Files to modify

| File | What |
|------|------|
| `tools/nika/src/main.rs` | Post-parse error handling, "did you mean?" |
| `tools/nika-cli/src/provider.rs` | Provider set UX (URL hint, connection test) |
| `tools/nika-serve/src/lib.rs` | Serve startup banner |
| `tools/nika-serve/src/config.rs` | Better error for missing NIKA_SERVE_TOKEN |
| `tools/nika-engine/src/display/` | Consistent error formatting helpers |

### 7. BUG (BLOCKER): Onboarding wizard triggers on EVERY infer, cannot be skipped

`nika infer "prompt" --provider openai` on a fresh VPS triggers the "Welcome to Nika! Let's set up your first provider" wizard, even though the user JUST ran `nika provider set openai` and got `Connection OK`. The wizard re-asks for the same API key.

**Root cause:** The onboarding check probably looks at a different state than the provider vault — maybe a config flag like `onboarding_complete` that `nika provider set` doesn't set.

**Fix:** If any provider key exists in the vault, skip the onboarding wizard. Or: `nika provider set` should mark onboarding as done.

### 8. BUG: Default provider is anthropic even when only openai is configured

`nika infer "prompt"` (without `--provider`) defaults to anthropic, fails with "No provider configured. Fix: export ANTHROPIC_API_KEY". Even though openai is configured and working.

The default should be the first CONFIGURED provider, not hardcoded anthropic. Or at minimum, the error should say "openai is configured, did you mean `--provider openai`?".

### 9. BUG: `nika provider list` shows phantom keys

`nika provider list` on nk-jungo-vps shows `openai ✓ [***] (daemon)` but `nika provider test openai` says "No API key". The daemon reports a key exists (probably from an env var in the system or a stale vault entry) but it's not actually usable. The list and test commands disagree.

**Fix:** `nika provider list` should run `test` internally (or at least verify the key is non-empty) before showing ✓.

### 8. BRAINSTORM: Rethink "provider" as "endpoint"

Current model is confusing:
- `provider: openai` = cloud API (api.openai.com, paid)
- `provider: openai` + `base_url: http://gpu:8000/v1` = self-hosted vLLM (free)

Same "provider" name, completely different destination and cost. The user should think in terms of WHERE the request goes, not WHICH protocol it speaks.

**Proposal:** Add `nika endpoint` as a first-class concept:

```bash
# Cloud endpoints (auto-configured from provider keys)
nika endpoint list
  openai      → api.openai.com          sk-proj-**** ✓   $0.002/req
  xai         → api.x.ai                xai-****     ✓   $0.003/req

# Custom endpoints (self-hosted)
nika endpoint add qwen-h100 --url http://51.159.153.241:8000/v1 --protocol openai
nika endpoint list
  openai      → api.openai.com          sk-proj-**** ✓   $0.002/req
  xai         → api.x.ai                xai-****     ✓   $0.003/req
  qwen-h100   → 51.159.153.241:8000     (no key)     ✓   free
```

In workflows:
```yaml
# Cloud
provider: openai
model: gpt-4.1-mini

# Self-hosted GPU (clearer than base_url hack)
endpoint: qwen-h100
model: Qwen3.5-27B
```

`endpoint:` = explicit name that maps to a URL + optional key + protocol.
`provider:` = kept for backward compat, maps to default cloud endpoint.

This is a bigger design change — brainstorm and ADR before implementing.

### 10. BUG (BLOCKER): `nika provider set` is silently overridden by env vars

`nika provider set openai` stores a new key in the vault and says "Connection OK". But at runtime, if `OPENAI_API_KEY` exists in the environment (from .bashrc, .profile, systemd EnvironmentFile, or parent process), the env var takes priority over the vault. The old/invalid env key is used, the new vault key is ignored.

The user has no idea this is happening. They just set a new key, tested it, got "Connection OK", and then `nika infer` fails with "invalid key".

**Resolution order (current):** env var > daemon IPC > vault > None
**The problem:** env var ALWAYS wins, even when stale/invalid

**Fix options:**
- **Option A (minimal):** `nika provider set` warns if an env var exists: "Warning: OPENAI_API_KEY found in environment. It will override the vault key. Remove it with: unset OPENAI_API_KEY"
- **Option B (better):** Invert priority: vault > env var. If the user explicitly set a key via `nika provider set`, that should win.
- **Option C (best):** `nika provider set` detects env var conflicts AND offers to unset/override: "OPENAI_API_KEY found in env (sk-old...). Replace with the new key? [Y/n]"

Also `nika provider get openai` shows `(env)` which is a hint, but most users won't notice or understand what it means.

### 11. BUG: `--no-interactive` missing on `nika infer`

`nika run` has `--no-interactive` to skip prompts. `nika infer` does NOT — it's not a recognized flag. This makes `nika infer` unusable in scripts and on headless VPS (the onboarding wizard blocks forever).

All verbs that can be called from CLI should support `--no-interactive`: `infer`, `fetch`, `invoke`, `agent`.

### 11. IMPROVEMENT: Missing flags consistency across verbs

| Flag | `nika run` | `nika infer` | `nika fetch` | `nika invoke` | `nika agent` |
|------|-----------|-------------|-------------|--------------|-------------|
| `--no-interactive` | ✓ | ✗ | ? | ? | ? |
| `-p --provider` | ✓ | ✓ | ✗ (N/A) | ✗ (N/A) | ✓ |
| `-y --yes` | ✓ | ✗ | ✗ | ✗ | ✗ |
| `--no-live` | ✓ | ✗ | ✗ | ✗ | ✗ |
| `-q --quiet` | ✓ | ✗ | ✗ | ✗ | ✗ |

All verbs should support `--no-interactive` and `-q` at minimum for headless/script usage.

### 12. IMPROVEMENT: `NIKA_NO_ONBOARDING=1` env var

For VPS/Docker/CI, there should be an env var to permanently skip the onboarding wizard. The wizard is great for first-time desktop users, terrible for servers.

```bash
# In .bashrc or systemd EnvironmentFile
export NIKA_NO_ONBOARDING=1
```

### 14. BUG SUMMARY: CLI is unusable on headless VPS

The CLI cannot be used on a headless VPS without hitting the onboarding wizard on every command. The combination of bugs 7-12 makes it impossible to use `nika infer` or `nika run` interactively on a VPS. The only workaround is to bypass the CLI entirely and use `nika serve` (embedded executor) with API keys in the .env file.

**Workaround:** Put API keys directly in `/home/nika/.nika/.env` as env vars:
```
OPENAI_API_KEY=sk-proj-...
XAI_API_KEY=xai-...
```
Then restart `nika serve`. The embedded executor reads env vars directly — no vault, no daemon, no wizard.

**Root cause chain:**
1. Onboarding wizard checks a flag that `nika provider set` doesn't update → wizard loops
2. Vault keys are overridden by stale env vars → wrong key used
3. `--no-interactive` doesn't exist on `nika infer` → can't skip wizard
4. Default provider is anthropic even when only openai configured → fails without -p flag
5. `nika provider list` shows phantom keys → user thinks it's configured

This needs a dedicated bug-fix sprint.

## Priority

This is a separate sprint from the serve V3 bugs. It's UX polish that makes nika feel professional. Every interaction should guide the user to success, not leave them guessing.

### 15. IMPROVEMENT: `nika provider list` should show models, endpoints, cost, status

Current display is useless — just a list of names with ✓/✗. Should show:

```
$ nika provider list

  Cloud Providers
  ──────────────────────────────────────────────────────────────────
  ✓ openai       api.openai.com         sk-proj-****
    ├── gpt-4.1           $2.00/M in   $8.00/M out   (smartest)
    ├── gpt-4.1-mini      $0.40/M in   $1.60/M out   (fast+cheap)
    ├── gpt-4o            $2.50/M in   $10.0/M out
    └── o4-mini           $1.10/M in   $4.40/M out   (reasoning)

  ✗ xai          api.x.ai               (not configured)
    ├── grok-3            $3.00/M in   $15.0/M out
    └── grok-3-mini       $0.30/M in   $0.50/M out
    → nika provider set xai

  ✗ anthropic    api.anthropic.com       (not configured)
    → nika provider set anthropic

  Custom Endpoints
  ──────────────────────────────────────────────────────────────────
  ✓ qwen-h100    51.159.153.241:8000    (no key needed)    free
    └── Qwen3.5-27B      0 tok/s       (vLLM)

  Local
  ──────────────────────────────────────────────────────────────────
  ✗ native       (no GGUF model loaded)
    → nika model pull <name>

  Test
  ──────────────────────────────────────────────────────────────────
  ✓ mock         (always available, deterministic responses)
```

This shows at a glance:
- Which providers are configured vs not
- What models are available on each
- Approximate cost per provider
- Custom endpoints (vLLM, Ollama, etc.)
- What action to take for unconfigured ones

### 16. IMPROVEMENT: `nika model list` should show what's AVAILABLE to me

Current `nika model list` shows ALL models from ALL providers. Should filter to what the user can actually use:

```
$ nika model list

  Available models (1 provider configured)
  ──────────────────────────────────────────────────────────────────
  openai/gpt-4.1-mini     $0.40/M in   $1.60/M out   128K ctx   recommended
  openai/gpt-4.1          $2.00/M in   $8.00/M out   128K ctx
  openai/gpt-4o           $2.50/M in   $10.0/M out   128K ctx
  openai/o4-mini          $1.10/M in   $4.40/M out   128K ctx   reasoning

  mock/mock               free         no API needed              testing

  Unavailable (need API key)
  ──────────────────────────────────────────────────────────────────
  anthropic/claude-sonnet-4   → nika provider set anthropic
  xai/grok-3-mini             → nika provider set xai
  gemini/gemini-2.5-flash     → nika provider set gemini
```

### 17. IMPROVEMENT: `nika doctor` should check VPS readiness

`nika doctor` should have a `--serve` mode that checks everything needed for nika serve:

```
$ nika doctor --serve

  System
  ✓ nika v0.58.1
  ✓ SQLite available
  ✓ Disk: 5.6 GB free

  Providers
  ✓ openai: connection OK (gpt-4.1-mini responded in 340ms)
  ✗ xai: not configured
  ✗ anthropic: not configured

  Serve Config
  ✓ NIKA_SERVE_TOKEN: set (64 chars)
  ✓ NIKA_SERVE_BIND: 0.0.0.0:3000
  ✓ NIKA_SERVE_WORKFLOWS: /opt/nika/nk-jungo/workflows/ (4 files)
  ✗ NIKA_SERVE_EXECUTOR: subprocess (recommend: embedded)

  Workflows
  ✓ translate.nika.yaml — valid
  ✓ translate-all.nika.yaml — valid
  ✓ pull-repo.nika.yaml — valid
  ✓ push-output.nika.yaml — valid

  Network
  ✓ Port 3000: available
  ✓ vLLM endpoint 51.159.153.241:8000: reachable (Qwen3.5-27B)

  Ready to serve!
  Run: nika serve
```

### 18. IMPROVEMENT: `nika serve` should validate on startup

Before accepting requests, nika serve should check:
- All workflows in the dir are valid YAML
- Required providers have API keys
- Custom endpoints are reachable
- SQLite DB is writable

Show warnings at startup:
```
  ⚠ translate.nika.yaml uses provider 'openai' but no API key found
  ⚠ vLLM endpoint 51.159.153.241:8000 unreachable
```

### 19. IMPROVEMENT: CLI should guide the user through the whole flow

After `nika provider set openai` succeeds, the CLI should say:

```
  ✓ openai configured!

  What's next?
  ├── Test it:        nika infer "hello" -p openai
  ├── Set more keys:  nika provider set xai
  ├── Migrate env:    nika provider migrate
  ├── Check status:   nika provider list
  └── Start serving:  nika serve
```

After `nika provider migrate`:
```
  ✓ 1 migrated, 1 skipped, 5 not found

  Keys are now in the encrypted vault (~/.nika/secrets/vault.enc).
  You can remove them from your environment:
    unset OPENAI_API_KEY XAI_API_KEY

  Vault keys persist across reboots. Env vars don't.
```

After `nika serve` starts:
```
  Next: test from another terminal
    curl http://localhost:3000/health
    curl -X POST http://localhost:3000/v1/run \
      -H "Authorization: Bearer <token>" \
      -H "Content-Type: application/json" \
      -d '{"workflow":"hello.nika.yaml"}'
```

**Principle:** Every command output should end with "what to do next". The user should never be left staring at a prompt wondering what to type. The CLI is a guide, not a wall.

### 20. IMPROVEMENT: `nika provider set` should suggest `migrate` when env vars exist

```
$ nika provider set openai
  Paste your openai API key:
  ●●●●●●●●●●

  ✓ Stored in vault

  ⚠ OPENAI_API_KEY also found in environment (env var takes priority).
    To use the vault key, either:
      unset OPENAI_API_KEY     (this session)
      nika provider migrate    (moves env → vault, then unset)
```

### 21. IMPROVEMENT: `nika provider list` should show source clearly

```
  ✓ openai   [sk-proj-****]  (vault)     ← secure, persistent
  ✓ xai      [xai-KVIT****]  (env)       ← ⚠ lost on reboot, use `nika provider migrate`
```

The `(env)` label should include a warning that it's ephemeral.

## Inspiration

- **git** — "Did you mean?" suggestions
- **cargo** — Colored errors with `--` underlines pointing to the problem
- **npm** — "Did you mean one of these?" with fuzzy match
- **railway** — Beautiful CLI with progress spinners and structured output
- **Vercel CLI** — Clean status display with actionable next steps
- **Docker** — `docker info` shows full system state at a glance
