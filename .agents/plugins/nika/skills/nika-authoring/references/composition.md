# Composition

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## Composition (a workflow is callable)

A job too big for one file becomes a parent that calls children. The
child is a normal workflow; the parent reaches it through the verb it
already knows. **The form is `workflow:` INSIDE `invoke:`, a sibling of
`tool:`, never a tool name.** This is a complete parent. Check is green
only when the child sits at that relative path — the next law:

```yaml
nika: site-audit-parent
inputs:
  target:
    type: string
permits: {}
tasks:
  audit:
    invoke:
      workflow: "./audits/site-audit.nika.yaml"   # sibling of tool:, not a tool
      args:
        url: "${{ inputs.target }}"
```

An agent drafting a workflow uses `nika:compose` through its `tools:`
whitelist and supplies `workflow_yaml`. This checks the draft without
executing it. The standalone builtin refuses the call; a parent calls
a child through `invoke: { workflow: … }`, as shown above.

Its `valid` field reports Core conformance, not execution admission.
The draft remains a proposal: check the actual file and its children with
`nika check --json <file>`. Any authorized run still passes through the
engine's normal admission; a compose result cannot grant or bypass it.

**Four laws, all judged at check.**

- **STATIC target.** A literal path or a pinned
  `registry:owner/name@version`. A `${{ }}`-templated target refuses
  `NIKA-COMP-001`: a call graph you cannot draw before the run is a
  call graph you cannot bound. A relative path resolves from the PARENT
  FILE's directory, not the shell's cwd.
- **The child is READ at check time.** A path that does not resolve is
  `NIKA-COMP-001` too, so a parent cannot pass check without its
  children present on disk.
- **One target per `invoke:`.** `tool:` and `workflow:` together is the
  same refusal class as two verbs on one task.
- **Containment, and the parent declares it (`NIKA-COMP-002`).** The
  child's boundary must be a SUBSET of the parent's, and the parent does
  not inherit anything by calling: a `permits: {}` parent calling a
  child that writes a file is refused at check AND at the run gate, once
  per effect: `child fs write <path> is outside the parent boundary` and
  `child tool nika:write is outside the parent boundary` (spec 14 laws 3
  and 4). The parent's `permits:` must be the union of what every child
  touches. `nika check --infer-permits` will NOT compute that half for
  you: it prints a review note saying the child's boundary is resolved
  by the composition lane, never inferred. **Read that message with
  suspicion**: `child tool X is outside the parent boundary` also fires
  when the parent grants X and the CHILD's own `permits:` is the block
  missing it. Check the child alone before widening the parent.

**What the parent reads back.** `${{ tasks.<id>.output }}` is the
child's whole `outputs:` map, and `${{ tasks.<id>.output.<name> }}`
reaches one entry (measured: a child declaring `greeting:` yields
`{"greeting": …}` and the deep reference passes TYPES). Add `returns:`
to pin the shape, with one trap: the shape is written INLINE — there is
no envelope `types:` block to declare a name in (it refuses
`NIKA-PARSE-005`) — and the grammar is Nika's own, not JSON Schema, so
it is
`{ object: { greeting: string } }` and never `{ type: object,
properties: … }` (that spelling is `NIKA-TYPE-001` too, on the
constructor). `schema:` on an `infer:` IS JSON Schema. Two type
languages in one file, and only one of them takes `type:`.

**What crosses the boundary, and what the claim covers.**

- The composed checker folds children's output-token estimates into
  the parent's `cost.composed` and totals, including calling-task
  multipliers. Verify `judged.composition` and `judged.children`, not
  only the parent's direct inference-task count. The reader-less Rust
  checker is not a substitute for the resolved CLI check.
- The child receives the parent's remaining metered budget at call
  time. This is not an atomic reservation across concurrent children:
  already-admitted calls can overshoot, and static estimates still
  exclude input-token cost. Tighten concurrency and per-call limits;
  never call the parent's cap a hard invoice ceiling.
- An executed child has its own trace when recording is enabled and
  delivery succeeds. Follow the child's trace id when the parent reports
  one; disabled or failed recording does not guarantee a second file.

Reach for composition when a workflow has two audiences (a reusable
audit any project can call) or when one file stops fitting in a
reviewer's head. Do NOT reach for it to avoid writing a task.
