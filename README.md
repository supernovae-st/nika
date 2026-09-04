<p align="center">
  <a href="https://nika.sh">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://nika.sh/brand/nika-logo-dark.svg">
      <img src="https://nika.sh/brand/nika-logo-light.svg" alt="Nika" width="220">
    </picture>
  </a>
</p>

<h1 align="center">Intent as Code.</h1>

<p align="center">
  Describe the outcome once. Nika turns it into a visible workflow,<br>
  checks the plan, runs it on your model, and keeps proof of what happened.
</p>

<p align="center">
  <a href="https://github.com/supernovae-st/nika/releases/latest"><img src="https://img.shields.io/github/v/release/supernovae-st/nika?label=release" alt="Latest release"></a>
  <a href="https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml"><img src="https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="https://github.com/supernovae-st/nika-spec"><img src="https://img.shields.io/badge/spec-open-8b8cf8.svg" alt="Open specification"></a>
</p>

<p align="center">
  <img src="media/gifs/intent-dag-proof.optimized.gif" alt="An intent becomes a checked workflow graph, runs, produces action items, and records tamper-evident proof" width="960">
</p>

## Intent → Check → Run → Proof

Install Nika:

```sh
curl -LsSf https://nika.sh/install.sh | sh
```

Open an empty folder and follow one path:

```sh
nika new           # Intent — say what result you want
nika check         # Check  — see the plan before it runs
nika run           # Run    — execute the plan and get the result
nika trace verify  # Proof  — verify the run record
```

| Step | Command | What you get |
|---|---|---|
| **Intent** | `nika new` | Answer three questions. Nika creates one readable workflow file. |
| **Check** | `nika check` | See every step and everything it can access. Nothing runs yet. |
| **Run** | `nika run` | Watch the steps execute. Get the text, data, or file you asked for. |
| **Proof** | `nika trace verify` | Check that the recorded run has not been altered. |

That is the whole loop. With one workflow in the folder, no filenames or flags
are required. Nika also verifies the latest run by default.

## One workflow. One file.

`nika new` creates a `.nika.yaml` file you can open, edit, review, and commit.
Its tasks are the DAG nodes. References connect them. Permissions state exactly
what the workflow may touch. `nika check` shows all of it before execution.

## Bring your model

Keep the workflow. Change only the model when you run it:

```sh
nika run --model ollama/qwen3.5:4b
```

Not sure what is ready on your machine?

```sh
nika doctor
```

Nika can use local models, API providers, and supported agentic CLIs without
changing the workflow itself.

<details>
<summary><strong>Learn the four building blocks</strong></summary>

| Verb | Plain meaning |
|---|---|
| `infer` | Ask a model |
| `exec` | Run a command |
| `invoke` | Call a tool or another workflow |
| `agent` | Let a model use allowed tools for a limited number of turns |

Most first workflows only need `infer`. Independent tasks run in parallel;
`${{ ... }}` references connect them when one needs another's result.

</details>

<details>
<summary><strong>Install another way</strong></summary>

```sh
# Homebrew
brew install supernovae-st/tap/nika

# Cargo
cargo binstall --git https://github.com/supernovae-st/nika nika-cli

# Nix
nix run github:supernovae-st/nika
```

Or use the [latest release archive](https://github.com/supernovae-st/nika/releases/latest).
Windows binaries are not shipped yet; use WSL2 or build from source.

</details>

<details>
<summary><strong>About run data</strong></summary>

Nika writes journals under `.nika/traces/`. Add that directory to `.gitignore`
unless you deliberately want to publish the runs.

`.nika/traces/` inherits the sensitivity of whatever the workflow read; on a
shared or CI machine, treat it as a data-at-rest surface.

`nika trace verify` checks the latest run. A trace proves what Nika recorded
and whether that record changed; it does not claim that an AI answer is true.

</details>

## Next

[Examples](examples/README.md) ·
[Install guide](https://nika.sh/install) ·
[Editor extension](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) ·
[Open specification](https://github.com/supernovae-st/nika-spec) ·
[Roadmap](https://github.com/orgs/supernovae-st/projects/3)

Nika is usable today and intentionally pre-1.0. The engine is
[AGPL-3.0-or-later](LICENSE); the specification is Apache-2.0.
