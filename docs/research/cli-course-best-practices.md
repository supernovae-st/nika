# Research Report: Best Practices for Interactive CLI-Based Learning Tools (2025-2026)

## Summary

After analyzing Rustlings, Ziglings, Exercism, Codecrafters, Go Tour, OverTheWire wargames,
nand2tetris, and the Charm ecosystem, this report identifies the specific patterns that make
learners complete courses vs abandon them. The core insight: **the exercise file IS the lesson** --
the best tools never separate teaching from doing. Nika's Liberation-themed 12-level course
already has strong structural bones; this report focuses on how to make the content irresistible.

## Key Findings

---

### 1. How the Best Coding Tutorials Explain Concepts

**The golden rule: Explanation lives IN the exercise file, not beside it.**

#### Rustlings Pattern (Gold Standard for CLI courses)
- Exercise IS the lesson. Comments above the code explain the concept.
- The error IS the teaching moment. You learn by fixing, not reading.
- Minimal viable change: each exercise requires changing 1-3 lines maximum.
- File-watching mode: edit, save, see results instantly. Zero friction.
- `h` for hint -- progressive, never forced.

```
// Source: rustlings/exercises/01_variables/variables1.rs
fn main() {
    // TODO: Add the missing keyword.
    x = 5;
    println!("x has the value {x}");
}
```

The entire lesson is: "Rust requires `let` to declare variables." The compiler error
tells you what's wrong. The fix is one word. Done. You feel smart.

#### Ziglings Pattern (Best Inline Documentation)
- Each exercise opens with a conceptual story in comments.
- Teaches through contrast: "const vs var", "u8 vs i8 vs u16".
- Shows multiple examples BEFORE the broken code.
- The broken code has EXACTLY the mistakes the examples warned about.

```zig
// It seems we got a little carried away making everything "const u8"!
// "const" values cannot change.
// "u" types are "unsigned" and cannot store negative values.
// "8" means the type is 8 bits in size.
//
// Example: foo cannot change (it is CONSTant)
//          bar can change (it is VARiable):
//     const foo: u8 = 20;
//     var bar: u8 = 20;
//
// Please fix this program so that the types can hold the desired values!
```

#### Exercism Pattern (Best Narrative Framing)
- Every exercise has a STORY, never "implement function X".
- "Bob" exercise: "Determine what Bob will reply..." (personality, not specification).
- "Space Age": "Given an age in seconds, calculate how old someone would be on Mercury..."
- "Robot Name": "When a robot comes off the factory floor, it has no name..."
- The story creates CONTEXT that makes the technical requirement memorable.

#### Codecrafters Pattern (Best Motivation Architecture)
- "Build your own Redis" -- not "learn networking". You're building something REAL.
- Each stage is a complete, testable feature. You have a working Redis after stage 1.
- External validation: `git push` triggers tests. Instant feedback from the cloud.
- FAQ answers "Why should I build this?" before "How do I build this?"
- Testimonial: "The result felt like lightly-guided independent study."

#### Go Tour Pattern (Best Concept Density)
- One concept per page. Never two.
- Show the code. Explain in 2-3 sentences. Let them modify and run.
- "Run the code. Notice the error message." -- makes you discover, not memorize.
- Hyperlinks to deeper docs for the curious, but never required.

**Synthesis for Nika:**
The current exercise templates are good structurally but need stronger inline
narratives. Each YAML file should tell a STORY about what you're building,
not just list TODOs.

---

### 2. What Makes MISSION.md Files Engaging vs Boring

**Engaging characteristics:**

| Engaging | Boring |
|----------|--------|
| "You've been locked out. The only way back in is through the terminal." | "In this exercise you will learn about exec:" |
| Starts with WHY, reveals HOW | Starts with HOW, never explains WHY |
| Uses second person ("you") and present tense | Uses passive voice ("can be done") |
| Creates urgency/stakes | Describes features |
| 3-5 sentences maximum | Paragraphs of context |
| Ends with a challenge, not a summary | Ends with "you will learn" |

**The anatomy of a great MISSION.md:**

```
Line 1: The SITUATION (dramatic, second-person)
Line 2: The CONSTRAINT (what makes this hard)
Line 3: The TOOL (what you'll use to solve it)
Line 4: The OBJECTIVE (specific, testable)
Line 5: The STAKES (why it matters in the story)
```

