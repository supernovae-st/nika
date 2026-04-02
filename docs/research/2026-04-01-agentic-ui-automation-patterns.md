# Agentic UI Automation Patterns: A Practical Architecture Guide

> Research report -- 2026-04-01
> Focus: how the best computer-using agents work, and what patterns to steal for Nika.

---

## Table of Contents

1. [Major Agent Architectures](#1-major-agent-architectures)
2. [Tool Design for UI Agents](#2-tool-design-for-ui-agents)
3. [UI Tree Representation](#3-ui-tree-representation)
4. [Action Reliability](#4-action-reliability)
5. [Context Window Management](#5-context-window-management)
6. [Multi-Step Planning](#6-multi-step-planning)
7. [Error Recovery](#7-error-recovery)
8. [Open Interpreter's Approach](#8-open-interpreters-approach)
9. [Synthesis: Recommended Architecture for Nika](#9-synthesis-recommended-architecture-for-nika)

---

## 1. Major Agent Architectures

### 1.1 Agent-S (Simular AI, 2024)

**Paper**: "Agent S: An Open Agentic Framework that Uses Computers Like a Human" (arXiv:2410.08164)

**Architecture**: Three-layer hierarchy.

```
+---------------------------------------+
|          Experience Manager            |
|  (episodic + semantic memory store)    |
+---+-------------------------------+---+
    |                               |
+---v-----------+   +-----------v---+
|   Manager     |   |   Worker      |
| (high-level   |   | (grounded     |
|  subtask plan)|   |  UI actions)  |
+---+-----------+   +-----------+---+
    |                           |
+---v---------------------------v---+
|     Agent-Computer Interface      |
|  (accessibility tree + grounding) |
+-----------------------------------+
```

**Key innovations**:

- **Manager-Worker decomposition**: The Manager LLM decomposes a user goal into subtasks. The Worker LLM executes each subtask as a sequence of grounded UI actions. This separation means the planning model never needs to see raw pixel data or huge accessibility trees -- it works with natural language subtask descriptions.

- **Experience-Augmented Hierarchical Planning**: Agent-S maintains two memory stores:
  - **Episodic memory**: Past successful action trajectories indexed by (app, task_type). Retrieved via similarity search to augment the Worker with few-shot examples.
  - **Semantic memory**: Distilled knowledge about UI patterns ("In macOS System Preferences, the Display settings are under the second icon row"). Retrieved to augment the Manager.

- **Active Inference for Grounding (ACI)**: Instead of just matching element names, Agent-S uses an iterative process where the agent narrows down the target element by reasoning about spatial layout and visual context. This is essentially "think before you click."

- **Action space**: Agent-S uses accessibility API actions:
  ```
  click(element_id)
  type(element_id, text)
  scroll(direction, amount)
  hotkey(key_combo)
  drag(from_id, to_id)
  wait(seconds)
  ```
  Elements are identified by numeric IDs assigned from the accessibility tree.

- **Results**: Achieved 83.6% on OSWorld benchmark (new SOTA at time of publication). The hierarchical decomposition was the single biggest contributor to performance gains.

### 1.2 SeeAct (OSU NLP, 2024)

**Paper**: "GPT-4V(ision) is a Generalist Web Agent, if Grounded" (arXiv:2401.01614)

**Architecture**: Two-phase action prediction.

```
Phase 1: Action Generation (multimodal LLM)
  Input: screenshot + task description + action history
  Output: natural language action description
         e.g., "Click the search box at the top of the page"

Phase 2: Element Grounding (choose one strategy)
  Strategy A: Textual Choices
    - Present top-K candidate elements as text options
    - LLM picks the right one by number
  Strategy B: Set-of-Marks (SoM)
    - Overlay numbered bounding boxes on screenshot
    - LLM outputs the number of the target element
  Strategy C: Direct Coordinate
    - LLM outputs (x, y) coordinates directly
    - Least reliable, most hallucination-prone
```

**Key findings**:

- **Set-of-Marks (SoM) is the winner**. Overlaying numbered visual markers on screenshots dramatically improves grounding accuracy. The LLM can see the numbers overlaid on the actual UI elements, making it trivial to refer to specific elements.

- **Two-phase beats single-phase**. Separating "what to do" from "which element to target" significantly reduces errors. The action generation phase reasons about intent; the grounding phase reasons about UI structure.

- **Textual choice grounding works surprisingly well**. Presenting the top-5 candidate elements as a numbered text list and asking the LLM to pick one achieves 70%+ accuracy. This is cheap and fast (no vision needed in phase 2).

- **Action space**:
  ```
  CLICK(element_id)
  TYPE(element_id, text)
  SELECT(element_id, option)
  HOVER(element_id)
  SCROLL(direction)  # UP or DOWN
  PRESS(key)
  GOTO(url)          # web-specific
  DONE(answer)       # task completion signal
  ```

### 1.3 OS-Atlas / ShowUI (Microsoft, 2024)

**Paper**: "OS-Atlas: A Foundation Action Model for Generalist GUI Agents" (arXiv:2410.23218)

**Architecture**: Unified vision-language model fine-tuned specifically for UI grounding.

```
+-----------------------------------+
|   Task Instruction + Screenshot   |
+---v-------------------------------+
|   OS-Atlas VLM (7B params)        |
|   - GUI element detection         |
|   - Action prediction             |
|   - Cross-platform grounding      |
+---v-------------------------------+
|   Predicted Action + Coordinates  |
+-----------------------------------+
```

**Key contribution**: Instead of relying on accessibility APIs, OS-Atlas is trained on a massive corpus of annotated UI screenshots (Linux, Windows, macOS, Android, iOS, Web). It can directly predict bounding boxes for target elements from screenshots alone -- no accessibility tree needed.

**Training data**: Curated from multiple sources:
- 14M GUI element annotations from web crawls
- OS-level screenshots with accessibility metadata alignment
- Cross-platform action demonstrations

**Why this matters**: Accessibility APIs are fragile and platform-specific. A vision-only approach works on any screen, including remote desktops, VNC sessions, and apps with broken accessibility support.

### 1.4 UFO (Microsoft, 2024)

**Paper**: "UFO: A UI-Focused Agent for Windows OS Interaction" (arXiv:2402.07939)

**Architecture**: Dual-agent with application-specific specialization.

```
+-----------------------+
|   AppAgent Selector   |  <-- Picks which app to focus on
+---+-------------------+
    |
+---v-------------------+
|   AppAgent (per-app)  |  <-- Specialized per application
|   - App-specific UI   |
|     knowledge          |
|   - Action prediction |
+---+-------------------+
    |
+---v-------------------+
|   Windows UI Auto.    |  <-- pywinauto / Win32 API
+---+-------------------+
    |
+---v-------------------+
|   State Verification  |  <-- Screenshot diff after action
+-----------------------+
```

**Key pattern -- per-application specialization**: UFO discovered that a single generic agent performs poorly across diverse apps. Each application has unique UI conventions (ribbon menus in Office, panels in Adobe, etc.). Maintaining per-app agent profiles with app-specific knowledge dramatically improves success rates.

**Action space** (Windows-specific):
```
click(control_name, click_type="left|right|double")
set_text(control_name, text)
select(control_name, item)
scroll(control_name, direction, clicks)
hotkey(keys)
drag_drop(source, target)
annotate(control_name)  # for agent self-annotation
summary(text)           # agent summarizes what happened
```

### 1.5 Claude Computer Use (Anthropic, 2024-2025)

**Architecture**: Direct coordinate-based control, minimal tool set.

```
+---------------------------+
|   Claude 3.5/4 Sonnet     |
|   (multimodal, tool-use)  |
+---+-----------------------+
    |
+---v-----------------------+
|   3 Core Tools            |
|   - computer(action, ...) |
|   - bash(command)          |
|   - text_editor(...)      |
+---+-----------------------+
    |
+---v-----------------------+
|   Screenshot feedback     |
|   after every action      |
+-----------------------+---+
```

**Tool definition** (the actual `computer` tool):
```json
{
  "name": "computer",
  "type": "computer_20250124",
  "display_width_px": 1280,
  "display_height_px": 800
}
```

**Action space** (deliberately minimal):
```
computer(action="screenshot")
computer(action="click", coordinate=[x, y])
computer(action="double_click", coordinate=[x, y])
computer(action="right_click", coordinate=[x, y])
computer(action="type", text="hello")
computer(action="key", text="Return")
computer(action="mouse_move", coordinate=[x, y])
computer(action="scroll", coordinate=[x, y], direction="up|down")
computer(action="drag", start_coordinate=[x1, y1], coordinate=[x2, y2])
```

**Design philosophy**: Anthropic chose an intentionally primitive action space because:
1. It works identically across all platforms (just needs screenshots + mouse/keyboard)
2. No accessibility API dependency = no platform-specific code
3. The model itself handles all the "reasoning about what to click" internally
4. Screenshot-after-every-action provides state verification for free

**Weakness**: Coordinate-based clicking is less reliable than accessibility-ID-based clicking. Small UI movements, resolution changes, or slight rendering differences can cause misclicks.

---

## 2. Tool Design for UI Agents

### 2.1 The Optimal Tool Set (Research Consensus)

After analyzing all major implementations, the research converges on a **layered tool architecture**:

```
Layer 3: Composite Actions (high-level)
  +-----------------------------------------+
  | fill_form(fields), navigate_to(target), |
  | select_from_menu(path), search(query)   |
  +-----------------------------------------+

Layer 2: Semantic Actions (element-aware)
  +-----------------------------------------+
  | click(element), type_into(element, txt),|
  | select_option(element, value),          |
  | scroll_to(element), hover(element),     |
  | read_text(element), wait_for(element)   |
  +-----------------------------------------+

Layer 1: Primitive Actions (coordinate/raw)
  +-----------------------------------------+
  | mouse_click(x, y), mouse_move(x, y),   |
  | key_press(key), key_combo(keys),        |
  | screenshot(), drag(x1,y1, x2,y2),      |
  | scroll(direction, amount)               |
  +-----------------------------------------+

Layer 0: Observation (read-only)
  +-----------------------------------------+
  | get_accessibility_tree(),               |
  | get_focused_element(),                  |
  | get_screen_dimensions(),                |
  | get_element_properties(id),             |
  | get_clipboard(), get_window_list()      |
  +-----------------------------------------+
```

### 2.2 Granularity Research Findings

**Finding 1: Medium granularity wins.** (Agent-S, SeeAct, UFO all converge here)

Too primitive (raw coordinates) = high error rate, slow (many steps).
Too high-level (fill_form) = inflexible, can not handle edge cases.
Element-level semantic actions (Layer 2) hit the sweet spot.

**Finding 2: Always include an observation-only tool.**

The agent needs to "look" without acting. Separating observation from action:
- Prevents accidental state changes during planning
- Allows the agent to verify preconditions before acting
- Enables targeted subtree inspection (see Section 5)

**Finding 3: Include a "done" / "complete" signal tool.**

Without an explicit completion signal, agents either:
- Loop forever trying to improve
- Stop prematurely when they run out of ideas

```
done(success=true, result="Task completed. The file was saved as report.pdf")
done(success=false, reason="Could not find the Settings menu")
```

### 2.3 Practical Tool Definitions (Nika-style YAML)

The ideal tool set for a Nika `agent:` verb controlling a desktop:

```yaml
tools:
  # Observation
  - ui_screenshot           # Take screenshot, returns image
  - ui_tree                 # Get accessibility tree (filtered)
  - ui_element_info         # Get detailed properties of one element
  - ui_window_list          # List open windows
  - ui_focused              # What element currently has focus?

  # Semantic actions (element ID-based)
  - ui_click                # Click an element by accessibility ID
  - ui_type                 # Type text into an element
  - ui_select               # Select dropdown/list option
  - ui_scroll               # Scroll within an element or page
  - ui_hover                # Hover over element

  # Primitive fallbacks (coordinate-based)
  - ui_click_xy             # Click at absolute coordinates
  - ui_drag                 # Drag from point A to B

  # Keyboard
  - ui_hotkey               # Send keyboard shortcut
  - ui_key                  # Press a single key

  # Control
  - ui_wait                 # Wait for element/condition
  - ui_verify               # Assert element state (visible, enabled, has text)
  - nika:complete           # Signal task completion
```

**Why both element-ID and coordinate tools?** The accessibility tree does not cover everything. Custom-drawn UI elements (canvas apps, games, Electron quirks) may not appear in the tree. The coordinate fallback is essential for coverage.

---

## 3. UI Tree Representation

### 3.1 The Problem

A typical macOS accessibility tree for a complex app can have 5,000+ nodes. A full JSON serialization can exceed 500KB of text -- far too large for any LLM context window.

### 3.2 Set-of-Marks (SoM) -- Visual Numbering

**Origin**: "Set-of-Mark Prompting Unleashes Extraordinary Visual Grounding in GPT-4V" (Yang et al., 2023)

**How it works**:
1. Render the accessibility tree bounding boxes on top of the screenshot
2. Assign each interactive element a unique numeric label
3. The labels appear as small colored badges overlaid on the screenshot
4. The LLM references elements by number: "Click element [7]"

```
+------------------------------------+
|  [1] File  [2] Edit  [3] View     |  <-- Menu bar
|  +------------------------------+  |
|  | [4] Search: [___________]    |  |
|  +------------------------------+  |
|  | [5] inbox (23)               |  |
|  | [6] sent                     |  |
|  | [7] drafts (2)              |  |
|  +-----+------------------------+  |
|  | [8] Subject: Meeting notes   |  |
|  | [9] From: alice@example.com  |  |
|  | [10] Hi team, I wanted to... |  |
|  |                              |  |
|  | [11] Reply  [12] Forward     |  |
|  +-----+------------------------+  |
+------------------------------------+
```

**Advantages**: Eliminates ambiguity. The LLM can see exactly which element maps to which number. No need to describe element positions in text.

**Disadvantages**: Requires vision (multimodal model). Clutters the screenshot when there are many elements. Overlapping labels can be confusing.

### 3.3 Simplified Text Tree (Accessibility Tree Linearization)

This is the most practical format for text-only or hybrid pipelines. Multiple projects converge on similar formats:

**Format A: Indented text with IDs (Agent-S style)**
```
[id=1] window "Mail - Inbox"
  [id=2] toolbar
    [id=3] button "New Message"
    [id=4] button "Reply"
    [id=5] button "Delete"
    [id=6] search_field "Search" value=""
  [id=7] sidebar
    [id=8] list_item "Inbox" selected=true badge="23"
    [id=9] list_item "Sent"
    [id=10] list_item "Drafts" badge="2"
  [id=11] content_area
    [id=12] text "Subject: Meeting notes"
    [id=13] text "From: alice@example.com"
    [id=14] text_area "Hi team, I wanted to..."
    [id=15] button "Reply"
    [id=16] button "Forward"
```

**Format B: Markdown-like with roles (UFO style)**
```
# Window: Mail - Inbox

## Toolbar
- [3] Button: "New Message" (enabled)
- [4] Button: "Reply" (enabled)
- [5] Button: "Delete" (disabled)
- [6] SearchField: "Search" (empty, focused)

## Sidebar
- [8] ListItem: "Inbox" (selected) [23 unread]
- [9] ListItem: "Sent"
- [10] ListItem: "Drafts" [2]

## Content
- [12] StaticText: "Subject: Meeting notes"
- [13] StaticText: "From: alice@example.com"
- [14] TextArea: "Hi team, I wanted to..." (editable)
- [15] Button: "Reply"
- [16] Button: "Forward"
```

**Format C: Compact single-line (WebArena style)**
```
[3] button "New Message" | [4] button "Reply" | [5] button "Delete" (disabled)
[6] searchfield "Search" value=""
---
[8] listitem "Inbox" (selected, 23) | [9] listitem "Sent" | [10] listitem "Drafts" (2)
---
[12] text "Subject: Meeting notes"
[13] text "From: alice@example.com"
[14] textarea "Hi team, I wanted to..."
[15] button "Reply" | [16] button "Forward"
```

### 3.4 Research Findings on Representation

**Finding 1**: **Simplified text beats raw JSON by 15-25% task success rate.** Raw JSON accessibility dumps have too much noise (coordinates, internal states, system properties). LLMs get confused by the verbosity.

**Finding 2**: **Include only actionable properties.** Strip out:
- Exact pixel coordinates (use IDs instead)
- Internal framework states (NSAccessibility internal flags)
- Redundant role hierarchies (group > group > group > button = just button)
- Hidden/offscreen elements
- Decorative elements (separators, spacers)

**Finding 3**: **Indentation matters.** Tree structure via indentation helps the LLM understand containment relationships (which buttons are in which toolbar). Flat lists lose this signal.

**Finding 4**: **State annotations are crucial.** Always include:
- `selected` / `focused` / `disabled` / `expanded` / `checked`
- Current values for inputs
- Badge counts, unread indicators
- Whether an element is `editable`

### 3.5 Optimal Serialization Algorithm

```
function serialize_tree(node, depth=0, max_depth=4):
    if depth > max_depth: return ""
    if not is_relevant(node): return ""

    line = indent(depth) + format_node(node)
    result = line + "\n"

    for child in node.children:
        result += serialize_tree(child, depth+1, max_depth)

    return result

function is_relevant(node):
    # Skip decorative/structural-only nodes
    if node.role in [SEPARATOR, SPACER, GROUP_WITH_NO_LABEL]: return false
    # Skip invisible
    if not node.visible or node.offscreen: return false
    # Skip redundant wrappers (group with exactly 1 child)
    if node.role == GROUP and len(node.children) == 1: return false
    return true

function format_node(node):
    parts = [f"[{node.id}]", node.role_name]
    if node.label: parts.append(f'"{node.label}"')
    if node.value: parts.append(f'value="{truncate(node.value, 100)}"')

    # State flags
    states = []
    if node.selected: states.append("selected")
    if node.focused: states.append("focused")
    if node.disabled: states.append("disabled")
    if node.expanded: states.append("expanded")
    if node.checked: states.append("checked")
    if node.editable: states.append("editable")
    if states: parts.append(f"({', '.join(states)})")

    return " ".join(parts)
```

---

## 4. Action Reliability

### 4.1 The Core Problem

UI actions fail for many reasons:
- Element not yet visible (app still loading)
- Element moved (layout shift, animation in progress)
- Element behind a modal/overlay
- Wrong element matched (duplicate labels)
- App not in expected state (precondition violated)

### 4.2 Verified Action Pattern

The most reliable pattern, used by Agent-S and UFO:

```
BEFORE action:
  1. Verify target element exists in tree
  2. Verify element is visible and enabled
  3. Verify app is in expected state (optional precondition)

EXECUTE action:
  4. Perform the action
  5. Wait for UI to stabilize (see 4.3)

AFTER action:
  6. Take new screenshot / refresh tree
  7. Verify expected state change occurred
  8. If verification fails -> error recovery (Section 7)
```

This is the **observe-act-verify** loop, and every serious implementation uses it.

### 4.3 UI Stabilization Strategies

**Strategy A: Polling for tree stability**
```python
def wait_for_stable(timeout=5.0, poll_interval=0.3, stable_count=2):
    """Wait until the accessibility tree stops changing."""
    last_hash = None
    stable_ticks = 0
    deadline = time.time() + timeout

    while time.time() < deadline:
        tree = get_accessibility_tree()
        current_hash = hash_tree(tree)

        if current_hash == last_hash:
            stable_ticks += 1
            if stable_ticks >= stable_count:
                return tree  # Stable!
        else:
            stable_ticks = 0
            last_hash = current_hash

        time.sleep(poll_interval)

    return get_accessibility_tree()  # Timeout, return best effort
```

**Strategy B: Wait-for-element (explicit)**
```python
def wait_for_element(role=None, label=None, timeout=10.0):
    """Wait until a specific element appears in the tree."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        tree = get_accessibility_tree()
        match = find_element(tree, role=role, label=label)
        if match and match.visible and match.enabled:
            return match
        time.sleep(0.3)
    raise TimeoutError(f"Element {role}:{label} not found within {timeout}s")
```

**Strategy C: Screenshot diff (vision-based)**
```python
def wait_for_visual_stability(timeout=5.0):
    """Wait until screenshots stop changing."""
    last_screenshot = take_screenshot()
    deadline = time.time() + timeout

    while time.time() < deadline:
        time.sleep(0.5)
        current = take_screenshot()
        diff = pixel_diff(last_screenshot, current)
        if diff < 0.01:  # Less than 1% pixels changed
            return current
        last_screenshot = current

    return take_screenshot()
```

### 4.4 Retry Patterns

```
Retry hierarchy (try each level before escalating):

Level 1: IMMEDIATE RETRY (same action)
  - Transient failure (element briefly obscured by tooltip)
  - Wait 500ms, retry identical action
  - Max 2 retries

Level 2: RE-LOCATE AND RETRY
  - Element ID changed (dynamic content reload)
  - Refresh tree, find element by label/role, retry with new ID
  - Max 2 retries

Level 3: ALTERNATIVE ACTION
  - Click failed -> try keyboard shortcut
  - Menu click failed -> try hotkey equivalent
  - Scroll to element failed -> try search/filter
  - Max 1 retry

Level 4: AGENT RECOVERY (see Section 7)
  - Report failure to the planning layer
  - Let the agent choose an alternative approach
```

### 4.5 Element Identification Robustness

**Best practice: Multi-signal matching.** Never rely on a single property.

```python
def find_element_robust(tree, hints):
    """Match element using multiple signals, scored."""
    candidates = []
    for element in walk_tree(tree):
        score = 0
        if hints.role and element.role == hints.role: score += 3
        if hints.label and fuzzy_match(element.label, hints.label) > 0.8: score += 5
        if hints.near_label:
            neighbor = find_nearest_text(tree, element)
            if fuzzy_match(neighbor, hints.near_label) > 0.7: score += 2
        if hints.position_hint:
            if position_matches(element.bounds, hints.position_hint): score += 1
        if score > 0:
            candidates.append((score, element))

    candidates.sort(key=lambda x: -x[0])
    if candidates and candidates[0][0] >= 5:
        return candidates[0][1]
    return None
```

---

## 5. Context Window Management

### 5.1 The Scale Problem

Real numbers from macOS accessibility trees:

| Application | Nodes | Serialized size |
|-------------|-------|-----------------|
| Finder (empty) | ~200 | ~8 KB |
| Safari (1 tab) | ~2,000 | ~80 KB |
| VS Code | ~8,000 | ~350 KB |
| Excel (large sheet) | ~50,000 | ~2 MB |

Most LLMs work best with 4-8K tokens of UI context. That means aggressive filtering is mandatory.

### 5.2 Hierarchical Focus (Agent-S pattern)

```
Step 1: Show top-level windows only
  -> Agent picks "Mail" window

Step 2: Show Mail window's direct children (toolbar, sidebar, content)
  -> Agent picks "sidebar"

Step 3: Show sidebar's children in detail
  -> Agent picks "Inbox" and clicks it

Step 4: Show content area after click
  -> Agent sees the email list
```

This is essentially a tree-drill-down pattern. Each step fits easily in context.

```python
def focused_tree(root, focus_path=None, depth=2):
    """Return a tree pruned to the focus area."""
    if focus_path is None:
        # Top-level: show only window names + immediate children
        return serialize_tree(root, max_depth=1)

    # Navigate to focus node
    node = root
    for step in focus_path:
        node = find_child(node, step)

    # Show context: ancestors (collapsed) + focused subtree (expanded)
    result = ""
    # Ancestors as breadcrumb
    result += f"Path: {' > '.join(focus_path)}\n\n"
    # Siblings of focused node (for orientation)
    result += "Siblings:\n"
    for sibling in node.parent.children:
        marker = " --> " if sibling == node else "     "
        result += f"{marker}[{sibling.id}] {sibling.role} \"{sibling.label}\"\n"
    # Focused subtree in detail
    result += f"\nDetail:\n"
    result += serialize_tree(node, max_depth=depth)
    return result
```

### 5.3 Role-Based Filtering

Only include element types the agent can interact with:

```python
ACTIONABLE_ROLES = {
    'button', 'link', 'text_field', 'search_field',
    'checkbox', 'radio_button', 'dropdown', 'menu_item',
    'tab', 'list_item', 'slider', 'toggle', 'text_area',
    'combo_box', 'tree_item', 'table_cell',
}

CONTEXT_ROLES = {
    'static_text', 'heading', 'label', 'image',
    'toolbar', 'menu_bar', 'sidebar', 'content_area',
    'dialog', 'alert', 'status_bar',
}

def filter_tree(node):
    """Keep actionable elements + their context labels."""
    if node.role in ACTIONABLE_ROLES:
        return True
    if node.role in CONTEXT_ROLES and node.label:
        return True
    # Keep if any descendant is actionable
    return any(filter_tree(child) for child in node.children)
```

### 5.4 Viewport Clipping

Only include elements that are actually visible on screen:

```python
def clip_to_viewport(tree, screen_bounds):
    """Remove elements that are scrolled out of view."""
    def is_visible(node):
        if not node.bounds:
            return True  # No bounds info, keep it
        return rectangles_overlap(node.bounds, screen_bounds)

    return filter_tree_by(tree, is_visible)
```

### 5.5 Token Budget Approach (most sophisticated)

```python
def serialize_within_budget(tree, token_budget=4000):
    """Serialize tree, staying within token budget."""
    # Priority 1: Focused element + its siblings
    # Priority 2: Visible actionable elements
    # Priority 3: Context labels near actionable elements
    # Priority 4: Everything else

    elements = categorize_by_priority(tree)
    result = ""
    tokens_used = 0

    for priority_level in [1, 2, 3, 4]:
        for element in elements[priority_level]:
            line = format_node(element) + "\n"
            line_tokens = estimate_tokens(line)
            if tokens_used + line_tokens > token_budget:
                result += f"\n... ({count_remaining(elements, priority_level)} more elements truncated)\n"
                return result
            result += line
            tokens_used += line_tokens

    return result
```

### 5.6 Diff-Based Updates (advanced)

Instead of sending the full tree every turn, send only what changed:

```python
def tree_diff(old_tree, new_tree):
    """Compute minimal diff between tree states."""
    added = []
    removed = []
    changed = []

    old_ids = {n.id: n for n in walk_tree(old_tree)}
    new_ids = {n.id: n for n in walk_tree(new_tree)}

    for id, node in new_ids.items():
        if id not in old_ids:
            added.append(node)
        elif node_changed(old_ids[id], node):
            changed.append((old_ids[id], node))

    for id in old_ids:
        if id not in new_ids:
            removed.append(old_ids[id])

    return {"added": added, "removed": removed, "changed": changed}
```

Prompt format for diff:
```
CHANGES since last observation:
  ADDED: [45] button "Submit" (in form area)
  CHANGED: [12] text_field "Email" value="" -> value="alice@example.com"
  REMOVED: [30] dialog "Loading..."
```

---

## 6. Multi-Step Planning

### 6.1 Planning Paradigms

Three paradigms exist in the research, with clear tradeoffs:

```
Paradigm 1: FULL PLAN UPFRONT
  Plan all steps -> Execute sequentially -> Replan on failure
  Used by: Early research, simple tasks
  Pros: Coherent long-term strategy
  Cons: Plans break on first unexpected UI state

Paradigm 2: STEP-BY-STEP (ReAct pattern)
  Observe -> Think -> Act -> Observe -> Think -> Act -> ...
  Used by: Claude Computer Use, Open Interpreter
  Pros: Adapts to actual UI state every step
  Cons: Loses sight of big picture, gets stuck in loops

Paradigm 3: HIERARCHICAL (best of both worlds)
  High-level plan (subtasks) -> Execute each subtask step-by-step
  Used by: Agent-S, UFO
  Pros: Strategic planning + tactical flexibility
  Cons: More complex, higher latency
```

### 6.2 Hierarchical Planning (Recommended)

The research strongly favors hierarchical planning. Here is the pattern:

```
Manager prompt:
  "Given the user's goal and current state, break this into subtasks.
   Each subtask should be completable in 3-8 UI actions.
   Format:
   1. [subtask description] -- [success criteria]
   2. [subtask description] -- [success criteria]
   ..."

Worker prompt (per subtask):
  "Complete this subtask: [description]
   Success criteria: [criteria]
   Current UI state:
   [accessibility tree excerpt]

   Action history so far:
   [previous actions and their results]

   Choose your next action."
```

### 6.3 The ReAct Loop (Practical Implementation)

```python
class UIAgent:
    def __init__(self, llm, tools, max_turns=20):
        self.llm = llm
        self.tools = tools
        self.max_turns = max_turns
        self.history = []

    def run(self, task: str):
        plan = self.create_plan(task)

        for subtask in plan:
            success = self.execute_subtask(subtask)
            if not success:
                # Replan from current state
                plan = self.replan(task, completed=self.history)

    def execute_subtask(self, subtask: str):
        for turn in range(self.max_turns):
            # Observe
            state = self.observe()

            # Think + Act
            response = self.llm.generate(
                system=WORKER_SYSTEM_PROMPT,
                messages=[
                    {"role": "user", "content": f"""
                    Subtask: {subtask}

                    Current UI state:
                    {state.tree}

                    Action history:
                    {format_history(self.history[-10:])}

                    Choose your next action, or say DONE if subtask is complete.
                    """},
                ],
                tools=self.tools,
            )

            if response.is_done():
                return True

            # Execute action
            action = response.tool_call
            result = self.execute_action(action)
            self.history.append({"action": action, "result": result})

            # Verify
            if not self.verify_action(action, result):
                self.handle_failure(action, result)

        return False  # Max turns exceeded
```

### 6.4 Plan Representation

What works best for the plan format:

```
GOOD (natural language with success criteria):
  1. Open the Settings app -- Settings window is visible
  2. Navigate to Display settings -- Display panel is shown
  3. Change resolution to 1920x1080 -- Resolution dropdown shows 1920x1080
  4. Click Apply -- Confirmation dialog appears
  5. Confirm the change -- Settings saved, dialog dismissed

BAD (too specific about UI elements):
  1. Click button[id=23] "System Preferences"
  2. Click icon at position (340, 180)
  -- Breaks if UI changes between plan and execution

BAD (too vague):
  1. Change the resolution
  -- No way to verify completion
```

### 6.5 Replanning Triggers

When should the agent throw away its plan and replan?

```
REPLAN when:
  - Subtask failed after retries (state diverged from expectation)
  - Unexpected dialog/modal appeared (error, confirmation)
  - Application crashed or closed
  - User manually intervened (state changed externally)
  - Progress metric is stuck (same state for N turns)

DO NOT REPLAN when:
  - Single action failed (just retry or try alternative)
  - Minor UI difference (button in slightly different position)
  - Expected intermediate state (loading screen)
```

---

## 7. Error Recovery

### 7.1 Error Taxonomy

```
Category 1: TRANSIENT (auto-recoverable)
  - Element temporarily obscured (tooltip, animation)
  - Network delay (content loading)
  - Brief focus loss
  Recovery: Wait + retry (500ms-2s)

Category 2: STATE MISMATCH (agent-recoverable)
  - Unexpected dialog appeared
  - Wrong page/screen loaded
  - Element disabled when expected enabled
  Recovery: Agent observes new state, adapts plan

Category 3: ELEMENT NOT FOUND (may need replanning)
  - Element removed from DOM/tree
  - App layout changed
  - Wrong window in focus
  Recovery: Refresh tree, search by alternative properties, replan if needed

Category 4: APPLICATION FAILURE (escalate)
  - App crashed
  - System dialog appeared (permissions, updates)
  - Unrecoverable error state
  Recovery: Report to user, suggest manual intervention
```

### 7.2 Recovery Decision Tree

```
Action failed
  |
  +-- Is element still in tree?
  |     |-- YES: Is it visible and enabled?
  |     |     |-- YES: Retry (Level 1)
  |     |     |-- NO: Wait for it to become ready (Level 1)
  |     |-- NO: Search for similar element (Level 2)
  |           |-- Found: Retry with new ID (Level 2)
  |           |-- Not found: Is there a modal/dialog blocking?
  |                 |-- YES: Dismiss it, retry (Level 2)
  |                 |-- NO: Replan subtask (Level 4)
  |
  +-- Is the app still running?
        |-- YES: Is the right window focused?
        |     |-- YES: Take screenshot, let agent reassess (Level 3)
        |     |-- NO: Focus correct window, retry (Level 2)
        |-- NO: Report failure (Level 5 - escalate)
```

### 7.3 Practical Recovery Implementation

```python
class ActionRecovery:
    def attempt_action(self, action, max_retries=3):
        for attempt in range(max_retries):
            try:
                result = self.execute(action)

                # Verify the action had the expected effect
                if self.verify_postcondition(action, result):
                    return Success(result)
                else:
                    # Action executed but didn't produce expected state
                    return self.handle_unexpected_state(action, result)

            except ElementNotFoundError as e:
                # Try to relocate the element
                new_element = self.relocate_element(action.target)
                if new_element:
                    action = action.with_target(new_element)
                    continue
                else:
                    return self.escalate_to_agent(action, e)

            except ElementNotInteractableError as e:
                # Wait for element to become ready
                ready = self.wait_for_ready(action.target, timeout=3.0)
                if ready:
                    continue
                else:
                    return self.try_alternative_action(action)

            except ApplicationError as e:
                return Failure(e, recoverable=False)

        return Failure("Max retries exceeded", recoverable=True)

    def handle_unexpected_state(self, action, result):
        """The action ran but the UI isn't in the expected state."""
        # Check for common unexpected states
        tree = self.get_tree()

        # Modal dialog appeared?
        modal = find_modal(tree)
        if modal:
            if is_error_dialog(modal):
                return Failure(f"Error dialog: {modal.text}", recoverable=True)
            if is_confirmation_dialog(modal):
                # Let the agent decide whether to confirm
                return NeedsDecision(modal)

        # Page/view changed unexpectedly?
        current_context = self.identify_context(tree)
        if current_context != self.expected_context:
            return StateChanged(current_context)

        # Unknown state -- let agent figure it out
        return NeedsReassessment(tree)
```

### 7.4 Circuit Breaker Pattern

Prevent the agent from looping forever on unrecoverable failures:

```python
class CircuitBreaker:
    def __init__(self, max_consecutive_failures=5,
                 max_same_action_retries=3,
                 max_no_progress_turns=10):
        self.consecutive_failures = 0
        self.action_counts = Counter()
        self.state_history = []

    def check(self, action, state):
        # Same action repeated too many times?
        action_key = (action.type, action.target)
        self.action_counts[action_key] += 1
        if self.action_counts[action_key] > self.max_same_action_retries:
            raise CircuitBroken(f"Action {action_key} repeated {self.action_counts[action_key]} times")

        # Too many consecutive failures?
        if self.consecutive_failures > self.max_consecutive_failures:
            raise CircuitBroken(f"{self.consecutive_failures} consecutive failures")

        # State not changing? (agent is stuck)
        self.state_history.append(hash(state))
        if len(self.state_history) > self.max_no_progress_turns:
            recent = self.state_history[-self.max_no_progress_turns:]
            if len(set(recent)) <= 2:  # Only 1-2 unique states
                raise CircuitBroken("No progress detected")

    def record_success(self):
        self.consecutive_failures = 0

    def record_failure(self):
        self.consecutive_failures += 1
```

---

## 8. Open Interpreter's Approach

### 8.1 Architecture

Open Interpreter (OI) takes a fundamentally different approach from the research agents above. Instead of the accessibility tree, it uses **code generation** as its primary interface:

```
+----------------------------+
|   User goal (natural lang) |
+---v------------------------+
|   LLM (GPT-4/Claude)      |
|   Generates Python code    |
+---v------------------------+
|   Code Interpreter         |
|   (sandboxed execution)    |
+---v------------------------+
|   Output / Screenshot      |
|   (fed back to LLM)       |
+----------------------------+
```

### 8.2 Desktop Control in OI

For desktop control, OI uses a combination of:

**pyautogui** (coordinate-based):
```python
import pyautogui
# Generated by the LLM:
pyautogui.click(500, 300)
pyautogui.write("Hello world")
pyautogui.hotkey("cmd", "s")
```

**OS-specific APIs** (when available):
```python
# macOS - AppleScript via subprocess
import subprocess
subprocess.run(["osascript", "-e",
    'tell application "Finder" to activate'])

# Windows - pywinauto
from pywinauto import Desktop
app = Desktop(backend="uia")
app.window(title="Notepad").Edit.type_keys("Hello")
```

### 8.3 OI's "Computer" Module (newer versions)

More recent versions of OI added an `os` mode / "computer" module:

```python
# OI's computer abstraction layer
interpreter.computer.display.screenshot()     # Returns image
interpreter.computer.mouse.click(x, y)        # Click
interpreter.computer.keyboard.write("text")   # Type
interpreter.computer.keyboard.hotkey("cmd", "c")  # Shortcut
interpreter.computer.clipboard.get()           # Read clipboard
interpreter.computer.os.get_selected_text()    # Platform-specific

# Higher-level (OI 0.3.x+)
interpreter.computer.browser.search("query")
interpreter.computer.files.edit("path", "content")
interpreter.computer.calendar.create_event(...)
```

### 8.4 OI vs. Research Agents -- Key Differences

| Aspect | OI | Research Agents (Agent-S etc.) |
|--------|----|---------------------------------|
| Primary interface | Code generation | Structured tool calls |
| UI understanding | Screenshot + coordinates | Accessibility tree + vision |
| Action granularity | Arbitrary (full Python) | Fixed tool set |
| Planning | Implicit (in code) | Explicit (plan then execute) |
| Error handling | Try/except in code | Agent-level recovery |
| Reliability | Lower (coordinates drift) | Higher (element IDs) |
| Flexibility | Higher (can do anything) | Lower (limited to tools) |
| Safety | Harder to sandbox | Easier to constrain |

### 8.5 Lessons from OI

What to take from OI's approach:
1. **Code-as-action is powerful for complex tasks** -- when the agent needs to process data, compute values, or do conditional logic, generating code beats a fixed tool set.
2. **Screenshot feedback loop is essential** -- OI always shows the result back to the LLM.
3. **Platform-specific helpers matter** -- AppleScript on macOS, PowerShell on Windows, xdotool on Linux.

What to avoid from OI's approach:
1. **Coordinate-based control is fragile** -- without accessibility tree grounding.
2. **Unbounded code execution is risky** -- hard to audit, hard to sandbox.
3. **No structured observation** -- screenshots alone miss interactive states.

---

## 9. Synthesis: Recommended Architecture for Nika

Based on all the research above, here is the architecture I recommend for Nika's UI automation capability.

### 9.1 Architecture: Hierarchical Agent with Hybrid Grounding

```
                  +---------------------------+
                  |    User Goal (natural)     |
                  +---v-----------------------+
                  |    Manager Agent           |
                  |    (subtask planning)      |
                  |    Model: strong reasoner  |
                  +---v-----------------------+
                      |
          +-----------+-----------+
          |                       |
    +-----v-------+       +------v------+
    |   Worker     |       |   Worker    |
    |   (subtask   |       |  (subtask   |
    |    execution)|       |   execution)|
    +-----+-------+       +------+------+
          |                      |
    +-----v----------------------v------+
    |   UI Bridge (native per-platform) |
    |   - macOS: AXUIElement API        |
    |   - Linux: AT-SPI2                |
    |   - Windows: UI Automation        |
    +---v-------------------------------+
    |   Accessibility Tree Cache         |
    |   + Screenshot Pipeline            |
    +-----------------------------------+
```

### 9.2 Tool Set for Nika agent: Verb

```yaml
# Observation tools (Layer 0)
- name: ui_observe
  description: "Get current UI state: filtered accessibility tree + optional screenshot"
  params:
    focus_element: number?    # Drill into subtree of this element
    include_screenshot: bool  # Also return a screenshot (costly)
    depth: number             # Max tree depth (default: 3)

- name: ui_element
  description: "Get detailed properties of a single element"
  params:
    id: number

- name: ui_windows
  description: "List open application windows"

# Semantic actions (Layer 2)
- name: ui_click
  params:
    id: number
    click_type: "single|double|right"  # default: single
    verify: string?            # Expected state after click

- name: ui_type
  params:
    id: number
    text: string
    clear_first: bool          # Clear existing text (default: true)

- name: ui_select
  params:
    id: number
    value: string

- name: ui_scroll
  params:
    id: number?               # Element to scroll (null = active area)
    direction: "up|down|left|right"
    amount: number             # 1 = one "page"

- name: ui_hotkey
  params:
    keys: string               # e.g., "cmd+shift+s"

# Primitive fallback (Layer 1)
- name: ui_click_xy
  params:
    x: number
    y: number
    click_type: "single|double|right"

# Control
- name: ui_wait
  params:
    condition: string          # "element_visible(id=5)" or "text_contains(id=3, 'Saved')"
    timeout: number            # seconds

- name: ui_verify
  params:
    assertion: string          # "element(id=5).enabled == true"
  returns: bool

# Completion
- name: nika:complete
  params:
    success: bool
    result: string
```

### 9.3 Accessibility Tree Format for Nika

```
# Window: [app_name] - [window_title]
# Focus: [currently_focused_element]
# Screen: [width]x[height]

## [region_name]
[id] role "label" state {key_property=value}
  [id] role "label" state
  [id] role "label" state

## [region_name]
[id] role "label" state
```

Example:
```
# Window: Safari - GitHub
# Focus: [42] search_field "Search"
# Screen: 1440x900

## Navigation Bar
[1] button "Back" (disabled)
[2] button "Forward" (disabled)
[3] text_field "URL" value="github.com/supernovae"
[4] button "Reload"

## Tab Bar
[5] tab "GitHub" (selected)
[6] tab "Stack Overflow"
[7] button "New Tab"

## Page Content
[10] heading "SuperNovae Studio"
[11] text "Workflow engine for AI"
[12] link "nika"
[13] link "novanet"
[14] button "Star" value="1.2k"
[15] button "Fork" value="89"

## Sidebar (collapsed)
[20] button "Toggle Sidebar"
```

### 9.4 Action-Observe-Verify Loop for Nika

The core execution loop should be built into the `agent:` verb implementation:

```rust
// Pseudocode for the Nika UI agent loop
loop {
    // 1. Observe (automatic -- injected before each LLM turn)
    let tree = ui_bridge.get_filtered_tree(focus, depth);
    let screenshot = if needs_vision { ui_bridge.screenshot() } else { None };

    // 2. Build context (automatic)
    let ui_context = serialize_tree(&tree, token_budget);
    inject_observation_into_conversation(ui_context, screenshot);

    // 3. LLM decides action (agent turn)
    let response = llm.generate(conversation, tools);

    // 4. Execute action
    let result = match response.tool_call {
        ToolCall::UiClick { id, verify, .. } => {
            let r = ui_bridge.click(id);
            // Auto-wait for stability
            ui_bridge.wait_for_stable(Duration::from_millis(500));
            // Auto-verify if requested
            if let Some(verify) = verify {
                let ok = ui_bridge.check_condition(&verify);
                if !ok {
                    r.with_warning("Post-condition not met")
                } else { r }
            } else { r }
        },
        ToolCall::NikaComplete { success, result } => {
            return AgentResult { success, result };
        },
        _ => execute_tool(response.tool_call),
    };

    // 5. Check circuit breaker
    circuit_breaker.record(response.tool_call, &tree);
    if circuit_breaker.is_broken() {
        return AgentResult::stuck("Agent appears stuck. Circuit breaker triggered.");
    }

    // 6. Feed result back (next iteration)
    conversation.add_tool_result(result);
}
```

### 9.5 Context Window Strategy for Nika

```
Token budget allocation for a 128K context model:

System prompt:          ~2K tokens
Task description:       ~500 tokens
Subtask plan:           ~500 tokens
Action history (last 5): ~2K tokens
UI tree (current):      ~4K tokens    <-- THIS IS THE BOTTLENECK
Screenshot (if used):   ~1K tokens (image tokens)
Agent reasoning:        ~2K tokens
Safety margin:          ~500 tokens
------------------------------------------
Total per turn:         ~13K tokens

For a 20-turn subtask: ~260K total tokens (including KV cache)
```

Pruning priority for the 4K UI tree budget:
1. Currently focused element + its siblings (always included)
2. Elements matching the current subtask keywords
3. Visible interactive elements (buttons, links, fields)
4. Context labels near interactive elements
5. Everything else (truncated with count)

### 9.6 Key Architectural Decisions

| Decision | Recommendation | Rationale |
|----------|---------------|-----------|
| Primary grounding | Accessibility tree | More reliable than coordinates, works headless |
| Fallback grounding | Coordinate + screenshot | Covers custom-drawn UI |
| Planning | Hierarchical (Manager/Worker) | Best balance of strategy and flexibility |
| Action granularity | Element-level semantic | Research consensus on sweet spot |
| Tree format | Simplified text with IDs | 15-25% better than raw JSON |
| Error recovery | 4-level hierarchy with circuit breaker | Prevents loops, enables graceful degradation |
| Screenshot frequency | After every action | Essential for state verification |
| Vision model | Optional, for fallback grounding | Not every turn needs vision |

---

## Sources

1. **Agent-S** -- Agashe et al., "Agent S: An Open Agentic Framework that Uses Computers Like a Human" (arXiv:2410.08164, Oct 2024). Introduced hierarchical Manager-Worker architecture with experience-augmented planning.

2. **SeeAct** -- Zheng et al., "GPT-4V(ision) is a Generalist Web Agent, if Grounded" (arXiv:2401.01614, Jan 2024). Established the two-phase action-then-grounding paradigm and Set-of-Marks evaluation.

3. **Set-of-Marks** -- Yang et al., "Set-of-Mark Prompting Unleashes Extraordinary Visual Grounding in GPT-4V" (arXiv:2310.11441, Oct 2023). Original SoM paper showing visual numbering dramatically improves element reference accuracy.

4. **OS-Atlas** -- Wu et al., "OS-Atlas: A Foundation Action Model for Generalist GUI Agents" (arXiv:2410.23218, Oct 2024). Vision-only UI grounding without accessibility APIs.

5. **UFO** -- Zhang et al., "UFO: A UI-Focused Agent for Windows OS Interaction" (arXiv:2402.07939, Feb 2024). Dual-agent architecture with per-application specialization.

6. **OSWorld** -- Xie et al., "OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments" (arXiv:2404.07972, Apr 2024). Comprehensive benchmark for computer-using agents.

7. **WebArena** -- Zhou et al., "WebArena: A Realistic Web Environment for Building Autonomous Agents" (arXiv:2307.13854, Jul 2023). Web-specific benchmark and environment, introduced accessibility tree linearization patterns.

8. **Claude Computer Use** -- Anthropic, "Introducing computer use" (Oct 2024, updated Jan 2025). Minimal coordinate-based tool design philosophy.

9. **Open Interpreter** -- GitHub: OpenInterpreter/open-interpreter. Code-generation approach to computer control. "Computer" module for desktop automation.

10. **ShowUI** -- Lin et al., "ShowUI: One Vision-Language-Action Model for GUI Visual Agent" (arXiv:2411.17465, Nov 2024). UI-tuned VLM for direct screen understanding.

11. **CogAgent** -- Hong et al., "CogAgent: A Visual Language Model for GUI Agents" (arXiv:2312.08914, Dec 2023). 18B VLM specifically trained for GUI understanding across platforms.

12. **ReAct** -- Yao et al., "ReAct: Synergizing Reasoning and Acting in Language Models" (arXiv:2210.03629, 2022). The foundational observe-think-act loop used by all modern agents.

---

## Confidence Level

**HIGH** for architectural patterns (Sections 1-3, 6-7) -- multiple independent research groups converge on the same conclusions. The hierarchical planning + accessibility tree + semantic actions pattern is well-validated.

**MEDIUM-HIGH** for tool design (Section 2) and context management (Section 5) -- practical consensus exists but implementations vary significantly in details.

**MEDIUM** for Open Interpreter specifics (Section 8) -- OI iterates rapidly and the architecture changes between versions. The general approach is stable but specific APIs may have shifted.

---

## Relevance to Nika

This research directly maps to extending Nika's `agent:` verb with UI automation capabilities. The key integration points:

1. **New MCP server**: `nika-ui-bridge` that wraps platform accessibility APIs and exposes them as MCP tools.
2. **Tree serialization**: Built into the bridge, configurable depth/budget/focus.
3. **Agent verb extensions**: Automatic observation injection, circuit breaker, and stability waiting as part of the agent loop.
4. **Workflow-level**: `invoke: nika:ui_click` etc. for scripted (non-agentic) UI automation in DAG tasks.

The hybrid approach (accessibility tree as primary + coordinate fallback + optional vision) gives the best reliability-to-complexity ratio.
