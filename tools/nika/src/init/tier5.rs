//! Tier 5: Developer Use Cases (11-20)
//!
//! Complex, real-world developer workflows demonstrating advanced Nika features.
//!
//! Prerequisites: LLM provider + optional MCP servers
//!
//! Workflows:
//! - 11: Code Review Pipeline
//! - 12: Content Localization
//! - 13: SEO Content Generator
//! - 14: Documentation Generator
//! - 15: Data ETL Pipeline
//! - 16: Research Assistant
//! - 17: PR Review Bot
//! - 18: Meeting Processor
//! - 19: API Health Checker
//! - 20: Knowledge Extractor

use super::WorkflowTemplate;

pub const TIER5_DIR: &str = "tier-5-dev";

// =============================================================================
// WORKFLOW 11: Code Review Pipeline
// =============================================================================
pub const WORKFLOW_11_CODE_REVIEW: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔍 WORKFLOW 11: CODE REVIEW PIPELINE                                         ║
# ║  Multi-stage automated code review with security, performance, and style      ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  PIPELINE STAGES:                                                             ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │   ┌─────────────┐                                                      │   ║
# ║  │   │   INPUT     │   Git diff or file path                              │   ║
# ║  │   │   CODE      │                                                      │   ║
# ║  │   └──────┬──────┘                                                      │   ║
# ║  │          │                                                             │   ║
# ║  │          ▼                                                             │   ║
# ║  │   ┌─────────────┐                                                      │   ║
# ║  │   │   PARSE     │   Extract code structure                             │   ║
# ║  │   │   CODE      │                                                      │   ║
# ║  │   └──────┬──────┘                                                      │   ║
# ║  │          │                                                             │   ║
# ║  │   ┌──────┴───────────────────────────────┐                            │   ║
# ║  │   │              │               │       │                            │   ║
# ║  │   ▼              ▼               ▼       ▼                            │   ║
# ║  │ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐                   │   ║
# ║  │ │ SECURITY │ │ PERF     │ │ STYLE    │ │ LOGIC    │   ← Parallel     │   ║
# ║  │ │ REVIEW   │ │ REVIEW   │ │ REVIEW   │ │ REVIEW   │     Reviews       │   ║
# ║  │ └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘                   │   ║
# ║  │      │            │            │            │                         │   ║
# ║  │      └────────────┴────────────┴────────────┘                         │   ║
# ║  │                          │                                            │   ║
# ║  │                          ▼                                            │   ║
# ║  │                   ┌─────────────┐                                     │   ║
# ║  │                   │  SYNTHESIZE │   Combine all findings              │   ║
# ║  │                   │   REPORT    │                                     │   ║
# ║  │                   └──────┬──────┘                                     │   ║
# ║  │                          │                                            │   ║
# ║  │                          ▼                                            │   ║
# ║  │                   ┌─────────────┐                                     │   ║
# ║  │                   │  GENERATE   │   Create actionable fixes           │   ║
# ║  │                   │   FIXES     │                                     │   ║
# ║  │                   └─────────────┘                                     │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: code-review-pipeline

# Sample code for review - contains intentional vulnerabilities for detection!
# NOTE: This code has security issues that the review pipeline will DETECT
inputs:
  code_to_review: |
    async function fetchUserData(userId: string) {
      const response = await fetch(`/api/users/${userId}`);
      // BUG: Missing await - response.json() returns a Promise!
      const data = response.json();
      return data;
    }

    function processPayment(amount, cardNumber) {
      // SECURITY ISSUE: Logging sensitive data
      console.log(`Processing ${cardNumber} for $${amount}`);
      // SECURITY ISSUE: Dynamic code execution
      // This is a vulnerability the review should DETECT
      return processAmount(amount);
    }

