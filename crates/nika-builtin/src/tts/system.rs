// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `system` provider — the host's OWN speech synthesizer, keyless and
//! offline: macOS `/usr/bin/say`, Linux `espeak-ng --stdout`.
//!
//! The engine starts the program ITSELF, as part of the builtin: it is
//! never the workflow's `exec:` — no `permits.exec` is required or
//! consulted, the program is fixed by the composition root (never workflow
//! data), and the text rides standard input (never the argument vector).
//! Each start is journaled as the builtin's own effect: one
//! `permit_checked` frame with `plane: builtin` (spec 17). The audio still
//! lands under `output_dir:` through the ordinary boundary-gated save.
//!
//! `say` cannot write to a pipe (it seeks back to finish the header), so it
//! writes a WAV into an engine scratch file the composition root chose; the
//! builtin reads it back through the kernel fs seam and removes it.
//! `espeak-ng` streams its WAV on stdout (its header declares the streamed
//! placeholder size — the sniffer counts the bytes present).
//!
//! A host with no synthesizer, or a missing program, fails `-002` naming
//! what is missing: never a silent fallback to another provider.

use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use nika_kernel::process::{ShellCommand, ShellError, ShellResult, ShellRunDyn};

use crate::BuiltinFailure;

use super::args::TtsArgs;
use super::types::{C_ARGS, C_NO_AUDIO, C_PROVIDER_UNAVAILABLE, C_REQUEST, ProviderAudio};

/// The `say` sample format: 16-bit little-endian PCM at 22.05 kHz, in a
/// WAVE container (the macOS voices' native rate · mono).
const SAY_DATA_FORMAT: &str = "--data-format=LEI16@22050";
/// Both synthesizers' default speaking rate (words per minute) — `speed`
/// scales it.
const DEFAULT_WPM: u32 = 175;
/// How much of the synthesizer's stderr a failure carries.
const STDERR_CAP: usize = 400;

/// The future a [`SpeechRunner`] returns (boxed: the seam is object-safe).
pub type SpeechFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ShellResult, ShellError>> + Send + 'a>>;

/// The process seam the `system` voice starts its synthesizer through —
/// object-safe, so the dispatcher needs no new type parameter. Every kernel
/// [`ShellRunDyn`] runner is one (the production `TokioShell` · a test's
/// recorder), through the blanket impl.
pub trait SpeechRunner: Send + Sync {
    /// Run one synthesizer command (the kernel shell contract: the child is
    /// killed when the future drops · stdin fed concurrently · the timeout
    /// on the command bounds it).
    fn run_speech(&self, command: ShellCommand) -> SpeechFuture<'_>;
}

impl<T: ShellRunDyn + Sync> SpeechRunner for T {
    fn run_speech(&self, command: ShellCommand) -> SpeechFuture<'_> {
        Box::pin(self.run(command))
    }
}

/// Which synthesizer this host offers — decided at the composition root
/// (the one sanctioned probe of the host), never from workflow data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SystemSynth {
    /// macOS `say`: writes a WAV file into `scratch_dir`, read back here.
    Say {
        /// The program (`/usr/bin/say`).
        program: PathBuf,
        /// The engine-owned directory the WAV is written into.
        scratch_dir: PathBuf,
    },
    /// `espeak-ng`: streams a WAV on standard output.
    EspeakNg {
        /// The resolved program.
        program: PathBuf,
    },
    /// No synthesizer here — `missing` names what is missing.
    Unavailable {
        /// What a `system` call is told (the program, the OS).
        missing: String,
    },
}

impl SystemSynth {
    /// macOS `say` writing into `scratch_dir`.
    #[must_use]
    pub fn say(program: impl Into<PathBuf>, scratch_dir: impl Into<PathBuf>) -> Self {
        Self::Say {
            program: program.into(),
            scratch_dir: scratch_dir.into(),
        }
    }

    /// `espeak-ng` at `program`.
    #[must_use]
    pub fn espeak_ng(program: impl Into<PathBuf>) -> Self {
        Self::EspeakNg {
            program: program.into(),
        }
    }

    /// No synthesizer on this host; `missing` is the refusal's teaching.
    #[must_use]
    pub fn unavailable(missing: impl Into<String>) -> Self {
        Self::Unavailable {
            missing: missing.into(),
        }
    }
}

