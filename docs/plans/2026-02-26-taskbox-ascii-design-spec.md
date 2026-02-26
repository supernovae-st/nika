# TaskBox & ChatNodeBox ASCII Design Specification

**Version:** v0.11.0
**Date:** 2026-02-26
**Status:** Reference Design for Implementation

---

## Table of Contents

1. [Color Taxonomy](#1-color-taxonomy)
2. [InferBox Design](#2-inferbox-design)
3. [ExecBox Design](#3-execbox-design)
4. [FetchBox Design](#4-fetchbox-design)
5. [InvokeBox Design](#5-invokebox-design)
6. [AgentBox & SpawnBox Design](#6-agentbox--spawnbox-design)
7. [ChatNodeBox Design](#7-chatnodebox-design)
8. [ChatEdgeLine Design](#8-chatedgeline-design)
9. [ChatTaskQueue Design](#9-chattaskqueue-design)
10. [ChatDagPanel Full Example](#10-chatdagpanel-full-example)
11. [BoxState Transitions](#11-boxstate-transitions)
12. [Implementation Checklist](#12-implementation-checklist)

---

## 1. Color Taxonomy

### Verb Colors (VerbColor enum)

| Verb | Icon | Label | Hex | RGB | Use Case |
|------|------|-------|-----|-----|----------|
| Infer | ⚡ | INFER | #8b5cf6 | (139, 92, 246) | LLM text generation |
| Exec | 📟 | EXEC | #f59e0b | (245, 158, 11) | Shell command execution |
| Fetch | 🛰️ | FETCH | #06b6d4 | (6, 182, 212) | HTTP request |
| Invoke | 🔌 | INVOKE | #10b981 | (16, 185, 129) | MCP tool call |
| Agent | 🐔 | AGENT | #f43f5e | (244, 63, 94) | Multi-turn agentic loop |
| Spawn | 🐤 | SPAWN | #fda4af | (253, 164, 175) | Nested agent (spawn_agent) |

### Status Colors

| Status | Hex | RGB | Icon |
|--------|-----|-----|------|
| Success | #22c55e | (34, 197, 94) | ✅ |
| Error | #ef4444 | (239, 68, 68) | ❌ |
| Warning | #f59e0b | (245, 158, 11) | ⚠️ |
| Running | #facc15 | (250, 204, 21) | ⏳ |
| Muted | #64748b | (100, 116, 139) | ⏸️ |
| Info | #3b82f6 | (59, 130, 246) | ℹ️ |

### HTTP Method Colors

| Method | Hex | RGB | Semantic |
|--------|-----|-----|----------|
| GET | #22c55e | (34, 197, 94) | Safe, idempotent |
| POST | #3b82f6 | (59, 130, 246) | Create |
| PUT | #f59e0b | (245, 158, 11) | Replace |
| PATCH | #a855f7 | (168, 85, 247) | Partial update |
| DELETE | #ef4444 | (239, 68, 68) | Destructive |
| HEAD | #64748b | (100, 116, 139) | Metadata only |
| OPTIONS | #64748b | (100, 116, 139) | CORS preflight |

### Exit Code Colors

| Code | Hex | RGB | Meaning |
|------|-----|-----|---------|
| 0 | #22c55e | (34, 197, 94) | Success |
| 126 | #f59e0b | (245, 158, 11) | Permission denied |
| 127 | #f59e0b | (245, 158, 11) | Command not found |
| 128-255 | #ef4444 | (239, 68, 68) | Signal termination |
| Other | #ef4444 | (239, 68, 68) | General error |

---

## 2. InferBox Design

### 2.1 Compact Mode (1 line)

```
Running:
╭─ ⚡ INFER ──────────────────────────────────── ⏳ Running ─╮
│ "Generate a landing page headline for QR Code AI"    [▓▓▓░░] 127 tok │
╰────────────────────────────────────────────────────────────╯

Success:
╭─ ⚡ INFER ──────────────────────────────────── ✅ 2.3s ────╮
│ "Generate a landing page headline..." 847 tokens           │
╰────────────────────────────────────────────────────────────╯
```

**Fields displayed:**
- Verb icon + label
- Truncated prompt (max 50 chars)
- Progress bar (streaming) OR token count (complete)
- Status icon + duration

### 2.2 Expanded Mode (default)

```
Running with streaming:
╭─ ⚡ INFER ──────────────────────────────────── ⏳ Running ─╮
│                                                            │
│  PROMPT                                                    │
│  ┊ Generate a landing page headline for QR Code AI        │
│  ┊ targeting French market. Make it catchy and SEO-       │
│  ┊ friendly.                                               │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  STREAMING                                                 │
│  ┊ Créez des QR codes intelligents qui                    │
│  ┊ transforment votre marketing█                          │
│                                                            │
│  ▁▂▃▅▇█▇▅▃ 42 tok/s │ [▓▓▓▓▓▓░░░░] 127/300               │
╰────────────────────────────────────────────────────────────╯

Success:
╭─ ⚡ INFER ──────────────────────────────────── ✅ 2.3s ────╮
│                                                            │
│  PROMPT                                                    │
│  ┊ Generate a landing page headline...                    │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  RESPONSE                                                  │
│  ┊ Créez des QR codes intelligents qui transforment       │
│  ┊ votre marketing digital en expériences engageantes     │
│                                                            │
│  🧠 Claude │ 847 tokens │ $0.0042                          │
╰────────────────────────────────────────────────────────────╯

Failed:
╭─ ⚡ INFER ──────────────────────────────────── ❌ Failed ──╮
│                                                            │
│  PROMPT                                                    │
│  ┊ Generate content...                                    │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  ERROR                                                     │
│  ┊ [NIKA-030] Provider error: Rate limit exceeded         │
│  ┊ Retry in 30 seconds                                    │
│                                                            │
╰────────────────────────────────────────────────────────────╯
```

**Sections:**
- PROMPT: Full prompt text (collapsible)
- STREAMING: Live token output with cursor + velocity sparkline
- RESPONSE: Final response text
- ERROR: Error message with code
- Footer: Provider icon, token count, cost

### 2.3 Full Mode (all details)

```
╭─ ⚡ INFER ──────────────────────────────────── ✅ 2.3s ────╮
│                                                            │
│  PROMPT                                                    │
│  ┊ Generate a landing page headline for QR Code AI        │
│  ┊ targeting French market. Make it catchy and SEO-       │
│  ┊ friendly. Include keywords: qr code, marketing,        │
│  ┊ digital transformation.                                 │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  RESPONSE                                                  │
│  ┊ Créez des QR codes intelligents qui transforment       │
│  ┊ votre marketing digital en expériences engageantes     │
│  ┊ et mémorables pour vos clients.                        │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  💭 THINKING (Claude Extended)                             │
│  ┊ The user wants a French headline for QR Code AI.       │
│  ┊ Key aspects: SEO-friendly, catchy, includes keywords.  │
│  ┊ I'll focus on transformation and engagement themes...  │
│                                                            │
├────────────────────────────────────────────────────────────┤
│  🧠 Claude │ claude-sonnet-4-20250514                       │
│  input: 234 │ output: 613 │ total: 847 │ $0.0042           │
╰────────────────────────────────────────────────────────────╯
```

**Additional sections in Full mode:**
- THINKING: Claude extended thinking capture
- Detailed token breakdown (input/output/total)
- Full model name

---

## 3. ExecBox Design

### 3.1 Compact Mode

```
Running:
╭─ 📟 EXEC ─────────────────────────────────────── ⏳ ───────╮
│ $ npm run build                                 [████░░░░]│
╰────────────────────────────────────────────────────────────╯

Success (exit 0):
╭─ 📟 EXEC ─────────────────────────────────────── ✅ 0 ────╮
│ $ npm run build                                    4.2s   │
╰────────────────────────────────────────────────────────────╯

Failed (exit 1):
╭─ 📟 EXEC ─────────────────────────────────────── ❌ 1 ────╮
│ $ npm run build                               FAILED 0.8s │
╰────────────────────────────────────────────────────────────╯
```

**Fields:**
- Command with `$` prefix
- Progress bar (running) OR exit code (complete)
- Duration

### 3.2 Expanded Mode

```
Running:
╭─ 📟 EXEC ─────────────────────────────────────── ⏳ Running ╮
│                                                             │
│  $ npm run build                                            │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  STDOUT                                                     │
│  ┊ > nika@0.12.0 build                                     │
│  ┊ > tsc && vite build                                     │
│  ┊ Building for production...                              │
│  ┊ [████████████████░░░░░░░░] 67%                          │
│                                                             │
│  exit: ? │ pid: 12345 │ cwd: .../nika                      │
╰─────────────────────────────────────────────────────────────╯

Success:
╭─ 📟 EXEC ─────────────────────────────────────── ✅ 4.2s ──╮
│                                                             │
│  $ npm run build                                            │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  STDOUT                                                     │
│  ┊ > nika@0.12.0 build                                     │
│  ┊ > tsc && vite build                                     │
│  ┊ ✓ 1247 modules transformed                              │
│  ┊ dist/index.js    145.2 kB │ gzip: 42.1 kB               │
│                                                             │
│  exit: 0 ✓ │ pid: 12345 │ cwd: .../nika                    │
╰─────────────────────────────────────────────────────────────╯

Failed with stderr:
╭─ 📟 EXEC ─────────────────────────────────────── ❌ 0.8s ──╮
│                                                             │
│  $ npm run build                                            │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  STDOUT                                                     │
│  ┊ > nika@0.12.0 build                                     │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  STDERR ⚠️                                                   │
│  ┊ error TS2304: Cannot find name 'TaskBox'                │
│  ┊ error TS2304: Cannot find name 'VerbColor'              │
│                                                             │
│  exit: 1 ✗ │ pid: 12345 │ cwd: .../nika                    │
╰─────────────────────────────────────────────────────────────╯
```

**Sections:**
- Command line with `$` prefix
- STDOUT: Standard output (expandable)
- STDERR: Standard error with ⚠️ indicator (amber color)
- Footer: exit code (colored), pid, truncated cwd

---

## 4. FetchBox Design

### 4.1 Compact Mode

```
Running:
╭─ 🛰️ FETCH ────────────────────────────────────── ⏳ ───────╮
│ GET https://api.example.com/users              [░░░░░░░░] │
╰────────────────────────────────────────────────────────────╯

Success:
╭─ 🛰️ FETCH ────────────────────────────────────── ✅ 200 ──╮
│ GET https://api.example.com/users                  0.4s   │
╰────────────────────────────────────────────────────────────╯

Failed:
╭─ 🛰️ FETCH ────────────────────────────────────── ❌ 404 ──╮
│ GET https://api.example.com/missing           Not Found   │
╰────────────────────────────────────────────────────────────╯
```

### 4.2 Expanded Mode

```
Running:
╭─ 🛰️ FETCH ────────────────────────────────────── ⏳ Running ╮
│                                                             │
│  [GET]  https://api.example.com/v1/users?limit=10          │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  REQUEST                                                    │
│  ┊ Authorization: Bearer sk-***...xyz                      │
│  ┊ Content-Type: application/json                          │
│                                                             │
│  ⏳ Waiting for response...                                 │
╰─────────────────────────────────────────────────────────────╯

Success:
╭─ 🛰️ FETCH ────────────────────────────────────── ✅ 0.4s ──╮
│                                                             │
│  [GET]  https://api.example.com/v1/users?limit=10          │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  RESPONSE   200 OK                                          │
│  ┊ {                                                        │
│  ┊   "users": [                                             │
│  ┊     { "id": 1, "name": "Alice" },                        │
│  ┊     { "id": 2, "name": "Bob" }                           │
│  ┊   ],                                                     │
│  ┊   "total": 42                                            │
│  ┊ }                                                        │
│                                                             │
│  200 OK │ 1.2 KB │ 423ms                                    │
╰─────────────────────────────────────────────────────────────╯

Failed:
╭─ 🛰️ FETCH ────────────────────────────────────── ❌ 0.1s ──╮
│                                                             │
│  [POST]  https://api.example.com/v1/auth                   │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  RESPONSE   401 Unauthorized                                │
│  ┊ {                                                        │
│  ┊   "error": "Invalid API key",                            │
│  ┊   "code": "AUTH_FAILED"                                  │
│  ┊ }                                                        │
│                                                             │
│  401 Unauthorized │ 89 B │ 102ms                            │
╰─────────────────────────────────────────────────────────────╯
```

### 4.3 HTTP Method Badge Colors

| Badge | Color | Hex |
|-------|-------|-----|
| `[GET]` | Green | #22c55e |
| `[POST]` | Blue | #3b82f6 |
| `[PUT]` | Amber | #f59e0b |
| `[PATCH]` | Purple | #a855f7 |
| `[DELETE]` | Red | #ef4444 |
| `[HEAD]` | Gray | #64748b |
| `[OPTIONS]` | Gray | #64748b |

---

## 5. InvokeBox Design

### 5.1 Compact Mode

```
Running:
╭─ 🔌 INVOKE ───────────────────────────────────── ⏳ ───────╮
│ novanet::novanet_generate                      [░░░░░░░░] │
╰────────────────────────────────────────────────────────────╯

Success:
╭─ 🔌 INVOKE ───────────────────────────────────── ✅ 1.2s ─╮
│ novanet::novanet_generate                      {3 fields} │
╰────────────────────────────────────────────────────────────╯

Retrying:
╭─ 🔌 INVOKE ───────────────────────────────────── ❌ ──────╮
│ novanet::novanet_generate                    🔄 retry 2/3 │
╰────────────────────────────────────────────────────────────╯
```

### 5.2 Expanded Mode

```
Running:
╭─ 🔌 INVOKE ───────────────────────────────────── ⏳ Running ╮
│                                                             │
│  novanet :: novanet_generate                                │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  PARAMS                                                     │
│  ┊ {                                                        │
│  ┊   "entity": "qr-code",                                   │
│  ┊   "locale": "fr-FR",                                     │
│  ┊   "forms": ["text", "title", "abbrev"]                   │
│  ┊ }                                                        │
│                                                             │
│  ⏳ Calling MCP server...                                   │
╰─────────────────────────────────────────────────────────────╯

Success:
╭─ 🔌 INVOKE ───────────────────────────────────── ✅ 1.2s ──╮
│                                                             │
│  novanet :: novanet_generate                                │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  PARAMS ▶                                                   │
│  ┊ entity: "qr-code" │ locale: "fr-FR" │ +1 more           │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  RESULT                                                     │
│  ┊ {                                                        │
│  ┊   "text": "QR Code",                                     │
│  ┊   "title": "Code QR",                                    │
│  ┊   "description": "Un code-barres bidimensionnel..."      │
│  ┊ }                                                        │
│                                                             │
╰─────────────────────────────────────────────────────────────╯

Failed with retry:
╭─ 🔌 INVOKE ───────────────────────────────────── ❌ ───────╮
│                                                             │
│  novanet :: novanet_generate           🔄 retry 2/3        │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  PARAMS ▶                                                   │
│  ┊ entity: "qr-code" │ locale: "fr-FR"                     │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  ERROR   -32602 Invalid params                              │
│  ┊ Missing required field: 'forms'                          │
│                                                             │
│  ├─ Attempt 1: -32602 Invalid params (0.1s)                │
│  └─ Attempt 2: -32602 Invalid params (0.1s)                │
│                                                             │
╰─────────────────────────────────────────────────────────────╯
```

### 5.3 RetryBadge Component

```
┌─────────────────────────────────────────────────────────────┐
│  RETRY BADGE STATES                                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  No retries:     (no badge displayed)                       │
│  Retrying:       🔄 retry 1/3                               │
│  Retrying:       🔄 retry 2/3                               │
│  Final attempt:  🔄 retry 3/3 (last)                        │
│  All failed:     ❌ failed after 3 attempts                 │
│                                                             │
│  Error history:                                             │
│  ├─ Attempt 1: -32602 Invalid params (0.1s)                │
│  ├─ Attempt 2: -32603 Internal error (0.5s)                │
│  └─ Attempt 3: -32000 Server error (1.2s)                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. AgentBox & SpawnBox Design

### 6.1 Compact Mode

```
Agent running:
╭─ 🐔 AGENT ────────────────────────────────────── ⏳ ───────╮
│ "Generate landing page"                  turn 3/10 │ 2 tools│
╰────────────────────────────────────────────────────────────╯

Spawn running:
╭─ 🐤 SPAWN ────────────────────────────────────── ⏳ ───────╮
│ "Generate header section"              depth 1/3 │ turn 2  │
╰────────────────────────────────────────────────────────────╯
```

### 6.2 Expanded Mode - AgentBox

```
╭─ 🐔 AGENT ────────────────────────────────────── ⏳ turn 3/10 ╮
│                                                               │
│  GOAL                                                         │
│  ┊ Generate a complete landing page for QR Code AI           │
│  ┊ with header, features section, and CTA.                   │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  TURN 1                                               0.8s    │
│  ┊ 💭 I need to get entity context first...                  │
│  ┊ 🔌 novanet_generate(entity: "qr-code", locale: "fr-FR")   │
│  ┊ ✅ Retrieved entity context with 3 forms                  │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  TURN 2                                               1.2s    │
│  ┊ 💭 Now I'll generate the header section...                │
│  ┊ ⚡ Generating header HTML...                               │
│  ┊ ✅ Header complete with headline and subheadline          │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  TURN 3 (current)                                     ⏳      │
│  ┊ 💭 Working on features section...█                        │
│                                                               │
│  🧠 Claude │ 2,341 tokens │ 2 MCP tools │ depth 0/3          │
╰───────────────────────────────────────────────────────────────╯
```

### 6.3 AgentBox with Nested Spawn

```
╭─ 🐔 AGENT ────────────────────────────────────── ⏳ turn 4/10 ╮
│                                                               │
│  GOAL                                                         │
│  ┊ Generate a complete landing page...                       │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  TURN 3                                               1.5s    │
│  ┊ 💭 Features section is complex, spawning sub-agent...     │
│  ┊ 🐤 spawn_agent("Generate features section")               │
│                                                               │
│  ╭─ 🐤 SPAWN ─────────────────────────── ⏳ turn 2/5 ╮        │
│  │                                                   │        │
│  │  GOAL: Generate features section                  │        │
│  │                                                   │        │
│  │  TURN 1 ✅ Retrieved feature list                │        │
│  │  TURN 2 ⏳ Generating feature cards...█          │        │
│  │                                                   │        │
│  │  depth 1/3 │ 847 tokens                          │        │
│  ╰───────────────────────────────────────────────────╯        │
│                                                               │
│  🧠 Claude │ 3,188 tokens │ depth 0/3                        │
╰───────────────────────────────────────────────────────────────╯
```

### 6.4 AgentBox Success State

```
╭─ 🐔 AGENT ────────────────────────────────────── ✅ 12.3s ───╮
│                                                               │
│  GOAL                                                         │
│  ┊ Generate a complete landing page...                       │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  SUMMARY   5 turns │ 2 spawns │ 4 tool calls                  │
│                                                               │
│  ├─ Turn 1: Retrieved entity context (🔌 novanet_generate)   │
│  ├─ Turn 2: Generated header section                         │
│  ├─ Turn 3: Spawned features sub-agent (🐤)                  │
│  ├─ Turn 4: Spawned CTA sub-agent (🐤)                       │
│  └─ Turn 5: Assembled final page                             │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  RESULT                                                       │
│  ┊ <!DOCTYPE html>                                            │
│  ┊ <html lang="fr">                                           │
│  ┊ <head>...                                                  │
│  ┊ [+142 lines]                                               │
│                                                               │
│  🧠 Claude │ 8,234 tokens │ $0.041 │ 5 turns │ depth 0        │
╰───────────────────────────────────────────────────────────────╯
```

### 6.5 Turn History Components

```
┌─────────────────────────────────────────────────────────────┐
│  TURN ENTRY FORMATS                                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Thinking:                                                  │
│  ┊ 💭 I need to analyze the requirements first...          │
│                                                             │
│  Tool call:                                                 │
│  ┊ 🔌 novanet_generate(entity: "qr-code", locale: "fr-FR") │
│                                                             │
│  Inference:                                                 │
│  ┊ ⚡ Generating header HTML...                             │
│                                                             │
│  Spawn:                                                     │
│  ┊ 🐤 spawn_agent("Generate features section")             │
│                                                             │
│  Fetch:                                                     │
│  ┊ 🛰️ GET https://api.example.com/data                      │
│                                                             │
│  Exec:                                                      │
│  ┊ 📟 npm run build                                        │
│                                                             │
│  Success:                                                   │
│  ┊ ✅ Retrieved entity context with 3 forms                │
│                                                             │
│  Error:                                                     │
│  ┊ ❌ MCP server timeout after 30s                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. ChatNodeBox Design

### 7.1 Four Node Kinds

```
USER NODE (Blue #3b82f6):
╭─ 👤 USER ──────────────────────────────────────── N1 ─╮
│                                                        │
│  Generate a landing page for QR Code AI targeting     │
│  the French market with SEO-optimized content.        │
│                                                        │
╰────────────────────────────────────────────────────────╯

ASSISTANT NODE (Green #22c55e):
╭─ 🤖 ASSISTANT ─────────────────────────────────── N2 ─╮
│                                                        │
│  I'll help you create a landing page. Let me first    │
│  gather context about QR codes from the knowledge     │
│  graph, then generate SEO-optimized French content.   │
│                                                        │
│  refs: @N1                                             │
╰────────────────────────────────────────────────────────╯

TOOL NODE (Amber #f59e0b):
╭─ 🔧 TOOL ────────────────────────────────────────── N3 ─╮
│                                                          │
│  novanet_generate                                        │
│  ┊ entity: "qr-code"                                    │
│  ┊ locale: "fr-FR"                                      │
│  ┊ forms: ["text", "title"]                             │
│                                                          │
│  refs: @N2                                               │
╰──────────────────────────────────────────────────────────╯

SYSTEM NODE (Gray #64748b):
╭─ ⚙️ SYSTEM ───────────────────────────────────────── N0 ─╮
│                                                          │
│  You are Nika, an AI workflow assistant. You have      │
│  access to MCP tools for knowledge graph operations.   │
│                                                          │
╰──────────────────────────────────────────────────────────╯
```

### 7.2 Four Node States

```
QUEUED (dashed border, muted color):
╭┄┄ 🔧 TOOL ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄ N5 ┄╮
┊  novanet_traverse (pending)                           ┊
╰┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄╯

STREAMING (pulsing border, verb color):
╭─ 🤖 ASSISTANT ─────────────────────────────── ⏳ N4 ─╮
│                                                       │
│  Voici votre page d'atterrissage optimisée pour      │
│  le référencement français...█                        │
│                                                       │
│  ▁▂▃▅▇█▇▅▃ 38 tok/s                                  │
╰───────────────────────────────────────────────────────╯
    ↑ border pulses with animation          ↑ cursor blinks

COMPLETE (solid border, success green):
╭─ 🤖 ASSISTANT ────────────────────────────── ✅ N4 ─╮
│                                                       │
│  Voici votre page d'atterrissage optimisée pour      │
│  le référencement français. Elle inclut un header    │
│  accrocheur et des sections features/CTA.            │
│                                                       │
│  847 tokens │ 2.3s                                    │
╰───────────────────────────────────────────────────────╯

FAILED (solid border, error red):
╭─ 🔧 TOOL ──────────────────────────────────── ❌ N3 ─╮
│                                                       │
│  novanet_generate                                     │
│  ┊ ERROR: -32602 Invalid params                      │
│  ┊ Missing required field 'locale'                   │
│                                                       │
╰───────────────────────────────────────────────────────╯
```

---

## 8. ChatEdgeLine Design

### 8.1 Edge Connections

```
╭─ 👤 USER ──────────────────────────────────── N1 ─╮
│  Generate a landing page for QR Code AI           │
╰───────────────────────────────────────────────────╯
                    │
                    │ ← Bezier curve (verb color)
                    ▼
╭─ 🤖 ASSISTANT ─────────────────────────────── N2 ─╮
│  I'll help you create a landing page...           │
│  refs: @N1                                        │
╰───────────────────────────────────────────────────╯
        │                     │
        │                     │ ← Multiple edges from one node
        ▼                     ▼
╭─ 🔧 TOOL ─── N3 ─╮   ╭─ 🔧 TOOL ─── N4 ─╮
│  novanet_generate │   │  novanet_traverse│
│  refs: @N2        │   │  refs: @N2       │
╰───────────────────╯   ╰──────────────────╯
        │                     │
        └──────────┬──────────┘
                   │ ← Edges merge at target
                   ▼
╭─ 🤖 ASSISTANT ─────────────────────────────── N5 ─╮
│  Based on the context from @N3 and structure      │
│  from @N4, here's your landing page...            │
│  refs: @N3, @N4                                   │
╰───────────────────────────────────────────────────╯
```

### 8.2 Edge Rendering Rules

| Source Kind | Target Kind | Edge Color | Style |
|-------------|-------------|------------|-------|
| USER | ASSISTANT | Blue #3b82f6 | Solid |
| ASSISTANT | TOOL | Amber #f59e0b | Solid |
| TOOL | ASSISTANT | Green #22c55e | Dashed |
| SYSTEM | * | Gray #64748b | Dotted |

---

## 9. ChatTaskQueue Design

### 9.1 Queue Layout

```
╭─ TASKS ──────────────────────────────────────────────────────╮
│                                                               │
│  🔥 HOT (currently executing)                                 │
│  ├─ ⚡ infer: "Generate headline"              [▓▓▓▓░░] 67%  │
│                                                               │
│  🌡️ WARM (ready to execute)                                   │
│  ├─ 🔌 invoke: novanet_generate                 ⏳ waiting    │
│  └─ 📟 exec: "npm run build"                    ⏳ waiting    │
│                                                               │
│  📋 QUEUED (dependencies pending)                             │
│  ├─ 🛰️ fetch: api.example.com                   deps: [1,2]  │
│  ├─ 🐔 agent: "Generate page"                   deps: [3]    │
│  └─ ⚡ infer: "Polish content"                  deps: [4]    │
│                                                               │
│  ────────────────────────────────────────────────────────────│
│  Total: 6 tasks │ 1 running │ 2 ready │ 3 queued             │
╰───────────────────────────────────────────────────────────────╯
```

### 9.2 Queue Categories

| Category | Icon | Color | Description |
|----------|------|-------|-------------|
| HOT | 🔥 | Red #ef4444 | Currently executing |
| WARM | 🌡️ | Amber #f59e0b | Ready, waiting for slot |
| QUEUED | 📋 | Gray #64748b | Blocked by dependencies |

---

## 10. ChatDagPanel Full Example

```
╭─ ⚙️ SYSTEM ────────────────────────────────────────────── N0 ─╮
│  You are Nika, an AI workflow assistant with MCP access.     │
╰───────────────────────────────────────────────────────────────╯
                                │
                                ▼
╭─ 👤 USER ──────────────────────────────────────────────── N1 ─╮
│  Generate a landing page for QR Code AI in French.           │
╰───────────────────────────────────────────────────────────────╯
                                │
                                ▼
╭─ 🤖 ASSISTANT ─────────────────────────────────────────── N2 ─╮
│  I'll create a French landing page. First, let me get the   │
│  entity context from the knowledge graph.                    │
╰───────────────────────────────────────────────────────────────╯
                                │
                ┌───────────────┼───────────────┐
                ▼               ▼               ▼
╭─ 🔌 INVOKE ─ N3 ─╮ ╭─ 🔌 INVOKE ─ N4 ─╮ ╭─ 🛰️ FETCH ── N5 ─╮
│ novanet_generate │ │ novanet_traverse│ │ GET /seo-kw    │
│ ✅ 1.2s          │ │ ✅ 0.8s         │ │ ✅ 200 OK 0.3s │
╰──────────────────╯ ╰─────────────────╯ ╰─────────────────╯
                │               │               │
                └───────────────┼───────────────┘
                                ▼
╭─ 🐔 AGENT ──────────────────────────────────── ⏳ turn 3/10 N6 ─╮
│                                                                 │
│  GOAL: Generate complete landing page with all sections        │
│                                                                 │
│  TURN 1 ✅ Analyzed context from @N3, @N4, @N5                 │
│  TURN 2 ✅ Generated header section                            │
│  TURN 3 ⏳ Spawning sub-agent for features...                  │
│                                                                 │
│  ╭─ 🐤 SPAWN ────────────────────────── ⏳ turn 2/5 N6.1 ─╮    │
│  │  Generate features section with 4 feature cards       │    │
│  │  ⏳ Working on feature cards...█                      │    │
│  ╰───────────────────────────────────────────────────────╯    │
│                                                                 │
│  🧠 Claude │ 2,847 tok │ depth 0/3 │ refs: @N3, @N4, @N5       │
╰─────────────────────────────────────────────────────────────────╯

════════════════════════════════════════════════════════════════════
TASK QUEUE
├─ 🔥 🐔 agent: N6 (turn 3/10)
├─ 🔥 🐤 spawn: N6.1 (turn 2/5)
└─ 📋 ⚡ infer: "Polish final content" (deps: N6)
════════════════════════════════════════════════════════════════════
```

---

## 11. BoxState Transitions

### 11.1 State Machine

```
                ┌──────────────────────────────────────┐
                │                                      │
                ▼                                      │
          ┌──────────┐                                 │
          │  QUEUED  │ ─────────────────────────┐      │
          │   ⏸️     │                          │      │
          └────┬─────┘                          │      │
               │                                │      │
               │ TaskStarted                    │      │
               ▼                                │      │
          ┌──────────┐                          │      │
   ┌──────│ RUNNING  │──────┐                   │      │
   │      │   ⏳     │      │                   │ Skip │
   │      └──────────┘      │                   │      │
   │                        │                   │      │
   │ TaskCompleted          │ TaskFailed        │      │
   ▼                        ▼                   ▼      │
┌──────────┐          ┌──────────┐        ┌──────────┐ │
│ SUCCESS  │          │  FAILED  │        │ SKIPPED  │ │
│   ✅     │          │    ❌    │        │   ⏭️      │ │
└──────────┘          └────┬─────┘        └──────────┘ │
                           │                           │
                           │ Retry                     │
                           └───────────────────────────┘
```

### 11.2 Visual Encoding by State

| State | Border Style | Border Color | Fill | Animation |
|-------|--------------|--------------|------|-----------|
| Queued | Dashed | Muted #64748b | None | None |
| Running | Solid | Verb color | None | Pulse (0.5-1.0 intensity) |
| Success | Solid | Green #22c55e | None | None |
| Failed | Solid | Red #ef4444 | None | None |
| Skipped | Dashed | Gray #64748b | None | None |

---

## 12. Implementation Checklist

### 12.1 Core Components

| Component | File | Status | Tests |
|-----------|------|--------|-------|
| VerbColor enum | `theme.rs` | ✅ Exists | ✅ |
| BoxState enum | `state.rs` | ✅ Exists | ✅ |
| RenderMode enum | `mod.rs` | ✅ Exists | ✅ |
| InferBox | `infer.rs` | ✅ Exists | ✅ |
| ExecBox | `exec.rs` | ✅ Exists | ✅ |
| FetchBox | `fetch.rs` | ⏳ Partial | ⏳ |
| InvokeBox | `invoke.rs` | ⏳ Partial | ⏳ |
| AgentBox | `agent.rs` | ✅ Exists | ✅ |
| SpawnBox | `agent.rs` | ⏳ Partial | ⏳ |

### 12.2 New Components Needed

| Component | Description | Priority |
|-----------|-------------|----------|
| TokenVelocity | Ring buffer for tok/s sparkline | HIGH |
| RetryBadge | MCP retry tracking | HIGH |
| HttpMethodBadge | Colored method badge | MEDIUM |
| TurnHistory | Agent turn visualization | HIGH |
| ChatNodeBox | DAG node widget | HIGH |
| ChatEdgeLine | Bezier edge connector | MEDIUM |
| ChatTaskQueue | Task queue panel | MEDIUM |
| ChatDagPanel | Full DAG visualization | HIGH |

### 12.3 Render Modes per Widget

| Widget | Compact | Expanded | Full |
|--------|---------|----------|------|
| InferBox | ✅ | ✅ | ✅ |
| ExecBox | ✅ | ✅ | ⏳ |
| FetchBox | ⏳ | ⏳ | ⏳ |
| InvokeBox | ⏳ | ⏳ | ⏳ |
| AgentBox | ⏳ | ✅ | ⏳ |
| SpawnBox | ⏳ | ⏳ | ⏳ |

### 12.4 Animation Support

| Animation | Implementation | Status |
|-----------|----------------|--------|
| Border pulse | `pulse_intensity` field | ✅ |
| Cursor blink | Timer-based toggle | ⏳ |
| Progress bar | Width calculation | ✅ |
| Sparkline | Ring buffer render | ⏳ |
| State transition | Easing functions | ✅ |

---

## References

- **Implementation Plan:** `2026-02-26-taskbox-v0.11-implementation-plan.md`
- **VerbColor Source:** `src/tui/theme.rs`
- **BoxState Source:** `src/tui/widgets/task_box/state.rs`
- **Animation System:** `src/tui/widgets/animation.rs`
- **Event Wiring:** Appendix A in implementation plan

---

*This document serves as the canonical visual reference for TaskBox and ChatNodeBox implementations.*
