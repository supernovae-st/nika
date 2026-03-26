# Research: Rust CLI Release Pipelines (Top Projects 2025-2026)

**Date**: 2026-03-26
**Methodology**: Direct analysis of `.github/workflows/` from 7 top Rust CLI projects
**Pages analyzed**: 40+ workflow YAML files across 7 repositories
**Confidence**: High -- based on actual source code, not documentation

---

## Executive Summary

The best Rust CLI projects in 2025-2026 share common release patterns: matrix-based cross-compilation (8-20 targets), multi-channel distribution (npm + GitHub Releases + package managers), build provenance attestation, and automated release-please or changeset-driven versioning. The most innovative patterns come from **Mise** (widest distribution), **Biome** (npm platform packages + Depot runners), **Ruff** (cargo-dist + maturin dual pipeline), and **OXC** (NAPI bindings for 19 targets including Android/RISC-V/OpenHarmony).

---

## 1. Biome (biomejs/biome)

**Repository**: https://github.com/biomejs/biome
**Workflows**: 32 files

### Distribution Channels
| Channel | Method |
|---------|--------|
| npm | Platform-specific `@biomejs/cli-*` packages (8 variants) |
| VS Code Marketplace | Separate `release_cli.yml` workflow |
| crates.io | `publish-crates.yml` |
| GitHub Releases | Binary artifacts |
| JS API (WASM) | `release_js_api.yml` via wasm-pack |

### Build Matrix (8 targets)
```
x86_64-pc-windows-msvc       (win32-x64)
aarch64-pc-windows-msvc      (win32-arm64)
x86_64-unknown-linux-musl    (linux-x64-musl)
aarch64-unknown-linux-musl   (linux-arm64-musl)
x86_64-unknown-linux-gnu     (linux-x64)      -- built in Docker/Debian 11 for glibc compat
aarch64-unknown-linux-gnu    (linux-arm64)     -- built in Docker/Debian 11 for glibc compat
x86_64-apple-darwin          (darwin-x64)
aarch64-apple-darwin         (darwin-arm64)
```

### Key Patterns
- **Changesets for versioning**: Uses `changesets/action` on push to main. When changesets are consumed (merged release PR), triggers binary builds.
- **Depot runners**: Uses `depot-ubuntu-24.04-arm-16` and `depot-macos-14` instead of GitHub-hosted runners for faster ARM builds.
- **RUSTFLAGS tuning**: `-C strip=symbols -C codegen-units=1` for smallest binaries.
- **Build provenance**: `actions/attest-build-provenance` on every binary.
- **cargo-audit in CI**: Runs `cargo audit` as part of the release build (not just CI).
- **Version inlined**: `BIOME_VERSION` env var baked into binary at compile time.
- **jemalloc page size**: Sets `JEMALLOC_SYS_WITH_LG_PAGE=16` for ARM64 Linux.
- **Dual GNU/musl**: Builds both musl (static, portable) and GNU (glibc, Docker-friendly) Linux binaries.

### What Makes It Good
- Clean separation: `release.yml` (orchestrator) -> `release_cli.yml` (emergency/manual) -> `publish-crates.yml` (library crates)
- JS API builds WASM (bundler + node + web targets) via wasm-pack
- Version check uses `EndBug/version-check` against `package.json`

---

## 2. Turbo (vercel/turborepo)

**Repository**: https://github.com/vercel/turborepo
**Workflows**: 14 files

### Distribution Channels
| Channel | Method |
|---------|--------|
| npm | Primary distribution via `npm publish` with OIDC trusted publishing |
| GitHub Releases | Git tags |

### Release Pipeline (9 steps)
```
1. Create staging branch (acts as lock to prevent concurrent releases)
2. Run smoke tests on staging branch
3. Build Rust binary (cross-platform)
4. Run security audits (cargo audit + pnpm audit)
5. Publish JS packages to npm (including turbo itself)
6. Create git tag (ONLY after npm publish succeeds)
7. Alias versioned docs (e.g., v2-5-4.turborepo.dev)
8. Create release branch and open PR
9. On failure, cleanup staging branch and release tag
```

