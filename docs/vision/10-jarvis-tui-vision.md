# 10 — Nika v0.30 TUI: The Jarvis Vision

> The complete visual design specification for Nika's shaka-mode TUI.
> Every panel, every data point, every color — designed to feel like piloting an AI.

**Nika** v0.30 · **NovaNet** v0.20.0 · Updated 2026-03-14

---

## The Cockpit — Full Screen Layout

**Scenario:** Round 4/8 of `generate-multilingual.nika.yaml`. Generating fr-FR landing page for QR Code AI. Three models active simultaneously. NovaNet connected. Records flowing. Shaka making live decisions.

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ 🦋 NIKA v0.30                generate-multilingual.nika.yaml                                                ┃
┃ ┈ Shaka Mode ┈              Goal: Landing pages × 5 locales        Round 4/8  ◉ 0.91  $0.037  ⏱ 47.2s    ┃
┣━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃ 🎯 SHAKA         ┃  📊 LIVE DAG                                                ┃  🧠 NOVANET              ┃
┃ COMMANDER        ┃                                                              ┃  INTELLIGENCE            ┃
┃──────────────────┃  ┌────────┐                              ┌──────────┐       ┃──────────────────────────┃
┃                  ┃  │🔌 ctx  │─────────────┬───────────────▶│✍️ hero   │       ┃  ◆ Entity: qr-code-ai    ┃
┃  Round: ④/8      ┃  │novanet │             │                │ Claude   │       ┃  ├─ text: "QR Code AI"   ┃
┃  State: EXEC     ┃  │✅ 1.2s │             │                │🧠thinking│       ┃  ├─ title: "QR Code AI"  ┃
┃  Quality: 0.91   ┃  │500 tok │             │                │⏳ 4.1s   │       ┃  ├─ abbrev: "QRCAI"      ┃
┃  ──────────────  ┃  └────────┘             │                │█████░░░░ │       ┃  └─ url: "qr-code-ai"    ┃
┃                  ┃  ┌────────┐             │                └─────┬────┘       ┃                          ┃
┃  Budget:         ┃  │🔌 know │─────────────┤                      │            ┃  🏳️ Locale: fr-FR         ┃
┃  ┌────────────┐  ┃  │novanet │             │                ┌─────▼────┐       ┃  ├─ register: formal     ┃
┃  │████████░░░ │  ┃  │✅ 0.8s │             │                │✍️ feat   │       ┃  └─ audience: B2B        ┃
┃  │ 12.1K/15K  │  ┃  │400 tok │             │                │ Claude   │       ┃                          ┃
┃  └────────────┘  ┃  └────────┘             │                │ (queued) │       ┃  📚 Knowledge Atoms:     ┃
┃                  ┃  ┌────────┐             │                └─────┬────┘       ┃  ┌─ expressions ────────┐┃
┃  Models:         ┃  │🧠 mem  │─────────────┘                      │            ┃  │ "code QR" (not "QR   ┃
┃  ◉ pythagoras    ┃  │recall  │        ┌──────────┐          ┌─────▼────┐       ┃  │  code" in French)    ┃
┃  ◉ edison        ┃  │✅ 0.3s │        │🔍research│          │✍️ price  │       ┃  │ "flash code" (alt)   ┃
┃  ◉ york          ┃  │2 found │───────▶│ Groq     │          │ (queued) │       ┃  │ "générer" > "créer"  ┃
┃  ○ atlas         ┃  └────────┘        │⏳ 2.3s   │          └─────┬────┘       ┃  └──────────────────────┘┃
┃                  ┃                     │████░░░░░ │          ┌─────▼────┐       ┃  ┌─ taboos ─────────────┐┃
┃  ── Decisions ── ┃                     └──────────┘          │🔬review  │       ┃  │ ⚠ "gratuit" in heads ┃
┃  R1 ✅ get ctx   ┃                           │               │ Claude   │       ┃  │   (implies low qual) ┃
┃  R2 ✅ get know  ┃                           │               │+thinking │       ┃  │ ⚠ informal "tu" form ┃
┃  R3 ✅ recall    ┃                           └──────────────▶│ (wait)   │       ┃  │   (B2B = "vous")     ┃
┃  R4 ⏳ hero+rsch ┃                                           └──────────┘       ┃  └──────────────────────┘┃
┃  R5 ○ features   ┃  ── Active with: bindings ──                                ┃  ┌─ audience traits ────┐┃
┃  R6 ○ pricing    ┃  hero ← { entity: EP-1, knowledge: EP-2, research: EP-3 }   ┃  │ formal tone          ┃
┃  R7 ○ faq        ┃  research ← { locale: "fr-FR", past: EP-recall<2> }         ┃  │ data-driven args     ┃
┃  R8 ○ review     ┃                                                              ┃  │ metric > anecdote    ┃
┃                  ┃  ── Parallel execution ──                                    ┃  └──────────────────────┘┃
┃  Thinking:       ┃  🔍 research [Groq/york]      ████░░░░░ 43%                 ┃                          ┃
┃  "I should run   ┃  ✍️  hero [Claude/edison+🧠]    █████░░░░ 55%                 ┃  🔗 Graph Neighbors:    ┃
┃   hero & research┃                                                              ┃  qr-code-ai             ┃
┃   in parallel    ┃  ── Structured Output ──                                     ┃  ├──HAS_NATIVE──▶ fr-FR ┃
┃   since they     ┃  hero → schema: { headline: str, subheadline: str,           ┃  ├──HAS_NATIVE──▶ en-US ┃
┃   don't depend   ┃                    cta_text: str, body: str }                ┃  ├──HAS_PAGE────▶ /home ┃
┃   on each other" ┃  Status: ⏳ Layer 1 (rig extractor) running...               ┃  ├──HAS_KEYWORD─▶ "qr"  ┃
┃                  ┃                                                              ┃  └──ENTITY_OF───▶ SaaS  ┃
┣━━━━━━━━━━━━━━━━━━╋━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╋━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃ 📦 RECORDS       ┃  🔬 TASK INSPECTOR ─ hero (write_section)                    ┃  📈 METRICS             ┃
┃──────────────────┃──────────────────────────────────────────────────────────────┃──────────────────────────┃
┃                  ┃                                                              ┃                          ┃
┃ ┌─ EP-1 ────────┐┃  Task: hero (write_section template)                        ┃  💰 Cost Breakdown:     ┃
┃ │ entity_context ┃  Verb: infer ⚡                                              ┃  ┌──────────────────────┐┃
┃ │ 500 tok ✅     ┃  Model: Claude claude-sonnet-4-6 (edison slot)                ┃  │ pythagoras $0.018    ┃
┃ │ src: novanet   ┃  Extended Thinking: ON (budget: 16384)                      ┃  │ edison     $0.012    ┃
┃ │ fields: ctx    ┃                                                              ┃  │ york       $0.004    ┃
┃ └────────────────┘┃  ── with: Bindings (resolved) ──────────────────────       ┃  │ atlas      $0.003    ┃
┃ ┌─ EP-2 ────────┐┃  │ alias          │ source          │ value preview  │       ┃  │────────────────────│┃
┃ │ knowledge      ┃  │────────────────│─────────────────│────────────────│       ┃  │ TOTAL      $0.037   ┃
┃ │ 400 tok ✅     ┃  │ entity         │ "$get_ctx"      │ {name: "QR..  │       ┃  └──────────────────────┘┃
┃ │ src: novanet   ┃  │ knowledge      │ "$get_know"     │ {expr: 12,..  │       ┃                          ┃
┃ │ expr:12 tab:3  ┃  │ research       │ "$research"     │ (⏳ pending)   │       ┃  📊 Token Usage:        ┃
┃ └────────────────┘┃  │ locale         │ "$ctx.locale"   │ "fr-FR"       │       ┃  ┌──────────────────────┐┃
┃ ┌─ EP-3 ────────┐┃  Transforms: knowledge | extract(expressions) | first(5)   ┃  │ Total In:   18,420   ┃
┃ │ research  ⏳   ┃                                                              ┃  │ Total Out:   4,890   ┃
┃ │ ~400 tok       ┃  ── structured: Output Schema ──────────────────────        ┃  │ Thinking:    8,200   ┃
┃ │ tgt: 400 max   ┃  {                                                          ┃  │ Records:     2,200   ┃
┃ │ retain: keys   ┃    "headline": { "type": "string" },      ← required       ┃  │────────────────────│┃
┃ └────────────────┘┃    "subheadline": { "type": "string" },                    ┃  │ Saved by EP: 74%    ┃
┃ ┌─ EP-R1 ←🧠 ──┐┃    "cta_text": { "type": "string" },     ← required       ┃  └──────────────────────┘┃
┃ │ RECALLED       ┃    "body": { "type": "string" }           ← required       ┃                          ┃
┃ │ past research  ┃  }                                                          ┃  🧠 NovaNet Memory:     ┃
┃ │ 280 tok        ┃  Validation: Layer 1 ⏳ │ Layer 2 ○ │ Layer 3 ○ │ L4 ○     ┃  ┌──────────────────────┐┃
┃ │ 2025-03-12     ┃  Max retries: 3  │  Repair model: atlas                  ┃  │ Recalled: 2 records  ┃
┃ │ entity: qr-code┃                                                              ┃  │ Stored:   1 record   ┃
┃ └────────────────┘┃  ── record: Config ────────────────────────────────        ┃  │ Entity: qr-code-ai   ┃
┃ ┌─ EP-R2 ←🧠 ──┐┃  compress: true │ max_tokens: 800 │ retain: [content]       ┃  │ Locales: fr-FR (1/5) ┃
┃ │ RECALLED       ┃  persist: novanet │ entity_link: qr-code-ai                  ┃  │ CSR: 0.96 (healthy)  ┃
┃ │ fr-FR trends   ┃  confidence_threshold: ─ (no threshold for writing)          ┃  └──────────────────────┘┃
┃ │ 190 tok        ┃                                                              ┃                          ┃
┃ │ 2026-02-28     ┃  ── context_budget ─────────────────────────────────        ┃  📉 Quality Trend:      ┃
┃ │ entity: qr-code┃  ┌──────────────────────────────────────────────────┐       ┃  R1 ─── R2 ─── R3 ───  ┃
┃ └────────────────┘┃  │████████████████████████████████░░░░░░░░░░░░░░░░ │       ┃       0.88  0.91  0.91  ┃
┃                  ┃  │ 5,200 / 8,000 tokens                     65%   │       ┃  threshold ─ ─ ─ 0.85   ┃
┃ Total working    ┃  └──────────────────────────────────────────────────┘       ┃                          ┃
┃ memory: 2,270tok ┃                                                              ┃  ⏱ Model Utilization:   ┃
┃ (raw: 8,900tok)  ┃  ── Extended Thinking (live) ───────────────────────        ┃  pythagoras████████ 72% ┃
┃ Savings: 74%     ┃  │ Let me think about how to write this hero section.       ┃  edison    █████░░░ 55% ┃
┃                  ┃  │ The entity is QR Code AI, a SaaS for dynamic QR codes.   ┃  york      ████░░░░ 43% ┃
┃                  ┃  │ For French B2B audience, I should:                        ┃  atlas     ██░░░░░░ 18% ┃
┃                  ┃  │ 1. Use "code QR" not "QR code" (knowledge atom)          ┃                          ┃
┃                  ┃  │ 2. Avoid "gratuit" in headline (taboo)                   ┃  🐔 Agents: 0 active    ┃
┃                  ┃  │ 3. Use formal "vous" (audience: B2B formal)              ┃  🐤 Subagents: 0        ┃
┃                  ┃  │ 4. Lead with data-driven argument...                      ┃  📟 exec: 0 running     ┃
┃                  ┃  │ █████████████░░░░░░░ 8,200/16,384 thinking tokens        ┃  🛰️ fetch: 0 pending    ┃
┣━━━━━━━━━━━━━━━━━━┻━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┻━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃ ⚡ LIVE │ hero [Claude/edison+🧠] "Créez des codes QR intelligents qui..." │ 5.2K in │ 890 out │ struct: ⏳  ┃
┃ 💰 $0.037 │ 🧠 pythagoras ◉ edison ◉ york ◉ atlas ○ │ 🔗 NovaNet: qr-code-ai/fr-FR │ EP: 4+2R    ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

---

## Panel Breakdown

### Panel 1: Shaka Commander (top-left)

The "brain" of Jarvis. Shows WHAT the AI is thinking and deciding.

| Element | Data | Update Frequency |
|---------|------|-----------------|
| Round counter | Current/Max with ring progress | Per round |
| State | PLAN → EXEC → EVAL → DECIDE | Real-time |
| Quality score | 0.0–1.0 with color (red < 0.85, green ≥ 0.85) | Per review task |
| Token budget | Bar + numbers (spent/total) | Per task completion |
| Model roster | 4 dots: ◉ active, ○ idle, per slot name + color | Real-time |
| Decision log | Chronological: R1 ✅, R2 ⏳, R3 ○ with task names | Per round |
| Shaka thinking | Live preview of strategist's reasoning (truncated) | Streaming |
| Next actions | Queue of planned tasks (from shaka) | Per decision |

**Color coding:**
- Round ring: teal gradient filling
- Quality: green ≥ 0.85, orange 0.70–0.84, red < 0.70
- Budget: teal → orange at 80% → red at 95%
- Model dots: purple (pythagoras), blue (edison), orange (york), gray (atlas)

---

### Panel 2: Live DAG (top-center)

The "holographic display". Shows the EXECUTION graph in real-time.

| Element | Visual | Behavior |
|---------|--------|----------|
| Task nodes | Rounded rectangles with verb icon + name + model badge | Glow when active |
| Edges | Arrows with binding labels | Pulse when data flows |
| Parallel branches | Side-by-side nodes at same Y level | Show concurrency |
| for_each expansion | N copies of a node with iteration index | Fan-out pattern |
| spawn_agent | 🐔 node → dotted line → 🐤 child nodes | Appear dynamically |
| decompose: | Dotted placeholder → materializes into real nodes | Animation |
| Status indicators | ✅ done, ⏳ running (with spinner), ○ pending, ❌ failed | Real-time |
| Timing | "2.1s" on completed nodes | On completion |
| Token count | "500 tok" badge on completed nodes (record size) | On completion |
| Progress bar | █████░░░░ percentage on running nodes | Real-time |

**Verb icons on nodes:**
- ⚡ infer (lightning)
- 📟 exec (terminal)
- 🛰️ fetch (satellite)
- 🔌 invoke (plug — for MCP calls)
- 🐔 agent (chicken)
- 🔍 search model slot indicator
- 🧠 extended thinking indicator

**Below the DAG:**
- Active `with:` bindings: which task receives what from where
- Parallel execution bars: side-by-side progress
- Structured output: current schema being validated

---

### Panel 3: NovaNet Intelligence (top-right)

The "intelligence database". Shows everything NovaNet provides.

| Section | Content | Source |
|---------|---------|--------|
| Entity card | Key, denomination forms (text/title/abbrev/url), class | novanet_context |
| Locale info | BCP-47 code, flag, register, audience type | novanet_context |
| Knowledge atoms | Expressions (with usage notes), taboos (with warnings), audience traits | novanet_context(mode=knowledge) |
| Graph neighbors | Mini tree showing entity's immediate connections (arcs + targets) | novanet_search (mode: walk) |
| Last MCP call | Tool name, params, response time, status | Live MCP client |

**Visual treatment:**
- Entity card: teal border (NovaNet color)
- Expressions: green text (safe to use)
- Taboos: orange/red with ⚠ warning icon
- Audience traits: blue informational
- Graph: mini ASCII tree with arc labels

---

### Panel 4: Records Timeline (bottom-left)

The "memory bank". Shows the compressed record history.

| Record type | Visual | Meaning |
|-------------|--------|---------|
| Generated (new) | Gold border, ✅ icon | Created this session |
| In-progress | Gold border, ⏳ spinner | Being compressed |
| Recalled from NovaNet | Teal border, ←🧠 icon | Loaded from memory |
| Persisted to NovaNet | Gold+teal border, 🧠→ icon | Saved for future |
| Failed | Red border, ❌ icon | Compression failed |

Each record card shows:
- Name (task ID that produced it)
- Token count (compressed size)
- Source (novanet, local, agent)
- Retained fields (structured keys extracted)
- Date (for recalled records)
- Entity link (for persisted records)

**Bottom stats:**
- Total working memory: sum of all active record tokens
- Raw equivalent: what it would be without compression
- Savings percentage: (raw - compressed) / raw × 100

---

### Panel 5: Task Inspector (bottom-center)

The "x-ray view". Deep detail on the currently selected/active task.

| Section | Content |
|---------|---------|
| **Task header** | ID, verb icon, template name, model slot + model name |
| **with: bindings** | Table: alias → source path → resolved value preview |
| **Transforms** | Active pipe chain: `knowledge | extract(expressions) | first(5)` |
| **structured: schema** | JSON schema with required fields highlighted |
| **Validation status** | 4 layers: Layer 1 ⏳/✅/❌ → Layer 2 → Layer 3 → Layer 4 |
| **record: config** | compress, max_tokens, retain fields, persist target |
| **context_budget** | Progress bar with current/max token count |
| **Extended thinking** | Live streaming of Claude's reasoning (if enabled) |

---

### Panel 6: Metrics Dashboard (bottom-right)

The "flight instruments". Performance and cost tracking.

| Metric | Visualization |
|--------|--------------|
| Cost breakdown | Per model slot: name + dollar amount |
| Token usage | Total in/out/thinking/record tokens |
| Record efficiency | Savings percentage (compression ratio) |
| NovaNet memory | Recalled count, stored count, entity link, locale progress |
| CSR score | NovaNet audit quality (constraint satisfaction rate) |
| Quality trend | Mini sparkline showing score across rounds |
| Model utilization | Horizontal bars per slot (% time active) |
| Active counts | Agents 🐔, subagents 🐤, exec 📟, fetch 🛰️ running |

---

### Footer (bottom bar)

The "heads-up display" — always visible, always current.

```
⚡ LIVE │ task [Model/slot] "streaming output..." │ tokens in/out │ struct status
💰 cost │ model dots │ 🔗 NovaNet entity/locale │ EP: N+NR (recalled)
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus between panels |
| `1-6` | Jump to specific panel |
| `↑↓` | Navigate within panel (records, decisions, tasks) |
| `Enter` | Expand selected item (record detail, task detail) |
| `Space` | Pause/resume shaka (manual override) |
| `d` | Toggle DAG mode (tree/flat/radial) |
| `e` | Toggle record panel (collapsed/expanded) |
| `n` | Toggle NovaNet panel |
| `t` | Toggle thinking panel (show/hide extended thinking) |
| `c` | Show cost detail overlay |
| `?` | Command palette (fuzzy search all actions) |
| `q` | Quit (with confirmation if running) |
| `Ctrl+P` | Pause shaka and enter manual mode |
| `Ctrl+S` | Save current state as snapshot |

---

## Color Palette

```
╔══════════════════════════════════════════════════════════════════════════════╗
║  SOLARIZED DARK + SUPERNOVAE SEMANTIC COLORS                               ║
╠══════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  Background:    #002b36  (Solarized base03)                                ║
║  Panel borders: #586e75  (Solarized base01)                                ║
║  Text:          #93a1a1  (Solarized base1)                                 ║
║  Bright text:   #fdf6e3  (Solarized base3)                                 ║
║  Dimmed text:   #657b83  (Solarized base00)                                ║
║                                                                            ║
║  ── Semantic Colors ──                                                     ║
║                                                                            ║
║  #0d9488  Teal     NovaNet, knowledge, entity links, brain                 ║
║  #7c3aed  Purple   Pythagoras model, shaka decisions, thinking              ║
║  #2563eb  Blue     Edison model, primary execution, MCP protocol           ║
║  #d97706  Orange   York model, warnings, alerts, atlas                     ║
║  #b58900  Gold     Records, compressed data, memory packets                ║
║  #16a34a  Green    Success, completed, validated, healthy                  ║
║  #dc2626  Red      Error, failed, invalid, critical                        ║
║  #06b6d4  Cyan     Active/streaming, live data, current focus              ║
║                                                                            ║
║  ── Model Slot Colors ──                                                   ║
║                                                                            ║
║  pythagoras #7c3aed  Purple  (deep thought, extended thinking)             ║
║  edison     #2563eb  Blue    (primary execution, quality output)           ║
║  york       #d97706  Orange  (fast, cheap, information gathering)          ║
║  atlas      #586e75  Gray    (utility, formatting, simple tasks)           ║
║                                                                            ║
║  ── Record Colors ──                                                       ║
║                                                                            ║
║  New record:       #b58900  Gold border                                    ║
║  Recalled:         #0d9488  Teal border (from NovaNet)                     ║
║  Persisted:        #b58900 + #0d9488  Gold+Teal (saved to NovaNet)        ║
║  In-progress:      #06b6d4  Cyan border (being compressed)                 ║
║  Failed:           #dc2626  Red border                                     ║
║                                                                            ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## Gemini Prompts

### Prompt 1: TUI Cockpit (Main Runner Screen)

```
Create a hyper-detailed UI mockup of a futuristic terminal-based AI workflow
orchestrator called "Nika v0.30 — Shaka Runner". This is the user's view
when piloting an AI — it should feel like JARVIS from Iron Man, but as a
real terminal application. Dark, information-dense, every pixel meaningful.

OVERALL AESTHETIC:
- Dark background: Solarized Dark (#002b36)
- Monospace font throughout (JetBrains Mono or similar)
- Box-drawing characters (Unicode ┌─┐│└─┘) for panel borders
- Subtle scan-line effect (very faint horizontal lines)
- No gradients on backgrounds — flat panels with sharp borders
- Panel borders: #586e75. Active panel border: #06b6d4 (cyan glow)
- Think: Bloomberg Terminal meets JARVIS meets mission control
- Aspect ratio: 21:9 ultrawide (cinematic cockpit feeling)

HEADER BAR (full width, ~3% height):
- Left: Butterfly emoji + "NIKA v0.30" in bright white (#fdf6e3)
- Center: Workflow filename "generate-multilingual.nika.yaml" in dimmed text
- Right side cluster: "Round 4/8" (cyan), quality dot "◉ 0.91" (green),
  cost "$0.037" (gold), timer "⏱ 47.2s" (dimmed)
- Subtle horizontal rule below, 1px, #586e75

6-PANEL LAYOUT (2 rows × 3 columns):

═══ TOP ROW (55% of screen height) ═══

PANEL 1 — SHAKA COMMANDER (top-left, 15% width):
- Header: "🎯 SHAKA COMMANDER" with teal accent line
- Round indicator: Large "④" with circular progress ring (teal, 50% filled)
- State badge: "EXEC" in cyan pill
- Quality: "0.91" in large green text with threshold line at 0.85
- Token budget: Horizontal bar, teal gradient filling to ~80%,
  showing "12.1K / 15K" below
- Model roster: 4 rows, each with:
  - Colored dot (◉ or ○): purple/blue/orange/gray
  - Slot name: "pythagoras", "edison", "york", "atlas"
  - Status: active model name when in use
- Decision log: Scrollable list
  R1 ✅ get_entity_context (invoke)
  R2 ✅ get_knowledge (invoke)
  R3 ✅ recall_records (invoke)
  R4 ⏳ hero + research (parallel)
  R5 ○ features
  R6 ○ pricing
  R7 ○ faq
  R8 ○ review
- Shaka thinking: Small box at bottom showing live reasoning text in
  purple italic: "hero & research can run in parallel since..."

PANEL 2 — LIVE DAG (top-center, 50% width):
This is the main stage. Shows a directed acyclic graph with live execution.
- 8 task nodes arranged in a flow layout (left-to-right with branches):
  - Left column (completed, dimmed to 60% opacity):
    - "🔌 ctx" node (teal border, ✅, "1.2s", "500 tok")
    - "🔌 know" node (teal border, ✅, "0.8s", "400 tok")
    - "🧠 mem" node (teal border, ✅, "0.3s", "2 found")
  - Middle column (ACTIVE, bright, cyan glow border):
    - "🔍 research" node (orange border, spinning loader, "2.3s",
      progress bar ████░░ 43%)
    - "✍️ hero" node (blue border + purple 🧠 indicator, spinning, "4.1s",
      progress bar █████░ 55%)
  - Right column (pending, very dimmed):
    - "✍️ features" node (gray)
    - "✍️ pricing" node (gray)
    - "✍️ faq" node (gray)
  - Far right (pending):
    - "🔬 review" node (gray, with purple reasoning icon)
- Arrows connecting nodes with glowing cyan edges for active data flow
- Parallel branches for hero and research shown side-by-side
- Below DAG: "with: bindings" text showing data flow labels
- Below that: structured output schema preview for hero task:
  { headline: str ✓, subheadline: str ✓, cta_text: str ✓, body: str ✓ }
  with "Layer 1 ⏳ validating..." status

PANEL 3 — NOVANET INTELLIGENCE (top-right, 35% width):
- Header: "🧠 NOVANET INTELLIGENCE" with teal accent
- Entity card (teal-bordered box):
  - "◆ qr-code-ai" in bright white
  - Denomination forms as key-value pairs:
    text: "QR Code AI", title: "QR Code AI", abbrev: "QRCAI", url: "qr-code-ai"
- Locale badge: "🏳️ fr-FR" with metadata: "formal register, B2B audience"
- Knowledge atoms section (3 sub-boxes):
  - Expressions (green-bordered):
    "code QR" (not "QR code"), "flash code" (alt), "générer" > "créer"
  - Taboos (orange-bordered with ⚠):
    "gratuit in headlines → implies low quality"
    "informal tu → B2B requires vous"
  - Audience traits (blue-bordered):
    "formal tone", "data-driven arguments", "metric > anecdote"
- Graph mini-map: Small tree showing entity connections:
  qr-code-ai ──HAS_NATIVE──▶ fr-FR, en-US, de-DE
              ──HAS_PAGE────▶ /homepage
              ──HAS_KEYWORD─▶ "qr code generator"

═══ BOTTOM ROW (40% of screen height) ═══

PANEL 4 — RECORDS TIMELINE (bottom-left, 15% width):
- Header: "📦 RECORDS" with gold accent
- Stack of 6 record cards (scrollable):
  - EP-1: "entity_context" | 500 tok | ✅ | gold border
  - EP-2: "knowledge" | 400 tok | ✅ | gold border
  - EP-3: "research" | ~400 tok | ⏳ | cyan border (in progress)
  - EP-4: "hero" | pending | ○ | gray border
  - EP-R1: "past research" | 280 tok | ←🧠 | teal border (recalled)
    "2025-03-12 • entity: qr-code"
  - EP-R2: "fr-FR trends" | 190 tok | ←🧠 | teal border (recalled)
    "2026-02-28 • entity: qr-code"
- Summary bar at bottom:
  "Working memory: 2,270 tok (raw: 8,900) — 74% savings"

PANEL 5 — TASK INSPECTOR (bottom-center, 50% width):
- Header: "🔬 TASK INSPECTOR — hero (write_section)"
- Task info: verb ⚡ infer, model Claude (edison slot + 🧠 thinking)
- with: bindings table (3 columns: alias | source | preview):
  entity → "$get_ctx" → {name: "QR Code AI", ...}
  knowledge → "$get_know" → {expressions: 12, taboos: 3}
  research → "$research" → (⏳ pending)
- Transform chain: "knowledge | extract(expressions) | first(5)"
- structured: schema (JSON with colored required markers)
- Validation pipeline: "Layer 1 ⏳ | Layer 2 ○ | Layer 3 ○ | Layer 4 ○"
- context_budget bar: ████████████████░░░░░░░░ 5,200/8,000 (65%)
- Extended thinking preview (purple background box):
  Live streaming Claude's reasoning about French QR code landing page:
  "1. Use 'code QR' not 'QR code' (knowledge atom)
   2. Avoid 'gratuit' in headline (taboo)
   3. Lead with data-driven argument..."
  Progress: ██████████████░░░░░ 8,200/16,384 thinking tokens

PANEL 6 — METRICS DASHBOARD (bottom-right, 35% width):
- Header: "📈 METRICS"
- Cost table: pythagoras $0.018, edison $0.012, york $0.004, atlas $0.003
  TOTAL: $0.037 (in gold)
- Token table: In 18,420 | Out 4,890 | Thinking 8,200 | Records 2,200
  "Saved by records: 74%"
- NovaNet memory box: "Recalled: 2 | Stored: 1 | Entity: qr-code-ai
  Locales: fr-FR (1/5) | CSR: 0.96"
- Quality trend sparkline: dots at 0.88, 0.91, 0.91 with 0.85 threshold line
- Model utilization bars:
  pythagoras████████░░ 72%
  edison    █████░░░░░ 55%
  york      ████░░░░░░ 43%
  atlas     ██░░░░░░░░ 18%

FOOTER BAR (full width, ~5% height):
- Line 1: "⚡ LIVE │ hero [Claude/edison+🧠] 'Créez des codes QR
  intelligents qui...' │ 5.2K in │ 890 out │ struct: ⏳"
- Line 2: "💰 $0.037 │ 🧠 ◉ pythagoras ◉ edison ◉ york ○ atlas │
  🔗 NovaNet: qr-code-ai/fr-FR │ EP: 4 + 2 recalled"

CRITICAL DETAILS FOR REALISM:
- Every number must look real and consistent (totals add up)
- French text visible in the streaming output: "Créez des codes QR
  intelligents qui transforment votre stratégie marketing..."
- The extended thinking text should show real reasoning about the task
- Progress bars should look partially filled (not at 0% or 100%)
- Dimmed completed nodes vs bright active nodes creates visual flow
- The gold record cards in the left panel should feel like "memory packets"
- The teal NovaNet panel should feel like an "intelligence briefing"
- The purple shaka thinking should feel like "the AI explaining itself"

RENDER AS:
- A high-fidelity terminal screenshot, as if captured from a real application
- Dark background, crisp monospace text, pixel-perfect box-drawing characters
- 4K resolution, suitable for a keynote presentation
- The feeling: "I am piloting the most advanced AI orchestrator on Earth"
```

---

### Prompt 2: Architecture Blueprint (The Brain/Body Diagram)

```
Create a technical engineering blueprint diagram of the "Nika v0.30" AI
orchestrator architecture. The style is a hybrid between a vintage aerospace
schematic and a cyberpunk neural network visualization. Dark blueprint
background (#002b36).

THE CENTRAL ORGANISM:
At the center is a large circular structure representing the Nika runtime.
It has a biological/mechanical hybrid feel — like a cybernetic brain with
circuit traces.

INNER CORE — "The Kernel" (center circle):
A gear/brain hybrid icon labeled "DAG SCHEDULER". This is the heart.
Around it, 5 smaller icons representing the 5 semantic verbs:
- ⚡ infer (lightning bolt, blue)
- 📟 exec (terminal, gray)
- 🛰️ fetch (satellite, cyan)
- 🔌 invoke (plug, teal)
- 🐔 agent (stylized bird, purple)

ORBITAL RING 1 — "v0.30 Features" (6 modules orbiting the core):
Arranged in a circle around the kernel, connected by circuit traces:

1. TOP: SHAKA_ORCHESTRATOR
   Icon: compass/target. Color: purple (#7c3aed)
   Label: "Dynamic task dispatch, goal-driven reasoning"
   Sub-elements: goal text, max_rounds counter, quality gauge

2. TOP-RIGHT: MODEL_ROUTER
   Icon: 4 colored dots in a diamond. Color: multi
   Label: "4-slot routing: pythagoras/edison/york/atlas"
   Sub-elements: 4 provider logos, model names

3. RIGHT: STRUCTURED_OUTPUT
   Icon: JSON brackets with checkmark. Color: green (#16a34a)
   Label: "4-layer validation engine"
   Sub-elements: Layer 1→2→3→4 pipeline, JSON schema icon

4. BOTTOM-RIGHT: RECORD_ENGINE
   Icon: film strip compressed. Color: gold (#b58900)
   Label: "LLM compression at task boundaries"
   Sub-elements: compress → retain → persist pipeline

5. BOTTOM-LEFT: CONTEXT_MANAGER
   Icon: measuring gauge/meter. Color: blue (#2563eb)
   Label: "Token budget tracking, working memory"
   Sub-elements: budget bar, token counter

6. LEFT: INTROSPECTION_TOOLS
   Icon: magnifying glass + wrench. Color: cyan (#06b6d4)
   Label: "6 runtime tools: records, threads, cost, dag, status, shaka"
   Sub-elements: 6 small tool icons

BETWEEN ORBITAL MODULES:
Golden hexagonal data packets (records) flowing along the orbital path.
Each packet is a small hexagon with a token count inside ("400 tok").
The flow direction is clockwise, showing data lifecycle:
BIND (with:) → EXECUTE → VALIDATE (structured:) → COMPRESS (record:) →
STORE (persist:) → RECALL (memory)

ORBITAL RING 2 — "Data Flow" (outer ring):
A larger ring showing the complete data lifecycle:
- INPUT section (top): workflow.nika.yaml file, context: files, inputs:
- BIND section (right): with: block, BindingPath arrows, 27 transforms
- OUTPUT section (bottom): structured: validation, record: compression
- MEMORY section (left): NovaNet persistence, cross-session recall

ROOTS — "NovaNet Knowledge Graph" (below the organism):
The bottom of the diagram shows roots/neural pathways extending downward
into a circuit-board landscape labeled "NOVANET KNOWLEDGE GRAPH".

Root branches labeled:
- "ENTITIES" → Entity, EntityNative, denomination forms
- "KNOWLEDGE" → expressions, taboos, audience traits, patterns
- "STRUCTURE" → Page, Block, PageNative, BlockNative
- "MEMORY" → Record persistence, recall pathways
- "SEO" → keywords, metrics, SERP data

The roots should look like neural pathways merging into a circuit board
pattern. Color: teal (#0d9488) fading into the background.

CROWN — "MCP Protocol" (above the organism):
At the top, antenna-like structures reaching upward, labeled "MCP PROTOCOL".
Connection lines showing:
- "invoke: novanet_context" → data flowing down into the organism
- "invoke: novanet_search" → data flowing down
- "invoke: novanet_write" → data flowing up from organism to NovaNet
- Generic "Any MCP Server" connections (Perplexity, GitHub, Slack, etc.)

PERIPHERAL ELEMENTS:

Left side: "PROVIDERS" panel showing 7 provider logos/icons arranged
vertically:
- Anthropic (Claude) — purple
- OpenAI — green
- Mistral — orange
- Groq — blue
- DeepSeek — cyan
- Gemini — multi
- Native (GGUF) — gray
Connected to MODEL_ROUTER via lines.

Right side: "TOOLS" panel showing the 11 builtin tools:
Core: sleep, log, emit, assert, prompt, run
File: read, write, edit, glob, grep
Connected to the agent verb via lines.

ANNOTATIONS:
Small labels throughout explaining key concepts:
- "with: is the universal connector" (near BIND section)
- "structured: is the quality gate" (near VALIDATE section)
- "record: prevents the dumb zone" (near COMPRESS section)
- "NovaNet remembers across sessions" (near roots)
- "Shaka = Slate's thread weaving" (near SHAKA module)
- "YAML-first: auditable, reproducible" (near INPUT section)

TECHNICAL LABELS:
- "61 NodeClasses" near NovaNet roots
- "182 ArcClasses" near NovaNet roots
- "5 Semantic Verbs" near kernel
- "27 Transforms" near BIND section
- "4 Validation Layers" near STRUCTURED_OUTPUT
- "48 MCP Aliases" near MCP crown

COLOR PALETTE:
- Background: #002b36 (dark blue-gray, like blueprint paper)
- Circuit traces: #586e75 (gray, thin lines)
- Teal (#0d9488): NovaNet elements, roots, knowledge
- Purple (#7c3aed): Shaka, pythagoras, thinking
- Blue (#2563eb): Edison execution, context management
- Orange (#d97706): York, atlas, warnings
- Gold (#b58900): Records, data packets, memory
- Green (#16a34a): Validation, success, health
- Cyan (#06b6d4): Active elements, MCP protocol, introspection
- White (#fdf6e3): Labels, important text

STYLE:
- Thin precise lines (1-2px) like engineering schematics
- Small, legible labels (monospace font)
- Subtle glow effects on active/important elements
- Circuit-board trace patterns in the background (very subtle)
- No cartoon elements — this is serious engineering visualization
- The feeling: "This is the schematic for the most advanced AI
  orchestrator ever built"

Aspect ratio: 16:9. Resolution: 4K.
Render style: technical illustration with subtle sci-fi elements.
```

---

### Prompt 3: The Poster (Marketing/Keynote Quality)

```
Create a cinematic hero image for "Nika v0.30 — Your AI Orchestrator" that
could be used in a keynote presentation or product launch. The image should
evoke the feeling of JARVIS from Iron Man — an AI copilot that sees
everything, knows everything, and orchestrates everything.

COMPOSITION:
- Ultrawide cinematic ratio (21:9)
- Dark background (#002b36 Solarized Dark) with subtle depth
- Center: A large butterfly silhouette (Nika's mascot 🦋) made entirely
  of glowing data flows, circuit traces, and knowledge graph connections
- The butterfly's wings are composed of:
  LEFT WING — "The Body" (Execution):
    - DAG flow visualization (nodes and edges)
    - 5 verb icons floating in the wing pattern
    - YAML code fragments visible in the wing texture
    - Record data packets (golden hexagons) flowing through
    - Model slot indicators (4 colored dots)
  RIGHT WING — "The Brain" (Knowledge):
    - Knowledge graph nodes and arcs
    - Entity cards with denomination forms
    - Locale flags (fr-FR, en-US, de-DE, ja-JP, es-ES)
    - Expression atoms floating like particles
    - NovaNet teal glow emanating from this wing
- The butterfly's BODY (center line):
    - MCP protocol connection (the bridge between Body and Brain)
    - Glowing blue spine with data flowing in both directions

SURROUNDING ELEMENTS:
- Top: "NIKA v0.30" in large, clean, white monospace text
- Below butterfly: "The AI Orchestrator" in smaller teal text
- Bottom: 6 feature badges in a horizontal row:
  "Model Slots" | "Records" | "Shaka" | "Context Budget" | "Memory" | "Introspect"
  Each with its icon and accent color

FLOATING DATA ELEMENTS around the butterfly:
- Golden record hexagons drifting like fireflies
- Teal knowledge graph fragments
- Purple reasoning traces (like thought bubbles decomposing)
- Blue execution paths with progress indicators
- Code snippets: `with: { entity: "$ctx" }` floating transparently

BACKGROUND:
- Subtle circuit-board pattern radiating from the butterfly
- Stars/particles (representing distributed execution)
- Very subtle grid lines (blueprint feel)
- Depth of field: sharp center, subtle blur at edges

TEXT OVERLAYS (small, positioned around the butterfly):
- "KNOWING × DOING × CONNECTING" (the golden rule)
- "5 Verbs. 4 Models. 1 DAG."
- "NovaNet remembers. Nika executes."
- Version: "v0.30 • YAML-first • Knowledge-aware • Shaka-driven"

COLOR TREATMENT:
- Predominantly dark with selective color accents
- Teal (#0d9488) for NovaNet/knowledge elements (right wing)
- Purple (#7c3aed) for shaka/pythagoras (top of butterfly)
- Blue (#2563eb) for execution paths (left wing)
- Gold (#b58900) for records (scattered throughout)
- The butterfly should have a subtle bioluminescent glow
- Light rays emanating from the MCP connection point (body center)

MOOD: Cinematic, premium, technical but beautiful. This is not a toy —
this is mission-critical AI infrastructure. The viewer should think:
"I want to build workflows with this."

4K resolution. Photorealistic lighting with subtle lens flare at the
MCP connection point.
```

---

## Summary: The Jarvis Comparison

```
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                            ║
║  JARVIS (Iron Man)              NIKA v0.30 (SuperNovae)                    ║
║  ─────────────────              ────────────────────────                    ║
║                                                                            ║
║  "Sir, I've analyzed the        Shaka: "I should run hero                ║
║   data and recommend..."        & research in parallel since               ║
║                                 they don't depend on each other"           ║
║                                                                            ║
║  HUD with real-time metrics     6-panel cockpit with live DAG,            ║
║                                 records, NovaNet, metrics                  ║
║                                                                            ║
║  Multi-system integration       5 verbs × 7 providers × MCP protocol     ║
║                                                                            ║
║  "I've cross-referenced         NovaNet: entities, locale atoms,          ║
║   the databases..."             knowledge graph with 200+ locales          ║
║                                                                            ║
║  "Based on previous             Record recall: "2 past records found      ║
║   encounters..."                from 2025-03-12 for qr-code/fr-FR"        ║
║                                                                            ║
║  "I'm running diagnostics"      Introspection: nika:cost, nika:records,   ║
║                                 nika:dag_info, nika:shaka                  ║
║                                                                            ║
║  "Shall I deploy the            Shaka: quality ≥ 0.85 → persist           ║
║   solution, sir?"               to NovaNet → next locale                   ║
║                                                                            ║
║  One key difference:                                                       ║
║  JARVIS is closed.              NIKA is YAML-first, open, auditable.      ║
║  You trust it blindly.          You SEE every decision in the TUI.        ║
║                                 Every record. Every binding. Every cost.   ║
║                                                                            ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

<div align="center">

[← 09 Use Cases Cookbook](./09-use-cases-cookbook.md) · [📋 Index](./00-README.md)

</div>
