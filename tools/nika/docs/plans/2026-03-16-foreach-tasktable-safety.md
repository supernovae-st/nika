# for_each + TaskTable Safety Hardening Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Verify and harden the for_each / TaskTable interaction after wave3's dangling-dep detection change.

**Architecture:** Nika's 3-phase AST pipeline (Parse -> Analyze -> Lower) builds an immutable TaskTable in Phase 2. for_each expansion happens only at runtime (runner.rs), using `Arc<str>` naming — never `TaskId`. The wave3 `.ok_or_else()` change in `Dag::from_analyzed()` is confirmed safe because identity spaces are disjoint.

**Tech Stack:** Rust, rig-core v0.32, tokio JoinSet

---

## Research Summary

### Agents Deployed

| Agent | Focus | Key Finding |
|-------|-------|-------------|
| **Explorer** | Trace full pipeline | for_each stays as metadata through all 3 phases; expansion happens only in `runner.rs:1057-1488` |
| **Rust Architect** | Architectural safety | Identity spaces disjoint: `TaskId(u32)` vs `Arc<str>` runtime IDs. Change is safe. Secondary target: `lower.rs:390` `filter_map` |
| **Web Researcher** | Industry patterns | Airflow/Dagster/Prefect all validate template graph, expand at runtime. Nika follows industry best practice. |
| **Context7** | tokio/petgraph | JoinSet pattern for dynamic fan-out confirmed correct |

### Core Finding: The Concern Was Unfounded

```
Phase 2 (Analyze):  TaskTable = { "step1" -> TaskId(0), "process" -> TaskId(1), "step3" -> TaskId(2) }
                    depends_on = Vec<TaskId>  (all validated against TaskTable)

DAG Construction:   Reads TaskTable + depends_on -> builds edges (ok_or_else is SAFE here)

Runtime Expansion:  "process[0]", "process[1]", "process[2]" -> Arc<str>, NOT TaskId
                    These NEVER enter TaskTable or depends_on vectors
```

The `for_each` expansion creates `Arc<str>` identifiers (`format!("{}[{}]", task.name, idx)`) at runtime. These are stored in `RunContext` (datastore), never in the `TaskTable` or as `TaskId` references. The DAG only sees the template task `"process"`.

### Secondary Finding: `lower.rs:task_dep_names` Has Same Silent-Skip Pattern

```rust
// lower.rs:390 — currently silently drops unknown TaskIds
fn task_dep_names(depends: &[TaskId], implicit: &[TaskId], table: &TaskTable) -> Option<Vec<String>> {
    let deps: Vec<String> = depends.iter()
        .chain(implicit.iter())
        .filter_map(|id| table.get_name(*id).map(String::from))  // <-- silent skip
        .collect();
}
```

This is the same pattern we fixed in `flow.rs`. While the lowering path is less critical (it feeds `Workflow.flow` which is the string-based representation), it's still a defense-in-depth opportunity.

---

## Tasks

### Task 1: Integration Test — for_each Workflow Survives DAG Construction

**Files:**
- Modify: `tools/nika/src/dag/flow.rs` (add test at bottom)

**Step 1: Write the failing test**

```rust
#[test]
fn test_for_each_workflow_dag_construction_succeeds() {
    // Prove that for_each tasks with depends_on pass through Dag::from_analyzed
    // without triggering false MissingDependency errors.
    let mut task_table = TaskTable::new();
    task_table.insert("produce");
    task_table.insert("process");
    task_table.insert("aggregate");

    let id_produce = task_table.get_id("produce").unwrap();
    let id_process = task_table.get_id("process").unwrap();

    let workflow = AnalyzedWorkflow {
        tasks: vec![
            AnalyzedTask {
                name: "produce".to_string(),
                depends_on: vec![],
                implicit_deps: vec![],
                ..Default::default()
            },
            AnalyzedTask {
                name: "process".to_string(),
                depends_on: vec![id_produce],
                implicit_deps: vec![],
                for_each: Some(AnalyzedForEach {
                    items: r#"["a", "b", "c"]"#.to_string(),
                    as_var: "item".to_string(),
                    parallel: Some(3),
                    fail_fast: true,
                    span: Span::dummy(),
                }),
                ..Default::default()
            },
            AnalyzedTask {
                name: "aggregate".to_string(),
                depends_on: vec![id_process],
                implicit_deps: vec![],
                ..Default::default()
            },
        ],
        task_table,
        ..Default::default()
    };

    let result = Dag::from_analyzed(&workflow);
    assert!(result.is_ok(), "for_each workflow should build DAG without false rejections: {:?}", result.err());

    let dag = result.unwrap();
    assert!(dag.has_path("produce", "process"));
    assert!(dag.has_path("process", "aggregate"));
}
```

**Step 2: Run test to verify it passes** (this is a proof test, should pass immediately)

Run: `CARGO_TARGET_DIR=target-main cargo test --lib dag::flow::tests::test_for_each_workflow_dag_construction_succeeds`
Expected: PASS (proves no false rejection)

**Step 3: Commit**

```bash
git add src/dag/flow.rs
git commit -m "test(dag): prove for_each workflows survive dangling-dep detection"
```

---

### Task 2: Harden `lower.rs:task_dep_names` — Replace filter_map with Error

**Files:**
- Modify: `tools/nika/src/ast/lower.rs:382-397`
- Modify: `tools/nika/src/ast/lower.rs:83` (adjust calling code)

**Step 1: Write the failing test**

