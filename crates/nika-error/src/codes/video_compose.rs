// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika:video_compose` ladder (stdlib §Media graduate #6): each of the
//! eleven codes with its spec category, its cause and its working exits.
//! `image_fx`'s ladder for `-001` to `-006`, the generators' `-007`, then
//! the encoder's own `-008` to `-011`.

/// The footer of a code whose recovery is not a contract (stdlib
/// §`nika:video_compose` · Refusing early): an engine that refuses the
/// launch never reaches a workflow's recovery. `None` keeps the namespace
/// footer.
pub(super) fn footer(num: &str) -> Option<&'static str> {
    matches!(num, "010" | "011").then_some(
        "Listing it in `retry.on_codes:` or `on_error.on_codes:` is accepted, but recovering \
         from it is not a contract: an engine that refuses the launch never reaches the recovery",
    )
}

/// `(category, lesson)` of `NIKA-BUILTIN-VIDEO_COMPOSE-<num>`, `None` for a
/// number the contract does not allocate.
pub(super) fn help(num: &str) -> Option<(&'static str, &'static str)> {
    Some(match num {
        "001" => ("validation_error", ARGS),
        "002" => ("tool_error", INPUT),
        "003" => ("validation_error", FORMAT),
        "004" => ("tool_error", DECODE),
        "005" => ("security_error", BUDGET),
        "006" => ("tool_error", SAVE),
        "007" => ("tool_error", VERIFY),
        "008" => ("tool_error", ENCODER),
        "009" => ("tool_error", DEADLINE),
        "010" => ("validation_error", NO_ENCODER),
        "011" => ("security_error", CONFINEMENT),
        _ => return None,
    })
}

const ARGS: &str = "  `nika:video_compose` refused its arguments before reading any input: an \
     unknown key (segments · audio · audio_codec · out · size · fps · scale · background · \
     quality · transition · metadata · timeout_ms — captions are not v1), `segments` not a \
     list of 1..=200 closed maps { image, audio, duration_ms, pad_ms }, a media reference \
     that is neither a path nor an object carrying BOTH `path` and a `sha256` of 64 \
     lowercase hex digits, a segment with no clip and no `duration_ms`, `pad_ms` beside \
     `duration_ms`, a duration shorter than its clip, a segment that would show no frame, \
     a top-level `audio` soundtrack beside a clip, an `out` not ending in .mp4, a `size` \
     that is not WIDTHxHEIGHT with both sides even (64..=3840 × 64..=2160), `fps` outside \
     1..=60, a word outside its closed set (`transition: fade` is reserved: v1 cuts), \
     `metadata` over 16 KiB as canonical JSON, or `timeout_ms` outside 1000..=3600000.\n\n  \
     exits (pick one):\n    \
     · a still without a clip holds:  { image: …, duration_ms: 3000 }\n    \
     · a producer's object passes as it is:  audio: ${{ tasks.voice.output.audio }}\n    \
     · `nika check` names the field before the run does.\n";

const INPUT: &str = "  An input could not be read (the message names the segment): a missing \
     file, a directory, a permission error. The read was inside the declared \
     `permits.fs.read` boundary — a path outside it is NIKA-SEC-004 instead, judged before \
     anything is opened.\n\n  \
     exits (pick one):\n    \
     · check the path relative to where the run starts\n    \
     · make the producing task run first: `after: { <task>: success }`\n";

const FORMAT: &str = "  An input this version does not take (the message names it, with a \
     hint). A still is a PNG of depth 8 — gray, gray+alpha, RGB or RGBA, not palette-indexed, \
     not interlaced. A clip or soundtrack is a WAV of 8-, 16- or 24-bit integer PCM or 32-bit \
     float, mono or stereo, at 8000..=96000 Hz. In this version clips share one sample rate, \
     and an `aac` track takes an MPEG-4 sampling rate.\n\n  \
     exits (pick one):\n    \
     · JPEG or WebP stills:  produce PNG upstream (nika:image_generate { …, format: png })\n    \
     · MP3 clips:            ask nika:tts_generate for `format: wav`\n    \
     · a rate off the table: `audio_codec: flac`, or resample upstream\n";

const DECODE: &str = "  An input could not be decoded — a corrupt PNG or WAV — or its bytes do \
     not hash to the `sha256` of its { path, sha256 } reference: the file changed after the \
     task that wrote it reported its digest.\n\n  \
     exits (pick one):\n    \
     · re-run the producing task, so the reference names the bytes on disk\n    \
     · re-produce a damaged file\n";

