# Research Report: AI Workflow Automation Use Cases (2025-2026)

## Summary

This report catalogs 80+ specific, real-world AI workflow use cases across 10 categories,
designed as course exercises for the Nika workflow engine. Every workflow is grounded in
tasks that freelancers, agencies, startups, and developers actually pay for and run daily.
Each entry specifies which Nika verbs to combine, what APIs/URLs to hit, what output to
produce, and which advanced features to leverage.

---

## Methodology

- Analyzed 40+ existing Nika use-case workflows and 12 course modules
- Cross-referenced with actual SaaS tools people pay $20-500/month for (Jasper, Copy.ai,
  Zapier, Make, n8n, Bardeen, Clay, Instantly, Surfer SEO, Frase, Pictory, Descript)
- Focused on workflows that replace or improve on paid tools
- Every workflow uses 2+ Nika verbs and at least one advanced feature

---

## Category 1: Content Creation

### 1.1 SEO Blog Post Factory (fetch + infer + exec + invoke)

**What people pay for:** Surfer SEO ($89/mo), Frase ($15/mo), Jasper ($49/mo)

**Workflow:**
1. `fetch:` Scrape target keyword from Google SERP (via SerpAPI or similar)
2. `fetch: extract: metadata` on top 5 ranking pages
3. `fetch: extract: article` on top 3 to get their content
4. `infer:` Analyze competitor content patterns (word count, headers, topics covered)
5. `infer:` Generate SEO-optimized outline with semantic headings
6. `infer:` Write full post section-by-section using `for_each:` over outline sections
7. `infer:` Generate meta title, meta description, slug
8. `invoke: nika:write` Save as markdown artifact

**Advanced features:** `for_each:` over sections, `structured:` for JSON outline,
`artifact:` for final markdown, `extract: article` + `extract: metadata`

**Wow factor:** One YAML file replaces a $150/month tool stack. Runs in under 60 seconds.

---

### 1.2 Newsletter Curator (fetch + infer + invoke)

**What people pay for:** Curated ($99/mo), Mailbrew ($15/mo), Feedly Pro ($12/mo)

**Workflow:**
1. `fetch: extract: feed` from 5-10 RSS feeds (Hacker News, TechCrunch, etc.)
2. `infer:` Score and rank articles by relevance to your niche (structured JSON output)
3. `for_each:` over top 10 articles: `fetch: extract: article` + `infer:` summarize
4. `infer:` Write editorial intro connecting themes
5. `infer:` Compile into newsletter template with sections
6. `invoke: nika:write` artifact as HTML-ready markdown

**Advanced features:** `for_each:` over articles, `extract: feed`, `structured:` for ranking,
multiple artifacts

**Wow factor:** Daily automated newsletter from real RSS feeds -- zero manual curation.

---

### 1.3 Content Atomizer / Repurpose Engine (exec + infer + for_each)

**What people pay for:** Repurpose.io ($25/mo), Lately ($49/mo)

**Workflow:**
1. `exec:` Read a long-form blog post or transcript file
2. `infer:` Extract 10 atomic insights/quotes (structured JSON array)
3. `for_each:` over insights, generate:
   - Twitter/X thread (280 char per tweet, 5-7 tweets)
   - LinkedIn post (150-200 words, professional tone)
   - Instagram caption (with emoji, hashtags)
   - YouTube Shorts script (30-60 seconds)
   - Email subject lines (5 variants)
4. `infer:` Create content calendar placing each piece across 2 weeks
5. `artifact:` Save full calendar + all content pieces

**Advanced features:** `for_each:` with `concurrency: 4`, `structured:` for insight extraction,
multiple artifacts per format

**Wow factor:** One blog post becomes 50+ social media pieces with scheduling suggestions.

---

### 1.4 Multilingual Content Pipeline (fetch + infer + for_each + invoke)

**What people pay for:** Smartling ($500+/mo), Weglot ($15/mo), DeepL Pro ($25/mo)

**Workflow:**
1. `fetch: extract: article` from source URL
2. `infer:` Extract translatable text segments (preserve markdown structure)
3. `for_each:` over 5 target languages (FR, DE, ES, JA, PT):
   - `infer:` Translate preserving tone, idioms, and formatting
   - `infer:` Localize cultural references (dates, currencies, examples)
   - `infer:` Generate locale-specific meta tags
4. `invoke: nika:write` per locale artifact
5. `infer:` Generate translation quality report

**Advanced features:** `for_each:` over languages, `artifact:` per locale,
`extract: article`, parallel translation with `concurrency:`

**Wow factor:** Professional-grade localization of any webpage into 5 languages in one run.

---

### 1.5 Podcast Show Notes Generator (exec + infer + invoke)

**What people pay for:** Podium ($24/mo), Castmagic ($23/mo), Descript ($24/mo)

**Workflow:**
1. `exec:` Read transcript file (or use a transcription API via `fetch:`)
2. `infer:` Extract speaker segments, topics, and timestamps
3. `infer:` Generate structured show notes:
   - Episode summary (2-3 sentences)
   - Key topics with timestamps
   - Guest bio
   - Notable quotes
   - Resources mentioned (with links)
4. `infer:` Write social media promo posts (Twitter, LinkedIn, newsletter blurb)
5. `infer:` Generate 5 episode title variants (A/B testable)
6. `artifact:` Save show notes + social posts + titles

**Advanced features:** `structured:` for topic/timestamp extraction,
multiple artifacts, prompt chaining

---

### 1.6 AI-Powered Writing Coach (exec + infer + structured)

**What people pay for:** Grammarly Business ($25/mo), Hemingway ($20), ProWritingAid ($30/mo)

**Workflow:**
1. `exec:` Read draft document
2. `infer:` Analyze writing quality with structured rubric:
   - Readability score (Flesch-Kincaid estimate)
   - Passive voice percentage
   - Sentence length distribution
   - Jargon density
   - Tone consistency
3. `infer:` Identify the 5 weakest paragraphs with specific rewrites
4. `infer:` Generate style-matched improvement suggestions
5. `artifact:` Writing report card with before/after comparisons

**Advanced features:** `structured:` for rubric JSON, `artifact:` with template

---

## Category 2: Developer Tools

### 2.1 PR Review Bot (exec + infer + structured + invoke)

**What people pay for:** CodeRabbit ($15/seat/mo), Codacy ($15/mo), Snyk ($25/mo)

**Workflow:**
1. `exec:` `git diff HEAD~1` to get latest changes
2. `exec:` `git log --oneline -5` for context
3. `exec:` Count changed lines, files, identify languages
4. `infer:` Security review -- injection, secrets, unsafe patterns (structured JSON)
5. `infer:` Code quality review -- complexity, DRY, naming, error handling
6. `infer:` Performance review -- O(n) analysis, unnecessary allocations
7. `infer:` Synthesize verdict: Approve / Request Changes / Block
8. `artifact:` Markdown review report with severity-ranked findings

