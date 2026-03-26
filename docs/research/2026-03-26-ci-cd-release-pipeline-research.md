# Research Report: Rust Project CI/CD Release Pipeline Best Practices (2025-2026)

**Date**: 2026-03-26
**Researcher**: Claude Opus 4.6 (1M context)
**Scope**: 7 distribution channels for Nika CLI

---

## Summary

This report covers concrete, actionable best practices for shipping a Rust CLI binary through 7 distribution channels: crates.io, npm, VS Code Marketplace, GitHub Releases, install.sh, Docker, and Homebrew. The analysis draws from real production workflows of biome, starship, mise, proto/moonrepo, and tauri -- the leading Rust CLI projects distributing via these channels in 2025-2026.

---

## 1. crates.io Publishing in CI

### release-plz vs cargo-release vs cargo-dist vs manual

| Tool | Best For | Workspace Support | CHANGELOG | GitHub Release | npm/Homebrew |
|------|----------|-------------------|-----------|----------------|--------------|
| **release-plz** | Automated PR-based flow | Full workspace, topo order | git-cliff integration | Yes | No (separate job) |
| **cargo-release** | Manual/semi-auto releases | Workspace-aware | No built-in | No | No |
| **cargo-dist** (axo) | Full artifact pipeline | Via `dist plan` | Built-in | Yes + installers | Homebrew yes, npm no |
| **Manual** | Full control | You implement it | You implement it | You implement it | You implement it |

### Verdict for Nika: release-plz is the right choice

You already have release-plz configured. It is the best fit for workspace crates because:

1. **Topological publish order**: release-plz automatically resolves workspace dependency order. When you have `nika-core -> nika-engine -> nika`, it publishes `nika-core` first, waits for crates.io index propagation, then publishes dependents.

2. **Conventional commit parsing**: Bumps versions based on `feat:` (minor), `fix:` (patch), `feat!:` (major).

3. **PR-based workflow**: Creates a "Release PR" with version bumps + CHANGELOG updates. Merging it triggers the release.

### Secrets Required

```
CARGO_REGISTRY_TOKEN  - crates.io API token (Settings > API Tokens)
GITHUB_TOKEN          - Auto-provided, but for PRs that trigger workflows,
                        use a GitHub App token (APP_ID + APP_PRIVATE_KEY)
```

**Critical**: The default `GITHUB_TOKEN` cannot trigger other workflows. If your release PR merge should trigger `release.yml`, you need a GitHub App token. release-plz's own repo demonstrates this pattern:

```yaml
- name: Generate GitHub token
  uses: actions/create-github-app-token@v3
  id: generate-token
  with:
    app-id: ${{ secrets.APP_ID }}
    private-key: ${{ secrets.APP_PRIVATE_KEY }}
    permission-contents: write
    permission-pull-requests: write
```

### Workspace Version Sync

Your current setup uses `[workspace.package] version = "0.46.1"` with individual crate versions at `"0.46.0"` in `[workspace.dependencies]`. This is the correct pattern -- workspace crates can inherit `version.workspace = true` while internal dependency versions can lag slightly.

**Recommendation**: For Nika's "version lock" policy (never >= 1.0.0), add to `release-plz.toml`:

```toml
[workspace]
# Prevent major version bumps
allow_major_updates = false
```

### Real release-plz.yml (production pattern from release-plz's own repo)

```yaml
name: Release-plz

on:
  push:
    branches: [main]

permissions: {}

jobs:
  release-plz-release:
    name: Release
    runs-on: ubuntu-24.04
    environment: release-plz
    permissions:
      contents: read
      id-token: write  # Required for trusted publishing on crates.io
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false

      - run: rustup update stable

      # GitHub App token so tags trigger release.yml
      - uses: actions/create-github-app-token@v3
        id: generate-token
        with:
          app-id: ${{ secrets.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}
          permission-contents: write

      - uses: release-plz/action@v0.5
        with:
          command: release
        env:
          GITHUB_TOKEN: ${{ steps.generate-token.outputs.token }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

  release-plz-pr:
    name: Release PR
    runs-on: ubuntu-24.04
    environment: release-plz
    permissions:
      contents: read
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false

      - run: rustup update stable

      - uses: actions/create-github-app-token@v3
        id: generate-token
        with:
          app-id: ${{ secrets.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}
          permission-contents: write
          permission-pull-requests: write

      - uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ steps.generate-token.outputs.token }}
```