/// The `system` voice as the composition root wires it: the runner and the
/// host's synthesizer. Installed voice names are listed once and cached.
pub struct SystemVoice {
    runner: Arc<dyn SpeechRunner>,
    synth: SystemSynth,
    voices: OnceLock<Vec<String>>,
}

impl SystemVoice {
    /// Wire the host synthesizer behind `runner`.
    #[must_use]
    pub fn new(runner: Arc<dyn SpeechRunner>, synth: SystemSynth) -> Self {
        Self {
            runner,
            synth,
            voices: OnceLock::new(),
        }
    }

    /// The synthesizer this voice starts.
    #[must_use]
    pub fn synth(&self) -> &SystemSynth {
        &self.synth
    }
}

impl fmt::Debug for SystemVoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemVoice")
            .field("synth", &self.synth)
            .finish_non_exhaustive()
    }
}

/// Speak `args.text` with the host synthesizer. `fs` reads back (and
/// removes) `say`'s scratch file — engine scratch, never a workflow path.
pub(crate) async fn generate<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    voice: Option<&SystemVoice>,
    args: &TtsArgs,
) -> Result<ProviderAudio, BuiltinFailure> {
    let voice = voice.ok_or_else(|| {
        BuiltinFailure::new(
            C_PROVIDER_UNAVAILABLE,
            "the system voice is not wired — this engine was composed without the host \
             synthesizer (a library host wires it with BuiltinDispatcher::with_system_voice)",
        )
    })?;
    let bytes = match &voice.synth {
        SystemSynth::Unavailable { missing } => {
            return Err(BuiltinFailure::new(
                C_PROVIDER_UNAVAILABLE,
                format!("provider: system cannot speak on this host — {missing}"),
            ));
        }
        SystemSynth::Say {
            program,
            scratch_dir,
        } => {
            if args.voice != "default" {
                ensure_installed(voice, program, &args.voice).await?;
            }
            say(fs, voice.runner.as_ref(), program, scratch_dir, args).await?
        }
        SystemSynth::EspeakNg { program } => espeak(voice.runner.as_ref(), program, args).await?,
    };
    Ok(ProviderAudio {
        bytes,
        // Local compute is unpriced, never free (the output names why).
        cost_usd: None,
        endpoint_host: None, // no wire was crossed
        warnings: Vec::new(),
    })
}

/// `say -o <scratch> --file-format=WAVE --data-format=LEI16@22050 [-v] [-r]`,
/// the text on stdin; the WAV is read back and the scratch removed.
async fn say<F: FsReadDyn + FsWriteDyn>(
    fs: &F,
    runner: &dyn SpeechRunner,
    program: &Path,
    scratch_dir: &Path,
    args: &TtsArgs,
) -> Result<Vec<u8>, BuiltinFailure> {
    // seam-bypass-ok: a scratch file name needs uniqueness across concurrent
    // calls, never a reproducible id — it never reaches the artifact.
    let scratch = scratch_dir.join(format!("nika-tts-{}.wav", uuid::Uuid::new_v4()));
    let mut command = ShellCommand::new(program.to_string_lossy())
        .arg("-o")
        .arg(scratch.to_string_lossy())
        .arg("--file-format=WAVE")
        .arg(SAY_DATA_FORMAT);
    if args.voice != "default" {
        command = command.arg("-v").arg(args.voice.clone());
    }
    if args.speed_explicit {
        command = command.arg("-r").arg(words_per_minute(args).to_string());
    }
    command.stdin = Some(args.text.clone());
    command.timeout = Some(args.timeout);
    witness(program, "synthesize the speech into an engine scratch file");
    let ran = run(runner, command, program).await;
    let read = match ran {
        Ok(()) => fs.read(&scratch).await.map_err(|e| {
            BuiltinFailure::new(
                C_NO_AUDIO,
                format!("the synthesizer exited 0 but left no audio file ({e})"),
            )
        }),
        Err(failure) => Err(failure),
    };
    // Best effort: the scratch never outlives the call (absent on failure).
    let _ = fs.remove_file(&scratch).await;
    Ok(read?.to_vec())
}

