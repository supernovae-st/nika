// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where a workflow reaches when it runs, as every host shows it: local files, a service on
//! this machine (an exact loopback host such as a contract server or a test sink), a connected
//! service (a public host), an MCP tool or a program whose destinations the check cannot
//! determine. Read from the check's data journey over the exact bytes, never from a label, a
//! model name or a guess, so a fixture or a local contract server is never shown as the real
//! service.
//!
//! The reading is declared. The trace witnesses file and tool permits per operation, but
//! network destinations stay declared only (the permit witness residual): a finished run does
//! not upgrade this reading into an observation of what was contacted. A connected host means
//! the bytes may reach it, not that a call succeeded.

use serde::Serialize;

/// What one place a workflow reaches is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlaceKind {
    /// A file the workflow reads (`fs.read`).
    FileRead,
    /// A file the workflow writes (`fs.write`).
    FileWrite,
    /// An exact loopback host (`localhost` · `127.x` · `::1`): a service on this machine, such
    /// as a contract server or a test sink, never the real service.
    LocalService,
    /// A public host: a connected service the bytes may reach.
    ConnectedService,
    /// A documentation host (`example.com` · `.test` · `.invalid` …): the live client does not
    /// dial it, so it stands for no service.
    Placeholder,
    /// A private, link-local or metadata host: the network floor refuses it even when named.
    Refused,
    /// An MCP tool: the check does not know which service its server reaches.
    ToolServer,
    /// A program the workflow runs: the check does not know what it contacts.
    Program,
    /// An endpoint kind this reading does not know yet: kept, never dropped.
    Unclassified,
}

impl PlaceKind {
    /// The words a host shows for the kind.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::FileRead => "reads file",
            Self::FileWrite => "writes file",
            Self::LocalService => "local service",
            Self::ConnectedService => "connected service",
            Self::Placeholder => "placeholder host",
            Self::Refused => "refused host",
            Self::ToolServer => "MCP tool",
            Self::Program => "program",
            Self::Unclassified => "unclassified",
        }
    }
}

/// One place the bytes reach, with the tasks that touch it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Place {
    /// What the place is.
    pub kind: PlaceKind,
    /// The declared path, host, tool id or program: a class, never a value.
    pub target: String,
    /// The tasks touching it, as the check named them.
    pub tasks: Vec<String>,
}

/// How far a workflow reaches, from its most external place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reach {
    /// Files and computation only: nothing leaves this machine.
    Local,
    /// Besides files, only services on this machine: a contract server or a test sink, never
    /// the real service.
    LocalServices,
    /// At least one public host: the bytes may reach a connected service.
    Connected,
    /// No public host, but an MCP tool or a program whose destinations the check cannot
    /// determine; also the reach of bytes that were not audited.
    #[default]
    Undetermined,
}

impl Reach {
    /// The words a host shows for the reach.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Local => "local only",
            Self::LocalServices => "local services only",
            Self::Connected => "connected services",
            Self::Undetermined => "reach undetermined",
        }
    }
}

/// What the reading stands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Basis {
    /// The check's data journey over the exact bytes.
    Declared,
    /// The bytes could not be audited: nothing is claimed.
    #[default]
    NotAudited,
}

/// Where one workflow's exact bytes reach.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct World {
    /// What the reading stands on.
    pub basis: Basis,
    /// The most external reach among the places.
    pub reach: Reach,
    /// Every place, in the journey's order.
    pub places: Vec<Place>,
    /// The declared secrets the bytes use: names, never values or lookup keys.
    pub credentials: Vec<String>,
}

