# Research Report: Advanced Ratatui Architecture Patterns (2025-2026)

## Summary

Ratatui 0.30 has matured into a serious framework for large-scale TUI applications. The ecosystem
has converged on two dominant architectural patterns: the **Component trait pattern** (used by the
official template, gitui, bottom) and **The Elm Architecture** (TEA, used by smaller apps). For
apps at Nika's scale (164 files), the Component trait with an Action enum message bus is the proven
approach. Animation is handled by TachyonFX or manual tick-based state updates. Testing uses
TestBackend with insta snapshots.

## 1. Component Architecture Patterns

### 1.1 The Official Component Trait (ratatui-org/templates)

The canonical pattern from the official ratatui component template. This is **the** reference
architecture for production apps.

```rust
// -- component trait (the contract) --
pub trait Component {
    /// Give this component a sender so it can emit actions
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        let _ = tx;
        Ok(())
    }

    /// Give this component access to config
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        let _ = config;
        Ok(())
    }

    /// Initialize with terminal size
    fn init(&mut self, area: Size) -> Result<()> {
        let _ = area;
        Ok(())
    }

    /// Handle raw events, return an Action if needed
    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> { Ok(None) }

    /// React to an Action, optionally emit another Action
    fn update(&mut self, action: Action) -> Result<Option<Action>> { Ok(None) }

    /// Render to frame (REQUIRED)
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;
}
```

```rust
// -- action enum (the message bus) --
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    // Add domain-specific actions here:
    // SwitchTab(TabId),
    // OpenModal(ModalKind),
    // DataLoaded(DataPayload),
}
```

```rust
// -- app orchestrator (the main loop) --
pub struct App {
    components: Vec<Box<dyn Component>>,
    should_quit: bool,
    mode: Mode,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
}

impl App {
    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?.tick_rate(1.0).frame_rate(60.0);
        tui.enter()?;

        // Phase 1: register all components
        for component in self.components.iter_mut() {
            component.register_action_handler(self.action_tx.clone())?;
            component.register_config_handler(self.config.clone())?;
            component.init(tui.size()?)?;
        }

        // Phase 2: event loop
        loop {
            self.handle_events(&mut tui).await?;
            self.handle_actions(&mut tui)?;
            if self.should_quit { break; }
        }
        tui.exit()?;
        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            // App-level handling
            match action {
                Action::Quit => self.should_quit = true,
                Action::Render => self.render(tui)?,
                Action::Resize(w, h) => tui.resize(Rect::new(0, 0, w, h))?,
                _ => {}
            }
            // Broadcast to all components, collect follow-up actions
            for component in self.components.iter_mut() {
                if let Some(follow_up) = component.update(action.clone())? {
                    self.action_tx.send(follow_up)?;
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            for component in self.components.iter_mut() {
                let _ = component.draw(frame, frame.area());
            }
        })?;
        Ok(())
    }
}
```

**Key insight**: Components communicate exclusively through Actions. A component emits
`Action::OpenModal(kind)`, the App catches it and pushes a modal component. No direct references
between components.

### 1.2 The Elm Architecture (TEA)

Better for smaller apps or specific subsystems within a larger app.

```rust
// -- Model: immutable app state --
pub struct Model {
    counter: u64,
    view: ViewState,
    data: Option<LoadedData>,
}

// -- Msg: all possible state transitions --
pub enum Msg {
    Increment,
    Decrement,
    Tick,
    DataLoaded(LoadedData),
    SwitchView(ViewState),
}

// -- update: pure function, returns new model + optional chained msg --
pub fn update(model: Model, msg: Msg) -> (Model, Option<Msg>) {
    match msg {
        Msg::Increment => (Model { counter: model.counter + 1, ..model }, None),
        Msg::DataLoaded(data) => (Model { data: Some(data), ..model }, None),
        Msg::SwitchView(v) => (Model { view: v, ..model }, Some(Msg::Tick)),
        _ => (model, None),
    }
}

// -- view: pure render --
pub fn view(model: &Model, area: Rect, buf: &mut Buffer) {
    match model.view {
        ViewState::Dashboard => render_dashboard(model, area, buf),
        ViewState::Detail => render_detail(model, area, buf),
    }
}
```

**When to use TEA vs Component trait**:
- TEA: < 30 files, functional style, easy to test (pure functions)
- Component trait: 30+ files, OOP-ish, each component owns its state

### 1.3 Hybrid: Component Trait + TEA Internals

