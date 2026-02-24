# Provider Modal v0.8.5 → v0.9.0 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete Provider Modal v2 from scaffold (v0.8.5) to production-ready release (v0.9.0)

**Architecture:** Tabbed modal with 4 tabs (Cloud, Ollama, Keys, Config), secure keyring integration, native Ollama HTTP client, real-time streaming UI

**Tech Stack:** Rust, ratatui, keyring-rs, reqwest, tokio, url crate

---

## Version Roadmap

| Version | Focus | Commits | Effort |
|---------|-------|---------|--------|
| v0.8.5 | ✅ Scaffold + Security Hardening | 16 | Done |
| v0.8.6 | Tab Implementations | ~20 | 1-2 days |
| v0.8.7 | Integration & Event Handling | ~10 | 1 day |
| v0.8.8 | Polish & Testing | ~10 | 1 day |
| v0.9.0 | Cosmetic WOW + Release | ~15 | 1-2 days |

---

## v0.8.5 (DONE)

### Completed Tasks
- [x] Core state types (ProviderModalTab, ConnectionStatus, ApiKeyState, DownloadState)
- [x] ProviderModalState with navigation
- [x] NikaKeyring wrapper for keyring-rs
- [x] OllamaClient with NDJSON streaming
- [x] ProviderCard widget
- [x] DownloadGauge widget
- [x] Main ProviderModal widget
- [x] Tab stubs (CloudTab, OllamaTab, KeysTab)
- [x] Security hardening (SEC-001 to SEC-004)
- [x] 51 tests

---

## v0.8.6: Tab Implementations

### Task 1: CloudTab - Provider Cards Grid

**Files:**
- Modify: `src/tui/widgets/provider_modal/tabs/cloud.rs`
- Create: `src/tui/widgets/provider_modal/provider_status.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_cloud_tab_renders_6_providers() {
    let mut state = ProviderModalState::default();
    state.item_count = 6; // 6 providers
    let tab = CloudTab::new(&state);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
    tab.render(Rect::new(0, 0, 80, 20), &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("Claude"));
    assert!(content.contains("OpenAI"));
    assert!(content.contains("Mistral"));
}
```

**Step 2: Implement CloudTab**

```rust
pub struct CloudTab<'a> {
    state: &'a ProviderModalState,
    providers: Vec<ProviderInfo>,
}

struct ProviderInfo {
    name: &'static str,
    icon: &'static str,
    model: &'static str,
    env_var: &'static str,
    status: ConnectionStatus,
}

impl<'a> CloudTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self {
            state,
            providers: vec![
                ProviderInfo { name: "Claude", icon: "🧠", model: "claude-sonnet-4", env_var: "ANTHROPIC_API_KEY", status: ConnectionStatus::Unknown },
                ProviderInfo { name: "OpenAI", icon: "🤖", model: "gpt-4o", env_var: "OPENAI_API_KEY", status: ConnectionStatus::Unknown },
                ProviderInfo { name: "Mistral", icon: "🌀", model: "mistral-large", env_var: "MISTRAL_API_KEY", status: ConnectionStatus::Unknown },
                ProviderInfo { name: "Groq", icon: "⚡", model: "llama-3.3-70b", env_var: "GROQ_API_KEY", status: ConnectionStatus::Unknown },
                ProviderInfo { name: "DeepSeek", icon: "🔬", model: "deepseek-chat", env_var: "DEEPSEEK_API_KEY", status: ConnectionStatus::Unknown },
                ProviderInfo { name: "Ollama", icon: "🦙", model: "llama3.2", env_var: "OLLAMA_HOST", status: ConnectionStatus::Unknown },
            ],
        }
    }
}

impl Widget for CloudTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 2x3 grid of ProviderCards
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(area);

        for (row_idx, row_area) in rows.iter().enumerate() {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Ratio(1, 3); 3])
                .split(*row_area);

            for (col_idx, col_area) in cols.iter().enumerate() {
                let provider_idx = row_idx * 3 + col_idx;
                if provider_idx < self.providers.len() {
                    let p = &self.providers[provider_idx];
                    let style = if provider_idx == self.state.selected_idx {
                        CardStyle::Selected
                    } else {
                        CardStyle::Normal
                    };
                    ProviderCard::new(p.icon, p.name, p.model, &p.status)
                        .style(style)
                        .render(*col_area, buf);
                }
            }
        }
    }
}
```

**Step 3: Run tests**

```bash
cargo test --lib -- cloud_tab
```

**Step 4: Commit**

```bash
git add src/tui/widgets/provider_modal/tabs/cloud.rs
git commit -m "feat(tui): implement CloudTab with 6 provider cards grid"
```

