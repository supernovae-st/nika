# Nika v0.11 "Six Views" — TUI Architecture Upgrade

> **For Claude:** Read this overview FIRST, then INDEX.md for navigation to specific plans.

> **NO v1.0** — Nika stays in 0.XX versioning. After v0.11.5, continue to v0.12.0.

---

## Vision

**v0.11 "Six Views"** upgrades the TUI from 4 views to 6 views with VS Code-inspired architecture.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.11 "Six Views" — VS Code-Inspired Architecture                            ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  CURRENT (4 Views)                    →    NEW (6 Views)                      ║
║  ──────────────────                        ─────────────                      ║
║  [h] Home (browse)                    →    [1/e] Explorer                     ║
║  [a] Chat (agent)                     →    [2/c] Chat                         ║
║  [s] Studio (editor)                  →    [3/d] Editor                       ║
║  [m] Monitor (execution)              →    [4/r] Runner                       ║
║                                            [5/s] Scheduler (NEW)              ║
║                                            [6/,] Settings (NEW)               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Key Insights (Agent Exploration 2026-02-25)

**Settings View:** 74% of Provider Modal components can be directly embedded:
- `ProviderCard`, `AnimatedHeader`, `VerificationEffect`
- `OllamaClient`, `DownloadGauge`
- `KeysTab` with keyring integration
- `ConfigTab` preferences

**Scheduler View:** New view for cron-based workflow automation.

---

## Dependencies

```
v0.10.4 (Animation Polish)
    │
    ▼
v0.11.0 (Explorer) ──┬──▶ v0.11.1 (Editor) ──┬──▶ v0.11.2 (Runner)
                     │                        │
                     └────────────────────────┘
                                │
                                ▼
                        v0.11.3 (Scheduler)
                                │
                                ▼
                        v0.11.4 (Settings)
                                │
                                ▼
                        v0.11.5 (Navigation Update)
```

---

## Statistics

| Version | Focus | Tasks | Tests | Sessions |
|---------|-------|-------|-------|----------|
| v0.11.0 | Explorer View (refactor Home) | 5 | 15 | 1 |
| v0.11.1 | Editor View (refactor Studio) | 5 | 15 | 1 |
| v0.11.2 | Runner View (new, uses TaskBox) | 5 | 15 | 1 |
| v0.11.3 | Scheduler View (NEW) | 6 | 18 | 1 |
| v0.11.4 | Settings View (NEW, 74% reuse) | 6 | 18 | 1 |
| v0.11.5 | Navigation Update (6 views) | 3 | 9 | 0.5 |
| **Total** | | **30** | **90** | **5.5** |

---

## View Architecture

| View | Hotkey | Panels | Purpose |
|------|--------|--------|---------|
| **Explorer** | `1` / `e` | 3 | File browser + DAG preview |
| **Chat** | `2` / `c` | 3-4 | Conversational agent + DAG panel |
| **Editor** | `3` / `d` | 3 | YAML editor with split-pane |
| **Runner** | `4` / `r` | 4 | Real-time execution + TaskBox widgets |
| **Scheduler** | `5` / `s` | 2 | Cron-based workflow automation |
| **Settings** | `6` / `,` | 6 | Configuration + Provider management |

---

## Key Files to Modify

| Module | Action | Purpose |
|--------|--------|---------|
| `src/tui/views/home.rs` | Rename | → `explorer.rs` |
| `src/tui/views/studio.rs` | Rename | → `editor.rs` |
| `src/tui/views/mod.rs` | Modify | Add Runner, Scheduler, Settings |
| `src/tui/views/runner.rs` | Create | New Runner view |
| `src/tui/views/scheduler.rs` | Create | New Scheduler view |
| `src/tui/views/settings.rs` | Create | New Settings view |

---

## Related Documents

- [ROADMAP.md](../v0.9.1/ROADMAP.md) — Master roadmap (v0.9-v0.12)
- [6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md) — Complete design spec
- [v0.10.4-AnimationPolish.md](../v0.10/v0.10.4-AnimationPolish.md) — Prerequisite

---

## Success Criteria

- [ ] 90 new tests passing
- [ ] 6 views navigable with Tab/Shift+Tab
- [ ] Hotkeys 1-6 switch views correctly
- [ ] Explorer shows file tree with DAG preview
- [ ] Scheduler displays cron schedules
- [ ] Settings embeds Provider Modal components (74% reuse verified)
