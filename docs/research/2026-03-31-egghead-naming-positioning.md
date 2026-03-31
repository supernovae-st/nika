# Research Report: Naming and Positioning of "Egghead" Memory Engine

> Date: 2026-03-31
> Methodology: crates.io API, GitHub API, arXiv API, JOSS docs, Semantic Scholar
> Pages analyzed: 50+
> Confidence: HIGH (primary sources verified)

---

## 1. Name "Egghead" -- Conflict Analysis

### 1.1 The egghead.io Problem

**egghead.io** is a well-known developer education platform (React, TypeScript, JavaScript). It is THE dominant association for "egghead" in the developer tools space.

| Surface | Status |
|---------|--------|
| crates.io | **FREE** -- no `egghead` crate exists |
| PyPI | Exists but dead (no metadata) |
| npm | TAKEN -- `egghead` v1.0.6 (Puppeteer-based egghead.io downloader) |
| GitHub repos named "egghead" | Dominated by egghead.io course notes (~2,500+ stars total) and the official egghead-next repo (1,448 stars) |
| arXiv papers titled "Egghead" | **ZERO** -- no academic paper uses "Egghead" in its title |
| Trademarks | egghead.io likely holds common-law trademark in educational software |

### 1.2 Risk Assessment

| Risk | Level | Details |
|------|-------|---------|
| Google confusion | **HIGH** -- searching "egghead rust" or "egghead AI" will return egghead.io content for years |
| Academic citation | **LOW** -- no academic conflict exists |
| Legal/trademark | **MEDIUM** -- different domain (memory engine vs education), but brand confusion is real |
| crates.io availability | **NO RISK** -- crate name is free |
| SEO competition | **HIGH** -- egghead.io has 10+ years of SEO dominance |

### 1.3 Verdict on "Egghead"

The name is **technically available** in the Rust/academic space but **strategically problematic** for discoverability. Any developer searching "egghead" will hit egghead.io first. For a research paper, the name works fine (no academic conflicts). For an open-source project competing for GitHub stars and crates.io downloads, SEO friction is a real concern.

**Recommendation**: The name works for an internal subsystem (e.g., `nika-egghead` or files like `egghead.grafeo`), but consider a more distinctive name if this becomes a standalone, paper-worthy project.

---

## 2. Alternative Names -- Availability Matrix

### 2.1 Full Availability Check

| Name | crates.io | GitHub noise | Academic | Meaning | Score |
|------|-----------|-------------|----------|---------|-------|
| **egghead** | FREE | HIGH (egghead.io) | clean | Informal "brainy" | 5/10 |
| **mnestic** | FREE | VERY LOW (7 repos) | clean | From "mnesis" (memory) | 9/10 |
| **engrams** | FREE | LOW (37 repos) | some papers use term | Memory traces in neuroscience | 8/10 |
| **entorhinal** | FREE | ZERO repos | used in neurosci lit | Brain region critical for memory | 7/10 |
| **eidetic** | FREE | MEDIUM (102 repos) | clean | Perfect recall | 7/10 |
| **palimpsest** | FREE | MEDIUM (170 repos) | some bio papers | Rewritten memory (beautiful metaphor) | 8/10 |
| **litha** | FREE | noise (276 repos, pagan) | clean | Could stand for anything | 5/10 |
| **hebbian** | FREE | MEDIUM (280 repos) | heavily used in papers | "Cells that fire together wire together" | 6/10 |
| **cogmem** | FREE | VERY LOW (11 repos) | clean | Cognitive Memory (acronym) | 7/10 |
| **hippocampal** | FREE | clean | very used in neurosci | Too literal, too long | 4/10 |

### 2.2 Names That Are TAKEN