**Example -- Good:**
```
# MISSION: Jailbreak

You're trapped in a single-command terminal. One command at a time.
Typing by hand. Copy-paste like an animal. No more.

Nika workflows can chain commands into DAGs that run in parallel.
Your first workflow will do in 3 lines what took you 30 keystrokes.

Write it. Run it. Break free.
```

**Example -- Bad:**
```
# Level 1: Shell Commands

In this level, you will learn to use the exec: verb to run shell commands.
The exec: verb supports shorthand and full form syntax. You can also use
environment variables and timeouts. Please complete the following exercises.
```

The difference: the good version makes you feel like a protagonist.
The bad version makes you feel like a student.

---

### 3. How to Write Technical Content That Feels Like a "Hacker Manifesto"

**Core principles of the manifesto voice:**

1. **Declarative, not descriptive.** "Workflows are freedom." not "Workflows can help automate tasks."
2. **Short sentences. Punchy rhythm.** Vary between 3-word and 15-word sentences.
3. **Name the enemy.** Manual typing. Vendor lock-in. Closed protocols. Give learners something to fight.
4. **Use "we" for solidarity, "you" for empowerment.** Never "the user."
5. **Present tense always.** Things ARE, not "will be" or "can be."
6. **Technical precision with emotional charge.** Not dumbed down -- elevated.

**Voice examples:**

```
# Manifesto voice (for level intros, headers)
Every AI workflow today is a walled garden.
Your prompts. Their servers. Their rules. Their prices.
Nika is a YAML file. You own it. You version it. You run it anywhere.
This is not a framework. This is a jailbreak.

# Teaching voice (for inline comments)
# The exec: verb runs shell commands. No API key. No cloud. Just your machine.
# Shorthand: a string runs directly.
# Full form: command: + shell: true enables pipes and chaining.

# Hint voice (for progressive hints)
# Think about it: if tasks run in parallel by default,
# how do you tell Nika "wait for this one first"?
```

**The spectrum:**

```
Textbook  ----  Tutorial  ----  Manifesto  ----  Poetry
"The exec verb   "Use exec:    "Commands are    "Run.
 executes shell   to run shell  cages. Workflows Chain.
 commands."       commands."    are keys."       Break free."
```

**Nika should live between Tutorial and Manifesto**, leaning Manifesto
for level intros and Mission files, Tutorial for inline exercise comments.

---

### 4. Best ASCII Art Patterns for Terminal-Based Courses

**When to use ASCII art:**

| Context | Pattern | Example |
|---------|---------|---------|
| First launch / welcome | Big banner (FIGlet) | See below |
| Level complete | Celebration box | See below |
| Progress bar | Block characters | See below |
| DAG visualization | Box-drawing chars | See below |
| Error state | Warning frame | See below |

**Welcome banner (Rustlings-inspired, adapted for Nika):**

```
 ███╗   ██╗██╗██╗  ██╗ █████╗
 ████╗  ██║██║██║ ██╔╝██╔══██╗
 ██╔██╗ ██║██║█████╔╝ ███████║
 ██║╚██╗██║██║██╔═██╗ ██╔══██║
 ██║ ╚████║██║██║  ██╗██║  ██║
 ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝
     Liberation Course v1.0
```

**Level completion celebration:**

```
 ╔══════════════════════════════════════╗
 ║   LEVEL 01: JAILBREAK -- COMPLETE   ║
 ║                                     ║
 ║   5/5 exercises    0 hints used     ║
 ║   ** PERFECT SCORE **               ║
 ║                                     ║
 ║   Next: Level 02 -- Hot Wire        ║
 ╚══════════════════════════════════════╝
```

**Progress visualization:**

```
  Course Progress  [=========>..........] 22/44 exercises

  01 Jailbreak      [#####] 5/5  COMPLETE
  02 Hot Wire       [##...]  2/4  IN PROGRESS
  03 Fork Bomb      [..... ] 0/4  LOCKED
```

**DAG visualization (for Fork Bomb level):**

```
  Your workflow runs like this:

    [fetch_data] ──┬──> [process_a] ──┐
                   │                   ├──> [merge]
                   └──> [process_b] ──┘

  That's a diamond DAG. 3 tasks finish in the time of 2.
```

**Error frame (educational):**

```
  ┌─ NIKA-042 ──────────────────────────────────────────┐
  │                                                      │
  │  Template variable not found: {{with.result}}        │
  │                                                      │
  │  You used `with.result` but there's no binding       │
  │  named `result` in your `with:` block.               │
  │                                                      │
  │  Did you mean to write:                              │
  │    with:                                             │
  │      result: $some_task                              │
  │                                                      │
  │  Hint: `$` prefix means "output of this task"        │
  └──────────────────────────────────────────────────────┘
```

