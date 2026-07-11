# Installing Nika — a runbook for AI agents

You are an agent installing the `nika` binary for a human. Take the
first path that matches the machine, verify, stop. Every command below
is non-interactive and idempotent. Prefer the package-manager paths —
they verify checksums for you; the shell install script is the human
lane (guided page: <https://nika.sh/install>), not the agent lane.

## 1 · Install

**macOS / Linux with Homebrew (preferred):**

```sh
brew install supernovae-st/tap/nika
```

**Any machine with a Rust toolchain (prebuilt download, no compile):**

```sh
cargo binstall --git https://github.com/supernovae-st/nika nika-cli
```

(`cargo install cargo-binstall` first if missing. The crate is named
`nika-cli`; the installed binary is `nika`.)

**Nix (builds the pinned release source via the flake):**

```sh
nix profile install github:supernovae-st/nika
# one-shot, no install: nix run github:supernovae-st/nika -- --version
```

**Windows:** not shipped yet. Say so plainly — do not improvise a build.

## 2 · Verify (always, before reporting success)

```sh
nika --version   # prints the installed release
nika welcome     # 30 seconds: what nika is + what it sees here · offline
nika doctor      # providers · PATH · what this machine can run
```

`nika: command not found` right after a brew install usually means the
shell needs a PATH refresh — open a new shell before diagnosing deeper.

## 3 · Prove it works (zero keys, zero network)

```sh
nika examples run 01-hello --model mock/echo
```

Exit 0 with a rendered run = the install is good. If Ollama is present,
the same run works fully local:

```sh
nika examples run 01-hello --model ollama/qwen3.5:4b
```

## 4 · Wire nika as an MCP server (optional)

`nika mcp` speaks MCP on stdio — read-only authoring tools (check ·
explain · examples · catalog · …). For known clients, wiring is one
idempotent command: `nika wire --help` lists the supported hosts.
Manual shape for anything else:

```json
{ "mcpServers": { "nika": { "command": "nika", "args": ["mcp"] } } }
```

## 5 · Uninstall (leave the machine clean)

```sh
brew uninstall nika    # if installed via brew — cargo/nix undo their own
rm -rf ~/.nika         # traces + caches · ask the human before deleting
```

## After install — authoring

Read [`AGENTS.md`](AGENTS.md), the authoring contract for agents. The
short form: `nika check <file>` before every handoff — exit 0 is the
bar, and the diagnostics teach the exact fix when it is not.
