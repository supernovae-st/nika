# Research Report: MCP Ecosystem State -- March 2026

## Summary

The Model Context Protocol ecosystem has matured dramatically since its November 2024 launch. As of March 2026, MCP has an official registry with hundreds of servers, four spec revisions (2024-11-05, 2025-03-26, 2025-06-18, 2025-11-25), SDKs in 10 languages, and native integration in virtually every major AI development platform. Google's A2A protocol has emerged as the complementary standard for agent-to-agent communication, while MCP handles agent-to-tool connections.

## 1. Major MCP Servers by Category

### Image Generation

| Server | Description | URL |
|--------|-------------|-----|
| **Cloudinary** (official) | Media upload, transformation, AI analysis, optimization, delivery | https://github.com/cloudinary/mcp-servers |
| **Fal.ai** | FLUX, Stable Diffusion, MusicGen models | https://github.com/raveenb/fal-mcp-server |
| **Replicate** | Search, run, manage ML models (incl. image gen) | https://github.com/deepfates/mcp-replicate |
| **OpenAI GPT Image** | GPT image generation/editing | https://github.com/SureScaleAI/openai-gpt-image-mcp |
| **Azure OpenAI DALL-E 3** | Azure-hosted DALL-E 3 | https://github.com/jacwu/mcp-server-aoai-dalle3 |
| **Pixelle MCP** | ComfyUI workflows as MCP tools, zero code, omnimodal (text/image/video/audio) | https://github.com/AIDC-AI/Pixelle-MCP |
| **Nanana** | Text-to-image / image-to-image via Google Gemini | https://github.com/nanana-app/mcp-server-nano-banana |
| **EverArt** (archived reference) | AI image generation using various models | https://github.com/modelcontextprotocol/servers-archived/tree/main/src/everart |
| **WaveSpeed** | AI image/video generation | https://github.com/WaveSpeedAI/mcp-server |

### SEO

| Server | Description | URL |
|--------|-------------|-----|
| **fetchSERP** (official) | All-in-One SEO & Web Intelligence toolkit | https://github.com/fetchSERP/fetchserp-mcp-server-node |
| **Keywords Everywhere** (official) | Keyword research MCP integration | https://api.keywordseverywhere.com/docs/#/mcp_integration |
| **kwrds.ai** | Keyword research, People Also Ask, SERP tools | https://github.com/mkotsollaris/kwrds_ai_mcp |
| **SEO MCP** | Free SEO tools based on Ahrefs data (backlinks, keyword ideas) | https://github.com/cnych/seo-mcp |
| **Exa** | Search engine made for AIs | https://github.com/exa-labs/exa-mcp-server |
| **Tavily** | Search engine for AI agents (search + extract) | https://github.com/tavily-ai/tavily-mcp |
| **Google Analytics** | 200+ dimensions & metrics for LLM analysis | https://github.com/surendranb/google-analytics-mcp |
| **Google Analytics 4** | GA Data API + Measurement Protocol | https://github.com/gomakers-ai/mcp-google-analytics |
| **AdAdvisor** | Meta Ads performance data | https://www.adadvisor.ai |

### Data Analysis

| Server | Description | URL |
|--------|-------------|-----|
| **Snowflake** (official, Snowflake-Labs) | Cortex Agents, structured & unstructured data | https://github.com/Snowflake-Labs/mcp |
| **Databricks** (official) | Turnkey managed MCP servers within Databricks governance | https://docs.databricks.com/aws/en/generative-ai/mcp/ |
| **ClickHouse** (official) | Query ClickHouse databases | https://github.com/ClickHouse/mcp-clickhouse |
| **Apache Doris** | MPP-based real-time data warehouse | https://github.com/apache/doris-mcp-server |
| **BigQuery** (multiple) | Google BigQuery integration | https://github.com/LucasHild/mcp-server-bigquery |
| **MCP Toolbox for Databases** (Google) | Database toolbox for AI agents | https://github.com/googleapis/mcp-toolbox-for-databases (unofficial path) |
| **Jupyter MCP Server** | Real-time Jupyter Notebook interaction for data analysis | https://github.com/datalayer/jupyter-mcp-server |
| **Alkemi** | Query Snowflake, BigQuery, DataBricks Data Products | https://github.com/alkemi-ai/alkemi-mcp |
| **Tinybird** | Serverless ClickHouse analytics | https://github.com/tinybirdco/mcp-tinybird |
| **PostHog** | Product analytics, feature flags, error tracking | https://github.com/posthog/mcp |
| **Mixpanel** (official) | Query and analyze product data | https://docs.mixpanel.com/docs/features/mcp |
| **AutoML** | End-to-end data science workflows | https://github.com/emircansoftware/MCP_Server_DataScience |

