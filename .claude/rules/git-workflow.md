# Git Workflow Rules

## 1 FIX = 1 COMMIT

Each logical change gets its own commit. No batching unrelated fixes.

## Commit Format

```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Types**: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
**Scopes**: Nika: `tui`, `ast`, `runtime`, `mcp`, `provider`, `dag`, `event` | NovaNet: `schema`, `mcp`, `tui`, `cli`, `db`

## Workflow

1. Mark todo in_progress
2. Make the fix
3. `git add <specific files>`
4. `git commit`
5. Mark todo completed
6. Repeat. Push when done.

## Batching Exceptions

OK when truly coupled: rename + usages, feature + tests, bugfix + regression test.

## Pre-Push

All must pass: `cargo check` (or `pnpm build`), `cargo test` (or `pnpm test`), no WIP commits, co-author lines.
