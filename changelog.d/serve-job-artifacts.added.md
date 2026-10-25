- **`nika serve` downloads the artifacts a settled job's sealed journal
  records.** `GET /v1/jobs/{id}/artifacts` lists them, bound to the job's
  receipt: each file `nika:chart`, `nika:image_generate`,
  `nika:tts_generate` or `nika:image_fx` saved inside the project, and each
  image an agent's harness sent, with its size, SHA-256 and media type.
  `GET /v1/jobs/{id}/artifacts/{name}` streams one, with `Content-Length`
  and `Nika-Artifact-Sha256`. Both need the bearer token, answer only once
  the job settled (`409 job_not_settled`), and serve nothing unless the
  journal verifies sealed and intact with the receipt's chain head. A
  name is looked up in the manifest, never on disk, and the file opens
  below the project root with no link on the way. One artifact is served
  up to 256 MiB. Bytes that no longer match the journal are refused
  (`409 artifact_changed`), and a file changed mid-stream never arrives
  whole. Health advertises `artifacts`; ADR-117 records the decision.