impl World {
    /// The reach of audited bytes, from the data journey's endpoints as `(kind, target,
    /// tasks)` with the journey's kinds (`fs.read` · `fs.write` · `net.http` · `mcp.tool` ·
    /// `exec`) and the names of the secrets they use. A kind this reading does not know stays
    /// visible as [`PlaceKind::Unclassified`] and makes the reach undetermined.
    #[must_use]
    pub fn declared<'a>(
        endpoints: impl IntoIterator<Item = (&'a str, &'a str, &'a [String])>,
        credentials: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let places: Vec<Place> = endpoints
            .into_iter()
            .map(|(kind, target, tasks)| Place {
                kind: kind_of(kind, target),
                target: target.to_owned(),
                tasks: tasks.to_vec(),
            })
            .collect();
        let mut credentials: Vec<String> = credentials.into_iter().map(str::to_owned).collect();
        credentials.sort();
        credentials.dedup();
        Self {
            basis: Basis::Declared,
            reach: reach_of(&places),
            places,
            credentials,
        }
    }

    /// The places of one kind, in order.
    pub fn of_kind(&self, kind: PlaceKind) -> impl Iterator<Item = &Place> {
        self.places.iter().filter(move |p| p.kind == kind)
    }

    /// One line a host may show: the reach, then the places that decide it.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.basis == Basis::NotAudited {
            return "reach unknown · the bytes were not audited".to_owned();
        }
        let named = |kind: PlaceKind| {
            self.of_kind(kind)
                .map(|p| p.target.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut line = match self.reach {
            Reach::Local if self.places.is_empty() => {
                "local only · nothing outside the process".to_owned()
            }
            Reach::Local => "local only · files, no service reached".to_owned(),
            Reach::LocalServices => format!(
                "local services only: {} · no connected service",
                named(PlaceKind::LocalService)
            ),
            Reach::Connected => format!("connected: {}", named(PlaceKind::ConnectedService)),
            Reach::Undetermined => {
                let unknown: Vec<&str> = self
                    .places
                    .iter()
                    .filter(|p| undetermined(p.kind))
                    .map(|p| p.target.as_str())
                    .collect();
                format!("reach undetermined: {}", unknown.join(", "))
            }
        };
        if !self.credentials.is_empty() {
            line.push_str(" · credentials: ");
            line.push_str(&self.credentials.join(", "));
        }
        line
    }
}

/// The kind of one journey endpoint. A host is judged by the same network predicates the
/// floor and the check use (`nika_types::net`), never by a second list.
fn kind_of(kind: &str, target: &str) -> PlaceKind {
    match kind {
        "fs.read" => PlaceKind::FileRead,
        "fs.write" => PlaceKind::FileWrite,
        "net.http" => host_kind(target),
        "mcp.tool" => PlaceKind::ToolServer,
        "exec" => PlaceKind::Program,
        _ => PlaceKind::Unclassified,
    }
}

/// A declared host: an exact loopback literal is a local service (the author's explicit
/// declassification), any other floor-blocked host is refused, a documentation name stands for
/// no service, and every other host is a connected service.
fn host_kind(host: &str) -> PlaceKind {
    if nika_types::net::is_exact_loopback_literal(host) {
        PlaceKind::LocalService
    } else if nika_types::net::host_is_blocked(host) {
        PlaceKind::Refused
    } else if nika_types::net::is_documentation_host(host) {
        PlaceKind::Placeholder
    } else {
        PlaceKind::ConnectedService
    }
}

/// Whether the check cannot say where this kind of place leads.
fn undetermined(kind: PlaceKind) -> bool {
    matches!(
        kind,
        PlaceKind::ToolServer | PlaceKind::Program | PlaceKind::Unclassified
    )
}

/// The most external reach: a public host first, then an undetermined destination, then a
/// local service; files, placeholders and refused hosts reach nothing outside.
fn reach_of(places: &[Place]) -> Reach {
    if places.iter().any(|p| p.kind == PlaceKind::ConnectedService) {
        Reach::Connected
    } else if places.iter().any(|p| undetermined(p.kind)) {
        Reach::Undetermined
    } else if places.iter().any(|p| p.kind == PlaceKind::LocalService) {
        Reach::LocalServices
    } else {
        Reach::Local
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