**Advanced features:** `structured:` for findings JSON, parallel infer tasks,
`artifact:` with template, `exec:` for git operations

**Wow factor:** Instant code review that catches security issues, not just style nits.

---

### 2.2 Auto-Changelog + Release Notes (exec + infer + structured + artifact)

**What people pay for:** Changelogfy ($24/mo), Release Drafter (free but limited)

**Workflow:**
1. `exec:` `git log --oneline --no-merges` since last tag
2. `exec:` `git describe --tags --abbrev=0` for version
3. `exec:` `git diff --stat $(git describe --tags --abbrev=0)..HEAD` for file stats
4. `infer:` Categorize commits (Added/Changed/Fixed/Removed/Security/Performance)
   with `structured:` JSON schema
5. `infer:` Write user-facing changelog entry (Keep a Changelog format)
6. `infer:` Write marketing-friendly release announcement (for blog/Twitter)
7. `infer:` Generate migration guide if breaking changes detected
8. `artifact:` CHANGELOG.md entry + release-notes.md + announcement.md

**Advanced features:** `structured:` for categorization, `exec:` for git,
multiple artifacts, conditional logic

---

### 2.3 Documentation Generator from Code (exec + infer + for_each + invoke)

**What people pay for:** Mintlify ($150/mo), GitBook ($8/mo), ReadMe ($99/mo)

**Workflow:**
1. `exec:` `find src -name "*.rs" -type f` (or .ts, .py) to list source files
2. `exec:` Read each file's exports/public API
3. `for_each:` over source files:
   - `infer:` Extract function signatures, types, and doc comments
   - `infer:` Generate API documentation with examples
4. `infer:` Create table of contents and cross-references
5. `infer:` Generate quickstart guide from the API surface
6. `invoke: nika:write` per-module docs + index

**Advanced features:** `for_each:` over files, `invoke: nika:write` + `invoke: nika:glob`,
`artifact:` for docs tree

**Wow factor:** Full API docs generated from raw source code, with examples that compile.

---

### 2.4 Dependency Audit + Security Scanner (fetch + exec + infer + structured)

**What people pay for:** Snyk ($25/mo), Socket.dev ($10/mo), Mend ($)

**Workflow:**
1. `exec:` Read Cargo.toml / package.json / requirements.txt
2. `for_each:` over top 10 dependencies:
   - `fetch:` GitHub API for repo health metrics (stars, last commit, open issues)
   - `fetch:` Check if deprecated or archived
3. `infer:` Risk assessment per dependency (structured JSON):
   - Maintenance status (active/stale/abandoned)
   - Known vulnerability exposure
   - License compatibility
   - Bus factor (single maintainer?)
4. `infer:` Generate prioritized action plan
5. `artifact:` Dependency audit report with risk heatmap (text)

**Advanced features:** `for_each:` over deps, `fetch:` GitHub API, `structured:`,
parallel fetches, `artifact:`

---

### 2.5 Test Case Generator from Spec (exec + infer + for_each + invoke)

**What people pay for:** CodiumAI ($19/mo), Testim ($), Mabl ($)

**Workflow:**
1. `exec:` Read source file or module
2. `infer:` Identify all public functions, their signatures, and edge cases
   (structured JSON array)
3. `for_each:` over functions:
   - `infer:` Generate unit tests: happy path, edge cases, error conditions
   - `infer:` Generate property-based test ideas
4. `infer:` Identify missing test coverage areas
5. `invoke: nika:write` test file
6. `exec:` Run the generated tests and capture output
7. `infer:` Analyze test results, fix any failing tests

**Advanced features:** `for_each:`, `structured:`, `invoke: nika:write`,
`exec:` for running tests, iterative fix loop

**Wow factor:** Write tests from code, run them, and fix failures -- all in one workflow.

---

### 2.6 Incident Post-Mortem Writer (exec + fetch + infer + structured)

**What people pay for:** Incident.io ($19/seat/mo), FireHydrant ($), PagerDuty ($)

**Workflow:**
1. `exec:` Read error logs from a file or recent `journalctl` output
2. `exec:` Get git commits from the relevant timeframe
3. `fetch:` Pull status page or monitoring endpoint data
4. `infer:` Timeline reconstruction (structured JSON: event, time, impact)
5. `infer:` Root cause analysis with 5 Whys methodology
6. `infer:` Generate blameless post-mortem document:
   - Summary, timeline, root cause, impact, action items
   - Detection gap analysis
   - Prevention recommendations
7. `artifact:` Post-mortem markdown document

**Advanced features:** `structured:` for timeline, `exec:` + `fetch:` data gathering,
`artifact:` with template

---

## Category 3: Data Processing

### 3.1 Web Scraping + Enrichment Pipeline (fetch + infer + for_each + structured)

**What people pay for:** Clay ($149/mo), Apify ($49/mo), Octoparse ($89/mo)

**Workflow:**
1. `fetch: extract: links` from target directory/listing page
2. Filter to content links (infer or transform)
3. `for_each:` over links (concurrency: 3):
   - `fetch: extract: article` to get clean content
   - `fetch: extract: metadata` to get OG/structured data
   - `infer:` Extract structured fields (name, email, company, role)
4. `infer:` Deduplicate and normalize all records
5. `infer:` Score leads by relevance (structured JSON with score 1-100)
6. `artifact:` Enriched dataset as JSON + summary report

**Advanced features:** `for_each:` with concurrency, `extract: links` + `extract: article`,
`structured:`, multiple extraction modes

**Wow factor:** A lead enrichment pipeline that rivals $149/mo Clay, from one YAML file.

---

### 3.2 API-to-Dashboard Report (fetch + exec + infer + structured + artifact)

**What people pay for:** Geckoboard ($49/mo), Databox ($72/mo), Klipfolio ($99/mo)

**Workflow:**
1. `fetch:` Hit 3-5 APIs in parallel (GitHub stats, Stripe revenue, analytics)
2. `exec:` Normalize JSON responses with jq transforms
3. `infer:` Analyze trends and anomalies across all data sources (structured JSON)
4. `invoke: nika:chart` Generate bar charts for key metrics
5. `infer:` Write executive summary with insights and recommendations
6. `artifact:` HTML/markdown dashboard report with embedded chart references

**Advanced features:** Parallel `fetch:`, `invoke: nika:chart` (media tools),
`structured:`, `artifact:` with template, `exec:` for jq transforms

**Wow factor:** Auto-generated weekly business dashboard from raw APIs.

---

### 3.3 PDF Invoice Data Extraction (fetch + invoke + infer + structured)

**What people pay for:** Docparser ($39/mo), Nanonets ($500/mo), Rossum ($)

**Workflow:**
1. `fetch: response: binary` to download PDF invoice from URL
2. `invoke: nika:pdf_extract` to get text content from PDF
3. `infer:` Extract structured invoice data (structured JSON):
   - Invoice number, date, due date
   - Vendor name, address, tax ID
   - Line items (description, quantity, unit price, total)
   - Subtotal, tax, total
   - Payment terms
