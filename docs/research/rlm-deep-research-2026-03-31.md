# Deep Research Report: Recursive Language Models (RLM)

**Date**: 2026-03-31
**Researcher**: Thibaut / Claude Opus 4.6
**Scope**: RLM architecture, community reactions, related approaches, agent memory implications, TurboQuant

---

## 1. Executive Summary

Recursive Language Models (RLMs) are an inference-time paradigm from MIT OASYS Lab (Alex Zhang, Tim Kraska, Omar Khattab) that enables LLMs to handle near-infinite context by recursively calling themselves through a REPL environment. Instead of stuffing all context into a single prompt, the LLM programmatically examines, decomposes, and sub-queries its own input -- treating context as a variable rather than cramming it into the attention window. The approach achieves 2x+ improvement over GPT-5 on hard long-context benchmarks while being cost-competitive, and maintains perfect accuracy on 10M+ token inputs where all baselines degrade.

The TurboQuant paper (Google, ICLR 2026) addresses the complementary problem of KV cache compression, achieving 6x memory reduction at 3.5 bits/channel with zero quality loss -- but is embroiled in an attribution controversy with the RaBitQ team.

Both papers point toward a future where agent memory systems combine recursive decomposition (RLM-style) with aggressive KV compression (TurboQuant-style) to create agents with effectively unlimited, persistent memory.

---

## 2. RLM: Architecture Deep Dive

### 2.1 GitHub Repository

| Metric | Value |
|--------|-------|
| Repo | `alexzhang13/rlm` |
| Stars | 3,231 |
| Forks | 597 |
| Language | Python |
| License | MIT |
| Created | 2025-12-20 |
| Last Updated | 2026-03-31 |
| Paper | arXiv:2512.24601 |
| Authors | Alex Zhang, Tim Kraska, Omar Khattab (MIT) |

### 2.2 Core Architecture

RLM replaces `llm.completion(prompt, model)` with `rlm.completion(prompt, model)`. Under the hood:

```
User Query + Context
        |
        v
  +------------------+
  |  Root LM (depth=0) |  <-- Only sees the query, NOT the full context
  |  + REPL Environment |  <-- Context stored as Python variable
  +------------------+
        |
        | Iterative loop (max_iterations)
        v
  +------------------+
  | LM generates code |  <-- ```repl``` blocks
  | Code executes     |  <-- In sandboxed Python namespace
  | Results returned  |  <-- Truncated output back to LM
  +------------------+
        |
        | Can spawn:
        |
  +------------------+     +------------------+
  | llm_query()      |     | rlm_query()      |
  | (depth=1, plain) |     | (depth=1, w/REPL)|
  | Fast, one-shot   |     | Full recursive   |
  +------------------+     +------------------+
        |
        v
  FINAL(answer) or FINAL_VAR(variable_name)
```

**Key implementation details from `rlm/core/rlm.py`:**

1. **RLM class** -- the main entry point. Configurable with:
   - `max_depth` (default 1) -- controls recursion depth
   - `max_iterations` (default 30) -- REPL interaction rounds
   - `max_budget` / `max_timeout` / `max_tokens` / `max_errors` -- guardrails
   - `compaction` -- summarizes conversation when context hits 85% of limit
   - `max_concurrent_subcalls` (default 4) -- parallel child RLM threads
   - Event callbacks for live tracking

2. **Environment types**: `local` (default, Python exec), `docker`, `modal`, `prime`, `daytona`, `e2b`

3. **Communication**: Socket-based (4-byte length prefix + JSON). The `LMHandler` runs a threaded TCP server that routes LLM requests from the REPL environment.

4. **Compaction**: When root context reaches 85% of model limit, the system summarizes the trajectory, appends it to a `history` REPL variable, and resets the message history. This is essentially automatic context window management.

### 2.3 How Recursive Self-Invocation Works

From the system prompt and `local_repl.py`:

1. The REPL environment is initialized with:
   - `context` variable (the user's potentially huge input)
   - `llm_query(prompt, model=None)` -- single LLM call, no REPL, 500K char capacity
   - `llm_query_batched(prompts)` -- concurrent version
   - `rlm_query(prompt, model=None)` -- spawns a **child RLM** with its own REPL
   - `rlm_query_batched(prompts)` -- concurrent recursive calls
   - `SHOW_VARS()` / `FINAL_VAR(name)` -- introspection and output

2. The root LM **never sees the full context**. It only knows the context exists and its metadata (total chars, chunk count). It must write Python code to examine it.

3. When `rlm_query()` is called, the `_subcall()` method in `rlm.py` spawns a child RLM:
   - Child gets `depth = parent.depth + 1`
   - Child gets its own fresh REPL environment
   - Child inherits budget/timeout limits (remaining, not full)
   - At `max_depth`, falls back to plain `llm_query()`

4. The child runs its own iterative loop, returns an `RLMChatCompletion`, and the result flows back into the parent's REPL as a string.

### 2.4 How It Handles 100x Context Length

The mechanism is fundamentally different from attention-based approaches:

1. **Context as variable, not prompt**: A 10M token document is stored as a Python string in memory. The root LM's context window only ever contains the query + REPL interaction history.

2. **Programmatic access patterns** the LM discovers at runtime:
   - **Peeking**: `context[:2000]` to inspect structure
   - **Grepping**: `re.findall(pattern, context)` to narrow search
   - **Partition + Map**: chunk context, `llm_query_batched()` over chunks
   - **Summarization**: recursive summarize-and-merge

3. **No architecture changes needed**: Works with any API-based LLM. The context never enters any attention computation.

4. **Scaling property**: If a model handles N tokens well, an RLM can handle ~100N tokens because:
   - Root LM context stays small (query + REPL history)
   - Sub-LMs each get reasonable-sized chunks (~500K chars)
   - Recursive depth can extend this further

### 2.5 Benchmark Results

**OOLONG (trec_coarse, 128K tokens)**:
| Method | Score | Cost/Query |
|--------|-------|------------|
| GPT-5 | ~30 | baseline |
| GPT-5-mini | ~25 | cheaper |
| RLM(GPT-5-mini) | **~64** | ~same as GPT-5 |
| ReAct + GPT-5 + BM25 | ~15 | expensive |

RLM(GPT-5-mini) outperforms GPT-5 by **34 points (114% increase)**.

**BrowseComp-Plus (1000 documents, 10M+ tokens)**:
| Method | Accuracy |
|--------|----------|
| GPT-5 | 0% (exceeds context) |
| GPT-5 + BM25 | ~50% |
| ReAct + GPT-5 + BM25 | ~85% |
| RLM(GPT-5) no sub-calls | ~90% |
| RLM(GPT-5) | **100%** |

---

## 3. Community Reactions & Discussions

### 3.1 Reddit Sentiment (r/LocalLLaMA, r/MachineLearning)

**Positive reactions:**
- Recognition that it is a clean formalization of a pattern many practitioners use informally
- Excitement about the REPL + recursion combination being more powerful than either alone
- Multiple independent implementations emerging (minRLM, rig-rlm in Rust, rlm-rs CLI)
- The minRLM project achieves 3.6x fewer tokens with maintained performance

**Skeptical reactions (most common):**
- "This is a trivial idea that tens of thousands of people have had" (top comment, 8 upvotes)
- "The paper is correct but not especially novel. In agentic this is a common pattern."
- "It feels a bit like a bandaid and not a truly integral solution. Like a person having to use paper notes to augment limited cognitive skills."
- "Stop praising such 'paradigm summarization paper' that takes industry common practice and wraps it with fancy names"

**Nuanced takes:**
- "It sounds similar to what Claude Code does -- if you imagine the repo is the context, it proposes itself a question, generates a grep query, reads those, generates more depending on what it read"
- "The value isn't in the idea itself but in the formalization and benchmark results"
- "There's so much low hanging fruit to optimize here" -- especially async execution and prefix caching

### 3.2 Key Criticism Points

1. **Novelty**: Many argue this is just agentic code execution with a recursive twist -- something tools like Claude Code, Cursor, and MemGPT already do informally
2. **Speed**: No async execution, no prefix caching. Each recursive call is blocking. Can take minutes per query.
3. **Cost control**: No strong guarantees on total API cost or runtime per call
4. **Not a real "model"**: RLM is a scaffold/inference strategy, not a new model architecture. The name "Recursive Language Model" overstates what it is.

### 3.3 Related Implementations & Forks

| Project | Language | Stars | Key Difference |
|---------|----------|-------|----------------|
| `alexzhang13/rlm` | Python | 3,231 | Reference implementation |
| `alexzhang13/rlm-minimal` | Python | -- | Stripped-down version |
| `avilum/minrlm` | Python | -- | 3.6x fewer tokens, Docker sandboxing |
| `joshua-mo-143/rig-rlm` | **Rust** | 66 | Uses rig crate + pyo3 for REPL |
| `zircote/rlm-rs` | **Rust** | 27 | CLI with SQLite persistence, BM25 |
| `Diogenesoftoronto/axon` | **Rust** | 4 | "One context, run everywhere" |
| `NarendraPatwardhan/lua-rlm` | **Rust** | 1 | Uses Luau as sandboxed REPL |

---

## 4. Comparison with Related Approaches

### 4.1 RLM vs RAG (Retrieval-Augmented Generation)

| Dimension | RAG | RLM |
|-----------|-----|-----|
| Index required | Yes (pre-built) | No |
| Retrieval strategy | Fixed (BM25, embedding) | LLM-chosen at runtime |
| Multi-hop reasoning | Requires iterative retrieval | Natural via recursion |
| Context rot | Partially mitigated | Fundamentally avoided |
| Latency | Fast (single retrieval) | Slower (multiple LLM calls) |
| Cost | Low | Higher (multiple calls) |
| Handles novel queries | Limited by index | Fully adaptive |

**Key insight**: RLM makes RAG-like decisions at runtime without needing a pre-built index. On BrowseComp-Plus, RLM outperforms ReAct + BM25 because the LLM can discover what to search for adaptively.

### 4.2 RLM vs Infini-attention (Google, "Leave No Context Behind")

| Dimension | Infini-attention | RLM |
|-----------|-----------------|-----|
| Approach | Architecture change (compressive memory in attention) | Inference scaffold (no model changes) |
| Requires training | Yes | No |
| Works with API models | No (needs model internals) | Yes |
| Memory type | Fixed compressive memory | Programmable Python environment |
| Scaling | Bounded by compressive memory capacity | Theoretically unbounded via recursion |
| Production readiness | Research prototype | Already pip-installable |

### 4.3 RLM vs MemGPT/Letta

| Dimension | MemGPT | RLM |
|-----------|--------|-----|
| Memory model | OS-inspired (main memory + archival) | Code-first (Python variables) |
| Context management | Fixed strategy (page in/out) | LLM-chosen strategy |
| Recursion | No recursive sub-calls | Core feature |
| Persistence | Built-in | Optional (persistent mode) |
| Multi-turn | Primary focus | Supported but not primary |

### 4.4 RLM vs MemWalker / LADDER

- **MemWalker**: Imposes a tree structure for context summarization. RLM lets the LLM discover the optimal decomposition strategy.
- **LADDER**: Decomposes from problem perspective. RLM decomposes from context perspective -- a key philosophical difference.

### 4.5 RLM vs Agentic Tools (Claude Code, Cursor)

The blog post explicitly addresses this: "RLMs are not agents." The difference is:
- **Agents**: decompose by task/problem (human-designed tool use)
- **RLMs**: decompose by context (LLM-chosen context management)

In practice, Claude Code's grep-then-read pattern is very similar to what RLM does, but:
1. Claude Code's strategy is partially prescribed by the system prompt
2. RLM formalizes this as a general-purpose API replacement
3. RLM's recursive depth allows sub-agents to spawn their own sub-agents

---

## 5. TurboQuant: KV Cache Compression

### 5.1 Paper Overview

| Field | Value |
|-------|-------|
| Title | TurboQuant: Online Vector Quantization with Near-optimal Distortion Rate |
| Authors | Amir Zandieh, Majid Daliri, Majid Hadian, Vahab Mirrokni (Google) |
| Venue | ICLR 2026 |
| arXiv | 2504.19874 |

### 5.2 How It Works

TurboQuant is a two-stage approach:

**Stage 1 -- PolarQuant (MSE-optimal)**:
1. Apply random orthogonal rotation (Haar matrix) to input vector
2. This induces a concentrated Beta distribution on coordinates
3. Apply optimal scalar quantizer per coordinate (Lloyd-Max)
4. Result: near-optimal MSE distortion at any bit-width

**Stage 2 -- QJL Correction (unbiased inner product)**:
1. MSE-optimal quantizers introduce bias in inner product estimation
2. Apply 1-bit Quantized Johnson-Lindenstrauss transform on the residual
3. Result: unbiased inner product estimation

**Key results for KV cache**:
- **3.5 bits/channel**: absolute quality neutrality (zero loss)
- **2.5 bits/channel**: marginal quality degradation
- **6x memory reduction** for KV cache
- **Up to 8x speedup** in attention computation
- **15x compression** at 2 bits (with some quality trade-off)

### 5.3 Attribution Controversy

The RaBitQ team (Jianyang Gao, first author) published a detailed technical clarification on r/LocalLLaMA (598 upvotes) alleging:

1. **Incomplete method description**: TurboQuant omits that RaBitQ already uses the same random rotation (Johnson-Lindenstrauss) technique, making the methods look more different than they are
2. **Unsupported theoretical claims**: TurboQuant calls RaBitQ's guarantees "suboptimal" without justification, when RaBitQ already proved asymptotic optimality
3. **Unfair benchmarks**: RaBitQ baseline was run on single CPU with multiprocessing disabled, while TurboQuant ran on A100 GPU

The TurboQuant authors reportedly agreed to fix issues only after ICLR 2026 conference.

### 5.4 Community Response

- **RotorQuant** (512 upvotes on r/LocalLLaMA): Replaces the dense rotation matrix with Clifford algebra rotors. 10-19x faster, 44x fewer parameters, same cosine similarity (0.990 vs 0.991). Already has CUDA + Metal shader implementations.
- **llama.cpp implementation** by TheTom: 4.57x KV cache compression, enabling 72K context for Llama-70B on dual RTX 3090s
- Multiple Python reimplementations within days of publication

---

## 6. Implications for Agent Memory

### 6.1 If Agents Can Recursively Process Their Own Memory

The RLM pattern maps directly to agent memory management:

```
Traditional Agent Memory:
  [conversation history] -> [fixed context window] -> response

RLM-Inspired Agent Memory:
  [memory store: conversations, knowledge, experiences]
       |
       v
  [Root Agent: "What do I need to remember for this query?"]
       |
       v
  [Programmatic search over memory store]
       |
       | spawn sub-agents for:
       |   - "Summarize interactions with user X"
       |   - "Find all decisions about architecture Y"
       |   - "What were the outcomes of approach Z?"
       |
       v
  [Synthesized memory context] -> response
```

### 6.2 Selective Attention Over Memory Stores

RLM's approach suggests a memory architecture where:

1. **Memory is stored as addressable data**, not jammed into context
2. **The agent decides what to recall** via programmatic search (grep, filter, regex over memories)
3. **Sub-agents specialize in memory retrieval**: one could search episodic memory, another semantic memory, another procedural memory
4. **Hierarchical memory decomposition**: Recent memories get direct access. Older memories get summarized. Ancient memories get recursively summarized summaries -- but the agent can always drill down.

### 6.3 Hierarchical Memory Decomposition

Combining RLM + compaction gives a natural memory hierarchy:

```
Level 0: Working memory (current conversation, ~4K tokens)
Level 1: Session memory (compacted summaries of recent sessions, ~50K tokens)
Level 2: Episodic memory (searchable store of all sessions, unlimited)
Level 3: Semantic memory (extracted facts, relationships, preferences, unlimited)

Access pattern:
  Level 0: Always in context
  Level 1: Loaded on demand via compaction
  Level 2: Searched via llm_query() over chunks
  Level 3: Indexed and retrieved via rlm_query() with decomposition
```

### 6.4 Concrete Application to Nika

For Nika's agent verb, the RLM pattern suggests:

1. **Agent context is a variable, not a prompt**: The agent's accumulated knowledge from previous tasks should be stored in an addressable memory store, not concatenated into the system prompt.

2. **Recursive sub-agents for memory retrieval**: When an agent needs to recall information, it could spawn sub-agents that search through:
   - Previous task outputs (the DAG execution history)
   - Workflow execution traces
   - User preferences and patterns

3. **Compaction as a first-class feature**: When agent context grows large, automatic summarization + preservation of key facts would prevent the "context rot" problem that plagues long agent sessions.

4. **Memory-aware guardrails**: Track not just token usage but memory retrieval quality -- did the agent find the right memories?

### 6.5 TurboQuant's Role in Memory Compression

If agents maintain persistent KV caches as their "memory":

- **3.5 bits/channel** means 6x more memory in the same GPU/RAM budget
- An agent with 128K context at FP16 could store the equivalent of **768K tokens** of memory at TurboQuant 3.5-bit with zero quality loss
- At 2.5 bits/channel, this extends to **~1M tokens** with marginal degradation
- **RotorQuant** makes this practical on consumer hardware (Metal shaders for Apple Silicon)

This is especially relevant for local/edge agents where memory is the primary constraint.

---

## 7. Sources

| # | Source | What It Provided |
|---|--------|-----------------|
| 1 | [RLM Paper (arXiv:2512.24601)](https://arxiv.org/abs/2512.24601) | Full paper |
| 2 | [RLM GitHub](https://github.com/alexzhang13/rlm) | Source code, 3,231 stars |
| 3 | [RLM Blog Post](https://alexzhang13.github.io/blog/2025/rlm/) | Detailed explanation, results, visualizations |
| 4 | [r/LocalLLaMA RLM thread](https://reddit.com/r/LocalLLaMA/comments/1q3i75u/) | 30 comments, community skepticism |
| 5 | [r/MachineLearning RLM minimal](https://reddit.com/r/MachineLearning/comments/1rdglh2/) | Alternative implementations |
| 6 | [minRLM](https://reddit.com/r/LocalLLaMA/comments/1rw2n9f/) | 3.6x token efficiency improvement |
| 7 | [rig-rlm (Rust)](https://github.com/joshua-mo-143/rig-rlm) | Rust implementation using rig + pyo3, 66 stars |
| 8 | [rlm-rs (Rust)](https://github.com/zircote/rlm-rs) | Rust CLI with SQLite persistence, 27 stars |
| 9 | [TurboQuant (arXiv:2504.19874)](https://arxiv.org/abs/2504.19874) | KV cache compression paper |
| 10 | [TurboQuant MarkTechPost](https://www.marktechpost.com/2026/03/25/google-introduces-turboquant/) | Summary article |
| 11 | [RaBitQ clarification](https://reddit.com/r/LocalLLaMA/comments/1s7nq6b/) | 598 upvotes, attribution controversy |
| 12 | [RotorQuant](https://reddit.com/r/LocalLLaMA/comments/1s44p77/) | 512 upvotes, Clifford algebra optimization |
| 13 | [Infini-attention (arXiv:2404.07143)](https://arxiv.org/abs/2404.07143) | Google's compressive memory approach |
| 14 | [ASURA](https://reddit.com/r/MachineLearning/comments/1rfskth/) | Alternative recursive LM approach |

## 8. Methodology

- **Tools used**: GitHub API, arXiv, Reddit JSON API, Google Scholar, direct URL scraping
- **Pages analyzed**: 18 primary sources, 6 GitHub repos
- **Source code read**: `rlm.py` (859 lines), `prompts.py`, `types.py`, `local_repl.py`, `lm_handler.py`, `base_env.py`
- **Time period**: Papers from 2024-2026, discussions from 2025-2026

## 9. Confidence Level

**HIGH** on architecture and implementation details (read full source code).
**HIGH** on benchmark results (directly from paper and blog post).
**MEDIUM-HIGH** on community sentiment (sampled from Reddit, but Twitter/X threads were not accessible).
**MEDIUM** on TurboQuant specifics (paper abstract + community discussion; did not read full paper).
**MEDIUM** on agent memory implications (extrapolated from RLM patterns; no production validation).

## 10. Key Takeaways for Nika

1. **RLM validates the "agent with REPL" pattern** -- Nika's `agent:` verb with tool access is conceptually similar, but without recursive self-invocation. Adding `spawn_agent` from within agent tools would be the equivalent.

2. **Context as data, not prompt** -- The most valuable insight. Nika workflows already do this via `with:` bindings, but agent memory could benefit from this pattern at a deeper level.

3. **Compaction is essential** -- RLM's compaction (summarize + reset when at 85% context) is exactly what Nika should do for long-running agents. The current `max_turns` limit is a blunt instrument; compaction would be smarter.

4. **TurboQuant for native provider** -- If Nika's `provider: native` ever needs to handle longer contexts with GGUF models, TurboQuant-style KV cache compression could extend effective context length 4-6x. The RotorQuant variant is especially interesting for Metal (Apple Silicon) deployment.

5. **The real competition is not architectures but scaffolds** -- The community consensus is that RLM is not novel as an idea, but its formalization and benchmarks prove that the scaffold approach works better than architecture changes for practical infinite context. This validates Nika's approach of being a workflow scaffold rather than a model provider.