---

### Task 2: Provider Status Checker (Async)

**Files:**
- Create: `src/tui/widgets/provider_modal/provider_checker.rs`

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_check_provider_with_valid_key() {
    std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-key-123456789012345678901234567890");
    let status = ProviderChecker::check("anthropic").await;
    // Will fail in CI without real key, but validates the API
    assert!(matches!(status, ConnectionStatus::Connected { .. } | ConnectionStatus::Failed { .. }));
    std::env::remove_var("ANTHROPIC_API_KEY");
}
```

**Step 2: Implement ProviderChecker**

```rust
pub struct ProviderChecker;

impl ProviderChecker {
    pub async fn check(provider: &str) -> ConnectionStatus {
        let start = std::time::Instant::now();

        // Check if API key exists
        let env_var = provider_env_var(provider);
        if std::env::var(env_var).is_err() && provider != "ollama" {
            return ConnectionStatus::NotConfigured;
        }

        // Ping provider
        let result = match provider {
            "anthropic" => Self::check_anthropic().await,
            "openai" => Self::check_openai().await,
            "ollama" => Self::check_ollama().await,
            _ => Self::check_generic(provider).await,
        };

        match result {
            Ok(()) => ConnectionStatus::Connected { latency_ms: start.elapsed().as_millis() as u64 },
            Err(e) => ConnectionStatus::Failed { error: e },
        }
    }

    async fn check_anthropic() -> Result<(), String> {
        // Simple API ping (models endpoint)
        let client = reqwest::Client::new();
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| "No API key")?;
        client.get("https://api.anthropic.com/v1/models")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn check_ollama() -> Result<(), String> {
        let client = OllamaClient::new();
        if client.is_available().await {
            Ok(())
        } else {
            Err("Not running".to_string())
        }
    }
}
```

**Step 3: Run tests, Step 4: Commit**

---

### Task 3: OllamaTab - Model List with Pull/Delete

**Files:**
- Modify: `src/tui/widgets/provider_modal/tabs/ollama.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_ollama_tab_renders_model_list() {
    let models = vec![
        OllamaModelInfo { name: "llama3.2".into(), size: 4_700_000_000, ... },
        OllamaModelInfo { name: "mistral".into(), size: 7_000_000_000, ... },
    ];
    let tab = OllamaTab::new(&models, 0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
    tab.render(Rect::new(0, 0, 60, 10), &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("llama3.2"));
    assert!(content.contains("4.7 GB"));
}
```

**Step 2: Implement OllamaTab**

```rust
pub struct OllamaTab<'a> {
    models: &'a [OllamaModelInfo],
    selected: usize,
    download_state: &'a DownloadState,
}

impl Widget for OllamaTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Header
        buf.set_string(area.x, area.y, "Installed Models", Style::default().bold());

        // Model list
        let list_area = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
        for (i, model) in self.models.iter().enumerate() {
            let y = list_area.y + i as u16;
            if y >= list_area.bottom() { break; }

            let style = if i == self.selected {
                Style::default().bg(Color::Rgb(30, 41, 59))
            } else {
                Style::default()
            };

            let line = format!("{} {} ({})",
                if i == self.selected { "▸" } else { " " },
                model.name,
                model.size_display()
            );
            buf.set_string(list_area.x, y, &line, style);
        }

        // Download progress if active
        if let DownloadState::Downloading { model, progress, downloaded, total } = self.download_state {
            let gauge_area = Rect::new(area.x, area.bottom() - 2, area.width, 2);
            DownloadGauge::new(model, *progress, *downloaded, *total)
                .render(gauge_area, buf);
        }
    }
}
```

**Step 3-4: Test and commit**

---

### Task 4: KeysTab - API Key Management

**Files:**
- Modify: `src/tui/widgets/provider_modal/tabs/keys.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_keys_tab_renders_provider_keys() {
    let state = ProviderModalState::default();
    let tab = KeysTab::new(&state);
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 12));
    tab.render(Rect::new(0, 0, 60, 12), &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("Anthropic"));
    assert!(content.contains("OpenAI"));
}
```

**Step 2: Implement KeysTab**

```rust
pub struct KeysTab<'a> {
    state: &'a ProviderModalState,
    providers: Vec<KeyEntry>,
}

struct KeyEntry {
    name: &'static str,
    provider: &'static str,
    key_state: ApiKeyState,
}

