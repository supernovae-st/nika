- **The text of a read is not records (fidelity Law 23).** A candidate whose `nika:jq`
  receives the untouched text of a single text-mode `nika:read` and applies a record operation
  first was admitted by Check and failed at Run with NIKA-BUILTIN-JQ-001. Examples are `.[]`,
  `.[0]`, `.field`, `map(…)`, `select(.field …)`, `group_by(…)`, `keys`, `add` and `first`; each
  listed form was measured to fail on a string. It is now refused before READY as RAW TEXT AS
  RECORDS, with the repair: `fromjson` first, or `nika:convert` for a CSV. The law is narrow on
  purpose: string operations, a parse first, `try`, `?`, `//`, fanned-out reads, binary reads
  and any unknown form are left alone.
