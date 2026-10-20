// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ceilings a person is offered for a run (a scheduled run's, or one run's the Session
//! cannot prove a ceiling for): listed amounts, one recommended from what the workflow is known
//! to cost, with why. An offer is a suggestion the person picks or not: it is never applied for
//! them, and only their answer states a ceiling.

use std::path::Path;

use nika_cli_host::oracle::{AuditOptions, audit_source};

use super::Lang;

/// The amounts, in USD, a ceiling is offered in.
const AMOUNTS: [f64; 4] = [0.10, 0.25, 0.50, 1.00];
/// The amount recommended when nothing of the workflow's cost is known.
const UNOBSERVED: f64 = 0.25;
/// How many times its evidence the recommended ceiling holds at least.
const HEADROOM: f64 = 3.0;

/// What a recommended ceiling is read from: what the workflow is known to cost, never a ceiling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Evidence {
    /// What the newest succeeded run of the workflow cost, as the terminal frame of its trace
    /// states it.
    LastRun(f64),
    /// The check's static cost floor: what the cheapest path spends at the declared bounds
    /// (every gate closed, first try, children included). A Run refuses a ceiling below it.
    Floor(f64),
}

impl Evidence {
    fn cost(self) -> f64 {
        match self {
            Self::LastRun(cost) | Self::Floor(cost) => cost,
        }
    }
}

/// The ceilings offered, as typed amounts, and why the one recommended is, in `lang`: the
/// smallest listed amount holding `HEADROOM` times the `evidence`, that multiple rounded up to
/// the cent when no listed amount holds it, and `UNOBSERVED` when nothing of the workflow's cost
/// is known.
pub(super) fn offered(evidence: Option<Evidence>, lang: Lang) -> Vec<(String, Option<String>)> {
    let wanted = evidence.map(|evidence| evidence.cost() * HEADROOM);
    let recommended = wanted.map_or(UNOBSERVED, |wanted| {
        (AMOUNTS.into_iter())
            .find(|amount| *amount >= wanted)
            .unwrap_or_else(|| (wanted * 100.0).ceil() / 100.0)
    });
    // Why it is recommended: how many such runs the amount holds, counted, never the multiple
    // it was chosen by (live gauntlet, 2026-10-16: « $0.10 · covers at least 3 runs like the last
    // one, $0.0015 » for an amount that holds 66 of them), said as a multiple of one run, never
    // as a total to spend (NIK-196: « covers 10 runs like the last one » read as a budget).
    let times = |cost: f64| (recommended / cost * (1.0 + 1e-9)).floor();
    let why = match evidence {
        Some(Evidence::LastRun(cost)) if cost > 0.0 => {
            let last = lang.pick("the last run", "le dernier run");
            format!("{} × {last} ({})", times(cost), usd(cost))
        }
        Some(Evidence::LastRun(_)) => lang
            .pick(
                "the last run cost nothing priced",
                "le dernier run n'a rien coûté (aucun appel payant)",
            )
            .to_owned(),
        Some(Evidence::Floor(cost)) => {
            let least = lang.pick(
                "the least a run can cost",
                "le moins qu'un run puisse coûter",
            );
            format!("{} × {least} ({})", times(cost), usd(cost))
        }
        None => lang
            .pick(
                "no run of this workflow observed yet",
                "aucun run de ce workflow observé pour l'instant",
            )
            .to_owned(),
    };
    let recommended = format!("{recommended:.2}");
    let mut amounts: Vec<String> = AMOUNTS
        .iter()
        .map(|amount| format!("{amount:.2}"))
        .collect();
    if !amounts.contains(&recommended) {
        amounts.push(recommended.clone());
    }
    (amounts.into_iter())
        .map(|amount| {
            let why = (amount == recommended).then(|| why.clone());
            (amount, why)
        })
        .collect()
}

