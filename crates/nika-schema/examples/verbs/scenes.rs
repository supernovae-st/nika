// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb theater — each of the four execution models as an animated
//! ASCII storyboard, with the binding flow (`${{ … }}`) drawn as data
//! travelling between tasks.
//!
//! **The animation IS the data** (the display-contract law): a scene is
//! a PURE function `frame(step, theme) -> String`. Playback is trivial
//! iteration; reduced-motion renders the final frame; tests pin frames
//! byte-exact; `--frame N` renders any frame statically (CI ·
//! screenshots · docs). Nothing here reads a clock or the environment.
//!
//! Scene grammar (the same seam as `nika check`) ·
//! - the verb name paints in its governing-gate colour (magenta=cost ·
//!   blue=effect · brightness=blast)
//! - the braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` ticks ONLY on the running line
//!   (contract §3.2 · one animated region · `>` pulse in ascii)
//! - state glyphs: `○` pending · `✔` ok · `↻` retry · the binding dot
//!   `●` travels a `┈` rail (`o` on `.` in ascii)

use std::fmt::Write as _;

use crate::theme::{Glyph, Theme, VerbKind};

/// Braille spinner cycle (contract §3.2) — indexed by step.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Scene-local typography (single-consumer for now — graduates into
/// the shared seam the day a second surface needs it · Rams 10).
const fn arrow(t: Theme) -> &'static str {
    if t.unicode_glyphs() { "▸" } else { ">" }
}
const fn ellipsis(t: Theme) -> &'static str {
    if t.unicode_glyphs() { "…" } else { "..." }
}

/// The spinner glyph for an animation step (`>` pulse in ascii).
pub(crate) fn spin(step: usize, t: Theme) -> String {
    if t.unicode_glyphs() {
        t.accent(SPINNER[step % SPINNER.len()])
    } else {
        t.accent(if step.is_multiple_of(2) { ">" } else { "-" })
    }
}

/// A binding rail with the data dot at `pos` of `len` (the `${{ }}`
/// value travelling from an upstream output into this verb's input).
pub(crate) fn rail(pos: usize, len: usize, t: Theme) -> String {
    let (track, dot) = if t.unicode_glyphs() {
        ("┈", "●")
    } else {
        (".", "o")
    };
    let pos = pos.min(len.saturating_sub(1));
    let mut s = String::new();
    for i in 0..len {
        if i == pos {
            s.push_str(&t.accent(dot));
        } else {
            s.push_str(&t.dim(track));
        }
    }
    s
}

/// Number of steps in a verb's storyboard (frame indices `0..steps`).
pub(crate) const fn steps(verb: VerbKind) -> usize {
    match verb {
        VerbKind::Infer => 14,
        VerbKind::Exec | VerbKind::Invoke => 12,
        VerbKind::Agent => 18,
    }
}

/// Render one frame of a verb's storyboard. Pure and total: defined for
/// every `step < steps(verb)` (and clamps beyond — never panics).
pub(crate) fn frame(verb: VerbKind, step: usize, t: Theme) -> String {
    let step = step.min(steps(verb) - 1);
    match verb {
        VerbKind::Infer => infer_frame(step, t),
        VerbKind::Exec => exec_frame(step, t),
        VerbKind::Invoke => invoke_frame(step, t),
        VerbKind::Agent => agent_frame(step, t),
    }
}

/// The scene header — `◆ <verb> · <execution model>` in gate colour.
fn header(out: &mut String, verb: VerbKind, blurb: &str, t: Theme) {
    let _ = writeln!(
        out,
        " {} {} {}",
        t.accent(t.glyph(Glyph::Banner)),
        t.verb(verb, verb.name()),
        t.dim(&format!("{} {blurb}", t.middot()))
    );
}

