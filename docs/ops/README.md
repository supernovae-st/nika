# nika serve operations

`nika serve` is a resident. Bare `nika serve` is the ARM firer: it fires
the schedules ("beats") declared under `arm:` in the project's `nika.yaml`
([arm reference](https://docs.nika.sh/reference/arm)). With `--bind`,
`--workflows` and `--token-file` the resident opens an authenticated HTTP
door on loopback. Either way it runs inside a project: the folder needs a
`nika.yaml` naming it, and one line is enough.

## First run of the HTTP door

```sh
test -f nika.yaml || printf 'nika: my-project\n' > nika.yaml   # an existing one is kept
mkdir -p workflows
test -f workflows/hello.nika || cat > workflows/hello.nika <<'EOF'
nika: hello
model: mock/echo
permits: {}
tasks:
  greeting:
    infer:
      prompt: "Say hello from nika serve."
      max_tokens: 32
outputs:
  greeting: ${{ tasks.greeting.output }}
EOF
nika check workflows/hello.nika
umask 077
mkdir -p .nika/serve
openssl rand -hex 24 > .nika/serve.token
chmod 600 .nika/serve.token
nika serve --bind 127.0.0.1:8787 --workflows ./workflows --token-file .nika/serve.token
```

`--workflows` names a folder that must exist: `nika serve` exits 1 when it
cannot open it. It serves the `.nika` files under that folder, each named
by its path from the project root, here `workflows/hello.nika`; the
rehearsal model `mock/echo` needs no key and no network. `nika serve` also
creates `.nika/serve` (mode 0700) when the default state root is missing.
The mkdir above is the operator-visible first run.
`nika init` adds only `.nika/traces/` to `.gitignore`: add
`.nika/serve.token` and `.nika/serve/` yourself, so neither the token nor
the job state is committed.

From a second terminal, check that it answers, then start a job by its
served name (the workflow's path from the project root):

```sh
curl -s http://127.0.0.1:8787/health
curl -s -X POST http://127.0.0.1:8787/v1/jobs \
  -H "Authorization: Bearer $(cat .nika/serve.token)" \
  -H 'Idempotency-Key: first-run' \
  -H 'Content-Type: application/json' \
  -d '{"workflow":"workflows/hello.nika"}'
```

`/health` needs no token and lists the server's `supportedCapabilities`.
A new job answers `202` with its id and `"status":"queued"`; read it with
`GET /v1/jobs/{id}`. Every other route needs the Bearer token, and
`POST /v1/jobs` also needs an `Idempotency-Key` and a JSON body.
`GET /v1/openapi.json` describes the live routes. Manual jobs run under a
per-run spend ceiling of USD 1 unless `--run-cost-ceiling <USD|none>` says
otherwise; `nika serve --help` lists the optional doors (`--sessions`,
`--cost-review`, a compile seat). The
[server runbook](https://docs.nika.sh/sdk/operations/server-runbook) and
the TypeScript SDK cover the API from an application.

- `POST /v1/jobs/{id}/cancel` cancels a queued job (`200`, settled
  `cancelled`) or asks a running one to stop (`202`, settled later). A job
  already settled or paused answers `200` with its record, unchanged. A
  running job gets a fixed 5-second grace to reach a wave boundary and
  settle; past it the resident drops the run and the job settles
  `interrupted`. A task inside one long step (an agent turn, a long wait)
  usually outlasts the grace.
- `GET /v1/jobs/{id}/artifacts` lists what a settled job's journal records
  as its artifacts: the files `nika:chart`, `nika:image_generate`,
  `nika:tts_generate` and `nika:image_fx` saved inside the project, and the
  images an agent's harness sent. Each entry has a `name`, `kind`, `size`,
  `sha256` and `mime`, and the manifest carries the job's `receipt`. Files
  written by `nika:write` or `nika:edit` are not listed, because their
  journal frame records no digest. To serve a file through this route, save
  it with one of those builtins.
- `GET /v1/jobs/{id}/artifacts/{name}` streams one of them, `{name}` being
  the entry's `name`, its `/` sent as is or as `%2F`. The bytes come with
  `Content-Type`, `Content-Length` and `Nika-Artifact-Sha256`. The server
  refuses to serve bytes that no longer match the journal
  (`409 artifact_changed`), and stops a stream at the first chunk that
  changed while it was being sent. A body shorter than its `Content-Length`
  is an incomplete download, also left by a resident that stops mid-stream:
  retry it, and a file that really changed then answers `409`.

Both artifact routes need a sealed journal. Create the resident's run key
with `NIKA_KEYCHAIN=off nika key init`, which keeps it under `~/.nika/keys`.
To keep it elsewhere, set both `NIKA_RUN_KEY_FILE` and `NIKA_RUN_PUB_FILE`
to paths outside it, for `nika key init` and then for `nika serve`. The
routes verify the seal against the keys `nika trace verify` weighs in the
resident's environment, in the order the resident signs: that pair's public
half, `~/.nika/keys/run-signing.pub`, then the retired-key ledger. A journal
left unsealed (no run key) answers `409 artifact_journal_unverified`, as
does one sealed with a key none of those hold. The job must have settled:
a queued, running or paused job answers `409 job_not_settled`. One artifact
is served up to 256 MiB; a larger one stays listed and its download answers
`403 artifact_too_large`.

```sh
TOKEN="$(cat .nika/serve.token)"
curl -s -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8787/v1/jobs/<id>/artifacts
curl -s -H "Authorization: Bearer $TOKEN" -o totals.svg \
  http://127.0.0.1:8787/v1/jobs/<id>/artifacts/file/out/totals.svg
```

## Files here

- `nika-serve.service` — systemd unit, loopback, SIGTERM drain.
- `Caddyfile.nika-serve` — TLS + unbuffered SSE. Caddy terminates TLS;
  `nika doctor` never claims TLS from a proxy guess.

MCP servers a workflow invokes — the registry, the approve step, the
confinement: [mcp-servers.md](mcp-servers.md).
