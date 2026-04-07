# 72 Hours, 56 Commits: The Sprint That Changed Nika

*Podcast script -- ~2500 words, ~10 minutes read time*
*Speaker: Thibaut Melen, founder of SuperNovae Studio*

---

## [COLD OPEN]

Fifty-six commits. Four hundred and ninety-six files changed. A hundred thousand lines added. In seventy-two hours.

[pause]

I know. I know what that sounds like. It sounds like someone who needs to go outside. And honestly... yeah, probably. But let me tell you what those seventy-two hours actually produced, because I think there's something worth talking about here -- not just the code, but the kind of pressure that turns a project from "this is promising" into "this is ready."

I'm Thibaut. I'm building Nika -- a workflow engine for AI tasks, written in Rust, launching open source on May fifth. And last weekend, something clicked. Not a eureka moment, not some brilliant insight. More like... the engine shifted into a gear I didn't know it had. So let me walk you through it.

---

## [SECTION 1 -- THE IDE REVOLUTION]

Let's start with the biggest thing. The VS Code extension.

Now, if you've ever used a YAML-based tool -- Kubernetes, GitHub Actions, Ansible -- you know the pain. You write your config, you run it, something fails, and now you're playing detective. Switching between your editor and the terminal, trying to figure out which step broke and why.

With Nika, your workflows can have ten, twenty, thirty tasks. They form a DAG -- a directed acyclic graph. Tasks depend on each other, run in parallel, fan out, fan in. And until this weekend, the way you understood what was happening was... you read the logs. Like an animal.

[beat]

So here's what we did. We rebuilt the extension from scratch. Not a refactor -- a redesign.

The centerpiece is the DAG webview. When you run a workflow now, you see it. Live. Every task is a node. When a task starts, its node lights up. When it completes, it goes green. When it fails, it goes red. You're watching your AI pipeline execute in real time, right there in your editor.

But the part I'm actually most proud of? Click-to-source. You see a node in the DAG -- say, your `research` task is failing. You click it. It jumps you straight to that task definition in the YAML file. Line number, cursor positioned. Done. No hunting. No scrolling through a three-hundred-line workflow trying to find where `research` is defined. Click. You're there.

Behind the scenes, this works through custom LSP notifications. We defined a new protocol message -- `nika/executionEvent` -- that streams task status updates from the language server to the webview in real time. It's not polling. It's not watching a file. It's a proper, typed notification channel.

And here's what made this possible: we embedded the daemon. Previously, the VS Code extension needed an external Nika daemon running. You had to start it separately, make sure it was healthy, handle reconnection if it died. It was friction. Every single piece of friction is a reason for someone to close the tab and go back to Python.

Now? The LSP starts an in-process daemon. You open VS Code, you open a `.nika.yaml` file, and everything just works. Your API keys resolve automatically -- first from environment variables, then from the daemon, then from the encrypted vault. Zero configuration. Zero setup. You write your workflow, you run it, you watch it execute. That's it.

Oh, and the old `extension.ts`? It was a thousand-and-sixty-one-line monolith. One file doing everything. We decomposed it into four clean modules -- binary installer, LSP client, MCP configuration, and the DAG panel. The main file is now three hundred and seventeen lines. It's readable. It's testable. We added twelve vitest tests for the workflow parser alone.

We even added Windsurf auto-detection. If you open Nika in Windsurf instead of VS Code, it auto-configures MCP. Because why would we make you do that manually?

I deleted two thousand four hundred lines of an obsolete VS Code scaffold that was sitting around doing nothing. Dead code. Gone. In a v0.x project, dead code is not "maybe we'll need this later." It's noise. It's confusion. It's a lie in your repository.

---

## [SECTION 2 -- THE GREAT DECOMPOSITION]

Okay, let's talk about engineering discipline. Because this is the part that nobody tweets about but everybody benefits from.

Nika's runner -- the core execution engine -- was a seven-thousand-line Rust file. `runner.rs`. Seven thousand lines. One file.

Now, look -- it got there honestly. When you're building fast, when you're a solo founder iterating on a design that's changing every week, sometimes the fastest path forward is to put everything in one place. You can see it all. You can reason about it. You're not jumping between fifteen files trying to understand a call chain.

But there's a moment when that stops being an advantage and starts being a liability. When the file is so long that your editor's minimap is useless. When you can't hold the structure in your head anymore. When you're scared to refactor because you might break something three thousand lines away that you forgot about.

We hit that moment. So we did the surgery.

First extraction: `structured_retry.rs`. Three hundred and twenty-four lines. This is the five-layer structured output defense -- the system that takes an LLM response, validates it against a JSON schema, retries with feedback if it fails, and even does LLM-powered repair as a last resort. It's one of Nika's killer features, and it deserves its own file with its own test suite.

Second extraction: `task_dispatch.rs`. Six hundred and fifty-six lines. This is the verb routing -- the code that looks at a task and says, "okay, this is an `infer:`, route it to the LLM provider" or "this is a `fetch:`, send the HTTP request." Five verbs, nine providers, structured output, retries -- all the dispatch logic, cleanly separated.

