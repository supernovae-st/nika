# Research Report: Ratatui Component Architecture (2025-2026)

## Summary

The official ratatui/templates repository provides a **Component trait pattern** as the
recommended architecture for large apps. Combined with an **Action enum** message bus,
**overlay rendering via `Clear` + `Rect::centered`**, and **tachyonfx** for terminal
animations, this gives us a complete, battle-tested foundation for the Nika TUI refactor.

---

## 1. Component Trait (Official Template)

**Source:** `github.com/ratatui/templates` -- `component-generated/` directory

The official Component trait has 7 methods (2 required, 5 optional with defaults):

```rust
pub trait Component {
    // --- Registration (called once at startup) ---

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        let _ = tx;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        let _ = config;
        Ok(())
    }

    fn init(&mut self, area: Size) -> Result<()> {
        let _ = area;
        Ok(())
    }

    // --- Event Handling (per-frame) ---

    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        let _ = key;
        Ok(None)
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        let _ = mouse;
        Ok(None)
    }

    // --- State Update (REQUIRED -- react to actions) ---

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let _ = action;
        Ok(None)
    }

    // --- Rendering (REQUIRED) ---

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;
}
```

### Key Design Decisions

1. **Events produce Actions, Actions mutate state** -- clean unidirectional flow
2. **Components can emit Actions** -- via `update()` returning `Some(Action)` or via the
   stored `action_tx` sender
3. **Components are stored as `Vec<Box<dyn Component>>`** -- App iterates them
4. **Each component receives the full area** -- components decide their own layout slice

---

## 2. Action Enum (Message Bus)

```rust
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
}
```

### For Nika's 3-View Architecture

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Lifecycle
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    Suspend,
    Resume,

    // Navigation
    SwitchView(View),           // Studio | Runner | Chat | Settings
    ToggleOverlay(Overlay),     // CommandPalette | Help | Confirm(...)
    DismissOverlay,

    // View-specific
    StudioAction(StudioAction),
    RunnerAction(RunnerAction),
    ChatAction(ChatAction),

    // Effects
    TriggerEffect(EffectId),

    // Errors
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View { Studio, Runner, Chat, Settings }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay { CommandPalette, Help, Confirm(String) }
```

---

## 3. App Main Loop (The Orchestrator)

```rust
pub struct App {
    config: Config,
    components: Vec<Box<dyn Component>>,
    should_quit: bool,
    should_suspend: bool,
    mode: Mode,
    last_tick_key_events: Vec<KeyEvent>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
}

// The main loop has 3 phases per frame:
// 1. handle_events() -- converts crossterm events -> Actions
// 2. handle_actions() -- drains action queue, dispatches to components
// 3. render()         -- calls draw() on each component

impl App {
    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        tui.enter()?;

        // One-time registration
        for component in self.components.iter_mut() {
            component.register_action_handler(self.action_tx.clone())?;
            component.register_config_handler(self.config.clone())?;
            component.init(tui.size()?)?;
        }

        loop {
            self.handle_events(&mut tui).await?;  // Phase 1
            self.handle_actions(&mut tui)?;         // Phase 2
            // Phase 3 is triggered by Action::Render inside handle_actions
            if self.should_quit { break; }
        }
        tui.exit()?;
        Ok(())
    }

    fn handle_actions(&mut self, tui: &mut Tui) -> Result<()> {
        while let Ok(action) = self.action_rx.try_recv() {
            match action {
                Action::Tick => { self.last_tick_key_events.drain(..); }
                Action::Quit => self.should_quit = true,
                Action::Resize(w, h) => { /* resize terminal */ }
                Action::Render => self.render(tui)?,
                _ => {}
            }
            // Every action is broadcast to every component
            for component in self.components.iter_mut() {
                if let Some(action) = component.update(action.clone())? {
                    self.action_tx.send(action)?;
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            for component in self.components.iter_mut() {
                if let Err(err) = component.draw(frame, frame.area()) {
                    let _ = self.action_tx
                        .send(Action::Error(format!("Failed to draw: {:?}", err)));
                }
            }
        })?;
        Ok(())
    }
}
```

---

## 4. Event Loop Infrastructure (Tui struct)

The template uses a dedicated async task for event polling with separate tick and render
intervals:

```rust
pub struct Tui {
    pub terminal: ratatui::Terminal<Backend<Stdout>>,
    pub task: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub event_rx: UnboundedReceiver<Event>,
    pub event_tx: UnboundedSender<Event>,
    pub frame_rate: f64,  // default 60.0
    pub tick_rate: f64,   // default 4.0
}

