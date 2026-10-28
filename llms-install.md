# Installing Nika — a runbook for AI agents

You are an agent installing the `nika` binary for a human. Take the
first path that matches the machine, verify, stop. Every command below
is non-interactive and idempotent. Prefer the package-manager paths —
they verify checksums for you; the shell install script is the human
lane (guided page: <https://nika.sh/install>), not the agent lane.

0.124 is the current release line. Work stays focused on maintenance,
security and the media capabilities workflows need; native video is planned
for 0.125. The current release is 0.124.1, the first maintenance release of
the line. Its known limits are at
<https://docs.nika.sh/changelog/known-limits>.

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

**Nix (builds the default branch via the flake; append `/<tag>` to the
flake reference, e.g. `github:supernovae-st/nika/v0.124.1`, to build a release):**

```sh
nix profile install github:supernovae-st/nika
# one-shot, no install: nix run github:supernovae-st/nika -- --version
```

**GitHub CLI present (Actions runners · Codespaces · agents with `gh`):**

```sh
gh extension install supernovae-st/gh-nika
gh nika --version   # pass-through to a PATH nika, else a one-time checksum-verified release fetch
```

**A container (Linux, amd64 · arm64), built from the same release binaries:**

```sh
docker run --rm ghcr.io/supernovae-st/nika:0.124.1 --version
docker run --rm -v "$PWD:/work" -w /work ghcr.io/supernovae-st/nika:0.124.1 check flow.nika
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

Refuse to proceed if the checksum line does not print `OK`. With `gh`
present, also check the build provenance of the same tarball:
`gh attestation verify "nika-$T-${V#v}.tar.gz" --repo supernovae-st/nika`
([RELEASING.md](RELEASING.md#what-a-user-can-prove) lists all three proofs).

**Windows:** native binaries are not shipped. Use WSL2 and the Linux
path above; say so plainly — do not improvise a build.

## 2 · Verify (always, before reporting success)

```sh
nika --version   # prints the installed release and its commit: nika 0.124.1 (<commit>)
nika doctor      # providers · PATH · what this machine can run
```

`nika: command not found` right after a brew install usually means the
shell needs a PATH refresh — open a new shell before diagnosing deeper.

## 3 · Prove it works (zero keys, zero network)

```sh
nika try 01-hello
```

Exit 0 with a rendered run = the install is good. If Ollama is present,
the same run works fully local once the model is pulled:

```sh
ollama pull qwen3.5:4b
nika try 01-hello --model ollama/qwen3.5:4b
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
rm -rf ~/.nika         # ask the human first (see below)
```

`~/.nika` holds the run-signing keys (`~/.nika/keys`), Session history
(`~/.nika/sessions`), the AI choice (`~/.nika/session-intelligence.json`),
pulled models (`~/.nika/models`) and, after a script install, the binary
itself (`~/.nika/bin`). Deleting the keys leaves runs sealed earlier on
this machine unverifiable here, so ask before deleting. Run traces are not
there: they live in each project's `.nika/traces/`. On macOS, `nika key
init` from a terminal may have kept the key in the keychain instead: also
delete the service `nika` items `run-signing-key` and `run-signing-key.pub`
(Keychain Access, or `security delete-generic-password -s nika -a run-signing-key`
and the same with `-a run-signing-key.pub`).

## After install — authoring

Read the authoring skill
([`nika-authoring/SKILL.md`](.agents/plugins/nika/skills/nika-authoring/SKILL.md)),
which [`AGENTS.md`](AGENTS.md) routes workflow authoring to. The
short form: `nika check <file>` before every handoff — exit 0 is the
bar, and the diagnostics teach the exact fix when it is not.