### Key Patterns
- **Staging branch as lock**: Creates `staging-{version}` branch to prevent concurrent releases. Has `clear-staging-branch` escape hatch for recovery.
- **Hourly canary releases**: `cron: "0 * * * *"` schedule for canary builds from main.
- **Smart skip detection**: Uses GitHub API (not git clone) to check if relevant files changed since last release. Avoids unnecessary canary builds.
- **SemVer increment as input**: `workflow_dispatch` with choices: prerelease/prepatch/preminor/premajor/patch/minor/major.
- **npm before tag**: Tag only created AFTER npm publish succeeds. If publish fails, no orphaned tag.
- **Auto-cleanup on failure**: Deletes staging branch and release tag automatically.
- **Release PR with auto-merge**: Creates PR to merge staging back to main, posts synthetic check statuses for required checks.
- **LTO enabled**: `CARGO_PROFILE_RELEASE_LTO: true` for release builds.
- **Turbobot identity**: `git config user.name 'Turbobot'` for release commits.
- **Stale canary detection**: Checks if stable version already published before creating canary.

### What Makes It Good
- Most sophisticated failure recovery of any project
- Staging branch pattern prevents race conditions
- Canary releases every hour keep bleeding-edge users happy
- Versioned docs aliasing is integrated into release

---

## 3. OXC / Oxlint (oxc-project/oxc)

**Repository**: https://github.com/oxc-project/oxc
**Workflows**: 27 files

### Distribution Channels
| Channel | Method |
|---------|--------|
| npm | Platform packages via NAPI-RS (oxlint, oxfmt, parser, transform, minify) |
| crates.io | `release_crates.yml` via custom `cargo-release-oxc` tool |
| GitHub Releases | Archived binaries per target |

### Build Matrix (19 targets -- widest of any project)
```
x86_64-pc-windows-msvc           aarch64-pc-windows-msvc
i686-pc-windows-msvc             x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu        armv7-unknown-linux-gnueabihf
x86_64-unknown-linux-musl        aarch64-unknown-linux-musl
armv7-unknown-linux-musleabihf   x86_64-apple-darwin
aarch64-apple-darwin             aarch64-linux-android
armv7-linux-androideabi          aarch64-unknown-linux-ohos
powerpc64le-unknown-linux-gnu    riscv64gc-unknown-linux-gnu
riscv64gc-unknown-linux-musl     s390x-unknown-linux-gnu
x86_64-unknown-freebsd           wasm32-wasip1-threads
```

### Key Patterns
- **NAPI-RS for npm distribution**: Builds native Node.js addon (.node files) for each platform. Users install `oxlint` npm package, gets correct binary automatically.
- **Custom release tool**: `cargo-release-oxc` manages workspace publish order and changelog generation.
- **crates-io-auth-action**: Uses OIDC token exchange (`rust-lang/crates-io-auth-action`) for crates.io publishing -- no long-lived token.
- **FreeBSD builds in VM**: Uses `cross-platform-actions/action` to build in FreeBSD 14.2 VM with 8GB RAM.
- **cargo-zigbuild for RISC-V musl**: Falls back to `cargo-zigbuild` when `cross` doesn't support a target.
- **No Rust cache on release builds**: Explicitly avoids caching to prevent cache poisoning attacks. Comment: "No Rust cache is provided, to avoid cache poisoning attack."
- **Reusable workflow pattern**: `reusable_release_napi.yml` is called by multiple release workflows (parser, transform, minify).
- **Separate app vs crate releases**: `release_apps.yml` (binary + npm) vs `release_crates.yml` (library crates to crates.io).
- **Version check via unpkg**: Compares local `package.json` version against `https://unpkg.com/oxlint@latest/package.json`.
- **`pnpm publish --provenance`**: npm publish with OIDC provenance.
- **Archive binaries**: Zips binaries to fix GitHub Actions permission loss bug.

### What Makes It Good
- Most targets of any project (19), including Android, OpenHarmony, RISC-V, s390x
- NAPI-RS pattern is the gold standard for shipping Rust to Node.js
- Security-conscious: no cache on release builds, OIDC auth for crates.io
- Custom `cargo-release-oxc` tool handles workspace dependency ordering

---

## 4. Starship (starship/starship)

**Repository**: https://github.com/starship/starship
**Workflows**: 8 files (leanest of all projects)

### Distribution Channels
| Channel | Method |
|---------|--------|
| GitHub Releases | Archived binaries + MSI + .pkg installers |
| install.sh | `curl -sS https://starship.rs/install.sh \| sh` |
| Homebrew | Via community tap |
| Scoop/Chocolatey | Via community |
| Windows MSI | Built with `cargo-wix` |
| macOS .pkg | Built and **notarized** with Apple Developer ID |

