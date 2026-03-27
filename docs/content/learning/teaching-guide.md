# Teaching Guide -- Instructor Reference

> How to teach Nika effectively in workshops, classrooms, and self-study groups. Includes lesson plans for 1-day, 2-day, and 5-day formats.

---

## Audience Profiles

### Profile A: Developer (Backend/Full-Stack)
- Knows YAML, CLI, APIs
- Wants to automate tasks and integrate LLMs
- Learns fast, wants hands-on immediately
- **Start**: Level 1, skip conceptual intros, go straight to exercises

### Profile B: DevOps/Platform Engineer
- Knows YAML well (Kubernetes, Terraform, CI/CD)
- Focused on automation, monitoring, infrastructure
- Cares about DAG execution, parallelism, reliability
- **Start**: Level 1 quickly, deep dive on Levels 3 (DAG) and 11 (media/resilience)

### Profile C: AI/ML Engineer
- Familiar with LLM APIs
- Wants orchestration, structured output, agents
- May not know YAML deeply
- **Start**: Brief YAML intro, then Levels 4-8 (LLM features)

### Profile D: Technical PM / No-Code Adjacent
- Basic technical literacy
- Wants to understand what Nika can do
- Needs more conceptual explanation
- **Start**: Conceptual overview, then Level 1 with pair programming

---

## Lab Setup Instructions

### Minimum Requirements
- macOS, Linux, or WSL2 on Windows
- Nika binary installed (`brew install supernovae/tap/nika`)
- A text editor with YAML support (VS Code recommended)
- Terminal access
- Internet connection (for `fetch:` exercises)

### Recommended Setup
```bash
# 1. Install Nika
brew install supernovae/tap/nika

# 2. Verify installation
nika --version

# 3. Set up at least one LLM provider
export ANTHROPIC_API_KEY="sk-ant-..."   # Or any other provider
nika provider list

# 4. Generate the course
mkdir ~/nika-workshop && cd ~/nika-workshop
nika init --course

# 5. Verify course is ready
nika course status
```

### VS Code Extensions
- YAML Language Support (Red Hat)
- Nika LSP (available via marketplace)

### Common Lab Issues
| Issue | Solution |
|-------|---------|
| "command not found: nika" | Ensure Homebrew bin is in PATH |
| "NIKA-032 Missing API key" | Export the provider's API key |
| "NIKA-001 Failed to parse" | Check YAML indentation (2 spaces) |
| Keychain popup on macOS | Use `cargo test --lib`, never `cargo test` |
| Slow fetches | Check internet; httpbin.org may be slow |

### Budget Considerations
- Levels 1-3, 7, 9: Zero LLM cost (exec, fetch, invoke only)
- Levels 4-6, 8: Moderate LLM cost (~$0.10-0.50 per student)
- Levels 10-12: Higher cost (~$0.50-2.00 per student)
- **Tip**: Use Groq (free tier) or local models for budget-constrained workshops

---

## 1-Day Workshop (6 hours)

### Target Outcome
Students can write multi-task workflows using all 5 verbs, with basic bindings and DAG patterns.

### Schedule

| Time | Duration | Topic | Activity |
|------|----------|-------|----------|
| 09:00 | 15 min | Welcome and Overview | Slide deck: what is Nika, 5 verbs diagram |
| 09:15 | 15 min | Lab Setup | Install, provider setup, generate course |
| 09:30 | 45 min | Level 1: Jailbreak | Exercises 1-3 (schema, exec, fetch) |
| 10:15 | 15 min | Break | |
| 10:30 | 45 min | Level 2: Hot Wire | Exercises 1-3 (with:, templates, transforms) |
| 11:15 | 30 min | Level 3: Fork Bomb | Exercises 1-2 (DAG, parallel) |
| 11:45 | 15 min | Q&A + Discussion | Common mistakes review |
| 12:00 | 60 min | Lunch | |
| 13:00 | 45 min | Level 4: Root Access | Exercises 1-2 (infer:, providers) |
| 13:45 | 45 min | Level 6: Pay-Per-Dream | Exercise 1 (structured output) |
| 14:30 | 15 min | Break | |
| 14:45 | 30 min | Level 7: Swiss Knife | Exercise 1 (builtin tools) |
| 15:15 | 30 min | Level 8: Gone Rogue | Exercise 1 (basic agent) |
| 15:45 | 45 min | Capstone Project | Students build their own workflow |
| 16:30 | 30 min | Presentations + Wrap-up | Show and tell, Q&A, next steps |

