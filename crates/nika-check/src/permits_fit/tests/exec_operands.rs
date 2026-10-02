// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

fn fixture(
    declarations: &str,
    argv: &[&str],
    cwd: Option<&str>,
    hosts: &[&str],
    reads: &[&str],
) -> RawWorkflow {
    let argv = serde_json::json!(argv);
    let hosts = serde_json::json!(hosts);
    let reads = serde_json::json!(reads);
    let cwd = cwd.map_or_else(String::new, |c| {
        format!("      cwd: {}\n", serde_json::json!(c))
    });
    let yaml = format!(
        "nika: static-operands\n{declarations}\npermits:\n  exec: [curl, bash, cat, echo]\n  net: {{http: {hosts}}}\n  fs: {{read: {reads}}}\ntasks:\n  work:\n    exec:\n      command: {argv}\n{cwd}"
    );
    parse(&yaml, FileId::new(0), ParseMode::Strict).expect("valid static operand fixture")
}

fn one_escape(wf: &RawWorkflow, category: &str, operand: &str) -> CapabilityEscape {
    let found = scan_escapes(wf);
    assert_eq!(found.len(), 1, "exactly one known effect: {found:?}");
    assert_eq!(found[0].category, category);
    assert!(found[0].detail.contains(operand), "{found:?}");
    assert!(!found[0].floor && !found[0].undeclared);
    found[0].clone()
}

#[test]
fn literal_and_const_urls_have_the_same_declared_boundary_verdict() {
    let url = "https://outside.example.com/data";
    let literal = one_escape(
        &fixture("", &["curl", url], None, &[], &[]),
        "net",
        "outside.example.com",
    );
    for declarations in [
        "const: {url: 'https://outside.example.com/data'}",
        "const: {url: {type: string, value: 'https://outside.example.com/data'}}",
    ] {
        for operand in [
            "${{ const.url }}",
            "${{ const['url'] }}",
            "${{ const[\"url\"] }}",
        ] {
            let denied = fixture(declarations, &["curl", operand], None, &[], &[]);
            assert_eq!(one_escape(&denied, "net", "outside.example.com"), literal);
            let admitted = fixture(
                declarations,
                &["curl", operand],
                None,
                &["outside.example.com"],
                &[],
            );
            assert!(scan_escapes(&admitted).is_empty());
        }
    }
    assert!(
        scan_escapes(&fixture(
            "",
            &["curl", url],
            None,
            &["outside.example.com"],
            &[]
        ))
        .is_empty()
    );
}

#[test]
fn a_known_url_is_judged_independently_of_other_dynamic_arguments() {
    let wf = fixture(
        "const: {url: 'https://outside.example.com/data'}\ninputs: {arg: {type: string}}",
        &["curl", "${{ const.url }}", "${{ inputs.arg }}"],
        None,
        &[],
        &[],
    );
    one_escape(&wf, "net", "outside.example.com");
}

#[test]
fn url_inputs_and_non_bare_expressions_are_never_substituted() {
    let declarations = "const: {url: 'https://outside.example.com/data', number: 7}\ninputs: {url: {type: string, default: 'https://outside.example.com/data'}}";
    for operand in [
        "${{ inputs.url }}",
        "${{ inputs['url'] }}",
        "${{ const.missing }}",
        "${{ const.number }}",
        "prefix${{ const.url }}",
        "${{ const.url + '/x' }}",
        "${{ const.url.field }}",
        " ${{ const.url }}",
        "${{ const.url }} ",
    ] {
        let wf = fixture(declarations, &["curl", operand], None, &[], &[]);
        assert!(
            scan_escapes(&wf).is_empty(),
            "unknown URL stays unknown: {operand}"
        );
    }
}

#[test]
fn literal_and_const_scripts_use_the_same_literal_working_directory() {
    for (cwd, path) in [(None, "job.sh"), (Some("scripts"), "scripts/job.sh")] {
        let literal = one_escape(&fixture("", &["bash", "job.sh"], cwd, &[], &[]), "fs", path);
        for declarations in [
            "const: {program: bash, script: job.sh}",
            "const: {program: {type: string, value: bash}, script: {type: string, value: job.sh}}",
        ] {
            for program in ["bash", "${{ const.program }}"] {
                for operand in ["job.sh", "${{ const.script }}", "${{ const['script'] }}"] {
                    let denied = fixture(declarations, &[program, operand], cwd, &[], &[]);
                    assert_eq!(one_escape(&denied, "fs", path), literal);
                    let admitted = fixture(declarations, &[program, operand], cwd, &[], &[path]);
                    assert!(scan_escapes(&admitted).is_empty());
                }
            }
        }
    }
}

#[test]
fn script_inputs_and_unknown_expressions_stay_unknown() {
    let declarations =
        "const: {script: job.sh, number: 7}\ninputs: {script: {type: string, default: job.sh}}";
    for operand in [
        "${{ inputs.script }}",
        "${{ inputs['script'] }}",
        "${{ const.missing }}",
        "${{ const.number }}",
        "prefix${{ const.script }}",
        "${{ const.script + '.sh' }}",
        "${{ const.script.field }}",
        " ${{ const.script }}",
        "${{ const.script }} ",
    ] {
        let wf = fixture(declarations, &["bash", operand], Some("scripts"), &[], &[]);
        assert!(
            scan_escapes(&wf).is_empty(),
            "unknown script stays unknown: {operand}"
        );
    }
}

