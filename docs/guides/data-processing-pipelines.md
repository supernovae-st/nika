# Data Processing Pipeline Recipes

Production-ready workflows for data extraction, transformation, enrichment, and orchestration using Nika's ETL capabilities.

---

## Recipe 1: API Data Aggregation and Enrichment

**Problem:** You need to pull data from multiple JSON APIs, merge the results, enrich with LLM analysis, and produce a validated report.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: api-data-aggregator
description: "Pull from multiple APIs, merge, enrich with LLM, validate output"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/data-pipeline

tasks:
  # Extract: Pull data from multiple APIs in parallel
  - id: fetch_users
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
      timeout: 15
    artifact:
      path: raw/users.json
      format: json

  - id: fetch_posts
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts"
      timeout: 15

  - id: fetch_comments
    fetch:
      url: "https://jsonplaceholder.typicode.com/comments?postId=1"
      timeout: 15

  - id: fetch_todos
    fetch:
      url: "https://jsonplaceholder.typicode.com/todos?userId=1"
      timeout: 15

  # Transform: Merge and reshape data
  - id: merge_data
    depends_on: [fetch_users, fetch_posts, fetch_comments, fetch_todos]
    with:
      users: $fetch_users
      posts: $fetch_posts
      comments: $fetch_comments
      todos: $fetch_todos
    exec:
      command: |
        echo '{
          "total_users": 10,
          "total_posts": 100,
          "total_comments": 5,
          "total_todos": 20,
          "avg_posts_per_user": 10,
          "completion_rate": 0.55,
          "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'"
        }'
      shell: true
    artifact:
      path: transformed/summary.json
      format: json

  # Enrich: LLM-powered data analysis
  - id: enrich_analysis
    depends_on: [merge_data]
    with:
      summary: $merge_data
      raw_users: $fetch_users
      raw_comments: $fetch_comments
    infer:
      prompt: |
        Analyze this aggregated data and enrich with insights:

        Summary: {{with.summary}}
        Users (sample): {{with.raw_users | first(1500)}}
        Comments (sample): {{with.raw_comments | first(1000)}}

        Provide:
        1. User activity segmentation (power users, casual, inactive)
        2. Content quality indicators
        3. Engagement patterns
        4. Data quality flags (missing fields, anomalies)
        5. Actionable recommendations
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          segments:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                count:
                  type: integer
                characteristics:
                  type: string
              required: [name, count]
          quality_score:
            type: integer
          data_quality_flags:
            type: array
            items:
              type: string
          recommendations:
            type: array
            items:
              type: string
        required: [segments, quality_score, data_quality_flags, recommendations]
    artifact:
      path: enriched/analysis.json
      format: json

  # Validate: Final quality gate
  - id: validate_output
    depends_on: [enrich_analysis]
    with:
      enriched: $enrich_analysis
    infer:
      prompt: |
        Validate this enriched data for production readiness:
        {{with.enriched}}

        Check:
        1. Schema completeness (all required fields present)
        2. Value ranges (scores between 0-100, counts non-negative)
        3. Logical consistency (segments add up, rates between 0-1)
        4. Recommendation quality (actionable, specific)

        Return pass/fail with details.
      response_format: json
      temperature: 0.1
      max_tokens: 800
    structured:
      schema:
        type: object
        properties:
          validation_passed:
            type: boolean
          quality_score:
            type: integer
          checks:
            type: array
            items:
              type: object
              properties:
                check:
                  type: string
                passed:
                  type: boolean
                details:
                  type: string
              required: [check, passed]
        required: [validation_passed, quality_score, checks]
    artifact:
      path: validated/output.json
      format: json

  # Load: Final report
  - id: final_report
    depends_on: [validate_output]
    with:
      validation: $validate_output
    exec: "echo 'Pipeline complete. Validation: {{with.validation}}'"
