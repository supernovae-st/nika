# Inspect workspace health

Use the repository's current manifests and hygiene tools for the requested
health scope. `bash scripts/refresh-status.sh` reports canonical counts; read
its flags before requesting builds or using `--write`. The existing generated
block is recorded evidence at its named revision, not a current test run.

Use `bash scripts/hygiene/check-all.sh` for the repository hygiene dashboard.
Run affected package tests and clippy when the request needs fresh verification.
Capture their real exit status, not the status of a pipeline's final filter.
A listed test is not a passing test. Keep missing, failed and unexecuted checks
distinct; do not label partial checks as whole-workspace health.

Report the requested measurements, revision, executed checks and limitations.
Do not regenerate every document or run all admission gates for a narrow query.
