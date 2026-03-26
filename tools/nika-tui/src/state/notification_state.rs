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
            max_items: 10,
        }
    }
}

impl NotificationState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a notification, evicting oldest if at capacity
    pub fn push(&mut self, notification: Notification) {
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
}