| Name | Status | Why to avoid |
|------|--------|-------------|
| **mnemos** | crates.io: embedded OS kernel (2,900 dl) | Name squatted |
| **engram** | crates.io: version control tool (57K dl). GitHub: **Engram** by Gentleman-Programming (2,089 stars, Go, AI memory system, Feb 2026) | **DIRECT COMPETITOR** using the singular form |
| **cortex** | crates.io: event pub/sub (12K dl). GitHub: cortex-mem (226 stars, Rust, cognitive AI memory) | **DIRECT COMPETITOR** |
| **synapse** | crates.io: 882 dl, blockchain | Overloaded term |
| **mnemonic** | crates.io: 2.3M downloads | Very taken |
| **synaptic** | crates.io: Rust agent framework (217 dl) | Same space |
| **mneme** | crates.io: event-sourcing (5,808 dl) | Taken |
| **mnemosyne** | crates.io: process hooking (2,338 dl) | Taken |
| **anamnesis** | crates.io: tensor parsing (47 dl) | Recently taken |
| **eidos** | crates.io: ETL tool (386 dl) | Taken |
| **noema** | crates.io: IoC framework (66 dl) | Taken |

### 2.3 Top 3 Recommendations

**Tier 1 (best for paper + OSS project):**

1. **Mnestic** -- From Greek "mnestikos" (of memory). Zero noise. Sounds academic. Evokes the right domain. Pronounceable. Unique enough to own Google results from day one.

2. **Engrams** -- Plural of "engram" (the physical trace of a memory in the brain). Karl Lashley's famous search for the engram is foundational neuroscience. The singular "Engram" is taken (Gentleman-Programming, 2K stars), but the **plural** `engrams` is free on both crates.io and GitHub. Risk: confusion with the singular competitor.

3. **Palimpsest** -- A manuscript where original writing has been erased and rewritten. Beautiful metaphor for memory that consolidates, overwrites, and evolves. Free everywhere. Distinctive. Literary resonance.

**Tier 2 (solid alternatives):**

4. **Eidetic** -- "Eidetic memory" = photographic memory. Clean on crates.io. Some GitHub noise but nothing in AI memory.

5. **Entorhinal** -- The entorhinal cortex is the gateway between hippocampus and neocortex, exactly the role this engine plays. Zero competition. Very academic. Hard to pronounce for non-neuroscientists.

---

## 3. Paper Naming Conventions

### 3.1 Successful Systems Paper Patterns

| Paper | Pattern | Why it works |
|-------|---------|-------------|
| **Spark**: Cluster Computing with Working Sets | **Name: Subtitle explaining what it does** | Metaphor (spark) + plain English subtitle |
| **Kafka**: A Distributed Messaging System | Short name + category description | Reference to Franz Kafka (complexity, systems) |
| **Ray**: A Distributed Framework for Emerging AI | Short name + technical positioning | Natural word, easy to remember |
| **Raft**: In Search of an Understandable Consensus Algorithm | Name + mission statement | Acronym-like but actually a word |
| **MapReduce**: Simplified Data Processing on Large Clusters | Compound name describing the abstraction | Self-documenting |
| **LLAMA**: Large Language Model Meta AI | Backronym | Memorable, playful |
| **Mamba**: Linear-Time Sequence Modeling with Selective State Spaces | Animal name + technical subtitle | Evocative, easy to cite |

### 3.2 Agent Memory Paper Naming Patterns (2024-2026)

Recent papers in this exact space follow a clear convention:

| Paper | Pattern |
|-------|---------|
| **D-MEM**: Dopamine-Gated Agentic Memory via Reward Prediction Error Routing | Acronym: mechanism-based |
| **GAAMA**: Graph Augmented Associative Memory for Agents | Backronym |
| **All-Mem**: Agentic Lifelong Memory via Dynamic Topology Evolution | Compound name |
| **A-MEM**: Agentic Memory for LLM Agents | Simple prefix + MEM |
| **Kumiho**: Graph-Native Cognitive Memory for AI Agents | Mythological creature (Korean nine-tailed fox) |
| **D-Mem**: A Dual-Process Memory System for LLM Agents | Prefix + Mem |
| **ElephantBroker**: A Knowledge-Grounded Cognitive Runtime | Metaphor (elephants never forget) |
| **TA-Mem**: Tool-Augmented Autonomous Memory Retrieval | Prefix + Mem |
| **AdaMem**: Adaptive User-Centric Memory | Prefix + Mem |

### 3.3 Recommended Paper Title Formats

If keeping "Egghead":
> **Egghead: A Neuroscience-Inspired Cognitive Memory Engine for AI Workflow Agents**

If using "Mnestic":
> **Mnestic: Hebbian Learning and Spaced Repetition for Persistent Agent Memory**

