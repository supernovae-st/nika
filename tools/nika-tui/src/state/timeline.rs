// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Timeline cache management for TuiState
//!
//! Manages cached timeline entries for the progress panel,
//! with version-based cache invalidation to avoid rebuilding
//! on every frame.

use super::TuiState;

use crate::widgets::TimelineEntry;

impl TuiState {
    /// Invalidate the timeline cache (call when task state changes)
    ///
    /// This increments the version counter, causing the next call to
    /// `ensure_timeline_cache()` to rebuild the entries.
    #[inline]
    pub fn invalidate_timeline_cache(&mut self) {
        self.timeline_version = self.timeline_version.wrapping_add(1);
    }

    /// Ensure the timeline cache is up to date
    ///
    /// Call this before rendering the progress panel to ensure
    /// `cached_timeline_entries` contains the latest data.
    /// Only rebuilds if the version has changed.
    pub fn ensure_timeline_cache(&mut self) {
        if self.timeline_cache_version != self.timeline_version {
            self.rebuild_timeline_cache();
        }
    }

    /// Rebuild the timeline cache from current task state
    fn rebuild_timeline_cache(&mut self) {
        self.cached_timeline_entries.clear();

        for id in &self.task_order {
            if let Some(task) = self.tasks.get(id) {
                let mut entry = TimelineEntry::new(&task.id, task.status);
                if let Some(ms) = task.duration_ms {
                    entry = entry.with_duration(ms);
                }
                if self.current_task.as_ref() == Some(&task.id) {
                    entry = entry.current();
                }
                entry = entry.with_breakpoint(self.has_breakpoint(&task.id));
                self.cached_timeline_entries.push(entry);
            }
        }

        self.timeline_cache_version = self.timeline_version;
    }
}
