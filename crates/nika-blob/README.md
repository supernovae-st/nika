# nika-blob

**Production `DiskBlobStore` — L1 implementation of the `nika-kernel` `BlobStore` trait.**

Content-addressed storage: blake3 hash is the key, sharded over the
filesystem, atomic temp+rename writes, dedup for free. The only
production site touching the blob filesystem; tests inject `MockBlob`.

```rust,no_run
use nika_blob::DiskBlobStore;
use nika_kernel::BlobStore;
use bytes::Bytes;

# async fn example() -> Result<(), nika_kernel::BlobError> {
let store = DiskBlobStore::new("/var/lib/nika/blobs");
let meta = store.put(Bytes::from_static(b"report"), "text/markdown").await?;
//  meta.hash == "blake3:<hex>"  ·  identical bytes always dedup
let bytes = store.get(&meta.hash).await?;
let info  = store.stat(&meta.hash).await?;   // info.mime_type == "text/markdown"
# Ok(())
# }
```

## Layout

```text
{root}/ab/cdef…       ← blob bytes  (ab = first 2 hex of the blake3 hash)
{root}/ab/cdef….mime  ← sidecar · the caller's declared mime
```

## Surface

| method | notes |
|---|---|
| `put(data, mime)` | blake3 hash · dedup · `.mime` sidecar · `TooLarge` over cap (default 500 MiB) · empty rejected |
| `get(hash)` | bytes by hash (`blake3:` prefix optional · case-insensitive hex) · `NotFound` if absent or malformed |
| `exists(hash)` | bool · I/O errors → false |
| `stat(hash)` | size + sidecar mime (fallback `application/octet-stream`) |
| `delete(hash)` | removes bytes + sidecar · `NotFound` if already absent (not a silent no-op · sidecar unlink is best-effort) |

Implements the `BlobStoreDyn` (`Send`-future) companion; the base
`BlobStore` arrives via the `trait_variant` blanket impl. No error enum
of its own — speaks the kernel `BlobError` (`NotFound`/`Io`/`TooLarge`).

The **sidecar** records the declared mime so `put`/`stat` round-trip
(brouillon re-sniffed and diverged). The store records, it never
guesses — content sniffing is a higher-layer concern.

---

AGPL-3.0-or-later · SuperNovae Studio · 🦋
