# MCP servers — the registry, the approve step, the confinement

A workflow reaches an MCP tool as `invoke: { tool: "mcp:<server>/<tool>" }`.
Three things must be true before that task runs, in this order:

1. the server is **declared** in `.nika/mcp_servers.json`;
2. its tools are **approved** — `nika mcp approve <server>` pinned them in
   `.nika/mcp_pins.json`;
3. the live server still serves **exactly** what was approved.

`nika check` judges (1). The run judges (2) and (3) before any `tools/call`,
and every refusal names its remedy.

## 1 · Declare — `.nika/mcp_servers.json`

```json
{
  "mcp_servers_format": 1,
  "servers": {
    "owned":  { "command": "python3", "args": ["mcp/server.py"] },
    "github": { "command": "./node_modules/.bin/gh-mcp", "network": "allow" },
    "web":    { "command": "./bin/web-mcp", "network": { "allowlist": ["api.example.com"] } }
  }
}
```

- `mcp_servers_format` is `1`. Server names are `[a-z][a-z0-9-]*`.
- Exactly one transport per entry: `command` (+ `args`) for stdio. A `url`
  entry is refused honestly — the remote transport is not wired.
- `network` is the child's egress arm: **absent = deny** (fail-closed),
  `"allow"` is the explicit escape hatch, `{ "allowlist": [...] }` reserves
  the host-granular arm (confined as `allow` until the loopback proxy lands,
  and the receipt says so).
- A workflow naming `mcp:<server>/…` whose server is not in this file fails
  `nika check` with `NIKA-INVOKE-001` (the C04 verdict), before any run.

## 2 · Approve — `nika mcp approve <server>`

```sh
nika mcp approve owned
# mcp `owned`: sandboxed (seatbelt · net deny)
# approved 1 tool(s) from MCP server `owned` — the lockfile now pins the CURRENT definitions
#   lockfile: .nika/mcp_pins.json · pinned_at 2026-09-13T19:11:44Z
#   ping  blake3:3992d6…
```

The command spawns the server once, reads its `tools/list`, prints every
tool name with its pin, and writes `.nika/mcp_pins.json` — one pin per tool
over `{name, description, inputSchema}`, snapshot included. **Review the
pinned definitions in the lockfile** (the description is what a model
reads · the schema is what it may send): the pin set is what a run trusts
from now on. Commit the lockfile with the
project; never hand-edit it (a pin that no longer matches its snapshot is
`NIKA-MCP-004`, refused, and nothing is re-pinned).

Re-run `approve` after a deliberate server upgrade — a changed or added
tool is otherwise a drift refusal at run.

## 3 · Run — what the dispatch guarantees

| Situation | Refusal | Where |
|---|---|---|
| server not in the registry | `NIKA-INVOKE-001` · names the registry + the approve step | check, then run |
| server declared, never approved | `NIKA-MCP-006` · `nika mcp approve <server>` | run, **before any spawn** |
| tool not in the approved pin set | `NIKA-MCP-006` · lists the pinned names | run, before any spawn |
| live `tools/list` differs from the pins | `NIKA-MCP-003` · the drift diff | run, after the handshake, before any call |
| the server refuses the call or answers `isError` | `NIKA-MCP-002` · the tool's own text | run · the tool's failure lane |
| the pipe dies mid-call | `NIKA-MCP-001` · transient, the session is dropped and reopened on the next call | run |
| the OS sandbox cannot confine the spawn | `NIKA-MCP-005` · no process started | approve and run |

One session per server is kept for the run: spawn + handshake + verify are
paid once, then every call is one request on the open pipe. The registry
and the lockfile are read once, on the first `mcp:` task; a run that names
no MCP tool touches neither. The `nika test` rehearsal keeps every `mcp:`
tool refused (effects disabled) — it never spawns a server.

A tool's `structuredContent` becomes the task's typed output
(`${{ tasks.X.output }}` navigates it); the text blocks are the model-facing
view. An agent task whose `tools:` names an `mcp:` glob is offered the
approved definitions under their `mcp:<server>/<tool>` names.

## 4 · The confinement contract

Every spawned server rides the same OS sandbox as the `exec` verb:

- **macOS** Seatbelt (`sandbox-exec`) · **Linux** bubblewrap (`bwrap`) · any
  other host runs it **unconfined and says so** on the connect line
  (`UNSANDBOXED …`) — never a silent fallback. A host where the sandbox
  exists but refuses the profile is `NIKA-MCP-005`, no process started.
- **Filesystem**: read + write confined to the project tree (the directory
  holding `.nika/`). System binaries and their loader paths stay readable —
  a `node` or `python3` installed anywhere runs; what it *reads and writes*
  must sit under the project.
- **Network**: the registry entry's `network` arm — deny by default.
- **Environment**: scrubbed to the runner floor (`PATH` `HOME` `TMPDIR`
  `LANG` `LC_ALL` `TZ` `USER` `LOGNAME`), minus the dangerous names. Provider
  keys and session tokens never reach a server; configuration travels via
  `args`.

The approve receipt names it first (`mcp \`owned\`: sandboxed (seatbelt ·
net deny)`); a run stays quiet for a confined server and raises a `⚠`
warning on the task the moment a server runs **unconfined**.

### The launcher trap (`npx -y …`)

A launcher that resolves, downloads or caches **outside the project tree**
dies under the confinement before the handshake — `npx -y <pkg>` reads the
global npm install under `~/.nvm/…/lib/node_modules/npm` and caches under
`~/.npm/_npx`, both outside the tree. The failure is `NIKA-MCP-001`, and it
reads its own cause: the transport error carries the child's **stderr
tail** and the confinement it ran under.

```
[NIKA-MCP-001] MCP server `srv` is not reachable: the server closed the pipe before answering
  the server ran sandboxed (seatbelt · net deny)
  its stderr said: node:fs:440 ⏎ … ⏎ Error: EPERM: operation not permitted, open '~/.nvm/versions/node/<v>/lib/node_modules/npm/bin/npx-cli.js'
  a launcher that resolves or installs outside the project tree (`npx -y …` · a global cache) dies confined: install the server inside the project and point `command` at what the tree contains
```

A misspelled `command` reads the same way, one line shorter:
`sandbox-exec: execvp() of 'does-not-exist-bin' failed: No such file or directory`.

The fix is structural, not a flag: install the server **inside the
project** (`npm install <pkg>` → `./node_modules/.bin/<server>` · a venv
under the tree) and point `command` at that path. Set `"network": "allow"`
only when the server itself needs egress at runtime; it does not make an
out-of-tree cache writable.

## Related

- `nika explain NIKA-MCP-001` · `NIKA-MCP-002` — the two spec-registered
  codes; `003`–`006` are the pin family's own (their text names the fix).
- `docs/crate-specs/nika-mcp.md` — the crate contract (client · session ·
  pin · dispatch).
