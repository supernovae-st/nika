- **Stages a proposal states over a plain filter are kept.** When the
  request's own reading of a clause was only a filter, a projection,
  number columns, an order or a limit that the proposal stated over the
  same clause were dropped, so the workflow wrote whole rows with amounts
  as text. They now replace the plain filter when both keep the same rows,
  and a proposal whose filter reads the clause differently leaves the
  request incomplete instead of silently narrowing it.