### Key improvement for your release-plz.toml

```toml
[workspace]
changelog_config = "cliff.toml"
git_release_enable = true
pr_labels = ["release", "automated", "armada"]
semver_check = true
publish_timeout = "10m"
allow_dirty = true
dependencies_update = true
git_tag_name = "v{{ version }}"
git_release_name = "v{{ version }}"
repo_url = "https://github.com/supernovae-st/nika"

# NEW: Prevent 1.0 from ever happening
allow_major_updates = false

# Publish all workspace crates (release-plz handles topo order)
# When publish = true, it runs cargo publish for each crate
[[package]]
name = "nika-core"
publish = true

[[package]]
name = "nika-event"
publish = true

[[package]]
name = "nika-media"
publish = true

[[package]]
name = "nika-mcp"
publish = true

[[package]]
name = "nika-engine"
publish = true

[[package]]
name = "nika-cli"
publish = true

[[package]]
name = "nika-tui"
publish = true

[[package]]
name = "nika"
publish = true
changelog_path = "CHANGELOG.md"
semver_check = true
git_tag_name = "v{{ version }}"
```

### crates.io Trusted Publishing (new in 2025)

crates.io now supports OIDC trusted publishing from GitHub Actions, similar to PyPI. This eliminates the need for long-lived `CARGO_REGISTRY_TOKEN`:

```yaml
permissions:
  id-token: write  # Required for trusted publishing
```

Configure at https://crates.io/settings/tokens -- link your GitHub repo. Then `cargo publish` works without a token. release-plz supports this via the `id-token: write` permission.

---

## 2. npm Binary Wrapper for Rust CLIs

### Two Patterns in Production

#### Pattern A: Platform-specific optionalDependencies (Biome, oxlint, Rolldown)

This is the **dominant pattern in 2025-2026** and the one you should adopt.

**How it works:**
- One main package `@supernovae/nika` with a JS bin shim
- 8 platform-specific packages as `optionalDependencies`
- npm/pnpm/yarn only install the one matching the current platform
- The bin shim resolves the correct native binary via `require.resolve()`

**Biome's main package.json:**
```json
{
  "name": "@biomejs/biome",
  "version": "2.4.9",
  "bin": { "biome": "bin/biome" },
  "optionalDependencies": {
    "@biomejs/cli-win32-x64": "2.4.9",
    "@biomejs/cli-win32-arm64": "2.4.9",
    "@biomejs/cli-darwin-x64": "2.4.9",
    "@biomejs/cli-darwin-arm64": "2.4.9",
    "@biomejs/cli-linux-x64": "2.4.9",
    "@biomejs/cli-linux-arm64": "2.4.9",
    "@biomejs/cli-linux-x64-musl": "2.4.9",
    "@biomejs/cli-linux-arm64-musl": "2.4.9"
  },
  "publishConfig": {
    "provenance": true
  }
}
```

**Platform package (e.g., `@biomejs/cli-darwin-arm64`):**
```json
{
  "name": "@biomejs/cli-darwin-arm64",
  "version": "2.4.9",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "publishConfig": { "provenance": true }
}
```

The package contains just the binary and `package.json`. The `os` and `cpu` fields tell npm to only install it on matching platforms.

