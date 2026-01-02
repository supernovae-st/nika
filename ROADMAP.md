# Nika Roadmap

> **Native Infrastructure Kernel for Automation**
>
> This roadmap reflects our current plans and priorities. It may change as we learn from the community.

## Vision

Nika aims to be the **infrastructure layer for AI-powered automation** - a declarative, type-safe way to orchestrate LLM agents with deterministic workflows.

---

## v0.1.0 - Foundation (Current)

> Status: **In Development**

The MVP release establishing core architecture.

- [x] 5 semantic verbs (`agent:`, `exec:`, `fetch:`, `invoke:`, `infer:`)
- [x] 4 scope presets (`minimal`, `default`, `debug`, `full`)
- [x] DAG-based execution with fan-out/fan-in
- [x] 7-layer validation pipeline
- [x] Template system with `${task.output}` syntax
- [x] JSONPath support for structured outputs
- [x] Event sourcing for audit trail
- [x] Multi-provider support

---

## v0.2.0 - Developer Experience

> Status: **Planned**

Focus on making Nika delightful to use.

- [ ] TUI with real-time DAG visualization
- [ ] `nika init` - Interactive project scaffolding
- [ ] `nika validate` - Pre-flight validation with helpful errors
- [ ] `nika run --dry` - Dry run mode
- [ ] Better error messages with fix suggestions
- [ ] VS Code extension (syntax highlighting + validation)

---

## v0.3.0 - SHAKA Integration

> Status: **Planned**

Runtime intelligence layer.

- [ ] ShakaService as runtime sidecar
- [ ] Epistemic awareness (health, quality, evidence signals)
- [ ] Collapse risk detection
- [ ] L1/L2 live actions (retry, switch model, trim context)
- [ ] `shaka.report.json` generation
- [ ] `shaka.proposal.yaml` for improvement suggestions

---

## v0.4.0 - Ecosystem

> Status: **Future**

Growing the Nika ecosystem.

- [ ] MCP server integration
- [ ] Asset folders (`.nika/skills/`, `.nika/agents/`)
- [ ] Hooks system (`pre_task`, `post_task`, `on_error`)
- [ ] Guardrails for safety constraints
- [ ] Community workflow registry

---

## v1.0.0 - Production Ready

> Status: **Future**

Stable API, production hardened.

- [ ] Stable API (no breaking changes)
- [ ] Comprehensive documentation
- [ ] Performance benchmarks
- [ ] Security audit
- [ ] Enterprise features (SSO, audit logs, compliance)

---

## How to Contribute

1. **Feature requests**: Open an issue with the `enhancement` label
2. **Bug reports**: Use the bug report template
3. **Discussions**: Join our GitHub Discussions

## Feedback

This roadmap is shaped by community feedback. Star the repo and watch for updates!
