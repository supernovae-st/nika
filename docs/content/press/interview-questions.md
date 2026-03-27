# Prepared Interview Q&A --- Thibaut Melen, Creator of Nika

> 25 questions with detailed, quotable answers. Organized by topic.
> For journalists, podcast hosts, and conference moderators.

---

## About the Creator and Motivation

### 1. What is Nika, in one sentence?

"Nika is a semantic YAML workflow engine for AI tasks --- a single Rust binary that lets you orchestrate LLM inference, web fetching, shell commands, MCP tool calls, and autonomous agent loops with five verbs and zero dependencies."

### 2. Why did you build Nika? What problem were you trying to solve?

"I was frustrated by the gap between what AI models can do and what most people can actually make them do. Every orchestration tool I looked at required Python, Docker, a server, or some combination of all three. I wanted something closer to how we think about infrastructure-as-code: write a declarative file, run a single command, get the result. Terraform did this for cloud infrastructure. Docker Compose did it for containers. GitHub Actions did it for CI/CD. Nobody had done it for AI workflows. So I built Nika."

### 3. You built 482,000 lines of code solo. How is that possible?

"Rust makes it possible. The type system catches entire categories of bugs at compile time --- null pointer dereferences, data races, use-after-free. Problems that would take hours of debugging in a dynamic language are compile errors in Rust. I spend almost no time debugging runtime crashes. I spend my time designing types and writing tests, and the compiler does the rest. The other factor is the three-phase AST architecture: once I got Raw, Analyzed, and Lower right, adding new features is structurally guided. The types tell me where to add code."

### 4. What is your background? How did you get here?

"I am a software engineer and the founder of SuperNovae Studio. My background is in full-stack development with a growing focus on systems programming and AI infrastructure. I chose Rust because I believe the orchestration layer for AI should have the same reliability standards as operating systems and databases. The models are probabilistic; the infrastructure around them should not be."

### 5. What does the daily workflow look like for a solo developer on a project this size?

"Test-driven development, obsessively. I write the test first, then the code. Every commit follows a strict protocol: git status, diff, test, lint, type-check, commit. No exceptions. Each logical change is one commit. The codebase has over 7,700 unit tests and zero clippy warnings. The discipline is not optional when you are the only reviewer."

---

## About Technical Decisions

### 6. Why Rust instead of Python, Go, or TypeScript?

"Three reasons. First, the single binary. Rust compiles to a native executable with no runtime dependencies. Users download one file, set an API key, and they are running workflows. No pip install, no node_modules, no Docker. Second, the type system. Nika has a three-phase AST pipeline where the compiler ensures you cannot execute an unvalidated workflow. This is impossible to enforce in dynamically typed languages. Third, performance. Nika's HTTP layer can sustain 10,000+ concurrent connections. Its text processing uses SIMD acceleration. Its content hashing runs at 30+ GB/s. These are not theoretical numbers --- they are architectural consequences of choosing Rust."

### 7. Why YAML and not a custom DSL, JSON, or a programming language?

"Three criteria. First, human readability. YAML is the format that non-programmers are most likely to understand. It is also what DevOps engineers already know from Docker Compose, GitHub Actions, Kubernetes, and Terraform. Second, version control friendliness. YAML diffs are clean and readable in pull requests. Third, the infrastructure-as-code precedent. Every major IaC tool uses a declarative format. AI workflows deserve the same rigor. I considered a custom DSL but rejected it because it would require learning a new language. YAML has a learning curve of approximately zero for anyone who has written a Docker Compose file."

### 8. Why only five verbs? Is that not too limiting?

"Five verbs cover 100% of the workflows I have encountered. infer handles any LLM operation. exec handles any system command. fetch handles any HTTP interaction. invoke handles any MCP tool call, which includes 24 built-in media tools, external services, and NovaNet knowledge graph operations. agent handles any multi-turn autonomous task. If a task cannot be expressed as one of these five operations, it can be expressed as a composition of them. The constraint is deliberate --- it forces composability. Every task has exactly one verb, and the DAG handles the orchestration."

