---
id: ADR-095
title: "exec verb security architecture · layered defense-in-depth (permits = declaration, 3 layers enforce)"
status: accepted
date: 2026-06-12
phase: "Phase B · announce-ladder"
deciders: ["@ThibautMelen"]
tags: ["security", "exec", "injection", "sandbox", "permits", "defense-in-depth", "l1", "l2", "l3"]
affects_crates: ["nika-verb-exec", "nika-exec-runner", "nika-kernel-core", "nika-runtime", "nika-schema", "nika-sandbox-seatbelt", "nika-sandbox-landlock"]
affects_layers: ["L0.5", "L1", "L2", "L3"]
supersedes: []
superseded_by: []
related: ["ADR-001", "ADR-003", "ADR-016", "ADR-080", "ADR-081", "ADR-105"]
requires: ["ADR-003"]
enables: []
amends: []
inv: ["INV-011", "INV-012"]
shadow_zones: ["l1-taint-runtime"]
---

# ADR-095 · exec verb security architecture

## Context

The `exec` verb is the most dangerous of the 4 verbs: it runs OS processes
whose arguments are CEL-interpolated from upstream task outputs — including
`infer`/`agent` (LLM) outputs, which are attacker-influenceable by
construction (indirect prompt injection · arXiv 2302.12173). A deep audit
(2026-06-12) plus two primary-source research sweeps (arXiv agent-security
literature + 2026 competitor practice) found one active vulnerability and
several missing SOTA layers.

### The audit findings

| Finding | Severity | Was |
|---|---|---|
| **Array form silently shell-joined** | P0 | The spec (02-verbs §exec) sells `command: ["prog", "{{ v }}"]` as "the STRUCTURAL fix for command injection". But the runtime joined the rendered argv with spaces and ran it through the OS shell (`ExecInput` carried only a `String`), so the *safe* form was injection-VULNERABLE while looking safe. |
| No OS sandbox | P1 | The verb summary reads "exec: Shell command **with sandboxing**"; none existed. The child ran with the engine's full privileges/network/filesystem/env. |
| Blocklist = denylist | P1 | A string-matching denylist that both leaks (∞ bypass variants) AND false-positives legit CI (`timeout`/`env`/`nice`/`printenv` blocked), re-scanning a flattened argv line that has no shell to parse. |
| Env inherited wholesale | P1 | The child inherited the engine's full environment — the env-var-injection class (`LD_PRELOAD`/`BASH_ENV`/`GIT_SSH_COMMAND` …) and secret leakage. |
| Grandchildren leak | P2 | `kill_on_drop` killed only the direct child; a backgrounded grandchild orphaned on timeout. |
| `permits:` not enforced at exec | P1 | `Permits {fs,net,exec,tool}` is fully modeled and spec-mandated ("the engine MUST enforce on both surfaces" · NIKA-SEC-004), but the runtime sink never consulted it. |

### Research convergence (theory + practice agree — high signal)

- **Control/data-plane separation** (CaMeL/DeepMind 2503.18813, design-patterns 2506.08837, Fides/MS 2505.23643): the workflow YAML is the *trusted control plane*; interpolated values are *untrusted data* that may fill declared slots but never select the program, flags, or DAG edges. A declarative DAG + `permits:` give this almost for free.
- **Array-not-shell** (GitHub Actions script-injection: 24,905 issues / 50 repos, arXiv 2208.03837; CVE-2025-30066): string-templating untrusted values into a shell is THE workflow-engine injection class.
- **Default-deny symbolic policy** (Progent 2504.11703, AgentSpec 2503.18666, AgentBound 2510.21236): an allowlist over binary + argv, deterministic, default-deny = `permits.exec`.
- **OS sandbox floor** (RedCode 2411.07781, SandboxEval 2504.00018; Claude Code / Codex / Cursor 2026): "assume the command is hostile" is uncontested; every coding-agent CLI ships an OS-level safe-by-default sandbox, all 100% local/OSS (sovereignty-aligned). Workflow engines (n8n/Airflow/Temporal) ship exec unsandboxed — Nika's chance to lead.
- **Argument-injection canon** (GTFOArgs, SonarSource/CAPEC-6): even without a shell, an argv value injects via flags (`ssh -o ProxyCommand`, `tar --checkpoint-action`, `git -c core.sshCommand`, `curl -K`, `find -exec`). The `--` end-of-options discipline + a per-binary flag denylist.
- **Detection ⇒ telemetry only** (2503.00061: 8/8 published detector defenses bypassed >50% under adaptive attack): a string-matching detector must never be the sole boundary.

## Decision

`permits:` is the **declaration** (the trusted control plane, author-written,
in-file · spec 01 §permits). Three layers **enforce** it; the blocklist
demotes from "the wall" to a shell-mode tripwire.

