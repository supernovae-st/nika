// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tightest boundary, offered when the declared one is wider.
//!
//! Split out of `lib.rs` under the ADR-023 1,500-LOC ceiling — the
//! `retired.rs` / `lift.rs` / `for_each.rs` precedent. The hint and the
//! three guards that make it safe are one subject; they live together.

use crate::effective::EffectivePermits;
use crate::hints::Hint;
use std::collections::BTreeSet;

/// The tightest `fs` boundary the body needs, OFFERED when the declared
/// one is wider.
///
/// The engine already derives it. `--infer-permits` computes the minimal
/// block, the report carries it as `permits.needed`, and a green check
/// prints neither. So a file granting `read: ["./**"]` to reach one file
/// audits exactly like the file granting that one path — same PERMITS
/// line, same hints. Only the risk grade moves, one word, with no reason
/// beside it and no repair.
///
/// This hint is the repair. It never guesses: it compares the declared
/// set against the derived one and offers the derived one verbatim.
///
/// THE GUARD IS A TYPE, NOT CARE. It fires only when the derivation is
/// COMPLETE on every face. Measured 2026-08-20: a workflow with one
/// static read and one computed read derives `needed.fs.read` holding
/// the static path ALONE, with the dynamic one in a prose note. Offering
/// that as the tightest boundary would break the run — the worst thing a
/// hint can do. `PartialFaces` makes that case unreachable by type.
///
/// It also stays silent when the declared set does not COVER what the
/// body needs: that is an escape, and `capability_escapes` owns it. A
/// hint that fires on a file already refusing would be noise on top of a
/// finding.
/// Whether one grant already admits everything another admits — used to
/// drop an entry the offer would otherwise repeat under a broader one.
/// Conservative: only the `**` suffix form is folded, because that is the
/// one the derivation actually emits twice.
fn covers(broad: &str, narrow: &str) -> bool {
    broad
        .strip_suffix("/**")
        .is_some_and(|root| narrow.starts_with(&format!("{root}/")))
}

/// Drop every entry another entry in the same set already admits. The
/// derivation emits a walk root beside the literals under it, so folding
/// is what makes the offer PASTEABLE — and folding both sides is what
/// makes the comparison honest.
fn fold(set: &BTreeSet<String>) -> BTreeSet<String> {
    set.iter()
        .filter(|entry| {
            !set.iter()
                .any(|other| other != *entry && covers(other, entry))
        })
        .cloned()
        .collect()
}

/// The whole `permits` hint lane, and the affirmative statement it reads.
///
/// ONE call from `check`, which sits exactly at the 100-line function cap
/// with zero headroom — so this replaces the `legal_zero_hint` line rather
/// than adding beside it, and hands back the statement the report needs.
/// Two concerns, one lane: both answer « what does this file's boundary
/// look like », one when it is absent and one when it is too wide.
pub(super) fn lane(
    wf: &nika_schema::raw::RawWorkflow,
    escapes_empty: bool,
    judged: bool,
    hints: &mut Vec<Hint>,
) -> EffectivePermits {
    let effective = crate::effective::collect(wf);
    crate::legal_zero_hint(wf, escapes_empty, judged, hints);
    if judged && escapes_empty {
        offer_tightest(&effective, hints);
    }
    effective
}