### Code Execution / Sandboxing

| Server | Description | URL |
|--------|-------------|-----|
| **E2B** (official) | Secure cloud sandboxes | https://github.com/e2b-dev/mcp-server |
| **Daytona** (official) | Secure AI code execution sandboxes | https://github.com/daytonaio/daytona/tree/main/apps/cli/mcp |
| **Riza** (official) | Arbitrary code execution platform for LLMs | https://github.com/riza-io/riza-mcp |
| **ForeverVM** (official) | Python in a persistent code sandbox | https://github.com/jamsocket/forevervm/tree/main/javascript/mcp-server |
| **HOPX** (official) | Execute Python, JavaScript, Bash, Go in isolated cloud containers | https://hopx.ai |
| **YepCode** (official) | Run code snippets, build automation tools | https://github.com/yepcode/yepcode-mcp-server |
| **pydantic-ai/mcp-run-python** | Secure Python sandbox via Deno + Pyodide | https://github.com/pydantic/pydantic-ai/tree/main/mcp-run-python |
| **code-sandbox-mcp** | Docker-based secure code sandbox | https://github.com/Automata-Labs-team/code-sandbox-mcp |
| **ipybox** | IPython + Docker stateful execution | https://github.com/gradion-ai/ipybox |

### Multimodal (Audio / Video / Voice)

| Server | Description | URL |
|--------|-------------|-----|
| **Cartesia** (official) | Voice platform: TTS, voice cloning | https://github.com/cartesia-ai/cartesia-mcp |
| **AllVoiceLab** (official) | TTS, voice cloning, video translation | https://www.allvoicelab.com/mcp |
| **ElevenLabs** | Text-to-speech with multiple voices | https://github.com/mamertofabian/elevenlabs-mcp-server |
| **Plainly Videos** (official) | Video generation from templates | https://github.com/plainly-videos/mcp-server |
| **VideoDB Director** | Video management agent toolkit | https://github.com/video-db/agent-toolkit |
| **VisionAgent MCP** (LandingAI, official) | Visual AI analysis | https://github.com/landing-ai/vision-agent |
| **Tencent RTC** (official) | Real-time audio/video communication | https://github.com/Tencent-RTC/mcp |
| **Fish Audio** | TTS with multiple voices, streaming, real-time playback | https://github.com/da-okazaki/mcp-fish-audio-server |
| **Kokoro TTS** | Text-to-speech to MP3 with S3 upload | https://github.com/mberg/kokoro-tts-mcp |
| **DaVinci Resolve** | Video editing, color grading, media management | https://github.com/samuelgursky/davinci-resolve-mcp |
| **HuggingFace Spaces** | Open source image, audio, text models | https://github.com/evalstate/mcp-hfspace |
| **Fal.ai** | Images, videos, and music (FLUX, SD, MusicGen) | https://github.com/raveenb/fal-mcp-server |

---

## 2. MCP Spec Evolution Since Late 2024

There have been **four published spec versions** plus an active draft:

### Spec Version Timeline

