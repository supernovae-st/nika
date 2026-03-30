# Custom Endpoints — Agent Session Prompt

Copy-paste this into a new Claude Code session opened at the Nika workspace root (`~/dev/supernovae/nika/tools/nika`).

---

## Prompt

```
I need you to implement the "Custom Endpoints" feature for Nika — support for OpenAI-compatible inference servers (vLLM, TGI, Ollama, LiteLLM, SGLang) via configurable base_url endpoints.

The complete implementation plan is at:
docs/plans/2026-03-27-custom-endpoints.md

Read the plan thoroughly before starting. It has 15 tasks across 7 phases, with exact file paths, code snippets, and test commands.

Key rules:
- Use the superpowers:executing-plans skill to work through the plan task-by-task
- TDD: write tests first, verify they fail, then implement
- One commit per task (format: type(scope): description + co-authors)
- Run `cargo test --workspace --lib` after every task (NEVER without --lib — keychain popups)
- Zero clippy warnings: `cargo clippy --workspace -- -D warnings`
- Errors use NikaError with NIKA-XXX codes, never anyhow
- AST flow: Raw → Analyzed → Lower. Never skip phases
- #[serde(default)] on new fields for backward compatibility

Architecture summary:
- rig-core 0.32 already has `openai::Client::from_url(key, url)` and reads OPENAI_BASE_URL
- New `OpenAiCompat` variant in RigProvider wraps an openai::Client pointed at a custom URL
- Named endpoints live in config.toml [endpoints.<name>], resolved at runtime
- Inline base_url on workflow/task creates transient (uncached) providers
- SSRF: localhost + private IPs allowed (use case), metadata IPs blocked (169.254.x.x)

Start by reading the plan, then execute phase by phase. Don't skip phases or batch tasks.
```
