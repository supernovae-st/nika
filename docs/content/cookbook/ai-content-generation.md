# AI Content Generation Recipes

Production-ready workflows for generating, translating, and managing AI-powered content at scale using the `infer:` verb.

---

## Recipe 1: Blog Post Generator with Research

**Problem:** You need to generate well-researched blog posts that pull from live web sources, not just the LLM's training data.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: blog-post-generator
description: "Research a topic from live sources, then generate a polished blog post"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "Rust async patterns in 2026"
  audience: "senior backend developers"
  word_count: 1500

artifacts:
  dir: ./output/blog-posts

tasks:
  # Step 1: Gather live research from multiple sources
  - id: research_sources
    description: "Fetch current articles on the topic"
    for_each:
      items:
        - { url: "https://blog.rust-lang.org/", name: "Rust Blog" }
        - { url: "https://news.ycombinator.com/", name: "Hacker News" }
        - { url: "https://www.reddit.com/r/rust/.json", name: "Reddit Rust" }
      as: source
      concurrency: 3
    fetch:
      url: "{{with.source.url}}"
      extract: article
      timeout: 20

  # Step 2: Generate an outline based on research
  - id: outline
    depends_on: [research_sources]
    with:
      sources: $research_sources
    infer:
      system: |
        You are an expert technical content strategist.
        Target audience: {{inputs.audience}}.
      prompt: |
        Based on these research sources:
        {{with.sources | first(4000)}}

        Create a detailed blog post outline for the topic: "{{inputs.topic}}"
        Target word count: {{inputs.word_count}}

        Return a structured outline with:
        - Working title (compelling, specific)
        - Hook paragraph concept
        - 4-6 sections with subheadings and key points
        - Conclusion angle
        - 3 internal linking opportunities
      temperature: 0.5
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          hook:
            type: string
          sections:
            type: array
            items:
              type: object
              properties:
                heading:
                  type: string
                key_points:
                  type: array
                  items:
                    type: string
              required: [heading, key_points]
        required: [title, hook, sections]
    artifact:
      path: outline.json
      format: json

  # Step 3: Write the full blog post
  - id: write_post
    depends_on: [outline]
    with:
      plan: $outline
      research: $research_sources
    infer:
      system: |
        You are a senior technical writer who creates engaging, accurate content
        for {{inputs.audience}}. Write in a clear, authoritative voice.
        Use code examples where appropriate. Avoid fluff.
      prompt: |
        Write a complete blog post following this outline:
        {{with.plan}}

        Reference material:
        {{with.research | first(3000)}}

        Requirements:
        - Target {{inputs.word_count}} words
        - Include at least 2 code snippets
        - Use Markdown formatting with proper headers
        - End with a clear call to action
      temperature: 0.6
      max_tokens: 4000
    artifact:
      path: "{{inputs.topic | lower | trim}}.md"

  # Step 4: Generate SEO metadata
  - id: seo_metadata
    depends_on: [write_post]
    with:
      post: $write_post
    infer:
      prompt: |
        Generate SEO metadata for this blog post:
        {{with.post | first(2000)}}

        Return JSON with: meta_title (60 chars), meta_description (155 chars),
        slug, keywords (5-8), og_description, twitter_card_text.
      response_format: json
      temperature: 0.2
      max_tokens: 500
    structured:
      schema:
        type: object
        properties:
          meta_title:
            type: string
          meta_description:
            type: string
          slug:
            type: string
          keywords:
            type: array
            items:
              type: string
        required: [meta_title, meta_description, slug, keywords]
    artifact:
      path: seo-metadata.json
      format: json