/// The amount recommended among the `offered` ones.
pub(super) fn recommended(offered: &[(String, Option<String>)]) -> Option<&str> {
    (offered.iter()).find_map(|(amount, why)| why.as_ref().map(|_| amount.as_str()))
}

/// A run's cost as said beside a ceiling: three decimals under a dollar, four under a cent, and
/// two significant digits under a tenth of a cent (live gauntlet r05, 2026-10-17: « 653 × le
/// dernier run ($0.0002) » for a run of $0.000153).
fn usd(cost: f64) -> String {
    if cost > 0.0 && cost < 0.001 {
        // Two significant digits: as many decimals as the first one needs, plus one.
        let decimals = (1..=12_i32)
            .zip(1..=12_usize)
            .find(|(power, _)| cost * 10f64.powi(*power) >= 1.0)
            .map_or(12, |(_, first)| first + 1);
        let digits = format!("{cost:.decimals$}");
        format!("${}", digits.trim_end_matches('0'))
    } else if cost < 0.01 {
        format!("${cost:.4}")
    } else if cost < 1.0 {
        format!("${cost:.3}")
    } else {
        format!("${cost:.2}")
    }
}

/// What the newest succeeded run of `workflow` cost, priced, as the terminal frame of its trace
/// under `root` states it. `None` when no such run was observed, or its total is unknown.
pub(super) fn last_cost(root: &Path, workflow: &Path) -> Option<f64> {
    let source = std::fs::read_to_string(root.join(workflow)).ok()?;
    let mode = nika_schema::ParseMode::Strict;
    let parsed = nika_schema::parse(&source, nika_schema::FileId::new(0), mode).ok()?;
    let name = parsed.workflow?.value;
    let traces = root.join(".nika").join("traces");
    let runs = nika_trace::forecast::gather::gather(&traces, &name).samples;
    // A succeeded run that recorded no priced cost (no model call, or only unpriced ones) is
    // still a run observed: it cost nothing priced (live gauntlet, 2026-10-16: three personas read
    // « no run of this workflow observed yet » after several green runs).
    (runs.into_iter())
        .find(|run| run.state == nika_dap::store::TraceState::Succeeded)
        .map(|run| run.total_cost_usd.unwrap_or(0.0))
        .filter(|cost| cost.is_finite() && *cost >= 0.0)
}

/// The static cost floor of `workflow` under `root`, as the check of its bytes on disk computes
/// it (its children composed). `None` when the file cannot be read or judged, or the floor is
/// zero: a floor of nothing is no evidence of cost.
fn floor(root: &Path, workflow: &Path) -> Option<f64> {
    let on_disk = root.join(workflow);
    let source = std::fs::read_to_string(&on_disk).ok()?;
    let base = on_disk.parent().map(Path::to_path_buf);
    let mut read = |path: &str| std::fs::read_to_string(path).map_err(|e| e.to_string());
    let logical = on_disk.display().to_string();
    let options = AuditOptions::default();
    let audit = audit_source(&source, &logical, Some(&mut read), base.as_deref(), options).ok()?;
    let floor = audit.report.cost.min_path_total_usd;
    (floor.is_finite() && floor > 0.0).then_some(floor)
}

/// What a run's ceiling is recommended from: the workflow's last run or its static floor,
/// whichever exists, and the larger when both do, so that the recommendation admits both.
pub(super) fn evidence(root: &Path, workflow: &Path) -> Option<Evidence> {
    let last = last_cost(root, workflow).map(Evidence::LastRun);
    let floor = floor(root, workflow).map(Evidence::Floor);
    match (last, floor) {
        (Some(last), Some(floor)) if floor.cost() > last.cost() => Some(floor),
        (last, floor) => last.or(floor),
    }
}

#[cfg(test)]
mod tests {
    use super::{Evidence, Lang, offered, recommended};

    /// The amounts offered and the one recommended, as `amount` and why.
    fn read(evidence: Option<Evidence>) -> (Vec<String>, Option<(String, String)>) {
        let offered = offered(evidence, Lang::En);
        let amounts = offered.iter().map(|(amount, _)| amount.clone()).collect();
        let recommended = (offered.into_iter()).find_map(|(amount, why)| Some((amount, why?)));
        (amounts, recommended)
    }

