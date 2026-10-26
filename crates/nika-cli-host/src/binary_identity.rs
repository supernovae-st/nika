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
//! Two limits are deliberate. A relative PATH entry is listed but never
//! executed: looking at a project directory must not run its code. The
//! installer is read from the path alone (a Homebrew Cellar, an npm
//! package, a cargo bin directory, `~/.nika/releases`, the `install.sh`
//! default, a Nix store, a cargo `target` directory); any other layout is
//! `unknown`, never a guess.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod render;

pub(crate) use render::{doctor_findings, insert_json, welcome_lines, with_json};

/// How long one `--version` answer may take before the copy is reported
/// as not answering (and killed).
pub(crate) const VERSION_DEADLINE: Duration = Duration::from_secs(2);

/// The most stdout read from one `--version` answer: a version is one line.
const VERSION_OUTPUT_CAP: u64 = 4096;

/// The most distinct `nika` files examined on PATH.
const MAX_ENTRIES: usize = 16;

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
/// with an optional `(<commit>)`. Anything else is not a nika answer.
pub(crate) fn parse_version_output(stdout: &str) -> Option<Build> {
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut words = line.split_whitespace();
    if words.next()? != "nika" {
        return None;
    }
    let version = words.next()?.trim_start_matches('v');
    if !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let commit = words
        .next()
        .and_then(|w| w.strip_prefix('(')?.strip_suffix(')'))
        .filter(|c| !c.is_empty())
        .map(str::to_owned);
    Some(Build {
        version: version.to_owned(),
        commit,
    })
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
    /// included (flagged), at most [`MAX_ENTRIES`].
    pub(crate) entries: Vec<Entry>,
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

    /// Entry `i` comes before the running binary when `nika` is typed (or the
    /// running binary is not on PATH at all).
    pub(crate) fn shadows_running(&self, i: usize) -> bool {
        self.entries
            .get(i)
            .is_some_and(|e| e.executable && !e.running)
            && self.running_at().is_none_or(|r| i < r)
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
    })
}

/// Walk PATH, then ask every other executable `nika` with an absolute path
/// for its version, concurrently, each under `inputs.deadline`.
#[must_use]
pub(crate) fn collect_from(inputs: Inputs) -> BinaryIdentity {
    let running = inputs.running.as_ref().ok().map(PathBuf::as_path);
    let mut entries = scan(inputs.path_var.as_deref(), running, &inputs.places);
    let mut asked = Vec::new();
    for (i, entry) in entries.iter_mut().enumerate() {
        if entry.running {
            entry.answer = Answer::Reported(inputs.build.clone());
        } else if !entry.executable {
            entry.answer = Answer::NotExecutable;
        } else if entry.path.is_relative() {
            entry.answer = Answer::Relative;
        } else {
            entry.answer = Answer::Failed("the version probe did not finish".to_owned());
            asked.push((i, entry.path.clone()));
        }
    }
    for (i, answer) in ask_all(asked, inputs.deadline) {
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
    }
}

/// Every distinct `nika` file on PATH, in order. A directory, a dangling
/// link or a missing file is no command; two entries that resolve to one
/// file are one entry (the first wins, as in the shell).
fn scan(path_var: Option<&OsStr>, running: Option<&Path>, places: &Places) -> Vec<Entry> {
    let Some(path_var) = path_var else {
        return Vec::new();
    };
    let file = format!("nika{}", std::env::consts::EXE_SUFFIX);
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for dir in std::env::split_paths(path_var) {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        // An empty PATH entry is the current directory, as in the shell.
        let path = if dir.as_os_str().is_empty() {
            PathBuf::from(&file)
        } else {
            dir.join(&file)
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
        let installer = places.installer(&path, resolved.as_deref().unwrap_or(&path));
        entries.push(Entry {
            running: resolved.is_some() && resolved.as_deref() == running,
            executable: is_executable(&path),
            installer,
            resolved,
            path,
            answer: Answer::NotExecutable,
        });
    }
    entries
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    nix::unistd::access(path, nix::unistd::AccessFlags::X_OK).is_ok()
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

/// Ask each `(index, path)` for its version, concurrently, on a private
/// runtime. A caller already inside a runtime cannot block on one: every
/// copy is then reported as not asked, never a panic.
fn ask_all(asked: Vec<(usize, PathBuf)>, deadline: Duration) -> Vec<(usize, Answer)> {
    if asked.is_empty() {
        return Vec::new();
    }
    let refuse = |asked: Vec<(usize, PathBuf)>, why: &str| -> Vec<(usize, Answer)> {
        asked
            .into_iter()
            .map(|(i, _)| (i, Answer::Failed(why.to_owned())))
            .collect()
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        return refuse(asked, "not asked from inside an async runtime");
    }
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return refuse(asked, "the probe runtime did not start");
    };
    runtime.block_on(async move {
        let mut set = tokio::task::JoinSet::new();
        for (i, path) in asked {
            set.spawn(async move { (i, ask_version(&path, deadline).await) });
        }
        let mut answers = Vec::new();
        while let Some(joined) = set.join_next().await {
            answers.push(joined.unwrap_or_else(|e| (usize::MAX, Answer::Failed(e.to_string()))));
        }
        answers
    })
}

/// `<path> --version`, bounded: stdin closed, stderr discarded, at most
/// [`VERSION_OUTPUT_CAP`] bytes of stdout read, the whole exchange within
/// `deadline`. A copy still running at the deadline is killed on drop.
///
/// CANCEL SAFETY: dropping this future kills the child (`kill_on_drop`).
async fn ask_version(path: &Path, deadline: Duration) -> Answer {
    use tokio::io::AsyncReadExt as _;
    let spawned = tokio::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return Answer::Failed(format!("could not start: {e}")),
    };
    let Some(stdout) = child.stdout.take() else {
        return Answer::Failed("no stdout".to_owned());
    };
    let exchange = async {
        let mut bytes = Vec::new();
        let mut limited = stdout.take(VERSION_OUTPUT_CAP);
        limited.read_to_end(&mut bytes).await?;
        // Closing the pipe ends a copy that keeps writing past the cap.
        drop(limited);
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, bytes))
    };
    match tokio::time::timeout(deadline, exchange).await {
        Err(_) => Answer::TimedOut,
        Ok(Err(e)) => Answer::Failed(e.to_string()),
        Ok(Ok((status, _))) if !status.success() => Answer::Failed(status.to_string()),
        Ok(Ok((_, bytes))) => parse_version_output(&String::from_utf8_lossy(&bytes)).map_or_else(
            || Answer::Failed("printed no `nika <version>` line".to_owned()),
            Answer::Reported,
        ),
    }
}

#[cfg(test)]
mod tests;
