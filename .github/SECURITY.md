# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.79.x+ | Yes |
| 0.63.x - 0.78.x | Security fixes only |
| < 0.63 | No |

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, email: **security@supernovae.studio**

Include:

1. Description of the vulnerability
2. Steps to reproduce
3. Potential impact
4. Suggested fix (if any)

### Response Timeline

| Stage | Timeline |
|-------|----------|
| Acknowledgment | 48 hours |
| Initial assessment | 7 days |
| Fix timeline shared | 14 days |
| Public disclosure | After fix released |

## Security Measures

### CI/CD Security

- **cargo-audit**: Dependency vulnerability scanning on every PR
- **cargo-deny**: License and advisory checks (`deny.toml` configured)
- **SAST**: CodeQL + Semgrep weekly scans
- **CI**: Format, clippy, test, coverage, security audit on every PR

### Runtime Security

- **SSRF protection**: `fetch:` blocks private IP ranges by default
- **Command blocklist**: `exec:` blocks dangerous commands (`rm -rf /`, `sudo`, fork bombs -- NIKA-053)
- **Template injection**: `$()` and backticks blocked in shell templates
- **Path traversal**: File tools validate against `../` attacks
- **Secret redaction**: API keys never appear in logs, traces, or error messages
- **NikaVault**: XChaCha20Poly1305 + Argon2i KDF for local secret storage (no OS keychain)

### Code Quality

- **Zero unsafe blocks** in Nika source
- **Zero CVE** in dependencies (cargo-audit enforced)
- **9,000+ tests** across 17 crates
- **Zero clippy warnings** (`-D warnings` enforced)
- **SLSA Level 2**: Signed provenance on releases

## Version Lock Policy

Nika will **NEVER** be version 1.0.0 or higher. Perpetual 0.x.x is by design.

## Hall of Fame

We thank the following security researchers for responsibly disclosing vulnerabilities:

_No reports yet -- be the first!_
