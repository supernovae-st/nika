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
| Yuanhua Lv, ChengXiang Zhai, *Lower-Bounding Term Frequency Normalization*, CIKM 2011 | `nika-bm25` — the reserved `BmParams::delta` (BM25+) lower-bound smoothing, consumer-signal gated |
| Gordon V. Cormack, Charles L. A. Clarke, Stefan Büttcher, *Reciprocal Rank Fusion outperforms Condorcet and individual rank learning methods*, SIGIR 2009 | the hybrid-retrieval fusion contract (`nika-rrf` · fuse ranks, never scores — the score-normalization note in `nika-bm25/src/lib.rs`) |
| Howard Turtle, James Flood, *Query evaluation: strategies and optimizations*, IP&M 1995 · arXiv anchor: Yifan Qiao, Yingrui Yang, Haixin Lin, Tao Yang, *Optimizing Guided Traversal for Fast Learned Sparse Retrieval*, 2023 — [arXiv:2305.01203](https://arxiv.org/abs/2305.01203) | `nika-bm25/src/eager.rs` — `EagerIndex::top_k_pruned`: MaxScore dynamic pruning (essential/non-essential term split · rank-exact · measured fewer postings visited) |

## Agent loop (`nika-verb-agent`)

| Work | Where it lands |
|---|---|
| Shunyu Yao, Jeffrey Zhao, Dian Yu, Nan Du, Izhak Shafran, Karthik Narasimhan, Yuan Cao, *ReAct: Synergizing Reasoning and Acting in Language Models*, 2022 — [arXiv:2210.03629](https://arxiv.org/abs/2210.03629) | the `agent` verb's multi-turn think→act→observe loop shape (turn-capped per the spec field table) |

## How citations enter this repo

1. Primary source verified first (arXiv API / publisher page — never
   from model memory).
2. Cited at the implementation site (module doc) with author·year·title·
   arXiv id, AND here with the module mapping.
3. A paper that merely *informed a survey* (read, not implemented) is
   marked as background.

Corrections welcome — if your work is used here and miscredited or
uncredited, open an issue.
