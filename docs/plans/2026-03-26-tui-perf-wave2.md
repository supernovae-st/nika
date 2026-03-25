# TUI Performance Wave 2 — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.
> Launch a swarm of 10 agents to investigate, then execute fixes in batches.

**Goal:** Eliminate all remaining per-frame allocations in the Nika TUI — chat message caching, DAG layout caching, char-to-string border fixes, static help overlay, header format elimination, security hardening.

**Architecture:** Cache-first rendering — compute once on data change, render from cache every frame. Replace `char.to_string()` with direct `cell.set_char()`. Guard expensive operations behind dirty flags or data-change detection.

**Tech Stack:** Rust, ratatui 0.30, tokio, crossterm

**Prior Work:** Wave 1 shipped 10 commits covering: Abyss Dark theme, startup I/O (300-900ms saved), DirtyFlags render guard, StarField pre-compute, TaskBox zero-copy Widget, native-inference removal (-35MB binary). All verified with 2111 tests passing.

**Wave 2 Findings Source:** 10 expert agents (rust-pro, rust-architect, rust-perf, rust-async-expert, rust-security, code-reviewer) audited the 86K-line TUI codebase in parallel.

---

## Batch A: DAG Widget — Cache Layout + Kill char.to_string() (HIGH impact)

### Task 1: Cache DagLayout in DagAscii widget

**Files:**
- Modify: `tools/nika-tui/src/widgets/dag/ascii.rs:140-353`
- Modify: `tools/nika-tui/src/widgets/dag/ascii.rs:62-64` (HashMap → FxHashMap)

**Problem:** `DagLayout::compute()` runs topological sort + barycenter ordering + position calculation every frame. `compute_node_widths()` (line 140) clones every node ID into a new FxHashMap every frame. For a 10-node DAG, hundreds of String clones + HashMap allocs per frame.

**Step 1: Add layout cache fields to DagAscii**

```rust
// In DagAscii struct, add:
cached_layout: Option<DagLayout>,
cached_node_widths: Option<FxHashMap<String, u16>>,
layout_node_count: usize,  // invalidation key
```

**Step 2: Gate layout computation on node count change**

```rust
// In render(), replace unconditional DagLayout::compute with:
let node_count = self.nodes.len();
if self.cached_layout.is_none() || self.layout_node_count != node_count {
    let node_widths = self.compute_node_widths();
    let layout = DagLayout::compute(&layout_nodes, &config, Some(&node_widths));
    self.cached_node_widths = Some(node_widths);
    self.cached_layout = Some(layout);
    self.layout_node_count = node_count;
}
let layout = self.cached_layout.as_ref().unwrap();
```

**Step 3: Replace `HashMap::new()` with `FxHashMap::default()` at lines 62-64**

```rust
// BEFORE:
dependencies: HashMap::new(),
bindings: HashMap::new(),
previews: HashMap::new(),

// AFTER:
dependencies: FxHashMap::default(),
bindings: FxHashMap::default(),
previews: FxHashMap::default(),
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- widgets::dag
```

**Step 5: Commit**

```bash
git add nika-tui/src/widgets/dag/ascii.rs
git commit -m "perf(tui): cache DagLayout + use FxHashMap in DAG widget"
```

---

### Task 2: Replace char.to_string() with cell.set_char() in DAG borders

**Files:**
- Modify: `tools/nika-tui/src/widgets/dag/node_box.rs:284-475` (12 instances)
- Modify: `tools/nika-tui/src/widgets/dag/edge.rs:242-353` (4 instances)

**Problem:** Every border character does `char.to_string()` = heap allocation. 12 calls in node_box + 4 in edge = 16 allocs per node/edge. For 10-node DAG with 8 edges = ~200 allocs/frame.

**Step 1: In node_box.rs, replace all `buf.set_string(x, y, char.to_string(), style)` with direct cell access**

Pattern for each of the 12 instances:

```rust
// BEFORE (e.g., line 284):
buf.set_string(area.x, area.y, border_chars.tl.to_string(), border_render_style);

// AFTER:
if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(area.x, area.y)) {
    cell.set_char(border_chars.tl);
    cell.set_style(border_render_style);
}
```

Apply to all 12 instances at lines: 284, 288, 293, 302, 378, 392, 411, 424, 454, 466, 470, 475.

**Step 2: In edge.rs, same pattern for 4 instances at lines: 242, 276, 315, 353**

```rust
// BEFORE:
buf.set_string(corner_x, y, line_char.to_string(), style);

// AFTER:
if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(corner_x, y)) {
    cell.set_char(line_char);
    cell.set_style(style);
}
```