**Biome's bin shim** (`bin/biome`):
```javascript
#!/usr/bin/env node
const { platform, arch, env, version, release } = process;

function isMusl() {
  let stderr;
  try {
    stderr = require("child_process").execSync("ldd --version", {
      stdio: ["pipe", "pipe", "pipe"]
    });
  } catch (err) {
    stderr = err.stderr;
  }
  return stderr && stderr.indexOf("musl") > -1;
}

const PLATFORMS = {
  win32:        { x64: "@biomejs/cli-win32-x64/biome.exe",
                  arm64: "@biomejs/cli-win32-arm64/biome.exe" },
  darwin:       { x64: "@biomejs/cli-darwin-x64/biome",
                  arm64: "@biomejs/cli-darwin-arm64/biome" },
  linux:        { x64: "@biomejs/cli-linux-x64/biome",
                  arm64: "@biomejs/cli-linux-arm64/biome" },
  "linux-musl": { x64: "@biomejs/cli-linux-x64-musl/biome",
                  arm64: "@biomejs/cli-linux-arm64-musl/biome" },
};

const key = platform === "linux" && isMusl() ? "linux-musl" : platform;
const binPath = env.BIOME_BINARY || PLATFORMS?.[key]?.[arch];

if (binPath) {
  const result = require("child_process").spawnSync(
    require.resolve(binPath),
    process.argv.slice(2),
    { shell: false, stdio: "inherit" }
  );
  if (result.error) throw result.error;
  process.exitCode = result.status;
} else {
  console.error("Unsupported platform: " + platform + "-" + arch);
  process.exitCode = 1;
}
```

#### Pattern B: postinstall download (your current approach, also used by older tools)

Your current `packages/npm/` uses a `postinstall` script that downloads from GitHub Releases. This works but has significant drawbacks:

| Aspect | optionalDependencies (A) | postinstall download (B) |
|--------|--------------------------|--------------------------|
| Offline install | Works (binary is in npm cache) | Fails |
| CI caching | npm cache includes binary | Must download every time |
| Corporate proxy | Works through npm config | May fail on GitHub URLs |
| pnpm/yarn/bun | Full support | postinstall may be blocked |
| Lockfile | Deterministic | Download URL is version-pinned |
| npm provenance | Supported | N/A (binary from GitHub) |
| Package size | Each platform pkg ~10-20MB | Main pkg is tiny |
| Windows | First-class | Often broken |

### Recommendation: Migrate to Pattern A

For Nika, create these packages:

```
packages/npm/
  @supernovae/nika/              # Main package (bin shim)
  @supernovae/nika-darwin-arm64/ # macOS Apple Silicon
  @supernovae/nika-darwin-x64/   # macOS Intel
  @supernovae/nika-linux-x64/    # Linux x64 (glibc)
  @supernovae/nika-linux-arm64/  # Linux ARM64 (glibc)
  @supernovae/nika-linux-x64-musl/   # Linux x64 (musl/Alpine)
  @supernovae/nika-linux-arm64-musl/  # Linux ARM64 (musl)
  @supernovae/nika-win32-x64/    # Windows x64
```

**Biome's generate-packages.mjs approach** -- a Node script that:
1. Reads version from the root package.json
2. For each platform: creates/updates package.json, copies binary, sets `os`/`cpu`/`libc` fields
3. CI then publishes each package with `npm publish --access public --provenance`

**CI workflow addition** (in release.yml, after build job):

```yaml
  npm-publish:
    name: Publish npm packages
    needs: [build, release]
    if: ${{ !inputs.dry_run }}
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write  # npm provenance
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ env.RELEASE_TAG }}

      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          registry-url: 'https://registry.npmjs.org'

      - name: Download all build artifacts
        uses: actions/download-artifact@v8
        with:
          path: artifacts

      - name: Extract version
        id: version
        run: echo "version=${RELEASE_TAG#v}" >> $GITHUB_OUTPUT

      - name: Generate platform packages
        run: node scripts/generate-npm-packages.mjs ${{ steps.version.outputs.version }}

      - name: Publish all npm packages
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: |
          # Publish platform packages first
          for pkg in packages/npm/@supernovae/nika-*; do
            echo "Publishing $pkg..."
            npm publish "$pkg" --access public --provenance
          done
          # Wait for npm registry propagation
          sleep 30
          # Publish main package last (depends on platform packages)
          npm publish packages/npm/@supernovae/nika --access public --provenance
```