### 9. Tell me about the MCP integration. Why is that significant?

"Nika is the first non-IDE, non-assistant CLI tool to implement the Model Context Protocol. All other MCP clients are either AI assistants like Claude and ChatGPT, IDEs like Cursor and VS Code, or operating systems like Windows 11. No CLI workflow engine had done this before. The significance is that MCP is becoming the standard for agent-tool interaction --- Anthropic created it, but Google, OpenAI, Microsoft, and others have adopted it. By implementing MCP, Nika can connect to any tool ecosystem that speaks the protocol. And through the invoke verb, those tools become first-class workflow steps."

### 10. What is content-addressable storage, and why does Nika use it for media?

"CAS is a pattern where content is stored by its hash rather than by a file path. Git uses it for source code --- every commit is identified by a SHA hash. Docker uses it for container layers. Nika uses it for media assets in AI workflows. When you import an image, Nika hashes it with SHA-256 and stores it in the CAS directory. Workflow tasks reference images by hash, not by path. This gives you three things: deduplication (the same image imported twice stores only one copy), reproducibility (the hash is deterministic), and security (file paths never leak to LLM APIs --- hashes are resolved to base64 at the provider boundary). No other AI tool does this."

---

## About the Market and Competition

### 11. Who are your competitors?

"There is no direct competitor that occupies the same position. Haystack is the closest in philosophy --- YAML-native AI pipelines --- but it requires Python. LangChain is the most popular orchestration library, but it is an imperative Python library, not a declarative workflow engine. Dify is a visual workflow builder, but it requires Docker and a server. Windmill is Rust-based, but it is a server application requiring PostgreSQL. Nika creates a new category at the intersection of YAML-native definitions and single-binary deployment. Based on research across 80+ sources, no other tool combines even three of Nika's eight distinctive properties."

### 12. Is the market ready for a non-Python AI tool?

"The question assumes the market is homogeneous. It is not. There are millions of developers who use Python for AI but would prefer not to. There are DevOps engineers who think in YAML and find Python import chains bewildering. There are data analysts who can write a GitHub Actions file but cannot set up a Python virtual environment. There are enterprises that need reproducible, auditable, version-controlled AI pipelines without the operational overhead of Docker and Kubernetes. These are my users. The question is not whether the market is ready for non-Python --- it is whether anyone has offered the alternative. Until Nika, the answer was no."

### 13. How do you compete with well-funded companies like LangChain or Dify?

"I do not compete with them. We are solving different problems for different users. LangChain is a Python library for developers who want fine-grained programmatic control. Dify is a visual builder for teams that want a web UI. Nika is a declarative CLI engine for people who want to write a YAML file and run it. The positioning is closer to how Terraform relates to the AWS console --- different interface, different user, same underlying capabilities."

### 14. What does the competitive landscape for AI orchestration look like in 2026?

"Fragmented across five categories. Python developer libraries like LangChain and LlamaIndex. Multi-agent frameworks like CrewAI and AutoGen. Visual builders like Dify and n8n. Data pipeline orchestrators like Prefect and Airflow. And workflow automation engines like Windmill and Argo. Nika does not fit neatly into any of these categories. It is a 'Declarative CLI AI Workflow Engine' --- a category of one. Every tool in those five categories requires either Python, a server, Docker, or Kubernetes. Nika requires none of them."

### 15. The project includes 115 showcase workflows and a 12-level course. Why invest so much in onboarding?

"Because onboarding is the product. The best workflow engine in the world is useless if people cannot learn it. The 12-level course with 44 exercises is designed so that someone with zero Nika experience can be productive in an afternoon. The 115 showcase workflows serve as a reference library --- instead of starting from scratch, you extract a template that is close to what you need and modify it. The LSP provides real-time validation in your editor. These are not extras. They are the difference between a tool that sits in a GitHub star list and a tool that people actually use."