**Step 3: Run tests**

```bash
cargo test -p nika-tui --lib -- widgets::dag
```

**Step 4: Commit**

```bash
git add nika-tui/src/widgets/dag/
git commit -m "perf(tui): replace char.to_string() with cell.set_char() in DAG borders"
```

---

## Batch B: Help Overlay + Header — Cache Static Content (HIGH impact)

### Task 3: Cache help overlay content (static, never changes)

**Files:**
- Modify: `tools/nika-tui/src/widgets/help_overlay.rs:183-211`

**Problem:** `build_content()` allocates Vec + ~50 format!() calls every frame while help is visible. `HELP_SECTIONS` is `const` — content never changes.

**Step 1: Add cached content field to HelpOverlayState**

```rust
// In HelpOverlayState struct, add:
cached_content: Option<Vec<Line<'static>>>,
```

**Step 2: Build once, reuse forever**

```rust
// In the render path, replace:
// let content = self.build_content();
// With:
if self.state.cached_content.is_none() {
    self.state.cached_content = Some(self.build_content());
}
let content = self.state.cached_content.as_ref().unwrap();
```

**Step 3: Invalidate on toggle (visibility change resets scroll, not content)**

No invalidation needed — content is truly static.

**Step 4: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- widgets::help_overlay
git add nika-tui/src/widgets/help_overlay.rs
git commit -m "perf(tui): cache static help overlay content"
```

---

### Task 4: Eliminate header format!() with static tab labels

**Files:**
- Modify: `tools/nika-tui/src/widgets/header.rs:110-116`

**Problem:** `format!(" {}:{} ", num, name)` per tab per frame (3 tabs = 6 format!() calls).

**Step 1: Replace format!() with const static strings**

```rust
// BEFORE (lines 110, 116):
format!(" {}:{} ", num, name)  // active
format!("{}:{}", num, name)    // inactive

// AFTER: Use pre-computed static strings
const TAB_LABELS_ACTIVE: [&str; 3] = [" 1:Studio ", " 2:Command ", " 3:Control "];
const TAB_LABELS_INACTIVE: [&str; 3] = ["1:Studio", "2:Command", "3:Control"];

// In the loop, index by view:
let label = if is_active {
    TAB_LABELS_ACTIVE[view.index()]
} else {
    TAB_LABELS_INACTIVE[view.index()]
};
```

Requires `TuiView::index()` method (already exists or add: `fn index(&self) -> usize`).

**Step 2: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- widgets::header
git add nika-tui/src/widgets/header.rs
git commit -m "perf(tui): static tab labels in header (zero format!)"
```

---

## Batch C: Chat View — Guard + Cache (CRITICAL impact)

### Task 5: Guard build_line_positions with selection check

**Files:**
- Modify: `tools/nika-tui/src/views/chat/messages/mod.rs:103-151`

**Problem:** `build_line_positions()` runs unconditionally every frame, allocating `ChatLinePosition` with `text: line_text.to_string()` for every visible line. Only needed when mouse selection is active.

**Step 1: Add guard at the top of build_line_positions**

```rust
fn build_line_positions(&mut self, area: Rect) {
    // PERF: Only build when selection is active
    if !self.is_selecting && self.text_selection.is_none() {
        self.line_positions.clear();
        return;
    }
    // ... existing logic
}
```

**Step 2: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- views::chat
git add nika-tui/src/views/chat/messages/mod.rs
git commit -m "perf(tui): guard build_line_positions behind selection check"
```

---

### Task 6: Cache rendered chat messages (incremental rebuild)

**Files:**
- Modify: `tools/nika-tui/src/views/chat/messages/mod.rs:87`
- Modify: `tools/nika-tui/src/views/chat/mod.rs` (add cache fields)

**Problem:** `build_message_items()` iterates ALL messages, calls `wrap_text()` on every body, creates Span/Line/ListItem for all lines — every frame at 10-60fps. For 100 messages = 100 word-wrap runs per second.

**Step 1: Add cache fields to ChatView**

```rust
// In ChatView struct:
cached_items: Vec<ListItem<'static>>,
cached_items_count: usize,       // number of messages when cache was built
cached_items_width: u16,          // content_width when cache was built
cached_streaming_len: usize,      // length of last streaming message
```

**Step 2: Only rebuild when data changes**

```rust
// In render_messages_v2(), replace:
// let mut items = self.build_message_items(theme, &colors, content_width, &sel_ctx);
// With:
let msg_count = self.messages.len();
let streaming_len = self.messages.last()
    .map(|m| m.content.len())
    .unwrap_or(0);
