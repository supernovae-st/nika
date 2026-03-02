# Security Policy

## Supported Versions

| Version | Supported          | Security Audit |
| ------- | ------------------ | -------------- |
| 0.16.x  | :white_check_mark: | [v0.16.5 Audit](../docs/SECURITY-AUDIT-v0.16.5.md) (Score: 92/100) |
| 0.15.x  | :white_check_mark: | - |
| 0.14.x  | :white_check_mark: | - |
| < 0.14  | :x:                | - |

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to: **security@supernovae.studio**

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

Nika implements the following security measures:

### CI/CD Security
- **cargo-audit**: Dependency vulnerability scanning (578 crates, 0 CVE)
- **cargo-deny**: License and advisory checks (deny.toml configured)
- **cargo-geiger**: Unsafe code inventory (weekly scans)
- **CodeQL**: Semantic SAST analysis (weekly)
- **Semgrep**: Pattern-based security scanning (weekly)
- **ARMADA**: 10 quality checkpoints for all PRs

### Code Quality
- **Zero unsafe blocks**: Nika contains 0 unsafe code
- **Zero CVE**: All dependencies verified safe
- **3,358 tests**: Comprehensive test coverage
- **Zero clippy warnings**: Strict linting enabled

### Security Posture
- **OWASP Top 10 (2021)**: 8/8 applicable checks passed
- **CWE Coverage**: 6/6 applicable weaknesses mitigated
- **SLSA Level**: 2 (signed provenance)
- **Latest Audit**: v0.16.5 - Score 92/100 (Excellent)

## Version Lock Policy

Nika will **NEVER** be version 1.0.0 or higher. This is by design:

- Perpetual 0.x.x enables continuous evolution
- SemVer 0.x allows breaking changes without drama
- See: [FORTRESS Design](docs/plans/2025-02-25-nika-fortress-design.md)

## Hall of Fame

We thank the following security researchers:

_No reports yet - be the first!_