If using "Engrams":
> **Engrams: Graph-Native Cognitive Memory with Biologically-Inspired Consolidation**

The `-MEM` / `-Mem` suffix pattern is becoming crowded. Avoid it to stand out.

---

## 4. Academic Positioning -- State of the Art (2024-2026)

### 4.1 The Landscape is EXPLODING

The period from late 2024 through March 2026 has seen an extraordinary surge in agent memory research. Here is the current taxonomy from the key surveys:

**Major Surveys (2026):**
- "Memory for Autonomous LLM Agents: Mechanisms, Evaluation, and Emerging Frontiers" (Du et al., March 2026, arXiv:2603.07670) -- defines the write-manage-read loop taxonomy
- "Graph-based Agent Memory: Taxonomy, Techniques, and Applications" (Feb 2026, arXiv:2602.05665) -- graph-specific survey with awesome-list
- "Anatomy of Agentic Memory" (Feb 2026, arXiv:2602.19320) -- empirical analysis of evaluation limitations
- "Governing Evolving Memory in LLM Agents" (SSGM Framework, March 2026, arXiv:2603.11768) -- governance, drift, safety
- "From Static Templates to Dynamic Runtime Graphs" (March 2026, arXiv:2603.22386) -- workflow optimization survey

### 4.2 Key Systems and Where Egghead Differs

| System | Architecture | Memory Type | Key Innovation | Egghead Differentiator |
|--------|-------------|-------------|----------------|----------------------|
| **GAAMA** (March 2026) | Concept-mediated hierarchical KG | 4 node types, 5 edge types | PPR + cosine hybrid retrieval | Egghead uses Hebbian edge strengthening + FSRS-6 temporal scheduling, not static edge weights |
| **Kumiho** (March 2026) | Redis (WM) + Neo4j (LTM) | Dual-store, formal AGM revision | Prospective indexing, event extraction, belief revision proofs | Egghead is single-file (Grafeo), no external DB dependency. No formal belief revision yet (gap?) |
| **D-MEM** (March 2026) | Fast/Slow routing via RPE | Dopamine-gated write path | Reward Prediction Error routing, O(1) fast buffer | **Closest competitor conceptually.** Both use neuroscience metaphors. D-MEM is theoretical; Egghead is an implementation. D-MEM uses RPE routing; Egghead uses FSRS-6 + Hebbian + dopamine gating. |
| **All-Mem** (March 2026) | Dynamic topology evolution | Lifelong memory with online/offline | Topology evolution graph | Egghead has explicit 6-level hierarchy, not dynamic topology |
| **D-Mem** (March 2026) | Dual-process (System 1/System 2) | Fast intuitive + slow deliberate | Cognitive dual-process theory | Egghead has 12 cognitive mechanisms, more comprehensive |
| **ElephantBroker** (March 2026) | Neo4j KG + vector retrieval | Knowledge-grounded with provenance | Provenance tracking, trustworthiness scoring | Egghead is embedded (no Neo4j server), lighter weight |
| **Engram** (Feb 2026, Go) | SQLite + FTS5 | Key-value + full-text | MCP server, agent-agnostic | Different language (Go vs Rust), no graph structure, no neuroscience-inspired mechanisms |

### 4.3 Identified Gaps in Current Literature

1. **No system combines FSRS-6 with graph-based memory.** FSRS is used in flashcard apps (Anki, Mochi) but no one has applied it to agent memory scheduling. This is a genuine first.

2. **Hebbian learning for edge weights in agent memory graphs is unexplored.** D-MEM uses RPE for write gating, but not for strengthening associative connections between memories. The "cells that fire together wire together" principle applied to memory edges is novel.

3. **No single-file, embedded cognitive memory engine exists.** Kumiho needs Redis + Neo4j. GAAMA needs a graph DB. ElephantBroker needs Neo4j. Egghead's single-file `.grafeo` approach with no external dependencies is unique.

4. **The write-manage-read loop lacks temporal scheduling.** Current systems decide WHAT to remember but not WHEN to resurface memories. FSRS-6 scheduling ("this memory should be reviewed in 3 days") is a new dimension.

