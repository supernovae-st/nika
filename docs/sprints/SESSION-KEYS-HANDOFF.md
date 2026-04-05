# nika keys — Mega Handoff

> Ce document est le handoff complet pour implémenter `nika keys`.
> Copy-paste dans une nouvelle session pour continuer.

## Contexte

On a brainstormé pendant une session entière (40+ agents) pour designer `nika keys` — la commande unifiée de gestion des clés API. Tout est décidé, validé, documenté. Il reste à implémenter.

## Décisions FINALES (non négociables)

### Nom : `keys`
Recherche sur 15+ CLI tools. Score 30/35. Précédent : Simon Willison's `llm keys set openai`. C'est ce que les gens DISENT : "t'as ta clé Anthropic ?"

### Commandes (5)
```
nika keys              ← bare = list (affichage riche, catégorisé)
nika keys set <name>   ← ajouter (smart provider detection + cliclack)
nika keys remove <name> ← supprimer du vault
nika keys check        ← tester toutes les clés avec barres latence
nika keys sync         ← push vers GitHub Actions via gh CLI
```

### Commandes SUPPRIMÉES (v0 = zéro backward compat)
```
nika provider set      → SUPPRIMÉ (nika keys set)
nika provider delete   → SUPPRIMÉ (nika keys remove)
nika provider get      → SUPPRIMÉ (nika keys montre tout)
nika provider migrate  → SUPPRIMÉ (absorbé par nika setup)
nika vault set         → SUPPRIMÉ
nika vault get         → SUPPRIMÉ
nika vault delete      → SUPPRIMÉ
nika vault list        → SUPPRIMÉ
```
PAS d'alias. PAS de deprecation hints. MAIS : "Did you mean?" intelligent quand l'user tape une ancienne commande.

### Commandes qui RESTENT (read-only)
```
nika provider list     ← catalogue read-only (modèles, pricing)
nika provider test     ← tester une connexion
nika provider recommend ← suggestion de modèle
```

### Catégories d'affichage (4, cachées si vides)
```
🧠 INFERENCE    — LLM providers (anthropic, openai, groq, gemini, mistral, deepseek, xai, openrouter, together, fireworks, cerebras, sambanova, cohere, ai21)
🔍 SEARCH       — web discovery (perplexity, firecrawl, ahrefs, dataforseo)
🔧 CUSTOM       — secrets utilisateur (ELEVENLABS, WEBHOOK_URL, etc.)
◎ LOCAL         — toujours disponible (mock, native)
```

### Icons clés (ZÉRO conflit avec les verb icons ✧⎈☄⊛❋)
```
●  (U+25CF) configured     .green()
·  (U+00B7) not configured .dimmed()
◎  (U+25CE) system/builtin .green()
○  (U+25CB) offline        .dimmed()
⚠  warning env-only        .yellow()
```

### Source provenance (colonne killer — unique dans l'écosystème)
```
vault   → .green()    persistent, chiffré XChaCha20
env     → .yellow()   éphémère, perdu au reboot
daemon  → .cyan()     cache IPC runtime
```

### Couleurs (Rule of 4)
```
green   → configuré, vault, succès
yellow  → env-only warning, attention
red     → erreur, échec test
dim     → structure (┈┈┈, hints, métadonnées)
bold    → titres, noms de providers
cyan    → headers de section, daemon
```

## 15 Smart UX Moments