The pattern used by the best large apps: Component trait for structure, TEA-like update inside each
component.

```rust
pub struct RunnerView {
    // Component owns its local state
    model: RunnerModel,
    action_tx: Option<UnboundedSender<Action>>,
}

struct RunnerModel {
    tasks: Vec<Task>,
    selected: usize,
    scroll_offset: u16,
    loading: bool,
}

impl Component for RunnerView {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // TEA-like: match action, update model, optionally emit
        match action {
            Action::TaskCompleted(id) => {
                if let Some(task) = self.model.tasks.iter_mut().find(|t| t.id == id) {
                    task.status = TaskStatus::Done;
                }
                // Emit follow-up
                Ok(Some(Action::Render))
            }
            Action::SelectNext => {
                self.model.selected = (self.model.selected + 1)
                    .min(self.model.tasks.len().saturating_sub(1));
                Ok(Some(Action::Render))
            }
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Pure view from model
        let items: Vec<ListItem> = self.model.tasks.iter().map(|t| {
            ListItem::new(format!("{}: {}", t.name, t.status))
        }).collect();
        let list = List::new(items)
            .highlight_style(Style::default().fg(Color::Yellow));
        frame.render_stateful_widget(list, area, &mut self.list_state());
        Ok(())
    }
}
```


## 2. Ratatui 0.30 API Best Practices

### 2.1 Layout::areas() -- Compile-Time Safe Splits

```rust
use ratatui::layout::{Layout, Constraint, Direction, Rect};

// OLD (0.28): Returns Vec<Rect>, runtime index panics possible
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(3), Constraint::Min(0)])
    .split(area);
let header = chunks[0]; // panic if wrong index

// NEW (0.30): Returns [Rect; N], compile-time checked
let [header, body] = Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(0),
]).areas(area);

// Even better with try_areas for fallible
let areas: [Rect; 3] = Layout::horizontal([
    Constraint::Percentage(20),
    Constraint::Min(40),
    Constraint::Percentage(20),
]).try_areas(area)?;
let [sidebar, main, panel] = areas;
```

### 2.2 Widget vs StatefulWidget

```rust
// Use Widget for stateless rendering (PREFERRED for most cases)
struct StatusBar<'a> {
    mode: &'a str,
    fps: u16,
}

impl Widget for &StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = Line::from(vec![
            Span::styled(self.mode, Style::default().bold()),
            Span::raw(" | "),
            Span::raw(format!("{}fps", self.fps)),
        ]);
        text.render(area, buf);
    }
}

// Use StatefulWidget ONLY when external mutable state is needed
// (scroll position, selection index, etc.)
struct TaskList<'a> {
    tasks: &'a [Task],
}

impl StatefulWidget for &TaskList<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut ListState) {
        let items: Vec<ListItem> = self.tasks.iter()
            .map(|t| ListItem::new(t.name.as_str()))
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().fg(Color::Yellow).bold())
            .highlight_symbol(">> ");
        StatefulWidget::render(list, area, buf, state);
    }
}
```

### 2.3 Flex Layout (new in 0.29+)

```rust
use ratatui::layout::Flex;

// SpaceEvenly: equal gaps including edges
let layout = Layout::horizontal([
    Constraint::Length(10),
    Constraint::Length(10),
]).flex(Flex::SpaceEvenly);

// SpaceBetween: no edge gaps, equal internal gaps
let layout = Layout::horizontal([
    Constraint::Length(10),
    Constraint::Length(10),
]).flex(Flex::SpaceBetween);

// Center: center the constraints
let layout = Layout::horizontal([
    Constraint::Length(20),
]).flex(Flex::Center);
```

### 2.4 Modern Style API

```rust
use ratatui::prelude::*;

// Fluent builder (0.30 preferred)
let style = Style::new()
    .fg(Color::Rgb(180, 190, 254))  // Catppuccin Lavender
    .bg(Color::Rgb(30, 30, 46))     // Catppuccin Base
    .bold()
    .italic();

// Stylize trait on any widget
let paragraph = Paragraph::new("styled text")
    .fg(Color::Yellow)
    .bold()
    .block(Block::bordered().title("Panel"));

// HSL colors
let color = Color::from_hsl(240.0, 0.8, 0.7);
```


## 3. State Management Patterns

### 3.1 Centralized State + Action Queue (gitui pattern)

The proven pattern for 100+ file apps. Used by gitui and adaptable for Nika.

