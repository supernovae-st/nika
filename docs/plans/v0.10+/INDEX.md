# Nika v0.10+ Plans Index

**Target Versions:** v0.10.0 → v0.12.0
**Status:** Future (after v0.9)

---

## Version Roadmap

| Version | Milestone | Key Features |
|---------|-----------|--------------|
| **v0.10.0** | Explorer + Editor | 6-view navigation, Explorer redesign, Editor DAG sync |
| **v0.10.1** | Chat-as-DAG Polish | Live DAG, YAML preview, @mentions, // fork syntax |
| **v0.11.0** | Runner + Scheduler | Animated execution, cron management, timeline view |
| **v0.11.1** | Provider Modal v2 | Tabbed providers, Ollama client, keyring integration |
| **v0.12.0** | Polish + Performance | NovaNet tree effects, minimap, 60fps animations |

---

## Plans

| File | Target | Description |
|------|--------|-------------|
| `v010-v012-6-views-design.md` | v0.10-v0.12 | 6-view TUI architecture design |
| `provider-modal-v2.md` | v0.11.1 | Provider modal redesign spec |
| `provider-modal-v2-implementation.md` | v0.11.1 | Implementation details |
| `provider-modal-v085-to-v090.md` | v0.11.x | Migration path |

---

## 6 Views Architecture

```
[1] EXPLORER → [2] CHAT → [3] EDITOR → [4] RUNNER → [5] SCHEDULER → [6] SETTINGS
     📁           💬          ✏️           ▶️            📅             ⚙️
   DEFAULT      Tab →       Tab →        Tab →        Tab →          Tab →
```

---

## Dependencies

v0.10+ requires v0.9 to be complete:
- Chat-as-DAG infrastructure
- Boot sequence
- Context/Agent/Skill systems
- New YAML files
