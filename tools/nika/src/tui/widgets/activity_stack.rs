//! Activity Stack Widget
//!
//! Shows hot (executing), warm (recent), and queued operations.

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};

use crate::tui::theme::VerbColor;

/// Activity temperature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityTemp {
    /// Currently executing
    Hot,
    /// Recently completed (< 30s)
    Warm,
    /// Waiting for dependencies
    #[default]
    Queued,
}

impl ActivityTemp {
    pub fn header(&self) -> (&'static str, Color) {
        match self {
            Self::Hot => ("🔥 HOT (executing now)", Color::Rgb(251, 146, 60)), // Orange
            Self::Warm => ("🟡 WARM (recently completed)", Color::Rgb(250, 204, 21)), // Yellow
            Self::Queued => ("⚪ QUEUED (waiting)", Color::Rgb(156, 163, 175)), // Gray
        }
    }
}

/// Activity item
#[derive(Debug, Clone)]
pub struct ActivityItem {
    pub id: String,
    pub verb: String,
    pub temp: ActivityTemp,
    pub started: Option<Instant>,
    pub duration: Option<Duration>,
    pub tokens: Option<(u32, u32)>, // (in, out)
    pub waiting_on: Option<String>,
    pub detail: Option<String>,
    pub frame: u8,
}

impl ActivityItem {
    /// Create a hot (currently executing) activity
    pub fn hot(id: impl Into<String>, verb: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Hot,
            started: Some(Instant::now()),
            duration: None,
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    /// Create a warm (recently completed) activity
    pub fn warm(id: impl Into<String>, verb: impl Into<String>, duration: Duration) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Warm,
            started: None,
            duration: Some(duration),
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    /// Create a queued (waiting) activity
    pub fn queued(
        id: impl Into<String>,
        verb: impl Into<String>,
        waiting_on: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Queued,
            started: None,
            duration: None,
            tokens: None,
            waiting_on: Some(waiting_on.into()),
            detail: None,
            frame: 0,
        }
    }

    pub fn with_tokens(mut self, tokens_in: u32, tokens_out: u32) -> Self {
        self.tokens = Some((tokens_in, tokens_out));
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Get elapsed time for hot items
    pub fn elapsed(&self) -> Option<Duration> {
        self.started.map(|s| s.elapsed())
    }

    fn verb_icon(&self) -> &'static str {
        VerbColor::from_verb(&self.verb).icon()
    }

    fn verb_color(&self) -> Color {
        VerbColor::from_verb(&self.verb).rgb()
    }
}

/// Activity stack widget
pub struct ActivityStack<'a> {
    items: &'a [ActivityItem],
    frame: u8,
}

impl<'a> ActivityStack<'a> {
    pub fn new(items: &'a [ActivityItem]) -> Self {
        Self { items, frame: 0 }
    }

    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    fn render_spinner(&self) -> &'static str {
        let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
        spinners[(self.frame as usize) % spinners.len()]
    }
}

