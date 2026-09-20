- **The resident binds a schedule's `inputs` on every fire (#1370 · the Serve
  door · prerequisite of #1719).** `PUT /v1/schedules/{id}` accepts
  `inputs: { key: <scalar> }` (string · number · boolean, one per declared
  input key), stores them with the definition, and folds them into the
  schedule revision only when bound, so every schedule written before the key
  existed keeps its exact revision, integrity bytes and store row. At fire the
  resident coerces each text by the workflow's declared type (`"7"` on an
  `integer` becomes `7`, the same `--var` law `nika arm fire` applies) and
  judges the result with the literal admission validator `POST /v1/jobs`
  uses; the bound values reach the run as `ApiCaller` inputs. A project beat's
  `inputs:` ride the same path: `nika serve` and `nika arm fire` now bind the
  identical values for one `arm:` line. Judged twice, named once: an unknown
  key (with the declared set), a value the declared type refuses, a missing
  required input (`NIKA-1708`) and the `@env:` channel (the CLI edge's, never
  a server's environment) refuse the `PUT` as `schedule.inputs`, and a file
  that stops declaring the key between the arm and the slot refuses the fire
  contained to that schedule (`schedule.admission`), the resident keeps
  running. `GET /v1/schedules/{id}` projects `definition.inputs` as the
  `--var` text it stores. OpenAPI `SchedulePut.inputs` documents the wire.