### npm Provenance (new standard in 2025)

Always use `--provenance` when publishing from CI. This creates a Sigstore-signed attestation proving the package was built in CI:

```json
{
  "publishConfig": {
    "provenance": true
  }
}
```

Requires `id-token: write` permission in GitHub Actions. npm shows a green checkmark on packages with provenance.

---

## 3. VS Code Extension Auto-Publish

### Your current setup assessment

Your `release.yml` already handles VS Code publishing well. Key improvements to add:

### Open VSX Publishing (for VSCodium, Cursor, Windsurf, Theia)

In 2025-2026, Open VSX support is essentially **mandatory** for developer tools. Cursor (the most popular AI code editor) uses Open VSX. So does VSCodium, Gitpod, and Eclipse Theia.

```yaml
  vscode-publish:
    name: Publish VS Code Extension
    needs: release
    if: ${{ !inputs.dry_run }}
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: editors/vscode

    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ env.RELEASE_TAG }}

      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: editors/vscode/package-lock.json

      - run: npm ci

      - name: Sync version from tag
        run: npm version "${RELEASE_TAG#v}" --no-git-tag-version

      - run: npm run compile

      # Package VSIX once, publish everywhere
      - name: Package VSIX
        run: npx @vscode/vsce package --no-git-tag-version

      # Upload to GitHub Release
      - name: Upload VSIX to Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release upload "$RELEASE_TAG" \
            nika-lang-*.vsix \
            --repo "${{ github.repository }}" \
            --clobber

      # VS Code Marketplace
      - name: Publish to VS Code Marketplace
        env:
          VSCE_PAT: ${{ secrets.VSCE_PAT }}
        run: npx @vscode/vsce publish --no-git-tag-version -p "$VSCE_PAT"

      # Open VSX (for Cursor, VSCodium, Gitpod, Theia)
      - name: Publish to Open VSX
        env:
          OVSX_PAT: ${{ secrets.OVSX_PAT }}
        run: npx ovsx publish nika-lang-*.vsix -p "$OVSX_PAT"
        continue-on-error: true  # Don't fail release if Open VSX is down
```

### Secrets needed

```
VSCE_PAT   - Azure DevOps PAT with Marketplace publish scope
             https://code.visualstudio.com/api/working-with-extensions/publishing-extension
OVSX_PAT   - Open VSX access token
             https://open-vsx.org/user-settings/tokens
```

### Version sync strategy

Your current approach is correct: extract version from git tag, use `npm version` to sync. The VSIX contains the version from `package.json`, so syncing at build time ensures it matches the release.

---

## 4. GitHub Release Template Best Practices

### What makes a great release page

Based on analysis of biome, starship, mise, and other popular projects:

1. **Clear version header** with project identity
2. **Changelog section** extracted from CHANGELOG.md (your git-cliff setup handles this)
3. **Installation table** with all distribution methods
4. **Platform/architecture table** for binary downloads with checksum links
5. **SHA256 checksums** (you already generate these)
6. **Docker pull command** with exact version tag
7. **Verification instructions** (how to check checksums)
8. **Links to docs/migration guide** for breaking changes

### git-cliff vs conventional-changelog

| Feature | git-cliff | conventional-changelog |
|---------|-----------|----------------------|
| Language | Rust (fast) | JavaScript |
| Config format | TOML (cliff.toml) | JS/JSON |
| Customization | Tera templates (powerful) | Handlebars |
| GitHub integration | Links PRs, authors | Links PRs |
| release-plz integration | Native | Manual |
| Breaking change grouping | Yes | Yes |

**Verdict**: git-cliff is the correct choice for Rust projects. Your cliff.toml is well-configured.

### Improved release notes template