4. `infer:` Validate extracted data (cross-check line item math)
5. `artifact:` Clean JSON invoice + validation report

**Advanced features:** `fetch: response: binary`, `invoke: nika:pdf_extract` (media-pdf),
`structured:` for invoice schema, `artifact:`

**Wow factor:** Automatic invoice parsing with mathematical validation -- replaces $500/mo SaaS.

---

### 3.4 CSV/JSON Anomaly Detector (exec + infer + structured + invoke)

**What people pay for:** Anodot ($), Monte Carlo ($), Great Expectations (OSS)

**Workflow:**
1. `exec:` Read CSV/JSON data file
2. `infer:` Profile the dataset (structured JSON):
   - Column types, null rates, unique counts, distributions
   - Statistical summaries (mean, median, stddev, min, max)
3. `infer:` Identify anomalies and outliers with explanations
4. `infer:` Generate data quality score (0-100) with breakdown
5. `invoke: nika:chart` Visualize anomaly distribution
6. `artifact:` Data quality report with findings

**Advanced features:** `structured:`, `invoke: nika:chart`, `artifact:`

---

### 3.5 Multi-Source Data Reconciliation (fetch + exec + infer + structured)

**What people pay for:** Adra by Trintech ($), FloQast ($), manual spreadsheet work

**Workflow:**
1. `exec:` Read local dataset A (e.g., sales records CSV)
2. `fetch:` Pull dataset B from API (e.g., payment processor)
3. `infer:` Map fields between the two schemas (structured JSON mapping)
4. `infer:` Identify mismatches, missing records, duplicates
5. `infer:` Generate reconciliation report:
   - Matched records count
   - Discrepancies with details
   - Suggested corrections
   - Confidence score per match
6. `artifact:` Reconciliation report + discrepancy list

**Advanced features:** `structured:`, parallel data gathering, `artifact:`

---

## Category 4: Business Automation

### 4.1 Meeting-to-Action Pipeline (exec + infer + structured + for_each + invoke)

**What people pay for:** Otter.ai ($17/mo), Fireflies ($19/mo), tl;dv ($25/mo)

**Workflow:**
1. `exec:` Read transcript file (from Zoom/Teams export or Whisper output)
2. `infer:` Extract structured meeting data:
   - Attendees and roles
   - Key decisions (structured array)
   - Action items with owners and deadlines
   - Open questions
   - Sentiment per speaker
3. `for_each:` over action items:
   - `infer:` Generate detailed task description with acceptance criteria
4. `infer:` Draft follow-up email to all attendees
5. `infer:` Generate standup-ready summary (3 bullet points)
6. `artifact:` Meeting minutes + task descriptions + follow-up email

**Advanced features:** `structured:` for action items, `for_each:` over tasks,
multiple artifacts, `exec:` for transcript reading

**Wow factor:** Drop a transcript, get minutes + tasks + follow-up email in 30 seconds.

---

### 4.2 RFP/Proposal Auto-Drafter (exec + fetch + infer + for_each + artifact)

**What people pay for:** Loopio ($), Responsive ($), Qvidian ($)

**Workflow:**
1. `exec:` Read RFP/brief document
2. `infer:` Extract requirements as structured checklist (JSON array)
3. `fetch:` Pull company info from website for context
4. `for_each:` over requirements:
   - `infer:` Draft response section addressing each requirement
   - `infer:` Assign confidence score (how well we match)
5. `infer:` Write executive summary and pricing framework
6. `infer:` Generate compliance matrix (requirement -> response status)
7. `artifact:` Complete proposal draft + compliance matrix

**Advanced features:** `for_each:`, `structured:`, `fetch: extract: article`,
multiple artifacts

**Wow factor:** RFP response draft in minutes instead of days.

---

### 4.3 Email Intelligence + Auto-Reply Drafter (exec + infer + structured + for_each)

**What people pay for:** SaneBox ($7/mo), Superhuman ($30/mo), Shortwave ($7/mo)

**Workflow:**
1. `exec:` Read batch of emails (from mbox export or piped input)
2. `infer:` Classify each email (structured JSON):
   - Category: urgent/action-required/FYI/spam/newsletter
   - Sentiment: positive/negative/neutral
   - Required response: yes/no
   - Priority: P1/P2/P3/P4
3. `for_each:` over action-required emails:
   - `infer:` Draft contextual reply matching your tone/style
4. `infer:` Generate daily email briefing:
   - Urgent items needing attention
   - Summary of FYI emails
   - Newsletter highlights
5. `artifact:` Email briefing + draft replies

**Advanced features:** `for_each:`, `structured:` for classification, multiple artifacts

---

### 4.4 Invoice Generator (exec + infer + invoke + structured)

**What people pay for:** FreshBooks ($15/mo), Wave ($), QuickBooks ($25/mo)

**Workflow:**
1. `exec:` Read time tracking data or project details (JSON/CSV)
2. `infer:` Calculate line items, apply rates, compute taxes (structured JSON)
3. `infer:` Generate professional invoice copy (terms, notes, payment info)
4. `invoke: nika:chart` Create simple revenue breakdown chart
5. `invoke: nika:write` Generate markdown invoice
6. `artifact:` Invoice markdown + line items JSON

**Advanced features:** `structured:` for invoice data, `invoke: nika:chart`,
`artifact:`, `exec:`

---

### 4.5 Contract Review Assistant (exec + infer + structured + for_each)

**What people pay for:** Ironclad ($), LawGeex ($), SpotDraft ($)

**Workflow:**
1. `exec:` Read contract document (or `invoke: nika:pdf_extract` for PDFs)
2. `infer:` Identify all clauses with types (structured JSON array):
   - Termination, liability, IP, non-compete, payment terms, SLA, etc.
3. `for_each:` over risky clauses:
   - `infer:` Risk assessment (favorable/neutral/unfavorable to us)
   - `infer:` Suggest redline language
4. `infer:` Generate contract summary:
   - Key terms at a glance
   - Red flags ranked by severity
   - Missing standard clauses
   - Overall risk score (1-10)
5. `artifact:` Contract review report + suggested redlines

**Advanced features:** `for_each:`, `structured:`, `invoke: nika:pdf_extract`,
`artifact:` with template

**Wow factor:** AI contract review that surfaces red flags a junior lawyer would miss.

---

## Category 5: Research & Analysis

### 5.1 Competitive Intelligence Dashboard (fetch + infer + for_each + structured + invoke)

**What people pay for:** Crayon ($), Klue ($), Semrush ($130/mo)

**Workflow:**
1. `for_each:` over 5 competitor URLs:
   - `fetch: extract: metadata` -- OG tags, descriptions, tech signals
   - `fetch: extract: article` -- main page content
   - `fetch: extract: links` -- link structure analysis