```rust
#[test]
fn test_task_dep_names_rejects_dangling_task_id() {
    let mut table = TaskTable::new();
    table.insert("step1");
    let dangling = TaskId::new(99);

    let result = task_dep_names(&[dangling], &[], &table);
    assert!(result.is_err(), "Dangling TaskId should be rejected in lowering");
}
```

**Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=target-main cargo test --lib ast::lower::tests::test_task_dep_names_rejects_dangling_task_id`
Expected: FAIL (function returns `Option`, not `Result`)

**Step 3: Change `task_dep_names` signature and implementation**

```rust
fn task_dep_names(
    depends: &[TaskId],
    implicit: &[TaskId],
    table: &TaskTable,
) -> Result<Option<Vec<String>>, NikaError> {
    let mut deps = Vec::new();
    for id in depends.iter().chain(implicit.iter()) {
        let name = table.get_name(*id).ok_or_else(|| {
            NikaError::InternalError(format!(
                "Lowering: TaskId({}) not found in TaskTable (invariant violation)",
                id.0
            ))
        })?;
        deps.push(name.to_string());
    }
    Ok(if deps.is_empty() { None } else { Some(deps) })
}
```

**Step 4: Update `lower_task` to propagate the error**

Change `lower_task` from infallible to `Result`:
```rust
fn lower_task(task: AnalyzedTask, table: &TaskTable) -> Result<Task, NikaError> {
    let flow = task_dep_names(&task.depends_on, &task.implicit_deps, table)?;
    // ... rest unchanged, wrap return in Ok(Task { ... })
}
```

And update `lower()` to propagate:
```rust
pub fn lower(analyzed: AnalyzedWorkflow) -> Result<Workflow, NikaError> {
    let tasks: Result<Vec<Task>, NikaError> = analyzed.tasks
        .into_iter()
        .map(|t| lower_task(t, &analyzed.task_table))
        .collect();
    // ...
}
```

**Step 5: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=target-main cargo test --lib`
Expected: PASS (all 5174+ tests)

**Step 6: Clippy**

Run: `CARGO_TARGET_DIR=target-main cargo clippy --lib -- -D warnings`

**Step 7: Commit**

```bash
git add src/ast/lower.rs
git commit -m "fix(ast): harden task_dep_names to reject dangling TaskIds in lowering"
```

---

### Task 3: E2E Integration Test — for_each with depends_on Full Pipeline

**Files:**
- Modify: `tools/nika/tests/for_each_test.rs` (add test)

**Step 1: Write the integration test**

```rust
#[tokio::test]
async fn test_for_each_with_depends_on_full_pipeline() {
    // Prove: YAML with for_each + depends_on goes through
    // parse -> analyze -> DAG -> run without false rejection
    let yaml = r#"
schema: nika/workflow@0.12
workflow: foreach-depends-pipeline
tasks:
  - id: produce
    exec: 'echo ''["alpha", "beta", "gamma"]'''

  - id: process
    depends_on: [produce]
    for_each: $produce
    exec: "echo Processing {{with.item}}"

  - id: aggregate
    depends_on: [process]
    exec: "echo Done"
"#;
    let workflow = nika::ast::parse_workflow(yaml)
        .expect("for_each + depends_on should parse and analyze without error");

    let mut runner = nika::runtime::Runner::new(workflow)
        .expect("for_each + depends_on should build DAG without false rejection");

    let result = runner.run().await;
    assert!(result.is_ok(), "for_each pipeline should execute: {:?}", result.err());
}
```

**Step 2: Run test**

Run: `CARGO_TARGET_DIR=target-main cargo test --test for_each_test test_for_each_with_depends_on_full_pipeline`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/for_each_test.rs
git commit -m "test(e2e): for_each + depends_on survives full pipeline"
```

---

### Task 4: Document the Finding in Code Comments

**Files:**
- Modify: `tools/nika/src/dag/flow.rs:84-86` (add comment before the edge-building loop)

**Step 1: Add documentation comment**

```rust
// Build edges from pre-computed dependencies (depends_on + implicit_deps).
// Both are Vec<TaskId> resolved by the analyzer.
//
// SAFETY: for_each expansion happens only at runtime (runner.rs), creating
// Arc<str> identifiers like "task[0]", "task[1]" — NOT TaskIds.
// The DAG sees only the template task. The .ok_or_else() below is
// defense-in-depth: if a TaskId is missing, it's an analyzer bug.
```

**Step 2: Commit**

```bash
git add src/dag/flow.rs
git commit -m "docs(dag): document for_each safety invariant in edge builder"
```

---

## Execution Order

```
Task 1 (proof test)     → Independent, commit
Task 2 (lower.rs)       → Independent, commit
Task 3 (E2E test)       → Independent, commit
Task 4 (docs comment)   → Independent, commit
```

All 4 tasks are independent and can be executed in any order.

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Task 2 breaks `lower()` callers | LOW | MED | `lower()` is called in 3 places; all can propagate `Result` |
| Task 3 E2E test flaky | LOW | LOW | Uses `exec: echo`, no network, no LLM |
| for_each false rejection | **ZERO** | N/A | Confirmed: identity spaces disjoint |

## Summary

The wave3 dangling-dep detection (`ok_or_else` in `flow.rs`) is **confirmed safe** for all for_each workflows. The concern about partial TaskTables was unfounded — for_each expansion operates in a completely separate identity space (`Arc<str>` runtime IDs) from the analyzer's `TaskId(u32)` system.

The plan adds:
1. Proof test in DAG layer (Task 1)
2. Defense-in-depth hardening in lowering layer (Task 2)
3. E2E pipeline test (Task 3)
4. Code documentation (Task 4)
