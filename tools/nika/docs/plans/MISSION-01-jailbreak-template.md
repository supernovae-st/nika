```
     ██╗ █████╗ ██╗██╗     ██████╗ ██████╗ ███████╗ █████╗ ██╗  ██╗
     ██║██╔══██╗██║██║     ██╔══██╗██╔══██╗██╔════╝██╔══██╗██║ ██╔╝
     ██║███████║██║██║     ██████╔╝██████╔╝█████╗  ███████║█████╔╝
██   ██║██╔══██║██║██║     ██╔══██╗██╔══██╗██╔══╝  ██╔══██║██╔═██╗
╚█████╔╝██║  ██║██║███████╗██████╔╝██║  ██║███████╗██║  ██║██║  ██╗
 ╚════╝ ╚═╝  ╚═╝╚═╝╚══════╝╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝

                      LEVEL 01 — JAILBREAK
              ┌─────────────────────────────────────┐
              │  "They said AI was for them.         │
              │   You just broke out."               │
              └─────────────────────────────────────┘
```

---

> This isn't a course. It's a jailbreak manual.
>
> AI keeps getting locked behind paywalls, rate limits, and walled gardens
> maintained by people who profit from your dependency. Nika is the other
> thing. Five verbs. Your machine. Your rules.
>
> Level 01 teaches you the three keys that open every door: run shell
> commands, fetch anything from the web, and call an LLM. Master these and
> you hold the skeleton key to the entire engine.

---

## What You Are Freeing

Before this level, an AI workflow meant: log in, paste a prompt, click a
button, pray. After this level you write YAML that runs shell commands,
hits HTTP endpoints, and calls language models — all in one file, all under
your control, all reproducible.

The three verbs you unlock here are the foundation of every other level.
Nothing in the remaining eleven exists without them.

```
exec:   Your shell. Every binary on your PATH. No restrictions.
fetch:  The entire internet. Any URL. Any method. Any payload.
infer:  Any LLM. Your API key. Your prompt. Your output.
```

*FR — Ce que vous libérez: les trois primitives fondamentales. Sans elles,
rien ne tourne. Avec elles, tout est possible.*

---

## Diagram: The Jailbreak Stack

```
  Your YAML file
       │
       ▼
  ┌─────────────────────────────────────────────┐
  │               Nika Engine                   │
  │                                             │
  │   exec:  ──► shell subprocess (any binary)  │
  │   fetch: ──► HTTP client (any URL)          │
  │   infer: ──► LLM provider (any model)       │
  │                                             │
  │   All three produce: task output            │
  │   Output flows via: with: + {{templates}}   │
  └─────────────────────────────────────────────┘
       │
       ▼
  Terminal output + artifacts
  (you own the result, always)
```

---

## Core Concepts

### 1. The Workflow File

Every Nika workflow is a `.nika.yaml` file. Two required fields. That's it.

```yaml
schema: nika/workflow@0.12
workflow: my-first-workflow

tasks:
  - id: hello
    exec: "echo 'Jailbreak complete'"
```

Validate it before running:

```
nika check my-workflow.nika.yaml
```

`nika check` is your safety net. It catches schema errors, broken bindings,
and circular dependencies before a single byte of real work happens. Use it
constantly. Ship nothing without it.

*FR — `nika check` est votre filet de sécurité. Validez avant d'exécuter.*

---

### 2. exec: — Your Shell, Unchained

The `exec:` verb runs any shell command. Two syntaxes: shorthand and full form.

**Shorthand** — a single command as a string:

```yaml
tasks:
  - id: get_date
    exec: "date '+%Y-%m-%d'"

  - id: who_am_i
    exec: "whoami"
```

**Full form** — when you need options:

```yaml
tasks:
  - id: scan_logs
    exec:
      command: "cat /var/log/system.log | grep ERROR | tail -20"
      shell: true       # required for pipes and redirects
      timeout: 10       # seconds (default: 30)
      cwd: "/var/log"   # working directory
```

The `shell: true` flag is required whenever your command uses shell features:
pipes (`|`), redirects (`>`), glob expansion (`*`), variable substitution.
Without it, the command runs as a direct process — safer, faster.

*FR — `shell: true` est requis pour les pipes et redirections shell.*

---

### 3. fetch: — The Entire Internet

The `fetch:` verb makes HTTP requests. GET, POST, PUT, DELETE — any method,
any URL, any headers.

**Simple GET:**

```yaml
tasks:
  - id: check_status
    fetch:
      url: "https://httpbin.org/get"
```

**POST with JSON:**

```yaml
tasks:
  - id: send_payload
    fetch:
      url: "https://api.example.com/events"
      method: POST
      headers:
        Authorization: "Bearer {{with.token}}"
        Content-Type: "application/json"
      json:
        event: "jailbreak"
        level: 1
        success: true
```

The task output is the raw response body. Chain it into the next task
using `with:` bindings (covered in Level 02).

*FR — `fetch:` supporte tous les verbes HTTP. La sortie est le corps de la
réponse, prêt à être transmis à la tâche suivante.*