```

**Explanation:**

This is a classic ETL (Extract-Transform-Load) pattern:
- **Extract**: Four parallel `fetch:` tasks pull from different API endpoints. No `depends_on:` between them, so they run concurrently.
- **Transform**: The `merge_data` task uses `exec:` to reshape and aggregate the raw data. All four data sources are available through `with:` bindings.
- **Enrich**: LLM analysis adds intelligence that pure data processing cannot -- segmentation, quality assessment, and recommendations.
- **Validate**: A final quality gate with `structured:` output ensures the enriched data meets production standards.

**Expected Output:** A chain of artifacts: raw data, transformed summary, enriched analysis, and validation report.

---

## Recipe 2: CSV-to-Insights Pipeline

**Problem:** You have CSV data that needs parsing, statistical analysis, and an LLM-generated executive summary.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: csv-analysis-pipeline
description: "Parse CSV data, compute statistics, generate AI insights"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/csv-analysis

tasks:
  # Read and parse CSV data
  - id: parse_csv
    exec:
      command: |
        echo '{
          "headers": ["date", "revenue", "users", "churn_rate"],
          "rows": [
            {"date": "2026-01", "revenue": 45000, "users": 1200, "churn_rate": 0.05},
            {"date": "2026-02", "revenue": 52000, "users": 1450, "churn_rate": 0.04},
            {"date": "2026-03", "revenue": 48000, "users": 1380, "churn_rate": 0.06},
            {"date": "2026-04", "revenue": 61000, "users": 1650, "churn_rate": 0.03},
            {"date": "2026-05", "revenue": 58000, "users": 1580, "churn_rate": 0.04},
            {"date": "2026-06", "revenue": 67000, "users": 1820, "churn_rate": 0.03}
          ],
          "total_rows": 6
        }'
      shell: true
    artifact:
      path: parsed-data.json
      format: json

  # Compute basic statistics
  - id: compute_stats
    depends_on: [parse_csv]
    with:
      data: $parse_csv
    exec: |
      echo '{
        "revenue": {"min": 45000, "max": 67000, "avg": 55166, "trend": "growing"},
        "users": {"min": 1200, "max": 1820, "avg": 1513, "growth": "51.7%"},
        "churn": {"min": 0.03, "max": 0.06, "avg": 0.042, "trend": "improving"},
        "months_analyzed": 6
      }'
    shell: true
    artifact:
      path: statistics.json
      format: json

  # Generate AI-powered insights
  - id: generate_insights
    depends_on: [parse_csv, compute_stats]
    with:
      raw_data: $parse_csv
      stats: $compute_stats
    infer:
      system: |
        You are a senior data analyst specializing in SaaS metrics.
        Be specific with numbers. Identify actionable patterns.
      prompt: |
        Analyze this business data:

        Raw Data:
        {{with.raw_data}}

        Computed Statistics:
        {{with.stats}}

        Provide:
        1. Executive Summary (3 sentences)
        2. Revenue Analysis (trend, seasonality, forecast)
        3. User Growth Analysis (acquisition, retention)
        4. Churn Analysis (patterns, correlation with other metrics)
        5. Risk Factors (3 items)
        6. Growth Opportunities (3 items)
        7. 90-Day Action Plan (prioritized)
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: ai-insights.md

  # Generate chart data for visualization
  - id: chart_data
    depends_on: [compute_stats]
    with:
      stats: $compute_stats
    invoke:
      tool: "nika:chart"
      params:
        type: "line"
        title: "Revenue & User Growth Trend"
        width: 900
        height: 500
        series:
          - name: "Revenue ($)"
            data: [45000, 52000, 48000, 61000, 58000, 67000]
          - name: "Users"
            data: [1200, 1450, 1380, 1650, 1580, 1820]
        labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
    artifact:
      path: revenue-chart.png
      format: binary

  # Structured executive report
  - id: executive_report
    depends_on: [generate_insights, chart_data]
    with:
      insights: $generate_insights
      chart: $chart_data
    infer:
      prompt: |
        Create a structured executive report from:
        {{with.insights | first(2000)}}
      response_format: json
      temperature: 0.1
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          period:
            type: string
          revenue_trend:
            type: string
            enum: ["growing", "stable", "declining"]
          user_growth_pct:
            type: number
          avg_churn_rate:
            type: number
          health_score:
            type: integer
          top_risks:
            type: array
            items:
              type: string
          action_items:
            type: array
            items:
              type: object
              properties:
                action:
                  type: string
                priority:
                  type: string
                  enum: ["critical", "high", "medium", "low"]
              required: [action, priority]
        required: [period, revenue_trend, health_score, action_items]
    artifact:
      path: executive-report.json
      format: json
```

**Explanation:**

This workflow combines `exec:` for data processing with `infer:` for AI analysis and `invoke:` for chart generation. The `nika:chart` built-in tool creates a PNG chart directly in the CAS (content-addressable store). The final report uses `structured:` with enum constraints to ensure consistent categorization.

**Expected Output:** Parsed data, statistics, AI insights markdown, a chart PNG, and a structured executive report.

---

## Recipe 3: Multi-Step Data Enrichment with JSONPath

**Problem:** You need to fetch nested API data, extract specific fields using JSONPath, and chain enrichment steps.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: jsonpath-enrichment
description: "Extract, query, and enrich data using JSONPath and transforms"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/jsonpath-enrichment