| Version | Date | Status |
|---------|------|--------|
| `2024-11-05` | Nov 2024 | Original release |
| `2025-03-26` | Mar 2025 | Major: Auth, Streamable HTTP, audio |
| `2025-06-18` | Jun 2025 | Major: Elicitation, structured output, security |
| `2025-11-25` | Nov 2025 | Major: Tasks, icons, governance |
| `draft` | Ongoing | Next version in development |

### 2025-03-26: OAuth, Streaming, Audio

**Major additions:**
- **OAuth 2.1 authorization framework** -- Comprehensive auth based on OAuth 2.1, enabling protected MCP servers with proper token-based access
- **Streamable HTTP transport** -- Replaced the old HTTP+SSE transport with a more flexible Streamable HTTP transport
- **JSON-RPC batching** -- Support for batching multiple requests (later removed in 2025-06-18)
- **Tool annotations** -- Metadata describing whether tools are read-only or destructive
- **Audio content type** -- First-class audio data alongside existing text and image types
- **Progress notification messages** -- Descriptive status updates via `message` field
- **Completions capability** -- Explicit support for argument autocompletion

Source: https://github.com/modelcontextprotocol/modelcontextprotocol/tree/main/docs/specification/2025-03-26/changelog.mdx

### 2025-06-18: Elicitation, Structured Output, Security Hardening

**Major additions:**
- **Elicitation** -- Servers can now request additional information from users mid-interaction, with JSON Schema validation. Enables interactive workflows
- **Structured tool output** -- Tools can return structured content (not just text/images)
- **Resource links in tool results** -- Tool call results can reference resources
- **OAuth resource server classification** -- MCP servers classified as OAuth Resource Servers with protected resource metadata discovery
- **Resource Indicators (RFC 8707)** -- Required for MCP clients to prevent malicious token theft
- **Security best practices page** -- New dedicated security guidance
- **`title` field** -- Human-friendly display names separate from programmatic `name`
- **`_meta` field** -- Added to additional interface types
- **Protocol version header** -- Negotiated version must be sent via `MCP-Protocol-Version` header
- Removed JSON-RPC batching (added in previous version)

Source: https://github.com/modelcontextprotocol/modelcontextprotocol/tree/main/docs/specification/2025-06-18/changelog.mdx

### 2025-11-25: Tasks, Icons, Governance (Current)

**Major additions:**
- **Tasks (experimental)** -- Durable state machines for tracking expensive/async operations. Support polling and deferred result retrieval. Tools can declare `execution.taskSupport` as `"required"`, `"optional"`, or `"forbidden"`
- **Icons** -- Servers can expose icons for tools, resources, resource templates, and prompts
- **OpenID Connect Discovery** -- Enhanced authorization server discovery
- **Incremental scope consent** -- Via `WWW-Authenticate` for authorization flows
- **URL mode elicitation** -- Servers can request URLs from users
- **Tool calling in sampling** -- `tools` and `toolChoice` parameters added to sampling
- **OAuth Client ID Metadata Documents** -- Recommended client registration mechanism
- **Tool name guidance** -- Formal guidance on tool naming conventions
- **Enhanced enum schemas** -- Titled, untitled, single-select, multi-select enums for elicitation
- **Polling SSE streams** -- Servers can disconnect at will, clients can resume via GET

**Governance changes:**
- Formalized governance structure
- Established Working Groups and Interest Groups
- SDK tiering system with feature support requirements
- Shared communication practices

Source: https://github.com/modelcontextprotocol/modelcontextprotocol/tree/main/docs/specification/2025-11-25/changelog.mdx

---

## 3. Platform Integrations

### Confirmed MCP Host Support (from spec and ecosystem data)