impl<'a> KeysTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        let providers = ["anthropic", "openai", "mistral", "groq", "deepseek"]
            .iter()
            .map(|&p| {
                let key_state = if let Some(masked) = NikaKeyring::get_masked(p) {
                    ApiKeyState::Configured { masked }
                } else {
                    ApiKeyState::NotConfigured
                };
                KeyEntry {
                    name: match p {
                        "anthropic" => "Anthropic (Claude)",
                        "openai" => "OpenAI (GPT)",
                        "mistral" => "Mistral AI",
                        "groq" => "Groq",
                        "deepseek" => "DeepSeek",
                        _ => p,
                    },
                    provider: p,
                    key_state,
                }
            })
            .collect();

        Self { state, providers }
    }
}

impl Widget for KeysTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, entry) in self.providers.iter().enumerate() {
            let y = area.y + i as u16;
            let selected = i == self.state.selected_idx;

            let style = if selected {
                Style::default().bg(Color::Rgb(30, 41, 59))
            } else {
                Style::default()
            };

            // Row: [icon] Provider Name          [status] sk-ant...x
            let icon = entry.key_state.status_icon();
            let masked = match &entry.key_state {
                ApiKeyState::Configured { masked } => masked.as_str(),
                ApiKeyState::Verified { masked, .. } => masked.as_str(),
                ApiKeyState::Invalid { masked, .. } => masked.as_str(),
                ApiKeyState::NotConfigured => "Not configured",
            };

            let line = format!("{} {} {} {}",
                if selected { "▸" } else { " " },
                icon,
                entry.name,
                masked
            );
            buf.set_string(area.x, y, &line, style);
        }

        // Input mode overlay
        if self.state.key_input_mode {
            let input_area = Rect::new(area.x + 2, area.bottom() - 3, area.width - 4, 3);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Enter API Key ")
                .border_style(Style::default().fg(Color::Rgb(99, 102, 241)));
            let inner = block.inner(input_area);
            block.render(input_area, buf);

            // Show masked input
            let display = "*".repeat(self.state.key_input_buffer.len().min(40));
            buf.set_string(inner.x, inner.y, &display, Style::default());
        }
    }
}
```

**Step 3-4: Test and commit**

---

### Task 5: Async Model/Key Loading

**Files:**
- Create: `src/tui/widgets/provider_modal/loader.rs`

**Implementation:**

```rust
use tokio::sync::mpsc;

pub enum LoaderMessage {
    ProviderStatus(String, ConnectionStatus),
    OllamaModels(Vec<OllamaModelInfo>),
    OllamaNotRunning,
    Error(String),
}

pub struct ModalLoader {
    tx: mpsc::Sender<LoaderMessage>,
}

impl ModalLoader {
    pub fn spawn() -> (Self, mpsc::Receiver<LoaderMessage>) {
        let (tx, rx) = mpsc::channel(32);
        (Self { tx }, rx)
    }

    pub async fn load_all(&self) {
        // Check all providers in parallel
        let providers = ["anthropic", "openai", "mistral", "groq", "deepseek", "ollama"];
        let handles: Vec<_> = providers.iter().map(|&p| {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let status = ProviderChecker::check(p).await;
                let _ = tx.send(LoaderMessage::ProviderStatus(p.to_string(), status)).await;
            })
        }).collect();

        // Load Ollama models
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let client = OllamaClient::new();
            match client.list_models().await {
                Ok(models) => { let _ = tx.send(LoaderMessage::OllamaModels(models)).await; }
                Err(_) => { let _ = tx.send(LoaderMessage::OllamaNotRunning).await; }
            }
        });

        for h in handles {
            let _ = h.await;
        }
    }
}
```

---

### Task 6: Add httpmock Tests for OllamaClient

**Files:**
- Modify: `src/tui/widgets/provider_modal/ollama_client.rs`
- Add dev-dependency: `httpmock = "0.7"`

**Tests to add:**
- `test_list_models_success`
- `test_list_models_connection_error`
- `test_delete_model_success`
- `test_delete_model_not_found`

---

## v0.8.7: Integration & Event Handling

### Task 7: Wire Modal to App State

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/views/chat.rs`

**Implementation:**
- Add `provider_modal: ProviderModalState` to App
- Add keyboard shortcut `Ctrl+P` to open modal
- Integrate modal rendering in main render loop

---

### Task 8: Modal Key Event Handler

**Files:**
- Create: `src/tui/widgets/provider_modal/handler.rs`