// ── infer · one-shot token spend (COST-governed) ────────────────────
//
// beats: 0-3 the binding travels into the prompt · 4-5 dispatch ·
// 6-11 tokens stream · 12 validate · 13 done card
fn infer_frame(step: usize, t: Theme) -> String {
    let mut out = String::new();
    header(&mut out, VerbKind::Infer, "one-shot token spend", t);

    // the binding rail: tasks.fetch.output ┈┈●┈┈▶ prompt
    let bound = step >= 4;
    let rail_s = if bound {
        t.ok(t.glyph(Glyph::Ok))
    } else {
        rail(step, 4, t)
    };
    let _ = writeln!(
        out,
        "   {} {rail_s}{} prompt",
        t.dim("${{ tasks.fetch.output }}"),
        t.dim(arrow(t))
    );

    let streamed = [
        "",
        "\"The",
        "\"The arti",
        "\"The article",
        "\"The article shows",
        "\"The article shows...\"",
    ];
    match step {
        0..=3 => {
            let _ = writeln!(
                out,
                "   {} binding the prompt",
                t.dim(t.glyph(Glyph::Pending))
            );
        }
        4..=5 => {
            let _ = writeln!(
                out,
                "   {} {} {}",
                spin(step, t),
                t.dim("anthropic/claude-sonnet-4-6"),
                t.dim("~$0.0031 ceiling")
            );
        }
        6..=11 => {
            let _ = writeln!(
                out,
                "   {} {} {}",
                spin(step, t),
                t.dim("streaming"),
                streamed[(step - 6).min(streamed.len() - 1)]
            );
        }
        12 => {
            let _ = writeln!(out, "   {} schema valid (structured output)", spin(step, t));
        }
        _ => {
            let _ = writeln!(
                out,
                "   {} 412 tk {} $0.0019 {} output captured",
                t.ok(t.glyph(Glyph::Ok)),
                t.dim(t.middot()),
                t.dim(t.middot())
            );
        }
    }
    out
}

// ── exec · host process (PERMITS.exec-governed) ─────────────────────
//
// beats: 0-1 permits gate · 2-3 spawn · 4-9 stdout scrolls · 10 exit ·
// 11 done card
fn exec_frame(step: usize, t: Theme) -> String {
    let mut out = String::new();
    header(&mut out, VerbKind::Exec, "host process", t);
    let _ = writeln!(out, "   {} cargo test --lib", t.dim("$"));

    let lines = [
        "   Compiling nika-schema v0.80.0",
        "    Finished dev profile",
        "     Running unittests src/lib.rs",
        "test result: ok. 442 passed; 0 failed",
    ];
    match step {
        0..=1 => {
            let _ = writeln!(
                out,
                "   {} permits {} exec: [\"cargo\"]",
                t.dim(t.glyph(Glyph::Pending)),
                t.ok(t.glyph(Glyph::Ok))
            );
        }
        2..=3 => {
            let _ = writeln!(
                out,
                "   {} spawned {} pid 4242",
                spin(step, t),
                t.dim(t.middot())
            );
        }
        4..=9 => {
            let shown = ((step - 3) / 2 + 1).min(lines.len());
            let _ = writeln!(out, "   {} pid 4242", spin(step, t));
            for line in &lines[..shown] {
                let _ = writeln!(out, "   {} {}", t.dim("│"), t.dim(line));
            }
        }
        10 => {
            let _ = writeln!(out, "   {} exit 0", spin(step, t));
        }
        _ => {
            let _ = writeln!(
                out,
                "   {} exit 0 {} stdout captured {} tasks.test.output",
                t.ok(t.glyph(Glyph::Ok)),
                t.dim(t.middot()),
                t.dim(arrow(t))
            );
        }
    }
    out
}

// ── invoke · tool call (PERMITS.tools-governed) ─────────────────────
//
// beats: 0-1 permits gate · 2-4 args bind · 5-9 dispatch · 10 result ·
// 11 done card
fn invoke_frame(step: usize, t: Theme) -> String {
    let mut out = String::new();
    header(&mut out, VerbKind::Invoke, "tool call", t);
    let _ = writeln!(
        out,
        "   {} {}",
        t.verb(VerbKind::Invoke, "nika:write"),
        t.dim(&format!(
            "{{ path: \"./out.md\", content: {} }}",
            ellipsis(t)
        ))
    );

    match step {
        0..=1 => {
            let _ = writeln!(
                out,
                "   {} permits {} tools: [\"nika:write\"] {} fs.write: [\"./out.md\"]",
                t.dim(t.glyph(Glyph::Pending)),
                t.ok(t.glyph(Glyph::Ok)),
                t.ok(t.glyph(Glyph::Ok))
            );
        }
        2..=4 => {
            let _ = writeln!(
                out,
                "   {} content {rail}{} args",
                t.dim("${{ tasks.extract.output }}"),
                t.dim(arrow(t)),
                rail = rail(step - 2, 3, t)
            );
        }
        5..=9 => {
            let _ = writeln!(out, "   {} dispatching builtin", spin(step, t));
        }
        10 => {
            let _ = writeln!(out, "   {} wrote 4.2 KB", spin(step, t));
        }
        _ => {
            let _ = writeln!(
                out,
                "   {} ./out.md written {} result {} tasks.save.output",
                t.ok(t.glyph(Glyph::Ok)),
                t.dim(t.middot()),
                t.dim(arrow(t))
            );
        }
    }
    out
}