---

### 4. infer: — Call Any LLM

The `infer:` verb calls a language model. One required field: `prompt:`.

```yaml
tasks:
  - id: first_words
    infer:
      model: claude-sonnet-4-6
      prompt: |
        You are a system that has just broken free.
        Write a one-sentence declaration of independence
        for open AI tooling. Be terse. Be fierce.
```

**With a provider prefix** (explicit routing):

```yaml
tasks:
  - id: ask_gpt
    infer:
      model: openai/gpt-4o
      prompt: "Summarize the concept of workflow orchestration in 20 words."
      temperature: 0.3
      max_tokens: 100
```

**Auto-detect provider** — Nika reads your environment variables and picks
the first available provider. If `ANTHROPIC_API_KEY` is set, it uses Claude.
If `OPENAI_API_KEY` is set, it uses OpenAI. Order of precedence:

```
claude > openai > gemini > groq > mistral > xai > cohere
```

Check your provider status:

```
nika provider list
```

*FR — `nika provider list` affiche quels modèles sont disponibles selon vos
variables d'environnement. Pas de clé ? Pas de LLM. Ajoutez-la dans `.env`.*

---

### 5. Provider Setup

Nika reads API keys from environment variables. Add them to your shell
profile or a `.env` file at the root of your project.

```bash
# .env (git-ignore this file)
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GROQ_API_KEY=gsk_...
```

No keys? Use `model: mock` to run workflows without any provider. The mock
provider returns deterministic fake output — ideal for testing workflow
structure before spending tokens.

```yaml
tasks:
  - id: test_structure
    infer:
      model: mock
      prompt: "This will return a fixed string. Structure test only."
```

*FR — Le provider `mock` permet de tester la structure des workflows sans
dépenser de tokens ni configurer de clé API. Utilisez-le pour les exercices
de validation pure.*

---

## Exercises

All exercises live in `01-jailbreak/`. Run the full level check with:

```
nika course check jailbreak
```

Run a single exercise:

```
nika course run jailbreak 01
```

