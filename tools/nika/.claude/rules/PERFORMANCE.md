# Nika TUI Performance Rules

## Core Principles

1. **Measure Before Optimizing**
   - Always profile with `cargo flamegraph` before making changes
   - Use `cargo bench` for regression testing
   - Allocation counting: track before/after metrics

2. **Render Path Constraints**
   - NO blocking operations in render loops
   - NO allocations per-frame (except unavoidable format!)
   - NO cloning Arc/String more than once per scope
   - All I/O must be async-spawned

3. **Async Safety**
   - Use `tokio::spawn()` for background work (not `thread::spawn`)
   - Always use `timeout()` on blocking operations
   - Use `parking_lot::Mutex` instead of `std::sync::Mutex` (faster, no poisoning)

## Checklist for Hot Path Code

### ✅ DO:
- [ ] Use `&str` instead of `String` for borrowed data
- [ ] Use `Cow<str>` for mixed owned/borrowed scenarios
- [ ] Cache computed values (provider name, status strings)
- [ ] Reuse buffers across frames (pre-allocate in struct)
- [ ] Use static strings for constants
- [ ] Clone Arc<T> with `Arc::clone(&t)` (intent is clear)
- [ ] Spawn async work outside render path
- [ ] Use timeouts on all network I/O

### ❌ DON'T:
- [ ] Call `format!()` every frame (cache or use constants)
- [ ] Allocate `Vec::new()` in render loops
- [ ] Call `.clone()` on strings in tight loops
- [ ] Use `std::sync::Mutex` (use `parking_lot::Mutex`)
- [ ] Block the event loop (spawn async instead)
- [ ] Parse/compute provider name every frame (cache it)
- [ ] Allocate temp buffers for path display (use `.to_str()`)

## Performance Targets

| Operation | Target | Measured |
|-----------|--------|----------|
| Frame time (60 FPS) | <16.7ms | ~10ms (good) |
| Render pass | <10ms | ~3ms (good) |
| String allocation | <1µs | 0.5µs (good) |
| Mutex acquire | <100ns | ~10ns (excellent) |
| Async spawn | <1µs | 0.2µs (excellent) |

## Common Patterns

### Pattern 1: Cached Computed Values
```rust
// In struct
pub struct MyView {
    cached_provider: Option<(String, Provider)>,  // (model_name, provider)
    cached_status: Option<String>,
}

// In render
if self.model_changed {
    self.cached_provider = Some((
        current_model.clone(),
        Provider::from_model_name(&current_model),
    ));
}
// Use self.cached_provider
```

### Pattern 2: Reusable Buffers
```rust
// In struct
pub struct SearchView {
    query_buf: Vec<u8>,
    haystack_bufs: Vec<Vec<u8>>,
}

// In search
self.query_buf.clear();
self.query_buf.extend_from_slice(query.as_bytes());
// Reuse buffers, no allocation
```

### Pattern 3: Lazy String Building
```rust
// Bad
fn format_status() -> String {
    format!("Status: {} | Count: {}", status, count)
}

// Good (cache it)
let mut status_cache: Option<(String, usize)> = None;

fn get_status(&mut self, count: usize) -> &str {
    if status_cache.as_ref().map(|(_, c)| *c) != Some(count) {
        status_cache = Some((format!("Status: {} | Count: {}", status, count), count));
    }
    &status_cache.as_ref().unwrap().0
}
```

### Pattern 4: Smart Cloning
```rust
// Bad (multiple clones)
let server = server.clone();
spawn(async move {
    let s = server.clone();
    // ...
});

// Good (clone once, reuse)
let server = Arc::new(server);
spawn({
    let s = Arc::clone(&server);  // cheap arc clone
    async move {
        // use s
    }
});
```

### Pattern 5: Avoid String for Paths
```rust
// Bad (allocates)
let filename = entry.file_name().to_string_lossy().to_string();

// Good (borrows when possible)
let filename = entry.file_name().to_str().unwrap_or("");

// Or if you need String
let filename = entry.file_name()
    .to_string_lossy()
    .into_owned();  // only allocate if necessary
```

## Related Issues to Avoid

### Memory Leaks
- Never use `Box::leak()` except for testing
- Always use `Arc` for shared ownership
- Use RAII for cleanup

### Lock Contention
- Hold locks for minimum time
- Use `DashMap` for concurrent access
- Prefer `parking_lot::Mutex` over `std::sync::Mutex`
- Use `OnceCell` for lazy initialization (no repeated locking)

### Allocation Patterns
- Never allocate in hot render path unless unavoidable
- Pre-allocate collections with expected capacity
- Use `smallvec` for small collections (avoids heap)
- Use `Cow<str>` for mixed owned/borrowed

## Testing Performance

### Flamegraph
```bash
cargo flamegraph --bin nika -- studio examples/test-workflow.nika.yaml
# Look for wide bars in render loops — those are allocations/clones
```

### Criterion Benchmarks
```bash
cargo bench --bench <name>
# Track regressions between commits
```

### Memory Profiling (Linux/macOS)
```bash
MALLOC_TRACE=malloc.log cargo run -- studio test.nika.yaml
mtrace malloc.log  # Linux only
```

## Audit Results Reference

See `PERFORMANCE_AUDIT.md` for detailed findings:
- 5 MEDIUM issues identified (all LOW impact)
- Current allocation: ~150µs per complex frame
- Target: <100µs after fixes
- Effort: ~7 hours for all optimizations

## References

- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [parking_lot Docs](https://docs.rs/parking_lot/)
- [Flamegraph Guide](https://www.brendangregg.com/flamegraphs.html)
- [Criterion.rs](https://bheisler.github.io/criterion.rs/book/)