impl Widget for ActivityStack<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 {
            return;
        }

        let block = Block::default()
            .title(" 🎯 ACTIVITY STACK ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(75, 85, 99)));

        let inner = block.inner(area);
        block.render(area, buf);

        // Group items by temperature
        let hot: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.temp == ActivityTemp::Hot)
            .collect();
        let warm: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.temp == ActivityTemp::Warm)
            .collect();
        let queued: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.temp == ActivityTemp::Queued)
            .collect();

        let mut y = inner.y;

        // Hot section
        if !hot.is_empty() && y < inner.y + inner.height {
            let (header, color) = ActivityTemp::Hot.header();
            buf.set_string(
                inner.x,
                y,
                header,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            );
            y += 1;

            for item in hot.iter().take(2) {
                if y >= inner.y + inner.height {
                    break;
                }

                let spinner = self.render_spinner();
                let elapsed = item
                    .elapsed()
                    .map(|d| format!("{:.1}s", d.as_secs_f64()))
                    .unwrap_or_default();

                let line = format!(
                    "└── {} {}:{}  {} {}",
                    item.verb_icon(),
                    item.verb,
                    item.id,
                    spinner,
                    elapsed
                );
                buf.set_string(inner.x, y, &line, Style::default().fg(item.verb_color()));

                // Token info
                if let Some((t_in, t_out)) = item.tokens {
                    let token_x = inner.x + line.chars().count() as u16 + 1;
                    if token_x < inner.x + inner.width {
                        buf.set_string(
                            token_x,
                            y,
                            format!("{}/{} tok", t_out, t_in),
                            Style::default().fg(Color::DarkGray),
                        );
                    }
                }
                y += 1;
            }
        }

        // Warm section
        if !warm.is_empty() && y < inner.y + inner.height {
            if !hot.is_empty() {
                y += 1; // spacing
            }

            if y < inner.y + inner.height {
                let (header, color) = ActivityTemp::Warm.header();
                buf.set_string(inner.x, y, header, Style::default().fg(color));
                y += 1;
            }

            for item in warm.iter().take(3) {
                if y >= inner.y + inner.height {
                    break;
                }

                let dur = item
                    .duration
                    .map(|d| format!("{:.1}s", d.as_secs_f64()))
                    .unwrap_or_default();

                let prefix = format!("├── {} {}:{}", item.verb_icon(), item.verb, item.id);
                buf.set_string(inner.x, y, &prefix, Style::default().fg(Color::DarkGray));

                // Success marker and duration
                let success_x = inner.x + prefix.chars().count() as u16 + 1;
                if success_x < inner.x + inner.width {
                    let detail = item.detail.as_deref().unwrap_or("");
                    let detail_short = if detail.len() > 20 {
                        format!("{}...", &detail[..17])
                    } else {
                        detail.to_string()
                    };
                    buf.set_string(
                        success_x,
                        y,
                        format!("✅ {} {}", dur, detail_short),
                        Style::default().fg(Color::Rgb(34, 197, 94)),
                    );
                }
                y += 1;
            }
        }

        // Queued section
        if !queued.is_empty() && y < inner.y + inner.height {
            if !hot.is_empty() || !warm.is_empty() {
                y += 1; // spacing
            }

            if y < inner.y + inner.height {
                let (header, color) = ActivityTemp::Queued.header();
                buf.set_string(inner.x, y, header, Style::default().fg(color));
                y += 1;
            }

            for item in queued.iter().take(3) {
                if y >= inner.y + inner.height {
                    break;
                }

                let waiting = item.waiting_on.as_deref().unwrap_or("dependencies");
                buf.set_string(
                    inner.x,
                    y,
                    format!(
                        "├── {} {}:{}  ○ waiting on {}",
                        item.verb_icon(),
                        item.verb,
                        item.id,
                        waiting
                    ),
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
                y += 1;
            }
        }

        // Empty state
        if hot.is_empty() && warm.is_empty() && queued.is_empty() && y < inner.y + inner.height {
            buf.set_string(
                inner.x,
                y,
                "(no activity)",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_item_hot() {
        let item = ActivityItem::hot("task1", "infer");
        assert_eq!(item.id, "task1");
        assert_eq!(item.verb, "infer");
        assert_eq!(item.temp, ActivityTemp::Hot);
        assert!(item.started.is_some());
    }

    #[test]
    fn test_activity_item_warm() {
        let item = ActivityItem::warm("task2", "exec", Duration::from_secs(5));
        assert_eq!(item.temp, ActivityTemp::Warm);
        assert_eq!(item.duration, Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_activity_item_queued() {
        let item = ActivityItem::queued("task3", "fetch", "task1");
        assert_eq!(item.temp, ActivityTemp::Queued);
        assert_eq!(item.waiting_on, Some("task1".to_string()));
    }

    #[test]
    fn test_verb_icon() {
        let item = ActivityItem::hot("t", "infer");
        assert_eq!(item.verb_icon(), "🧠");

        let item = ActivityItem::hot("t", "exec");
        assert_eq!(item.verb_icon(), "⚡");

        let item = ActivityItem::hot("t", "invoke");
        assert_eq!(item.verb_icon(), "🔧");
    }

    #[test]
    fn test_verb_color() {
        let item = ActivityItem::hot("t", "infer");
        assert_eq!(item.verb_color(), Color::Rgb(139, 92, 246)); // Violet

        let item = ActivityItem::hot("t", "exec");
        assert_eq!(item.verb_color(), Color::Rgb(245, 158, 11)); // Amber
    }

    #[test]
    fn test_with_tokens() {
        let item = ActivityItem::hot("t", "infer").with_tokens(100, 50);
        assert_eq!(item.tokens, Some((100, 50)));
    }

    #[test]
    fn test_with_detail() {
        let item =
            ActivityItem::warm("t", "exec", Duration::from_secs(1)).with_detail("build completed");
        assert_eq!(item.detail, Some("build completed".to_string()));
    }

    #[test]
    fn test_elapsed() {
        let item = ActivityItem::hot("t", "infer");
        // Elapsed should be very small (just created)
        let elapsed = item.elapsed();
        assert!(elapsed.is_some());
        assert!(elapsed.unwrap() < Duration::from_secs(1));

        // Queued items have no elapsed
        let item = ActivityItem::queued("t", "exec", "other");
        assert!(item.elapsed().is_none());
    }

    #[test]
    fn test_activity_temp_headers() {
        let (header, _) = ActivityTemp::Hot.header();
        assert!(header.contains("HOT"));

        let (header, _) = ActivityTemp::Warm.header();
        assert!(header.contains("WARM"));

        let (header, _) = ActivityTemp::Queued.header();
        assert!(header.contains("QUEUED"));
    }

    #[test]
    fn test_activity_stack_frame() {
        let items = vec![];
        let stack = ActivityStack::new(&items).frame(5);
        assert_eq!(stack.frame, 5);
    }

    #[test]
    fn test_render_spinner() {
        let items = vec![];
        let stack = ActivityStack::new(&items).frame(0);
        let s1 = stack.render_spinner();

        let stack = ActivityStack::new(&items).frame(1);
        let s2 = stack.render_spinner();

        assert_ne!(s1, s2);
    }

    #[test]
    fn test_default_temp() {
        assert_eq!(ActivityTemp::default(), ActivityTemp::Queued);
    }
}