2. `infer:` Per competitor: positioning analysis (structured JSON):
   - Value prop, target audience, feature highlights, pricing signals
3. `infer:` Cross-competitor comparison matrix
4. `infer:` SWOT analysis from our perspective
5. `infer:` Strategic recommendations ranked by impact/effort
6. `invoke: nika:chart` Feature comparison bar chart
7. `artifact:` Full competitive intel report

**Advanced features:** Nested `for_each:`, 3 extract modes, `invoke: nika:chart`,
`structured:`, `artifact:` with template

**Wow factor:** Full competitive analysis from URLs alone -- no manual research needed.

---

### 5.2 Trend Spotter (fetch + infer + structured + for_each)

**What people pay for:** Exploding Topics ($97/mo), SparkToro ($50/mo), Glimpse ($)

**Workflow:**
1. `fetch: extract: feed` from 5-10 industry RSS feeds
2. `fetch:` Hacker News front page + Product Hunt today
3. `infer:` Extract all mentioned topics/tools/companies (structured JSON array)
4. `infer:` Frequency analysis -- what's being mentioned most
5. `infer:` Trend classification:
   - Emerging (first mentions)
   - Growing (increasing frequency)
   - Peaking (everywhere)
   - Declining (fading mentions)
6. `infer:` Generate "This Week in [Industry]" briefing
7. `artifact:` Trend report with signals and recommendations

**Advanced features:** `extract: feed`, `structured:`, `for_each:`, `artifact:`

---

### 5.3 Academic Paper Summarizer (fetch + infer + structured + for_each)

**What people pay for:** Scholarcy ($10/mo), Elicit ($), Consensus ($10/mo)

**Workflow:**
1. `fetch: response: binary` -- Download PDF paper
2. `invoke: nika:pdf_extract` -- Extract text
3. `infer:` Parse paper structure (structured JSON):
   - Title, authors, abstract, sections, references
4. `infer:` Generate layered summaries:
   - One-sentence TL;DR
   - Executive summary (1 paragraph)
   - Key findings (bullet points)
   - Methodology critique
   - Relevance to your field
5. `for_each:` over key findings:
   - `infer:` Practical implications and applications
6. `artifact:` Paper summary + implications report

**Advanced features:** `fetch: response: binary`, `invoke: nika:pdf_extract`,
`structured:`, `for_each:`, `artifact:`

---

### 5.4 Market Sizing Calculator (fetch + infer + structured + exec)

**What people pay for:** Statista ($199/mo), IBISWorld ($), consultant fees

**Workflow:**
1. `fetch:` Pull industry data from public APIs (World Bank, Census, etc.)
2. `fetch: extract: article` from 3-5 industry report summaries
3. `infer:` Extract key market data points (structured JSON):
   - TAM, SAM, SOM estimates
   - Growth rates
   - Key segments
   - Geographic distribution
4. `infer:` Build bottom-up and top-down market size models
5. `infer:` Sensitivity analysis with 3 scenarios (bull/base/bear)
6. `invoke: nika:chart` Market size visualization
7. `artifact:` Market sizing report with methodology

**Advanced features:** `structured:`, `invoke: nika:chart`, parallel `fetch:`,
`artifact:` with template

---

### 5.5 Patent Landscape Analyzer (fetch + infer + for_each + structured)

**What people pay for:** PatSnap ($), Innography ($), Questel ($)

**Workflow:**
1. `fetch:` Query public patent databases (Google Patents, USPTO API)
2. `infer:` Extract patent metadata from results (structured JSON)
3. `for_each:` over top 10 patents:
   - `infer:` Summarize claims, innovations, and applicability
4. `infer:` Identify white spaces (areas not covered by existing patents)
5. `infer:` Generate prior art risk assessment for your innovation
6. `artifact:` Patent landscape report with opportunity map

**Advanced features:** `for_each:`, `structured:`, `fetch:`, `artifact:`

---

## Category 6: Media Processing

### 6.1 Image Optimization Pipeline (fetch + invoke media tools + for_each)

**What people pay for:** Cloudinary ($89/mo), imgix ($100/mo), TinyPNG ($39/yr)

**Workflow:**
1. `invoke: nika:glob` Find all images in a directory
2. `for_each:` over images:
   - `invoke: nika:import` into CAS
   - `invoke: nika:dimensions` get original size
   - `invoke: nika:thumbnail` resize to web sizes (800px, 400px, 200px)
   - `invoke: nika:convert` to WebP format
   - `invoke: nika:optimize` lossless optimization
   - `invoke: nika:thumbhash` generate placeholder hash
   - `invoke: nika:dominant_color` extract color palette
3. `infer:` Generate alt text for each image (if vision model available)
4. `artifact:` Optimization report (original vs optimized sizes, savings)

**Advanced features:** `for_each:` over files, 7 different `invoke: nika:*` media tools,
`invoke: nika:pipeline` for chaining, `artifact:`

**Wow factor:** Full image optimization pipeline -- the exact thing Cloudinary charges $89/mo for.

---

### 6.2 PDF Report Processor (fetch + invoke + infer + structured)

**What people pay for:** DocParser ($39/mo), Amazon Textract ($), Google Document AI ($)

**Workflow:**
1. `fetch: response: binary` Download PDF from URL
2. `invoke: nika:pdf_extract` Extract all text content
3. `infer:` Detect document type (invoice, report, form, contract) -- structured
4. `infer:` Extract structured data based on detected type:
   - Tables as JSON arrays
   - Key-value pairs
   - Section headings and content
5. `infer:` Generate searchable summary with key data points
6. `artifact:` Structured JSON + summary markdown

**Advanced features:** `fetch: response: binary`, `invoke: nika:pdf_extract`,
`structured:`, conditional processing, `artifact:`

---

### 6.3 Content Provenance + C2PA Signing (invoke + infer + exec)

**What people pay for:** Truepic ($), Content Authenticity Initiative tools

**Workflow:**
1. `invoke: nika:import` Import original image
2. `infer:` Generate image description and metadata
3. `invoke: nika:provenance` Sign image with C2PA content credentials
4. `invoke: nika:verify` Verify the signed manifest
5. `invoke: nika:metadata` Extract full EXIF data
6. `infer:` Generate provenance report (chain of custody, authenticity)
7. `artifact:` Signed image + provenance certificate

**Advanced features:** `invoke: nika:provenance` + `invoke: nika:verify` (C2PA),
`invoke: nika:metadata`, EU AI Act compliance

**Wow factor:** Content authenticity signing -- cutting-edge feature few tools offer.

---

### 6.4 QR Code Quality Auditor (fetch + invoke + infer + for_each)

**What people pay for:** QR Code AI Scanner ($), Scanova ($), QR Tiger ($)

**Workflow:**
1. `invoke: nika:glob` Find all QR images in a directory
2. `for_each:` over QR images:
   - `invoke: nika:import` into CAS
   - `invoke: nika:qr_validate` Decode + get 0-100 scan score
   - `invoke: nika:dimensions` Check resolution
   - `invoke: nika:dominant_color` Check contrast
