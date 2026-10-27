// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which `nika` is running, and which other `nika` sits on PATH (#1614).
//!
//! `doctor` and `welcome` printed a version and nothing else, so a shadowed
//! install read healthy: an upgraded copy stood behind an older one that
//! kept running, and the report still said `11 ok`. This module names the
//! running binary by its resolved path, its build and its installer, walks
//! PATH the way a shell does, and asks every other `nika` found there for
//! its `--version`, all under one deadline: a copy that hangs or fails is
//! reported, never fatal.
//!
//! Three limits are deliberate. A relative PATH entry, or one spelled with
//! a literal `~` (bash expands it, zsh does not), is listed but never
//! executed: looking at a project directory must not run its code. Each
//! copy that is asked runs contained (see [`ask`]): an allow-listed
//! environment without credentials, a PATH of absolute entries only, `/`
//! as its working directory, and its whole process group killed after the
//! answer. The installer is read from the path alone (a Homebrew Cellar,
//! an npm package, a cargo bin directory, `~/.nika/releases`, the
//! `install.sh` default, a Nix store, a cargo `target` directory); any
//! other layout is `unknown`, never a guess.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod ask;
mod render;

pub(crate) use render::{doctor_findings, insert_json, welcome_lines, with_json};

/// How long one `--version` answer may take before the copy is reported
/// as not answering (and killed).
pub(crate) const VERSION_DEADLINE: Duration = Duration::from_secs(2);

/// The most distinct `nika` files listed from PATH (the running binary is
/// still looked for beyond them).
pub(crate) const MAX_ENTRIES: usize = 16;

/// The longest version or commit word read from another copy.
const MAX_WORD: usize = 64;

/// A version and the commit it was built from, as `nika --version` prints
/// them (`nika 0.123.2 (5591478a7)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Build {
    pub(crate) version: String,
    /// `None` for a build that names no commit (a gitless source build).
    pub(crate) commit: Option<String>,
}

impl Build {
    /// This process's own build, compiled in.
    fn current() -> Self {
        let identity = nika_runtime::engine_identity();
        let sha = identity.build_sha();
        Self {
            version: identity.engine_version().to_owned(),
            commit: (sha != "unknown" && !sha.is_empty()).then(|| sha.to_owned()),
        }
    }

    /// `nika 0.123.2 (5591478a7)`, the words `--version` prints.
    pub(crate) fn label(&self) -> String {
        match &self.commit {
            Some(commit) => format!("nika {} ({commit})", self.version),
            None => format!("nika {}", self.version),
        }
    }

    /// `v0.123.2 (5591478a7)`, the `binary` row's spelling: a line under
    /// `this binary` that opened on `nika ` would read as a command to type.
    pub(crate) fn short(&self) -> String {
        match &self.commit {
            Some(commit) => format!("v{} ({commit})", self.version),
            None => format!("v{}", self.version),
        }
    }

    /// The same release build: the same version, and the same commit when
    /// both name one. A build that names no commit is judged by its version.
    pub(crate) fn same_build(&self, other: &Self) -> bool {
        self.version == other.version
            && match (&self.commit, &other.commit) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}

/// Read `nika --version` output: the first non-empty line, `nika <version>`
/// with an optional `(<commit>)`. Anything else is not a nika answer, and
/// neither is a version or commit word outside its plain characters (this
/// text reaches a terminal: an escape sequence is never echoed).
pub(crate) fn parse_version_output(stdout: &str) -> Option<Build> {
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut words = line.split_whitespace();
    if words.next()? != "nika" {
        return None;
    }
    let version = words.next()?.trim_start_matches('v');
    if !version.starts_with(|c: char| c.is_ascii_digit()) || !plain_word(version, ".-+") {
        return None;
    }
    let commit = match words
        .next()
        .and_then(|w| w.strip_prefix('(')?.strip_suffix(')'))
    {
        Some(commit) if !plain_word(commit, "-") => return None,
        commit => commit.map(str::to_owned),
    };
    Some(Build {
        version: version.to_owned(),
        commit,
    })
}

/// A non-empty word of at most [`MAX_WORD`] ASCII letters, digits and `extra`.
fn plain_word(word: &str, extra: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// `a` against `b` as release versions (`MAJOR.MINOR.PATCH[-pre]`): a
/// pre-release sorts before its release. Two different pre-releases of one
/// version, or a version that does not parse, have no order.
pub(crate) fn version_order(a: &str, b: &str) -> Option<Ordering> {
    /// `MAJOR.MINOR.PATCH` and the pre-release tag, when there is one.
    type Key<'a> = ((u64, u64, u64), Option<&'a str>);
    fn key(v: &str) -> Option<Key<'_>> {
        let v = v.trim_start_matches('v');
        let v = v.split('+').next()?;
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (v, None),
        };
        let mut parts = core.split('.');
        let triple = (
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        );
        parts.next().is_none().then_some((triple, pre))
    }
    let (a, b) = (key(a)?, key(b)?);
    match (a.0.cmp(&b.0), a.1, b.1) {
        (Ordering::Equal, Some(x), Some(y)) => (x == y).then_some(Ordering::Equal),
        (Ordering::Equal, Some(_), None) => Some(Ordering::Less),
        (Ordering::Equal, None, Some(_)) => Some(Ordering::Greater),
        (order, _, _) => Some(order),
    }
}

