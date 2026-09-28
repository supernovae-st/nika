✅ **nika check** — clean · `flows/pr-risk-review.nika` · 3 task(s) · 3 wave(s)

💰 **cost floor ≥ $0.00** · ⚠ 1 unpriced/unbounded task(s) — never rendered as $0
- `assess` · ollama/llama3.1 · unpriced (NoPrice)

🔐 **requires** — models: `ollama/llama3.1` · all resolve in this engine · secrets: none

🌊 **schedule** — 3 wave(s), max width 1

<details><summary>🗺 DAG</summary>

```mermaid
graph TD
  diff["diff · exec"]:::exec
  assess["assess · infer · ollama/llama3.1"]:::infer
  comment["comment · invoke · nika:write"]:::invoke
  assess --> comment
  assess --> comment
  diff --> assess
  classDef infer fill:#5b8cff22,stroke:#5b8cff,color:#5b8cff
  classDef exec fill:#ff7a3c22,stroke:#ff7a3c,color:#ff7a3c
  classDef invoke fill:#22d3ee22,stroke:#22d3ee,color:#22d3ee
```

</details>
---
<sub>nika 0.121.0 · report_version 1 · floor semantics: spend ≥ floor · [what this checks](https://docs.nika.sh/reference/machine-surfaces)</sub>
<!-- nika-action:v1:flows/pr-risk-review.nika -->