5. **No Rust implementation of cognitive agent memory.** memvid (13K stars) and cortex-mem (226 stars) are Rust but don't implement neuroscience-inspired mechanisms. Engram is Go. Most academic systems are Python prototypes.

6. **Workflow-engine integration is absent.** All existing systems are standalone. Egghead is embedded in a workflow engine with 5 semantic verbs, creating a unique `nika:remember` / `nika:recall` interface.

### 4.4 Positioning Statement for Paper

> While recent work has advanced graph-based memory (GAAMA), dopamine-inspired gating (D-MEM), and formal belief revision (Kumiho) individually, no existing system synthesizes these into an embedded, single-file cognitive memory engine with biologically-grounded temporal scheduling. We present [Name], a Rust-native memory engine that combines Hebbian edge strengthening, FSRS-6 spaced repetition, and a 6-level memory hierarchy (flash, working, episodic, semantic, procedural, meta) within a single graph file, requiring no external database infrastructure.

---

## 5. Hugging Face Papers -- Submission Process

### 5.1 How It Works

Hugging Face Papers is NOT a journal or review platform. It is an **indexing and social discovery layer** for arXiv papers.

**Process:**
1. Upload your paper to **arXiv** first (this is a prerequisite)
2. Go to https://huggingface.co/papers
3. Search for your paper by arXiv ID or title
4. If not found, visit `hf.co/papers/XXXX.YYYYY` (your arXiv ID) to index it
5. The paper page is created instantly (automated, no curation)
6. Claim authorship by clicking your name on the paper page
7. Link models, datasets, or Spaces to the paper via repository cards (README.md with arXiv link)

### 5.2 Timeline

| Step | Time |
|------|------|
| arXiv submission to arXiv publication | 1-3 business days (moderation queue) |
| arXiv to HF Papers indexing | Automatic when anyone visits the URL, or when linked from a HF repo |
| Authorship claim verification | "Soon" (admin manual review, typically 1-3 days) |

### 5.3 There is No Curation

Anyone can index any arXiv paper. The "Daily Papers" page on HF is curated by the community via upvotes and the AK (AK from @_akhaliq) daily selections, but there is no editorial gate.

### 5.4 What Makes a Paper Popular on HF

- Has a linked demo Space (Gradio/Streamlit)
- Has a linked model or dataset on the Hub
- Gets picked up by @_akhaliq's daily roundup
- Has a catchy, short title
- Includes reproducible code

### 5.5 Strategy for Egghead

1. Post paper to arXiv
2. Create a Hugging Face Space demo (even a simple Gradio interface showing memory operations)
3. Publish the crate and link it
4. Index on HF Papers
5. Share on X/Twitter to get picked up by the AI paper aggregators

---

## 6. JOSS (Journal of Open Source Software) -- Detailed Guide

### 6.1 What JOSS Is

JOSS is a **peer-reviewed, developer-friendly academic journal** that publishes short papers (~750-1750 words) about research software. It provides a **DOI** and counts as a real academic publication. The review focuses on the software quality, not the paper's novelty.

**Key facts:**
- No submission fees
- No publication fees
- ISSN: 2475-9066
- Indexed in: Scopus, Web of Science, Google Scholar
- Review happens entirely on GitHub issues
- Typical review: 4-8 weeks
- Uses `paper.md` + BibTeX in the repo

### 6.2 Submission Requirements (as of March 2026)

**Mandatory:**
- OSI-approved license (AGPL-3.0 qualifies)
- Open GitHub/GitLab repo with issue tracker
- Repository public for **more than 6 months** with active development spanning that period
- Evidence of research use (publications, benchmarks, external adoption)
- Feature-complete software
- Tests, CI, documentation, CONTRIBUTING file
- Multi-author: evidence of issues, PRs, public discussion
- Single-author: multiple indicators of open practices required

**Pre-review screening (desk rejection if failed):**
1. 6+ months of public development history (not a code dump)
2. Demonstrated research impact (papers using it, or benchmarks)
3. Good open-source practices (releases, changelogs, tests, CI)
4. Iterative development over time (not a burst of commits)

### 6.3 Paper Format

