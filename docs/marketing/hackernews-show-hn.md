# Show HN: Nika — Automate AI tasks in YAML, not Python (open source, Rust)

## Post Title

```
Show HN: Nika – 5 verbs. 22 providers. Zero Python. (open source, Rust)
```

## Post URL

```
https://github.com/supernovae-st/nika
```

---

## Post Body (~300 words)

Chaining two LLM calls today means installing Python, managing dependencies,
writing API client boilerplate, and debugging async code. A typical LangChain
project pulls in 150+ packages and idles at ~140 MB of RAM just to call an API
twice. I wanted something where you write a text file and run it.

Nika is an open-source workflow engine that runs AI tasks from YAML files. Five
verbs cover everything: `infer:` (LLM generation), `fetch:` (HTTP), `exec:`
(shell), `invoke:` (MCP tool calls), and `agent:` (autonomous multi-turn
loops). It ships as a single 15 MB Rust binary with zero runtime dependencies.

Here's a complete workflow that scrapes Hacker News and summarizes the front page:

```yaml
nika: workflow@0.12
name: hn-summary

tasks:
  scrape:
    fetch:
      url: https://news.ycombinator.com
      extract: article

  summarize:
    infer:
      model: claude/claude-sonnet-4-20250514
      prompt: "Summarize the top stories: {{with.page}}"
    with:
      page: $scrape
```

That's it. `nika run hn-summary.nika.yaml` and you get a summary. Swap
`claude/claude-sonnet-4-20250514` for `openai/gpt-4o` or `ollama/llama3` — no other
changes needed.

**Benchmarks** (measured, not marketing):

| | Nika | LangChain | CrewAI |
|---|---|---|---|
| Cold start | 4 ms | 62 ms | ~80 ms |
| RSS (typical workflow) | ~28 MB | ~140 MB | ~160 MB |
| Binary size | 15 MB | 150+ packages | 120+ packages |
| Runtime deps | 0 | Python 3.10+ | Python 3.10+ |

The DAG scheduler runs tasks in parallel automatically when there are no
dependencies. 451K lines of Rust, 10 crates, 7,800+ tests.

Honest limitations: Nika is pre-1.0 (schema @0.12). There's no web GUI — only a
terminal UI and CLI. YAML has a learning curve if you've never used it. The
ecosystem is young — no plugin marketplace yet.

- GitHub: https://github.com/supernovae-st/nika
- Docs: https://nika.supernovae.studio
- Quick start: `brew install supernovae-studio/tap/nika && nika init --course`
- License: AGPL-3.0 (the engine stays open; your YAML workflows are yours)

7,800+ tests. Single binary. Zero dependencies. Built with Rust.

---

## Prepared Responses for Common HN Criticism

### "YAML sucks"

> Fair criticism — YAML has real footguns (implicit typing, indentation
> sensitivity, the Norway problem). We mitigate this with strict schema
> validation, an LSP with real-time diagnostics, and clear error messages that
> point to the exact line and column.
>
> The tradeoff is intentional: YAML is readable by non-developers. A product
> manager can read a `.nika.yaml` file and understand what it does. That
> matters more to us than syntactic elegance. If you prefer code, the engine
> is embeddable as a Rust library — you can skip YAML entirely.

### "Why not Python?"

> Python is great for prototyping AI. But for automation — things you run
> repeatedly, in CI, on servers, on edge devices — the dependency story is
> painful. A typical LangChain project needs Python 3.10+, pip/conda, dozens
> of packages, and ~140 MB of RAM just to chain two API calls.
>
> Nika is a 15 MB static binary. No runtime, no virtualenv, no Docker.
> `curl | tar` and you're running. For teams where not everyone is a Python
> developer, that accessibility matters.

### "Rust is overkill for this"

> Rust gives us three things that matter here:
>
> 1. **Memory**: ~28 MB RSS vs ~140 MB for Python equivalents. On a $5 VPS
>    running 10 workflows, that's the difference between fitting in RAM or
>    not.
> 2. **Startup**: <50ms cold start. Important for CLI tools and CI pipelines.
> 3. **Correctness**: The type system catches entire categories of bugs at
>    compile time. Our DAG scheduler, template engine, and AST pipeline are
>    complex — Rust makes them reliable.
>
> The build time cost is real (~3 min clean build), but users never see it.
> They download a binary.

### "This looks like Airflow / Temporal / Prefect"

> Those are data/workflow orchestration platforms designed for ETL pipelines,
> microservice choreography, and batch processing at scale. They need a
> scheduler service, a database, often Kubernetes.
>
> Nika is a single-user CLI tool designed specifically for AI tasks. No
> server, no database, no infra. The closest analogy is `make` or `just`,
> but for LLM workflows instead of build steps. If you need distributed
> execution across a cluster, use Temporal. If you need to chain 5 AI calls
> on your laptop, use Nika.

### "Why AGPL?"

> Because I've watched too many open-source AI tools get absorbed by cloud
> platforms that contribute nothing back. AGPL means: use it freely, modify
> it freely, but if you offer it as a service, share your changes. Your
> private workflows are yours — the license applies to the engine, not your
> YAML files.
>
> For companies that need a commercial license without AGPL obligations,
> we'll offer one. But the default is: open stays open.

### "What about LangChain / CrewAI / AutoGen?"

> Those are Python frameworks for building AI agents programmatically. They're
> powerful and flexible. Nika is not trying to replace them for developers who
> want full programmatic control.
>
> Nika targets a different use case: declarative automation. You describe
> *what* you want, not *how* to wire it. The tradeoff is less flexibility for
> more simplicity. If your workflow fits in 5 verbs and a DAG, Nika is
> faster to write, faster to run, and easier to maintain. If you need custom
> Python logic at every step, use LangChain.
>
> The practical difference: a LangChain workflow is 50-200 lines of Python.
> The equivalent Nika workflow is 8-20 lines of YAML. Both are valid
> approaches for different people.

---

## Timing Notes

- Post between 8-9 AM ET on a weekday (Tuesday-Thursday preferred)
- Be available for 4-6 hours after posting to respond to comments
- Respond to every substantive comment, even critical ones
- Never be defensive — acknowledge valid criticism, explain tradeoffs
- Upvote thoughtful comments, even negative ones