```

**Explanation:**

This workflow demonstrates a four-stage content pipeline:

1. **Research** (`for_each` + `fetch:` + `extract: article`): Scrapes three live web sources in parallel with `concurrency: 3`. The `extract: article` mode uses the Readability algorithm to strip navigation and ads, returning only the main content.

2. **Outline** (`infer:` + `structured:`): The LLM creates a JSON-validated outline. The `structured:` block enforces the schema, ensuring the output always has the required fields. The `{{inputs.audience}}` template pulls from workflow inputs.

3. **Write** (`infer:` with bindings): Combines the outline and research via `with:` bindings. The `first(3000)` transform truncates the research to fit within token limits.

4. **SEO** (`infer:` + `structured:`): Generates metadata with strict JSON schema validation.

**Expected Output:** Four files in `./output/blog-posts/`: the outline JSON, the full markdown post, and SEO metadata JSON.

---

## Recipe 2: Social Media Content Suite

**Problem:** You need to generate platform-specific social media content from a single piece of source material, each adapted to the platform's constraints and style.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: social-media-suite
description: "Generate platform-specific posts from a single topic"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  source_url: "https://blog.rust-lang.org/"
  brand_voice: "technical but approachable, uses analogies, avoids jargon"

artifacts:
  dir: ./output/social-media

tasks:
  # Extract source material
  - id: source_content
    fetch:
      url: "{{inputs.source_url}}"
      extract: markdown
      timeout: 20

  # Generate platform-specific content in parallel
  - id: platform_posts
    depends_on: [source_content]
    with:
      content: $source_content
    for_each:
      items:
        - platform: "Twitter/X"
          constraints: "280 characters max. Use thread format (1/N). Include 2-3 hashtags."
          count: 5
        - platform: "LinkedIn"
          constraints: "600-1200 characters. Professional tone. Include a hook question. Use line breaks for readability."
          count: 1
        - platform: "Mastodon"
          constraints: "500 characters max. No tracking links. Use CamelCase hashtags."
          count: 3
        - platform: "Dev.to"
          constraints: "Full article teaser, 200-400 words. Include cover image suggestion and tags."
          count: 1
        - platform: "Hacker News"
          constraints: "Submission title only. Factual, no clickbait, under 80 characters."
          count: 1
      as: platform
      concurrency: 5
    infer:
      system: |
        You are a social media content specialist.
        Brand voice: {{inputs.brand_voice}}
      prompt: |
        Source material:
        {{with.content | first(3000)}}

        Generate {{with.platform.count}} post(s) for {{with.platform.platform}}.
        Platform constraints: {{with.platform.constraints}}

        Format each post clearly separated with "---" between them.
      temperature: 0.7
      max_tokens: 1000

  # Create a content calendar entry
  - id: content_calendar
    depends_on: [platform_posts]
    with:
      posts: $platform_posts
    infer:
      prompt: |
        Create a content calendar from these social media posts:
        {{with.posts}}

        Return JSON with posting schedule (suggested times, platforms, post text).
        Suggest optimal posting times for tech audience engagement.
      response_format: json
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: content-calendar.json
      format: json
```

**Explanation:**

The `for_each:` block iterates over five platform definitions in parallel. Each iteration gets the full source content via `with:` bindings but generates content tailored to that platform's constraints. The `concurrency: 5` setting means all five platforms are processed simultaneously.

**Expected Output:** A JSON content calendar with timestamped, platform-specific posts ready for scheduling.

---

## Recipe 3: Email Campaign Generator

**Problem:** You need to generate a complete email drip campaign with personalization tokens and A/B test variants.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: email-campaign-generator
description: "Generate a 5-email drip campaign with A/B variants"
provider: openai
model: gpt-4o

inputs:
  product_name: "Nika Workflow Engine"
  target_persona: "DevOps engineers at mid-size companies"
  campaign_goal: "trial signup"

context:
  files:
    brand: ./context/brand-guide.md

artifacts:
  dir: ./output/email-campaign

