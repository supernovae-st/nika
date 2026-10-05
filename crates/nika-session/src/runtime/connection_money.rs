// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A selected subscription is separate from retained API exposure, never a replacement allowance.
use super::{IntelligenceKind, SessionRuntime};

pub(super) const SUBSCRIPTION_HOLD: &str = "a USD ceiling cannot meter subscription authoring; no call was sent; use /intelligence to explicitly choose the subscription again with its invoice unknown, then state the work without a USD ceiling";

impl SessionRuntime {
    pub(super) fn subscription(&self) -> bool {
        matches!(self.intelligence.kind, IntelligenceKind::Harness { .. })
    }
    pub(super) fn subscription_open(&self) -> bool {
        self.subscription() && !self.intent.decisions.iter().any(|d| d == SUBSCRIPTION_HOLD)
    }
    pub(super) fn hold_subscription(&mut self) {
        if self.subscription() && !self.intent.decisions.iter().any(|d| d == SUBSCRIPTION_HOLD) {
            self.intent.decisions.push(SUBSCRIPTION_HOLD.into());
        }
    }
    /// Only the human's successful connection choice releases this connection's own refusal.
    pub(super) fn connection_money(&mut self) -> &'static str {
        if !self.subscription() {
            return "";
        }
        self.intent.decisions.retain(|d| d != SUBSCRIPTION_HOLD);
        if let Some(account) = &self.money.account {
            let _ = account.close("API admission suspended for subscription authoring");
            self.money.reconfirm = true;
        }
        "\n  subscription authoring: invoice unknown; no API fallback; any retained API allowance is suspended, with its expenses preserved; using that allowance again requires a fresh TOTAL ceiling"
    }
}
