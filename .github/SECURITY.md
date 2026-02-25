# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.9.x   | :white_check_mark: |
| 0.8.x   | :white_check_mark: |
| < 0.8   | :x:                |

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

- **cargo-audit**: Dependency vulnerability scanning in CI
- **cargo-deny**: License and advisory checks
- **Secret scanning**: Enabled via GitHub
- **SAST**: Static analysis security testing
- **FORTRESS mode**: 10 quality gates for all PRs

## Version Lock Policy

Nika will **NEVER** be version 1.0.0 or higher. This is by design:

- Perpetual 0.x.x enables continuous evolution
- SemVer 0.x allows breaking changes without drama
- See: [FORTRESS Design](docs/plans/2025-02-25-nika-fortress-design.md)

## Hall of Fame

We thank the following security researchers:

_No reports yet - be the first!_
