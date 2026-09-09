// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#[cfg(unix)]
mod journal_grants;
use super::*;

fn apply(outcome: ShellAdapterOutcome) -> Result<(), nika_kernel::ShellError> {
    nika_kernel::ShellResult::new(126, r#"{"ok":true}"#, "", std::time::Duration::ZERO)
        .with_adapter_outcome(outcome)
        .into_process_result()
        .map(|_| ())
}

#[test]
fn terminal_outcome_table_keeps_authority_ahead_of_capture() {
    assert!(matches!(
        apply(classify_terminal_outcome(126, r#"{"ok":true}"#)),
        Err(nika_kernel::ShellError::Blocked { .. })
    ));
    assert!(matches!(
        apply(classify_terminal_outcome(
            1,
            "sandbox-exec: deny(1) file-read-data"
        )),
        Err(nika_kernel::ShellError::Blocked { .. })
    ));
    assert!(
        matches!(
            apply(classify_terminal_outcome(
                1,
                "cat: secret/key.txt: Operation not permitted\n"
            )),
            Err(nika_kernel::ShellError::Blocked { .. })
        ),
        "#1068: inner cat EPERM at status 1 is confinement, not structured data"
    );
    assert!(
        apply(classify_terminal_outcome(7, "business validation failed")).is_ok(),
        "an ordinary non-zero remains business data"
    );
    assert!(
        apply(classify_terminal_outcome(0, "Operation not permitted")).is_ok(),
        "status 0 is the inner process even if stderr mentions EPERM"
    );
    let named = apply(classify_terminal_outcome(
        2,
        "grep: ./build-status.txt: Operation not permitted\n",
    ));
    assert!(
        matches!(
            &named,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("`./build-status.txt`")
                    && reason.contains("was denied")
                    && !reason.contains("outside the read grants")
                    && !reason.contains("add it to permits.fs.read")
        ),
        "{named:?}"
    );
    let unnamed = apply(classify_terminal_outcome(126, r#"{"ok":true}"#));
    assert!(
        matches!(
            &unnamed,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("status 126")
                    && !reason.contains("read grants")
                    && !reason.contains("permits.fs.read")
        ),
        "{unnamed:?}"
    );
}

#[test]
fn refusal_reason_does_not_invent_a_grant() {
    let sock = apply(classify_terminal_outcome(
        1,
        "permission denied while trying to connect to the Docker daemon socket at unix:///Users/x/.docker/run/docker.sock\n",
    ));
    assert!(
        matches!(
            &sock,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("`/Users/x/.docker/run/docker.sock`")
                    && reason.contains("admits no socket")
                    && !reason.contains("permits.fs.read")
        ),
        "{sock:?}"
    );
    let mention = apply(classify_terminal_outcome(
        126,
        "config example unix:///tmp/docker.sock\n",
    ));
    assert!(
        matches!(
            &mention,
            Err(nika_kernel::ShellError::Blocked { reason })
                if !reason.contains("unix socket") && !reason.contains("permits.fs.read")
        ),
        "a unix:// URI without a denial is not a socket refuse: {mention:?}"
    );
    let write = apply(classify_terminal_outcome(
        1,
        "touch: ./out.txt: Operation not permitted\n",
    ));
    assert!(
        matches!(
            &write,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("`./out.txt`")
                    && reason.contains("was denied")
                    && !reason.contains("write grants")
                    && !reason.contains("permits.fs.read")
        ),
        "a program heuristic is not a write proof: {write:?}"
    );
    let cp = apply(classify_terminal_outcome(
        1,
        "cp: ./source: Permission denied\n",
    ));
    assert!(
        matches!(
            &cp,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("`./source`")
                    && !reason.contains("write grants")
                    && !reason.contains("add it to permits.fs.read")
        ),
        "cp source deny may be a read: {cp:?}"
    );
    let sbpl = apply(classify_terminal_outcome(
        1,
        "sandbox-exec: deny(1) file-read-data /tmp/x\n",
    ));
    assert!(
        matches!(
            &sbpl,
            Err(nika_kernel::ShellError::Blocked { reason })
                if reason.contains("read denial of `/tmp/x`")
                    && reason.contains("check `permits.fs.read`")
                    && !reason.contains("outside the read grants")
        ),
        "{sbpl:?}"
    );
}

/// The availability truth table — all four rows, platform-free.
/// Kills Gate 5's three survivors: `-> true` (row 4 fails), `->
/// false` (row 1 fails), `&& → ||` (rows 2+3 fail).
#[test]
fn available_given_is_the_and_of_both_facts() {
    assert!(available_given(true, true));
    assert!(
        !available_given(true, false),
        "macOS without the launcher is UNAVAILABLE"
    );
    assert!(
        !available_given(false, true),
        "a launcher path on non-macOS is UNAVAILABLE"
    );
    assert!(!available_given(false, false));
}

/// The binder reflects THIS platform's truth (belt over the seam).
#[test]
fn available_binder_matches_the_real_world() {
    let expected = cfg!(target_os = "macos") && Path::new(LAUNCHER).exists();
    assert_eq!(SeatbeltSandbox::available(), expected);
}

/// On a macOS host with the launcher, confine PROCEEDS — the wrapped
/// command execs the launcher, not the original program. Kills the
/// `delete !` mutant (which would return Unavailable exactly here).
#[test]
fn confine_proceeds_when_available() {
    if !SeatbeltSandbox::available() {
        return; // linux CI: the truth-table test carries the logic
    }
    let spec = SandboxSpec::default();
    let cmd = ShellCommand::new("/usr/bin/true");
    let wrapped = SeatbeltSandbox::new()
        .confine(&spec, cmd)
        .expect("available host confines");
    assert_eq!(wrapped.program, LAUNCHER);
}

#[test]
fn literal_prefix_extracts_the_directory_head() {
    assert_eq!(literal_prefix("/data/**"), "/data");
    assert_eq!(literal_prefix("/data/out/*.txt"), "/data/out");
    assert_eq!(literal_prefix("/data/x.txt"), "/data/x.txt");
    assert_eq!(literal_prefix("/data/lo*"), "/data");
    assert_eq!(literal_prefix("**/y"), "");
    assert_eq!(literal_prefix("./output/**"), "./output");
}

#[test]
fn sbpl_string_escapes_quote_and_backslash() {
    assert_eq!(sbpl_string("/a/b").unwrap(), "\"/a/b\"");
    assert_eq!(sbpl_string("/a\"b").unwrap(), "\"/a\\\"b\"");
    assert_eq!(sbpl_string("/a\\b").unwrap(), "\"/a\\\\b\"");
}

#[test]
fn sbpl_string_refuses_a_control_char_path() {
    assert!(matches!(
        sbpl_string("/a\nb"),
        Err(CommandSandboxError::Profile { .. })
    ));
    assert!(matches!(
        sbpl_string("/a\0b"),
        Err(CommandSandboxError::Profile { .. })
    ));
}

#[test]
fn profile_denies_network_by_default_and_allows_when_granted() {
    let denied = build_profile(&SandboxSpec::new(), None).unwrap();
    assert!(denied.contains("(deny default)"));
    assert!(
        !denied.contains("(allow network*)"),
        "no unrestricted network by default"
    );
    assert!(
        denied.contains("(allow network-outbound (remote ip \"localhost:*\"))"),
        "the deny arm admits loopback outbound only (the srt posture)"
    );

    let mut allow = SandboxSpec::new();
    allow.net = NetPolicy::Allow;
    assert!(
        build_profile(&allow, None)
            .unwrap()
            .contains("(allow network*)")
    );
}

#[test]
fn allowlist_fences_outbound_loopback_to_the_proxy_port() {
    // The srt seatbelt line: the channel is fenced to the proxy's port;
    // the proxy (not the profile) fences the hosts — a Seatbelt host
    // rule is TLS-blind.
    let mut spec = SandboxSpec::new();
    let mut allowlist =
        nika_kernel::process::EgressAllowlist::new(vec!["api.example.com".to_owned()]);
    allowlist.proxy_port = Some(60080);
    spec.net = NetPolicy::Allowlist(allowlist);
    let p = build_profile(&spec, None).unwrap();
    assert!(
        p.contains("(allow network-outbound (remote ip \"localhost:60080\"))"),
        "port-scoped fence: {p}"
    );
    assert!(!p.contains("(allow network*)"), "never unrestricted");

    // A spec that never passed the runner (no proxy yet) degrades to
    // loopback-any — fail-closed, the allowlist simply cannot be served.
    let mut spec = SandboxSpec::new();
    spec.net = NetPolicy::Allowlist(nika_kernel::process::EgressAllowlist::new(vec![
        "api.example.com".to_owned(),
    ]));
    let p = build_profile(&spec, None).unwrap();
    assert!(p.contains("(allow network-outbound (remote ip \"localhost:*\"))"));
}

#[test]
fn profile_emits_declared_reads_and_writes() {
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec!["/data/in/**".to_owned()];
    spec.fs_write = vec!["/data/out/**".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    assert!(p.contains("(allow file-read* (subpath \"/data/in\"))"));
    assert!(p.contains("(allow file-write* file-read* (subpath \"/data/out\"))"));
}

/// The `SQLite` durability family (the 2026-07-29 finding, closed): an
/// EXACT-file write grant carries `-wal` / `-shm` / `-journal` as exact
/// literals on its own rule — without them a confined WAL open dies with
/// `SQLITE_CANTOPEN` (14).
#[test]
fn an_exact_file_write_grant_carries_its_journal_sidecars() {
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/data/state.db".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    assert!(
        p.contains(
            "(allow file-write* file-read* (subpath \"/data/state.db\") \
             (literal \"/data/state.db-wal\") (literal \"/data/state.db-shm\") \
             (literal \"/data/state.db-journal\"))"
        ),
        "the durability family rides the file's own rule: {p}"
    );
}

/// The read side inherits the family as READ-ONLY literals — the access
/// class follows the grant, never widened by the sidecars.
#[test]
fn an_exact_file_read_grant_carries_read_only_sidecars() {
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec!["/data/state.db".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    assert!(
        p.contains(
            "(allow file-read* (subpath \"/data/state.db\") \
             (literal \"/data/state.db-wal\") (literal \"/data/state.db-shm\") \
             (literal \"/data/state.db-journal\"))"
        ),
        "read-only sidecars on the read rule: {p}"
    );
    assert!(
        !p.contains("file-write* file-read* (subpath \"/data/state.db\""),
        "the sidecars never smuggle a write into a read grant: {p}"
    );
}

/// A directory-shaped grant (a `**` glob · a trailing-slash path) already
/// covers same-dir sidecars — NO literal is added (no profile bloat, and
/// the exact-file extension stays the only same-directory reach).
#[test]
fn directory_grants_add_no_sidecar_literals() {
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec!["/data/in/**".to_owned()];
    spec.fs_write = vec!["/data/out/".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    assert!(
        !p.contains("-wal") && !p.contains("-journal"),
        "directory grants already cover their sidecars: {p}"
    );
}

/// The sidecar literals cross the same injection boundary as any path:
/// a quote-bearing base is emitted ESCAPED, suffix included — the three
/// literals stay inert string content, never live directives.
#[test]
fn sidecar_literals_are_escaped_like_any_path() {
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/data/x\"y.db".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    let escaped_wal = sbpl_string("/data/x\"y.db-wal").unwrap();
    assert!(
        p.contains(&format!("(literal {escaped_wal})")),
        "the sidecar literal is one escaped string: {p}"
    );
}

/// The getcwd walk (the finding's second half): the child's OWN cwd is
/// listed as a `file-read-data` literal — a directory LISTING, never file
/// contents — so a relative open in the child stops dying on the walk.
#[test]
fn the_child_cwd_is_listed_as_read_data_only() {
    let p = build_profile(&SandboxSpec::new(), Some(Path::new("/data/project"))).unwrap();
    assert!(
        p.contains("(allow file-read-data (literal \"/data/project\"))"),
        "the cwd listing is emitted: {p}"
    );
    assert!(
        !p.contains("file-read* (subpath \"/data/project\")")
            && !p.contains("file-read* file-read-metadata\n    (subpath \"/data/project\")"),
        "the listing never widens to contents: {p}"
    );
    // No cwd → no listing rule at all (the profile stays minimal).
    let p = build_profile(&SandboxSpec::new(), None).unwrap();
    assert!(
        !p.contains("(allow file-read-data"),
        "no cwd, no listing: {p}"
    );
    // A root cwd is already the preamble's `(literal "/")` — not re-emitted.
    let p = build_profile(&SandboxSpec::new(), Some(Path::new("/"))).unwrap();
    assert!(
        !p.contains("(allow file-read-data"),
        "the root listing is the preamble's own: {p}"
    );
}

/// An exact-file grant lists its PARENT (names only — same-directory
/// tooling scans the opened file's home); a directory grant adds nothing
/// (its subpath already covers the listing). Two files in one directory
/// emit the parent ONCE.
#[test]
fn an_exact_file_grant_lists_its_parent_directory_once() {
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/data/state.db".to_owned(), "/data/other.db".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    let needle = "(literal \"/data\")";
    assert_eq!(
        p.matches(needle).count(),
        1,
        "the shared parent is listed exactly once: {p}"
    );
    assert!(
        p.contains("(allow file-read-data (literal \"/data\"))"),
        "as read-data only, never contents: {p}"
    );

    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/data/out/**".to_owned()];
    let p = build_profile(&spec, None).unwrap();
    assert!(
        !p.contains("(allow file-read-data"),
        "a directory grant's subpath already covers its listing: {p}"
    );
}

/// The confined child runs in the command's own `cwd` when set, else
/// inherits the runner's — the listing follows the SAME directory the
/// child's relative opens will resolve against.
#[test]
fn confined_cwd_prefers_the_command_then_the_runner() {
    let mut cmd = ShellCommand::new("/usr/bin/true");
    cmd.cwd = Some(PathBuf::from("/data/project"));
    assert_eq!(confined_cwd(&cmd), Some(PathBuf::from("/data/project")));
    let cmd = ShellCommand::new("/usr/bin/true");
    assert_eq!(
        confined_cwd(&cmd),
        std::env::current_dir().ok(),
        "a None cwd inherits the runner's own directory"
    );
}

#[test]
fn grant_subpath_refuses_an_over_granting_or_non_canonical_permit() {
    // The floor must make a whole-filesystem / system-root / non-canonical
    // grant IMPOSSIBLE to express — even from a hostile or wrong permit
    // (review P1-1/P1-2/P2-1). Each of these is fail-closed (Profile error).
    for over in [
        "/",                  // whole filesystem (P1-1)
        "//",                 // root, trailing-slash form
        "/etc/passwd*",       // trims to the /etc system root (P2-1)
        "/Users/*",           // trims to /Users (every home)
        "/usr/**",            // bare system root
        "./output/**",        // relative — SBPL can't canonicalize (P1-2)
        "../shared/**",       // parent traversal
        "~/.aws/**",          // ~ is not expanded by SBPL (P3-1)
        "$HOME/secrets/**",   // $VAR is not expanded by SBPL
        "/data/../../etc/x*", // a `..` escape
    ] {
        assert!(
            matches!(
                grant_subpath(over),
                Err(CommandSandboxError::Profile { .. })
            ),
            "permit {over:?} must be refused (fail-closed), not granted"
        );
    }
}

#[test]
fn grant_subpath_allows_a_specific_absolute_permit() {
    // A genuinely-scoped permit (≥2 absolute segments, not a system root)
    // is granted as its directory subpath.
    assert_eq!(
        grant_subpath("/data/project/in/**").unwrap(),
        Some("/data/project/in".to_owned())
    );
    assert_eq!(
        grant_subpath("/srv/app/cache/x.txt").unwrap(),
        Some("/srv/app/cache/x.txt".to_owned())
    );
    // A glob with no literal prefix is SKIPPED (safe · no grant emitted),
    // not an error — `**/y` and root-level globs like `/*` `/**`.
    assert_eq!(grant_subpath("**/y").unwrap(), None);
    assert_eq!(grant_subpath("/*").unwrap(), None);
    assert_eq!(grant_subpath("/**").unwrap(), None);
}

/// Issue 754 — the blanket `(allow file-read* file-write* (subpath
/// "/private/tmp"))` family bypassed every declared `permits.fs`
/// boundary. The empty spec's profile must not name the shared tmp
/// trees at all; a DECLARED grant re-admits exactly its own subpath.
#[test]
fn the_shared_host_tmp_is_no_ambient_grant() {
    let p = build_profile(&SandboxSpec::new(), None).expect("profile");
    for tree in ["/private/tmp", "/private/var/tmp", "/private/var/folders"] {
        assert!(
            !p.contains(tree),
            "the empty profile must not grant {tree}:\n{p}"
        );
    }
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/private/tmp/nika-x/**".to_owned()];
    let p = build_profile(&spec, None).expect("profile");
    assert!(
        p.contains("(subpath \"/private/tmp/nika-x\")"),
        "a declared tmp subpath is granted exactly:\n{p}"
    );
}

/// The interpreter's home is PROGRAM space, read-only, on both Mac
/// architectures. `(subpath "/usr")` already covers `/usr/local` (the
/// Intel Homebrew prefix); `/opt/homebrew` (the Apple-Silicon prefix)
/// was not in the preamble, so a Homebrew `bash`/`node`/`python3` first
/// on PATH died at dyld (`Library not loaded … libreadline.8.dylib ·
/// file system sandbox blocked open()`) under any `permits:` block —
/// measured 2026-08-18: 12 daily runs of the studio's own ledger rendered
/// 36 ✔ legs whose captured `exit_code` was -1, and the same leg exits 0
/// once the prefix is readable. Read only · never a write · never a
/// declared-grant substitute (the child still cannot read the workspace
/// without `fs.read`).
#[test]
fn the_apple_silicon_homebrew_prefix_is_program_space_like_usr_local() {
    let p = build_profile(&SandboxSpec::new(), None).expect("profile");
    let preamble_reads = p
        .split("(allow file-write-data")
        .next()
        .expect("the read block precedes the write block");
    assert!(
        preamble_reads.contains("(subpath \"/opt/homebrew\")"),
        "the Apple-Silicon Homebrew prefix is readable program space:\n{p}"
    );
    assert!(
        preamble_reads.contains("(subpath \"/usr\")"),
        "the Intel prefix (/usr/local) stays covered by /usr:\n{p}"
    );
    assert!(
        !p.contains("file-write* file-read* (subpath \"/opt/homebrew\")"),
        "program space is never writable:\n{p}"
    );
}

/// Live, macOS-only, and honest about its own applicability: if a
/// Homebrew bash is installed at the Apple-Silicon prefix and the
/// launcher exists, a confined `bash -c true` must START (exit 0). Where
/// either is absent the test says so and passes vacuously — a skip that
/// names itself, never a green that looked at nothing.
#[test]
#[cfg(target_os = "macos")]
// The launcher IS the seam under test: this test spawns `sandbox-exec`
// itself to prove the profile lets a real interpreter start · the
// kernel `ShellExecutor` seam sits ABOVE this crate and would hide the
// very thing being measured (the nika-cli-host git.rs precedent).
#[allow(clippy::disallowed_types)]
fn a_homebrew_interpreter_starts_under_the_seatbelt() {
    let brew_bash = Path::new("/opt/homebrew/bin/bash");
    if !brew_bash.exists() || !SeatbeltSandbox::available() {
        // a skip that names itself · never a green that looked at nothing
        return;
    }
    let profile = build_profile(&SandboxSpec::new(), None).expect("profile");
    let status = std::process::Command::new(LAUNCHER)
        .arg("-p")
        .arg(&profile)
        .arg("--")
        .arg(brew_bash)
        .arg("-c")
        .arg("true")
        .status()
        .expect("the launcher spawns");
    assert!(
        status.success(),
        "a Homebrew bash must start under the empty profile · status {status}"
    );
}

#[test]
fn a_root_write_permit_fails_the_whole_profile() {
    // The end-to-end fail-closed: a spec asking to write `/` does not yield
    // a permissive profile — build_profile refuses it.
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec!["/".to_owned()];
    assert!(matches!(
        build_profile(&spec, None),
        Err(CommandSandboxError::Profile { .. })
    ));
}

#[test]
fn profile_injection_via_a_malicious_path_is_refused() {
    // Two independent boundaries defend the profile against a path crafted
    // to inject an SBPL directive (no glob metachar in these payloads, so
    // `literal_prefix` returns them whole and the escaping/refusal runs).

    // (1) A newline cannot be a valid path char in an SBPL string and is
    //     refused at the control-char boundary → Profile error.
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec!["/data\n(allow system-socket)".to_owned()];
    assert!(matches!(
        build_profile(&spec, None),
        Err(CommandSandboxError::Profile { .. })
    ));

    // (2) A quote break-out is ESCAPED — the whole payload is emitted as
    //     escaped string content inside ONE `(subpath "<escaped>")` filter
    //     (plus, this being an exact-file grant, its three escaped journal
    //     sidecars), so the injected `(allow system-socket)` is inert
    //     string content, not a top-level form. Proven by reconstructing
    //     the escaped literals: the profile must contain precisely them
    //     (their internal quotes are `\"`).
    let payload = "/data\") (allow system-socket) (subpath \"/etc";
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec![payload.to_owned()];
    let p = build_profile(&spec, None).unwrap();
    let escaped = sbpl_string(payload).unwrap();
    assert!(
        escaped.contains("\\\""),
        "the payload's quotes are escaped: {escaped}"
    );
    let escaped_wal = sbpl_string(&format!("{payload}-wal")).unwrap();
    assert!(
        p.contains(&format!(
            "(allow file-read* (subpath {escaped}) (literal {escaped_wal})"
        )),
        "the payload and its sidecars are escaped string content, not live directives: {p}"
    );
}

#[test]
fn wrap_argv_form_runs_program_directly() {
    let cmd = ShellCommand::new("cat").arg("/data/x");
    let w = wrap(cmd, "(version 1)(deny default)");
    assert_eq!(w.program, LAUNCHER);
    assert!(!w.shell);
    assert!(w.pre_validated, "the launcher argv must not be re-scanned");
    assert!(w.sandbox.is_none(), "already confined");
    assert_eq!(w.args[0], "-p");
    assert_eq!(w.args[2], "--");
    assert_eq!(w.args[3], "cat");
    assert_eq!(w.args[4], "/data/x");
}

#[test]
fn wrap_shell_form_reconstructs_sh_dash_c() {
    let mut cmd = ShellCommand::new("echo hi | wc -l");
    cmd.shell = true;
    let w = wrap(cmd, "(version 1)");
    assert_eq!(w.args[3], "/bin/sh");
    assert_eq!(w.args[4], "-c");
    assert_eq!(w.args[5], "echo hi | wc -l");
    assert!(!w.shell, "the OUTER command runs the launcher directly");
}

#[test]
fn backend_name_is_stable() {
    assert_eq!(SeatbeltSandbox::new().backend(), "seatbelt");
}

// Security fixtures propagate setup and profile failures to the test harness.
type FixtureResult<T = ()> = Result<T, String>;

fn fixture<T, E: std::fmt::Debug>(context: &str, result: Result<T, E>) -> FixtureResult<T> {
    result.map_err(|error| format!("{context}: {error:?}"))
}

fn canon(p: &Path) -> FixtureResult<String> {
    let canonical = fixture("canonicalize fixture path", std::fs::canonicalize(p))?;
    fixture(
        "fixture path must be UTF-8",
        canonical.into_os_string().into_string(),
    )
}

fn scratch(tag: &str) -> FixtureResult<PathBuf> {
    let d = std::env::temp_dir().join(format!("nika-sb-kr-{tag}-{}", std::process::id()));
    fixture("create fixture root", std::fs::create_dir_all(&d))?;
    Ok(d)
}

/// KR-02 · an alias whose canonical form is a protected root must
/// refuse exactly like the literal root — profile GENERATION only,
/// zero writes near any system path (the alias lives in scratch).
#[cfg(target_os = "macos")]
#[test]
fn a_canonical_alias_of_a_protected_root_is_refused() -> FixtureResult {
    let s = scratch("alias")?;
    fixture(
        "create fixture symlink",
        std::os::unix::fs::symlink("/private", s.join("alias")),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/etc/**", s.join("alias").display())];
    assert!(
        build_profile(&spec, None).is_err(),
        "alias → /private then /etc/** must refuse like bare /etc"
    );
    let mut direct = SandboxSpec::new();
    direct.fs_read = vec!["/private/etc/**".to_owned()];
    assert!(
        build_profile(&direct, None).is_err(),
        "the direct canonical spelling of a protected root refuses too"
    );
    let mut tmp = SandboxSpec::new();
    tmp.fs_read = vec!["/tmp/**".to_owned()];
    assert!(
        build_profile(&tmp, None).is_err(),
        "the lexical bare-/tmp refusal is preserved"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;
    Ok(())
}

/// KR-03 · a DIRECTORY grant (trailing slash · terminal `/.` ·
/// metachar) emits NEITHER the three journal siblings (they live
/// outside the subtree) NOR the parent listing — while an exact
/// FILE keeps its whole family on the canonical spelling.
#[cfg(unix)]
#[test]
fn directory_grants_emit_no_sidecars_exact_files_keep_theirs() -> FixtureResult {
    let s = scratch("sidecar")?;
    fixture(
        "create fixture directory",
        std::fs::create_dir_all(s.join("data")),
    )?;
    let s_canon = canon(&s)?;
    for grant in [
        format!("{}/", s.join("data").display()),
        format!("{}.", s.join("data").display().to_string() + "/"),
        format!("{}/**", s.join("data").display()),
    ] {
        let mut spec = SandboxSpec::new();
        spec.fs_write = vec![grant.clone()];
        let p = build_profile(&spec, None);
        assert!(p.is_ok(), "a directory grant builds: {grant}");
        let p = fixture("profile must build", p)?;
        assert!(
            !p.contains("-wal") && !p.contains("-shm") && !p.contains("-journal"),
            "{grant}: no sibling sidecars outside the subtree: {p}"
        );
        assert!(
            !p.contains(&format!("(literal \"{s_canon}\"")),
            "{grant}: no parent listing either: {p}"
        );
    }
    // the exact file keeps all three suffixes + the parent listing
    fixture(
        "write fixture file",
        std::fs::write(s.join("state.db"), b""),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}", s.join("state.db").display())];
    let p = fixture("profile must build", build_profile(&spec, None))?;
    let db = canon(&s.join("state.db"))?;
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            p.contains(&format!("(literal \"{db}{suffix}\"")),
            "the exact file keeps its {suffix} sibling: {p}"
        );
    }
    assert!(
        p.contains(&format!("(literal \"{s_canon}\"")),
        "the exact file's parent listing stays: {p}"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;
    Ok(())
}

/// KR-04 · resolution failures are NOT absence: symlink loops,
/// dangling ancestors and not-a-directory components refuse
/// through the fallible profile boundary, while a genuinely-absent
/// tail folds lexically (the legal new-write).
#[cfg(unix)]
#[test]
fn resolution_errors_refuse_clean_absence_still_folds() -> FixtureResult {
    // loop a <-> b
    let s = scratch("loop")?;
    fixture(
        "create fixture symlink",
        std::os::unix::fs::symlink(s.join("b"), s.join("a")),
    )?;
    fixture(
        "create fixture symlink",
        std::os::unix::fs::symlink(s.join("a"), s.join("b")),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/leaf/**", s.join("a").display())];
    assert!(
        build_profile(&spec, None).is_err(),
        "a symlink loop must refuse (ELOOP is not absence)"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;

    // dangling ancestor link
    let s = scratch("dangling")?;
    fixture(
        "create fixture symlink",
        std::os::unix::fs::symlink(s.join("missing"), s.join("dangling")),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/leaf/**", s.join("dangling").display())];
    assert!(
        build_profile(&spec, None).is_err(),
        "a dangling ancestor must refuse (the link EXISTS)"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;

    // a regular file used as a directory component
    let s = scratch("notdir")?;
    fixture("write fixture file", std::fs::write(s.join("afile"), b""))?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/leaf/**", s.join("afile").display())];
    assert!(
        build_profile(&spec, None).is_err(),
        "a not-a-directory component must refuse (ENOTDIR is not absence)"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;

    // the legal case: a clean-absent tail under a healthy ancestor
    let s = scratch("newwrite")?;
    let grant = format!("{}/fresh/out/**", s.display());
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![grant.clone()];
    let p = build_profile(&spec, None);
    assert!(p.is_ok(), "a clean-absent suffix still folds: {grant}");
    let expected = format!("{}/fresh/out", canon(&s)?);
    let p = fixture("profile must build", p)?;
    assert!(
        p.contains(&format!("(subpath \"{expected}\"")),
        "the folded effective suffix is emitted: {p}"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;
    Ok(())
}

#[test]
fn regular_file_ancestors_refuse_both_access_classes() -> FixtureResult {
    let s = scratch("file-ancestor")?;
    fixture("write fixture file", std::fs::write(s.join("afile"), b""))?;
    for write in [false, true] {
        for suffix in ["leaf", "leaf/**", "missing/leaf/**"] {
            let grant = format!("{}/afile/{suffix}", s.display());
            let mut spec = SandboxSpec::new();
            if write {
                spec.fs_write = vec![grant.clone()];
            } else {
                spec.fs_read = vec![grant.clone()];
            }
            assert!(
                matches!(
                    build_profile(&spec, None),
                    Err(CommandSandboxError::Profile { .. })
                ),
                "file ancestor must refuse: write={write}, {grant}"
            );
        }
    }
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;
    Ok(())
}

/// The macOS /tmp-alias positive (KIMI-SEC-02's fixed case): a
/// grant through the system `/tmp` symlink is spelled canonically,
/// and the lexical spelling leaves the rules entirely.
#[cfg(target_os = "macos")]
#[test]
fn a_tmp_alias_grant_is_spelled_canonically() -> FixtureResult {
    let dir = PathBuf::from("/tmp").join(format!("nika-sb-kr-tmp-{}", std::process::id()));
    fixture(
        "create fixture directory",
        std::fs::create_dir_all(dir.join("arena")),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/**", dir.join("arena").display())];
    let p = fixture("profile must build", build_profile(&spec, None))?;
    let want = canon(&dir.join("arena"))?;
    assert!(
        p.contains(&format!("(subpath \"{want}\"")),
        "the canonical spelling is emitted: {p}"
    );
    assert!(
        !p.contains(&format!("(subpath \"{}\"", dir.join("arena").display())),
        "the lexical alias spelling is gone: {p}"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&dir))?;
    Ok(())
}

/// The final-pivot invariant (CVE-2024-42472 class): a symlink AT
/// the grant root keeps its own name in the rule — the rule never
/// spells the pivot's target.
#[cfg(unix)]
#[test]
fn a_symlink_at_the_grant_root_keeps_its_own_name() -> FixtureResult {
    let s = scratch("pivot")?;
    fixture(
        "create fixture directory",
        std::fs::create_dir_all(s.join("real")),
    )?;
    fixture(
        "create fixture symlink",
        std::os::unix::fs::symlink(s.join("real"), s.join("pivot")),
    )?;
    let mut spec = SandboxSpec::new();
    spec.fs_write = vec![format!("{}/**", s.join("pivot").display())];
    let p = fixture("profile must build", build_profile(&spec, None))?;
    let target = canon(&s.join("real"))?;
    assert!(
        !p.contains(&format!("(subpath \"{target}\"")),
        "the rule never spells the pivot's target: {p}"
    );
    fixture("remove fixture tree", std::fs::remove_dir_all(&s))?;
    Ok(())
}
