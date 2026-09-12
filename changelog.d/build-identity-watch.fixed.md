- **Stable incremental builds.** Build identity watches only existing Git
  paths. An absent `packed-refs` file previously made every Cargo invocation
  rebuild the runtime, including every CLI probe in the WASM differential.
  Packed branches watch their existing ref directory so a new loose ref still
  refreshes the embedded commit. Both build-script consumers also watch their
  shared helper source so edits invalidate cached build scripts.