const BUDGET: &str = "  A budget: more than 200 segments, a video over one hour, a still over \
     512 MiB or a clip or soundtrack over 2 GiB (judged on the file's size before it is \
     read), a still over 16384 per side or 2^26 pixels (judged from its header before any \
     decompression), a `fit` or `fill` scaled box over the same caps, the plan's \
     intermediates over 1 GiB ((segments + 1) × W × H × 3/2 bytes of frames plus \
     T × channels × 2 bytes of audio), an argument to the encoder over 64 KiB, or an output \
     over 256 MiB, a tripped size limit included. Never transient: the same inputs and \
     arguments hit the same cap.\n\n  \
     exits (pick one):\n    \
     · a smaller frame:    args: { …, size: \"1280x720\" }\n    \
     · shrink the stills:  nika:image_fx { ops: [ { resize: { width: 1280 } } ] }\n    \
     · split the video:    two calls over fewer segments\n";

const SAVE: &str = "  Saving failed: the private scratch directory could not be created or \
     written (a full disk included), or the atomic save at `out:` failed. `out:` was inside \
     the declared `permits.fs.write` boundary — a path outside it is NIKA-SEC-004 \
     instead.\n\n  \
     exits (pick one):\n    \
     · free disk space where the engine stages its scratch\n    \
     · check that the directory of `out:` is writable\n";

const VERIFY: &str = "  The file the encoder produced does not match the plan, so nothing was \
     written. An exit status of 0 proves nothing: before saving, the engine reads the file \
     back — `ftyp` first and `moov` before `mdat`, one H.264 track of the planned size and \
     frame count, every segment start a keyframe with no gap above ten seconds of frames, \
     one audio track of the planned codec, rate and channels whose presented length is \
     exactly the planned samples.\n\n  \
     exits (pick one):\n    \
     · try another ffmpeg build (NIKA_FFMPEG names it)\n    \
     · report the message: it names the first mismatch\n";

const ENCODER: &str = "  The encoder failed: it exited non-zero, or a signal the engine did not \
     send killed it. The message carries the status and at most 400 characters of its \
     error output. A muxer, filter or option the engine's capability probe does not cover \
     fails here too. Not transient.\n\n  \
     exits (pick one):\n    \
     · read the error output in the message\n    \
     · upgrade or rebuild ffmpeg (6.0 or later, with libx264)\n";

const DEADLINE: &str = "  The deadline passed — `timeout_ms` or the task's, whichever comes \
     first — while the call waited for the encode slot or encoded: the program, if it had \
     started, was killed, drained and reaped, and nothing was written. An engine process \
     may run one encode at a time, and waiting counts. A user's Stop is not this code: it \
     ends the call through the run's cancellation. Not transient.\n\n  \
     exits (pick one):\n    \
     · a longer deadline:   args: { …, timeout_ms: 1800000 } (at most 3600000)\n    \
     · less to encode:      `quality: draft`, a smaller `size`, fewer segments\n";

const NO_ENCODER: &str = "  No usable encoder on this host (the message names what is missing \
     and the fix). The engine resolves ffmpeg from its own configuration, never from the \
     workflow: NIKA_FFMPEG (an absolute path · a set value that is not trusted is refused, \
     never a fall-through), else the first trusted candidate among \
     /opt/homebrew/bin/ffmpeg, /usr/local/bin/ffmpeg and /usr/bin/ffmpeg — PATH is never \
     consulted. A candidate is trusted when its canonical path is a regular executable and \
     no directory above the candidate or above its canonical path is writable by others; a \
     program the workflow's `permits.fs.write` could write is refused on every call. The \
     floor is FFmpeg 6.0 (libavformat and libavcodec 60 or later) with libx264 and the \
     requested audio encoder (aac · flac). An operating system other than macOS and Linux \
     is refused here, and so is an engine build that carries no video backend. Never a \
     fallback to another program or codec. The engine includes, links and distributes no \
     codec: it starts your own ffmpeg, whose build carries its own licence.\n\n  \
     exits (pick one):\n    \
     · install ffmpeg:    brew install ffmpeg (macOS) · apt install ffmpeg (Debian, Ubuntu)\n    \
     · name a build:      export NIKA_FFMPEG=/absolute/path/to/ffmpeg\n    \
     · Nix or Conda:      a build whose libraries live outside the system roots cannot \
     load inside the jail — point NIKA_FFMPEG at a static build\n";

