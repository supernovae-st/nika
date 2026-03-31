# Academic Publishing & Code Archival Strategy for Nika

> Research conducted 2026-03-31. Actionable findings for establishing Nika's academic
> presence, getting DOIs, archiving code, and potentially publishing a paper.

---

## 1. Hugging Face Papers (papers.huggingface.co)

### How it works
- A community-curated daily feed of AI research papers, primarily sourced from **arXiv**.
- A curator known as **AK** (plus community contributors) selects papers for the "Daily Papers" page.
- Over **10,000 papers** featured since launch; **12,000+ email subscribers** to the daily digest.
- Papers get a dedicated page on HF with upvotes, comments, and links to related models/datasets/Spaces.

### Submission process
- **Anyone can submit** by indexing an arXiv paper onto the HF Paper Page.
- There is a browser extension that lets you check if a paper is already featured and add it.
- First-time submitters who "claim" a paper are recognized for future contributions.
- **arXiv-only**: no confirmed path for non-arXiv papers.

### Relevance for Nika
- **Prerequisite**: Nika would need an arXiv paper first.
- **Audience**: ML/AI researchers and practitioners -- exactly the people who build with LLMs.
- **Action**: If we publish on arXiv, we can submit to HF Papers the same day for amplification.
- **Upvote system**: Community upvotes drive trending visibility. The HF community is engaged.

### Verdict: HIGH VALUE, but requires arXiv paper first.

---

## 2. arXiv Categories for Nika

### Category comparison

| Category | Volume (2025) | Audience | Fit for Nika |
|----------|---------------|----------|--------------|
| **cs.AI** (Artificial Intelligence) | ~40,000/year | Broadest AI audience | Best primary category |
| **cs.SE** (Software Engineering) | ~2,000/year | Dev tools researchers | Good cross-list |
| **cs.PL** (Programming Languages) | ~500/year | Language theory academics | Marginal (YAML DSL angle) |
| **cs.LG** (Machine Learning) | Massive | ML practitioners | If paper focuses on LLM inference |

### Recommendation
- **Primary**: `cs.AI` -- highest visibility, aligns with "AI workflow engine"
- **Cross-list**: `cs.SE` -- captures the software engineering / developer tools audience
- **Optional cross-list**: `cs.PL` if the paper emphasizes the YAML DSL / "Inference as Code" paradigm

### Submission requirements for non-academics
- **Endorsement needed** for first-time authors in a category.
- Process: start submission, receive endorsement request link, contact established arXiv authors in the field.
- **Tips**: use an institutional or professional email, cite recent arXiv papers you reference, link to public code.
- Solo developers CAN publish if the work is framed as a scientific contribution.
- Submissions before 14:00 ET are announced the same day at 20:00 ET.

### Verdict: cs.AI primary + cs.SE cross-list. Endorsement is the main hurdle.

---

## 3. Open-Source Tools That Published Papers

### Hall of fame (systems papers)

| Project | Paper | Venue | Approx. Citations |
|---------|-------|-------|-------------------|
| TensorFlow | "A System for Large-Scale Machine Learning" | OSDI 2016 | ~19,000 |
| PyTorch | "PyTorch: An Imperative Style..." | NeurIPS 2019 | ~15,000+ |
| Apache Spark | "Resilient Distributed Datasets" | NSDI 2012 | ~12,000+ |
| Ray | "A Distributed Framework for Emerging AI" | OSDI 2018 | ~2,000+ |
| vLLM | "Efficient Memory Management for LLM Serving" | SOSP 2023 | ~2,000+ |

### AI orchestration tools with papers

| Tool | Paper | Where |
|------|-------|-------|
| **DSPy** | Khattab et al., 2023 | arXiv (Stanford) |
| **AutoGen** | Wu et al., 2024 | arXiv (Microsoft) |
| **TapeAgents** | arXiv:2412.08445, 2024 | arXiv |
| **Pocketflow** | arXiv:2504.03771, 2025 | arXiv |
| **ALARA** | arXiv:2603.20380, 2026 | arXiv |
| **SGLang** | Zheng et al., 2023 | arXiv (UC Berkeley) |

### Who did NOT publish papers
- **LangChain / LangGraph**: no paper (they are referenced IN other papers)
- **CrewAI**: no paper
- **Prefect, Airflow, dbt, Terraform, Temporal**: no academic papers
- **n8n, Flowise, Rivet, Activepieces**: no papers

