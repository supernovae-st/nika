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
    let mine = nika_serve::WriterStamp::this_engine();
    let finding = match (report.writer(), report.alive) {
        (Some(writer), true) if writer.newer_than_this_engine().is_some() => Finding {
            level: Level::Fail,
            label: "resident".to_owned(),
            detail: format!(
                "alive · engine {} (machine protocol {}) — NEWER than this binary ({}): this binary cannot serve its stores",
                writer.engine_version, writer.machine_protocol_version, mine.engine_version
            ),
            fix: Some("upgrade this binary to the resident's version, or stop the resident and start it from this binary".to_owned()),
        },
        (Some(writer), true) if writer.skews_from_this_engine() => Finding {
            level: Level::Warn,
            label: "resident".to_owned(),
            detail: format!(
                "alive · engine {} — this binary is {}: the resident keeps firing with the engine it was started with",
                writer.engine_version, mine.engine_version
            ),
            fix: Some("restart the resident (`nika serve`) from this binary so the two agree".to_owned()),
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
                if writer.skews_from_this_engine() { " (the next `nika serve` re-stamps them with this binary)" } else { "" }
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
    };
    let mut findings = vec![finding];
    if let Some(loss) = mirror_loss_finding(report.mirror_losses) {
        findings.push(loss);
    }
    findings
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