const CONFINEMENT: &str = "  Confinement failed: ffmpeg runs inside the engine's own OS jail, \
     the one that confines `exec:` tasks (Seatbelt on macOS, the landlock backend on Linux), \
     and none could start with no waiver, a profile could not be built, or the jail \
     reported a denial during the call. On stock Ubuntu 24.04 the AppArmor restriction on \
     unprivileged user namespaces stops the Linux jail, as an unprivileged container does. \
     Never transient.\n\n  \
     exits (pick one):\n    \
     · change the host:   an AppArmor profile for the jail, or the user-namespace setting\n    \
     · waive it on purpose: NIKA_SANDBOX=off runs ffmpeg unconfined (still on raw samples \
     only) and the outputs say `backend.confinement: waived`\n";

#[cfg(test)]
mod tests {
    use super::super::namespace_help;

    /// Each of the eleven codes teaches its spec category, its cause and its
    /// exits — never the namespace boilerplate — and no page claims the
    /// encoder has no network access.
    #[test]
    fn every_code_teaches_its_category_cause_and_exits() {
        for (num, category, lesson) in [
            ("001", "validation_error", "captions are not v1"),
            ("002", "tool_error", "names the segment"),
            (
                "003",
                "validation_error",
                "ask nika:tts_generate for `format: wav`",
            ),
            ("004", "tool_error", "do not hash to the `sha256`"),
            ("005", "security_error", "Never transient"),
            ("006", "tool_error", "atomic save"),
            ("007", "tool_error", "An exit status of 0 proves nothing"),
            ("008", "tool_error", "at most 400 characters"),
            ("009", "tool_error", "killed, drained and reaped"),
            ("010", "validation_error", "PATH is never consulted"),
            ("011", "security_error", "NIKA_SANDBOX=off"),
        ] {
            let code = format!("NIKA-BUILTIN-VIDEO_COMPOSE-{num}");
            let help = namespace_help(&code, "docs").expect("teaches");
            assert!(
                help.starts_with(&format!(
                    "{code} · builtin · {category} · the `nika:video_compose` contract"
                )),
                "{help}"
            );
            assert!(help.contains(lesson), "{num}: {help}");
            assert!(help.contains("exits (pick one)"), "{num}: {help}");
            assert!(
                !help.to_lowercase().contains("no network"),
                "{num} must never claim no network access: {help}"
            );
        }
        let unknown = namespace_help("NIKA-BUILTIN-VIDEO_COMPOSE-012", "docs").expect("namespace");
        assert!(
            unknown.contains("per-builtin runtime diagnostic"),
            "{unknown}"
        );
        // The host program's resolution is spelled as the engine resolves it.
        let resolution = namespace_help("NIKA-BUILTIN-VIDEO_COMPOSE-010", "docs").expect("teaches");
        for candidate in [
            "NIKA_FFMPEG",
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
            "FFmpeg 6.0",
            "libx264",
        ] {
            assert!(resolution.contains(candidate), "{candidate}: {resolution}");
        }
    }

    /// Recovering from `-010` or `-011` is not a contract a workflow can rely
    /// on (an engine that refuses the launch never reaches the recovery):
    /// their pages never offer `on_codes:` as a way out. The other codes keep
    /// the namespace footer.
    #[test]
    fn the_host_codes_never_offer_recovery_as_a_contract() {
        let footer = "Valid in `retry.on_codes:` and `on_error.on_codes:`";
        for num in ["010", "011"] {
            let code = format!("NIKA-BUILTIN-VIDEO_COMPOSE-{num}");
            let help = namespace_help(&code, "docs").expect("teaches");
            assert!(!help.contains(footer), "{help}");
            assert!(
                help.contains("recovering from it is not a contract")
                    && help.contains("refuses the launch"),
                "{help}"
            );
        }
        let help = namespace_help("NIKA-BUILTIN-VIDEO_COMPOSE-002", "docs").expect("teaches");
        assert!(help.contains(footer), "{help}");
    }

    /// `-010` states the trust rule and the platform case as the contract
    /// does: no directory above the candidate or above its canonical path is
    /// writable by others, and a host other than macOS and Linux is refused.
    #[test]
    fn the_no_encoder_page_states_the_trust_rule_whole() {
        let help = namespace_help("NIKA-BUILTIN-VIDEO_COMPOSE-010", "docs").expect("teaches");
        for clause in [
            "no directory above the candidate or above its canonical path is writable by others",
            "operating system other than macOS and Linux is refused",
        ] {
            assert!(help.contains(clause), "{clause}: {help}");
        }
    }

    /// A ladder that states no category keeps the plain header.
    #[test]
    fn a_ladder_without_categories_keeps_its_header() {
        let help = namespace_help("NIKA-BUILTIN-IMAGE_COMPOSE-005", "docs").expect("teaches");
        assert!(
            help.starts_with(
                "NIKA-BUILTIN-IMAGE_COMPOSE-005 · builtin · the `nika:image_compose` contract"
            ),
            "{help}"
        );
    }
}
