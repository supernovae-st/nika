# Post-Launch Backlog — v0.75+

> 4 items found by the v0.75.0 10-agent audit that are not blocking launch but
> should be cleared in the post-launch grooming window. Each item is scoped,
> estimated, and has concrete file pointers.
> **Owner:** TBD · **Priority:** P3 (polish)

---

## Item 1 — LSP stubs: 64 `#[allow(dead_code)]` attributes

### Why it matters

The LSP crate carries intentional stubs for features that will be wired as the
IDE integration evolves. Each `#[allow(dead_code)]` is a promise the compiler
silently accepts. Too many promises = features nobody remembers to deliver.
After launch, we should either wire the stub or delete it.

### Concrete locations

| File | Lines | Reason the stub exists |
|---|---|---|
| `tools/nika-lsp/src/position.rs` | 142, 166, 175, 183, 192, 213, 219 | Position/range helpers not yet called |
| `tools/nika-lsp/src/document.rs` | 35, 111 | Document state introspection |
| `tools/nika-lsp/src/ast_integration.rs` | 20, 31, 61, 82, 206 | AST-based go-to-definition — not wired |
| `tools/nika-lsp/src/mcp_discovery.rs` | 167 | "Public API for future use" |
| `tools/nika-lsp/src/backend.rs` | 51, 66, 87 | Post-launch wiring/channel lifetime |
| `tools/nika-lsp/src/template_validation.rs` | 27 | Single stub field |
| `tools/nika-lsp/src/daemon_bridge.rs` | 26, 34, 40 | "Methods used incrementally as LSP features are wired up" |
| `tools/nika-media/src/types.rs` | 43, 53 | Used in tests only |
| `tools/nika-media/src/processor.rs` | 28, 37 | Used in tests only |
| `tools/nika-media/src/detect.rs` | 28 | "Extension variant reserved for future use" |
| `tools/nika-media/src/tools/error.rs` | 68 | Single error variant unused |
| `tools/nika-sdk/src/types.rs` | 165, 167, 213 | Public API placeholders |
| `tools/nika-engine/src/runtime/tests_e2e_workflow.rs` | 30 | Test helper |
| `tools/nika-engine/src/runtime/runner/mod.rs` | 249 | Single field |
| `tools/nika-engine/src/runtime/builtin/media/tests_*.rs` | various | Test fixtures |

### Suggested plan

1. Create a tracking issue: **"LSP stub cleanup sweep"**.
2. For each file, decide one of:
   - **Wire** the stub into a real code path and remove the attribute.
   - **Delete** the dead symbol entirely.
   - **Gate** it behind a `cfg(test)` or `#[cfg(feature = "...")]`.
3. Commit per file to keep the diff reviewable.
4. Exit criterion: `rg '#\[allow\(dead_code\)\]' tools/nika-lsp tools/nika-media`
   returns <10 hits total.

### Estimated effort

Medium. Each stub is small; the work is deciding intent. ~4-6 hours focused.

---

## Item 2 — TUI: 84 source files without `#[cfg(test)]` modules

### Why it matters

The TUI crate has **220 source files** and **136 test modules** — 84 files
without any test coverage at all. The untested files are concentrated in the
UI event path, which is exactly where regressions are most expensive because
they are visible to users and hard to reproduce.

### Highest-impact gaps

| Area | Files | Reason it's critical |
|---|---|---|
| `app/events.rs` | Event dispatch | Every keypress flows through here |
| `app/render.rs` | Frame render loop | Crashes freeze the TUI |
| `app/routing.rs` | View routing | Navigation bugs leak state |
| `app/commands.rs` | Command palette | User-facing command parsing |
| `app/lifecycle.rs` | App setup/teardown | Exit cleanup matters |
| `chat_agent/commands.rs` | Chat command parser | Agent loop entry point |
| `widgets/*` | ratatui widgets | Reusable components should be tested |

Full list via:

```bash
cd tools/nika-tui
rg -L '#\[cfg\(test\)\]' --type rust src
```

### Suggested plan

1. Start with **5 files** from the critical list above. Not all 84 — that's a
   false goal. Focus on the ones that cause user-visible bugs.
2. For each file, write 3-5 tests that exercise the public API and one
   error path.
3. Use `ratatui::backend::TestBackend` for rendering tests — it captures the
   frame buffer into a snapshot you can assert against.
4. Exit criterion: **coverage of the 7 critical areas** above. Leave the rest
   for organic growth.

### Estimated effort

Medium-large. ~8-12 hours for the critical path. Do NOT try to reach 100%
coverage — the long tail is waste.

---

## Item 3 — NIKA error codes: 43 undocumented, 4 dead in docs