### 16. Are you worried about big tech building something similar?

"No. Large companies optimize for platform lock-in. They will build AI workflow tools that integrate tightly with their cloud services --- AWS Step Functions for AI, Google Cloud Workflows with Vertex AI, Azure Logic Apps with Azure OpenAI. They will not build a single binary that runs on your laptop with zero dependencies and an AGPL license. That is not a product that fits their incentive structure. My positioning is precisely in the space that big tech will not occupy: local-first, zero-dependency, open source, cloud-agnostic."

---

## About Open Source Philosophy

### 16. Why AGPL and not MIT or Apache 2.0?

"Because MIT and Apache 2.0 are invitations for cloud providers to enclose your work. AGPL says: you can use this, you can modify this, you can deploy this, but if you provide it as a network service, you must share your modifications. This prevents the Elasticsearch scenario, the MongoDB scenario, the Redis scenario --- where a cloud provider takes open source software, wraps it in a managed service, captures the value, and gives nothing back. I would rather have a smaller community that is genuinely free than a larger community that is one AWS announcement away from irrelevance."

### 17. Does AGPL hurt adoption?

"In theory, yes --- some enterprises have blanket AGPL prohibitions. In practice, Nika is a CLI binary that users run on their own machines. The AGPL's network copyleft provision --- the part that worries enterprises --- only triggers when you provide the software as a network service to third parties. Running nika on your laptop and executing workflow files does not trigger it. Internal use does not trigger it. The restriction is narrow and specific: it prevents cloud exploitation. For 99% of use cases, AGPL has the same practical impact as MIT."

### 18. What is the connection between One Piece and open source?

"One Piece is fundamentally about liberation. The World Government hoards knowledge. The Marines enforce unjust hierarchies. The pirates fight to free everyone. Whitebeard's last words --- 'The One Piece is real!' --- are the declaration that truth cannot be suppressed. The Sun God Nika, whose power is limited only by imagination, embodies joy-as-liberation. The parallel to open source AI is direct: a small number of powerful companies control AI models, compute, and distribution. Open source communities fight to keep AI accessible. Nika the software, like Nika the Sun God, is limited only by what you can imagine --- in this case, the YAML you write. The butterfly symbol represents the idea that small, beautiful things can change everything. Freedom spreading, impossible to contain."

### 19. Do you see yourself as an activist?

"I see myself as a builder who makes deliberate choices. Every technical decision is also a political decision. Choosing Rust over Python, YAML over code, AGPL over MIT, a single binary over Docker --- these are not just engineering preferences. They are statements about who should have access to AI tooling, who should benefit from community contributions, and who should control the infrastructure that connects people to AI. If making those choices deliberately and publicly makes me an activist, then yes."

### 20. What do you think of 'open source' model releases that are not truly open?

"The term has been stretched to meaninglessness in the AI space. Releasing model weights under a license that restricts commercial use above a revenue threshold is not open source --- it is a marketing strategy. Open source has a definition: the OSI definition. It requires freedom to use, study, modify, and distribute without discrimination against persons, groups, or fields of endeavor. If your license discriminates against companies above a certain revenue, it is not open source. Call it 'source-available' or 'open weight' --- just do not call it open source."

---

## About the Future Vision

### 21. What is on the roadmap?

"Three waves. Wave 1 is model routing --- a 4-slot system where different tasks can use different LLM providers based on the cognitive role needed: a fast cheap model for structured tasks, a powerful model for reasoning, a vision model for images. Wave 2 is dynamic orchestration --- where an LLM decides at runtime which tasks to execute next, replacing static DAGs with intelligent dispatch. Wave 3 is a three-tier memory architecture: hot (in-memory during a run), warm (NDJSON on disk between runs), cold (in NovaNet's knowledge graph for long-term persistence). Each wave builds on the previous one."

