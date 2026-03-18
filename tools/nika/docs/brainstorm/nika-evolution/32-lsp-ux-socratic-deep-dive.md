# 32 -- LSP User Experience: A Socratic Deep Dive

> Research date: 2026-03-18
> Sources: rust-analyzer GitHub issues (#3267, #4549, #2876), VS Code accessibility issues (#142532, #158915, #247435), Red Hat yaml-language-server, textLSP (hangyav/textLSP), JetBrains IntelliJ Inspections docs, Stack Overflow 2024 Developer Survey, matklad's design notes, Grammarly/CoEdIT architecture (via Hugging Face), Nika LSP source audit
> Method: Socratic questioning -- six hard questions, no easy answers

---

## Table of Contents

1. [Question 1: What If the LSP Was Smarter Than the User?](#question-1-what-if-the-lsp-was-smarter-than-the-user)
2. [Question 2: What Makes Developers STOP Using an LSP?](#question-2-what-makes-developers-stop-using-an-lsp)
3. [Question 3: What If the Editor Understood Your Workflow's INTENT?](#question-3-what-if-the-editor-understood-your-workflows-intent)
4. [Question 4: Progressive Disclosure in Developer Tools](#question-4-progressive-disclosure-in-developer-tools)
5. [Question 5: What Would Make Someone Choose Nika Over Raw Scripts?](#question-5-what-would-make-someone-choose-nika-over-raw-scripts)
6. [Question 6: Accessibility Features LSPs Need](#question-6-accessibility-features-lsps-need)
7. [Synthesis: The Dangerous Middle](#synthesis-the-dangerous-middle)
8. [Concrete Recommendations for Nika](#concrete-recommendations-for-nika)

---

## Question 1: What If the LSP Was Smarter Than the User?

### How Grammarly and LanguageTool Actually Work

The popular narrative is "Grammarly checks grammar." The reality is far more layered:

**LanguageTool (Open Source, 2003-present)**
- Uses a hybrid architecture: rule-based patterns (XML-defined, ~10,000 rules per language) combined with n-gram statistical models and, more recently, neural network classifiers
- The rule engine operates on annotated tokens, not raw text. Each token carries POS tags, lemma, chunk tags, and disambiguator results
- The critical insight: rules are grouped by *category* (Grammar, Style, Punctuation, Redundancy, Typography), and each category has a user-facing severity level. Users suppress entire categories, not individual rules
- False positive suppression is rule-local via `<antipattern>` XML elements -- a rule author explicitly lists known false triggers
- Key UX pattern: LanguageTool distinguishes between "this is wrong" (red underline) and "this could be better" (blue underline). The *color alone* communicates intent certainty

**Grammarly (Proprietary + CoEdIT research, 2009-present)**
- Published research through the CoEdIT project (Grammarly's research arm at Google): models like `grammarly/coedit-large` and `grammarly/coedit-xl-composite` on Hugging Face
- CoEdIT is trained on *edit operations*, not just corrections. It learns from real human editorial passes: what writers actually change, not what grammar rules say they should change
- The model takes an instruction like "Fix grammar errors in this text" or "Make this text more formal" -- it understands *intent categories*, not just error categories
- Critical insight: Grammarly's power comes from understanding *what kind of writing the user is doing*. An email gets different suggestions than a research paper. Context is everything

**textLSP (Open Source LSP implementation, 2023-present)**
- A concrete bridge between these ideas and the LSP protocol
- Uses the LSP diagnostic model (squiggly lines) for grammar errors, and code actions for fix suggestions
- Supports both local (LanguageTool, Ollama) and remote (OpenAI) analyzers
- Critical design choice: analyzers are **disabled by default** -- the user must explicitly opt in. This prevents the "I didn't ask for this" problem
- Supports `on_open`, `on_save`, and `on_change` granularity per analyzer. Expensive analyzers only run on save

### Could a Workflow LSP Understand User Intent?

Here is where the analogy becomes powerful and dangerous.

**The Parallelization Detector**

Consider three sequential `infer:` tasks with no data dependencies:

```yaml
tasks:
  - id: translate_en
    infer: "Translate to English: {{input}}"
  - id: translate_fr
    depends_on: [translate_en]
    infer: "Translate to French: {{input}}"
  - id: translate_de
    depends_on: [translate_fr]
    infer: "Translate to German: {{input}}"
```

A "smart" LSP could detect that `translate_fr` and `translate_de` both reference `{{input}}` (not each other's outputs) and suggest removing the `depends_on` chain for parallelization.

But here is the critical counterargument: **the user might have added the chain deliberately** for cost control (serial execution avoids bursting 3 simultaneous API calls), for rate limiting, or because they want to see intermediate results before committing to the next call.

The Grammarly model works because natural language has a widely agreed-upon "correct" form. Workflow orchestration does not. There is no "correct" way to order API calls -- only tradeoffs.

**The Binding Deduplication Detector**

```yaml
tasks:
  - id: research
    infer: "Research the topic of quantum computing"
  - id: summarize
    infer: "Summarize the following research about quantum computing: {{with.data}}"
    with:
      data: $research
```

An LSP could detect the repeated "quantum computing" string and suggest extracting it into the workflow context:

```yaml
context:
  topic: "quantum computing"
tasks:
  - id: research
    infer: "Research the topic of {{context.topic}}"
  - id: summarize
    infer: "Summarize the following research about {{context.topic}}: {{with.data}}"
    with:
      data: $research
```

This is genuinely useful. But the detection heuristic matters enormously. String matching will produce false positives on common words. Semantic analysis ("these two strings refer to the same concept") is a research problem, not an engineering problem.

**The Line Between Helpful and Annoying: JetBrains Inspections Model**

JetBrains IntelliJ IDEA defines the clearest framework I found for managing this tension:

1. **Severity Levels**: Error > Warning > Weak Warning > Information > Server Problem > Typo > Grammar Error. Each maps to a distinct visual treatment (underline color, gutter icon, or none)
2. **Inspection Profiles**: Named collections of enabled inspections + their severity levels. "Default" profile is conservative; "Project" profile can be aggressive
3. **Scopes**: Inspections can be limited to specific file sets (e.g., "only production code", "only test code", "only modified files")
4. **Disable new inspections by default**: A checkbox that prevents plugin-installed inspections from immediately lighting up the editor. The user discovers them through the settings UI, not through surprise diagnostics
5. **Qodana integration**: The most expensive, opinionated inspections run in CI, not in the editor. The IDE is for fast feedback; the CI is for thorough analysis

This model directly maps to Nika:

| JetBrains Concept | Nika LSP Equivalent |
|---|---|
| Error severity | NIKA-xxx validation errors (already exist) |
| Warning severity | Workflow anti-pattern hints (new) |
| Weak Warning | Style suggestions (new) |
| Information | Performance tips (new) |
| Inspection profiles | `nika.lsp.inspectionLevel: "strict" \| "standard" \| "minimal"` |
| Scopes | Already scoped to `.nika.yaml` files |
| Qodana (CI) | `nika check --strict` for CI pipelines |

**Verdict**: An intent-aware LSP is possible but requires the JetBrains discipline of severity levels and user control. Never present a suggestion at the same severity as an error. The user must always be able to distinguish "your workflow is broken" from "your workflow could be improved."

---

## Question 2: What Makes Developers STOP Using an LSP?

### Evidence from rust-analyzer GitHub Issues

The rust-analyzer project provides the most transparent record of LSP friction I could find, because matklad (Aleksey Kladov, the original author) discussed design philosophy openly.

**Issue #3267: "Disable deduced types or make it on-hover only" (24 upvotes, 12 comments)**

Key quote from matklad:
> "In general, we prefer to enable all bells and whistles by default, because that helps with **discoverability**. If a particular feature annoys you, it usually is straightforward to look at the docs for the way to disable it. OTOH, if you would love to use a feature, but do not know that it exists in the first place, you probably won't even try to find it in the docs."

The community response was instructive:
- shamatar: "Current implementation of type hints after 'let x' statement provides very strange behavior when cursor jumps to the end of the type hint instead of the end of the variable name that makes a lot of distraction and visually misguiding"
- nbro (9 upvotes): "This issue should at least link to the documentation that describes how to disable these annoying type hints"
- hraftery (4 upvotes): "I'd hate to actually **lose** these hints, which are really valuable *when you want them*. Is there a way to display them without the associated cursor movement?"
- airstrike (2 upvotes): "I wish toggling these inlay hints on/off was available in the command palette. They're useful every now and then, but often feel like clutter so I keep toggling them in the settings"
- mrcampbell: "The '7 Implementations' is from 'Code Lens', and the VS Code Setting JSON key is `rust-analyzer.lens.enable: false`. While valuable, it's provided in the popup window on hover, so it's not necessary to have it floating between your lines of code at all times"

**Issue #4549: "disable all inlay hints, but still see implementations" (7 upvotes)**

This reveals the granularity problem: users want per-feature control, not all-or-nothing. The eventual solution was separate toggle settings for each hint category.

### The Five Reasons Developers Disable LSP Features

Synthesizing from the rust-analyzer issues, Red Hat YAML server issues, and broader developer forums:

**1. Visual Noise / Cognitive Overhead**
- Inlay hints that make lines too long to read
- Code lens labels between every function
- Diagnostic squiggles on code that is "work in progress"
- The "Christmas tree" effect: every line has some decoration

**2. Cursor/Editing Interference**
- Inlay hints that shift cursor position during editing (the #1 complaint in rust-analyzer)
- Autocompletion that triggers at the wrong moment (especially on `:` in YAML)
- Format-on-type that fights the user's indentation intent
- Ghost text suggestions that interfere with normal typing flow

**3. False Positives That Erode Trust**
- Schema validation errors on valid-but-unusual constructs
- Warnings on patterns that are intentional (e.g., "unused variable" in a workflow that uses it via template)
- Stale diagnostics that persist after a fix is applied (latency between edit and re-validation)
- Cross-file diagnostics that flash during intermediate save states

**4. Performance Degradation**
- LSP server CPU usage visible in system monitors
- Typing latency introduced by synchronous analysis
- Memory growth from document caches that are never cleared
- Startup time that delays first-keystroke feedback

**5. Discoverability Without Consent**
- Features that appear with no opt-in and no documentation
- No command to disable a specific feature from within the editor
- Settings buried in JSON config rather than accessible via UI
- Diagnostic messages that don't link to an explanation

### The Trust Equation

```
Trust = (Accuracy * Relevance) / (Frequency * Intrusiveness)
```

- **Accuracy**: What percentage of diagnostics are actually problems? If below ~95%, trust decays rapidly
- **Relevance**: Is the diagnostic about something the user cares about right now? "Missing semicolon" is relevant; "function could be more concise" is often not
- **Frequency**: How often does the feature activate? Something that fires on every line must be very accurate. Something that fires once per file can be less accurate
- **Intrusiveness**: Does it block editing? Does it require dismissal? Or is it a passive indicator?

**For Nika specifically**: The workflow files are small (typically 20-200 lines). This means every diagnostic is highly visible. There is no "noise floor" of a 10,000-line codebase to dilute a false positive. A single wrong diagnostic on a 30-line workflow will make the user question the entire tool.

---

## Question 3: What If the Editor Understood Your Workflow's INTENT?

### Semantic Workflow Classification

A Nika workflow is not just "a YAML file." It encodes a specific pattern:

| Pattern | Indicators | LSP Behavior |
|---|---|---|
| Content Pipeline | Serial `infer:` tasks, string outputs flowing forward | Suggest template bindings, warn on missing `with:` |
| Data ETL | `fetch:` -> `exec:` (transform) -> `exec:` (write) | Suggest error handling, warn on missing retry |
| Research Agent | `agent:` verb, MCP tool calls, iterative loops | Suggest tool validation, warn on missing timeout |
| Test Suite | Multiple independent tasks, `assert:` builtin tools | Suggest parallelization, warn on serial execution |
| Deployment Pipeline | `exec:` tasks with shell commands, conditional flow | Suggest security review, warn on unquoted variables |

The LSP could classify a workflow into one of these patterns at parse time (heuristically, based on verb distribution and dependency graph shape) and adjust its suggestions accordingly.

**Example: Content Pipeline Pattern Detection**

```
If >50% of tasks use `infer:` verb
AND the dependency graph is a chain (A -> B -> C)
AND tasks reference previous task outputs
THEN classify as "Content Pipeline"
THEN enable: template binding completions, prompt optimization hints
THEN disable: parallelization suggestions (serial is intentional for content)
```

**Example: Data ETL Pattern Detection**

```
If tasks include `fetch:` followed by `exec:` or `infer:` transformation
AND outputs are written to files or sent to external services
THEN classify as "Data ETL"
THEN enable: error handling warnings, retry suggestions
THEN suggest: adding `on_error:` blocks, `timeout:` fields
```

### Anti-Pattern Detection for AI Workflows

These are specific to AI/LLM workflows and have no equivalent in traditional programming LSPs:

**1. Prompt Duplication**
- Detect when two `infer:` tasks have >70% prompt text overlap
- Suggest extracting the common part into a workflow-level `context:` field
- Confidence: Medium (string overlap doesn't always mean semantic overlap)

**2. Missing Context Propagation**
- Detect when a task's prompt references a concept (keyword) that a previous task already resolved
- Suggest adding `with:` binding to pass the resolved result
- Confidence: Low (requires NLP to identify concept references)

**3. Model Mismatch**
- Detect when a task uses a large model (claude-opus-4-20250514, gpt-4) for a simple operation (translation, formatting, extraction)
- Suggest a smaller model (claude-haiku-4-20250514, gpt-4o-mini) for cost optimization
- Confidence: Low (the LSP cannot judge prompt complexity)

**4. Serial Bottleneck**
- Detect independent tasks chained with `depends_on:` where the outputs are not used by subsequent tasks
- Suggest removing unnecessary dependencies for parallelization
- Confidence: High (this is a graph analysis, fully deterministic)

**5. Unbounded Agent Loop**
- Detect `agent:` tasks without `max_turns:` or `timeout:` set
- Warn about potential infinite loops
- Confidence: High (structural check, no heuristics needed)

**6. Missing Error Handling on External Calls**
- Detect `fetch:` tasks without `on_error:` or `retry:` configuration
- Suggest adding resilience patterns for network calls
- Confidence: High (best practice, not heuristic)

### What "Workflow Linting" Looks Like Beyond Syntax

The hierarchy, from most to least confident:

```
Level 1: Structural (always correct)
  - DAG cycle detection
  - Missing required fields
  - Unknown task references
  - Schema version validation

Level 2: Semantic (high confidence)
  - Unused task outputs
  - Unreachable tasks (no path from start)
  - Duplicate task IDs
  - Type mismatches in with: bindings

Level 3: Best Practice (medium confidence)
  - Missing timeout on agent/fetch
  - Missing retry on fetch
  - Missing on_error handlers
  - Serial tasks that could be parallel

Level 4: Style (low confidence, suggestions only)
  - Prompt optimization hints
  - Model selection suggestions
  - Context extraction recommendations
  - Naming convention enforcement
```

**Critical insight**: Level 1-2 should be errors/warnings. Level 3 should be "information" severity. Level 4 should be OFF by default and available as a "nika.lsp.inspectionLevel: strict" setting.

---

## Question 4: Progressive Disclosure in Developer Tools

### How rust-analyzer Handles Progressive Disclosure

rust-analyzer's approach (documented in matklad's writing and the issue discussions):

1. **Everything ON by default** -- this is the "discoverability" argument. matklad explicitly stated: if you don't know a feature exists, you can never decide to use it
2. **Granular per-feature toggles** -- after community pushback, each inlay hint type got its own setting (`typeHints.enable`, `parameterHints.enable`, `chainingHints.enable`, `closureReturnTypeHints.enable`, etc.)
3. **No "levels" abstraction** -- there is no "beginner/intermediate/expert" mode. Each setting is independent
4. **VS Code command palette integration** -- `Toggle Inlay Hints` became a command (VS Code 1.67), removing the need to edit settings.json

The result: power users can fine-tune exactly what they want. But beginners are overwhelmed by 50+ settings. This tradeoff is acceptable for rust-analyzer because its audience is Rust developers, who self-select for comfort with configuration.

### How VS Code Handles Progressive Disclosure

VS Code uses a multi-layer approach:

1. **Default settings that "just work"** -- most features are configured with sensible defaults
2. **Settings UI with search** -- the GUI settings editor groups settings by category and supports full-text search
3. **"Modified" filter** -- shows only settings the user has changed from defaults
4. **Settings scopes** -- User (global) > Workspace > Folder. Beginners use global; teams use workspace
5. **Extension-contributed settings** -- each extension defines its settings in `package.json` with descriptions, types, and defaults. The settings UI renders these automatically

### How JetBrains Handles Progressive Disclosure

JetBrains uses explicit levels:

1. **Inspection profiles**: "Default" (conservative) and "Project Default" (can be aggressive)
2. **"Disable new inspections by default" checkbox** -- prevents surprise diagnostics from plugin updates
3. **Severity-based filtering** -- the inspection settings UI can filter by severity level
4. **Per-scope overrides** -- an inspection can be "Error" in production code but "Warning" in test code

### What Nika Should Do

Nika's audience is broader than Rust developers. It includes:
- Python/JS developers who write scripts and want a better orchestration tool
- DevOps engineers who are comfortable with YAML (Kubernetes, GitHub Actions)
- AI engineers who care about prompts, not infrastructure
- Beginners who found Nika through a tutorial

This diversity demands progressive disclosure. The proposal:

**Three Inspection Levels**

```jsonc
// VS Code settings
{
  // "essential" (default): Only errors and high-confidence warnings
  // "recommended": Adds best-practice hints (timeout, retry, error handling)
  // "comprehensive": Adds style suggestions (parallelization, model selection)
  "nika.inspectionLevel": "essential"
}
```

| Feature | essential | recommended | comprehensive |
|---|---|---|---|
| Syntax errors | ON | ON | ON |
| Schema validation | ON | ON | ON |
| DAG cycle detection | ON | ON | ON |
| Unknown task references | ON | ON | ON |
| Missing timeout on agent | OFF | ON | ON |
| Missing retry on fetch | OFF | ON | ON |
| Parallelization hints | OFF | OFF | ON |
| Model selection hints | OFF | OFF | ON |
| Prompt optimization | OFF | OFF | ON |
| Naming convention hints | OFF | OFF | ON |

**Plus Individual Overrides**

```jsonc
{
  "nika.inspectionLevel": "recommended",
  // Override: I don't want parallelization hints even in comprehensive
  "nika.hints.parallelization": false,
  // Override: I do want timeout warnings even in essential
  "nika.hints.missingTimeout": true
}
```

**The Critical Default**

The default MUST be `"essential"`. Here is why:

The rust-analyzer approach ("everything on") works for a mature tool with a sophisticated audience. Nika is new. First impressions are irreversible. A new user who installs the VS Code extension, opens their first `.nika.yaml` file, and sees 5 "information" diagnostics alongside 2 real errors will not learn to distinguish them. They will either:
- Ignore all diagnostics (the boy who cried wolf)
- Uninstall the extension (path of least resistance)
- Spend 20 minutes configuring settings (friction that kills adoption)

Start conservative. Add a "Want more suggestions? Try `nika.inspectionLevel: recommended`" link in the first diagnostic message. Let the user opt in.

---

## Question 5: What Would Make Someone Choose Nika Over Raw Scripts?

### The Competitive Frame

A developer deciding between Nika and a Python script is making a cost-benefit calculation:

**Costs of Nika**:
- Learning a new YAML schema
- Trusting a workflow engine to do what you mean
- Debugging through an abstraction layer
- Ecosystem maturity (fewer examples, smaller community)

**Benefits of Nika** (without LSP):
- Declarative: describe WHAT, not HOW
- Built-in parallelism, retry, timeout
- DAG visualization
- Provider abstraction (swap LLM providers without code changes)
- MCP integration
- Reproducibility (YAML is version-controllable, diffable)

**Benefits of the LSP** (the differentiator):

The LSP must make the YAML editing experience *better than writing Python in a good IDE*. Here is what that requires:

### 1. Zero-to-Working Faster Than a Script

**Snippet-driven development**: When a user types `- id:` and hits Tab, the LSP should offer completion items that scaffold entire task patterns:

```
Completions after "- id: my_task\n  ":
  infer (simple)    -> infer: "YOUR_PROMPT"
  infer (full)      -> infer:\n  prompt: ""\n  model: claude-sonnet-4-6\n  temperature: 0.7
  fetch (GET)       -> fetch:\n  url: ""\n  method: GET
  exec (shell)      -> exec: ""
  agent (loop)      -> agent:\n  prompt: ""\n  tools: []\n  max_turns: 10
```

In Python, you would need to write the httpx import, the async function, the error handling, the retry logic. In Nika with this LSP, you select a pattern and fill in the blanks.

### 2. Instant Feedback That Scripts Cannot Provide

**DAG visualization in the editor**: An inlay hint or code lens that shows the execution graph:

```yaml
tasks:
  - id: research          # [1/3] Independent
    infer: "Research..."
  - id: translate          # [2/3] Independent (runs parallel with research)
    infer: "Translate..."
  - id: combine            # [3/3] Depends on: research, translate
    depends_on: [research, translate]
    infer: "Combine..."
```

The `[1/3] Independent` and `[3/3] Depends on: research, translate` annotations tell the user the execution order *without running the workflow*. In Python, you would need to mentally trace the asyncio graph or draw it on paper.

### 3. Provider-Aware Intelligence

**Cost estimation**: The LSP knows which model each `infer:` task uses and can estimate cost:

```yaml
- id: generate
  infer:
    prompt: "Write a 2000 word article"
    model: claude-opus-4-20250514          # ~$0.15 per run (est. 2k input + 2k output tokens)
```

No Python IDE can do this because the model and prompt are just strings. In Nika, the LSP knows the schema and can compute estimates.

**Provider compatibility checking**: If a user specifies `model: gpt-4` with `tools:` (function calling), the LSP can verify that the provider supports tool use. If they specify `model: claude-sonnet-4-6` with `response_format: json_schema`, the LSP can verify Anthropic supports structured output.

### 4. Cross-Task Reasoning

**Template validation across task boundaries**: The LSP can trace `{{with.data}}` through the `with:` binding to the source task and validate that the source task's output type matches the template's expected type.

In Python, this requires runtime debugging. In Nika, the LSP catches it at edit time.

**Dependency chain visualization**: Hovering on a `with:` binding shows the full data flow path:

```
with.data -> $research -> tasks[0].infer.output (string)
  Resolved through: direct binding
  Estimated output: ~500 words (based on prompt analysis)
```

### 5. The "Can't Go Back" Feature

The single feature that would make users say "I can't go back to raw scripts" is **live workflow preview**:

The LSP provides a code lens "Preview DAG" on the workflow name. Clicking it opens a side panel (via VS Code custom editor or webview) showing:
- The dependency graph as a visual DAG
- Estimated execution time per task
- Estimated total cost
- Highlighted critical path
- Click-to-navigate: clicking a node in the DAG jumps to the task definition

This is what Terraform does with `terraform graph` and what makes Terraform users unwilling to go back to raw cloud API scripts. But Terraform requires a separate command. Nika could do it *live, in the editor, as you type*.

### What Python Cannot Match

| Nika LSP Feature | Python Equivalent | Advantage |
|---|---|---|
| Schema-validated completions | Copilot guessing | Deterministic, always correct |
| DAG preview | Manual tracing | Visual, instant, updated on every edit |
| Cost estimation | None | Unique to workflow engines |
| Cross-task type checking | Runtime errors | Caught at edit time |
| Provider compatibility | Runtime errors | Caught at edit time |
| One-click task scaffold | Copy-paste from docs | 10x faster |

---

## Question 6: Accessibility Features LSPs Need

### The Current State of LSP Accessibility

**VS Code Issue #142532: "Make inlay hints accessible"** (closed, implemented)

The VS Code team recognized that inlay hints were completely invisible to screen readers. The fix involved:
- Making inlay hints navigable via keyboard (Tab through hints)
- Adding ARIA labels to hint elements
- Providing an "accessible view" that reads hint content aloud
- Adding audio cues (configurable sounds) when the cursor enters/exits a region with hints

**VS Code Issue #158915: "Shared noise for different cases"**

Users requested different audio cues for different diagnostic severities. A single "beep" for both errors and information-level diagnostics provides no useful signal to a screen-reader user.

### What Nika's LSP Must Support

**1. Color-Blind Friendly Diagnostics**

LSP diagnostic severity maps to colors in the editor:
- Error: red underline
- Warning: yellow underline
- Information: blue underline
- Hint: gray (barely visible)

For deuteranopia (~8% of males), red and green are indistinguishable. For protanopia, red appears as dark brown/gray.

The LSP itself does not control colors (the editor does), but it CAN:
- Use diagnostic *tags* (`DiagnosticTag::Unnecessary`, `DiagnosticTag::Deprecated`) which some editors render with additional visual cues (strikethrough, fade)
- Set `DiagnosticRelatedInformation` to provide additional context that doesn't rely on color alone
- Use the `CodeDescription` field to link to documentation, providing an alternative to color-coded severity
- Ensure diagnostic messages include the severity level in text: "Error NIKA-020: ..." vs just "NIKA-020: ..."

**Recommendation for Nika**: Prefix all diagnostic messages with severity:

```
[ERROR] NIKA-020: Cycle detected in task dependencies
[WARN]  NIKA-071: Task 'step3' output is never referenced
[HINT]  NIKA-301: Consider adding timeout for agent task
```

This ensures the severity is communicated even without color.

**2. Screen Reader Support for Inlay Hints**

When Nika adds inlay hints (DAG order, cost estimates, model info), they must be:
- Accessible via the VS Code "accessible view" (Alt+F2 on macOS)
- Announced with sufficient context: not just "3 of 5" but "Task step3: executes 3rd of 5 tasks, depends on step1 and step2"
- Navigable via keyboard without interfering with normal editing

The LSP protocol's `InlayHint` type supports:
- `tooltip`: Full description for hover/screen reader
- `paddingLeft` / `paddingRight`: Visual spacing that also helps screen readers parse boundaries
- `kind`: `Type` or `Parameter` (limited, but helps screen readers categorize)

**Recommendation for Nika**: Always set `tooltip` on inlay hints with a full, descriptive sentence. Never rely on the hint's short label alone.

**3. High Contrast Mode**

VS Code has built-in high contrast themes. The LSP must:
- Test all semantic token types in high contrast themes
- Ensure semantic token modifiers (e.g., "deprecated") produce visible changes in high contrast
- Verify that diagnostic underlines are visible against high contrast backgrounds

**Recommendation for Nika**: Add a CI test that renders Nika semantic tokens in all VS Code built-in themes and verifies minimum contrast ratios. This is not an LSP-server concern (the editor renders tokens), but the extension's `contributes.semanticTokenScopes` mapping must be tested.

**4. Keyboard-Only Navigation**

Critical paths that must work without a mouse:
- Tab through diagnostic markers (already supported by VS Code)
- Navigate between tasks via document symbols (Ctrl+Shift+O)
- Trigger code actions via keyboard (Ctrl+.)
- Open hover documentation via keyboard (Ctrl+K Ctrl+I)
- Jump to definition via keyboard (F12)

**Recommendation for Nika**: Ensure the document symbols handler returns a hierarchical structure (workflow > tasks > task fields) that makes keyboard navigation logical. The current Nika LSP `symbols.rs` handler should be audited for this.

**5. Reduced Motion**

Some users have `prefers-reduced-motion` enabled. The LSP cannot control editor animations directly, but:
- Avoid code actions that trigger animations (e.g., "preview workflow" opening an animated webview)
- Prefer static DAG images over animated graph layout
- Ensure diagnostic updates don't cause visible "flickering" (batch diagnostic publishes rather than sending one-at-a-time)

**6. Cognitive Accessibility**

Beyond physical accessibility:
- Error messages should be written at a reading level appropriate for non-native English speakers
- Avoid jargon in diagnostic messages ("DAG cycle" should also say "tasks depend on each other in a loop")
- Provide examples in hover documentation (show what correct code looks like, not just what's wrong)
- Use consistent terminology (never mix "task" and "step" and "job" in diagnostic messages)

**Recommendation for Nika**: Write a diagnostic message style guide with these rules:
1. First sentence: what is wrong, in plain English
2. Second sentence: why it matters
3. Third sentence (or link): how to fix it
4. Always include the NIKA-XXX code for searchability

Example:
```
Task 'combine' creates a dependency loop (NIKA-020).
Tasks that depend on each other in a circle can never execute.
Remove one of these dependencies: combine -> research -> combine.
```

---

## Synthesis: The Dangerous Middle

The six questions converge on a single insight:

**The dangerous middle is where most LSPs fail.**

```
           Too little                           Too much
              |                                     |
   "Why bother with an LSP?     "I disabled everything,
    It doesn't help me."         it was too noisy."
              |                                     |
              |        THE DANGEROUS MIDDLE          |
              |    "Some features are great,         |
              |     but I can't tell which          |
              |     ones to trust."                 |
              |                                     |
```

- Too little intelligence: the LSP is just a syntax checker. The user could use the Red Hat YAML server and get the same experience. No differentiation.
- Too much intelligence: the LSP second-guesses every decision. The user disables it or, worse, stops trusting its errors because it cried wolf on its suggestions.
- The dangerous middle: the LSP has good features AND noisy features, and the user cannot easily distinguish them. Trust decays for the whole system.

**The solution is not "be smart" or "be conservative." The solution is radical transparency about confidence levels.** When the LSP says something, the user must instantly know:
1. Is this a fact about my code (structural analysis) or an opinion (heuristic)?
2. Can I trust this? (accuracy track record)
3. Can I dismiss this? (one click, never see it again for this pattern)
4. Can I learn more? (link to documentation)

---

## Concrete Recommendations for Nika

### Tier 1: Foundation (Before Public LSP Release)

| # | Feature | Rationale |
|---|---|---|
| 1 | Three-level inspection setting (`essential`/`recommended`/`comprehensive`) | Progressive disclosure without per-feature complexity |
| 2 | Severity prefix in all diagnostic messages | Accessibility: works without color |
| 3 | Diagnostic messages follow the 3-sentence pattern | Cognitive accessibility, reduces false-positive frustration |
| 4 | Document symbols return hierarchical structure | Keyboard navigation, screen reader support |
| 5 | `CodeDescription.href` links on all NIKA-xxx diagnostics | Users can always learn more |

### Tier 2: Differentiation (What Makes Nika's LSP Special)

| # | Feature | Rationale |
|---|---|---|
| 6 | DAG order inlay hints (`[1/5] Independent`, `[3/5] Depends on: a, b`) | The "can't go back" feature -- visual execution order at edit time |
| 7 | Task scaffold completions (full-form verb snippets) | Zero-to-working faster than scripts |
| 8 | Cross-task binding validation | Catch data flow errors at edit time, not runtime |
| 9 | Provider compatibility checking | Unique to workflow LSPs |
| 10 | "Serial bottleneck" detection (Level 3, off by default) | High-confidence optimization hint |

### Tier 3: Intelligence (Cautious, Evidence-Based)

| # | Feature | Rationale |
|---|---|---|
| 11 | Workflow pattern classification (content pipeline, ETL, etc.) | Context-aware suggestions |
| 12 | Missing error handling detection | Best practice, high confidence |
| 13 | Unbounded agent loop warning | Safety, high confidence |
| 14 | Cost estimation inlay hints | Unique differentiator, but accuracy varies |
| 15 | Model mismatch suggestions | Low confidence, only in `comprehensive` mode |

### Tier 4: Never (Anti-Patterns to Avoid)

| # | Anti-Pattern | Why Not |
|---|---|---|
| 1 | Auto-refactoring without consent | Users must explicitly accept changes |
| 2 | Prompt quality scoring | Too subjective, no ground truth |
| 3 | "You should use X model instead of Y" as a warning | Models change; the LSP is not a model advisor |
| 4 | Breaking changes to diagnostic behavior in minor versions | Destroys trust in CI pipelines that use `nika check` |
| 5 | Real-time LLM analysis of workflow semantics | Latency, cost, and privacy concerns |

### Implementation Priority

```
Phase 1 (v0.13): Tier 1 complete + items 6, 7 from Tier 2
Phase 2 (v0.14): Tier 2 complete + items 12, 13 from Tier 3
Phase 3 (v0.15+): Tier 3 items based on user feedback data
```

---

## Sources

1. rust-analyzer Issue #3267: "Disable deduced types or make it on-hover only" -- https://github.com/rust-lang/rust-analyzer/issues/3267 (24 upvotes, 12 comments, matklad design philosophy quote)
2. rust-analyzer Issue #4549: "disable all inlay hints, but still see implementations" -- https://github.com/rust-lang/rust-analyzer/issues/4549 (7 upvotes)
3. VS Code Issue #142532: "Make inlay hints accessible" -- https://github.com/microsoft/vscode/issues/142532
4. VS Code Issue #158915: "Shared noise for different cases" (accessibility, audio cues) -- https://github.com/microsoft/vscode/issues/158915
5. VS Code Issue #247435: "Inlay hints command has title 'Inline', which is confusing" -- https://github.com/microsoft/vscode/issues/247435
6. JetBrains IntelliJ Inspections documentation -- https://www.jetbrains.com/help/idea/code-inspection.html (severity levels, profiles, scopes, Qodana CI integration)
7. Red Hat yaml-language-server (1,417 stars) -- https://github.com/redhat-developer/yaml-language-server (schema validation, diagnostic suppression via comments, custom tags)
8. textLSP (86 stars) -- https://github.com/hangyav/textLSP (Grammarly CoEdIT integration, LanguageTool via LSP, analyzers disabled by default)
9. Grammarly CoEdIT models -- https://huggingface.co/grammarly (coedit-large, coedit-xl-composite, edit-operation training)
10. Stack Overflow 2024 Developer Survey -- https://survey.stackoverflow.co/2024/ (62% use AI tools in development, 76% plan to use AI tools)
11. VS Code Accessibility Documentation -- https://code.visualstudio.com/docs/editor/accessibility
12. Nika LSP source audit: `src/lsp/` (14 files, handlers for completion, hover, code_action, definition, symbols, semantic_tokens)
13. Nika VS Code extension: `editors/vscode/package.json` (current settings: server.path, server.extraArgs, trace.server -- no inspection level settings yet)
14. VS Code 1.67 Release Notes: Toggle Inlay Hints command -- https://code.visualstudio.com/updates/v1_67#_toggle-inlay-hints

---

## Methodology

- Tools used: GitHub API (issue search, comment retrieval), Hacker News Algolia API (story/comment search), curl (documentation scraping), Nika source code audit
- Issues analyzed: 7 (rust-analyzer: 2, VS Code: 3, Red Hat YAML: referenced in docs)
- Source repositories examined: 5 (rust-analyzer, VS Code, yaml-language-server, textLSP, Nika)
- Developer survey data: Stack Overflow 2024 (65,437 respondents)

## Confidence Level

**High** for Questions 2 (LSP friction), 4 (progressive disclosure), and 6 (accessibility) -- these are well-documented with primary sources.

**Medium** for Questions 1 (intent-aware LSP) and 3 (semantic understanding) -- the concepts are sound but largely unproven in production workflow-DSL LSPs.

**Medium-High** for Question 5 (competitive advantage) -- the features described are technically feasible and differentiated, but "can't go back" claims require user validation.