```yaml
      - name: Generate release notes
        env:
          VERSION: ${{ steps.version.outputs.version }}
        run: |
          cat > release-notes.md << 'EOF'
          > **Nika** -- Semantic YAML Workflow Engine for AI
          EOF

          echo "" >> release-notes.md

          # Extract changelog for this version
          if [ -f "CHANGELOG.md" ]; then
            awk -v ver="$VERSION" '
              /^## \[/ {
                if (match($0, ver)) { found=1; next }
                if (found) exit
              }
              found { print }
            ' CHANGELOG.md >> release-notes.md || true
          fi

          cat >> release-notes.md << INSTALL

          ---

          ## Installation

          | Method | Command |
          |--------|---------|
          | Shell (recommended) | \`curl -fsSL https://nika.supernovae.studio/install.sh \\| sh\` |
          | Homebrew | \`brew install supernovae-st/tap/nika\` |
          | npm | \`npx @supernovae/nika\` |
          | Docker | \`docker run --rm ghcr.io/supernovae-st/nika:${VERSION}\` |
          | Cargo | \`cargo install nika\` |

          ## Downloads

          | Platform | Architecture | Download | Checksum |
          |----------|-------------|----------|----------|
          | macOS | Apple Silicon (arm64) | nika-macos-arm64-${VERSION}.tar.gz | .sha256 |
          | macOS | Intel (x64) | nika-macos-x64-${VERSION}.tar.gz | .sha256 |
          | Linux | x64 (glibc) | nika-linux-x64-${VERSION}.tar.gz | .sha256 |
          | Linux | ARM64 (glibc) | nika-linux-arm64-${VERSION}.tar.gz | .sha256 |
          | Windows | x64 | nika-windows-x64-${VERSION}.zip | .sha256 |
          | Docker | amd64 + arm64 | ghcr.io/supernovae-st/nika:${VERSION} | attestation |

          ## Verification

          \`\`\`bash
          # Verify checksum after download
          shasum -a 256 -c nika-macos-arm64-${VERSION}.tar.gz.sha256

          # Verify build provenance (requires gh CLI)
          gh attestation verify nika-macos-arm64-${VERSION}.tar.gz --repo supernovae-st/nika
          \`\`\`

          ---
          INSTALL
```

### Build provenance attestations (new in 2025)

GitHub's `actions/attest-build-provenance` creates SLSA provenance. Biome uses this for every binary. Add to your build job:

```yaml
      - name: Attest build provenance
        uses: actions/attest-build-provenance@v4
        with:
          subject-path: dist/*.tar.gz
```

Users can verify with:
```bash
gh attestation verify nika-macos-arm64-0.46.1.tar.gz --repo supernovae-st/nika
```

---

## 5. install.sh Best Practices

### Analysis of top install scripts (2025-2026)

| Project | Shell | Platform detect | musl detect | Checksum | Compression |
|---------|-------|----------------|-------------|----------|-------------|
| **starship** | `sh` (POSIX) | `uname -s` + `uname -m` | `ldd --version` | Yes | tar.gz |
| **bun** | `bash` | `uname -ms` combined | No | No | zip |
| **mise** | `sh` (POSIX) | `uname -s` + `uname -m` | `ldd` + libc check | Yes | tar.zst preferred |
| **proto** | cargo-dist installer | auto | auto | Yes | tar.xz/gz |
| **rustup** | `sh` (POSIX) | `uname -s` + `uname -m` | Yes | GPG signature | tar.gz |
| **Nika** | `sh` (POSIX) | `uname -s` + `uname -m` | No | SHA256 | tar.gz |

### Best practices distilled

Your install.sh is already quite good. Key improvements based on the research:

#### 1. musl detection (from mise) -- Important for Alpine/NixOS users

```sh
detect_libc() {
  if [ "${OS_NAME}" != 'linux' ]; then
    LIBC_VARIANT=''
    return
  fi
  if command -v ldd > /dev/null 2>&1; then
    if ldd --version 2>&1 | grep -qi musl; then
      LIBC_VARIANT='-musl'
      return
    fi
  fi
  # Check if /lib contains musl
  if ls /lib/ld-musl-* > /dev/null 2>&1; then
    LIBC_VARIANT='-musl'
    return
  fi
  LIBC_VARIANT=''
}
```