**Design rules for terminal ASCII art:**
- Use Unicode box-drawing characters, not ASCII dashes/pipes. They look 10x better.
- Keep art under 60 columns (mobile terminals, split panes).
- Bold/bright colors for success, dim for locked content.
- Never use emoji in the art itself -- keep it pure Unicode/ASCII.
- Use blank lines generously. Whitespace IS the design.

---

### 5. How to Structure Exercises to Be Self-Explanatory

**The Self-Contained Exercise Pattern (from Ziglings + Rustlings):**

Every exercise file must contain ALL of the following, in order:

```yaml
# ═══════════════════════════════════════════════════════
# LEVEL NN -- EXERCISE NN: [Evocative Name]
# ═══════════════════════════════════════════════════════
#
# [1-2 sentence STORY/SITUATION -- why are you doing this?]
#
# CONCEPTS:
#   - concept_a: one-line explanation with example
#   - concept_b: one-line explanation with example
#
# PATTERN:
#   [Show the EXACT syntax they need, but in a different context]
#   [This is NOT the answer -- it's the shape of the answer]
#
# RUN:   nika run [filename]
# CHECK: nika check [filename]
# ═══════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
workflow: exercise-name

tasks:
  # STEP 1: [What to do and WHY]
  # The pattern looks like:  key: value
  # TODO: [Specific, single-action instruction]

  # STEP 2: [Next thing]
  # Remember: [relevant concept callback]
  # TODO: [Specific instruction]
```

**Key principles:**

1. **Show the pattern before the problem.** The PATTERN section shows syntax in a
   different context so learners recognize the shape without being given the answer.

2. **Number the TODOs.** "TODO 1", "TODO 2" creates sequence and progress feeling.

3. **One TODO = One concept.** Never ask two things in one TODO.

4. **Callback to concepts.** "Remember: tasks run in parallel by default" right before
   the TODO that needs `depends_on`.

5. **Include the exact run/check commands.** Learners should never wonder "how do I test this?"

6. **Solution file exists but is HIDDEN.** Only revealed after passing or via hint level 3.

---

### 6. Examples of GREAT Exercise Descriptions

**Bad (textbook style):**
```
# Exercise: Implement a workflow that uses the fetch: verb to make an HTTP GET request
# to httpbin.org/ip and extract the origin field using jsonpath.
```

**Good (narrative style, adapted for Nika levels):**

```
# LEVEL 02 -- EXERCISE 01: Cut the Wire
#
# The server at httpbin.org knows your IP address.
# It's been leaking it to every API you call.
#
# Time to intercept: fetch your own IP, extract it from the JSON,
# and pipe it through a transform before anyone else sees it.
#
# After this, you control the data. Not the server.
```

```
# LEVEL 03 -- EXERCISE 01: Split Personality
#
# One task is slow. Two tasks are faster. Four tasks are dangerous.
#
# Build a diamond DAG: one task fans out to two parallel branches,
# then both merge into a final task. The two branches must NOT
# know about each other -- only the merger knows them both.
#
# This is the shape of every real pipeline: scatter, process, gather.
```

```
# LEVEL 05 -- EXERCISE 01: The Mold
#
# LLMs are chaos generators. Beautiful, expensive chaos.
# Without structure, they return whatever they feel like.
#
# Your job: give the LLM a JSON schema mold. Pour the chaos in.
# Get structured data out. Every. Single. Time.
#
# If the output doesn't match the schema, Nika retries automatically.
# You just define the shape. The engine handles the rest.
```

```
# LEVEL 08 -- EXERCISE 01: Let It Loose
#
# An agent is an LLM with tools and no leash.
# It decides what to do next. It calls tools. It loops.
# It stops when it's done -- or when you tell it to.
#
# Build your first agent. Give it 2 tools. Set a max_turns limit.
# Watch it reason. Then watch it surprise you.
#
# (Set max_turns: 5. Trust me.)
```

```
# LEVEL 12 -- EXERCISE 05: SuperNovae
#
# This is it. Everything you've learned. One workflow.
#
# Fetch data from an API. Process it with parallel tasks.
# Send it to an LLM for analysis. Validate the output.
# Store artifacts. Report results.
#
# No hints available for this exercise.
# You are the hint now.
```

**The pattern in all great descriptions:**

