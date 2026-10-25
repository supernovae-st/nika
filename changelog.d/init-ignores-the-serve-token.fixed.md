- **`nika init` keeps the `nika serve` token and job state out of Git.** The generated `.gitignore`
  covered only `.nika/traces/`, while every `nika serve` surface teaches minting the HTTP Bearer
  token at `.nika/serve.token` and the resident keeps job events and results under `.nika/serve/`.
  In a founded repository `git status` listed the token as a file to commit. Init's ignore section
  now covers both, as does the same adds-only section `nika compile` writes beside a workflow, and
  `NIKA.md` says so. Repositories founded earlier keep their section; add the two lines by hand.