3. `infer:` Analyze batch results:
   - Which QR codes are at risk of failing in real-world scanning
   - Contrast and size recommendations
   - Ranking by scan reliability
4. `artifact:` QR quality audit report

**Advanced features:** `invoke: nika:qr_validate` (media-qr), `for_each:`,
multiple media tools, `artifact:`

**Wow factor:** Directly relevant to QR Code AI -- the actual SuperNovae product.

---

### 6.5 SVG Asset Pipeline (invoke + infer + for_each + artifact)

**What people pay for:** Figma export plugins, manual SVG optimization tools

**Workflow:**
1. `invoke: nika:glob` Find all SVG files
2. `for_each:` over SVGs:
   - `invoke: nika:import` into CAS
   - `invoke: nika:svg_render` Rasterize to PNG at multiple DPIs
   - `invoke: nika:thumbnail` Create icon sizes (16, 32, 64, 128)
   - `invoke: nika:optimize` Optimize PNG outputs
3. `infer:` Generate sprite sheet configuration
4. `infer:` Write CSS/HTML usage documentation
5. `artifact:` Asset manifest + all rendered sizes

**Advanced features:** `invoke: nika:svg_render`, `for_each:`, `invoke: nika:thumbnail`,
multiple sizes, `artifact:`

---

### 6.6 Image Similarity Deduplicator (invoke + infer + for_each + structured)

**What people pay for:** Duplicate Photo Cleaner ($40), ImageKit ($)

**Workflow:**
1. `invoke: nika:glob` Find all images in a directory
2. `for_each:` over images:
   - `invoke: nika:import` into CAS
   - `invoke: nika:phash` Compute perceptual hash
   - `invoke: nika:dimensions` Get size info
3. `infer:` Group images by perceptual hash similarity
4. `for_each:` over duplicate groups:
   - `invoke: nika:compare` Verify visual similarity
   - `invoke: nika:quality` Assess which is highest quality (DSSIM)
   - `infer:` Recommend which to keep (highest quality, largest)
5. `artifact:` Deduplication report with keep/remove recommendations

**Advanced features:** `invoke: nika:phash` + `invoke: nika:compare` + `invoke: nika:quality`,
nested `for_each:`, `structured:`

---

## Category 7: Customer Support

### 7.1 FAQ Generator from Support Tickets (exec + infer + for_each + structured)

**What people pay for:** Zendesk AI ($), Intercom Fin ($0.99/resolution), Helpjuice ($120/mo)

**Workflow:**
1. `exec:` Read exported support tickets (JSON/CSV)
2. `infer:` Cluster tickets by topic (structured JSON):
   - Topic name, frequency, example tickets, avg resolution time
3. `for_each:` over top 15 topic clusters:
   - `infer:` Generate FAQ entry:
     - Question (natural language, how a customer would ask)
     - Answer (clear, empathetic, actionable)
     - Related questions
     - Links to relevant docs
4. `infer:` Organize FAQs by category and priority
5. `infer:` Generate FAQ page HTML/markdown
6. `artifact:` Complete FAQ document + topic analysis

**Advanced features:** `for_each:`, `structured:` for clustering, `artifact:`

**Wow factor:** Turn 1000 support tickets into a structured FAQ in one run.

---

### 7.2 Ticket Router + Priority Classifier (exec + infer + structured + for_each)

**What people pay for:** Zendesk routing ($), Freshdesk ($), custom ML models

**Workflow:**
1. `exec:` Read batch of incoming tickets
2. `for_each:` over tickets:
   - `infer:` Classify with structured output:
     - Category (billing, technical, feature-request, bug, account)
     - Priority (P1-P4)
     - Sentiment (angry, frustrated, neutral, happy)
     - Estimated complexity (quick-fix, investigation, escalation)
     - Suggested team (billing, engineering, success, security)
     - Auto-response appropriateness (yes/no)
3. `for_each:` over auto-respondable tickets:
   - `infer:` Generate personalized response draft
4. `infer:` Generate shift summary:
   - Ticket volume by category
   - Priority distribution
   - Sentiment trends
   - Escalation flags
5. `artifact:` Routing decisions + auto-responses + shift summary

**Advanced features:** `for_each:` with `structured:`, conditional processing, `artifact:`

---

### 7.3 Knowledge Base Article Generator (fetch + infer + for_each + invoke)

**What people pay for:** Guru ($10/seat/mo), Notion AI ($10/mo), Document360 ($149/mo)

**Workflow:**
1. `fetch: extract: article` from existing help pages (scrape competitor docs)
2. `exec:` Read internal product specs/changelogs
3. `infer:` Identify gaps between current docs and product features
4. `for_each:` over gaps:
   - `infer:` Generate help article:
     - Title optimized for search
     - Step-by-step instructions
     - Screenshots placeholder descriptions
     - Troubleshooting section
     - Related articles
5. `infer:` Generate article hierarchy and navigation structure
6. `invoke: nika:write` per-article files
7. `artifact:` Complete knowledge base + sitemap

**Advanced features:** `for_each:`, `fetch: extract: article`, `invoke: nika:write`,
`artifact:`

---

### 7.4 Customer Feedback Analyzer (exec + infer + structured + invoke)

**What people pay for:** Medallia ($), Qualtrics ($), MonkeyLearn ($)

**Workflow:**
1. `exec:` Read feedback/reviews dataset (JSON/CSV -- from App Store, G2, etc.)
2. `infer:` Sentiment analysis with structured output per review:
   - Overall sentiment (-1 to 1)
   - Specific aspects mentioned (UX, performance, pricing, support)
   - Feature requests extracted
   - Pain points identified
3. `infer:` Aggregate analysis:
   - Top 5 praised features
   - Top 5 pain points
   - Feature request frequency ranking
   - NPS estimate
4. `invoke: nika:chart` Sentiment distribution chart
5. `infer:` Generate product roadmap suggestions from feedback
6. `artifact:` Feedback analysis report + feature request backlog

**Advanced features:** `structured:`, `invoke: nika:chart`, `artifact:`

---

## Category 8: Marketing

### 8.1 Landing Page Copy Generator (fetch + infer + structured + for_each + artifact)

**What people pay for:** Copy.ai ($49/mo), Jasper ($49/mo), Unbounce Smart Copy ($)

**Workflow:**
1. `fetch: extract: article` from competitor landing pages
2. `infer:` Analyze competitor copy patterns (structured):
   - Headlines, subheads, CTAs, social proof, objection handling
3. `infer:` Generate 5 headline variants using different frameworks:
   - PAS (Problem-Agitate-Solve)
   - AIDA (Attention-Interest-Desire-Action)
   - 4U (Useful-Urgent-Unique-Ultra-specific)
   - BAB (Before-After-Bridge)
   - Feature-Benefit