Third: the tests. Four thousand eight hundred and twenty-five lines of tests, moved to `runner/tests.rs`. The main module went from seven thousand lines to twenty-two hundred. That's a seventy-two percent reduction. And we didn't delete a single test. We didn't skip a single assertion. The test count went up, not down.

This is what I mean by engineering discipline. It's not glamorous. It's not a feature you put on a landing page. But when you're maintaining a two-hundred-thousand-line Rust codebase alone, the difference between a well-factored module and a monolith is the difference between moving fast and moving not at all.

---

## [SECTION 3 -- MULTI-TENANT AUTH AND MCP EXPANSION]

Now, let's talk about `nika serve`.

Nika has a serve mode. You run `nika serve`, it starts an HTTP server, and you can trigger workflows via API. It's how Nika integrates with the rest of the world -- your Next.js app calls the API, Nika runs the workflow, streams results back via Server-Sent Events.

Until now, auth was... basic. One token. Set `NIKA_SERVE_TOKEN` in your environment, and every request needs to include it. That's fine for a solo developer. It's not fine for a team. It's not fine for production.

So we built a real auth system. `TokenStore` backed by SQLite, with BLAKE3 hashing and a moka cache for hot-path lookups. New CLI commands: `nika token add`, `nika token list`, `nika token revoke`. You can give each team member their own token. You can revoke access without rotating everyone else's credentials. The old single-token mode still works -- we call it Legacy mode. The new system is MultiKey mode. Clean migration. V6 schema. No breaking changes.

On the MCP side -- MCP is the protocol Nika uses to talk to external tools and to NovaNet, our knowledge graph -- we added three new tools and three prompt templates.

The tools: `generate_task` helps you scaffold a new task from a description. `dag_visualization` returns the DAG structure for any workflow. `error_fix` takes a Nika error code and suggests fixes. These are tools that AI assistants can call. So when you're working with Claude or Copilot and you say "add a translation task to my workflow," the assistant can call `generate_task` through MCP and get a properly structured task definition. Not a hallucinated one. A real one, validated against the schema.

The prompt templates -- `create-workflow`, `add-task`, `fix-error` -- are the companion pieces. They give AI assistants the context they need to help you effectively. Nine new tests covering all of it.

---

## [SECTION 4 -- ZERO OPT-IN]

Here's a philosophy I keep coming back to: one binary, everything included.

When you `cargo install nika`, or you `brew install nika`, or you download the release binary -- you get everything. Every feature. Every tool. Every provider. Sixty-three builtin tools. Nine LLM providers. Media processing. Content credentials. PDF extraction. SVG rendering. All of it.

The last holdout was `media-provenance` -- C2PA content credentials, the standard for proving where AI-generated content came from. It was behind a feature flag because the dependency was heavy and we weren't sure about the compile-time cost. Well, we measured it. It's fine. Flag removed. Bundled by default.

Zero opt-in. Zero feature flags. Zero "oh, you wanted *that* feature? Rebuild with `--features media-provenance`." No. You wanted Nika. You got Nika. All of it.

This matters because the competition -- and let's be honest, the competition is LangChain and the Python ecosystem -- the competition has you installing twelve packages, managing version conflicts, setting up virtual environments, and hoping that `pip install langchain[all]` doesn't break your system Python for the third time this month.

One binary. Static linking. No runtime dependencies. It works on your MacBook, it works on your CI server, it works on your Scaleway VPS. Same binary. Same features. Every time.

---

## [SECTION 5 -- THE ROAD TO MAY FIFTH]

So where does this leave us?

We're at v0.75. We have ten thousand three hundred and sixty-five tests passing. The engine handles five verbs, sixty-four transforms, sixty-three builtin tools, nine LLM providers. The extension is live. The DAG visualization works. Auth is multi-tenant. The binary is zero-config.

May fifth is the launch date. Twenty-eight days from when I'm recording this.

What's left? Honestly? It's mostly polish and documentation. The auth system needs rate limiting. The LSP needs a few more diagnostic features. We need to finish the twelve-level tutorial course -- it's a built-in learning system, `nika init --course`, that teaches you the engine from first principles. Forty-four exercises. Interactive. In your terminal.

And then there's the launch itself. The Show HN post. The documentation site. The showcase -- a hundred and fifteen example workflows covering everything from podcast production to multilingual content pipelines to competitive research.

But the engine? The engine is done. Not "done" as in "we stopped working on it." Done as in... it does what we said it would do. You write a YAML file. You describe your AI workflow. Five verbs, clear semantics. And Nika runs it. Against any provider. With structured output that actually validates. With a DAG that handles parallelism and dependencies. With retries and error recovery and artifacts and a media pipeline.

That's what seventy-two hours looks like when you're four weeks from launch and the adrenaline is doing the engineering for you.

[pause]

Fifty-six commits. I should probably sleep.

But first -- one more `cargo test --workspace --lib`.

[end]

---

*Runtime: ~10 minutes at 250 WPM*
*Recorded: April 2026, Paris*
