// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Canonical durable schedule declarations.
//!
//! Project YAML and HTTP JSON lower into [`ScheduleDraft`]. Only a validated
//! [`ScheduleDefinition`] may enter durable state. No clock, I/O or timer lives
//! in this module.

use jiff::Timestamp;

use crate::cron::Field;
use crate::firing::{quoted, sha256_hex};
use crate::parse::{valid_tolerance, valid_workflow_path};
use crate::registry::{AfterSkip, Beat, Cadence, Locus, MissPolicy, Overlap};

pub const MAX_SCHEDULE_ID_BYTES: usize = 255;
pub const MAX_SCHEDULE_WORKFLOW_BYTES: usize = 1_024;
pub const MAX_SCHEDULE_CADENCE_BYTES: usize = 4_096;
pub const MAX_SCHEDULE_PAUSE_REASON_BYTES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleOrigin {
    Project,
    Api,
}

impl ScheduleOrigin {
    #[must_use]
    pub fn key(self, id: &str) -> String {
        match self {
            Self::Project => format!("project:{id}"),
            Self::Api => format!("api:{id}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleWhenDraft {
    Once { at: String },
    Cadence { expression: String },
    Webhook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleWhen {
    Once { at: Timestamp },
    Cadence { expression: String },
    Webhook,
}

impl ScheduleWhen {
    #[must_use]
    pub fn cadence_expression(&self) -> Option<&str> {
        match self {
            Self::Cadence { expression } => Some(expression),
            Self::Once { .. } | Self::Webhook => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleJitter {
    Hash,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ScheduleDraft {
    pub id: String,
    pub workflow: String,
    pub when: ScheduleWhenDraft,
    pub max_cost_usd: f64,
    pub missed: MissPolicy,
    pub max_lateness_seconds: Option<u64>,
    pub overlap: Option<Overlap>,
    pub after_skip: Option<AfterSkip>,
    pub jitter: Option<ScheduleJitter>,
    pub tolerance: Option<String>,
    pub active: Option<bool>,
    pub pause_reason: Option<String>,
    pub pause_until: Option<String>,
}

impl ScheduleDraft {
    /// Lower a parsed project beat into the one canonical vocabulary.
    ///
    /// # Errors
    /// Required project data and unsupported resident loci refuse as data.
    pub fn from_project(id: impl Into<String>, beat: &Beat) -> Result<Self, ScheduleFinding> {
        if beat.locus() != Locus::Local {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::UnsupportedLocus,
                "resident schedules require the local locus",
            ));
        }
        let max_cost_usd = beat.plafond.ok_or_else(|| {
            ScheduleFinding::new(ScheduleFindingKind::Cost, "maxCostUsd is required")
        })?;
        let missed = beat.manque.ok_or_else(|| {
            ScheduleFinding::new(ScheduleFindingKind::Missed, "missed is required")
        })?;
        let when = match Cadence::parse(&beat.cadence)
            .map_err(|error| ScheduleFinding::from_cadence(&error))?
        {
            Cadence::Webhook => ScheduleWhenDraft::Webhook,
            Cadence::Cron { .. } => ScheduleWhenDraft::Cadence {
                expression: beat.cadence.clone(),
            },
        };
        let jitter = match beat.decalage.as_deref() {
            None => None,
            Some("hash") => Some(ScheduleJitter::Hash),
            Some(_) => {
                return Err(ScheduleFinding::new(
                    ScheduleFindingKind::Jitter,
                    "only hash jitter is supported",
                ));
            }
        };
        Ok(Self {
            id: id.into(),
            workflow: beat.workflow.clone(),
            when,
            max_cost_usd,
            missed,
            max_lateness_seconds: None,
            overlap: beat.chevauchement,
            after_skip: beat.apres_saut,
            jitter,
            tolerance: beat.tolerance.clone(),
            active: beat.actif,
            pause_reason: beat.raison.clone(),
            pause_until: beat.jusqu_au.clone(),
        })
    }

    /// Validate and normalize before any durable mutation.
    ///
    /// # Errors
    /// The first semantic refusal, with a stable finding kind.
    pub fn validate(self) -> Result<ScheduleDefinition, ScheduleFinding> {
        validate_id(&self.id)?;
        validate_workflow(&self.workflow)?;
        if !(self.max_cost_usd > 0.0 && self.max_cost_usd.is_finite()) {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Cost,
                "maxCostUsd must be positive and finite",
            ));
        }
        let overlap = self.overlap.unwrap_or(Overlap::Sauter);
        if self.after_skip.is_some() && overlap != Overlap::Sauter {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::AfterSkip,
                "afterSkip requires overlap=skip",
            ));
        }
        if let Some(value) = self.tolerance.as_deref()
            && !valid_tolerance(value)
        {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Tolerance,
                "tolerance must be m/k with 1 <= m <= k",
            ));
        }
        let active = self.active.unwrap_or(true);
        validate_pause(
            active,
            self.pause_reason.as_deref(),
            self.pause_until.as_deref(),
        )?;
        Ok(ScheduleDefinition {
            id: self.id,
            workflow: self.workflow,
            when: validate_when(self.when)?,
            max_cost_usd: self.max_cost_usd,
            missed: self.missed,
            max_lateness_seconds: self.max_lateness_seconds,
            overlap,
            after_skip: self.after_skip.unwrap_or(AfterSkip::ProchainCreneau),
            jitter: self.jitter,
            tolerance: self.tolerance,
            active,
            pause_reason: self.pause_reason,
            pause_until: self.pause_until,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ScheduleDefinition {
    id: String,
    workflow: String,
    when: ScheduleWhen,
    max_cost_usd: f64,
    missed: MissPolicy,
    max_lateness_seconds: Option<u64>,
    overlap: Overlap,
    after_skip: AfterSkip,
    jitter: Option<ScheduleJitter>,
    tolerance: Option<String>,
    active: bool,
    pause_reason: Option<String>,
    pause_until: Option<String>,
}

impl ScheduleDefinition {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn workflow(&self) -> &str {
        &self.workflow
    }
    #[must_use]
    pub const fn when(&self) -> &ScheduleWhen {
        &self.when
    }
    #[must_use]
    pub const fn max_cost_usd(&self) -> f64 {
        self.max_cost_usd
    }
    #[must_use]
    pub const fn missed(&self) -> MissPolicy {
        self.missed
    }
    #[must_use]
    pub const fn max_lateness_seconds(&self) -> Option<u64> {
        self.max_lateness_seconds
    }
    #[must_use]
    pub const fn overlap(&self) -> Overlap {
        self.overlap
    }
    #[must_use]
    pub const fn after_skip(&self) -> AfterSkip {
        self.after_skip
    }
    #[must_use]
    pub const fn jitter(&self) -> Option<ScheduleJitter> {
        self.jitter
    }
    #[must_use]
    pub fn tolerance(&self) -> Option<&str> {
        self.tolerance.as_deref()
    }
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
    #[must_use]
    pub fn pause_reason(&self) -> Option<&str> {
        self.pause_reason.as_deref()
    }
    #[must_use]
    pub fn pause_until(&self) -> Option<&str> {
        self.pause_until.as_deref()
    }

    #[must_use]
    pub fn revision(&self) -> ScheduleRevision {
        ScheduleRevision::compute(self)
    }

    fn canonical(&self) -> String {
        let optional = |value: Option<String>| value.unwrap_or_else(|| "null".to_owned());
        let when = match &self.when {
            ScheduleWhen::Once { at } => format!("once:{}", quoted(&at.to_string())),
            ScheduleWhen::Cadence { expression } => format!("cadence:{}", quoted(expression)),
            ScheduleWhen::Webhook => "webhook".to_owned(),
        };
        [
            format!("id={}", quoted(&self.id)),
            format!("workflow={}", quoted(&self.workflow)),
            format!("when={when}"),
            format!("max_cost_usd={:?}", self.max_cost_usd),
            format!("missed={}", miss_word(self.missed)),
            format!(
                "max_lateness_seconds={}",
                optional(self.max_lateness_seconds.map(|v| v.to_string()))
            ),
            format!("overlap={}", overlap_word(self.overlap)),
            format!("after_skip={}", after_skip_word(self.after_skip)),
            format!(
                "jitter={}",
                optional(self.jitter.map(|_| "hash".to_owned()))
            ),
            format!(
                "tolerance={}",
                optional(self.tolerance.as_deref().map(quoted))
            ),
            format!("active={}", self.active),
            format!(
                "pause_reason={}",
                optional(self.pause_reason.as_deref().map(quoted))
            ),
            format!(
                "pause_until={}",
                optional(self.pause_until.as_deref().map(quoted))
            ),
        ]
        .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleRevision(String);

impl ScheduleRevision {
    fn compute(definition: &ScheduleDefinition) -> Self {
        let digest =
            sha256_hex(format!("nika/schedule-revision@1\n{}", definition.canonical()).as_bytes());
        Self(format!("sha256:{digest}"))
    }

    #[must_use]
    pub fn from_wire(raw: &str) -> Option<Self> {
        let digest = raw.strip_prefix("sha256:")?;
        (digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScheduleFindingKind {
    Id,
    Workflow,
    When,
    Cadence,
    Cost,
    Missed,
    AfterSkip,
    Jitter,
    Tolerance,
    Pause,
    UnsupportedLocus,
}

impl ScheduleFindingKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Id => "schedule.id",
            Self::Workflow => "schedule.workflow",
            Self::When => "schedule.when",
            Self::Cadence => "schedule.cadence",
            Self::Cost => "schedule.cost",
            Self::Missed => "schedule.missed",
            Self::AfterSkip => "schedule.after-skip",
            Self::Jitter => "schedule.jitter",
            Self::Tolerance => "schedule.tolerance",
            Self::Pause => "schedule.pause",
            Self::UnsupportedLocus => "schedule.unsupported-locus",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{} · {detail}", kind.code())]
#[non_exhaustive]
pub struct ScheduleFinding {
    kind: ScheduleFindingKind,
    detail: String,
}

impl ScheduleFinding {
    #[must_use]
    pub fn new(kind: ScheduleFindingKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
    fn from_cadence(error: &crate::CadenceError) -> Self {
        Self::new(
            ScheduleFindingKind::Cadence,
            format!("{} · {}", error.kind().spec_code(), error.detail()),
        )
    }
    #[must_use]
    pub const fn kind(&self) -> ScheduleFindingKind {
        self.kind
    }
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

fn validate_id(id: &str) -> Result<(), ScheduleFinding> {
    if id.is_empty()
        || id.len() > MAX_SCHEDULE_ID_BYTES
        || id.trim() != id
        || matches!(id, "." | "..")
        || id.chars().any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(ScheduleFinding::new(
            ScheduleFindingKind::Id,
            "id is not a bounded opaque segment",
        ));
    }
    Ok(())
}

fn validate_workflow(workflow: &str) -> Result<(), ScheduleFinding> {
    if workflow.len() > MAX_SCHEDULE_WORKFLOW_BYTES || !valid_workflow_path(workflow) {
        return Err(ScheduleFinding::new(
            ScheduleFindingKind::Workflow,
            "workflow is not a contained relative *.nika.yaml path",
        ));
    }
    Ok(())
}

fn validate_pause(
    active: bool,
    reason: Option<&str>,
    until: Option<&str>,
) -> Result<(), ScheduleFinding> {
    if active && (reason.is_some() || until.is_some()) {
        return Err(ScheduleFinding::new(
            ScheduleFindingKind::Pause,
            "pause fields require active=false",
        ));
    }
    if !active {
        let Some(reason) = reason else {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Pause,
                "active=false requires pauseReason",
            ));
        };
        if reason.is_empty() || reason.len() > MAX_SCHEDULE_PAUSE_REASON_BYTES {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Pause,
                "pauseReason is empty or oversized",
            ));
        }
        let Some(until) = until else {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Pause,
                "active=false requires pauseUntil",
            ));
        };
        if until.parse::<jiff::civil::Date>().is_err() {
            return Err(ScheduleFinding::new(
                ScheduleFindingKind::Pause,
                "pauseUntil must be an ISO date",
            ));
        }
    }
    Ok(())
}

fn validate_when(when: ScheduleWhenDraft) -> Result<ScheduleWhen, ScheduleFinding> {
    match when {
        ScheduleWhenDraft::Once { at } => at
            .parse::<Timestamp>()
            .map(|at| ScheduleWhen::Once { at })
            .map_err(|_| {
                ScheduleFinding::new(
                    ScheduleFindingKind::When,
                    "once.at must be an RFC 3339 instant",
                )
            }),
        ScheduleWhenDraft::Cadence { expression } => {
            if expression.len() > MAX_SCHEDULE_CADENCE_BYTES {
                return Err(ScheduleFinding::new(
                    ScheduleFindingKind::Cadence,
                    "cadence is oversized",
                ));
            }
            match Cadence::parse(&expression)
                .map_err(|error| ScheduleFinding::from_cadence(&error))?
            {
                Cadence::Webhook => Err(ScheduleFinding::new(
                    ScheduleFindingKind::When,
                    "webhook is not a cadence expression",
                )),
                Cadence::Cron { tz, spec } => Ok(ScheduleWhen::Cadence {
                    expression: canonical_cadence(&tz, &spec),
                }),
            }
        }
        ScheduleWhenDraft::Webhook => Ok(ScheduleWhen::Webhook),
    }
}

fn canonical_cadence(tz: &str, spec: &crate::CronSpec) -> String {
    format!(
        "TZ={tz} {} {} {} {} {}",
        canonical_field(*spec.minutes()),
        canonical_field(*spec.hours()),
        canonical_field(*spec.dom()),
        canonical_field(*spec.months()),
        canonical_field(*spec.dow())
    )
}

fn canonical_field<const LO: u8, const HI: u8>(field: Field<LO, HI>) -> String {
    if field.is_full() {
        return "*".to_owned();
    }
    field
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

const fn miss_word(v: MissPolicy) -> &'static str {
    match v {
        MissPolicy::Rattraper => "catch-up",
        MissPolicy::RattraperUneFois => "catch-up-once",
        MissPolicy::Sauter => "skip",
    }
}
const fn overlap_word(v: Overlap) -> &'static str {
    match v {
        Overlap::Sauter => "skip",
        Overlap::File => "queue",
        Overlap::Remplacer => "replace",
    }
}
const fn after_skip_word(v: AfterSkip) -> &'static str {
    match v {
        AfterSkip::ProchainCreneau => "next-slot",
        AfterSkip::ACompletion => "on-completion",
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn draft(when: ScheduleWhenDraft) -> ScheduleDraft {
        ScheduleDraft {
            id: "daily-report".into(),
            workflow: "workflows/report.nika.yaml".into(),
            when,
            max_cost_usd: 0.25,
            missed: MissPolicy::RattraperUneFois,
            max_lateness_seconds: Some(21_600),
            overlap: None,
            after_skip: None,
            jitter: Some(ScheduleJitter::Hash),
            tolerance: None,
            active: None,
            pause_reason: None,
            pause_until: None,
        }
    }

    #[test]
    fn equivalent_declarations_have_one_revision() {
        let a = draft(ScheduleWhenDraft::Cadence {
            expression: "TZ=Europe/Paris lundi 9h00".into(),
        })
        .validate()
        .expect("readable");
        let b = draft(ScheduleWhenDraft::Cadence {
            expression: " TZ=Europe/Paris  0 9 * * 1 ".into(),
        })
        .validate()
        .expect("cron");
        assert_eq!(a.when(), b.when());
        assert_eq!(a.revision(), b.revision());
    }

    #[test]
    fn exact_instant_is_offset_independent() {
        let a = draft(ScheduleWhenDraft::Once {
            at: "2026-09-01T09:00:00+02:00".into(),
        })
        .validate()
        .expect("offset");
        let b = draft(ScheduleWhenDraft::Once {
            at: "2026-09-01T07:00:00Z".into(),
        })
        .validate()
        .expect("utc");
        assert_eq!(a.revision(), b.revision());
    }

    #[test]
    fn boundaries_fail_closed_and_origins_do_not_collide() {
        let mut bad = draft(ScheduleWhenDraft::Webhook);
        bad.workflow = "../../outside.nika.yaml".into();
        assert_eq!(
            bad.validate().expect_err("path").kind(),
            ScheduleFindingKind::Workflow
        );
        assert_eq!(ScheduleOrigin::Project.key("daily"), "project:daily");
        assert_eq!(ScheduleOrigin::Api.key("daily"), "api:daily");
    }
}
