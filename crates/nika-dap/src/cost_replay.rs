// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The F-P18 cost-replay leg of `nika trace verify` (NEP-0017 · la
//! table de prix DANS le pin).
//!
//! The law: « la table de prix versionnée qui donne sens à ρ (usd)
//! fait partie du pin sémantique du run · un coût rejoué en 2031 se
//! lit contre la table 2026 pinnée ». A post-F-P18 boot frame pins the
//! pricing table identity (`pricing` = `{schema, as_of, sha256_16}`)
//! and — when the operator bounded the run — the resolved budget
//! (`budget` = `{max_cost_usd}`). This module is the verify-side
//! judge, a SCOPED STATED VERDICT (the F-P2 `Incomplete` posture): it
//! speaks its own `COST-REPLAY — …` line and never moves the chain
//! verdict nor the exit code:
//!
//! - no `pricing` key on the boot frame → [`CostReplay::Unrecorded`] —
//!   the run predates the law (the `LOCK_UNRECORDED` honest posture:
//!   stated, never a failure of the trace itself);
//! - `pricing` ≠ the verifying engine's compile-time table identity →
//!   [`CostReplay::Refused`] — an unknown table version never silently
//!   re-prices; the refusal names BOTH identities (pinned vs local);
//! - identical → [`CostReplay::Rejudged`] — the budget verdict is
//!   re-judged CONSISTENCY-GRADE: the journaled per-task `cost_usd`
//!   re-sum is compared against the journaled budget at the ledger's
//!   micro-USD grain (within-budget agrees with the run's PASS · a
//!   crossed budget agrees with the NIKA-1704 abort), and against the
//!   terminal frame's journaled `total_cost_usd` when that total rode.
//!   With no journaled budget the leg confirms totals-consistency only.
//!
//! RE-PRICING from the usage split is the named v2 owe: the journal
//! carries no input/output token split today, so v1 re-judges the
//! journaled DOLLARS, never re-derives them from tokens.
//!
//! Pure over its inputs (the tier idiom): the LOCAL identity is
//! injected by the caller from its own compile-time catalog — this
//! crate reads no catalog itself. The leg reads only chain-attested
//! journals (the CLI plugs it in above an intact verdict), so every
//! line here already passed the walk's line bound; an unparseable
//! line is simply skipped (the torn tail is the walk's business).

/// A pricing table identity — the triple the F-P18 boot pin journals
/// (`pricing` = `{schema, as_of, sha256_16}`). Owned: the pinned half
/// is parsed out of the journal, the local half is injected from the
/// verifying engine's compile-time catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PricingPin {
    /// The table's schema marker (e.g. `nika/model-pricing@1.1`).
    pub schema: String,
    /// The snapshot date, ISO `YYYY-MM-DD`.
    pub as_of: String,
    /// First 16 hex chars of the upstream payload's sha256.
    pub sha256_16: String,
}

impl PricingPin {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(schema: &str, as_of: &str, sha256_16: &str) -> Self {
        Self {
            schema: schema.to_owned(),
            as_of: as_of.to_owned(),
            sha256_16: sha256_16.to_owned(),
        }
    }
}

/// The leg's verdict — scoped and stated, never the chain verdict.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CostReplay {
    /// The boot frame pins no pricing table — the run predates F-P18.
    Unrecorded,
    /// The pinned table is not the verifying engine's — cost-replay
    /// REFUSES (the law's negative case). `pinned: None` when the pin
    /// rides but is unreadable (not a `{schema, as_of, sha256_16}`
    /// object): a pin that cannot name its table can never be matched
    /// against it.
    Refused {
        /// What the journal pinned (`None` = an unreadable pin).
        pinned: Option<PricingPin>,
        /// What this engine carries (compile-time).
        local: PricingPin,
    },
    /// Pinned ≡ local — the journaled cost story re-judged.
    Rejudged(Rejudged),
}