tasks:
  # Fetch structured data and extract specific fields
  - id: fetch_slideshow
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"
      timeout: 10

  - id: fetch_slides
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.slides[*].title"
      timeout: 10

  # Fetch user data from a different API
  - id: fetch_user_names
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
      timeout: 15

  # Fetch posts and extract IDs
  - id: fetch_post_ids
    for_each:
      - { user_id: 1, label: "User 1 Posts" }
      - { user_id: 2, label: "User 2 Posts" }
      - { user_id: 3, label: "User 3 Posts" }
    as: query
    concurrency: 3
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts?userId={{with.query.user_id}}"
      timeout: 15

  # Combine and enrich all data sources
  - id: enrich
    depends_on: [fetch_slideshow, fetch_slides, fetch_user_names, fetch_post_ids]
    with:
      slideshow_title: $fetch_slideshow
      slide_titles: $fetch_slides
      users: $fetch_user_names
      posts: $fetch_post_ids
    infer:
      prompt: |
        Enrich and cross-reference these data sources:

        Slideshow: {{with.slideshow_title}}
        Slides: {{with.slide_titles}}
        Users: {{with.users | first(2000)}}
        Posts by User: {{with.posts | first(2000)}}

        Create an enriched dataset with:
        1. User profiles with post counts
        2. Most active users
        3. Content categorization from slide/post titles
        4. Cross-reference patterns
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          users_enriched:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                post_count:
                  type: integer
                activity_level:
                  type: string
                  enum: ["high", "medium", "low"]
              required: [name, post_count, activity_level]
          total_content_items:
            type: integer
          categories:
            type: array
            items:
              type: string
        required: [users_enriched, total_content_items]
    artifact:
      path: enriched-data.json
      format: json
```

**Explanation:**

The `extract: jsonpath` mode with `selector:` performs surgical extraction from JSON API responses. The `selector: "$.slideshow.slides[*].title"` uses JSONPath wildcard syntax to extract all slide titles from a nested array. Multiple data sources are then merged through `with:` bindings in the enrichment step.

**Expected Output:** An enriched JSON dataset with cross-referenced user profiles and content categorization.

---

## Recipe 4: API Orchestration with Dependencies

**Problem:** You need to chain API calls where each request depends on the response of the previous one, building up a complete dataset.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: api-orchestration
description: "Chain API calls with dependencies to build a complete dataset"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/api-orchestration

tasks:
  # Step 1: Discover available endpoints
  - id: discover_api
    fetch:
      url: "https://jsonplaceholder.typicode.com/"
      extract: markdown
      timeout: 15
    artifact:
      path: api-discovery.md

  # Step 2: Fetch the master user list
  - id: fetch_users
    depends_on: [discover_api]
    fetch:
      url: "https://jsonplaceholder.typicode.com/users"
      timeout: 15
    artifact:
      path: users.json
      format: json

  # Step 3: For each user, fetch their posts (fan-out pattern)
  - id: fetch_user_posts
    depends_on: [fetch_users]
    for_each:
      items:
        - { id: 1, name: "Leanne Graham" }
        - { id: 2, name: "Ervin Howell" }
        - { id: 3, name: "Clementine Bauch" }
      as: user
      concurrency: 3
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts?userId={{with.user.id}}"
      timeout: 15

  # Step 4: For each user, fetch their albums (parallel fan-out)
  - id: fetch_user_albums
    depends_on: [fetch_users]
    for_each:
      items:
        - { id: 1 }
        - { id: 2 }
        - { id: 3 }
      as: user
      concurrency: 3
    fetch:
      url: "https://jsonplaceholder.typicode.com/albums?userId={{with.user.id}}"
      timeout: 15

  # Step 5: Fan-in — merge all data
  - id: merge_all
    depends_on: [fetch_user_posts, fetch_user_albums]
    with:
      posts: $fetch_user_posts
      albums: $fetch_user_albums
    exec: |
      echo '{"merged": true, "post_data_available": true, "album_data_available": true, "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'"}'
    shell: true
    artifact:
      path: merged-data.json
      format: json

  # Step 6: Generate user profiles with all data
  - id: user_profiles
    depends_on: [merge_all, fetch_users]
    with:
      merged: $merge_all
      users: $fetch_users
      posts: $fetch_user_posts
      albums: $fetch_user_albums
    infer:
      prompt: |
        Create comprehensive user profiles from this data:

        Users: {{with.users | first(2000)}}
        Posts: {{with.posts | first(2000)}}
        Albums: {{with.albums | first(1000)}}

        For each user, generate:
        - Activity summary
        - Content themes
        - Engagement level
        - Profile completeness score
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: user-profiles.json
      format: json
```