tasks:
  # Generate campaign strategy
  - id: strategy
    infer:
      system: "You are an email marketing strategist specializing in developer tools."
      prompt: |
        Create a 5-email drip campaign strategy for {{inputs.product_name}}.
        Target: {{inputs.target_persona}}
        Goal: {{inputs.campaign_goal}}
        Brand guidelines: {{context.files.brand | first(1000)}}

        For each email, define: subject line, purpose, CTA, timing (days after signup).
      response_format: json
      temperature: 0.5
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          campaign_name:
            type: string
          emails:
            type: array
            items:
              type: object
              properties:
                day:
                  type: integer
                subject:
                  type: string
                purpose:
                  type: string
                cta:
                  type: string
              required: [day, subject, purpose, cta]
        required: [campaign_name, emails]
    artifact:
      path: campaign-strategy.json
      format: json

  # Write each email with A/B subject line variants
  - id: write_emails
    depends_on: [strategy]
    with:
      plan: $strategy
    for_each:
      items:
        - { email_num: 1, tone: "welcoming and curious" }
        - { email_num: 2, tone: "educational and helpful" }
        - { email_num: 3, tone: "social proof and urgency" }
        - { email_num: 4, tone: "technical deep-dive" }
        - { email_num: 5, tone: "final push with offer" }
      as: brief
      concurrency: 3
    infer:
      system: |
        You are an email copywriter for developer tools.
        Brand voice: {{context.files.brand | first(500)}}
      prompt: |
        Write email #{{with.brief.email_num}} of the campaign.
        Campaign plan: {{with.plan | first(1000)}}
        Tone: {{with.brief.tone}}
        Product: {{inputs.product_name}}
        Target: {{inputs.target_persona}}

        Include:
        - Subject line (A variant)
        - Subject line (B variant for A/B testing)
        - Preview text (90 chars)
        - Full email body in HTML-ready Markdown
        - Personalization tokens: {{first_name}}, {{company}}, {{trial_days_left}}
        - Clear CTA button text
      temperature: 0.6
      max_tokens: 1500

  # Generate campaign report
  - id: campaign_report
    depends_on: [write_emails, strategy]
    with:
      emails: $write_emails
      plan: $strategy
    infer:
      prompt: |
        Create a campaign brief document:
        Strategy: {{with.plan}}
        Emails: {{with.emails | first(4000)}}

        Include: campaign overview, email sequence timeline, KPIs to track,
        A/B testing recommendations, and suggested audience segments.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: campaign-brief.md
```

**Explanation:**

This workflow uses `context.files` to load a brand guide that persists across all tasks. The `{{context.files.brand}}` template injects the brand voice into multiple prompts without re-reading the file. The `for_each:` block generates all five emails in parallel with `concurrency: 3`, and each email uses the campaign strategy from the `with:` binding.

**Expected Output:** Campaign strategy JSON, five email drafts with A/B subject lines, and a comprehensive campaign brief.

---

## Recipe 4: Product Description Factory

**Problem:** You need to generate product descriptions for an e-commerce catalog, each optimized for different channels (web, mobile, marketplace).

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: product-description-factory
description: "Generate multi-channel product descriptions from raw specs"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/product-descriptions

tasks:
  # Simulate product data (in production, this would be a fetch: from your API)
  - id: product_data
    exec:
      command: |
        echo '[
          {"sku": "NK-001", "name": "Nika Pro License", "category": "software", "features": ["unlimited workflows", "team collaboration", "priority support", "custom MCP servers"], "price": 49.99},
          {"sku": "NK-002", "name": "Nika Enterprise", "category": "software", "features": ["SSO", "audit logs", "SLA guarantee", "dedicated instance", "custom integrations"], "price": 199.99},
          {"sku": "NK-003", "name": "Nika Starter Kit", "category": "bundle", "features": ["10 workflow templates", "getting started guide", "community access"], "price": 0}
        ]'
      shell: true

  # Generate descriptions per product per channel
  - id: generate_descriptions
    depends_on: [product_data]
    with:
      products: $product_data
    for_each:
      items:
        - { channel: "website", max_length: 500, format: "markdown with bullet points" }
        - { channel: "mobile_app", max_length: 200, format: "short paragraphs, no markdown" }
        - { channel: "marketplace", max_length: 300, format: "plain text with key specs" }
      as: channel
      concurrency: 3
    infer:
      system: "You are a product copywriter specializing in developer tools."
      prompt: |
        Generate {{with.channel.channel}} descriptions for these products:
        {{with.products}}

        Channel: {{with.channel.channel}}
        Max length per description: {{with.channel.max_length}} words
        Format: {{with.channel.format}}

        For each product include: headline, description, key differentiator, CTA.
      temperature: 0.5
      max_tokens: 2000
    artifact:
      path: "descriptions-{{with.channel.channel}}.md"

  # Quality check
  - id: quality_review
    depends_on: [generate_descriptions]
    with:
      descriptions: $generate_descriptions
    infer:
      prompt: |
        Review these product descriptions for quality:
        {{with.descriptions}}

        Check for:
        1. Consistency across channels
        2. Feature accuracy
        3. Tone alignment
        4. Length compliance
        5. CTA effectiveness

        Score each channel 1-10 and suggest improvements.
      temperature: 0.2
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          overall_score:
            type: integer
          channel_scores:
            type: array
            items:
              type: object
              properties:
                channel:
                  type: string
                score:
                  type: integer
                issues:
                  type: array
                  items:
                    type: string
              required: [channel, score]
        required: [overall_score, channel_scores]
    artifact:
      path: quality-review.json
      format: json
```