```
  permits: { fs, net, exec, tool }      DECLARATION · trusted · spec 01 §permits
        |  enforced at 3 layers
  L2 nika-verb-exec  — ExecCommand::{ Shell(String) | Argv(Vec<String>) }
     • argv = the structural injection fix (execve, no shell)        [LAYER 1]
     • env scrub (deny LD_PRELOAD/BASH_ENV/… · reset PATH)           [LAYER 3]
  L3 nika-runtime    — dispatch the array form as a vector (no join)
     • permits.exec allowlist on the resolved program (NIKA-SEC-004) [LAYER 2]
  L0 nika-schema     — one-obvious-way/008 steers string→array; the
     arg-injection rule-pack flags dangerous flags from interpolation [LAYER 2b]
  L1 nika-exec-runner — shell-mode blocklist tripwire · argv program-floor
     • process-group kill (reap grandchildren) · output cap · timeout [LAYER 4/5]
  L1 nika-sandbox    — Landlock+seccomp (Linux) / Seatbelt (macOS):
     net-deny + FS-confine from permits.fs/net + rlimits             [LAYER 6]
```

The inversion: **array (structural) + sandbox (OS) are the walls; the
`permits.exec` allowlist is the intent gate; the blocklist is the shell-mode
tripwire** — not the sole, leaky, false-positive denylist it was.

## The layers

### Layer 1 — Array execution, no shell (L2 + L3 · SHIPPED)

`ExecInput.command` is now `ExecCommand { Shell(String) | Argv(Vec<String>) }`.
The array form maps to `execve` with the shell flag OFF; every element is one
literal argv token — an interpolated value can never break out of its argument
because there is no shell to parse it. The runtime renders each array element
and passes a vector (no join). The string form keeps the OS shell for genuine
pipelines. This closes the P0 and is the structural fix the spec mandates.

### Layer 2 — permits.exec allowlist (L3 · SHIPPED)

The runtime threads the workflow's `Permits` to the exec sink. `exec: false`
(or omitted under a declared boundary) refuses any process; a program
allowlist must contain the resolved leading token (argv[0], or the first word
of a shell line) — matching the static `nika check` (permits_fit) so a
workflow that passes the check never fails at runtime for a different reason.
The static check owns statically-detectable escapes; the runtime owns the
interpolated programs static analysis defers. A `#[non_exhaustive]` future
permit form fails CLOSED.

### Layer 2b — Discoverability + arg-injection lints (L0 nika-schema)

- **one-obvious-way/008** (SHIPPED) warns when an interpolated value lands in a
  string command that needs no shell, pointing to the injection-safe array
  form — fires only when the literal parts carry no shell metacharacter (a
  genuine pipeline keeps the string form · low-false-positive).
- **arg-injection rule-pack** (DESIGNED · the differentiator) — a clean-room
  catalog (from the public GTFOArgs + SonarSource/CAPEC-6 facts) of dangerous
  flags per binary; flags an interpolated value landing in a flag slot / a
  leading-dash value for a known binary, and recommends the `--` end-of-options
  separator. No competing engine ships this.

### Layer 3 — Env hygiene (L1 + L2 · SHIPPED · three sub-layers)

Three sub-layers, weakest-to-strongest, each independent of the OS sandbox:

1. **POSIX-name validation (L2 · `nika-verb-exec`).** An `env:` key must match
   `[A-Za-z_][A-Za-z0-9_]*` — this refuses the dynamic-name carrier class,
   chiefly the exported-shell-function form `BASH_FUNC_x%%` that a bash `sh -c`
   child imports and runs (the payload never appears in the scanned command
   string). Empty / `=` / NUL keys still fail (a strict narrowing).
