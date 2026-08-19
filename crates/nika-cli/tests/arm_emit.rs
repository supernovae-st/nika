// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This suite's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// arm_fire.rs / ascii_contract.rs / bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! `nika arm --emit` end-to-end (W3 · « LE PONT », the OS bridge): a
//! tempdir project, the real binary, HOME redirected into the tempdir
//! (the write tests never touch a developer's `LaunchAgents`), and the
//! machine zone read through the same jiff call the verb makes.
//!
//! The pinned laws: print by default, nothing written without
//! `--write` · the units land where the OS reads them · D7 (an env file
//! rides by PATH, a secret VALUE never crosses) · D2 (every unit calls
//! `arm fire`, never `run`) · D10 (a foreign zone refuses on launchd,
//! with teaching, and rides systemd's `OnCalendar=`).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// The machine's IANA zone — the same read the verb does. A zoneless
/// machine cannot run this suite (the verb refuses it too, by design).
fn machine_zone() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .expect("cette machine n'a pas de fuseau IANA nommé — le verbe le refuserait aussi")
        .to_owned()
}

/// A tempdir project: the registry (the workflows never run — emit
/// renders, it never fires).
fn project(tag: &str, registry: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-arm-emit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("project dir");
    let mut f = std::fs::File::create(dir.join("nika.yaml")).expect("registry file");
    f.write_all(registry.as_bytes()).expect("registry body");
    dir
}

/// A redirected HOME inside the tempdir.
fn home(dir: &Path) -> PathBuf {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).expect("home dir");
    home
}

/// A one-beat registry in the machine's zone.
fn registry_in(zone: &str) -> String {
    format!(
        "nika: v1\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ={zone} 0 3 * * *\"\n    plafond: 0.05\n    manqué: sauter\n"
    )
}

/// Run `nika-cli arm …` inside the project with HOME redirected.
fn arm(dir: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    bin()
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .output()
        .expect("spawn arm")
}

