# Security Policy

## Supported Versions

Nika is under active Diamond rebuild on `main` (renamed 2026-05-06 from
`nika-diamond`). Security fixes ship on the active branch only — see
[ADR-002](docs/adr/adr-002-forever-v0x.md) for the
release model (real semver toward 1.0 · amended D-2026-06-20-N1).

| Branch       | Status     | Security fixes |
|--------------|------------|----------------|
| `main`       | active     | ✅ yes         |
| `brouillon`  | read-only legacy (v0.79.3) | ❌ frozen |

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

## Defense Layers Already Shipped

Per the Nika Shield threat model (v0.79 carry-over · v0.80+ kernel
enforcement) ·

| Code        | Layer                          | Defense                                      |
|-------------|--------------------------------|----------------------------------------------|
| `NIKA-271`  | Skill integrity                | blake3 hash mismatch on skill load           |
| `NIKA-380`  | Capability enforcement         | dangerous tool on untrusted data → block     |
| `NIKA-381`  | Trust propagation              | strict-mode trust-level violation            |
| `NIKA-382`  | Exfil canary                   | canary token found in output                 |
| `NIKA-383`  | Prompt injection               | scanner + ML detection                       |
| `NIKA-384`  | Spotlight                      | untrusted data without spotlight wrapping    |
| `NIKA-385`  | ML model availability          | injection detection model missing            |
| `NIKA-386`  | Workflow recursion             | depth-exceeded block                         |
| `NIKA-387`  | Workflow cycle                 | cycle-detection block                        |
| `NIKA-388`  | Canary-in-thinking             | extended-thinking trace leak detection       |
| `NIKA-389`  | Vision sanitization            | untrusted images from untrusted sources      |
| `NIKA-390`  | Memory-write injection (queued · ADR-073) | `MemoryRemember` payload scan at ingest · prevents stored prompt-injection from triggering via later `MemoryRecall` semantic match (symmetric to recall-time gate ADR-030) |

## Structural Hardening (architectural)

- **`unsafe_code = "forbid"`** workspace-wide ([Cargo.toml](Cargo.toml))
- **Zero `.unwrap()` / `.expect()` in `src/`** — CI-enforced
- **`cargo deny check`** on every PR ([deny.toml](deny.toml))
- **`cargo audit`** on every dependency change
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

- [ADR-002 — release model (real semver toward 1.0 · amended)](docs/adr/adr-002-forever-v0x.md)
- [ADR-014 — Sealed kernel traits](docs/adr/adr-014-sealed-kernel-traits.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution + 12-gate process
- [`BLUEPRINT_2036.md`](docs/architecture/BLUEPRINT_2036.md) — 10-year horizon