### Instructor Notes
- Skip Level 5 (Shapeshifter) in 1-day format -- cover transforms during Level 2
- Skip Level 9-12 entirely -- point to course for self-study
- Focus on "Try it yourself" time -- students learn by doing
- Have the cheat sheet printed or projected at all times
- Keep the showcase list open for inspiration during the capstone

---

## 2-Day Workshop (12 hours)

### Target Outcome
Students can build production-grade workflows with agents, guardrails, and structured output.

### Day 1 Schedule

| Time | Duration | Topic |
|------|----------|-------|
| 09:00 | 30 min | Welcome, Overview, Lab Setup |
| 09:30 | 60 min | Levels 1-2: exec, fetch, with:, templates |
| 10:30 | 15 min | Break |
| 10:45 | 45 min | Level 3: DAG patterns, for_each |
| 11:30 | 45 min | Level 4: infer:, providers, LLM pipelines |
| 12:15 | 60 min | Lunch |
| 13:15 | 45 min | Level 5: Transforms deep dive |
| 14:00 | 60 min | Level 6: Structured output, JSON schemas |
| 15:00 | 15 min | Break |
| 15:15 | 45 min | Day 1 Project: Build a "Website Analyzer" |
| 16:00 | 30 min | Review, Q&A, Day 2 preview |

### Day 2 Schedule

| Time | Duration | Topic |
|------|----------|-------|
| 09:00 | 15 min | Day 1 recap |
| 09:15 | 60 min | Level 7: Builtin tools, file tools |
| 10:15 | 15 min | Break |
| 10:30 | 75 min | Level 8: Agents, completion, guardrails |
| 11:45 | 15 min | Q&A |
| 12:00 | 60 min | Lunch |
| 13:00 | 60 min | Level 9: Fetch extraction (markdown, metadata, links) |
| 14:00 | 45 min | Level 11: Media pipeline (import, thumbnail, vision) |
| 14:45 | 15 min | Break |
| 15:00 | 75 min | Capstone: Production workflow from scratch |
| 16:15 | 45 min | Presentations, peer review, wrap-up |

---

## 5-Day Workshop (30 hours)

### Target Outcome
Students achieve expert-level proficiency and build a portfolio of production workflows.

### Weekly Schedule

| Day | Theme | Levels | Project |
|-----|-------|--------|---------|
| Mon | Foundations | 1-3 | System Health Dashboard |
| Tue | LLM Mastery | 4-6 | Git PR Summary Generator |
| Wed | Tools & Agents | 7-8 | Code Review Agent |
| Thu | Extraction & Media | 9-11 | Content Pipeline |
| Fri | Orchestration | 12 + Expert | Full-Stack Capstone |

### Daily Structure (6 hours)

| Time | Duration | Activity |
|------|----------|----------|
| 09:00 | 15 min | Review yesterday, today's goals |
| 09:15 | 90 min | Concept teaching + guided exercises |
| 10:45 | 15 min | Break |
| 11:00 | 60 min | Exercises from the exercise bank |
| 12:00 | 60 min | Lunch |
| 13:00 | 90 min | Project work (individual or pairs) |
| 14:30 | 15 min | Break |
| 14:45 | 45 min | Advanced topic or showcase exploration |
| 15:30 | 30 min | Show and tell + Q&A |

### Day-by-Day Detail