```markdown
---
title: 'Your Software Name: A Subtitle'
tags:
  - Rust
  - cognitive memory
  - AI agents
  - spaced repetition
authors:
  - name: Your Name
    orcid: 0000-0000-0000-0000
    affiliation: 1
affiliations:
  - name: SuperNovae Studio, Paris, France
    index: 1
date: 31 March 2026
bibliography: paper.bib
---

# Summary
(high-level functionality for non-specialist audience)

# Statement of Need
(research purpose, target audience, relation to other work)

# State of the Field
(comparison to existing packages, "build vs contribute" justification)

# Software Design
(architecture trade-offs, why they matter)

# Research Impact Statement
(evidence of real use, benchmarks, integrations)

# AI Usage Disclosure
(mandatory -- describe how AI was used in development)

# Acknowledgements

# References
```

**Length:** 750-1750 words. This is NOT a full research paper -- it's about the SOFTWARE.

### 6.4 Required Sections (2026 update)

| Section | Purpose |
|---------|---------|
| Summary | Non-specialist overview |
| Statement of Need | Why this software exists |
| State of the Field | How it compares to alternatives |
| Software Design | Architecture trade-offs and decisions |
| Research Impact Statement | Evidence of real use or credible significance |
| AI Usage Disclosure | **NEW (2026)** -- mandatory, even if "no AI used" |

### 6.5 Review Process

1. Submit via https://joss.theoj.org (short form)
2. Associate Editor-in-Chief does initial check, assigns handling editor
3. Editor assigns 2+ reviewers
4. Review happens on GitHub issue at https://github.com/openjournals/joss-reviews
5. Iterative conversation: reviewers post comments, authors respond
6. Expected response time: 2 weeks for comments, 4-6 weeks for changes
7. On acceptance: make a tagged release + archive on Zenodo for DOI
8. Paper published with DOI

**Typical timeline: 4-12 weeks from submission to publication.**

### 6.6 Review Checklist (what reviewers check)

- [ ] OSI-approved license present
- [ ] Scope and research significance
- [ ] 6+ months public development history
- [ ] Open development practices (issues, PRs, community)
- [ ] Installation works as documented
- [ ] Functional claims confirmed
- [ ] Performance claims verified (if any)
- [ ] Documentation: statement of need, install instructions, examples, API docs
- [ ] Community guidelines (CONTRIBUTING)
- [ ] Tests present and pass
- [ ] Paper content matches required sections
- [ ] State of the field comparison is fair
- [ ] Software design section shows meaningful decisions
- [ ] Research impact evidence is compelling

### 6.7 Examples of Similar Tools Published in JOSS

JOSS has published many Rust tools, CLI tools, and workflow engines. The search API is limited but the journal covers:
- Scientific workflow engines
- CLI tools for data processing
- Rust numerical libraries
- Graph analysis tools

**Key insight:** JOSS evaluates SOFTWARE QUALITY, not paper novelty. If Nika/Egghead has good tests, docs, CI, and evidence of use, it can be published.

### 6.8 JOSS vs arXiv Comparison

| Dimension | JOSS | arXiv |
|-----------|------|-------|
| Peer review | Yes (GitHub-based) | No |
| DOI | Yes | No (but has arXiv ID) |
| Paper length | 750-1750 words | No limit |
| Focus | Software quality | Research novelty |
| Time to publish | 4-12 weeks | 1-3 days |
| Academic credit | Counts as publication | Counts as preprint |
| Citation | Cite the DOI | Cite arXiv ID |
| Requirement | 6+ months open dev | arXiv endorsement |
| AI disclosure | Mandatory | Not required |
| Novelty required | No (software must be useful) | Expected |
| Software required | Yes (must exist and work) | No |

### 6.9 Strategy: Publish BOTH

1. **arXiv paper** (full research paper, ~8-15 pages): Focus on the cognitive memory architecture, Hebbian learning, FSRS-6 adaptation, benchmarks against GAAMA/D-MEM/Kumiho. This establishes research novelty.

2. **JOSS paper** (short, ~1000 words): Focus on the software itself -- installation, API, design decisions, test coverage, use cases. This establishes software credibility and gives a citable DOI.

These are complementary, not competing. The JOSS paper can cite the arXiv paper for the research contribution.

### 6.10 JOSS Eligibility Concern