| # | L'user tape | Nika répond |
|---|-------------|-------------|
| 1 | `nika provider set X` | ✗ Did you mean? `nika keys set X` |
| 2 | `nika vault set X` | ✗ Did you mean? `nika keys set X` |
| 3 | `nika keys set claude` | ✓ Auto-resolve alias: claude → anthropic |
| 4 | `nika keys set antrhopic` | 💡 Did you mean? anthropic (Levenshtein) |
| 5 | `nika keys set ANTHROPIC_API_KEY` | 💡 Did you mean? `nika keys set anthropic` |
| 6 | `nika keys set sk-ant-abc123` | 💡 That's a key, not a name → anthropic |
| 7 | `nika keys set` (no name) | 📋 Interactive picker (cliclack::select) |
| 8 | `nika keys set anthropic` (exists) | ⚠ Update? Current: sk-ant-••••7f2k |
| 9 | `nika keys set openai` (env exists) | 💡 Found in env. Save to vault? (Y/n) |
| 10 | `nika keys remove` (no name) | 📋 Pick from configured keys |
| 11 | `nika keys check` (zero keys) | 💡 No keys. Run: nika keys set anthropic |
| 12 | `nika keys sync` (no git remote) | 💡 Not in a git repo. Use --repo owner/name |
| 13 | `nika keys sync` (no gh CLI) | 💡 Install: brew install gh |
| 14 | `nika run` (missing key) | 💡 Fix command + alternatives configurées |
| 15 | Wrong prefix (sk- for anthropic) | 💡 Looks like OpenAI → nika keys set openai |

## Affichage EXACT — 7 écrans

### Écran 1 : `nika keys` (power user, 8 clés)
```
  🔑 Keys                                           8 configured

  🧠 INFERENCE                                — LLM providers
  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
  ● anthropic      sk-ant-••••7f2k     vault    Sonnet 4, Haiku
  ● openai         sk-••••a3b9         env      GPT-4.1, o4-mini
                                        ⚠ env only — nika keys set openai
  ● groq           gsk_••••mN3p        vault    Llama 3.3-70b
  ● gemini         AIza••••wQ7x        vault    Gemini 2.5 Pro, Flash
  · deepseek                                     nika keys set deepseek
  · xai                                          nika keys set xai
  ● cerebras       csk-••••            vault    Llama 70B (fastest)
  ● openrouter     sk-or-••••          vault    Any model via gateway

  🔍 SEARCH                               — web discovery
  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
  ● perplexity     pplx-••••           vault
  ● firecrawl      fc-••••             vault

  🔧 CUSTOM                               — your secrets
  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
  ● ELEVENLABS     ••••••••            vault

  ◎ LOCAL                                  — always available
  ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
  ◎ mock           no key needed                deterministic
  ○ native         no model loaded              nika model pull

  · 2 not set: deepseek, xai  ⚠ 1 env-only: openai
  💡 nika keys set ‹name›  ·  nika keys check  ·  nika keys sync
```

### Écran 2 : `nika keys` (zero clés — onboarding)
```
  🔑 Keys

  ╭─────────────────────────────────────────────────────────╮
  │                                                         │
  │  Welcome! Nika needs API keys to call LLM providers.   │
  │                                                         │
  │  Get started in 30 seconds:                            │
  │                                                         │
  │  1. nika keys set anthropic   Best quality              │
  │     → https://console.anthropic.com/settings/keys       │
  │                                                         │
  │  2. nika keys set groq        Free tier, no card        │
  │     → https://console.groq.com/keys                     │
  │                                                         │
  │  You can also run workflows right now:                  │
  │  nika run hello.nika.yaml --provider mock               │
  │                                                         │
  ╰─────────────────────────────────────────────────────────╯

  ◎ mock     always available — deterministic test responses
  ○ native   local models — nika model pull ‹name›

  💡 nika setup   Full interactive wizard
```

### Écran 3 : `nika keys set anthropic`
```
  ◆ nika keys set

  ┌  anthropic — Anthropic Claude
  │
  │  Get your key at:
  │  https://console.anthropic.com/settings/keys
  │
  ◇  ANTHROPIC_API_KEY:
  │  ••••••••••••••••••••••••••
  │
  │  ✓ Format valid (sk-ant-...)
  │  ✓ Encrypted and saved to vault
  │  ✓ Connected — 247ms
  │
  │  Models now available:
  │  · claude-sonnet-4-6       $3/$15 per 1M tokens
  │  · claude-haiku-4-5        $0.80/$4 per 1M tokens
  │  · claude-opus-4-6         $15/$75 per 1M tokens
  │
  ◇  Sync to GitHub CI? (Y/n)
  │  Yes
  │  ✓ ANTHROPIC_API_KEY → supernovae-st/nika
  │
  └  ✓ anthropic configured

  💡 Next: nika run hello.nika.yaml
```

