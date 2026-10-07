- **A proposed fill can no longer vanish or redirect a sketched workflow.** The sketch authoring
  door now reads the proposed graph exactly and checks every fill against the holes of the
  accepted graph before it emits anything. A fill for a task or field the graph does not have, a
  second fill of one hole, a missing or wrongly typed value, an unfilled required hole, an
  argument object that replaces a path, input, bound content or channel the graph owns, and a
  content template that drops its bound input are each refused by name before a candidate
  exists, and repaired within the same bounded budget. Malformed graph fields and edge names
  that collide or shadow a gate or loop binding are refused instead of silently dropped. A
  lawful graph and fills emit exactly the same workflow as before.