tasks:
  # ═══════════════════════════════════════════════════════════════════════════════
  # PARSE CODE STRUCTURE
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: parse_code
    infer:
      prompt: |
        Analyze this code and extract its structure:

        ```typescript
        {{inputs.code_to_review}}
        ```

        Output a JSON structure with:
        - functions: array of function names and their signatures
        - imports: any imports/requires
        - async_operations: any async/await patterns
        - external_calls: API calls, database queries, etc.
      temperature: 0.1
      max_tokens: 500
    output:
      schema:
        type: object
        required: [functions, async_operations]
        properties:
          functions:
            type: array
            items:
              type: object
              properties:
                name: { type: string }
                params: { type: array, items: { type: string } }
                is_async: { type: boolean }
          imports: { type: array, items: { type: string } }
          async_operations: { type: array, items: { type: string } }
          external_calls: { type: array, items: { type: string } }

  # ═══════════════════════════════════════════════════════════════════════════════
  # PARALLEL REVIEWS (4 aspects)
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: security_review
    use:
      structure: $parse_code
    infer:
      prompt: |
        Perform a SECURITY review of this code:

        ```typescript
        {{inputs.code_to_review}}
        ```

        Code structure: {{use.structure}}

        Check for:
        1. SQL/Command injection vulnerabilities
        2. XSS vulnerabilities
        3. Sensitive data exposure (logs, errors)
        4. Insecure functions (dynamic code execution, etc.)
        5. Authentication/authorization issues
        6. Input validation problems

        Rate severity: CRITICAL, HIGH, MEDIUM, LOW
      temperature: 0.2
      max_tokens: 400
      system: You are a security auditor specializing in OWASP Top 10.
    output:
      schema:
        type: object
        required: [findings, risk_score]
        properties:
          findings:
            type: array
            items:
              type: object
              required: [issue, severity, line_hint, recommendation]
              properties:
                issue: { type: string }
                severity: { type: string, enum: [CRITICAL, HIGH, MEDIUM, LOW] }
                line_hint: { type: string }
                recommendation: { type: string }
          risk_score: { type: number, minimum: 0, maximum: 10 }

  - id: performance_review
    use:
      structure: $parse_code
    infer:
      prompt: |
        Perform a PERFORMANCE review of this code:

        ```typescript
        {{inputs.code_to_review}}
        ```

        Code structure: {{use.structure}}

        Check for:
        1. Unnecessary re-renders or recomputations
        2. Missing memoization opportunities
        3. Inefficient algorithms (O(n²) etc.)
        4. Memory leaks
        5. Blocking operations in async code
        6. Missing error boundaries

        Focus on practical, measurable improvements.
      temperature: 0.2
      max_tokens: 400
      system: You are a performance engineer focused on runtime efficiency.
    output:
      schema:
        type: object
        required: [findings, perf_score]
        properties:
          findings:
            type: array
            items:
              type: object
              required: [issue, impact, suggestion]
              properties:
                issue: { type: string }
                impact: { type: string, enum: [HIGH, MEDIUM, LOW] }
                suggestion: { type: string }
          perf_score: { type: number, minimum: 0, maximum: 10 }

  - id: style_review
    use:
      structure: $parse_code
    infer:
      prompt: |
        Perform a CODE STYLE review:

        ```typescript
        {{inputs.code_to_review}}
        ```

        Check for:
        1. Naming conventions (camelCase, PascalCase, etc.)
        2. Function length and complexity
        3. Comments and documentation
        4. Consistent formatting
        5. TypeScript type usage
        6. Modern JS/TS patterns
      temperature: 0.3
      max_tokens: 300
      system: You follow TypeScript best practices and clean code principles.
    output:
      schema:
        type: object
        required: [findings, style_score]
        properties:
          findings:
            type: array
            items:
              type: object
              properties:
                issue: { type: string }
                suggestion: { type: string }
          style_score: { type: number, minimum: 0, maximum: 10 }

  - id: logic_review
    use:
      structure: $parse_code
    infer:
      prompt: |
        Perform a LOGIC review:

        ```typescript
        {{inputs.code_to_review}}
        ```

        Check for:
        1. Missing error handling
        2. Edge cases not covered
        3. Race conditions
        4. Incorrect async/await usage
        5. Type safety issues
        6. Null/undefined handling
      temperature: 0.2
      max_tokens: 400
      system: You are a senior developer focused on correctness and robustness.
    output:
      schema:
        type: object
        required: [findings, logic_score]
        properties:
          findings:
            type: array
            items:
              type: object
              required: [issue, fix]
              properties:
                issue: { type: string }
                fix: { type: string }
          logic_score: { type: number, minimum: 0, maximum: 10 }

  # ═══════════════════════════════════════════════════════════════════════════════
  # SYNTHESIZE REPORT
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: synthesize_report
    use:
      security: $security_review
      performance: $performance_review
      style: $style_review
      logic: $logic_review
    infer:
      prompt: |
        Synthesize these code review findings into an executive summary:

        🔒 SECURITY: {{use.security}}
        ⚡ PERFORMANCE: {{use.performance}}
        🎨 STYLE: {{use.style}}
        🧠 LOGIC: {{use.logic}}

        Create a prioritized action plan with:
        1. Must-fix issues (blockers)
        2. Should-fix issues (important)
        3. Nice-to-have improvements
        4. Overall code health score (0-100)
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: ./output/code-review/report_{{date}}.txt
      format: text

  # ═══════════════════════════════════════════════════════════════════════════════
  # GENERATE FIXES
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: generate_fixes
    use:
      report: $synthesize_report
    agent:
      prompt: |
        Based on this code review report, generate fixed code:

        {{use.report}}

        Original code:
        ```typescript
        {{inputs.code_to_review}}
        ```

        Write the corrected version addressing all CRITICAL and HIGH issues.
        Include comments explaining each fix.
      tools:
        - nika:write
      max_turns: 5
      temperature: 0.2
    artifact:
      path: ./output/code-review/fixed_code_{{date}}.ts
      format: text