let needs_rebuild = self.cached_items_count != msg_count
    || self.cached_items_width != content_width
    || (self.is_streaming && self.cached_streaming_len != streaming_len);

if needs_rebuild {
    self.cached_items = self.build_message_items(theme, &colors, content_width, &sel_ctx);
    self.cached_items_count = msg_count;
    self.cached_items_width = content_width;
    self.cached_streaming_len = streaming_len;
}
let items = &self.cached_items;
```

**Step 3: Handle streaming — only rebuild last message**

For the streaming case, ideally rebuild only the last ListItem. But as a first pass, rebuild all when streaming content changes. Optimize later.

**Step 4: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- views::chat
git add nika-tui/src/views/chat/
git commit -m "perf(tui): cache rendered chat messages (rebuild on data change only)"
```

---

## Batch D: StatusBar Span Splits (MEDIUM impact, ~14 allocs eliminated)

### Task 7: Zero-alloc StatusBar span splits

**Files:**
- Modify: `tools/nika-tui/src/widgets/status_bar.rs`

**Problem:** ~14 `format!()` calls per frame in StatusBar::render(). Many can be eliminated by splitting into multiple Spans or using static strings.

**Fixes (apply all in one task):**

1. **Mode indicator** (line 505): `format!("[{}]", mode_char)` → static match `"[N]"` / `"[I]"` / `"[/]"`
2. **Error code** (line 605): `format!("[{}]", code)` → 3 spans: `"["` + `code` + `"]"`
3. **Hint brackets** (line 743): `format!("[{}]", &*hint.key)` → 3 spans: `"["` + `hint.key` + `"]"`
4. **Hint action** (line 750): `hint.action.to_string()` → `hint.action` (pass Cow directly)
5. **Custom text** (line 543): `text.clone()` → move `text` out of `self` (render consumes self)

**Step 1: Apply all 5 span-split fixes**