| Platform | MCP Support | Details |
|----------|-------------|---------|
| **Claude Desktop** | Native, day-one | Original MCP host. Full support for tools, resources, prompts |
| **Claude Code** | Native | CLI agent with MCP support via `.mcp.json` config |
| **VS Code / GitHub Copilot** | Native | VS Code supports MCP via `mcp` section in settings. GitHub Copilot agent mode uses MCP servers. Microsoft also ships an official Azure MCP Server |
| **Cursor** | Native | MCP server configuration in settings, used for tool augmentation |
| **Windsurf (Codeium)** | Native | MCP support integrated into Cascade agent |
| **JetBrains IDEs** | Native (2025.1+) | MCP support added to IntelliJ, WebStorm, PyCharm, etc. via built-in AI Assistant |
| **Zed** | Native | MCP extension support for the Zed editor |
| **Amazon Q Developer** | MCP support | AWS ships specialized MCP servers for their services |
| **Sourcegraph Cody** | MCP support | Context fetching via MCP |

### Key Platform MCP Servers

- **Microsoft Azure MCP Server**: https://github.com/microsoft/mcp -- Azure Storage, Cosmos DB, Azure CLI, Azure DevOps
- **AWS MCP Servers**: https://github.com/awslabs/mcp -- AWS-specific best practices servers
- **Atlassian MCP**: https://www.atlassian.com/platform/remote-mcp-server -- Jira + Confluence via remote MCP
- **Cloudflare MCP**: https://github.com/cloudflare/mcp-server-cloudflare -- Workers, KV, R2, D1
- **Databricks MCP**: https://docs.databricks.com/aws/en/generative-ai/mcp/ -- Managed MCP within Databricks governance
- **Chrome DevTools MCP**: https://github.com/ChromeDevTools/chrome-devtools-mcp -- Debug web pages from AI assistants

### Official SDKs (10 languages)

| SDK | URL |
|-----|-----|
| TypeScript | https://github.com/modelcontextprotocol/typescript-sdk |
| Python | https://github.com/modelcontextprotocol/python-sdk |
| Rust | https://github.com/modelcontextprotocol/rust-sdk |
| Go | https://github.com/modelcontextprotocol/go-sdk |
| Java | https://github.com/modelcontextprotocol/java-sdk |
| Kotlin | https://github.com/modelcontextprotocol/kotlin-sdk |
| C# | https://github.com/modelcontextprotocol/csharp-sdk |
| Swift | https://github.com/modelcontextprotocol/swift-sdk |
| Ruby | https://github.com/modelcontextprotocol/ruby-sdk |
| PHP | https://github.com/modelcontextprotocol/php-sdk |

---

## 4. Google A2A Protocol vs MCP

### What is A2A?

The **Agent2Agent (A2A) Protocol** is an open protocol (now under the Linux Foundation, contributed by Google) that enables communication between **opaque agentic applications** -- agents that collaborate without exposing their internal state, memory, or tools.

- Repository: https://github.com/a2aproject/A2A
- Website: https://a2a-protocol.org
- License: Apache 2.0

### A2A Key Features

- **Agent Discovery** via "Agent Cards" (capability descriptions)
- **JSON-RPC 2.0 over HTTP(S)** (same base transport as MCP)
- **Flexible interaction**: synchronous request/response, SSE streaming, async push notifications
- **Rich data exchange**: text, files, structured JSON
- **Enterprise-ready**: security, auth, observability built-in
- **Framework agnostic**: works with Google ADK, LangGraph, BeeAI, CrewAI, etc.

### A2A SDKs

| SDK | Package |
|-----|---------|
| Python | `pip install a2a-sdk` |
| Go | `go get github.com/a2aproject/a2a-go` |
| JavaScript | `npm install @a2a-js/sdk` |
| Java | Maven |
| .NET | `dotnet add package A2A` |

### MCP vs A2A: Complementary, Not Competing

| Aspect | MCP | A2A |
|--------|-----|-----|
| **Purpose** | Agent-to-Tool connectivity | Agent-to-Agent collaboration |
| **Metaphor** | "Giving an agent hands" (tools, context) | "Agents talking to each other" |
| **Visibility** | Host sees tool inputs/outputs | Agents are opaque to each other |
| **State** | Server is stateless/thin | Agents have internal state, memory |
| **Discovery** | Registry + capability negotiation | Agent Cards |
| **Interaction** | Request/response tool calls | Long-running tasks, negotiation |
| **Transport** | stdio, Streamable HTTP | HTTP(S) with SSE/push |

