// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`CadenceError`] — the registry's refusal, named and teaching.
//!
//! This crate owes no `NIKA-*` range: every fault is rendered as a
//! taught fix at the L4 verb boundary (`exit 2`, the FILE plane), never
//! crossing into the workflow/runtime plane as a code. The
//! `check-error-one-voice.sh` allowlist row is already in place (class
//! `spec-plane`, the same class as `nika-cel`'s `CelErrorKind`, which
//! also carries its own wire slugs rather than a registry range).
//!
//! Corrected 2026-08-13 at Gate 11: this comment and the crate spec both
//! cited `wrapped-intermediate` and the `ExprError` precedent. Both were
//! wrong. That class is for an error WRAPPED into a coded one before it
//! crosses a boundary; this one is never wrapped, it carries its own
//! plane. The real row (`scripts/ci/error-one-voice-allowlist.tsv`) and
//! the canonical audit table have said `spec-plane` all along. The gate
//! matches enum and crate and ignores the class, so nothing broke; a
//! wrong precedent is worse than a broken gate because the next crate
//! copies it.
//!
//! [`CadenceErrorKind::spec_code`] follows the `CelError` model — a
//! stable per-kind slug — but the slugs are this grammar's OWN names
//! (`cadence.*`): they resolve nowhere else by design, and the Display
//! never presents them as registry codes.

/// The refusal class — one variant per grammar law.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CadenceErrorKind {
    /// The YAML itself does not parse, or a key is unknown (closed
    /// grammar).
    Grammar,
    /// `nika:` is not the frozen `v1` tag.
    TagFrozen,
    /// `ceiling:` present but not a positive finite number.
    CeilingNonPositive,
    /// Round-2+ key present (`traces:` · `registry:` · `signature:` ·
    /// `budget:`) — refused by name, never ignored.
    DeferredKey,
    /// The cadence carries no `TZ=` prefix (only `on-webhook` may).
    TzMissing,
    /// The `TZ=` name is unknown to the EMBEDDED IANA tzdb.
    TzUnknown,
    /// Not exactly five cron fields (validated before field semantics).
    FieldCount,
    /// A field does not parse (bad token · bad step · empty).
    FieldSyntax,
    /// A field value is outside its range.
    FieldRange,
    /// `dom` and `dow` restricted together (the Vixie OR trap).
    DomDowOr,
    /// No day in the `dom` set exists in any month of the `months` set
    /// (`31 4`). Five well-formed fields that no calendar can satisfy: the
    /// walk covers no day and the beat never fires. Named rather than left
    /// silent, because a `None` teaches nothing.
    DateImpossible,
    /// The readable form is not exactly `<jour-fr> <H>h<MM>`.
    PhraseSyntax,
    /// `workflow:` empty, or not a `*.nika.yaml` path.
    WorkflowPath,
    /// The same workflow armed twice.
    DuplicateWorkflow,
    /// `plafond:` absent — required, no default (the pay law).
    PlafondMissing,
    /// `plafond:` present but not a positive finite number.
    PlafondNonPositive,
    /// `manqué:` absent — required, no default (the run-missed law).
    ManqueMissing,
    /// `après_saut:` written under a non-`sauter` overlap.
    ApresSautMisplaced,
    /// `actif: false` without `raison:`.
    SuspensionSansRaison,
    /// `actif: false` without `jusqu_au:`.
    SuspensionSansEcheance,
    /// `jusqu_au:` not an ISO date.
    EcheanceSyntaxe,
    /// `tolérance:` not the `m/k` (m,k)-firm form with `m ≤ k`.
    ToleranceSyntaxe,
    /// `décalage:` other than `hash`.
    DecalageInconnu,
}