### Build Matrix (11 targets)
```
x86_64-unknown-linux-gnu         x86_64-unknown-linux-musl
i686-unknown-linux-musl          aarch64-unknown-linux-musl
arm-unknown-linux-musleabihf     x86_64-apple-darwin
aarch64-apple-darwin             x86_64-pc-windows-msvc
i686-pc-windows-msvc             aarch64-pc-windows-msvc
x86_64-unknown-freebsd
```

### Key Patterns
- **Release Please**: Uses `googleapis/release-please-action` with `release-type: rust`. Automatically creates release PRs with changelogs.
- **macOS code signing + notarization**: Full Apple Developer ID workflow -- imports certificates, creates keychain, signs binary, notarizes with `notarytool`, builds `.pkg` installer.
- **Windows code signing**: Uses `signpath/github-action-submit-signing-request` for Windows binary and MSI signing.
- **Windows MSI installer**: `cargo-wix` generates Windows Installer packages.
- **SHA256 checksums**: `openssl dgst -sha256` for every artifact.
- **install.sh is tested in CI**: Dedicated `install-script.yml` workflow runs shellcheck, shfmt, and actual installation tests.
- **Crowdin integration**: Merges translations before release, macOS .pkg includes translated docs.
- **`cross` for Linux**: Uses `taiki-e/install-action@cross` for Linux cross-compilation.
- **Static Windows binaries**: `RUSTFLAGS: -C target-feature=+crt-static`.

### What Makes It Good
- The only project that does macOS notarization + Windows code signing
- install.sh is tested in CI (shellcheck + functional tests)
- Release Please automates versioning and changelogs with zero manual work
- Lean: 8 workflows total, clean and readable
- `.pkg` installers for macOS (not just tarballs)

---

## 5. Zed (zed-industries/zed)

**Repository**: https://github.com/zed-industries/zed
**Workflows**: 41 files (most complex)

### Distribution Channels
| Channel | Method |
|---------|--------|
| Direct download | zed.dev website |
| GitHub Releases | Draft releases with generated notes |

### Key Patterns
- **Workflows generated from code**: `cargo xtask workflows` generates `.github/workflows/*.yml` from Rust code. CI verifies no uncommitted changes after generation.
- **sccache with R2**: Uses Cloudflare R2 for shared compilation cache across all CI jobs. Massive speed improvement.
- **Namespace runners**: Uses `namespace-profile-mac-large`, `namespace-profile-16x32-ubuntu-2204` instead of GitHub-hosted runners.
- **Self-hosted Windows**: `self-32vcpu-windows-2022` with 32 vCPUs.
- **Nightly releases**: Separate `release_nightly.yml` workflow.
- **Draft release pattern**: Creates draft release first, builds attach artifacts, then publishes.
- **Release channel detection**: `script/determine-release-channel` determines stable/preview/nightly.
- **Release notes from JS**: `node ./script/draft-release-notes` generates notes.
- **Bundling step**: Separate `run_bundling.yml` for creating .dmg/.deb/.rpm packages.
- **Extension CLI**: `publish_extension_cli.yml` for the Zed extension ecosystem.
- **Agent evals**: `run_agent_evals.yml` -- runs AI agent evaluations as part of CI.
- **actionlint in CI**: Lints GitHub Actions workflows themselves.

### What Makes It Good
- **Workflow-as-code**: `cargo xtask workflows` is brilliant -- workflows are generated from Rust, not hand-written YAML
- sccache + R2 eliminates redundant compilation across 40+ workflows
- Agent evaluation in CI is cutting-edge (AI testing AI)
- Most comprehensive CI (tests + clippy + scripts + bundling on all 3 platforms)

---

## 6. Mise (jdx/mise)

**Repository**: https://github.com/jdx/mise
**Workflows**: 22 files

### Distribution Channels (WIDEST -- 12 channels)
| Channel | Workflow |
|---------|----------|
| GitHub Releases | `release.yml` |
| npm | `npm-publish.yml` (@jdxcode/mise) |
| Docker Hub | `docker.yml` (jdxcode/mise) |
| GHCR | `docker.yml` (ghcr.io/jdx/mise) |
| Homebrew | Via Homebrew core formula |
| crates.io | `release-plz.yml` |
| RPM (Fedora/EPEL) | `release.yml` (builds .rpm in container) |
| DEB (Debian/Ubuntu) | `release.yml` (builds .deb in container) |
| Alpine (APKBUILD) | `release-alpine.yml` |
| Snapcraft | `snapcraft-publish.yml` |
| WinGet | `winget.yml` |
| COPR (Fedora) | `copr-publish.yml` |

