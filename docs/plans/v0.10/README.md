# Nika v0.10 "TaskBox" — TUI Widgets + DAG Panel

> **For Claude:** Read this overview FIRST, then INDEX.md for navigation to specific plans.

> **NO v1.0** — Nika stays in 0.XX versioning. After v0.10.4, continue to v0.11.0.

---

## Vision

**v0.10 "TaskBox"** implements TUI widgets for visualizing task execution in real-time.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.10 "TaskBox" — Bring the DAG to Life                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  NodeBox Widget   → Individual task visualization (verb-colored)              ║
║  EdgeLine Widget  → Dependency arrows with data flow indicators               ║
║  TaskQueue Widget → Pending/running/completed task list                       ║
║  ChatDagPanel     → Integrated DAG panel in Chat view (Ctrl+D toggle)         ║
║  Animation Polish → Pulse effects, edge flow, smooth transitions              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Dependencies

```
v0.9.3 (BuiltinTools)
    │
    ▼
v0.10.0 (NodeBox) ─────────────────────────┐
    │                                       │
v0.10.1 (EdgeLine)                          │
    │                                       │
v0.10.2 (TaskQueue)                         │
    │                                       │
    └──────────────────────────────────────▶│
                                            ▼
                                    v0.10.3 (ChatDagPanel)
                                            │
                                            ▼
                                    v0.10.4 (Animation Polish)
```

---

## Statistics

| Version | Focus | Tasks | Tests | Sessions |
|---------|-------|-------|-------|----------|
| v0.10.0 | NodeBox Widget | 5 | 15 | 1 |
| v0.10.1 | EdgeLine Widget | 4 | 12 | 0.5 |
| v0.10.2 | TaskQueue Widget | 4 | 12 | 0.5 |
| v0.10.3 | ChatDagPanel Integration | 5 | 20 | 1 |
| v0.10.4 | Animation Polish | 4 | 16 | 1 |
| **Total** | | **22** | **75** | **4** |

---

## Key Files to Create

| Module | Purpose |
|--------|---------|
| `src/tui/widgets/node_box.rs` | Task visualization box |
| `src/tui/widgets/edge_line.rs` | Dependency arrows |
| `src/tui/widgets/task_queue.rs` | Task list widget |
| `src/tui/widgets/chat_dag_panel.rs` | Integrated DAG panel |

---

## Archived Files

The `archive/` folder contains detailed TaskBox widget specs from the v0.9.x planning phase:

- `v0.9.4a-TaskBoxFoundation.md` — Shared infrastructure (StreamChunk integration)
- `v0.9.4b-InferBox.md` — Infer verb widget spec
- `v0.9.4c-ExecBox.md` — Exec verb widget spec
- `v0.9.4d-FetchBox.md` — Fetch verb widget spec
- `v0.9.4e-InvokeBox.md` — Invoke verb widget spec
- `v0.9.4f-AgentBox.md` — Agent verb widget spec

These specs contain detailed implementation guidance and can be referenced during development.

---

## Related Documents

- [ROADMAP.md](../v0.9.1/ROADMAP.md) — Master roadmap (v0.9-v0.12)
- [6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md) — TUI architecture
- [v0.9.3-BuiltinTools.md](../v0.9.1/v0.9.3-BuiltinTools.md) — Prerequisite

---

## Success Criteria

- [ ] 75 new tests passing
- [ ] NodeBox renders all 5 verb types with correct colors
- [ ] EdgeLine shows data flow direction
- [ ] TaskQueue displays pending/running/completed states
- [ ] ChatDagPanel toggles with Ctrl+D
- [ ] Animations run at 60fps without blocking