#[test]
fn emit_prints_units_and_never_writes_without_write() {
    let zone = machine_zone();
    let dir = project("print", &registry_in(&zone));
    let home = home(&dir);
    let out = arm(&dir, &home, &["arm", "--emit", "launchd"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("# ── nika.arm.doctor.plist"),
        "le séparateur: {stdout}"
    );
    assert!(
        stdout.contains("<string>nika.arm.doctor</string>"),
        "le Label: {stdout}"
    );
    assert!(
        stdout.contains("launchctl bootstrap gui/$UID"),
        "la commande de charge: {stdout}"
    );
    // Print mode writes NOTHING (the local gesture is --write's).
    assert!(
        !home.join("Library").exists(),
        "sans --write, rien ne s'écrit"
    );
    assert!(!dir.join(".nika/arm/logs").exists(), "ni les logs");
}

#[test]
fn emit_write_puts_files_where_the_os_reads_them() {
    let zone = machine_zone();
    let dir = project("write", &registry_in(&zone));
    let home = home(&dir);

    // launchd → ~/Library/LaunchAgents/
    let out = arm(&dir, &home, &["arm", "--emit", "launchd", "--write"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plist = home.join("Library/LaunchAgents/nika.arm.doctor.plist");
    assert!(plist.exists(), "launchd lit là: {}", plist.display());
    let body = std::fs::read_to_string(&plist).expect("plist body");
    assert!(body.contains("<string>nika.arm.doctor</string>"), "{body}");
    assert!(body.contains("<key>StartCalendarInterval</key>"), "{body}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("launchctl bootstrap gui/$UID {}", plist.display())),
        "la commande porte le chemin réel: {stdout}"
    );
    // The log dir the units name must exist (launchd creates no parent).
    assert!(dir.join(".nika/arm/logs").is_dir(), "le dossier de logs");

    // systemd → ~/.config/systemd/user/ (timer + service)
    let out = arm(&dir, &home, &["arm", "--emit", "systemd", "--write"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let timer = home.join(".config/systemd/user/nika.arm.doctor.timer");
    let service = home.join(".config/systemd/user/nika.arm.doctor.service");
    assert!(timer.exists(), "systemd lit là: {}", timer.display());
    assert!(service.exists());
    let timer_body = std::fs::read_to_string(&timer).expect("timer body");
    assert!(
        timer_body.contains(&format!("03:00:00 {zone}")),
        "le fuseau voyage dans OnCalendar: {timer_body}"
    );
    assert!(timer_body.contains("Persistent=false"), "{timer_body}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("systemctl --user enable --now nika.arm.doctor.timer"),
        "{stdout}"
    );

    // --out pose elsewhere.
    let elsewhere = dir.join("elsewhere");
    let out = arm(
        &dir,
        &home,
        &[
            "arm",
            "--emit",
            "launchd",
            "--write",
            "--out",
            elsewhere.to_str().expect("utf8"),
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(elsewhere.join("nika.arm.doctor.plist").exists(), "--out");
}

#[test]
fn emit_never_carries_a_secret_value() {
    let zone = machine_zone();
    let dir = project("secret", &registry_in(&zone));
    let home = home(&dir);
    let env_file = dir.join("providers.env");
    std::fs::write(&env_file, "MISTRAL_API_KEY=hunter2-live-zz\n").expect("env file");
    let env_arg = env_file.to_str().expect("utf8").to_owned();

    // Print mode: the PATH rides, the VALUE never crosses (D7).
    let out = arm(
        &dir,
        &home,
        &["arm", "--emit", "launchd", "--env-file", &env_arg],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("providers.env"),
        "le chemin, nommé: {stdout}"
    );
    assert!(
        !stdout.contains("hunter2-live-zz"),
        "D7 — la valeur ne traverse JAMAIS: {stdout}"
    );

    // Write mode: the written unit neither.
    let out = arm(
        &dir,
        &home,
        &[
            "arm",
            "--emit",
            "systemd",
            "--write",
            "--env-file",
            &env_arg,
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    let service = home.join(".config/systemd/user/nika.arm.doctor.service");
    let body = std::fs::read_to_string(&service).expect("service body");
    assert!(body.contains("EnvironmentFile="), "{body}");
    assert!(body.contains("providers.env"), "{body}");
    assert!(!body.contains("hunter2-live-zz"), "D7: {body}");
}

#[test]
fn emit_renders_arm_fire_never_run() {
    // D2: the firer is ONE. Every emitted unit calls `arm fire <label>`
    // (per-beat) — `run` appears in NO invocation.
    let zone = machine_zone();
    let dir = project("firer", &registry_in(&zone));
    let home = home(&dir);

    let out = arm(&dir, &home, &["arm", "--emit", "launchd"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout.contains("<string>arm</string>"), "{stdout}");
    assert!(stdout.contains("<string>fire</string>"), "{stdout}");
    assert!(
        !stdout.contains("<string>run</string>"),
        "D2 — jamais run: {stdout}"
    );

    let out = arm(&dir, &home, &["arm", "--emit", "systemd"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0));
    let exec_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("ExecStart="))
        .collect();
    assert!(!exec_lines.is_empty(), "le service se rend: {stdout}");
    for line in exec_lines {
        assert!(line.contains(" arm fire "), "D2: {line}");
        assert!(!line.contains(" run"), "D2 — jamais run: {line}");
    }
}

#[test]
fn emit_refuses_a_foreign_zone_on_launchd_with_teaching() {
    // D10: launchd fires in the MACHINE's zone. The test's zone is the
    // machine's, so the beat takes the OTHER one.
    let zone = machine_zone();
    let other = if zone == "Asia/Tokyo" {
        "America/New_York"
    } else {
        "Asia/Tokyo"
    };
    let dir = project("zone", &registry_in(other));
    let home = home(&dir);

    let out = arm(&dir, &home, &["arm", "--emit", "launchd"]);
    assert_eq!(out.status.code(), Some(2), "le refus: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(other), "le fuseau du beat: {stdout}");
    assert!(stdout.contains(&zone), "le fuseau de la machine: {stdout}");
    assert!(stdout.contains("systemd"), "le remède: {stdout}");

    // The same beat RENDERS for systemd (the zone rides OnCalendar=).
    let out = arm(&dir, &home, &["arm", "--emit", "systemd"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(other), "le fuseau voyage: {stdout}");
}

#[test]
fn emit_flags_without_emit_and_mode_system_refuse_honestly() {
    let zone = machine_zone();
    let dir = project("flags", &registry_in(&zone));
    let home = home(&dir);

    // --write sans --emit: un drapeau qui ne fait rien est un mensonge.
    let out = arm(&dir, &home, &["arm", "--write"]);
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--emit"),
        "le refus nomme le drapeau: {stdout}"
    );

    // --mode system: la portée root arrive avec serve (W5).
    let out = arm(
        &dir,
        &home,
        &["arm", "--emit", "launchd", "--mode", "system"],
    );
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("system"), "nommé: {stdout}");
}

// ── W3 · disarm --write (the teardown) ─────────────────────────────

#[test]
fn disarm_refuses_a_foreign_unit() {
    // A unit WITHOUT the GENERATED mark is foreign — never touched,
    // the whole gesture refuses (nothing is ever half-torn down).
    let zone = machine_zone();
    let dir = project("foreign", &registry_in(&zone));
    let home = home(&dir);
    let foreign = home.join("Library/LaunchAgents/nika.arm.doctor.plist");
    std::fs::create_dir_all(foreign.parent().expect("parent")).expect("dir");
    std::fs::write(
        &foreign,
        "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict/></plist>\n",
    )
    .expect("foreign unit");

    let out = arm(&dir, &home, &["arm", "disarm", "doctor", "--write"]);
    assert_eq!(out.status.code(), Some(2), "le refus: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("GENERATED"),
        "la marque est nommée: {stdout}"
    );
    assert!(
        stdout.contains(&*foreign.display().to_string()),
        "le chemin est nommé: {stdout}"
    );
    assert!(foreign.exists(), "l'unité étrangère demeure");
    // … and nothing was journaled (the gesture refused).
    assert!(!dir.join(".nika/arm/doctor/history.ndjson").exists());
}

#[test]
fn disarm_removes_an_emitted_unit_and_journals() {
    let zone = machine_zone();
    let dir = project("teardown", &registry_in(&zone));
    let home = home(&dir);
    // Emit first (the local gesture), then disarm.
    let out = arm(&dir, &home, &["arm", "--emit", "launchd", "--write"]);
    assert_eq!(out.status.code(), Some(0));
    let plist = home.join("Library/LaunchAgents/nika.arm.doctor.plist");
    assert!(plist.exists(), "émise");

    let out = arm(&dir, &home, &["arm", "disarm", "doctor", "--write"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!plist.exists(), "retirée");
    assert!(
        stdout.contains(&format!("launchctl bootout gui/$UID {}", plist.display())),
        "le bootout est imprimé, jamais exécuté: {stdout}"
    );
    // The beat's history carries the disarm (N4 made machine-real).
    let history = std::fs::read_to_string(dir.join(".nika/arm/doctor/history.ndjson"))
        .expect("history.ndjson");
    assert!(history.contains("\"kind\":\"disarmed\""), "{history}");
    assert!(stdout.contains("disarmed"), "le journal est dit: {stdout}");
}

#[test]
fn disarm_without_a_unit_says_nothing_was_torn_down() {
    let zone = machine_zone();
    let dir = project("absent", &registry_in(&zone));
    let home = home(&dir);
    let out = arm(&dir, &home, &["arm", "disarm", "doctor", "--write"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "rien à démonter n'est pas une faute"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("aucune unité"), "{stdout}");
    assert!(
        !dir.join(".nika/arm/doctor/history.ndjson").exists(),
        "rien ne s'est passé, rien ne se journalise"
    );
}
