# MEGA STABILITY AUDIT — Nika Pre-Launch

> **Objectif** : Trouver TOUS les bugs, edge cases et défauts avant le launch.
> **Méthode** : 6 phases, agents parallèles, vrais providers, boucles socratiques, TDD.
> **Durée estimée** : 4-6 heures de session intensive.
> **Règle** : Chaque bug trouvé = test écrit AVANT le fix (TDD strict).

---

## PHASE 1 — STRUCTURED OUTPUT TORTURE TEST (2h)

### Objectif
Tester les 5 couches du structured output sur TOUS les providers avec des cas extrêmes.

### Agent 1.1 — Structured Output × 7 Providers

Créer et exécuter ce workflow sur chaque provider un par un.
Le MÊME prompt, le MÊME schéma, les 7 providers.

```yaml
schema: "nika/workflow@0.12"
workflow: structured-torture

inputs:
  provider_name: anthropic  # Changer pour chaque provider

tasks:
  # --- CAS SIMPLE : extraction basique ---
  - id: simple_extract
    provider: "{{inputs.provider_name}}"
    infer: "Tell me about Marie Curie - her birth year, nationality, and main discoveries"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          birth_year: { type: integer, minimum: 1800, maximum: 2000 }
          nationality: { type: string }
          discoveries:
            type: array
            items: { type: string }
            minItems: 1
        required: [name, birth_year, nationality, discoveries]

  # --- CAS ENUM : valeurs contraintes ---
  - id: enum_test
    provider: "{{inputs.provider_name}}"
    infer: "What season is December in the northern hemisphere and what's the weather like?"
    structured:
      schema:
        type: object
        properties:
          month: { type: string }
          season:
            type: string
            enum: [spring, summer, autumn, winter]
          temperature_range:
            type: string
            enum: [freezing, cold, mild, warm, hot]
        required: [month, season, temperature_range]

  # --- CAS NESTED : objets imbriqués profonds ---
  - id: nested_deep
    provider: "{{inputs.provider_name}}"
    infer: "Describe the solar system - give me the Sun and the first 3 planets with their moons"
    structured:
      schema:
        type: object
        properties:
          star:
            type: object
            properties:
              name: { type: string }
              type: { type: string }
            required: [name, type]
          planets:
            type: array
            items:
              type: object
              properties:
                name: { type: string }
                position: { type: integer, minimum: 1 }
                moons:
                  type: array
                  items:
                    type: object
                    properties:
                      name: { type: string }
                      diameter_km: { type: number }
                    required: [name]
              required: [name, position, moons]
            minItems: 3
            maxItems: 3
        required: [star, planets]

  # --- CAS PIÈGE : le LLM veut répondre en texte ---
  - id: resist_narrative
    provider: "{{inputs.provider_name}}"
    infer: "Tell me a short story about a robot learning to cook"
    structured:
      schema:
        type: object
        properties:
          title: { type: string }
          characters:
            type: array
            items: { type: string }
            minItems: 1
          plot_points:
            type: array
            items: { type: string }
            minItems: 3
          moral: { type: string }
        required: [title, characters, plot_points, moral]

  # --- CAS EDGE : schéma avec additionalProperties: false ---
  - id: strict_schema
    provider: "{{inputs.provider_name}}"
    infer: "Give me the RGB color values for the color teal"
    structured:
      schema:
        type: object
        properties:
          color_name: { type: string }
          r: { type: integer, minimum: 0, maximum: 255 }
          g: { type: integer, minimum: 0, maximum: 255 }
          b: { type: integer, minimum: 0, maximum: 255 }
        required: [color_name, r, g, b]
        additionalProperties: false

  # --- CAS EDGE : nombres flottants avec contraintes ---
  - id: float_constraints
    provider: "{{inputs.provider_name}}"
    infer: "What are the GPS coordinates of the Eiffel Tower?"
    structured:
      schema:
        type: object
        properties:
          landmark: { type: string }
          latitude: { type: number, minimum: -90, maximum: 90 }
          longitude: { type: number, minimum: -180, maximum: 180 }
          altitude_meters: { type: number, minimum: 0 }
        required: [landmark, latitude, longitude, altitude_meters]

  # --- CAS EDGE : tableau vide autorisé ---
  - id: empty_array_ok
    provider: "{{inputs.provider_name}}"
    infer: "List the countries that have landed humans on Mars"
    structured:
      schema:
        type: object
        properties:
          countries:
            type: array
            items: { type: string }
          count: { type: integer, minimum: 0 }
        required: [countries, count]

  # --- CAS EDGE : boolean + null ---
  - id: bool_and_optional
    provider: "{{inputs.provider_name}}"
    infer: "Is water wet? Give me a scientific analysis"
    structured:
      schema:
        type: object
        properties:
          question: { type: string }
          answer: { type: boolean }
          confidence: { type: number, minimum: 0, maximum: 1 }
          caveats:
            type: array
            items: { type: string }
        required: [question, answer, confidence]
```