    #[test]
    fn the_ceiling_recommended_holds_three_times_the_last_run() {
        let listed = ["0.10", "0.25", "0.50", "1.00"];
        for (observed, amount, why) in [
            (Some(0.033), "0.10", "3 × the last run ($0.033)"),
            (Some(0.034), "0.25", "7 × the last run ($0.034)"),
            (Some(0.1), "0.50", "5 × the last run ($0.100)"),
            (Some(0.3), "1.00", "3 × the last run ($0.300)"),
            (Some(0.0042), "0.10", "23 × the last run ($0.0042)"),
            (Some(0.0), "0.10", "the last run cost nothing priced"),
            (None, "0.25", "no run of this workflow observed yet"),
        ] {
            let (amounts, recommended) = read(observed.map(Evidence::LastRun));
            assert_eq!(amounts, listed, "{observed:?}");
            assert_eq!(recommended, Some((amount.to_owned(), why.to_owned())));
        }
    }

    #[test]
    fn a_run_costlier_than_every_listed_amount_offers_its_multiple_rounded_up() {
        let (amounts, recommended) = read(Some(Evidence::LastRun(0.7)));
        assert_eq!(amounts, ["0.10", "0.25", "0.50", "1.00", "2.10"]);
        let why = "3 × the last run ($0.700)";
        assert_eq!(recommended, Some(("2.10".to_owned(), why.to_owned())));
        let (_, recommended) = read(Some(Evidence::LastRun(1.234)));
        let why = "3 × the last run ($1.23)";
        assert_eq!(recommended, Some(("3.71".to_owned(), why.to_owned())));
    }

    /// The static floor recommends as a run does, and says it is the floor.
    #[test]
    fn the_static_floor_recommends_three_times_itself() {
        let (amounts, recommended) = read(Some(Evidence::Floor(0.12)));
        assert_eq!(amounts, ["0.10", "0.25", "0.50", "1.00"]);
        let why = "4 × the least a run can cost ($0.120)";
        assert_eq!(recommended, Some(("0.50".to_owned(), why.to_owned())));
        let offered = super::offered(Some(Evidence::Floor(0.0004)), Lang::En);
        assert_eq!(super::recommended(&offered), Some("0.10"));
        assert_eq!(
            offered[0].1.as_deref(),
            Some("250 × the least a run can cost ($0.0004)")
        );
    }

    /// The count is the runs the amount holds, however many more than three (live gauntlet
    /// r08: « $0.10 · covers at least 3 runs like the last one, $0.0015 » for 66 of them).
    #[test]
    fn a_cheap_run_is_counted_as_the_runs_the_amount_holds() {
        let (_, recommended) = read(Some(Evidence::LastRun(0.0015)));
        let why = "66 × the last run ($0.0015)";
        assert_eq!(recommended, Some(("0.10".to_owned(), why.to_owned())));
    }

    /// The reason reads in the person's language, as a multiple of one run.
    #[test]
    fn the_reason_reads_in_french_for_a_french_speaker() {
        let offered = offered(Some(Evidence::LastRun(0.0094)), Lang::Fr);
        let why = (offered.iter()).find_map(|(_, why)| why.clone());
        assert_eq!(why.as_deref(), Some("10 × le dernier run ($0.0094)"));
    }

    #[test]
    fn exactly_one_amount_is_recommended() {
        for evidence in [
            None,
            Some(Evidence::LastRun(0.2)),
            Some(Evidence::Floor(2.0)),
        ] {
            let offered = offered(evidence, Lang::En);
            let marked = offered.iter().filter(|(_, why)| why.is_some()).count();
            assert_eq!(marked, 1, "{evidence:?}");
            assert!(recommended(&offered).is_some(), "{evidence:?}");
        }
    }
}
