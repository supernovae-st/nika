// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The intelligence list on the real binary, over the rig's kept local route: its own
//! navigation guides its head and says nothing in the conversation (live gauntlet r02 to r09:
//! « Choose an intelligence below. » and « Choose a connection below. No selection has
//! changed. » stayed in the history before « Intelligence: … »). Keyless: nothing is chosen,
//! saved or run, and no model is asked.

use super::*;

/// The guide the connections list opens with.
const CONNECTIONS: &str = "Choose a connection below";

/// 32 · `/intelligence`, `Other choice…`, `Esc` back home and, at once, `Enter` on « Keep
/// current »: the connections guide reads in the list's head while it is open, the `Enter`
/// that follows the step back closes the list, and the conversation then holds none of the
/// list's guides.
#[test]
fn the_intelligence_lists_guides_stay_out_of_the_conversation() {
    let rig = Rig::new("picker");
    let mut term = rig.spawn("picker", 120, 40);
    wait_workspace(&mut term);
    term.send("/intelligence\r");
    term.wait_until("the intelligence list", |screen| {
        screen.contains("Intelligence for this conversation") && screen.contains("Keep current")
    });
    term.keys(DOWN);
    term.send("\r");
    term.wait_workspace_frame("the connections list and its guide", |screen| {
        screen.contains(CONNECTIONS)
    });
    term.settle();
    term.send(ESC);
    term.wait_workspace_frame("home again", |screen| {
        screen.contains("Keep current") && !screen.contains(CONNECTIONS)
    });
    // `Enter` right after the step back is a new act, never its repeat: it keeps the current
    // choice and closes the list.
    term.send("\r");
    term.wait_until("the list closed", |screen| {
        screen.contains("the choice stands") && screen.contains("Ask, change, or run")
    });
    term.settle();
    for guide in ["below.", "No selection has changed", CONNECTIONS] {
        assert!(
            !term.screen.contains(guide),
            "{guide:?} stays in the conversation\n{}",
            term.dump()
        );
    }
    rig.no_model_call("the intelligence list's navigation");
    term.leave();
}
