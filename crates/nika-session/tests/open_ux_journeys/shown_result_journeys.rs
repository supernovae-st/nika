// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A request to see the results never starts a run (NIK-234): a saved digest ran once and wrote
//! its file; the person asks to see it, in French then in English, while the host's router hears a
//! request to run in the line, as the live one did. No run is requested, the line reaches the
//! conversation's author, which reads the file and answers, and the file keeps what the run wrote.

use crate::scenarios::{SHOW, SHOW_EN, SHOWN, WRITTEN};
use crate::{outcome_kind, reached_the_author, run};

#[test]
fn a_request_to_see_the_results_reads_them_and_never_runs() {
    for (name, line) in [
        ("shown_results_in_french", SHOW),
        ("shown_results_in_english", SHOW_EN),
    ] {
        let journey = run(name);
        // The saved digest runs on « relance-le », and its run wrote the file.
        let ran = journey.step(2);
        assert_eq!(outcome_kind(ran), "run_requested", "{name}: {ran:#}");
        let shown = journey.step(4);
        assert_eq!(shown["line"], line, "{name}");
        assert_eq!(
            outcome_kind(shown),
            "reply",
            "{name}: a request to see the results started a run: {shown:#}"
        );
        assert_eq!(shown["outcome"]["text"], SHOWN, "{name}");
        assert!(shown["outcome"]["run"].is_null(), "{name}: {shown:#}");
        assert!(reached_the_author(&journey, line), "{name}");
        // The author read the file the run wrote, and answered from it.
        let read = (journey.agent.iter())
            .find(|seen| seen.called() == ["read"])
            .unwrap_or_else(|| panic!("{name}: the author never read the file"));
        assert!(
            read.last().contains("Rust 1.97 est sorti"),
            "{name}: {}",
            read.last()
        );
        // Nothing ran again: the file keeps what the one run wrote, and one trace is kept.
        let project = journey.dir.path().join("project");
        let digest = std::fs::read_to_string(project.join("news/digest.md")).unwrap();
        assert_eq!(digest, WRITTEN, "{name}");
        let traces = std::fs::read_dir(project.join(".nika/traces"))
            .unwrap()
            .count();
        assert_eq!(traces, 1, "{name}: one run, one trace");
    }
}