### Build Matrix (9 targets)
```
x86_64-unknown-linux-gnu         x86_64-unknown-linux-musl
aarch64-unknown-linux-gnu        aarch64-unknown-linux-musl
armv7-unknown-linux-gnueabi      armv7-unknown-linux-musleabi
x86_64-apple-darwin              aarch64-apple-darwin
x86_64-pc-windows-msvc           aarch64-pc-windows-msvc
```

### Key Patterns
- **release-plz for Rust crates**: Runs daily on cron and on push to main. Handles crates.io publishing with GPG-signed tags.
- **GPG signing everywhere**: Binary signing with `zipsign`, git tag signing with GPG.
- **macOS code signing**: `apple-actions/import-codesign-certs` for Apple Developer ID.
- **Retry with `nick-fields/retry`**: Build and test steps retry up to 3 times (flaky cross-compilation).
- **Multiple compression formats**: Produces `.tar.xz`, `.tar.gz`, `.tar.zst` for every target.
- **E2E test tranche**: Splits e2e tests into 8 parallel tranches for speed.
- **Dedicated packaging containers**: `ghcr.io/jdx/mise:rpm`, `ghcr.io/jdx/mise:deb`, `ghcr.io/jdx/mise:alpine`, `ghcr.io/jdx/mise:copr` for reproducible packaging.
- **Docker multi-arch**: Builds linux/amd64 and linux/arm64, creates multi-arch manifest.
- **WinGet automation**: `vedantmgoyal9/winget-releaser` updates Microsoft winget-pkgs repo.
- **Alpine APKBUILD**: Submits to Alpine Linux community repository via GitLab.
- **COPR**: Builds RPMs for Fedora rawhide/43/42 and EPEL 9/10 across x86_64 and aarch64.
- **Snapcraft**: Publishes to Snap Store (beta channel).
- **DRY_RUN mode**: Most workflows support dry-run via `workflow_dispatch`.
- **cargo-cache cleanup**: `cargo cache --autoclean` to reduce cache size.

### What Makes It Good
- **12 distribution channels** -- more than any other project
- Every Linux packaging format covered (deb, rpm, apk, snap, copr)
- Windows covered via WinGet
- macOS covered via Homebrew + direct download
- Docker on both DockerHub and GHCR with multi-arch manifests
- Dedicated container images for each packaging format ensures reproducibility

---

## 7. Ruff (astral-sh/ruff)

**Repository**: https://github.com/astral-sh/ruff
**Workflows**: 19 files

### Distribution Channels
| Channel | Workflow |
|---------|----------|
| PyPI | `publish-pypi.yml` via maturin + uv publish |
| GitHub Releases | `release.yml` via cargo-dist |
| Docker | `build-docker.yml` (ghcr.io/astral-sh/ruff) |
| WASM Playground | `publish-wasm.yml` |
| Homebrew | Via community |

### Build Matrix (10 targets for wheels)
```
x86_64-apple-darwin              aarch64-apple-darwin
x86_64-pc-windows-msvc           i686-pc-windows-msvc
aarch64-pc-windows-msvc          x86_64-unknown-linux-gnu (manylinux)
x86_64-unknown-linux-musl        aarch64-unknown-linux-gnu (manylinux)
aarch64-unknown-linux-musl       armv7-unknown-linux-gnueabihf
i686-unknown-linux-gnu
```

### Key Patterns
- **cargo-dist for release orchestration**: `release.yml` is generated by cargo-dist v0.31.0. Handles plan -> build -> host -> announce lifecycle.
- **maturin for Python wheels**: Uses `PyO3/maturin-action` to build Python wheels from Rust source. Each platform gets a native wheel.
- **Dual artifact strategy**: Each build produces BOTH a Python wheel (for PyPI) AND an archived binary (for GitHub Releases).
- **PyPI Trusted Publishing**: Uses OIDC `id-token: write` for PyPI publishing -- no stored API tokens.
- **uv for publishing**: `astral-sh/setup-uv` -> `uv publish wheels/*` (dogfooding their own tools).
- **Docker multi-arch**: Builds linux/amd64 and linux/arm64 Docker images separately, then creates merged manifest.
- **sdist + wheel testing**: Installs the built wheel and runs `ruff --help` to verify it works.
- **README transformation**: `python scripts/transform_readme.py --target pypi` for PyPI-specific README.
- **Build provenance attestation**: `actions/attest-build-provenance` on release artifacts.
- **Per-binary SHA256**: `shasum -a 256 $ARCHIVE_FILE > $ARCHIVE_FILE.sha256` for each artifact.
- **Depot runners**: Uses `depot-ubuntu-latest-4` for CI.

