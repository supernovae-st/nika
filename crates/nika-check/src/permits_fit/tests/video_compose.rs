// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! stdlib §`nika:video_compose` Authority: every media input (each
//! `segments[k].image`, each `segments[k].audio`, the top-level `audio` · a
//! path or an object's `path`) is a read inside `permits.fs.read`, and `out`
//! a write inside `permits.fs.write`. A literal or whole bare-const path is
//! judged before anything runs; a computed one is the run's, EXCEPT under a
//! side that admits no path, where no rendered value can fit (the `vision:`
//! pattern); under an absent block the call is refused whatever form its
//! paths take (`NIKA-AUTH-006`). No `permits.exec` is required or implied.

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

const DIGEST: &str = "bde5666f4a0fab4949c4ca597fce3dd3dbfb55dfd0723f7bc0e01b044c978a29";

/// One `nika:video_compose` task with `args` under the `permits` lines
/// (`""` = no block).
fn escapes(permits: &str, args: &str) -> Vec<CapabilityEscape> {
    let yaml = format!(
        "nika: video\n{permits}const:\n  still: \"./out/frames/c.png\"\ninputs:\n  \
         cut: {{ type: string, required: true }}\n  film: {{ type: string, required: true }}\n  \
         first: {{ type: string, required: true }}\n  p: {{ type: string, required: true }}\n  \
         d: {{ type: string, required: true }}\ntasks:\n  video:\n    invoke:\n      tool: \
         \"nika:video_compose\"\n      args: {args}\n"
    );
    match parse(&yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => scan_escapes(&wf),
        Err(e) => panic!("the fixture must parse: {e}\n{yaml}"),
    }
}