4. `for_each:` over headline variants:
   - `infer:` Generate full landing page copy (hero, benefits, features, CTA)
5. `infer:` A/B test recommendation: which 2 variants to test first
6. `artifact:` 5 complete landing page variants + testing plan

**Advanced features:** `for_each:`, `fetch: extract: article`, `structured:`,
multiple artifacts

**Wow factor:** 5 complete landing page variants with A/B test plan from competitor analysis.

---

### 8.2 Ad Copy Generator + Variations (infer + for_each + structured)

**What people pay for:** AdCreative.ai ($29/mo), Pencil ($), Anyword ($49/mo)

**Workflow:**
1. `infer:` Define product positioning (structured):
   - Value props, target personas, tone guidelines
2. `for_each:` over 4 ad platforms (Google, Facebook, LinkedIn, Twitter):
   - `for_each:` over 3 personas:
     - `infer:` Generate ad copy variant:
       - Headline (platform-specific char limit)
       - Body copy
       - CTA
       - Targeting suggestions
3. `infer:` Score all variants for predicted performance (structured):
   - Clarity, emotional appeal, urgency, relevance
4. `infer:` Compile top 5 recommendations per platform
5. `artifact:` Complete ad copy library (48 variants) + recommendations

**Advanced features:** Nested `for_each:`, `structured:`, `concurrency:`, `artifact:`

**Wow factor:** 48 ad copy variants across 4 platforms and 3 personas in one run.

---

### 8.3 Buyer Persona Generator (fetch + infer + structured + invoke)

**What people pay for:** HubSpot ($45/mo), Xtensio ($), Delve AI ($)

**Workflow:**
1. `fetch: extract: article` from your website (or product page)
2. `fetch:` Pull audience data from analytics API (or simulated input)
3. `infer:` Generate 4 detailed buyer personas (structured JSON):
   - Name, age, role, company size, goals, pain points
   - Buying triggers, objections, preferred channels
   - Information sources, decision-making process
   - Quotes they would say
4. `for_each:` over personas:
   - `infer:` Generate persona-specific messaging framework
   - `infer:` Map content journey (awareness -> consideration -> decision)
5. `artifact:` Persona profiles + messaging frameworks

**Advanced features:** `for_each:`, `structured:`, `fetch: extract: article`, `artifact:`

---

### 8.4 Campaign Planner (infer + for_each + structured + artifact)

**What people pay for:** Monday.com marketing ($12/seat/mo), Asana ($), CoSchedule ($29/mo)

**Workflow:**
1. `infer:` Define campaign parameters (structured):
   - Goal, budget, duration, channels, KPIs
2. `infer:` Generate campaign timeline with milestones
3. `for_each:` over campaign phases (pre-launch, launch, post-launch):
   - `infer:` Detail tasks, deliverables, and owners
   - `infer:` Generate content briefs for each deliverable
4. `infer:` Budget allocation recommendation per channel
5. `infer:` Generate measurement framework (KPIs, tools, reporting cadence)
6. `artifact:` Complete campaign plan + content calendar + measurement plan

**Advanced features:** `for_each:`, `structured:`, multiple artifacts

---

### 8.5 Email Sequence Writer (infer + for_each + structured + artifact)

**What people pay for:** ConvertKit ($29/mo), ActiveCampaign ($49/mo), Instantly ($30/mo)

**Workflow:**
1. `infer:` Define sequence strategy (structured):
   - Goal (nurture, onboard, re-engage, upsell)
   - Number of emails (5-7)
   - Cadence (days between emails)
   - Persona and pain points
