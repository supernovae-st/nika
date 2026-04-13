# Showcase Workflows

Nika ships with a library of 115 showcase workflows covering a wide range of use cases. These are ready-to-run examples that demonstrate patterns, techniques, and real-world applications. You can browse them, extract individual workflows to your project, or extract the entire collection at once.

## Browsing Showcases

### List All Showcases

```bash
nika showcase list
```

Output:

```
  Nika Showcase Workflows (115 workflows)

  -- course/builtin --
    progress-tracker                Tracking progress with logging tools
    data-validator                  Validate and transform data
    ...

  -- course/llm --
    blog-post-generator             Generate blog posts with AI
    code-review-assistant           Automated code review
    ...

  -- course/exec --
    git-stats-reporter              Git repository statistics
    system-monitor                  System health monitoring
    ...

  -- init/patterns --
    01-exec                         patterns workflow
    02-fetch                        patterns workflow
    ...

  Extract: nika showcase extract <name>
  Extract all: nika showcase extract --all
```

### Filter by Category

```bash
# Show only LLM showcases
nika showcase list --category llm

# Show only builtin tool showcases
nika showcase list --category builtin

# Show only exec-based showcases
nika showcase list --category exec

# Show infrastructure showcases
nika showcase list --category infra

# Show content-related showcases
nika showcase list --category content
```

Categories span multiple sources:
- **builtin** -- Workflows using Nika's builtin tools
- **llm** -- Workflows requiring LLM providers
- **exec** -- Shell command-based workflows
- **content** -- Content creation and processing
- **system** -- System administration and monitoring
- **core** -- Core Nika patterns
- **file** -- File processing workflows
- **media** -- Media pipeline workflows
- **patterns** -- Common workflow patterns
- **advanced** -- Advanced techniques
- **infra** -- Infrastructure and DevOps
- **fetch** -- HTTP and web scraping

## Extracting Showcases

### Extract a Single Workflow

```bash
nika showcase extract blog-post-generator
```

This creates `blog-post-generator.nika.yaml` in the current directory.

### Extract to a Specific Directory

```bash
nika showcase extract blog-post-generator --output ./examples/
```

### Extract All Showcases

```bash
nika showcase extract --all
```

This creates a `nika-showcase/` directory with all workflows organized by category:

```
nika-showcase/
├── builtin/
│   ├── progress-tracker.nika.yaml
│   ├── data-validator.nika.yaml
│   └── ...
├── content/
│   ├── blog-post-generator.nika.yaml
│   └── ...
├── exec/
│   ├── git-stats-reporter.nika.yaml
│   └── ...
└── ...
```

### Extract All to a Custom Directory

```bash
nika showcase extract --all --output ./my-showcases/
```

## Showcase Sources

Showcases come from multiple sources within Nika:

| Source | Count | Description |
|--------|:-----:|-------------|
| course/builtin | ~15 | Builtin tool demonstrations |
| course/llm | ~20 | LLM-powered workflows |
| course/exec | ~20 | Shell command workflows |
| init/patterns | ~15 | Common workflow patterns |
| init/advanced | ~15 | Advanced techniques |
| init/infra | ~15 | Infrastructure workflows |
| init/fetch | ~15 | HTTP and web scraping |

Total: 115 workflows.

## Using Showcase Workflows

### 1. Find a relevant showcase

```bash
nika showcase list --category content
```

### 2. Extract it

```bash
nika showcase extract blog-post-generator
```

### 3. Validate it

```bash
nika check blog-post-generator.nika.yaml
```

### 4. Review the YAML

Open the file to understand the workflow structure, providers, and tasks.

### 5. Customize and run

Edit the workflow to match your needs (change prompts, URLs, providers) and run it:

```bash
nika run blog-post-generator.nika.yaml
```

## What Showcases Teach

Showcases are designed to be educational. Each one demonstrates specific patterns:

**Builtin showcases** demonstrate:
- Logging and event emission
- Assertions and validation
- File import and media processing
- Pipeline chaining

**LLM showcases** demonstrate:
- Prompt engineering patterns
- Multi-provider comparison
- Structured output with JSON schemas
- Vision and multimodal inputs
- Agent loops with tools

**Exec showcases** demonstrate:
- Shell scripting patterns
- System monitoring
- Git integration
- Data processing pipelines

**Fetch showcases** demonstrate:
- API integration patterns
- Web scraping with extraction modes
- RSS feed aggregation
- Binary file downloads

## LLM Badge

In the showcase list, workflows that require an LLM provider are marked with `[LLM]`:

```
    blog-post-generator             Generate blog posts with AI [LLM]
    git-stats-reporter              Git repository statistics
```

Workflows without the badge can run without any API keys -- useful for learning the basics or testing in CI/CD environments.

## Example Showcases

Here are some notable showcases you can try immediately:

### Blog Post Generator (LLM)

A complete content production pipeline that generates a blog post from a topic:

```bash
nika showcase extract blog-post-generator
nika run blog-post-generator.nika.yaml
```

This workflow:
1. Takes a topic as input
2. Researches the topic (agent or infer)
3. Creates an outline with structured output
4. Writes the full article
5. Saves the result as an artifact

### Git Stats Reporter (No API Key)

Analyze a git repository without needing any API key:

```bash
nika showcase extract git-stats-reporter
nika run git-stats-reporter.nika.yaml
```

Uses only `exec:` to run git commands and compile statistics.

### Data Validator (Builtin Tools)

Demonstrates Nika's builtin tools for data validation:

```bash
nika showcase extract data-validator
nika run data-validator.nika.yaml
```

Uses `nika:assert`, `nika:log`, and `nika:emit` to validate and track data processing.

### System Monitor (No API Key)

Monitor system health with shell commands:

```bash
nika showcase extract system-monitor
nika run system-monitor.nika.yaml
```

Runs parallel `exec:` tasks to check CPU, memory, disk, and network status.

## Building on Showcases

The recommended workflow for using showcases as starting points:

1. **Extract** -- `nika showcase extract <name>`
2. **Read** -- Open the file and study the structure
3. **Check** -- `nika check <file>.nika.yaml`
4. **Copy** -- `cp original.nika.yaml my-version.nika.yaml`
5. **Modify** -- Change prompts, URLs, providers, and parameters
6. **Validate** -- `nika check my-version.nika.yaml`
7. **Run** -- `nika run my-version.nika.yaml`

This way you always have the original showcase for reference while building your customized version.

## Tips

1. **Start with non-LLM showcases** if you do not have API keys yet
2. **Use showcases as templates** -- extract, modify, and build on them
3. **Compare similar showcases** to understand different approaches to the same problem
4. **Check before running** -- `nika check` validates without spending API credits
5. **Read the YAML carefully** -- showcases demonstrate best practices in their structure
6. **Extract all** -- Use `nika showcase extract --all` to get the complete library for offline browsing
7. **Filter by category** -- Use `--category` to find showcases relevant to your use case