#[derive(Clone, Debug)]
pub enum Event {
    Init,
    Quit,
    Error,
    Closed,
    Tick,
    Render,
    FocusGained,
    FocusLost,
    Paste(String),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
}

// The event loop runs in a separate tokio task:
async fn event_loop(event_tx: UnboundedSender<Event>, ...) {
    let mut event_stream = EventStream::new();
    let mut tick_interval = interval(Duration::from_secs_f64(1.0 / tick_rate));
    let mut render_interval = interval(Duration::from_secs_f64(1.0 / frame_rate));

    loop {
        let event = tokio::select! {
            _ = cancellation_token.cancelled() => break,
            _ = tick_interval.tick() => Event::Tick,
            _ = render_interval.tick() => Event::Render,
            crossterm_event = event_stream.next().fuse() => {
                // convert CrosstermEvent -> Event
            }
        };
        event_tx.send(event).ok();
    }
}
```

---

## 5. Modal Overlay Pattern

**Source:** `ratatui/ratatui/examples/apps/popup/`

The official pattern is dead simple -- **render the overlay LAST** with `Clear` underneath:

```rust
fn render(frame: &mut Frame, show_popup: bool) {
    let area = frame.area();

    // 1. Render the main content (always)
    frame.render_widget(Block::bordered().title("Content").on_blue(), area);

    // 2. If overlay active, render on top
    if show_popup {
        // Center the popup: 60% wide, 20% tall
        let popup_area = area.centered(
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        );

        // Clear the background behind the popup
        frame.render_widget(Clear, popup_area);

        // Render the popup content
        let popup = Paragraph::new("Hello from popup")
            .block(Block::bordered().title("Popup"));
        frame.render_widget(popup, popup_area);
    }
}
```

### For Nika: Overlay Stack Pattern

```rust
pub struct OverlayStack {
    layers: Vec<Box<dyn Overlay>>,
}

pub trait Overlay: Component {
    /// Whether this overlay captures all keyboard input (modal)
    fn is_modal(&self) -> bool { true }

    /// The area this overlay occupies
    fn layout(&self, parent: Rect) -> Rect;
}

impl OverlayStack {
    pub fn push(&mut self, overlay: Box<dyn Overlay>) { ... }
    pub fn pop(&mut self) -> Option<Box<dyn Overlay>> { ... }
    pub fn is_empty(&self) -> bool { self.layers.is_empty() }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // Only the topmost overlay receives input
        if let Some(top) = self.layers.last_mut() {
            return top.handle_key_event(key);
        }
        Ok(None)
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        for overlay in self.layers.iter_mut() {
            let overlay_area = overlay.layout(area);
            frame.render_widget(Clear, overlay_area);
            overlay.draw(frame, overlay_area)?;
        }
        Ok(())
    }
}

// Command Palette overlay
pub struct CommandPalette {
    input: String,
    filtered_commands: Vec<Command>,
    selected: usize,
    action_tx: Option<UnboundedSender<Action>>,
}

impl Overlay for CommandPalette {
    fn layout(&self, parent: Rect) -> Rect {
        // Top-center, 60% width, max 15 lines
        let width = (parent.width * 60 / 100).min(80);
        let height = (self.filtered_commands.len() as u16 + 3).min(15);
        Rect::new(
            parent.x + (parent.width - width) / 2,
            parent.y + 2,
            width,
            height,
        )
    }
}

// Help overlay
pub struct HelpOverlay { ... }

impl Overlay for HelpOverlay {
    fn layout(&self, parent: Rect) -> Rect {
        parent.centered(Constraint::Percentage(80), Constraint::Percentage(80))
    }
}
```

---

## 6. Responsive Layout Pattern

Ratatui uses `Rect::centered()` and conditional layout based on terminal size:

```rust
fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
    // Responsive: switch between layouts based on terminal width
    if area.width < 60 {
        // Compact: stack vertically
        self.draw_compact(frame, area)
    } else if area.width < 120 {
        // Normal: sidebar + main
        self.draw_normal(frame, area)
    } else {
        // Wide: sidebar + main + detail panel
        self.draw_wide(frame, area)
    }
}