#[test]
fn computed_cwd_stays_unknown_except_for_an_absolute_script() {
    let declarations =
        "const: {script: job.sh, cwd: scripts}\ninputs: {cwd: {type: string, default: scripts}}";
    for cwd in ["${{ const.cwd }}", "${{ inputs.cwd }}"] {
        let wf = fixture(
            declarations,
            &["bash", "${{ const.script }}"],
            Some(cwd),
            &[],
            &[],
        );
        assert!(scan_escapes(&wf).is_empty());
        let absolute = fixture(
            "const: {script: /opt/scripts/job.sh, cwd: scripts}\ninputs: {cwd: {type: string, default: scripts}}",
            &["bash", "${{ const.script }}"],
            Some(cwd),
            &[],
            &[],
        );
        one_escape(&absolute, "fs", "/opt/scripts/job.sh");
    }
}

#[test]
fn unknown_later_arguments_and_non_interpreters_keep_their_existing_scope() {
    let declarations = "const: {script: job.sh}\ninputs: {arg: {type: string}}";
    let dynamic = fixture(
        declarations,
        &["bash", "${{ const.script }}", "${{ inputs.arg }}"],
        None,
        &[],
        &[],
    );
    assert!(scan_escapes(&dynamic).is_empty());
    let echo = fixture(
        declarations,
        &["echo", "${{ const.script }}"],
        None,
        &[],
        &[],
    );
    assert!(scan_escapes(&echo).is_empty());
}

#[test]
fn known_plumbing_operands_keep_host_and_workspace_reads_distinct() {
    let workspace = fixture(
        "const: {path: README.md}",
        &["cat", "${{ const.path }}"],
        None,
        &[],
        &[],
    );
    assert!(scan_escapes(&workspace).is_empty());
    let denied = fixture(
        "const: {path: /etc/passwd}",
        &["cat", "${{ const.path }}"],
        None,
        &[],
        &[],
    );
    one_escape(&denied, "fs", "/etc/passwd");
    let admitted = fixture(
        "const: {path: /etc/passwd}",
        &["cat", "${{ const.path }}"],
        None,
        &[],
        &["/etc/passwd"],
    );
    assert!(scan_escapes(&admitted).is_empty());
    let dynamic = fixture(
        "inputs: {path: {type: string, default: README.md}}",
        &["cat", "${{ inputs.path }}"],
        None,
        &[],
        &[],
    );
    let found = scan_escapes(&dynamic);
    assert_eq!(
        found.len(),
        1,
        "computed plumbing policy is retained: {found:?}"
    );
    assert_eq!(found[0].category, "fs");
}

#[test]
fn shell_text_is_not_expanded_as_array_arguments() {
    for (command, count) in [
        ("curl ${{ const.url }}", 0),
        ("curl https://outside.example.com/data", 1),
    ] {
        let yaml = format!(
            "nika: shell-scope\nconst: {{url: 'https://outside.example.com/data'}}\npermits: {{exec: true}}\ntasks:\n  work:\n    exec: {{shell: {}}}\n",
            serde_json::json!(command)
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("shell fixture");
        assert_eq!(scan_escapes(&wf).len(), count);
    }
}

#[test]
fn script_inference_reads_the_same_known_operands_as_fit() {
    for program in ["bash", "${{ const.program }}"] {
        let mut wf = fixture(
            "const: {program: bash, script: job.sh}",
            &[program, "${{ const.script }}"],
            Some("scripts"),
            &[],
            &[],
        );
        let inferred = crate::permits_infer::infer(&wf);
        assert_eq!(
            inferred.permits.exec,
            Some(ExecPermit::Programs(vec!["bash".to_owned()]))
        );
        assert!(inferred.permits.jail_admits_read("scripts/job.sh"));
        assert!(!inferred.partial.exec);
        wf.permits.as_mut().expect("fixture declares permits").value = inferred.permits;
        assert!(scan_escapes(&wf).is_empty());
    }
}

#[test]
fn input_program_inference_retains_its_conservative_unknown_boundary() {
    let wf = fixture(
        "inputs: {program: {type: string, default: bash}, script: {type: string, default: job.sh}}",
        &["${{ inputs.program }}", "${{ inputs.script }}"],
        Some("scripts"),
        &[],
        &[],
    );
    let inferred = crate::permits_infer::infer(&wf);
    assert_eq!(inferred.permits.exec, Some(ExecPermit::Any));
    assert!(inferred.partial.exec);
    assert!(!inferred.permits.jail_admits_read("scripts/job.sh"));
}

#[test]
fn a_literal_url_prefix_with_a_dynamic_query_keeps_its_existing_host_check() {
    let wf = fixture(
        "inputs: {query: {type: string}}",
        &[
            "curl",
            "https://outside.example.com/data?q=${{ inputs.query }}",
        ],
        None,
        &[],
        &[],
    );
    one_escape(&wf, "net", "outside.example.com");
}

#[test]
fn unresolved_arguments_are_retained_in_place_without_a_placeholder() {
    let wf = fixture(
        "const: {script: job.sh}\ninputs: {flag: {type: string, default: '-n'}}",
        &["bash", "${{ inputs.flag }}", "${{ const.script }}"],
        Some("scripts"),
        &[],
        &[],
    );
    let RawAction::Exec(action) = &wf.tasks[0].value.action else {
        panic!("fixture uses exec");
    };
    let consts = ConstStrings::of(&wf);
    assert_eq!(
        resolve_const_argv(&action.command, &consts).map(Iterator::collect::<Vec<_>>),
        Some(vec!["bash", "${{ inputs.flag }}", "job.sh"]),
    );
    assert!(scan_escapes(&wf).is_empty());
}
