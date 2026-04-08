//! Sprint 2 Item 5 — Security Summary tests.
//!
//! Verifies the `ShieldStats::apply_event` dispatch table and the
//! `SecuritySummary` `Display` impl that renders the end-of-run security
//! block.

#![cfg(test)]

use std::sync::Arc;

use nika_core::policy::{SecurityPolicyConfig, TaintMode};
use nika_event::{Event, EventKind};

use crate::renderer::{RunStats, ShieldStats};
use crate::summary::SecuritySummary;

fn ev(kind: EventKind) -> Event {
    Event {
        id: 0,
        timestamp_ms: 0,
        kind,
    }
}

#[test]
fn shield_stats_default_is_clean() {
    let stats = ShieldStats::default();
    assert_eq!(stats.trust.trusted, 0);
    assert_eq!(stats.spotlight.applied, 0);
    assert_eq!(stats.canary.detected, 0);
    assert_eq!(stats.findings, 0);
    assert!(!stats.has_activity());
}

#[test]
fn shield_stats_apply_trust_level_assigned() {
    let mut stats = ShieldStats::default();
    let task_id: Arc<str> = Arc::from("t1");

    stats.apply_event(&EventKind::TrustLevelAssigned {
        task_id: Arc::clone(&task_id),
        trust_level: "Trusted".to_string(),
    });
    stats.apply_event(&EventKind::TrustLevelAssigned {
        task_id: Arc::clone(&task_id),
        trust_level: "Untrusted".to_string(),
    });
    stats.apply_event(&EventKind::TrustLevelAssigned {
        task_id: Arc::clone(&task_id),
        trust_level: "ModelTainted".to_string(),
    });

    assert_eq!(stats.trust.trusted, 1);
    assert_eq!(stats.trust.untrusted, 1);
    assert_eq!(stats.trust.model_tainted, 1);
    assert!(stats.has_activity());
}

#[test]
fn shield_stats_apply_spotlight_and_canary_events() {
    let mut stats = ShieldStats::default();
    let task_id: Arc<str> = Arc::from("t1");

    stats.apply_event(&EventKind::SpotlightApplied {
        task_id: Arc::clone(&task_id),
        binding_alias: "article".into(),
        trust_level: "Untrusted".into(),
    });
    stats.apply_event(&EventKind::SpotlightSkipped {
        task_id: Arc::clone(&task_id),
        reason: "trust: elevated".into(),
    });
    stats.apply_event(&EventKind::CanaryInjected {
        task_id: Arc::clone(&task_id),
    });
    stats.apply_event(&EventKind::CanaryDetected {
        task_id: Arc::clone(&task_id),
        match_type: "exact".into(),
    });

    assert_eq!(stats.spotlight.applied, 1);
    assert_eq!(stats.spotlight.skipped, 1);
    assert_eq!(stats.canary.injected, 1);
    assert_eq!(stats.canary.detected, 1);
}

#[test]
fn shield_stats_apply_capability_and_restriction_events() {
    let mut stats = ShieldStats::default();
    let task_id: Arc<str> = Arc::from("t1");

    stats.apply_event(&EventKind::AgentToolRestricted {
        task_id: Arc::clone(&task_id),
        removed_tool: "nika:write".into(),
        reason: "tainted".into(),
    });
    stats.apply_event(&EventKind::CapabilityDenied {
        task_id: Arc::clone(&task_id),
        action: "nika:run".into(),
        reason: "tainted".into(),
    });
    stats.apply_event(&EventKind::SkillIntegrityFailed {
        path: "skills/tone.md".into(),
        expected: "abc123".into(),
        actual: "def456".into(),
    });

    assert_eq!(stats.restrictions, 1);
    assert_eq!(stats.capability_denied, 1);
    assert_eq!(stats.skill_integrity_failed, 1);
}

#[test]
fn shield_stats_apply_returns_false_for_non_security_events() {
    let mut stats = ShieldStats::default();
    let task_id: Arc<str> = Arc::from("t1");
    let matched = stats.apply_event(&EventKind::TaskCompleted {
        task_id,
        output: Arc::new(serde_json::json!("ok")),
        duration_ms: 100,
    });
    assert!(!matched);
    assert!(!stats.has_activity());
}

#[test]
fn run_stats_apply_event_routes_security_via_shield() {
    let mut stats = RunStats::default();
    let task_id: Arc<str> = Arc::from("t1");
    stats.apply_event(&ev(EventKind::CanaryInjected {
        task_id: Arc::clone(&task_id),
    }));
    stats.apply_event(&ev(EventKind::SpotlightApplied {
        task_id: Arc::clone(&task_id),
        binding_alias: "article".into(),
        trust_level: "Untrusted".into(),
    }));
    assert_eq!(stats.shield.canary.injected, 1);
    assert_eq!(stats.shield.spotlight.applied, 1);
}

#[test]
fn security_summary_display_clean_run_under_taint_off_is_empty() {
    let stats = ShieldStats::default();
    let policy = SecurityPolicyConfig {
        taint_mode: TaintMode::Off,
        ..SecurityPolicyConfig::default()
    };
    let summary = SecuritySummary {
        stats: &stats,
        policy: &policy,
    };
    let output = format!("{summary}");
    assert!(output.is_empty(), "no activity + taint off → empty render");
}

#[test]
fn security_summary_display_active_run_renders_trust_block() {
    let stats = ShieldStats {
        spotlight: crate::renderer::SpotlightCounters {
            applied: 3,
            ..Default::default()
        },
        canary: crate::renderer::CanaryCounters {
            injected: 2,
            ..Default::default()
        },
        trust: crate::renderer::TrustCounters {
            untrusted: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let policy = SecurityPolicyConfig::default();
    let summary = SecuritySummary {
        stats: &stats,
        policy: &policy,
    };
    let output = format!("{summary}");
    assert!(output.contains("Security Summary"));
    assert!(output.contains("Spotlight"));
    assert!(output.contains("3 applied"));
    assert!(output.contains("Canary"));
    assert!(output.contains("2 injected"));
    assert!(output.contains("Policy:"));
    assert!(output.contains("taint_mode=warn"));
}

#[test]
fn security_summary_display_canary_leak_is_highlighted() {
    let stats = ShieldStats {
        canary: crate::renderer::CanaryCounters {
            injected: 1,
            detected: 1,
        },
        ..Default::default()
    };
    let policy = SecurityPolicyConfig::default();
    let summary = SecuritySummary {
        stats: &stats,
        policy: &policy,
    };
    let output = format!("{summary}");
    assert!(output.contains("DETECTED"));
    assert!(output.contains("LEAK"));
}

#[test]
fn security_summary_display_capability_denied_renders_when_present() {
    let stats = ShieldStats {
        capability_denied: 2,
        findings: 1,
        ..Default::default()
    };
    let policy = SecurityPolicyConfig::default();
    let summary = SecuritySummary {
        stats: &stats,
        policy: &policy,
    };
    let output = format!("{summary}");
    assert!(output.contains("Capability:   2 denied"));
    assert!(output.contains("Scan finds:   1"));
}
