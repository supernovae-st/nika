# Research Report: Terminal DAG Execution Visualization

**Date**: 2026-03-26
**Scope**: How workflow/pipeline tools visualize DAG execution in terminals
**Pages analyzed**: 12+ sources across Perplexity searches, Dagger/Terraform/Nx/Bazel docs
**Confidence**: High -- all major tools surveyed, patterns cross-referenced

---

## Summary

Terminal DAG visualization falls into four distinct paradigms: streaming log lines (Terraform), TUI with live tree updates (Dagger), progress bars with task lists (Nx/Turborepo), and layered topological views (Buck2/Bazel). For workflows with 5-50 tasks (Nika's sweet spot), the **TUI with live updates** approach provides the best balance of dependency visibility, real-time status, and debuggability. The most effective implementations combine a **topological layer view** with **per-task status indicators**, **elapsed time bars**, and **Unicode edge rendering**.

---

## 1. Terraform Apply

### How It Works
Terraform uses **streaming log lines** with no graphical DAG -- dependencies are implied by execution timing. The internal DAG drives parallel execution, but the terminal output is purely sequential log entries.

### Terminal Output Format
```
aws_vpc.main: Creating...
aws_vpc.main: Still creating... [10s elapsed]
aws_vpc.main: Creation complete after 12s [id=vpc-12345678]

aws_security_group.sg: Creating...
aws_instance.web: Creating...
aws_security_group.sg: Creation complete after 8s [id=sg-12345678]
aws_instance.web: Still creating... [15s elapsed]
aws_instance.web: Creation complete after 25s [id=i-12345678]

Apply complete! Resources: 3 added, 0 changed, 0 destroyed.
```

### Key Visual Elements

| Symbol | Meaning |
|--------|---------|
| `+` | Create |
| `~` | Update in-place |
| `-` | Destroy |
| `-/+` | Destroy and recreate |
| `[10s elapsed]` | Periodic timing heartbeat |
| `Creation complete after 12s` | Final timing |

### Patterns Worth Adopting
- **Heartbeat messages** ("Still creating... [10s elapsed]") -- prevents the user from thinking the process is stuck
- **Resource ID injection** as soon as available: `[id=vpc-12345678]`
- **Summary line** with counts: "3 added, 0 changed, 0 destroyed"
- Dependencies are **implicit from execution order**, not drawn

### Limitations
- No visual DAG structure in terminal (must use `terraform graph | dot` separately)
- Parallel tasks interleave unpredictably in log output
- No progress bars or completion percentages

---

## 2. GitHub Actions Local Runners (act)

### How It Works
`act` runs GitHub Actions workflows locally by parsing YAML and executing jobs. It displays job execution with timestamped INFO logs, handling `needs:` dependencies via sequential ordering.

### Terminal Output Format
```
[Build/build]   Run actions/checkout@v4
[Build/build]     Getting action ref...
[Build/build]   Run npm install
[Build/build]     | npm warn ...
[Build/build]   Run npm test
[Build/build]     | PASS src/test.js
[Build/build]   Job succeeded
```

### Key Elements
- **Job ID prefix** in brackets: `[Build/build]`
- Sequential execution respects `needs:` graph
- Parallel jobs show interleaved output with distinct prefixes
- Green/red colored status for success/failure
- Timestamped `INFO[0023]` entries

### Patterns Worth Adopting
- **Bracketed task prefix** on every line makes interleaved parallel output parseable
- Clear `Job succeeded` / `Job failed` terminal states

---

## 3. Nx Build System

### How It Works
Nx provides a **Task Progress Header** with live-updating task lines, spinners, and a summary footer. It groups tasks by project/target and respects dependency ordering.

### Terminal Output Format
```
> nx run-many --target=build --projects=app1,app2,lib

   +----- nx.json: 3/3 tasks (100%) ------+
(1/3) [done]  lib:build    (cached 2s)    |
(2/3) [spin]  app1:build   (1s)           |  deps: lib:build
(3/3) [spin]  app2:build   (0.5s)         |  deps: lib:build
   +---------------------------------------+

> lib:build (cached)
  > ng build lib --configuration=production
  Cache hit! Output replayed.

> app1:build
  > ng build app1 --configuration=production
  Compiling... 42 modules (0.8s)

[done] 3/3 tasks succeeded (cached: 1, ran: 2, 1.2s)
```

### Key Visual Elements

| Element | Description |
|---------|-------------|
| Braille spinners | Rotating `\u28BC \u28A6 \u28CB \u2819 \u2839 \u2838 \u28B4 \u28A6 \u2847 \u280F` for running tasks |
| Checkmark | Done/cached tasks |
| `(cached 2s)` | Cache hit indicator with original time |
| `deps: lib:build` | Dependency annotation |
| Progress fraction | `3/3 tasks (100%)` |
| Summary line | `cached: 1, ran: 2, 1.2s` |

### Patterns Worth Adopting
- **Compact task list** with spinner + name + elapsed + deps
- **Cache distinction** (cached vs ran) in both per-task and summary
- **Concurrency limit display** (only N spinners active at once)
- **Per-task output grouping** below the progress header

---

## 4. Concourse CI

### How It Works
Concourse's `fly` CLI (`fly execute`, `fly watch`) shows build plans as a sequential log with task status prefixes. Pipeline visualization is primarily web-based, but the CLI shows:

### Terminal Output Pattern
```
initializing
running build/task.yml
fetching resource: git-repo
  cloning...
  done
running test
  PASS: 42 tests
succeeded
```

### Patterns Worth Adopting
- Minimal but effective -- task name + status is sufficient for linear pipelines
- Web UI handles the complex DAG visualization; CLI stays simple

---

## 5. Airflow CLI

### How It Works
Airflow's CLI (`airflow dags show`, `airflow tasks list`) displays DAG structure as an ASCII tree with upstream/downstream relationships.

### Terminal Output Pattern
```
airflow tasks list my_dag --tree
<Task(BashOperator): start>
    <Task(PythonOperator): extract>
        <Task(PythonOperator): transform>
            <Task(BashOperator): load>
    <Task(PythonOperator): validate>
        <Task(BashOperator): load>
```

### Key Elements
- Tree indentation shows dependency depth
- Task type shown in parentheses
- State badges for task status (success/failed/running)
- `airflow dags show` can output DOT format for graphviz

### Patterns Worth Adopting
- **Tree indentation** is the simplest DAG representation
- Task type annotation helps distinguish verb types

---

## 6. Make / Just

### Make with -j Flag
```
make -j4 all
  CC      src/foo.o
  CC      src/bar.o
  CC      src/baz.o
  LD      bin/app
```

- Simple `[N/N]` running job counter
- Interleaved stdout from parallel processes
- No DAG visualization; ordering implicit from Makefile rules
- `--output-sync=target` groups output per target

### Just Task Runner
```
just build test deploy
  Running recipe 'build'...
  Running recipe 'test'...
  Running recipe 'deploy'...
```

- Minimal sequential logging
- No real-time progress or DAG display

### Patterns Worth Adopting
- Make's `--output-sync=target` pattern: buffer output per task, display grouped
- `[target]` prefix on interleaved lines

---

## 7. Dagger.io Pipeline Execution

### How It Works
Dagger (v0.6) introduced a TUI with split-pane layout: **tree view on top** showing the DAG as a collapsible hierarchy, **log output on bottom**. By v0.11-0.12, this shifted to OpenTelemetry-based traces with flame charts.

### TUI Layout (v0.6)
```
+------ Pipeline: build-and-test --------+
| > source                         [done] |
|   > build                     [running] |
|     > install-deps            [cached]  |
|     > compile                 [2.3s]    |
|   > test                      [pending] |
|     > unit-tests              [pending] |
|     > integration-tests       [pending] |
+-----------------------------------------+
| [LOG] compile: Building src/main.rs...  |
| [LOG] compile: Compiled 42 modules      |
+-----------------------------------------+
```

### Key Design Decisions
- **Library**: Custom Go implementation (previously "progrock" library), not Bubble Tea
- **Follow mode**: Auto-focuses on currently running step (toggle with `f`)
- **Browse mode**: Manual navigation with arrow keys
- **Collapse/expand**: Left/right arrows for tree nodes, `[`/`]` for all
- **v0.12 optimization**: Only renders visible region (important for large traces)
- **Tab**: Toggle focus between tree and log panels

### Patterns Worth Adopting
- **Collapsible tree view** for the DAG -- most intuitive for 5-50 tasks
- **Split pane** (DAG structure + task logs) is the gold standard
- **Follow mode** that auto-scrolls to the active task
- **Cache indicators** integrated into the tree
- **Post-execution persistence** -- TUI stays visible for review

---

## 8. Buck2 and Bazel

### Buck2
```
[build] 4/6 actions running  [====    ] 67%
  [done]    //src:lib          0.3s
  [running] //src:app          1.2s  [=====>     ]
  [running] //src:test         0.8s  [===>       ]
  [queued]  //deploy:prod      -     deps: app, test
```

- Live action graph with colored blocks (running=yellow, complete=green)
- Worker utilization bar: `[====    ] 4/6 workers`
- Layered progress grouped by build phases (coarse DAG layers)
- Real-time throughput: actions/sec

### Bazel
```
INFO: Analyzed 3 targets
[2/16] Compiling src/lib.rs
[3/16] Compiling src/main.rs
[4/16] Linking bin/app
INFO: Build completed successfully, 16 total actions
```

- Progress counter `[N/M]` at action level
- Target tree showing ready/running/completed
- Parallel actions highlighted with worker slots
- Color-coded dependency states (blue=queued, green=done)

### Patterns Worth Adopting
- **Worker utilization bar** shows parallelism at a glance
- **Action counter** `[N/M]` for global progress
- **Layered grouping** by DAG depth reveals bottlenecks

---

## 9. DAG Layer Parallel Execution Patterns

### Pattern Comparison Matrix

| Pattern | Pros | Cons | Best For |
|---------|------|------|----------|
| **Streaming logs** (Terraform) | Simple, pipeable, grep-friendly | No structure, interleaving chaos | Linear workflows, CI logs |
| **TUI tree + logs** (Dagger) | Rich, interactive, dep-aware | CPU overhead, non-scriptable | Interactive dev, 10-50 tasks |
| **Progress bars + list** (Nx) | Compact, clear aggregate progress | Limited dep visibility | Build tools with caching |
| **Layered topological** (Buck2) | Best for critical path analysis | Complex rendering, learning curve | Large DAGs (50+ tasks) |
| **Hybrid bar + list** (Bazel) | Good overview with detail | Screen flicker with redraws | CI/CD with medium parallelism |

### Recommended Approach for Nika (5-50 tasks)

**TUI with live tree updates** (Dagger-inspired) is optimal:
1. Tree view with collapsible task hierarchy grouped by DAG layer
2. Per-task status indicators (spinner/checkmark/cross)
3. Elapsed time per task
4. Log panel for selected task
5. Follow mode auto-tracking active task

---

## 10. Dependency Arrows in Terminal

### Box-Drawing Characters (Unicode U+2500-U+257F)

#### Lines
| Char | Code | Name | Use |
|------|------|------|-----|
| `\u2500` | U+2500 | Light horizontal | Edge lines |
| `\u2502` | U+2502 | Light vertical | Edge lines |
| `\u2501` | U+2501 | Heavy horizontal | Active/highlighted edges |
| `\u2503` | U+2503 | Heavy vertical | Active/highlighted edges |
| `\u2504` | U+2504 | Triple dash horizontal | Dashed/pending edges |
| `\u2506` | U+2506 | Triple dash vertical | Dashed/pending edges |

#### Corners (Sharp)
| Char | Code | Direction |
|------|------|-----------|
| `\u250C` | U+250C | Top-left (down-right) |
| `\u2510` | U+2510 | Top-right (down-left) |
| `\u2514` | U+2514 | Bottom-left (up-right) |
| `\u2518` | U+2518 | Bottom-right (up-left) |

#### Corners (Smooth/Rounded)
| Char | Code | Direction |
|------|------|-----------|
| `\u256D` | U+256D | Rounded top-left |
| `\u256E` | U+256E | Rounded top-right |
| `\u2570` | U+2570 | Rounded bottom-left |
| `\u256F` | U+256F | Rounded bottom-right |

#### T-Junctions and Crossings
| Char | Code | Use |
|------|------|-----|
| `\u251C` | U+251C | Left T-junction (branch right) |
| `\u2524` | U+2524 | Right T-junction (branch left) |
| `\u252C` | U+252C | Top T-junction (merge/fan-out) |
| `\u2534` | U+2534 | Bottom T-junction (fan-in) |
| `\u253C` | U+253C | Cross (edge crossing) |

#### Arrows
| Char | Code | Use |
|------|------|-----|
| `\u25BC` | U+25BC | Down arrow (dependency flow) |
| `\u25B6` | U+25B6 | Right arrow (horizontal dep) |
| `\u25C0` | U+25C0 | Left arrow (reverse) |
| `\u2192` | U+2192 | Light right arrow |
| `\u21D2` | U+21D2 | Double right arrow (strong dep) |
| `\u279C` | U+279C | Heavy right arrow |

### Edge State Encoding

| State | Line Style | Example |
|-------|-----------|---------|
| Pending/Inactive | Thin line `\u2502 \u2500` | Dependency exists but not executing |
| Active/Flowing | Heavy line `\u2503 \u2501` | Data currently flowing |
| Completed | Thin + green color | Dependency satisfied |
| Failed | Thin + red + dashed `\u2506` | Dependency failed |
| Animated | Alternating `\u257D \u2503 \u257F \u2502` | Pulsing flow effect |

### git log --graph Reference
```
*   1a2b3c4 (HEAD -> main) Merge branch 'feature'
|\
| * 5d6e7f8 (feature) Add feature
* | 9a0b1c2 Fix bug
|/
* 2d3e4f5 Initial commit
```
Uses: `*`, `|`, `/`, `\` -- minimal ASCII, works everywhere.

---

## 11. Gantt-Chart Elapsed Time Visualization

### Basic Horizontal Bar (Unicode Blocks)

| Char | Code | Name | Fill Level |
|------|------|------|------------|
| `\u2588` | U+2588 | Full block | 100% |
| `\u2589` | U+2589 | Left 7/8 block | 87.5% |
| `\u258A` | U+258A | Left 3/4 block | 75% |
| `\u258B` | U+258B | Left 5/8 block | 62.5% |
| `\u258C` | U+258C | Left half block | 50% |
| `\u258D` | U+258D | Left 3/8 block | 37.5% |
| `\u258E` | U+258E | Left 1/4 block | 25% |
| `\u258F` | U+258F | Left 1/8 block | 12.5% |
| `\u2591` | U+2591 | Light shade | Background/empty |
| `\u2592` | U+2592 | Medium shade | In-progress |
| `\u2593` | U+2593 | Dark shade | Nearly done |

### Waterfall/Timeline Layout

```
Timeline:  0s       5s       10s      15s      20s
           |--------|--------|--------|--------|
research   [============================]      12.3s
           |                            |
summarize  :        [==================]       8.1s
           :        |                  |
format     :        :     [============]       5.4s
           :        :     |            |
publish    :        :     :        [===]       2.1s
```

Key elements:
- Left-pad bars to show **start offset** relative to workflow start
- Bar width proportional to **elapsed duration**
- Duration label at end of bar
- Vertical dotted lines show dependency chains

### Gantt with Status Colors

```
  Task       Status    Elapsed     Timeline (0-30s)
  research   [done]    12.3s       [################...............]
  summarize  [running]  8.1s       [........########...............]
  format     [pending]  -.--       [..............................]
  publish    [pending]  -.--       [..............................]
```

Where:
- `#` / full blocks = completed time (green)
- `=` / medium shade = currently executing (yellow/amber)
- `.` / light shade = remaining/unused time (gray)

### Compact Single-Line Per Task

```
  research   [done] ==================== 12.3s
  summarize  [>>  ] =========            8.1s
  format     [wait]                      --
```

---

## 12. Rust Crates for Terminal DAG Rendering

### ascii-dag
- Purpose-built for terminal DAG rendering
- Zero dependencies
- Sugiyama-style hierarchical layout
- `Graph::from_edges()` for batch or builder API for incremental
- Edge routing: direct, L-shaped, side channels, multi-segment
- `compute_layout()` returns canvas dimensions and node positions
- O(1) node lookup via `node_by_id()`

### ratatui Ecosystem
- **tui-tree-widget**: Collapsible tree with `TreeItem`/`TreeState` -- good for DAG-as-tree
- **tui-nodes**: Node graph visualization with connections -- closest to full DAG widget
- **Custom Widget trait**: Most ratatui DAG UIs are custom implementations
- **tui-scrollview**: Needed for large DAGs that exceed viewport

### Nika's Current Implementation
Nika already has a solid foundation in `nika-tui/src/widgets/dag/`:
- `layout.rs`: Sugiyama-style layout with `NodePosition`, `LayoutConfig`, topological layering
- `edge.rs`: Full edge rendering with animated flow (`FLOW_FRAMES_V/H`), L-shaped routing, merge points, binding labels
- `ascii.rs`: `DagAscii` widget composing layout + nodes + edges
- `node_box.rs` / `node_data.rs`: Per-node boxes with verb colors and status

**What Nika already does well:**
- Sugiyama layout algorithm
- Animated flow on active edges (alternating `\u257D \u2503 \u257F \u2502`)
- Smooth and sharp corner styles
- Binding labels on edges (`{{with.data}}`)
- Data preview on edges
- Merge point rendering with `\u252C` junction
- Theme-aware colors
- Bounds checking for off-screen elements
- Content hash caching (PERF: avoids re-parsing at 60 FPS)

---

## 13. Recommended Patterns for Nika's DAG Execution View

### Architecture: Split-Pane TUI (Dagger-inspired)

```
+------ Workflow: research-and-summarize (3/4 tasks) --------+
| Layer 0 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ |
|   [done] research      infer  12.3s  [################]    |
| Layer 1 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ |
|   [>>  ] summarize     infer   8.1s  [========        ]    |
|   [>>  ] validate      exec    2.0s  [==              ]    |
| Layer 2 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ |
|   [wait] publish        fetch  --     [                ]    |
+-------------------------------------------------------------+
| [LOG] summarize: Generating summary from research data...   |
| [LOG] summarize: Processing 2.4k tokens...                  |
| [LOG] validate:  Running schema check...                    |
+-------------------------------------------------------------+
```

### Task Status Indicators

| Indicator | Meaning | Characters |
|-----------|---------|------------|
| `[done]` | Completed successfully | Green checkmark or `[done]` |
| `[>>  ]` | Currently executing | Animated spinner (`\u28BC \u28A6 \u28CB...`) or `[>> ]` |
| `[wait]` | Pending (deps not met) | Gray `[wait]` |
| `[fail]` | Failed | Red `[FAIL]` |
| `[skip]` | Skipped | Dim `[skip]` |
| `[cache]` | Cache hit | Cyan `[cache]` |

### Per-Task Line Format
```
  [status] task_id        verb    elapsed  [timeline_bar]
  [done]   research       infer   12.3s    [################]
```

Fields:
1. Status indicator (5 chars, colored)
2. Task ID (left-aligned, max 20 chars)
3. Verb type (5 chars, verb-colored: infer=purple, exec=green, fetch=blue, invoke=orange, agent=red)
4. Elapsed time (right-aligned, 6 chars)
5. Timeline bar (remaining width, Unicode blocks)

### Layer Separators
```
Layer 0 ~~~~~~~~  (parallel group)
Layer 1 ~~~~~~~~  (depends on Layer 0)
```

Use `~` or thin `\u2500\u2500\u2500` for layer separators. This shows topological depth and reveals which tasks can run in parallel.

### Edge Rendering Between Layers
For the static DAG preview (Studio view), keep the current Sugiyama layout. For the execution view, use the **flattened layer list** with optional dependency annotations:

```
  [done] research      infer  12.3s  -> summarize, validate
  [>>  ] summarize     infer   8.1s  <- research
  [>>  ] validate      exec    2.0s  <- research
  [wait] publish        fetch  --     <- summarize, validate
```

### Heartbeat Pattern (Terraform-inspired)
For long-running tasks, emit periodic status:
```
  [>>  ] research      infer  [15s elapsed, 1.2k tokens generated]
  [>>  ] research      infer  [30s elapsed, 2.8k tokens generated]
```

### Summary Footer
```
  4 tasks: 1 done, 2 running, 1 pending | Elapsed: 15.3s | Layer 1/2
```

---

## Sources

1. Terraform CLI documentation -- resource creation display format and DAG execution model
2. Nx documentation (nx.dev) -- parallel task execution TUI and output styles
3. Dagger.io release notes (v0.6, v0.11, v0.12) -- TUI evolution, progrock library, OpenTelemetry migration
4. Buck2 documentation (buck2.build) -- live action graph and worker utilization display
5. Bazel documentation (bazel.build) -- progress bar and action counter patterns
6. act (nektos/act) GitHub documentation -- local GitHub Actions runner output format
7. Unicode Standard, Box Drawing block (U+2500-U+257F) -- complete character reference
8. Unicode Standard, Block Elements (U+2580-U+259F) -- bar chart characters
9. ascii-dag crate documentation (crates.io) -- Rust DAG layout library
10. ratatui third-party widgets showcase -- tui-tree-widget, tui-nodes
11. Concourse CI fly CLI documentation -- build plan display
12. Apache Airflow CLI documentation -- `tasks list --tree` output format

---

## Methodology

- Tools used: Perplexity (sonar-pro) for web search, source code analysis of Nika TUI widgets
- Searches: 10 targeted queries across all major tools
- Direct code review of Nika's existing `widgets/dag/` implementation
- Cross-referenced patterns across 10+ tools

---

## Further Research Suggestions

1. **Dagger progrock source code** (Go) -- study the original tree TUI implementation before it was deprecated
2. **Buck2 TUI source** (Rust) -- Facebook's open-source build tool has one of the best terminal DAG UIs
3. **Turbopack/Turborepo** terminal output -- Vercel's Rust-based build tool with task graph display
4. **Buildkite CLI** -- another CI tool with interesting terminal pipeline visualization
5. **ascii-dag crate** -- evaluate for potential integration or pattern adoption in Nika
6. **tui-nodes crate** -- evaluate for ratatui-native graph rendering
7. **Flame chart in terminal** -- Dagger v0.12's approach to trace visualization could inspire a "timeline view"