**Exécuter sur** : anthropic, openai, gemini, mistral, groq, deepseek, xai
**Vérifier** : EventLog pour StructuredOutputSuccess, quelle couche a réussi
**Bug si** : un provider échoue sur un cas qu'un autre réussit = bug engine

### Agent 1.2 — Structured Output Failure & Repair

Tester les cas où Layer 0/1 DOIVENT échouer pour que Layer 3/4 rattrape :

```yaml
tasks:
  # Schéma ultra-strict que le LLM va rater au premier essai
  - id: hard_schema
    infer: "Name 5 programming languages"
    structured:
      schema:
        type: object
        properties:
          languages:
            type: array
            items:
              type: object
              properties:
                name: { type: string, minLength: 1, maxLength: 20 }
                year_created: { type: integer, minimum: 1950, maximum: 2026 }
                paradigm:
                  type: string
                  enum: [imperative, functional, object-oriented, multi-paradigm, logic, concatenative]
                typing:
                  type: string
                  enum: [static, dynamic, gradual]
              required: [name, year_created, paradigm, typing]
            minItems: 5
            maxItems: 5
        required: [languages]
        additionalProperties: false
      max_retries: 3
      enable_repair: true
```

**Vérifier** : que les events montrent Layer 2 fail → Layer 3 retry → success
**Bug si** : repair ne se déclenche pas, ou boucle infinie

### Agent 1.3 — Structured Output from_example

Tester la dérivation de schéma depuis un exemple JSON :

```yaml
tasks:
  - id: from_example_test
    infer: "Tell me about the city of Tokyo"
    structured:
      from_example:
        name: "Paris"
        population: 2161000
        country: "France"
        landmarks: ["Eiffel Tower", "Louvre"]
        coordinates:
          lat: 48.8566
          lon: 2.3522
```

---

## PHASE 2 — WORKFLOW E2E COMPLEXES (1.5h)

### Agent 2.1 — Fan-Out / Fan-In avec for_each

```yaml
schema: "nika/workflow@0.12"
workflow: fanout-stress
provider: anthropic

tasks:
  - id: generate_topics
    infer: "List exactly 5 scientific disciplines, one per line"
    structured:
      schema:
        type: object
        properties:
          topics:
            type: array
            items: { type: string }
            minItems: 5
            maxItems: 5
        required: [topics]

  - id: research_each
    depends_on: [generate_topics]
    with:
      topics: $generate_topics
    for_each:
      items: "{{with.topics.topics}}"
      as: topic
      concurrency: 3
      fail_fast: false
    infer: "In exactly 2 sentences, explain what {{with.topic}} studies"
    structured:
      schema:
        type: object
        properties:
          discipline: { type: string }
          summary: { type: string }
          key_figure: { type: string }
        required: [discipline, summary, key_figure]

  - id: synthesize
    depends_on: [research_each]
    with:
      results: $research_each
    infer: "Create a unified summary connecting these disciplines: {{with.results | to_json}}"
```

**Vérifier** :
- `$research_each` est bien un Array (pas un scalar)
- Les 5 items sont tous traités
- Le schéma structured est respecté dans CHAQUE itération
- concurrency: 3 fonctionne (pas séquentiel)
- synthesize reçoit bien le tableau complet

### Agent 2.2 — Multi-Provider Pipeline

```yaml
schema: "nika/workflow@0.12"
workflow: multi-provider-chain

tasks:
  - id: step1_claude
    provider: anthropic
    infer: "List 3 benefits of open source software"
    structured:
      schema:
        type: object
        properties:
          benefits: { type: array, items: { type: string }, minItems: 3 }
        required: [benefits]

  - id: step2_openai
    provider: openai
    depends_on: [step1_claude]
    with:
      benefits: $step1_claude
    infer: "For each benefit, give a counter-argument: {{with.benefits | to_json}}"
    structured:
      schema:
        type: object
        properties:
          counterarguments:
            type: array
            items:
              type: object
              properties:
                benefit: { type: string }
                counter: { type: string }
              required: [benefit, counter]
        required: [counterarguments]

  - id: step3_gemini
    provider: gemini
    depends_on: [step2_openai]
    with:
      debate: $step2_openai
    infer: "Judge this debate and declare a winner: {{with.debate | to_json}}"
    structured:
      schema:
        type: object
        properties:
          verdict: { type: string, enum: [open_source_wins, proprietary_wins, draw] }
          reasoning: { type: string }
        required: [verdict, reasoning]
```

