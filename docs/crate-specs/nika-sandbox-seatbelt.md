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

- **Network** — the `spec.net` tri-state: `deny` keeps network out of the
  profile; `allow` emits `(allow network*)`; `allowlist` confines as `allow`
  until the loopback egress proxy lands (Seatbelt host filtering is TLS-blind,
  so host-granular egress is the proxy's job, never the profile's).
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
| 5 MUTATION ≥90% | ✅ 96% killed (26/27 viable · 2026-07-10 · `check-mutation-floor.sh` — the availability decision extracted pure + the confine happy-path pinned to kill the 4 survivors) |
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
