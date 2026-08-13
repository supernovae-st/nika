---
id: ADR-081
title: "L1 effect-crate guard contract · 7 security guards as forever crate-admission criteria"
status: proposed
date: 2026-05-14
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["l1", "security", "computer-use", "admission", "guards", "forever"]
affects_crates: ["nika-screen", "nika-ocr", "nika-a11y", "nika-input", "nika-browser", "nika-vision-local"]
affects_layers: ["L1"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-005", "ADR-014", "ADR-016", "ADR-017", "ADR-083", "ADR-090", "ADR-091", "ADR-095"]
requires: ["ADR-003"]
enables: []
amends: []
fci: ["FCI-002", "FCI-003", "FCI-005"]
inv: ["INV-017", "INV-019"]
shadow_zones: ["screen-capture", "synthetic-input", "accessibility-tree", "vision-prompt-injection", "browser-clickjacking"]
nika_codes: ["NIKA-1000..1099 nika-screen", "NIKA-1100..1199 nika-ocr", "NIKA-1200..1299 nika-a11y", "NIKA-1300..1399 nika-input", "NIKA-1400..1499 nika-browser", "NIKA-1500..1599 nika-vision-local"]
timeline: "ships SAME-BATCH as nika-screen M2.1 B.1 batch · canonizes 7-guard contract forever for M2.2..M2.6"
follow_ups: ["per-guard impl review at each M2.X close", "DEF-1 KernelError sealed trait if drift signal", "Diamond memory subsystem absorb consent persistence at Phase 1.5 nika-memory ship"]
---

# ADR-081: L1 effect-crate guard contract · 7 security guards as forever crate-admission criteria

## Context

Phase 2 M2 dispatches 6 L1 effect crates (`nika-screen` · `nika-ocr` · `nika-a11y` · `nika-input` · `nika-browser` · `nika-vision-local`) which implement the L0.5 sealed traits shipped in M1 (per the Phase 2 M1 kernel-modules sprint plan · private DX surface). These crates collectively give the Olympus cockpit (atelier consumer) headless computer-use capability — screen capture · OCR · accessibility tree query · synthetic input · browser automation · local vision inference.

An internal security review at M1 close surfaced HIGH-severity findings (private DX surface) · all deferrable to L1 effect crate admission because L0.5 ships only sealed traits + DTOs (zero impl). The findings cluster around 7 concrete guards required across the 6 effect crates · documented verbatim in the M2 entry-conditions doc §2 EC-1 (private DX surface) ·

```
L1 nika-input · password-typing redaction guard (intercept type_text into
              password fields · per macOS NSSecureTextField + Linux a11y
              IS_SENSITIVE flag · refuse OR mask)
L1 nika-input · ConsentProof TTL TYPE-ENFORCEMENT (not just doc) · macOS
              CGEventCheckAccessToTrustedAppsForPostingEvents on every call
L1 nika-a11y · AX-secure-field redaction (NSAccessibilityProtectedContent
              + IS_SENSITIVE attr · strip from AxNode.value before exposing)
L1 nika-vision-local · prompt injection sanitization (Anthropic-style
              classifier OR allowlist for known-safe prompts)
L1 nika-browser · selector clickjacking guard (verify selector points to
              expected DOM tree shape · prevent malicious resolver)
L1 nika-screen · capture LED indicator (canonical per telemetry-canon §0
              · OS-native if available · explicit UI indicator otherwise)
L1 nika-screen · user consent UX (explicit pre-capture grant · per-app OR
              session-scoped · revocable)
```

Without a canonical contract documenting these 7 guards as **forever admission criteria**, each L1 crate would re-litigate the security stance at admission time · drift risk = HIGH (per `dx/.claude/rules/cross-source-validation.md` §2.7 self-application doctrine · prior canon claims falsified by empirical re-verify cost amendment cascades). Cohérent `dx/.claude/rules/skinning-discipline.md` license-posture-matrix pattern + `olympus/os/crates/olympus-os-prober/src/vectors/` hygiene vector graduation pattern (P2/warn → P1/fail post baseline GREEN) · structural enforcement > posture-only.

The 5 deferrals from M2 entry conditions (`§3 DEF-1..DEF-5`) are empirical-signal-gated per LOCK-031 spirit (KernelError sealed trait · CancellationToken param convention · panic boundary convention · rate-limit policy · structured logging) · this ADR does NOT pre-empt them · only canonizes the 7 guards because they are LOAD-BEARING for first-L1-admission readiness (`nika-screen` M2.1 ships 2 of 7 guards directly · template for M2.2..M2.6).

## Decision

**Adopt the 7-guard contract verbatim as forever crate-admission criteria for L1 effect crates** · embedded in each L1 crate's `Cargo.toml` `[package.metadata.adr]` row + `crates/<name>/README.md` § Security · and audited per-crate via hygiene vector `check-l1-guard-compliance.sh` (P2/warn ratchet candidate post 3+ L1 crates admitted · cohérent Vector #39 graduation trajectory per `hq-hygiene.md`).

### Guard ownership matrix (per-crate · per-Mode)

| Guard | Owner crate | Admission Mode | Sprint shipping |
|---|---|---|---|
| 1 · Password-typing redaction | `nika-input` | MANDATORY-at-admission | M2.4 |
| 2 · ConsentProof TTL TYPE-ENFORCEMENT | `nika-input` | MANDATORY-at-admission | M2.4 |
| 3 · AX-secure-field redaction | `nika-a11y` | MANDATORY-at-admission | M2.3 |
| 4 · Prompt injection sanitization | `nika-vision-local` | MANDATORY-at-admission | M2.6 |
| 5 · Selector clickjacking guard | `nika-browser` | MANDATORY-at-admission | M2.5 |
| 6 · Capture LED indicator | `nika-screen` | MANDATORY-at-admission | M2.1 |
| 7 · User consent UX (capture) | `nika-screen` | MANDATORY-at-admission | M2.1 |

**MANDATORY-at-admission** = the guard MUST be implemented + tested before the crate passes Gate 2 (ADR linked) of the 12-gate per ADR-003. Skeleton-option-A placeholder per `dx/.claude/rules/skeleton-option-a-pattern.md` §3 is ALLOWED for a single batch window IF the closure ceremony ships SAME-COMMIT at the next batch (cohérent §5 closure ceremony · audit.finding event mandatory).

### Per-guard structural contract

Each guard MUST satisfy ALL of ·

1. **OS-native primitive when available** · explicit UI indicator OR explicit error gate fallback otherwise
2. **`#[non_exhaustive]` on public types** · per FCI-3
3. **Cancel-safe per async trait method** · CANCEL SAFETY doc-comment mandatory per FCI-2
4. **NIKA-XXXX code per failure mode** · sub-range allocated per crate (NIKA-1000..1099 nika-screen · NIKA-1100..1199 nika-ocr · etc.) · canonical `error_code() -> &'static str` helper
5. **Test coverage** · ≥3 unit tests per guard (happy path · denial path · cancel-mid-guard) · ≥1 integration test (cross-crate consumer e.g. cockpit-overlay smoke)
6. **Telemetry-canon §0 compliance** · ZERO cloud telemetry for guard state · local journal NDJSON only (when olympus-os-journal absorbs at S33+ · in-memory only pre-S33)
7. **Sovereignty Rule 1 compliance** · zero vendor-hosted state · all guard config local-first

### Decision · what we chose (verbatim audit)

- ✅ **Single canonical ADR for all 7 guards** · NOT per-crate ADRs (NIKA-1000..1599 sub-ranges allocate per-crate · but the GUARD CONTRACT stays single canonical per `dx/.claude/rules/no-legacy-no-back-compat.md` Class 1 single-canonical-enum spirit · NUKE parallel taxonomies)
- ✅ **Skeleton-option-A allowed for 1 batch window** · per `dx/.claude/rules/skeleton-option-a-pattern.md` §3 conditions (downstream impl scheduled within 1 cascade · real source identified · pass-through semantic · test-compat) · closure ceremony mandatory per §5
- ✅ **Hygiene vector `check-l1-guard-compliance.sh` queued** · P2/warn ratchet candidate post 3+ L1 crates admitted (graduation pattern per `hq-hygiene.md` Vector #39 · skill-frontmatter-compliance precedent)
- ✅ **Consent persistence location DEFERRED to M2.4 nika-input** · in-memory only for nika-screen M2.1 · cohérent vendor-agnostic-architecture.md Mandate 1 (consent is daemon-domain · NOT engine scope · `~/.olympus/cache/consent/` when olympus-os daemon ships at S33+)

### What we explicitly rejected

- ❌ **Per-crate ADRs (-082 · -083 · ...) for each guard** · drift risk = HIGH (one guard per crate · 6 ADRs · 6 drift surfaces · cross-crate inconsistency · cohérent `no-legacy-no-back-compat.md` Class 1 + Class 2 git-is-our-archive)
- ❌ **Implementing guards in L0.5 sealed traits directly** · violates ADR-014 sealed kernel traits + ADR-006 layered kernel ISP · L0.5 is impl-free trait surface · guards are L1 IMPL concern by construction
- ❌ **Cloud telemetry for guard state** · violates `dx/.claude/rules/telemetry-canon.md` §0 + `dx/.claude/rules/supernovae-alignment.md` Rule 1 structural · zero exceptions
- ❌ **Skipping guard tests for « trivial » paths** · all 7 guards are LOAD-BEARING for atelier integrity · ≥3 unit + ≥1 integration test mandatory per guard
- ❌ **Defer-all-guards-to-Phase-3** approach · would expose the atelier consumer to the security findings the internal review surfaced during M2 dispatch · structural sovereignty > sprint velocity per supernovae-alignment Rule 5

## Consequences

### Positive

- **Forever-canon for next 5 L1 crates** · M2.2..M2.6 inherit template · zero re-litigation at admission · cohérent `cross-source-validation.md` §2.7 self-application discipline (canon claims preserved · amendment cascade only on empirical falsification)
- **Single grep-anchor** · « ADR-081 guards » resolves to this doc · enables future audit trail · cohérent `phantom-feature-recheck.md` §3 primary-source verify discipline
- **Structural sovereignty** · 7 guards are STRUCTURAL enforcement of supernovae-alignment Rule 5 « architecture IS the protection · not posture » · vendor-capture resistance baked into admission ceremony
- **Hygiene vector graduation path** · `check-l1-guard-compliance.sh` queued post-baseline · structural enforcement upgrade path (discipline → P2/warn → P1/fail) cohérent Vector #39 trajectory
- **Atelier integrity preserved** · per `olympus-vs-nika-distinction.md` D-2026-05-08-N1 cross-flow asymmetric · Nika engine ships the guards · Olympus cockpit consumes via L1 effect crates · guards are public AGPL surface (NOT atelier-internal · this is product code with structural sovereignty)

### Negative (acknowledged · mitigated)

- **+2-4h per L1 crate admission** · guard impl + tests not free · mitigated by skeleton-option-A 1-cascade window + per-batch atomic discipline (Pattern 1) · sustainable per `time-architecture.md` Layer 5 weekly cadence
- **Cross-crate consent persistence carry-forward** · M2.4 nika-input lands canonical consent persistence · M2.1 nika-screen in-memory only interim · documented carry in CHANGELOG · NOT silent drift
- **macOS Sequoia CHANGED capture-LED API drift risk** · primary-source verify Apple docs mandatory per `phantom-feature-recheck.md` §3 Step 2 turn-1 at each guard impl batch · feature-gate `#[cfg(target_os = "macos")]` + fallback tray icon if API unavailable
- **NIKA-1000..1599 code range commitment** · 100 codes per crate × 6 crates = 600 codes reserved · per `dx/.claude/rules/security.md` § Nika Shield + `nika/engine/docs/architecture/forward-compat-invariants.md` already reserves NIKA-1000..1199 · this ADR extends to NIKA-1000..1599 · companion code-canon doc update mandatory at first L1 admission close

### Forward-compat invariants respected

- **FCI-2 cancel-safety** · all guards async-trait-impl · CANCEL SAFETY doc-comment per method (cohérent ADR-016 cancellation model + ADR-017 streaming policy)
- **FCI-3 `#[non_exhaustive]`** · all public guard types + enums respect
- **FCI-5 `#[trait_variant::make]` async** · all guard traits use trait_variant for dyn-compat (cohérent ADR-014 sealed kernel traits)
- **INV-zero-unwrap** · `?` propagation only · enforced via hygiene vector `dx-rust-quality` (per `dx/.claude/rules/hq-hygiene.md` vector #30)
- **INV-pattern-1-atomic** · per-batch atomic commits · single-bash invocation per `concurrent-session-conflict.md` Pattern 1

### Shadow zones touched (cf DIAMOND.md §7)

1. **screen-capture** · LED indicator + consent UX guards close 2 of 7 (nika-screen scope)
2. **synthetic-input** · password redaction + ConsentProof TTL guards close 2 of 7 (nika-input scope · M2.4)
3. **accessibility-tree** · AX-secure-field redaction guard closes 1 of 7 (nika-a11y scope · M2.3)
4. **vision-prompt-injection** · classifier OR allowlist guard closes 1 of 7 (nika-vision-local scope · M2.6)
5. **browser-clickjacking** · selector verify guard closes 1 of 7 (nika-browser scope · M2.5)

All 5 shadow zones addressed by 7-guard contract · cohérent Diamond engine 7 shadow zones canon per DIAMOND.md §7 (5 of 7 touched · zones 6+7 remain · separate cycle).

## Alternatives considered

### Option A · Single canonical ADR (chosen)
- Pro · zero drift surface · single grep-anchor · cohérent Class 1 single-canonical doctrine
- Con · all 7 guards canon in one doc · larger surface · acceptable given evergreen status

### Option B · Per-crate ADRs (rejected)
- Pro · per-crate locality · scoped review
- Con · 6 ADRs × 1 guard avg = 6 drift surfaces · cross-crate inconsistency risk HIGH · violates Class 1

### Option C · Defer guard contract to per-crate impl review (rejected)
- Pro · zero ADR overhead pre-admission
- Con · re-litigation at each admission · drift HIGH · violates structural sovereignty per supernovae-alignment Rule 5

### Option D · Hygiene vector only (rejected as standalone · adopted as companion)
- Pro · structural enforcement post-baseline
- Con · standalone = no canon doc to grep · no audit trail · adopted as P2/warn ratchet POST this ADR ships

## Implementation timeline

- ✅ **Same-batch as nika-screen M2.1 B.1** · this ADR-081 ships
- ⏳ **nika-screen M2.1 B.5** · Guards 6+7 (capture LED + consent UX) ship (within ~1-2 weeks)
- ⏳ **nika-a11y M2.3** · Guard 3 (AX-secure-field redaction) ships
- ⏳ **nika-input M2.4** · Guards 1+2 (password redaction + ConsentProof TTL) ship · consent persistence location resolved (Olympus daemon vs engine scope)
- ⏳ **nika-browser M2.5** · Guard 5 (selector clickjacking) ships
- ⏳ **nika-vision-local M2.6** · Guard 4 (prompt injection sanitization) ships
- ⏳ **Post 3+ L1 crates admitted** · `check-l1-guard-compliance.sh` hygiene vector ships P2/warn (likely M2.3 close)
- ⏳ **Post baseline GREEN** · vector ratchet P2/warn → P1/fail per Vector #39 trajectory

## Coherence test · 5 raisons

| Raison | ✅ | Justification |
|---|:-:|---|
| ① Liberté cognitive | ✅ | Single canonical ADR · zero re-litigation · the internal review's findings closed via forever-criteria · zero LLM-memory drift on admission stance |
| ② Souveraineté | ✅ | 7 guards = structural enforcement of Rule 1 + Rule 5 · zero cloud telemetry · local-first persistence · macOS-native + Linux-native + Windows-native primitives preferred · explicit fallbacks documented |
| ③ Joy 🦋 | ✅ | Craft mode applied to admission ceremony · 7 guards canonized forever · M2.2..M2.6 inherit template · cohérent take-time + quality user directive · sustainable cadence preserved |
| ④ Composable galaxy | ✅ | Cross-crate composition · nika-screen consumes nika-input ConsentProof at consent persistence ship M2.4 · M2.4 GAP-1 ConsentProof 2-axis convergence closes · constellation pattern · cohérent atelier-vs-produit D-N1 |
| ⑤ Studio signature | ✅ | « 7 guards canon forever via ADR-081 » = NOUS-signature applied to L1 admission ceremony · vendor-capture resistance baked in · atelier integrity = distinctive vs vendor-template security theater |

## References

### Canon · doctrine
- `dx/.claude/rules/supernovae-alignment.md` Rule 1 memory sovereignty + Rule 5 structural enforcement (parent doctrine)
- `dx/.claude/rules/telemetry-canon.md` §0 zero-cloud (companion · ZERO cloud guard state)
- `dx/.claude/rules/skeleton-option-a-pattern.md` §3 + §5 (1-cascade placeholder + closure ceremony)
- `dx/.claude/rules/no-legacy-no-back-compat.md` Class 1 single canonical (single ADR for 7 guards)
- `dx/.claude/rules/cross-source-validation.md` §2.7 self-application (amendment cascade if empirical falsify)
- `dx/.claude/rules/phantom-feature-recheck.md` §3 (primary-source verify at each guard impl batch)
- `dx/.claude/rules/olympus-vs-nika-distinction.md` D-2026-05-08-N1 (cross-flow asymmetric · guards in Nika public AGPL · cockpit consumes)
- `dx/.claude/rules/socratic-research-discipline.md` §1 empirical + §6 anti-hallucination

### Canon · engine
- `nika/engine/docs/adr/adr-003-12-gate-admission.md` (parent · 12-gate verbatim)
- `nika/engine/docs/adr/adr-005-error-hierarchy.md` (NIKA-XXXX codes · sub-range allocation)
- `nika/engine/docs/adr/adr-014-sealed-kernel-traits.md` (sealed traits · ISP)
- `nika/engine/docs/adr/adr-016-cancellation-model.md` (cancel-safety contract)
- `nika/engine/docs/adr/adr-017-streaming-policy.md` (Stream<T> additive policy)
- `nika/engine/docs/architecture/forward-compat-invariants.md:130-138` (NIKA-1000..1199 reserved · this ADR extends to NIKA-1000..1599)
- `nika/engine/DIAMOND.md` §7 (7 shadow zones · 5 of 7 addressed by this ADR)

### Source · empirical research
- Internal security review · private DX surface (the trigger for this ADR · the 7 guards derive from its findings)
- Internal M2 entry-conditions §2 EC-1 · private DX surface (7 L1 guards verbatim)
- nika-screen L1 admission · private DX surface (companion sprint plan · this ADR ships SAME-BATCH as B.1)
- Internal computer-use master plan §3 M2 · private DX surface (parent · M2.1..M2.6 sequence)

### Companion · future
- (TBD) `nika/engine/scripts/hygiene/check-l1-guard-compliance.sh` (P2/warn ratchet candidate post 3+ L1 admissions)
- (TBD) `nika/engine/docs/architecture/l1-guard-compliance-baseline.json` (per-crate guard impl status · refreshed at each M2.X close)

## Update log

```
2026-05-14  v1.0 — Initial ADR proposed
              · Trigger · Phase 2 M2 entry conditions doc 64b5fdf6 §2 EC-1
                acknowledged 7 L1 security guards as crate-admission criteria ·
                user explicit « fais reco » directive 2026-05-14 PM · Option 1
                (sprint plan + ADR companion) chosen post EC-4 GATE CLOSED.
              · Synthesizes · the internal security review findings ·
                4 EC + 5 DEF Phase 2 M2 entry conditions · master plan §3 M2
                sequence (M2.1..M2.6) · Diamond DIAMOND.md §7 shadow zones
                (5 of 7 addressed by this ADR).
              · Option A chosen · single canonical ADR for all 7 guards · NOT
                per-crate ADRs (drift risk HIGH · violates Class 1 single-
                canonical-enum spirit per no-legacy-no-back-compat.md).
              · 7 guards ownership matrix · per-crate · per-Mode (MANDATORY-
                at-admission) · skeleton-option-A 1-cascade allowed per
                skeleton-option-a-pattern.md §3 with closure ceremony §5.
              · Per-guard structural contract · 7 items (OS-native preferred ·
                #[non_exhaustive] · cancel-safe · NIKA-XXXX code · ≥3 unit +
                ≥1 integration test · telemetry-canon §0 compliance ·
                sovereignty Rule 1 compliance).
              · Hygiene vector candidate · check-l1-guard-compliance.sh ·
                P2/warn ratchet post 3+ L1 crates admitted · cohérent Vector
                #39 graduation trajectory.
              · NIKA-1000..1599 code range commitment · 100 codes per crate
                × 6 crates · extends existing NIKA-1000..1199 reservation
                in forward-compat-invariants.md:130-138.
              · 5 raisons coherence ✅✅✅✅✅
              · Status · PROPOSED · awaiting acceptance at first L1
                (nika-screen M2.1) admission close · auto-flip to Accepted
                when Gate 2 audit confirms ADR-081 cited in nika-screen
                Cargo.toml + README.md.
              · Companion · the nika-screen L1 admission sprint plan
                (private DX surface · ships SAME-BATCH as B.1 trait
                extension + ADR companion).
```