/// Where a `nika` file came from, read from its path alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Installer {
    /// The `install.sh` default, `~/.nika/bin/nika`.
    InstallSh,
    /// A Homebrew Cellar (or Caskroom) keg.
    Homebrew,
    /// A global npm package (`<prefix>/lib/node_modules`).
    NpmGlobal,
    /// An npm package inside a project's `node_modules`.
    NpmPayload,
    /// `cargo install` (the cargo home's `bin`).
    Cargo,
    /// A Nix store path.
    Nix,
    /// A release kept under `~/.nika/releases/<name>/`.
    ReleaseDir,
    /// A cargo `target/debug` or `target/release` directory.
    SourceBuild,
    /// No known layout.
    Unknown,
}

impl Installer {
    /// The machine token `--json` carries.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InstallSh => "install.sh",
            Self::Homebrew => "homebrew",
            Self::NpmGlobal => "npm-global",
            Self::NpmPayload => "npm-payload",
            Self::Cargo => "cargo",
            Self::Nix => "nix",
            Self::ReleaseDir => "release-dir",
            Self::SourceBuild => "source-build",
            Self::Unknown => "unknown",
        }
    }
}

/// The directories the installer reading compares against, each as given
/// and as resolved (a symlinked home must still match a resolved path).
#[derive(Debug, Clone, Default)]
pub(crate) struct Places {
    homes: Vec<PathBuf>,
    cargo_bins: Vec<PathBuf>,
}

impl Places {
    /// The home directory as given, when there is one.
    fn home(&self) -> Option<&Path> {
        self.homes.first().map(PathBuf::as_path)
    }

    pub(crate) fn new(home: Option<&Path>, cargo_home: Option<&Path>) -> Self {
        let both = |p: &Path| {
            let mut out = vec![p.to_path_buf()];
            if let Ok(resolved) = std::fs::canonicalize(p)
                && resolved != p
            {
                out.push(resolved);
            }
            out
        };
        let homes: Vec<PathBuf> = home.map(both).unwrap_or_default();
        let cargo_bins = match cargo_home {
            Some(cargo) => both(cargo),
            None => homes.iter().map(|h| h.join(".cargo")).collect(),
        }
        .into_iter()
        .map(|c| c.join("bin"))
        .collect();
        Self { homes, cargo_bins }
    }

    /// The installer of a `nika` found at `found` that resolves to `resolved`.
    pub(crate) fn installer(&self, found: &Path, resolved: &Path) -> Installer {
        let text = resolved.to_string_lossy();
        if text.contains("/Cellar/nika/") || text.contains("/Caskroom/nika/") {
            return Installer::Homebrew;
        }
        if text.starts_with("/nix/store/") {
            return Installer::Nix;
        }
        if text.contains("/lib/node_modules/") {
            return Installer::NpmGlobal;
        }
        if text.contains("/node_modules/") {
            return Installer::NpmPayload;
        }
        if self
            .homes
            .iter()
            .any(|h| resolved.starts_with(h.join(".nika").join("releases")))
        {
            return Installer::ReleaseDir;
        }
        let default = |h: &PathBuf| h.join(".nika").join("bin").join("nika");
        if self
            .homes
            .iter()
            .any(|h| found == default(h) || resolved == default(h))
        {
            return Installer::InstallSh;
        }
        let in_cargo_bin = |p: &Path| {
            p.parent()
                .is_some_and(|d| self.cargo_bins.iter().any(|b| b == d))
        };
        if in_cargo_bin(found) || in_cargo_bin(resolved) {
            return Installer::Cargo;
        }
        let parent = resolved.parent().and_then(Path::file_name);
        if matches!(parent.and_then(OsStr::to_str), Some("debug" | "release"))
            && resolved.components().any(|c| c.as_os_str() == "target")
        {
            return Installer::SourceBuild;
        }
        Installer::Unknown
    }
}