**Explanation:**

The `exec:` verb produces structured JSON product data (simulating an API response). The `for_each:` iterates over three marketing channels, generating tailored descriptions for each. The final quality review task uses `structured:` output to enforce a consistent scoring schema.

**Expected Output:** Three channel-specific description files plus a JSON quality review with scores.

---

## Recipe 5: SEO Content Cluster Generator

**Problem:** You need to generate a complete SEO content cluster -- a pillar page plus supporting articles, all interlinked and optimized for search.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: seo-content-cluster
description: "Generate a pillar page with supporting articles for topic authority"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  pillar_topic: "workflow automation"
  cluster_size: 5
  target_domain: "nika.dev"

artifacts:
  dir: ./output/seo-cluster

tasks:
  # Research competitor content and keywords
  - id: competitor_research
    for_each:
      items:
        - "https://zapier.com/blog/"
        - "https://n8n.io/blog/"
        - "https://www.make.com/en/blog"
      as: competitor_url
      concurrency: 3
    fetch:
      url: "{{with.competitor_url}}"
      extract: metadata
      timeout: 20

  # Generate cluster strategy
  - id: cluster_strategy
    depends_on: [competitor_research]
    with:
      competitors: $competitor_research
    infer:
      system: "You are an SEO content strategist with 10+ years of experience."
      prompt: |
        Analyze these competitor metadata for the topic "{{inputs.pillar_topic}}":
        {{with.competitors}}

        Design a content cluster with:
        1. Pillar page topic and angle (differentiated from competitors)
        2. {{inputs.cluster_size}} supporting article topics
        3. Internal linking structure
        4. Target keywords per article (primary + 3 secondary)
        5. Content gaps the competitors miss
      response_format: json
      temperature: 0.4
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          pillar:
            type: object
            properties:
              title:
                type: string
              primary_keyword:
                type: string
            required: [title, primary_keyword]
          supporting_articles:
            type: array
            items:
              type: object
              properties:
                title:
                  type: string
                primary_keyword:
                  type: string
                link_to_pillar:
                  type: string
              required: [title, primary_keyword]
        required: [pillar, supporting_articles]
    artifact:
      path: cluster-strategy.json
      format: json

  # Write the pillar page
  - id: pillar_page
    depends_on: [cluster_strategy]
    with:
      strategy: $cluster_strategy
    infer:
      system: |
        You are a senior content writer for {{inputs.target_domain}}.
        Write comprehensive, authoritative content that demonstrates E-E-A-T.
      prompt: |
        Write the pillar page based on this strategy:
        {{with.strategy}}

        Requirements:
        - 2000+ words
        - Include an FAQ section (5 questions)
        - Add internal link placeholders to supporting articles: [LINK:article-title]
        - Use proper heading hierarchy (H2, H3)
        - Include a table of contents
        - Add schema.org FAQ markup suggestions at the end
      temperature: 0.5
      max_tokens: 5000
    artifact:
      path: pillar-page.md

  # Write supporting articles
  - id: supporting_articles
    depends_on: [cluster_strategy]
    with:
      strategy: $cluster_strategy
    for_each:
      items:
        - { index: 0 }
        - { index: 1 }
        - { index: 2 }
        - { index: 3 }
        - { index: 4 }
      as: article
      concurrency: 3
    infer:
      system: |
        You are a content writer for {{inputs.target_domain}}.
        Each article should link back to the pillar page on "{{inputs.pillar_topic}}".
      prompt: |
        Write supporting article #{{with.article.index}} from this cluster strategy:
        {{with.strategy | first(2000)}}

        Requirements:
        - 800-1200 words
        - Include [LINK:pillar] placeholder linking back to the main guide
        - Focus on the specific keyword for this article
        - Include 2-3 practical examples
        - End with a section linking to the pillar page
      temperature: 0.6
      max_tokens: 3000

  # Generate linking report
  - id: linking_report
    depends_on: [pillar_page, supporting_articles]
    with:
      pillar: $pillar_page
      articles: $supporting_articles
    infer:
      prompt: |
        Analyze the internal linking structure:
        Pillar: {{with.pillar | first(1000)}}
        Articles: {{with.articles | first(3000)}}

        Generate a linking map with: source page, anchor text, target page, link type.
      response_format: json
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: linking-report.json
      format: json