### Key insight
The tools that published papers are overwhelmingly **backed by research labs or university affiliations**
(Stanford, Microsoft Research, UC Berkeley). Pure dev-tool companies (HashiCorp, Prefect, dbt Labs)
did not go the academic route.

However, the newer AI agent frameworks (DSPy, AutoGen, TapeAgents) show that **arXiv papers for
AI orchestration tools are now common and expected** in the space.

### Nika's angle
A paper on Nika would be novel because:
1. **No existing paper** covers a declarative YAML DSL for multi-provider LLM orchestration.
2. The "5-verb" paradigm (infer/exec/fetch/invoke/agent) is a genuine design contribution.
3. Structured output with 5-layer defense is a differentiated system.
4. Rust-based, AGPL-licensed -- a different philosophy from the Python-heavy landscape.

### Verdict: STRONG opportunity. The orchestration space has papers but no one owns the "declarative workflow" niche.

---

## 4. Software Heritage (softwareheritage.org)

### What it is
- Non-profit by **Inria**, backed by **UNESCO**.
- The "Library of Alexandria" for source code -- archives ALL publicly available software.
- Uses a **Merkle DAG** (like Git) with cryptographic hashes for integrity.

### SWHID (SoftWare Hash IDentifier)
- Persistent, intrinsic identifier for any software artifact.
- Format: `swh:1:snp:<hash>` (snapshot), `swh:1:rev:<hash>` (revision), etc.
- **ISO/IEC 18670 standard** since April 2025 -- legally recognized.

### How to archive Nika
1. Go to `https://archive.softwareheritage.org/save/`
2. Paste `https://github.com/SuperNovae-studio/nika`
3. Click "Save Code Now"
4. Software Heritage snapshots the entire repo with full git history.
5. Each snapshot gets a SWHID -- a permanent, verifiable reference.

### Prior art value
- **Timestamped proof of existence**: the archive records WHEN code was ingested.
- **Immutable**: once archived, the SWHID cannot be forged or altered.
- **Legal standing**: ISO standard, recognized by French open science policy.
- Useful for: patent disputes, license compliance, provenance tracking.

### Cost: FREE. Entirely free, open access.

### Action items
1. Archive Nika NOW via "Save Code Now" (takes seconds).
2. Record the SWHID in the README and CITATION.cff.
3. Re-archive before each major release for timestamped snapshots.
4. Use SWHID in any future paper for permanent code references.

### Verdict: DO THIS TODAY. Zero cost, 2 minutes, permanent prior art timestamp.

---

## 5. Zenodo (DOI for Code)

### What it is
- CERN-operated open repository for research data, software, and publications.
- **Free** (up to 50GB per upload).
- Assigns **DOI** (Digital Object Identifier) -- the standard for academic citations.

