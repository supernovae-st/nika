//! Spinner constants and progress bar templates for LiveRenderer.
//!
//! Tick strings use Braille dots — the de facto standard for CLI spinners.
//! All chars are Narrow East Asian Width (eaw=N), matching the icon palette.

use std::time::Duration;

/// Braille dot spinner — animates as: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
pub const TICK_STRINGS: &[&str] = &[
    "\u{280B}", // ⠋
    "\u{2819}", // ⠙
    "\u{2839}", // ⠹
    "\u{2838}", // ⠸
    "\u{283C}", // ⠼
    "\u{2834}", // ⠴
    "\u{2826}", // ⠦
    "\u{2827}", // ⠧
    "\u{2807}", // ⠇
    "\u{280F}", // ⠏
];

/// Final tick char shown after `finish_with_message()`.
/// Not used directly — the finish message embeds the icon.
pub const TICK_DONE: &str = " ";

/// Spinner tick interval (80ms = ~12.5 fps — smooth without CPU waste).
pub const TICK_INTERVAL: Duration = Duration::from_millis(80);

/// Maximum number of task bars displayed at once.
/// DAGs with more tasks show "+N more pending" instead of all bars.
pub const MAX_VISIBLE_TASKS: usize = 24;

// ── Progress bar templates (indicatif syntax) ─────────────────────────

/// Overall progress bar at the bottom.
///
/// ```text
///   ━━━━━━━━━╸─────────────────── 2/6 (33%)  +3.1s  ETA 6s  $0.004
/// ```
pub const OVERALL_TEMPLATE: &str =
    "  {bar:30.cyan/dim} {pos}/{len} ({percent}%)  {elapsed_precise}  ETA {eta}  {msg}";

/// Progress bar characters: filled ━, tip ╸, empty ─
pub const PROGRESS_CHARS: &str = "\u{2501}\u{257A}\u{2500}";

/// Task bar in running state (with spinner).
///
/// ```text
///   ⠹ ✧ fetch_data       running  +2.3s  out:1.2k
/// ```
/// Uses `{prefix}` for stable part (verb + id), `{msg}` for status label (running/turn N),
/// and `{tokens}` for the live streaming counter (registered via `with_key`, zero-alloc).
/// The `{tokens}` closure writes the leading spaces itself ("  out:1.2k") so the line
/// ends cleanly with no trailing whitespace when no tokens have been received yet.
/// `{elapsed}` auto-updates on every tick — no event needed for live time.
pub const TASK_RUNNING_TEMPLATE: &str = "  {spinner:.cyan} {prefix} {msg}  {elapsed:.dim}{tokens}";

/// Task bar in pending/completed state (no spinner, space placeholder).
pub const TASK_STATIC_TEMPLATE: &str = "  {msg}";

/// Separator line between scrolling log and fixed task bars.
pub const SEPARATOR_TEMPLATE: &str = "  {msg}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_strings_are_10_frames() {
        assert_eq!(TICK_STRINGS.len(), 10);
    }

    #[test]
    fn tick_interval_is_reasonable() {
        assert!(TICK_INTERVAL.as_millis() >= 50);
        assert!(TICK_INTERVAL.as_millis() <= 200);
    }

    #[test]
    fn progress_chars_are_3_chars() {
        assert_eq!(PROGRESS_CHARS.chars().count(), 3);
    }

    #[test]
    fn max_visible_tasks_positive() {
        assert_ne!(MAX_VISIBLE_TASKS, 0);
    }
}