flows:
  - source: parse_code
    target: security_review
  - source: parse_code
    target: performance_review
  - source: parse_code
    target: style_review
  - source: parse_code
    target: logic_review
  - source: security_review
    target: synthesize_report
  - source: performance_review
    target: synthesize_report
  - source: style_review
    target: synthesize_report
  - source: logic_review
    target: synthesize_report
  - source: synthesize_report
    target: generate_fixes
"##;

// =============================================================================
// WORKFLOW 12: Content Localization
// =============================================================================
pub const WORKFLOW_12_LOCALIZATION: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🌍 WORKFLOW 12: CONTENT LOCALIZATION                                         ║
# ║  Culturally-aware multi-locale content generation                             ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │    ┌────────────┐                                                     │   ║
# ║  │    │  SOURCE    │   English master content                             │   ║
# ║  │    │  CONTENT   │                                                      │   ║
# ║  │    └─────┬──────┘                                                      │   ║
# ║  │          │                                                             │   ║
# ║  │          ▼                                                             │   ║
# ║  │    ┌────────────┐                                                      │   ║
# ║  │    │  ANALYZE   │   Identify culture-sensitive elements                │   ║
# ║  │    │  CONTENT   │                                                      │   ║
# ║  │    └─────┬──────┘                                                      │   ║
# ║  │          │                                                             │   ║
# ║  │    ┌─────┴──────────────────────────────────────┐                     │   ║
# ║  │    │              FOR-EACH LOCALE               │                     │   ║
# ║  │    │   ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐  │                     │   ║
# ║  │    │   │ fr-FR│  │ de-DE│  │ ja-JP│  │ es-ES│  │                     │   ║
# ║  │    │   └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘  │                     │   ║
# ║  │    │      │         │         │         │      │                     │   ║
# ║  │    │      └─────────┴─────────┴─────────┘      │                     │   ║
# ║  │    │                    │                       │                     │   ║
# ║  │    └────────────────────┼───────────────────────┘                     │   ║
# ║  │                         ▼                                              │   ║
# ║  │                  ┌────────────┐                                       │   ║
# ║  │                  │  QUALITY   │                                       │   ║
# ║  │                  │   CHECK    │                                       │   ║
# ║  │                  └─────┬──────┘                                       │   ║
# ║  │                        │                                               │   ║
# ║  │                        ▼                                               │   ║
# ║  │                  ┌────────────┐                                       │   ║
# ║  │                  │  EXPORT    │   JSON/YAML per locale                │   ║
# ║  │                  │   FILES    │                                       │   ║
# ║  │                  └────────────┘                                       │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: content-localization-pipeline

inputs:
  source_content:
    headline: "Transform Your Home with Smart AI"
    subheadline: "Set it and forget it - your home learns your habits"
    cta: "Get Started Free"
    features:
      - title: "Voice Control"
        description: "Just say the word. Works with Alexa, Siri, and Google."
      - title: "Energy Savings"
        description: "Save up to 30% on your electricity bill automatically."
      - title: "24/7 Security"
        description: "Sleep tight knowing AI watches over your home."