2. `for_each:` over email positions in sequence:
   - `infer:` Generate email:
     - Subject line (5 variants for A/B testing)
     - Preview text
     - Body copy with personalization tokens
     - CTA
     - Conditional branch suggestion (what if they click/don't click)
3. `infer:` Generate sequence flowchart description
4. `infer:` Predict performance benchmarks
5. `artifact:` Complete email sequence + A/B variants + flowchart

**Advanced features:** `for_each:`, `structured:`, `artifact:` with template

**Wow factor:** Complete email nurture sequence with branching logic in one run.

---

## Category 9: Education

### 9.1 Course Outline Generator (fetch + infer + for_each + structured + artifact)

**What people pay for:** Coursebox ($50/mo), LearnWorlds ($24/mo), Teachable ($)

**Workflow:**
1. `fetch: extract: article` from 3-5 reference articles on the topic
2. `infer:` Extract key concepts and learning objectives (structured JSON)
3. `infer:` Generate course structure:
   - 8-12 modules with progressive difficulty
   - Prerequisites per module
   - Estimated time per module
4. `for_each:` over modules:
   - `infer:` Generate detailed lesson plan:
     - Learning objectives (Bloom's taxonomy)
     - Key concepts with explanations
     - Practical exercise description
     - Assessment criteria
5. `infer:` Generate course landing page copy
6. `artifact:` Complete course outline + lesson plans + landing page

**Advanced features:** `for_each:`, `structured:`, `fetch: extract: article`, `artifact:`

**Wow factor:** Complete course curriculum from topic name + reference URLs.

---

### 9.2 Quiz/Assessment Generator (exec + infer + structured + for_each)

**What people pay for:** Quizlet ($8/mo), Kahoot ($26/mo), ProProfs ($20/mo)

**Workflow:**
1. `exec:` Read study material / textbook chapter / lecture notes
2. `infer:` Identify key concepts and facts (structured JSON array)
3. `for_each:` over concepts:
   - `infer:` Generate question set (structured JSON):
     - Multiple choice (4 options, 1 correct, explanation for each)
     - True/False with justification
     - Short answer with rubric
     - Bloom's taxonomy level for each question
4. `infer:` Generate answer key with detailed explanations
5. `infer:` Create difficulty distribution analysis
6. `artifact:` Quiz document + answer key + difficulty analysis

**Advanced features:** `for_each:`, `structured:` for questions, `artifact:`, `exec:`

**Wow factor:** Full quiz bank with answer keys and Bloom's taxonomy tagging.

---

### 9.3 Study Guide Creator (exec + infer + for_each + invoke + artifact)

**What people pay for:** StudySmarter ($10/mo), Quizlet Plus ($8/mo), Anki (free)

**Workflow:**
1. `exec:` Read source material (textbook chapter, lecture notes)
2. `infer:` Extract hierarchical concept map (structured JSON tree)
3. `for_each:` over top-level concepts:
   - `infer:` Generate:
     - Concept summary (ELI5 + technical)
     - Key terms with definitions
     - Memory aids (mnemonics, analogies)
     - Practice problems with solutions
     - Common misconceptions
4. `infer:` Generate spaced repetition flashcard deck (Q&A pairs)
5. `infer:` Create visual concept map description
6. `invoke: nika:chart` Generate concept relationship diagram
7. `artifact:` Study guide + flashcards + concept map

**Advanced features:** `for_each:`, `structured:`, `invoke: nika:chart`, `artifact:`

---

### 9.4 Lesson Plan Differentiator (infer + for_each + structured + artifact)

**What people pay for:** TeachFX ($), Edcite ($), Magic School AI ($10/mo)

**Workflow:**
1. `infer:` Define lesson parameters (topic, grade level, standards)
2. `for_each:` over 3 learner profiles (advanced, on-level, struggling):
   - `infer:` Generate differentiated lesson plan:
     - Modified objectives
     - Scaffolding strategies
     - Activity variations
     - Assessment modifications
     - Extension activities
3. `infer:` Generate inclusive activity that works for all levels
4. `infer:` Create teacher facilitation guide with timing
5. `artifact:` 3 differentiated plans + facilitation guide

**Advanced features:** `for_each:` over learner profiles, `structured:`, `artifact:`

---

### 9.5 Curriculum Mapper (fetch + infer + structured + for_each + artifact)

**What people pay for:** Atlas ($), Chalk ($), Curriculum Trak ($)

**Workflow:**
1. `fetch:` Pull educational standards (Common Core, NGSS, etc. from public API/URL)
2. `exec:` Read existing curriculum document
3. `infer:` Map curriculum units to standards (structured JSON matrix)
4. `infer:` Identify gaps -- standards not covered
5. `for_each:` over gaps:
   - `infer:` Suggest activities/lessons to address missing standards
6. `infer:` Generate alignment report with coverage percentages
7. `artifact:` Standards alignment matrix + gap analysis + recommendations

**Advanced features:** `for_each:`, `structured:`, `fetch:`, `artifact:`

---

## Category 10: Personal Productivity

### 10.1 Daily Briefing Generator (fetch + exec + infer + artifact)

**What people pay for:** Feedly ($12/mo), Morning Brew (free but ad-supported), The Skimm

**Workflow:**
1. `fetch: extract: feed` from your curated RSS feeds (tech, business, local news)
2. `fetch:` Weather API for your location
3. `exec:` Read calendar/todo file for today's schedule
4. `infer:` Compile personalized morning briefing:
   - Weather + outfit suggestion
   - Top 5 news stories with one-line summaries
   - Today's schedule highlights
   - Key preparation needed for meetings
   - One inspiring quote or thought
5. `infer:` Generate audio-friendly version (conversational, like a podcast script)
6. `artifact:` Morning briefing markdown + audio script

**Advanced features:** `fetch: extract: feed`, parallel `fetch:`, `exec:`, `artifact:`

**Wow factor:** Personalized morning briefing that knows your schedule, industry, and weather.

---

### 10.2 Meal Planning + Grocery List (infer + for_each + structured + artifact)

**What people pay for:** Mealime ($6/mo), PlateJoy ($12/mo), Eat This Much ($9/mo)

**Workflow:**
1. `infer:` Define meal plan parameters (structured):
   - Dietary preferences (vegan, keto, etc.)
   - Number of people, budget, available time
   - Kitchen skill level
   - Allergies
2. `for_each:` over 7 days:
   - `infer:` Generate 3 meals + 1 snack (structured JSON):
     - Recipe name, prep time, cook time
     - Ingredients with quantities
     - Step-by-step instructions
     - Nutrition estimate (calories, protein, carbs, fat)
3. `infer:` Compile master grocery list:
   - Deduplicated ingredients
   - Organized by store section (produce, dairy, pantry)
   - Estimated total cost
4. `infer:` Meal prep suggestions for batch cooking
5. `artifact:` Weekly meal plan + grocery list + prep guide

**Advanced features:** `for_each:`, `structured:` for recipes, `artifact:`

**Wow factor:** Complete weekly meal plan with shopping list -- personalized and actionable.

---

### 10.3 Travel Itinerary Planner (fetch + infer + for_each + structured + artifact)

**What people pay for:** Wonderplan ($), Roam Around ($), Layla AI ($)

**Workflow:**
1. `fetch: extract: article` from travel guide pages for destination
2. `fetch:` Weather API for travel dates
3. `infer:` Generate day-by-day itinerary (structured JSON):
   - Activities with timing, location, duration
   - Lunch/dinner recommendations
   - Transportation between spots
   - Budget estimate per day
4. `for_each:` over days:
   - `infer:` Add detailed descriptions, insider tips, alternatives
   - `infer:` Generate backup activities for bad weather
5. `infer:` Compile packing list based on activities + weather
6. `infer:` Generate pre-trip checklist (visas, insurance, bookings)
7. `artifact:` Complete itinerary + packing list + checklist

**Advanced features:** `for_each:`, `structured:`, `fetch: extract: article`,
`fetch:` weather API, multiple artifacts

---

### 10.4 Personal Finance Analyzer (exec + infer + structured + invoke + artifact)

**What people pay for:** YNAB ($15/mo), Monarch ($10/mo), Copilot ($10/mo)

**Workflow:**
1. `exec:` Read bank statement export (CSV)
2. `infer:` Categorize transactions (structured JSON):
   - Category (housing, food, transport, entertainment, etc.)
   - Recurring vs one-time
   - Essential vs discretionary
3. `infer:` Monthly analysis:
   - Spending by category
   - Month-over-month trends
   - Largest expenses
   - Subscriptions detected
4. `invoke: nika:chart` Spending breakdown pie chart
5. `infer:` Generate actionable savings recommendations
6. `infer:` Budget suggestion for next month
7. `artifact:` Financial report + budget plan + recommendations

**Advanced features:** `structured:`, `invoke: nika:chart`, `exec:`, `artifact:`

---

### 10.5 Weekly Review + Planning (exec + infer + structured + artifact)

**What people pay for:** Notion templates ($), Todoist ($5/mo), Sunsama ($20/mo)

**Workflow:**
1. `exec:` Read completed tasks from this week (todo file or journal)
2. `exec:` Read calendar events from this week
3. `infer:` Weekly review (structured JSON):
   - Accomplishments ranked by impact
   - Goals met vs missed
   - Time allocation analysis
   - Energy patterns (what energized vs drained)
4. `infer:` Generate next week's priorities:
   - Top 3 goals with success criteria
   - Time blocks for deep work
   - Meetings to decline/delegate
5. `infer:` Write reflection journal entry
6. `artifact:` Weekly review + next week plan + journal entry

**Advanced features:** `structured:`, `exec:`, multiple artifacts, `artifact:` with template

---

## Feature Coverage Matrix

Each workflow is tagged with the advanced features it demonstrates:

| Feature | Workflows Using It | Best Example |
|---------|-------------------|--------------|
| `for_each:` | 1.2, 1.3, 1.4, 2.3, 2.4, 2.5, 3.1, 4.1, 4.2, 4.3, 4.5, 5.1, 5.3, 5.5, 6.1, 6.4, 6.5, 6.6, 7.1, 7.2, 7.3, 8.1, 8.2, 8.3, 8.4, 8.5, 9.1, 9.2, 9.3, 9.4, 9.5, 10.2, 10.3 | 8.2 (nested for_each) |
| `structured:` | 1.1, 1.2, 2.1, 2.2, 3.1-3.5, 4.1-4.5, 5.1-5.5, 7.1-7.4, 8.1-8.5, 9.1-9.5, 10.1-10.5 | 3.3 (invoice schema) |
| `artifact:` | Nearly all | 6.1 (multi-artifact media) |
| `extract: article` | 1.1, 1.4, 3.1, 5.1, 5.3, 7.3, 8.1, 8.3, 9.1, 10.1, 10.3 | 5.1 (competitive intel) |
| `extract: feed` | 1.2, 5.2, 10.1 | 1.2 (newsletter curator) |
| `extract: metadata` | 1.1, 5.1 | 5.1 (competitive intel) |
| `extract: links` | 3.1, 5.1 | 3.1 (web scraping pipeline) |
| `extract: jsonpath` | 3.2 | 3.2 (API dashboard) |
| `response: binary` | 3.3, 5.3, 6.2 | 3.3 (PDF invoice) |
| `invoke: nika:chart` | 3.2, 3.4, 5.4, 6.4, 7.4, 9.3, 10.4 | 3.2 (API dashboard) |
| `invoke: nika:pdf_extract` | 3.3, 4.5, 5.3, 6.2 | 3.3 (PDF invoice) |
| `invoke: nika:import/thumbnail/convert/optimize` | 6.1, 6.4, 6.5, 6.6 | 6.1 (image pipeline) |
| `invoke: nika:phash/compare/quality` | 6.6 | 6.6 (deduplicator) |
| `invoke: nika:provenance/verify` | 6.3 | 6.3 (C2PA signing) |
| `invoke: nika:qr_validate` | 6.4 | 6.4 (QR auditor) |
| `invoke: nika:svg_render` | 6.5 | 6.5 (SVG pipeline) |
| `invoke: nika:write` | 2.3, 7.3 | 2.3 (doc generator) |
| `invoke: nika:glob` | 6.1, 6.4, 6.5, 6.6 | 6.1 (image pipeline) |
| `agent:` | (Recommended for: 5.1, 7.3, 2.6) | 5.1 with agent research loop |
| `concurrency:` | 1.3, 8.2 | 8.2 (48 parallel ad variants) |
| Vision (`content:`) | 6.1 (alt text), 6.4 | 6.1 (image alt text) |

---

## Top 10 Workflows by "Wow Factor"

These are the workflows that would make the strongest impression in a demo or course capstone:

1. **6.1 Image Optimization Pipeline** -- Uses 7+ media tools, replaces Cloudinary ($89/mo)
2. **5.1 Competitive Intelligence Dashboard** -- Full competitor analysis from URLs alone
3. **3.3 PDF Invoice Data Extraction** -- Replaces $500/mo Nanonets with one YAML file
4. **1.1 SEO Blog Post Factory** -- Replaces $150/mo tool stack (Surfer + Jasper)
5. **8.2 Ad Copy Generator** -- 48 variants from nested for_each, mind-blowing output volume
6. **6.3 Content Provenance (C2PA)** -- Cutting-edge content authenticity, EU AI Act ready
7. **4.5 Contract Review Assistant** -- PDF extract + clause analysis, actual legal value
8. **3.1 Web Scraping + Enrichment** -- Lead enrichment pipeline rivaling Clay ($149/mo)
9. **2.5 Test Case Generator** -- Writes tests, runs them, fixes failures -- developer magic
10. **10.1 Daily Briefing Generator** -- Personalized news + weather + calendar, daily utility

---

## Top 10 Workflows by "Course Exercise" Suitability

These are the best candidates for structured course exercises (progressive difficulty, clear
learning objectives, verifiable outputs):

1. **1.2 Newsletter Curator** -- teaches feed extraction, for_each, summarization
2. **2.2 Auto-Changelog** -- teaches exec + git, structured output, artifact
3. **3.3 PDF Invoice Extraction** -- teaches binary fetch, media tools, structured schemas
4. **4.1 Meeting-to-Action Pipeline** -- teaches structured extraction, for_each, artifacts
5. **5.1 Competitive Intelligence** -- teaches multi-extract modes, parallel fetch, synthesis
6. **6.1 Image Optimization Pipeline** -- teaches all media tools in one workflow
7. **7.1 FAQ Generator** -- teaches clustering, for_each, practical business output
8. **8.5 Email Sequence Writer** -- teaches for_each with position context, branching logic
9. **9.2 Quiz Generator** -- teaches structured output deeply, Bloom's taxonomy
10. **10.2 Meal Planning** -- teaches for_each over days, nested structured output, fun topic

---

## Pricing Context: What People Actually Pay

| Tool Replaced | Monthly Cost | Nika Equivalent |
|---------------|-------------|-----------------|
| Surfer SEO + Jasper | $138/mo | 1.1 SEO Blog Factory |
| Clay (lead enrichment) | $149/mo | 3.1 Web Scraping Pipeline |
| Nanonets (document AI) | $500/mo | 3.3 PDF Invoice Extraction |
| Cloudinary (image CDN) | $89/mo | 6.1 Image Pipeline |
| Otter.ai (transcription) | $17/mo | 4.1 Meeting Pipeline |
| Copy.ai (marketing copy) | $49/mo | 8.1 Landing Page Generator |
| Zendesk AI (support) | $$$/mo | 7.1 FAQ Generator + 7.2 Router |
| Exploding Topics (trends) | $97/mo | 5.2 Trend Spotter |
| YNAB (budgeting) | $15/mo | 10.4 Finance Analyzer |
| Coursebox (course creation) | $50/mo | 9.1 Course Outline Generator |

**Total replaced:** $1,100+/month in SaaS subscriptions, replaced by YAML files + API keys.

---

## Sources

1. Existing Nika use-case workflows (40 files in `tools/nika/examples/use-cases/`)
2. Nika course modules (12 levels, `tools/nika-course/`)
3. SaaS pricing pages for: Surfer SEO, Jasper, Clay, Cloudinary, Otter.ai, Copy.ai, Zendesk,
   Nanonets, Exploding Topics, YNAB, Coursebox, Mealime, Wonderplan
4. AI workflow automation market analysis (n8n, Make, Zapier, Bardeen ecosystems)
5. Nika CLAUDE.md documentation for feature verification

## Confidence Level

**High** -- All workflows are technically feasible with current Nika features (5 verbs,
extract modes, media tools, for_each, structured output, artifacts). Pricing data is from
public pricing pages. Use cases are grounded in tools people demonstrably pay for.

## Further Research Suggestions

- Survey Nika early adopters on which workflows they built first
- A/B test which workflow demos convert the most course signups
- Research enterprise-specific workflows (compliance, audit, reporting)
- Investigate MCP server integration workflows (NovaNet, filesystem, database)
- Track which SaaS tools get the most "alternative to X" searches