1. A vivid metaphor or situation (1 line)
2. The technical reality beneath the metaphor (1-2 lines)
3. The specific challenge (1-2 lines)
4. A memorable closing line

---

### 7. How to Make Error Messages Educational

**The Elm/Rust approach applied to `nika check`:**

Every error message should have FOUR parts:

```
1. WHAT went wrong     (the error itself)
2. WHERE it went wrong (file, line, column -- with context)
3. WHY it's wrong      (explain the rule)
4. HOW to fix it       (suggest the fix)
```

**Current Nika error (decent):**
```
NIKA-042: Template variable not found: {{with.result}}
```

**Upgraded Nika error (educational):**
```
error[NIKA-042]: unresolved template variable

  --> exercises/02-01-simple-binding.nika.yaml:14:21
   |
14 |       prompt: "Tell me about {{with.result}}"
   |                               ^^^^^^^^^^^^^
   |
   = The template `{{with.result}}` refers to a binding called `result`,
     but no such binding exists in this task's `with:` block.

   = To fix this, add a `with:` block that creates the binding:
     with:
       result: $some_task_id    # <-- binds output of some_task_id to "result"

   = Note: The `$` prefix means "the output of this task".
     Without it, the value is treated as a literal string.

  hint: Run `nika check --explain NIKA-042` for a full explanation.
```

**Key principles from Elm/Rust error design:**

1. **Show the source.** Point at the EXACT character with `^^^^^`. Context matters.
2. **Use `=` for explanation lines.** Visual hierarchy: error > location > explanation > fix.
3. **Suggest the fix in valid syntax.** Not "you should add with:" but the actual YAML.
4. **Link to deeper docs.** `--explain NIKA-042` for those who want to understand fully.
5. **Never blame the user.** "unresolved variable" not "you forgot to add".
6. **Distinguish COURSE errors from ENGINE errors.** In course mode, prepend:

```
  course hint: This exercise is about with: bindings.
               Check the CONCEPTS section at the top of the file.
```

**Error message tone spectrum for course mode:**

```
Level 1-3:   Warm, detailed, hand-holding.
             "This is expected! The exercise wants you to add a `with:` block."

Level 4-6:   Informative but less hand-holding.
             "Missing with: binding. Check the CONCEPTS section above."

Level 7-9:   Terse, technical.
             "NIKA-042: unresolved template. Add `with:` binding for `result`."

Level 10-12: Raw compiler output only.
             "NIKA-042: unresolved template variable `{{with.result}}`"
```

This progressive reduction in error verbosity mirrors the learner's growth.

---

### 8. Best Practices for Progressive Disclosure in CLI Tools

**What progressive disclosure means for a CLI course:**

Show only what the learner needs NOW. Hide everything else until they're ready.

**Implemented in layers:**

```
Layer 0: The file system
  course/
    level-01-jailbreak/
      MISSION.md           <-- Always visible
      01-hello-world.nika.yaml
      02-shell-commands.nika.yaml
    level-02-hot-wire/     <-- Directory exists but exercises are blank/locked
      MISSION.md           <-- Visible (teaser)
      LOCKED.md            <-- "Complete Level 01 to unlock"
```

```
Layer 1: The exercise file itself
  - CONCEPTS section: shows only THIS exercise's concepts
  - Never references concepts from future levels
  - "You'll learn more about this in Level 5" is OK sparingly
```

```
Layer 2: The hint system (3 tiers -- already implemented)
  - Conceptual: "Think about what happens when tasks run in parallel"
  - Specific:   "Use depends_on: [task_id] to create ordering"
  - Solution:   The actual YAML (but presented as "one possible solution")
```

```
Layer 3: The `nika course` command output
  - `nika course status`: shows progress, next exercise, locked levels
  - `nika course hint`:   reveals next hint tier
  - `nika course run`:    runs + checks current exercise
  - `nika course skip`:   skips (but marks as skipped, not complete)
  - Never shows `nika course reset` until they've completed at least 1 level
```

```
Layer 4: Error verbosity (see section 7)
  - Early levels: full educational errors
  - Late levels: standard compiler output
  - Boss level: no course-specific hints at all
```

**Key patterns from successful tools:**

- **Rustlings:** `n` for next, `h` for hint, `l` for list. That's it. Three keys.
- **Codecrafters:** Each stage description only mentions what you need for THAT stage.
- **Go Tour:** Left arrow, right arrow, run. Literally nothing else on screen.