tasks:
  - id: analyze_content
    infer:
      prompt: |
        Analyze this content for localization challenges:

        {{inputs.source_content}}

        Identify:
        1. Idioms that need cultural adaptation
        2. Measurements that need conversion
        3. Cultural references that may not translate
        4. Legal/regulatory considerations per region
        5. Tone adjustments needed
      temperature: 0.2
      max_tokens: 400

  - id: localize
    for_each:
      - locale: fr-FR
        language: French
        notes: "Use formal 'vous', metric units, emphasize elegance"
      - locale: de-DE
        language: German
        notes: "Technical precision, compound words OK, energy efficiency focus"
      - locale: ja-JP
        language: Japanese
        notes: "Polite keigo, emphasize harmony and precision"
      - locale: es-ES
        language: Spanish (Spain)
        notes: "Use vosotros, warm tone, family focus"
    as: target
    concurrency: 4
    use:
      analysis: $analyze_content
    infer:
      prompt: |
        Localize this content for {{use.target.language}} ({{use.target.locale}}):

        SOURCE:
        {{inputs.source_content}}

        LOCALIZATION ANALYSIS:
        {{use.analysis}}

        LOCALE GUIDELINES:
        {{use.target.notes}}

        Output a JSON object with the same structure as the source,
        but fully localized for the target audience.
      temperature: 0.4
      max_tokens: 600
      system: |
        You are a native {{use.target.language}} speaker and professional localizer.
        Focus on cultural adaptation, not just translation.
    output:
      schema:
        type: object
        required: [headline, subheadline, cta, features]
        properties:
          headline: { type: string }
          subheadline: { type: string }
          cta: { type: string }
          features:
            type: array
            items:
              type: object
              properties:
                title: { type: string }
                description: { type: string }

  - id: quality_check
    use:
      localizations: $localize
    infer:
      prompt: |
        Review these localizations for quality:

        {{use.localizations}}

        For each locale, check:
        1. Natural language usage (sounds native?)
        2. Brand consistency
        3. Cultural appropriateness
        4. Technical accuracy

        Score each locale 1-10 and flag any issues.
      temperature: 0.2
      max_tokens: 500
    artifact:
      path: ./output/localization/quality_report_{{date}}.txt
      format: text

  - id: export_files
    use:
      localizations: $localize
      quality: $quality_check
    agent:
      prompt: |
        Export each localization to its own JSON file:

        {{use.localizations}}

        Create files:
        - ./output/localization/fr-FR.json
        - ./output/localization/de-DE.json
        - ./output/localization/ja-JP.json
        - ./output/localization/es-ES.json

        When done, say EXPORT_COMPLETE.
      tools:
        - nika:write
      max_turns: 5

flows:
  - source: analyze_content
    target: localize
  - source: localize
    target: quality_check
  - source: quality_check
    target: export_files
"##;

// =============================================================================
// WORKFLOW 13: SEO Content Generator
// =============================================================================
pub const WORKFLOW_13_SEO_CONTENT: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  📈 WORKFLOW 13: SEO CONTENT GENERATOR                                        ║
# ║  Keyword research + content optimization + schema markup                      ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │    ┌─────────┐   ┌─────────┐   ┌─────────┐                            │   ║
# ║  │    │ KEYWORD │──►│ CONTENT │──►│ OPTIMIZE│                            │   ║
# ║  │    │ RESEARCH│   │ OUTLINE │   │  & SEO  │                            │   ║
# ║  │    └─────────┘   └─────────┘   └────┬────┘                            │   ║
# ║  │                                     │                                 │   ║
# ║  │                   ┌─────────────────┼─────────────────┐              │   ║
# ║  │                   ▼                 ▼                 ▼              │   ║
# ║  │            ┌───────────┐    ┌───────────┐    ┌───────────┐          │   ║
# ║  │            │   META    │    │  SCHEMA   │    │  CONTENT  │          │   ║
# ║  │            │   TAGS    │    │  MARKUP   │    │   BODY    │          │   ║
# ║  │            └─────┬─────┘    └─────┬─────┘    └─────┬─────┘          │   ║
# ║  │                  │                │                │                │   ║
# ║  │                  └────────────────┴────────────────┘                │   ║
# ║  │                                   │                                 │   ║
# ║  │                                   ▼                                 │   ║
# ║  │                            ┌───────────┐                            │   ║
# ║  │                            │  FINAL    │                            │   ║
# ║  │                            │   PAGE    │                            │   ║
# ║  │                            └───────────┘                            │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: seo-content-generator

inputs:
  topic: "AI-powered QR codes for small businesses"
  target_audience: "Small business owners in retail and hospitality"
  word_count: 1500