fn draw_normal(&self, frame: &mut Frame, area: Rect) {
    let [sidebar, main] = Layout::horizontal([
        Constraint::Length(30),
        Constraint::Min(40),
    ]).areas(area);

    self.render_sidebar(frame, sidebar);
    self.render_main(frame, main);
}

fn draw_compact(&self, frame: &mut Frame, area: Rect) {
    // In compact mode, only show the active panel
    match self.focus {
        Focus::Sidebar => self.render_sidebar(frame, area),
        Focus::Main    => self.render_main(frame, area),
    }
}

// Minimum size guard
fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
    if area.width < 40 || area.height < 10 {
        let msg = Paragraph::new("Terminal too small\nMinimum: 40x10")
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return Ok(());
    }
    // ... normal rendering
}
```

---

## 7. tachyonfx Animation Effects

**Source:** `github.com/junkdog/tachyonfx` v0.7.0

### Available Effects Catalog

| Category | Effects |
|----------|---------|
| **Color** | `fade_from`, `fade_from_fg`, `fade_to`, `fade_to_fg`, `hsl_shift`, `hsl_shift_fg`, `term256_colors` |
| **Text/Char** | `coalesce`, `dissolve`, `slide_in`, `slide_out`, `sweep_in`, `sweep_out` |
| **Timing** | `consume_tick`, `never_complete`, `ping_pong`, `prolong_start`, `prolong_end`, `repeat`, `repeating`, `sleep`, `timed_never_complete`, `with_duration`, `delay` |
| **Geometry** | `translate`, `translate_buf`, `resize_area` |
| **Combination** | `parallel`, `sequence` |
| **Custom** | `effect_fn` (cell iterator), `effect_fn_buf` (buffer), `offscreen_buffer` |
| **Special** | `Glitch` (random char corruption) |

### Integration Pattern

```rust
use tachyonfx::{fx, Effect, EffectRenderer, Shader, Duration, Interpolation};
use tachyonfx::fx::Direction;

struct MyComponent {
    // Store active effects
    enter_effect: Option<Effect>,
    last_frame: std::time::Instant,
}

impl Component for MyComponent {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let elapsed: Duration = self.last_frame.elapsed().into();
        self.last_frame = std::time::Instant::now();

        // 1. Render widget normally
        let paragraph = Paragraph::new("Hello").block(Block::bordered());
        frame.render_widget(paragraph, area);

        // 2. Apply effect on top (post-render)
        if let Some(effect) = self.enter_effect.as_mut() {
            if effect.running() {
                frame.render_effect(effect, area, elapsed);
            } else {
                self.enter_effect = None;
            }
        }

        Ok(())
    }
}

// Creating effects for view transitions
fn view_enter_effect() -> Effect {
    fx::parallel(&[
        fx::fade_from_fg(
            Color::DarkGray,
            tachyonfx::EffectTimer::from_ms(300, Interpolation::QuadOut),
        ),
        fx::sweep_in(
            Direction::LeftToRight,
            10,
            0,
            Color::Black,
            tachyonfx::EffectTimer::from_ms(400, Interpolation::CubicOut),
        ),
    ])
}

fn view_exit_effect() -> Effect {
    fx::fade_to_fg(
        Color::DarkGray,
        tachyonfx::EffectTimer::from_ms(200, Interpolation::QuadIn),
    )
}

fn glitch_on_error() -> Effect {
    fx::timed_never_complete(
        Duration::from_millis(500),
        fx::Glitch::builder()
            .cell_glitch_ratio(0.03)
            .action_start_delay_ms(0..100)
            .action_ms(50..200)
            .build()
            .into_effect(),
    )
}
```

### CellFilter (Targeted Effects)

```rust
use tachyonfx::CellFilter;

// Apply effect only to text cells (not spaces)
effect.with_cell_selection(CellFilter::Text);

// Apply to border only
let margin = Margin::new(1, 1);
effect.with_cell_selection(CellFilter::Outer(margin));

