// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `system` voice: the exact commands it starts (recorded by a fake
//! runner), a STUBBED BINARY spawned through the real exec runner (CI on
//! any Unix), and — on a Mac — the real `/usr/bin/say`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_fs::TokioFs;
use nika_kernel::process::{ShellCommand, ShellError, ShellResult, ShellRunDyn};

use super::*;
use crate::tts::args;
use crate::tts::types::{C_ARGS, C_PROVIDER_UNAVAILABLE, C_REQUEST};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("nika-system-voice-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn tts_args(extra: &serde_json::Value) -> TtsArgs {
    let mut map = serde_json::json!({
        "provider": "system", "text": "The sheet is ready.", "output_dir": "./out"
    });
    if let (Some(m), Some(e)) = (map.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    args::parse(map.as_object().unwrap()).expect("valid system args")
}

/// What one fake `say` start leaves behind.
#[derive(Debug, Default)]
struct Seen {
    commands: Vec<ShellCommand>,
}

/// A fake runner playing `say`: `-v ?` answers a voice listing, a synthesis
/// writes a real WAV at the `-o` path. Everything it was asked is recorded.
#[derive(Debug, Clone, Default)]
struct FakeSay {
    seen: Arc<Mutex<Seen>>,
    exit: i32,
}

const LISTING: &str = "Albert              en_US    # Hello! My name is Albert.\n\
Eddy (English (UK)) en_GB    # Hello! My name is Eddy.\n\
Samantha            en_US    # Hello! My name is Samantha.\n";

impl ShellRunDyn for FakeSay {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        self.seen.lock().unwrap().commands.push(command.clone());
        if command.args == ["-v", "?"] {
            return Ok(ShellResult::new(0, LISTING, "", Duration::ZERO));
        }
        if self.exit != 0 {
            return Ok(ShellResult::new(
                self.exit,
                "",
                "Voice engine failed: -128",
                Duration::ZERO,
            ));
        }
        let out = command
            .args
            .iter()
            .position(|a| a == "-o")
            .map(|i| &command.args[i + 1]);
        std::fs::write(
            out.expect("say is given -o"),
            crate::tts::mock::render_wav("hi", 1000),
        )
        .unwrap();
        Ok(ShellResult::new(0, "", "", Duration::ZERO))
    }
}

fn say_voice(fake: &FakeSay, scratch: &Path) -> SystemVoice {
    SystemVoice::new(
        Arc::new(fake.clone()),
        SystemSynth::say("/usr/bin/say", scratch),
    )
}