### Why it matters

Error codes are the contract between Nika and its users. When a user hits
`NIKA-034` and googles it, we want them to find a doc page that explains what
happened and how to fix it. Right now, 43 codes are emitted by the code but
have no reference documentation, and 4 codes are documented but never emitted.

### Gaps found

**Undocumented codes actively used (43):**

- Provider & schema: NIKA-000, NIKA-010, NIKA-025, NIKA-034, NIKA-040
- With/binding: NIKA-070, NIKA-075, NIKA-083, NIKA-084
- Execution: NIKA-097, NIKA-098
- Agent/MCP: NIKA-110, NIKA-111, NIKA-114
- Resilience: NIKA-120, NIKA-122
- AST analysis: NIKA-152, NIKA-153, NIKA-155, NIKA-162, NIKA-163, NIKA-164, NIKA-167, NIKA-170
- File/tools: NIKA-211, NIKA-220, NIKA-230, NIKA-240
- Media: NIKA-257, NIKA-258, NIKA-259
- Course/misc: NIKA-319
- Record compression: NIKA-320-324 (has fix suggestions but not in reference doc)
- Unknown: NIKA-500-503, NIKA-999 (likely test placeholders — verify or delete)

**Dead documentation (remove or implement):**

- NIKA-125 `McpToolCallFailed` — listed in Resilience, never emitted
- NIKA-146 `InvalidVerb` — listed in AST Analysis, never used
- NIKA-147 `MissingAction` — listed in AST Analysis, never used
- NIKA-148 `InvalidField` — listed in AST Analysis, never used

### Files involved

- **Source of truth:** `tools/nika-core/src/error_codes.rs` — fix suggestion lookup (165 codes)
- **Error enum:** `tools/nika-engine/src/error.rs`
- **AST errors:** `tools/nika-core/src/ast/analyzer/errors.rs`
- **User reference:** `docs/content-suite/01-technical-bible/07-error-codes-reference.md`

### Suggested plan

1. **Single source of truth**: the `NikaError` enum in `tools/nika-engine/src/error.rs`.
2. Write a test: `error_codes_match_documentation()` that reads the reference
   doc, extracts all `NIKA-XXX` codes, and asserts bidirectional coverage with
   the enum. Run it in CI. Fail loudly on drift.
3. For each undocumented code, add a section to the reference doc with:
   - Code + short name
   - When it fires
   - How to fix it (copy from `error_codes.rs`)
   - An example workflow that triggers it
4. For the 4 dead codes, either delete them from the doc or implement them in
   the analyzer. Pick based on whether the case could actually happen.

### Estimated effort

Medium. ~6-8 hours. The documentation is formulaic once you have the pattern.
The value is the CI guard.

---

## Item 4 — Extract modes: 2 new modes undocumented in README

### Why it matters

The `fetch:` verb supports 11 extract modes (confirmed by `ExtractMode::ALL_NAMES`
in `tools/nika-core/src/ast/extract.rs`). The README and CLAUDE.md rule files
both document only 9. The two missing modes — `sitemap` and `metadata_links` —
exist in code, work in tests, and will silently confuse users who expect them
to be documented.

### Concrete fix

**Files to update:**

- `README.md` — around the "9 Extract Modes" mermaid block and the extract table
- `tools/nika-cli/rules/cursor.mdc` — "9 Extract Modes" table
- `/Users/thibaut/.claude/rules/nika.md` — same table

**New entries to add:**

```markdown
| `sitemap`        | sitemap.xml parsing (URL discovery)           | No       |
| `metadata_links` | Combined metadata + link classification       | No       |
```

Also update the header count from "9 Extract Modes" → "11 Extract Modes".

### Estimated effort

Small. ~30 minutes.

---

## Execution order (recommended)

```
Item 4  (30 min)   — quick win, unblocks marketing claims
Item 3  (6-8 h)    — error codes = user-visible contract
Item 1  (4-6 h)    — LSP stubs — mostly mechanical
Item 2  (8-12 h)   — TUI tests — most complex, most optional
```

Total: ~20-25 hours of focused work. Can be spread across 1-2 sprints.

## What is NOT in this backlog

- **Dead code warnings in nika-engine test files** — these are test fixtures,
  leave them alone.
- **100% test coverage** — coverage is not a goal, behavior verification is.
  Focus on the 7 critical TUI paths in Item 2, not all 84 files.
- **Full NIKA-500/501/502/503/999 investigation** — probably test placeholders.
  Check once, delete or document, move on.
- **Refactoring LSP ast_integration.rs** — the stubs exist because the feature
  is pending design. Do not refactor a placeholder.