tasks:
  - id: keyword_research
    infer:
      prompt: |
        Conduct keyword research for an article about:
        "{{inputs.topic}}"

        Target audience: {{inputs.target_audience}}

        Provide:
        1. Primary keyword (high intent, moderate competition)
        2. 5 secondary keywords (related topics)
        3. 10 long-tail keywords (specific queries)
        4. Question keywords (what, how, why queries)
        5. LSI keywords (semantically related)
      temperature: 0.4
      max_tokens: 500
    output:
      schema:
        type: object
        required: [primary, secondary, longtail, questions, lsi]
        properties:
          primary: { type: string }
          secondary: { type: array, items: { type: string }, maxItems: 5 }
          longtail: { type: array, items: { type: string }, maxItems: 10 }
          questions: { type: array, items: { type: string }, maxItems: 5 }
          lsi: { type: array, items: { type: string }, maxItems: 10 }

  - id: content_outline
    use:
      keywords: $keyword_research
    infer:
      prompt: |
        Create a detailed content outline for:
        "{{inputs.topic}}"

        Target: {{inputs.word_count}} words
        Keywords: {{use.keywords}}

        Structure:
        1. H1 (include primary keyword)
        2. Introduction hook (problem/solution)
        3. H2 sections (3-5 main topics)
        4. H3 subsections under each H2
        5. FAQ section (use question keywords)
        6. Conclusion with CTA
      temperature: 0.5
      max_tokens: 600

  - id: generate_meta
    use:
      keywords: $keyword_research
      outline: $content_outline
    infer:
      prompt: |
        Generate SEO meta tags:

        Topic: {{inputs.topic}}
        Keywords: {{use.keywords}}

        Create:
        1. Title tag (50-60 chars, primary keyword near start)
        2. Meta description (150-160 chars, compelling CTA)
        3. OG title and description
        4. Twitter card content
      temperature: 0.3
      max_tokens: 300
    output:
      schema:
        type: object
        required: [title, description, og, twitter]
        properties:
          title: { type: string, maxLength: 65 }
          description: { type: string, maxLength: 165 }
          og:
            type: object
            properties:
              title: { type: string }
              description: { type: string }
          twitter:
            type: object
            properties:
              title: { type: string }
              description: { type: string }

  - id: generate_schema
    use:
      keywords: $keyword_research
      outline: $content_outline
    infer:
      prompt: |
        Generate JSON-LD schema markup for this article:

        Topic: {{inputs.topic}}
        Outline: {{use.outline}}

        Create Schema.org structured data for:
        1. Article schema
        2. FAQPage schema (from question keywords)
        3. Organization schema
        4. BreadcrumbList schema
      temperature: 0.2
      max_tokens: 800
    output:
      schema:
        type: object
        required: [article, faq]
        properties:
          article: { type: object }
          faq: { type: object }
          organization: { type: object }
          breadcrumbs: { type: object }

  - id: generate_content
    use:
      keywords: $keyword_research
      outline: $content_outline
    infer:
      prompt: |
        Write the full article content following this outline:

        {{use.outline}}

        Keywords to naturally incorporate:
        {{use.keywords}}

        Guidelines:
        - {{inputs.word_count}} words target
        - Conversational but authoritative tone
        - Include keyword in first 100 words
        - Use short paragraphs (2-3 sentences)
        - Add internal link placeholders [LINK: topic]
        - Include data/statistics where relevant
      temperature: 0.6
      max_tokens: 3000
      system: |
        You are an expert content writer specializing in SEO-optimized articles.
        Write naturally while strategically placing keywords.

  - id: assemble_page
    use:
      meta: $generate_meta
      schema_markup: $generate_schema
      content: $generate_content
    infer:
      prompt: |
        Assemble the final HTML page with all components:

        META: {{use.meta}}
        SCHEMA: {{use.schema_markup}}
        CONTENT: {{use.content}}

        Output a complete HTML document with:
        1. Proper head section (meta, title, schema)
        2. Article body with semantic HTML
        3. Accessibility attributes (alt, aria)
      temperature: 0.2
      max_tokens: 4000
    artifact:
      path: ./output/seo/article_{{date}}.html
      format: text

flows:
  - source: keyword_research
    target: content_outline
  - source: content_outline
    target: generate_meta
  - source: content_outline
    target: generate_schema
  - source: content_outline
    target: generate_content
  - source: generate_meta
    target: assemble_page
  - source: generate_schema
    target: assemble_page
  - source: generate_content
    target: assemble_page
"##;

