# Nika — One-Pager

> For investors, partners, conference organizers, and press.

---

## The Problem

AI is the new electricity — but electricity doesn't cost $49/month with a 1,000-run limit.

Six closed labs control frontier AI. Chips cost $6M per rack. LLM subscriptions run
$20-$200/month. The platforms that promise "democratization" charge you to run automations
on THEIR servers, with THEIR limits. And even if you pay — you still need a software
engineer to wire anything useful together.

**The result:** the technology that should empower billions is gatekept by a handful of corporations.

---

## The Solution

**Nika** is a single open-source binary that reads a YAML text file and executes AI workflows.
No Python. No Docker. No cloud. No subscription.

```yaml
schema: "nika/workflow@0.12"
tasks:
  - id: scrape
    fetch: { url: "https://news.ycombinator.com", extract: article }
  - id: summarize
    with: { page: $scrape }
    infer: "3-bullet summary: {{with.page}}"
```

Two steps. Zero lines of code. Runs on a Raspberry Pi.

---

## The Proof

| Metric | Nika | LangChain | CrewAI |
|--------|------|-----------|--------|
| **Peak RAM** (10 web pages) | **1,046 MB** | 5,706 MB | excluded |
| **Cold start** | **4 ms** | 62 ms | unknown |
| **Throughput** | **4.97 rps** | 4.26 rps | 44% failure rate |
| **Dependencies** | **0** (single binary) | 400+ pip packages | 300+ |
| **Config lines** (same task) | **12** | 48 | 55 |

> "The memory advantage is 5x, and it's structural — not something you tune away."
> — AutoAgents Benchmark, Feb 2026

---

## Five Verbs — The Entire API

| Verb | Does |
|------|------|
| `infer:` | Call any LLM (22 providers, vision, structured output) |
| `exec:` | Run shell commands (28-pattern security blocklist) |
| `fetch:` | HTTP + 9 extract modes (markdown, article, RSS, JSONPath...) |
| `invoke:` | MCP tool calls (43 built-in + any MCP server) |
| `agent:` | Multi-turn autonomous loops (guardrails, cost limits) |

Five words to describe any AI workflow. From a 3-step summary to a 50-task parallel pipeline.

---

## Key Numbers

| | |
|---|---|
| **451K lines** of Rust across 10 crates | **7,800+ tests**, zero clippy warnings |
| **22 LLM providers** (cloud + local) | **43 built-in tools** (12 core + 24 media + 5 file) |
| **200+ showcase workflows** | **44 exercises** across 12 interactive levels |
| **AGPL-3.0** — stays open forever | **4ms cold start** — runs in CI, on Pi, in serverless |

---

## Market

The AI orchestration market is projected at **$10B+ by 2026** (Gartner). The landscape is
fragmented: Python-heavy frameworks (LangChain, CrewAI), GUI builders (Dify, n8n), and
traditional workflow engines bolting on AI (Temporal, Airflow).

**No existing tool combines:** YAML-native + Rust binary + MCP protocol + media pipeline +
LSP + interactive course. Nika occupies an entirely uncontested quadrant.

### Framework fatigue is real

- LangChain: CVSS 9.3 vulnerability, debugging hell, breaking changes every month
- CrewAI: 44% failure rate in production benchmarks
- Zapier: $49/mo for 750 tasks — Nika: $0 for unlimited
- All of them: require Python, Docker, or both

---

## Use Cases (real-world, sourced)

| Industry | Use Case | Evidence |
|----------|----------|----------|
| **Sales** | Lead enrichment from LinkedIn → AI scoring → CRM push | n8n's #1 workflow category |
| **Content** | AI video pipeline: script → voice → video → publish | 6 trending n8n templates |
| **Healthcare** | Clinical docs → structured datasets (Flatiron Health) | **Saved 2.5 FTE weeks/project** |
| **IT Ops** | Employee account recovery (Delivery Hero) | **Saved 200 hours/month** |
| **DevOps** | SIEM → AI triage → containment → ticket | Meta, Microsoft, Vodafone |
| **Legal** | Contract risk scanning → clause analysis → report | Langflow featured template |
| **QR Code** | AI design → scan validation → C2PA provenance | **Nika's core domain** (qrcode-ai.com) |

---

## Business Model

**Open source (AGPL-3.0).** No SaaS. No cloud tier. No per-execution pricing.

Revenue comes from **QR Code AI** (qrcode-ai.com) — a SaaS platform powered by Nika
and NovaNet for AI-powered QR code generation. Every feature in Nika is battle-tested
in production.

---

## Team

**Thibaut Melen** — Creator. Solo developer with AI assistance (Claude).
451K lines of Rust. 7,800+ tests. Pre-launch.

**SuperNovae Studio** — @SuperNovae-studio
**Product:** QR Code AI (qrcode-ai.com)
**GitHub:** github.com/supernovae-st/nika

---

> "AI shouldn't have a subscription fee. Nika is a single binary that gives everyone access."

**AGPL-3.0-or-later. Open source. Forever.**