**Step 2: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- widgets::status_bar
git add nika-tui/src/widgets/status_bar.rs
git commit -m "perf(tui): zero-alloc StatusBar spans (14 format! eliminated)"
```

---

## Batch E: Security Hardening (IMPORTANT)

### Task 8: Cancel background tasks in Drop impl

**Files:**
- Modify: `tools/nika-tui/src/app/mod.rs:612-619` (Drop impl)

**Problem:** On error-path exit, `cancel_background_tasks()` is skipped. Background tasks leak, `PROVIDER_VERIFICATION_RUNNING` AtomicBool can get stuck.

**Step 1: Add cancel_background_tasks to Drop**

```rust
impl Drop for App {
    fn drop(&mut self) {
        // Cancel background tasks BEFORE terminal cleanup
        self.cancel_background_tasks();
        if self.terminal.is_some() {
            let _ = self.cleanup();
        }
    }
}
```

**Step 2: Reset AtomicBool guard in cancel_background_tasks**

```rust
// In cancel_background_tasks(), add at the end:
PROVIDER_VERIFICATION_RUNNING.store(false, Ordering::SeqCst);
```

**Step 3: Run tests + commit**

```bash
cargo test -p nika-tui --lib
git add nika-tui/src/app/mod.rs nika-tui/src/app/lifecycle.rs
git commit -m "fix(tui): cancel background tasks on all exit paths"
```

---

### Task 9: Add symlink guard to tree browser

**Files:**
- Modify: `tools/nika-tui/src/widgets/tree/node.rs:204-243`

**Problem:** Tree browser follows symlinks (uses `std::fs::read_dir` → `entry.path()` which resolves symlinks). Can expose files outside project root or cause performance issues with symlinks to large dirs.

**Step 1: Skip symlinks in load_children**

```rust
// In load_children(), after getting DirEntry, add:
if let Ok(file_type) = entry.file_type() {
    if file_type.is_symlink() {
        continue;  // Skip symlinks — security + performance
    }
}
```

**Step 2: Run tests + commit**

```bash
cargo test -p nika-tui --lib -- widgets::tree
git add nika-tui/src/widgets/tree/node.rs
git commit -m "fix(tui): skip symlinks in tree browser (security hardening)"
```

---

### Task 10: Document panic=abort LockfileGuard limitation

**Files:**
- Modify: `tools/nika-engine/src/runtime/runner.rs:59-85`

**Problem:** `panic = "abort"` in release profile means `Drop` for `LockfileGuard` is not called on panic. The comment claims all exit paths are covered.

**Step 1: Update LockfileGuard comment**

```rust
/// NOTE: With `panic = "abort"` (release profile), panics bypass Drop.
/// The lockfile includes a timestamp — callers should treat locks older
/// than 10 minutes as stale.
```

**Step 2: Add TTL check to lockfile acquisition**

```rust
// When checking for existing lockfile, also check age:
if lock_path.exists() {
    if let Ok(metadata) = lock_path.metadata() {
        if let Ok(modified) = metadata.modified() {
            if modified.elapsed().unwrap_or_default() > Duration::from_secs(600) {
                tracing::warn!("Removing stale lockfile (>10min old)");
                let _ = std::fs::remove_file(&lock_path);
            }
        }
    }
}
```

**Step 3: Run tests + commit**

```bash
cargo test -p nika-engine --lib -- runtime::runner
git add nika-engine/src/runtime/runner.rs
git commit -m "fix(runtime): handle stale lockfiles from panic=abort"
```

---

## Batch F: Async Startup (MEDIUM, architectural)

### Task 11: Move git status to spawn_blocking

**Files:**
- Modify: `tools/nika-tui/src/views/studio/mod.rs:124-149`
- Modify: `tools/nika-tui/src/app/mod.rs` (add BackgroundResult channel)

**Problem:** `build_git_status_cache()` spawns `git status` subprocess (100-300ms blocking). Currently runs in constructor. The Tokio runtime IS alive at this point.

**Step 1: Add BackgroundResult enum + channel to App**

```rust
pub enum BackgroundResult {
    GitStatusReady(FxHashMap<String, GitStatus>),
    TreeReady(TreeNode),
}
// In App: pub(crate) bg_rx: mpsc::Receiver<BackgroundResult>,
```

**Step 2: Fire git status in background from with_root()**

```rust
// Pass bg_tx into with_root, spawn git + tree in background
let bg_tx_clone = bg_tx.clone();
let root = root_dir.clone();
tokio::task::spawn_blocking(move || {
    let root_path = Utf8Path::from_path(&root).unwrap_or(Utf8Path::new("."));
    let cache = build_git_status_cache(root_path);
    let _ = bg_tx_clone.blocking_send(BackgroundResult::GitStatusReady(cache));
});
```

**Step 3: Poll bg_rx in event loop, update StudioView when results arrive**

**Step 4: Run tests + commit**

```bash
cargo test -p nika-tui --lib
git add nika-tui/src/
git commit -m "perf(tui): move git status to spawn_blocking (non-blocking startup)"
```

---

## Verification

After all tasks:
1. `cargo check -p nika-tui` — compiles clean
2. `cargo test -p nika-tui --lib` — 2111+ tests pass
3. `cargo test --workspace --lib` — no regressions
4. `cargo clippy -p nika-tui` — no warnings
5. Manual: `nika ui` — startup is instant, stars twinkle, DAG renders, help overlay works

---

## Execution Prompt for Next Session

Copy this prompt to start a new Claude Code session:

```
Tu es en mode orchestration performance pour la TUI Nika.

CONTEXTE:
- 86K lignes de Rust, ratatui 0.30, tokio
- Wave 1 a shippe 10 commits (startup, DirtyFlags, StarField, TaskBox zero-copy, binary size)
- Wave 2 plan at docs/plans/2026-03-26-tui-perf-wave2.md

MISSION:
1. Lis le plan Wave 2 complet
2. Lance un swarm de 10 agents en parallele:
   - 3x rust-pro: Tasks 1-2 (DAG cache + char fix), Task 3-4 (help+header), Task 7 (StatusBar)
   - 1x rust-architect: Task 6 (chat message cache — le plus complexe)
   - 1x rust-security: Tasks 8-10 (Drop fix, symlinks, lockfile)
   - 1x rust-async-expert: Task 11 (spawn_blocking startup)
   - 2x rust-perf: Bug hunting — cherche TOUS les faux positifs, .clone() inutiles,
     format!() dans les hot paths, unwrap() dangereux dans render
   - 1x code-reviewer: Review chaque batch apres implementation
   - 1x rust-pro: Cherche des nouvelles optimisations pas encore trouvees
3. Execute le plan task par task avec 1 FIX = 1 COMMIT
4. Apres chaque batch: cargo test, cargo clippy, commit, push
5. A la fin: rapport consolide avec metriques avant/apres

REGLES:
- ZERO backward compat (0 users = 0 compat)
- Commit format: type(scope): description + co-authors
- 2111+ tests doivent passer a chaque commit
- Rust idiomatique: &T > T.clone(), Cow > String, SmallVec > Vec
- Ne jamais deviner — lire le code avant de modifier
```