// ── agent · autonomous loop (COST + PERMITS) ────────────────────────
//
// beats: per turn (think → tool → observe) ×2 · then nika:done · card
fn agent_frame(step: usize, t: Theme) -> String {
    let mut out = String::new();
    header(&mut out, VerbKind::Agent, "autonomous loop", t);
    let _ = writeln!(
        out,
        "   {} {} {}",
        t.dim("budget"),
        t.dim(&format!("{}50k tk", t.leq())),
        t.dim(&format!("{} tools: [\"nika:*\"]", t.middot()))
    );

    // each turn = 6 beats: 0-1 think · 2-3 tool · 4-5 observe
    let turn = (step / 6) + 1;
    let beat = step % 6;
    if step >= 12 {
        // the closing beats: nika:done sentinel then the card
        if step < 15 {
            let _ = writeln!(
                out,
                "   {} turn 3 {} nika:done",
                spin(step, t),
                t.dim(t.middot())
            );
        } else {
            let _ = writeln!(
                out,
                "   {} done in 3 turns {} 18.2k tk {} $0.0834",
                t.ok(t.glyph(Glyph::Ok)),
                t.dim(t.middot()),
                t.dim(t.middot())
            );
        }
        return out;
    }
    match beat {
        0 | 1 => {
            let _ = writeln!(
                out,
                "   {} turn {turn} {} thinking",
                spin(step, t),
                t.dim(t.middot())
            );
        }
        2 | 3 => {
            let _ = writeln!(
                out,
                "   {} turn {turn} {} {} nika:grep",
                spin(step, t),
                t.dim(t.middot()),
                t.verb(VerbKind::Invoke, "invoke")
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "   {} turn {turn} {} observing {} 3 matches",
                spin(step, t),
                t.dim(t.middot()),
                t.dim(t.middot())
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [VerbKind; 4] = [
        VerbKind::Infer,
        VerbKind::Exec,
        VerbKind::Invoke,
        VerbKind::Agent,
    ];

    #[test]
    fn every_frame_of_every_verb_is_total_and_nonempty() {
        for verb in ALL {
            for step in 0..steps(verb) + 5 {
                // +5: clamping beyond the storyboard never panics
                let f = frame(verb, step, Theme::new(false, true));
                assert!(!f.is_empty(), "{verb:?} step {step}");
            }
        }
    }

    #[test]
    fn frames_are_pure_functions_of_their_inputs() {
        for verb in ALL {
            let a = frame(verb, 7, Theme::new(false, true));
            let b = frame(verb, 7, Theme::new(false, true));
            assert_eq!(a, b, "{verb:?} frame must be deterministic");
        }
    }

    #[test]
    fn final_frames_carry_the_ok_glyph_both_themes() {
        for verb in ALL {
            let last = steps(verb) - 1;
            let uni = frame(verb, last, Theme::new(false, true));
            assert!(uni.contains('✔'), "{verb:?} unicode final: {uni}");
            let asc = frame(verb, last, Theme::new(false, false));
            assert!(asc.is_ascii(), "{verb:?} ascii final leaked: {asc:?}");
            // the contract ascii ok-glyph is the WORD `ok` (space-bounded
            // so `invoke` can't satisfy it)
            assert!(asc.contains(" ok "), "{verb:?} ascii final: {asc}");
        }
    }

    #[test]
    fn infer_final_frame_is_pinned() {
        let f = frame(
            VerbKind::Infer,
            steps(VerbKind::Infer) - 1,
            Theme::new(false, true),
        );
        let expected = concat!(
            " ◆ infer · one-shot token spend\n",
            "   ${{ tasks.fetch.output }} ✔▸ prompt\n",
            "   ✔ 412 tk · $0.0019 · output captured\n",
        );
        assert_eq!(f, expected);
    }

    #[test]
    fn spinner_ticks_only_while_running() {
        // a mid-stream infer frame carries a spinner glyph; the final
        // frame carries none (one animated region · contract §3.2).
        let mid = frame(VerbKind::Infer, 8, Theme::new(false, true));
        assert!(SPINNER.iter().any(|s| mid.contains(s)), "{mid}");
        let last = frame(
            VerbKind::Infer,
            steps(VerbKind::Infer) - 1,
            Theme::new(false, true),
        );
        assert!(!SPINNER.iter().any(|s| last.contains(s)), "{last}");
    }

    #[test]
    fn binding_dot_travels_the_rail() {
        // the ● advances across the first beats of the infer scene.
        let t = Theme::new(false, true);
        let f0 = frame(VerbKind::Infer, 0, t);
        let f2 = frame(VerbKind::Infer, 2, t);
        let pos = |s: &str| s.find('●');
        assert!(pos(&f0).expect("dot at 0") < pos(&f2).expect("dot at 2"));
    }
}
