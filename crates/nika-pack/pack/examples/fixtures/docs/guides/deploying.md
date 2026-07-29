# Deploying a workflow

A workflow is a file. Deploying it means putting the file somewhere a
runner can read, and giving that runner the secrets the file declares.

## Checklist

1. `nika check` is green in CI.
2. Every `secrets:` entry has a value in the runner's environment.
3. The `permits:` block names every host and path the run will touch.

Nothing else is required. There is no server to stand up.
