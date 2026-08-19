// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! `nika serve` — LE TIREUR RÉSIDENT (W5): the SAME `fire` (D2), the wall
//! clock in place of the OS. Gate 1: reads ONLY `nika.yaml` (vocab +
//! cadence judge it before any shot) and its own sidecar. Exit 0 · 1 else.
// A server's whole job is its log lines (the run/mod.rs precedent).
#![allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]
use super::arm::{
    self,
    fire::{FireCtx, fire_beat, labels},
    state::ArmState,
};
use super::{VerbOutput, exit};
use jiff::{SignedDuration, Zoned};
use nika_cadence::registry::ArmRegistry;
use std::path::{Path, PathBuf};
/// `nika serve` — the resident firer's args.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Fire what is due once, then exit — the rehearsal.
    #[arg(long)]
    pub once: bool,
    /// Say what WOULD fire, run nothing.
    #[arg(long)]
    pub dry: bool,
    /// Inject the clock (RFC 3339 · D5) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub now: Option<String>,
    /// Stop the loop at this instant (RFC 3339) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub until: Option<String>,
}
/// The injected edges — `Zoned::now` + `tokio::time::sleep`, or the
/// harness's scripted clock whose sleep ADVANCES it (trap ② avoided).
struct Clock {
    now: Box<dyn Fn() -> Zoned>,
    sleep: SleepFn,
}
/// The sleeper's shape: a span in, a future out (factored for the lint).
type SleepFn = Box<dyn Fn(SignedDuration) -> std::pin::Pin<Box<dyn Future<Output = ()>>>>;
/// The verb edge: both lanes carry a `VerbOutput` (0 clean · 1 otherwise).
#[must_use]
pub fn run(args: &ServeArgs) -> VerbOutput {
    match go(args) {
        Ok(out) | Err(out) => out,
    }
}
fn go(args: &ServeArgs) -> Result<VerbOutput, VerbOutput> {
    let fail = |text: String| VerbOutput {
        text,
        code: exit::WORKFLOW,
    };
    let now = instant(args.now.as_deref()).map_err(&fail)?;
    let until = instant(args.until.as_deref()).map_err(&fail)?;
    if now.is_some() && !args.once && until.is_none() {
        return Err(fail(
            "serve · --now sans --once exige --until — une horloge scriptée sans borne tourne à vide"
                .to_owned(),
        ));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (path, registry) = arm::load(&cwd).map_err(|out| fail(out.text))?;
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    serve(&root, registry, args, until.as_ref(), &clock(now)).map_err(fail)?;
    Ok(VerbOutput::ok(String::new()))
}
/// An RFC 3339 instant — the zoned form keeps its zone, a bare one rides
/// UTC (the arm fire `--now` precedent).
fn instant(raw: Option<&str>) -> Result<Option<Zoned>, String> {
    raw.map_or(Ok(None), |text| {
        text.parse::<Zoned>()
            .or_else(|_| {
                text.parse::<jiff::Timestamp>()
                    .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC))
            })
            .map(Some)
            .map_err(|_| format!("serve · `{text}` · RFC 3339 attendu — 2026-08-19T03:02:00Z"))
    })
}
/// The two edges: the wall clock + a real sleep, or the scripted harness
/// (start at the injected instant · a sleep ADVANCES it, never waits).
fn clock(start: Option<Zoned>) -> Clock {
    match start {
        None => Clock {
            now: Box::new(Zoned::now),
            sleep: Box::new(|d| {
                Box::pin(tokio::time::sleep(
                    std::time::Duration::try_from(d).unwrap_or_default(),
                ))
            }),
        },
        Some(t) => {
            let cell = std::rc::Rc::new(std::cell::RefCell::new(t));
            let advance = cell.clone();
            Clock {
                now: Box::new(move || cell.borrow().clone()),
                sleep: Box::new(move |d| {
                    let next = advance.borrow().clone() + d;
                    *advance.borrow_mut() = next;
                    Box::pin(async {})
                }),
            }
        }
    }
}
/// The loop: the clock, the reload on change (« le fichier propose » —
/// re-read, never cache; a broken edit is told and the last-good registry
/// keeps serving), the SAME firer for what is due, then the sleep to the
/// next slot (≤ 60 s) racing the signals — a signal breaks clean (the
/// current fire, synchronous, finishes first).
fn serve(
    root: &Path,
    mut reg: ArmRegistry,
    args: &ServeArgs,
    until: Option<&Zoned>,
    clock: &Clock,
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("serve · the signal runtime refused: {e}"))?;
    #[cfg(unix)]
    let mut term = rt.block_on(async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok()
    });
    let path = root.join(nika_vocab::project::FILE_NAME);
    let mut mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    loop {
        let now = (clock.now)();
        if until.is_some_and(|u| now >= *u) {
            return Ok(());
        }
        if let Ok(fresh) = std::fs::metadata(&path).and_then(|m| m.modified())
            && Some(fresh) != mtime
        {
            match arm::load(root) {
                Ok((_, reloaded)) => {
                    reg = reloaded;
                    mtime = Some(fresh);
                }
                Err(out) => eprintln!("nika: {}", out.text),
            }
        }
        let state = ArmState::at_project(root);
        let names = labels(&reg);
        let dues: Vec<(usize, nika_cadence::Slot)> = nika_cadence::due(&reg, &now, &|i| {
            names.get(i).and_then(|l| state.last_fired(l))
        })
        .map_err(|e| format!("serve · a validated registry refuses: {e}"))?
        .map(|d| (d.index, d.slot))
        .collect();
        for (index, slot) in dues {
            let label = names[index].clone();
            if args.dry {
                println!("would fire {label} · slot {}", slot.at.timestamp());
                continue;
            }
            let ctx = FireCtx {
                project_root: root.to_path_buf(),
                registry: reg,
                index,
                label,
                now: now.clone(),
                state: ArmState::at_project(root),
            };
            println!("{}", fire_beat(&ctx).line);
            reg = ctx.registry;
        }
        if args.once {
            return Ok(());
        }
        let next = nika_cadence::earliest_next(&reg, &now)
            .map_err(|e| format!("serve · a validated registry refuses: {e}"))?;
        let secs = next.map_or(60, |(_, s)| {
            (s.at.timestamp().as_second() - now.timestamp().as_second()).clamp(1, 60)
        });
        let stop = rt.block_on(async {
            #[cfg(unix)]
            let sig = async {
                if let Some(s) = &mut term {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            #[cfg(not(unix))]
            let sig = std::future::pending::<()>();
            tokio::select! {
                () = (clock.sleep)(SignedDuration::from_secs(secs)) => false,
                _ = tokio::signal::ctrl_c() => true,
                () = sig => true,
            }
        });
        if stop {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn at(text: &str) -> Zoned {
        text.parse::<jiff::Timestamp>()
            .expect("ts")
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    /// A tempdir project (registry + workflow shelf) — the arm precedent.
    fn project(tag: &str, registry: &str) -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-serve-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        std::fs::create_dir_all(dir.path().join("workflows")).expect("workflows");
        std::fs::write(dir.path().join("nika.yaml"), registry).expect("registry");
        dir
    }

    fn write_workflow(root: &Path, name: &str) {
        let body = format!(
            "nika: {name}\npermits: {{ exec: true }}\ntasks:\n  ok:\n    exec: {{ shell: \"true\" }}\n"
        );
        std::fs::write(root.join("workflows").join(name), body).expect("workflow");
    }

    /// Hourly beats — mid-hour nothing is ever ON TIME, so the loop only
    /// ever SKIPS (an in-process run chdirs: the binary tests' ground —
    /// parallel tests race on the process CWD).
    const HOURLY_A: &str = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );

    /// The v2: a second beat appears between two ticks.
    const HOURLY_AB: &str = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/nightly.nika.yaml\n",
        "    cadence: \"TZ=UTC 0 * * * *\"\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );

    /// Seed a decided slot (a past skip) so the planner counts a silence.
    fn seed_last(root: &Path, label: &str, slot: &str) {
        let dir = root.join(".nika/arm").join(label);
        std::fs::create_dir_all(&dir).expect("sidecar");
        std::fs::write(
            dir.join("last.json"),
            format!(
                "{{\"slot\":\"{slot}\",\"fired_at\":\"{slot}\",\"trace\":null,\"exit\":0,\"kind\":\"skipped\"}}\n"
            ),
        )
        .expect("seed last.json");
    }

    fn history(root: &Path, label: &str) -> String {
        std::fs::read_to_string(root.join(".nika/arm").join(label).join("history.ndjson"))
            .unwrap_or_default()
    }

    /// The scripted clock: starts at `start`, each `sleep` ADVANCES it
    /// (trap ② avoided by construction) and lands ready — the first sleep
    /// also runs the test's actor (the reload test rewrites the file there).
    fn scripted(start: &str, on_first_sleep: impl FnMut() + 'static) -> Clock {
        let cell = std::rc::Rc::new(std::cell::RefCell::new(at(start)));
        let advance = cell.clone();
        let actor = std::cell::RefCell::new(Some(on_first_sleep));
        Clock {
            now: Box::new(move || cell.borrow().clone()),
            sleep: Box::new(move |span| {
                let next = advance.borrow().clone() + span;
                *advance.borrow_mut() = next;
                if let Some(mut act) = actor.borrow_mut().take() {
                    act();
                }
                Box::pin(async {})
            }),
        }
    }

    fn serve_args() -> ServeArgs {
        ServeArgs {
            once: false,
            dry: false,
            now: None,
            until: None,
        }
    }

    #[test]
    fn serve_reloads_the_registry_when_the_file_changes() {
        let dir = project("reload", HOURLY_A);
        write_workflow(dir.path(), "doctor.nika.yaml");
        write_workflow(dir.path(), "nightly.nika.yaml");
        seed_last(dir.path(), "doctor", "2026-08-19T00:00:00Z");
        seed_last(dir.path(), "nightly", "2026-08-19T00:00:00Z");
        let path = dir.path().join("nika.yaml");
        let registry = match arm::load(dir.path()) {
            Ok((_, registry)) => registry,
            Err(out) => panic!("load v1: {}", out.text),
        };
        let rewritten = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = rewritten.clone();
        let yaml = path.clone();
        let clock = scripted("2026-08-19T03:30:00Z", move || {
            // mtime must move: a real beat of the wall between the writes.
            std::thread::sleep(std::time::Duration::from_millis(5));
            std::fs::write(&yaml, HOURLY_AB).expect("rewrite");
            flag.set(true);
        });
        let until = at("2026-08-19T03:35:00Z");
        let rc = serve(dir.path(), registry, &serve_args(), Some(&until), &clock);
        assert!(rc.is_ok(), "{rc:?}");
        assert!(rewritten.get(), "the actor ran");
        // Tick 1 (registry v1): doctor's silence is 3 slots — skipped.
        let doctor = history(dir.path(), "doctor");
        assert_eq!(doctor.lines().count(), 1, "{doctor}");
        assert!(doctor.contains("\"reason\":\"missed:3\""), "{doctor}");
        // Tick 2 (registry v2): nightly's line is the reload's proof —
        // v1 never named it, and the loop never restarted.
        let nightly = history(dir.path(), "nightly");
        assert_eq!(nightly.lines().count(), 1, "the reload: {nightly}");
        assert!(nightly.contains("\"reason\":\"missed:3\""), "{nightly}");
    }

    #[test]
    fn a_scripted_clock_without_a_bound_refuses() {
        let args = ServeArgs {
            once: false,
            dry: false,
            now: Some("2026-08-19T03:02:00Z".to_owned()),
            until: None,
        };
        let out = run(&args);
        assert_eq!(out.code, exit::WORKFLOW, "{}", out.text);
        assert!(out.text.contains("--until"), "{}", out.text);
    }

    #[test]
    fn the_injected_instant_parses_rfc3339_and_refuses_garbage() {
        assert!(instant(Some("demain")).is_err());
        let zoned = instant(Some("2026-08-19T05:02:00+02:00[Europe/Paris]"))
            .expect("parses")
            .expect("some");
        assert_eq!(zoned.timestamp().to_string(), "2026-08-19T03:02:00Z");
    }
}