/// The consistency-grade re-judgment over the journaled dollars.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Rejudged {
    /// The re-summed per-task `cost_usd` (journal order — the same
    /// values the ledger folded, compared at the micro-USD grain, so
    /// f64 fold-order dust under concurrent waves never false-fires).
    pub spent_usd: f64,
    /// Frames that carried a priced `cost_usd`.
    pub priced_frames: usize,
    /// The journaled operator budget, when the boot frame pinned one.
    pub budget_usd: Option<f64>,
    /// The budget verdict's agreement with the journal — `Some` only
    /// when a budget was journaled AND the run reached a final budget
    /// verdict (a paused · cancelled · killed run has none yet).
    pub budget: Option<Agreement>,
    /// The re-sum against the terminal frame's journaled
    /// `total_cost_usd` — `Some` only when that total rode.
    pub totals: Option<Agreement>,
}

/// Does the re-judgment say what the journal says?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Agreement {
    /// The re-judgment and the journal agree.
    Agrees,
    /// They diverge — the journal's cost story does not re-judge
    /// (stated on the journal's internal consistency; the chain
    /// verdict above it is untouched).
    Diverges,
}

/// The judged leg: the verdict + its rendered lines (DATA — the CLI
/// writes them verbatim, the `TierReport.lines` idiom).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CostReplayLeg {
    /// The verdict.
    pub verdict: CostReplay,
    /// The rendered `COST-REPLAY — …` line(s).
    pub lines: Vec<String>,
}

/// The run's terminal budget story as the journal states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunEnd {
    /// No terminal frame read (a killed run's journal).
    None,
    /// `workflow_completed` — the run's PASS.
    Completed,
    /// `workflow_failed` with the NIKA-1704 detail — the budget abort.
    BudgetAbort,
    /// `workflow_failed` for any other cause — the budget was not the
    /// failure (a crossing would have aborted the run first: the
    /// wave-boundary trip check precedes the finalize).
    FailedOther,
    /// `workflow_paused` · `workflow_cancelled` — no final budget
    /// verdict exists yet (mid-flight).
    MidFlight,
}

/// Read one named field out of a frame's `fields` array (the
/// `seal_tier` closure idiom, factored for the three reads here).
fn field<'a>(line: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    line.get("fields")?
        .as_array()?
        .iter()
        .find(|f| f.get("key").and_then(|k| k.as_str()) == Some(name))
        .and_then(|f| f.get("value"))
}

/// A string field, parsed as one JSON document (the nested-object idiom:
/// `pricing` · `budget` ride as JSON text — the wire `Value` has no
/// object variant).
fn json_field(line: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
    serde_json::from_str(field(line, name)?.as_str()?).ok()
}

/// A numeric field (`cost_usd` · `total_cost_usd` ride as JSON numbers).
fn num_field(line: &serde_json::Value, name: &str) -> Option<f64> {
    field(line, name)?.as_f64()
}

/// The boot frame's pricing pin, three-way (the `Option<Option<T>>`
/// smell, named): absent (pre-law) · unreadable (rides but cannot name
/// its table) · pinned.
enum BootPin {
    /// No `pricing` key on the boot frame.
    Absent,
    /// The key rides but is not a readable `{schema, as_of, sha256_16}`
    /// object — never silently treated as absent NOR as a match.
    Unreadable,
    /// A readable pinned identity.
    Pinned(PricingPin),
}

/// The pin a boot frame carries.
fn pinned_identity(boot: &serde_json::Value) -> BootPin {
    let Some(raw) = field(boot, "pricing") else {
        return BootPin::Absent;
    };
    let parsed = raw
        .as_str()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
    let triple = parsed.and_then(|pin| {
        Some(PricingPin::new(
            pin.get("schema")?.as_str()?,
            pin.get("as_of")?.as_str()?,
            pin.get("sha256_16")?.as_str()?,
        ))
    });
    triple.map_or(BootPin::Unreadable, BootPin::Pinned)
}

/// The budget a boot frame carries (`{"max_cost_usd": dollars}` — a
/// malformed claim reads as absent, never as zero).
fn journaled_budget(boot: &serde_json::Value) -> Option<f64> {
    json_field(boot, "budget")?.get("max_cost_usd")?.as_f64()
}

/// Whole micro-USD (1e-6) — the budget-comparison grain, mirroring the
/// run ledger's own `micro_usd` (nika-runtime `ledger.rs`): the engine
/// trips on `micro(spent) > micro(budget)`, so the judge re-judges at
/// EXACTLY that grain — check≡run≡journal, one voice.
fn micro_usd(usd: f64) -> i64 {
    // REASON: budgets/spend are operator-scale dollars (≪ the i64 micro
    // range) and every value here was journaled from a finite f64 the
    // engine itself validated — a non-finite cannot reach the wire.
    #[allow(clippy::cast_possible_truncation)]
    {
        (usd * 1_000_000.0).round() as i64
    }
}