```

**Explanation:**

This workflow demonstrates the full SEO content production process:

1. **Competitor research** (`for_each` + `fetch: metadata`): Scrapes competitor metadata in parallel to identify content gaps.
2. **Strategy** (`infer:` + `structured:`): Creates a validated cluster plan with pillar + supporting article topics.
3. **Pillar page** (`infer:`): Generates a comprehensive 2000+ word anchor page.
4. **Supporting articles** (`for_each` + `infer:`): Writes 5 supporting articles concurrently.
5. **Linking report** (`infer:` + bindings): Maps all internal links for implementation.

**Expected Output:** A complete content cluster: strategy JSON, pillar page markdown, supporting articles, and a linking report.

---

## Recipe 6: Multi-Language Content Translation Pipeline

**Problem:** You need to translate content into multiple languages while preserving technical terminology, formatting, and brand voice.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: translation-pipeline
description: "Translate content to multiple languages with quality validation"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  source_language: "English"

context:
  files:
    glossary: ./context/technical-glossary.md

artifacts:
  dir: ./output/translations

tasks:
  # Source content
  - id: source_content
    exec: |
      echo '# Getting Started with Nika

      Nika is a semantic YAML workflow engine for AI tasks.
      Use the `infer:` verb for LLM generation and `fetch:` for HTTP requests.

      ## Installation

      ```bash
      cargo install nika
      ```

      ## Your First Workflow

      Create a file called `hello.nika.yaml` and run it with `nika run hello.nika.yaml`.'

  # Translate to all target languages in parallel
  - id: translate
    depends_on: [source_content]
    with:
      source: $source_content
    for_each:
      items:
        - { code: "fr", name: "French", notes: "Use vous (formal). Keep CLI commands in English." }
        - { code: "es", name: "Spanish", notes: "Use Latin American Spanish. Keep CLI commands in English." }
        - { code: "de", name: "German", notes: "Use Sie (formal). Keep CLI commands in English." }
        - { code: "ja", name: "Japanese", notes: "Use desu/masu form. Keep CLI commands in English." }
        - { code: "zh", name: "Chinese (Simplified)", notes: "Use simplified characters. Keep CLI commands in English." }
      as: lang
      concurrency: 5
    infer:
      system: |
        You are a professional technical translator specializing in developer documentation.
        Technical glossary: {{context.files.glossary | first(1000)}}
      prompt: |
        Translate this content from {{inputs.source_language}} to {{with.lang.name}}.

        Translation notes: {{with.lang.notes}}

        Rules:
        - Keep all code blocks, commands, and file names in English
        - Keep Markdown formatting intact
        - Translate comments within code blocks
        - Use the glossary for consistent terminology
        - Preserve heading hierarchy

        Source content:
        {{with.source}}
      temperature: 0.2
      max_tokens: 3000
    artifact:
      path: "content-{{with.lang.code}}.md"

  # Quality validation for each translation
  - id: quality_check
    depends_on: [translate, source_content]
    with:
      translations: $translate
      original: $source_content
    infer:
      prompt: |
        Validate these translations against the original:

        Original ({{inputs.source_language}}):
        {{with.original | first(1000)}}

        Translations:
        {{with.translations | first(4000)}}

        For each language, check:
        1. Completeness (all sections translated)
        2. Code block preservation
        3. Markdown formatting intact
        4. Technical accuracy
        5. Natural fluency

        Return a quality scorecard.
      response_format: json
      temperature: 0.1
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          languages:
            type: array
            items:
              type: object
              properties:
                language:
                  type: string
                score:
                  type: integer
                issues:
                  type: array
                  items:
                    type: string
              required: [language, score]
          overall_quality:
            type: string
        required: [languages, overall_quality]
    artifact:
      path: quality-report.json
      format: json
```