### What Makes It Good
- **cargo-dist integration** eliminates most boilerplate -- the release workflow is mostly generated
- maturin bridges Rust -> Python perfectly (native wheels, not just wrappers)
- Dogfoods their own tools (uv for publishing)
- Clean separation of concerns via reusable workflows (`build-binaries.yml`, `build-docker.yml`, `publish-pypi.yml`)

---

## Cross-Project Comparison

### Distribution Channels

| Channel | Biome | Turbo | OXC | Starship | Zed | Mise | Ruff |
|---------|:-----:|:-----:|:---:|:--------:|:---:|:----:|:----:|
| GitHub Releases | Y | Y | Y | Y | Y | Y | Y |
| npm | Y | Y | Y | - | - | Y | - |
| crates.io | Y | - | Y | - | - | Y | - |
| PyPI | - | - | - | - | - | - | Y |
| Docker | - | - | - | - | - | Y | Y |
| Homebrew | - | - | - | Y* | - | Y* | Y* |
| WinGet | - | - | - | - | - | Y | - |
| Snapcraft | - | - | - | - | - | Y | - |
| DEB | - | - | - | - | Y | Y | - |
| RPM | - | - | - | - | - | Y | - |
| Alpine APK | - | - | - | - | - | Y | - |
| COPR | - | - | - | - | - | Y | - |
| install.sh | - | - | - | Y | - | Y | Y |
| VS Code | Y | - | - | - | - | - | - |
| WASM | Y | - | Y | - | - | - | Y |
| MSI installer | - | - | - | Y | - | - | - |
| .pkg installer | - | - | - | Y | - | - | - |

*Y* = via community, not official workflow

### Build Targets

| Feature | Biome | Turbo | OXC | Starship | Zed | Mise | Ruff |
|---------|:-----:|:-----:|:---:|:--------:|:---:|:----:|:----:|
| Target count | 8 | ~6 | **19** | 11 | 3 | 9 | 10 |
| musl | Y | - | Y | Y | - | Y | Y |
| FreeBSD | - | - | Y | Y | - | - | - |
| Android | - | - | Y | - | - | - | - |
| RISC-V | - | - | Y | - | - | - | - |
| ARM32 | - | - | Y | Y | - | Y | Y |
| s390x | - | - | Y | - | - | - | - |
| OpenHarmony | - | - | Y | - | - | - | - |
| WASM | Y | - | Y | - | - | - | Y |

### Release Automation

| Feature | Biome | Turbo | OXC | Starship | Zed | Mise | Ruff |
|---------|:-----:|:-----:|:---:|:--------:|:---:|:----:|:----:|
| Versioning | Changesets | Manual | package.json | Release Please | Tag push | release-plz | cargo-dist |
| Canary/Nightly | Y | Y (hourly) | - | - | Y | - | - |
| Code signing (macOS) | - | - | - | **Y** | - | Y | - |
| Code signing (Windows) | - | - | - | **Y** | - | - | - |
| Build provenance | Y | - | - | - | - | - | Y |
| OIDC publishing | - | Y (npm) | Y (crates.io) | - | - | - | Y (PyPI) |
| Failure cleanup | - | **Y** | - | - | - | - | - |
| Smart skip | - | **Y** | - | - | - | - | - |
| Workflow-as-code | - | - | - | - | **Y** | - | Y* |

*Y* = cargo-dist generates the workflow

### CI Infrastructure

| Feature | Biome | Turbo | OXC | Starship | Zed | Mise | Ruff |
|---------|:-----:|:-----:|:---:|:--------:|:---:|:----:|:----:|
| Runner type | Depot | GitHub | GitHub | GitHub | Namespace + self-hosted | GitHub | Depot |
| Build cache | - | - | - | - | sccache+R2 | cargo cache | - |
| Security audit | cargo-audit | cargo+pnpm audit | - | security-audit.yml | - | - | - |
| E2E in release | - | Y | - | - | Y | **Y (8 tranches)** | - |

---

## Innovative Patterns Worth Copying

### 1. Staging Branch Lock (Turbo)
Prevents concurrent releases by creating a `staging-{version}` branch. If a release fails, cleanup deletes the branch. Includes `clear-staging-branch` escape hatch.

