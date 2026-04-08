// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task status and mission phase types for theme-aware styling.
//!
//! These types bridge the runtime task states with visual presentation,
//! providing icons, labels, and theme-aware colors for the TUI.

use ratatui::style::Color;

use super::Theme;

/// Task status for styling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Pending,
    Running,
    Success,
    Failed,
    Paused,
    Skipped,
}

impl TaskStatus {
    /// Get theme-aware color for this status
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            Self::Queued => theme.text_muted,
            Self::Pending => theme.status_pending,
            Self::Running => theme.status_running,
            Self::Success => theme.status_success,
            Self::Failed => theme.status_failed,
            Self::Paused => theme.status_paused,
            Self::Skipped => theme.text_muted,
        }
    }
}

/// Mission phase for space theme
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionPhase {
    /// Pre-launch checks, DAG validation
    Preflight,
    /// Loading configs, MCP connections
    Countdown,
    /// First task executing
    Launch,
    /// Nominal execution
    Orbital,
    /// MCP tool invocation
    Rendezvous,
    /// Workflow completed successfully
    MissionSuccess,
    /// Workflow failed
    Abort,
    /// Workflow paused by user
    Pause,
}

impl MissionPhase {
    /// Get icon for mission phase
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Preflight => "◦",
            Self::Countdown => "⊙",
            Self::Launch => "⊛",
            Self::Orbital => "◉",
            Self::Rendezvous => "◈",
            Self::MissionSuccess => "✦",
            Self::Abort => "⊗",
            Self::Pause => "⏸",
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Preflight => "PREFLIGHT",
            Self::Countdown => "COUNTDOWN",
            Self::Launch => "LAUNCH",
            Self::Orbital => "ORBITAL",
            Self::Rendezvous => "RENDEZVOUS",
            Self::MissionSuccess => "MISSION SUCCESS",
            Self::Abort => "ABORT",
            Self::Pause => "PAUSED",
        }
    }

    /// Get color for this mission phase (theme-aware)
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            Self::Preflight => theme.status_pending,
            Self::Countdown => theme.status_running,
            Self::Launch => theme.phase_launch,
            Self::Orbital => theme.phase_orbital,
            Self::Rendezvous => theme.phase_rendezvous,
            Self::MissionSuccess => theme.status_success,
            Self::Abort => theme.status_failed,
            Self::Pause => theme.status_paused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══ TASK STATUS TESTS ═══

    #[test]
    fn test_task_status_can_be_created() {
        let _ = TaskStatus::Pending;
        let _ = TaskStatus::Running;
        let _ = TaskStatus::Success;
        let _ = TaskStatus::Failed;
        let _ = TaskStatus::Paused;
    }

    #[test]
    fn test_task_status_equality() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_ne!(TaskStatus::Pending, TaskStatus::Running);
        assert_ne!(TaskStatus::Success, TaskStatus::Failed);
    }

    #[test]
    fn test_task_status_copy_clone() {
        let status = TaskStatus::Running;
        let copied = status;
        assert_eq!(status, copied);
    }

    // ═══ MISSION PHASE TESTS ═══

    #[test]
    fn test_mission_phase_icons() {
        assert_eq!(MissionPhase::Preflight.icon(), "◦");
        assert_eq!(MissionPhase::Orbital.icon(), "◉");
        assert_eq!(MissionPhase::MissionSuccess.icon(), "✦");
        assert_eq!(MissionPhase::Abort.icon(), "⊗");
    }

    #[test]
    fn test_mission_phase_names() {
        assert_eq!(MissionPhase::Countdown.name(), "COUNTDOWN");
        assert_eq!(MissionPhase::MissionSuccess.name(), "MISSION SUCCESS");
    }

    #[test]
    fn test_mission_phase_preflight_icon_and_name() {
        let phase = MissionPhase::Preflight;
        assert_eq!(phase.icon(), "◦");
        assert_eq!(phase.name(), "PREFLIGHT");
    }

    #[test]
    fn test_mission_phase_countdown_icon_and_name() {
        let phase = MissionPhase::Countdown;
        assert_eq!(phase.icon(), "⊙");
        assert_eq!(phase.name(), "COUNTDOWN");
    }

    #[test]
    fn test_mission_phase_launch_icon_and_name() {
        let phase = MissionPhase::Launch;
        assert_eq!(phase.icon(), "⊛");
        assert_eq!(phase.name(), "LAUNCH");
    }

    #[test]
    fn test_mission_phase_orbital_icon_and_name() {
        let phase = MissionPhase::Orbital;
        assert_eq!(phase.icon(), "◉");
        assert_eq!(phase.name(), "ORBITAL");
    }

    #[test]
    fn test_mission_phase_rendezvous_icon_and_name() {
        let phase = MissionPhase::Rendezvous;
        assert_eq!(phase.icon(), "◈");
        assert_eq!(phase.name(), "RENDEZVOUS");
    }

    #[test]
    fn test_mission_phase_success_icon_and_name() {
        let phase = MissionPhase::MissionSuccess;
        assert_eq!(phase.icon(), "✦");
        assert_eq!(phase.name(), "MISSION SUCCESS");
    }

    #[test]
    fn test_mission_phase_abort_icon_and_name() {
        let phase = MissionPhase::Abort;
        assert_eq!(phase.icon(), "⊗");
        assert_eq!(phase.name(), "ABORT");
    }

    #[test]
    fn test_mission_phase_pause_icon_and_name() {
        let phase = MissionPhase::Pause;
        assert_eq!(phase.icon(), "⏸");
        assert_eq!(phase.name(), "PAUSED");
    }

    #[test]
    fn test_mission_phase_all_icons_unique() {
        let icons = [
            MissionPhase::Preflight.icon(),
            MissionPhase::Countdown.icon(),
            MissionPhase::Launch.icon(),
            MissionPhase::Orbital.icon(),
            MissionPhase::Rendezvous.icon(),
            MissionPhase::MissionSuccess.icon(),
            MissionPhase::Abort.icon(),
            MissionPhase::Pause.icon(),
        ];

        // All icons should be unique
        for i in 0..icons.len() {
            for j in (i + 1)..icons.len() {
                assert_ne!(
                    icons[i], icons[j],
                    "Icons at positions {} and {} are identical: {}",
                    i, j, icons[i]
                );
            }
        }
    }

    #[test]
    fn test_mission_phase_equality() {
        assert_eq!(MissionPhase::Preflight, MissionPhase::Preflight);
        assert_ne!(MissionPhase::Preflight, MissionPhase::Countdown);
        assert_ne!(MissionPhase::MissionSuccess, MissionPhase::Abort);
    }

    #[test]
    fn test_mission_phase_copy_clone() {
        let phase = MissionPhase::Orbital;
        let copied = phase;
        assert_eq!(phase, copied);
    }
}
