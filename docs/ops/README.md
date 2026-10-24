# nika serve operations

`nika serve` is a resident. Bare `nika serve` is the ARM firer: it fires
the schedules ("beats") declared under `arm:` in the project's `nika.yaml`
([arm reference](https://docs.nika.sh/reference/arm)). With `--bind`,
`--workflows` and `--token-file` the resident opens an authenticated HTTP
door on loopback. Either way it runs inside a project: the folder needs a
`nika.yaml` naming it, and one line is enough.

## First run of the HTTP door

```sh
printf 'nika: my-project\n' > nika.yaml   # the project file, if the folder has none
umask 077
mkdir -p .nika/serve
openssl rand -hex 24 > .nika/serve.token
chmod 600 .nika/serve.token
nika serve --bind 127.0.0.1:8787 --workflows ./workflows --token-file .nika/serve.token
```

`nika serve` also creates `.nika/serve` (mode 0700) when the default
state root is missing. The mkdir above is the operator-visible first run.
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
  already settled or paused answers `200` with its record, unchanged.
- `/v1/jobs/{id}/artifacts` is not served (`404`).

## Files here

- `nika-serve.service` — systemd unit, loopback, SIGTERM drain.
- `Caddyfile.nika-serve` — TLS + unbuffered SSE. Caddy terminates TLS;
  `nika doctor` never claims TLS from a proxy guess.

MCP servers a workflow invokes — the registry, the approve step, the
confinement: [mcp-servers.md](mcp-servers.md).
