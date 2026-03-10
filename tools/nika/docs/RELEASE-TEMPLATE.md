# 🦋 Nika Release Template

**Ce template est OBLIGATOIRE pour toutes les nouvelles releases.**

---

## Structure Standard

Chaque release dans CHANGELOG.md doit suivre cette structure :

```
## [X.Y.Z] - YYYY-MM-DD

╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║    🦋  N I K A   v X . Y . Z                                                  ║
║                                                                               ║
║    [TITRE] — [SOUS-TITRE]                                                     ║
║                                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║    📊 Stats                                                                   ║
║    Tests:    X,XXX passing  │  Coverage: XX%  │  Clippy: Zero warnings        ║
║    Files:    XX changed     │  +XXX lines     │  -XX lines                    ║
║                                                                               ║
║    🎯 Highlights                                                              ║
║    ├── ✨ Feature 1                                                           ║
║    ├── 🐛 Bug fix 2                                                           ║
║    └── ⚡ Performance 3                                                       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Sections Obligatoires

### 1. Banner Header

Toujours commencer par le banner ASCII avec :
- Version number
- Titre accrocheur
- Stats (tests, files, lines)
- Highlights (3-5 bullet points)

### 2. Intro Friendly

```markdown
### 👋 Hey! Here's What Changed

[2-3 phrases conversationnelles expliquant le "pourquoi"]

**TL;DR:** [Une phrase résumé]
```

### 3. Features (si applicable)

```markdown
### ✨ New Features

#### 🎁 Feature Name

**What:** [Explication]
**Why:** [Bénéfice utilisateur]

\`\`\`yaml
# Exemple d'utilisation
\`\`\`

┌─────────────────────────────────────────────────────────────────────────────────┐
│  💡 TIP                                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│  Pro tip actionnable                                                            │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 4. Before/After Diagrams

```
Before v0.X.Y:                          After v0.X.Y:
──────────────────────────────────      ──────────────────────────────────

[Old behavior]                          [New behavior]
    ↓                                       ↓
❌ Problem                               ✅ Solution
```

### 5. Bug Fixes

```markdown
### 🐛 Bug Fixes

#### 🔧 Bug Title

**Problem:** Ce qui ne marchait pas
**Root Cause:** Pourquoi
**Fix:** Ce qu'on a changé

┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔍 TECHNICAL DETAIL                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│  File: src/module/file.rs:123                                                   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 6. Tables de Comparaison

```markdown
| Feature | Before | After | Notes |
|---------|:------:|:-----:|-------|
| Speed   | 100ms  | 10ms  | 10x faster |
```

### 7. Migration Guide (si breaking changes)

```markdown
### 🔄 Migration

\`\`\`bash
cargo install nika --version X.Y.Z
\`\`\`

┌─────────────────────────────────────────────────────────────────────────────────┐
│  ⚠️  BREAKING CHANGE                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│  old_syntax → new_syntax                                                        │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Emoji Reference

| Category | Emoji | Usage |
|----------|-------|-------|
| **Features** | ✨ | New feature |
| | 🎁 | Major feature |
| | 🚀 | Performance |
| | ⚡ | Quick improvement |
| **Bugs** | 🐛 | Bug fix |
| | 🔧 | Technical fix |
| | 🩹 | Hotfix |
| **Status** | ✅ | Success |
| | ❌ | Failure |
| | ⚠️ | Warning |
| | 💡 | Tip |
| | 🔍 | Technical detail |
| **Mascots** | 🦋 | Nika (runtime) |
| | 🐔 | Agent (space chicken) |
| | 🐤 | Subagent (chick) |
| **5 Verbs** | ⚡ | infer: |
| | 📟 | exec: |
| | 🛰️ | fetch: |
| | 🔌 | invoke: |
| | 🐔 | agent: |

---

## Box Styles

### Banner (Major releases)

```
╔══════════════════════════════════════════════════════════════╗
║  Double-line border                                          ║
╚══════════════════════════════════════════════════════════════╝
```

### Info Box (Tips)

```
┌──────────────────────────────────────────────────────────────┐
│  💡 TIP                                                      │
├──────────────────────────────────────────────────────────────┤
│  Content                                                     │
└──────────────────────────────────────────────────────────────┘
```

### Warning Box

```
┌──────────────────────────────────────────────────────────────┐
│  ⚠️  WARNING                                                 │
├──────────────────────────────────────────────────────────────┤
│  Content                                                     │
└──────────────────────────────────────────────────────────────┘
```

### Technical Detail Box

```
┌──────────────────────────────────────────────────────────────┐
│  🔍 TECHNICAL DETAIL                                         │
├──────────────────────────────────────────────────────────────┤
│  Content                                                     │
└──────────────────────────────────────────────────────────────┘
```

---

## Checklist Avant Release

- [ ] Banner avec version, titre, stats, highlights
- [ ] Intro friendly avec TL;DR
- [ ] Features avec exemples YAML et tips
- [ ] Before/After diagrams si pertinent
- [ ] Bug fixes avec problem/cause/fix
- [ ] Tables de comparaison
- [ ] Migration guide si breaking changes
- [ ] Emojis cohérents

---

## Exemple Complet

Voir CHANGELOG.md pour des exemples réels de releases bien formatées.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║    🦋 Use this template for ALL future releases!                              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```
