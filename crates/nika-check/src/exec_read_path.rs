// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#[cfg(test)]
mod tests {

    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    #[test]
    fn the_wrap_prints_the_rule_id_on_the_h6_shape() {
        let yaml = "\
nika: t
permits:
  exec: [\"grep\"]
tasks:
  probe:
    exec: { command: [\"grep\", \"READY\", \"./build-status.txt\"] }
";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let hints: Vec<_> = crate::check(&wf)
            .hints
            .into_iter()
            .filter(|hint| hint.kind == "exec-read-path")
            .collect();
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].kind, "exec-read-path");
        assert_eq!(
            hints[0].code,
            Some(nika_check_analyzer::exec_read_path::RULE)
        );
        assert!(hints[0].advice.contains("./build-status.txt"));
    }
}
