// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Notification state slice
//!
//! Extracted from TuiState to isolate notification management
//! (system notifications with severity levels and dismissal).

use std::collections::VecDeque;

use super::notification::Notification;

/// Notification-related state extracted from TuiState
///
/// Manages system notifications with automatic eviction
/// and dismissal tracking.
#[derive(Debug)]
pub struct NotificationState {
    /// System notifications
    pub items: VecDeque<Notification>,
    /// Maximum number of notifications to keep
    pub max_items: usize,
}

impl Default for NotificationState {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            max_items: Self::DEFAULT_MAX_ITEMS,
        }
    }
}

impl NotificationState {
    /// Maximum notifications to retain before eviction
    const DEFAULT_MAX_ITEMS: usize = 10;

    pub fn new() -> Self {
        Self::default()
    }

    /// Add a notification, evicting oldest if at capacity.
    /// Consecutive identical notifications (same level + message) are deduped.
    pub fn push(&mut self, notification: Notification) {
        // Deduplicate consecutive identical notifications
        if let Some(last) = self.items.back() {
            if last.level == notification.level && last.message == notification.message {
                return;
            }
        }
        if self.items.len() >= self.max_items {
            self.items.pop_front();
        }
        self.items.push_back(notification);
    }

    /// Dismiss a notification by index, then compact dismissed items
    pub fn dismiss(&mut self, index: usize) {
        if let Some(notif) = self.items.get_mut(index) {
            notif.dismissed = true;
        }
        self.compact();
    }

    /// Dismiss the most recent non-dismissed notification, then compact
    pub fn dismiss_latest(&mut self) {
        for notif in self.items.iter_mut().rev() {
            if !notif.dismissed {
                notif.dismissed = true;
                break;
            }
        }
        self.compact();
    }

    /// Dismiss all notifications (clears the list entirely)
    pub fn dismiss_all(&mut self) {
        self.items.clear();
    }

    /// Remove dismissed items to free memory
    fn compact(&mut self) {
        self.items.retain(|item| !item.dismissed);
    }

    /// Count of active (non-dismissed) notifications
    pub fn active_count(&self) -> usize {
        self.items.iter().filter(|n| !n.dismissed).count()
    }

    /// Get active (non-dismissed) notifications
    pub fn active(&self) -> Vec<&Notification> {
        self.items.iter().filter(|n| !n.dismissed).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_state_default() {
        let ns = NotificationState::new();
        assert!(ns.items.is_empty());
        assert_eq!(ns.max_items, 10);
    }

    #[test]
    fn test_notification_state_push() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("hello", 0));
        assert_eq!(ns.items.len(), 1);
        assert_eq!(ns.active_count(), 1);
    }

    #[test]
    fn test_notification_state_eviction() {
        let mut ns = NotificationState {
            items: VecDeque::new(),
            max_items: 3,
        };
        for i in 0..5 {
            ns.push(Notification::info(format!("msg_{}", i), i as u64));
        }
        assert_eq!(ns.items.len(), 3);
        assert_eq!(ns.items[0].message, "msg_2");
        assert_eq!(ns.items[1].message, "msg_3");
        assert_eq!(ns.items[2].message, "msg_4");
    }

    #[test]
    fn test_notification_state_dismiss() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("a", 0));
        ns.push(Notification::warning("b", 1));
        ns.push(Notification::error("c", 2));

        assert_eq!(ns.active_count(), 3);

        // Dismissing index 1 ("b") compacts it out of the list
        ns.dismiss(1);
        assert_eq!(ns.active_count(), 2);
        assert_eq!(ns.items.len(), 2);
        assert_eq!(ns.items[0].message, "a");
        assert_eq!(ns.items[1].message, "c");

        // Out of bounds is a no-op
        ns.dismiss(99);
        assert_eq!(ns.active_count(), 2);
    }

    #[test]
    fn test_notification_state_dismiss_all() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("a", 0));
        ns.push(Notification::info("b", 1));
        assert_eq!(ns.active_count(), 2);

        ns.dismiss_all();
        assert_eq!(ns.active_count(), 0);
        assert_eq!(ns.items.len(), 0);
    }

    #[test]
    fn test_notification_state_active_filter() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("a", 0));
        ns.push(Notification::warning("b", 1));
        ns.push(Notification::success("c", 2));

        ns.dismiss(0);
        let active = ns.active();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].message, "b");
        assert_eq!(active[1].message, "c");
    }

    #[test]
    fn test_notification_state_compact_frees_memory() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("a", 0));
        ns.push(Notification::warning("b", 1));
        ns.push(Notification::error("c", 2));

        ns.dismiss(0);
        ns.dismiss(0); // Now dismisses "b" (which shifted to index 0)

        assert_eq!(ns.items.len(), 1);
        assert_eq!(ns.items[0].message, "c");
    }

    #[test]
    fn test_notification_dedup_consecutive_only() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("dup", 0));
        ns.push(Notification::warning("other", 1)); // breaks the sequence
        ns.push(Notification::info("dup", 2)); // same as first, but not consecutive

        assert_eq!(
            ns.items.len(),
            3,
            "non-consecutive duplicate must be accepted"
        );
    }

    #[test]
    fn test_notification_dedup_requires_both_level_and_message() {
        let mut ns = NotificationState::new();
        ns.push(Notification::info("same message", 0));

        // Same message, different level -> NOT deduped
        ns.push(Notification::warning("same message", 1));
        assert_eq!(
            ns.items.len(),
            2,
            "same message with different level must not be deduped"
        );

        // Same level, different message -> NOT deduped
        ns.push(Notification::warning("different message", 2));
        assert_eq!(
            ns.items.len(),
            3,
            "same level with different message must not be deduped"
        );

        // Both same -> deduped
        ns.push(Notification::warning("different message", 3));
        assert_eq!(
            ns.items.len(),
            3,
            "exact consecutive duplicate must be deduped"
        );
    }
}