/// The pure leg: walk the journal once, judge the cost story against
/// the pinned pricing identity. The boot frame speaks FIRST (the run's
/// own pin); the terminal frame's LAST read wins (a chain-attested
/// journal has one; a rewritten one gets judged on its final claim).
#[must_use]
pub fn cost_replay_leg(raw: &str, local: &PricingPin) -> CostReplayLeg {
    let mut boot: Option<serde_json::Value> = None;
    let mut spent_usd = 0.0_f64;
    let mut priced_frames = 0_usize;
    let mut end = RunEnd::None;
    let mut journaled_total: Option<f64> = None;
    let mut journaled_qualifier: Option<String> = None;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(frame) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match frame.get("kind").and_then(|k| k.as_str()) {
            Some("workflow_started") => {
                if boot.is_none() {
                    boot = Some(frame.clone());
                }
            }
            Some("workflow_completed") => {
                end = RunEnd::Completed;
                journaled_total = num_field(&frame, "total_cost_usd");
                journaled_qualifier = field(&frame, "cost_qualifier")
                    .and_then(|q| q.as_str())
                    .map(str::to_owned);
            }
            Some("workflow_failed") => {
                let aborted = field(&frame, "detail")
                    .and_then(|d| d.as_str())
                    .is_some_and(|d| d.contains("NIKA-1704"));
                end = if aborted {
                    RunEnd::BudgetAbort
                } else {
                    RunEnd::FailedOther
                };
                journaled_total = num_field(&frame, "total_cost_usd");
                journaled_qualifier = field(&frame, "cost_qualifier")
                    .and_then(|q| q.as_str())
                    .map(str::to_owned);
            }
            Some("workflow_paused" | "workflow_cancelled") => {
                end = RunEnd::MidFlight;
                journaled_total = num_field(&frame, "total_cost_usd");
                journaled_qualifier = field(&frame, "cost_qualifier")
                    .and_then(|q| q.as_str())
                    .map(str::to_owned);
            }
            _ => {}
        }
        // Real spend rides ONLY as `cost_usd` (task_completed · failed ·
        // skipped — the whole-task figure, attempts folded; the fan-out
        // parent's aggregate, iterations frameless). No other frame
        // carries the key, so the re-sum can never double-count.
        if let Some(c) = num_field(&frame, "cost_usd") {
            spent_usd += c;
            priced_frames += 1;
        }
    }
    let Some(boot) = boot else {
        return unrecorded();
    };
    let pinned = match pinned_identity(&boot) {
        BootPin::Absent => return unrecorded(),
        BootPin::Unreadable => return refused(None, local),
        BootPin::Pinned(pin) => pin,
    };
    if pinned != *local {
        return refused(Some(pinned), local);
    }
    rejudged(
        &pinned,
        journaled_budget(&boot),
        spent_usd,
        priced_frames,
        end,
        journaled_total,
        journaled_qualifier.as_deref(),
    )
}

/// The pre-law posture — stated, never a failure of the trace itself.
fn unrecorded() -> CostReplayLeg {
    CostReplayLeg {
        verdict: CostReplay::Unrecorded,
        lines: vec![
            "COST-REPLAY — unrecorded · the boot frame pins no pricing table (a pre-F-P18\n  journal — the run predates the law) · cost-replay not attempted, the trace\n  verdict itself is unaffected"
                .to_owned(),
        ],
    }
}

/// The law's negative case: the pinned table is unknown to (or
/// unreadable by) this engine — cost-replay REFUSES, naming both
/// identities. Journal-originated strings are escaped at birth
/// (NEP-0012 law 2 — the pin rides an untrusted artifact).
fn refused(pinned: Option<PricingPin>, local: &PricingPin) -> CostReplayLeg {
    let pinned_render = match &pinned {
        Some(pin) => render_pin(pin),
        None => {
            "unreadable (the `pricing` pin is not a {schema, as_of, sha256_16} object)".to_owned()
        }
    };
    let pinned_render = crate::escape_tty(&pinned_render);
    CostReplayLeg {
        verdict: CostReplay::Refused {
            pinned,
            local: local.clone(),
        },
        lines: vec![format!(
            "COST-REPLAY — REFUSED · the pinned pricing table is not this engine's: an unknown\n  table version never silently re-prices (F-P18 · NEP-0017)\n  pinned: {pinned_render}\n  local:  {}",
            render_pin(local)
        )],
    }
}

