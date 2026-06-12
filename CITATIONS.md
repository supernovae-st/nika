# Citations — the research Nika stands on

Nika's static-analysis ladder, certificates, and memory satellites are
grounded in published research. Every claim of the form « per X et al. »
in this codebase was verified against the primary source before
implementation (no memory-cited papers — the retrieval protocol lives in
the studio's research discipline). This file credits everyone, mapped to
the module that implements or builds on their work.

## Static analysis & verification (`nika-schema/src/check/`)

| Work | Where it lands |
|---|---|
| Dorothy E. Denning, *A Lattice Model of Secure Information Flow*, CACM 1976 | `check/flow.rs` — the IFC taint lattice behind the secret-leak / egress analysis (ADR-092 #1) |
| Thomas M. Prinz, Christopher T. Schwanen, Wil M. P. van der Aalst, *Deciding Reachability and the Covering Problem with Diagnostics for Sound Acyclic Free-Choice Workflow Nets*, 2026 — [arXiv:2602.02447](https://arxiv.org/abs/2602.02447) | `check/reach.rs` — acyclicity makes gate-reachability polynomial **with diagnostics**; the « can only be {…} » detail follows their discipline (ADR-092 #6) |
| Michael Blondin, Filip Mazowiecki, Philip Offtermatt, *The complexity of soundness in workflow nets*, 2022 — [arXiv:2201.05588](https://arxiv.org/abs/2201.05588) | `check/reach.rs` — why the general problem is EXPSPACE-complete and Nika's acyclic-by-construction class is the tractability moat |
| Michael Blondin, Filip Mazowiecki, Philip Offtermatt, *Verifying generalised and structural soundness of workflow nets via relaxations*, 2022 — [arXiv:2206.02606](https://arxiv.org/abs/2206.02606) | background for the reachability design survey (relaxation techniques considered, not needed for the acyclic class) |
| Jan Hoffmann, Ankush Das, Shu-Chun Weng, *Towards Automatic Resource Bound Analysis for OCaml*, 2016 — [arXiv:1611.00692](https://arxiv.org/abs/1611.00692) | `check/certificate.rs` — AARA: resource bounds as polynomials parametric in input sizes; Nika reads the degree-1 coefficients directly off the structure (no LP solver — the workflow is its own typing derivation) (ADR-092 #7) |
| Ethan Chu, Yiyang Guo, Jan Hoffmann, *Handling Exceptions and Effects with Automatic Resource Analysis*, 2026 — [arXiv:2603.02260](https://arxiv.org/abs/2603.02260) | `check/certificate.rs` — the AARA line is active research; cited as the contemporary anchor |
| Ahmed Shokry, Amr Elmasry, Ayman Khalafallah, Amr Aly, *Verifying Shortest Paths in Linear Time*, 2024 — [arXiv:2412.06121](https://arxiv.org/abs/2412.06121) | `check/certificate.rs` — the certifying-algorithm discipline: emit (result, witness) with a checker simpler than the solver → `RunCertificate::{derivation, audit}` |
| Xiao-Yang Liu Yanglet, Xiaodong Wang, Agostino Capponi, *No Certificate, No Execution: Certified Traces as a Foundation for Trustworthy AI Agents*, 2026 — [arXiv:2605.24462](https://arxiv.org/abs/2605.24462) | the Proposal–Certification–Execution architecture (« generation is not permission ») that `nika check`'s check-before-run + re-checkable certificates feed |
| Jingwen Wu, Jiajing Zheng, Zhenyu Yang, Zhongxing Yu, *Compiler Optimization Testing Based on Optimization-Guided Equivalence Transformations*, 2025 — [arXiv:2504.04321](https://arxiv.org/abs/2504.04321) | `check/metamorphic.rs` — differential testing without a second engine: equivalence transformations on one system (ADR-092 #9 first slice) |
| Jinsheng Ba, Yuancheng Jiang, Manuel Rigger, *Metamorphic Coverage*, 2025 — [arXiv:2508.16307](https://arxiv.org/abs/2508.16307) | `check/metamorphic.rs` — the pairs-of-executions methodology lineage behind relations R0–R6 |
| Richard P. Brent, *The Parallel Evaluation of General Arithmetic Expressions*, JACM 1974 · Joseph Tassarotti, *Probabilistic Recurrence Relations for Work and Span of Parallel Algorithms*, 2017 — [arXiv:1704.02061](https://arxiv.org/abs/1704.02061) | `check/certificate.rs` — the work/span model: `span_attempts` (longest sequential chain · retries serial · fan-out parallel) alongside the work bound gives the Brent parallelism envelope |

## Memory satellites (the Connectome line)

| Work | Where it lands |
|---|---|
| Stephen E. Robertson, Steve Walker, *Some Simple Effective Approximations to the 2-Poisson Model for Probabilistic Weighted Retrieval*, SIGIR 1994 | `nika-bm25` — the canonical Okapi BM25 scorer (`scorer.rs` · `index.rs`) |
| Christopher D. Manning, Prabhakar Raghavan, Hinrich Schütze, *Introduction to Information Retrieval*, Cambridge UP 2008, ch. 11 | `nika-bm25` — invariants the property suite pins (saturation · monotonicity · non-negativity) |
| Yuhao Lù, *BM25S: Orders of magnitude faster lexical search via eager sparse scoring*, 2024 — [arXiv:2407.03618](https://arxiv.org/abs/2407.03618) | `nika-bm25/src/eager.rs` — `EagerIndex`: per-posting scores precomputed at freeze time, queries as sparse accumulation (byte-identical to the lazy path, pinned by property test) |
| Yuanhua Lv, ChengXiang Zhai, *Lower-Bounding Term Frequency Normalization*, CIKM 2011 | `nika-bm25` — `BmParams::delta` (BM25+) lower-bound smoothing, ACTIVE in both the lazy and eager paths (matching terms bounded below by `δ·idf`) |
| Gordon V. Cormack, Charles L. A. Clarke, Stefan Büttcher, *Reciprocal Rank Fusion outperforms Condorcet and individual rank learning methods*, SIGIR 2009 | the hybrid-retrieval fusion contract (`nika-rrf` · fuse ranks, never scores — the score-normalization note in `nika-bm25/src/lib.rs`) |
| Howard Turtle, James Flood, *Query evaluation: strategies and optimizations*, IP&M 1995 · arXiv anchor: Yifan Qiao, Yingrui Yang, Haixin Lin, Tao Yang, *Optimizing Guided Traversal for Fast Learned Sparse Retrieval*, 2023 — [arXiv:2305.01203](https://arxiv.org/abs/2305.01203) | `nika-bm25/src/eager.rs` — `EagerIndex::top_k_pruned`: MaxScore dynamic pruning (essential/non-essential term split · rank-exact · measured fewer postings visited) |

## Agent loop (`nika-verb-agent`)

| Work | Where it lands |
|---|---|
| Shunyu Yao, Jeffrey Zhao, Dian Yu, Nan Du, Izhak Shafran, Karthik Narasimhan, Yuan Cao, *ReAct: Synergizing Reasoning and Acting in Language Models*, 2022 — [arXiv:2210.03629](https://arxiv.org/abs/2210.03629) | the `agent` verb's multi-turn think→act→observe loop shape (turn-capped per the spec field table) |
| Noah Shinn, Federico Cassano, Edward Berman, Ashwin Gopinath, Karthik Narasimhan, Shunyu Yao, *Reflexion: Language Agents with Verbal Reinforcement Learning*, 2023 — [arXiv:2303.11366](https://arxiv.org/abs/2303.11366) | `src/guard.rs` — the bounded corrective nudge: verbal feedback in the transcript is the cheapest effective corrective; Nika bounds it (`max_reflections`) and triggers it deterministically (cycle / error-streak detection), no self-evaluation LLM pass (ADR-093) |
| Darshan Deshpande, Varun Gangal, Hersh Mehta, et al., *TRAIL: Trace Reasoning and Agentic Issue Localization*, 2025 — [arXiv:2505.08638](https://arxiv.org/abs/2505.08638) | `src/guard.rs` — repetitive-action loops as a dominant agentic failure class; the stall verdict (NIKA-467) carries the evidence (period · repeats) IN the trace |
| Xiang Fei, Xiawu Zheng, Hao Feng, *MCP-Zero: Active Tool Discovery for Autonomous LLM Agents*, 2025 — [arXiv:2506.01056](https://arxiv.org/abs/2506.01056) | `src/router.rs` — the active-discovery direction (don't inject every schema, surface tools per need); Nika's variant is SOVEREIGN: deterministic BM25 over the whitelisted universe via `nika-bm25`, zero extra LLM calls, fail-open |
| Shishir G. Patil, Tianjun Zhang, Xin Wang, Joseph E. Gonzalez, *Gorilla: Large Language Model Connected with Massive APIs*, 2023 — [arXiv:2305.15334](https://arxiv.org/abs/2305.15334) | `src/router.rs` — the retrieval-augmented tool-selection lineage (retriever-aware tool use) |
| Siyu Yuan, Kaitao Song, Jiangjie Chen, et al., *EASYTOOL: Enhancing LLM-based Agents with Concise Tool Instruction*, 2024 — [arXiv:2401.06201](https://arxiv.org/abs/2401.06201) | background — context pressure from verbose tool documentation; Nika selects definitions rather than rewriting them (descriptions stay verbatim — the source's own docs are the contract) |
| Xingyao Wang, Yangyi Chen, Lifan Yuan, et al., *Executable Code Actions Elicit Better LLM Agents*, 2024 — [arXiv:2402.01030](https://arxiv.org/abs/2402.01030) | `src/intrinsic.rs` — actions-as-code: the agent's «code» IS the Nika workflow language, so `agent:compose` gets the full static checker as its verifier for free |
| Zora Zhiruo Wang, Jiayuan Mao, Daniel Fried, Graham Neubig, *Agent Workflow Memory*, 2024 — [arXiv:2409.07429](https://arxiv.org/abs/2409.07429) | `src/intrinsic.rs` — the induce-reusable-workflows direction: a certified `agent:compose` draft is exactly the reusable-workflow artifact (delivered + checked, never self-executed); the `skill:` namespace is the forward seam for serving them back as tools |
| Guanzhi Wang, Yuqi Xie, Yunfan Jiang, et al., *Voyager: An Open-Ended Embodied Agent with Large Language Models*, 2023 — [arXiv:2305.16291](https://arxiv.org/abs/2305.16291) | the skill-library direction behind the `skill:` tool-source classification (`src/observe.rs` · stored workflows as retrievable, executable skills) |
| Xiao-Yang Liu Yanglet, Xiaodong Wang, Agostino Capponi, *No Certificate, No Execution*, 2026 — [arXiv:2605.24462](https://arxiv.org/abs/2605.24462) | `src/intrinsic.rs` — «generation is not permission» applied to self-composition: compose returns verdict + AARA certificate; execution stays a separate, gated decision |
| Liming Dong, Qinghua Lu, Liming Zhu, *AgentOps: Enabling Observability of LLM Agents*, 2024 — [arXiv:2411.05285](https://arxiv.org/abs/2411.05285) | `src/observe.rs` + `nika-event` agent kinds — agent observability must expose DECISIONS (routing · reflection · stall · compose · budget), not just I/O |
| Sehoon Kim, Suhong Moon, Ryan Tabrizi, et al., *An LLM Compiler for Parallel Function Calling*, 2023 — [arXiv:2312.04511](https://arxiv.org/abs/2312.04511) | `src/lib.rs::run_batch` (ADR-094) — one turn's batched calls are independent by construction and resolve CONCURRENTLY (order-preserving `buffered`); Nika keeps the dependency planning at the workflow-DAG layer instead of a model-declared intra-turn graph |
| Binfeng Xu, Zhiyuan Peng, Bowen Lei, Subhabrata Mukherjee, Yuchen Liu, Dongkuan Xu, *ReWOO: Decoupling Reasoning from Observations for Efficient Augmented Language Models*, 2023 — [arXiv:2305.18323](https://arxiv.org/abs/2305.18323) | background — the case against interleaved halts where no dependency exists (ADR-094); full plan-ahead rejected at v0.1 (would change the author-observable ReAct contract) |
| Andy Zhou, Kai Yan, Michal Shlapentokh-Rothman, Haohan Wang, Yu-Xiong Wang, *Language Agent Tree Search Unifies Reasoning Acting and Planning in Language Models*, 2023 — [arXiv:2310.04406](https://arxiv.org/abs/2310.04406) | background — tree-search agents evaluated and deliberately NOT adopted for v0.1 (search multiplies provider spend against Nika's budget-first posture; revisit post-v0.81 behind the same observer seam) |
| Tianneng Shi, Jingxuan He, Zhun Wang, et al., *Progent: Securing AI Agents with Privilege Control*, 2025 — [arXiv:2504.11703](https://arxiv.org/abs/2504.11703) | background — programmable privilege control lineage; Nika's standing answer is the default-deny `tools:` whitelist + `permits:` static analysis (ADR-092) |

## How citations enter this repo

1. Primary source verified first (arXiv API / publisher page — never
   from model memory).
2. Cited at the implementation site (module doc) with author·year·title·
   arXiv id, AND here with the module mapping.
3. A paper that merely *informed a survey* (read, not implemented) is
   marked as background.

Corrections welcome — if your work is used here and miscredited or
uncredited, open an issue.
