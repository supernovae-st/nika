//! The resident's line of `nika doctor` (ADR-132 · #1352): the engine
//! that last wrote the stores under `<cwd>/.nika/serve`, beside whether
//! a resident holds the server lease now.

use nika_cli::verbs::doctor::{Finding, Level};

/// The resident's line (ADR-132 · #1352): the engine that last wrote the
/// stores under `<cwd>/.nika/serve` beside whether a resident holds the
/// server lease now — a running resident that is not this binary keeps
/// firing with the engine it was started with, and says so here.
pub(crate) fn resident_finding() -> Vec<Finding> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let Some(report) = nika_serve::inspect_resident(&cwd.join(".nika/serve")) else {
        return Vec::new();
    };
    let mut findings = vec![writer_finding(report.writer(), report.alive)];
    if let Some(loss) = mirror_loss_finding(report.mirror_losses) {
        findings.push(loss);
    }
    findings
}

/// The row from the stores' writer stamp and whether a resident runs. Stores
/// a NEWER machine protocol wrote are never served by this binary, running or
/// not (0.123 raised the protocol to 2): the row fails, and its fix names the
/// ways out, never a restart from this binary, which would refuse them too.
fn writer_finding(writer: Option<&nika_serve::WriterStamp>, alive: bool) -> Finding {
    let mine = nika_serve::WriterStamp::this_engine();
    match (writer, alive) {
        (Some(writer), alive) if writer.newer_than_this_engine().is_some() => Finding {
            level: Level::Fail,
            label: "resident".to_owned(),
            detail: format!(
                "{} · stores last written by engine {} (machine protocol {}), NEWER than this binary ({} · protocol {}): this binary cannot serve them",
                if alive { "alive" } else { "not running" },
                writer.engine_version,
                writer.machine_protocol_version,
                mine.engine_version,
                mine.machine_protocol_version
            ),
            fix: Some(format!(
                "upgrade this binary to {}; or stop the resident and serve a copy of .nika/serve taken before the upgrade, or a fresh --state-root",
                writer.engine_version
            )),
        },
        (Some(writer), true) if writer.skews_from_this_engine() => Finding {
            level: Level::Warn,
            label: "resident".to_owned(),
            detail: format!(
                "alive · engine {} — this binary is {}: the resident keeps firing with the engine it was started with",
                writer.engine_version, mine.engine_version
            ),
            fix: Some(
                "restart the resident (`nika serve`) from this binary so the two agree".to_owned(),
            ),
        },
        (Some(writer), true) => Finding {
            level: Level::Ok,
            label: "resident".to_owned(),
            detail: format!("alive · engine {} (this binary)", writer.engine_version),
            fix: None,
        },
        (Some(writer), false) => Finding {
            level: Level::Ok,
            label: "resident".to_owned(),
            detail: format!(
                "not running · stores last written by engine {}{}",
                writer.engine_version,
                if writer.skews_from_this_engine() {
                    " (the next `nika serve` re-stamps them with this binary)"
                } else {
                    ""
                }
            ),
            fix: None,
        },
        (None, alive) => Finding {
            level: Level::Warn,
            label: "resident".to_owned(),
            detail: format!(
                "{} · stores carry no writer stamp (written before 0.118)",
                if alive { "alive" } else { "not running" }
            ),
            fix: Some("restart the resident from this binary: it stamps the stores".to_owned()),
        },
    }
}

fn mirror_loss_finding(count: Option<usize>) -> Option<Finding> {
    let count = count.filter(|count| *count > 0)?;
    Some(Finding {
        level: Level::Warn,
        label: "resident journals".to_owned(),
        detail: format!("{count} jobs report lost journal mirrors; their execution status is unchanged"),
        fix: Some("inspect each job's evidence and trace verdict; repair journal storage before the next run (do not replay effects just to replace a missing journal)".to_owned()),
    })
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    /// 0.123 raised the machine protocol: stores a newer protocol wrote are
    /// never served by this binary, running or not, and the row names the
    /// ways out (a copy taken before the upgrade, a fresh state root), never a
    /// restart from this binary, which would refuse them too.
    #[test]
    fn stores_a_newer_protocol_wrote_fail_the_row_running_or_not() {
        let mine = nika_serve::WriterStamp::this_engine();
        let newer = nika_serve::WriterStamp {
            engine_version: "9.9.0".to_owned(),
            machine_protocol_version: mine.machine_protocol_version + 1,
        };
        for alive in [true, false] {
            let row = writer_finding(Some(&newer), alive);
            assert_eq!(row.level, Level::Fail, "alive: {alive}");
            assert!(
                row.detail.contains("NEWER than this binary"),
                "{}",
                row.detail
            );
            let fix = row.fix.unwrap_or_default();
            assert!(
                fix.contains("9.9.0")
                    && fix.contains("taken before the upgrade")
                    && fix.contains("--state-root")
                    && !fix.contains("start it from this binary"),
                "{fix}"
            );
        }
        let older = nika_serve::WriterStamp {
            engine_version: "0.1.0".to_owned(),
            machine_protocol_version: mine.machine_protocol_version,
        };
        let row = writer_finding(Some(&older), false);
        assert_eq!(row.level, Level::Ok);
        assert!(
            row.detail.contains("re-stamps them with this binary"),
            "{}",
            row.detail
        );
    }

    #[test]
    fn mirror_loss_warning_counts_reported_jobs_without_inventing_missing_facts() {
        assert!(mirror_loss_finding(None).is_none());
        assert!(mirror_loss_finding(Some(0)).is_none());
        let Some(finding) = mirror_loss_finding(Some(2)) else {
            panic!("reported losses must be visible");
        };
        assert_eq!(finding.level, Level::Warn);
        assert!(finding.detail.starts_with("2 jobs"));
        assert!(
            finding
                .fix
                .as_deref()
                .is_some_and(|fix| fix.contains("do not replay effects"))
        );
    }
}