#[tokio::test]
async fn say_reads_the_text_from_stdin_and_reads_back_its_scratch_wav() {
    let root = scratch();
    let fake = FakeSay::default();
    let voice = say_voice(&fake, &root);
    let args = tts_args(&serde_json::json!({}));
    let audio = generate(&TokioFs, Some(&voice), &args)
        .await
        .expect("speaks");
    assert_eq!(audio.bytes, crate::tts::mock::render_wav("hi", 1000));
    assert_eq!(
        audio.cost_usd, None,
        "local compute is unpriced, never a priced 0"
    );
    assert_eq!(audio.endpoint_host, None, "no wire was crossed");
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.commands.len(), 1, "the default voice needs no listing");
    let cmd = &seen.commands[0];
    assert_eq!(cmd.program, "/usr/bin/say");
    assert_eq!(cmd.args[0], "-o");
    let scratch_file = PathBuf::from(&cmd.args[1]);
    assert!(
        scratch_file.starts_with(&root),
        "an engine scratch path: {scratch_file:?}"
    );
    assert_eq!(
        &cmd.args[2..],
        ["--file-format=WAVE", "--data-format=LEI16@22050"]
    );
    assert_eq!(
        cmd.stdin.as_deref(),
        Some("The sheet is ready."),
        "the text rides stdin"
    );
    assert!(
        !cmd.args.iter().any(|a| a.contains("sheet")),
        "never the argv"
    );
    assert_eq!(cmd.timeout, Some(Duration::from_millis(120_000)));
    assert!(
        !scratch_file.exists(),
        "the scratch never outlives the call"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_named_voice_is_checked_against_the_installed_list_once() {
    let root = scratch();
    let fake = FakeSay::default();
    let voice = say_voice(&fake, &root);
    let named = tts_args(&serde_json::json!({ "voice": "Eddy (English (UK))", "speed": 1.5 }));
    generate(&TokioFs, Some(&voice), &named)
        .await
        .expect("an installed voice speaks");
    generate(&TokioFs, Some(&voice), &named)
        .await
        .expect("again");
    let missing = tts_args(&serde_json::json!({ "voice": "Nope" }));
    let err = generate(&TokioFs, Some(&voice), &missing)
        .await
        .expect_err("never the default voice instead");
    assert_eq!(err.code, C_ARGS);
    assert!(
        err.message.contains("voice `Nope` is not installed")
            && err.message.contains("lists 3 voices"),
        "{}",
        err.message
    );
    let seen = fake.seen.lock().unwrap();
    let listings = seen
        .commands
        .iter()
        .filter(|c| c.args == ["-v", "?"])
        .count();
    assert_eq!(listings, 1, "the installed list is read once and cached");
    let synth = seen
        .commands
        .iter()
        .find(|c| c.args.first().is_some_and(|a| a == "-o"))
        .unwrap();
    let tail: Vec<&str> = synth.args[4..].iter().map(String::as_str).collect();
    // round(175 × 1.5) = 263 words per minute
    assert_eq!(tail, ["-v", "Eddy (English (UK))", "-r", "263"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_failing_synthesizer_is_a_non_transient_003_with_its_words() {
    let root = scratch();
    let fake = FakeSay {
        exit: 1,
        ..FakeSay::default()
    };
    let err = generate(
        &TokioFs,
        Some(&say_voice(&fake, &root)),
        &tts_args(&serde_json::json!({})),
    )
    .await
    .expect_err("exit 1");
    assert_eq!(err.code, C_REQUEST);
    assert!(!err.transient);
    assert!(
        err.message.contains("exited 1: Voice engine failed: -128"),
        "{}",
        err.message
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// One fixed outcome (`ShellError` is not `Clone`: built per start).
#[derive(Debug, Clone)]
enum Outcome {
    Exit(ShellResult),
    NotFound(&'static str),
    Timeout(u64),
}

/// A runner that answers every start with one fixed outcome.
#[derive(Debug, Clone)]
struct Answer(Outcome, Arc<Mutex<Vec<ShellCommand>>>);

impl ShellRunDyn for Answer {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        self.1.lock().unwrap().push(command);
        match &self.0 {
            Outcome::Exit(result) => Ok(result.clone()),
            Outcome::NotFound(program) => Err(ShellError::NotFound {
                program: (*program).to_owned(),
            }),
            Outcome::Timeout(ms) => Err(ShellError::Timeout { duration_ms: *ms }),
        }
    }
}

#[tokio::test]
async fn espeak_streams_its_wav_on_stdout() {
    let wav = crate::tts::mock::render_wav("bonjour", 1000);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let answer = Answer(
        Outcome::Exit(
            ShellResult::new(0, "", "", Duration::ZERO).with_raw(wav.clone(), Vec::new()),
        ),
        Arc::clone(&seen),
    );
    let voice = SystemVoice::new(
        Arc::new(answer),
        SystemSynth::espeak_ng("/usr/bin/espeak-ng"),
    );
    let args = tts_args(&serde_json::json!({ "voice": "fr", "speed": 0.5 }));
    let audio = generate(&TokioFs, Some(&voice), &args)
        .await
        .expect("speaks");
    assert_eq!(audio.bytes, wav);
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.len(),
        1,
        "espeak-ng refuses an unknown voice itself · no listing"
    );
    assert_eq!(seen[0].program, "/usr/bin/espeak-ng");
    // round(175 × 0.5) = 88 words per minute
    assert_eq!(
        seen[0].args,
        ["--stdout", "--stdin", "-v", "fr", "-s", "88"]
    );
    assert_eq!(seen[0].stdin.as_deref(), Some("The sheet is ready."));
}

#[tokio::test]
async fn a_missing_synthesizer_is_002_naming_what_is_missing() {
    let args = tts_args(&serde_json::json!({}));
    let err = generate(&TokioFs, None, &args).await.expect_err("unwired");
    assert_eq!(err.code, C_PROVIDER_UNAVAILABLE);
    assert!(err.message.contains("with_system_voice"), "{}", err.message);

    let none = Answer(
        Outcome::Exit(ShellResult::new(0, "", "", Duration::ZERO)),
        Arc::default(),
    );
    let host = SystemVoice::new(
        Arc::new(none),
        SystemSynth::unavailable("espeak-ng was not found on PATH (install espeak-ng)"),
    );
    let err = generate(&TokioFs, Some(&host), &args)
        .await
        .expect_err("no synthesizer");
    assert_eq!(err.code, C_PROVIDER_UNAVAILABLE);
    assert!(
        err.message.contains("espeak-ng was not found on PATH"),
        "{}",
        err.message
    );

    let gone = Answer(Outcome::NotFound("espeak-ng"), Arc::default());
    let host = SystemVoice::new(
        Arc::new(gone),
        SystemSynth::espeak_ng("/opt/none/espeak-ng"),
    );
    let err = generate(&TokioFs, Some(&host), &args)
        .await
        .expect_err("not found at start");
    assert_eq!(err.code, C_PROVIDER_UNAVAILABLE);
    assert!(
        err.message.contains("/opt/none/espeak-ng` was not found"),
        "{}",
        err.message
    );

    let slow = Answer(Outcome::Timeout(5), Arc::default());
    let host = SystemVoice::new(Arc::new(slow), SystemSynth::espeak_ng("/usr/bin/espeak-ng"));
    let err = generate(&TokioFs, Some(&host), &args)
        .await
        .expect_err("timed out");
    assert_eq!((err.code, err.transient), (C_REQUEST, true));
}

/// The audit trail: every program start is a `permit_checked` frame on the
/// `builtin` plane, naming the program — never an exec decision.
#[tokio::test]
async fn each_start_is_journaled_as_the_builtins_own_effect() {
    let root = scratch();
    let fake = FakeSay::default();
    let voice = say_voice(&fake, &root);
    let witness = Arc::new(nika_cap::PermitWitness::new());
    let args = tts_args(&serde_json::json!({ "voice": "Samantha" }));
    crate::witness::scope_attempt_witness(
        Arc::clone(&witness),
        generate(&TokioFs, Some(&voice), &args),
    )
    .await
    .expect("speaks");
    let frames = witness.take();
    assert_eq!(frames.len(), 2, "the listing and the synthesis: {frames:?}");
    for frame in &frames {
        assert_eq!(
            (frame.plane, frame.gate.as_str(), frame.decision),
            ("builtin", "/usr/bin/say", "allow")
        );
        assert!(frame.why.contains("never an exec: task"), "{}", frame.why);
    }
    assert!(frames[0].why.contains("list the voices"));
    assert!(frames[1].why.contains("synthesize the speech"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn say_voice_names_keep_their_spaces_and_parentheses() {
    assert_eq!(
        parse_say_voices(LISTING),
        ["Albert", "Eddy (English (UK))", "Samantha"]
    );
    assert!(parse_say_voices("\n").is_empty());
}

/// Write an executable stub program into `dir`.
#[cfg(unix)]
fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The CI proof: a STUBBED `say` and a stubbed `espeak-ng`, spawned through
/// the production exec runner (`TokioShell`: composed environment · stdin
/// fed concurrently · kill on drop). Each stub records its argv and stdin
/// and plays back a fixture WAV the way the real program delivers it.
#[cfg(unix)]
#[tokio::test]
async fn stubbed_binaries_run_through_the_real_exec_runner() {
    let root = scratch();
    let fixture = root.join("fixture.wav");
    std::fs::write(&fixture, crate::tts::mock::render_wav("stub", 1000)).unwrap();
    let log = root.join("argv.log");
    let heard = root.join("stdin.txt");
    let say = stub(
        &root,
        "say",
        &format!(
            "printf '%s\\n' \"$@\" > '{log}'\n/bin/cat > '{heard}'\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = -o ]; then /bin/cp '{fixture}' \"$2\"; fi\n  shift\ndone",
            log = log.display(),
            heard = heard.display(),
            fixture = fixture.display()
        ),
    );
    let runner: Arc<dyn SpeechRunner> = Arc::new(nika_exec_runner::TokioShell::new());
    let voice = SystemVoice::new(Arc::clone(&runner), SystemSynth::say(&say, &root));
    let args = tts_args(&serde_json::json!({}));
    let audio = generate(&TokioFs, Some(&voice), &args)
        .await
        .expect("the stub speaks");
    assert_eq!(audio.bytes, std::fs::read(&fixture).unwrap());
    let logged = std::fs::read_to_string(&log).unwrap();
    let words: Vec<&str> = logged.lines().collect();
    assert_eq!(words[0], "-o");
    assert_eq!(
        &words[2..],
        ["--file-format=WAVE", "--data-format=LEI16@22050"]
    );
    assert_eq!(
        std::fs::read_to_string(&heard).unwrap(),
        "The sheet is ready."
    );
    assert!(!Path::new(words[1]).exists(), "the scratch WAV was removed");

    let espeak = stub(
        &root,
        "espeak-ng",
        &format!(
            "printf '%s\\n' \"$@\" > '{log}'\n/bin/cat > '{heard}'\n/bin/cat '{fixture}'",
            log = log.display(),
            heard = heard.display(),
            fixture = fixture.display()
        ),
    );
    let voice = SystemVoice::new(runner, SystemSynth::espeak_ng(&espeak));
    let audio = generate(&TokioFs, Some(&voice), &args)
        .await
        .expect("the stub streams");
    assert_eq!(audio.bytes, std::fs::read(&fixture).unwrap());
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["--stdout", "--stdin"]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// On a Mac, the REAL `/usr/bin/say`: a playable 16-bit mono WAV whose
/// duration is computed from the bytes it wrote.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn the_real_say_speaks_a_wav_on_this_mac() {
    // The only test that reads the bytes back: its imports live with it,
    // so a non-macOS build has nothing unused.
    use crate::tts::sniff;
    use crate::tts::types::AudioFormat;

    let program = Path::new("/usr/bin/say");
    if !program.is_file() {
        return; // a Mac without the speech tools: nothing to prove here
    }
    let root = scratch();
    let voice = SystemVoice::new(
        Arc::new(nika_exec_runner::TokioShell::new()),
        SystemSynth::say(program, &root),
    );
    let args =
        tts_args(&serde_json::json!({ "text": "Nika composes images and speaks, with no key." }));
    let audio = generate(&TokioFs, Some(&voice), &args)
        .await
        .expect("say speaks");
    let sniffed = sniff::sniff(&audio.bytes).expect("a WAV");
    assert_eq!(sniffed.format, AudioFormat::Wav);
    let ms = sniffed.duration_ms.expect("WAV header math");
    // 44 bytes per 1 ms at 22.05 kHz · 16-bit mono: a short sentence is seconds, never hours.
    assert!(
        (1_000..=20_000).contains(&ms),
        "a real sentence, measured: {ms} ms"
    );
    assert_eq!(&audio.bytes[22..24], &1u16.to_le_bytes(), "mono");
    assert_eq!(&audio.bytes[24..28], &22_050u32.to_le_bytes(), "22.05 kHz");
    let _ = std::fs::remove_dir_all(&root);
}