### 22. What is NovaNet and how does it relate to Nika?

"NovaNet is the brain; Nika is the body. NovaNet is a knowledge graph backed by Neo4j that stores persistent knowledge --- entities, relationships, metadata. Nika is the execution engine that runs workflows, calls LLMs, processes media. They communicate exclusively via the Model Context Protocol. Nika never touches Neo4j directly --- it uses the invoke verb to call NovaNet's MCP tools. This separation is deliberate: the body does not need to know how the brain stores memories. It just needs to ask and receive."

### 23. Where do you want Nika to be in two years?

"I want Nika to be the Terraform of AI. When someone needs to define a reproducible AI pipeline, version-control it, review it in a pull request, and deploy it with confidence, I want the default answer to be: write a .nika.yaml file. The same way the default answer for cloud infrastructure is 'write a Terraform file' and the default answer for CI/CD is 'write a GitHub Actions file.' The format matters. Declarative YAML files that describe exactly what happens, reproducibly, are better for most workflows than imperative code."

### 24. What would make you consider the project a success?

"If someone who has never written Python can automate an AI workflow in 10 minutes by writing a YAML file and running a single command. If a data team can replace five different tools with one .nika.yaml file. If an enterprise can audit AI workflows by reading version-controlled YAML diffs. If the AGPL license holds and no cloud provider can enclose the software. If the butterfly flies. That is success."

### 25. Any advice for other solo developers building ambitious open source projects?

"Three things. First, choose a language with a strong type system. Rust's compiler is your pair programmer, your code reviewer, and your QA team. It catches bugs that would cost you days in a dynamic language. Second, write tests before code. Not because it is virtuous but because it is practical. When you are the only developer, automated tests are the only thing that prevents regressions from eating you alive. Third, choose your license early and do not compromise. The license is the foundation of your project's social contract. Get it right at the beginning, because changing it later is painful and politically charged. I chose AGPL from day one and have never regretted it."

---

## Bonus: Rapid-Fire Questions

### 26. Tabs or spaces?

"Spaces. Two of them. Non-negotiable."

### 27. Favorite Rust crate?

"ratatui, without hesitation. What that team has built for terminal user interfaces is extraordinary. Our TUI has 42 widgets and 92,000 lines, and ratatui handles all of it gracefully. The ecosystem around it --- including the component patterns and the community --- is one of the best in the Rust world."

### 28. If Nika were a One Piece character, which one?

"Nika, obviously. But if you mean the project's personality? Franky. Loud, unconventional, builds incredible things from spare parts, cries when something beautiful happens, and says 'SUPER!' a lot. Building a workflow engine in Rust when everyone uses Python is very Franky energy."

### 29. What is the hardest bug you have ever fixed in the codebase?

"The timeout unit bug. The YAML schema says `timeout: 30` and that means 30 seconds. But internally, the runtime worked in milliseconds. At some point, the parser was converting correctly in one code path but not in another. Some timeouts were 30 seconds, some were 30 milliseconds. It took a full day to find because the tests were passing --- they were just passing very quickly. The fix was one line. The debugging was eight hours."

### 30. Coffee or tea while coding?

"Coffee. Black. In quantities that would concern a cardiologist."

---

## Usage Notes for Interviewers

- All answers can be used as direct quotes with attribution to "Thibaut Melen, founder of SuperNovae Studio and creator of Nika"
- For shorter formats (podcasts, panels), questions 1, 2, 6, 16, and 24 provide the strongest standalone answers
- For technical audiences, questions 6--10 form a cohesive technical deep dive
- For policy/philosophy audiences, questions 16--20 address the open source licensing debate
- For general interest, questions 1--5 plus 18 (the One Piece connection) provide an accessible narrative

**Media contact:** thibaut@supernovae.studio
**GitHub:** https://github.com/supernovae-st/nika
