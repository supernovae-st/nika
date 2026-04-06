# nika keys — Session Handoff

> Copy-paste this into any new Claude session to continue implementation.

## Status

### DONE ✅
- [x] Design brainstorm (15+ agents, 13 design docs)
- [x] Naming research → `keys` validated (30/35 pts)
- [x] Display design → tree with categories, source provenance
- [x] UX helpers → 15 smart moments (did-you-mean, typo, alias, etc.)
- [x] Design system → `docs/design-system.md` (723 lines, icon registry)
- [x] P0-1 fix: custom vault keys injected at boot
- [x] P0-2 fix: $vault wired into RunContext
- [x] P0-3 fix: atomic vault writes
- [x] P0-4 fix: 192 "provider set" → "keys set" references updated
- [x] Implementation plan → `docs/plans/2026-04-05-nika-keys-mega-plan.md`

### IN PROGRESS 🔄
- [ ] Phase 1: keys.rs skeleton + clap (agent running)
- [ ] Phase 2: keys set with smart detection (agent running)
- [ ] Phase 3: keys list display (agent running)

### TODO 📋
- [ ] Phase 4: Remove `provider set/delete` from ProviderAction enum
- [ ] Phase 5: keys remove + keys check (latency bars)
- [ ] Phase 6: keys sync --github
- [ ] Phase 7: Wire nika setup to use keys
- [ ] Phase 8: E2E tests
- [ ] Phase 9: Update CLAUDE.md, AGENTS.md, docs

## Key Files

```
PLANS:
  docs/plans/2026-04-05-nika-keys-mega-plan.md     ← Master plan (17k)
  docs/plans/2026-04-05-nika-keys-implementation.md ← Original plan (16k)
  docs/design-system.md                             ← Icon/color/typography (723 lines)

DESIGN DOCS (in /tmp/nika-releases/):
  ux-keys-spec.md               ← Complete UX specification
  plan-keys-implementation.md   ← Rust architecture plan
  keys-display-mockups.md       ← 5 display mockups
  keys-onboarding-design.md     ← 5 onboarding scenarios
  keys-ultimate-design.md       ← 6 scenarios with colors
  keys-set-flow-design.md       ← 5 interactive flows
  keys-categories-taxonomy.md   ← 3 taxonomies → hybrid selected
  keys-wow-concepts.md          ← 3 radical concepts
  keys-edge-cases-ultrathink.md ← 10 edge cases analysis
  keys-env-integration-analysis.md ← $env resolution + 6 gaps
  keys-minimal-design.md        ← Radical simplification
  jobs-ive-review.md            ← Steve Jobs UX review
  design-system.md              ← Design system (copy)

CODE:
  tools/nika-cli/src/keys.rs        ← NEW main handler
  tools/nika-cli/src/provider.rs    ← STRIP set/delete (keep list/test)
  tools/nika/src/main.rs            ← Add Keys variant
  tools/nika-vault/src/lib.rs       ← Vault API (unchanged)
  tools/nika-core/src/catalogs/providers.rs ← Provider catalog (unchanged)
  tools/nika-display/src/           ← Display primitives (reuse)
```

## Design Decisions (FINAL)

### Commands
```
nika keys              ← bare = list (rich tree display)
nika keys set <name>   ← smart provider detection + cliclack
nika keys remove <name> ← delete from vault
nika keys check        ← test all keys with latency bars
nika keys sync         ← push to GitHub Actions
```

### Display — Categories (hidden if empty)
```
🧠 INFERENCE    — LLM providers (anthropic, openai, groq, gemini, etc.)
🔍 SEARCH       — web discovery (perplexity, firecrawl, ahrefs, dataforseo)
🔧 CUSTOM       — user secrets (ELEVENLABS, WEBHOOK_URL, etc.)
◎ LOCAL         — always available (mock, native)
```

### Icons (ZERO conflict with verb icons ✧⎈☄⊛❋)
```
●  configured (green)
·  not configured (dim)
◎  system/builtin (green)
○  offline/unloaded (dim)
⚠  env-only warning (yellow)
```

### Source Provenance
```
vault   → .green()    — persistent, encrypted
env     → .yellow()   — ephemeral, lost on reboot
daemon  → .cyan()     — runtime IPC cache
```

### v0 Philosophy
- `provider set/delete/get/migrate` → DELETED (not aliased)
- `vault set/get/delete/list` → DELETED
- Smart "did you mean?" error when old commands used
- NO backward compat, NO deprecation hints

### 15 Smart UX Moments
See mega plan for full table. Key ones:
- Typo detection (Levenshtein): `antrhopic` → `anthropic`
- Key-as-name: `sk-ant-...` → "That's a key → anthropic"
- Env var name: `ANTHROPIC_API_KEY` → "Did you mean? anthropic"
- No name: interactive picker (cliclack::select)
- Already exists: show current, ask to update
- Env exists: "Found in env. Save to vault?"

## Testing Strategy

### Unit Tests (per phase)
```bash
cargo test -p nika-cli --lib -- keys       # keys.rs tests
cargo test -p nika-vault --lib             # vault tests
cargo test --workspace --lib               # full workspace
```

### E2E Tests

#### Test 1: keys set + keys list roundtrip
```bash
# Set a mock key via stdin
echo "test-key-123" | nika keys set test_provider --stdin --no-test
# Verify it appears in list
nika keys --json | jq '.keys[] | select(.name == "test_provider")'
# Remove it
nika keys remove test_provider
# Verify it's gone
nika keys --json | jq '.keys | length'
```

#### Test 2: keys set with known provider
```bash
echo "sk-ant-test123" | nika keys set anthropic --stdin --no-test
nika keys --json | jq '.keys[] | select(.name == "anthropic") | .source'
# Should output: "vault"
nika keys remove anthropic
```

#### Test 3: keys set custom key
```bash
echo "my-secret-value" | nika keys set MY_CUSTOM --stdin
nika keys --json | jq '.keys[] | select(.name == "MY_CUSTOM")'
nika keys remove MY_CUSTOM
```

#### Test 4: env-only detection
```bash
export TEST_PROVIDER_KEY="from-env"
nika keys --json | jq '.keys[] | select(.source == "env")'
unset TEST_PROVIDER_KEY
```

#### Test 5: alias resolution
```bash
echo "sk-ant-test" | nika keys set claude --stdin --no-test
# Should store as "anthropic"
nika keys --json | jq '.keys[] | select(.name == "anthropic")'
nika keys remove anthropic
```

#### Test 6: empty state
```bash
# With no keys configured
nika keys 2>&1 | grep -q "Welcome"
```

#### Test 7: --json output schema
```bash
nika keys --json | python3 -c "
import json, sys
data = json.load(sys.stdin)
assert 'keys' in data
assert 'summary' in data
assert 'configured' in data['summary']
"
```

## Verification Checklist (before marking done)

- [ ] `cargo test --workspace --lib` passes (10,000+ tests)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `nika keys` shows tree display with categories
- [ ] `nika keys set anthropic` interactive flow works
- [ ] `nika keys set MY_CUSTOM` stores with custom: prefix
- [ ] `nika keys remove anthropic` deletes from vault
- [ ] `nika keys check` tests keys with latency bars
- [ ] `nika keys sync` pushes to GitHub via gh CLI
- [ ] `nika keys --json` outputs valid JSON
- [ ] `nika provider set` shows "Did you mean? nika keys set"
- [ ] Empty state shows welcome box
- [ ] Env-only warning shows for env-sourced keys
- [ ] Categories hidden when empty
- [ ] Problem summary shows what's missing
- [ ] All 15 UX helpers work
