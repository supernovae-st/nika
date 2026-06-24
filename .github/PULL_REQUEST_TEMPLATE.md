## What & why

<!-- One paragraph: what this change does and the motivation. -->

## Checklist

- [ ] Conventional commit + `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` green
- [ ] `cargo fmt --check` clean
- [ ] 0 `.unwrap()` / `.expect()` in `src/`
- [ ] `bash scripts/hygiene/check-all.sh` green
- [ ] Admitting a crate? All **12 gates** green in this PR (see [`CONTRIBUTING.md`](../CONTRIBUTING.md))

<!-- Nika has a public release-candidate. This template documents the bar every
     change clears before it reaches users. -->