```rust
// -- Centralized AppState --
pub struct AppState {
    // Navigation
    pub active_tab: TabId,
    pub overlay_stack: Vec<OverlayKind>,

    // Domain data
    pub workflows: Vec<Workflow>,
    pub run_results: HashMap<WorkflowId, RunResult>,

    // UI state
    pub focus: FocusManager,
    pub notifications: VecDeque<Notification>,
    pub loading: HashSet<LoadingKey>,
}

// -- Action queue for decoupled updates --
pub struct ActionQueue {
    tx: mpsc::UnboundedSender<Action>,
    rx: mpsc::UnboundedReceiver<Action>,
}

impl ActionQueue {
    pub fn send(&self, action: Action) {
        let _ = self.tx.send(action);
    }

    pub fn drain(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Ok(action) = self.rx.try_recv() {
            actions.push(action);
        }
        actions
    }
}
```

### 3.2 Async Data Loading with Channels

```rust
use tokio::sync::mpsc;

pub enum AsyncResult {
    WorkflowLoaded(Workflow),
    RunCompleted { id: WorkflowId, result: RunResult },
    Error(String),
}

// Spawn async work, send results back to UI thread
fn load_workflow(path: PathBuf, tx: mpsc::UnboundedSender<Action>) {
    tokio::spawn(async move {
        tx.send(Action::SetLoading(LoadingKey::Workflow, true)).ok();
        match parse_workflow(&path).await {
            Ok(wf) => tx.send(Action::WorkflowLoaded(wf)).ok(),
            Err(e) => tx.send(Action::Error(e.to_string())).ok(),
        };
        tx.send(Action::SetLoading(LoadingKey::Workflow, false)).ok();
    });
}

// In the main loop, actions from async tasks arrive naturally
fn handle_actions(&mut self) {
    while let Ok(action) = self.action_rx.try_recv() {
        match action {
            Action::WorkflowLoaded(wf) => {
                self.state.workflows.push(wf);
            }
            Action::SetLoading(key, loading) => {
                if loading {
                    self.state.loading.insert(key);
                } else {
                    self.state.loading.remove(&key);
                }
            }
            _ => {}
        }
    }
}
```

### 3.3 Focus Management

```rust
// -- Focus manager for multi-widget navigation --
#[derive(Debug)]
pub struct FocusManager {
    elements: Vec<FocusId>,
    current: usize,
}

impl FocusManager {
    pub fn new(elements: Vec<FocusId>) -> Self {
        Self { elements, current: 0 }
    }

    pub fn next(&mut self) {
        self.current = (self.current + 1) % self.elements.len();
    }

    pub fn prev(&mut self) {
        self.current = self.current
            .checked_sub(1)
            .unwrap_or(self.elements.len() - 1);
    }

    pub fn is_focused(&self, id: &FocusId) -> bool {
        self.elements.get(self.current) == Some(id)
    }

    pub fn current(&self) -> Option<&FocusId> {
        self.elements.get(self.current)
    }

    /// Replace the focus ring (e.g., when switching tabs)
    pub fn set_elements(&mut self, elements: Vec<FocusId>) {
        self.elements = elements;
        self.current = 0;
    }
}

// Usage in component:
fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
    let border_style = if self.focus_mgr.is_focused(&FocusId::TaskList) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::bordered()
        .title("Tasks")
        .border_style(border_style);
    // ...
}
```

### 3.4 Modal/Overlay Stack (gitui pattern)

```rust
pub enum OverlayKind {
    ProviderConfig,
    Confirmation { title: String, on_confirm: Action },
    Error(String),
    CommandPalette,
}

impl App {
    fn push_overlay(&mut self, kind: OverlayKind) {
        self.state.overlay_stack.push(kind);
    }

    fn pop_overlay(&mut self) {
        self.state.overlay_stack.pop();
    }

    /// Event routing: top overlay gets first crack
    fn route_event(&mut self, event: Event) -> Option<Action> {
        // Top overlay handles first
        if let Some(overlay) = self.state.overlay_stack.last_mut() {
            match overlay.handle_event(event) {
                EventResult::Consumed(action) => return action,
                EventResult::Dismissed => {
                    self.pop_overlay();
                    return Some(Action::Render);
                }
                EventResult::Passthrough => {} // fall through
            }
        }
        // Then active tab
        self.active_tab_component().handle_events(Some(event)).ok().flatten()
    }

    /// Rendering: base layer first, overlays on top
    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            let area = frame.area();
            // 1. Draw base tab
            self.active_tab_component().draw(frame, area)?;
            // 2. Draw overlays bottom-to-top
            for overlay in &mut self.state.overlay_stack {
                overlay.draw(frame, area)?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

pub enum EventResult {
    Consumed(Option<Action>),
    Dismissed,
    Passthrough,
}
```


