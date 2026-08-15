# nika-event

**Canonical event log + trace types + emitter for the Nika diamond.**
Layer **L0** — pure, zero I/O, zero async.

```rust
use nika_event::{Event, EventKind, Emitter, InMemoryEmitter};
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

let sink = InMemoryEmitter::unbounded();
let ev = Event::new(
    EventId::new(Uuid::now_v7()),
    Timestamp::from_unix_ms(1_716_000_000_000),
    EventKind::WorkflowStarted,
);
sink.emit(ev).expect("unbounded emit never fails");
assert_eq!(sink.len(), 1);
```

## Three pieces

- **`Event`** — the immutable envelope. The caller supplies id + timestamp;
  L0 never reads a clock (that's the L1 `nika-clock` effect).
- **`EventKind`** — the `#[non_exhaustive]` taxonomy: workflow + task
  lifecycle + the 4-verb dispatch surface (`infer · exec · invoke · agent`).
- **`Emitter`** — the object-safe sink trait, with `NoOpEmitter` and
  `InMemoryEmitter` (bounded or unbounded) L0 impls. I/O-backed emitters
  (file, IPC, the Connectome) live at higher layers and reuse this contract.

## Domain boundary

This is the **engine** chronicle (runtime events). The **studio** keeps a
separate chronicle in its own private tree — same NDJSON spirit, disjoint
taxonomy, never conflated.

## Error codes

`NIKA-420` serialization failed · `NIKA-421` buffer full
(`Category::Observability`).

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
