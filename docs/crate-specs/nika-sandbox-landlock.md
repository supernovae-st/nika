# Crate spec — `nika-sandbox-landlock`

| | |
|---|---|
| Status | **Linux backend SHIPPED (bwrap wrapper) · adversarial jail proof CI-gated (runs when bwrap present, skips otherwise · matches the sibling's macOS-runner pattern) · Landlock LSM hardening + mutation ≥90% + review swarm pending** (ADR-095 Layer 6) |
| Layer | L1 — effect crate · the Linux impl of the L0.5 `nika_kernel::command_sandbox::CommandSandbox` seam |
| Design | wraps a command in the `bwrap` (bubblewrap) launcher with a mount + namespace jail generated from `SandboxSpec` (derived from `permits.fs/net`) · the wrapper model · NO unsafe · NO FFI · NO heavy deps |
| LOC budget | well under the caps · an argv builder + the shared spec-to-grant validation |
| License | `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| NIKA codes | none — speaks the kernel `CommandSandboxError` (`Unavailable` · `Profile`) |

---

## 1. Purpose

Confine the `exec` verb's CHILD process to the workflow's declared filesystem +
network boundary (`permits.fs/net`) on Linux. The Linux half of the per-platform
sandbox pair the kernel's reserved `plugin::sandbox` doc names
(`nika-sandbox-seatbelt` is the macOS sibling). Distinct from that reserved
capability `Sandbox` (WASM / MCP · `enter()` self-restriction · later 1.x): this
implements the additive `CommandSandbox` "transform-the-command" seam.

## 2. The wrapper model (no `unsafe`)

`confine(spec, command)` returns `bwrap <jail args> -- <inner command>` — the
runner spawns it. No in-process `pre_exec` closure (which would need `unsafe`,
banned workspace-wide). The same model coding-agent CLIs use, and byte-for-byte
the model the macOS sibling uses. The inner command is reconstructed faithfully
(argv verbatim · shell form → `/bin/sh -c <line>`); `cwd`/`env`/`stdin`/`timeout`
ride the outer launcher so the confined child inherits them.

## 3. What the jail enforces (deny-default)

- **Network** — the `spec.net` tri-state: `--unshare-all` drops the net
  namespace (`deny` — loopback included, the stricter reading: bwrap cannot
  bring `lo` up alone); `--share-net` re-adds it (`allow`, and `allowlist`).
  Under `allowlist` the child gets the loopback egress proxy's env contract
  (the sandbox-runtime model — the proxy serves exactly the declared
  `permits.net.http` set). HONEST LIMIT: bwrap cannot fence loopback-only,
  so on Linux the allowlist arm's OS floor IS the env contract (an
  env-stripping client is not fenced — unlike macOS's Seatbelt port fence);
  srt's `--unshare-net` + socat-bridge-over-unix-socket is the named
  follow-on that closes it.
- **Writes** — read-write `--bind` only under the validated `fs_write` literal
  prefixes; everything else is read-only or absent.
- **Reads** — the system trees (linker, libs, shell) are `--ro-bind`; declared
  `fs_read` prefixes add read-only reach; `$HOME` is bound NOWHERE, so sensitive
  files (`~/.ssh`, `~/.aws`) are absent from the jail — deny-default by absence.

## 4. Soundness (shared with the sibling)

`grant_subpath` / `literal_prefix` are the same validation the macOS backend
uses: a permit glob's literal prefix, refused if it is `/`, non-absolute,
`~`/`$`-bearing, a `..` traversal, contains a NUL, or a bare system-root
directory (which would bind a whole tree). Fail-closed: a rejected permit maps to
a refusal to spawn, never a wider grant.

## 5. Honest limits (future work)

Path-prefix granularity, not per-file (the precise glob check is `permits_fit`'s
static job; the sandbox is the OS floor). Host-granular network needs a proxy.
The named hardening is **Landlock LSM** (unprivileged in-kernel path filtering,
kernel ≥ 5.13) applied via a helper alongside the bwrap namespaces — a follow-on
that reuses this crate's spec-to-grant logic. The adversarial jail proof runs in
CI when bwrap is present and skips (never fails) otherwise, exactly like the
sibling's macOS-runner-gated proof.