## 4. Reference Projects (Architectural Breakdown)

### 4.1 gitui (extrawurst/gitui) -- Best Overall Architecture

| Aspect | Pattern |
|--------|---------|
| **Size** | ~100 files |
| **Architecture** | Component trait + centralized App + Action queue |
| **State** | App owns tabs, overlay stack, shared repo context |
| **Events** | Queue-based: event -> top overlay -> active tab -> App |
| **Popups** | `Vec<Overlay>` stack, draw bottom-to-top |
| **Keybindings** | `CommandInfo` struct per component, dynamic status bar |
| **Async** | Background git ops via channels |
| **URL** | https://github.com/gitui-org/gitui |

### 4.2 yazi (sxyazi/yazi) -- Best Async + Plugin Architecture

| Aspect | Pattern |
|--------|---------|
| **Size** | 200+ files across multiple crates |
| **Architecture** | Multi-crate: yazi-fm (TUI) / yazi-core (logic) / yazi-plugin (Lua) |
| **State** | Reactive pub-sub model, client-server DDS |
| **Async** | Tokio throughout, priority task scheduler with cancellation |
| **Preview** | Async decoders with 15+ terminal protocol support |
| **Plugin** | Lua-based, hot-loadable, typed hooks |
| **URL** | https://github.com/sxyazi/yazi |

### 4.3 bottom (ClementTsang/bottom) -- Best Testability

| Aspect | Pattern |
|--------|---------|
| **Size** | ~150 files |
| **Architecture** | MVC: controllers fetch data, models hold state, display renders |
| **State** | `StateManager` trait per widget type |
| **Testing** | 90%+ coverage, mock data providers, TestBackend snapshots |
| **Layout** | Dynamic recalculation on resize |
| **URL** | https://github.com/ClementTsang/bottom |

### 4.4 zellij (zellij-org/zellij) -- Best Plugin/Extensibility

| Aspect | Pattern |
|--------|---------|
| **Size** | 300+ files |
| **Architecture** | Event-driven, WASM plugins, client-server |
| **State** | Hierarchical: session > tab > pane |
| **Routing** | RouteId-based navigation |
| **Plugin** | WASM-based, sandboxed |
| **URL** | https://github.com/zellij-org/zellij |

### 4.5 television -- Best for Fuzzy Finder Patterns

| Aspect | Pattern |
|--------|---------|
| **Size** | ~60 files, compact but clean |
| **Architecture** | Stateful List + engine pattern |
| **URL** | https://github.com/alexpasmantier/television |


## 5. Animation Patterns

### 5.1 Tick-Based Animation (Manual)

The fundamental pattern. No dependencies required.

```rust
pub struct AnimationState {
    frame: u64,
    last_tick: Instant,
    tick_rate: Duration,
}

impl AnimationState {
    pub fn new(fps: u32) -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            tick_rate: Duration::from_secs_f64(1.0 / fps as f64),
        }
    }

    /// Call each loop iteration. Returns true if a new frame should render.
    pub fn tick(&mut self) -> bool {
        if self.last_tick.elapsed() >= self.tick_rate {
            self.frame += 1;
            self.last_tick = Instant::now();
            true
        } else {
            false
        }
    }

    pub fn frame(&self) -> u64 { self.frame }

    /// Normalized progress 0.0..1.0 for an animation of `duration` frames
    pub fn progress(&self, start_frame: u64, duration: u64) -> f64 {
        let elapsed = self.frame.saturating_sub(start_frame);
        (elapsed as f64 / duration as f64).min(1.0)
    }
}
```

### 5.2 Smooth Scrolling with Lerp

```rust
pub struct SmoothScroll {
    current: f64,
    target: f64,
    speed: f64,  // 0.0..1.0, higher = faster snap
}

impl SmoothScroll {
    pub fn new(speed: f64) -> Self {
        Self { current: 0.0, target: 0.0, speed }
    }

    pub fn set_target(&mut self, target: usize) {
        self.target = target as f64;
    }

    /// Call each frame
    pub fn update(&mut self) {
        self.current += (self.target - self.current) * self.speed;
        // Snap when close enough
        if (self.target - self.current).abs() < 0.5 {
            self.current = self.target;
        }
    }

    pub fn offset(&self) -> u16 {
        self.current.round() as u16
    }
}

// Usage in render:
fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
    self.scroll.update();
    let paragraph = Paragraph::new(self.content.clone())
        .scroll((self.scroll.offset(), 0));
    frame.render_widget(paragraph, area);
    Ok(())
}
```

