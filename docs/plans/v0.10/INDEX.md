# Nika v0.10.x Plan Index

> **For Claude:** Start with README.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [README.md](./README.md) | v0.10 overview | Starting v0.10 work |
| [v0.10.3-ChatDagPanel.md](./v0.10.3-ChatDagPanel.md) | DAG panel integration | After widgets complete |
| [v0.10.4-AnimationPolish.md](./v0.10.4-AnimationPolish.md) | Effects & polish | Final v0.10 step |
| [archive/](./archive/) | Detailed widget specs | Reference during implementation |

---

## Version Plans

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.10.0** | [v0.10.0-TaskBoxCore.md](./v0.10.0-TaskBoxCore.md) | NodeBox Widget | 5 | 20 |
| **v0.10.1** | [v0.10.1-EdgeLine.md](./v0.10.1-EdgeLine.md) | EdgeLine Widget | 4 | 15 |
| **v0.10.2** | [v0.10.2-TaskQueue.md](./v0.10.2-TaskQueue.md) | TaskQueue Widget | 5 | 15 |
| **v0.10.3** | [v0.10.3-ChatDagPanel.md](./v0.10.3-ChatDagPanel.md) | ChatDagPanel Integration | 5 | 20 |
| **v0.10.4** | [v0.10.4-AnimationPolish.md](./v0.10.4-AnimationPolish.md) | Animation Polish | 4 | 16 |

**Totals:** 23 tasks, 86 tests, ~4 sessions (~2 days)

---

## Implementation Order

```
v0.10.0 NodeBox
    │
    ├──▶ v0.10.1 EdgeLine (parallel possible)
    │
    ├──▶ v0.10.2 TaskQueue (parallel possible)
    │
    └──▶ v0.10.3 ChatDagPanel (requires 0.10.0-2)
              │
              ▼
         v0.10.4 Animation Polish
```

---

## WIRING Checkpoints

Run after each version:

```bash
cargo test wiring_checkpoint_10_0  # After v0.10.0
cargo test wiring_checkpoint_10_1  # After v0.10.1
cargo test wiring_checkpoint_10_2  # After v0.10.2
cargo test wiring_checkpoint_10_3  # After v0.10.3
cargo test wiring_checkpoint_10_4  # After v0.10.4
```

---

## Archived Specs

The `archive/` folder contains detailed widget specifications from the v0.9.x planning phase.
These provide implementation guidance:

| File | Purpose | Lines |
|------|---------|-------|
| v0.9.4a-TaskBoxFoundation.md | Shared infrastructure, StreamChunk integration | ~2,300 |
| v0.9.4b-InferBox.md | Infer verb visualization | ~1,500 |
| v0.9.4c-ExecBox.md | Exec verb visualization | ~1,200 |
| v0.9.4d-FetchBox.md | Fetch verb visualization | ~1,000 |
| v0.9.4e-InvokeBox.md | Invoke verb visualization | ~1,400 |
| v0.9.4f-AgentBox.md | Agent verb visualization | ~2,000 |

---

## Skills & Agents

| Version | Primary Skills | Agents |
|---------|---------------|--------|
| v0.10.0-2 | @rust-core, @frontend-design | rust-pro |
| v0.10.3 | @frontend-design, @test-driven-development | feature-dev:code-reviewer |
| v0.10.4 | @verification-before-completion | nika-deep-verify |