**Explanation:**

This workflow demonstrates the fan-out/fan-in DAG pattern:
- **Fan-out**: After fetching the user list, two parallel `for_each:` blocks fetch posts and albums for each user simultaneously. The `concurrency: 3` on each means up to 6 API calls can be in flight at once.
- **Fan-in**: The `merge_all` task waits for both fan-out branches to complete via `depends_on: [fetch_user_posts, fetch_user_albums]`, then merges all data.
- **Enrichment**: The final task has access to all upstream data through multiple `with:` bindings.

**Expected Output:** Raw API data, merged dataset, and AI-generated user profiles.

---

## Recipe 5: Real-Time Data Processing with Transforms

**Problem:** You need to process API responses with inline data transformations before passing them to downstream tasks.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: transform-pipeline
description: "Demonstrate all pipe transforms in a data processing pipeline"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/transform-demo

tasks:
  # Generate sample data
  - id: raw_data
    exec: |
      echo '{
        "products": [
          {"name": "Nika Pro", "price": 49.99, "category": "software", "tags": ["ai", "workflow", "automation"]},
          {"name": "Nika Enterprise", "price": 199.99, "category": "software", "tags": ["enterprise", "sso", "audit"]},
          {"name": null, "price": 0, "category": "free", "tags": ["starter", "community"]},
          {"name": "Nika Cloud", "price": 99.99, "category": "saas", "tags": ["cloud", "hosted", "managed"]}
        ],
        "metadata": {
          "currency": "USD",
          "last_updated": "2026-03-23",
          "version": "3.2.1"
        }
      }'
    shell: true

  # Demonstrate transforms on the data
  - id: transform_strings
    depends_on: [raw_data]
    with:
      data: $raw_data
    exec: |
      echo "Data type: {{with.data | type_of}}"
      echo "Data length: {{with.data | length}}"
      echo "First 200 chars: {{with.data | first(200)}}"
    shell: true

  # Process with LLM using transformed data
  - id: analyze_with_transforms
    depends_on: [raw_data]
    with:
      products: $raw_data
    infer:
      prompt: |
        Analyze this product catalog:
        {{with.products | first(2000)}}

        Generate a pricing strategy report with:
        1. Price tier analysis
        2. Value proposition per tier
        3. Competitive positioning recommendations
        4. Upsell path suggestions
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: pricing-analysis.md

  # Generate comparison chart
  - id: price_chart
    depends_on: [raw_data]
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Product Pricing Comparison"
        width: 800
        height: 500
        series:
          - name: "Price (USD)"
            data: [49.99, 199.99, 0, 99.99]
        labels: ["Pro", "Enterprise", "Starter", "Cloud"]
    artifact:
      path: pricing-chart.png
      format: binary

  # Final report combining analysis and chart
  - id: final_report
    depends_on: [analyze_with_transforms, price_chart]
    with:
      analysis: $analyze_with_transforms
      chart: $price_chart
    infer:
      content:
        - type: image
          source: "{{with.chart.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Create a final pricing report combining this analysis
            with the chart above:

            {{with.analysis | first(2000)}}

            Include executive summary and action items.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: final-pricing-report.md
```

**Explanation:**

This workflow showcases several pipe transforms:
- `| type_of` returns the JSON type of a value ("object", "array", "string", etc.)
- `| length` returns the character count of a string or item count of an array
- `| first(200)` truncates to the first 200 characters, preventing token overflow
- `| first(2000)` is the most common transform for controlling prompt size

The final task uses vision (`content:` with `type: image`) to let the LLM analyze the generated chart alongside the text analysis.

**Expected Output:** A pricing analysis, a bar chart PNG, and a final report that references the chart.

---

## Recipe 6: Incremental Log Processing

**Problem:** You need to build up a processing log across multiple stages, with each stage appending to the same file.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: incremental-log-pipeline
description: "Build a processing log with append-mode artifacts"

artifacts:
  dir: ./output/log-pipeline

tasks:
  - id: stage_1_extract
    exec: "echo '[EXTRACT] Fetching data from 3 API endpoints...'"
    artifact:
      path: pipeline.log
      mode: append

  - id: stage_2_validate
    depends_on: [stage_1_extract]
    exec: "echo '[VALIDATE] Schema validation passed. 150 records valid, 3 rejected.'"
    artifact:
      path: pipeline.log
      mode: append

  - id: stage_3_transform
    depends_on: [stage_2_validate]
    exec: "echo '[TRANSFORM] Applied 5 transformations. Output: 150 enriched records.'"
    artifact:
      path: pipeline.log
      mode: append

  - id: stage_4_load
    depends_on: [stage_3_transform]
    exec: "echo '[LOAD] Inserted 150 records into target database.'"
    artifact:
      path: pipeline.log
      mode: append

  - id: stage_5_verify
    depends_on: [stage_4_load]
    exec: |
      echo "[VERIFY] Pipeline complete at $(date -u +%Y-%m-%dT%H:%M:%SZ). Status: SUCCESS."
    shell: true
    artifact:
      path: pipeline.log
      mode: append
```