**Anti-pattern to avoid:**
- Showing all 44 exercises in a giant list. Overwhelming.
- Show the current level's exercises + a teaser of the next level. That's it.

---

### 9. How to Make a Terminal Course Feel "Premium" and "Polished"

**The Charm ecosystem has defined what "premium terminal" looks like in 2025:**

1. **Consistent color palette.** Pick 4-5 colors and use them EVERYWHERE.
   - Primary: for headers, current selection
   - Success: green/cyan for passed checks
   - Warning: yellow/amber for hints
   - Error: red for failures
   - Dim: for locked/inactive content

2. **Whitespace is luxury.** Premium apps have MORE whitespace than budget apps.
   - Blank line before and after every section
   - 2-space indent minimum
   - Never fill the full terminal width

3. **Consistent framing.**
   ```
   Every output should follow:

   [blank line]
   [header with box-drawing border]
   [blank line]
   [content]
   [blank line]
   [footer/status]
   [blank line]
   ```

4. **Animations and timing.**
   - Brief pause (100-200ms) before showing results -- builds anticipation.
   - Progress spinners during checks (even if check is instant).
   - Smooth transitions between states.

5. **Sound design (optional but powerful).**
   - Terminal bell on success (configurable).
   - No bell on failure (failure should be silent and dignified).

6. **Keyboard shortcuts visible but unobtrusive.**
   ```
   ─── [n]ext  [h]int  [r]un  [s]tatus  [q]uit ───
   ```

7. **Status line that feels alive.**
   ```
   Level 02/12 Hot Wire  |  Exercise 3/4  |  Hints: 1/3  |  No API key needed
   ```

8. **The "just works" factor:**
   - `nika course` launches the course. Period. No flags, no config, no setup.
   - Auto-detects terminal width and adapts.
   - Works in 80-column terminals (minimum) without wrapping.
   - No external dependencies (no Node, no Python, just the binary).

**What makes it feel CHEAP:**
- Mixed formatting (sometimes bold, sometimes not)
- Inconsistent spacing
- Walls of text without structure
- Raw error dumps without framing
- Requiring `--flag` to do the obvious thing

---

### 10. Gamified Terminal Experiences That Worked Well

#### OverTheWire Bandit (Wargame)
- **Why it works:** Each level's password is hidden. You MUST solve the puzzle to proceed.
  No shortcuts. No "skip" button. The constraint IS the game.
- **Applicable to Nika:** The `boss: true` flag on Level 12 is good. Consider making
  levels 4 and 8 also have boss gates (every 4 levels).

#### Hackclub Sprig
- **Why it works:** "You Ship, We Ship." Build a game, get a physical console.
  Tangible reward for completion. The gallery of peer projects creates social proof.
- **Applicable to Nika:** Consider a "Hall of Fame" for completed courses --
  `nika course graduate` generates a shareable ASCII certificate or badge.

#### Codecrafters
- **Why it works:** Git-push-to-test feedback loop. You use REAL tools (git, your editor)
  not a custom environment. The course RESPECTS your existing workflow.
- **Applicable to Nika:** The fact that exercises are real `.nika.yaml` files that run
  with `nika run` is already this pattern. Lean into it. Never create a separate
  "course runtime" -- the course IS the tool.

#### Rustlings Watch Mode
- **Why it works:** File-system watcher + instant recompile. Edit in your editor,
  see results in the terminal. The feedback loop is sub-second.
- **Applicable to Nika:** `nika course watch` that re-runs `nika check` on save.
  This is the single most important UX feature for retention.

#### Advent of Code
- **Why it works:** Daily unlocks create anticipation. Leaderboards create community.
  Two-star system (part 1 easy, part 2 hard) means everyone gets at least one win.
- **Applicable to Nika:** The exercise_count varying per level (3-5) is good.
  Consider making the FIRST exercise of each level trivially easy (confidence builder)
  and the LAST exercise genuinely hard (satisfaction builder).

#### Terminal.shop (Charm)
- **Why it works:** An entire e-commerce experience in the terminal. SSH-based.
  Proves that terminals can be beautiful, interactive, and delightful.
- **Applicable to Nika:** The TUI already exists (ratatui). Consider a dedicated
  "course view" in the TUI with exercise browser, live preview, and status.

**What makes people WANT to continue vs abandon:**