### Écran 4 : `nika keys set MY_CUSTOM` (custom)
```
  ◆ nika keys set

  ┌  MY_CUSTOM — custom secret
  │
  ◇  Value:
  │  ••••••••••••••••
  │
  │  ✓ Encrypted and saved to vault
  │
  └  ✓ MY_CUSTOM configured

  💡 Use in workflows: $env.MY_CUSTOM
```

### Écran 5 : `nika keys check`
```
  🔑 Keys Check                                  testing 8 keys

  ● anthropic      ████████████████░░░░  247ms   Sonnet 4 ✓
  ● openai         ██████████████████░░  312ms   GPT-4.1 ✓
  ● groq           ████████████████████   89ms   Llama 3.3 ✓  ⚡
  ● gemini         ███████████████░░░░░  189ms   Gemini 2.5 ✓
  ● cerebras       ████████████████████   52ms   Llama 70B ✓  ⚡
  ● openrouter     ████████████████░░░░  203ms   gateway ✓
  ● perplexity     █████████████████░░░  267ms   search ✓
  ● firecrawl      ██████████████████░░  156ms   scrape ✓

  8/8 passed · avg 189ms · fastest: cerebras 52ms
```

### Écran 6 : `nika keys sync`
```
  🔑 Sync to GitHub                   supernovae-st/nika

  Preview:
  + ANTHROPIC_API_KEY                  new
  + OPENAI_API_KEY                     new
  = GEMINI_API_KEY                     already synced
  + ELEVENLABS_API_KEY                 new

  Push 3 keys? (Y/n) y

  ✓ ANTHROPIC_API_KEY     pushed
  ✓ OPENAI_API_KEY        pushed
  ✓ ELEVENLABS_API_KEY    pushed

  3 pushed · 1 already synced
```

### Écran 7 : Erreur workflow, clé manquante
```
  $ nika run translate.nika.yaml

  ✗ NIKA-032: Provider 'mistral' not configured

    This workflow needs provider: mistral but no API key is set.

    Fix: nika keys set mistral
         → https://console.mistral.ai/api-keys

    Configured providers you can use instead:
    ● anthropic  ● openai  ● groq
```

## État du code

### FAIT ✅
- `tools/nika-cli/src/keys.rs` — 1,102 lignes, 15 tests passent
  - Data types: KeyCategory, KeySource, KeyStatus, ResolvedKey
  - Category mapping: categorize_provider()
  - Key resolution: resolve_provider_key(), resolve_custom_keys(), gather_all_keys()
  - Display: render_keys_list() avec tree, icons, source provenance
  - Empty state: welcome box avec rounded corners
  - JSON: build_json_output() pour --json
  - Masking: mask_key_pretty()
- `tools/nika-cli/src/lib.rs` — `pub mod keys;` ajouté
- P0 bugs fixés : vault RunContext, custom keys boot, atomic writes
- 192 références "provider set" → "keys set" mises à jour

### À FAIRE 📋
1. **Wire keys.rs into main.rs** — ajouter `Keys` variant au `Commands` enum, dispatch vers `handle_keys_command`
2. **Implement `keys set`** — smart detection, cliclack flow, console URLs, auto-test, sync offer, 15 UX helpers
3. **Remove `provider set/delete`** de `ProviderAction` enum — garder list/test/recommend
4. **Implement `keys remove`** — delete vault + daemon + in-process SecretStore
5. **Implement `keys check`** — test all keys, latency bars (████░░), ⚡ fastest, avg summary
6. **Implement `keys sync`** — detect repo, preview diff (+/=), push via `gh secret set` stdin
7. **Wire `nika setup`** — wizard appelle keys set, propose sync en fin
8. **"Did you mean?" errors** — quand user tape `provider set` ou `vault set`
9. **Update docs** — CLAUDE.md, AGENTS.md, README