impl CadenceErrorKind {
    /// The stable refusal slug — this grammar's own wire name, NOT a
    /// `NIKA-*` registry code (none is owed: the L4 verb renders the
    /// taught fix, `exit 2`).
    #[must_use]
    pub const fn spec_code(self) -> &'static str {
        match self {
            Self::Grammar => "cadence.grammar",
            Self::TagFrozen => "cadence.tag-frozen",
            Self::CeilingNonPositive => "cadence.ceiling-non-positive",
            Self::DeferredKey => "cadence.deferred-key",
            Self::TzMissing => "cadence.tz-missing",
            Self::TzUnknown => "cadence.tz-unknown",
            Self::FieldCount => "cadence.field-count",
            Self::FieldSyntax => "cadence.field-syntax",
            Self::FieldRange => "cadence.field-range",
            Self::DomDowOr => "cadence.dom-dow-or",
            Self::DateImpossible => "cadence.date-impossible",
            Self::PhraseSyntax => "cadence.phrase-syntax",
            Self::WorkflowPath => "cadence.workflow-path",
            Self::DuplicateWorkflow => "cadence.duplicate-workflow",
            Self::PlafondMissing => "cadence.plafond-missing",
            Self::PlafondNonPositive => "cadence.plafond-non-positive",
            Self::ManqueMissing => "cadence.manque-missing",
            Self::ApresSautMisplaced => "cadence.apres-saut-misplaced",
            Self::SuspensionSansRaison => "cadence.suspension-sans-raison",
            Self::SuspensionSansEcheance => "cadence.suspension-sans-echeance",
            Self::EcheanceSyntaxe => "cadence.echeance-syntaxe",
            Self::ToleranceSyntaxe => "cadence.tolerance-syntaxe",
            Self::DecalageInconnu => "cadence.decalage-inconnu",
        }
    }
}

/// One named refusal: the law's reason + the fix form + the beat it
/// rides on (`None` when file-level) + the byte span of the faulty token
/// when the refusal was born on authored text (`None` for a structural
/// absence — a missing `plafond:` has no bytes to paint). The span is
/// the painter's data (the `CelError` precedent: a refusal that cannot
/// point at the faulty byte teaches half its fix).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{} · {detail}", kind.spec_code())]
#[non_exhaustive]
pub struct CadenceError {
    kind: CadenceErrorKind,
    detail: String,
    remedy: String,
    on: Option<String>,
    span: Option<(usize, usize)>,
    /// The cron field label the error was born on (crate-internal — the
    /// caller attaches the token's byte span from it).
    field: Option<&'static str>,
}

impl CadenceError {
    /// A file-level refusal.
    #[must_use]
    pub fn file(
        kind: CadenceErrorKind,
        detail: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            remedy: remedy.into(),
            on: None,
            span: None,
            field: None,
        }
    }

    /// A refusal riding one beat (its `workflow:` path).
    #[must_use]
    pub fn beat(
        workflow: &str,
        kind: CadenceErrorKind,
        detail: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            remedy: remedy.into(),
            on: Some(workflow.to_owned()),
            span: None,
            field: None,
        }
    }

    /// Mark the cron field this error was born on (builder, crate-internal).
    #[must_use]
    pub(crate) fn on_field(mut self, label: &'static str) -> Self {
        self.field = Some(label);
        self
    }

    /// Attach the byte span of the faulty token (builder).
    #[must_use]
    pub fn with_span(mut self, span: (usize, usize)) -> Self {
        self.span = Some(span);
        self
    }

    /// The refusal class.
    #[must_use]
    pub const fn kind(&self) -> CadenceErrorKind {
        self.kind
    }

    /// What is refused, and why (the law, one sentence).
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// The fix form — every refusal teaches its remedy.
    #[must_use]
    pub fn remedy(&self) -> &str {
        &self.remedy
    }

    /// The beat carrying this refusal, when one does.
    #[must_use]
    pub fn on(&self) -> Option<&str> {
        self.on.as_deref()
    }

    /// The byte span of the faulty token in the cadence expression,
    /// when the refusal was born on authored text (`None` for a
    /// structural absence — there are no bytes to paint).
    #[must_use]
    pub fn span(&self) -> Option<(usize, usize)> {
        self.span
    }

    /// The cron field label this error was born on, when one applies
    /// (crate-internal: `Cadence::parse` maps it to the token's span).
    #[must_use]
    pub(crate) fn field(&self) -> Option<&'static str> {
        self.field
    }

    /// Re-attach this refusal to a beat — mutating `on` ONLY: the span
    /// and the field label survive the transit (a refusal born on
    /// authored text must still paint its byte when it surfaces
    /// through `validate`).
    #[must_use]
    pub(crate) fn on_beat(mut self, workflow: &str) -> Self {
        if self.on.is_none() {
            self.on = Some(workflow.to_owned());
        }
        self
    }
}
