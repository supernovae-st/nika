# arXiv Endorsement Strategy for First-Time Submitters Without University Affiliation

> Deep research conducted 2026-03-31. Actionable guide for Thibaut Melen / SuperNovae Studio
> submitting a Nika paper to arXiv cs.AI.

---

## Table of Contents

1. [How the arXiv Endorsement System Works](#1-how-the-arxiv-endorsement-system-works)
2. [January 2026 Policy Update -- Critical Change](#2-january-2026-policy-update--critical-change)
3. [Strategies for Getting Endorsed](#3-strategies-for-getting-endorsed)
4. [HAL as Alternative/Complement](#4-hal-french-open-archive-as-alternativecomplement)
5. [What Happens If You Submit Without Endorsement](#5-what-happens-if-you-submit-without-endorsement)
6. [French AI Researchers Who Could Endorse](#6-french-ai-researchers-who-could-endorse)
7. [Realistic Timeline](#7-realistic-timeline)
8. [Preprint Servers That Don't Require Endorsement](#8-preprint-servers-that-dont-require-endorsement)
9. [Recommended Strategy](#9-recommended-strategy-for-nika)

---

## 1. How the arXiv Endorsement System Works

### The Basics

arXiv requires that **first-time submitters** be endorsed before submitting their first paper
to arXiv, or to a new category. Endorsement is **per endorsement domain** (a group of related
subject areas), not per individual category.

For Computer Science, the entire `cs.*` family (cs.AI, cs.SE, cs.PL, cs.CL, etc.) forms a
**single endorsement domain**. This means an endorser who has published in cs.CL can endorse
you for cs.AI. This is favorable -- it broadens the pool of potential endorsers.

### Two Paths to Endorsement (as of January 2026)

**Path 1 -- Automatic (institutional)**:
- Requires BOTH:
  1. An institutional email address (university, research lab)
  2. Previous authorship on an existing arXiv paper in the target endorsement domain
- Since SuperNovae Studio is not an academic institution, this path is **not available**.

**Path 2 -- Personal endorsement**:
- An established arXiv author in the same endorsement domain personally vouches for you.
- They need your **endorsement code** (a 6-character alphanumeric code you receive by email).
- Only ONE positive endorsement is needed.

### Who Can Be an Endorser?

An endorser must meet ALL of these criteria:
- Has authored a certain number of papers in the cs.* endorsement domain on arXiv
- Papers must have been submitted between **3 months and 5 years ago** (not too old, not too new)
- Must be registered as an author of those papers on arXiv
- Must themselves have active positive endorsement for that area

**Key insight**: Any active CS researcher with arXiv papers from the past 5 years can
potentially endorse you. You can check if a specific author can endorse by going to any
arXiv paper's abstract page and clicking "Which authors of this paper are endorsers?" at
the bottom.

### What Endorsers Are Expected to Do

The endorsement process is **NOT peer review**. Endorsers should:
- Know you personally OR have seen the paper you intend to submit
- Check that the paper is appropriate for the subject area
- Verify the author is not unfamiliar with basic facts of the field
- Treat any shared content as privileged/confidential

Endorsers are NOT expected to:
- Read the paper in detail
- Verify correctness of the work
- Guarantee quality

---

## 2. January 2026 Policy Update -- Critical Change

**Source**: https://blog.arxiv.org/2026/01/21/attention-authors-updated-endorsement-policy/

As of **January 21, 2026**, arXiv **no longer accepts institutional email addresses as the
sole qualifier** for endorsement. Previously, having an `.edu` or research institution email
was enough. Now you need BOTH institutional email AND prior arXiv authorship.

This was implemented to stem "an unsustainable increase in non-scientific submissions."

**Impact for Thibaut**: This change actually does not worsen your situation -- since you
don't have an institutional email anyway, you were always going to need personal endorsement.
But it DOES mean that the pool of people who got endorsed automatically is smaller, so there
may be slightly more people seeking personal endorsements, potentially making endorsers
busier or more cautious.

**Important**: arXiv staff **cannot** waive endorsement requirements or provide personal
endorsements. The only path is finding an endorser yourself.

---

## 3. Strategies for Getting Endorsed

### Strategy A: Direct Outreach to Researchers (HIGHEST SUCCESS RATE)

**How it works**: Email researchers who work on related topics, send them your paper draft,
and ask for endorsement.

**Best practices**:
1. Write a short, professional email (not a form letter)
2. Briefly introduce yourself and your work
3. Attach or link to the paper you intend to submit
4. Include your endorsement code (you get this after starting a submission)
5. Explain why their work is relevant to yours (cite them if possible)
6. Make it easy for them -- they just need to click a link and enter a code

**DO NOT**:
- Mass-email dozens of researchers at once (arXiv explicitly says this is inappropriate)
- Repeatedly email the same person
- Send generic requests without context

**Who to target** (in order of likelihood of success):
1. Researchers whose work you cite in your paper
2. Researchers working on adjacent topics (AI orchestration, LLM agents, workflow systems)
3. French researchers (cultural proximity, easier to establish rapport)
4. Open-source-friendly academics (more sympathetic to non-institutional work)

### Strategy B: Post Preprint Elsewhere First, Then Seek Endorsement

**Rationale**: Having a visible, polished preprint on another platform gives you credibility
when approaching endorsers.

**Steps**:
1. Upload to HAL, Zenodo, or SSRN first
2. Share on Twitter/X academic community
3. When approaching endorsers, link to the existing preprint as proof of quality

### Strategy C: Publish in JOSS First

**Journal of Open Source Software** (https://joss.theoj.org) is a legitimate, peer-reviewed
journal (ISSN 2475-9066) specifically for research software. After acceptance, you get a
Crossref DOI.

**Nika is a strong JOSS candidate** because:
- It is research software: an AI workflow engine (solves complex modeling/orchestration problems)
- It is open source (AGPL-3.0)
- It has substantial documentation, tests (9000+), and a clear research contribution
- JOSS explicitly welcomes software that "supports the execution of research experiments"

**How JOSS helps with arXiv**:
- A JOSS DOI gives you academic credibility
- JOSS reviewers/editors may be willing to endorse you afterward
- You can reference the JOSS paper in your arXiv submission

**JOSS requirements**:
- Software must be open source and have a research purpose
- Paper is short (typically 1-2 pages, focused on the software)
- Review is about software quality, documentation, and tests -- not traditional academic review
- Timeline: typically 4-8 weeks from submission to acceptance

**IMPORTANT caveat**: JOSS has recently updated scope requirements. Pre-trained ML models
and notebooks are out of scope, but AI workflow engines should still qualify. Read
https://joss.readthedocs.io/en/latest/review_criteria.html carefully.

### Strategy D: Academic Twitter/X Community

The AI research community on Twitter/X is very active. Posting your work there can attract
attention from researchers who might endorse you.

**Tactics**:
- Tweet a thread about the paper with key findings and a link to the preprint
- Tag relevant researchers (do NOT cold-DM asking for endorsement)
- Engage with the #LLM, #AIAgents, #NLProc communities
- Post in the Hugging Face community, r/MachineLearning, etc.

### Strategy E: Attend Conferences / Workshops

If timeline allows, presenting at a workshop or conference (even a poster) creates
face-to-face connections with potential endorsers.

**Relevant venues**:
- AAAI, ICML, NeurIPS, ICLR workshops
- ECAI (European Conference on AI) -- often in Europe
- JFLA (Journees Francophones des Langages Applicatifs) -- French community
- Compas, RJCIA -- French CS conferences

---

## 4. HAL (French Open Archive) as Alternative/Complement

### What is HAL?

HAL (Hyper Articles en Ligne) is France's national open archive, operated by the CCSD
(Centre pour la Communication Scientifique Directe, CNRS). URL: https://hal.science

### Key Facts

| Aspect | Details |
|--------|---------|
| **Endorsement required?** | **NO** -- anyone can create an account and deposit |
| **Peer review?** | **NO** -- but deposits are moderated (basic quality check) |
| **Affiliation required?** | Not strictly -- you need an account, but anyone can create one |
| **DOI?** | HAL assigns a HAL identifier, and can mint DOIs for some deposits |
| **Respected internationally?** | Yes, within Europe. Less known in US/Asia than arXiv |
| **OAI-PMH harvesting?** | Yes -- Google Scholar, BASE, CORE, and others index HAL |
| **arXiv transfer?** | **YES** -- HAL has a built-in "Transfer to arXiv" feature |
| **Software deposits?** | YES -- HAL supports source code deposits (Software Heritage integration) |

### HAL Deposit Requirements

- Create an account (email address, no institutional requirement)
- Fill in metadata (title, authors, abstract in English recommended, domain/discipline)
- Upload PDF of the paper
- For transfer to arXiv: must include abstract in English, specific domain, no existing
  arXiv ID, and all LaTeX source files including figures (max 3MB per file, 10MB total)
- The `.bbl` file is required (arXiv rejects `.bib` files)

### HAL to arXiv Transfer

HAL has a **built-in feature** to transfer deposits to arXiv:
1. Deposit on HAL first (with full-text PDF + LaTeX sources)
2. At the last step of deposit, the "Transfer to arXiv" option appears
3. Conditions: file present, English abstract, appropriate domain, no existing arXiv ID
4. After transfer, arXiv moderates the submission independently

**IMPORTANT**: This transfer does **not bypass endorsement**. You still need to be endorsed
on arXiv for the transfer to succeed. The HAL-arXiv transfer is a convenience feature for
uploading, not an endorsement bypass.

### HAL vs arXiv for Visibility

| Factor | arXiv | HAL |
|--------|-------|-----|
| CS/AI community visibility | Very high (global standard) | Moderate (strong in France) |
| Google Scholar indexing | Excellent | Good |
| Speed to appear | 1-2 days after moderation | 1-3 days after moderation |
| Hugging Face Papers integration | Yes (requires arXiv) | No |
| International recognition | Global | Primarily European |
| Semantic Scholar coverage | Full | Partial |

### Recommendation

**Use HAL as a complement, not a replacement.** Deposit on HAL immediately (no endorsement
needed), then work on arXiv endorsement in parallel. Having the paper on HAL gives you a
citable, timestamped preprint while you work on arXiv access.

---

## 5. What Happens If You Submit Without Endorsement

### The Process

1. **Create an arXiv account** (anyone can do this)
2. **Start a new submission** and select your category (cs.AI)
3. **arXiv checks endorsement** -- if you don't have it, the submission is **held**
4. You receive an **email with your endorsement code** and instructions on how to find endorsers
5. The submission is **not rejected** -- it is in a "pending endorsement" state
6. You have **time to find an endorser** (arXiv does not specify an exact deadline, but the
   submission will eventually expire if not endorsed)
7. Once endorsed, the submission proceeds to normal moderation

### Key Points

- Your paper is NOT publicly visible during this waiting period
- The clock is effectively paused until endorsement is received
- You can start the submission, get your endorsement code, and then reach out to endorsers
- This is actually the **recommended workflow**: start submission first, get the code, THEN
  find endorsers

---

## 6. French AI Researchers Who Could Endorse

### Tier 1: Most Relevant to Nika's Domain (AI systems, SE, workflow, LLM agents)

#### Martin Monperrus
- **Affiliation**: KTH Royal Institute of Technology (previously INRIA)
- **Nationality**: French
- **Research**: Software engineering, program repair, AI for code
- **arXiv presence**: Very active (100+ papers in cs.SE, cs.AI)
- **Recent papers**: "Bootstrapping Coding Agents", "Project Rachel: Can an AI Become a Scholarly Author?"
- **Why relevant**: SE + AI intersection, exactly Nika's domain
- **Contact**: Available on his website (monperrus.net)
- **Likelihood**: MEDIUM -- very busy, but directly relevant topic

#### Benoit Combemale
- **Affiliation**: INRIA Rennes / Universite de Rennes
- **Research**: Software engineering, model-driven engineering, domain-specific languages
- **arXiv presence**: Active (cs.SE)
- **Recent papers**: "Reclaiming Software Engineering as the Enabling Technology for the Digital Age"
- **Why relevant**: DSLs and workflow systems are his core domain; Nika's YAML DSL is exactly this
- **Likelihood**: HIGH -- MDE/DSL researchers are rare and tend to be enthusiastic about new DSLs

#### Jean-Marc Jezequel
- **Affiliation**: INRIA Rennes / Universite de Rennes
- **Research**: Software engineering, software product lines, configuration
- **arXiv presence**: Active (cs.SE)
- **Why relevant**: Software architecture and configuration systems
- **Likelihood**: MEDIUM

#### Lionel Seinturier
- **Affiliation**: INRIA Lille / Universite de Lille
- **Research**: Middleware, software engineering, cloud computing
- **arXiv presence**: Active (cs.SE, cs.DC)
- **Why relevant**: Middleware and orchestration systems
- **Likelihood**: MEDIUM

### Tier 2: Broader AI/ML Researchers in France

#### Xavier Hinaut
- **Affiliation**: INRIA Bordeaux (FLOWERS team)
- **Research**: Computational neuroscience, LLM deployment, reservoir computing
- **arXiv presence**: Active
- **Recent paper**: "Deploying Open-Source Large Language Models: A Performance Analysis"
- **Why relevant**: Open-source LLM deployment is adjacent to Nika's multi-provider approach
- **Likelihood**: MEDIUM-HIGH -- open-source focused, may be sympathetic

#### Patrick Gallinari
- **Affiliation**: Sorbonne Universite / ISIR (now also Criteo)
- **Research**: Machine learning, deep learning, AI systems
- **arXiv presence**: Active (cs.AI, cs.LG)
- **Why relevant**: Broad AI expertise, Sorbonne-based
- **Likelihood**: MEDIUM

#### Pierre Senellart
- **Affiliation**: ENS Paris (DI department)
- **Research**: Data management, web data, provenance
- **arXiv presence**: Active
- **Why relevant**: Data workflows, web scraping (Nika's fetch verb)
- **Likelihood**: LOW-MEDIUM -- more database-focused

### Tier 3: Industry-Adjacent Researchers

#### Guillaume Lample
- **Affiliation**: Meta AI (FAIR Paris)
- **Research**: LLMs, code generation
- **Why relevant**: LLM-focused, but may be too busy at Meta

#### Yann LeCun
- **Affiliation**: Meta AI / NYU (French)
- **Why relevant**: French, AI, very high profile
- **Likelihood**: VERY LOW -- too famous, unlikely to respond to cold outreach

### How to Find More Endorsers

For any arXiv paper in cs.AI or cs.SE, go to the abstract page and click:
**"Which authors of this paper are endorsers?"**

This link appears at the bottom of every arXiv abstract. It shows you exactly which authors
of that paper have endorsement privileges. This is the most reliable way to identify
potential endorsers.

### Recommended Approach Order

1. Search arXiv for papers closest to Nika's topic (AI workflow engines, LLM orchestration,
   multi-agent systems, DSLs for AI)
2. Use the "Which authors are endorsers?" link to confirm eligibility
3. Prioritize French researchers (easier rapport, shared context)
4. Contact 2-3 people at most initially, with personalized emails
5. Include your draft paper and explain the connection to their work

---

## 7. Realistic Timeline

### Fastest Path (2-4 weeks)

```
Day 1:     Create arXiv account
Day 1:     Start submission, get endorsement code
Day 1:     Deposit preprint on HAL and Zenodo (instant visibility)
Day 2-3:   Email 2-3 targeted researchers with paper + endorsement code
Day 3-14:  Wait for response (most academics respond within 1-2 weeks)
Day 14:    If endorsed, submission proceeds to arXiv moderation
Day 15-17: arXiv moderation (typically 1-2 business days)
Day 17:    Paper appears on arXiv
```

### Moderate Path with JOSS (2-3 months)

```
Week 1:    Submit to HAL + Zenodo for immediate visibility
Week 1:    Submit to JOSS (software paper about Nika)
Week 2-8:  JOSS review process
Week 8:    JOSS acceptance -> DOI, credibility
Week 8:    Use JOSS credibility to approach arXiv endorsers
Week 9-10: Get endorsed, submit to arXiv
Week 10:   Paper on arXiv
```

### Backup Path (1-2 months)

```
Week 1:    Deposit on HAL + Zenodo
Week 1:    Start arXiv submission, get endorsement code
Week 1-2:  Email targeted researchers
Week 2-3:  If no response, try 2-3 more researchers
Week 3-4:  Post on Twitter/X to attract attention
Week 4-6:  If still no endorsement, attend a workshop or conference to meet people
Week 6-8:  Get endorsed via personal connection
```

---

## 8. Preprint Servers That Don't Require Endorsement

### Zenodo (CERN/OpenAIRE)

- **URL**: https://zenodo.org
- **Endorsement**: NONE required
- **DOI**: YES -- automatic Zenodo DOI assigned
- **Visibility**: Good (indexed by Google Scholar, OpenAIRE, DataCite)
- **Format**: Any (PDF, code, data, videos)
- **Speed**: Immediate publication
- **Best for**: Software artifacts, datasets, and papers simultaneously
- **GitHub integration**: Direct integration to archive GitHub releases with a DOI
- **Verdict**: EXCELLENT complement. Use Zenodo to get a DOI immediately.

### HAL (already covered above)

- **Endorsement**: NONE
- **DOI**: HAL ID (DOI possible for some deposits)
- **Visibility**: Strong in Europe, Google Scholar indexed
- **Verdict**: ESSENTIAL for French researchers. Deposit immediately.

### TechRxiv (IEEE)

- **URL**: https://www.techrxiv.org
- **Endorsement**: NONE required
- **Peer moderation**: Basic editorial screening
- **DOI**: YES (IEEE DOI)
- **Visibility**: Good (IEEE ecosystem, Google Scholar)
- **Focus**: Engineering and technology
- **Verdict**: GOOD for engineering/systems papers. Less AI-focused than arXiv.

### SSRN (Elsevier)

- **URL**: https://www.ssrn.com
- **Endorsement**: NONE
- **Focus**: Originally social sciences, now has Computer Science Network (CSN)
- **Visibility**: Moderate for CS (strong in economics/law)
- **Verdict**: LOW priority for CS/AI papers.

### ResearchGate

- **URL**: https://www.researchgate.net
- **Type**: Academic social network with preprint hosting
- **Endorsement**: NONE (need account)
- **DOI**: No (references external DOIs)
- **Visibility**: Good within academic community
- **Verdict**: MEDIUM -- useful for networking, but not a true preprint server.

### OpenReview

- **URL**: https://openreview.net
- **Type**: Open peer review platform, primarily used for conference submissions
- **Endorsement**: Varies by venue
- **Self-posting**: Not available for arbitrary preprints (tied to conference/workshop submissions)
- **Verdict**: NOT suitable for standalone preprints.

### Comparison Summary

| Platform | Endorsement | DOI | CS/AI Visibility | Speed |
|----------|-------------|-----|------------------|-------|
| **arXiv** | Required | Yes (arXiv ID) | Highest | 1-2 days after endorsement |
| **HAL** | None | HAL ID | High (Europe) | 1-3 days |
| **Zenodo** | None | Yes (DOI) | Medium | Immediate |
| **TechRxiv** | None | Yes (IEEE DOI) | Medium | 1-3 days |
| **SSRN** | None | Yes | Low (for CS) | 1-3 days |
| **JOSS** | Peer review | Yes (Crossref DOI) | High (for software) | 4-8 weeks |

---

## 9. Recommended Strategy for Nika

### Phase 1: Immediate (Day 1-2)

1. **Create arXiv account** using your best email (supernovae.studio domain is fine)
2. **Start submission to cs.AI** -- this gives you the endorsement code
3. **Deposit on HAL** with full metadata, English abstract, PDF
4. **Upload to Zenodo** with GitHub integration for the Nika repo -- get a DOI immediately
5. **Create an ORCID** if you don't have one (free, 2 minutes, gives you academic identity)

### Phase 2: Endorsement Hunt (Day 2-14)

1. **Identify 3-5 target endorsers** using the method above (arXiv "which authors are endorsers?" links)
2. **Prioritize**:
   - Benoit Combemale (INRIA Rennes) -- DSLs, MDE, closest match
   - Martin Monperrus (KTH/ex-INRIA) -- SE + AI, very active
   - Xavier Hinaut (INRIA Bordeaux) -- open-source LLM deployment
3. **Write personalized emails** (see template below)
4. **Include**: your paper draft, endorsement code, link to HAL/Zenodo preprint, link to Nika GitHub
5. Wait 7-10 days for responses

### Phase 3: Parallel Tracks (Week 2-8)

1. **Submit to JOSS** -- independent of arXiv, gets you a peer-reviewed DOI
2. **Share on Twitter/X** -- build visibility for the preprint
3. If no endorsement by Week 3, reach out to 2-3 more researchers
4. Post on Hacker News, r/MachineLearning, r/Rust -- build grassroots visibility

### Email Template for Endorsement Request

```
Subject: arXiv endorsement request -- AI workflow engine paper (cs.AI)

Dear Prof./Dr. [Name],

I am Thibaut Melen, an independent researcher and founder of SuperNovae Studio
in Paris. I am writing to request your endorsement for a first submission to
arXiv in cs.AI.

My paper presents Nika, an open-source (AGPL-3.0) AI workflow engine written in
Rust. It introduces "inference as code" -- a declarative YAML-based approach to
orchestrating LLM tasks with 5 semantic verbs, 7 cloud providers, and 41 builtin
tools. The system has 356K lines of code and 9,000+ tests.

I believe this work connects to your research on [SPECIFIC TOPIC from their work],
particularly [SPECIFIC PAPER you cite].

You can find the preprint here: [HAL/Zenodo link]
The source code is at: https://github.com/SuperNovae-studio/nika

My arXiv endorsement code is: [CODE]

If you would be willing to endorse this submission, the process takes just a
few minutes -- you would enter the code at [arXiv endorsement URL]. I understand
endorsement is not a review of the work, just a confirmation that it is
appropriate for the cs.AI category.

Thank you for considering this request.

Best regards,
Thibaut Melen
SuperNovae Studio, Paris
[links to ORCID, GitHub, website]
```

### What NOT to Do

- Do NOT email more than 5 people at once
- Do NOT send identical form letters
- Do NOT email arXiv staff asking for help (they cannot endorse you)
- Do NOT claim university affiliation you don't have
- Do NOT get discouraged if the first 2-3 people don't respond -- this is normal

---

## Sources

1. **arXiv Endorsement System** -- https://info.arxiv.org/help/endorsement.html
   (Official documentation, scraped 2026-03-31)

2. **arXiv Updated Endorsement Policy (Jan 2026)** --
   https://blog.arxiv.org/2026/01/21/attention-authors-updated-endorsement-policy/
   (Blog post announcing the major policy change)

3. **HAL Documentation** -- https://documentation.hal.science/deposer/
   (HAL deposit guide, French)

4. **HAL FAQ on arXiv Transfer** -- https://documentation.hal.science/faq/
   (Transfer conditions: file, English abstract, domain, LaTeX sources)

5. **JOSS About Page** -- https://joss.theoj.org/about
   (Submission scope, requirements, process)

6. **Zenodo About** -- https://about.zenodo.org/
   (CERN-hosted, no endorsement, DOI minting)

7. **arXiv Search Results** -- various searches for French AI/SE researchers
   (Identified endorser candidates)

## Methodology

- Tools used: curl (web scraping), arXiv search API, HAL documentation
- Pages analyzed: 15+
- Time period covered: arXiv policy current as of January 2026
- Cross-referenced: arXiv official docs, blog posts, HAL docs, JOSS docs

## Confidence Level

**HIGH** for endorsement system mechanics (sourced directly from arXiv official docs and
the January 2026 blog post).

**MEDIUM-HIGH** for researcher recommendations (based on arXiv publication records, but
actual willingness to endorse is unpredictable).

**MEDIUM** for HAL-arXiv transfer details (HAL docs were partially unavailable due to
site migration; core facts cross-referenced with arXiv docs and FAQ).

## Further Research Suggestions

- Check specific endorser eligibility using "Which authors of this paper are endorsers?"
  links on arXiv papers closest to Nika's topic
- Investigate whether CIFRE or ANRT conventions could provide an institutional affiliation
  (long-term play, probably not worth it just for arXiv)
- Consider co-authoring with an academic researcher who would then handle submission
  (eliminates endorsement problem entirely, but requires finding a willing collaborator)
- Look into the INRIA "Startup Studio" program -- some INRIA-affiliated startups may
  qualify for institutional email