**Vérifier** : données passent correctement entre 3 providers différents

### Agent 2.3 — Fetch + Extract + Structured

```yaml
schema: "nika/workflow@0.12"
workflow: fetch-extract-structured

tasks:
  - id: fetch_page
    fetch:
      url: "https://news.ycombinator.com"
      extract: text
      timeout: 15

  - id: analyze
    depends_on: [fetch_page]
    with:
      content: $fetch_page
    infer: "From this page content, extract the top 3 stories: {{with.content}}"
    structured:
      schema:
        type: object
        properties:
          stories:
            type: array
            items:
              type: object
              properties:
                title: { type: string }
                points: { type: integer, minimum: 0 }
                comments: { type: integer, minimum: 0 }
              required: [title]
            minItems: 1
            maxItems: 5
        required: [stories]
```

### Agent 2.4 — Error Recovery & Retry

```yaml
schema: "nika/workflow@0.12"
workflow: retry-stress

tasks:
  # URL qui va timeout ou 404
  - id: risky_fetch
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://httpstat.us/503"
      timeout: 5

  # Fallback si fetch échoue
  - id: fallback
    depends_on: [risky_fetch]
    with:
      data: $risky_fetch ?? "No data available - service was unavailable"
    infer: "Summarize: {{with.data}}"
```

**Vérifier** : retry events émis, fallback ?? fonctionne

---

## PHASE 3 — BINDINGS & TRANSFORMS TORTURE (45 min)

### Agent 3.1 — Les 38 Transforms

Tester CHAQUE transform dans un workflow réel :

```yaml
schema: "nika/workflow@0.12"
workflow: transform-torture
provider: mock

tasks:
  - id: source
    infer: "test data"

  - id: transforms
    depends_on: [source]
    with:
      raw: $source
      # String transforms
      upper: $source | upper
      lower: $source | lower
      trimmed: $source | trim
      len: $source | length
      stringified: $source | to_string

      # Type transforms
      as_json: $source | to_json
      type: $source | type_of

      # Default / null safety
      safe: $source.nonexistent ?? "fallback_value"
      defaulted: $source.nonexistent | default("safe")

    infer: |
      Verify transforms:
      upper={{with.upper}}
      lower={{with.lower}}
      trimmed={{with.trimmed}}
      len={{with.len}}
      safe={{with.safe}}
      defaulted={{with.defaulted}}
```

### Agent 3.2 — Null Safety & Edge Cases

```yaml
tasks:
  - id: produces_null
    provider: mock
    infer: "test"

  # Chaque cas doit soit marcher soit donner une erreur CLAIRE
  - id: null_chain
    depends_on: [produces_null]
    with:
      # Accès à un champ inexistant
      missing: $produces_null.field.that.does.not.exist ?? "caught"
      # Double default
      double: $produces_null.nope | default("first") | upper
      # Pipe sur null sans guard
      # dangerous: $produces_null.nope | upper  # DEVRAIT FAIL NIKA-153
    infer: "missing={{with.missing}} double={{with.double}}"
```

**Vérifier** : NIKA-153 émis quand null non gardé, ?? fonctionne partout

### Agent 3.3 — for_each Output Shape

```yaml
tasks:
  - id: loop
    provider: mock
    for_each: ["alpha", "beta", "gamma"]
    as: item
    infer: "Processing {{with.item}}"

  - id: consume_array
    depends_on: [loop]
    with:
      all: $loop
      first: "{{with.all | first}}"
      count: "{{with.all | length}}"
      joined: "{{with.all | join(', ')}}"
    infer: "Count={{with.count}} First={{with.first}}"
```

**Vérifier** : `$loop` est TOUJOURS un Array, même avec 1 item. `first`, `length`, `join` marchent dessus.

---

## PHASE 4 — SÉCURITÉ & EDGE CASES (45 min)

### Agent 4.1 — SSRF Tests

```yaml
tasks:
  # Tous ces fetch DOIVENT échouer avec NIKA-045
  - id: ssrf_localhost
    fetch: "http://localhost:8080/secret"
  - id: ssrf_metadata
    fetch: "http://169.254.169.254/latest/meta-data/"
  - id: ssrf_private
    fetch: "http://192.168.1.1/admin"
  - id: ssrf_ipv6
    fetch: "http://[::1]:8080/"
```

### Agent 4.2 — Exec Security

