- **An agent application's images are task values.** An `agent:` task on a seat can declare
  `images:` (spec 02 §Harness images): `.output` becomes `{ message, images }`, each image with
  its `index`, `path`, `sha256`, `size_bytes`, `mime_type`, `width` and `height`, and
  `images: { save: <path> }` has Nika write the received bytes through `nika:write`, inside
  `permits.fs.write`. Only the bytes sent on the task's own session count: the path an
  application reports (Codex's `~/.codex/generated_images`, which every session shares) is
  never opened, so a workflow no longer asks Codex to copy its image and cannot pick up another
  session's. A save with no image received fails the new `NIKA-AGENT-007`. The image's
  `agent_image_observed` frame records the `saved_path`, and `nika serve` lists the file among
  the run's artifacts with the received digest. On an API or local model, `check` and the run
  refuse `images:`.
