# Nika Cortex — Psychology & Behavioral Science Integration

> Date: 2026-03-31
> Research: 10 agents on cognitive psychology, behavioral science, consumer patterns
> This extends the FINAL design doc with psychological foundations

## The Insight

Thibaut has 10-15 years of consumer app + behavioral psychology experience. Every mechanism in Cortex maps to a cognitive psychology principle. This is NOT coincidence — AI memory IS cognitive psychology implemented in code.

## Mapping: 19 Cortex Mechanisms ↔ Psychology Principles

### Original 12 mechanisms + their psychological foundation

| # | Mechanism | Psychology Principle | Reference |
|---|-----------|---------------------|-----------|
| ① | Hebbian +2.5%/-10% | **Loss aversion** (Kahneman) — losses 2x more powerful | Tversky & Kahneman 1979 |
| ② | FSRS-6 + ACT-R decay | **Spacing effect** (Ebbinghaus 1885) | Ebbinghaus 1885, Bjork 1992 |
| ③ | Dopamine gate | **Emotional enhancement** + Negativity bias | Cahill & McGaugh 1995 |
| ④ | Prospective indexing | **Elaborative interrogation** (depth of processing) | Craik & Lockhart 1972 |
| ⑤ | Narrative consolidation | **Chunking** (Miller 1956) + **Narrative bias** | Miller 1956, Kahneman 2011 |
| ⑥ | Contradiction detection | **Cognitive dissonance** (Festinger 1957) | Festinger 1957, AGM 1985 |
| ⑦ | Salience encoding | **Von Restorff isolation effect** | Von Restorff 1933 |
| ⑧ | Feedback correction | **Testing effect** (retrieval practice) | Roediger & Karpicke 2006 |
| ⑨ | Synaptic tagging | **State-dependent memory** | Godden & Baddeley 1975 |
| ⑩ | Interference detection | **Interference theory** (proactive/retroactive) | McGeoch 1932, Underwood 1957 |
| ⑪ | Auto-linking | **Associative learning** (Pavlov → Hebb) | Hebb 1949 |
| ⑫ | Conditional triggers | **Prospective memory** | Einstein & McDaniel 1990 |

### NEW 7 mechanisms from psychology research

| # | Mechanism | Psychology Principle | What it does | Implementation |
|---|-----------|---------------------|-------------|----------------|
| ⑬ | **Peak-End Compression** | Peak-end rule (Kahneman 1993) | Store peak moment + final result, compress middle | -60-80% episodic volume, 90%+ info preserved |
| ⑭ | **Dunning-Kruger Correction** | Dunning-Kruger effect (1999) | Sparse topic = LOW effective confidence | `effective = per_fact_conf × coverage_factor` |
| ⑮ | **Deframing** | Framing effect (Tversky & Kahneman 1981) | Strip valence at write, store neutral canonical form | Metadata: `source_sentiment` field |
| ⑯ | **Anti-Echo-Chamber** | Mere exposure effect (Zajonc 1968) | Log diminishing returns on exposure count | `boost = log(1 + count) × base_weight` |
| ⑰ | **Dual-Process Retrieval** | System 1/System 2 (Kahneman 2011) | Fast path (satisfice) vs slow path (maximize) | Route by query complexity |
| ⑱ | **Zeigarnik Priority** | Zeigarnik effect (1927) | Incomplete/failed = higher recall priority | Failed workflows get activation boost |
| ⑲ | **Challenger Mechanism** | Anti sunk-cost (Arkes & Blumer 1985) | Periodic re-evaluation of procedures | Bayesian reliability ONLY, ignore past investment |
| ⑳ | **Adversarial Retrieval** | Anti confirmation bias | 15% token budget for "devil's advocate" | Search contradicting evidence actively |
| ㉑ | **Endowment Correction** | Endowment effect (Thaler 1980) | Fresh data gets 1.3x boost, 30% floor | Prevent overvaluing stored memories |
| ㉒ | **Goal Gradient Recall** | Goal gradient effect | Narrow search as workflow nears goal | Broad early → focused late, reset on failure |

### Additional sub-mechanisms discovered

