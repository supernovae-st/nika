# Nika v0.11.x Plan Index

> **For Claude:** Start with README.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [README.md](./README.md) | v0.11 overview | Starting v0.11 work |
| [6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md) | Complete design spec | Before ANY TUI work |

---

## Version Plans

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.11.0** | *To be created* | Explorer View (refactor Home) | 5 | 15 |
| **v0.11.1** | *To be created* | Editor View (refactor Studio) | 5 | 15 |
| **v0.11.2** | *To be created* | Runner View (new) | 5 | 15 |
| **v0.11.3** | *To be created* | Scheduler View (NEW) | 6 | 18 |
| **v0.11.4** | *To be created* | Settings View (NEW, 74% reuse) | 6 | 18 |
| **v0.11.5** | *To be created* | Navigation Update | 3 | 9 |

**Totals:** 30 tasks, 90 tests, ~5.5 sessions (~3 days)

---

## Implementation Order

```
v0.11.0 Explorer (refactor Home)
    │
    ├──▶ v0.11.1 Editor (refactor Studio) ─ parallel possible
    │
    └──▶ v0.11.2 Runner (new view)
              │
              ▼
         v0.11.3 Scheduler (NEW)
              │
              ▼
         v0.11.4 Settings (NEW, 74% Provider Modal reuse)
              │
              ▼
         v0.11.5 Navigation Update (TuiView enum → 6 variants)
```

---

## WIRING Checkpoints

Run after each version:

```bash
cargo test wiring_checkpoint_11_0  # After v0.11.0
cargo test wiring_checkpoint_11_1  # After v0.11.1
cargo test wiring_checkpoint_11_2  # After v0.11.2
cargo test wiring_checkpoint_11_3  # After v0.11.3
cargo test wiring_checkpoint_11_4  # After v0.11.4
cargo test wiring_checkpoint_11_5  # After v0.11.5 (full 6-view test)
```

---

## Provider Modal Component Reuse (v0.11.4)

The Settings view embeds 74% of existing Provider Modal components:

| Component | Location | Reusable |
|-----------|----------|----------|
| `ProviderCard` | `provider_modal/components/` | ✅ |
| `AnimatedHeader` | `provider_modal/components/` | ✅ |
| `VerificationEffect` | `provider_modal/components/` | ✅ |
| `DownloadGauge` | `provider_modal/components/` | ✅ |
| `CloudTab` | `provider_modal/tabs/` | ✅ |
| `OllamaTab` | `provider_modal/tabs/` | ✅ |
| `KeysTab` | `provider_modal/tabs/` | ✅ |
| `ConfigTab` | `provider_modal/tabs/` | ✅ |
| `handler.rs` | `provider_modal/` | ⚠️ Adapt |
| `state.rs` | `provider_modal/` | ⚠️ Adapt |

---

## Skills & Agents

| Version | Primary Skills | Agents |
|---------|---------------|--------|
| v0.11.0-2 | @rust-core, @frontend-design | rust-pro |
| v0.11.3 | @frontend-design, @test-driven-development | feature-dev:code-architect |
| v0.11.4 | @frontend-design, @test-driven-development | feature-dev:code-reviewer |
| v0.11.5 | @verification-before-completion | nika-deep-verify |

---

## New TuiView Enum (v0.11.5)

```rust
pub enum TuiView {
    Explorer,   // 1 / e (was Home)
    Chat,       // 2 / c
    Editor,     // 3 / d (was Studio)
    Runner,     // 4 / r (new, TaskBox-powered)
    Scheduler,  // 5 / s (NEW)
    Settings,   // 6 / , (NEW)
}
```