### 2. Workflow-as-Code (Zed)
`cargo xtask workflows` generates GitHub Actions YAML from Rust code. CI verifies generated files match committed files. Eliminates YAML drift.

### 3. cargo-dist (Ruff)
Single tool generates the entire release workflow. `dist plan` -> `dist build` -> `dist host` -> `dist announce`. Handles installers, checksums, GitHub Releases, and more.

### 4. NAPI-RS Platform Packages (OXC)
For npm distribution of Rust binaries: each platform gets a separate npm package (`@oxlint/linux-x64-musl`, etc.), and the main package auto-selects the right one.

### 5. release-plz + GPG Signing (Mise)
Daily cron runs `release-plz` which auto-detects version bumps, generates changelogs, and publishes to crates.io with GPG-signed tags.

### 6. macOS Notarization (Starship)
Full Apple Developer ID workflow: certificate import -> keychain creation -> binary signing -> notarization -> `.pkg` build. The gold standard for macOS distribution.

### 7. No Cache on Release Builds (OXC)
Explicitly disables cargo cache on release builds to prevent cache poisoning. A security best practice most projects miss.

### 8. Hourly Canary + Smart Skip (Turbo)
Canary releases run hourly, but check via GitHub API whether relevant files changed since last release. Zero-cost when nothing changed.

### 9. Dedicated Packaging Containers (Mise)
Pre-built Docker images for each packaging format (`ghcr.io/jdx/mise:rpm`, `:deb`, `:alpine`, `:copr`). Ensures reproducible package builds.

### 10. Build Provenance Attestation (Biome, Ruff)
`actions/attest-build-provenance` creates SLSA-compliant attestation for every binary. Emerging standard for supply chain security.

---

## Recommendations for Nika

Based on this analysis, here are the most relevant patterns for Nika's distribution:

### Phase 1: Foundation
1. **cargo-dist** for release workflow generation (like Ruff) -- eliminates boilerplate
2. **Build matrix**: Start with 8 targets (linux x64/arm64 musl+gnu, macOS x64/arm64, Windows x64/arm64)
3. **GitHub Releases** with SHA256 checksums and build provenance attestation
4. **install.sh** script tested in CI (like Starship)

### Phase 2: Package Managers
5. **Homebrew tap** (official, not community) -- `supernovae-st/homebrew-tap`
6. **npm platform packages** via NAPI-RS pattern (like OXC/Biome) if targeting Node.js users
7. **crates.io** via release-plz with OIDC auth

### Phase 3: Wider Distribution
8. **Docker** on GHCR with multi-arch manifests
9. **WinGet** via `vedantmgoyal9/winget-releaser`
10. **AUR** (Arch Linux) for the Linux power-user crowd

### Cargo Workspace Publish Order
For Nika's 10-crate workspace, use **topological dependency order**:
```
1. nika-core       (zero deps)
2. nika-event      (depends on core)
3. nika-mcp        (depends on core)
4. nika-media      (depends on core)
5. nika-lsp-core   (depends on core)
6. nika-engine     (depends on core, event, mcp, media)
7. nika-cli        (depends on engine)
8. nika-tui        (depends on engine)
9. nika-lsp        (depends on lsp-core)
10. nika            (binary, depends on all)
```

Use `cargo-release-oxc`-style custom tool or `release-plz` with `--packages` flag.
Wait 30s between publishes for crates.io index propagation.

---

## Sources
1. [biomejs/biome/.github/workflows/](https://github.com/biomejs/biome/tree/main/.github/workflows) -- 32 workflow files
2. [vercel/turborepo/.github/workflows/](https://github.com/vercel/turborepo/tree/main/.github/workflows) -- 14 workflow files
3. [oxc-project/oxc/.github/workflows/](https://github.com/oxc-project/oxc/tree/main/.github/workflows) -- 27 workflow files
4. [starship/starship/.github/workflows/](https://github.com/starship/starship/tree/master/.github/workflows) -- 8 workflow files
5. [zed-industries/zed/.github/workflows/](https://github.com/zed-industries/zed/tree/main/.github/workflows) -- 41 workflow files
6. [jdx/mise/.github/workflows/](https://github.com/jdx/mise/tree/main/.github/workflows) -- 22 workflow files
7. [astral-sh/ruff/.github/workflows/](https://github.com/astral-sh/ruff/tree/main/.github/workflows) -- 19 workflow files