2. **Dangerous-env denylist (L1 · `nika-exec-runner`).** The runner ALWAYS
   strips the injection vectors that grant code/library injection with no
   dangerous flag: `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `BASH_ENV`,
   `GIT_SSH_COMMAND`, `PYTHONSTARTUP`, `NODE_OPTIONS`, `IFS`, plus
   `HOSTALIASES` / `TERMINFO(_DIRS)` / `TERMCAP` (file-read / crafted-entry
   load) — independent of `pre_validated` (the floor wins over an explicit set).
3. **Ambient-secret scrub (L1 · `nika-exec-runner` ← composed by `nika-cli`).**
   The engine loads provider API keys from ITS env (`config_from_env`) for its
   OWN inference calls; an exec child inherits them by ambient default and an
   untrusted command could exfiltrate them (`printenv`, `cat /proc/self/environ`).
   The composition root injects the provider-key NAMES (from the catalog) into
   `TokioShell`, which strips each from every child UNLESS the workflow set it
   EXPLICITLY in `env:` (explicit intent wins · only the ambient copy is
   removed). Least-privilege by default; empty scrub set = legacy behavior.

The stronger *full* clean-slate posture (drop ALL inherited env + a curated
`PATH`) still ships with the sandbox (Layer 6), which knows the confined
workflow's complete declared env needs; sub-layer 3 is the targeted
secret-scrub floor that holds before the sandbox is composed in.

### Layer 4/5 — Process lifecycle (L1 · SHIPPED partial)

The child spawns in its own process group (the safe std `process_group(0)`, no
unsafe `pre_exec`); on timeout/cancel the whole group is signalled (SIGTERM →
grace → SIGKILL via `nix::killpg`) so detached grandchildren die. Output is
capped (64 MiB/stream) and the timeout hard-kills. Resource rlimits
(RLIMIT_CPU/FSIZE/NPROC + no-core-dump) need a child-side `pre_exec` (unsafe),
so they move to `nika-sandbox` (which owns the low-level confinement).

### Layer 6 — OS sandbox (PER-PLATFORM L1 crates · macOS SHIPPED + verified)

**Reconciliation (2026-06-13).** The kernel already RESERVES a capability
`Sandbox` (`plugin::sandbox` · WASM/MCP · the `enter()` self-restriction model
· v0.100), whose doc names the future impl crates `nika-sandbox-landlock`
(Linux) and `nika-sandbox-seatbelt` (macOS). Confining the `exec` verb's CHILD
process is a DISTINCT operation — "transform the command into its confined
form" — so it rides a NEW additive kernel seam, NOT the reserved trait, and
ships as the per-platform crates the reservation already names (operator-locked
2026-06-13: per-platform crates + a new trait · supersedes the original single
`nika-sandbox` framing of this ADR).

**The seam (L0.5 · `nika-kernel-core/src/io/command_sandbox.rs` · SHIPPED).**
A new `CommandSandbox` trait — sync, object-safe, `confine(spec, ShellCommand)
-> ShellCommand` — plus `SandboxSpec` (`fs_read`/`fs_write` prefixes ·
`allow_network` · derived from `permits.fs/net`), `CommandSandboxError`
(`Unavailable`/`Profile` · fail-closed), and a `NoopSandbox` (the explicit,
auditable "unconfined" choice). Add a `ShellCommand.sandbox` field. The
**wrapper model** (no `unsafe`, no FFI · the seam returns a TRANSFORMED command,
not a `pre_exec` closure) replaces the original `Confinement`-closure design —
it keeps the crates at `unsafe_code = forbid` and is what every coding-agent CLI
uses.

**macOS — `nika-sandbox-seatbelt` (SHIPPED · adversarially verified).** Wraps a
command in the OS-shipped `sandbox-exec` launcher with a deny-default SBPL
profile generated from the spec: network denied unless granted; writes only
under declared `fs_write` + scratch; reads of the system paths every binary
needs, plus declared `fs_read` — so sensitive home paths (`~/.ssh`, `~/.aws`)
are denied. Profile-injection via a malicious path is closed (control-char
refusal + quote-escaping). An adversarial jail suite RUNS `sandbox-exec` on
darwin and proves: a home-secret read is denied, a declared path is readable, a
write outside the allowlist is denied, an ordinary command still runs. Wired
into `TokioShell::with_sandbox` (applied after the blocklist · fail-closed).

**Linux — `nika-sandbox-landlock` (CI-GATED).** The sibling crate: `landlock`
(FS allowlist) + `extrasafe`/seccomp (block ptrace/unshare/AF_UNIX) + net-deny
+ rlimits, via `bwrap` or a Landlock helper. Only partly verifiable off a Linux
box → a focused CI-backed arc. Same `CommandSandbox` seam.

**Follow-on plumbing.** Runtime derivation (`permits.fs/net` → `SandboxSpec` →
`ExecInput.sandbox`) and composer injection (select + inject the platform
backend into `TokioShell`) connect a workflow's `permits:` to confinement
end-to-end; the composer is the `nika-cli`/engine surface (in flight).

Honest limits (documented in the crate): glob→subpath is a coarsening (path-
prefix, not per-file); network is allow-all-or-deny (host-granular needs a
proxy); resource rlimits + the SandboxEval egress suite are part of the
landlock arc. Full 12-gate admission (Linux backend · mutation ≥90% · review
swarm) is the CI completion.

### Layer 7 — Blocklist demoted to a tripwire (L1 · SHIPPED)

The string-matching blocklist is no longer the wall. Shell mode keeps the full
battle-tested blocklist + the expansion-char refusal (the tripwire for the
genuinely-dangerous string path). Array mode gets an exact-basename
`check_program` against a small dangerous-program set (privilege escalation +
system control) — no fake-shell scan, which removes the `timeout`/`env`/`nice`/
`printenv` false-positives. Per the adaptive-attack evidence, the blocklist is
defense-in-depth, never the primary boundary.

## Consequences

**Positive.** The structural injection hole is closed; the safe form is the
default and discoverable; the capability boundary is enforced; the env / process
classes are shut; the sandbox (when shipped) makes "even if the blocklist
misses it, it can't hurt you" true. The architecture matches the converging
SOTA and stays 100% local/OSS (sovereignty). The `permits:` declaration unifies
all enforcement (the file is the security boundary).

**Negative / interim gaps (logged, not silent).**

- The argv program-floor no longer blocks a directly-authored dangerous command
  (e.g. a destructive `rm` on root) — that is gated by `permits.exec` + the
  sandbox, not the floor. Acceptable: the floor is identity-based; intent is
  policy + sandbox.
- The OS sandbox (Layer 6) is the big remaining lift, cross-platform and only
  partly verifiable off a Linux box — a focused CI-backed admission arc.
- Runtime taint propagation (which resolved values are interpolation-derived)
  is deferred — the arg-injection guard ships as a static lint; runtime
  taint-enforcement lands when the binding layer carries taint (shadow zone
  `l1-taint-runtime`).
- Operator/host policy + token budgets remain `nika-policy`'s job (gated on the
  kernel migration); the runtime `permits` enforcement is the interim.

## Alternatives considered

- **Blocklist-only (status quo).** Rejected: both leaks and false-positives;
  the adaptive-attack literature refutes detector-as-boundary.
- **Container/microVM per exec** (e2b, Firecracker). Rejected as the default:
  non-sovereign (hosted) or heavy startup latency; the agent-CLI OS-sandbox
  model fits a local-first engine better.
- **Sandbox as a module in nika-exec-runner.** Rejected: heavy platform deps +
  unsafe `pre_exec` + independent adversarial testing → its own L1 crate behind
  a kernel seam is the Diamond-coherent shape.

## Implementation status

| Layer | Crate(s) | Status |
|---|---|---|
| 1 array execution | nika-verb-exec · nika-runtime | ✅ shipped |
| 2 permits.exec | nika-runtime | ✅ shipped |
| 2b /008 steer | nika-schema | ✅ shipped |
| 2b arg-injection pack | nika-schema | ✅ shipped (arg-injection/001) |
| 3 env POSIX-name validation | nika-verb-exec | ✅ shipped (kills `BASH_FUNC_x%%` carrier) |
| 3 env dangerous-denylist | nika-exec-runner | ✅ shipped (+HOSTALIASES/TERMINFO/TERMCAP) |
| 3 env ambient-secret scrub | nika-exec-runner ← nika-cli | ✅ shipped (provider keys · explicit `env:` wins) |
| 3 env full clean-slate | nika-sandbox | 🔜 → L6 (drop-all + curated PATH) |
| 4/5 process group | nika-exec-runner | ✅ shipped · rlimits → L6 |
| 6 sandbox seam | nika-kernel-core (CommandSandbox) | ✅ shipped |
| 6 sandbox macOS | nika-sandbox-seatbelt | ✅ shipped · adversarially verified |
| 6 sandbox wiring | nika-exec-runner (with_sandbox) | ✅ shipped |
| 6 sandbox Linux | nika-sandbox-landlock | ✅ shipped · CI-gated (bwrap in the tests leg · 4 jail proofs) |
| 6 permits→spec + composer inject | nika-runtime · engine | ✅ shipped (v0.105.x · dispatch derives the spec from `permits:` · the composer wires seatbelt/landlock · noop note when no backend) |
| 7 blocklist tripwire | nika-exec-runner | ✅ shipped (mode-split) |

## References

- Spec: `nika-spec spec/02-verbs.md §exec` · `spec/01-envelope.md §permits` · `spec/05-errors.md` (NIKA-EXEC-001/002 · NIKA-SEC-001/004).
- arXiv: 2302.12173 · 2208.03837 · 2503.18813 (CaMeL) · 2506.08837 · 2505.23643 (Fides) · 2504.11703 (Progent) · 2503.18666 (AgentSpec) · 2510.21236 (AgentBound) · 2411.07781 (RedCode) · 2504.00018 (SandboxEval) · 2503.00061 (adaptive attacks).
- Catalogs: GTFOArgs · SonarSource argument-injection-vectors (CAPEC-6) · CVE-2024-24576 (BatBadBut · Windows guard).
- Crates: `landlock` 0.4.5 · `extrasafe` · `nix` 0.30 · std `CommandExt::process_group`.
- Related: ADR-001 (CRAFT) · ADR-003 (12-gate) · ADR-016 (cancellation) · ADR-080 (mcp-stdio sandbox) · ADR-081 (L1 guard contract).
