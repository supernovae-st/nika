<p align="center">
  <a href="https://nika.sh">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://nika.sh/brand/nika-logo-dark.svg">
      <img src="https://nika.sh/brand/nika-logo-light.svg" alt="Nika" width="220">
    </picture>
  </a>
</p>

# Nika

**Repeatable AI work, in a file.**

[![Release](https://img.shields.io/github/v/release/supernovae-st/nika?label=release)](https://github.com/supernovae-st/nika/releases/latest)
[![CI](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml/badge.svg?branch=main)](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml)
[![Engine license](https://img.shields.io/badge/engine-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Open spec](https://img.shields.io/badge/spec-Apache--2.0-brightgreen.svg)](https://github.com/supernovae-st/nika-spec)

Put an AI task in a `.nika.yaml` file. Nika checks it before it runs, runs it on
the model you choose, and keeps a receipt afterwards.

## Try it

```sh
curl -LsSf https://nika.sh/install.sh | sh
nika try 01-hello
```

That first run is a rehearsal: no account, API key, or model server needed.

<p align="center">
  <img src="media/gifs/dag-execution.optimized.gif" alt="A Nika workflow and its execution graph" width="820">
</p>

## Make one

```sh
nika new 01-hello hello.nika.yaml
nika check hello.nika.yaml
nika run hello.nika.yaml
```

The whole idea is this small:

```yaml
nika: hello
model: mock/echo
permits: {}

tasks:
  greet:
    infer:
      prompt: "Say hello in French."
      max_tokens: 64

outputs:
  greeting: ${{ tasks.greet.output }}
```

Change the prompt. Add tasks. Connect them with `${{ ... }}`. Keep the file in
Git like any other piece of your product.

## Use a real model

See what this machine can already use:

```sh
nika doctor
nika catalog
```

Then choose a model when you run:

```sh
nika run hello.nika.yaml --model ollama/qwen3.5:4b
```

Nika works with local models, API providers, and supported agentic CLIs. The
workflow stays the same when the model changes.

## Why Nika?

- **Check before spending.** Catch invalid dataflow, missing permissions,
  secret leaks, and impossible budgets before a model call.
- **Run anywhere.** Use a local model today and a hosted model tomorrow.
- **See what happened.** Every run produces a local, hash-chained trace.
- **Review the work.** Prompts, tools, dependencies, limits, and outputs live in
  one diffable file.

## Four verbs

| Verb | Use it for |
|---|---|
| `infer` | Ask a model |
| `exec` | Run a command |
| `invoke` | Call an MCP tool |
| `agent` | Give a model a bounded tool loop |

Tasks form a directed graph. Independent tasks run in parallel; references make
dependencies explicit.

## One door

You do not need to learn the engine to start:

- `nika try 01-hello` rehearses an example.
- `nika new` creates a workflow.
- `nika check` inspects it before execution.
- `nika run` runs it.

The current pre-1.0 work is making that same door coherent for interactive
sessions, resumes, and long-lived runs. The public
[roadmap](https://github.com/orgs/supernovae-st/projects/3) is organized around
conditions for 1.0, not a promised date.

## Install another way

<details>
<summary>Homebrew, Cargo, Nix, Docker, and release archives</summary>

### Homebrew

```sh
brew install supernovae-st/tap/nika
```

### Cargo

```sh
cargo binstall --git https://github.com/supernovae-st/nika nika-cli
```

The package name is `nika-cli`; the command is `nika`.

### Nix

```sh
nix run github:supernovae-st/nika
```

### Docker

```sh
docker run --rm -v "$PWD:/work" -w /work ghcr.io/supernovae-st/nika \
  check hello.nika.yaml
```

### Release archive

Download a platform archive and `SHA256SUMS` from the
[latest release](https://github.com/supernovae-st/nika/releases/latest), verify
it, and put `nika` on your `PATH`.

Windows binaries are not shipped yet. Use WSL2 or build from source.

</details>

## Your run data

Nika writes journals under `.nika/traces/`. Add that directory to `.gitignore`
unless you deliberately want to publish the runs.

`.nika/traces/` inherits the sensitivity of whatever the workflow read; on a
shared or CI machine, treat it as a data-at-rest surface.

Verify a journal at any time:

```sh
nika trace verify .nika/traces/<run>.ndjson
```

## Go further

- [Examples](examples/README.md)
- [Install guide](https://nika.sh/install)
- [TypeScript SDK](nika-client/README.md)
- [VS Code, Cursor, and Windsurf extension](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang)
- [Open workflow specification](https://github.com/supernovae-st/nika-spec)
- [Roadmap](https://github.com/orgs/supernovae-st/projects/3)
- [Contributing](CONTRIBUTING.md)

Nika is usable today and intentionally pre-1.0. The engine is licensed under
[AGPL-3.0-or-later](LICENSE); the specification is licensed under Apache-2.0.