/// What one `nika` on PATH said when asked for its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer {
    /// It printed `nika <version>`.
    Reported(Build),
    /// It did not answer within [`VERSION_DEADLINE`] and was killed.
    TimedOut,
    /// It could not start, exited non-zero, or printed no version line.
    Failed(String),
    /// It is not executable: the shell skips it.
    NotExecutable,
    /// It sits in a relative PATH entry: listed, never executed here.
    Relative,
    /// It sits in a PATH entry spelled with a literal `~`: bash expands it
    /// when it looks a command up, zsh does not. Listed, never executed.
    LiteralTilde,
}

/// One distinct `nika` file on PATH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The path as PATH names it (`<entry>/nika`).
    pub(crate) path: PathBuf,
    /// Symlinks resolved, when the file resolves.
    pub(crate) resolved: Option<PathBuf>,
    pub(crate) installer: Installer,
    pub(crate) executable: bool,
    /// The same file as the running binary.
    pub(crate) running: bool,
    /// Found through a `~` or `~/…` PATH entry, expanded against the home.
    pub(crate) literal_tilde: bool,
    pub(crate) answer: Answer,
}

/// The running binary and every `nika` on PATH, observed once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinaryIdentity {
    /// The running binary's path, symlinks resolved, or why it could not be read.
    pub(crate) path: Result<PathBuf, String>,
    pub(crate) build: Build,
    pub(crate) installer: Installer,
    /// Every distinct `nika` file on PATH in PATH order, the running binary
    /// included (flagged): at most [`MAX_ENTRIES`], plus the running binary
    /// when it stands beyond them.
    pub(crate) entries: Vec<Entry>,
    /// More `nika` files sit on PATH than [`MAX_ENTRIES`]; the rest are not listed.
    pub(crate) truncated: bool,
}

impl BinaryIdentity {
    /// The entry typing `nika` runs: the first executable one.
    pub(crate) fn winner(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.executable)
    }

    /// Where the running binary sits on PATH, when it does.
    pub(crate) fn running_at(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.running)
    }

    /// Entry `i` stands in the running binary's way when `nika` is typed: it
    /// comes before it on PATH or, when a launcher started this binary from
    /// outside PATH, it is the copy that runs and does not report this build.
    pub(crate) fn shadows_running(&self, i: usize) -> bool {
        let Some(e) = self.entries.get(i).filter(|e| e.executable && !e.running) else {
            return false;
        };
        match self.running_at() {
            Some(r) => i < r,
            None => Some(i) == self.winner() && !self.reports_this_build(e),
        }
    }

    /// `e` answered with this binary's own build.
    pub(crate) fn reports_this_build(&self, e: &Entry) -> bool {
        matches!(&e.answer, Answer::Reported(build) if build.same_build(&self.build))
    }

    /// The running binary comes before entry `i` on PATH.
    pub(crate) fn running_shadows(&self, i: usize) -> bool {
        self.running_at().is_some_and(|r| r < i)
    }
}

/// The facts [`collect_from`] reads; the process supplies them in
/// [`collect`], tests supply their own.
#[derive(Debug, Clone)]
pub(crate) struct Inputs {
    pub(crate) path_var: Option<OsString>,
    pub(crate) running: Result<PathBuf, String>,
    pub(crate) build: Build,
    pub(crate) places: Places,
    pub(crate) deadline: Duration,
    /// The environment the probed copies' own is drawn from ([`ask::probe_env`]).
    pub(crate) env: Vec<(OsString, OsString)>,
}