**Monday -- Foundations**:
- Morning: Levels 1-2 (exec, fetch, with:, templates)
- Afternoon: Level 3 (DAG, for_each, parallel execution)
- Project: System Health Dashboard
- Advanced: Showcase exploration -- run 3 exec/fetch showcase workflows

**Tuesday -- LLM Mastery**:
- Morning: Level 4 (infer:, providers, model selection)
- Late morning: Level 5 (transforms, structured output)
- Afternoon: Level 6 (multi-provider, JSON schemas)
- Project: Git PR Summary Generator
- Advanced: Cost optimization, provider comparison

**Wednesday -- Tools & Agents**:
- Morning: Level 7 (builtin tools, file tools, sub-workflows)
- Afternoon: Level 8 (agents, completion, guardrails)
- Project: Code Review Agent with guardrails
- Advanced: Multi-agent chaining patterns

**Thursday -- Extraction & Media**:
- Morning: Level 9 (all 9 extract modes)
- Afternoon: Level 11 (media pipeline, 24 tools, vision)
- Project: Content Pipeline with web scraping
- Advanced: Level 10 (MCP integration, NovaNet)

**Friday -- Orchestration**:
- Morning: Level 12 boss exercises (all 5)
- Afternoon: Full capstone project (student's choice)
- Show and tell: Each student presents their capstone
- Wrap-up: Portfolio review, next steps, community resources

---

## Slide Suggestions

### Slide 1: What is Nika?
- "Semantic YAML workflow engine for AI tasks"
- One binary, one file format, one schema line
- 5 verbs: exec, fetch, infer, invoke, agent
- Diagram: YAML file --> DAG --> Parallel Execution --> Results

### Slide 2: The 5 Verbs
- Visual table with verb, purpose, LLM required (yes/no)
- Code example for each verb (shorthand form)

### Slide 3: The DAG
- ASCII art diamond pattern
- "Tasks without dependencies run in parallel automatically"
- Before (sequential): 5 API calls x 500ms = 2500ms
- After (parallel): 5 API calls x 500ms = 500ms

### Slide 4: Data Flow
- `with:` block diagram: Task A output --> alias --> Template --> Task B
- Template syntax: `{{with.alias | transform}}`
- Three rules: $ prefix, alias naming, {{with.alias}} rendering

### Slide 5: Agent Architecture
- Loop diagram: Prompt --> LLM --> Tool Call --> Result --> LLM --> ...
- Guardrails as validation gates
- Safety limits as circuit breakers

### Slide 6: The Course Map
- Constellation diagram of 12 levels
- Liberation theme names
- Progress tracking with star ratings

---

## Assessment Rubrics

### Exercise Assessment (per exercise)

| Criterion | Points | Description |
|-----------|--------|-------------|
| Passes `nika check` | 3 | No validation errors |
| No TODO markers | 2 | All placeholders replaced |
| Correct verb usage | 3 | Uses the right verb for the task |
| Proper bindings | 2 | `with:` and `depends_on:` correct |
| **Total** | **10** | |

### Level Assessment (per level)

| Criterion | Points | Description |
|-----------|--------|-------------|
| All exercises pass | 50 | Each exercise worth ~10 points |
| Bonus: no hints used | 10 | Completed without hints |
| Bonus: first try | 10 | Passed on first `nika course check` |
| Understanding Q&A | 30 | Can explain WHY, not just WHAT |
| **Total** | **100** | |

### Capstone Assessment

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Correctness | 25% | Workflow validates and runs |
| Complexity | 20% | Uses multiple verbs, DAG patterns |
| Creativity | 15% | Novel use case, elegant solution |
| Code quality | 15% | Clean YAML, good naming, comments |
| Presentation | 15% | Clear explanation of design decisions |
| Error handling | 10% | Timeouts, on_error, retries |
| **Total** | **100%** | |

---

## Common Student Questions

### "Why YAML instead of a programming language?"

YAML is declarative. You describe WHAT you want, not HOW to do it. This means:
- No boilerplate (imports, error handling, async/await)
- Automatic parallelism (the DAG scheduler handles it)
- Portability (any developer can read and modify it)
- No compilation step

### "How does Nika compare to LangChain / LlamaIndex?"

Nika is a workflow engine, not a framework. Key differences:
- No SDK, no library, no code dependencies
- YAML files are version-controlled, auditable, and shareable
- Built-in DAG execution with automatic parallelism
- Multi-provider support without code changes
- Agent guardrails are declarative, not imperative

### "Can I use my own LLM provider?"

Yes. Nika supports 9 providers via environment variables: Anthropic (Claude), OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, native (local GGUF), and others. If your provider has a compatible API, you can use it. The `native` provider runs local GGUF models with zero API cost.

### "Is it production-ready?"

Nika is pre-release (0.x.x versioning). The engine is functional and tested (8,100+ tests), but the API may change. It is suitable for internal automation, prototyping, and learning. Production deployment should account for the evolving schema.

### "What happens if a task fails?"

By default, a failed task stops the workflow. You can change this:
- `on_error: continue` -- mark failed but let other branches continue
- `retry:` -- automatically retry with backoff
- Agent guardrails -- `on_failure: retry` asks the agent to fix output

### "How much does it cost to run?"

- `exec:`, `fetch:`, `invoke:` (builtin): Zero cost
- `infer:`, `agent:`: Provider API costs apply (varies by model)
- `native` provider: Zero cost (runs locally)
- Cost control: `max_cost_usd`, `token_budget`, and `max_turns` prevent budget overruns

### "Can students work in pairs?"

Yes, pair programming works well. One student drives (writes YAML), the other navigates (reviews, suggests, checks docs). Rotate roles every exercise.

---

## Post-Workshop Resources

### For Students
- Run `nika showcase list` and extract 5 showcases to study
- Complete remaining course levels (the course persists between sessions)
- Try 10 exercises from the Exercise Bank (Chapter 6)
- Build one project from the Project Ideas (Chapter 7)
- Review Common Mistakes (Chapter 8) to avoid pitfalls
- Keep the Cheat Sheet (Chapter 9) as a daily reference

### For Instructors
- Collect feedback on which exercises were too easy/hard
- Track which error codes students hit most (NIKA-XXX)
- Note which concepts needed the most explanation
- Share student capstone projects as inspiration for future cohorts
- Report bugs or unclear behavior to the Nika repository

### Community
- GitHub: supernovae-st/nika
- Issues: Bug reports and feature requests
- Discussions: Questions and showcase sharing

---

## Teaching Tips

1. **Show, do not tell**: Run a workflow live before explaining any concepts. The "magic moment" is seeing `nika run` produce results.

2. **Start without LLMs**: Levels 1-3 use `exec:` and `fetch:` only. This avoids API key setup issues and lets students focus on workflow mechanics.

3. **Use the hint system**: Encourage students to use `nika course hint` before asking the instructor. The 3-tier system (conceptual, specific, solution) teaches self-sufficiency.

4. **Celebrate bonus achievements**: The course tracks "no hints" and "first try" bonuses. Make these visible to motivate clean solutions.

5. **Let errors happen**: Do not pre-correct student YAML. Let them hit `NIKA-020 Cycle detected` and understand why. Error-driven learning is effective.

6. **Pair the cheat sheet with exercises**: Project the cheat sheet on a second screen during exercise time. Students should always have the reference visible.

7. **Use showcases as examples**: When a student asks "how do I do X?", run `nika showcase list` and find a relevant example. This teaches self-discovery.

8. **Grade with `nika check`**: The check command is the grading tool. If it passes, the workflow is structurally correct. Use Q&A for conceptual understanding.

9. **Budget LLM time**: Give students a fixed token budget for the day. This teaches cost-consciousness and prevents runaway agents.

10. **End with show-and-tell**: The capstone presentation is where learning crystallizes. Students must explain their design decisions to peers.

---

*"The best teacher does not give answers. They create conditions where answers discover themselves."*