const GRANTED: &str = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./out/frames/**\", \"./out/voice/**\"]\n    write: [\"./out/v.mp4\"]\n";

fn literal_call() -> String {
    format!(
        "{{ segments: [{{ image: \"./out/frames/a.png\", audio: \"./out/voice/a.wav\", pad_ms: 600 }}, \
         {{ image: {{ path: \"./out/frames/b.png\", sha256: \"{DIGEST}\" }}, \
         audio: {{ path: \"./out/voice/b.wav\", sha256: \"{DIGEST}\" }} }}, \
         {{ image: \"${{{{ const.still }}}}\", duration_ms: 3000 }}], out: \"./out/v.mp4\" }}"
    )
}

#[test]
fn a_lawful_call_fits_with_no_exec_grant() {
    let found = escapes(GRANTED, &literal_call());
    assert!(found.is_empty(), "{found:?}");
}

/// Each media input is judged on its own; the object form's `path` is the
/// read, and the finding names the slot.
#[test]
fn each_media_input_outside_the_read_boundary_escapes_by_name() {
    let narrow = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./out/frames/**\"]\n    write: [\"./out/v.mp4\"]\n";
    let found = escapes(narrow, &literal_call());
    let reads: Vec<&CapabilityEscape> = found
        .iter()
        .filter(|e| e.detail.contains("permits.fs.read"))
        .collect();
    assert_eq!(reads.len(), 2, "both clips escape: {found:?}");
    assert!(
        reads[0].detail.contains("`segments[0].audio`")
            && reads[1].detail.contains("`segments[1].audio`"),
        "{reads:?}"
    );
    assert!(reads.iter().all(|e| !e.undeclared && !e.floor));
    // The soundtrack is a media input like a clip.
    let track = escapes(
        narrow,
        "{ segments: [{ image: \"./out/frames/a.png\", duration_ms: 2000 }], audio: \"./music/track.wav\", out: \"./out/v.mp4\" }",
    );
    assert_eq!(track.len(), 1, "{track:?}");
    assert!(
        track[0].detail.contains("`audio`") && track[0].detail.contains("./music/track.wav"),
        "{track:?}"
    );
    // A bare const is a literal: judged too.
    let constant = escapes(
        "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./elsewhere/**\"]\n    write: [\"./out/v.mp4\"]\n",
        "{ segments: [{ image: \"${{ const.still }}\", duration_ms: 3000 }], out: \"./out/v.mp4\" }",
    );
    assert_eq!(constant.len(), 1, "{constant:?}");
    assert!(
        constant[0].detail.contains("./out/frames/c.png"),
        "{constant:?}"
    );
}

#[test]
fn out_outside_the_write_boundary_escapes_and_a_read_grant_never_satisfies_it() {
    let clips = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./out/**\"]\n    write: [\"./out/clips/*\"]\n";
    let found = escapes(
        clips,
        "{ segments: [{ image: \"./out/frames/a.png\", duration_ms: 2000 }], out: \"./out/storyboard.mp4\" }",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].detail.contains("permits.fs.write"), "{found:?}");
}

/// A computed media path or `out` defers to the run's boundary — except
/// under a side that admits no path: no rendered value can fit an empty
/// grant, so the call is refused at check (spec fixtures 086 and 116).
#[test]
fn an_empty_side_refuses_a_computed_path_at_check() {
    let deferred = "{ segments: \"${{ inputs.cut }}\", out: \"${{ inputs.film }}\" }";
    let both = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./out/**\"]\n    write: [\"./out/**\"]\n";
    assert!(escapes(both, deferred).is_empty(), "deferred to the run");
    let no_read = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    write: [\"./out/**\"]\n";
    let found = escapes(no_read, deferred);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .detail
            .contains("reads media at paths computed at run time")
            && found[0].detail.contains("permits.fs.read grants none"),
        "{found:?}"
    );
    let no_write = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./out/**\"]\n";
    let found = escapes(no_write, deferred);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .detail
            .contains("writes `out` at a path computed at run time"),
        "{found:?}"
    );
    // A templated element, or an object's templated path, hides a read too.
    for hidden in [
        "{ segments: [\"${{ inputs.first }}\"], out: \"./out/v.mp4\" }",
        "{ segments: [{ image: { path: \"${{ inputs.p }}\", sha256: \"${{ inputs.d }}\" }, duration_ms: 500 }], out: \"./out/v.mp4\" }",
    ] {
        let found = escapes(
            "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    write: [\"./out/v.mp4\"]\n",
            hidden,
        );
        assert_eq!(found.len(), 1, "{hidden}: {found:?}");
        assert!(
            found[0].detail.contains("permits.fs.read grants none"),
            "{found:?}"
        );
    }
}

const NARROW_READ: &str = "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    read: [\"./in/**\"]\n    write: [\"./out/v.mp4\"]\n";

const NO_READ: &str =
    "permits:\n  tools: [\"nika:video_compose\"]\n  fs:\n    write: [\"./out/v.mp4\"]\n";

/// A templated list or element defers to the run, and a whole bare
/// `${{ const.x }}` is no exception: naming a string, it renders a string,
/// which the run refuses before it reads (`-001`), so its text is never
/// judged as a read path. Only a side that admits no read refuses it here.
#[test]
fn a_templated_list_or_element_is_never_judged_as_a_path() {
    for hidden in [
        "{ segments: [\"${{ const.still }}\"], out: \"./out/v.mp4\" }",
        "{ segments: \"${{ const.still }}\", out: \"./out/v.mp4\" }",
    ] {
        let found = escapes(NARROW_READ, hidden);
        assert!(found.is_empty(), "{hidden}: {found:?}");
        let found = escapes(NO_READ, hidden);
        assert_eq!(found.len(), 1, "{hidden}: {found:?}");
        assert!(
            found[0]
                .detail
                .contains("reads media at paths computed at run time")
                && !found[0].detail.contains("./out/frames/c.png"),
            "{found:?}"
        );
    }
}

/// A literal `segments` or element of the wrong shape reads nothing: the
/// shape rules refuse it (`NIKA-BUILTIN-001`), and the boundary adds no read
/// finding beside it, under a narrow read grant or under none.
#[test]
fn a_malformed_literal_is_the_shape_rules_never_a_read() {
    for malformed in [
        "{ segments: [\"./secret/a.png\"], out: \"./out/v.mp4\" }",
        "{ segments: \"./secret/a.png\", out: \"./out/v.mp4\" }",
        "{ segments: 7, out: \"./out/v.mp4\" }",
    ] {
        for permits in [NARROW_READ, NO_READ] {
            let found = escapes(permits, malformed);
            assert!(found.is_empty(), "{malformed}: {found:?}");
        }
    }
}

/// The inferred boundary never prints a hidden slot's text as a grant: a
/// templated list or element names the fs face partial, and a malformed
/// literal reads nothing, so neither adds a read.
#[test]
fn the_inference_never_grants_a_hidden_or_malformed_slot() {
    for (segments, partial) in [
        ("[\"${{ const.seg }}\"]", true),
        ("\"${{ const.seg }}\"", true),
        ("[\"./in/title.png\"]", false),
        ("\"./in/title.png\"", false),
    ] {
        let args = format!("{{ segments: {segments}, out: \"./out/v.mp4\" }}");
        let yaml = format!(
            "nika: video\nconst:\n  seg: \"./in/title.png\"\ntasks:\n  video:\n    invoke:\n      \
             tool: \"nika:video_compose\"\n      args: {args}\n"
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let inferred = crate::infer_permits(&wf);
        let fs = inferred.permits.fs.as_ref().expect("`out` is a write");
        assert!(fs.read.is_empty(), "{args}: {:?}", fs.read);
        assert_eq!(fs.write, ["./out/v.mp4"], "{args}");
        assert_eq!(inferred.partial.fs, partial, "{args}: {:?}", inferred.notes);
    }
}

/// The same owner serves `nika:image_compose`'s `inputs`: a whole templated
/// list, a bare const naming a string included, is the run's, and a literal
/// that is not a list reads nothing (the shape rules refuse it).
#[test]
fn image_compose_s_hidden_or_malformed_inputs_are_never_a_path() {
    let narrow = "permits:\n  tools: [\"nika:image_compose\"]\n  fs:\n    read: [\"./in/**\"]\n    write: [\"./out/s.png\"]\n";
    for inputs in ["\"${{ const.frames }}\"", "\"./secret/a.png\""] {
        let yaml = format!(
            "nika: sheet\nconst:\n  frames: \"./secret/a.png\"\n{narrow}tasks:\n  sheet:\n    \
             invoke:\n      tool: \"nika:image_compose\"\n      args: {{ inputs: {inputs}, out: \
             \"./out/s.png\" }}\n"
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let found = scan_escapes(&wf);
        assert!(found.is_empty(), "{inputs}: {found:?}");
        let inferred = crate::infer_permits(&wf);
        assert!(
            inferred
                .permits
                .fs
                .as_ref()
                .is_none_or(|fs| fs.read.is_empty()),
            "{inputs}: {:?}",
            inferred.permits.fs
        );
    }
}

/// Under an absent block the call is refused whatever form its paths take:
/// every call reads a still and writes `out`.
#[test]
fn an_absent_block_refuses_every_form() {
    for args in [
        literal_call(),
        "{ segments: \"${{ inputs.cut }}\", out: \"${{ inputs.film }}\" }".to_owned(),
    ] {
        let found = escapes("", &args);
        assert!(!found.is_empty(), "{args}");
        assert!(found.iter().all(|e| e.undeclared), "AUTH-006: {found:?}");
    }
}

/// The absent-block repair names both sides the call needs, so following it
/// as worded (the tool, the media reads, the `out` write) re-checks clean;
/// `--infer-permits` prints that block.
#[test]
fn the_absent_block_repair_names_both_sides() {
    let found = escapes("", &literal_call());
    let fs: Vec<&CapabilityEscape> = found.iter().filter(|e| e.category == "fs").collect();
    assert_eq!(fs.len(), 1, "{found:?}");
    assert_eq!(
        fs[0].fix.as_deref(),
        Some(
            "add \"nika:video_compose\" to permits.tools, every path it reads to \
             permits.fs.read and the path it writes to permits.fs.write (`nika check \
             --infer-permits` prints the block)"
        ),
        "{found:?}"
    );
}

/// No `permits.exec` is required, consulted or implied — and a program
/// grant buys nothing: the tool grant is the door.
#[test]
fn the_tool_grant_is_the_door_never_exec() {
    let exec_only = "permits:\n  exec: [\"ffmpeg\"]\n  fs:\n    read: [\"./out/**\"]\n    write: [\"./out/**\"]\n";
    let found = escapes(exec_only, &literal_call());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].category, "tools");
    assert_eq!(
        found[0].fix.as_deref(),
        Some("add \"nika:video_compose\" to permits.tools")
    );
}

/// The inferred boundary grants every literal media path and `out`, so it
/// re-checks clean; a computed path names the fs face partial, and a path
/// that escapes the workspace is named for review, never pasted as a grant.
#[test]
fn the_inferred_boundary_round_trips_and_names_what_it_cannot_print() {
    let yaml = format!(
        "nika: video\ntasks:\n  video:\n    invoke:\n      tool: \"nika:video_compose\"\n      args: {}\n",
        literal_call().replace("${{ const.still }}", "./out/frames/c.png")
    );
    let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
    let inferred = crate::infer_permits(&wf);
    let fs = inferred.permits.fs.as_ref().expect("fs");
    assert_eq!(
        fs.read,
        [
            "./out/frames/a.png",
            "./out/frames/b.png",
            "./out/frames/c.png",
            "./out/voice/a.wav",
            "./out/voice/b.wav"
        ]
    );
    assert_eq!(fs.write, ["./out/v.mp4"]);
    assert_eq!(
        inferred.permits.tools.as_deref(),
        Some(&["nika:video_compose".to_owned()][..])
    );
    assert!(!inferred.partial.any(), "{:?}", inferred.notes);
    assert!(inferred.notes.is_empty(), "{:?}", inferred.notes);
    let spliced = yaml.replacen(
        "nika: video\n",
        &format!("nika: video\n{}", inferred.to_yaml()),
        1,
    );
    let rechecked = parse(&spliced, FileId::new(0), ParseMode::Strict).expect("parses");
    assert!(
        scan_escapes(&rechecked).is_empty(),
        "the inferred block re-checks clean:\n{spliced}"
    );

    let computed = "nika: video\ntasks:\n  video:\n    invoke:\n      tool: \"nika:video_compose\"\n      args: { segments: \"${{ inputs.cut }}\", out: \"/tmp/film.mp4\" }\ninputs:\n  cut: { type: string, required: true }\n";
    let wf = parse(computed, FileId::new(0), ParseMode::Strict).expect("parses");
    let inferred = crate::infer_permits(&wf);
    assert!(
        inferred.partial.fs,
        "a computed list and an escaping out leave fs partial"
    );
    assert!(
        inferred
            .permits
            .fs
            .as_ref()
            .is_none_or(|fs| fs.write.is_empty()),
        "an escaping out is never printed as a grant: {:?}",
        inferred.permits.fs
    );
    let notes = inferred.notes.join("\n");
    assert!(
        notes.contains("task `video` uses a dynamic path"),
        "{notes}"
    );
    assert!(
        notes.contains("`/tmp/film.mp4`") && notes.contains("escapes the workspace"),
        "{notes}"
    );
}
