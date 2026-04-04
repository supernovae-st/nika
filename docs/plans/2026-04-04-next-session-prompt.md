# Next Session Prompt — Media Security Hardening v0.68.1

> Copy-paste this as the first message of the next Claude session.

---

## Prompt

```
Lis docs/plans/2026-04-04-media-handoff.md en entier. C'est ton plan.

Context : v0.68.1 — nika:decode déjà implémenté (62 builtins). 3 agents
(rust-security + rust-pro + Explore) ont audité le media pipeline.
Findings : 1 HIGH + 6 MEDIUM.

EXÉCUTE DANS L'ORDRE, TDD strict, 1 fix = 1 commit.

═══ SPRINT SÉCURITÉ (45 min) ═══

Fix 1 — HIGH: import.rs path confinement
  File: tools/nika-media/src/tools/import.rs:24-35
  Bug: nika:import can read ~/.ssh/id_rsa, ~/.aws/credentials.
       SENSITIVE_PREFIXES blocklist is incomplete (inherently).
  Fix: Validate canonical path falls within project working directory.
       Pass project_root via MediaToolContext or fallback to cwd.
  TDD:
    RED:  test import_rejects_home_ssh_key → assert error
    RED:  test import_rejects_aws_credentials → assert error
    GREEN: add cwd confinement check after canonicalize()
    RED:  test import_allows_file_in_project_dir → assert OK
    GREEN: ensure project files still work
  Commit: fix(media): confine nika:import to project working directory

Fix 2 — MEDIUM: pipeline step limit
  File: tools/nika-media/src/tools/pipeline.rs:69-76
  Bug: No max step count. 1000 steps = CPU exhaustion.
  Fix: Add MAX_PIPELINE_STEPS = 50, reject with error.
  TDD:
    RED:  test pipeline_rejects_51_steps → assert error
    GREEN: add len check at top of execute()
    RED:  test pipeline_accepts_50_steps → assert OK
    GREEN: boundary correct
  Commit: fix(media): limit pipeline to 50 steps — prevent CPU exhaustion

Fix 3 — MEDIUM: pipeline budget bypass
  File: tools/nika-media/src/tools/pipeline.rs:272-282
  Bug: Returns Binary result without budget check.
  Fix: Call ctx.budget.check_and_add(data.len()) before returning.
  TDD:
    RED:  test pipeline_output_charges_budget → measure budget before/after
    GREEN: add budget charge
  Commit: fix(media): charge media budget for pipeline Binary output

Fix 4 — MEDIUM: svg_render div-by-zero
  File: tools/nika-media/src/tools/svg.rs:102-107
  Bug: viewBox 0x0 → division by zero in ratio calc.
  Fix: Guard svg_size.width() == 0.0 before ratio calculation.
  TDD:
    RED:  test svg_zero_viewbox → assert error (not panic!)
    GREEN: add zero guard
  Commit: fix(media): guard svg_render against zero-size viewBox

Fix 5 — MEDIUM: pdf_extract unbounded threads
  File: tools/nika-media/src/tools/pdf.rs:75-78
  Bug: Each call spawns OS thread. 100 PDFs = 100 threads.
  Fix: Use tokio::sync::Semaphore(4) or rayon ComputePool.
  TDD:
    RED:  test pdf_concurrent_limit → spawn 10, assert max 4 concurrent
    GREEN: add semaphore
  Commit: fix(media): limit pdf_extract to 4 concurrent threads

═══ SPRINT SHOWCASE (30 min) ═══

Fix 6 — Create showcase workflows
  File: examples/showcase/media/image-generation-url.nika.yaml
    Pattern: fetch DALL-E → extract URL → fetch binary → thumbnail → artifact
    Use provider: mock for validation
  File: examples/showcase/media/base64-decode.nika.yaml
    Pattern: nika:decode base64 → dimensions → thumbnail → artifact
    Use provider: mock
  Validate: nika check on both files
  Commit: docs(showcase): add image generation + base64 decode examples

Fix 7 — DX update
  Update AGENTS.md workspace structure if needed
  Update nika-bugs-and-patterns.md with base64→CAS pattern
  Commit: docs(dx): add nika:decode base64→CAS workflow pattern

═══ RULES ═══

Skills (MANDATORY — use Skill tool before each):
  test-driven-development    → EVERY fix follows RED-GREEN-REFACTOR
  verification-before-completion → cargo test + clippy before EVERY commit
  systematic-debugging       → if any test breaks unexpectedly
  rust                       → all Rust code
  requesting-code-review     → after Sprint Sécurité (before showcases)

Agents:
  rust-security → verify security fixes after Sprint Sécurité
  code-reviewer → final review before push

Methodology:
  1. Read the target file FIRST (never edit blind)
  2. Write the failing test FIRST (TDD RED)
  3. Write minimal code to pass (TDD GREEN)
  4. Refactor if needed
  5. cargo test -p <crate> --lib -- <test_name>  (per-test verify)
  6. cargo test --workspace --lib --exclude nika-py  (full suite)
  7. cargo clippy --all-targets --all-features -- -D warnings
  8. git add <specific files> && git commit (1 fix = 1 commit)

Commit format:
  type(scope): description
  Co-Authored-By: Claude <noreply@anthropic.com>
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

Testing:
  cargo test --workspace --lib --exclude nika-py  (always --lib, no keychain)
  cargo clippy --all-targets --all-features -- -D warnings

Push: HTTPS pattern
  git remote set-url origin https://github.com/supernovae-st/nika.git
  git push
  git remote set-url origin git@github.com:supernovae-st/nika.git

NE PAS: new verbs, new features, TUI changes, serve changes, Egghead,
         new feature flags, changes to CAS format, provider changes.
         This is security hardening + DX only.
```

---

## Why This Prompt Works

1. **Exact file:line for each fix** — no exploration needed
2. **TDD sequence per fix** — RED/GREEN explicitly stated
3. **Skills mandated** — not optional, Claude must invoke Skill tool
4. **1 fix = 1 commit** — atomic, reviewable
5. **Security first, features second** — Sprint order reflects priority
6. **Verification gate** — code-reviewer agent between sprints
7. **Copy-paste ready** — the prompt block above is the complete instruction