### BLOCKER CONNU ⚠
Il y a des changements WIP `on_error` dans le working tree (d'une autre session) qui cassent clippy. Ils sont stashés dans `git stash list`. Il faut soit les finir soit les abandonner avant de commiter.

## Fichiers clés

```
IMPLEMENTATION:
  tools/nika-cli/src/keys.rs         ← NOUVEAU, 1102 lignes (Phase 3 done)
  tools/nika-cli/src/lib.rs          ← mod keys ajouté
  tools/nika/src/main.rs             ← TODO: ajouter Keys variant
  tools/nika-cli/src/provider.rs     ← TODO: supprimer set/delete
  tools/nika-vault/src/lib.rs        ← API vault (inchangé, P0 fixé)
  tools/nika-core/src/catalogs/providers.rs ← 27 providers (inchangé)
  tools/nika-display/src/            ← primitives display (réutiliser)

PLANS:
  docs/plans/2026-04-05-nika-keys-mega-plan.md          ← plan principal
  docs/plans/2026-04-05-nika-keys-session-handoff.md    ← checklist
  docs/design-system.md                                  ← icons/colors/typo

DESIGN (en /tmp/, volatile — les décisions sont dans CE document):
  /tmp/nika-releases/ux-keys-spec.md
  /tmp/nika-releases/keys-display-mockups.md
  /tmp/nika-releases/keys-ultimate-design.md
  /tmp/nika-releases/keys-set-flow-design.md
  /tmp/nika-releases/keys-categories-taxonomy.md
  /tmp/nika-releases/keys-edge-cases-ultrathink.md
  /tmp/nika-releases/keys-env-integration-analysis.md
  /tmp/nika-releases/jobs-ive-review.md
```

## Console URL Registry (pour keys set)
```
anthropic  → https://console.anthropic.com/settings/keys
openai     → https://platform.openai.com/api-keys
gemini     → https://aistudio.google.com/apikey
groq       → https://console.groq.com/keys
mistral    → https://console.mistral.ai/api-keys
deepseek   → https://platform.deepseek.com/api_keys
xai        → https://console.x.ai/
openrouter → https://openrouter.ai/settings/keys
together   → https://api.together.xyz/settings/api-keys
fireworks  → https://fireworks.ai/api-keys
cerebras   → https://cloud.cerebras.ai/platform
cohere     → https://dashboard.cohere.com/api-keys
ai21       → https://studio.ai21.com/account/api-key
sambanova  → https://cloud.sambanova.ai/apis
```

## Tests E2E à écrire

```bash
# Test 1: set + list roundtrip
echo "test-key-123" | nika keys set test_provider --stdin --no-test
nika keys --json | jq '.keys[] | select(.name == "test_provider")'
nika keys remove test_provider

# Test 2: known provider
echo "sk-ant-test123" | nika keys set anthropic --stdin --no-test
nika keys --json | jq '.keys[] | select(.name == "anthropic") | .source'
nika keys remove anthropic

# Test 3: custom key
echo "my-secret" | nika keys set MY_CUSTOM --stdin
nika keys --json | jq '.keys[] | select(.name == "MY_CUSTOM")'
nika keys remove MY_CUSTOM

# Test 4: alias resolution
echo "sk-ant-test" | nika keys set claude --stdin --no-test
nika keys --json | jq '.keys[] | select(.name == "anthropic")'
nika keys remove anthropic

# Test 5: empty state
nika keys 2>&1 | grep -q "Welcome"

# Test 6: --json schema
nika keys --json | python3 -c "
import json, sys
data = json.load(sys.stdin)
assert 'keys' in data
assert 'summary' in data
"

# Test 7: did-you-mean
nika provider set anthropic 2>&1 | grep -q "Did you mean"
```

## Vérification finale

- [ ] `cargo test --workspace --lib` passe
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `nika keys` affiche le tree catégorisé
- [ ] `nika keys set anthropic` flow interactif marche
- [ ] `nika keys set MY_CUSTOM` stocke avec prefix custom:
- [ ] `nika keys remove` supprime du vault
- [ ] `nika keys check` teste avec barres latence
- [ ] `nika keys sync` push vers GitHub
- [ ] `nika keys --json` output JSON valide
- [ ] `nika provider set` → "Did you mean? nika keys set"
- [ ] Empty state affiche welcome box
- [ ] ⚠ env-only warning visible
- [ ] Catégories cachées quand vides
- [ ] Résumé problèmes en bas
- [ ] Les 15 UX helpers marchent
