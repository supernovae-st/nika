# nika-exec-runner

**Production `TokioShell` — L1 implementation of the `nika-kernel` shell-exec traits.**

The only production site spawning PLAIN subprocesses (`tokio::process`) —
one deliberate second site exists: `nika-mcp`'s stdio MCP client (a
persistent pipe session the one-shot shell seam cannot express). Gated
safe-by-default by a command blocklist; tests inject a mock. Announce-floor
`exec` (slice s7).

```rust,no_run
use nika_exec_runner::TokioShell;
use nika_kernel::{ShellCommand, ShellRun};

# async fn example() -> Result<(), nika_kernel::ShellError> {
let shell = TokioShell::new();
let r = shell.run(ShellCommand::new("echo").arg("hi")).await?;   // blocklist-checked, then spawned
assert!(r.success());
println!("{}", r.stdout.trim());
# Ok(())
# }
```

## Security — safe by default

Workflows run attacker-influenced commands, so the MECHANISM is safe before
`nika-policy` (L1.5 · step 8) adds richer gating. Unless `command.pre_validated`:

- **`run()` checks the blocklist BEFORE spawning** — ~100 dangerous patterns
  (rm -rf /, fork bomb, reverse shells, priv-esc, interpreter `-c`, Windows
  destructive, …) matched after **NFKC + zero-width strip + whitespace collapse
  + quote dequoting + first-token basename + full-string scan** (six layers,
  each closing a documented bypass — `su""do`, `/usr/bin/sudo`, fullwidth `ｒｍ`,
  zero-width `r​m`, an 8000-char pad then `&& rm -rf /`).
- **`shell:true` additionally refuses expansion/substitution** — `$VAR`,
  `${IFS}`, `$(...)`, `$'\t'`, backticks. `sh -c` expands these *after* a
  template-only check (the `rm${IFS}-rf${IFS}/` → `rm -rf /` TOCTOU); a baseline
  executor can't predict the expansion, so it refuses. Plain pipes/redirects
  (`yes | head`, `echo x 1>&2`) are unaffected.

`pre_validated` is the seam for `nika-policy` to allow expansion after
intent-aware validation. Over-blocking a rare-but-safe command beats
under-blocking an attack (a few documented false positives — e.g. `printenv`
trips the `env ` wrapper pattern; route via `pre_validated`).

## Process safety (kernel CANCEL SAFETY contract)

| guarantee | how |
|---|---|
| no orphan/zombie on cancel/timeout/panic | `kill_on_drop(true)` (INV-011) — the PRIMARY cancellation (ADR-016 future-drop) |
| no pipe-buffer deadlock on large output | concurrent stdout/stderr drain + `wait()` via `try_join!` (INV-012) |
| out-of-band kill | `cancel(pid)` — registry-backed (ADR-016 · unknown/dead pid = idempotent `Ok`) |
| deadline | `tokio::time::timeout` → `ShellError::Timeout` |

## Surface

| trait | methods |
|---|---|
| `ShellRun` | `run` (blocklist → spawn → drain + timeout + cancel) |
| `ShellCancel` | `cancel(id)` — kill the registered pid |

Implements the `*Dyn` (`Send`-future) companions; the base traits +
`ShellExecutor` umbrella arrive via the blanket impl. Speaks the kernel
`ShellError` (`NotFound`/`Timeout`/`Cancelled`/`Blocked`/`Other`).

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
