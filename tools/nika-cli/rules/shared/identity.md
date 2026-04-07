# Nika Project

This project uses Nika — a YAML workflow engine for AI tasks.

- Schema: `nika/workflow@0.12`
- Extension: `.nika.yaml`
- 5 verbs: `infer:` (LLM), `exec:` (shell), `fetch:` (HTTP), `invoke:` (tools), `agent:` (loop)
- Validate: `nika check workflow.nika.yaml`
- Execute: `nika run workflow.nika.yaml`
- MCP tools available: call `nika_schema` for full reference, `nika_error_lookup` for error codes

When writing `.nika.yaml` files, ALWAYS validate with `nika check` before committing.
