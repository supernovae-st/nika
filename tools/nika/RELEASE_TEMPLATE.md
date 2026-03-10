# Nika Release Template

Ce template est la référence officielle pour toutes les releases GitHub de Nika.

---

## Structure

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║  🦋  N I K A   v X . Y . Z                                                    ║
║                                                                               ║
║  [Release Title - One Line Description]                                       ║
║                                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  📊 Stats                                                                     ║
║  Tests: X,XXX passing  │  Files: XX changed  │  +X,XXX lines                  ║
║                                                                               ║
║  🎯 Highlights                                                                ║
║  ├── ✨/🐛/⚡ Highlight 1                                                     ║
║  ├── ✨/🐛/⚡ Highlight 2                                                     ║
║  └── ✨/🐛/⚡ Highlight 3                                                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

### 🚀 What's New

> **TL;DR:** [Punchy one-liner summary with emoji] 😎

#### 🔥 The Big [Features/Fixes/Changes]

| Feature | Before | After |
|---------|--------|-------|
| Feature 1 | Old behavior 🤦 | New behavior ✨ |
| Feature 2 | Old behavior | New behavior |

#### 📋 Detailed Changes

**✨ New Features** (si applicable)
- **Feature Name** — Description détaillée

**🐛 Bug Fixes** (si applicable)
- **Bug Name** — Description détaillée

**⚡ Performance** (si applicable)
- **Improvement** — Description

**🚨 New Error Codes** (si applicable)
- **NIKA-XXX** — Description

---

### 🏗️ Architecture Changes (si applicable)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  [DIAGRAM TITLE]                                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   [ASCII diagram showing the change]                                            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

### 🐛 Bug Fixes (Before/After) (si applicable)

**[Bug Name]** — Description

```
Before:                           After:
─────────────────────────         ─────────────────────────
Old behavior ❌                   New behavior ✅
```

---

### ⚡ Performance (section standalone si applicable)

- **Improvement 1** — Description
- **Improvement 2** — Description

---

### 🚨 New Error Codes (si applicable)

| Code | Description | When |
|------|-------------|------|
| NIKA-XXX | Description | Condition |

---

### 📦 Install

```bash
brew upgrade nika
# or
cargo install nika
```

| Platform | File |
|----------|------|
| 🍎 macOS (Apple Silicon) | `nika-macos-arm64-X.Y.Z.tar.gz` |
| 🍎 macOS (Intel) | `nika-macos-x64-X.Y.Z.tar.gz` |
| 🐧 Linux (x64) | `nika-linux-x64-X.Y.Z.tar.gz` |
| 🐧 Linux (ARM64) | `nika-linux-arm64-X.Y.Z.tar.gz` |

---

<p align="center">
  <b>🦋 Happy workflows!</b><br>
  <i>[Custom message adapté à la release]</i><br><br>
  Made with 💜 by <a href="https://supernovae.studio">SuperNovae Studio</a>
</p>

---

## Guidelines

### Emojis par type
- ✨ New feature
- 🐛 Bug fix
- ⚡ Performance
- 🚨 Breaking change / Error codes
- 🔧 Refactor
- 📚 Documentation
- 🎨 UI/UX

### Ton
- Punchy et fun dans le TL;DR
- Technique mais accessible dans les détails
- Emojis pour rendre vivant
- Before/After pour les fixes importants
- Diagrammes ASCII pour les changements d'architecture

### Sections optionnelles
- 🏗️ Architecture Changes — Seulement si changement majeur
- 🚨 Error Codes — Seulement si nouveaux codes
- Before/After diagrams — Seulement pour les fixes significatifs

### Messages de fin (exemples)
- Bug fix release: "This release squashes bugs that were bugging us."
- Feature release: "New superpowers unlocked! Your workflows just got smarter."
- Performance: "Speed demons rejoice! Everything is faster now."
- Major: "A giant leap for workflow-kind."