#### 2. Prefer zstd compression (from mise) -- 30-40% smaller downloads

```sh
get_compression_ext() {
  if command -v zstd > /dev/null 2>&1; then
    if tar --version 2>&1 | grep -q 'bsdtar' || \
       tar --version 2>&1 | grep -qE '1\.(3[1-9]|[4-9][0-9])'; then
      echo "tar.zst"
      return
    fi
  fi
  echo "tar.gz"
}
```

#### 3. Verify shell is POSIX (from starship)

```sh
verify_shell_is_posix_or_exit() {
  if [ -n "${ZSH_VERSION+x}" ]; then
    error "Running with zsh may cause errors. Use: sh install.sh"
    exit 1
  elif [ -n "${BASH_VERSION+x}" ] && [ -z "${POSIXLY_CORRECT+x}" ]; then
    error "Running with non-POSIX bash may cause errors. Use: sh install.sh"
    exit 1
  fi
}
```

#### 4. Existing install detection -- Check if already installed

```sh
check_existing() {
  if command -v nika > /dev/null 2>&1; then
    EXISTING_VERSION="$(nika --version 2>/dev/null | awk '{print $NF}')" || true
    if [ -n "${EXISTING_VERSION}" ]; then
      info "Found existing nika v${EXISTING_VERSION}"
      if [ "v${EXISTING_VERSION}" = "${INSTALL_VERSION}" ]; then
        success "Already up to date (v${EXISTING_VERSION})"
        exit 0
      fi
    fi
  fi
}
```

#### 5. Vanity install URL

Use a redirect from your domain:
```
curl -fsSL https://nika.supernovae.studio/install.sh | sh
```

This is just a redirect to the raw GitHub URL but looks more professional and survives repo renames.

---

## 6. Docker Multi-Arch Best Practices

### Your current Dockerfile assessment

Your Dockerfile is **excellent**. The `FROM scratch` runtime with pre-built musl static binaries is the gold standard pattern. The two-stage approach (builder for tests, runtime from scratch) is correct.

### Key improvements

#### 1. Use `--platform=$BUILDPLATFORM` for the builder stage

This avoids slow QEMU emulation during the build:

```dockerfile
# Builder stage runs on host platform (fast)
FROM --platform=$BUILDPLATFORM rust:1.86-bookworm AS builder
# ... build using cross-compilation, not QEMU

# Runtime stage is multi-arch (just copies binary)
FROM scratch AS runtime
ARG TARGETARCH
COPY ${TARGETARCH}/nika /nika
```

#### 2. Supply chain security

You already use `actions/attest-build-provenance`. Also ensure:

```yaml
      - name: Build and push
        uses: docker/build-push-action@v7
        with:
          provenance: true  # SLSA provenance attestation
          sbom: true        # Software Bill of Materials
```

#### 3. Size tracking

Your musl static binary should be ~15-25MB. The scratch image adds 0 bytes. Total: ~15-25MB. This is optimal.

#### 4. Multi-arch manifest

Your workflow already handles this correctly with `platforms: linux/amd64,linux/arm64`. The `docker/metadata-action` generates proper semver tags. This is the correct pattern.

#### 5. docker-compose for users

```yaml
# docker-compose.yml (for users who want to try nika via Docker)
services:
  nika:
    image: ghcr.io/supernovae-st/nika:latest
    volumes:
      - .:/workspace
    working_dir: /workspace
    environment:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
```

---

## 7. Homebrew Tap Automation

### Your current setup assessment

You use `mislav/bump-homebrew-formula-action@v3` which is the standard approach. However, you only reference the x64 binary. Modern Homebrew formulas should support both architectures.

### Recommended: Multi-arch formula

Create a proper formula in your `homebrew-tap` repo:

```ruby
# Formula/nika.rb
class Nika < Formula
  desc "Semantic YAML workflow engine for AI tasks"
  homepage "https://supernovae.studio"
  license "AGPL-3.0-or-later"
  version "0.46.1"

  on_macos do
    on_arm do
      url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-macos-arm64-#{version}.tar.gz"
      sha256 "PLACEHOLDER_ARM64_SHA"
    end
    on_intel do
      url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-macos-x64-#{version}.tar.gz"
      sha256 "PLACEHOLDER_X64_SHA"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-linux-arm64-#{version}.tar.gz"
      sha256 "PLACEHOLDER_LINUX_ARM64_SHA"
    end
    on_intel do
      url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-linux-x64-#{version}.tar.gz"
      sha256 "PLACEHOLDER_LINUX_X64_SHA"
    end
  end

  def install
    bin.install "nika"
    # Generate shell completions if nika supports it
    generate_completions_from_executable(bin/"nika", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/nika --version")
  end
end
```

### Automated formula update in CI

Instead of `mislav/bump-homebrew-formula-action` which only handles one URL, use a custom step for multi-arch:

```yaml
  update-homebrew:
    name: Update Homebrew Formula
    needs: release
    if: ${{ !inputs.dry_run }}
    runs-on: ubuntu-latest
    steps:
      - name: Checkout homebrew-tap
        uses: actions/checkout@v6
        with:
          repository: supernovae-st/homebrew-tap
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
          path: homebrew-tap

      - name: Download checksums from release
        env:
          VERSION: ${{ steps.version.outputs.version }}
          TAG: ${{ env.RELEASE_TAG }}
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          BASE_URL="https://github.com/supernovae-st/nika/releases/download/${TAG}"
          for platform in macos-arm64 macos-x64 linux-arm64 linux-x64; do
            SHA=$(curl -sL "${BASE_URL}/nika-${platform}-${VERSION}.tar.gz.sha256" | awk '{print $1}')
            echo "${platform}=${SHA}" >> "$GITHUB_ENV"
          done

      - name: Update formula
        env:
          VERSION: ${{ steps.version.outputs.version }}
        run: |
          cd homebrew-tap
          # Generate the formula from a template
          cat > Formula/nika.rb << FORMULA
          class Nika < Formula
            desc "Semantic YAML workflow engine for AI tasks"
            homepage "https://supernovae.studio"
            license "AGPL-3.0-or-later"
            version "${VERSION}"

            on_macos do
              on_arm do
                url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-macos-arm64-#{version}.tar.gz"
                sha256 "${macos_arm64}"
              end
              on_intel do
                url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-macos-x64-#{version}.tar.gz"
                sha256 "${macos_x64}"
              end
            end

            on_linux do
              on_arm do
                url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-linux-arm64-#{version}.tar.gz"
                sha256 "${linux_arm64}"
              end
              on_intel do
                url "https://github.com/supernovae-st/nika/releases/download/v#{version}/nika-linux-x64-#{version}.tar.gz"
                sha256 "${linux_x64}"
              end
            end

            def install
              bin.install "nika"
            end

            test do
              assert_match version.to_s, shell_output("#{bin}/nika --version")
            end
          end
          FORMULA

      - name: Commit and push
        run: |
          cd homebrew-tap
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/nika.rb
          git commit -m "nika ${VERSION}"
          git push
```

### Secrets needed

```
HOMEBREW_TAP_TOKEN  - GitHub PAT with repo + workflow scopes
                      Must have write access to supernovae-st/homebrew-tap
```

---

## Action Items for Nika

### Priority 1 (Before first public release)

1. **Enable crates.io publishing**: Set `publish = true` for all workspace crates in `release-plz.toml`. Create a GitHub App for tokens.
2. **Migrate npm to optionalDependencies pattern**: Create platform-specific packages under `@supernovae/nika-{platform}`.
3. **Add Open VSX publishing**: Get OVSX_PAT token, add publish step to release.yml.
4. **Add build provenance attestations**: `actions/attest-build-provenance@v4` for all binaries.

### Priority 2 (Polish)