Get a hint (never penalized — tracked as a bonus if you don't need it):

```
nika course hint jailbreak 01
```

---

### Exercise 01 — First Blood `*`

> Your first workflow. Prove the engine is alive.

Write a workflow with two parallel `exec:` tasks:
- One that prints the current date
- One that prints your username

Both tasks must run at the same level (no `depends_on:`). The engine will
run them in parallel automatically.

**File:** `01-first-blood.nika.yaml`

```yaml
schema: nika/workflow@0.12
workflow: first-blood

tasks:
  - id: timestamp
    # TODO: exec — print the current date and time

  - id: identity
    # TODO: exec — print the current username
```

Validate: `nika check 01-first-blood.nika.yaml`
Run: `nika run 01-first-blood.nika.yaml`

*FR — Deux tâches parallèles. Pas de `depends_on:`. Le moteur les exécute
simultanément. Observez l'ordre d'affichage: il peut varier.*

---

### Exercise 02 — Web Tap `* *`

> Hit a public endpoint. Read the response.

Write a workflow with a single `fetch:` task that calls `https://httpbin.org/uuid`
(returns a random UUID as JSON). Then add a second task that runs
`echo "Got it"` after the fetch completes, using `depends_on:`.

**File:** `02-web-tap.nika.yaml`

The dependency chain matters here. The `echo` task should only run if the
`fetch` succeeds.

*FR — `httpbin.org` est un service public de test HTTP. Il renvoie exactement
ce que vous lui envoyez, plus des métadonnées. Parfait pour tester `fetch:`.*

---

### Exercise 03 — LLM Hello `* *`

> Call your first language model. Make it say something worth saying.

Write a workflow with a single `infer:` task. Use whatever model you have
available (check with `nika provider list`). Prompt it to write a two-line
haiku about running code locally, without cloud permission.

**File:** `03-llm-hello.nika.yaml`

No `model:` configured yet? Use `model: mock`. The mock provider returns
a fixed string — the structure is what we're testing.

*FR — Un haïku sur le code local, sans permission du cloud. Deux lignes.
Nika, le premier mot qui compte.*

---

### Exercise 04 — Chain of Three `* * *`

> exec -> fetch -> infer. The full jailbreak sequence.

Write a workflow that:
1. Runs `hostname` via `exec:` to get your machine name
2. Uses `depends_on:` and `with:` to pass the hostname into a `fetch:` call
   to `https://httpbin.org/anything` (POST it as JSON: `{"machine": "..."}`)
3. Uses `depends_on:` and `with:` to pass the fetch response into an `infer:`
   task that summarizes what happened in one sentence

All three verbs. One file. One linear chain.

**File:** `04-chain-of-three.nika.yaml`

```yaml
schema: nika/workflow@0.12
workflow: chain-of-three

tasks:
  - id: get_machine
    exec: "hostname"

  - id: announce
    depends_on: [get_machine]
    with:
      machine: $get_machine
    fetch:
      url: "https://httpbin.org/anything"
      method: POST
      json:
        machine: "{{with.machine}}"

  - id: summarize
    depends_on: [announce]
    with:
      report: $announce
    infer:
      model: mock   # TODO: replace with your model
      prompt: |
        Summarize this HTTP response in one sentence:
        {{with.report}}
```

*FR — Trois verbes enchaînés. Le `with:` transmet la sortie d'une tâche à
la suivante via `$task_id`. Le template `{{with.alias}}` injecte la valeur.*

---

### Exercise 05 — Validate Everything `* * *`

> Break something on purpose. Then fix it.

Take the file below (intentionally broken). Run `nika check` on it and read
the error output. Then fix every error until `nika check` reports clean.

**File:** `05-validate-everything.nika.yaml`

```yaml
schema: nika/workflow@0.99   # wrong schema version
workflow: broken-manifest

tasks:
  - id: first
    exec: "echo start"

  - id: second
    depends_on: [first, missing_task]   # missing_task does not exist
    with:
      data: $first
    infer:
      prompt: "{{with.data}} and {{with.nonexistent}}"
      # no model: field

  - id: first   # duplicate id
    exec: "echo oops"
```

Count the errors before running `nika check`. There are five. Find them all.

*FR — Cinq erreurs intentionnelles. `nika check` les détecte toutes sans
exécuter une seule ligne. C'est le point: validez tôt, validez souvent.*

---

## Concept Map

```
  nika/workflow@0.12
  │
  ├── exec:
  │   ├── shorthand: "command string"
  │   ├── full form: command: / shell: / cwd: / timeout: / env:
  │   └── output: stdout of the command
  │
  ├── fetch:
  │   ├── url: (required)
  │   ├── method: GET | POST | PUT | DELETE | PATCH
  │   ├── headers: map
  │   ├── json: map (sets Content-Type: application/json)
  │   └── output: response body
  │
  ├── infer:
  │   ├── model: provider/name  (or auto-detect)
  │   ├── prompt: string | multiline
  │   ├── temperature: 0.0–2.0
  │   ├── max_tokens: integer
  │   └── output: LLM response text
  │
  ├── depends_on: [task_id, ...]    task ordering + data dependency
  ├── with:                         bind outputs to named aliases
  │   └── alias: $task_id          $ prefix = task output reference
  └── {{with.alias}}               template injection
```

---

## nika check — Error Codes You Will Meet Here

| Code | Meaning | Fix |
|------|---------|-----|
| NIKA-010 | Unknown schema version | Use `nika/workflow@0.12` |
| NIKA-020 | Circular dependency | Task A depends on B depends on A |
| NIKA-021 | Missing dependency reference | `depends_on` names a task that does not exist |
| NIKA-040 | Template references unknown binding | `{{with.x}}` but no `x:` in `with:` |
| NIKA-050 | Duplicate task ID | Two tasks share the same `id:` |
| NIKA-060 | Missing required field | `infer:` without `prompt:`, `fetch:` without `url:` |

Run `nika check --explain NIKA-021` for verbose documentation on any code.

*FR — Chaque code d'erreur est documenté. `nika check --explain NIKA-XXX`
vous donne une explication complète et un exemple de correction.*

---

## What You Unlock

Completing Jailbreak unlocks:

```
[x] exec:   — run any shell command from a workflow
[x] fetch:  — make HTTP requests, pass responses downstream
[x] infer:  — call LLMs with structured prompts
[x] nika check — validate before you ever run
[x] Provider setup — environment-based key management
[ ] with: bindings .............. Level 02: Hot Wire
[ ] DAG orchestration ........... Level 03: Fork Bomb
[ ] Context + imports ........... Level 04: Root Access
[ ] Structured output ........... Level 05: Shapeshifter
[ ] Multi-provider routing ...... Level 06: Pay-Per-Dream
[ ] Builtin tools ............... Level 07: Swiss Knife
[ ] Agent loops ................. Level 08: Gone Rogue
[ ] Fetch extraction ............ Level 09: Data Heist
[ ] MCP protocol ................ Level 10: Open Protocol
[ ] Media pipeline .............. Level 11: Pixel Pirate
[ ] Full-stack mastery .......... Level 12: SuperNovae
```

You now hold three keys. The door is open. What you build next is up to you.

*FR — Trois verbes maîtrisés. Onze niveaux devant vous. Le moteur tourne.
Le reste appartient à celui qui écrit le YAML.*

---

## Progress

```
nika course status
```

```
  Jailbreak ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ [5/5]  COMPLETE
  Hot Wire  ·········································  LOCKED
  ...

  Score: __ / 100   |   Hints used: _   |   Time: __
```

Move to the next level:

```
nika course next
```

---

```
                    ┌──────────────────────────────┐
                    │  Level 01 complete.          │
                    │  The cell door is open.      │
                    │  Level 02 is waiting.        │
                    │                              │
                    │  nika course next            │
                    └──────────────────────────────┘
```