/// The positive half: pinned ≡ local, so the journaled dollars re-judge
/// against the journaled budget (when one rode) and the journaled
/// total — CONSISTENCY-GRADE, the ledger's own micro-USD grain.
fn rejudged(
    local: &PricingPin,
    budget: Option<f64>,
    spent_usd: f64,
    priced_frames: usize,
    end: RunEnd,
    journaled_total: Option<f64>,
    journaled_qualifier: Option<&str>,
) -> CostReplayLeg {
    let mut lines = vec![format!(
        "COST-REPLAY — the pinned pricing table is this engine's ({})\n  the budget verdict re-judged from the journaled dollars (re-pricing from the\n  token split is the v2 owe — the journal carries no input/output usage split)",
        render_pin(local)
    )];
    lines.push(format!(
        "  re-summed ${spent_usd:.6} across {priced_frames} priced frame(s)"
    ));
    // Totals-consistency: the re-sum against the terminal frame's own
    // journaled total (the ledger folded the same values at the same
    // grain — a rewrite that moved dollars between frames shows here).
    let totals = journaled_total.map(|total| {
        let agreement = if micro_usd(spent_usd) == micro_usd(total) {
            Agreement::Agrees
        } else {
            Agreement::Diverges
        };
        match agreement {
            Agreement::Agrees => lines.push(format!(
                "  totals: agrees with the journaled total_cost_usd (${total:.6} · {})",
                journaled_qualifier.unwrap_or("qualifier unrecorded · an older journal")
            )),
            Agreement::Diverges => lines.push(format!(
                "  totals: DIVERGES — re-summed ${spent_usd:.6} vs journaled ${total:.6} (the journal's cost story does not re-judge)"
            )),
        }
        agreement
    });
    // ADR-128 · a run that metered nothing carries no total — said with
    // the journal's own qualifier, never re-summed to a zero.
    if journaled_total.is_none()
        && let Some(qualifier) = journaled_qualifier
        && !matches!(end, RunEnd::None | RunEnd::MidFlight)
    {
        lines.push(format!(
            "  totals: nothing metered — the journal says `{qualifier}` (no total_cost_usd, never a zero)"
        ));
    }
    // The budget verdict — only a run that REACHED a final budget
    // verdict can be re-judged on it (a mid-flight run has none yet).
    let budget_verdict = budget.and_then(|budget_usd| {
        let over = micro_usd(spent_usd) > micro_usd(budget_usd);
        let (agreement, line) = match (end, over) {
            (RunEnd::Completed, false) => (
                Agreement::Agrees,
                format!(
                    "  budget: spent ${spent_usd:.6} of ${budget_usd:.6} — within, agrees with the run's PASS"
                ),
            ),
            (RunEnd::BudgetAbort, true) => (
                Agreement::Agrees,
                format!(
                    "  budget: spent ${spent_usd:.6} of ${budget_usd:.6} — crossed, agrees with the run's NIKA-1704 abort"
                ),
            ),
            (RunEnd::FailedOther, false) => (
                Agreement::Agrees,
                format!(
                    "  budget: spent ${spent_usd:.6} of ${budget_usd:.6} — within, agrees with the journal (the budget was not the run's failure)"
                ),
            ),
            (RunEnd::MidFlight | RunEnd::None, _) => return None,
            (RunEnd::Completed | RunEnd::FailedOther, true) => (
                Agreement::Diverges,
                format!(
                    "  budget: spent ${spent_usd:.6} of ${budget_usd:.6} — crossed, yet the journal shows no budget abort: DIVERGES"
                ),
            ),
            (RunEnd::BudgetAbort, false) => (
                Agreement::Diverges,
                format!(
                    "  budget: spent ${spent_usd:.6} of ${budget_usd:.6} — within, yet the journal aborted on NIKA-1704: DIVERGES"
                ),
            ),
        };
        lines.push(line);
        Some(agreement)
    });
    if budget.is_none() {
        lines.push(
            "  budget: none journaled (the run was unbounded) — totals-consistency only".to_owned(),
        );
    } else if budget_verdict.is_none() {
        lines.push(
            "  budget: journaled, but the run has no final budget verdict yet (mid-flight)"
                .to_owned(),
        );
    }
    CostReplayLeg {
        verdict: CostReplay::Rejudged(Rejudged {
            spent_usd,
            priced_frames,
            budget_usd: budget,
            budget: budget_verdict,
            totals,
        }),
        lines,
    }
}