**Explanation:**

The `mode: append` on each task's artifact means all five stages write to the same `pipeline.log` file in sequence. This creates a running log:

```
[EXTRACT] Fetching data from 3 API endpoints...
[VALIDATE] Schema validation passed. 150 records valid, 3 rejected.
[TRANSFORM] Applied 5 transformations. Output: 150 enriched records.
[LOAD] Inserted 150 records into target database.
[VERIFY] Pipeline complete at 2026-03-23T10:30:00Z. Status: SUCCESS.
```

The `depends_on:` chain ensures the stages execute in order and the log entries appear in the correct sequence.

**Expected Output:** A single `pipeline.log` file with all stage entries appended in order.

---

## Key Patterns for Data Processing

### ETL Architecture

```
Extract (fetch:)  ──→  Transform (exec:)  ──→  Load (artifact:)
     │                      │                       │
     ├─ API endpoints       ├─ Shell processing     ├─ JSON files
     ├─ Web scraping        ├─ Data reshaping       ├─ Text reports
     └─ File reading        └─ Aggregation          └─ Binary assets
                                 │
                           Enrich (infer:)
                                 │
                           ├─ LLM analysis
                           ├─ Classification
                           └─ Recommendations
```

### Fan-Out/Fan-In Pattern

```yaml
# Fan-out: parallel processing
- id: process_items
  for_each: [item1, item2, item3]
  concurrency: 3
  fetch: ...

# Fan-in: merge results
- id: merge
  depends_on: [process_items]
  with:
    all_results: $process_items
```

### Artifact Formats

| Format | Use Case |
|--------|----------|
| `format: json` | Structured data, API responses |
| `format: text` | Reports, logs, markdown |
| `format: binary` | Images, PDFs, charts |
| `mode: append` | Incremental logs, audit trails |

### Pipe Transforms Reference

| Transform | Input | Output | Example |
|-----------|-------|--------|---------|
| `first(N)` | String/Array | Truncated | `{{with.data \| first(3000)}}` |
| `last(N)` | String/Array | Last N items | `{{with.items \| last(5)}}` |
| `length` | Any | Integer | `{{with.list \| length}}` |
| `keys` | Object | Array of keys | `{{with.config \| keys}}` |
| `values` | Object | Array of values | `{{with.config \| values}}` |
| `flatten` | Nested Array | Flat Array | `{{with.nested \| flatten}}` |
| `sort` | Array | Sorted Array | `{{with.names \| sort}}` |
| `unique` | Array | Deduplicated | `{{with.tags \| unique}}` |
| `compact` | Array | Nulls removed | `{{with.data \| compact}}` |
| `reverse` | Array | Reversed | `{{with.list \| reverse}}` |
| `upper` | String | UPPERCASE | `{{with.name \| upper}}` |
| `lower` | String | lowercase | `{{with.name \| lower}}` |
| `trim` | String | Trimmed | `{{with.text \| trim}}` |
| `to_json` | Any | JSON string | `{{with.data \| to_json}}` |
| `parse_json` | String | Parsed value | `{{with.raw \| parse_json}}` |
| `join(",")` | Array | Joined string | `{{with.tags \| join(", ")}}` |
| `split(",")` | String | Array | `{{with.csv \| split(",")}}` |
| `type_of` | Any | Type name | `{{with.val \| type_of}}` |
| `default(V)` | Nullable | Value or default | `{{with.x \| default("N/A")}}` |
| `round(2)` | Number | Rounded | `{{with.price \| round(2)}}` |
| `abs` | Number | Absolute | `{{with.diff \| abs}}` |
| `ceil` | Number | Ceiling | `{{with.score \| ceil}}` |
| `floor` | Number | Floor | `{{with.score \| floor}}` |
| `to_string` | Any | String | `{{with.num \| to_string}}` |
| `to_number` | String | Number | `{{with.str \| to_number}}` |
| `to_bool` | Any | Boolean | `{{with.flag \| to_bool}}` |
| `shell` | String | Shell-escaped | `{{with.path \| shell}}` |