// Apply to content only (inside border)
effect.with_cell_selection(CellFilter::Inner(margin));

// Combine filters
effect.with_cell_selection(CellFilter::AllOf(vec![
    CellFilter::Outer(margin),
    CellFilter::Text,
]));

// Layout-based selection
let layout = Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)]);
effect.with_cell_selection(CellFilter::Layout(layout, 1)); // index 1 = content area
```

---

## 8. Concrete Architecture for Nika TUI (3 Views + Overlays)

```
App (orchestrator)
 |
 |-- action_tx / action_rx  (mpsc channel)
 |-- Tui                    (terminal + event loop)
 |-- active_view: View      (Studio | Runner | Chat | Settings)
 |
 |-- components: HashMap<View, Box<dyn Component>>
 |     |-- Studio   (file browser + editor + preview)
 |     |-- Runner   (DAG view + log output)
 |     |-- Chat     (message list + input)
 |     |-- Settings (config panels)
 |
 |-- overlays: OverlayStack
 |     |-- CommandPalette
 |     |-- HelpScreen
 |     |-- ConfirmDialog
 |
 |-- effects: EffectManager
       |-- view_transition: Option<Effect>
       |-- notifications: Vec<(Effect, Rect)>
```

### Rendering Order

```rust
fn render(&mut self, tui: &mut Tui) -> Result<()> {
    tui.draw(|frame| {
        let area = frame.area();

        // 1. Render chrome (tab bar, status bar)
        let [tab_bar, content, status_bar] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ]).areas(area);

        self.render_tab_bar(frame, tab_bar);

        // 2. Render active view
        if let Some(view) = self.components.get_mut(&self.active_view) {
            view.draw(frame, content)?;
        }

        // 3. Apply transition effects
        if let Some(fx) = self.effects.view_transition.as_mut() {
            if fx.running() {
                frame.render_effect(fx, content, self.last_tick);
            }
        }

        self.render_status_bar(frame, status_bar);

        // 4. Render overlays (LAST -- on top of everything)
        self.overlays.draw(frame, area)?;
    })?;
    Ok(())
}
```

### Input Routing

```rust
fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
    // 1. Overlays eat input first (if modal)
    if !self.overlays.is_empty() {
        if let Some(action) = self.overlays.handle_key_event(key)? {
            self.action_tx.send(action)?;
        }
        return Ok(());
    }

    // 2. Global keybindings (Ctrl-P for command palette, etc.)
    if let Some(action) = self.check_global_keys(key) {
        self.action_tx.send(action)?;
        return Ok(());
    }

    // 3. Active view gets the event
    if let Some(view) = self.components.get_mut(&self.active_view) {
        if let Some(action) = view.handle_key_event(key)? {
            self.action_tx.send(action)?;
        }
    }

    Ok(())
}
```

---

## Sources

1. [ratatui/templates (component-generated)](https://github.com/ratatui/templates/tree/main/component-generated) -- Official Component trait, Action enum, App loop, Tui event loop
2. [ratatui popup example](https://github.com/ratatui/ratatui/tree/main/examples/apps/popup) -- Official overlay pattern with Clear + centered
3. [ratatui demo2](https://github.com/ratatui/ratatui/tree/main/examples/apps/demo2) -- Tab-based multi-view app with Widget impl
4. [junkdog/tachyonfx](https://github.com/junkdog/tachyonfx) v0.7.0 -- All animation effects, Shader trait, EffectRenderer, CellFilter
5. [tachyonfx open-window example](https://github.com/junkdog/tachyonfx/blob/main/examples/open-window.rs) -- Animated popup overlays with glitch/dissolve/sweep effects
6. [tachyonfx common/window.rs](https://github.com/junkdog/tachyonfx/blob/main/examples/common/window.rs) -- OpenWindow shader composing pre-render + content effects

## Methodology

- Tools used: GitHub API, raw file fetching
- Files analyzed: 15 source files across 3 repositories
- Versions: ratatui latest (main branch), tachyonfx 0.7.0, templates latest

## Confidence Level

**High** -- All code comes from official repositories and maintained crates.
The Component trait pattern is the official recommendation from the ratatui team.
tachyonfx is the de facto standard for ratatui animations (used in demos and recommended in ratatui docs).
