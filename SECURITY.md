# Security Policy

## Supported Versions

0.124 is the current release line. Work stays focused on maintenance,
security and the media capabilities workflows need; native video is planned
for 0.125. Security fixes land on `main`, the only maintenance branch, and
ship in the next release. The 0.123.x line is the previous one and receives
no further patches. Report against the latest release or `main`.

| Version or branch        | Status                            | Security fixes |
|--------------------------|-----------------------------------|----------------|
| 0.124.x (latest release) | maintained                        | ✅ yes         |
| `main`                   | maintenance branch                | ✅ yes         |
| 0.123.x                  | previous line: upgrade to 0.124.x | ❌ no          |
| older than 0.123         | unsupported: upgrade              | ❌ no          |
| `brouillon`              | read-only legacy (v0.79.3)        | ❌ frozen      |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub
issues, discussions, or pull requests.**

Instead, send an email to **security@supernovae.studio** with ·

- A description of the vulnerability and where it lives in the codebase
- Steps to reproduce or a minimal proof-of-concept
- The version / branch / commit SHA where you observed it
- Your assessment of the impact (CVSS optional · plain-language fine)

We will acknowledge receipt within **72 hours** and aim to provide a
substantive response (initial triage + ETA) within **7 days**.

## Disclosure Process

1. **Triage** · maintainers verify the report and confirm the scope
2. **Fix development** · patch authored privately on a security branch
3. **Pre-disclosure notice** · downstream consumers (when applicable)
   notified via private channel before public release
4. **Public release** · CVE assigned · GitHub Security Advisory published
   · CHANGELOG entry references the advisory
5. **Credit** · reporter named in advisory unless anonymity is requested

We aim for **≤90 days** between report and public disclosure for
non-trivial vulnerabilities, shorter for actively-exploited issues.

## Defense Layers in 0.124

What the released engine enforces. Every code below resolves with
`nika explain <code>` on 0.124.1; most are refused by `nika check` before a
run starts, and the runtime enforces the same boundary again.

| Code | Layer | Defense |
|---|---|---|
| `NIKA-AUTH-006` | Default deny | a workflow with effects and no `permits:` block has zero authority |
| `NIKA-SEC-004` | Capability boundary | an effect outside the declared `permits:` (fs · net · exec · tool) is refused, including an `infer:` `vision:` file outside `permits.fs.read` |
| `NIKA-SEC-001` | Shell floor | `exec` refused before the child spawns on a destructive or privilege pattern, whatever the permits say |
| `NIKA-SEC-002` | Agent tool whitelist | an agent's tool call outside its `tools:` list is refused |
| `NIKA-SEC-003` | Recursion bound | nested-run depth exceeded, or a self-launching workflow |
| `NIKA-SEC-005` | SSRF floor | a `nika:fetch` or `nika:notify` URL that resolves to a loopback, private, link-local or metadata address is refused, whatever the permits say |
| `NIKA-SEC-006` · `NIKA-SEC-007` | Secret flow | a `secrets.<name>` value reaching an unsanctioned sink (exec argument, invoke payload, prompt, `vision:` entry) or the workflow's outputs |
| `NIKA-SEC-008` | Data as code | a fetched URL naming code-bearing content without an explicit `lift:` door |
| `NIKA-SEC-009` | Lethal trifecta | private read + untrusted ingress + external egress, with no blocking human gate in between |
| `NIKA-SEC-010` · `NIKA-SEC-014` | Approval and consent | an approval replayed, mismatched or rate-limited; a refused confirm gate that still lets an egress effect through |
| `NIKA-SEC-011` | Preview-commit divergence | the bytes about to fire differ from the previewed ones |
| `NIKA-SEC-012` | Unordered shared writes | two tasks with no order between them write the same target |
| `NIKA-SEC-015` | Order law | an `exec:` task runs downstream of a network effect |
| `NIKA-1710` · `NIKA-1711` | OS sandbox | `exec` children and MCP servers run confined (macOS Seatbelt · Linux bubblewrap); a host that cannot confine a `permits:`-declaring workflow with `exec:` children refuses to run it, and a mistyped `NIKA_SANDBOX` refuses to start |
| `NIKA-MCP-003` · `NIKA-MCP-004` · `NIKA-MCP-005` · `NIKA-MCP-006` | MCP definition pins | `nika mcp approve <server>` pins each tool's definition ([docs/ops/mcp-servers.md](docs/ops/mcp-servers.md)); before any tool call, a run refuses a server whose definitions drifted from its pins, a pin lockfile it cannot trust, a server the OS sandbox cannot confine and a tool that was never approved |

Without a code `nika explain` resolves:

- **Tamper-evident, signed runs.** Every run journal is hash-chained, and
  runs are signed once `nika key init` minted a run-signing key;
  `nika trace verify` checks both.
- **Harness seats hold only what they ask.** An `agent:` task on an agent
  application you are signed into (Codex, Claude Code, Gemini CLI over ACP)
  runs that application's own tools. Nika roots the session in the project,
  holds it in the application's measured asking mode (refusing to start
  otherwise), judges every permission it asks against `permits:` and journals
  each decision as `permit_checked`. A path is judged where the operating
  system lands, through every symlink and `..`; an ask for network access, for
  a wider application sandbox, to run a command outside it (every command
  Codex asks about in `read-only`) or to approve one of the application's own
  MCP tool calls always pauses for the operator, even under
  `permits.exec: true`. The pause question is one line the application cannot
  steer (controls and invisible characters removed) and names the command and
  its reason; one `--answer` decides one ask for the task, never an ask in an
  attempt `retry:` replays. What the mode lets the application do without asking
  is outside `permits:`: Codex, for one, reads any file and writes the project,
  `/tmp` and `$TMPDIR` inside its own sandbox. An action Nika allows runs under
  the application's own sandbox. `nika check`
  states it on each such task (`[harness]`, `nika explain harness`);
  [docs/usage/harness-agents.md](docs/usage/harness-agents.md) lists it per
  seat. A report that a harness acted beyond what that page says it can is in
  scope.

## Structural Hardening (architectural)

- **`unsafe_code = "forbid"`** workspace-wide ([Cargo.toml](Cargo.toml))
- **Zero `.unwrap()` / `.expect()` in `src/`** — CI-enforced
- **`cargo deny check`** on every PR ([deny.toml](deny.toml)): RustSec
  advisories, licenses, bans and sources
- **`cargo audit`** in the nightly hygiene run
- **AGPL-3.0-or-later** — share-alike on hosted forks
- **`#[non_exhaustive]` everywhere** + `pub fn new()` — additive evolution
- **Sealed kernel traits** ([ADR-014](docs/adr/adr-014-sealed-kernel-traits.md)) — external impl-bypass blocked

## Out of Scope

The following are NOT vulnerabilities we will accept reports for ·

- Issues in dependencies not used by Nika
- Theoretical attacks without working proof-of-concept
- Social engineering or phishing reports against maintainers
- Denial-of-service via resource exhaustion in user-supplied workflows
  (this is by design — workflows execute with capability bounds, not
  resource bounds, by default)

## Hall of Fame

Researchers who responsibly disclose security issues will be credited
here once fixes ship. Anonymity respected on request.

## Related

- [ADR-002 — release model](docs/adr/adr-002-forever-v0x.md)
- [ADR-014 — Sealed kernel traits](docs/adr/adr-014-sealed-kernel-traits.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — what contributions are in scope, and the 12-gate process
- [Known limits in 0.124](https://docs.nika.sh/changelog/known-limits) — what the release does not do, each with a way around it
