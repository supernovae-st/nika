# Run Nika Workflow · GitHub Action

Runs one `.nika.yaml` in CI the way Nika always runs: **audited before a
token is spent** (`nika check` gates the job), **cost-bounded while it
runs** (`--max-cost-usd` is required, not optional), **hash-chain traced
after** (the trace path is an output — upload it as an artifact and the
run's evidence travels with the build).

```yaml
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run the review workflow
        id: nika
        uses: supernovae-st/nika/action@main
        with:
          file: workflows/pr-review.nika.yaml
          max-cost-usd: "0.50"
          args: --var pr=${{ github.event.number }}

      - name: Keep the evidence
        if: always() && steps.nika.outputs.trace != ''
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: nika-trace
          path: ${{ steps.nika.outputs.trace }}
```

## Inputs

| Input | Required | What it does |
|---|---|---|
| `file` | yes | the workflow file to check and run |
| `max-cost-usd` | yes | hard USD ceiling — the run stops at the cap |
| `args` | no | extra `nika run` arguments (`--var k=v` · `--answer task=value` to pre-seed a human gate for CI) |
| `version` | no | pin a Nika version (default: latest release) |

## Outputs

| Output | What it is |
|---|---|
| `trace` | path of the run's NDJSON trace (verify it later with `nika trace verify`) |

## Exit behavior

The job fails when `nika check` refuses the file (findings before any
execution) or the run fails. Exit 4 (paused on a human gate) also stops
the job with a loud error: CI has no human — pre-seed the answer via
`args: --answer <task>=<value>` when the gate's outcome is known.

Secrets ride the normal way — export them as env vars in the step and
reference them from the workflow's `secrets:` block. The action adds no
secret surface of its own.
