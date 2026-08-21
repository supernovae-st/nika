// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Live [`WorkflowIntrospect`] — the cell the runtime writes as tasks
//! settle. Injected at composition (`Arc` shared with the dispatcher).
//! Every view is `available: true` once the DAG is seeded; zeros are
//! never served as a stand-in for « no run » (that remains [`crate::NoWorkflow`]).

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::WorkflowIntrospect;

/// One settled task as `view: records` wants it.
#[derive(Clone, Debug)]
struct TaskSnap {
    status: String,
    duration_ms: Option<u64>,
}

/// The snapshot the lock guards.
#[derive(Clone, Debug, Default)]
struct Snap {
    seeded: bool,
    nodes: Vec<String>,
    edges: Vec<(String, String)>,
    waves: Vec<Vec<String>>,
    tasks: BTreeMap<String, TaskSnap>,
    spent_usd: f64,
    any_priced: bool,
    by_source: BTreeMap<String, f64>,
}

/// Shared live-run introspection. `Send + Sync`. Poison recovers.
#[derive(Debug, Default)]
pub struct LiveInspect {
    inner: RwLock<Snap>,
}

impl LiveInspect {
    /// Empty cell — views stay `available: false` until [`Self::seed_dag`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> Snap {
        self.inner
            .read()
            .map_or_else(|e| e.into_inner().clone(), |g| g.clone())
    }

    fn write<R>(&self, f: impl FnOnce(&mut Snap) -> R) -> R {
        let mut g = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }

    /// Seed the static DAG (call once at run start, before any task).
    pub fn seed_dag(
        &self,
        nodes: Vec<String>,
        edges: Vec<(String, String)>,
        waves: Vec<Vec<String>>,
    ) {
        self.write(|s| {
            s.seeded = true;
            s.nodes = nodes;
            s.edges = edges;
            s.waves = waves;
        });
    }

    /// Replace the settled-task map (call after each wave merge).
    pub fn replace_records(&self, rows: impl IntoIterator<Item = (String, String, Option<u64>)>) {
        self.write(|s| {
            s.tasks = rows
                .into_iter()
                .map(|(id, status, duration_ms)| {
                    (
                        id,
                        TaskSnap {
                            status,
                            duration_ms,
                        },
                    )
                })
                .collect();
        });
    }

    /// Copy the ledger fold (spent · priced? · by attribution key).
    pub fn set_spend(&self, spent_usd: f64, any_priced: bool, by_source: BTreeMap<String, f64>) {
        self.write(|s| {
            s.spent_usd = spent_usd;
            s.any_priced = any_priced;
            s.by_source = by_source;
        });
    }
}

impl WorkflowIntrospect for LiveInspect {
    fn cost(&self) -> serde_json::Value {
        let s = self.read();
        if !s.seeded {
            return no_run("cost");
        }
        serde_json::json!({
            "available": true,
            "view": "cost",
            "total_usd": if s.any_priced { s.spent_usd } else { 0.0 },
            "metered": s.any_priced,
            "by_task": {},
            "by_provider": s.by_source,
        })
    }

    fn records(&self) -> serde_json::Value {
        let s = self.read();
        if !s.seeded {
            return no_run("records");
        }
        let tasks: Vec<serde_json::Value> = s
            .tasks
            .iter()
            .map(|(id, t)| {
                serde_json::json!({
                    "id": id,
                    "status": t.status,
                    "duration_ms": t.duration_ms,
                })
            })
            .collect();
        serde_json::json!({
            "available": true,
            "view": "records",
            "tasks": tasks,
        })
    }

    fn dag_info(&self) -> serde_json::Value {
        let s = self.read();
        if !s.seeded {
            return no_run("dag_info");
        }
        let edges: Vec<serde_json::Value> = s
            .edges
            .iter()
            .map(|(from, to)| serde_json::json!({ "from": from, "to": to }))
            .collect();
        serde_json::json!({
            "available": true,
            "view": "dag_info",
            "nodes": s.nodes,
            "edges": edges,
            "waves": s.waves,
        })
    }

    fn threads(&self) -> serde_json::Value {
        let s = self.read();
        if !s.seeded {
            return no_run("threads");
        }
        let completed = s.tasks.len();
        let total = s.nodes.len();
        let remaining = total.saturating_sub(completed);
        let (active, queued) = if remaining == 0 {
            (0, 0)
        } else {
            (1, remaining.saturating_sub(1))
        };
        serde_json::json!({
            "available": true,
            "view": "threads",
            "active": active,
            "queued": queued,
            "completed": completed,
        })
    }
}

fn no_run(view: &str) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "view": view,
        "reason": "nika:inspect has no live run context here — the DAG was not seeded",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unseeded_is_honestly_unavailable() {
        let cell = LiveInspect::new();
        for view in [cell.cost(), cell.records(), cell.dag_info(), cell.threads()] {
            assert_eq!(view["available"], false);
        }
    }

    #[test]
    fn seeded_dag_is_available_and_names_its_nodes() {
        let cell = LiveInspect::new();
        cell.seed_dag(
            vec!["look".into(), "done".into()],
            vec![("look".into(), "done".into())],
            vec![vec!["look".into()], vec!["done".into()]],
        );
        let dag = cell.dag_info();
        assert_eq!(dag["available"], true);
        assert_eq!(dag["nodes"][0], "look");
        assert_eq!(dag["waves"].as_array().map(Vec::len), Some(2));
        cell.replace_records([("look".into(), "success".into(), Some(3))]);
        let rec = cell.records();
        assert_eq!(rec["tasks"][0]["id"], "look");
        assert_eq!(rec["tasks"][0]["duration_ms"], 3);
        let th = cell.threads();
        assert_eq!(th["completed"], 1);
        assert_eq!(th["active"], 1);
    }
}
