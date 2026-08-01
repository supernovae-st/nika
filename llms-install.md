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

**GitHub CLI present (Actions runners · Codespaces · agents with `gh`):**

```sh
gh extension install supernovae-st/gh-nika
gh nika --version   # pass-through to a PATH nika, else a one-time checksum-verified release fetch
```

**No package manager (bare Linux/macOS box):** fetch the release
tarball and verify it against `SHA256SUMS` — auditable, no `curl | sh`.
Discover the version via the web redirect, NOT the GitHub API (anonymous
`api.github.com` calls rate-limit to 403 in shared/CI environments; the
redirect never does):

```sh
V=$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/supernovae-st/nika/releases/latest); V=${V##*/tag/}
T=linux-x64   # or: linux-arm64 · macos-arm64 · macos-x64
curl -fsSLO "https://github.com/supernovae-st/nika/releases/download/$V/nika-$T-${V#v}.tar.gz"
curl -fsSLO "https://github.com/supernovae-st/nika/releases/download/$V/SHA256SUMS"
grep "nika-$T-${V#v}.tar.gz" SHA256SUMS | sha256sum -c -   # macOS: shasum -a 256 -c -
tar -xzf "nika-$T-${V#v}.tar.gz"
install -d ~/.local/bin && install -m 755 nika ~/.local/bin/nika   # any PATH dir works
```

Refuse to proceed if the checksum line does not print `OK`.

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
nika try 01-hello
```

Exit 0 with a rendered run = the install is good. If Ollama is present,
the same run works fully local:

```sh
nika try 01-hello --model ollama/qwen3.5:4b
```

## 4 · Wire nika as an MCP server (optional)

`nika mcp` speaks MCP on stdio — read-only authoring tools (check ·
explain · try · catalog · …). For known clients, wiring is one
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
