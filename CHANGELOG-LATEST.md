# Changelog (Latest)

For complete history, see [CHANGELOG.md](./CHANGELOG.md).

## [Unreleased]

## [0.21.0] - 2026-03-05

╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.21.0 — STRUCTURED OUTPUT ENGINE                                   ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  📋 4-Layer Validation  │  🔄 Auto-Retry  │  🔧 LLM Repair  │  📊 3,808 tests ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

### ✨ Highlights

| Feature | Status | Impact |
|---------|--------|--------|
| **📋 Structured Output Engine** | ✅ New | 4-layer defense for ~99.99% JSON Schema compliance |
| **🔄 Auto-Retry with Feedback** | ✅ New | Error messages + schema sent to LLM for correction |
| **🔧 LLM Repair Layer** | ✅ New | Separate repair call as last resort |
| **📊 New Error Codes** | ✅ New | NIKA-300 to NIKA-303 for structured output |

### 🏗️ Architecture: 4-Layer Defense

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  STRUCTURED OUTPUT ENGINE — 4 LAYERS                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Layer 1: rig Extractor ──► Rust type extraction via schemars                  │
│      │                                                                          │
│      ▼ (if fails)                                                               │
│  Layer 2: Provider-native ──► tool_use / response_format                        │
│      │                                                                          │
│      ▼ (if fails)                                                               │
│  Layer 3: Retry with feedback ──► error messages + schema → LLM                 │
│      │                                                                          │
│      ▼ (if fails)                                                               │
│  Layer 4: LLM repair ──► separate repair call with original + errors            │
│                                                                                 │
│  Result: ~99.99% JSON Schema compliance                                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Added

- **📋 Structured Output Engine** — 4-layer defense system
  - **Layer 1**: rig Extractor (Rust type extraction via schemars)
  - **Layer 2**: Provider-native (tool_use / response_format)
  - **Layer 3**: Retry with feedback (error messages + schema)
  - **Layer 4**: LLM repair (separate repair call with original + errors)

- **🔧 `structured:` task field** — Configure validation per task
  ```yaml
  - id: generate
    infer: "Generate user profile"
    structured:
      schema: ./schemas/user.json
      max_retries: 3
      enable_repair: true
  ```

- **⚠️ Error codes NIKA-300-303** — Structured output variants
  | Code | Name | Description |
  |------|------|-------------|
  | NIKA-300 | ExtractionFailed | Parsing failure |
  | NIKA-301 | ValidationFailed | Schema mismatch |
  | NIKA-302 | RepairFailed | Repair LLM failed |
  | NIKA-303 | AllLayersFailed | All layers exhausted |

- **📊 StructuredOutput events** — Observability
  - `StructuredOutputAttempt`: Logs each layer attempt
  - `StructuredOutputRepaired`: Logs successful repairs

- **📁 Example workflow** — `examples/v21-structured-output.nika.yaml`

### Changed

- **Runner integration** — `execute_task_iteration()` validates output when `task.structured` is set

### 📊 Statistics

```
╭─────────────────────────────────────────────────────────────────────────────────╮
│  📊 v0.21.0 METRICS                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  🧪 Tests:        3,808 passing                                                 │
│  📏 Clippy:       Zero warnings (with -D warnings)                              │
│  📦 Crates:       6 workspace crates                                            │
│  🔮 Providers:    7 (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)   │
│  🖥️ TUI Views:    8 views                                                       │
│                                                                                 │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

---

## [0.20.1] - 2026-03-04

╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.20.1 — 8-VIEW TUI + TWO-PHASE IR                                   ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  🖥️ 8 TUI Views  │  🔍 Two-Phase IR  │  🔐 spn Daemon  │  📊 3,808 tests      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

### ✨ Highlights

| Feature | Impact |
|---------|--------|
| **🖥️ 8-View TUI Architecture** | VS Code-inspired unified workspace |
| **🔍 Two-Phase IR** | Raw AST → Analyzed AST pipeline |
| **🔐 spn Daemon Integration** | Unified secret management via Unix socket |
| **🌲 Tree Widget** | VS Code-like file browser |

### Added

- **🖥️ 8-View TUI Architecture**
  - `WorkspaceView`: 3-panel layout (Browser │ Editor │ DAG Preview)
  - `SplitView`: Editor + Runner side-by-side
  - Keyboard shortcuts: `7` for Split, `8` for Workspace

- **🔍 Two-Phase IR Architecture**
  ```
  📄 YAML ──► 🔍 Raw AST (Spanned<T>) ──► ✅ Analyzed AST (validated)
  ```

- **🌲 Tree Widget Integration** — tui-tree-widget v0.24

- **🔐 spn Daemon** — Unified keychain access via Unix socket IPC

---

## [0.19.5] - 2026-03-04

### Fixed
- **simple-exec template** — Changed invalid `shell:` verb to proper `exec: { command, shell: true }` format

---

## [0.19.4] - 2026-03-04

### Added
- **Output Policy for JSON Schema Injection** — Runtime schema enforcement
- **{{inputs.*}} Template Resolution** — Access workflow inputs in templates

### Fixed
- **Benchmark thresholds** — Relaxed for debug builds
- **Execute signature migration** — Updated all tests for 5-argument signature

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.21.0...HEAD
[0.21.0]: https://github.com/supernovae-st/nika/compare/v0.20.1...v0.21.0
[0.20.1]: https://github.com/supernovae-st/nika/compare/v0.20.0...v0.20.1
[0.19.5]: https://github.com/supernovae-st/nika/compare/v0.19.4...v0.19.5
[0.19.4]: https://github.com/supernovae-st/nika/compare/v0.19.3...v0.19.4