/// `{schema} · as_of {date} · sha256_16 {hex}` — the identity render
/// both the boot pin and this judge speak (one shape, two writers).
fn render_pin(pin: &PricingPin) -> String {
    format!(
        "{} · as_of {} · sha256_16 {}",
        pin.schema, pin.as_of, pin.sha256_16
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// The local identity the tests judge against (a stand-in for the
    /// compile-time catalog triple the CLI injects).
    fn local() -> PricingPin {
        PricingPin::new("nika/model-pricing@1.1", "2026-07-07", "0123456789abcdef")
    }

    /// The boot frame's `pricing` field value (JSON text, as journaled).
    fn pin_field(pin: &PricingPin) -> String {
        serde_json::json!({
            "schema": pin.schema,
            "as_of": pin.as_of,
            "sha256_16": pin.sha256_16,
        })
        .to_string()
    }

    /// One journal frame (kind + fields — the `chained_with` idiom,
    /// unchained: the leg reads chain-attested text, it never walks).
    fn frame(kind: &str, fields: &[(&str, serde_json::Value)]) -> String {
        let fields: Vec<serde_json::Value> = fields
            .iter()
            .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
            .collect();
        serde_json::json!({
            "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
            "timestamp": 1000, "kind": kind, "run": null,
            "correlation": null, "fields": fields
        })
        .to_string()
    }

    fn journal(frames: &[String]) -> String {
        let mut out = frames.join("\n");
        out.push('\n');
        out
    }

    /// A post-law boot frame: the pin (+ a budget when bounded).
    fn boot(pin: &PricingPin, budget: Option<f64>) -> String {
        let mut fields = vec![("pricing", serde_json::Value::String(pin_field(pin)))];
        if let Some(b) = budget {
            fields.push((
                "budget",
                serde_json::Value::String(format!("{{\"max_cost_usd\":{b}}}")),
            ));
        }
        frame("workflow_started", &fields)
    }

    /// A priced task terminal (the whole-task `cost_usd` figure).
    fn task_completed(cost: f64) -> String {
        frame("task_completed", &[("cost_usd", serde_json::json!(cost))])
    }

    /// The run's PASS terminal with its journaled total.
    fn completed(total: f64) -> String {
        frame(
            "workflow_completed",
            &[("total_cost_usd", serde_json::json!(total))],
        )
    }

    /// A pre-law journal (no `pricing` key) states `unrecorded` — the
    /// `LOCK_UNRECORDED` honest posture, never a failure of the trace.
    #[test]
    fn a_pre_law_journal_states_unrecorded() {
        let raw = journal(&[frame("workflow_started", &[]), completed(0.0)]);
        let leg = cost_replay_leg(&raw, &local());
        assert!(matches!(leg.verdict, CostReplay::Unrecorded));
        let text = leg.lines.join("\n");
        assert!(text.contains("COST-REPLAY — unrecorded"), "{text}");
        assert!(text.contains("pre-F-P18"), "{text}");
        assert!(text.contains("unaffected"), "{text}");
    }

    /// A journal with NO boot frame at all reads the same way — the
    /// absence of the pin is the absence of the claim.
    #[test]
    fn a_bootless_journal_states_unrecorded() {
        let raw = journal(&[completed(0.0)]);
        assert!(matches!(
            cost_replay_leg(&raw, &local()).verdict,
            CostReplay::Unrecorded
        ));
    }

    /// The law's negative case: the pinned table's `as_of` names a
    /// snapshot this engine does not carry — cost-replay REFUSES and
    /// names BOTH identities (pinned vs local).
    #[test]
    fn an_unknown_table_version_refuses_naming_both_identities() {
        let future = PricingPin::new("nika/model-pricing@1.1", "2031-01-15", "deadbeefdeadbeef");
        let raw = journal(&[
            boot(&future, Some(0.05)),
            task_completed(0.01),
            completed(0.01),
        ]);
        let leg = cost_replay_leg(&raw, &local());
        let CostReplay::Refused { pinned, local } = &leg.verdict else {
            panic!("an unknown table version refuses cost-replay");
        };
        assert_eq!(pinned.as_ref(), Some(&future));
        assert_eq!(local, &super::tests::local());
        let text = leg.lines.join("\n");
        assert!(text.contains("COST-REPLAY — REFUSED"), "{text}");
        assert!(text.contains("2031-01-15"), "the pinned side named: {text}");
        assert!(text.contains("2026-07-07"), "the local side named: {text}");
        assert!(text.contains("never silently re-prices"), "{text}");
    }

    /// A `pricing` key that rides but cannot name its table is a
    /// refusal too — never silently treated as a match NOR as absent.
    #[test]
    fn an_unreadable_pin_refuses() {
        let raw = journal(&[
            frame(
                "workflow_started",
                &[("pricing", serde_json::Value::String("not json".to_owned()))],
            ),
            completed(0.0),
        ]);
        let leg = cost_replay_leg(&raw, &local());
        assert!(matches!(
            leg.verdict,
            CostReplay::Refused { pinned: None, .. }
        ));
        assert!(
            leg.lines.join("\n").contains("unreadable"),
            "{:?}",
            leg.lines
        );
    }

    /// The positive half: pinned ≡ local, bounded run, PASS — the leg
    /// re-judges the budget verdict and the totals, both agreeing.
    #[test]
    fn a_matching_pin_rejudges_the_budget_verdict() {
        let raw = journal(&[
            boot(&local(), Some(0.05)),
            task_completed(0.01),
            task_completed(0.02),
            completed(0.03),
        ]);
        let leg = cost_replay_leg(&raw, &local());
        let CostReplay::Rejudged(rejudged) = &leg.verdict else {
            panic!("a matching pin re-judges");
        };
        assert!((rejudged.spent_usd - 0.03).abs() < 1e-12);
        assert_eq!(rejudged.priced_frames, 2);
        assert_eq!(rejudged.budget_usd, Some(0.05));
        assert_eq!(rejudged.budget, Some(Agreement::Agrees));
        assert_eq!(rejudged.totals, Some(Agreement::Agrees));
        let text = leg.lines.join("\n");
        assert!(
            text.contains("within, agrees with the run's PASS"),
            "{text}"
        );
        assert!(text.contains("totals: agrees"), "{text}");
        assert!(text.contains("v2 owe"), "the named owe rides: {text}");
    }

    /// A budget-aborted run (NIKA-1704 in the journal) agrees with a
    /// re-summed spend that crosses the journaled budget.
    #[test]
    fn a_budget_abort_agrees_with_the_crossed_budget() {
        let raw = journal(&[
            boot(&local(), Some(0.05)),
            task_completed(0.06),
            frame(
                "workflow_failed",
                &[
                    (
                        "detail",
                        serde_json::Value::String(
                            "NIKA-1704 · run budget exceeded — spent $0.060000 of $0.050000"
                                .to_owned(),
                        ),
                    ),
                    ("total_cost_usd", serde_json::json!(0.06)),
                ],
            ),
        ]);
        let leg = cost_replay_leg(&raw, &local());
        let CostReplay::Rejudged(rejudged) = &leg.verdict else {
            panic!("a matching pin re-judges");
        };
        assert_eq!(rejudged.budget, Some(Agreement::Agrees));
        assert!(
            leg.lines
                .join("\n")
                .contains("crossed, agrees with the run's NIKA-1704 abort")
        );
    }

    /// The consistency grade's teeth: a journal whose re-summed spend
    /// CROSSES the journaled budget yet claims PASS does not re-judge —
    /// stated as a divergence (the chain verdict above is untouched).
    #[test]
    fn a_rewritten_cost_story_diverges() {
        let raw = journal(&[
            boot(&local(), Some(0.05)),
            task_completed(0.06),
            completed(0.03), // the total no longer matches the frames
        ]);
        let leg = cost_replay_leg(&raw, &local());
        let CostReplay::Rejudged(rejudged) = &leg.verdict else {
            panic!("a matching pin re-judges");
        };
        assert_eq!(rejudged.budget, Some(Agreement::Diverges));
        assert_eq!(rejudged.totals, Some(Agreement::Diverges));
        let text = leg.lines.join("\n");
        assert!(text.contains("DIVERGES"), "{text}");
    }

    /// No journaled budget → the leg states totals-consistency only
    /// (absent is honest — an unbounded run carries no budget claim).
    #[test]
    fn an_unbounded_run_confirms_totals_only() {
        let raw = journal(&[boot(&local(), None), task_completed(0.01), completed(0.01)]);
        let leg = cost_replay_leg(&raw, &local());
        let CostReplay::Rejudged(rejudged) = &leg.verdict else {
            panic!("a matching pin re-judges");
        };
        assert_eq!(rejudged.budget_usd, None);
        assert_eq!(rejudged.budget, None);
        assert_eq!(rejudged.totals, Some(Agreement::Agrees));
        assert!(leg.lines.join("\n").contains("totals-consistency only"));
    }

    /// A mid-flight journal (paused · cancelled · killed) has no final
    /// budget verdict — the leg states the re-sum and judges nothing
    /// it cannot.
    #[test]
    fn a_mid_flight_run_has_no_budget_verdict_to_rejudge() {
        for terminal in [
            frame("workflow_paused", &[]),
            frame("workflow_cancelled", &[]),
        ] {
            let raw = journal(&[boot(&local(), Some(0.05)), task_completed(0.01), terminal]);
            let CostReplay::Rejudged(rejudged) = &cost_replay_leg(&raw, &local()).verdict else {
                panic!("a matching pin re-judges");
            };
            assert_eq!(rejudged.budget, None, "no final verdict, no agreement");
        }
        // Killed mid-flight: no terminal frame at all.
        let raw = journal(&[boot(&local(), Some(0.05)), task_completed(0.01)]);
        let CostReplay::Rejudged(rejudged) = &cost_replay_leg(&raw, &local()).verdict else {
            panic!("a matching pin re-judges");
        };
        assert_eq!(rejudged.budget, None);
        assert!(leg_mid_flight_line(&raw));
    }

    fn leg_mid_flight_line(raw: &str) -> bool {
        cost_replay_leg(raw, &local())
            .lines
            .join("\n")
            .contains("mid-flight")
    }

    /// Spending EXACTLY the budget is within it (the ledger's own
    /// micro-USD grain: `micro(spent) > micro(budget)` trips — equality
    /// never does).
    #[test]
    fn spending_exactly_the_budget_is_within() {
        let raw = journal(&[
            boot(&local(), Some(0.05)),
            task_completed(0.05),
            completed(0.05),
        ]);
        let CostReplay::Rejudged(rejudged) = &cost_replay_leg(&raw, &local()).verdict else {
            panic!("a matching pin re-judges");
        };
        assert_eq!(rejudged.budget, Some(Agreement::Agrees));
    }

    /// NEP-0012 law 2: a journal-originated identity reaches the
    /// refusal line escaped (the OSC52 class dies at birth).
    #[test]
    fn a_hostile_pinned_identity_is_escaped_in_the_refusal() {
        let hostile = PricingPin::new("nika/model-pricing@1.1", "2031-\u{1b}]52;;x\u{7}01", "aa");
        let raw = journal(&[boot(&hostile, None), completed(0.0)]);
        let leg = cost_replay_leg(&raw, &local());
        assert!(matches!(leg.verdict, CostReplay::Refused { .. }));
        let text = leg.lines.join("\n");
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n'),
            "escaped at birth (the line's own newlines excepted): {text:?}"
        );
        assert!(text.contains("2031-]52;;x01"), "sanitized head: {text}");
    }

    /// The re-sum never double-counts: the workflow terminal's
    /// `total_cost_usd` is a DIFFERENT key and must not join the sum.
    #[test]
    fn the_terminal_total_never_joins_the_re_sum() {
        let raw = journal(&[boot(&local(), None), task_completed(0.01), completed(0.01)]);
        let CostReplay::Rejudged(rejudged) = &cost_replay_leg(&raw, &local()).verdict else {
            panic!("a matching pin re-judges");
        };
        assert!(
            (rejudged.spent_usd - 0.01).abs() < 1e-12,
            "only per-task cost_usd sums: {}",
            rejudged.spent_usd
        );
    }
}
