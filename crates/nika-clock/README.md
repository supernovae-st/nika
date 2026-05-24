# nika-clock

**Production `SystemClock` — L1 implementation of the `nika-kernel` `Clock` trait.**

The only production site touching `tokio::time` + `std::time`. Pure crates
(L0) and the kernel (L0.5) stay clock-free; tests inject the mock. One
trait impl per effect crate.

```rust
use nika_clock::SystemClock;
use nika_kernel::Clock;
use std::time::Duration;

let clock = SystemClock;          // zero-size, Copy
let start = clock.now();          // monotonic Instant
clock.sleep(Duration::from_millis(10)).await;
let elapsed = clock.elapsed(start); // >= 10ms, never negative
let wall = clock.system_now();    // wall-clock SystemTime
```

## Surface

| method | returns | backed by |
|---|---|---|
| `now()` | `Instant` | `Instant::now()` (monotonic) |
| `system_now()` | `SystemTime` | `SystemTime::now()` (wall) |
| `sleep(Duration)` | `async ()` | `tokio::time::sleep` |
| `elapsed(Instant)` | `Duration` | trait default (`now() - since`) |

Infallible — no error enum. Satisfies `Clock` + the object-safe `ClockDyn`.

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