```rust
pub enum ModalAction {
    Close,
    SelectProvider(String),
    PullModel(String),
    DeleteModel(String),
    SaveKey(String, String),
    TestKey(String),
    None,
}

pub fn handle_key(state: &mut ProviderModalState, key: KeyEvent) -> ModalAction {
    match key.code {
        KeyCode::Esc => ModalAction::Close,
        KeyCode::Tab => { state.next_tab(); ModalAction::None }
        KeyCode::BackTab => { state.prev_tab(); ModalAction::None }
        KeyCode::Up => { state.navigate_up(); ModalAction::None }
        KeyCode::Down => { state.navigate_down(); ModalAction::None }
        KeyCode::Enter => handle_enter(state),
        KeyCode::Char('1'..='4') => {
            if let Some(tab) = ProviderModalTab::from_key(key.code.into()) {
                state.switch_tab(tab);
            }
            ModalAction::None
        }
        KeyCode::Char('p') if state.active_tab == ProviderModalTab::Ollama => {
            // Pull selected model
            ModalAction::PullModel(get_selected_model(state))
        }
        KeyCode::Char('d') if state.active_tab == ProviderModalTab::Ollama => {
            ModalAction::DeleteModel(get_selected_model(state))
        }
        KeyCode::Char('t') if state.active_tab == ProviderModalTab::Keys => {
            ModalAction::TestKey(get_selected_provider(state))
        }
        _ => ModalAction::None,
    }
}
```

---

### Task 9: Provider Selection Updates Config

**Files:**
- Modify: `src/tui/config.rs`

**Implementation:**
- When provider is selected, update `[providers].default` in config
- Persist to `.nika/config.toml`
- Emit event for runtime provider switching

---

### Task 10: Key Input Mode

**Files:**
- Modify: `src/tui/widgets/provider_modal/handler.rs`
- Modify: `src/tui/widgets/provider_modal/tabs/keys.rs`

**Implementation:**
- Enter key input mode on Enter in Keys tab
- Handle text input character by character
- Validate key format on submit
- Store in keyring on success
- Clear buffer on cancel (Esc)

---

## v0.8.8: Polish & Testing

### Task 11: Error Handling & User Feedback

- Toast notifications for key save success/failure
- Error messages in modal footer
- Loading spinners during async operations

---

### Task 12: Comprehensive Test Suite

- Integration tests for full modal flow
- Mock keyring for CI testing
- Snapshot tests for UI rendering

---

### Task 13: Performance Optimization

- Cache provider status (don't re-check every render)
- Debounce key input validation
- Lazy load Ollama models (only when tab is active)

---

## v0.9.0: Cosmetic WOW + Release

### Phase 1: Visual Polish

- Sparklines for provider latency history
- Animated connection status indicators
- Gradient backgrounds for cards

### Phase 2: Micro-interactions

- Smooth tab transitions
- Card hover effects (if terminal supports)
- Progress bar animations

### Phase 3: Accessibility

- Screen reader hints
- High contrast mode support
- Keyboard navigation indicators

### Phase 4: Documentation

- Update CLAUDE.md with modal usage
- Add screenshots to README
- Document keyboard shortcuts

### Phase 5: Release Checklist

- [ ] All 2200+ tests passing
- [ ] Zero clippy warnings
- [ ] Cargo fmt clean
- [ ] Version bump in Cargo.toml
- [ ] CHANGELOG.md updated
- [ ] Git tag v0.9.0
- [ ] GitHub release with notes

---

## Test Coverage Target

| Module | Current | Target |
|--------|---------|--------|
| state.rs | 20 tests | 25 tests |
| keyring.rs | 12 tests | 15 tests |
| ollama_client.rs | 9 tests | 20 tests |
| tabs/cloud.rs | 0 tests | 8 tests |
| tabs/ollama.rs | 0 tests | 10 tests |
| tabs/keys.rs | 0 tests | 12 tests |
| handler.rs | 0 tests | 15 tests |
| loader.rs | 0 tests | 8 tests |
| **Total** | **51 tests** | **~115 tests** |

---

## Estimated Timeline

| Phase | Start | Duration | Complete |
|-------|-------|----------|----------|
| v0.8.5 Security | Done | - | ✅ |
| v0.8.6 Tabs | Day 1 | 1-2 days | - |
| v0.8.7 Integration | Day 3 | 1 day | - |
| v0.8.8 Polish | Day 4 | 1 day | - |
| v0.9.0 WOW | Day 5-6 | 1-2 days | - |

**Total: ~5-7 days to v0.9.0**

---

## Dependencies

```toml
# Added in v0.8.5
url = "2"  # URL validation

# To add for v0.8.6
httpmock = "0.7"  # [dev-dependencies] Mock HTTP server
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Keyring not available on CI | Mock keyring in tests |
| Ollama not installed | Graceful degradation, show "Not installed" |
| API key validation fails | Clear error messages, retry option |
| Performance with many models | Virtual list, lazy loading |
| Terminal doesn't support colors | Fallback to ASCII indicators |
