// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Notification delegation methods for TuiState
//!
//! Thin wrappers that delegate to `self.notifs` (NotificationState)
//! and mark the dirty flag. Keeps notification logic close to the
//! notification slice while providing ergonomic methods on TuiState.

use super::notification::Notification;
use super::TuiState;

impl TuiState {
    /// Add a notification (TIER 3.4)
    ///
    /// Delegates to NotificationState slice, then marks dirty.
    pub fn add_notification(&mut self, notification: Notification) {
        self.notifs.push(notification);
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Get active (non-dismissed) notifications
    pub fn active_notifications(&self) -> Vec<&Notification> {
        self.notifs.active()
    }

    /// Get count of active notifications
    pub fn active_notification_count(&self) -> usize {
        self.notifs.active_count()
    }

    /// Dismiss the most recent notification
    pub fn dismiss_notification(&mut self) {
        self.notifs.dismiss_latest();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Dismiss all notifications
    pub fn dismiss_all_notifications(&mut self) {
        self.notifs.dismiss_all();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Clear all notifications (removes from list entirely)
    pub fn clear_notifications(&mut self) {
        self.notifs.items.clear();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }
}
