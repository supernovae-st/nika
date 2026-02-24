# Nika v0.9.x — UX/UI Components to Preserve

> **For Claude:** These components MUST be preserved across all v0.9.x releases. Do NOT remove or significantly alter them.

---

## 1. Matrix Rain Effect

**File:** `src/tui/widgets/matrix_rain.rs`

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  MATRIX RAIN — Background Animation                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  WHEN TO TRIGGER:                                                             ║
║  ├── Panel becomes active (receives focus)                                    ║
║  ├── Panel receives important information                                     ║
║  ├── LLM streaming starts                                                     ║
║  └── Background "ambient" mode (low density)                                  ║
║                                                                               ║
║  GLYPH DISTRIBUTION:                                                          ║
║  ├── 80% Katakana (ア-ン range: U+30A0-U+30FF)                                 ║
║  ├── 15% ASCII symbols (!, @, #, $, %, etc.)                                  ║
║  └──  5% Nika mascots (custom emoji set)                                      ║
║                                                                               ║
║  CONFIGURABLE:                                                                ║
║  ├── density: f32 (0.0-1.0, default 0.3)                                      ║
║  ├── speed: u16 (ms per frame, default 50)                                    ║
║  ├── fade_length: u8 (trail length, default 8)                                ║
║  └── color_scheme: Solarized green (#859900)                                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Code Pattern:**
```rust
pub struct MatrixRain {
    columns: Vec<MatrixColumn>,
    density: f32,
    speed: Duration,
    fade_length: u8,
}

impl MatrixRain {
    pub fn trigger_burst(&mut self, intensity: f32) {
        // Increase density temporarily for "important info" effect
    }

    pub fn set_ambient(&mut self) {
        // Low density background mode
    }
}
```

**Integration Points:**
- `ChatView::on_focus()` → triggers matrix burst
- `ChatView::on_stream_start()` → increases density
- All views → ambient mode when idle

---

## 2. Matrix Decrypt Effect

**File:** `src/tui/widgets/matrix_decrypt.rs`

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  MATRIX DECRYPT — Text Reveal Effect                                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  WHEN TO TRIGGER:                                                             ║
║  ├── LLM streaming response (character by character)                          ║
║  ├── Tool result display                                                      ║
║  └── Important status messages                                                ║
║                                                                               ║
║  VERB-THEMED EMOJI CHAOS:                                                     ║
║  ├── fetch:   → 🏴‍☠️ Pirate theme (🏴‍☠️ ⚓ 🦜 💎 🗺️)                                │
║  ├── infer:   → 🌌 Cosmic theme (🌌 ✨ 🌟 💫 🔭)                                 │
║  ├── exec:    → 🤖 Robot theme (🤖 ⚙️ 🔧 💾 🖥️)                                   │
║  ├── invoke:  → 🔌 Electric theme (🔌 ⚡ 🔋 💡 🌩️)                                │
║  ├── agent:   → 🔮 Magic theme (🔮 🪄 ✨ 🌙 🦉)                                   │
║  └── creative → 🦄 Unicorn theme (🦄 🌈 💖 ✨ 🎨)                                 │
║                                                                               ║
║  EFFECT STAGES:                                                               ║
║  1. Random glyphs appear (chaos phase)                                        ║
║  2. Characters progressively lock to final value                              ║
║  3. Full text revealed (complete)                                             ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Code Pattern:**
```rust
pub struct MatrixDecrypt {
    target_text: String,
    revealed_chars: usize,
    theme: DecryptTheme,
    chaos_pool: Vec<char>,
}

pub enum DecryptTheme {
    Pirate,   // fetch:
    Cosmic,   // infer:
    Robot,    // exec:
    Electric, // invoke:
    Magic,    // agent:
    Unicorn,  // creative
}

impl MatrixDecrypt {
    pub fn new_for_verb(verb: &str, text: String) -> Self {
        let theme = match verb {
            "fetch" => DecryptTheme::Pirate,
            "infer" => DecryptTheme::Cosmic,
            "exec" => DecryptTheme::Robot,
            "invoke" => DecryptTheme::Electric,
            "agent" => DecryptTheme::Magic,
            _ => DecryptTheme::Unicorn,
        };
        // ...
    }

    pub fn tick(&mut self) -> &str {
        // Reveal one more character, return current display
    }
}
```

---

## 3. One Panel At A Time

**File:** `src/tui/focus.rs`

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  PANEL FOCUS SYSTEM                                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  RULE: Only ONE panel is active at any time                                   ║
║                                                                               ║
║  NAVIGATION:                                                                  ║
║  ├── Tab        → Next panel (cyclic)                                         ║
║  ├── Shift+Tab  → Previous panel (cyclic)                                     ║
║  ├── Number keys → Direct view switch (1-4)                                   ║
║  └── h/a/s/m    → View hotkeys (Home/Chat/Studio/Monitor)                     ║
║                                                                               ║
║  12 PANEL IDS:                                                                ║
║  ├── Home (4):    FileList, RecentFiles, QuickActions, Preview                ║
║  ├── Chat (3):    MessageList, InputBox, DagPanel (v0.9.4)                    ║
║  ├── Studio (3):  Editor, FileTree, Diagnostics                               ║
║  └── Monitor (4): TraceList, EventLog, TaskGraph, Details                     ║
║                                                                               ║
║  VISUAL INDICATOR:                                                            ║
║  ├── Active panel: Bright border (Solarized blue #268bd2)                     ║
║  ├── Inactive:     Dim border (Solarized base01 #586e75)                      ║
║  └── Matrix rain burst on focus change                                        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Code Pattern:**
```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanelId {
    // Home
    FileList,
    RecentFiles,
    QuickActions,
    Preview,
    // Chat
    MessageList,
    InputBox,
    DagPanel,  // Added in v0.9.4
    // Studio
    Editor,
    FileTree,
    Diagnostics,
    // Monitor
    TraceList,
    EventLog,
    TaskGraph,
    Details,
}

pub struct FocusState {
    current: PanelId,
    view_panels: HashMap<View, Vec<PanelId>>,
}

impl FocusState {
    pub fn next(&mut self, view: View) {
        // Cycle to next panel in current view
    }

    pub fn prev(&mut self, view: View) {
        // Cycle to previous panel in current view
    }

    pub fn is_focused(&self, panel: PanelId) -> bool {
        self.current == panel
    }
}
```

---

## 4. Provider Modal (v0.8.8)

**File:** `src/tui/modals/provider_modal.rs`

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  PROVIDER MODAL — Shift+P                                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  5 TABS:                                                                      ║
║  ├── Cloud    → Claude, OpenAI, Mistral, Groq, DeepSeek                       ║
║  ├── Ollama   → Local model management                                        ║
║  ├── Keys     → API key configuration (masked input)                          ║
║  ├── Config   → Model parameters (temperature, max_tokens)                    ║
║  └── Status   → Connection status, token usage, latency                       ║
║                                                                               ║
║  6 PROVIDERS:                                                                 ║
║  ├── Claude   (ANTHROPIC_API_KEY)   → claude-sonnet-4-6                       ║
║  ├── OpenAI   (OPENAI_API_KEY)      → gpt-4o                                  ║
║  ├── Mistral  (MISTRAL_API_KEY)     → mistral-large-latest                    ║
║  ├── Ollama   (OLLAMA_API_BASE_URL) → llama3.2                                ║
║  ├── Groq     (GROQ_API_KEY)        → llama-3.3-70b-versatile                 ║
║  └── DeepSeek (DEEPSEEK_API_KEY)    → deepseek-chat                           ║
║                                                                               ║
║  AUTO-DETECTION: Checks env vars in priority order                            ║
║                                                                               ║
║  HOTKEYS:                                                                     ║
║  ├── Shift+P     → Open modal                                                 ║
║  ├── Tab         → Next tab                                                   ║
║  ├── Shift+Tab   → Previous tab                                               ║
║  ├── Enter       → Select/Apply                                               ║
║  └── Escape      → Close modal                                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Code Pattern:**
```rust
pub struct ProviderModal {
    active_tab: ProviderTab,
    providers: Vec<ProviderConfig>,
    selected_provider: usize,
}

pub enum ProviderTab {
    Cloud,
    Ollama,
    Keys,
    Config,
    Status,
}

impl ProviderModal {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Tab bar at top
        // Content based on active_tab
        // Status bar at bottom
    }
}
```

---

## 4-View Architecture

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  CURRENT VIEW STRUCTURE (v0.8.x)                                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │  View Bar:  [1] Home  [2] Chat  [3] Studio  [4] Monitor                 │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  HOME (h/1)           CHAT (a/2)          STUDIO (s/3)       MONITOR (m/4)   ║
║  ┌──────────────┐     ┌──────────────┐    ┌──────────────┐   ┌──────────────┐║
║  │ FileList     │     │ MessageList  │    │ Editor       │   │ TraceList    │║
║  │              │     │              │    │              │   │              │║
║  ├──────────────┤     ├──────────────┤    ├──────────────┤   ├──────────────┤║
║  │ RecentFiles  │     │ InputBox     │    │ FileTree     │   │ EventLog     │║
║  ├──────────────┤     ├──────────────┤    ├──────────────┤   ├──────────────┤║
║  │ QuickActions │     │ DagPanel     │    │ Diagnostics  │   │ TaskGraph    │║
║  ├──────────────┤     │ (v0.9.4)     │    │              │   ├──────────────┤║
║  │ Preview      │     └──────────────┘    └──────────────┘   │ Details      │║
║  └──────────────┘                                            └──────────────┘║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Do NOT Remove

1. **Matrix Rain** — Core visual identity of Nika
2. **Matrix Decrypt** — Essential for streaming UX
3. **One Panel Focus** — Core navigation paradigm
4. **Provider Modal** — Essential for multi-provider support
5. **Verb-themed chaos** — Distinctive personality
6. **Solarized colors** — Accessibility and consistency

---

## May Evolve

1. **Panel count** — DagPanel added in v0.9.4
2. **View count** — 5th view may be added in v1.0.0
3. **Hotkeys** — New shortcuts for DAG operations
4. **Theme variants** — May add more decay themes

---

## Integration with Chat-as-DAG

The new DAG panel (v0.9.4) will:
- Appear in Chat view as third panel
- Use Matrix Rain for node highlight effects
- Use Matrix Decrypt for node label reveal
- Follow One Panel At A Time navigation
- Integrate with Provider Modal for tool status