// =============================================================================
// WORKFLOW 14: Documentation Generator
// =============================================================================
pub const WORKFLOW_14_DOCS_GENERATOR: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  📚 WORKFLOW 14: DOCUMENTATION GENERATOR                                      ║
# ║  Auto-generate docs from code with examples                                   ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │   ┌──────────┐                                                        │   ║
# ║  │   │  SCAN    │   Find code files                                      │   ║
# ║  │   │  PROJECT │                                                         │   ║
# ║  │   └────┬─────┘                                                         │   ║
# ║  │        │                                                               │   ║
# ║  │        ▼                                                               │   ║
# ║  │   ┌──────────────────────────────────────────────────────────┐       │   ║
# ║  │   │                   FOR-EACH FILE                          │       │   ║
# ║  │   │    ┌────────┐    ┌────────┐    ┌────────┐               │       │   ║
# ║  │   │    │ Parse  │───►│Generate│───►│ Create │               │       │   ║
# ║  │   │    │  AST   │    │  Docs  │    │Examples│               │       │   ║
# ║  │   │    └────────┘    └────────┘    └────────┘               │       │   ║
# ║  │   └─────────────────────────────────┬────────────────────────┘       │   ║
# ║  │                                     │                                │   ║
# ║  │                                     ▼                                │   ║
# ║  │                            ┌──────────────┐                          │   ║
# ║  │                            │   GENERATE   │                          │   ║
# ║  │                            │    INDEX     │                          │   ║
# ║  │                            └──────────────┘                          │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: docs-generator

tasks:
  - id: scan_project
    agent:
      prompt: |
        Scan the current project for code files that need documentation.
        Look for:
        - TypeScript/JavaScript files (.ts, .tsx, .js)
        - Rust files (.rs)
        - Python files (.py)

        List all files that export public APIs.
        Say SCAN_COMPLETE when done.
      tools:
        - nika:glob
        - nika:read
      max_turns: 10

  - id: generate_docs
    use:
      files: $scan_project
    agent:
      prompt: |
        Generate documentation for each file found:

        {{use.files}}

        For each file:
        1. Extract function/class signatures
        2. Infer purpose from code
        3. Generate JSDoc/RustDoc style comments
        4. Create usage examples
        5. Document parameters and return types

        Write docs to ./docs/api/<filename>.md
        Say DOCS_COMPLETE when all files are documented.
      tools:
        - nika:read
        - nika:write
        - nika:glob
      max_turns: 20
      temperature: 0.3

  - id: generate_index
    use:
      docs: $generate_docs
    agent:
      prompt: |
        Create a documentation index:

        1. Read all generated docs in ./docs/api/
        2. Create ./docs/README.md with:
           - Project overview
           - Quick start guide
           - API reference index (links to each doc)
           - Getting help section

        Say INDEX_COMPLETE when done.
      tools:
        - nika:glob
        - nika:read
        - nika:write
      max_turns: 10

flows:
  - source: scan_project
    target: generate_docs
  - source: generate_docs
    target: generate_index
"##;

// =============================================================================
// WORKFLOWS 15-20: Developer use-cases
// =============================================================================

pub const WORKFLOW_15_ETL: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔄 WORKFLOW 15: DATA ETL PIPELINE                                            ║
# ║  Extract, Transform, Load with validation                                     ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: data-etl-pipeline

tasks:
  - id: extract
    fetch:
      url: https://httpbin.org/json
      method: GET
      headers:
        Accept: application/json

  - id: validate
    use:
      raw_data: $extract
    infer:
      prompt: |
        Validate this data for consistency:
        {{use.raw_data}}

        Check: required fields, data types, value ranges
      temperature: 0.1

  - id: transform
    use:
      validated: $validate
      data: $extract
    infer:
      prompt: |
        Transform this data:
        {{use.data}}

        Validation results: {{use.validated}}

        Apply transformations:
        1. Normalize field names to camelCase
        2. Convert dates to ISO 8601
        3. Remove null values
        4. Add computed fields
      temperature: 0.1
    output:
      schema:
        type: array
        items:
          type: object

  - id: load
    use:
      transformed: $transform
    agent:
      prompt: |
        Load this data to output files:
        {{use.transformed}}

        Create: ./output/etl/data_{{date}}.json
        Say LOAD_COMPLETE when done.
      tools:
        - nika:write
      max_turns: 3

flows:
  - source: extract
    target: validate
  - source: validate
    target: transform
  - source: transform
    target: load
"##;

pub const WORKFLOW_16_RESEARCH: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔬 WORKFLOW 16: RESEARCH ASSISTANT                                           ║
# ║  Deep research with nested agents                                             ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: research-assistant

inputs:
  topic: "Impact of AI on software development productivity"

tasks:
  - id: research_agent
    agent:
      prompt: |
        Research this topic thoroughly:
        "{{inputs.topic}}"

        Your process:
        1. Break down into sub-questions
        2. Research each sub-question
        3. Synthesize findings
        4. Identify gaps or contradictions
        5. Provide evidence-based conclusions

        Write your final report to ./output/research/report_{{date}}.md
        Include citations and confidence levels.
        Say RESEARCH_COMPLETE when done.
      tools:
        - nika:read
        - nika:write
        - nika:glob
      max_turns: 15
      depth_limit: 2
      temperature: 0.4
    artifact:
      path: ./output/research/summary_{{date}}.txt
      format: text
