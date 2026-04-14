# nika/docs/mintlify

Source of [docs.nika.sh](https://docs.nika.sh) — user documentation published via [Mintlify](https://mintlify.com).

## Structure

```
docs/mintlify/
├── docs.json                    Mintlify config + navigation
├── introduction.mdx             Landing page
├── getting-started/             Install + first workflow + editors
├── concepts/                    Organism / verbs / workflows / bindings / events / providers
├── guides/                      Agent loop / structured output / MCP / decompose / local
├── reference/                   CLI / YAML / errors / schema / ndjson
├── examples/                    Curated examples from registry
└── images/                      Logo + favicon
```

## Local preview

```bash
cd docs/mintlify
npm install -g mintlify           # first time only
mintlify dev                      # http://localhost:3000
```

## Deploy

Auto-deployed by Mintlify on push to `main` (once the project is connected
via the Mintlify dashboard, monorepo mode path `/docs/mintlify`).

Custom domain: `docs.nika.sh` → CNAME to Mintlify target (configured in
Mintlify dashboard after OSS Program approval).

## Content conventions

- **Narrative vocabulary** per [NIKA_NARRATIVE_LOCKED](../../../../.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/NIKA_NARRATIVE_LOCKED.md)
  (private): "organ" not "module", "admitted" not "added", "grew" not "shipped",
  "chrysalis" not "beta", "emerge" reserved for v0.90.
- **Butterfly 🦋** scarcity: only in introduction.mdx closing, never in nav/chrome.
- **Headings**: sentence case, not title case.
- **Voice**: direct, technical, AGPL-proud, never try-hard.

## License

AGPL-3.0-or-later, same as the engine.
