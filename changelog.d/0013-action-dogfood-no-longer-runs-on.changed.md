- **Action dogfood no longer runs on tags.** `install.sh` resolves the
  latest published binary, which lags the tagged tree until release.yml
  finishes. The smoke workflow now lives outside the checkout so project
  discovery cannot walk into this engine's `nika.yaml`. An additive
  `working-directory` input on the composite action is the seam.