### 5.3 Spinner / Loading Animation

```rust
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    frame: usize,
    interval: Duration,
    last_update: Instant,
    label: String,
}

impl Spinner {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            frame: 0,
            interval: Duration::from_millis(80),
            last_update: Instant::now(),
            label: label.into(),
        }
    }

    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
            self.last_update = Instant::now();
        }
    }
}

impl Widget for &Spinner {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = Line::from(vec![
            Span::styled(
                SPINNER_FRAMES[self.frame],
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" "),
            Span::raw(&self.label),
        ]);
        text.render(area, buf);
    }
}
```

### 5.4 TachyonFX Integration (Advanced)

For complex shader-like effects: fades, dissolves, slides.

```toml
# Cargo.toml
[dependencies]
tachyonfx = "0.22"  # Compatible with ratatui 0.30
```

```rust
use tachyonfx::{fx, Effect, EffectTimer, Interpolation};
use tachyonfx::fx::{fade_from, fade_to, parallel, sequence, slide_in};
use std::time::Instant;

pub struct TransitionManager {
    effects: Vec<(Rect, Effect)>,
    last_frame: Instant,
}

impl TransitionManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            last_frame: Instant::now(),
        }
    }

    /// Trigger a fade-in on a specific area
    pub fn fade_in(&mut self, area: Rect, duration_ms: u32) {
        let effect = fade_from(
            Color::Black,
            EffectTimer::from_ms(duration_ms, Interpolation::CubicOut),
        );
        self.effects.push((area, effect));
    }

    /// Trigger a slide-in from direction
    pub fn slide_in(&mut self, area: Rect, direction: Direction, duration_ms: u32) {
        let effect = slide_in(
            direction,
            area.width.max(area.height),
            EffectTimer::from_ms(duration_ms, Interpolation::CubicOut),
        );
        self.effects.push((area, effect));
    }

    /// Call after rendering widgets but before frame.flush()
    pub fn process(&mut self, buf: &mut Buffer) {
        let elapsed = self.last_frame.elapsed();
        self.last_frame = Instant::now();

        self.effects.retain_mut(|(area, effect)| {
            // process returns true while effect is still running
            effect.process(elapsed, buf, *area)
        });
    }
}
```

### 5.5 Recommended Tick Rate

| Scenario | Tick Rate | Notes |
|----------|-----------|-------|
| Static UI (forms, config) | 4-10 fps | Saves CPU, events still responsive |
| Active UI (lists, navigation) | 30 fps | Good balance |
| Animations active | 60 fps | Smooth motion, standard target |
| Heavy rendering (large buffers) | 30 fps | Terminal can't keep up faster anyway |

The official template separates tick_rate (logic updates) from frame_rate (rendering):
```rust
Tui::new()?
    .tick_rate(4.0)    // Logic ticks: 4/sec (enough for most state)
    .frame_rate(60.0)  // Render frames: 60/sec (smooth animations)
```


## 6. Testing Patterns

### 6.1 TestBackend for Widget Testing

```rust
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn test_status_bar_renders_mode() {
    let backend = TestBackend::new(40, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let status = StatusBar { mode: "NORMAL", fps: 60 };

    terminal.draw(|frame| {
        frame.render_widget(&status, frame.area());
    }).unwrap();

    let buf = terminal.backend().buffer();
    let content: String = (0..40)
        .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
        .collect::<String>()
        .trim_end()
        .to_string();
    assert!(content.contains("NORMAL"));
    assert!(content.contains("60fps"));
}
```

### 6.2 Snapshot Testing with insta

```rust
use insta::assert_snapshot;

#[test]
fn test_task_list_snapshot() {
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    let tasks = vec![
        Task { name: "fetch:api".into(), status: TaskStatus::Running },
        Task { name: "infer:gpt4".into(), status: TaskStatus::Pending },
    ];
    let list = TaskList { tasks: &tasks };
    let mut state = ListState::default().with_selected(Some(0));

    terminal.draw(|frame| {
        frame.render_stateful_widget(&list, frame.area(), &mut state);
    }).unwrap();

    assert_snapshot!(terminal.backend().buffer().to_string());
}
// Run: cargo insta review  -- to approve/reject snapshots
```

