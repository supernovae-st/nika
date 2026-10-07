- **Scaleway has a distinct API provider and saved intelligence choice.**
  Its credentials and project endpoint remain separate from OpenAI. The exact
  hosted model keeps observed usage and an unknown price. The exported Rust
  `CANONICAL_IDS` array changes from 17 to 18 entries; consumers that name its
  array length must update with this release.
  Explicit compatible API endpoints remain supported by admission and SDK
  export; a selected ACP/harness path cannot borrow an unused API endpoint.