"##;

pub const WORKFLOW_17_PR_REVIEW: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔀 WORKFLOW 17: PR REVIEW BOT                                                ║
# ║  Automated pull request analysis                                              ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: pr-review-bot

inputs:
  pr_diff: |
    --- a/src/utils.ts
    +++ b/src/utils.ts
    @@ -1,5 +1,10 @@
    +import { cache } from './cache';
    +
     export function processData(input: any) {
    -  return input.map(x => x * 2);
    +  const cached = cache.get(input);
    +  if (cached) return cached;
    +  const result = input.map(x => x * 2);
    +  cache.set(input, result);
    +  return result;
     }

tasks:
  - id: analyze_diff
    infer:
      prompt: |
        Analyze this PR diff:

        {{inputs.pr_diff}}

        Provide:
        1. Summary of changes (1-2 sentences)
        2. Files modified
        3. Type of change (feature/fix/refactor/etc)
        4. Impact assessment (low/medium/high)
      temperature: 0.2
      max_tokens: 300

  - id: review_changes
    use:
      analysis: $analyze_diff
    for_each:
      - aspect: correctness
        prompt: "Check for bugs, edge cases, type safety"
      - aspect: performance
        prompt: "Check for performance implications"
      - aspect: security
        prompt: "Check for security vulnerabilities"
    as: review
    concurrency: 3
    infer:
      prompt: |
        Review this change for {{use.review.aspect}}:

        {{inputs.pr_diff}}

        Focus: {{use.review.prompt}}
      temperature: 0.2
      max_tokens: 300

  - id: generate_review
    use:
      analysis: $analyze_diff
      reviews: $review_changes
    infer:
      prompt: |
        Generate a PR review comment:

        Analysis: {{use.analysis}}
        Detailed reviews: {{use.reviews}}

        Format as GitHub review comment with:
        - Summary
        - Specific line comments
        - Overall recommendation (approve/request changes/comment)
      temperature: 0.3
      max_tokens: 600

flows:
  - source: analyze_diff
    target: review_changes
  - source: review_changes
    target: generate_review
"##;

pub const WORKFLOW_18_MEETING: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  📅 WORKFLOW 18: MEETING PROCESSOR                                            ║
# ║  Extract action items and summaries from meeting notes                        ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: meeting-processor

inputs:
  transcript: |
    John: Let's discuss the Q2 roadmap. Sarah, can you update us on the API work?
    Sarah: We're behind schedule by 2 weeks. Need more backend resources.
    John: Okay, I'll talk to HR about that. When can we realistically ship?
    Sarah: End of April if we get help by next week.
    Mike: I can help with the auth module starting Monday.
    John: Perfect. Sarah, can you send Mike the specs today?
    Sarah: Will do. Also, we need to discuss the pricing changes.
    John: Let's schedule that for Thursday. Can someone set up the meeting?
    Mike: I'll handle it.

tasks:
  - id: extract_info
    infer:
      prompt: |
        Extract structured information from this meeting:

        {{inputs.transcript}}

        Identify:
        1. Participants and their roles
        2. Topics discussed
        3. Decisions made
        4. Action items with owners
        5. Follow-up meetings needed
      temperature: 0.1
      max_tokens: 500
    output:
      schema:
        type: object
        required: [participants, action_items, decisions]
        properties:
          participants: { type: array, items: { type: string } }
          topics: { type: array, items: { type: string } }
          decisions: { type: array, items: { type: string } }
          action_items:
            type: array
            items:
              type: object
              required: [task, owner, deadline]
              properties:
                task: { type: string }
                owner: { type: string }
                deadline: { type: string }
          followups: { type: array, items: { type: string } }

  - id: generate_summary
    use:
      extracted: $extract_info
    infer:
      prompt: |
        Create an executive summary:
        {{use.extracted}}

        Format for email with:
        - TL;DR (2 sentences)
        - Key decisions
        - Action items table
        - Next steps
      temperature: 0.3
      max_tokens: 400
    artifact:
      path: ./output/meetings/summary_{{date}}.txt
      format: text

flows:
  - source: extract_info
    target: generate_summary
"##;

pub const WORKFLOW_19_API_HEALTH: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🏥 WORKFLOW 19: API HEALTH CHECKER                                           ║
# ║  Monitor multiple endpoints in parallel                                       ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: api-health-checker