**The consensus view**: MCP and A2A are complementary layers in the AI stack. MCP connects agents to tools/data (vertical integration), while A2A connects agents to other agents (horizontal integration). An agent might use MCP to access a database tool, then use A2A to delegate a subtask to a specialized agent.

The A2A README explicitly states: "Learn how A2A complements MCP by enabling agents to collaborate with each other."

There is even an **AgentTrust MCP server** in the registry that bridges both protocols: "Identity, trust, and A2A orchestration for autonomous AI agents. Official A2A partner." (https://agenttrust.ai)

---

## 5. MCP Registries and Marketplaces

### Official MCP Registry

- **URL**: https://registry.modelcontextprotocol.io
- **GitHub**: https://github.com/modelcontextprotocol/registry
- **Status**: Preview launched September 2025, API freeze (v0.1) since October 2025
- **API**: REST API at `https://registry.modelcontextprotocol.io/v0.1/servers`
- **Schema**: Servers use `server.schema.json` (currently `2025-12-11` schema version)
- **Publishing**: CLI tool `mcp-publisher` for submitting servers
- **Backend**: Go + PostgreSQL, open-source, Docker-deployable
- **Maintainers**: Anthropic (Adam Jones), PulseMCP (Tadas Antanavicius), GitHub (Toby Padilla), Stacklok (Rado Dimitrov)

### Server Discovery Mechanisms

1. **Official Registry** -- The primary, authoritative source for discovering MCP servers
2. **`modelcontextprotocol/servers` GitHub repo** -- Reference implementations + curated community list (README). NOTE: The README lists are "no longer maintained and will eventually be removed" in favor of the registry
3. **npm / PyPI / OCI registries** -- Servers are published as packages (`registryType: "npm"`, `"pypi"`, `"oci"`)
4. **MCP `.mcp.json` config files** -- Projects include `.mcp.json` for declaring server dependencies (similar to `package.json`)
5. **Third-party directories**:
   - **PulseMCP** (https://pulsemcp.com) -- Community directory
   - **MCP Servers Rating** (https://www.deepnlp.org/store/ai-agent/mcp-server) -- Rating and reviews
   - **Smithery** (https://smithery.ai) -- MCP server marketplace
   - **Glama** (https://glama.ai/mcp/servers) -- MCP server directory

### Registry Server Entry Format

Servers in the registry use a standardized JSON format:

```json
{
  "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
  "name": "com.example/my-server",
  "description": "...",
  "title": "Human-Friendly Name",
  "version": "1.0.0",
  "websiteUrl": "https://...",
  "icons": [{ "src": "https://...", "mimeType": "image/png" }],
  "repository": { "url": "https://github.com/...", "source": "github" },
  "remotes": [{ "type": "streamable-http", "url": "https://..." }],
  "packages": [{
    "registryType": "npm",
    "identifier": "@scope/package",
    "version": "1.0.0",
    "transport": { "type": "stdio" },
    "environmentVariables": [{ "name": "API_KEY", "isRequired": true, "isSecret": true }]
  }]
}
```

---

## 6. MCP for Multimodal Content

### Protocol-Level Support

The MCP spec has progressively added multimodal content types:

| Version | Content Types |
|---------|---------------|
| 2024-11-05 | Text, Image (base64/URL) |
| 2025-03-26 | + **Audio** (base64 audio data) |
| 2025-06-18 | + Structured content in tool output |
| 2025-11-25 | Same + experimental Tasks for long-running generation |

### How Multimodal Works in MCP

MCP tool results can include `content` arrays with mixed types:

```json
{
  "content": [
    { "type": "text", "text": "Generated image description" },
    { "type": "image", "data": "<base64>", "mimeType": "image/png" },
    { "type": "audio", "data": "<base64>", "mimeType": "audio/wav" }
  ]
}
```

Resources can also serve multimodal content via `BlobResourceContents`:

```json
{
  "uri": "file:///output/result.png",
  "mimeType": "image/png",
  "blob": "<base64>"
}
```

### Current Multimodal MCP Servers

**Audio/Voice**: Cartesia, AllVoiceLab, ElevenLabs, Fish Audio, Kokoro TTS, Telnyx, VoiceMode, Carbon Voice, Tencent RTC

**Video**: Plainly Videos, VideoDB Director, DaVinci Resolve, Fal.ai (video generation), Video Jungle, json2video, ZapCap, Creatify, Pixelle MCP (ComfyUI)

**Image**: Cloudinary, Fal.ai, Replicate, OpenAI GPT Image, DALL-E 3, Pixelle, WaveSpeed, VisionAgent, HuggingFace Spaces

**Multi-modal frameworks**: Pixelle MCP is notable -- it converts any ComfyUI workflow into an MCP tool with zero code, supporting "Text, Image, Video, Audio, 3D" generation.

### Gaps and Limitations

- **Streaming binary data**: Large video/audio files are still base64-encoded in JSON, which is inefficient. The Tasks system (2025-11-25) partially addresses this by allowing deferred retrieval
- **Real-time streams**: No native support for continuous audio/video streams in MCP (would need external signaling)
- **No native video content type**: Video is served as base64 blobs or URLs, not as a first-class `"type": "video"` content type

---

## Sources

1. [MCP Specification (2025-11-25, current)](https://github.com/modelcontextprotocol/modelcontextprotocol/tree/main/docs/specification/2025-11-25) -- Authoritative spec
2. [MCP Servers Repository README](https://github.com/modelcontextprotocol/servers) -- Reference servers + community list
3. [Official MCP Registry](https://registry.modelcontextprotocol.io) -- Live searchable registry
4. [MCP Registry GitHub](https://github.com/modelcontextprotocol/registry) -- Registry source and docs
5. [A2A Protocol Repository](https://github.com/a2aproject/A2A) -- Google's Agent2Agent protocol
6. [A2A Protocol Website](https://a2a-protocol.org) -- A2A documentation
7. [MCP 2025-03-26 Changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-03-26/changelog.mdx)
8. [MCP 2025-06-18 Changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-06-18/changelog.mdx)
9. [MCP 2025-11-25 Changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/changelog.mdx)

## Methodology

- Tools used: GitHub API (raw content + releases API), MCP Registry API (v0.1), direct README scraping
- Pages analyzed: ~25 (spec files, changelogs, READMEs, registry API responses)
- Data sources: All from live GitHub repositories and registry API as of March 2026

## Confidence Level

**High** for spec evolution, registry details, A2A comparison, and server listings -- these are sourced directly from the official repositories and live API responses.

**Medium** for platform integration details -- confirmed through official server repos (Microsoft, AWS, Atlassian shipping MCP servers implies deep integration), but exact IDE feature details were not verified against each platform's documentation.

## Relevance to Nika

For Nika's `invoke:` verb and MCP client implementation (via `rmcp`):

1. **Spec version**: Nika should target `2025-11-25` (current stable). Key features to consider:
   - **Tasks**: For long-running tool calls (image generation, data analysis), the experimental Tasks system enables polling instead of blocking
   - **Elicitation**: Servers can now request user input mid-workflow -- Nika's agent loop could support this
   - **Structured output**: Tools return typed data, not just text
   - **OAuth 2.1**: For remote MCP servers requiring auth

2. **Registry integration**: Nika could discover servers from `registry.modelcontextprotocol.io/v0.1/servers`

3. **A2A consideration**: If Nika agents need to communicate with external agents (not just tools), A2A is the emerging standard for that layer

4. **Multimodal**: Audio content type is now native in MCP -- relevant for QR Code AI workflows that might involve voice/audio

5. **Remote servers**: The shift from stdio to Streamable HTTP means more MCP servers are hosted remotely. Nika's MCP client should support both transports