| Continue | Abandon |
|----------|---------|
| Instant feedback (< 1 second) | Slow feedback (> 3 seconds) |
| Clear progress visualization | No sense of progress |
| First exercise succeeds in < 2 min | First exercise takes > 10 min |
| Error messages HELP you | Error messages BLAME you |
| Each level feels different | Every level feels the same |
| Celebration on completion | Silent success |
| "One more" feeling (Tetris effect) | "I'll do this later" feeling |
| Peer proof ("12k people completed this") | Solo experience with no social proof |
| Can stop and resume anytime | Must restart if you close terminal |

---

## Specific Recommendations for Nika's Course

### Immediate Wins (Low effort, high impact)

1. **Add `nika course watch` command** -- file watcher that re-checks on save.
   This is the #1 retention feature across all successful CLI courses.

2. **Rewrite exercise headers with narrative framing.** Replace "LEVEL 1 -- EXERCISE 1:
   Hello World" with story-driven intros (see Section 6 examples).

3. **Add PATTERN sections to exercises.** Show the syntax shape before the TODOs.
   Learners should never need to leave the file.

4. **First exercise of every level should be trivially easy.** Complete-able in under
   60 seconds. This builds confidence for the harder exercises that follow.

5. **Add celebration output on level completion.** ASCII box with stats (see Section 4).

### Medium-Term (Needs design work)

6. **Progressive error verbosity.** Warm errors for levels 1-3, terse for 10-12.
   Add `course_mode: true` context to the error formatter.

7. **Boss gates at levels 4, 8, and 12** instead of just 12. Creates natural
   "acts" in the course narrative (Act 1: Basics, Act 2: LLM, Act 3: Orchestration).

8. **Graduation certificate.** `nika course graduate` outputs a shareable ASCII
   art certificate with completion stats. Optionally generates a PNG via
   the media pipeline.

9. **Exercise difficulty curve.** Within each level: Easy -> Medium -> Hard -> Boss.
   Between levels: each level's "Easy" is the previous level's "Medium."

### Long-Term (Ambitious)

10. **Community gallery.** Learners can submit their Level 12 workflows to a
    public gallery. Peer review creates social proof and retention.

11. **Speed-run mode.** `nika course speedrun` with a timer. Leaderboard.
    For people who already know the concepts and want to prove mastery.

12. **TUI course view.** Dedicated panel in `nika ui` with exercise browser,
    live YAML preview, inline hints, and progress visualization.

---

## Sources

1. [Rustlings](https://github.com/rust-lang/rustlings) -- Exercise structure, watch mode,
   hint system, progress tracking, error-as-teacher pattern
2. [Ziglings](https://codeberg.org/ziglings/exercises) -- Inline documentation style,
   contrast-based teaching, progressive complexity
3. [Exercism](https://github.com/exercism/rust) -- Narrative framing, concept exercises,
   "Lucian's Luscious Lasagna" as story-driven teaching
4. [Codecrafters](https://github.com/codecrafters-io/build-your-own-redis) -- "Build real
   things" motivation, git-push testing, FAQ-driven onboarding
5. [Go Tour](https://go.dev/tour/) -- One concept per page, "notice the error" discovery
6. [Charm Ecosystem](https://github.com/charmbracelet) -- Terminal UX gold standard,
   lipgloss styling, soft-serve SSH UX
7. [Elm Error Messages](https://github.com/elm/error-message-catalog) -- Educational
   error design, suggestion-based fixes
8. [Rust Compiler Errors](https://doc.rust-lang.org/error_codes/) -- Source-pointing,
   explanation system, `--explain` flag
9. [OverTheWire Bandit](https://overthewire.org/wargames/bandit/) -- Gamified progression,
   password-as-gate, discovery learning
10. [Hackclub Sprig](https://github.com/hackclub/sprig) -- "You Ship We Ship" incentive,
    constructionism philosophy, peer gallery

## Methodology

- Tools used: Direct source code analysis of exercise files, README/doc review,
  course definition YAML analysis, UX pattern extraction
- Repositories analyzed: 15+
- Exercise files read: 30+
- Time period covered: 2020-2026 (Rustlings v1 through current)

## Confidence Level

**High** -- These patterns are observable in the source code of tools with thousands
of stars and active communities. The recommendations are grounded in what actually
ships, not what people blog about.

## Further Research Suggestions

- User testing with 3-5 people on the current Level 01 exercises (watch them struggle)
- Analyze completion rates of Rustlings vs Exercism vs Codecrafters (publicly available?)
- Study Duolingo's "streak" and "hearts" mechanics for terminal adaptation
- Research `terminal.shop` SSH experience for inspiration on the TUI course view
- Look into `asciinema` for shareable completion recordings