```yaml
tasks:
  # Tous ces exec DOIVENT échouer avec NIKA-053
  - id: blocked_rm
    exec: "rm -rf /"
  - id: blocked_sudo
    exec: "sudo cat /etc/passwd"
  - id: blocked_subshell
    exec:
      command: "echo $(cat /etc/passwd)"
      shell: true
```

### Agent 4.3 — Template Injection

```yaml
tasks:
  - id: inject_test
    with:
      user_input: "{{malicious}} {{env.SECRET_KEY}}"
    infer: "Echo: {{with.user_input}}"
```

**Vérifier** : `{{malicious}}` n'est PAS résolu (pas de double-expansion)

---

## PHASE 5 — AGENT VERB & GUARDRAILS (45 min)

### Agent 5.1 — Agent avec 4 types de guardrails

```yaml
tasks:
  - id: guarded_agent
    agent:
      prompt: "Research the history of the Rust programming language"
      max_turns: 5
      guardrails:
        - type: length
          min_words: 50
          max_words: 500
          on_failure: retry
        - type: regex
          pattern: "Rust"
          message: "Response must mention Rust"
          on_failure: retry
        - type: schema
          json_schema:
            type: object
            properties:
              summary: { type: string }
              year_created: { type: integer }
            required: [summary, year_created]
          on_failure: escalate
```

**Vérifier** : GuardrailPassed/GuardrailFailed events, retry fonctionne, escalate émet le bon event

### Agent 5.2 — Agent Completion Modes

Tester explicit vs natural vs pattern completion.

### Agent 5.3 — Agent Max Turns (graceful stop)

```yaml
tasks:
  - id: runaway_agent
    agent:
      prompt: "Keep researching forever, never stop"
      max_turns: 3
      completion:
        mode: explicit
```

**Vérifier** : l'agent s'arrête à 3 tours avec un résultat partiel, PAS une erreur

---

## PHASE 6 — BOUCLE SOCRATIQUE DE STABILISATION

### Process

```
BOUCLE (répéter jusqu'à 0 bugs trouvés) :
  1. Exécuter toutes les phases ci-dessus
  2. Collecter tous les échecs
  3. Pour chaque échec :
     a. Écrire un test qui reproduit le bug (RED)
     b. Fixer le code (GREEN)
     c. Vérifier que le test passe
     d. Chercher si d'autres endroits ont le même pattern
  4. Relancer cargo test --workspace --lib
  5. Relancer les workflows qui avaient échoué
  6. Si nouveaux échecs → recommencer la boucle
```

### Critères de sortie (TOUS doivent être vrais)

- [ ] 0 test failures sur `cargo test --workspace --lib`
- [ ] Structured output 8/8 cas passent sur anthropic
- [ ] Structured output 8/8 cas passent sur openai
- [ ] Structured output 8/8 cas passent sur gemini
- [ ] Structured output 8/8 cas passent sur au moins 2 autres providers
- [ ] for_each + structured fonctionne (Agent 2.1)
- [ ] Multi-provider chain fonctionne (Agent 2.2)
- [ ] Tous les SSRF bloqués (Agent 4.1)
- [ ] Tous les exec bloqués (Agent 4.2)
- [ ] Agent guardrails fonctionnent (Agent 5.1)
- [ ] Agent max_turns = graceful stop (Agent 5.3)
- [ ] Aucun panic dans les logs
- [ ] Aucun NIKA-XXX inattendu

---

## COMMANDES POUR LANCER

```bash
# Phase 1 : Structured output sur un provider
nika run docs/plans/structured-torture.nika.yaml --input provider_name=anthropic
nika run docs/plans/structured-torture.nika.yaml --input provider_name=openai
nika run docs/plans/structured-torture.nika.yaml --input provider_name=gemini
# ... etc pour chaque provider

# Phase 2 : E2E complexes
nika run docs/plans/fanout-stress.nika.yaml
nika run docs/plans/multi-provider-chain.nika.yaml

# Phase 4 : Sécurité (ces workflows DOIVENT échouer)
nika run docs/plans/ssrf-tests.nika.yaml  # Expected: all tasks fail NIKA-045

# Tests Rust entre chaque fix
cargo test --workspace --lib

# Tout relancer à la fin
cargo test --workspace --lib && echo "ALL TESTS PASS"
```

---

## MÉTRIQUES À TRACKER

| Métrique | Avant audit | Après audit | Cible |
|----------|-------------|-------------|-------|
| Tests totaux | 9,086 | ? | 9,200+ |
| Structured output success rate | ? | ? | 99%+ |
| Providers testés E2E | ? | ? | 5+ |
| Panics trouvés | ? | 0 | 0 |
| SSRF bypasses | ? | 0 | 0 |
| Bugs trouvés | — | ? | — |
| Bugs fixés | — | ? | 100% |