**Explanation:**

The `context.files.glossary` ensures consistent terminology across all translations. The `for_each:` block processes five languages in parallel with `concurrency: 5`. Each language gets its own translation notes via the structured iterator. The quality check task receives all translations through `with:` bindings and validates them against the original.

**Expected Output:** Five translated markdown files (`content-fr.md`, `content-es.md`, etc.) plus a quality scorecard JSON.

---

## Key Patterns for Content Generation

### Template Variables

Nika supports three variable namespaces:

| Pattern | Source | Example |
|---------|--------|---------|
| `{{with.alias}}` | Task output via `with:` block | `{{with.research}}` |
| `{{context.files.X}}` | File loaded at workflow start | `{{context.files.brand}}` |
| `{{inputs.param}}` | Workflow input parameter | `{{inputs.topic}}` |

### Pipe Transforms

Transform values inline with pipes:

| Transform | Purpose | Example |
|-----------|---------|---------|
| `first(N)` | Truncate to N characters | `{{with.data \| first(3000)}}` |
| `upper` | Uppercase | `{{inputs.format \| upper}}` |
| `lower` | Lowercase | `{{inputs.topic \| lower}}` |
| `trim` | Strip whitespace | `{{with.text \| trim}}` |
| `length` | Count items/chars | `{{with.items \| length}}` |
| `join(S)` | Join array with separator | `{{with.tags \| join(", ")}}` |
| `default(V)` | Fallback value | `{{with.name \| default("Unknown")}}` |

### Structured Output

Force the LLM to return validated JSON:

```yaml
structured:
  schema:
    type: object
    properties:
      title:
        type: string
      score:
        type: integer
        minimum: 1
        maximum: 10
    required: [title, score]
```

### Parallel Generation

Use `for_each:` with `concurrency:` for batch content:

```yaml
for_each:
  - { topic: "AI", tone: "technical" }
  - { topic: "Cloud", tone: "strategic" }
as: brief
concurrency: 3
```

### Context Files

Load brand voice, glossaries, or templates once and reference everywhere:

```yaml
context:
  files:
    brand: ./brand-guide.md
    glossary: ./terms.md

# In any task prompt:
prompt: "Follow: {{context.files.brand}}"
```
