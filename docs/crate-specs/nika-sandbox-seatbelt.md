# Crate spec — `nika-sandbox-seatbelt`

| | |
|---|---|
| Status | **macOS backend SHIPPED + adversarially verified · Linux sibling (`nika-sandbox-landlock`) CI-gated · full 12-gate = mutation ≥90% + Linux + review swarm pending** (ADR-095 Layer 6) |
| Layer | L1 — effect crate · the macOS impl of the L0.5 `nika_kernel::command_sandbox::CommandSandbox` seam |
| Design | wraps a command in the OS-shipped `sandbox-exec` launcher with an SBPL profile generated from `SandboxSpec` (derived from `permits.fs/net`) · the wrapper model · NO unsafe · NO FFI · NO heavy deps |
| LOC budget | well under the caps · only a profile-string builder + the launcher argv |
| License | `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| NIKA codes | none — speaks the kernel `CommandSandboxError` (`Unavailable` · `Profile`) |

---

## 1. Purpose

Confine the `exec` verb's CHILD process to the workflow's declared filesystem +
network boundary (`permits.fs/net`). The macOS half of the per-platform sandbox
pair the kernel's reserved `plugin::sandbox` doc names (`nika-sandbox-landlock`
is the Linux sibling). Distinct from that reserved capability `Sandbox` (WASM /
MCP · `enter()` self-restriction · later 1.x): this implements the NEW additive
`CommandSandbox` "transform-the-command" seam.

## 2. The wrapper model (no `unsafe`)

`confine(spec, command)` returns `sandbox-exec -p <profile> -- <inner command>`
— the runner spawns it. No in-process `pre_exec` closure (which would need
`unsafe`, banned workspace-wide). Same model Claude Code / Codex / Cursor use on
macOS. The inner command is reconstructed faithfully (argv verbatim · shell form
→ `/bin/sh -c <line>`); `cwd`/`env`/`stdin`/`timeout` ride the outer launcher so
the confined child inherits them.

## 3. The SBPL profile (the security heart · deny-default)

- **Network** — the `spec.net` tri-state (the Anthropic sandbox-runtime
  seatbelt model, verified live against `sandbox-exec`): `deny` admits
  loopback outbound only (`(remote ip "localhost:*")`); `allow` emits
  `(allow network*)` (the explicit escape hatch); `allowlist` admits
  loopback scoped to the per-run egress proxy's port
  (`(remote ip "localhost:PORT")`) — the proxy (`nika-exec-runner`'s
  CONNECT+SOCKS5 mux) serves exactly the declared `permits.net.http` set and
  the child gets its env contract, because a Seatbelt host rule is
  TLS-blind: the profile fences the CHANNEL, the proxy fences the HOSTS.
- **Writes** allowed ONLY under declared `fs_write` prefixes + scratch.
- **Reads** allow the system paths every binary + the dynamic linker need;
  declared `fs_read` prefixes added; sensitive home paths (`~/.ssh`) denied.
- **Profile-injection closed** at two boundaries: a control char in a path is
  refused; `\`/`"` are escaped (a path can never break out of its SBPL string).
- **Over-grant impossible to express** (`grant_subpath` · audit P1): a permit
  resolving to root `/`, a bare system-root (`/etc`, `/Users`, …), a
  non-absolute / `~` / `$VAR` path, or a `..` traversal FAILS CLOSED.

## 4. Adversarial proof (`tests/seatbelt_jail.rs` · macOS · runs `sandbox-exec`)

A confined `cat ~/.secret` is DENIED · a declared `fs_read` path is readable · a
write outside the allowlist is DENIED · an ordinary command still runs. Plus the
unit battery: deny-default network, declared reads/writes, the injection-refusal
and over-grant-refusal sets.

## 5. The 12 gates (status)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file |
| 2 TDD | ✅ unit (profile/escape/grant/wrap) + the adversarial jail suite |
| 3 IMPL | ✅ no unsafe · no heavy deps (only `nika-kernel`) |
| 4 CLIPPY 0 | ✅ |
| 5 MUTATION ≥90% | ✅ 97% killed (36/37 viable · 2026-07-29 · `check-mutation-floor.sh` — the availability decision extracted pure + the confine happy-path pinned to kill the 4 survivors; the one survivor is the `available()` binder's `-> true`, equivalent on a macOS+launcher host by construction) |
| 6 PROPERTY | ✅ injection + over-grant refusal batteries |
| 8 DOCS | ✅ module + per-fn |
| 9 CANARY | ✅ the jail suite IS the canary (macOS) |
| 11 REVIEW | ✅ adversarial security review folded (audit P1 → grant_subpath) · swarm CI |
| 12 ATOMIC | ✅ |

## 6. Dependencies

`nika-kernel` (the `CommandSandbox` trait + `ShellCommand` + `SandboxSpec`).
Nothing else — the wrapper model needs no sandboxing crate.

## 7. Honest limits

Glob→subpath is path-prefix granularity (the precise per-file check is
`permits_fit`'s static job); network is allow-all-or-deny; `sandbox-exec` is
deprecated-but-shipping (as Chromium/Bazel use it). Resource rlimits + a
confinement-took-effect self-test + the network-egress executed test are part of
the Linux/CI completion arc.

The ONE same-directory extension: an EXACT-file grant (no glob metacharacter)
also admits its SQLite journal family — `<db>-wal`, `<db>-shm`, `<db>-journal`
— as three exact-path `literal` filters on the same rule, access class
inherited. Without them a confined WAL open dies with `SQLITE_CANTOPEN` (14):
bisected live 2026-07-29 (macOS 15.6.1 · sqlite 3.43.2 — bare file grant
fails, the three literals pass WAL/rollback/reopen, an `ATTACH`-ed db outside
the grant stays refused, and an arbitrary sibling write stays denied). The
multi-db super-journal (`<db>-mj*`) is deliberately out (an `ATTACH`-ed database
needs its own grant). The Linux sibling cannot express the family (bwrap has
no future-file bind) — there a database grant must name its directory.

The finding's second half: every relative open in the child rides the libc
`getcwd`/`realpath` walk, which reads directory ENTRIES — under deny-default
that read dies (`file-read-data` on the cwd, per the kernel denial log), so
even a fully-granted database CANTOPEN'd when the workflow handed the child a
relative path (the qrsmart bisect). The profile therefore lists the child's
cwd and each exact-file grant's parent as `file-read-data` literals —
directory LISTINGS only, never file contents — ending the run with zero
residual fs denials (verified against the same log).