| Sub-mechanism | Psychology | Where it applies |
|---------------|-----------|-----------------|
| **Valence dimension on surprise** | Negativity bias | Gate ③: failures encoded STRONGER than successes |
| **Curiosity score** | Intrinsic motivation (Schmidhuber 2010) | Audit tool: `curiosity = learnability × relevance` for gap-filling |
| **Flow-based task assignment** | Flow state (Csikszentmihalyi 1990) | Orchestration: `flow_score = 1 - |challenge - skill|` |
| **Two-tier mandatory/voluntary** | Psychological reactance (Brehm 1966) | Audit log (forced) vs agent memory (agent chooses) |
| **Coverage-weighted confidence** | Epistemic vs aleatoric uncertainty | Read filter: sparse topics penalized |
| **Importance^α × Urgency^β** | Mere urgency effect (Zhu 2018) | Retrieval: α=1.5, β=0.8 (importance > urgency) |
| **Narrative coherence check** | Confabulation prevention | Consolidation ⑤: require causal evidence, not just co-occurrence |
| **Alternative narrative generation** | Confirmation bias counter | Consolidation ⑤: generate ≥1 alternative explanation |
| **Competence trajectory** | Self-determination theory (Ryan & Deci 2000) | Procedural: track not just skill level but trend |
| **Memory visibility model** | Relatedness (SDT) | Three-tier: private / shared / global |
| **Exploration bonus** | Exploration-exploitation (ε-greedy, UCB) | Anti-echo-chamber: small incentive to try alternatives |
| **Contradiction premium** | Falsificationism (Popper) | Contradictions get EXTRA weight, not less |
| **3:1 revision threshold** | Calibrated stubbornness | AGM: need 3x evidence weight to revise well-established belief |
| **Path diversity in graph** | Occam's razor + exceptions | Show shortest path AND one longer alternative |
| **Fragmentation tolerance** | Anti-narrative bias | Allow storing fragments that don't form a story yet |

## 5 Design Principles (from cross-bias synthesis)

