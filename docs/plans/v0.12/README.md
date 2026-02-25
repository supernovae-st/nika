# Nika v0.12 "Providers" — Complete Provider Modal Wiring

> **For Claude:** Read this overview FIRST, then INDEX.md for navigation to specific plans.

> **NO v1.0** — Nika stays in 0.XX versioning. After v0.12.3, continue to v0.13.0.

---

## Vision

**v0.12 "Providers"** completes the Provider Modal wiring. Key insight: **95% of code exists, this is a WIRING release.**

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.12 "Providers" — Wire the Missing Links                                   ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Component              Code Exists    Missing                                ║
║  ─────────────────────  ─────────────  ────────────────────────               ║
║  NikaKeyring            100%           app.rs handler ⚡                       ║
║  ModalAction variants   100%           app.rs match arms ⚡                    ║
║  OllamaClient           100%           loader command wiring ⚡                ║
║  Keys tab UI            95%            detect_state keyring check             ║
║                                                                               ║
║  ⚡ = Simple wiring, no new code needed                                       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Parallel Execution Note

**v0.12 can run PARALLEL with v0.10** after v0.9 completes.

```
v0.9.3 (BuiltinTools)
    │
    ├──▶ v0.10.x (TaskBox) ─────────────────────▶ v0.11.x (Six Views)
    │
    └──▶ v0.12.x (Providers) ─ PARALLEL TRACK ──▶ Merges before v0.11
```

---

## Key Insights (Agent Exploration 2026-02-25)

**NikaKeyring (`keyring.rs`):** Fully implemented (220 lines, 22 tests)
- `get()`, `set()`, `delete()`, `exists()`, `get_masked()` all exist
- `validate_key_format()` validates provider-specific key formats
- `provider_env_var()` maps provider to env var name

**Missing Wiring Identified:**
1. `ModalAction::SaveAndTestApiKey` does NOT call `NikaKeyring::set()` — it just emits the action
2. `ModalAction::PullModel/DeleteModel` NOT processed anywhere
3. `detect_state()` in keys.rs reads env vars but NOT keyring

---

## Dependencies

```
v0.9.3 (BuiltinTools)
    │
    ▼
v0.12.0 (KeyringWiring) ──▶ v0.12.1 (EnvMigration)
    │
    ├──▶ v0.12.2 (ProviderAutoSelect)
    │
    └──▶ v0.12.3 (OllamaEnhancement)
```

---

## Statistics

| Version | Focus | Tasks | Tests | Sessions |
|---------|-------|-------|-------|----------|
| v0.12.0 | Keyring Wiring | 4 | 12 | 0.5 |
| v0.12.1 | Env Migration | 4 | 12 | 0.5 |
| v0.12.2 | Provider Auto-Select | 3 | 9 | 0.25 |
| v0.12.3 | Ollama Enhancement | 4 | 12 | 0.25 |
| **Total** | | **15** | **45** | **1.5** |

---

## Key Files

| File | Purpose |
|------|---------|
| `src/tui/app.rs` | Main wiring point — process ModalAction variants |
| `src/tui/widgets/provider_modal/handler.rs` | Defines all ModalAction variants |
| `src/tui/widgets/provider_modal/keyring.rs` | NikaKeyring (ready to use) |
| `src/tui/widgets/provider_modal/tabs/keys.rs` | Keys tab with detect_state() |
| `src/tui/widgets/provider_modal/loader.rs` | Background loader (needs PullModel) |

---

## Related Documents

- [ROADMAP.md](../v0.9.1/ROADMAP.md) — Master roadmap (v0.9-v0.12)
- [v0.9.3-BuiltinTools.md](../v0.9.1/v0.9.3-BuiltinTools.md) — Prerequisite

---

## Success Criteria

- [ ] 45 new tests passing
- [ ] `SaveAndTestApiKey` calls `NikaKeyring::set()`
- [ ] `nika init --migrate-keys` migrates .env to keyring
- [ ] Enter key in CloudTab selects provider
- [ ] Ollama pull/delete handlers work
- [ ] `detect_state()` checks keyring BEFORE env vars
