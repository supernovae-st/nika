# Nika v0.12.x Plan Index

> **For Claude:** Start with README.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [README.md](./README.md) | v0.12 overview | Starting v0.12 work |
| Provider Modal source | Reference existing code | During implementation |

---

## Version Plans

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.12.0** | [v0.12.0-KeyringWiring.md](./v0.12.0-KeyringWiring.md) | Keyring Wiring | 4 | 12 |
| **v0.12.1** | [v0.12.1-EnvMigration.md](./v0.12.1-EnvMigration.md) | Env Migration | 4 | 12 |
| **v0.12.2** | [v0.12.2-ProviderAutoSelect.md](./v0.12.2-ProviderAutoSelect.md) | Provider Auto-Select | 3 | 9 |
| **v0.12.3** | [v0.12.3-OllamaEnhancement.md](./v0.12.3-OllamaEnhancement.md) | Ollama Enhancement | 4 | 12 |

**Totals:** 15 tasks, 45 tests, ~1.5 sessions (~1 day)

---

## Implementation Order

```
v0.12.0 KeyringWiring (wire SaveAndTestApiKey to NikaKeyring::set)
    │
    ├──▶ v0.12.1 EnvMigration (nika init --migrate-keys)
    │
    ├──▶ v0.12.2 ProviderAutoSelect (Enter key selects provider)
    │
    └──▶ v0.12.3 OllamaEnhancement (pull/delete handlers)
```

---

## WIRING Checkpoints

Run after each version:

```bash
cargo test wiring_checkpoint_12_0  # After v0.12.0
cargo test wiring_checkpoint_12_1  # After v0.12.1
cargo test wiring_checkpoint_12_2  # After v0.12.2
cargo test wiring_checkpoint_12_3  # After v0.12.3 (full providers test)
```

---

## Key Wiring Points

### v0.12.0 — app.rs Match Arms

```rust
// src/tui/app.rs - ADD these match arms
match modal_action {
    ModalAction::SaveAndTestApiKey { provider, key } => {
        // Wire to NikaKeyring::set()
        NikaKeyring::set(provider, &key)?;
        // Then verify
    }
    ModalAction::SelectProvider { provider, model } => {
        // Wire to active provider state
        self.active_provider = Some((provider.to_string(), model));
    }
    // ... other actions
}
```

### v0.12.1 — CLI Migration Command

```rust
// src/main.rs - ADD subcommand
Commands::Init { migrate_keys } => {
    if migrate_keys {
        migrate::migrate_env_to_keyring()?;
    }
}
```

### v0.12.2 — detect_state Priority

```rust
// src/tui/widgets/provider_modal/tabs/keys.rs
// CHANGE: Check keyring BEFORE env vars
pub fn detect_state(provider: &str) -> ApiKeyState {
    // 1. Check keyring first
    if NikaKeyring::exists(provider) {
        return ApiKeyState::Stored;
    }
    // 2. Fall back to env var
    if std::env::var(provider_env_var(provider)).is_ok() {
        return ApiKeyState::EnvVar;
    }
    ApiKeyState::Missing
}
```

---

## Skills & Agents

| Version | Primary Skills | Agents |
|---------|---------------|--------|
| v0.12.0 | @rust-core, @test-driven-development | rust-pro |
| v0.12.1 | @rust-core | rust-pro |
| v0.12.2 | @test-driven-development | feature-dev:code-reviewer |
| v0.12.3 | @verification-before-completion | nika-deep-verify |

---

## Parallel Execution

**v0.12 can run PARALLEL with v0.10** because:
- v0.10 focuses on TUI widgets (new code)
- v0.12 focuses on wiring (existing code)
- No file conflicts between tracks

Schedule:
```
Day 1-2: v0.10.0-v0.10.2 (widgets) + v0.12.0-v0.12.1 (wiring) in parallel
Day 3:   v0.10.3-v0.10.4 (DAG panel) + v0.12.2-v0.12.3 (providers) in parallel
Day 4+:  v0.11.x (Six Views) - requires both v0.10 and v0.12 complete
```