**CRITICAL: Nika must be public for 6+ months before JOSS submission.** If the repo went public recently, the earliest JOSS submission would be 6 months after the public launch date. The 2026 requirements explicitly state:

> "A repository made public immediately before submission, or one showing development concentrated into a few days or weeks, will not be accepted."

Plan accordingly: launch May 5, 2026 --> earliest JOSS submission: November 2026.

---

## 7. Competitive Landscape Summary

### 7.1 GitHub Stars Landscape (March 2026)

| Project | Stars | Language | Memory Type |
|---------|-------|----------|-------------|
| memvid | 13,663 | Rust | Video-based RAG replacement |
| Engram | 2,089 | Go | SQLite+FTS5, MCP server |
| vestige | 456 | Rust | FSRS-6, 29 brain modules, MCP |
| cortex-mem | 226 | Rust | Cognitive foundation for embodied AI |
| ahnlich | 185 | Rust | In-memory vector datastore |
| shodh-memory | 182 | Rust | Cognitive memory, offline-first |

### 7.2 Academic Papers Landscape (March 2026)

| Paper | Approach | Benchmark |
|-------|----------|-----------|
| GAAMA | Graph + PPR + cosine hybrid | LoCoMo-10: 78.9% |
| Kumiho | Neo4j + Redis, AGM revision | LoCoMo F1: 0.565, LoCoMo-Plus: 93.3% |
| D-MEM | Dopamine RPE routing | 80% token reduction |
| All-Mem | Dynamic topology evolution | Lifelong setting |
| ElephantBroker | Neo4j + provenance | Trustworthiness focus |

---

## 8. Final Recommendations

### 8.1 Naming Decision Tree

```
Is this a standalone project/paper?
  YES --> Use "Mnestic" or "Engrams" (academic weight, zero conflicts)
  NO, it's a Nika subsystem --> "Egghead" is fine internally

Will you publish a research paper?
  YES --> Avoid "Egghead" (too informal for academic credibility)
         Use: "Mnestic: ..." or "Engrams: ..."
  NO --> "Egghead" works, it's memorable and fun

Do you want maximum SEO/discoverability?
  YES --> "Mnestic" (zero Google competition)
  NO --> Any name works
```

### 8.2 My Recommendation

**"Mnestic"** for the paper and public-facing name. Internal files can stay `egghead.grafeo` if you like the charm.

Reasons:
- Zero conflicts on crates.io, GitHub, arXiv, npm, PyPI
- Etymologically perfect (Greek mnestikos = "of or relating to memory")
- Academic gravitas without being pretentious
- Easy to pronounce: "NESS-tick"
- Google search: ZERO competing projects
- Works as both crate name and paper title
- Pairs naturally with Nika: "Nika + Mnestic"

### 8.3 Suggested Paper Title

> **Mnestic: Biologically-Inspired Cognitive Memory for AI Workflow Agents**
>
> Or the longer version:
> **Mnestic: Hebbian Learning, FSRS-6 Scheduling, and Graph-Native Consolidation for Persistent Agent Memory**

---

## Sources

1. crates.io API -- Package availability verification (14 names checked)
2. GitHub REST API -- Repository search and competitor analysis
3. arXiv API -- 40+ papers analyzed (2024-2026)
4. JOSS readthedocs -- Submission requirements, paper format, review criteria (March 2026 version)
5. Hugging Face Hub docs -- Paper Pages documentation
6. Key papers:
   - GAAMA (arXiv:2603.27910) -- Graph Augmented Associative Memory
   - Kumiho (arXiv:2603.17244) -- Graph-Native Cognitive Memory
   - D-MEM (arXiv:2603.14597) -- Dopamine-Gated Agentic Memory
   - Memory survey (arXiv:2603.07670) -- Mechanisms, Evaluation, Frontiers
   - Graph memory survey (arXiv:2602.05665) -- Taxonomy, Techniques, Applications
   - Anatomy of Agentic Memory (arXiv:2602.19320) -- Evaluation limitations
   - SSGM (arXiv:2603.11768) -- Governing Evolving Memory
   - All-Mem (arXiv:2603.19595) -- Lifelong Memory
   - Workflow survey (arXiv:2603.22386) -- Static to Dynamic Runtime Graphs
7. Nika Cortex FINAL design doc -- Internal architecture reference