### 6.3 Testing State Transitions (No UI)

```rust
#[test]
fn test_runner_model_select_next() {
    let mut model = RunnerModel {
        tasks: vec![task("a"), task("b"), task("c")],
        selected: 0,
        ..Default::default()
    };

    // Simulate action
    model.update(Action::SelectNext);
    assert_eq!(model.selected, 1);

    model.update(Action::SelectNext);
    assert_eq!(model.selected, 2);

    // Boundary: should not exceed length
    model.update(Action::SelectNext);
    assert_eq!(model.selected, 2);
}

#[test]
fn test_runner_model_task_completion() {
    let mut model = RunnerModel {
        tasks: vec![task("a"), task("b")],
        ..Default::default()
    };

    model.update(Action::TaskCompleted(TaskId(0)));
    assert_eq!(model.tasks[0].status, TaskStatus::Done);
    assert_eq!(model.tasks[1].status, TaskStatus::Pending);
}
```


## 7. Applicability to Nika TUI

### Current Nika Structure (164 files)

```
tools/nika/src/tui/
  app/          -- App struct, events, lifecycle, routing
  state/        -- Centralized state modules
  views/        -- View-level components (chat, etc.)
  widgets/      -- Reusable widgets (panels, progress, tree, task_box, provider_modal)
  highlight/    -- Syntax highlighting
  providers/    -- Provider status/icons
  wizard/       -- Setup wizard
  tokens/       -- Token display
```

### Recommendations

1. **Nika already follows the Component + Action pattern** -- the `app/events.rs` + `state/`
   separation is close to the gitui/template architecture. Formalize with a `Component` trait if
   not already done.

2. **Action enum consolidation** -- ensure all cross-component communication goes through a single
   `Action` enum. This is the message bus that makes the architecture testable.

3. **Overlay stack for modals** -- the `provider_modal` and any future modals should use a
   `Vec<OverlayKind>` stack with top-first event routing (gitui pattern).

4. **Focus management** -- with multiple panes (Studio, Runner, Chat, Settings), a `FocusManager`
   with per-tab focus rings would clean up keyboard navigation.

5. **Animation for `nika run` output** -- use the tick-based spinner + smooth scroll patterns above.
   TachyonFX is overkill unless you want view transition effects.

6. **Testing** -- add TestBackend + insta snapshots for widget regression testing. Test state
   transitions separately from rendering.


## Sources

1. **ratatui.rs/templates/component/** -- Official component template documentation
2. **github.com/ratatui/templates/tree/main/component** -- Source code for component template
3. **ratatui 0.30 changelog and API docs** -- docs.rs/ratatui
4. **github.com/gitui-org/gitui** -- gitui architecture reference
5. **github.com/sxyazi/yazi** -- yazi multi-crate architecture
6. **github.com/ClementTsang/bottom** -- bottom testing patterns
7. **github.com/zellij-org/zellij** -- zellij plugin architecture
8. **github.com/junkdog/tachyonfx** -- TachyonFX animation library
9. **github.com/alexpasmantier/television** -- television fuzzy finder
10. **Perplexity AI searches** -- aggregated from multiple 2025 web sources

## Methodology

- Tools used: Perplexity sonar-pro (6 queries), Firecrawl scrape (3 pages)
- Pages analyzed: ~15 sources cross-referenced
- Focus: Patterns applicable to Nika's 164-file TUI codebase on ratatui 0.30

## Confidence Level

**High** for architectural patterns (Component trait, Action enum, overlay stack) -- these are
battle-tested in production apps with 100K+ users (gitui, yazi, bottom).

**Medium** for ratatui 0.30 specific APIs -- some API details (try_areas, run()) may have evolved
since last docs update. Verify against `cargo doc` locally.

**Medium** for TachyonFX -- the API examples are illustrative; verify against the actual crate
docs as the DSL syntax may differ from what Perplexity returned.

## Further Research Suggestions

- Clone gitui and trace the event flow from keypress to render for one specific action
- Benchmark ratatui 0.30 `try_areas()` vs `split()` in Nika's layout code
- Evaluate whether Nika should adopt the ratatui component template's `Tui` wrapper
- Investigate `ratatui-interact` for form-heavy views (Settings tab)
- Look at `tui-textarea` for the Chat view's input handling