### 1. Cognitive Load Management
- Decision fatigue + paradox of choice
- Fixed evidence slots: 5-7 per decision (Miller's 7±2)
- Progressive context compression as workflow advances
- Token budget = cognitive load limiter

### 2. Calibrated Stubbornness
- Status quo bias + Occam's razor
- Easy to update low-confidence beliefs
- Proportionally harder to update high-confidence ones
- 3:1 evidence ratio for revision of established beliefs
- AGM success postulate: NEVER impossible to update

### 3. Adaptive Retrieval Strategy
- Satisficing vs maximizing (Simon 1956)
- Simple queries → System 1 (fast, top-3, satisfice if confidence > 0.85)
- Complex queries → System 2 (thorough, top-10, recursive, re-rank)
- Time-pressured → satisfice regardless

### 4. Structured Diversity
- Default effect (Johnson & Goldstein 2003)
- Penalize overuse of generic node types
- Suggest specific alternatives during `nika:remember`
- Periodic re-classification audit in `nika:consolidate`

### 5. Cautious Narrative Construction
- Narrative bias (Kahneman 2011) + confabulation risk
- Require causal EVIDENCE, not just co-occurrence
- Generate alternative narratives during consolidation
- Tolerate fragmentation (some episodes are genuinely disconnected)
- Tag inferred vs observed relationships

## Write Pipeline — Psychology-Enhanced (11 steps, was 8)

```
Input
  │
  ├─1. DEDUP (blake3 exact + Grafeo cosine > 0.85)
  │
  ├─2. DOPAMINE GATE ③ + VALENCE
  │     surprise = novelty (cosine distance to nearest existing)
  │     utility = source.confidence × workflow.importance
  │     valence = +1 (success), -1 (failure), 0 (neutral)
  │     encoding_strength = surprise × (1 + |valence| × 0.5)
  │     Failures encoded 50% stronger than successes (negativity bias)
  │
  ├─3. DEFRAMING ⑮
  │     Strip valence from content → neutral canonical form
  │     Store original framing as metadata: source_sentiment
  │
  ├─4. SALIENCE ENCODING ⑦
  │     0.4×novelty + 0.3×importance + 0.1×extremity + 0.2×specificity
  │
  ├─5. PEAK-END CHECK ⑬
  │     If this is an episodic event from a workflow:
  │       Is this the PEAK (highest surprise in workflow)? → full detail
  │       Is this the END (final result)? → full detail
  │       Otherwise → compressed DAG-only (structure, no content)
  │
  ├─6. CONTRADICTION CHECK ⑥
  │     Find potentially conflicting facts in Grafeo
  │     3:1 evidence ratio for revision of established beliefs
  │     AGM contraction before expansion
  │
  ├─7. AUTO-LINKING ⑪ + DIVERSITY CHECK
  │     Find related (cosine > 0.6) → create edges
  │     Check: are we overusing generic edge types?
  │     Suggest specific alternatives if available
  │
  ├─8. PROSPECTIVE INDEXING ④ (only if FULL PROCESSING)
  │     LLM: "Why would this be useful in the future?"
  │     = elaborative interrogation (deeper encoding)
  │
  ├─9. SYNAPTIC TAGGING ⑨
  │     If important: boost related recent (< 6h) memories
  │
  ├─10. ZEIGARNIK CHECK ⑱
  │      If this fact relates to an unresolved issue:
  │        Boost activation of the unresolved issue
  │        "Open loop" = priority signal
  │
  └─11. PERSIST
        Grafeo: CREATE node + edges + vector embedding
        SQLite: INSERT cognitive state + changelog entry
        Two-tier: audit log (mandatory) + agent memory (voluntary)
```

## Read Pipeline — Psychology-Enhanced

```
Query
  │
  ├─ COMPLEXITY CLASSIFICATION (System 1 or 2?)
  │   Simple/factual → System 1 (fast, satisfice)
  │   Complex/analytical → System 2 (deep, maximize)
  │
  ├─ System 1 PATH (fast):
  │   Grafeo hybrid (BM25 + HNSW + graph) → top-3
  │   If max_confidence > 0.85 → return immediately
  │   Else → escalate to System 2
  │
  ├─ System 2 PATH (deep):
  │   ⓪ Trigger check
  │   ① Grafeo hybrid (BM25 + HNSW + PageRank)
  │   ④ ACT-R spreading activation
  │   ⑤ Intent classification
  │   ⑥ Confidence × FSRS retrievability
  │   ⑦ Interference penalty
  │   ⑧ Salience boost
  │   ⑱ Zeigarnik boost (unresolved = priority)
  │   → RRF merge → token budget → evidence packets
  │
  ├─ POST-PROCESSING:
  │   Dunning-Kruger check ⑭: sparse topic? → penalty
  │   Coverage factor: < 5 facts on topic → halve confidence
  │   Importance^1.5 × Urgency^0.8 weighting
  │   Path diversity: include shortest + 1 alternative path
  │
  └─ RecallResult { packets, coverage_score, truncated }
```

## Concrete Algorithm Implementations (Rust)

| Algorithm | LOC | Source | License | Notes |
|-----------|-----|--------|---------|-------|
| Spreading activation | ~100 | Rewrite from lucid-core pattern | Own (GPL ref only) | BFS + fan-out norm, sub-ms @ 10K nodes |
| Personalized PageRank | ~80 | graphops v0.1.3 | MIT ✅ | Power iteration, damping=0.85 |
| FSRS-6 (simplified) | ~53 | pensyve-core decay.rs | Apache-2.0 ✅ | R(t,S)=(1+t/9S)^(-1), 80% of full benefit |
| RRF merge | ~40 | pensyve-core | Apache-2.0 ✅ | Adaptive k = max(1, count/10) |
| AGM belief revision | ~60 | pensyve-core MemoryGraph | Apache-2.0 ✅ | invalidate_edge + supersession |
| Louvain clustering | ~60 | graphops label_propagation | MIT ✅ | Fast approx, or graphrs 580 LOC full |
| Bayesian reliability | ~15 | Own impl | — | Beta(α+s, β+f), Wilson score |
| blake3 dedup | ~20 | blake3 1.8.4 | MIT+Apache ✅ | 5 GB/s, 3-phase dedup |
| Cosine similarity | ~10 | Own impl | — | Threshold: 0.92 near-match |

**Total estimated core algorithms: ~440 LOC of pure math/logic.**

### Crates to reference (learn from, don't depend on)

| Crate | What to learn | License | Why not depend |
|-------|--------------|---------|----------------|
| `pensyve-core` | RRF, FSRS, AGM, salience | Apache-2.0 | 1 star, not published |
| `lucid-core` | ACT-R spreading activation | GPL-3.0 | License incompatible |
| `graphops` | PPR, label propagation | MIT | Lightweight reference |
| `graphrs` | Louvain full + Leiden | MIT | Alternative if needed |
| `fsrs` v5.2.0 | Full 21-param FSRS | BSD-3 | Heavy dep (burn tensor) |

### growth.design: 106 cognitive patterns catalog

Extracted from growth.design/psychology (Wayback Machine snapshot). 4 categories:

- **How We Perceive** — 28 patterns (anchoring, framing, halo, priming...)
- **How We Decide** — 28 patterns (loss aversion, sunk cost, defaults, FOMO...)
- **How We Engage** — 33 patterns (flow, variable reward, endowment, commitment...)
- **What We Remember** — 17 patterns (peak-end, Zeigarnik, chunking, spacing, negativity bias...)

Full catalog saved in research files. Each pattern mapped to Cortex mechanisms.

## Consolidated Reference Library

### Foundational (pre-2000)
- Ebbinghaus (1885) — Forgetting curve / spacing effect
- Miller (1956) — The magical number 7±2
- Festinger (1957) — Cognitive dissonance
- Zajonc (1968) — Mere exposure effect
- Tversky & Kahneman (1974) — Anchoring and heuristics
- Hebb (1949) — Hebbian learning
- Kahneman & Tversky (1979) — Prospect theory / loss aversion
- Tversky & Kahneman (1981) — Framing effect
- AGM (1985) — Belief revision
- Arkes & Blumer (1985) — Sunk cost fallacy
- Einstein & McDaniel (1990) — Prospective memory
- Csikszentmihalyi (1990) — Flow state
- Bjork & Bjork (1992) — Dual-strength memory model
- Kahneman (1993) — Peak-end rule
- Kruger & Dunning (1999) — Dunning-Kruger effect

### Modern (2000-2020)
- Ryan & Deci (2000) — Self-determination theory
- Roediger & Karpicke (2006) — Testing effect
- Schmidhuber (2010) — Formal theory of curiosity
- Kahneman (2011) — System 1 / System 2
- Pathak et al. (2017) — Intrinsic Curiosity Module
- Zhu et al. (2018) — Mere urgency effect
- Burda et al. (2019) — Random Network Distillation

### AI-Specific (2022-2026)
- Kadavath et al. (2022) — LLM confidence calibration
- Park et al. (2023) — Generative Agents shared memory (score = α×recency + β×importance + γ×relevance)
- CoALA (2309.02427) — Canonical cognitive architecture (working + episodic/semantic/procedural LTM)
- Liu et al. (2023) — Lost in the middle
- A-Mem (2502.12110) — Zettelkasten agentic memory, 85-93% token reduction, 396 citations
- Mem0 (2504.19413) — Production memory layer, graph variant outperforms, 275 citations
- D-MEM (2603.18631) — **Validates our System 1/System 2 dual-process** (96.7% accuracy at fraction of cost)
- Kumiho (2603.17244) — Prospective indexing, AGM, 93.3% LoCoMo
- GAAMA (2603.27910) — 4-node hierarchy + PageRank
- TraceMem (2602.09712) — Narrative consolidation

### Novel Opportunities (nobody has done this yet)
- **BJ Fogg B=MAP for AI memory**: Motivation(importance) × Ability(capacity+schema fit) × Prompt(triggers)
  No existing paper applies this to AI agents — greenfield territory for Nika
- **Ebbinghaus forgetting curve in AI**: R(t) = e^(-t/S) — no agent implements this formally
- **No cognitive/behavioral Rust crates exist** — entire domain is greenfield in Rust