/// Observe this process: its own executable, its PATH, its home.
#[must_use]
pub(crate) fn collect() -> BinaryIdentity {
    // PATH and CARGO_HOME are locations, not secrets: the raw reads are the
    // same sanctioned class as `probe::home_dir`.
    #[allow(clippy::disallowed_methods)]
    let (path_var, cargo_home) = (
        std::env::var_os("PATH"),
        std::env::var_os("CARGO_HOME").map(PathBuf::from),
    );
    let running = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| e.to_string());
    collect_from(Inputs {
        path_var,
        running,
        build: Build::current(),
        places: Places::new(crate::probe::home_dir().as_deref(), cargo_home.as_deref()),
        deadline: VERSION_DEADLINE,
        // Only the allow-listed, credential-free part ever reaches a copy.
        env: std::env::vars_os().collect(),
    })
}

/// Walk PATH, then ask every other executable `nika` with an absolute path
/// for its version, concurrently, each under `inputs.deadline`.
#[must_use]
pub(crate) fn collect_from(inputs: Inputs) -> BinaryIdentity {
    let running = inputs.running.as_ref().ok().map(PathBuf::as_path);
    let (mut entries, truncated) = scan(inputs.path_var.as_deref(), running, &inputs.places);
    let mut asked = Vec::new();
    for (i, entry) in entries.iter_mut().enumerate() {
        if entry.running {
            entry.answer = Answer::Reported(inputs.build.clone());
        } else if !entry.executable {
            entry.answer = Answer::NotExecutable;
        } else if entry.literal_tilde {
            entry.answer = Answer::LiteralTilde;
        } else if entry.path.is_relative() {
            entry.answer = Answer::Relative;
        } else {
            entry.answer = Answer::Failed("the version probe did not finish".to_owned());
            asked.push((i, entry.path.clone()));
        }
    }
    let env = ask::probe_env(&inputs.env, inputs.path_var.as_deref());
    for (i, answer) in ask::ask_all(asked, inputs.deadline, &env) {
        if let Some(entry) = entries.get_mut(i) {
            entry.answer = answer;
        }
    }
    let installer = running.map_or(Installer::Unknown, |p| inputs.places.installer(p, p));
    BinaryIdentity {
        path: inputs.running,
        build: inputs.build,
        installer,
        entries,
        truncated,
    }
}

/// Every distinct `nika` file on PATH, in order, and whether more stood
/// beyond [`MAX_ENTRIES`]. A directory, a dangling link or a missing file is
/// no command; two entries that resolve to one file are one entry (the
/// first wins, as in the shell). Past the cap only the running binary is
/// still looked for, so where it stands is never lost.
fn scan(path_var: Option<&OsStr>, running: Option<&Path>, places: &Places) -> (Vec<Entry>, bool) {
    let Some(path_var) = path_var else {
        return (Vec::new(), false);
    };
    let file = format!("nika{}", std::env::consts::EXE_SUFFIX);
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut truncated = false;
    for dir in std::env::split_paths(path_var) {
        let tilde = tilde_entry(&dir, places.home());
        let path = match &tilde {
            Some(expanded) => expanded.join(&file),
            // An empty PATH entry is the current directory, as in the shell.
            None if dir.as_os_str().is_empty() => PathBuf::from(&file),
            None => dir.join(&file),
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let resolved = std::fs::canonicalize(&path).ok();
        if !seen.insert(resolved.clone().unwrap_or_else(|| path.clone())) {
            continue;
        }
        let is_running = resolved.is_some() && resolved.as_deref() == running;
        if entries.len() >= MAX_ENTRIES && !is_running {
            truncated = true;
            continue;
        }
        let installer = places.installer(&path, resolved.as_deref().unwrap_or(&path));
        entries.push(Entry {
            running: is_running,
            executable: is_executable(&path),
            literal_tilde: tilde.is_some(),
            installer,
            resolved,
            path,
            answer: Answer::NotExecutable,
        });
    }
    (entries, truncated)
}

/// A PATH entry spelled `~` or `~/…`, expanded against `home` the way bash
/// does when it looks a command up. `None` for any other entry, or with
/// no home (the entry then reads as relative).
fn tilde_entry(dir: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let text = dir.to_str()?;
    let rest = if text == "~" {
        ""
    } else {
        text.strip_prefix("~/")?
    };
    Some(home?.join(rest))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    nix::unistd::access(path, nix::unistd::AccessFlags::X_OK).is_ok()
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests;
