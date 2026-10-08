// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opening the Session as bare `nika` opens it, for a door that is not a terminal: the kept
//! intelligence choice (or none yet), the machine's census, the reasoner the choice names, the
//! HOME history with its recovery notice and the restored state, continuous preparation and
//! the binary that runs a rehearsal's `nika:jq` steps. No caller names a model, a provider, a
//! path or a HOME: the door's own process and project decide them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_onboard::compile::room::JqHelper;
use nika_session::intelligence::{IntelligenceKind, UserIntelligencePreference};
use nika_session::reasoner::{NoReasoner, ProviderReasoner, SessionReasoner};
use nika_session::{IntelligenceCensus, ResolvedSessionIntelligence, SessionRuntime};

use crate::host::SessionHost;
use crate::run::{LaneRunDoor, NoRunDoor, RunDoor};
use crate::wire::{Frame, Refused};

/// The reasoner for a resolved choice — the seat, the provider, or none — the one factory
/// every door builds a Session's reasoner with.
#[must_use]
pub fn reasoner_for(resolved: &ResolvedSessionIntelligence) -> Box<dyn SessionReasoner> {
    match &resolved.kind {
        #[cfg(feature = "access-harness")]
        IntelligenceKind::Harness { seat, transport } => Box::new(
            nika_session::reasoner::HarnessReasoner { seat: seat.clone() }
                .with_transport(resolved.model.clone(), *transport),
        ),
        #[cfg(not(feature = "access-harness"))]
        IntelligenceKind::Harness { .. } => Box::new(NoReasoner),
        IntelligenceKind::Api { provider } => Box::new(ProviderReasoner {
            model: resolved
                .model
                .clone()
                .unwrap_or_else(|| default_model(provider)),
            label: format!("{provider} API"),
        }),
        IntelligenceKind::Local { provider } => Box::new(ProviderReasoner {
            model: resolved
                .model
                .clone()
                .unwrap_or_else(|| default_model(provider)),
            label: format!("{provider} · local"),
        }),
        _ => Box::new(NoReasoner),
    }
}

/// The provider's first cataloged model when the human named none.
#[must_use]
pub fn default_model(provider: &str) -> String {
    nika_catalog::all_providers()
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(provider))
        .map_or_else(
            || format!("{provider}/default"),
            |p| format!("{provider}/{}", p.default_model),
        )
}

/// The Session over `cwd`, opened as bare `nika` opens it, with the notices its opening said:
/// the banner, the history's recovery notice and what the restored state says.
///
/// # Errors
/// The Session refused its history (held by another Session of this project, corrupt, or
/// unreadable): the refusal's words. No Session is usable then.
pub fn open_bare(
    cwd: &Path,
    home: Option<&Path>,
    jq: Option<JqHelper>,
) -> Result<(SessionRuntime, Vec<String>), String> {
    let census = IntelligenceCensus::take();
    let mut session = match home.and_then(UserIntelligencePreference::load) {
        Some(pref) => SessionRuntime::open_with(cwd, census, &pref, home, Box::new(reasoner_for)),
        None => SessionRuntime::open_unchosen(cwd, census, home, Box::new(reasoner_for)),
    };
    session.enable_continuous_preparation();
    if let Some(helper) = jq {
        session.with_jq_helper(helper);
    }
    let recovered = match home {
        Some(home) => session
            .enable_history(home)
            .map_err(|why| why.to_string())?,
        None => Some("conversation is temporary: no home directory is available".to_owned()),
    };
    let mut notices = vec![session.banner()];
    notices.extend(recovered);
    notices.extend(session.restore_state());
    Ok((session, notices))
}

/// The native machine door on this process's stdio (`nika session --json`): the Session opened
/// as bare `nika` in `cwd`, its runs through `exe` (this binary's machine lane), until its log
/// closes.
///
/// # Errors
/// The Session could not open (one `refused` frame said why on stdout), or stdio failed.
pub fn run_stdio(
    cwd: &Path,
    home: Option<&Path>,
    jq: Option<JqHelper>,
    exe: Option<PathBuf>,
) -> Result<(), String> {
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let (runtime, notices) = match open_bare(cwd, home, jq) {
        Ok(opened) => opened,
        Err(why) => {
            let frame = Frame::refused("", Refused::SessionUnavailable, &why, None, None, None);
            let mut stdout = std::io::stdout().lock();
            let _written =
                std::io::Write::write_all(&mut stdout, format!("{}\n", frame.to_line()).as_bytes());
            return Err(why);
        }
    };
    let door: Box<dyn RunDoor> = match exe {
        Some(exe) => Box::new(LaneRunDoor::new(exe)),
        None => Box::new(NoRunDoor::new(
            "this binary cannot name itself · no run was started",
        )),
    };
    let host = SessionHost::start(runtime, door, notices).map_err(|error| error.to_string())?;
    let host = Arc::new(host);
    let input = std::io::BufReader::new(std::io::stdin());
    crate::machine::drive(&host, input, &output).map_err(|error| error.to_string())
}
