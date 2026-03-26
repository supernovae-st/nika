# Nika Release Template

> Shared template: See `dx/.github/RELEASE_TEMPLATE.md` for full documentation.

## Title Format

```
{emoji} Nika v{version} — {Tagline}
```

## Body Template

```markdown
## 🔐 Security Fixes (if any)

- **{Fix Name}** - Description
- Prevents {attack vector}

## ✨ New Features

### {Feature Name} (Schema @{version})
Description of the feature:
\`\`\`yaml
# Example usage
feature:
  option: value
\`\`\`

## 🚀 CI Changes (if any)

- Added `{job-name}` job
- Validates {what it validates}

## 📚 New Examples (if any)

- `{filename}.nika.yaml` - Description
- `{filename}.nika.yaml` - Description

## 🔄 Changed

- **{Component}** - Description of change

## 🐛 Fixed

- **{Bug}** - Description of fix

## 📊 Stats

- **{N} tests passing**
- Zero clippy warnings
- Schema @{version} fully supported

---
**Full Changelog**: https://github.com/supernovae-st/nika/compare/v{prev}...v{current}
```

## Nika-Specific Sections

### For Schema Version Bumps
```markdown
## 📋 Schema @{version}

New fields:
- `{field}:` - Description

Breaking changes:
- `{old_field}` → `{new_field}`
```

### For TUI Changes
```markdown
## 🖥️ TUI Updates

- **{View}** - Description of change
- New keybinding: `{key}` → {action}
```

### For Provider Changes
```markdown
## 🤖 Providers

- **{Provider}** - Description
- New model support: `{model-name}`
```

## Emoji Quick Reference

| Context | Emoji |
|---------|-------|
| Security | `🔐` |
| Features | `✨` |
| CI | `🚀` |
| Examples | `📚` |
| Schema | `📋` |
| TUI | `🖥️` |
| Providers | `🤖` |
| Performance | `⚡` |