/// `espeak-ng --stdout --stdin [-v] [-s]`, the WAV captured from stdout.
async fn espeak(
    runner: &dyn SpeechRunner,
    program: &Path,
    args: &TtsArgs,
) -> Result<Vec<u8>, BuiltinFailure> {
    let mut command = ShellCommand::new(program.to_string_lossy())
        .arg("--stdout")
        .arg("--stdin");
    if args.voice != "default" {
        command = command.arg("-v").arg(args.voice.clone());
    }
    if args.speed_explicit {
        command = command.arg("-s").arg(words_per_minute(args).to_string());
    }
    command.stdin = Some(args.text.clone());
    command.timeout = Some(args.timeout);
    witness(program, "synthesize the speech onto its standard output");
    let result = runner
        .run_speech(command)
        .await
        .map_err(|e| shell_failure(&e, program))?;
    exit_ok(&result, program)?;
    Ok(result.stdout_octets().to_vec())
}

/// Refuse a voice the host does not have — `say` would read the text with
/// its default voice instead (measured: exit 0 · no message), the silent
/// substitution this contract forbids.
async fn ensure_installed(
    voice: &SystemVoice,
    program: &Path,
    wanted: &str,
) -> Result<(), BuiltinFailure> {
    let installed = if let Some(list) = voice.voices.get() {
        list.clone()
    } else {
        let mut command = ShellCommand::new(program.to_string_lossy())
            .arg("-v")
            .arg("?");
        command.timeout = Some(std::time::Duration::from_secs(30));
        witness(program, "list the voices this host has installed");
        let result = voice
            .runner
            .run_speech(command)
            .await
            .map_err(|e| shell_failure(&e, program))?;
        exit_ok(&result, program)?;
        let list = parse_say_voices(&String::from_utf8_lossy(result.stdout_octets()));
        let _ = voice.voices.set(list.clone());
        list
    };
    if installed.iter().any(|v| v.eq_ignore_ascii_case(wanted)) {
        return Ok(());
    }
    let sample: Vec<&str> = installed.iter().take(6).map(String::as_str).collect();
    Err(BuiltinFailure::new(
        C_ARGS,
        format!(
            "voice `{wanted}` is not installed on this host — `say -v '?'` lists {} voices \
             (e.g. {}); omit `voice:` for the host's default voice",
            installed.len(),
            sample.join(" · ")
        ),
    ))
}

/// The voice names of `say -v '?'` — each line is `<name> <locale> # <sample>`,
/// and a name may itself hold spaces and parentheses (`Eddy (English (UK))`).
pub(crate) fn parse_say_voices(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let head = line
                .split_once(" # ")
                .map_or(line, |(head, _)| head)
                .trim_end();
            let (name, _locale) = head.rsplit_once(char::is_whitespace)?;
            let name = name.trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

/// `round(175 × speed)` words per minute (both programs' rate unit).
fn words_per_minute(args: &TtsArgs) -> u32 {
    (args.speed_permille() * DEFAULT_WPM + 500) / 1000
}

/// One `permit_checked` frame per program start: the builtin's own effect.
fn witness(program: &Path, purpose: &str) {
    crate::witness::record_decision(
        "builtin",
        program.display().to_string(),
        "allow",
        format!(
            "nika:tts_generate (provider: system) starts the host synthesizer itself to \
             {purpose} — the builtin's own effect, never an exec: task, no permits.exec consulted"
        ),
    );
}

async fn run(
    runner: &dyn SpeechRunner,
    command: ShellCommand,
    program: &Path,
) -> Result<(), BuiltinFailure> {
    let result = runner
        .run_speech(command)
        .await
        .map_err(|e| shell_failure(&e, program))?;
    exit_ok(&result, program)
}

fn exit_ok(result: &ShellResult, program: &Path) -> Result<(), BuiltinFailure> {
    if result.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(result.stderr_octets());
    let said: String = stderr.trim().chars().take(STDERR_CAP).collect();
    Err(BuiltinFailure::new(
        C_REQUEST,
        format!(
            "the host synthesizer `{}` exited {}{}",
            program.display(),
            result.status,
            if said.is_empty() {
                String::new()
            } else {
                format!(": {said}")
            }
        ),
    ))
}

fn shell_failure(e: &ShellError, program: &Path) -> BuiltinFailure {
    match e {
        ShellError::NotFound { .. } => BuiltinFailure::new(
            C_PROVIDER_UNAVAILABLE,
            format!(
                "provider: system cannot speak on this host — the synthesizer `{}` was not found",
                program.display()
            ),
        ),
        ShellError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!("the host synthesizer timed out after {duration_ms} ms"),
        )
        .with_transient(true),
        other => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "the host synthesizer `{}` could not run: {other}",
                program.display()
            ),
        ),
    }
}

#[cfg(test)]
mod tests;