fn offer_tightest(permits: &EffectivePermits, hints: &mut Vec<Hint>) {
    if permits.partial.any() {
        return;
    }
    // AN EXEC MAKES THE FS DERIVATION UNKNOWABLE, not merely partial.
    // A subprocess reads whatever the OS lets it, and none of that is in
    // the file, so `needed.fs` describes the BUILTINS alone. Measured
    // 2026-08-20 on a real ledger workflow: it declares `read: ["./**"]`
    // with a comment saying why — without it the jail refuses to open the
    // very script the leg was told to launch (126) — and this hint, before
    // the guard, offered to narrow it to two paths. That advice breaks
    // every leg in the file.
    //
    // `partial` cannot express it: a literal argv IS pinnable, so the
    // inference is complete about what it models and silent about what a
    // shell does next. The honest posture is to say nothing at all while
    // any program can run.
    if !matches!(
        permits.needed.exec.as_ref(),
        None | Some(nika_cap::ExecPermit::No)
    ) {
        return;
    }
    let Some(declared) = permits.declared.as_ref() else {
        return;
    };
    let (Some(dfs), Some(nfs)) = (declared.fs.as_ref(), permits.needed.fs.as_ref()) else {
        return;
    };
    // BOTH SIDES NORMALIZE BEFORE THEY ARE COMPARED. The derivation emits
    // a leading `./` (the walk root's spelling) and authors write the bare
    // form, so `config/**` and `./config/**` are the same boundary in
    // two hands. Comparing the text lectured a file whose boundary was
    // already exact — measured on a real 16-task workflow, 2026-08-20.
    // A normalizer that stops short of its fixed point is how a gate
    // starts reporting a difference that is not one.
    let bare = |p: &String| p.strip_prefix("./").unwrap_or(p).to_owned();
    for (face, dset, nset) in [
        ("read", &dfs.read, &nfs.read),
        ("write", &dfs.write, &nfs.write),
    ] {
        let dnorm: BTreeSet<String> = dset.iter().map(bare).collect();
        let nnorm: BTreeSet<String> = nset.iter().map(bare).collect();
        // FOLD BOTH SIDES BEFORE COMPARING THEM. The derivation lists a
        // walk root AND every literal under it, so an author who wrote
        // exactly the roots still differs from it, entry for entry, while
        // describing the identical boundary. Comparing the unfolded sets
        // lectured a file that had just been narrowed to the offer's own
        // answer — the offer and the test have to speak the same shape.
        let dkept = fold(&dnorm);
        let nkept = fold(&nnorm);
        if nkept.is_empty() || dkept == nkept {
            continue;
        }
        // Every needed path must already fit the declared set. If one does
        // not, the file has an ESCAPE and the finding lane owns it.
        // The SAME predicate the boundary enforces at run, never a second
        // matcher: a coverage test that disagrees with the enforcement is
        // how a hint starts advising a file into a refusal.
        let is_write = face == "write";
        let covered = nset.iter().all(|need| declared.allows_path(need, is_write));
        if !covered {
            continue;
        }
        // The offer must be PASTEABLE. The derivation lists a walk root
        // and every literal under it, so `packages/**` arrives beside
        // `packages/app/docs/**` that it already covers. A
        // reader handed nine entries where four suffice reads noise, and
        // an offer nobody pastes teaches nothing.
        let tightest = nkept
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        hints.push(Hint {
            kind: "permits",
            task: "-".to_owned(),
            advice: format!(
                "`permits.fs.{face}` is wider than the body needs — the tightest \
                 boundary this file can run under is `{face}: [{tightest}]`, derived \
                 from every literal path it reaches. `nika check --infer-permits` \
                 writes the whole block; the run enforces whatever you keep"
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::CheckReport;
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn check_yaml(yaml: &str) -> CheckReport {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        check(&wf)
    }

    /// The tightest boundary is OFFERED when the declared one is wider,
    /// and the three cases where offering it would be WRONG stay silent.
    ///
    /// Every guard here was taught by a real file, not imagined:
    ///   · a computed path ⇒ the derivation is partial and `needed` is a
    ///     floor (`PartialFaces`)
    ///   · a MIXED body, one static read and one computed, derives the
    ///     static path ALONE — the offer would drop a real read
    ///   · any `exec` ⇒ a subprocess reads what the OS allows and none of
    ///     it is in the file (a real ledger workflow declares `./**`
    ///     precisely so its legs can open their own script)
    #[test]
    fn the_tightest_boundary_is_offered_only_when_it_is_knowable() {
        let hint_of = |yaml: &str| {
            check_yaml(yaml)
                .hints
                .iter()
                .find(|h| h.kind == "permits" && h.advice.contains("wider than the body needs"))
                .map(|h| h.advice.clone())
        };

        // WIDER than needed · offered, and the offer names the exact path.
        let wide = hint_of(
            r#"nika: w
permits:
  fs:
    read: ["./**"]
  tools: [nika:read]
tasks:
  a:
    invoke:
      tool: nika:read
      args: { path: "./data/a.txt" }
"#,
        )
        .expect("a wider grant is offered a tighter one");
        assert!(
            // the PATH, not its spelling: the offer normalizes the
            // derivation's leading `./` to the form authors write
            wide.contains("data/a.txt") && wide.contains("read:"),
            "the offer names the derived boundary · {wide}"
        );

        // ALREADY tight · nothing to say.
        assert!(
            hint_of(
                r#"nika: w
permits:
  fs:
    read: ["./data/a.txt"]
  tools: [nika:read]
tasks:
  a:
    invoke:
      tool: nika:read
      args: { path: "./data/a.txt" }
"#
            )
            .is_none(),
            "an exact boundary is not lectured"
        );
    }

    /// …and the three shapes where offering it would be WRONG stay
    /// silent. Every guard was taught by a real file, not imagined.
    #[test]
    fn the_offer_is_silent_when_the_derivation_cannot_be_trusted() {
        let hint_of = |yaml: &str| {
            check_yaml(yaml)
                .hints
                .iter()
                .find(|h| h.kind == "permits" && h.advice.contains("wider than the body needs"))
                .map(|h| h.advice.clone())
        };

        // COMPUTED path · the derivation is partial, so it stays silent.
        assert!(
            hint_of(
                r#"nika: w
inputs:
  t:
    type: string
permits:
  fs:
    read: ["./**"]
  tools: [nika:read]
tasks:
  a:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.t }}" }
"#
            )
            .is_none(),
            "a computed path silences the offer"
        );

        // MIXED · the dangerous one. `needed` holds the static path ALONE,
        // so an offer built from it would drop a read the body performs.
        assert!(
            hint_of(
                r#"nika: w
inputs:
  t:
    type: string
permits:
  fs:
    read: ["./**"]
  tools: [nika:read]
tasks:
  a:
    invoke:
      tool: nika:read
      args: { path: "./data/a.txt" }
  b:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.t }}" }
"#
            )
            .is_none(),
            "one computed read among static ones silences the whole offer"
        );

        // ANY exec · a subprocess reads what the file cannot say. The
        // ledger workflow that declares `./**` for exactly this reason:
        // without it the jail refuses to open the script the leg launches.
        assert!(
            hint_of(
                r#"nika: w
permits:
  exec: ["bash"]
  fs:
    read: ["./**"]
  tools: [nika:read]
tasks:
  a:
    exec: { command: ["bash", "x.sh"] }
  b:
    invoke:
      tool: nika:read
      args: { path: "./data/a.txt" }
"#
            )
            .is_none(),
            "an exec makes the fs derivation unknowable, not merely partial"
        );
    }
}