tasks:
  - id: check_endpoints
    for_each:
      - name: httpbin (JSON)
        url: https://httpbin.org/json
        expected_status: 200
      - name: httpbin (status)
        url: https://httpbin.org/status/200
        expected_status: 200
      - name: httpbin (headers)
        url: https://httpbin.org/headers
        expected_status: 200
    as: endpoint
    concurrency: 3
    fetch:
      url: "{{use.endpoint.url}}"
      method: GET
      timeout: 5000

  - id: analyze_results
    use:
      checks: $check_endpoints
    infer:
      prompt: |
        Analyze these health check results:
        {{use.checks}}

        Provide:
        1. Overall system status (healthy/degraded/down)
        2. Any failing endpoints
        3. Recommended actions
        4. Alert priority (P1/P2/P3/P4)
      temperature: 0.1
      max_tokens: 300
    output:
      schema:
        type: object
        required: [status, failing, priority]
        properties:
          status: { type: string, enum: [healthy, degraded, down] }
          failing: { type: array, items: { type: string } }
          recommendations: { type: array, items: { type: string } }
          priority: { type: string, enum: [P1, P2, P3, P4] }

flows:
  - source: check_endpoints
    target: analyze_results
"##;

pub const WORKFLOW_20_KNOWLEDGE_EXTRACT: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🧠 WORKFLOW 20: KNOWLEDGE EXTRACTOR                                          ║
# ║  Extract entities and relationships from text                                 ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: nika/workflow@0.10
workflow: knowledge-extractor

inputs:
  text: |
    Apple Inc., founded by Steve Jobs and Steve Wozniak in Cupertino,
    California, is a technology company. Tim Cook became CEO in 2011.
    Apple's main products include the iPhone, iPad, and Mac computers.
    The company is headquartered in Apple Park, completed in 2017.

tasks:
  - id: extract_entities
    infer:
      prompt: |
        Extract named entities from this text:
        {{inputs.text}}

        Categories: Person, Organization, Location, Product, Date, Event
      temperature: 0.1
      max_tokens: 400
    output:
      schema:
        type: object
        required: [entities]
        properties:
          entities:
            type: array
            items:
              type: object
              required: [name, type]
              properties:
                name: { type: string }
                type: { type: string }
                attributes: { type: object }

  - id: extract_relations
    use:
      entities: $extract_entities
    infer:
      prompt: |
        Extract relationships between these entities:
        {{use.entities}}

        Original text: {{inputs.text}}

        Identify relations like:
        - FOUNDED_BY, CEO_OF, LOCATED_IN, PRODUCES, WORKS_AT
      temperature: 0.1
      max_tokens: 400
    output:
      schema:
        type: object
        required: [relations]
        properties:
          relations:
            type: array
            items:
              type: object
              required: [source, relation, target]
              properties:
                source: { type: string }
                relation: { type: string }
                target: { type: string }

  - id: build_graph
    use:
      entities: $extract_entities
      relations: $extract_relations
    infer:
      prompt: |
        Create a knowledge graph representation:

        Entities: {{use.entities}}
        Relations: {{use.relations}}

        Output as Cypher CREATE statements for Neo4j.
      temperature: 0.1
      max_tokens: 600
    artifact:
      path: ./output/knowledge/graph_{{date}}.cypher
      format: text

flows:
  - source: extract_entities
    target: extract_relations
  - source: extract_relations
    target: build_graph
"##;

/// Returns all Tier 5 workflows (11-20)
pub fn get_tier5_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "11-code-review-pipeline.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_11_CODE_REVIEW,
        },
        WorkflowTemplate {
            filename: "12-content-localization.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_12_LOCALIZATION,
        },
        WorkflowTemplate {
            filename: "13-seo-content-generator.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_13_SEO_CONTENT,
        },
        WorkflowTemplate {
            filename: "14-documentation-generator.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_14_DOCS_GENERATOR,
        },
        WorkflowTemplate {
            filename: "15-data-etl-pipeline.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_15_ETL,
        },
        WorkflowTemplate {
            filename: "16-research-assistant.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_16_RESEARCH,
        },
        WorkflowTemplate {
            filename: "17-pr-review-bot.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_17_PR_REVIEW,
        },
        WorkflowTemplate {
            filename: "18-meeting-processor.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_18_MEETING,
        },
        WorkflowTemplate {
            filename: "19-api-health-checker.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_19_API_HEALTH,
        },
        WorkflowTemplate {
            filename: "20-knowledge-extractor.nika.yaml",
            tier_dir: TIER5_DIR,
            content: WORKFLOW_20_KNOWLEDGE_EXTRACT,
        },
    ]
}