5. **Improve install.sh**: Add musl detection, existing version check, POSIX shell verification.
6. **Multi-arch Homebrew formula**: Update to support both arm64 and x64.
7. **npm provenance**: Add `--provenance` flag and `id-token: write` permission.
8. **Vanity install URL**: Set up `nika.supernovae.studio/install.sh` redirect.

### Priority 3 (Nice to have)

9. **zstd compression**: Add tar.zst artifacts alongside tar.gz.
10. **cargo-dist installer**: Consider generating a PowerShell installer for Windows.
11. **cosign signing**: Sign binaries and Docker images with Sigstore cosign.

---

## Complete Secrets Inventory

| Secret | Purpose | Where to get it |
|--------|---------|-----------------|
| `CARGO_REGISTRY_TOKEN` | crates.io publish | https://crates.io/settings/tokens |
| `APP_ID` + `APP_PRIVATE_KEY` | GitHub App (triggers workflows) | https://github.com/settings/apps |
| `NPM_TOKEN` | npm publish | https://www.npmjs.com/settings/tokens |
| `VSCE_PAT` | VS Code Marketplace | Azure DevOps PAT |
| `OVSX_PAT` | Open VSX Registry | https://open-vsx.org/user-settings/tokens |
| `HOMEBREW_TAP_TOKEN` | Homebrew tap writes | GitHub PAT (repo + workflow) |
| `CODECOV_TOKEN` | Coverage uploads | https://app.codecov.io |

---

## Sources

1. [Biome release_cli.yml](https://github.com/biomejs/biome/blob/main/.github/workflows/release_cli.yml) -- Full multi-platform build + npm publish pipeline
2. [Biome npm packages](https://github.com/biomejs/biome/tree/main/packages/%40biomejs) -- optionalDependencies pattern implementation
3. [Biome generate-packages.mjs](https://github.com/biomejs/biome/blob/main/packages/%40biomejs/biome/scripts/generate-packages.mjs) -- Script that creates platform packages
4. [release-plz own workflow](https://github.com/release-plz/release-plz/blob/main/.github/workflows/release-plz.yml) -- Dogfood setup with GitHub App tokens
5. [release-plz documentation](https://release-plz.dev/docs) -- Configuration, workspace handling, trusted publishing
6. [starship install.sh](https://github.com/starship/starship/blob/master/install/install.sh) -- POSIX install script with platform detection
7. [mise install script](https://mise.run) -- musl detection, zstd compression
8. [bun install script](https://bun.sh/install) -- Bash-based, fast platform detection
9. [mislav/bump-homebrew-formula-action](https://github.com/mislav/bump-homebrew-formula-action) -- Homebrew tap automation
10. [proto/moonrepo release.yml](https://github.com/moonrepo/proto/blob/master/.github/workflows/release.yml) -- cargo-dist based pipeline
11. [tauri publish-cli-rs.yml](https://github.com/tauri-apps/tauri/blob/dev/.github/workflows/publish-cli-rs.yml) -- Multi-platform Rust CLI + npm publish

## Methodology

- Tools used: Direct GitHub raw file fetching, analysis of production CI/CD workflows
- Projects analyzed: biome (2.4.9), starship, mise, proto/moonrepo, tauri, release-plz
- Files analyzed: ~25 workflow files, package.json manifests, install scripts, Dockerfiles
- Time period covered: 2024-2026

## Confidence Level

**High** -- All recommendations are based on production patterns from projects with millions of monthly downloads. The optionalDependencies npm pattern and release-plz for Rust workspaces are the clear consensus choices in 2025-2026.

## Further Research Suggestions

- **Winget/Scoop**: Windows package manager distribution
- **Nix flakes**: NixOS distribution for Nika
- **AUR**: Arch Linux User Repository packaging
- **cargo-binstall**: Binary installation fallback for `cargo install`
- **Apple notarization**: Code signing for macOS binaries (mise does this)
- **Sigstore cosign**: Keyless signing for all artifacts