### GitHub integration (step by step)
1. Log in to [zenodo.org](https://zenodo.org) with GitHub.
2. Go to Settings > GitHub and click "Sync now".
3. Find `SuperNovae-studio/nika` in the list, toggle ON.
4. Create a GitHub Release (e.g., v0.56.0).
5. Zenodo **automatically** archives the release and assigns a DOI.
6. Each release gets its own DOI + a "concept DOI" that always points to latest.
7. Copy the DOI badge into the README.

### Metadata best practices
- Add a `CITATION.cff` file to the repo root:
  ```yaml
  cff-version: 1.2.0
  message: "If you use this software, please cite it as below."
  type: software
  title: "Nika: Semantic YAML Workflow Engine for AI Tasks"
  authors:
    - family-names: Melen
      given-names: Thibaut
      orcid: "https://orcid.org/XXXX-XXXX-XXXX-XXXX"
  repository-code: "https://github.com/SuperNovae-studio/nika"
  license: AGPL-3.0-or-later
  version: "0.56.0"
  date-released: "2026-03-31"
  ```
- Add a `zenodo.json` for richer metadata (keywords, grants, related identifiers).
- GitHub will render a "Cite this repository" button from CITATION.cff.

### Zenodo vs arXiv

| Aspect | Zenodo | arXiv |
|--------|--------|-------|
| **What it hosts** | Data, software, papers, anything | Preprint papers only |
| **DOI** | Yes, automatic | No (must link externally) |
| **Peer review** | No | No (preprints) |
| **Best for software** | Yes -- designed for it | No -- papers about software |
| **GitHub integration** | Native (auto-archive on release) | Manual upload |
| **Academic credit** | Citable DOI, indexed | High visibility, no DOI |

### Verdict: SET UP THIS WEEK. Auto-DOI on every release. Essential for citations.

---

## 6. BONUS: JOSS (Journal of Open Source Software)

Discovered during research -- highly relevant.

### What it is
- A **peer-reviewed journal** specifically for open-source research software.
- Published by the Open Journals Foundation.
- Review happens **entirely on GitHub** via issues.
- Accepted papers get a **CrossRef DOI** and formal publication.

### Requirements for Nika
- Open source: YES (AGPL-3.0)
- Public Git repo with >6 months history: YES
- Feature-complete with docs and tests: YES (9000+ tests)
- Substantial effort (>3 months individual work): YES
- **Research application**: must demonstrate that the software enables new research or
  significantly improves existing workflows. Nika's multi-provider LLM orchestration qualifies.

### Submission process
1. Write `paper.md` (short -- 250-1000 words) with summary, statement of need, references.
2. Host it alongside the code in the repo.
3. Submit at joss.theoj.org with ORCID.
4. 2 reviewers assigned, review via GitHub issues.
5. Typical timeline: **2-6 months**.
6. Accepted = DOI + formal peer-reviewed publication.

### Why JOSS over arXiv alone
- **Peer-reviewed**: arXiv is not. JOSS gives academic legitimacy.
- **DOI**: included automatically.
- **Developer-friendly**: review on GitHub, paper is short, focus is on the SOFTWARE not the prose.
- **Recognized**: indexed by ADS, Google Scholar, DOAJ, Crossref.

### Verdict: STRONG CANDIDATE. Short paper, GitHub-native review, peer-reviewed DOI.

---

## Recommended Action Plan

### Immediate (this week)

| Action | Time | Value |
|--------|------|-------|
| Archive Nika on Software Heritage | 2 min | Prior art timestamp, SWHID |
| Set up Zenodo-GitHub integration | 15 min | Auto-DOI on every release |
| Create CITATION.cff | 30 min | GitHub "Cite this repo" button |
| Get an ORCID | 5 min | Required for JOSS, useful everywhere |

### Short-term (April 2026)

| Action | Time | Value |
|--------|------|-------|
| Write JOSS paper.md (~500 words) | 1-2 days | Peer-reviewed publication |
| Submit to JOSS | 1 hour | Review starts in ~2 weeks |
| Draft arXiv paper (longer, ~8 pages) | 1-2 weeks | Broader academic visibility |

### Medium-term (after JOSS acceptance)

| Action | Time | Value |
|--------|------|-------|
| Submit arXiv paper to cs.AI + cs.SE | 1 day | Preprint for HF Papers |
| Submit to HF Daily Papers | 5 min | ML community amplification |
| Target a conference (ICSE NIER, CHASE, or ASE) | Varies | Peer-reviewed venue |

### Paper angle for Nika

**Suggested title**: "Nika: Declarative Multi-Provider LLM Orchestration via Semantic YAML Workflows"

**Key contributions to highlight**:
1. The 5-verb abstraction (infer/exec/fetch/invoke/agent) as a minimal complete set
2. Provider-agnostic structured output with 5-layer defense
3. DAG-based task scheduling with dependency resolution
4. Comparison with imperative approaches (LangChain, CrewAI) showing fewer LOC for equivalent tasks
5. Performance characteristics of Rust-based engine vs Python alternatives

---

## Sources

1. Hugging Face Daily Papers -- https://huggingface.co/papers
2. arXiv category taxonomy -- https://arxiv.org/category_taxonomy
3. Software Heritage -- https://www.softwareheritage.org/
4. Zenodo -- https://zenodo.org/
5. JOSS -- https://joss.theoj.org/
6. CITATION.cff spec -- https://citation-file-format.github.io/
7. SWHID spec (ISO/IEC 18670) -- https://www.swhid.org/
8. DSPy paper -- arXiv:2310.03714
9. AutoGen paper -- arXiv:2308.08155
10. vLLM paper -- SOSP 2023
11. TensorFlow paper -- OSDI 2016

## Methodology
- Tools used: Perplexity AI (sonar model), 8 queries
- Cross-referenced across multiple sources per topic
- Focused on actionable, verified information

## Confidence Level
**High** for Zenodo, Software Heritage, JOSS (well-documented processes).
**Medium** for arXiv strategy (endorsement process varies by individual case).
**Medium** for HF Papers (curator selection is somewhat opaque).
