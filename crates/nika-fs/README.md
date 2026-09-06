# nika-fs

**Production `TokioFs` — L1 implementation of the `nika-kernel` `Fs` trait family.**

The only production site touching `tokio::fs`. Pure crates (L0) and the
kernel (L0.5) stay filesystem-free; tests inject `MockFs`. One effect
family per crate.

```rust,no_run
use nika_fs::TokioFs;
use nika_kernel::{FsRead, FsWrite, FsList};
use std::path::Path;

# async fn example() -> Result<(), nika_kernel::fs::FsError> {
let fs = TokioFs;                       // zero-size, Copy
fs.write(Path::new("out/report.md"), b"# done").await?;   // atomic temp+rename · parents auto-created
let text = fs.read_to_string(Path::new("out/report.md")).await?;
let workflows = fs.glob(Path::new("."), "**/*.nika.yaml").await?;  // sorted · hidden dirs skipped
# Ok(())
# }
```

## Surface

| trait | methods | backed by |
|---|---|---|
| `FsRead` | `read` · `read_to_string` · `exists` · `canonicalize` | `tokio::fs` |
| `FsWrite` | `write` (replaces) · `write_new` (exclusive) · `create_dir_all` · `remove_file` | `write`: temp + `rename`; `write_new`: temp + exclusive hard link |
| `FsMeta` | `metadata` | `tokio::fs::metadata` |
| `FsList` | `list_dir` (sorted) · `glob` (globset · `literal_separator`) | iterative walk |

Implements the `*Dyn` (`Send`-future) kernel companions — the base traits
and the `Fs` umbrella arrive via the `trait_variant` blanket impls, and
futures are `tokio::spawn`-able.

Filesystem trait errors use `nika_kernel::fs::FsError`; no crate-owned
error enum. See the [kernel backend migration contract](../../docs/crate-specs/nika-kernel-core.md#4-filesystem-backend-migration)
and [TokioFs publication and cancellation limits](../../docs/crate-specs/nika-fs.md#exclusive-publication-and-backend-migration).
Policy (sandbox roots, allow-lists) lives in `nika-policy` (L1.5), not here.

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
