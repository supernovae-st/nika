# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows real semver toward a **1.0.0** public launch (amended
D-2026-06-20-N1) — quality over speed. v0.90.0 is the first public release.

Nika Diamond is a ground-up rewrite on an orphan branch (`main` ·
renamed 2026-05-06 from `nika-diamond`). Legacy v0.79.3 lives on
`brouillon` (renamed 2026-05-06 from `main`). Diamond starts at v0.80.0.

**Version history.** The pre-Diamond engine (the v0.1 → v0.79.3 legacy
era) is preserved in the private `nikab-legacy` reference repo — its tags
and releases were removed from this public repository in the 2026-06-21
cleanup. This public repo carries the Diamond arc only: `v0.80.0-alpha.*`
(the rewrite history) and `v0.90.0` (the first public release). This
changelog tracks the Diamond rebuild from **v0.80.0-alpha** onward.

---

## [0.92.0](https://github.com/supernovae-st/nika/compare/v0.91.0..v0.92.0) - 2026-07-03

### ✨ Features
- **media** — Permits-audit + on-error-recover motion scenes ([5ea0658bb](https://github.com/supernovae-st/nika/commit/5ea0658bbb18f9376324dd441b945ca02fdda21e))
- **nika-browser** — Guard navigate against private/loopback SSRF ([e81720031](https://github.com/supernovae-st/nika/commit/e81720031543554aa02050640e4ebbb9084fd517))
- **nika-cli** — Wire codex — mcp wiring for the codex cli ([b8246605c](https://github.com/supernovae-st/nika/commit/b8246605cfb31b92ab32972e616e7980eeb8d523))
- **nika-cli** — Codex plugin + repo-level agent skill ([4db87a89a](https://github.com/supernovae-st/nika/commit/4db87a89a7e4590b3509a8b70af4b3918634b0e3))
- **nika-cli** — Nika init scaffolds the AGENTS.md hard rules + learning surface ([45668958f](https://github.com/supernovae-st/nika/commit/45668958fea7e4313054c99ddf6728ddce2589a2))
- **nika-mcp** — Add the learning tools — schema · examples · template · canon ([0b38d3b28](https://github.com/supernovae-st/nika/commit/0b38d3b28514e3dc96190e59edefbdb5ff67e8c7))
- **nika-verb-exec** — Scrub the engine's provider API keys from exec children ([fa7a503dd](https://github.com/supernovae-st/nika/commit/fa7a503dd4b7ce4afacb2b1c78acd6e39ab17282))
- **workspace** — Claude code marketplace — one plugin dir, two ecosystems ([7e4a0b800](https://github.com/supernovae-st/nika/commit/7e4a0b80025f3a304c3bf8d84bb77dddf8351db2))

### 🐛 Bug Fixes
- **nika-cel** — Compare int64 exactly, not through f64 ([406af0072](https://github.com/supernovae-st/nika/commit/406af0072500c4f48c6c9e003274544be099e333))
- **nika-cli** — Cold-first-run truth — readme dual-line + tip guard ([484ca318b](https://github.com/supernovae-st/nika/commit/484ca318b91268233b90069e225deb15d162303b))
- **nika-pack** — Vendor the egress-sanctioned templates from nika-spec ([ede60533c](https://github.com/supernovae-st/nika/commit/ede60533c62a9e49c181b654963c811a0752856e))
- **nika-schema** — Flag the shell-string + permits.exec allowlist pairing at check ([d096698b8](https://github.com/supernovae-st/nika/commit/d096698b8d63bdfa339924d1abc470683cd8e44f))
- **nika-verb-exec** — Restrict exec env keys to POSIX names ([e33384543](https://github.com/supernovae-st/nika/commit/e33384543793c5b65ec653774d1253f6c08a55de))
- **security** — Quick-xml advisory pair · xcap bump + documented ignores ([1c7faed69](https://github.com/supernovae-st/nika/commit/1c7faed695c9204ace1c819e9066d284caa2d89f))
- **workspace** — The hero one-liner is verified-real now ([1855ada6b](https://github.com/supernovae-st/nika/commit/1855ada6bcb39c6e0486f779fae531234f9ab756))
- **workspace** — The plugin's .mcp.json was gitignored out of the publish ([4af2f2d68](https://github.com/supernovae-st/nika/commit/4af2f2d68a045f0a9823d004aa44bd5ac70fd71a))

### 🔨 Refactors
- **kernel** — Remove the deprecated cost_usd field ([2b3c33edc](https://github.com/supernovae-st/nika/commit/2b3c33edc2c40c36e0c94b9627d3ef0ae994ab9b))
- **nika-builtin** — Split date/uuid out of data.rs before the LOC cap ([a0753e716](https://github.com/supernovae-st/nika/commit/a0753e716116c0deb26d316a3d97cd6415545dec))
- **nika-schema** — Drop the now-dead shell-form program lookup ([d664944dd](https://github.com/supernovae-st/nika/commit/d664944ddd9f5f442a77818730f9d4cc18aeb95d))

### 📚 Documentation
- **adr** — Resolve ADR-093/094 id collisions — agent ADRs to 096/097 ([#144](https://github.com/supernovae-st/nika/issues/144)) ([fb01a4966](https://github.com/supernovae-st/nika/commit/fb01a4966736335a40f8e5cb5e53009afbb36995)) ([#144](https://github.com/supernovae-st/nika/pull/144))
- **changelog** — Append v0.91.0 ([#139](https://github.com/supernovae-st/nika/issues/139)) ([12a60b3b1](https://github.com/supernovae-st/nika/commit/12a60b3b162f04e631f5593863382da369bf5c08)) ([#139](https://github.com/supernovae-st/nika/pull/139))
- **clarity** — Kill stale maturity claims across visitor-reachable files ([eaa966a16](https://github.com/supernovae-st/nika/commit/eaa966a16054019453ce6f893f421bf09f2b9585))
- **crate-specs** — Drop hardcoded workspace version (anti-drift) ([4133ac0ce](https://github.com/supernovae-st/nika/commit/4133ac0cea6fc9359e4779fed98414f1af6a2882))
- **crate-specs** — Nika-cap gate-1 spec — the capability boundary as L0 ([20740b3fc](https://github.com/supernovae-st/nika/commit/20740b3fc41d723f8852574f38df5bb45e64266b))
- **dx** — De-drift the .claude commands + crate-admit skill ([6c203d6d0](https://github.com/supernovae-st/nika/commit/6c203d6d040ac9de5743a3fe2dd06c463766c7d2))
- **examples** — Real mermaid plan from nika graph + checks-clean line ([0f166dba9](https://github.com/supernovae-st/nika/commit/0f166dba924ca11d577f937676e0d24cd7e6dfb6))
- **media** — Motion media pipeline + 3 real-capture brand assets ([1a257ba98](https://github.com/supernovae-st/nika/commit/1a257ba98846097293f622e6a1f3107b5874ed39))
- **media** — V2 polish — narrative beats, data-flow pulses, 16fps gifs ([c319ae1d6](https://github.com/supernovae-st/nika/commit/c319ae1d6b07e6b909ddb5786e4c678b2f044954))
- **media** — Og + github social-preview cards from the motion pipeline ([c622dd39a](https://github.com/supernovae-st/nika/commit/c622dd39a132684fdc7f979928ac4828a764a7ba))
- **media** — Editor-diagnostics asset — the audit as you type ([3f1cb9e09](https://github.com/supernovae-st/nika/commit/3f1cb9e09d058bf454788c6435373e6fa2be3fe7))
- **media** — Workflow-gallery asset — start from a workflow ([ec089cbf8](https://github.com/supernovae-st/nika/commit/ec089cbf81b6e48b9a3e7028641dfd62171ba3a0))
- **media** — Social poster pair — the wedge + the audit ([6630f8a78](https://github.com/supernovae-st/nika/commit/6630f8a78dee134fe382efd641f78c4820bda1b7))
- **nika-builtin** — Correct the nika:jq internal-cost comment ([7f00faf3e](https://github.com/supernovae-st/nika/commit/7f00faf3e653e3ca55c3a2de395d14cd12ea6a41))
- **nika-pack** — Vendor the em-dash-swept quickstart from nika-spec ([ecc69c37a](https://github.com/supernovae-st/nika/commit/ecc69c37a316240048edff9bb27bc6f42f1a02f5))
- **readme** — Wedge-first rewrite — run-today proof + example gallery surfaced ([e4ee6f08a](https://github.com/supernovae-st/nika/commit/e4ee6f08a0c3f7ff5ff0cd1d9f2ea96c37b76962))
- **readme** — Real terminal gif — check audits, run executes (96KB) ([2ed5520ce](https://github.com/supernovae-st/nika/commit/2ed5520ce30034904f3803f958e6a70c4c756a72))
- **readme** — Embed the permits-audit + on-error-recover captures ([9e047ae85](https://github.com/supernovae-st/nika/commit/9e047ae85799268988511cca21e69a9859de3e65))
- **readme** — Plugin install block in work-with-your-agents ([e3b6f3155](https://github.com/supernovae-st/nika/commit/e3b6f31559a30fc1e55c5d7a29fd8c6565aa5acc))
- **readme** — Plugin marketplace → the lean nika-agents repo ([76a1cf8ef](https://github.com/supernovae-st/nika/commit/76a1cf8ef7375664c836241c8ff5c063cdd40160))
- **roadmap** — Fix self-contradicting release scars ([5e4c13990](https://github.com/supernovae-st/nika/commit/5e4c13990ad3c5a6e9c07cfd67b42c72f289e4ce))
- **roadmap** — The 0.92→0.95 ladder — three admissions to 42/42 ([71aa58db4](https://github.com/supernovae-st/nika/commit/71aa58db4b63ce8d4a79b80a1048141356e63f11))
- **workspace** — Prose em-dash sweep in the readme ([3f20694e8](https://github.com/supernovae-st/nika/commit/3f20694e87a2d0f6697c591106c010efd015e1d6))
- **workspace** — Join the readme to the new site pages ([587ad053e](https://github.com/supernovae-st/nika/commit/587ad053ec5ef1291a4b5d4584240df169a52c56))
- **workspace** — The hero run is real — local model register everywhere ([b5e7fecec](https://github.com/supernovae-st/nika/commit/b5e7fececa1f184e0f0494816e926a6fd7f17f1b))
- Refresh version pointers to v0.91.0 / main 0.92.0-dev ([95962d5cd](https://github.com/supernovae-st/nika/commit/95962d5cdca68b0a7992acc0f5bab6f4fce20af0))
- Fix stale version + builtin-count drift across engine docs ([4092a87de](https://github.com/supernovae-st/nika/commit/4092a87ded4e14aad92acac72b7fbec37f7b343f))

### 📦 Build
- **hygiene** — Stop nightly drift-issue spam + resync status block ([#143](https://github.com/supernovae-st/nika/issues/143)) ([c64082276](https://github.com/supernovae-st/nika/commit/c640822766a89dc8d79a5a5509fb2a552392ce52)) ([#143](https://github.com/supernovae-st/nika/pull/143))

### 🧹 Chore
- **catalog** — Refresh nika-types public-api baseline ([4a59e2551](https://github.com/supernovae-st/nika/commit/4a59e2551860b20d4a5fde14f6121c2424c0baef))
- **ci** — Lock 8 more public-api surfaces — vector 38 ratchet 27/38 → 35/38 ([1f398ef9f](https://github.com/supernovae-st/nika/commit/1f398ef9fef4ec27593c1c5baceca3e6d39d33c9))


## [0.91.0](https://github.com/supernovae-st/nika/compare/v0.90.0..v0.91.0) - 2026-06-25

### ✨ Features
- **cli** — Add explicit nika onboarding wiring ([fce281a1a](https://github.com/supernovae-st/nika/commit/fce281a1a031a711441804aaa0c381cf69e66dc6))
- **nika-cli** — Examples run --model override + offline mock hint ([ede69ccfd](https://github.com/supernovae-st/nika/commit/ede69ccfd2b025af4575293742772d5c09adf3af))
- **nika-screen** — Optional xcap backend (default-off) for headless builds ([9f2281f05](https://github.com/supernovae-st/nika/commit/9f2281f0588b124a59fa9f9a9cb4eb08c29900c4))

### 🐛 Bug Fixes
- **ci** — Clippy excludes macOS-only metal feature on the Linux runner ([93b7990a9](https://github.com/supernovae-st/nika/commit/93b7990a94d3ac71bb87c66b2b97f0c5d6d4f4dc))
- **doctor** — Recognize workspace cursor mcp config ([723fcd9e2](https://github.com/supernovae-st/nika/commit/723fcd9e2220f42c787dfe523966768c929c8f56))
- **nika-a11y** — Atspi 0.29 API — Role::Button + direct zbus dep ([53c19ef88](https://github.com/supernovae-st/nika/commit/53c19ef88552cb637fcbeb7fbbdee814efc28edf))
- **nika-a11y** — Make the atspi walk future Send ([ed055d94d](https://github.com/supernovae-st/nika/commit/ed055d94dd52bc5d56e5c3133c72202170fdc256))
- **nika-a11y** — Cfg-gate macOS-only AX helpers (Linux dead-code) ([bd9645483](https://github.com/supernovae-st/nika/commit/bd9645483fe186db1532a43c646bfa3a3977af99))
- **nika-a11y** — Backtick PascalCase in the atspi doc comment ([6e8960f51](https://github.com/supernovae-st/nika/commit/6e8960f51180ebcc20bf5cc486873a69ba14ec07))
- **nika-catalog** — Gate pricing/capabilities-conditional imports ([6648a389f](https://github.com/supernovae-st/nika/commit/6648a389f03b2f6dde75211ac5d885b18e01317e))
- **nika-types** — Gate serde so the no-default-features build compiles ([05e0de2d9](https://github.com/supernovae-st/nika/commit/05e0de2d92df29c211bcf3d1cd52029045c9604f))
- **release** — Separate dev version from brew assets ([16b9ac480](https://github.com/supernovae-st/nika/commit/16b9ac480d44f6ee110c4537eea87d07bf29a8c1))
- **typos** — Exclude generated/fuzz + allowlist domain vocab ([33411ce7c](https://github.com/supernovae-st/nika/commit/33411ce7c78fca318d6e63c5a03e0769ff062210))

### 📚 Documentation
- **changelog** — Append v0.90.0 — auto-generated ([#133](https://github.com/supernovae-st/nika/issues/133)) ([a424d559b](https://github.com/supernovae-st/nika/commit/a424d559b05cb078fa46d6d42a68dfa095db1164)) ([#133](https://github.com/supernovae-st/nika/pull/133))
- **coherence** — Fix stale facts post-ship + tag-cleanup ([de6fcbf49](https://github.com/supernovae-st/nika/commit/de6fcbf4942569e11db9a3258aae43ab36b48c36))
- **kernel** — Retire forever-v0.x doc refs (real-semver cascade) ([5b60fa0aa](https://github.com/supernovae-st/nika/commit/5b60fa0aa4023c78b93e3f8c019f2ede39ad8e44))
- **readme** — Real brew install + zero-setup quickstart; fix version residual ([b2ba50821](https://github.com/supernovae-st/nika/commit/b2ba5082140b444b40b4b8b060d7350591d1be71))
- **readme** — Document curl install + add editor-support section ([55551e4ce](https://github.com/supernovae-st/nika/commit/55551e4ce3c984adea60040d66a2b04fde38840e))
- **readme** — Correct the install-script PATH note ([895ce3a11](https://github.com/supernovae-st/nika/commit/895ce3a1182b3788b0e9ffcf5ecf0ed356b2f497))
- **release** — Clarify main versus tagged binaries ([47ff5f5d5](https://github.com/supernovae-st/nika/commit/47ff5f5d581bfe77af587df96da02bd7bb3aec9d))
- **roadmap** — Current-state reflects v0.90.0 SHIPPED (first public release) ([38218ab7c](https://github.com/supernovae-st/nika/commit/38218ab7c7dcf60f11886f05bc861941245bb68b))
- **status** — Sync dev version projections ([60c5a7828](https://github.com/supernovae-st/nika/commit/60c5a78285711a3642a1f8ec64ba56dddb107ede))

### 📦 Build
- **diamond** — Pipewire dep + ignore unmaintained paste advisory ([450477646](https://github.com/supernovae-st/nika/commit/4504776466af1e7095828629748f0c2b59bd042e))
- **diamond** — Egl/gl deps + miri toolchain pin + typos allowlist ([26d1cd098](https://github.com/supernovae-st/nika/commit/26d1cd098a56520e99aad0f91eab9cc02550bdb3))
- **diamond** — Exclude xcap from clippy features (system-lib, like metal) ([bf5d91ddc](https://github.com/supernovae-st/nika/commit/bf5d91ddc128e1024874c26dbb25ffed7fa5cd70))

### 🧹 Chore
- **hooks** — Accept nika-<crate> + diamond/coherence commit scopes ([744d1202d](https://github.com/supernovae-st/nika/commit/744d1202d2d2a1ca11200823aa80bca6cd8f9d58))
- **hooks** — Add typos to the commit-scope allowlist ([c0f7c070b](https://github.com/supernovae-st/nika/commit/c0f7c070b9e42587b12733ce1e99f95c6ee680f9))

### 🦋 New Contributors
- @github-actions[bot] made their first contribution in [#133](https://github.com/supernovae-st/nika/pull/133)


## [Unreleased]

### 🆕 Crates admitted
- **nika-a11y** — Admit to workspace — all 12 gates passed ([047e180d1](https://github.com/supernovae-st/nika/commit/047e180d196c984eab516fe17a9a6d5e3bbb00d6))
- **nika-blob** — Admit to workspace — all 12 gates passed ([e91adcef2](https://github.com/supernovae-st/nika/commit/e91adcef273eeb88f24ed8c91f8828581830cc4d))
- **nika-bm25** — Admit to workspace — all 12 gates passed ([36ca8c3ee](https://github.com/supernovae-st/nika/commit/36ca8c3eed8ab2154fd9a0db59bd1a6a6de39f26))
- **nika-browser** — Admit to workspace — all 12 gates passed ([e1bce0283](https://github.com/supernovae-st/nika/commit/e1bce0283165c33e4689f1eceab89c9ff8a9cf95))
- **nika-builtin** — Admit to workspace — 12 gates (Gate 5 via GATE5-EXEMPT budget) ([16142cf60](https://github.com/supernovae-st/nika/commit/16142cf605957539a6f4b3a03aec80b9ce5d029f))
- **nika-catalog-codegen** — Admit to workspace — 12 gates passed (10/12 + 2 deferred) ([23ab2fef8](https://github.com/supernovae-st/nika/commit/23ab2fef8e080e391374d359478adafb8a2543f6))
- **nika-cli** — Admit to workspace — all 12 gates passed ([a904c2db7](https://github.com/supernovae-st/nika/commit/a904c2db7196e77d1b21a72c6756aedc07ded9a1))
- **nika-clock** — Admit to workspace — all 12 gates passed ([74a8ff483](https://github.com/supernovae-st/nika/commit/74a8ff48373c80047dfd8c85142288e3d3a7a00f))
- **nika-event** — Admit to workspace — all 12 gates passed ([d009b1dd8](https://github.com/supernovae-st/nika/commit/d009b1dd8f46cd3cfaffaaff5dccef637b1dd913))
- **nika-exec-runner** — Admit to workspace — all 12 gates passed ([7ba7b51d8](https://github.com/supernovae-st/nika/commit/7ba7b51d8915638e3366717dbb010e7e7f332ab6))
- **nika-extract** — Admit to workspace — all 12 gates passed ([28ba11760](https://github.com/supernovae-st/nika/commit/28ba1176038e0d3200c47220bf6aa7328d7c20c5))
- **nika-fs** — Admit to workspace — all 12 gates passed ([47825df4a](https://github.com/supernovae-st/nika/commit/47825df4a460d01791bb7c5ad86eda1b7faab95b))
- **nika-http** — Admit to workspace — all 12 gates passed ([221c5d5a9](https://github.com/supernovae-st/nika/commit/221c5d5a98eed2e0720e7d55f91eb93c6c717858))
- **nika-input** — Admit to workspace — all 12 gates passed ([e4686eccb](https://github.com/supernovae-st/nika/commit/e4686eccbaf354e188eaecadf8a883b10b325ecf))
- **nika-kernel-ai** — Admit to workspace — kernel split step 3 (ai sibling) ([9eb6e225c](https://github.com/supernovae-st/nika/commit/9eb6e225c1c965f0fd60111428f8e585f43cf7b8))
- **nika-kernel-core** — Admit to workspace — kernel split step 2 (base sibling) ([7180576b0](https://github.com/supernovae-st/nika/commit/7180576b0377f79e0b3273cd5e6b82509390ad3b))
- **nika-kernel-plugin** — Admit to workspace — kernel split step 5 (plugin sibling) ([27bf36d78](https://github.com/supernovae-st/nika/commit/27bf36d788ab0b77fff52af579b565363a124ca9))
- **nika-kernel-runtime** — Admit to workspace — kernel split step 4 (runtime sibling) ([393ddef86](https://github.com/supernovae-st/nika/commit/393ddef86acbc64967b4acf6b72ef90ea01245ea))
- **nika-lsp** — Admit to workspace — all 12 gates passed ([85ba7f513](https://github.com/supernovae-st/nika/commit/85ba7f513fcebb3e6dbf6273dd91362ba874a552))
- **nika-mcp** — Admit to workspace — all 12 gates passed ([850f1219f](https://github.com/supernovae-st/nika/commit/850f1219ff6d42edda92b76af23c21c68689339b))
- **nika-ocr** — Admit to workspace — all 12 gates passed ([2541a9181](https://github.com/supernovae-st/nika/commit/2541a91818f4b2e0d3a6f7ebb84e24047cd6b7f2))
- **nika-pack** — Admit to workspace — all 12 gates passed ([5f37637c3](https://github.com/supernovae-st/nika/commit/5f37637c3cd8336a26d70d32071585d8ee7ee5dc)) ([#112](https://github.com/supernovae-st/nika/pull/112))
- **nika-providers** — Admit to workspace — all 12 gates passed ([9537dcf82](https://github.com/supernovae-st/nika/commit/9537dcf82c47ba1b5be3b40e9e80f2b8dfa798c1))
- **nika-runtime** — Admit to workspace — all 12 gates passed ([2e0386d3a](https://github.com/supernovae-st/nika/commit/2e0386d3a0603957d113b8522de9c4494d73b32a))
- **nika-schema** — Admit to workspace — all 12 gates passed ([99c4dfb00](https://github.com/supernovae-st/nika/commit/99c4dfb003b7b129021875f17c4ea9d0da56b7ae))
- **nika-screen** — Admit to workspace — all 12 gates passed ([181da3148](https://github.com/supernovae-st/nika/commit/181da3148868d5a41f22711489c1e3b8a1a18157))
- **nika-verb-agent** — Admit to workspace — all 12 gates passed ([0e2900d92](https://github.com/supernovae-st/nika/commit/0e2900d92e5ffb549f3ebe2e4b4e815f75a70089))
- **nika-verb-exec** — Admit to workspace — all 12 gates passed ([9b3284979](https://github.com/supernovae-st/nika/commit/9b3284979aaff149dd5ed1ef8e6c0fe82cba0e32))
- **nika-verb-infer** — Admit to workspace — all 12 gates passed ([c5a0b3e74](https://github.com/supernovae-st/nika/commit/c5a0b3e74d1be60a65e00eac9360848e7f347dd5))
- **nika-verb-invoke** — Admit to workspace — all 12 gates passed ([11c42947a](https://github.com/supernovae-st/nika/commit/11c42947a2a0cc9cc834a7db4e9cb6494962b16d))

### ✨ Features
- **adr** — Supersedes-DAG cycle detection + self-contained contract ([8c7559cd4](https://github.com/supernovae-st/nika/commit/8c7559cd49c602e2e0601b8674695bbd28922c5b))
- **arch** — Decide + gate the kernel I/O error convention (Pattern A universal) ([e72a3e12a](https://github.com/supernovae-st/nika/commit/e72a3e12ad9b682df7e22044baa729cf92c1da28))
- **ci** — Add check-crate-gates.sh emitting olympus CrateGates JSON ([01f18a381](https://github.com/supernovae-st/nika/commit/01f18a381bb50573ac4bde632f3b1952f8a84404))
- **ci** — Real executable Gate 5 — cargo-mutants kill-floor enforcement ([97c8ad476](https://github.com/supernovae-st/nika/commit/97c8ad47627cacf243bc0e810505ddd979fdc148))
- **ci** — Mutation cross-platform calibration + public-API coverage ratchet ([6dce16899](https://github.com/supernovae-st/nika/commit/6dce1689980b742eba9bce5ed649d64056983b9d))
- **ci** — Lift public-API coverage 5→15 + project the floor into CI ([9688a2916](https://github.com/supernovae-st/nika/commit/9688a29162400211e1f28a9da0eac6c6b3eec94e))
- **ci** — Floor public-API for nika-fs/http/blob (post-merge coherence) ([5097478c2](https://github.com/supernovae-st/nika/commit/5097478c2cd1ce19ef4a6afd9a0cc316485fb42d))
- **dx** — Wire HQ dashboard hooks + /dashboard command ([3a6c11e32](https://github.com/supernovae-st/nika/commit/3a6c11e32500955ea333f3694fa15527bb577157))
- **dx** — Live roadmap projection + fix layer-count digit regex ([c00e1d16b](https://github.com/supernovae-st/nika/commit/c00e1d16bcb10b2d24c259fb5936c96aabd8f706))
- **error** — Wire NIKA-601..604 memory subsystem codes · diamond w2.1 ([8a02d152f](https://github.com/supernovae-st/nika/commit/8a02d152f429f7c21cdbf35e4804c8629a9bb7af))
- **fuzz** — Cargo-fuzz harness · 2 targets · corpus · nightly ci ([1b91aebb8](https://github.com/supernovae-st/nika/commit/1b91aebb8e06e9f3bf1677f6cf7fe5821ed24ff1))
- **hooks** — Post-commit auto-fires olympus xtask in background (wave 3a) ([220e8d9cb](https://github.com/supernovae-st/nika/commit/220e8d9cb0488e93a5f4e737c1ca4a3cfecd9c5e))
- **hygiene** — Autonomous ecosystem hygiene stack — WAVE 1+2 ([a8c01194d](https://github.com/supernovae-st/nika/commit/a8c01194da213a4c3e4b4cb36cef1327f5ab4d3c))
- **hygiene** — Vector 23 — status-claims-sync (Phase B.2) ([17602d073](https://github.com/supernovae-st/nika/commit/17602d0733876e546cde1f6dd55fbb18e6209341))
- **hygiene** — Vector 12 three-tier file-LOC + clippy too_many_lines (B.3, ADR-023) ([2225ac52f](https://github.com/supernovae-st/nika/commit/2225ac52fa080c2e88c9fb53d7d971f3a2c2444b))
- **hygiene** — Vector 25 — L0 sibling-dep fanout cap (B.4, ADR-027) ([8d35a946d](https://github.com/supernovae-st/nika/commit/8d35a946df4f6dd2bc0acabeb310b1761b0a77a0))
- **hygiene** — +vector 30 cancel-safety docs on kernel async fn (batch i.b) ([65700834a](https://github.com/supernovae-st/nika/commit/65700834a076601a424dcec3b8de5df5b6c16a21))
- **hygiene** — +vector 33 layer-deps bans (batch i.b) ([4bb4082d8](https://github.com/supernovae-st/nika/commit/4bb4082d867b81c1867931064970e576afcd48bb))
- **hygiene** — 3 new gates — ADR-081 guard enforcement + supply-chain policy ([b9e0bd75d](https://github.com/supernovae-st/nika/commit/b9e0bd75db4f825ddc3982debfa57cc0295eeaa7))
- **hygiene** — Vector 37 — error one-voice doctrine enforcement ([9c05e88d6](https://github.com/supernovae-st/nika/commit/9c05e88d6f8b4e34846be9d796fa170bbdfb2fcb))
- **hygiene** — Crate-spec LOC anchors deterministic — projector + freshness gate ([dfb3f6266](https://github.com/supernovae-st/nika/commit/dfb3f62667c97831e273f6a29fe35bb71c1b7fd7))
- **kernel** — Type the Fs traits with FsError, not std::io::Result ([5c19de82f](https://github.com/supernovae-st/nika/commit/5c19de82ff4d62f75927bbf0ef5b113d194abf8f))
- **kernel** — Type input + browser traits — Pattern A 100% uniform ([88371e56d](https://github.com/supernovae-st/nika/commit/88371e56d5cf300049b1d39e424f724b7807a3f5))
- **kernel** — Command-sandbox seam — OS confinement for the exec child ([a9abd4c3e](https://github.com/supernovae-st/nika/commit/a9abd4c3e83434014d6b8e3cfaf54e705a4e9b52))
- **kernel-a11y** — M1.3 add l0.5 io::a11y sealed traits + dtos ([e969e351f](https://github.com/supernovae-st/nika/commit/e969e351fc40fba99031e4477094779a88f82af7))
- **kernel-browser** — M1.5 add l0.5 io::browser sealed traits + dtos ([92aaf9b3c](https://github.com/supernovae-st/nika/commit/92aaf9b3cad5b2fdfe84ff4a6996a07c2d289d32))
- **kernel-input** — M1.4 add l0.5 io::input type-state trait + dtos ([0b0406167](https://github.com/supernovae-st/nika/commit/0b0406167749737e6fdd2ce13754cebdb51c11f5))
- **kernel-mock** — Enqueue_ok_with_headers — header-carrying canned responses ([cd9c98f77](https://github.com/supernovae-st/nika/commit/cd9c98f771b1ce1f28b767675a80009e1e8995d2))
- **kernel-ocr** — M1.2 add l0.5 io::ocr sealed traits + dtos ([e0bfa5b26](https://github.com/supernovae-st/nika/commit/e0bfa5b26583cbbab59ebf0fde6d9e00f105c9fb))
- **kernel-screen** — M1.1 add l0.5 io::screen sealed traits + dtos ([344b853ba](https://github.com/supernovae-st/nika/commit/344b853ba08f68aac8cc08da0b78f7974f8f9036))
- **kernel-screen** — M2.1.b1 capture_stream additive trait method ([da0a83358](https://github.com/supernovae-st/nika/commit/da0a83358d99f66ffa4aa96a3feeb357b2d1e66e))
- **kernel-vision** — M1.6 add l0.5 ai::vision sealed traits + dtos · m1 sealed ([1d3ff38e6](https://github.com/supernovae-st/nika/commit/1d3ff38e69e83f21ae5e826d1dd73247c53d0369))
- **mintlify** — Rebuild introduction with live snapshot + journey table ([58e0ab47d](https://github.com/supernovae-st/nika/commit/58e0ab47d0cb95c60606312aa0b239d7c3e7f564))
- **mintlify** — Architecture tab — layers + FCI + L0 decisions + admission ([74f18e211](https://github.com/supernovae-st/nika/commit/74f18e2115a33b4edd49af41072b4b4bdf6eab8e))
- **mintlify** — Reference — providers catalog (32 providers, 7 dialects) ([245243a38](https://github.com/supernovae-st/nika/commit/245243a38851ddb6395b1b107972050e36bac595))
- **mintlify** — Reference — capability rules (49 rules, 4 match kinds) ([2594b3b8e](https://github.com/supernovae-st/nika/commit/2594b3b8e7c160f5456af769bd6ba55d543ed495))
- **mintlify** — Concepts — architecture + providers rebuilt from current state ([ad55454e5](https://github.com/supernovae-st/nika/commit/ad55454e5bd4e495882938d0e311bb36cd4b439e))
- **mintlify** — Getting-started — honest v0.80 pre-release framing ([5e18782db](https://github.com/supernovae-st/nika/commit/5e18782db288657929c55ceab5383adace6cae99))
- **mintlify** — Architecture — ADR index (35 records, 11 thematic groups) ([1a1f2fc03](https://github.com/supernovae-st/nika/commit/1a1f2fc03ee106d2c2124542fabd941e588d2b4e))
- **mintlify** — Changelog tab — releases + roadmap + forever-v0.x ([edc91bbf9](https://github.com/supernovae-st/nika/commit/edc91bbf92c8e9d98e06da344d088ab0d6c7f641))
- **nika-a11y** — M2.3.b1 spec + b2 skeleton + guard 3 redaction ([54716278b](https://github.com/supernovae-st/nika/commit/54716278bead055726a74b364cddbedb6b09c5f9))
- **nika-a11y** — M2.3.b3 wire macos axuielement walk + ref cache ([a3ec54ee8](https://github.com/supernovae-st/nika/commit/a3ec54ee8b6538586511bf12c9394647e3cd8eb6))
- **nika-a11y** — Cross-platform linux atspi backend + guard 3 fix ([6cbd8108a](https://github.com/supernovae-st/nika/commit/6cbd8108af29993bfbe241cc1e61c5001e20581d))
- **nika-a11y** — Error one-voice — A11yError speaks NikaErrorCode ([662daf30c](https://github.com/supernovae-st/nika/commit/662daf30cff6af2207788fd7b2cb72fb5827b4be))
- **nika-bm25** — W3 admission prep · gate 1 spec + gate 3 scaffold ([3444e4131](https://github.com/supernovae-st/nika/commit/3444e41310ba8601d33b23bd8ec262092254e430))
- **nika-bm25** — Gate 3 green · pure-algo bm25 kernel shipped ([92e5d39fb](https://github.com/supernovae-st/nika/commit/92e5d39fbfb4fa848c785beb1673b14b78c9b698))
- **nika-bm25** — Gate 6 proptest + gate 7 criterion bench ([2da7c1e24](https://github.com/supernovae-st/nika/commit/2da7c1e2494fbf3751b715a42ed46190babb55a8))
- **nika-bm25** — EagerIndex — the BM25S eager sparse scoring architecture ([1fe44af34](https://github.com/supernovae-st/nika/commit/1fe44af345cae9166bd9d4b7f4c1c7abc3c2d844))
- **nika-bm25** — Activate BM25+ — the reserved delta wired through both paths ([27fa7ed1f](https://github.com/supernovae-st/nika/commit/27fa7ed1f733ce7a3814b0897e3ed068165213c1))
- **nika-bm25** — Canonical bm25+ preset — the lv-zhai delta default ([2402ed4f5](https://github.com/supernovae-st/nika/commit/2402ed4f50e1b9de1f5ad2881377935eaa2702f2))
- **nika-bm25** — MaxScore dynamic pruning + the research-conformance suite ([b7e5634a0](https://github.com/supernovae-st/nika/commit/b7e5634a0131dc778f34a96ea0dd5e1507f4e400))
- **nika-bm25,adr-039** — Sota 2026 q2 expert convergence · 10 locks ([82741170f](https://github.com/supernovae-st/nika/commit/82741170fbe893c58989723154402b863367c071))
- **nika-browser** — Scaffold security core — guard 5 pure verify ([eb6f07e0d](https://github.com/supernovae-st/nika/commit/eb6f07e0d25a82ee392f421487a8b956be81b592))
- **nika-browser** — Wire chromiumoxide backend — B.3, smoke-verified ([3e348cc43](https://github.com/supernovae-st/nika/commit/3e348cc431f8661442eda6cb13596cf8cdd60bcc))
- **nika-browser** — Guard 5 occlusion hit-test — SOTA actionability ([24e401e93](https://github.com/supernovae-st/nika/commit/24e401e930e8964a48e87703fe89bdc8c47cc70e))
- **nika-builtin** — Seed the 22-builtin dispatcher — the real tool layer (s16 · WIP) ([4ab88adeb](https://github.com/supernovae-st/nika/commit/4ab88adeb221dd55b0a1a9490ea0c38051d91604))
- **nika-builtin** — Fold the 3-lens review — transient · wait until · date six · jq bounds ([1d3025a56](https://github.com/supernovae-st/nika/commit/1d3025a5671b271c713a272609877e9e0c3d3c4f))
- **nika-builtin** — Wire the nika:fetch extract modes (step 13) ([3923767a9](https://github.com/supernovae-st/nika/commit/3923767a91dda46c50da4b8758c01e5a5a3d717f))
- **nika-builtin** — Fetch truth pass — runtime pairing mirrors · Cow decode · feed bytes · battery ([bbd8a3c19](https://github.com/supernovae-st/nika/commit/bbd8a3c195fb52ad4458846ca0a0842944197223))
- **nika-builtin** — Charset-aware decode for fetch extract modes ([c1487cc8f](https://github.com/supernovae-st/nika/commit/c1487cc8f6799a1e09ea397b998bae7969e86708))
- **nika-builtin** — Conformance pair + tz honesty — binary write · status details ([eda483459](https://github.com/supernovae-st/nika/commit/eda483459228d94696b73f28519bff8728c2626d))
- **nika-builtin** — Three Rams quick-wins — notify data · validate structured · log level clamp ([2f4b47470](https://github.com/supernovae-st/nika/commit/2f4b474702ee04c1b306fafb1be37bb5697d2331))
- **nika-builtin** — Nika:write serializes structured content to JSON ([53a721fae](https://github.com/supernovae-st/nika/commit/53a721fae5c10173742e5658d0aa82c2e0bce76e))
- **nika-catalog** — Tag enum + ParseTagError + FromStr ([cc3a1482f](https://github.com/supernovae-st/nika/commit/cc3a1482fd999c7b48796d80f3fd22bd456cf277))
- **nika-catalog** — Add tags + extra_tags fields to Provider, McpServer, Embedding ([bc21e5c23](https://github.com/supernovae-st/nika/commit/bc21e5c23a116bbf09df0c800807446869e6620b))
- **nika-catalog** — Populate tags across 3 catalogs + sort/dedup assertions ([a848b1916](https://github.com/supernovae-st/nika/commit/a848b1916bf98ff9588e20d627a7adc639e109c8))
- **nika-catalog** — Cargo features for subset compilation ([ceccfc39b](https://github.com/supernovae-st/nika/commit/ceccfc39b51fdbad965f2edf5fe3b2d5fa45931f))
- **nika-catalog** — MCP safety-tag XOR enforcement + runtime tag invariants ([83a9afaf4](https://github.com/supernovae-st/nika/commit/83a9afaf4b3d9e15a6c51b321550a5d9b44c69c2))
- **nika-catalog** — Session 2b foundation — modality + tokenizer + param_flag ([4f43db883](https://github.com/supernovae-st/nika/commit/4f43db88373a354c4ef633077e781d444e64c93a))
- **nika-catalog** — Session 2b — grow ModelCapabilities + CapPatch + codegen (no new rules) ([b2b8ce190](https://github.com/supernovae-st/nika/commit/b2b8ce1905387d42ef10fc09e5da401a1b30af62))
- **nika-catalog** — Session 2b rules — 28 capability rules + per-rule provenance ([1e0fd93bf](https://github.com/supernovae-st/nika/commit/1e0fd93bfb94df71460770e60c41f97a95e81fa7))
- **nika-catalog** — Session 3 — add 4 providers + 14 capability rules ([4d085afb0](https://github.com/supernovae-st/nika/commit/4d085afb05e2e2556568b6018db09b4ad3338502))
- **nika-catalog** — Add TokenizerFamily::Qwen variant ([4dbbe5db9](https://github.com/supernovae-st/nika/commit/4dbbe5db953890f7a1d744f0fa7653c95157c87f))
- **nika-catalog** — Pricing — add cached_input / image / reasoning axes ([1b3ea2e26](https://github.com/supernovae-st/nika/commit/1b3ea2e26f75f09aa3e160204de573884812a927))
- **nika-catalog** — Add NIKA-230..235 catalog error codes ([41198ec08](https://github.com/supernovae-st/nika/commit/41198ec08434024506ddb5cd9667824d70790d41))
- **nika-catalog** — Add context_window_tokens + max_output_tokens fields ([0fcabf6ef](https://github.com/supernovae-st/nika/commit/0fcabf6ef4c028b6a31a71817381e8b5478ffd59))
- **nika-catalog** — Add JsonMode enum, delete StructuredOutputNative ([6b74109aa](https://github.com/supernovae-st/nika/commit/6b74109aaf6dd429d22f9b07c1deb2e3dfed0a06))
- **nika-catalog** — Add ModelCapabilitiesView trait (Cortex v0.95) ([a142575f7](https://github.com/supernovae-st/nika/commit/a142575f70428aa7a196a5572b16fa802c3ed0a1))
- **nika-catalog** — Add Matcher::ContainsAny with word-boundary anchoring ([f8bcca19c](https://github.com/supernovae-st/nika/commit/f8bcca19c8aeea83a769ed5a99dea67622758dfa))
- **nika-catalog** — Promote CapRule/CapPatch/Matcher to pub #[non_exhaustive] ([69f10f5a9](https://github.com/supernovae-st/nika/commit/69f10f5a9062ff4afb23ff8485730fbdad1030b5))
- **nika-catalog** — Add Region enum and CapRule region scope dimension ([bb0a8c6e9](https://github.com/supernovae-st/nika/commit/bb0a8c6e90f7944366fcb63c5d90f0a6abce8755))
- **nika-catalog** — Toml-driven pricing, split cache_write/cache_read ([9453971db](https://github.com/supernovae-st/nika/commit/9453971db2fe62d7fcc96883e03345e4fe46d023))
- **nika-catalog** — Add CatalogDataSource trait + OverlayOrigin enum ([e88c3a78c](https://github.com/supernovae-st/nika/commit/e88c3a78ce36aa11e597ac24aa6c2140fabba058))
- **nika-catalog** — Add criterion benchmarks for catalog hot paths ([5998c39fc](https://github.com/supernovae-st/nika/commit/5998c39fc62ada5a463a5003933f372d8d7b080a))
- **nika-catalog** — Add 6 new ParamFlags (OpenRouter vocab align) ([e9e77a258](https://github.com/supernovae-st/nika/commit/e9e77a258eea3176344623790a4563c62cde4ee7))
- **nika-catalog** — Add 3 new Modalities (Embedding, Speech, ImageGen) ([892d5aff5](https://github.com/supernovae-st/nika/commit/892d5aff540c8f09bbf43b2ad44bee2c25004491))
- **nika-catalog** — Add 4 new TokenizerFamilies (LlamaV4/Granite/Glm/Grok) ([5dc3f2735](https://github.com/supernovae-st/nika/commit/5dc3f27358f612451f2bd31ea65d8ffd4f57b481))
- **nika-catalog** — Add 7 new providers + capability rules ([0415880c7](https://github.com/supernovae-st/nika/commit/0415880c79c7d5ede9dac7963e891025e9002aff))
- **nika-cel** — Admit the cel-subset/0.1 expression engine (L0) ([af4c2f8b8](https://github.com/supernovae-st/nika/commit/af4c2f8b885318f12be5886f0e006b73d403e369))
- **nika-cli** — Seed the L4 operator surface — display fold + trace reader + e2e pipeline rehearsal ([9fe99a5f8](https://github.com/supernovae-st/nika/commit/9fe99a5f80543b336da5bc4afdc3dceb216d9573))
- **nika-cli** — The static verb suite — audit a workflow before a single token ([b64c1074f](https://github.com/supernovae-st/nika/commit/b64c1074fd7623d1a4100a2c8ff617521337f245))
- **nika-cli** — Explain teaches spec codes + PLAN says the true width ([7b837d3a1](https://github.com/supernovae-st/nika/commit/7b837d3a14acbbb43cdba5781d754d481ab03733))
- **nika-cli** — The §3.1 state machine completes — retrying + cancelled ([d718f426e](https://github.com/supernovae-st/nika/commit/d718f426e8de83a14ef51d6c0679585fc2094645))
- **nika-cli** — Inspect carries the engineering read ([f523a16ea](https://github.com/supernovae-st/nika/commit/f523a16ea73164cd6645c0ad68703f4aa349b9a8))
- **nika-cli** — New routes free-form intent to the best template ([dc17c22f8](https://github.com/supernovae-st/nika/commit/dc17c22f827e53d9ee3b97f1757860a1142517f0))
- **nika-cli** — The nika run composer foundation — production seams + the two bridges ([68ed4a44c](https://github.com/supernovae-st/nika/commit/68ed4a44c5cdcb42a9776dfa3d7ebc80d6d47ff5))
- **nika-cli** — Nika run — the verb executes a checked workflow for real ([10f4ccdd6](https://github.com/supernovae-st/nika/commit/10f4ccdd64079a208aaab4d0aacb2debb0a5aec3))
- **nika-cli** — Examples run flips from refusal to execution ([6c1c93f16](https://github.com/supernovae-st/nika/commit/6c1c93f16005d002f68ee962ea07bd6e7d8255c2))
- **nika-cli** — Wire the nika lsp subcommand ([d290d809e](https://github.com/supernovae-st/nika/commit/d290d809ebd2c7d3c78ad594f7a663300663b74c))
- **nika-cli** — Explain teaches per-builtin NIKA-BUILTIN-<NAME> codes ([8d7a81754](https://github.com/supernovae-st/nika/commit/8d7a81754f95280313e3b1b1b63301176834e856))
- **nika-cli** — Nika run --output json emits the outputs: contract on stdout ([6b8dab5ce](https://github.com/supernovae-st/nika/commit/6b8dab5ce9d71b35659831a18735910e36e2b751))
- **nika-cli** — Nika doctor — environment diagnosis (spec §8 floor) ([c4b6472f0](https://github.com/supernovae-st/nika/commit/c4b6472f0ed91d1861487c63838bb99101db5f03))
- **nika-cli** — Nika init — scaffold a repo (spec §2 floor) ([65d7e24fc](https://github.com/supernovae-st/nika/commit/65d7e24fc1aff2658fd6df5466e322fbe78bc9d6))
- **nika-cli** — Check warns about required inputs before run ([2d7753e43](https://github.com/supernovae-st/nika/commit/2d7753e436f602db66d372b802f1a888fc8525e6))
- **nika-cli** — Explain teaches the NIKA-PROVIDER namespace ([88b3e0e3e](https://github.com/supernovae-st/nika/commit/88b3e0e3ef4da15034c1f3dd7c44c86e7a195e01))
- **nika-cli** — Add --no-progress/--quiet/--dry-run run flags ([eb8c8d7f7](https://github.com/supernovae-st/nika/commit/eb8c8d7f790c0665544ac251b28a0fa44e7678c2))
- **nika-compose** — The agent's self-check is nika:compose, the 23rd builtin ([44b701ac8](https://github.com/supernovae-st/nika/commit/44b701ac8fb9435811cbbf5c66e8056454a7d1e6))
- **nika-error** — Add 23 L0 foundational types + evolve kernel DTOs (ADR-033) ([d7b55b1e5](https://github.com/supernovae-st/nika/commit/d7b55b1e5f7dd9c0ce01bce563374c1a22b97404))
- **nika-error** — Cost stdlib arithmetic + checked_add/sub + remove TrustLevel::Default ([83294e026](https://github.com/supernovae-st/nika/commit/83294e026b3c06d01c20054ad933b7cc0990649b))
- **nika-error** — Register M2 computer-use code ranges — NIKA-1000..1206 ([951b3af39](https://github.com/supernovae-st/nika/commit/951b3af396e15686a0f1cee3bc26e3c972351da7))
- **nika-error** — Register verb codes NIKA-430..433 in the registry ([7a2fa2317](https://github.com/supernovae-st/nika/commit/7a2fa2317e671acd85be85b538b109453d62ead0))
- **nika-error** — Register NIKA-467 agent-stalled ([fcd5b1002](https://github.com/supernovae-st/nika/commit/fcd5b10029f5796d9f39d2ad9cf9e24dbd022edc))
- **nika-event** — Close the vocabulary over the display contract — 6 kinds + EventClass ([3820b9039](https://github.com/supernovae-st/nika/commit/3820b9039ff2f82531c2c6e8232e0b535ca2bee8))
- **nika-event** — The agent-loop telemetry vocabulary — 5 kinds + EventClass::Agent ([ea4ee3188](https://github.com/supernovae-st/nika/commit/ea4ee3188e09a3bbbb888f4141a0baec7911837b))
- **nika-exec-runner** — Argv program-floor, shell-line tripwire ([2efc48492](https://github.com/supernovae-st/nika/commit/2efc48492f14c2422aead9595f285663f759a1c0))
- **nika-exec-runner** — Strip dangerous-env injection vectors ([d5e63886a](https://github.com/supernovae-st/nika/commit/d5e63886af77fe894c75adc718c13aeb67f9e0d5))
- **nika-exec-runner** — Process-group kill — reap grandchildren ([6c5b10743](https://github.com/supernovae-st/nika/commit/6c5b10743e3308f0265b3e89ce05bf6975e4ddc7))
- **nika-exec-runner** — Wire the OS sandbox into the spawn path ([05a624f7d](https://github.com/supernovae-st/nika/commit/05a624f7de977c3b13ded9f044b36f089b6f3ffc))
- **nika-extract** — Seed the fetch extraction pipeline — 8 modes, pure (s17) ([241b2ab32](https://github.com/supernovae-st/nika/commit/241b2ab32c42bd9aee489a13b87de317700ce924))
- **nika-extract** — Science-grounded extraction wave — boilerpipe cascade + sitemap truth + RFC 8288 ([6c838f514](https://github.com/supernovae-st/nika/commit/6c838f5149f9fe892bb62bbb9425fd0b560e66e9))
- **nika-extract** — Round-2 hardening — DoS depth guard + adversarial battery + JSON-LD ([e24c875ae](https://github.com/supernovae-st/nika/commit/e24c875ae925ad0a2d569361d2bf612716e3b4d7))
- **nika-extract** — Feed mode surfaces full content + author ([35277fdd9](https://github.com/supernovae-st/nika/commit/35277fdd9e2df981c1c9d8a50f5b8a11ec9e5f99))
- **nika-extract** — Metadata mode extracts schema.org microdata ([0297334d6](https://github.com/supernovae-st/nika/commit/0297334d6e2a99f9330c3c3217adaf136ea724ad))
- **nika-extract** — Article mode → Trafilatura-grade 3-stage cascade ([2f1e1d6d5](https://github.com/supernovae-st/nika/commit/2f1e1d6d5842119c4b6d4e9a62b2dc8ec5003f48))
- **nika-extract** — Resolve lazy-loaded images to the real URL ([d0eaf328b](https://github.com/supernovae-st/nika/commit/d0eaf328b80460a40c3b979368b1327febafb45e))
- **nika-extract** — Honor <base href> in links + metadata resolution ([2d952eda2](https://github.com/supernovae-st/nika/commit/2d952eda27fb320e25cb45cf5a45f204f87bc8b3))
- **nika-extract** — Metadata title/description fall back to og/twitter ([b161a941d](https://github.com/supernovae-st/nika/commit/b161a941d9e582346fd95841e14f8b063607b182))
- **nika-extract** — Absolutize og:image/og:url/twitter:image URLs ([9dab7d9d8](https://github.com/supernovae-st/nika/commit/9dab7d9d846fd858a3bb370927f3bc109a505fbe))
- **nika-extract** — Feed items surface attached media (enclosure + MediaRSS) ([ddaee5318](https://github.com/supernovae-st/nika/commit/ddaee5318a26614c4b34acbf09f9a7867832c973))
- **nika-http** — Resolver-enforced SSRF closes the TOCTOU window ([7fdf3ac9e](https://github.com/supernovae-st/nika/commit/7fdf3ac9eb3bff777beb1bef83b1c0f913af0244))
- **nika-http** — Compression + h2 + streaming-true timeouts — the transport wave ([00c262111](https://github.com/supernovae-st/nika/commit/00c2621112b75960a739d4b399fd89da285266a3))
- **nika-infer-local** — Native SOTA decode-time algorithms — min-p, repeat-penalty, token-mask ([27f070f22](https://github.com/supernovae-st/nika/commit/27f070f221467ce24cb4646d5ac2c98cf65f4c46))
- **nika-infer-local** — The candle backend — sovereign inference runs (ADR-091) ([320b94d39](https://github.com/supernovae-st/nika/commit/320b94d39e192851b53dd8fa1d869eef4289ce95))
- **nika-infer-local** — Top-nσ sampling — native, temperature-invariant (arXiv:2411.07641) ([c4a2f0e2d](https://github.com/supernovae-st/nika/commit/c4a2f0e2d7a404cb179ca39f48aa0231dbc157ba))
- **nika-infer-local** — V1 sidecar http server — tiny_http per adr-093 ([34bb2441e](https://github.com/supernovae-st/nika/commit/34bb2441e8dc22b23dc511f3794d6e0236b9fc56))
- **nika-infer-local** — 12-gate admission as the local inference sidecar ([c5052cd4a](https://github.com/supernovae-st/nika/commit/c5052cd4a8f59d9ea5bcf1c49edd7ec470377cea))
- **nika-input** — Scaffold security core — type-state + guards 1+2 ([95fcf4002](https://github.com/supernovae-st/nika/commit/95fcf40021bc4460e75b076b6d8f13ded3aeffe2))
- **nika-input** — Wire enigo backend — B.3 + 3-lens review fixes ([c27bd850e](https://github.com/supernovae-st/nika/commit/c27bd850efb6e595d6243c8e663c643954c69443))
- **nika-kernel** — Add forward-compat seams for v0.95 Cortex + v0.100 WASM (Batch B) ([b68e58d4b](https://github.com/supernovae-st/nika/commit/b68e58d4b298561be7ea28d13fe76d37425f62e8))
- **nika-kernel** — Add 6 L0.5 traits + sealing pattern + mocks (ADR-034) ([32088a76d](https://github.com/supernovae-st/nika/commit/32088a76de26124d2ca6f3b1f4aaaebadfc264b5))
- **nika-kernel** — Inferresponse.cost: option<cost> + structured DenialKind ([0e2c3938e](https://github.com/supernovae-st/nika/commit/0e2c3938e8284b92bc2590f56271f8ea67460a4a))
- **nika-kernel** — Migrate MemoryId to UUIDv7 + deprecate cost_usd ([64afe77e3](https://github.com/supernovae-st/nika/commit/64afe77e3f916d381a5073c0dcb01013702cd466))
- **nika-kernel** — Add HttpStreamResponse::new() + #[non_exhaustive] on 20 mocks ([0aa41e8fe](https://github.com/supernovae-st/nika/commit/0aa41e8fec6c18df6c47c1a42a6e44efdd8ebbc6))
- **nika-kernel** — Add 7 forward-compat seams for v0.95/v0.100 ([a536b03ec](https://github.com/supernovae-st/nika/commit/a536b03ec4bf3b8b678352adc8027cc991388003))
- **nika-kernel** — Add prelude re-export hub (Q7) ([d967f4a7a](https://github.com/supernovae-st/nika/commit/d967f4a7a099a04647147d00aa863280aee80540))
- **nika-kernel** — Add AuditSink trait (Q12 Phase B — compliance channel) ([4be9a00a5](https://github.com/supernovae-st/nika/commit/4be9a00a51eeb020810ad4b99b9ac233e850aab3))
- **nika-kernel** — Genai_attrs OTel semconv bridge (Q13 executed) ([1ff35b759](https://github.com/supernovae-st/nika/commit/1ff35b7597264be4674582bbe71cdee34ecbe3ce))
- **nika-kernel** — Reserve WasmPluginError OutOfFuel + Trap + PluginCallContext (wave 4a r4) ([368820e42](https://github.com/supernovae-st/nika/commit/368820e42a129f33c3b10bf5dd7d8e89f047bb8c))
- **nika-kernel** — Reserve MemoryLifecycle trait with consolidate+prune (wave 4a r5) ([ac46b9ca5](https://github.com/supernovae-st/nika/commit/ac46b9ca5c7fcebda3239e3e746bd916fbaf1ca7))
- **nika-kernel** — Reserve parent_span_id + span links on SpanGuard (wave 4b #1) ([861f09bc9](https://github.com/supernovae-st/nika/commit/861f09bc9bef5dc51fc910f1d3765af26b1517c2))
- **nika-kernel** — Seal MemoryRecall/Remember/Forget per ADR-078 step 1 ([d642ee19e](https://github.com/supernovae-st/nika/commit/d642ee19e0b88329ea87342e0549f7607f7fc43e))
- **nika-kernel-ai** — Reserve ai::audio seam — stt + tts + vad traits (R6) ([1f34f3cc0](https://github.com/supernovae-st/nika/commit/1f34f3cc0a602719040a16b9979bf08652b81d39))
- **nika-kernel-ai** — Type vision + audio errors — Pattern A complete ([6af181177](https://github.com/supernovae-st/nika/commit/6af18117716e2959a96d3e301a6a4cd7904debc6))
- **nika-kernel-ai** — The tool-definition seam — ToolDefinitionProvider ([bb412572f](https://github.com/supernovae-st/nika/commit/bb412572f8b7f3b115d8b13c50aadb4a71294209))
- **nika-kernel-core** — Redact credential headers in http Debug ([8fcf9193d](https://github.com/supernovae-st/nika/commit/8fcf9193dafb7bb6cc38f4c05f55ebd1ce54d48e))
- **nika-lsp** — Editor UX — hover on task refs + bracket completion trigger ([2a3ac4a66](https://github.com/supernovae-st/nika/commit/2a3ac4a660977de14ff2fc6730010e3a43aa7f50))
- **nika-mcp** — In-binary MCP server closes the v0.81 cli floor ([7070b4636](https://github.com/supernovae-st/nika/commit/7070b46367c6bf2c9705564b3ccba843dabfeb61))
- **nika-ocr** — M2.2.b1 spec + b2 skeleton — ocrs backend, NIKA-1100..1109 ([a8013d62d](https://github.com/supernovae-st/nika/commit/a8013d62da4ee81504582ecebcf37be9bd367d2e))
- **nika-ocr** — M2.2.b3 wire real ocrs inference, close skeleton ([0a95cb157](https://github.com/supernovae-st/nika/commit/0a95cb1572cf4c3173c4933a575e2700c7fca30d))
- **nika-ocr** — Error one-voice — OcrError speaks NikaErrorCode ([a9d8ccbf4](https://github.com/supernovae-st/nika/commit/a9d8ccbf4b52984a8e632069d93fc6e835a2b48a))
- **nika-pack** — The 49-code registry + the emitted-within-registered ratchet ([990259c10](https://github.com/supernovae-st/nika/commit/990259c109ede3b497550700915b985b5083d8ce))
- **nika-providers** — Wire the gemini adapter — 14/14 providers ([5c50ffaa0](https://github.com/supernovae-st/nika/commit/5c50ffaa009847c014835baea7b04ac3cab558b6))
- **nika-runtime** — V2 spec-parity engine — concurrency + the full task pipeline ([dbb65c2c4](https://github.com/supernovae-st/nika/commit/dbb65c2c4765de87d46d6ba3d9f0bf5849b4abc5))
- **nika-runtime** — Property battery + jitter-herd fix — round 2 of the socratic pass ([2580838b6](https://github.com/supernovae-st/nika/commit/2580838b697e14455963a1e7c09f46d9e071fc82))
- **nika-runtime** — Agent telemetry wired — decisions on the canonical stream ([e269670e9](https://github.com/supernovae-st/nika/commit/e269670e914408ef23d4089f0b48772f3a9d9433))
- **nika-runtime** — Enforce permits.exec at the exec sink (NIKA-SEC-004) ([7c6cd9ceb](https://github.com/supernovae-st/nika/commit/7c6cd9ceb9350355e899bc454b08ec34c0319290))
- **nika-runtime** — Evaluate full cel-subset/0.1 via nika-cel ([0ae3c4f41](https://github.com/supernovae-st/nika/commit/0ae3c4f41ee6caf60825970fb511d7d1772a3569))
- **nika-runtime** — Output named-bindings + exec structured capture ([46e3f18fe](https://github.com/supernovae-st/nika/commit/46e3f18feac48a1f860d12387bd019e1d76ceb94))
- **nika-runtime** — Resolve workflow secrets from env/file at runtime ([b3264e4d7](https://github.com/supernovae-st/nika/commit/b3264e4d7c32a97aaef928e958309a6e5c31a8f9))
- **nika-runtime** — Warn on a reasoning model's blank answer (OBS-E) ([f18fef511](https://github.com/supernovae-st/nika/commit/f18fef51159734a02efe09510516068fc3cf81c4))
- **nika-sandbox-seatbelt** — MacOS command sandbox — adversarially verified ([c0941f145](https://github.com/supernovae-st/nika/commit/c0941f1450ac0808b22fb71ba9063bd33bb5c201))
- **nika-schema** — Scaffold crate — source tracking + error types (Round 1a) ([668a3b8bc](https://github.com/supernovae-st/nika/commit/668a3b8bc069f7ab41efb73ee3c92192fd9dc9cf))
- **nika-schema** — Add types module — 19 workflow config types (Round 2a) ([1cda7c5d0](https://github.com/supernovae-st/nika/commit/1cda7c5d05ec7bbaf0c3fdfd53ffc20a542822a3))
- **nika-schema** — Add trust, guardrails, and raw AST modules (Round 2b) ([9604d784e](https://github.com/supernovae-st/nika/commit/9604d784e25fca9e9bdde6c03f324f66e43058ee))
- **nika-schema** — Parser skeleton — top-level scalars (Round 2c) ([b85b612ca](https://github.com/supernovae-st/nika/commit/b85b612ca14bdeaee09553b6637b740c3a0677dd))
- **nika-schema** — Task-list parsing with action discriminator (Round 2d) ([2480822df](https://github.com/supernovae-st/nika/commit/2480822df8d40c3c1ce13f77a572294630bf910f))
- **nika-schema** — Task depends_on, condition, for_each (Round 2e-part-1) ([eac346c71](https://github.com/supernovae-st/nika/commit/eac346c71049282b6714c7e1e0fb243dbd9199d0))
- **nika-schema** — Codegen · invoke.tool no-drift gate + ADR-085 ([51ee7195a](https://github.com/supernovae-st/nika/commit/51ee7195a2948e39ecf46e6ee5923b4e197a3a47))
- **nika-schema** — Canonical v1 types — secrets, vars, retry, on_error, duration ([333ab9e9c](https://github.com/supernovae-st/nika/commit/333ab9e9c24401b7838e848bc931a1ff2cc1fcb5))
- **nika-schema** — Canonical raw ast + error taxonomy — envelope, task, verbs ([e5435fa2f](https://github.com/supernovae-st/nika/commit/e5435fa2fe009ed663185755e5d14d3d3d9abcba))
- **nika-schema** — Parser rewrite — canonical keys, strict/lenient, 4 verbs ([cf45bc7cf](https://github.com/supernovae-st/nika/commit/cf45bc7cf42727e05c6a30bf0d47cd34d88739a0))
- **nika-schema** — Expression module — CEL v0.1 subset, hand-rolled L0 ([abb2de0d6](https://github.com/supernovae-st/nika/commit/abb2de0d6e9633be025cb2341f9b739667aa7958))
- **nika-schema** — Analyzer — DAG topology, namespace resolution, when shape ([1e0a0ab68](https://github.com/supernovae-st/nika/commit/1e0a0ab68e9f4c804511a9894d197e76aaa6da3c))
- **nika-schema** — Spec-facing error codes — NIKA-<NS>-<NNN> surface ([a50f45bc7](https://github.com/supernovae-st/nika/commit/a50f45bc75a7ee4258b9dcec25454d628778e8e8))
- **nika-schema** — Core conformance harness + spec examples — 46/46 GREEN ([207a8db2e](https://github.com/supernovae-st/nika/commit/207a8db2e2763e3b3543ba981169d0cfa410c05f))
- **nika-schema** — One-obvious-way lint pass · 7 spec preference rules ([d3d62f797](https://github.com/supernovae-st/nika/commit/d3d62f7970c44b7ad4a35c51869baf593871f4be))
- **nika-schema** — Static binding validation · NIKA-VAR-003 at parse time ([752207aaa](https://github.com/supernovae-st/nika/commit/752207aaa9f035cf87ee91f3b3294862a4a98c9b))
- **nika-schema** — Parse the permits capability boundary ([9dfe2fda6](https://github.com/supernovae-st/nika/commit/9dfe2fda699dd9b7a2498bf8230613793beb5499))
- **nika-schema** — The check module — the nika check static pre-flight ([c369fc7ba](https://github.com/supernovae-st/nika/commit/c369fc7ba482393b854bc0f00a533199f7492c35))
- **nika-schema** — Runnable check example — the pre-flight, available now ([057b4bdd1](https://github.com/supernovae-st/nika/commit/057b4bdd15de30f7542f7083078525e5c7a02540))
- **nika-schema** — Cel parser learns cel-subset/0.1 — ternary, has, string tests ([5ad47b298](https://github.com/supernovae-st/nika/commit/5ad47b298644be245596048c723f15d5b335e4a6))
- **nika-schema** — Cost ceiling accounts for for_each fan-out ([c391e6183](https://github.com/supernovae-st/nika/commit/c391e61834839daa1bbc41ff18cf03b6b09e27df))
- **nika-schema** — Exec command string|array — the argv injection-safe form ([0a26e8703](https://github.com/supernovae-st/nika/commit/0a26e8703c57c374eaa4380b8ddfbffe6da75887))
- **nika-schema** — Spec catch-up — the four static-validator gaps close ([#121](https://github.com/supernovae-st/nika/issues/121)) ([0168f4abf](https://github.com/supernovae-st/nika/commit/0168f4abfd8360a1fc55c759b30dbf6257e7b087)) ([#121](https://github.com/supernovae-st/nika/pull/121))
- **nika-schema** — Deep conformance tier + DAG-004 + the registry remaps ([#122](https://github.com/supernovae-st/nika/issues/122)) ([3f3439cb2](https://github.com/supernovae-st/nika/commit/3f3439cb23b1f614ac0408a90202ad3ea80ecacd)) ([#122](https://github.com/supernovae-st/nika/pull/122))
- **nika-schema** — Ifc taint engine — provable information-flow control (ADR-092) ([1d1d231cd](https://github.com/supernovae-st/nika/commit/1d1d231cd749826daf5a5260ac42b8bb851be37e))
- **nika-schema** — Capability inference — --infer-permits (adr-092 #2) ([7b634d600](https://github.com/supernovae-st/nika/commit/7b634d60053c21839ceff33ee1e6d0c94508d3a1))
- **nika-schema** — Dataflow schema typing — typo'd fields caught statically (adr-092 #4) ([db7178c10](https://github.com/supernovae-st/nika/commit/db7178c1052ef92e432010de3af9b0cadcadcf75))
- **nika-schema** — Structural cost interval — retry and when:-aware envelope (adr-092 #5) ([e97e1e2a0](https://github.com/supernovae-st/nika/commit/e97e1e2a03441d60f33e709cc1d30d01f62dba56))
- **nika-schema** — Agent intelligence layer — deterministic suggestions + json repair surface ([1345259c6](https://github.com/supernovae-st/nika/commit/1345259c6dc937f37486e21b7564a9edbe294d16))
- **nika-schema** — Improvement hints — the deterministic ameliorateur ([3122669a4](https://github.com/supernovae-st/nika/commit/3122669a4cb7517eb3ebecd2a70dc97c6063b8b3))
- **nika-schema** — Analyzer did-you-mean + infallible maximal check report ([f82e2b2ef](https://github.com/supernovae-st/nika/commit/f82e2b2ef918dfc34e93d784dc17d597dc57e55f))
- **nika-schema** — Strictness hint — deterministic structured-output shape ([b1f395282](https://github.com/supernovae-st/nika/commit/b1f395282677e339bcbd325a377e6b68d79a6b07))
- **nika-schema** — Close the untrusted-input bound trio + bank proptests ([dcb76d1f1](https://github.com/supernovae-st/nika/commit/dcb76d1f119e68da58442a2337a29a6ba21c4327))
- **nika-schema** — Check example visual polish — DAG lanes + the colour seam ([c3a62b8ec](https://github.com/supernovae-st/nika/commit/c3a62b8eccf0c75f7853ed5bf62dfbed7ce8aba6))
- **nika-schema** — Builtin arg-shapes close four ledger rows + the lints corpus moves to the spec ([#123](https://github.com/supernovae-st/nika/issues/123)) ([af19c751f](https://github.com/supernovae-st/nika/commit/af19c751f4a0de132cc5dc16d610f5a5ca12872f)) ([#123](https://github.com/supernovae-st/nika/pull/123))
- **nika-schema** — Canonical theme — Role taxonomy + verb-gate colour logic ([3c69cfd2f](https://github.com/supernovae-st/nika/commit/3c69cfd2ff58ec4ff03bb7e4443bcca1ccb2df6e))
- **nika-schema** — Theme owns the glyph grammar too — first-class ASCII set ([2da0174b8](https://github.com/supernovae-st/nika/commit/2da0174b82a41aacb2566459fdd6443ae449c95d))
- **nika-schema** — Rustc-grade span diagnostics — source excerpts under findings ([634570aa3](https://github.com/supernovae-st/nika/commit/634570aa3df59580e0e53fed8cf6ef2781439b59))
- **nika-schema** — The verb theater — four execution models, animated ([10176204e](https://github.com/supernovae-st/nika/commit/10176204e1300d64028bda1602538da680761bfa))
- **nika-schema** — The event tape — real telemetry, one truth, two renderers ([d2fcfaf4d](https://github.com/supernovae-st/nika/commit/d2fcfaf4d4094aabef1541a0c732c8707888bda3))
- **nika-schema** — The tape speaks the full vocabulary — retry arc, stream, live meters ([a0b839829](https://github.com/supernovae-st/nika/commit/a0b839829a174df0818979b6c7b83201ba5c67c7))
- **nika-schema** — The third renderer — NDJSON wire for the event tape ([517798c31](https://github.com/supernovae-st/nika/commit/517798c31183d0abf754986b4701ea6cf5c99392))
- **nika-schema** — Ladder #6 — when:-gate reachability, arXiv-grounded, no SMT ([ddfff6198](https://github.com/supernovae-st/nika/commit/ddfff6198961b7953068930e75048a2d8f3ebe9f))
- **nika-schema** — Ladder #7 — the run certificate (AARA degree-1, no solver) ([fbdcd1684](https://github.com/supernovae-st/nika/commit/fbdcd16841fb85a567bc7b16d23999df1c7742a9))
- **nika-schema** — Parametric spend axis + ladder #9 first slice (metamorphic) ([2e6d045d1](https://github.com/supernovae-st/nika/commit/2e6d045d1f9db18bc309b977796ad4469fe20f90))
- **nika-schema** — Fetch arg-shape rules — closed mode set + pairings ([98db1a815](https://github.com/supernovae-st/nika/commit/98db1a815ed0b2fb98b35e9f742c0c27c1c58ef8))
- **nika-schema** — The certificate becomes CERTIFYING — witness + audit checker ([89ae459f4](https://github.com/supernovae-st/nika/commit/89ae459f4e6c91b49c205010a80990a9b0833378))
- **nika-schema** — The span axis + the research-conformance suite ([182f30807](https://github.com/supernovae-st/nika/commit/182f30807d5e7f9cb674d284c3e30cee134459a4))
- **nika-schema** — Fetch requires url — the check-time net widens ([5d3ae81c7](https://github.com/supernovae-st/nika/commit/5d3ae81c75e0ebecbdd2f32359dd21d18a0cec68))
- **nika-schema** — The parallelism rung — exact width, pinch, blast ([c929ee0dd](https://github.com/supernovae-st/nika/commit/c929ee0dd9cd9c63ea1f207fb6148c009e6e441f))
- **nika-schema** — Retry-effects hint — at-least-once made visible ([275a3ce1e](https://github.com/supernovae-st/nika/commit/275a3ce1e88439ecc55b9de97809a0126ed6e4b1))
- **nika-schema** — One-obvious-way/008 — steer to the injection-safe array form ([09255b4ff](https://github.com/supernovae-st/nika/commit/09255b4ffb26c3b4a400b731a12b6cc6ad938bd8))
- **nika-schema** — Arg-injection rule-pack — the array-form differentiator ([10623905b](https://github.com/supernovae-st/nika/commit/10623905b7f9393b8797bc82010c32914f6aa386))
- **nika-schema** — Sanctioned secret egress (IFC declassification) ([636de96ae](https://github.com/supernovae-st/nika/commit/636de96ae1dfbea79ae5c3a28bbd95333767027c))
- **nika-schema** — Flag unknown builtin arg keys in nika check ([e9f0a5f59](https://github.com/supernovae-st/nika/commit/e9f0a5f5971938eef90e536cf29b90a1af17fe6a))
- **nika-schema** — Cost pre-flight counts a static vars-array for_each ([102e99ed9](https://github.com/supernovae-st/nika/commit/102e99ed9f44209d9f83fbf02b9a13a981c7000e))
- **nika-schema** — Schema check catches enum/type + numeric-bound conflicts ([fda7e38f3](https://github.com/supernovae-st/nika/commit/fda7e38f354a148e8f577f17ac0021a3efd14da1))
- **nika-schema** — Static jq compile-check closes deep-gap 006 ([b2a62e7ad](https://github.com/supernovae-st/nika/commit/b2a62e7ad0a65399f1b604c190d859d79573a7a0))
- **nika-schema** — Static schema meta-check closes deep-gap 005 ([c82036db6](https://github.com/supernovae-st/nika/commit/c82036db685c8e4638c71a046d09f1046e69f441))
- **nika-schema** — One-obvious-way/009 warns on bare-iterator output bindings ([7fd27776d](https://github.com/supernovae-st/nika/commit/7fd27776de5465eef6b5e75fdb1ccef0495cd01f))
- **nika-screen** — Error one-voice — ScreenError speaks NikaErrorCode ([1cfbc8812](https://github.com/supernovae-st/nika/commit/1cfbc88123d7d2a3c7a5deb71dc9d71f3d22a418))
- **nika-types** — Gate no_std/alloc seam (Phase F1 — forward-compat WASM) ([d48db4897](https://github.com/supernovae-st/nika/commit/d48db48976833c299705e9b0e4a68865601ba1d3))
- **nika-types** — Reserve EmbeddingSpec (wave 4a r1, adr-029 seed) ([001ae0b6f](https://github.com/supernovae-st/nika/commit/001ae0b6f2680e4086621036203871087445f43d))
- **nika-types** — Reserve trust on MemoryFrameRef + tenant on RecallQuery (wave 4a r2+r3) ([41e8a1467](https://github.com/supernovae-st/nika/commit/41e8a1467212aa4cd6bcb6eb08a25d91bf42cbd9))
- **nika-types** — Add Timestamp + WallDuration value types (q9, wave 4b #3 seed) ([c5d292b6e](https://github.com/supernovae-st/nika/commit/c5d292b6eb8d28c60452a81d856b6a7bd379ada8))
- **nika-types** — Docid + score + rankeddoc newtypes for 9-satellite cascade ([3ae189ae7](https://github.com/supernovae-st/nika/commit/3ae189ae79c8b989713111c3e59e0eacf13f0f0c))
- **nika-types** — Extract-mode vocabulary — closed stdlib v0.1 set ([0ea48316c](https://github.com/supernovae-st/nika/commit/0ea48316c442a62b466aaa40bb5affe7670431bd))
- **nika-types** — Delay_for_ms — the shared backoff-with-jitter semantics ([2c5ee514c](https://github.com/supernovae-st/nika/commit/2c5ee514c472ca6621f17a6c69eb92264c9ed130))
- **nika-types** — Retry budget + retry-after honoring — the anti-storm kit ([f373fd645](https://github.com/supernovae-st/nika/commit/f373fd645b4c427ea52df6795e804525d8f9db44))
- **nika-verb-agent** — The intelligence layer — routing, stall guard, compose, telemetry ([f252b1500](https://github.com/supernovae-st/nika/commit/f252b1500129dd2ba086a4d31a4050709e5ed438))
- **nika-verb-agent** — Run_observed — the run-scoped observer seam ([56885ef3d](https://github.com/supernovae-st/nika/commit/56885ef3d0eed07b8071fee2f61a4e2707ebfee8))
- **nika-verb-agent** — Parallel intra-turn dispatch — concurrent resolve, request-order fold ([397d79e84](https://github.com/supernovae-st/nika/commit/397d79e84b3734663a3c8f3648020614f69c0244))
- **nika-verb-exec** — Scaffold the s10 L2 verb crate — WIP pre-admission ([733c113d8](https://github.com/supernovae-st/nika/commit/733c113d8f8a7f76a50ba5b3ab7cbbd91da34cb5))
- **nika-verb-exec** — Real argv execution, no shell (injection fix) ([75d8e59cb](https://github.com/supernovae-st/nika/commit/75d8e59cb13397aab88b3086568ae83a60149343))
- **nika-verb-infer** — Scaffold the s9 L2 verb crate — WIP pre-admission ([cf4783180](https://github.com/supernovae-st/nika/commit/cf478318096b9dc19e0ca9f739f25eaa27ce56f2))
- **nika-verb-invoke** — Scaffold the s11 L2 verb crate — WIP pre-admission ([da1d55d96](https://github.com/supernovae-st/nika/commit/da1d55d960cb454dd2c5db1477350c0a1925813f))
- **schema** — Static missing-required-arg check for all 22 builtins ([8456f1f6e](https://github.com/supernovae-st/nika/commit/8456f1f6e3909c6d83987837868c5ec36c39a61c))
- **screen** — M2.1.b2 crate skeleton + nika-1000..1009 codes ([546fb201c](https://github.com/supernovae-st/nika/commit/546fb201cb1b7b4fbf97998fa22edda2cbf0e6dc))
- **screen** — M2.1.b3 single-shot capture via xcap · close skeleton ([cf9d4cd80](https://github.com/supernovae-st/nika/commit/cf9d4cd80c7461b19352da9e40b77eb08bb65849))
- **screen** — M2.1.b4 capture_stream via mpsc · skeleton fully closed ([08a5c180a](https://github.com/supernovae-st/nika/commit/08a5c180a84c923d320139a895e840b6459a49c2))
- **screen** — M2.1.b5 adr-081 guards 6+7 real + enforced ([0daec9bf7](https://github.com/supernovae-st/nika/commit/0daec9bf7c0998b8cd28dc5eed97ced63c1fd803))
- **screen** — M2.1.b6 12-gate close · gap-3 shim carry-forward ([e975320cd](https://github.com/supernovae-st/nika/commit/e975320cd772026961d194cbd9c1e6fd245eafa6))
- **workspace** — Scaffold nika-infer-local — sovereign inference seam (ADR-091) ([00ed3ee96](https://github.com/supernovae-st/nika/commit/00ed3ee968a8221782240d593247308072cac3e0))
- **workspace** — Nika-infer-local generation-control core — template, sampling, stop ([9ef8f4a4a](https://github.com/supernovae-st/nika/commit/9ef8f4a4a6293c56a4146226cd2109dfaf7f65f4))

### 🐛 Bug Fixes
- **release** — Bump post-0.90 development to `0.91.0-dev` and fail releases whose tag does not match the Cargo workspace version, preventing Homebrew/local binaries from sharing a version while exposing different CLI flags.
- **adr** — Address review P0 + P1 findings from 3-agent swarm ([9a395bb6f](https://github.com/supernovae-st/nika/commit/9a395bb6ff782e4efbb35d3920418b540a6a93bb))
- **adr** — Resolve 13 relationship asymmetries + harden scripts ([baa680cac](https://github.com/supernovae-st/nika/commit/baa680cace9e23c91ecfe590eddc6ee387c7838e))
- **adr** — Refresh evidence paths for kernel subdir reorg ([fcbb00841](https://github.com/supernovae-st/nika/commit/fcbb008418540c50fc393051b27090af773f3b8f))
- **adr** — Adr-034 requires add adr-016 · close adr-016 enables backref ([9745be740](https://github.com/supernovae-st/nika/commit/9745be740d93e439aa54c38d0da251c73056cdd2))
- **adr** — Adr-007 + adr-014 related add adr-016 · close cascade backrefs ([ee841d42f](https://github.com/supernovae-st/nika/commit/ee841d42f211c655f7d3bd3ae58add2bc225893e))
- **adr** — Batch close 54 bidirectional related backrefs · diamond w2 cascade ([4e8ca67a7](https://github.com/supernovae-st/nika/commit/4e8ca67a7f98f1cb83925200b432fc66773492e6))
- **catalog** — Correct COMMUNITY_EXTENSIONS.md doc link post tools→crates ([9ecca6af3](https://github.com/supernovae-st/nika/commit/9ecca6af385fa93b9fc044aa1adb9933af3ed2dd))
- **catalog** — Total_cmp + name tie-break for deterministic suggestions ([ef7d873ad](https://github.com/supernovae-st/nika/commit/ef7d873ad960f49bc84bad17452a17b461174643))
- **ci** — Make crate-size glob recursive (P1-6) ([898e2de1e](https://github.com/supernovae-st/nika/commit/898e2de1ee86adbc72bf2e7b16a480f4b761018e))
- **ci** — Add scoped-fail for new crate ADR coverage (P1-5) ([593394b8b](https://github.com/supernovae-st/nika/commit/593394b8b49ea3993e4daad3a4c7cc9016703c83))
- **ci** — Add allowlists for pre-existing ratchet violations (P1-2 follow-up) ([b22e49cb9](https://github.com/supernovae-st/nika/commit/b22e49cb969931556788d2bbb4dd53b69574ce52))
- **ci** — Update allowlists + deny.toml for crates/ rename ([f780c37e5](https://github.com/supernovae-st/nika/commit/f780c37e566aee3ac9f0e4a2061702eb62e5f3a4))
- **ci** — Mutation-floor v2 — budget-as-budget + cross-platform honesty ([74f1c1409](https://github.com/supernovae-st/nika/commit/74f1c1409560d8b520d5156c52f1b13fefa52b22))
- **ci** — Vector 40 audits ai/ traits — vision + audio Pattern-A gap surfaced ([5da3a02c6](https://github.com/supernovae-st/nika/commit/5da3a02c65fb0160448a4fb1480defe218a63150))
- **ci** — Allowlist first_balanced_span — fn-length heuristic false positive ([495119ade](https://github.com/supernovae-st/nika/commit/495119adec921bae9e93a0ce50e50895f164d1a8))
- **ci** — Unblock the push train — char-literal-aware fn counter + mit-0 ([ed10784c7](https://github.com/supernovae-st/nika/commit/ed10784c7504ea91dbbea7c68909475d839becc4))
- **ci** — Adr validator learns the l1.5 service layer ([4737bf184](https://github.com/supernovae-st/nika/commit/4737bf18411ac21b53e3ac0662e08f9f0674222b))
- **ci** — Crate-size counts prod LOC — the mutation ratchet must not fight the size budget ([e023a73e8](https://github.com/supernovae-st/nika/commit/e023a73e860304f2f4b9aa5d6694cc8e29ac292f))
- **ci** — Mutation-sandbox skip at the SOURCE — the global --lib had blast radius ([9c2683625](https://github.com/supernovae-st/nika/commit/9c2683625bc13d25efb4bd3cd8a9ad59128c8765))
- **ci** — Adr-validate catches duplicate IDs — the gate was blind to collisions ([9a3c9d2b4](https://github.com/supernovae-st/nika/commit/9a3c9d2b408667f868ef5e5e17b651d18a813757))
- **ci** — Strip string literals + line comments in brace-counting gates ([d970e3f4e](https://github.com/supernovae-st/nika/commit/d970e3f4e8f835c313a9233c558f5affedf0c2c2))
- **ci** — Refresh-status survives an empty wip array ([d0fb8d17b](https://github.com/supernovae-st/nika/commit/d0fb8d17b83240166b2d93250db4261aaab78f1a))
- **cli** — Surface nika:log + nika:emit on stderr, not /dev/null ([bc5179392](https://github.com/supernovae-st/nika/commit/bc5179392f6239de5bd64465216068c202ca89cb))
- **docs** — Close 3 review-swarm P1s — MetricsExporter, sealing, L4 label ([54a402efe](https://github.com/supernovae-st/nika/commit/54a402efe2dcd9c940c94d0eff03af97a4db9540))
- **dx** — Address phase c-g review-swarm p1 findings ([d1469b5cf](https://github.com/supernovae-st/nika/commit/d1469b5cf5b101a83e362538083e715844736a03))
- **dx** — Gitnexus auto-reindex hook — match compound git commands ([56e793d0b](https://github.com/supernovae-st/nika/commit/56e793d0bc2ca6f907e5b836ab974cf49ee51ed5))
- **dx** — Apply compound-command regex to all 4 git posttooluse hooks ([4d6430e34](https://github.com/supernovae-st/nika/commit/4d6430e34b5e39f408023267104c86fae1eb493e))
- **dx** — Privacy-strict refactor + pretooluse compound regex + reindex lockfile ([66a8846bb](https://github.com/supernovae-st/nika/commit/66a8846bbf47ba176a5dd56db0f32d060f389dd5))
- **dx** — Address executive-swarm findings — privacy + injection + stale counts ([983463154](https://github.com/supernovae-st/nika/commit/98346315468abe2923faa2b6da03fb2c56540fcb))
- **dx** — Remove trailing commas from settings.json ([d24de258f](https://github.com/supernovae-st/nika/commit/d24de258f912c5975da636be00004ca49024a2f0))
- **dx** — Roadmap.sh WIP seam + L1.5 row + derived M2 frontier ([dd8a88b5f](https://github.com/supernovae-st/nika/commit/dd8a88b5fae055fe4e158268761d1d59addb23ef))
- **error** — Register kernel code ranges + private MemoryId + mock imports ([544da1ad3](https://github.com/supernovae-st/nika/commit/544da1ad3c6b7d5dd16d963dbbf8d7e93f045a94))
- **error** — Code_help covers Schema + Provider; fix stale ranges ([00352928c](https://github.com/supernovae-st/nika/commit/00352928c97cba4c92c389afc641f38c500802ad))
- **hooks** — Anchor co-author trailer check to line boundaries (P0-2) ([3f4cb1650](https://github.com/supernovae-st/nika/commit/3f4cb16501454f1757a14159fbf53b6fb54c47ea))
- **hooks** — Remove escaped backslash in privacy pattern (P0-3) ([f8e89bb1d](https://github.com/supernovae-st/nika/commit/f8e89bb1d227a08cc7b2a0f437d9e71d832f9c1b))
- **hooks** — Simplify squash co-author detection (P0-5) ([451919c68](https://github.com/supernovae-st/nika/commit/451919c685e854b4904f8a32e634442fcea3a142))
- **hooks** — Use --prefix=none + sed for reverse-dep resolution (P1-1) ([b71bf9f7c](https://github.com/supernovae-st/nika/commit/b71bf9f7c39b46ea8845e0d88b9114d4e01a1408))
- **hooks** — Use git toplevel for activity-log path (P1-9) ([7514b7942](https://github.com/supernovae-st/nika/commit/7514b79427ca4b76957f8249ab8c376c96e7bce4))
- **hooks** — Capture ratchet exit code before errexit kills subshell (P1-2) ([1991acb0e](https://github.com/supernovae-st/nika/commit/1991acb0eb210256cded149a9c7db816aa81d7c6))
- **hooks** — Portable stdin detection in force-push-guard (P1-3) ([a8092f2a5](https://github.com/supernovae-st/nika/commit/a8092f2a5365a853bd272d73bf558a62132f21f2))
- **hooks** — Wire post-rewrite hook for ADR seal check (P1-4) ([62dc35cb3](https://github.com/supernovae-st/nika/commit/62dc35cb39c13619d6c947c5b1b87dcd37c135a5))
- **hygiene** — V2 — 5 new vectors, bug fixes, catalog-verify alpha.4 ([495efc5f5](https://github.com/supernovae-st/nika/commit/495efc5f5c1eabe1d1043e1006a52444a97d690c))
- **hygiene** — Rename lefthook-engine.yml → lefthook.yml ([7a148c13c](https://github.com/supernovae-st/nika/commit/7a148c13c7d4c7faae86df64026dfb3ab252c794))
- **hygiene** — Forward pre-push stdin to force-push-guard ([ed2345055](https://github.com/supernovae-st/nika/commit/ed2345055e5f225f259e90e9282b86f8e492b3cc))
- **hygiene** — Force-push-guard works without stdin forwarding ([d6ef7989e](https://github.com/supernovae-st/nika/commit/d6ef7989ed744db28eeac8910c5c4801ffd26b0f))
- **hygiene** — Gate engine-hygiene on RED only, not YELLOW ([8167ffc9e](https://github.com/supernovae-st/nika/commit/8167ffc9e5ef2ab38546762d84e0a7affc2243d7))
- **hygiene** — Drop unused COMMIT_TYPE + FOUND_BLANK from validator ([06bda01a1](https://github.com/supernovae-st/nika/commit/06bda01a1d874f8aee3684f0e2222a49733e9931))
- **hygiene** — Expand block-private-paths self-exclusion (P1-7) ([501fc13d1](https://github.com/supernovae-st/nika/commit/501fc13d1c4eb9ce228627247f9aa3c4367c8074))
- **hygiene** — Tighten Claude trailer detection (vector 13, P0-1) ([41b6451a0](https://github.com/supernovae-st/nika/commit/41b6451a030e2fd08f5f91f628c6ab9471d7fc8b))
- **kernel** — Add displayid::new constructor · inv-19 gap ([0dfb13683](https://github.com/supernovae-st/nika/commit/0dfb13683bc755944c8a01965304b8b6b76f4f91))
- **kernel-browser** — Doc render · arc generic prose · adr-081 enables schema clean ([1245e976f](https://github.com/supernovae-st/nika/commit/1245e976f9bc4e1ffa910675f154615b70059e80))
- **kernel-core** — Redact credential headers in HttpStreamResponse Debug ([b67be8da3](https://github.com/supernovae-st/nika/commit/b67be8da30772b99cdd78d23bc38c82c3d116327))
- **kernel-mock** — Real glob matching in MockFs, not substring ([ec30145da](https://github.com/supernovae-st/nika/commit/ec30145daaa70584bd0552996ed243350058dbf4))
- **kernel-mock** — Fs/http/clock mocks implement the Dyn seams ([ea2e30902](https://github.com/supernovae-st/nika/commit/ea2e30902f3a654c3a86ebf2832579088bbf4e8e))
- **m2** — Suppress panic-payload leak in all JoinError mappings (Guard-1 class) ([f1aae8164](https://github.com/supernovae-st/nika/commit/f1aae8164df99f939eca628829e478f86347b3da))
- **mintlify** — Revert docs.json rename + Node 22 setup ([6f772968f](https://github.com/supernovae-st/nika/commit/6f772968fbeb2b086ed221d9c0d1ec4e3cb28440))
- **mintlify** — Escape <500 MDX parse error in crates.mdx ([f8f604d25](https://github.com/supernovae-st/nika/commit/f8f604d25a59c6b76af529e3dd3faccbc36d2511))
- **nika** — Polish stale docs + error messages + honest Gate-5 note ([a46a30d84](https://github.com/supernovae-st/nika/commit/a46a30d840d0bc4c455abb1a5ea9bce50412e09e))
- **nika-a11y** — Guard 3 fail-closed on secure-marker read error ([a84d96438](https://github.com/supernovae-st/nika/commit/a84d96438e97d7d6513e47032606d6debfd27d67))
- **nika-a11y** — Guard 3 scrubs the secure field's ENTIRE subtree ([8273bae27](https://github.com/supernovae-st/nika/commit/8273bae2730e5539b5d570ea2b90016199f006e2))
- **nika-blob** — Clean the temp file on write failure, not just rename ([a812217e6](https://github.com/supernovae-st/nika/commit/a812217e625fd4ac4c88022c767c28eae8ee7c81))
- **nika-bm25** — Test-scope expect allow + the stale-rlib lesson banked ([c8bbd5f03](https://github.com/supernovae-st/nika/commit/c8bbd5f03bb7eb1829122677e467f7d3c0b746d6))
- **nika-bm25** — Export PruneStats — top_k_pruned_stats return type was unnameable ([6bf6a2508](https://github.com/supernovae-st/nika/commit/6bf6a2508fde688283a43fa7a781c23b8dc47fc0))
- **nika-browser** — Harden Guard 5 — node-identity pin + no failure-downgrade ([fba1360c1](https://github.com/supernovae-st/nika/commit/fba1360c13b587cbcd4bbca596dd9ea993f1b7d6))
- **nika-browser** — Occlusion hardening — scroll-stable point + full-depth subtree ([5ec5415f9](https://github.com/supernovae-st/nika/commit/5ec5415f94f2e63b3427f5355f3476d638479f0e))
- **nika-builtin** — Prompt rejects a wrong-typed default value ([892802fcb](https://github.com/supernovae-st/nika/commit/892802fcb739be025304e9af1172294953d2e1db))
- **nika-builtin** — Nika:glob exclude string form + nika:date weeks unit ([763b15aec](https://github.com/supernovae-st/nika/commit/763b15aec7afb815b3ad623f3048e110729c013a))
- **nika-builtin** — Nika:date add/subtract calendar units via Zoned ([d787d5879](https://github.com/supernovae-st/nika/commit/d787d5879de41e34ede173c904c7ef8c7ef0d5d9))
- **nika-builtin** — Canonicalize fs paths before permits.fs match ([457f27213](https://github.com/supernovae-st/nika/commit/457f2721355e67e989ab1ae1106711970da1c745))
- **nika-builtin** — Nika:convert has_header is a strict bool ([3bab272fc](https://github.com/supernovae-st/nika/commit/3bab272fc17e1f9eeacf7d9b084caeac62b18276))
- **nika-builtin** — Glob absolute patterns + grep-on-file error ([01e49cc39](https://github.com/supernovae-st/nika/commit/01e49cc39bdaa78a812da111d8f086ca85554e9f))
- **nika-builtin** — Nika:prompt confirm-default error names mode: input ([93072824d](https://github.com/supernovae-st/nika/commit/93072824d729c1dede579d6f67b37420d3cca683))
- **nika-builtin** — Nika:inspect reports unavailable, not fake-empty ([ab44c3ee9](https://github.com/supernovae-st/nika/commit/ab44c3ee96e46ff22ee9da3c38e7bcebc215ccac))
- **nika-builtin** — Classify transient nika:fetch failures as retryable ([98507a7f2](https://github.com/supernovae-st/nika/commit/98507a7f262baefae79b9a2ddf29160f777d25d2))
- **nika-builtin** — Exact-file permits.fs path admits its own new file ([e4863ddcb](https://github.com/supernovae-st/nika/commit/e4863ddcb04aa950986446740ec77d2b6a2deba6))
- **nika-builtin** — Nika:glob strips a leading ./ from a relative pattern ([a59429475](https://github.com/supernovae-st/nika/commit/a59429475b87b98440c1c725c7ec88a8ef6a0616))
- **nika-builtin** — Map an SSRF block to NIKA-SEC-005 (F-01) ([64e5f7215](https://github.com/supernovae-st/nika/commit/64e5f7215b3f38466d80576b34d162f7979cee5f))
- **nika-builtin** — Grep re-enforces the fs boundary per matched file ([ec84d9e39](https://github.com/supernovae-st/nika/commit/ec84d9e3908cfd096d9e22abaa8ee4c10433410b))
- **nika-builtin** — Doc-integrity + model-facing description cleanup ([91c3acdfa](https://github.com/supernovae-st/nika/commit/91c3acdfa65b67e6fac2ffeef804390605e2bdcf))
- **nika-catalog** — Address 3-agent review findings (P0 + P1) ([ffe8af986](https://github.com/supernovae-st/nika/commit/ffe8af986e825d50d4d16553c676e3bd7b3f749d))
- **nika-catalog** — Address 2b review swarm — 2 P0 + 2 P1 same session ([ce0eab1bd](https://github.com/supernovae-st/nika/commit/ce0eab1bd7f74e318e3c7c20c1e049e884d21f09))
- **nika-catalog** — Validate_caps_patch — require every field on [defaults] ([731f11bfe](https://github.com/supernovae-st/nika/commit/731f11bfec9092f3711b9b5e85e1ff9f7df81f5e))
- **nika-catalog** — Canonicalise scope.providers at parse — check_any_last_in_scope ([2d1f53c15](https://github.com/supernovae-st/nika/commit/2d1f53c15277b1cd6fc7558b5c173a4df9997574))
- **nika-catalog** — Address session 3 review swarm p1/p2 findings ([fe311de4b](https://github.com/supernovae-st/nika/commit/fe311de4b59f468d991773b8989bd7baa2db9ba6))
- **nika-catalog** — Renumber NIKA-230..235 → NIKA-010..015 (code collision) ([1d9f85c13](https://github.com/supernovae-st/nika/commit/1d9f85c138f061112a6431b8ee08339ec9305890))
- **nika-catalog** — Remove broken doc link to OverlayCatalogDataSource ([e44bb0789](https://github.com/supernovae-st/nika/commit/e44bb07895b6da7adc9a8cc6572448fd12f5cd56))
- **nika-catalog** — Review swarm P0 fixes (inv #19 + region guard) ([cbc5209bb](https://github.com/supernovae-st/nika/commit/cbc5209bb04b7cfeae45a8a4a4430487efc96d10))
- **nika-catalog** — Add #[non_exhaustive] to ParseRegionError ([aedfdf4c4](https://github.com/supernovae-st/nika/commit/aedfdf4c4a9d3ec40480b8e443912c977eac81d9))
- **nika-catalog** — Wire tokenizer variants into TOML rules (review P1) ([820bd1949](https://github.com/supernovae-st/nika/commit/820bd1949b3759caf7a41047a7c43e0bc8e13e37))
- **nika-cel** — Close 2 Gate-11 review P1s + 2 forward-compat gaps ([e70950c04](https://github.com/supernovae-st/nika/commit/e70950c04e7bce21540d793eb9a7c562982ccffa))
- **nika-cel** — Bound expression depth, close stack-overflow DoS ([e4e474762](https://github.com/supernovae-st/nika/commit/e4e474762fef429d449620043a03f67488d2554c))
- **nika-cel** — Numbers compare by value on the continuous number line ([2396e6d4c](https://github.com/supernovae-st/nika/commit/2396e6d4c823a13cc56c6e479bb5fb99325e54dc))
- **nika-cli** — Fold the S6 review — §6 permits field + ANSI-safe columns + stable explain contract ([7354d40c8](https://github.com/supernovae-st/nika/commit/7354d40c85d1e1e5879a4b17abde97a4dfd7b273))
- **nika-cli** — Exempt the provider HTTP path from the SSRF guard ([ffd4cf600](https://github.com/supernovae-st/nika/commit/ffd4cf6004ddecabe971ababb926cd96f0cf0616))
- **nika-cli** — Report the resolved provider's response_format (BUG#11) ([c1355f515](https://github.com/supernovae-st/nika/commit/c1355f51548fc9bace8e4453393c2b72e1d7ea95))
- **nika-cli** — Examples-run help no longer claims the run verb is unshipped ([3cdf1b37c](https://github.com/supernovae-st/nika/commit/3cdf1b37c3e125e32a5cb6c40acf9ad581defb48))
- **nika-cli** — Parse-stage errors now carry their spec code ([b355fb918](https://github.com/supernovae-st/nika/commit/b355fb91818a37ad0b834018cb0fe27adb5c3f80))
- **nika-cli** — Escape model/tool in graph mermaid+dot labels ([a9fef08c9](https://github.com/supernovae-st/nika/commit/a9fef08c90a94c527177a3adf23f3ff3850ef9ff))
- **nika-cli** — `new --from '?'` lists templates without a dest ([7de9107ec](https://github.com/supernovae-st/nika/commit/7de9107eca08f3da4f35b90d685e78538f56220d))
- **nika-cli** — Route_intent ignores boilerplate/stopword queries ([c6aa0020e](https://github.com/supernovae-st/nika/commit/c6aa0020eadc32d2b8ac6cc8c7a2efa5ac4436c9))
- **nika-cli** — Trace recovers a crashed run's truncated tail ([b5d27519c](https://github.com/supernovae-st/nika/commit/b5d27519cecab0b6ba2d7cc7febbee7cbc643b73))
- **nika-cli** — Human run flags conflict with the machine modes ([343423a6b](https://github.com/supernovae-st/nika/commit/343423a6bc9e2750077f4fbc159fe5712f34b1aa))
- **nika-cli** — The public binary name is `nika`, not `nika-cli` ([4857d4819](https://github.com/supernovae-st/nika/commit/4857d48197d5ac2c2f9a7f28b2cc43b9ef9ceb63))
- **nika-engine** — Rules index + gitignore generated skills dir ([068e1e31b](https://github.com/supernovae-st/nika/commit/068e1e31bc2fa09aaea09088c75bf3a8645e1221))
- **nika-engine** — Reconcile memory satellite count 3 → 9 crates ([68240f922](https://github.com/supernovae-st/nika/commit/68240f922222d589cf08ece98084f97d82a0a2c3))
- **nika-error** — Remove colliding NIKA-XXX placeholders (Wave 1.3) ([2fe8401d1](https://github.com/supernovae-st/nika/commit/2fe8401d1803ab43c246b5c56adac6e1333eaff2))
- **nika-exec-runner** — Cap captured output at 64 MiB — NIKA-054 ([dd788d7d9](https://github.com/supernovae-st/nika/commit/dd788d7d9908e1e63fddf363374e027ef5ddac24))
- **nika-exec-runner** — Close the argv re-exec bypass + shell expansion TOCTOU ([31614bbd9](https://github.com/supernovae-st/nika/commit/31614bbd959b89788007262369132b0cf2afb45f))
- **nika-extract** — Depth guard — close 2 under-count bypass P0s ([ef6f13477](https://github.com/supernovae-st/nika/commit/ef6f13477c0d3598a045c31080f57e264a027f4d))
- **nika-extract** — Unblock the push gates — fn split + brace-balanced fixture ([0edf354fe](https://github.com/supernovae-st/nika/commit/0edf354fe60a6e7400d9666d0ca9326e020feebd))
- **nika-extract** — Article fallback compares trimmed length ([a5f6a7eb3](https://github.com/supernovae-st/nika/commit/a5f6a7eb30fc669335f949f30f9724e13f4ca03c))
- **nika-extract** — Review-swarm fixes + microdata property cap ([a643466c4](https://github.com/supernovae-st/nika/commit/a643466c46bdd4d3d195b6d1236a9103afe54221))
- **nika-fs** — Clean the temp file on write failure, not just rename ([9c6dfaca1](https://github.com/supernovae-st/nika/commit/9c6dfaca1a00d2344ca7d141da717e264f94e31a))
- **nika-http** — Widen the SSRF oracle to every non-public-unicast range ([52c182691](https://github.com/supernovae-st/nika/commit/52c182691bc0c89b0dc4ed755b378b57e6c97f42))
- **nika-http** — Drop the dead metadata-IP hostname entry — the IP branch is authoritative ([0bc0d71c4](https://github.com/supernovae-st/nika/commit/0bc0d71c4d0e5365f80ecc63ea71755baf7f4307))
- **nika-http** — Comma-join repeated response headers — rfc 9110 lowering ([d09a8ba47](https://github.com/supernovae-st/nika/commit/d09a8ba47fde9682775856c3f8d32169f3a8697d))
- **nika-http** — Enforce permits.net.http at runtime (NIKA-SEC-004) ([8c2d1d87a](https://github.com/supernovae-st/nika/commit/8c2d1d87aa900fa86c87cba630685572b1f9992f))
- **nika-http** — Permits.net.http gates before DNS resolution ([8e9df5568](https://github.com/supernovae-st/nika/commit/8e9df5568453340d104642559f15f4b6ed626c36))
- **nika-infer-local** — Fold the 3-lens review — 2 P1 + 4 P2/P3, e2e wire contract ([6f8ec45fd](https://github.com/supernovae-st/nika/commit/6f8ec45fde6bfd9e15fb449fa2cd0d1022fe91be))
- **nika-infer-local** — Execute the mutation audit — kill survivors, wire min-p, O(window) stop ([803469a3c](https://github.com/supernovae-st/nika/commit/803469a3c17c9b3ca1533163b7d41e00014f1211))
- **nika-infer-local** — Enforce the context window — ContextOverflow fires ([67d56b0d8](https://github.com/supernovae-st/nika/commit/67d56b0d8bf15cf361582c8c0c701bc75a0b04c6))
- **nika-infer-local** — Fold candle-backend review + gate-7 bench + GGUF arch de-hardcode ([d982f5e12](https://github.com/supernovae-st/nika/commit/d982f5e12050ab30355ce271689dd0f114b47699))
- **nika-kernel** — Seal SecretResolver + Acquire/Release CancelCtx + reserve NIKA-700..819 ([940908c7a](https://github.com/supernovae-st/nika/commit/940908c7ad28194b9415a129dbcb86910d49f551))
- **nika-kernel** — Close 5 bug-hunt findings before review swarm ([5a5b1e6fa](https://github.com/supernovae-st/nika/commit/5a5b1e6fa1df4a7ea74614b7206212d168c7a5a4))
- **nika-kernel** — Close review-swarm P1s — registry, proptest, cost bridge, oracle doc ([244dcc807](https://github.com/supernovae-st/nika/commit/244dcc807796f8c3bea8e3bd9c5e9852f2324f67))
- **nika-kernel** — Intra-doc link TenantId::DEFAULT → default_tenant (gate 8) ([fdc10d916](https://github.com/supernovae-st/nika/commit/fdc10d916361aedc2dad1e6e3eac0abf6340f380))
- **nika-kernel-mock** — Tool-executor doubles implement the Dyn seam ([b063577f4](https://github.com/supernovae-st/nika/commit/b063577f44f4af39cc05ac184570343adfc5ae31))
- **nika-lsp** — Complete admission hygiene — layer registry · status block · error-voice ([2f17dfeac](https://github.com/supernovae-st/nika/commit/2f17dfeacfaac93cc55eee982558f0e0e94998f9))
- **nika-mcp** — Fold checkpoint review — version negotiation · batch · full report ([c7dc18e25](https://github.com/supernovae-st/nika/commit/c7dc18e2530e984c806f7a62f01074b1193a83d5))
- **nika-ocr** — Saturating bbox span avoids i32 overflow panic ([3970a83e7](https://github.com/supernovae-st/nika/commit/3970a83e7f96b350aad8d68b9f6cf20154922724))
- **nika-pack** — Tojson structured data before nika:write in 3 showcases ([72f253e7e](https://github.com/supernovae-st/nika/commit/72f253e7ee2b5dd425473602d77dafa0cbd1bfb9))
- **nika-pack** — Re-sync embedded canon to SSOT (builtins 22→23 · compose) + derive-test ([1244973d3](https://github.com/supernovae-st/nika/commit/1244973d385a1e37a603ee6e069cd2737b8db14d))
- **nika-pack** — Re-vendor embedded pack from spec SSOT ([de5029602](https://github.com/supernovae-st/nika/commit/de5029602517cb520986caa72916d4ef37948e57))
- **nika-providers** — Gemini in-band errors speak the shared status table ([d6727ad7d](https://github.com/supernovae-st/nika/commit/d6727ad7df71180331df6892978bfc4a431baf00))
- **nika-providers** — Normalize openai strict structured-output schema ([6afce214a](https://github.com/supernovae-st/nika/commit/6afce214abe37fa2b0be2e9f6089e23f6a7f9bba))
- **nika-providers** — Sanitize tool names for openai + anthropic ([91c360a01](https://github.com/supernovae-st/nika/commit/91c360a016f8752b26f7144e189bcbd189748958))
- **nika-providers** — Adapt gemini structured-output schema ([a679beddb](https://github.com/supernovae-st/nika/commit/a679beddb48b64fe73cdd297719ad45f412e2fe3))
- **nika-providers** — Gemini output_tokens fold thoughts — budget guard was blind ([a5e42ebdd](https://github.com/supernovae-st/nika/commit/a5e42ebddf0702cb70a7a0a93676a0c9f368aef0))
- **nika-providers** — Rewrite oneOf to anyOf for openai strict ([ca08eca45](https://github.com/supernovae-st/nika/commit/ca08eca45c0d1224ed6bdd56ee2eb17edcc80574))
- **nika-providers** — Rewrite const + strip uniqueItems for openai ([08c8977d5](https://github.com/supernovae-st/nika/commit/08c8977d5d151deed65cb2f267d9d69a300983cb))
- **nika-providers** — Inline $ref/$defs for gemini, error on cycle ([4d2d59904](https://github.com/supernovae-st/nika/commit/4d2d59904f80e9c600c7e39b4f799a803a557a59))
- **nika-providers** — Map multi-type unions to anyOf for gemini ([232f06655](https://github.com/supernovae-st/nika/commit/232f066555731691a3a5c514c6e460dc72af6bce))
- **nika-providers** — Preserve integer enum value types for gemini ([ea1715512](https://github.com/supernovae-st/nika/commit/ea17155124105789d88f9e5daf9c1e673a1d7703))
- **nika-providers** — Rewrite const + strip uniqueItems for gemini ([718c82125](https://github.com/supernovae-st/nika/commit/718c821259720e8585d5511e825995899e91145b))
- **nika-providers** — Stringify gemini enum members (revert ea1715512) ([c274216d9](https://github.com/supernovae-st/nika/commit/c274216d9f98f4f0184f77ff9b08a6e7d1f77a98))
- **nika-runtime** — Land the v2 module files — repair the lagging-file commit ([0c38a5698](https://github.com/supernovae-st/nika/commit/0c38a56982ceb540a788302e91a882df3a2dd8a8))
- **nika-runtime** — Agent telemetry review fold — evidence survives the timeout ([7818280df](https://github.com/supernovae-st/nika/commit/7818280dfde758cc8418e10276d9196d111a96b3))
- **nika-runtime** — Review fold — registry consts, spec-code pins, the gate-scope hazard ([b9fbff9f3](https://github.com/supernovae-st/nika/commit/b9fbff9f34345d2e726cc289e958e76118804caf))
- **nika-runtime** — Deny shell under a program allowlist; fan-out on_finally permits ([4277d2bb8](https://github.com/supernovae-st/nika/commit/4277d2bb86d4aba2268f926c1536295172b071f2))
- **nika-runtime** — Wire exec cwd/env/stdin through to the spawn ([c4a4136b5](https://github.com/supernovae-st/nika/commit/c4a4136b5715db5b2d9e463b8bd3067e47c63c88))
- **nika-runtime** — Match on_codes against the user-facing spec code ([76139737d](https://github.com/supernovae-st/nika/commit/76139737d6a3ae170c9bdd477e0aabe1bd4d1524))
- **nika-runtime** — Enforce NIKA-VAR-009 — typed outputs validated at run end ([46a45a0db](https://github.com/supernovae-st/nika/commit/46a45a0db4e66f708c70ab63b50373a63dafcae2))
- **nika-runtime** — Run banner states the declared permits boundary ([530dde4cf](https://github.com/supernovae-st/nika/commit/530dde4cf05314561656854d601553e205aa4e64))
- **nika-runtime** — Cel eval errors carry spec-plane wire codes ([c5c8f19bd](https://github.com/supernovae-st/nika/commit/c5c8f19bdf67ec9d9553b6af4e5cf13a401ba317))
- **nika-runtime** — Fold adversarial review findings on the error surface ([789391b2e](https://github.com/supernovae-st/nika/commit/789391b2e42c94484a6663bfa72b87103a206a4b))
- **nika-runtime** — Render honors the backslash-escape (spec 04) ([1b0c9a82c](https://github.com/supernovae-st/nika/commit/1b0c9a82cad5fdab893fef364dc943ff21bd55de))
- **nika-runtime** — Render close-find is quote-aware (spec 04) ([f296fc10b](https://github.com/supernovae-st/nika/commit/f296fc10b871277ab05f6796c4d6057a65d61aab))
- **nika-sandbox-seatbelt** — Refuse over-granting permits (audit P1) ([f5ee11962](https://github.com/supernovae-st/nika/commit/f5ee11962a925a7f3a9b32b64f6672ae7c9924e5))
- **nika-schema** — Rename lints module to preference_rules · doc collision ([b8d3609f4](https://github.com/supernovae-st/nika/commit/b8d3609f4ba2e962cf8adabac840d339abfa29d5))
- **nika-schema** — Fold review findings — net/fs literal escapes, secret + cost fixes ([9a0c20510](https://github.com/supernovae-st/nika/commit/9a0c20510707816a115c828510f4c26a6d7626af))
- **nika-schema** — Repair-fix idiom unified + convergence test + honest gaps (review fold) ([73b24bb0b](https://github.com/supernovae-st/nika/commit/73b24bb0b7654ed8723949afa064440d33dfaf10))
- **nika-schema** — Close the proven stack-overflow class — two loud depth bounds ([41a6dcb81](https://github.com/supernovae-st/nika/commit/41a6dcb81efa4f4c3c7fad9612737f41e8ec1f78))
- **nika-schema** — Check honors the locked exit-code contract + json parse payload ([0188d4e3a](https://github.com/supernovae-st/nika/commit/0188d4e3a0cac57a41d7b411f1c8cf92c061425e))
- **nika-schema** — Fold the review-swarm round — ascii contract + Verdict + hardening ([de517b532](https://github.com/supernovae-st/nika/commit/de517b5320967c89774de78a7514157834558f9c))
- **nika-schema** — Unbreak mutation testing + kill the predicted span survivor ([f2eec1917](https://github.com/supernovae-st/nika/commit/f2eec19172b6da2475a87025ab2aa13d369ea023))
- **nika-schema** — The loud-skip allow names the right lint (print_stderr) ([a55a7428a](https://github.com/supernovae-st/nika/commit/a55a7428aeec93688d0dbefecb595086cbe69842))
- **nika-schema** — Review fold — HK bound restored, DoS cap, one voice ([250a05f72](https://github.com/supernovae-st/nika/commit/250a05f7291b9d35aaafc30ae2a9b53a51280267))
- **nika-schema** — Agent-whitelist namespace gate — reject a second colon (invoke parity) ([2a5f26ec5](https://github.com/supernovae-st/nika/commit/2a5f26ec54219ae0ea126469087659d9d5c88fca))
- **nika-schema** — Arg-injection catalog holes + per-kind suggestions (audit) ([c5a19ccf4](https://github.com/supernovae-st/nika/commit/c5a19ccf4c78847abe3f57451fa27943b6846b76))
- **nika-schema** — Treat infer/agent prompt as an IFC egress sink ([1dcaa5d2f](https://github.com/supernovae-st/nika/commit/1dcaa5d2f924c981204f7d78be891a44b755b64d))
- **nika-schema** — Closed-island CEL grammar errors are NIKA-VAR-005 not VAR-008 ([7f7816941](https://github.com/supernovae-st/nika/commit/7f7816941c47aa6e5ecdfb33e8f8400d62d636e0))
- **nika-schema** — Cap postfix chain depth — untrusted-input stack overflow ([d782f16ad](https://github.com/supernovae-st/nika/commit/d782f16ad3d1917d30412251942cae9923dcdd34))
- **nika-schema** — Intern the IFC taint-trace to kill an O(n2) DoS ([50598fac5](https://github.com/supernovae-st/nika/commit/50598fac56972ee055c7e85a965f8bdf5e6eb826))
- **nika-schema** — Bound the gate in-list scan and secrets membership ([d05abe56b](https://github.com/supernovae-st/nika/commit/d05abe56bbd935b636394dc31caf060bcfedd0a6))
- **nika-schema** — Remove quadratic dedup in when-gate literal scan ([fe9fdf72d](https://github.com/supernovae-st/nika/commit/fe9fdf72d054156e6a32291af6f5ca5b89565bbf))
- **nika-schema** — Non_exhaustive on public source-position types ([8ab013bd4](https://github.com/supernovae-st/nika/commit/8ab013bd4c57ce5888879f0d6b651a5c8953e6c5))
- **nika-schema** — Adapt flow.rs IFC test to interned-taint signature ([e1127ed74](https://github.com/supernovae-st/nika/commit/e1127ed747bfd53e81b836c7f9296771abf624f4))
- **nika-types** — Negative sub-ms remainder in WallDuration Display ([e2bd999ad](https://github.com/supernovae-st/nika/commit/e2bd999adf225840a31f0e37ed12b108270ead1d))
- **nika-types** — Permits.net.http host match is case-insensitive (RFC 4343) ([fd0c3277a](https://github.com/supernovae-st/nika/commit/fd0c3277a86918d74a8137d702f5062d60d11a8d))
- **nika-verb-agent** — Review fold — reach invariant, wire shape, amplification ([d0c5bf125](https://github.com/supernovae-st/nika/commit/d0c5bf125e3a7e2367b28a35374cb0603a762b27))
- **nika-verb-agent** — Agent:compose is not a tool invocation — stop reporting it as one ([aac3170a1](https://github.com/supernovae-st/nika/commit/aac3170a14c0c64f3a20812002920c3cb6a1390c))
- **nika-verb-agent** — Enforce agent schema at the wire (BUG#11) ([7a0e94e43](https://github.com/supernovae-st/nika/commit/7a0e94e432c4615387e4fe8d32b104524d516757))
- **nika-verb-agent** — Agent schema re-ask drops orphan tool_calls (openai) ([f82e4ba4d](https://github.com/supernovae-st/nika/commit/f82e4ba4d482ba963c0abdbd8c34d0e76ccb237b))
- **nika-verb-exec** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([f82c76d1e](https://github.com/supernovae-st/nika/commit/f82c76d1e39630b31c85333080031393ef79760e))
- **nika-verb-exec** — Reject NUL in an env value (review F2) ([62a7ff8b3](https://github.com/supernovae-st/nika/commit/62a7ff8b34a6100356f6e2c28a6160c83f085f27))
- **nika-verb-exec** — Exec blocklist hit speaks NIKA-SEC-001 not EXEC-002 ([09dd0816b](https://github.com/supernovae-st/nika/commit/09dd0816b5f59b00ae84f16cdf2d5cee208b90df))
- **nika-verb-infer** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([ff4865ff8](https://github.com/supernovae-st/nika/commit/ff4865ff8b5eec9e1c73ee382174ef1a145ab275))
- **nika-verb-invoke** — Fold gate-11 swarm findings — 3 lenses, 0 P0 ([23500f7ce](https://github.com/supernovae-st/nika/commit/23500f7ce80b5a71a539af8d0c93701093e58f0b))
- **schema** — Stop double-backticking task ids in DAG-003 + loop-local ([befd93e90](https://github.com/supernovae-st/nika/commit/befd93e901d11cfdac4a00525fd67a6cdfb497f0))
- **spec** — Correct nika-catalog-verify layer L2 → L4 ([5f9e9553f](https://github.com/supernovae-st/nika/commit/5f9e9553f4bab5e5f0ec5372eb71bf7bd3279938))
- **stabilize** — Allow MIT-0 + refresh exec-runner LOC anchor ([2a8553942](https://github.com/supernovae-st/nika/commit/2a85539425cb74accd096116e1b3fd064edebdc2))
- **workspace** — Add crates/ to index after rename ([20a49306a](https://github.com/supernovae-st/nika/commit/20a49306a247056132c54a9897376bdc53acf788))
- **workspace** — Defer nika-bm25 layer entry to W3 admission · adr-038 cleanup ([8ddb750e6](https://github.com/supernovae-st/nika/commit/8ddb750e64f605e6cc2d6753df42b52c3af4c001))
- **workspace** — Invoke tool outputs keep their structured type ([8d556aada](https://github.com/supernovae-st/nika/commit/8d556aadafe35cc0c33e5fce8cb81cdfc1b64040))

### ⚡ Performance
- **diamond** — Release profile + const fn + blueprint v1.3 amendments ([def291c2b](https://github.com/supernovae-st/nika/commit/def291c2b2f8fbbdcc86a7c8b00495d3f1808644))
- **nika-bm25** — Post-admission stabilization · architect + rust-perf converge ([7fcd75fef](https://github.com/supernovae-st/nika/commit/7fcd75feffdff3f0ce8fe33392aca5c6cf8bc806))
- **nika-extract** — Single-pass tidy_text — drop two full-size copies ([c8781a6ec](https://github.com/supernovae-st/nika/commit/c8781a6ecb010bdef54f813445ef2fa560ba9b58))

### 🔨 Refactors
- **catalog** — Reconcile builtin set to spec 26 + ADR-084 ([a7193eba0](https://github.com/supernovae-st/nika/commit/a7193eba0e59cafdaf208661d4b8a080dc30f03f))
- **catalog** — Nika:csv_to_json → nika:convert · ADR-086 rams sweep ([c346cba19](https://github.com/supernovae-st/nika/commit/c346cba19a5d69ee1422a974db87af099f56e666))
- **catalog** — Nika:sleep + wait_until → nika:wait · ADR-087 rams ([bcf5c8e63](https://github.com/supernovae-st/nika/commit/bcf5c8e63380ab4755681e7b0e368082a46de59b))
- **catalog** — 4 introspection → nika:inspect · ADR-088 rams sweep ([37af410a4](https://github.com/supernovae-st/nika/commit/37af410a4c17fe8669953eba2c4b7109ef2ecc5a))
- **ci** — Centralise the workspace-members parser in _lib.sh ([13e2e6c8c](https://github.com/supernovae-st/nika/commit/13e2e6c8c51719e5070787577d6aa1413483d02c))
- **dx** — DX file routing cleanup + mintlify v4 rename ([14edf9b68](https://github.com/supernovae-st/nika/commit/14edf9b6830f672dd00df96139ef1fdd9747375a))
- **dx** — Add commit-granularity rules ([c87fc9372](https://github.com/supernovae-st/nika/commit/c87fc937274e951dd19e0a27a3e4baa84247a318))
- **dx** — Expand post-commit hooks for admission + push reminders ([218933375](https://github.com/supernovae-st/nika/commit/2189333759299c0c90620360a0b0e75e8386a111))
- **dx** — Move hq dashboard to port 4242 + wire shadcn/magicui/tailwind/threejs MCPs ([55d8c4beb](https://github.com/supernovae-st/nika/commit/55d8c4beb03f9d1c78074eb87c33afb0f2d13fa6))
- **dx** — Count nika-screen WIP · refresh status block to fe2b ([3b2daaf02](https://github.com/supernovae-st/nika/commit/3b2daaf02eb173407438fc6264a4668fb48fb456))
- **dx** — Lock v0.90 crate target at 42 (was 40-42 range) ([ba2f65236](https://github.com/supernovae-st/nika/commit/ba2f652366000597c99177e06d38245b818b1f29))
- **dx+docs** — Hygiene all GREEN + mintlify rewrite ([cd3cde9a1](https://github.com/supernovae-st/nika/commit/cd3cde9a101ad364d49a4847903dc743b344d748))
- **error** — As_str on Category/Severity — delete explain's parallel taxonomy ([bafa20762](https://github.com/supernovae-st/nika/commit/bafa2076234fce76299a8271c390401235d136e6))
- **hygiene** — Single-source the WIP split + fix vector 3 LOC drift ([7af76eeb5](https://github.com/supernovae-st/nika/commit/7af76eeb5097274f701c0b67ecabd192838153f3))
- **kernel** — Co-locate memoryerror nikaerrorcode impl · diamond w2.2 nuke drift ([ba9bd9c1b](https://github.com/supernovae-st/nika/commit/ba9bd9c1b259c53c5e91288a6542a04141fb285c))
- **kernel** — M1 polish · contract fix h1 h2 + sprint 0 additive ([4836cf7aa](https://github.com/supernovae-st/nika/commit/4836cf7aa97aeb359a7a8b42add7b63c6a7f8162))
- **kernel** — Ec-4 ratchet · captured_at_ms → captured_at_ns · ns canonical ([438234ad2](https://github.com/supernovae-st/nika/commit/438234ad2dc98488e7a0cec980d3f52fad6f89c3))
- **kernel** — M2 trio implements the Send trait variants ([9c455e2cd](https://github.com/supernovae-st/nika/commit/9c455e2cdfe316dd5cdc63423226d626f0bedb21))
- **mintlify** — Restructure nav to 2 tabs (Guide | Reference) ([365a31d5f](https://github.com/supernovae-st/nika/commit/365a31d5f8056ed16aca1e7b14492309ae7455e5))
- **mintlify** — Reference workspace — live snapshot + constellation + delete duplicate ([8e85241af](https://github.com/supernovae-st/nika/commit/8e85241afb30744a75b3122178f8feeafe4e429f))
- **mintlify** — Split docs out to supernovae-st/nika-docs repo ([eb671f8a6](https://github.com/supernovae-st/nika/commit/eb671f8a6bc640d80d45eeeeb444905de008fd25))
- **nika-a11y** — Migrate to Pattern A — A11yError typed at the kernel ([29b749631](https://github.com/supernovae-st/nika/commit/29b74963156296a35d87a15ef2c0bca8fb5844e6))
- **nika-bm25** — Q6 split · core (pure-algo) + kernel (adapter) ([0c3b3f4a2](https://github.com/supernovae-st/nika/commit/0c3b3f4a275fe6c45b492ec27a6c7986480b04a4))
- **nika-bm25** — Revert q6 split · option e feature-gated · v1.4 reinforce ([edb72e9cb](https://github.com/supernovae-st/nika/commit/edb72e9cb2aed74f9c4ab5a1ec4d8a3fe10360af))
- **nika-bm25** — Sota rust 2026 perfecting pass post rust-pro + rust-architect audit ([3fa8e5eb8](https://github.com/supernovae-st/nika/commit/3fa8e5eb8d2f27e69ca3d2b3741e91b3ad775824))
- **nika-bm25** — Rank.rs — ONE selection algorithm, heap-bounded, tie-bug caught ([e655948f5](https://github.com/supernovae-st/nika/commit/e655948f5aeb5925b2d732aa8a4ff2474b2d3d4e))
- **nika-catalog** — Collapse public API from 3 paths to 2 — data/ is pub(crate) ([3360f1023](https://github.com/supernovae-st/nika/commit/3360f1023749a1a5a11c2d495654ce038937f904))
- **nika-catalog** — Migrate model_capabilities to TOML-driven rule table ([8c5cb4866](https://github.com/supernovae-st/nika/commit/8c5cb48668d9520dfea60a6e853a91ed7ae983be))
- **nika-catalog** — Hardening pass on Session 2a (5-agent review) ([e766a122c](https://github.com/supernovae-st/nika/commit/e766a122c2843a2ec2965964aa48222ff2c614ad))
- **nika-catalog** — Post-commit 5-agent audit findings ([9feb96956](https://github.com/supernovae-st/nika/commit/9feb9695668e309b5400930744650149e6a79cdf))
- **nika-catalog** — All_pricing — struct literals → ModelPricing::new ([e123acafc](https://github.com/supernovae-st/nika/commit/e123acafc45d7e2d6f60fb84f20c802253f96a3f))
- **nika-catalog** — Retire supports_vision — use input_modalities.contains(Image) ([34a488207](https://github.com/supernovae-st/nika/commit/34a4882073c2e2997426c12781dec254e27274c2))
- **nika-catalog** — Wire build.rs to nika-catalog-codegen · nuke twin ([0e85c9618](https://github.com/supernovae-st/nika/commit/0e85c9618b7f5416953b9606c058c904c5159bf1))
- **nika-catalog-codegen** — Satisfy push-hook ratchets · fn-length + loc-limits + machete ([074fc0614](https://github.com/supernovae-st/nika/commit/074fc0614baef084c669e38f3a138f4d97d094b6))
- **nika-cli** — Fed_back helper — the wave-3 test ducks under the fn cap ([be582b247](https://github.com/supernovae-st/nika/commit/be582b2470daff888808b966274a53d40562c822))
- **nika-clock** — Implement the ClockDyn Send companion ([9d2f58d88](https://github.com/supernovae-st/nika/commit/9d2f58d880522ff2168d34360a93840bc986214f))
- **nika-error** — Split into nika-types + nika-error ([5baeee044](https://github.com/supernovae-st/nika/commit/5baeee044d12c94b767a22cbe28cd1b81fff0e15))
- **nika-extract** — Sitemap event arms move into SitemapParser ([61e63159e](https://github.com/supernovae-st/nika/commit/61e63159e10d0060b57de4bebad9098a1e4f1658))
- **nika-http,nika-cli** — Give net the NetBoundary newtype + one capabilities_of derivation ([3f86e33ba](https://github.com/supernovae-st/nika/commit/3f86e33baa0f3101eaf7ab44dd2e60f3750b7a71))
- **nika-input** — Extract pure keymap module + structural proptest ([c7943ab9b](https://github.com/supernovae-st/nika/commit/c7943ab9bea6a470fabe6f328ec449ccc4c5fe07))
- **nika-kernel** — Prepare split — descend shared types to nika-error ([1513da3a2](https://github.com/supernovae-st/nika/commit/1513da3a2d0f7464c750f3db0c0a9ad488509c38))
- **nika-kernel** — Drop ObservabilitySink (Q12 Phase A — 5 → 4 channels) ([1119f42a5](https://github.com/supernovae-st/nika/commit/1119f42a53a6b94c5fa7fa9331da66070bf247d8))
- **nika-kernel** — Reassign provider error codes 380-429 → 330-379 ([1b812e664](https://github.com/supernovae-st/nika/commit/1b812e6643df01e2fb7dd62049568f6467acd19f))
- **nika-kernel** — Trim facade hub to actual deps — Gate 11 review finding ([a41db4098](https://github.com/supernovae-st/nika/commit/a41db4098858b6c5f1f9e313121a94a38ba4d216))
- **nika-kernel-mock** — Align MockShell to the Send-variant traits ([c5b44e170](https://github.com/supernovae-st/nika/commit/c5b44e1703614d5962b93c76e666779a4d54d5d3))
- **nika-ocr** — Migrate to Pattern A — OcrError typed at the kernel ([a6719e213](https://github.com/supernovae-st/nika/commit/a6719e2131c4c419f1ff4f93cfdb5bfdb1e33569))
- **nika-pack** — Error_codes() typed accessor — one parser, every consumer ([1e0d8b83d](https://github.com/supernovae-st/nika/commit/1e0d8b83d874aaaab86eaeb8c771dc6404d54f1f))
- **nika-providers** — Split gemini schema adapter to its own module ([4c6b0cc23](https://github.com/supernovae-st/nika/commit/4c6b0cc23ac118773bae8bbefc55e832cc3266a1))
- **nika-runtime** — Split run() under the fn-length ratchet ([47b7f43d2](https://github.com/supernovae-st/nika/commit/47b7f43d26d20fda21dc9c68f3e37dc0424ab234))
- **nika-runtime** — Extract dispatch_result from attempt_loop ([153f74d66](https://github.com/supernovae-st/nika/commit/153f74d66655dd1975f41dccdf82f7b70b85b36b))
- **nika-schema** — Nuke brouillon-era types ([5d08fd5be](https://github.com/supernovae-st/nika/commit/5d08fd5be60e7f89eecadfd0a32378e3497ac860))
- **nika-schema** — Rename parser expect to expect_token ([e6a1630f3](https://github.com/supernovae-st/nika/commit/e6a1630f36db3231fe706138a6c5b1a6610ea268))
- **nika-schema** — Split preference_rules into a tests.rs dir module ([292adc31d](https://github.com/supernovae-st/nika/commit/292adc31dcd8f4eb04a93a92a7aabb9e5371b9fe))
- **nika-screen** — Migrate to Pattern A — ScreenError typed at the kernel ([b8043ea96](https://github.com/supernovae-st/nika/commit/b8043ea96d2896a6cc99701e638a35b864740250))
- **nika-verb-agent** — Run() under the 100-line cap — extract classify_turn ([9dec6fff9](https://github.com/supernovae-st/nika/commit/9dec6fff9d9db19fa5cb9b90ca5a693ec5cecba8))
- **nika-verb-agent** — Tests split to src/tests.rs — the file cap unblocks the train ([15e0558b7](https://github.com/supernovae-st/nika/commit/15e0558b7eb0f79bde3a77b28ba125c2d5a54025))
- **nika-verb-agent** — Nuclear-review judo fold — fence parity, dead path, one predicate ([a9bd9f9d4](https://github.com/supernovae-st/nika/commit/a9bd9f9d4531d49dd65f5ab57864e965c9ccc10d))
- **schema** — Drop fetch verb · nika:fetch is a builtin not a verb ([b8e736d32](https://github.com/supernovae-st/nika/commit/b8e736d32add62009bf6be31384f71391fd80722))
- **scripts** — Cluster by responsibility + 3 READMEs ([d3099f99a](https://github.com/supernovae-st/nika/commit/d3099f99a576951a613edc7e669c3e8ab1d2f763))
- **types** — Privatise TaskId + ToolCallId inner fields (Wave 1.2) ([0e3ca19de](https://github.com/supernovae-st/nika/commit/0e3ca19dee9aff94fc2fcfa967eb8d5785ad63cb))
- **workspace** — Rename tools/ to crates/ + add layer metadata ([bb6863714](https://github.com/supernovae-st/nika/commit/bb6863714dcbddc101d9322d0a72113b53a4c16a))
- **workspace** — Split four functions under the 100-LOC fn cap ([76e4f8d0d](https://github.com/supernovae-st/nika/commit/76e4f8d0d55298751d86dd39ae0eea5240d604c9))

### 📚 Documentation
- **adr** — Bootstrap diamond adr process + 9 inaugural decisions ([4cac646e9](https://github.com/supernovae-st/nika/commit/4cac646e9d99e287654e029831370812280b7754))
- **adr** — Add adr-010 through adr-014 (5 sota improvement decisions) ([f0e032bd3](https://github.com/supernovae-st/nika/commit/f0e032bd3906d5e26441a4039e36e83c47c94517))
- **adr** — Add adr-015 expect-test for inline snapshot assertions ([751e85ec8](https://github.com/supernovae-st/nika/commit/751e85ec861107c49c004d476b4b199d093bd89f))
- **adr** — Add bidirectional cross-references in Related sections ([199119e94](https://github.com/supernovae-st/nika/commit/199119e9439e94f4f6058bd9ec9f9874e71cd5c2))
- **adr** — Add ADR DX system -- schema, scripts, updated template ([fe75f384f](https://github.com/supernovae-st/nika/commit/fe75f384f38d0a1c47ee272f1ce3fa5cc3b081d9))
- **adr** — Migrate 15 ADRs to YAML frontmatter + generate indexes ([196ba13ce](https://github.com/supernovae-st/nika/commit/196ba13cef763e64e2c11179c2fd437dfaaab738))
- **adr** — Write ADRs 016-020 — kernel design decisions (Batch F part 1) ([9b9e75d15](https://github.com/supernovae-st/nika/commit/9b9e75d1553001bd09d5003997200dc4b75f0445))
- **adr** — Write ADRs 033-034 — Phase C L0/L0.5 expansion plans ([69c284245](https://github.com/supernovae-st/nika/commit/69c2842458ee689452f3fa2cdf3714d92ac732e3))
- **adr** — Regenerate index.toml + index.json (22 ADRs) ([8bced775c](https://github.com/supernovae-st/nika/commit/8bced775c0d5bcc119d2470eec98f823ef299bfa))
- **adr** — Lock foundation v0.81 — 7 new ADRs + ADR-006 amendment ([6ee7d99de](https://github.com/supernovae-st/nika/commit/6ee7d99decc171d391d23e474d79b5c707101642))
- **adr** — Add ISP capability-axes × crate matrix (batch v.2) ([1718f2cc1](https://github.com/supernovae-st/nika/commit/1718f2cc151ee7992a6b038ec70464b5b87f8acb))
- **adr** — Regenerate index.toml + index.json (22 → 30) ([66beb4d17](https://github.com/supernovae-st/nika/commit/66beb4d177d49415f87dfdd47b975b0b981e01f0))
- **adr** — Stub ADR-029/030/031/032/035 for Wave 4A/4B reservations ([d58d981f8](https://github.com/supernovae-st/nika/commit/d58d981f8b77cbe38f600d9e4798883f47d60cff))
- **adr** — Add adr-037 bottom-up diamond progression (Accepted) ([50cbf9d2b](https://github.com/supernovae-st/nika/commit/50cbf9d2be3b05a7dd5f98540a748980682b6cc1))
- **adr** — Amend adr-028 — feature scheduling dropped per ADR-037 ([c3c4be389](https://github.com/supernovae-st/nika/commit/c3c4be3898414d738548039b8c522c72b2ab9703))
- **adr** — Add adr-036 MSRV policy stub (reserves FCI-036) ([61d3e830b](https://github.com/supernovae-st/nika/commit/61d3e830b8e77937fc3902823497665480355c0a))
- **adr** — Revert adr-028 status to accepted (schema valid enum) ([06c92950b](https://github.com/supernovae-st/nika/commit/06c92950b9da228a444f88336c3972a9da42517f))
- **adr** — Fix adr-028 date format (YYYY-MM-DD) + separate amended_date ([ac700434b](https://github.com/supernovae-st/nika/commit/ac700434b9e20f8955cd60f8052e2ccd9ae2eb47))
- **adr** — Fix adr-036/037 schema validation (id + affects_crates) ([bed1aabe0](https://github.com/supernovae-st/nika/commit/bed1aabe0b1d958c1ea3772266c5d549cf07d5c8))
- **adr** — Adr-038 nika-bm25 admission pre-flight · 12-gate readiness · proposed ([a632d3ca3](https://github.com/supernovae-st/nika/commit/a632d3ca35845e525df850dd91f7d6506e7c0fb2))
- **adr** — Nika-bm25 l1 row + workspace metadata · diamond w2.5 ([4c5fa01da](https://github.com/supernovae-st/nika/commit/4c5fa01daf18a39e5e48f4999b46a605dff0c658))
- **adr** — Adr-038 enables [] · drop ADR-NNN placeholders ([93d7f245c](https://github.com/supernovae-st/nika/commit/93d7f245c3fd877100d7713a0cd1dcd6b4bd1bcf))
- **adr** — Adr-040 cargo feature matrix · zero-cost modularity ([1934fcb62](https://github.com/supernovae-st/nika/commit/1934fcb62339d060d600931983d6fa7cb2352475))
- **adr** — Adr-039 + adr-041 + adr-042 · phase 1.5 architecture trio ([0f1ef3f25](https://github.com/supernovae-st/nika/commit/0f1ef3f25b4abd412fa528942c535140e2a90bd9))
- **adr** — Review-cycle amendments · adr-039 040 041 042 ([1612fb800](https://github.com/supernovae-st/nika/commit/1612fb80069a3399b01cd9de0fe0a21c48608506))
- **adr** — Ship ADR-078/079/080 trio · v1.5 best-architects 2030 audit close ([6317a8b27](https://github.com/supernovae-st/nika/commit/6317a8b27d13a210ed75875e8ca99263a36bfe02))
- **adr** — Adr-081 l1 effect-crate guard contract · 7 guards forever ([3e40c18b3](https://github.com/supernovae-st/nika/commit/3e40c18b3b92ea5d5498bef10643510978b411c5))
- **adr** — Adr-089 nika:json_diff jq-subsume REJECTED · keep rfc-6902 patch ([27c563c79](https://github.com/supernovae-st/nika/commit/27c563c79327d4af4da15da24d0e9a34b1808fe4))
- **adr** — Reciprocate related cross-links across the 081-089 cohort ([6c3b12a85](https://github.com/supernovae-st/nika/commit/6c3b12a8590700e44d4ec379475c29cf12ca2ee6))
- **adr** — Lock enables[] = curated-highlight (DRI D-2026-05-30) ([5cd40bb4c](https://github.com/supernovae-st/nika/commit/5cd40bb4c994f2685f7fe40e21a8651508901b1d))
- **adr** — Sovereign local inference via pure-Rust candle sidecar (ADR-091) ([eb60cbb94](https://github.com/supernovae-st/nika/commit/eb60cbb948eb839286b154b1ed21339ec250ba58))
- **adr** — Adr-092 — make nika check a verifier, not a linter ([1810d241a](https://github.com/supernovae-st/nika/commit/1810d241acd2e5b9829acbb91339e45f0865df89))
- **adr** — Adr-093 tiny_http sidecar server + adr-094 nika-pck registry architecture ([300c52a3f](https://github.com/supernovae-st/nika/commit/300c52a3f36bf772cc079e4a353b5bbbe928a753))
- **adr** — Adr-092 evidence path follows suggest.rs out of check/ ([1c6f56456](https://github.com/supernovae-st/nika/commit/1c6f56456cb3913f6dfd2a9d7c88ff5365607716))
- **adr** — Adr-092 second check-example path follows the dir migration ([6822e1206](https://github.com/supernovae-st/nika/commit/6822e120678639c3181e6773b60143c5133ad03a))
- **adr** — Exec verb security architecture (ADR-095) ([9e9462636](https://github.com/supernovae-st/nika/commit/9e9462636623299a42fc171ab333e1e6f19b1515))
- **adr** — Reconcile ADR-095 with the reserved plugin::sandbox (per-platform crates) ([651fef111](https://github.com/supernovae-st/nika/commit/651fef11157415b9ae0b52e961a7a17adbf0c9d4))
- **adr** — Nika-sandbox-seatbelt crate-spec (Gate 1) ([8734fdc44](https://github.com/supernovae-st/nika/commit/8734fdc44ae8a9b3c18678763e8f235674225881))
- **adr** — Amend ADR-002 — real semver toward a 1.0 launch ([e8c7aad64](https://github.com/supernovae-st/nika/commit/e8c7aad648c9c21d8637a05aee4392fb86d333c7))
- **adr** — Align ADR cross-refs to real semver toward 1.0 (D-2026-06-20-N1) ([a1548d8e8](https://github.com/supernovae-st/nika/commit/a1548d8e8ee6a29bab3bf6ae28b50bb61124d8ea))
- **adr-003** — Record social→structural gate enforcement (Gate 5 + ADR-081) ([65787a058](https://github.com/supernovae-st/nika/commit/65787a05844c0326f5745696a93489b3cc04060f))
- **adr-080** — V1.1 amendment · phantom-CVE scrub + nika error code migration + seccomp-bpf ([80d5da96b](https://github.com/supernovae-st/nika/commit/80d5da96bd22478c29d7df078588ed7559088312))
- **adr-090** — Structural doctrine enforcement — gates project the SSOT ([ff54083af](https://github.com/supernovae-st/nika/commit/ff54083af2911c616e06c8e38a5b3de18aecba2f))
- **agents** — Un-ignore AGENTS.md + refresh HEAD + olympus rename ([44dd2ac2f](https://github.com/supernovae-st/nika/commit/44dd2ac2faaf43598b6eb1bf90f37496988c5962))
- **agents** — De-drift the agnostic entry — projection-by-default snapshot ([91f65f237](https://github.com/supernovae-st/nika/commit/91f65f237f6f44c7f262301bf5473e5dd9e0db44))
- **agents** — Route workflow authoring to the spec protocol ([8497207c2](https://github.com/supernovae-st/nika/commit/8497207c2734b1a7f98637a4283d7898875421cd))
- **arch** — Land v0.81 forward-compat seams + L0-L4 layer registry ([61d229547](https://github.com/supernovae-st/nika/commit/61d229547fc52000b14e8b19ed393a3769dd7a8f))
- **arch** — Land L0 brainstorm decisions + dep audit alignment ([e36dd8de7](https://github.com/supernovae-st/nika/commit/e36dd8de7527a34a7ef3d16f72a21a21dd5a600b))
- **arch** — L0/l0.5 swarm audit — revert q8, add q9-q10, fix incoherences ([5e810a94a](https://github.com/supernovae-st/nika/commit/5e810a94adbb377816500588eaebac4f87e3de97))
- **arch** — Blueprint 2036 final v0.x · 10-year nika horizon ([3a6e36869](https://github.com/supernovae-st/nika/commit/3a6e3686989e7ea7469e478eac065b19e8c2457b))
- **arch** — Blueprint 2036 v1.1 · per-crate detail + best-enemies sota ([efdf7c114](https://github.com/supernovae-st/nika/commit/efdf7c114452822a757a799b5cfb98275edf0c03))
- **arch** — Blueprint v1.2 · 11/10 amplifiers + ai-2027 guardian doctrine ([f9f4f6e1a](https://github.com/supernovae-st/nika/commit/f9f4f6e1a5cebff586651e4c555e30fa60a45f82))
- **arch** — Blueprint v1.2 fold-pass · 9→4 amplifier adrs · prose-only ([1fb3a5d63](https://github.com/supernovae-st/nika/commit/1fb3a5d636dcf61de0486b6f8e700abf4986f8a5))
- **arch** — Blueprint v1.5 · best-architects 2030 discipline ratchet ([3741eae91](https://github.com/supernovae-st/nika/commit/3741eae91a52379fc9aff764777a16e7f6293660))
- **architecture** — Reconcile layer model to 6-tier L0..L5 (P0-6A) ([0bc4df618](https://github.com/supernovae-st/nika/commit/0bc4df61880ee1de97e45c61aeb21e862b0e2d4f))
- **architecture** — Add FCI-NNN and INV-NNN numbered anchors ([9fdfea52b](https://github.com/supernovae-st/nika/commit/9fdfea52b12dd0389a712fc516540f6d58ab858d))
- **architecture** — Add constellation reconciliation 2026-04-17 report ([e7bef7e74](https://github.com/supernovae-st/nika/commit/e7bef7e74bb3eb96aeab082e4ed13925911c47c2))
- **architecture** — Review fixes · templatable carve-out + honest codegen claim + registry parity ([b5c9d15ae](https://github.com/supernovae-st/nika/commit/b5c9d15ae5821ac93ee288b3c067d0776783d64b))
- **architecture** — Gate-12 error-code contract + kernel split trigger status ([5959bd88a](https://github.com/supernovae-st/nika/commit/5959bd88af6f3268f0bf3c0c4a4015cb9c459ed6))
- **architecture** — Kernel 4-way split · census freeze + 4 sibling specs ([b837b3cc4](https://github.com/supernovae-st/nika/commit/b837b3cc430fa49da85065edc8fcd46badf6584b))
- **architecture** — Kernel split step 6 — registry + status + evidence-path cascade ([a1f065efa](https://github.com/supernovae-st/nika/commit/a1f065efa137053f5ed10703508c2ed927812488))
- **architecture** — R4 error-trait completeness audit table — b5 close ([a06662dca](https://github.com/supernovae-st/nika/commit/a06662dca01db8408538c65a0b34a43f70a97917))
- **architecture** — Blueprint-2036 catalog count derives from spec canon ([de174e317](https://github.com/supernovae-st/nika/commit/de174e317357177a093e80380af8a08346d5cda5))
- **canon** — Cascade 4-verb taxonomy · fetch is a tool not a verb ([efcc4df94](https://github.com/supernovae-st/nika/commit/efcc4df946c1aaba4d7eba3faebeddc23da10e7f))
- **canon** — Lock nika: v1 envelope · ADR-082 supersedes ADR-021 ([a7d0c656f](https://github.com/supernovae-st/nika/commit/a7d0c656f7abc60ed8e12ceff9c58353eb83b377))
- **canon** — Live docs state the current canon only — narrative purged ([31335c42c](https://github.com/supernovae-st/nika/commit/31335c42c7015a94131fd042fa6f098ea0cd2d34))
- **changelog** — Record swarm-3 batch i.b + wave 3a/4a/4b/4c session ([6f394aa23](https://github.com/supernovae-st/nika/commit/6f394aa23a20c1d80b4832bcc4f1b8b51429dfae))
- **changelog** — Fix stale MCP alias count 113 to 105 (grep-verified) ([ed443d081](https://github.com/supernovae-st/nika/commit/ed443d081050c0e79ecd6b0f6f3222f6d42dd60a))
- **changelog** — Document the v0.1-v0.28 public version history ([ea4af285c](https://github.com/supernovae-st/nika/commit/ea4af285c32e78343837eff205e2ac861083eba6))
- **claude** — Sync narrative with canonical block (905 tests, 32 providers) ([8241ab7ed](https://github.com/supernovae-st/nika/commit/8241ab7edcdd9550b6979fe79b27c56ae9cea6b8))
- **claude** — Refresh auto-state HEAD to ee74d97e0 · post-rename drift fix ([611ccdf7e](https://github.com/supernovae-st/nika/commit/611ccdf7ed08aaae303df310a8e8ae9fa697c410))
- **coherence** — Deep de-stale sweep — shipped names, 1+10, no hand-counts ([bacf5385c](https://github.com/supernovae-st/nika/commit/bacf5385c71319aea8c2255d87df535b3490c3b3))
- **contributing** — Add CONTRIBUTING.md with 12-gate workflow ([1f750c800](https://github.com/supernovae-st/nika/commit/1f750c8006cdd6bc2f90da889d1be4a0cafdc3da))
- **crate-spec** — Add nika-catalog-codegen Gate 1 spec ([41da7e565](https://github.com/supernovae-st/nika/commit/41da7e565745e53d7e74210d6cf95a4dbb3899e1))
- **crate-specs** — Nika-bm25 gate table update · 7/12 shipped ([c0d3a8f40](https://github.com/supernovae-st/nika/commit/c0d3a8f40f32eb9fc2f5d712e4af84bb0b9b606c))
- **crate-specs** — Nika-bm25 gate 5 mutation 96.9% kill ✅ ([636da4a21](https://github.com/supernovae-st/nika/commit/636da4a21ca9fc9a4a756c402d5c2cc632650aae))
- **crate-specs** — Reconcile fs/http/blob gate evidence to ground truth ([a5d214c00](https://github.com/supernovae-st/nika/commit/a5d214c00cde169c4510abed902174b084a5b4b0))
- **crate-specs** — Nika-policy (s8) — design locked, impl sequenced post-kernel-migration ([130019031](https://github.com/supernovae-st/nika/commit/130019031141beb5e27f3451842ed5bd9ba53562))
- **crate-specs** — Nika-providers (s8.5) — design proposal, kernel seam verified ([8b7529540](https://github.com/supernovae-st/nika/commit/8b752954060c5dab7f4a8895017018a49a2440db))
- **crate-specs** — Nika-browser (m2.5) — gate-1 spec, backend resolved ([e8afd8f1a](https://github.com/supernovae-st/nika/commit/e8afd8f1a2e681275aa8a7cb7780b35faf067d68))
- **crate-specs** — Nika-browser — b.2+b.3 shipped, headful-default clarified ([dd111a0e4](https://github.com/supernovae-st/nika/commit/dd111a0e41558675a8b717b9909c8cff64bf6b0f))
- **crate-specs** — Scaffold Connectome climb — 10 Gate-1 specs ([#113](https://github.com/supernovae-st/nika/issues/113)) ([618cdb2df](https://github.com/supernovae-st/nika/commit/618cdb2df2a367910c187f13076428e143cd40fa)) ([#113](https://github.com/supernovae-st/nika/pull/113))
- **crate-specs** — Nika-browser — guard-5 hardened §5b + gates B.2/B.3 shipped ([fdb42d3e4](https://github.com/supernovae-st/nika/commit/fdb42d3e40f35f10cb255cdd12fdfac382a149a0))
- **crate-specs** — Nika-infer-local — Gate-1 contract + candle loop design ([311e49870](https://github.com/supernovae-st/nika/commit/311e498700ca019a4a8c05f54833b346abff04c7))
- **crate-specs** — Flip 12 stale status rows to admitted + convention readme ([e611f612a](https://github.com/supernovae-st/nika/commit/e611f612a8a5d09378ea5913e49cec5aa55619fe))
- **crate-specs** — Nika-infer-local §5bis — the connection path (build-ready) ([7ab0ed1fa](https://github.com/supernovae-st/nika/commit/7ab0ed1faefc839350551660a6573520f1945cec))
- **crate-specs** — Nika-cli display contract + runnable render prototype ([0b714eee4](https://github.com/supernovae-st/nika/commit/0b714eee4a4bea1c52c95e012e4616312437cef6))
- **crates** — Readme for codegen + verify + schema + types crates ([3ba6de3b8](https://github.com/supernovae-st/nika/commit/3ba6de3b8d7d786061534fab78c61ad9acd64b86))
- **diamond** — Refresh auto-state · post pre-w3 stabilization ([37534abaf](https://github.com/supernovae-st/nika/commit/37534abaf58187b9f3bbe6cb1df6c9a8be09334a))
- **diamond** — Pre-w3 doc quality · 5 critical fixes ([40dde1110](https://github.com/supernovae-st/nika/commit/40dde1110afb3015fde46b96916469e8a16ee52b))
- **diamond** — Ship 4 per-crate readmes + code-of-conduct + security ([d0ef54445](https://github.com/supernovae-st/nika/commit/d0ef54445d8c1565f1b02529690a301712b9e0e1))
- **diamond** — Refresh auto-state + changelog · 2026-05-12 session arc ([41797d452](https://github.com/supernovae-st/nika/commit/41797d452b78aefbd418a1a0a4760ab0029f4289))
- **diamond** — Security lens audit · adr-071/072/073 + cross-link cohesion ([20c0f21eb](https://github.com/supernovae-st/nika/commit/20c0f21eb9ddda1ba355e7a5e73962d6c1f0f025))
- **diamond** — Contributing · cross-link + branch rename carry ([5f7510a32](https://github.com/supernovae-st/nika/commit/5f7510a32cf4b5b7a275921bc27306fd9dd33987))
- **diamond** — Connectome cluster count 9→10 satellites — rerank m13 ([144c5bca6](https://github.com/supernovae-st/nika/commit/144c5bca65530fc4ca176ff3a5d45b12aa75f555))
- **diamond** — Crate tree mirrors the layer registry — shipped reality ([492068e68](https://github.com/supernovae-st/nika/commit/492068e6846c580ab7b9895860b90388cdb605ff))
- **docs** — Rebuild readme with current v0.80 state + docs.nika.sh links ([460ef9851](https://github.com/supernovae-st/nika/commit/460ef9851603f7da832f1dc974c05a56298d5154))
- **docs** — Add wave 4e mintlify split entry to unreleased ([b2b4dceeb](https://github.com/supernovae-st/nika/commit/b2b4dceeb2cc0e2ad618c72242eb698cc4952752))
- **docs** — Refresh roadmap current-state adr/hygiene numbers + docs repo ([942b18aec](https://github.com/supernovae-st/nika/commit/942b18aec843df927d64a8cf2099879ecafd3978))
- **docs** — Rewrite diamond.md to current state ([3ec8c6456](https://github.com/supernovae-st/nika/commit/3ec8c645636b8238b03265d9e85d6f0367a83d1a))
- **docs** — Purge internal handoff + superseded docs from docs/ ([9ebaf05ca](https://github.com/supernovae-st/nika/commit/9ebaf05ca48221aea5de7f0ed674e9ca2c8855b1))
- **docs** — Nika 2040 intelligence-layer vision + llms.txt agent on-ramp ([3d283738f](https://github.com/supernovae-st/nika/commit/3d283738ff7a1cc6e8d467c4d7dc8afaf6a055c7))
- **docs** — Adr-090 evidence paths — list literals, not a brace glob ([51d50ffbe](https://github.com/supernovae-st/nika/commit/51d50ffbebed6fe46aeea2366bb2d3f367d3573b))
- **docs** — Reconcile FCI-016 (public fields require non_exhaustive) ([a8d5da279](https://github.com/supernovae-st/nika/commit/a8d5da279b19a5064c5894fb01b55f9d4a1fda92))
- **docs** — Mark error one-voice unification done (was TRANSITIONAL) ([5ff7bc488](https://github.com/supernovae-st/nika/commit/5ff7bc488b31100cc9b99e70fa3fa129ecdd7dab))
- **docs** — Add machine-readable GATE5-EXEMPT marker to nika-screen spec ([4cb4a9fbc](https://github.com/supernovae-st/nika/commit/4cb4a9fbc0fe7748f81d6aeadbeff55e2fe93d88))
- **docs** — Record nika-types Gate-5 measured result + exempt marker ([5540eda3d](https://github.com/supernovae-st/nika/commit/5540eda3ddd2a6dfbb81eac8345e56deccda95f8))
- **docs** — Bm25 spec — correct false "postings list" claim (P-4 perf) ([c1301e1a8](https://github.com/supernovae-st/nika/commit/c1301e1a83109712716d25b98bef416535ce6268))
- **docs** — Attest nika-catalog Gate-5 (measured 96.8% + exempt marker) ([ba8257021](https://github.com/supernovae-st/nika/commit/ba8257021261af375320b21aa8cae414b9be672e))
- **docs** — Nika-schema parser untrusted-input DoS gates (pre-admission) ([ce8106275](https://github.com/supernovae-st/nika/commit/ce810627568a0cfb09b40eabd57c935105accfae))
- **docs** — Document kernel I/O error-typing convention (FCI-023bis) ([587b58bfb](https://github.com/supernovae-st/nika/commit/587b58bfb4c0e2eedd7264236fe83839596807c3))
- **dx** — Add NEXT_SESSION orientation + /admit command + golden-commits ([5b222f795](https://github.com/supernovae-st/nika/commit/5b222f795909691fb9577f328a8a40fc53ddba1a))
- **dx** — Sync .claude/CLAUDE.md current-state to 2026-04-15 ([5c877aad6](https://github.com/supernovae-st/nika/commit/5c877aad6a71127be7e6f9a3fb2f29b587454f0a))
- **dx** — Sync diamond-progress + roadmap + claude current-state ([a1076cc2a](https://github.com/supernovae-st/nika/commit/a1076cc2a80b1a17ff9928c0a5c71369fef97b49))
- **dx** — Sync .claude/CLAUDE.md current-state — S2b+3 done ([12e88c610](https://github.com/supernovae-st/nika/commit/12e88c610a0e10862dbb7e0f7cc2600d24e62c7a))
- **dx** — Post-hygiene spot-fix — mintlify + diamond + readme + specs ([47889add5](https://github.com/supernovae-st/nika/commit/47889add54a00831723a0c78dbcd16590045fb0c))
- **engine** — Refresh status block head to ba2f65236 post 42-lock ([07525dbc3](https://github.com/supernovae-st/nika/commit/07525dbc3bf4ebd47f02f668864bbf5e20298871))
- **engine** — Sync status block — L0 6, admitted 9/42 post nika-event ([ab9302037](https://github.com/supernovae-st/nika/commit/ab930203722cc19f9a83052115083df567245663))
- **engine** — Sync roadmap status block — L0 6, admitted 9/42 ([b6ec46dde](https://github.com/supernovae-st/nika/commit/b6ec46ddef8b2048b1b32b8fee0b640ef6d00f80))
- **engine** — Sync status block · admitted 10 of 42 · l1 2 post nika-clock ([386024d6b](https://github.com/supernovae-st/nika/commit/386024d6b5ad58e152f48ccad35ed936411cbfef))
- **fci** — De-number the provider rationale (counts rot in prose) ([a6c87bf81](https://github.com/supernovae-st/nika/commit/a6c87bf8129ba276ae518803f950b2a689706de6))
- **gate12** — Forward-compat invariants — connectome names + verb range truth ([d21902cf0](https://github.com/supernovae-st/nika/commit/d21902cf0e5b7874c5491c6dd4c9078784293197))
- **hygiene** — Refresh vector count 15/20 → 31 across 5 files ([a08077b62](https://github.com/supernovae-st/nika/commit/a08077b625555e17457716336ee2271aad147323))
- **invariants** — Extend §9 with Wave 4A/4B reservations (FCI-035) ([2027856be](https://github.com/supernovae-st/nika/commit/2027856beb0fa20bfabb227eba362fe888956fed))
- **invariants** — Correct §See-also ADR-021..028 titles + status ([3f9fdb208](https://github.com/supernovae-st/nika/commit/3f9fdb20836fedeb05687a2f9dfe7bc7bf119370))
- **kernel** — Register computer-use error ranges 1000-1499 in the hub ([aa51f00da](https://github.com/supernovae-st/nika/commit/aa51f00da36c13ab91e63e9a283c7bba4cf094d1))
- **kernel** — Browser trait — typed BrowserError refs, drop stale ErrorKind ([33ab17247](https://github.com/supernovae-st/nika/commit/33ab17247735761f25e4e0602b67e6a3a3b2e3b3))
- **mintlify** — Add crate inventory reference page ([58d009df8](https://github.com/supernovae-st/nika/commit/58d009df86be370b8277c36aa9939919568bfa56))
- **mintlify** — Dark theme diagrams + status page + live numbers ([39b53e331](https://github.com/supernovae-st/nika/commit/39b53e331c297a2448f5b6e8939504127a7fd5c3))
- **mintlify** — Sync crates.mdx numbers with ground truth ([6135a5f0c](https://github.com/supernovae-st/nika/commit/6135a5f0c9e4f8a77e026a155086f36a60fcdc9a))
- **nika** — Adr-083 cross-platform doctrine for l1 computer-use ([1e94191ea](https://github.com/supernovae-st/nika/commit/1e94191ea62564bb91cccceeb8a2400837fbc312))
- **nika** — Refresh status block + active-arc narrative to 0b558f7f8 ([847529fec](https://github.com/supernovae-st/nika/commit/847529fec849c56fb95022a391182fa5ebbdc027))
- **nika-browser** — Module doc — guard-5 hardened contract (node_ref · peek/consume) ([85e282fce](https://github.com/supernovae-st/nika/commit/85e282fceb1021c04a8db6cc4525d5459b7466ec))
- **nika-browser** — Gate-5 budget 4→5 post-occlusion — honest re-measure ([4cc631e30](https://github.com/supernovae-st/nika/commit/4cc631e30e6efc15b6fd812e55bbe4e355123638))
- **nika-builtin** — Retire GATE5-EXEMPT budget → clean FLOOR 91.3% ([a67baa391](https://github.com/supernovae-st/nika/commit/a67baa3919d109b092de125942d5a5b49a4de2be))
- **nika-catalog** — Adr-008 addendum — materialize defaults source of truth ([42fb140e4](https://github.com/supernovae-st/nika/commit/42fb140e4ff5bc4996177e84b236bed98f91f770))
- **nika-catalog** — Renumber wire decision N3 to N4 · de-collide ([86e2ebd36](https://github.com/supernovae-st/nika/commit/86e2ebd360e4df28d3ece8462d49e5a62a52927f))
- **nika-cel** — Re-measure Gate 5 post the Gate-11 fixes (0 missed) ([29b1ec288](https://github.com/supernovae-st/nika/commit/29b1ec2884dc0bbc51c17eeff93cf562d3578bc7))
- **nika-error** — Refresh NIKA-464 explain — engine now enforces schema ([9751f8843](https://github.com/supernovae-st/nika/commit/9751f8843c57ec3028efa87945d95c37c06190d0))
- **nika-kernel-core** — Fs cancel-safety teaches detach-not-abort ([4fd7cd3cc](https://github.com/supernovae-st/nika/commit/4fd7cd3cc60e97242408e4213346c2561e4d2c70))
- **nika-providers** — Fix broken intra-doc links (Gate 8) ([3a03558d5](https://github.com/supernovae-st/nika/commit/3a03558d5f7276a70606ed30e1db6c858bd578fd))
- **nika-schema** — Cascade csv_to_json → convert in 2 doc-comments ([82c5ca980](https://github.com/supernovae-st/nika/commit/82c5ca980af0f9a647fb525a8d947eb7b65d9199))
- **nika-schema** — Nika check section — shipped surface, honest gaps, next steps ([f8e350070](https://github.com/supernovae-st/nika/commit/f8e350070bf4a2f0c406b8acd31982547d805380))
- **nika-schema** — Backtick argv[0] — rustdoc read it as an intra-doc link ([d9b8f1027](https://github.com/supernovae-st/nika/commit/d9b8f1027ae41d09e984e6b99b505498ab8e9cc4))
- **nika-schema** — Spec audit row — span axis + research-conformance suite ([8ad72b33b](https://github.com/supernovae-st/nika/commit/8ad72b33b5a510c3200c940e0226a8d22b43dcb4))
- **nika-schema** — Infer/agent prompt-secret sink is now canonical (F-03) ([510c2cde0](https://github.com/supernovae-st/nika/commit/510c2cde085be0046f0a72162b37d741bd40140e))
- **nika-schema** — Author the 12-gate admission ledger (11/12 green) ([2777d83fa](https://github.com/supernovae-st/nika/commit/2777d83fa28d2b70f2068a208f94cb6480207feb))
- **nika-schema** — Record the Gate-5 floor v2 + survivor rounds 1-2 ([aa096b0c0](https://github.com/supernovae-st/nika/commit/aa096b0c04d5a687a8041e801f68135b6fc7bc09))
- **nika-types** — Doctests import nika_types, not nika_error ([e41c33708](https://github.com/supernovae-st/nika/commit/e41c33708a5cfad2cec74fea15e193ad92e3418d))
- **nika-verb-agent** — Gate 1 spec — s12 the 4th verb, impl deferred ([6f6d63cb5](https://github.com/supernovae-st/nika/commit/6f6d63cb538ac07a0f87fda58a2f4d628730d8be))
- **nika-verb-agent** — Record the ToolDefinitionProvider blocker ([7709b6e5f](https://github.com/supernovae-st/nika/commit/7709b6e5f8e9b187685c0205407b4094b2b57f3f))
- **nika-verb-agent** — Close the spec↔impl coherence debt the agent arc introduced ([3f15d4e54](https://github.com/supernovae-st/nika/commit/3f15d4e5498625dd5dfbedcf3cd67b1d0b710ac2))
- **nika-verb-exec** — Gate 1 spec — s10 second L2 verb crate ([a9df92fee](https://github.com/supernovae-st/nika/commit/a9df92fee59b17fe8f50fa1187082c0095ab6baa))
- **nika-verb-exec** — Note NIKA-442 has no spec counterpart ([0c8504e16](https://github.com/supernovae-st/nika/commit/0c8504e162a9f8045f250a4f6b535594e12ef877))
- **nika-verb-infer** — Gate 1 spec — s9 first L2 verb crate ([5943503f6](https://github.com/supernovae-st/nika/commit/5943503f6d34afa288009bf8b8411642bc9e8078))
- **nika-verb-invoke** — Gate 1 spec — s11 third L2 verb crate ([1d83e12a4](https://github.com/supernovae-st/nika/commit/1d83e12a4d9af9bedc6413d015d332ac71c31219))
- **observability** — Purge ObservabilitySink ghost refs (Q12 rev.3) ([8dc307a98](https://github.com/supernovae-st/nika/commit/8dc307a98cb4e7f25cd4273eb426f44163fb3b2c))
- **plan** — Nika run shipped — mark B1-B5 done + the CEL follow-ups ([3211457ee](https://github.com/supernovae-st/nika/commit/3211457eefe54fbc1ee5cf80c58689d61340358b))
- **plans** — Record swarm-3 audit implementation plan ([6d9c92f85](https://github.com/supernovae-st/nika/commit/6d9c92f856ab1db12365b857618c4c840b081da8))
- **readme** — Mermaid architecture + timeline, honest status, fix stale badges ([24bd193d0](https://github.com/supernovae-st/nika/commit/24bd193d0a88b0cb5d33ecb9e94fa924cbddf0b2))
- **readme** — Cross-link the Nika language spec (engine ↔ spec coherence) ([c5651a12a](https://github.com/supernovae-st/nika/commit/c5651a12ae6b54fdfd0e564967321036a3c2636b))
- **readme** — Intent-as-code framing + connectome codename ([51b4c27b9](https://github.com/supernovae-st/nika/commit/51b4c27b9cd6565e6dc29900297f42d6e71a949b))
- **readme** — Clean up for NLnet readiness · de-confuse legacy version ([b162acf1c](https://github.com/supernovae-st/nika/commit/b162acf1c9751b0eb656ca6b7c5df932c7f3add7))
- **readme** — Drop legacy nika-diamond branch note from status table ([d9121a3a5](https://github.com/supernovae-st/nika/commit/d9121a3a57aa7d15f412d5a0e26fabafba054aa5))
- **roadmap** — Refresh canonical status block to HEAD 6d9c92f85 ([ee6b60a77](https://github.com/supernovae-st/nika/commit/ee6b60a772b9cd55c9b215732636e69cbaba2958))
- **roadmap** — Fix ADR count (021-027 → 021-028) + flag Wave 4A/4B stubs ([ac961b72b](https://github.com/supernovae-st/nika/commit/ac961b72b729650c64d0b6eac5d39078b17a1b17))
- **roadmap** — Add bottom-up progression banner per ADR-037 ([c46cdcc35](https://github.com/supernovae-st/nika/commit/c46cdcc358185497d24c21d992fe02c2edfc0e26))
- **roadmap** — Restructure per ADR-037 bottom-up progression ([cb157fa0e](https://github.com/supernovae-st/nika/commit/cb157fa0efafd026c7475782239aad35b3e08fa4))
- **roadmap** — Correct "spec curates 42→26" + flag pre-d-n6 builtin lists ([e5f262c56](https://github.com/supernovae-st/nika/commit/e5f262c563f0cd7eeb298e3fa9620132d523ecf8))
- **roadmap** — Fix line 96 spec-contract "42 builtins" → 26 ([c06192270](https://github.com/supernovae-st/nika/commit/c06192270a88b7eade8aaeb49b6f6d8c113d08e4))
- **roadmap** — 14 providers · openrouter promotion cascade ([64144a0fb](https://github.com/supernovae-st/nika/commit/64144a0fb202c3be215da3c985c87560e5b9f577))
- **roadmap** — Connectome cluster 1+10 — memory section de-staled to ratified canon ([272c942a7](https://github.com/supernovae-st/nika/commit/272c942a7929fe0576ab8eea6ba160de137a426c))
- **roadmap** — Providers rows reflect the shipped shape — rig not carried ([5301dd873](https://github.com/supernovae-st/nika/commit/5301dd8733ea3b7d70e380caf11c26e2bf64ee3e))
- **roadmap** — Refresh the auto-block — 32 crates, 2325 tests (vector 23) ([60998274f](https://github.com/supernovae-st/nika/commit/60998274f142d508c1d988fb753d86d4941f43bb))
- **roadmap** — Sync status block to the nika-cli admission (38/42) ([b85b2722b](https://github.com/supernovae-st/nika/commit/b85b2722b883bd66a4bfe46bac7279ef231669b6))
- **rules** — Collapse-vs-publish status precision — proposal not locked ([b9b4b8b91](https://github.com/supernovae-st/nika/commit/b9b4b8b91625d97fa2a4ef1505b4946129454e45))
- **screen** — Add crate-spec + sync status block · hygiene v6+v23 green ([fe2be76b0](https://github.com/supernovae-st/nika/commit/fe2be76b00e875e97e450efbeb9f35f3b415493a))
- **skills** — Adopt gitnexus MCP integration guide ([2343e89f1](https://github.com/supernovae-st/nika/commit/2343e89f146b4dbacf2f5c29a21551d090c1de44))
- **spec** — Attest nika-schema Gate-5 budget + when-gate DoS mitigation ([c56eddc0f](https://github.com/supernovae-st/nika/commit/c56eddc0f9e3a84d356debb092bd7efb1c53ae5b))
- **spec-sync** — Sweep engine docs to the curated-22 + closed namespaces ([#118](https://github.com/supernovae-st/nika/issues/118)) ([d0f76c632](https://github.com/supernovae-st/nika/commit/d0f76c632ea77dde517d26b95d590e0596401d73)) ([#118](https://github.com/supernovae-st/nika/pull/118))
- **state** — Refresh auto-block + ladder prose — s7 admitted, next s8 ([7c258d83f](https://github.com/supernovae-st/nika/commit/7c258d83f7cdc44c4bed5c740074246538948d61))
- **state** — Re-sync canonical blocks from main — vector 23 green ([1ca15ac3e](https://github.com/supernovae-st/nika/commit/1ca15ac3eee320826a1766ea883a9a6213f08b28))
- **state** — Re-sync canonical blocks post-merge — vector 23 green ([bfc654c4d](https://github.com/supernovae-st/nika/commit/bfc654c4d8ddf3f5b52e684b78ec88cbe7b17579))
- **state** — Narrative — m2.4 b.2+b.3 shipped, dyn-variant canon uniform ([0a6a227af](https://github.com/supernovae-st/nika/commit/0a6a227af53ec6c47c1dc35b312441ed6dc9e98d))
- **state** — Re-sync auto-block post s8.5 — L1.5 layer row added ([c1d86f02f](https://github.com/supernovae-st/nika/commit/c1d86f02fcb46ff32fc5fc3b1aa5851999e4b81c))
- **state** — Post-rebase block re-sync — 24/42 admitted, 1459 tests ([95bce169a](https://github.com/supernovae-st/nika/commit/95bce169a952bc36869516d65d7e0e7158be3587))
- **state** — Re-splice canonical blocks — vector 23 parity ([7a51e4db7](https://github.com/supernovae-st/nika/commit/7a51e4db78a0b873c81e64b740781f499b8930c3))
- **status** — Refresh status block HEAD 6d9c92f85 → 9ebaf05ca ([393fdefa8](https://github.com/supernovae-st/nika/commit/393fdefa8f89e174cbb8ea608e699b5589414521))
- **status** — Refresh canonical block · m1 kernel sealed · 1110 tests ([98f5a61b7](https://github.com/supernovae-st/nika/commit/98f5a61b7233f9ad7e45387f8f1a86833c42d39e))
- **status** — Refresh canonical block — HEAD b5a528e84 · 1267 lib tests ([d3693506b](https://github.com/supernovae-st/nika/commit/d3693506bfd2f4b89a90148ba741d60a92cc056c))
- **status** — Correct leaked branch name in canonical block — main not feat ([20375a62e](https://github.com/supernovae-st/nika/commit/20375a62e481bd9d9998f481de0c6ede9f98d3f3))
- **status** — Refresh auto-generated block — nika-runtime admitted ([a55b0f11e](https://github.com/supernovae-st/nika/commit/a55b0f11edefa05e737c0165e90e46da69282acd))
- **status** — Refresh the auto-block — 32 crates admitted, 2325 tests ([dc34ce450](https://github.com/supernovae-st/nika/commit/dc34ce4509a52f7c4b99d58e082fdc89c3019fe8))
- **status** — Refresh the status-block HEAD to the audit-fix tip ([4d94a7179](https://github.com/supernovae-st/nika/commit/4d94a717942308b339da6c8b81846e0f4a8b79e2))
- **workspace** — Post-wave-3 coherence review — align all logs + arch docs ([e5c17e781](https://github.com/supernovae-st/nika/commit/e5c17e781d0c69f074bebffddcd4f97e6413b5e5))
- Scaffold docs.nika.sh via Mintlify ([90ce455b0](https://github.com/supernovae-st/nika/commit/90ce455b039e3965900290df67c628665d114563))
- Ultrathink alignment — zero feature lost, philosophy clear ([8a2ef99fa](https://github.com/supernovae-st/nika/commit/8a2ef99fad3bf8e20b20de59cec52e682dba00b4))
- Update CHANGELOG + ROADMAP post Phase D Session 1 ([883112cdc](https://github.com/supernovae-st/nika/commit/883112cdcdb286c690c3c36e5601ebdf397753a7))
- Align DX + rules + public docs post Phase D Session 1 ([1a29bd32f](https://github.com/supernovae-st/nika/commit/1a29bd32fc9e6a5ff665a2781f7b32c3bd47be59))
- Ecosystem bible + GitNexus safe-install protocol ([9495d1a07](https://github.com/supernovae-st/nika/commit/9495d1a07a982ae4ce816467bc58a5468bcceb59))
- Align DX + CHANGELOG + ROADMAP post Phase D Session 2a ([133ffa0ff](https://github.com/supernovae-st/nika/commit/133ffa0ff8d5094e778334f8f84eb0ea21deddee))
- Deep DX audit — fix 6 stale P0 findings ([b708edf58](https://github.com/supernovae-st/nika/commit/b708edf58c3ef8535dfa0dce082b9f79b99bb0a5))
- Update ROADMAP + CHANGELOG for Session 4A stabilization ([c4ae1ab5e](https://github.com/supernovae-st/nika/commit/c4ae1ab5e9c010c9176ec19a97dbd7388d26df2f))
- Update ROADMAP + CHANGELOG for Session 4B data enrichment ([d6d30b810](https://github.com/supernovae-st/nika/commit/d6d30b8100b9e6113a035d7a13e3d0834550f974))
- Rewrite readme from scratch · SOTA · destination not journey ([e2d6fbf16](https://github.com/supernovae-st/nika/commit/e2d6fbf1633b2287c8587f34c8c4d6ce313fa3e9))
- Pillar-1 de-hardcode — agents.md + roadmap totals + claude narrative ([7bfceee91](https://github.com/supernovae-st/nika/commit/7bfceee913cdbe376ffe0e8c8273320a526c9ac5))
- Annotate the branch rename in 7 live mentions of nika-diamond ([945b9ef8c](https://github.com/supernovae-st/nika/commit/945b9ef8cfca7a0f979ec6b033df587b755fd82a))
- One-voice release ladder + canonical mcp tool ref ([dc9227445](https://github.com/supernovae-st/nika/commit/dc9227445f1ed2be3120e62c9ecdaa1f67a00b0e))
- CITATIONS.md — credit every work the engine stands on ([c915d592e](https://github.com/supernovae-st/nika/commit/c915d592e65549660d25067750d68c6eae107f39))
- CITATIONS — the Lv & Zhai row reflects BM25+ activation ([612326942](https://github.com/supernovae-st/nika/commit/6123269420f413354de3c38ee49452e061d26749))
- Post-runtime truth sweep — counts · claims · the error census ([9dfc5fd7a](https://github.com/supernovae-st/nika/commit/9dfc5fd7a2eb9c8ee4944f30c8fe0d8d035ddeab))
- Fix versioning residuals + restore ADR-002 status + sync mirror ([060998f2e](https://github.com/supernovae-st/nika/commit/060998f2ef31865ae963cb8cc61cb54d8bed7b15))

### 🧪 Tests
- **catalog** — Kill 14 mutation survivors (CapPatchBuilder + suggest_in) ([18f529271](https://github.com/supernovae-st/nika/commit/18f529271651f7393ad3c8a066928708d95a230e))
- **error** — Proptest registry uniqueness + memory cross-mapping · diamond w2.3 ([f13847f63](https://github.com/supernovae-st/nika/commit/f13847f635ed781a20333d3a2ea86a4b9bf52f01))
- **event** — Pin EventKind wire slug — serde and as_str() must agree ([636873ebe](https://github.com/supernovae-st/nika/commit/636873ebe26983600f22d196ede48672055be8d5))
- **hygiene** — Add batch-h-plus red-team harness scaffold ([ebfa16b44](https://github.com/supernovae-st/nika/commit/ebfa16b44bae3b5ff08aaec5454e0234612dd9b1))
- **nika-a11y** — Real-walk smoke skips on no-focus, never false-fails ([f88ef62dc](https://github.com/supernovae-st/nika/commit/f88ef62dcddfda151ff719992dbc3a1dc1f8861e))
- **nika-bm25** — Gate 2 red · manning iir ch.11 fixture + tdd tests ([82b70662f](https://github.com/supernovae-st/nika/commit/82b70662fa0718b403d93ffef46c4eb6b07d255e))
- **nika-bm25** — Gate 5 mutation killers · ranking parity tests ([23d648572](https://github.com/supernovae-st/nika/commit/23d648572c61a06c2f019badeb8a375c495d70fc))
- **nika-bm25** — Gate 5 golden values · pin exact scores within 1e-9 ([be751ea63](https://github.com/supernovae-st/nika/commit/be751ea635318b22cd99d6a4cc7fbdbf0d63de98))
- **nika-bm25** — Ultrathink improvements · okapibm25 parity + 10k bench + sourced 2030 ratchet ([bedb92929](https://github.com/supernovae-st/nika/commit/bedb92929f6510ef2d6ae8e47f8722b752ccfb93))
- **nika-bm25** — Property-test the BM25 invariants over the input space ([04c62417d](https://github.com/supernovae-st/nika/commit/04c62417dbce674644f7f34646b8a53a2c5c03e2))
- **nika-browser** — Pin backend_ref + bbox guard exact — 6 mutants killed ([16c4d8631](https://github.com/supernovae-st/nika/commit/16c4d8631df6bb81e7d77b34603aff4d19879991))
- **nika-browser** — Kill consume + epoch-timestamp mutants ([02f1502ac](https://github.com/supernovae-st/nika/commit/02f1502ac03c672d86d05953307dcb3f7d6c53d5))
- **nika-builtin** — Gate-6 property tests — the seed's promise kept ([b337e4372](https://github.com/supernovae-st/nika/commit/b337e437203572c1073edf2dd0b376dec5268659))
- **nika-builtin** — The polynomial proof is completion, not wall-clock ([a43a29ec1](https://github.com/supernovae-st/nika/commit/a43a29ec19f1d6b51589ca630c38c4813a81b395))
- **nika-builtin** — Decoder padding-gate pin + the disjoint-bits note ([5968ed3cf](https://github.com/supernovae-st/nika/commit/5968ed3cf66fb455c897dc8cf6d1e8bcdfbd7fed))
- **nika-builtin** — Harden Gate-5 surfaces + 12-gate readiness table ([59984a287](https://github.com/supernovae-st/nika/commit/59984a287d6cd89e6c3360d004157a0466def45c))
- **nika-catalog** — Add pricing proptest invariants (1000 cases) ([7f9625cfa](https://github.com/supernovae-st/nika/commit/7f9625cfa1da82f9fc1be0b96bae4681768cebbb))
- **nika-catalog** — Add capabilities proptest invariants (10k cases) ([0fba30556](https://github.com/supernovae-st/nika/commit/0fba305567e6fcfe7b3b33cb474a7500a15498b7))
- **nika-catalog** — Extend merge_with + estimate_cost regression tests ([9da58d7ac](https://github.com/supernovae-st/nika/commit/9da58d7ac96ce788000e6dc87f3f31fc51e3ffbb))
- **nika-catalog-codegen** — Kill 43 mutation survivors — 87% → 98.9% ([f0b5367e3](https://github.com/supernovae-st/nika/commit/f0b5367e3e86b9162b080f2f182065eb5f8bdab2))
- **nika-cli** — Kill the display-fold mutation survivors + deny wrapper ([5bf733032](https://github.com/supernovae-st/nika/commit/5bf733032264590c8451f407e4df67cda4ca0c3e))
- **nika-cli** — Pin per-task cost attribution — graph projector 100% viable-kill ([ac1d598dc](https://github.com/supernovae-st/nika/commit/ac1d598dc5e42d45147528e8183f0c083395b96b))
- **nika-cli** — Rehearse the agent loop over the real builtin dispatcher ([2c7e60f78](https://github.com/supernovae-st/nika/commit/2c7e60f78383deb3b250cd5a5bf1a9cd65f3b819))
- **nika-cli** — Wave-2 agent rehearsal — repair · batch · security · budget · schema ([0e712ce80](https://github.com/supernovae-st/nika/commit/0e712ce80dff1b972395ac2f8dba12cb6511e078))
- **nika-cli** — Golden frames for the two new §3.1 states + doc truth ([7a8a0b694](https://github.com/supernovae-st/nika/commit/7a8a0b6948282a3b4534900e1b9ba40c7f9bd14c))
- **nika-cli** — Wave-3 rehearsal — binary round-trip + tz through the real chain ([7f41b7d90](https://github.com/supernovae-st/nika/commit/7f41b7d90a18db1013d4d4981c750d42a7e5adad))
- **nika-cli** — E2e offers reflect the 23rd builtin (nika:compose) ([638f51b5b](https://github.com/supernovae-st/nika/commit/638f51b5b463263c3bf076549af760983ea131ec))
- **nika-cli** — E2e failure card expects the spec code NIKA-EXEC-001 ([0b558f7f8](https://github.com/supernovae-st/nika/commit/0b558f7f8139bcbb4f96ba4b54decea702ad08f7))
- **nika-cli** — Un-stale two pre-existing verbs_static failures ([560a483b9](https://github.com/supernovae-st/nika/commit/560a483b943425084dd965111abf257271adc6ce))
- **nika-cli** — Pin the run verb's locked exit codes (0/1/2) ([3f533bbb7](https://github.com/supernovae-st/nika/commit/3f533bbb7eb732f4c82002f212db5bf2a1b06bc9))
- **nika-cli** — Ignore the env-dependent examples-run smoke ([e0f2be722](https://github.com/supernovae-st/nika/commit/e0f2be72249fc4e95abd031c88337df7a9035d50))
- **nika-cli** — Harden render surface to Gate-5 mutation 91% ([e0f0cfa4e](https://github.com/supernovae-st/nika/commit/e0f0cfa4e6991afc429df345496dc5b9c89ad595))
- **nika-cli** — Add the Gate-6 fold property battery ([c43a8d0cd](https://github.com/supernovae-st/nika/commit/c43a8d0cdc2401e42606f8766d9af37b52cd525c))
- **nika-error** — Proptest lattice/identity laws + sealed.rs doc truthing ([2e61823c3](https://github.com/supernovae-st/nika/commit/2e61823c3895fd0124190d1755a86c7620097fa6))
- **nika-error** — Commit proptest regression seed for codes ([73244e50f](https://github.com/supernovae-st/nika/commit/73244e50f64106dc15e251e1c5328dca3b62b301))
- **nika-event** — Defuse the stale integration landmine — contract suite synced to 17 ([856964f10](https://github.com/supernovae-st/nika/commit/856964f10531f6ed4d183104db90833f3c35cc67))
- **nika-extract** — Real-socket fetch rehearsal + mutation killers · finalize deps ([b743babce](https://github.com/supernovae-st/nika/commit/b743babce61b49a239208c60eca554938cb03da2))
- **nika-extract** — Mutation ladder 83% → 100% — every viable mutant dies ([24309511e](https://github.com/supernovae-st/nika/commit/24309511e78ef04ebd6cd8a762136f38c7119adb))
- **nika-extract** — Harden the Gate-6 totality proptest ([be1edb25e](https://github.com/supernovae-st/nika/commit/be1edb25e20718212329970c220dac44c6795be7))
- **nika-extract** — Kill 73 mutation survivors to Gate-5 93% ([361f755d7](https://github.com/supernovae-st/nika/commit/361f755d74108aa43b0fe48fe8017f2ff5a5789e))
- **nika-http** — E2e over real loopback sockets — redirect · cred-strip · caps · timeout · stream ([5fe95b405](https://github.com/supernovae-st/nika/commit/5fe95b4051b3173fd4ccad263368d809b018471a))
- **nika-http** — Fix stale tls smoke test — TEST-NET is SSRF-blocked ([e14688c0f](https://github.com/supernovae-st/nika/commit/e14688c0f2e0cb4e4d8bbb16395daf857ae98388))
- **nika-http,nika-schema** — Pin check↔runtime host-extraction parity ([89396a1e0](https://github.com/supernovae-st/nika/commit/89396a1e0d066236373983567f47d281f0aabe33))
- **nika-kernel** — Add MemoryId deserialize error path tests ([a325cd564](https://github.com/supernovae-st/nika/commit/a325cd564d2632472baf3ab65cc024887b252c0c))
- **nika-providers** — Json-mode structured-output parity ([2ce6912cb](https://github.com/supernovae-st/nika/commit/2ce6912cb10adc52f5a4adc4fe20d206d9120a51))
- **nika-providers** — Cross-provider tool-call parse parity ([a07ff35f5](https://github.com/supernovae-st/nika/commit/a07ff35f5a3b0dfb83a47ca43a1ff80c1cb5011c))
- **nika-runtime** — The theorems extend over the buffered agent telemetry ([6fee61350](https://github.com/supernovae-st/nika/commit/6fee613506b87f35d811a7fbc64f46191c049c9f))
- **nika-runtime** — Close the agent-adapter mutation gaps — attempt stamps, compose, streak ([65ebb0153](https://github.com/supernovae-st/nika/commit/65ebb015352c488757e7d7dc355e3048425cc1d4))
- **nika-runtime** — Add required path to floor wide-fan write fixture ([8f6fd84ce](https://github.com/supernovae-st/nika/commit/8f6fd84cee0882107cc925ec1e407e1ade5051cc))
- **nika-runtime** — Lock for_each on_error:skip positional-null path ([81ca81976](https://github.com/supernovae-st/nika/commit/81ca81976527ec7b2335957514314edadacc94d0))
- **nika-schema** — Pin canonical envelope contract (RED · admission-prep) ([991700643](https://github.com/supernovae-st/nika/commit/991700643630ca1b050fb65be819e2f63fae5768))
- **nika-schema** — Kill the 5 mutation survivors — 100% on the new modules ([927f23b3b](https://github.com/supernovae-st/nika/commit/927f23b3bd6a74b390a6275a342cc6ab0229c8d0))
- **nika-schema** — Snapshot-pin both glyph themes + themed chrome typography ([453d3fa5b](https://github.com/supernovae-st/nika/commit/453d3fa5bf51a338ce24e2317e2e82f1dc2057a9))
- **nika-schema** — Gate-6 properties on the reference fold ([1fa313ffb](https://github.com/supernovae-st/nika/commit/1fa313ffb7a51acb20184c73d51ec7edcd298c84))
- **nika-schema** — Fetch fixtures gain their url — the new net caught them ([c57ea8b8d](https://github.com/supernovae-st/nika/commit/c57ea8b8d56132fa5402a5686e30350fa2bad84c))
- **nika-schema** — Deep conformance verdicts against the full check() surface ([afa8c10b4](https://github.com/supernovae-st/nika/commit/afa8c10b40f823f57e10915319c9a210f532c729))
- **nika-schema** — Gate-7 criterion benchmarks for the parse hot path ([7853d3c03](https://github.com/supernovae-st/nika/commit/7853d3c034ebb7a9f071174c1b4a501ddf71adb0))
- **nika-schema** — Cover gate-5 survivor clusters across four files ([8cf1225eb](https://github.com/supernovae-st/nika/commit/8cf1225ebadd5728003ee1ecd4f7a72dd45ec7db))
- **nika-schema** — Cover the remaining preference_rules lint survivors ([42667ef19](https://github.com/supernovae-st/nika/commit/42667ef195d4d5b8d206a0641156ddd26dbeab1a))
- **nika-schema** — Close the gate-5 long-tail survivors across ten files ([2f8817f17](https://github.com/supernovae-st/nika/commit/2f8817f17a7f0211c6520f0fd704bb9d5352d4fc))
- **nika-schema** — Kill the three lexer survivors (round 4) ([6134aca00](https://github.com/supernovae-st/nika/commit/6134aca00ac1b365f8351ddfb731f02deb1dfded))
- **nika-schema** — Cover parser/mod source-bounds + check/mod codes (round 5) ([6c9a078ac](https://github.com/supernovae-st/nika/commit/6c9a078acc0935f1860c7f21716178183959c484))
- **nika-schema** — Cover read_dag cap + pinch boundaries (round 6) ([c8350ee40](https://github.com/supernovae-st/nika/commit/c8350ee40588339d7d54b2da9b39c455b801b99b))
- **nika-schema** — Pin default-gate runnable path in reach ([27e0f3ddf](https://github.com/supernovae-st/nika/commit/27e0f3ddf5429abfbaca4e3df6f442ceb9b2e4b1))
- **nika-schema** — Kill expression-parser mutation gaps (round 7) ([7bba982a0](https://github.com/supernovae-st/nika/commit/7bba982a08789ecd1e205cd93e91de1708923800))
- **nika-schema** — Add parser + check benchmark (Gate 7) ([bcc1c8f32](https://github.com/supernovae-st/nika/commit/bcc1c8f329cd4696e6ebe55abc4906821b34009a))
- **nika-types** — Proptest audit for TrustLevel lattice + ID serde roundtrip ([73518494c](https://github.com/supernovae-st/nika/commit/73518494cd1c47b25ae7add3d487148a85812126))
- **nika-types** — Loom interleaving tests for CancelCtx (inv-029, batch ii ε.2) ([3a54b80d4](https://github.com/supernovae-st/nika/commit/3a54b80d46040ec54572f5e744526a7206cb82f4))
- **nika-types** — Kill from_unix_ms + unix_us surviving mutants ([ec479108d](https://github.com/supernovae-st/nika/commit/ec479108d933705e8130258f80ce9a78b34efd03))
- **nika-types** — Kill 3 baggage.rs mutation survivors (Gate-5 gap) ([ea66e301d](https://github.com/supernovae-st/nika/commit/ea66e301d607417d7059f26e5a4c9674dad6de85))
- **nika-types** — Close Gate-5 — kill 24 mutation survivors across 5 files ([0b03a0569](https://github.com/supernovae-st/nika/commit/0b03a0569652c4b58ab21bf15ad976f07fa7fda5))
- **nika-types** — Add tab/CRLF host-extraction bypass vectors ([3cd1a346d](https://github.com/supernovae-st/nika/commit/3cd1a346da8ff02f6f2272660db60a6a8765e028))
- **nika-verb-exec** — Pin stderr-tail boundary walk; note equivalent mutant ([7b0d7477f](https://github.com/supernovae-st/nika/commit/7b0d7477f0f1ffd0cb9a5cd62b6b4491cf464dc1))
- **nika-verb-infer** — Gate 10 parity — request shaping pinned vs brouillon ([0f2f5126a](https://github.com/supernovae-st/nika/commit/0f2f5126ae17b793f74df263dfd98a81b5e1d592))
- **nika-verb-infer** — Pin render_schema cap boundaries — mutants 8/8 killed ([db070e4b3](https://github.com/supernovae-st/nika/commit/db070e4b359ad7a71bf7faff86931ebe6e7aff0f))
- **nika-verb-invoke** — Pin the control-char byte rule exactly ([42acd7826](https://github.com/supernovae-st/nika/commit/42acd782688ab6837cb06f901176464a6607b313))

### 📦 Build
- **release** — Cross-platform binary pipeline + homebrew formula bump ([a77153ae3](https://github.com/supernovae-st/nika/commit/a77153ae300fa232138af69f1c1ca88054b75be3))
- Align prod-scoped size/unwrap checks with the tests.rs convention ([4c4113181](https://github.com/supernovae-st/nika/commit/4c411318101d8d8078e4c13f153267935cf500c0))

### 🧹 Chore
- **ci** — Allowlist proptest .expect() in cfg(all(test, ...)) modules ([4c22a5c17](https://github.com/supernovae-st/nika/commit/4c22a5c175be2116e365707bd02643e621cbb7fe))
- **ci** — Add nika-kernel to tokio deny wrappers (dev-dep) ([edb7283a9](https://github.com/supernovae-st/nika/commit/edb7283a9fce8777cd8f6905b8164f3e464c2269))
- **ci** — Wire cargo-public-api snapshots (P0 — Gate 12 enforcement) ([3edfc6fa0](https://github.com/supernovae-st/nika/commit/3edfc6fa0c2e24c3644455e2869d7778bbdeee68))
- **ci** — Floor nika-exec-runner public-api + re-sync status block ([499133308](https://github.com/supernovae-st/nika/commit/4991333080d224c199c67550ed5d695352ea83e9))
- **ci** — Wire s8.5 into the gate scripts — first L1.5 crate ([4f1a9fb8d](https://github.com/supernovae-st/nika/commit/4f1a9fb8d11c68e4816698516fd405af51b83b1d))
- **ci** — Floor nika-pack public-api baseline — ratchet 23/25 ([51896b145](https://github.com/supernovae-st/nika/commit/51896b145b39f8eaedb0f88bed46fcd37259ca53))
- **ci** — Floor the 3 L2 verb-crate public-api baselines — ratchet 26/28 ([4d56b8b5b](https://github.com/supernovae-st/nika/commit/4d56b8b5bb663461ef9bc302d8ea057a4185b110))
- **ci** — Retire the first_balanced_span allowlist entry — parser fixed ([fdd5ea3f0](https://github.com/supernovae-st/nika/commit/fdd5ea3f0562e3b4506988bc8fde05fa54cdcd3a))
- **ci** — Deny wrappers — tower-http + nika-infer-local ([5e52bfce4](https://github.com/supernovae-st/nika/commit/5e52bfce442ee12579789b7d3a7c593bcda99410))
- **ci** — Unblock the train — block re-splice + infer-local exemption ([ec602f000](https://github.com/supernovae-st/nika/commit/ec602f00021b72222f93b7c092c0a20c4305d13e))
- **ci** — Green the shared push train — extract gate rows + anchors ([870b0eb3c](https://github.com/supernovae-st/nika/commit/870b0eb3c9d09a1fd8deb37e4a5278e9a45b7841))
- **ci** — Fix-forward the push train — schema clippy + adr-093 frontmatter ([d7367c242](https://github.com/supernovae-st/nika/commit/d7367c242e0a07485638f4826f0244d5dd7d6ad7))
- **claude** — Harden CC hooks — A2 + P1-10 + P1-11 ([a7311be98](https://github.com/supernovae-st/nika/commit/a7311be9817b984b4c2d8881a2c8ed853f7ff5af))
- **crates-io** — Publish=false on 7 foundation crates (Phase B.0) ([a4ed8c309](https://github.com/supernovae-st/nika/commit/a4ed8c3092fa0222cc66646c75c83c2e400df59c))
- **deny** — Scope tokio wrapper to nika-clock · l1 time effect ([b529d8759](https://github.com/supernovae-st/nika/commit/b529d8759276aef6653af659e24617d618d021b6))
- **deny** — Tokio ban-wrappers follow the kernel split ([d1b9bffa7](https://github.com/supernovae-st/nika/commit/d1b9bffa7de1aeb3989a5d378d123a99fa0164fd))
- **deny** — Allow the chromiumoxide transitive stack through l1 wrappers ([d1df5dd18](https://github.com/supernovae-st/nika/commit/d1df5dd18f02033afd61e4b6a514eea3d3ecddf8))
- **diamond** — Pre-w3 stabilization · 5 fixes ([dd2ec28e5](https://github.com/supernovae-st/nika/commit/dd2ec28e5831bc4cb66e95f5d92cb4f5409a074c))
- **dx** — Add miri + cargo-hack ci jobs + activate tokio layer bans ([7beb24dcb](https://github.com/supernovae-st/nika/commit/7beb24dcb1290a1f83c864d990840db03f5a01b4))
- **dx** — Add machete + semver-checks + typos CI + fix unused deps ([31128e9a2](https://github.com/supernovae-st/nika/commit/31128e9a209cf4204a4b7b59d2cf05d85ab33f1d))
- **dx** — Wire gitnexus session-status + auto-reindex hooks ([f7479cb08](https://github.com/supernovae-st/nika/commit/f7479cb0831dc3df7eff8dadf30bb70c650be222))
- **dx** — Add cliff.toml v2 for changelog automation (A1) ([a99abb9bd](https://github.com/supernovae-st/nika/commit/a99abb9bdc8c5d1f9e48b366d5f5e34be0b5dd77))
- **dx** — Decommission gitnexus — spn-insight replaces it (S8-S9) ([fd7b9f672](https://github.com/supernovae-st/nika/commit/fd7b9f6728c9bb8e1bd22d71ca5d4b7cf4b42ce0))
- **dx** — Remove scripts/gitnexus/ + clean remaining refs ([5d4a49a5f](https://github.com/supernovae-st/nika/commit/5d4a49a5fa79e7e5e4acfbb90624fa712ea30134))
- **dx** — Drop legacy tools/ fallback in SessionStart hook ([1ee7b31dd](https://github.com/supernovae-st/nika/commit/1ee7b31dd5087b56e9cece235fc88baf54e869f5))
- **hooks** — Add prepare-commit-msg auto-inject Nika trailer (A3) ([73ab5ff8f](https://github.com/supernovae-st/nika/commit/73ab5ff8f464d47546c54435f6870df5cc2d60c1))
- **hooks** — Register screen scope for nika-screen m2.1 ([be8cc6749](https://github.com/supernovae-st/nika/commit/be8cc6749424ce307776cd827c0b2dd76f8d27d9))
- **hygiene** — Install lefthook pipeline + hook scripts ([320563165](https://github.com/supernovae-st/nika/commit/3205631651ebc0bc4016e25d37e59ff26714673c))
- **hygiene** — Add layering enforcement + fix stale paths → 21/21 green ([27eca5cf6](https://github.com/supernovae-st/nika/commit/27eca5cf6c2fdcbfa8d30c050e5563d1079bc402))
- **hygiene** — Unblock push — fix 4 REDs from accumulated debt (Phase B.0) ([8e18475e2](https://github.com/supernovae-st/nika/commit/8e18475e2cc7febf96ae4b5dd577444630df4fd5))
- **hygiene** — Vector 27 grows the box-dyn-ok exemption marker ([bd013f8b3](https://github.com/supernovae-st/nika/commit/bd013f8b3273a01e21ac3129492efb077dd30a7b))
- **hygiene+claude** — Wire vector 25 into dashboard + cargo-yank/publish guard (K2) ([7a8b2f9fd](https://github.com/supernovae-st/nika/commit/7a8b2f9fd237defdd99a852743b619cf1970c7e2))
- **mintlify** — Status snapshot infra + purge 8 dead pages ([c5225d5b8](https://github.com/supernovae-st/nika/commit/c5225d5b821b32b811cd9a3e1c527fa15228b30b))
- **nika** — Refresh auto-state block — HEAD 3a03558d5 · 2524 lib tests ([42e8cd1bc](https://github.com/supernovae-st/nika/commit/42e8cd1bc28d1cb32c1f55755ff068ad3cc80930))
- **nika-builtin** — Drop the seed template's dead deps — machete gate ([ff2b5c59f](https://github.com/supernovae-st/nika/commit/ff2b5c59f1244caa63500c14e466d5b334b95188))
- **nika-catalog** — Mark supports_vision deprecated — session 3 decommission ([bcb2adc36](https://github.com/supernovae-st/nika/commit/bcb2adc362942e0ddb7e03c24f3eab595eee1631))
- **nika-catalog** — Machete ignore for build-dep generator ([ac3eb0263](https://github.com/supernovae-st/nika/commit/ac3eb02630e0f902d22abcc497fc1e0485c7eaf3))
- **nika-event** — Regenerate the public-api baseline — vocabulary cohort ([a716f292c](https://github.com/supernovae-st/nika/commit/a716f292cc9ea0e5766821d2719a3f56022aae8e))
- **nika-infer-local** — Converge tokenizers on candle's 0.22 — one copy ([c6ce23dc5](https://github.com/supernovae-st/nika/commit/c6ce23dc55f85646b92c1cef0ae76a80965fb538))
- **nika-pack** — Re-sync pack — spec divergence-audit hardening ([#114](https://github.com/supernovae-st/nika/issues/114)) ([24a167bda](https://github.com/supernovae-st/nika/commit/24a167bda92885a6710022e270dd2c54a25bbba9)) ([#114](https://github.com/supernovae-st/nika/pull/114))
- **nika-pack** — Re-sync pack — rounds 2+3 (spec 6c18927) ([#116](https://github.com/supernovae-st/nika/issues/116)) ([923ec04d4](https://github.com/supernovae-st/nika/commit/923ec04d4074c75a42e54eb6cc60c6ab8c9317f9)) ([#116](https://github.com/supernovae-st/nika/pull/116))
- **nika-pack** — Re-sync pack — argv exec, CEL expansion, permits, registry 30 ([#119](https://github.com/supernovae-st/nika/issues/119)) ([cbf0bbb50](https://github.com/supernovae-st/nika/commit/cbf0bbb50ad490438a15ac585abed172658f8511)) ([#119](https://github.com/supernovae-st/nika/pull/119))
- **nika-pack** — Re-sync pack — quickstart one-voice posture ([#120](https://github.com/supernovae-st/nika/issues/120)) ([688bc8357](https://github.com/supernovae-st/nika/commit/688bc83573a21cd1534b1e01e7121c13941439e0)) ([#120](https://github.com/supernovae-st/nika/pull/120))
- **nika-pack** — Re-sync extract-modes — jq one-output law · metadata-links unphantomed ([4c7de7da6](https://github.com/supernovae-st/nika/commit/4c7de7da6b28bd2cfdd7e258a9d15dc5bc4644bd))
- **nika-pack** — Re-sync 08-out-of-scope — H23 cursor-pagination posture ([0c23589ab](https://github.com/supernovae-st/nika/commit/0c23589ab106191940f00977e42f0f5daceb06b8))
- **nika-pack** — Re-sync builtins-v0.1 — notify data: field ([6b7d2eef7](https://github.com/supernovae-st/nika/commit/6b7d2eef75ced4f1859bbd3528d038a1147ca9cb))
- **nika-pack** — Re-vendor 14 clean SSOT pack files to embed ([807abedc7](https://github.com/supernovae-st/nika/commit/807abedc76d36b7e1c086908dd90bbaee0204ff9))
- **nika-pack** — Re-vendor embedded pack (egress reconcile + F-01 + F-03) ([7bdf1390d](https://github.com/supernovae-st/nika/commit/7bdf1390d3a45c7211a296d222f939dff613a4f1))
- **nika-schema** — Exclude analysis.rs graph divergers from cargo-mutants ([d2f05b970](https://github.com/supernovae-st/nika/commit/d2f05b97015ed7b06b3aba60d324f6263fbc935e))
- **nika-types** — Refresh stale public-api baseline ([78fe6a1ed](https://github.com/supernovae-st/nika/commit/78fe6a1edeae8115c871a1471261b61e058f3e08))
- **rename** — Pre-rename cascade · nika-diamond → main · main → brouillon ([94ebc0954](https://github.com/supernovae-st/nika/commit/94ebc0954593c9838932cca360a1474a5f9dfe94))
- **rename** — Post-rename cleanup · branch refs + rustls advisory ([ee74d97e0](https://github.com/supernovae-st/nika/commit/ee74d97e043633845ccfbeacce58ca4e96b5ab27))
- **state** — Sync status docs to 37/42 after admission + origin rebase ([9bdbd70e7](https://github.com/supernovae-st/nika/commit/9bdbd70e711b407994b7dc4eb8cfc6b5d25328d6))
- **status** — Regenerate baseline from single source of truth (Phase A) ([cd9602ca0](https://github.com/supernovae-st/nika/commit/cd9602ca0854510b86d4add8ac38335df711d8de))
- **status** — Refresh canonical block post-merge (20 crates, 1334 tests) ([c4fdab692](https://github.com/supernovae-st/nika/commit/c4fdab692fa9dfacb527cb530d08707600bd08e7))
- **status** — Re-sync canonical block after main merge ([4cd3fed39](https://github.com/supernovae-st/nika/commit/4cd3fed39b9c796ef80ecee1222a8a5fda7f7dfd))
- **status** — Refresh auto-block — 31 crates · 3 WIP · L4=2 · 1814 lib tests ([04301ca88](https://github.com/supernovae-st/nika/commit/04301ca88607de12273bc69d9010c0b7e5e93a48))
- **status** — Refresh auto-block — 32 crates · 29/42 admitted · L2=4 (all verbs) · 1860 lib tests ([454a764db](https://github.com/supernovae-st/nika/commit/454a764db643ce4191579fa40217c4f457177a4d))
- **status** — Refresh auto-block — 33 crates · 29/42 admitted · L1.5=3 · 1915 lib tests ([2baaaa22d](https://github.com/supernovae-st/nika/commit/2baaaa22dbe774f8087926e3fabdf1a991d4ba4a))
- **status** — Refresh auto-block — 34 crates · 2000 lib tests · L1.5=4 ([d768d0120](https://github.com/supernovae-st/nika/commit/d768d012040df441b44d6d472ec228a1e293d081))
- **workspace** — Pin zeroize=1.8 + nika-error mutation report (H8) ([47c2284cb](https://github.com/supernovae-st/nika/commit/47c2284cb8e3ce319c984f6e0685f43e85425db4))
- **workspace** — Refresh 4 stale public-api baselines ([7dd8fd081](https://github.com/supernovae-st/nika/commit/7dd8fd081d13cbd831eaabd42d0c14e5c308637b))
- **workspace** — Format the nika-compose commit output ([c3739147a](https://github.com/supernovae-st/nika/commit/c3739147aacbbedffd80c1f6a92cc3554b9ec56b))
- **workspace** — Clear pre-push hygiene RED (spec LOC, doc link, error-voice) ([1944737f4](https://github.com/supernovae-st/nika/commit/1944737f4c9949e8b2b55288c2091caae9aeb638))
- Re-version engine 0.80.0 → 0.90.0 + propagate versioning docs ([2a596209a](https://github.com/supernovae-st/nika/commit/2a596209a10d448f218de5c812b5e862d1aef65e))

### 💼 Other
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs

# Conflicts:
#	.claude/CLAUDE.md
#	AGENTS.md
#	ROADMAP.md ([f76e397cd](https://github.com/supernovae-st/nika/commit/f76e397cd314e0fe597058bb58447fceed50a557))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([a8e734f02](https://github.com/supernovae-st/nika/commit/a8e734f02d26062d76a2134132d0ddd940569c43))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([4816e636b](https://github.com/supernovae-st/nika/commit/4816e636b77d134da7425c15f5e18ff846467f5d))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([d6bb9fd7e](https://github.com/supernovae-st/nika/commit/d6bb9fd7ee0598caf9aa84ac171c70d41c7762f7))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([2d5a3301b](https://github.com/supernovae-st/nika/commit/2d5a3301b8ea7de524bc2c76c9c4ab32285410e5))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([b1f81aafe](https://github.com/supernovae-st/nika/commit/b1f81aafef81cbca4319b1b4c01c3e4d36bac352))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([98d60e66c](https://github.com/supernovae-st/nika/commit/98d60e66ca8d4210527c5beaa57666408ce15d56))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([3421467c0](https://github.com/supernovae-st/nika/commit/3421467c020a29499dd1777c093e41648e70e8c8))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([c78d73a4a](https://github.com/supernovae-st/nika/commit/c78d73a4a95040d9b79ce9928d5a503576b731d3))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([47cb6a763](https://github.com/supernovae-st/nika/commit/47cb6a763160dba69a11414c3aff393f85ef66fc))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([6250721fd](https://github.com/supernovae-st/nika/commit/6250721fd51d83462b250f354dbf9840236b573a))
- Merge remote-tracking branch 'origin/main' into feat/s4-nika-fs ([d7636a08d](https://github.com/supernovae-st/nika/commit/d7636a08db9d3117f1591b732fee73ddaf7dc3de))
- Merge branch 'feat/s4-nika-fs' — canon purge + hygiene vector 41

Brings the docs-canon truth pass (live docs state current canon only ·
crate tree mirrors the layer registry · gate12 connectome names + verb
range truth) and scripts/hygiene/check-canon-stale-terms.sh (vector 41:
dead names cannot return).
 ([6dbca5baa](https://github.com/supernovae-st/nika/commit/6dbca5baaa09de0a005172e6422471d9ea217a6b))
- Merge branch 'feat/permits-fit-analyzer' — builtin arg-shapes ([#123](https://github.com/supernovae-st/nika/issues/123))

Brings the analyzer builtin arg-shape pass: four ledger rows close and
the lints corpus moves to the spec repo (companion nika-spec c9233c9,
already on its main).
 ([20aea2143](https://github.com/supernovae-st/nika/commit/20aea21432891cfe39c575ecf4a2402bc1036743))


## [Unreleased]

### 🏁 Both WIP crates ADMITTED — the engine wip array is EMPTY (39/42 · 2026-06-21)

- **`nika-cli` crate** admitted (L4 · the operator surface · the `nika` verb
  tree: check · run · trace · inspect · graph · explain · spec · schema ·
  examples · new · doctor · pack · completions · lsp · mcp). New this admission:
  the spec §3.5 reduced surfaces — `--no-progress` (plain · CI default),
  `--quiet` (the compact verdict card), `--dry-run` (plan only · zero effects) —
  via a 3-mode `RenderMode` over a shared, drift-free failure-card render.
  - **Gate 5** mutation 91% (264/290) · **Gate 6** the fold property battery
    (`tests/fold_property.rs` — cost-conservation · one-row-per-task ·
    permutation-invariance · sequential ≡ interleaved-wave).
  - **Review swarm** caught + fixed a real P1: `--dry-run --output json` had
    corrupted the clean-JSON lane → the human flags now `conflicts_with_all`
    the machine modes (clap).
- **`nika-extract` crate** admitted (L1.5 · the 9 fetch extract modes behind
  the `nika:fetch` extract step — article Trafilatura cascade · feed · sitemap ·
  metadata + schema.org microdata · blocks · zones · page-type · links).
  - **Gate 5** mutation 79.7% → 93.2% (~50 boundary tests killed 73/81
    survivors in the heuristic functions) · **Gate 6** totality over all 9 modes.
  - **Review swarm** (3 agents): the adversarial refuter SURVIVED (totality +
    DoS-bounding hold); fixed og:video/audio absolutization + host-only search
    URLs; added a per-item microdata property cap (defense-in-depth). One agent
    finding was **rejected** verify-before-fix (`<a itemprop=url>` with no href
    → `""` is W3C-correct, not a text fallback).

### 🚚 Release engineering — cross-platform binary pipeline (2026-06-21)

- **`.github/workflows/release.yml`** — on a `vX.Y.Z` tag, builds the four
  `nika` binaries (macOS arm64/x64 · Linux arm64/x64), cuts the GitHub release
  with the exact tarballs the Homebrew formula points at, and (with a
  `TAP_GITHUB_TOKEN` secret) bumps the tap formula. Fires only on a tag —
  nothing publishes until you tag. `scripts/release/update-formula.sh` does the
  version + sha256 rewrite (runnable by hand too). Unblocks the Homebrew path
  that had no pipeline.

### 🏛️ nika-schema L0 admission — parser + analyzer + static-check (ADMITTED · all 12 gates · 2026-06-18)

- **`nika-schema` crate** admitted — the workflow AST, parser, analyzer, and
  the ADR-092 `nika check` static-check ladder (the last L0 WIP crate).
- **Gate 5** closed in BUDGET mode (`survivors ≤ 300`): 269 timeout-divergers
  + 21 enumerated equivalents, each scoped-re-verified. Rounds 1-7 (~190 tests)
  killed the floor's real-gap tail — analyzer/check collection + lint logic,
  the `read_dag` cap/pinch boundaries, the default-gate runnable path, and the
  expression-parser offset/depth/byte-scanner.
- **Security**: two complementary `when:`-gate DoS fixes integrated — a
  `MAX_GATE_LIST_ITEMS` cap on the leaf-evaluation re-scan and a `BTreeSet`
  dedup in `collect_bad_literals` (an O(n²) `Vec::contains` scan that burned
  ~3 s of CPU on a 2-task workflow before the fix). Plus 7 `#[non_exhaustive]`
  source types (FCI-002) and a parse+check criterion benchmark (Gate 7 · parse
  10-task 30.9 µs).

### 🧩 Announce ladder s19.6 · nika-lsp L4 admission — the `nika lsp` language server (ADMITTED · 12-gate closed · 2026-06-15)

- **`nika-lsp` crate** · the Nika language server (`nika lsp`, stdio) — the
  v0.1 editor brain for `.nika.yaml`. ONE crate (nika-lsp-core collapsed in as
  internal `analysis::*` modules · per `nika-invariants` + collapse-vs-publish ·
  reconciles `D-2026-06-10-N6` steps 19.6/19.7). Stack: `lsp-server` 0.7 sync
  stdio loop + `lsp-types` 0.97 · pure analysis over `nika-schema`.
- **Diagnostics** reuse the SAME ADR-092 `nika check` ladder (one source of
  truth · task-anchored ranges) · **hover** on the 4 verbs + keywords AND on a
  task reference (`depends_on` item / `${{ tasks.X }}` → the target task's id +
  verb) · **completion** (keys · verbs · `model:` providers · the workflow's own
  task ids · auto-trigger on `.` `/` `[`) · **document symbols** ·
  **go-to-definition** for task refs.
- Feeds the `nika-vscode` extension, auto-detected via `caps.lsp` once
  `nika --help` lists `lsp` — zero extension change. 124 lib tests · mutation
  96.9% · the `nika lsp` subcommand wired into `nika-cli` (owns stdout · LSP
  exit-code convention).

### 🤖 Announce ladder s12 · nika-verb-agent L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-agent` crate** · the `agent` verb executor — the multi-turn
  ReAct loop (model → whitelisted tool dispatch → results fed back → repeat)
  per `nika-spec spec/02-verbs.md §agent`. The **4th and last verb**
  (`D-2026-05-22-N18` · the verb count is 4, absolute). Generic over three
  injected kernel seams: `ProviderInferDyn` (inference) · `ToolExecuteDyn`
  via `InvokeVerb` (dispatch) · `ToolDefinitionProviderDyn` (the tool-def
  source). Zero runtime tokio dep — every turn rides the injected providers.
- **The ToolDefinitionProvider seam** (`nika-kernel-ai`) · resolves the s12
  §8 blocker found 2026-06-11: the agent hands the model its whitelisted
  tools as `ToolDef`s, but only tool NAMES were in hand — nothing enumerated
  definitions. A new kernel trait (the `ToolExecute` pattern · `Dyn` twin ·
  `ToolDefsError` → NIKA-234) + `MockToolDefinitionProvider`. The wiring
  layer implements it over the builtin catalog + (later) live MCP
  `tools/list`.
- **Loop semantics (normative · spec §2)** · terminal-1 (no tool calls →
  `Completed`) and terminal-2 (`nika:done` → `ExplicitCompletion`, with the
  `result:` arg or the last assistant message) BOTH precede the budget gate
  — a concluded answer is a success even if its turn crossed the budget.
  Budgets FAIL (max_turns → NIKA-460 · max_tokens_total → NIKA-461, `>=`
  exhaustion, checked before spending more) with `partial_output` preserved.
- **Security (spec §3 · default-deny)** · the whole tool batch is whitelist-
  validated BEFORE any dispatch (a denied sibling fails the turn with zero
  side effects · NIKA-462 immediate, not fed back). `nika:done` is loop-owned
  (never dispatched · wins over batch-mates). Model-emitted names are length-
  capped + control-char-rejected, and the violation error carries a REDACTED
  name (NIKA-450 log-injection parity). Source-supplied tool defs are
  sanitized before reaching the model.
- **The glob whitelist** · gitignore semantics canonically (a spec
  portability invariant): `*` bounded by `/` and `:`, `**` crosses them,
  `!` negation, last-match-wins. Matched by an O(n·m) DP (correct under
  interleaved `*`/`**`) + a totality proptest on the model-controlled input.
- **Structured output** · the final message validates against the task
  `schema:` (NIKA-464) with `infer.schema:` parity — bare-parse then a
  string-aware balanced-span extraction (tolerates fences + prose).
- **3-lens review swarm** (spn-nika + rust-pro + feature-dev) · all findings
  folded same session: the budget-before-completion bug, the batch-validate
  security ordering, the `**`/`*` glob backtrack gap, log-injection
  redaction, saturating token math, INV-019 `AgentOutput::new()`, the
  max_turns ceiling. NIKA_460..466 registered · hub 460-469 row · API-locked.

### 📡 Telemetry vocabulary closes over the display contract (nika-event · additive)

- **6 new `EventKind`s** · `task_retrying` · `task_cancelled` ·
  `workflow_cancelled` · `cost_incurred` · `infer_chunk` ·
  `permit_checked` — every state the run UI can show (contract §3.1
  state machine) and every live-meter refold driver the contract names
  (§3.3) is now expressible by a canonical engine event. Cancellation is
  terminal-not-failure (a decision, not a defect). `permit_checked`
  makes the declared `permits:` boundary observable at runtime (the
  ADR-092 audit moat).
- **`EventClass`** · the coarse 7-class classifier (`EventKind::class()`)
  — renderers/routers branch on stable classes, not 17 variants.
- **Reference fold** · the `nika-schema` `verbs` example consumes the
  full vocabulary: `--events` renders the whole tape digestibly; `verbs
  workflow` folds the SAME tape into the animated DAG (retry arc ↻ ·
  live stream · ticking cost meter · permits counter). The state-machine
  coverage test pins « every UI row status is event-reachable ».

### 🔌 Announce ladder s11 · nika-verb-invoke L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-invoke` crate** · the `invoke` verb executor per
  `nika-spec spec/02-verbs.md §invoke` (third of the 4 verbs). Rides the
  kernel `ToolExecuteDyn` seam with the engine's builtin+MCP dispatcher
  injected — zero tool implementation of its own, zero Cargo dep on
  `nika-builtin`/`nika-mcp`.
- **The closed-namespace contract** · the tool-ref namespace set is CLOSED
  at v1 (`nika:` · `mcp:` only · `mcp:` requires the `server/tool` slash);
  the verb does the lightweight semantic check before dispatch (grammar
  SHAPE stays the upstream `nika-schema` `NIKA-PARSE` concern). Result
  mapping: `is_error: true` → NIKA-451, dispatcher `NotFound` →
  `UnresolvableTool`, other dispatch failures → NIKA-452.
- **Security guards (swarm)** · whitespace padding and ASCII control chars
  in the tool id are rejected before it reaches a `ToolCall`/log field
  (log-injection class); the derived fallback `call_id` appends a
  process-monotonic counter so repeated same-tool invokes don't collide on
  the kernel's unique-call-id contract.
- **Error one-voice** · NIKA-450..452 registered in the Verb range; the
  verb-range help moved into a `verb_help` helper (keeps `code_help` under
  the 100-line cap).
- 16 lib tests (1 totality proptest cross-checked against an independent
  predicate) · mutation all viable killed bar one documented equivalent ·
  clippy 0 · doc 0 · layering + deny green · tag `v0.80.0-alpha.7`.

### ⚙️ Announce ladder s10 · nika-verb-exec L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-exec` crate** · the `exec` verb executor per
  `nika-spec spec/02-verbs.md §exec` (second of the 4 verbs). Rides the
  kernel `ShellRunDyn` seam with the effect injected (`TokioShell` in prod ·
  `MockShell` in tests) — zero subprocess code of its own, zero Cargo dep on
  `nika-exec-runner` (the L2→L1 inversion through the kernel trait).
  `pre_validated` is NEVER set, so the s7 runner blocklist stays the floor
  (structurally pinned by test).
- **The capture one-obvious-way split** · default modes (`stdout` · `stderr`
  · `combined`) fail the task on a non-zero exit (NIKA-440 / spec
  NIKA-EXEC-001 · with a capped stderr tail); `capture: structured` returns
  `{ stdout, stderr, exit_code }` as DATA — the workflow branches on it, the
  task succeeds.
- **Verb-boundary input guards (NIKA-442)** · a NUL byte in command/stdin
  (silent shell truncation) and a malformed env key (`=` · NUL · empty ·
  child-env corruption) are refused before the runner call — the security
  swarm's two findings.
- **Error one-voice** · NIKA-440..442 registered in the Verb range ·
  `MockShell` aligned to the Send-variant traits + gained `enqueue_result`.
- 19 lib tests (3 proptests · Gate 10 parity vs brouillon) · mutation all
  viable killed bar one documented equivalent · clippy 0 · doc 0 · layering
  + deny green · tag `v0.80.0-alpha.6`.

### 🗣️ Announce ladder s9 · nika-verb-infer L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-infer` crate** · FIRST L2 verb crate — the `infer` verb executor
  per `nika-spec spec/02-verbs.md §infer` (one of the 4 verbs locked forever ·
  D-2026-05-22-N18). Resolves `model: provider/name` through the s8.5
  `nika-providers` registry (D-N17: providers live BELOW the verbs · no
  verb→verb sideways dep), shapes the kernel `InferRequest`, returns the full
  `InferResponse` for the future L3 engine's event/cost seam.
- **Structured-output floor in-crate** · `schema:` tasks get native
  `ResponseFormat::JsonSchema` when the profile supports it (instruction
  fallback otherwise), lenient JSON extraction (bare → fenced → first balanced
  string-aware span), `jsonschema` 0.33 validation (compiled ONCE per run —
  an uncompilable schema is NIKA-432 with zero paid round-trips), and a
  bounded validation retry (default 2 · spec-sanctioned before NIKA-INFER-002).
  Schema text re-injected into prompts is capped at 4096 chars.
- **Error one-voice** · `VerbInferError` speaks `NikaErrorCode` via the new
  registry-owned NIKA-430..433 (Verb range 430-479 opened · same pattern as
  the M2 computer-use ranges) · transience inherited from `ProviderError`,
  never overridden.
- **Gate 11 swarm (3 lenses · 0 P0)** folded same-session: compile-once
  validator · u8→u32 attempts counter (closes the u8::MAX budget saturation
  loop) · schema render cap · both transience branches pinned.
- 33 lib tests (3 proptests · Gate 10 parity vs brouillon shaping pinned) ·
  mutation 95.8% overall + 8/8 on the cap helpers · clippy 0 · doc 0 ·
  layering + deny bans green. New workspace dep `jsonschema` (default-features
  off · no network resolver).

### ♿ Phase 2 M2.3 · nika-a11y L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-a11y` crate** · third computer-use L1 effect crate · implements the
  L0.5 `io::a11y::AccessibilityTree` trait (`snapshot` + `find` + `resolve_ref`)
  exposing the active window's accessibility tree as `AxNode` records. **macOS-first**
  (decision §4 of `docs/crate-specs/nika-a11y.md`): backend via the safe
  **`accessibility` 0.2** crate (`AXUIElement` · `TreeWalker` · the unsafe
  `ApplicationServices` FFI is encapsulated → crate stays `unsafe_code = forbid`);
  Linux `atspi` / Windows `uiautomation` deferred to a consumer signal (LOCK-031).
  B.1 spec (backend research: 3 vetted permissive crates verified on crates.io)
  → B.2 skeleton (`A11yError` NIKA-1200..1206 · `AxBackend` · `snapshot`/`find`/
  `resolve_ref` route through a `walk_tree` placeholder returning `BackendNotWired`).
- **ADR-081 Guard 3 (AX-secure-field redaction · MANDATORY-at-admission) is
  headless-complete at B.2** · a pure recursive tree-transform (`redact_secure_fields`
  / `is_secure_field`) strips `value` from any secure-text node (macOS
  `AXSecureTextField` subrole · AT-SPI `STATE_SENSITIVE`) to `None` (zero leak),
  applied before any node leaves the crate. The pure `find` filter
  (`matches_query` + depth-bounded `collect_matches`) ships too. 12 lib tests
  (incl. a proptest pinning the redaction invariant) · clippy 0 · doc 0 ·
  `cargo-machete` clean · `cargo deny` ok. `nika-a11y` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/eiz/accessibility`) before recommending the backend.
- **B.3 macOS `AXUIElement` walk wired** · `system_wide().focused_window()`
  rooted recursive `build_node` (role/label/value/subrole → `AxNode`) inside
  `spawn_blocking` (the `!Send` handle stays worker-local · CANCEL SAFETY) ·
  macOS-gated deps `accessibility` 0.2 + `core-foundation` 0.10 (CFString/CFType
  reads · all upstream symbols — `focused_window` · `value().downcast::<CFString>()`
  · `children().iter()` · `subrole()` — verified against the crate source before
  use). Non-macOS compiles to `BackendUnavailable` (NIKA-1205). `resolve_ref`
  backed by a `Mutex<Option<AxNode>>` cache of the last redacted snapshot + pure
  `find_by_id`. Pure `ax_role_from_str`. Closed the `BackendNotWired` placeholder
  (NIKA-1200 retired · slot reserved). `bbox` deferred (`None` · frame→`Rect`
  refinement).
- **B.4 12-gate close · ADMITTED** · extracted the pure `assemble_node` (role
  map + empty-title/subrole filter + `AxNode::new`) out of the FFI `build_node`
  to maximize headless coverage; added a `MAX_WALK_DEPTH` (512) recursion cap so
  an untrusted/deep/cyclic focused-app tree can't overflow the stack (caught by
  the Foreman-direct review). **Gate 5 mutation 34/41 viable caught (82.9 %)** ·
  100 % of the headless surface · 7 `AXUIElement`-walk mutants documented-exempt
  per ADR-003 Rule 2 (`docs/crate-specs/nika-a11y.md` §7.1). **Gate 11** ·
  sub-agents hit the 1M-context credit wall → Foreman-direct 3-lens review
  (PE-5.1 · rust-pro + Diamond + bug-hunt · all ADMIT). 14 lib tests + 1
  `#[ignore]` smoke · clippy 0 · doc 0 · machete clean · deny ok · workspace
  `--lib` 1170. Workspace 13/42 admitted · WIP nika-schema only.

### 🔤 Phase 2 M2.2 · nika-ocr L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-ocr` crate** · second computer-use L1 effect crate · implements the
  L0.5 `io::ocr::OcrEngine` trait (`read` + `read_region`) via the pure-Rust
  **`ocrs` 0.12** engine (**`rten` 0.24** runtime · no C system dep · keeps
  `unsafe_code = forbid`). B.1 spec → B.2 skeleton (`OcrError` NIKA-1100..1109
  · pure frame/region validation · `BackendNotWired` placeholder) → B.3 real
  inference: `OcrBackend::with_models(detection, recognition)` eager-loads two
  `.rten` weight files from **explicit local paths** (sovereignty Rule 1 ·
  reads local files only · NEVER auto-downloads · models are operator/daemon-
  provisioned), `read`/`read_region` validate the RGBA8 `Frame` purely then run
  `prepare_input → detect_words → find_text_lines → recognize_text` inside
  `tokio::task::spawn_blocking` (the sync CPU-bound engine runs off the async
  runtime · kernel CANCEL SAFETY: a dropped future abandons the read with no
  side effects). The B.2 `BackendNotWired` placeholder is CLOSED (NIKA-1100
  retired · slot reserved) per `skeleton-option-a-pattern.md` §5.
- **`nika-ocr` 12-gate close (B.4)** · admitted — all 12 gates green
  (registry L1 · ADR-081 inherits 7-guard contract, owns none mandatory ·
  `#[non_exhaustive]` · zero-unwrap src · ~290 LOC · NIKA-1101..1109 ·
  cancel-safety · `test --workspace --lib` 1156 · clippy 0 · `cargo doc` 0 ·
  `cargo-machete` clean · `cargo deny` ok). **Gate 5 mutation 81/87 viable
  caught (93.1 %)** · 100 % of headless-reachable logic · 6 model-inference
  mutants documented-exempt per ADR-003 Rule 2 (need real `.rten` weights ·
  `docs/crate-specs/nika-ocr.md` §6.1). Pure helpers (`rgba_to_rgb` ·
  `crop_rgba` · `words_bbox_union` · `validate_frame` · `validate_region`)
  proptested + 100 % mutation-killed. **Gate 11 review** · sub-agents hit the
  1M-context credit wall → Foreman-direct 3-lens review per
  `orchestrator-autonomous-v6.md` PE-5.1 (rust-pro + Diamond-discipline +
  bug-hunt · all ADMIT · 1 P1 stale-module-doc fixed). Deps: `+ocrs +rten`
  (workspace) `+tokio` rt + `tempfile` dev · `nika-ocr` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/robertknight/ocrs`) before wiring · no phantom symbols.

### 🖥️ Phase 2 M2.1 · nika-screen L1 admission (ADMITTED · 12-gate closed · 2026-05-23)

- **`nika-kernel` `io::screen`** · NEW `capture_stream` additive trait method +
  `FrameStream` type alias (`Pin<Box<dyn Stream<Item = io::Result<Frame>> + Send>>`),
  the canonical kernel streaming idiom (cohérent `ai::provider::InferEventStream`).
  Zero breaking change · uses `futures-core` (NOT `tokio-stream`, which is
  L0.5 layer-banned per `Cargo.toml`). Begins the M2.1 6-batch dispatch (B.1).
- **`crate-layer-registry`** · `nika-screen` registered L1 — first computer-use
  effect crate (Gate 1). ADR-081 7-guard contract already shipped (`3e40c18b3`).
- **`nika-screen` crate** · B.2 skeleton (`ScreenError` NIKA-1000..1009 · 10 codes
  · `ScreenBackend` + consent/LED guard skeletons) → B.3 single-shot capture WIRED
  via `xcap` 0.9.5 (`list_displays` / `capture_full` / `capture_region` · sync OS
  calls wrapped in `spawn_blocking` so the `!Send` `Monitor` stays worker-local and
  dropped futures surrender promptly · zero-copy RGBA8 `Frame`) → B.4 wires
  `capture_stream` (bounded `tokio::mpsc` + dedicated capture thread · ~30fps
  cadence · drop-stop cancellation via channel-close · `futures_core::Stream`
  adapter over `poll_recv`). All 4 `ScreenCapture` methods now real — the B.2
  `BackendNotWired` skeleton is fully CLOSED. B.5 makes the ADR-081 guards real
  + ENFORCED · a fail-closed `ConsentGate` (guard 7 · in-memory · session-scoped
  · revocable · per-frame re-check inside the stream worker) gates every pixel
  capture, and a RAII `LedIndicator` (guard 6 · engaged-count) stays lit for the
  whole capture. xcap encapsulates the OS FFI
  (objc2 / x11 / windows) so the crate is `unsafe_code = forbid`-clean. Plan-dep
  correction · the
  plan's `nokhwa` is a WEBCAM lib (docs.rs verbatim); `xcap` is the screen-capture
  crate (per `cross-source-validation.md` §2.7).
- **`nika-screen` 12-gate close (B.6)** · admitted as the first L1 effect crate —
  all 12 gates green (registry · ADR-081 · `#[non_exhaustive]` · zero-unwrap ·
  LOC 943 · NIKA-1000..1009 · cancel-safety · `test --workspace --lib` 1125 ·
  clippy 0 · `cargo deny` ok · forward-compat). GAP-3 `From<ScreenPoint>` shim
  CARRIED FORWARD to M2.4 `nika-input` · `ScreenPoint` is a `cockpit_overlay`
  (Olympus) type, so a `From` impl in `nika-screen` would violate cross-flow
  D-2026-05-08-N1 (Nika→Olympus) and is an `io::input` (cursor) concern, not
  `io::screen`; the conversion lives on the Olympus consumer side (where
  `cockpit-input-injection` already mirrors it).

### ⚡ Perf profile + craft amendments (2026-05-12)

Pre-W3 perf-craft + architecture polish per 2-agent SOTA audit
(`spn-rust:rust-async-expert` + `spn-rust:rust-perf` parallel) ·

- **`Cargo.toml [profile.release]`** · `lto=fat` + `codegen-units=1` +
  `strip=symbols` + `panic=unwind` + `debug=line-tables-only` +
  `incremental=false` · matches ADR-061 SLSA L3 prep · ~5-10% perf
  delta on BGE-M3 cosine + BM25 + RRF hot paths · 2× build cost
  release only · dev unaffected.
- **`Cargo.toml [profile.bench]`** · inherits release + `debug=true`
  for `cargo flamegraph` + `perf annotate` at W3 admission Gate 7.
- **4 `const fn` promotions in `nika-types`** · `Cost::new` ·
  `Cost::zero` · `Cost::is_zero` · `Trust::new` · `Trust::is_at_least` ·
  unlocks `const SATELLITE_COST: Cost = Cost::from_milli_usd(5)` at
  call-sites = zero runtime eval. `From`-trait + `Option::map` blocked
  (not const-stable yet · 2027+ horizon · per Rust 1.91 limits).
  Forward-compat per ADR-007 · `pub fn → pub const fn` non-breaking.

### 📐 BLUEPRINT_2036 v1.3 amendments (2026-05-12)

Cumulative cascade v1.0 → v1.1 → v1.2 → v1.3 per `docs/architecture/
BLUEPRINT_2036.md` frontmatter · status proposal · annual decennial
review 2027-04+.

- **v1.1 (per-crate detail + best-enemies SOTA)** · 42-crate table
  with LOC + deps + trait + Gate-9 + admission target per row ·
  Restate/LangGraph/Temporal/Mem0/Letta differentiation matrix ·
  collapse-vs-publish principle § 1.5 locked
- **v1.2 (11/10 amplifiers + guardian framing)** · 9→4 amplifier ADR
  fold (saves 5 empty shells · `socratic-research-discipline.md`
  Step 5 Option D) · §4.7 anti-Palantir + AI-2027 trajectory mapping ·
  14 prior Nika-mappings re-validated 2026-Q2
- **v1.3 (perf craft + async depth · this entry)** · §4 RRF fairness ·
  Loom scope (2-thread minimal + Shuttle PCT for full DAG) ·
  `consume_budget` cooperative scheduling · `[profile.release]`
  mirror · §4.5 ADR-066 `#[tracing::instrument]` discipline · NEW
  ADR-070 (`TaskTracker` + child-token fan-out · kernel-pure preserved
  per ADR-016 Alt-A) · ADR-041 `#[track_caller]` builder amendment

### 📚 Pre-launch hygiene shipped (2026-05-12)

- **Per-crate READMEs** · 4 missing of 8 shipped (`nika-error` ·
  `nika-catalog` · `nika-kernel` · `nika-kernel-mock`) following
  tokio/serde/thiserror SOTA pattern (~80-120L each)
- **`CODE_OF_CONDUCT.md`** · Contributor Covenant v2.1 boilerplate ·
  conduct@supernovae.studio · 4-tier enforcement ladder
- **`SECURITY.md`** · vulnerability disclosure policy · 72h ack · 90d
  disclosure · 11-row NIKA-271..389 defense layers table
- **`Cargo.toml [workspace.lints.rustdoc]`** · compile-time doc gate
  (broken_intra_doc_links=deny · private=warn · invalid_codeblock=deny)
- **`.github/workflows/diamond-ci.yml`** · semver-checks baseline ·
  `origin/nika-diamond` (renamed branch · stale since 2026-05-06) →
  `origin/main` · was silently failing

### 📚 Wave 4E — Mintlify rebuild + docs repo split (2026-04-17)

End-user documentation split out to a dedicated public repository and
rebuilt from the current workspace state.

- **`supernovae-st/nika-docs`** — new public repo, serves
  [`docs.nika.sh`](https://docs.nika.sh) via Mintlify. Replaces the
  in-engine `docs/mintlify/` directory, which is removed from this
  repo. Engine-internal docs (`docs/adr/`, `docs/architecture/`,
  `docs/crate-specs/`) stay here.
- **Mintlify content refreshed** — 2-tab navigation (Guide / Reference),
  honest v0.80 pre-release framing, live snapshot of 32 providers, 49
  capability rules, 35 ADRs (11 thematic groups), L0 architecture
  decisions, admission 12-gate walkthrough.
- **Dead pages purged** — 8 Mintlify pages that no longer mapped to the
  Diamond workspace state removed pre-split.
- Cross-links from this repo's README + ROADMAP point to
  `docs.nika.sh` for end-user content.

### ⚡ Swarm-3 Batches I.b + II ε.2/ε.3 + Wave 3A + Wave 4A + 4B seeds + Wave 4C (2026-04-17)

**Hygiene — Batch I.b vectors 30-33 (+4 new):**

- **Vector 30 `check-cancel-safety.sh`** — every `async fn` in
  `crates/nika-kernel/src/**` now carries a `// CANCEL SAFETY:` or
  `/// CANCEL SAFETY:` marker. 43 kernel methods annotated
  (cancel-safe contract: drop semantics, atomic vs non-atomic writes,
  `kill_on_drop` requirement, billing/telemetry exposure).
- **Vector 31 `check-owned-strings.sh`** — preventive ratchet: bans
  non-static `&str` in nika-catalog `pub` fields / `pub fn` return
  types. Catalog stays 100% `&'static str` per ADR-008 codegen pragma.
- **Vector 32 `check-unsafe-count.sh`** — `unsafe` token counter
  vs `scripts/hygiene/baselines/unsafe-count.txt` (currently 0).
  Substitutes cargo-geiger which is hostile to virtual manifests.
- **Vector 33 `check-layer-deps.sh`** — per-layer banned third-party
  deps (`[workspace.metadata.diamond] layer-bans`). L0 rejects 17
  deps (tokio family, rayon, async-std, smol, futures family,
  reqwest, hyper, axum, actix-web); L0.5 rejects 11.
- **Killed vector 7** (linear-issue-states stub) **and vector 18**
  (adr-dangling duplicate of vector 16).

**Wave 3A — engine post-commit hook for Olympus snapshots:**

- `scripts/hooks/post-commit-olympus-xtask.sh` wired via lefthook.
  Background `pnpm tsx olympus/scripts/xtask.ts` regenerates
  workspace.json + snapshots + hygiene-status.json on every engine
  commit; Olympus live-refreshes `/timeline`, `/graph/diff`,
  `/graph/fitness`, `/hygiene`.

**Wave 4A — v0.95 Cortex + v0.100 WASM reservations (R1-R5):**

- **R1 `EmbeddingSpec`** (`nika-types::embedding`) — Dtype,
  DistanceMetric, EmbeddingSpec; `#[non_exhaustive]` + snake_case wire.
- **R2 `MemoryFrameRef.trust: TrustLevel`** — sticky ingest taint;
  `#[serde(default)]` → UNTRUSTED fail-safe.
- **R3 `RecallQuery.tenant: TenantId`** — mandatory multi-tenant
  keyspace scope. `TenantId::default_tenant()` → `"default"`.
- **R4 `WasmPluginError::OutOfFuel` + `Trap { kind: TrapKind }` +
  `PluginCallContext`** — fuel metering, W3C-style trap taxonomy,
  per-call context with trust + cancel + budget.
- **R5 `MemoryLifecycle` trait** with default-impl consolidate/prune
  returning empty reports. Standalone; Cortex opts in at v0.95.

**Wave 4B seeds (telemetry foundations):**

- **#1 `SpanGuard.parent_span_id` + `links: Vec<SpanRef>`** — W3C
  Trace Context parent linkage unblocks Olympus `/trace`. Default
  `TracerProvider::start_child_span` backfills parent.
- **#3 `Timestamp(i64 unix_ns)` + `WallDuration(i64 nanos)`** in
  `nika-types::timestamp`. RFC 3339 Display via inlined Hinnant
  civil-from-days algorithm. Serde-transparent wire. Field retrofit
  (`_ms: u64` → `timestamp`) deferred.

**Batch II — test depth:**

- **ε.2 Loom** — `#[cfg(loom)]` interleaving tests for `CancelCtx`
  (INV-029). Conditional `[target.'cfg(loom)'.dependencies]`.
  Run explicitly via `RUSTFLAGS="--cfg loom" cargo test`.
- **ε.3 proptest audit** — 14 new properties: TrustLevel lattice
  invariants (meet/join bounds, idempotence, commutativity,
  associativity, absorption); ID serde roundtrip (TenantId,
  ProviderId, ModelId, TaskId, TraceId full 2^128 surface, SpanId
  full 2^64 surface).
- **ε.1 mutation baseline** — `cargo mutants -p nika-error` run:
  60 mutants, 31 caught, 13 missed (mostly miette::Diagnostic
  accessor returns — no observable behaviour), 16 unviable.
  Viable kill rate 70.5%. Pushing to ≥90% requires dedicated
  miette diagnostic-method assertion tests; deferred to a focused
  follow-up session.

**Batch V.2** — `docs/architecture/axes.md`: 12-axis × crate ISP
matrix with shipped/reserved/not-yet markers. Source of truth for
Olympus `/graph/architecture` edge rendering + Gate 12 audits.

**Observability locks (parallel work already landed):**

- Q12 — `ObservabilitySink` dropped (5→4 effect channels);
  `AuditSink` added as compliance-grade 5th channel.
- Q13 — `GenAiAttrs` OTel semconv bridge on Infer{Request,Response}.

**CI ratchets:**

- `cargo-public-api` snapshot workflow (Gate 12 mechanical).
- `cargo-semver-checks` workflow.
- Public-api baseline files regenerated on every reservation commit
  (`--all-features --omit auto-trait-impls` to match CI invocation).

**Forward-compat seams:**

- nika-types `no_std`/`alloc` seam at module level (F1 complete;
  shipped 2026-04-17 morning).
- F2 (full per-module cfg-gating) deferred — requires uuid dep
  re-architecture (currently in `serde` feature but used in
  non-serde struct fields in RunId/EventId/CorrelationId/MemoryId).
  Re-open trigger: uuid becomes unconditional OR UUID-backed IDs
  move to a dedicated feature separate from serde.

**Numbers at close:**

| field              | value                                      |
|--------------------|--------------------------------------------|
| HEAD               | (updated at commit time)                   |
| lib tests          | 905 (+58 this session)                     |
| integration tests  | 10                                         |
| loom tests         | 2 (cfg-gated)                              |
| clippy             | 0 warnings                                 |
| hygiene vectors    | 31 deployed (27 green / 4 yellow)          |
| crates admitted    | 6 + 1 WIP (unchanged)                      |
| ADRs               | 25+ (seeds ADR-029-032 + 035 authored)     |

### ⚡ Phase D Session 4B — Data enrichment (2026-04-16)

Pure data expansion on the structural foundation laid by Session 4A.
Zero trait/struct changes — only enum variants, TOML data, and tests.

- **6 new `ParamFlag` variants** — `BatchApi`, `ContextCaching`,
  `PredictedOutputs`, `ComputerUse`, `Citations`, `IncludeReasoning`.
  Aligned with `OpenRouter` 25-value `supported_parameters` vocabulary.
  Enum: 7→13 variants.
- **3 new `Modality` variants** — `Embedding` (vector output), `Speech`
  (TTS/ASR), `ImageGen` (text-to-image). Covers non-LLM provider
  capabilities. Enum: 5→8 variants.
- **4 new `TokenizerFamily` variants** — `LlamaV4` (~200k vocab, distinct
  from LlamaV3), `Granite` (IBM `StarCoder` BPE), `Glm` (Zhipu
  `SentencePiece`), `Grok` (xAI custom). Enum: 8→12 variants.
- **7 new providers** — nvidia-nim (FIX: inventory discrepancy),
  deepinfra, replicate, hyperbolic, writer, databricks, cloudflare.
  All `openai-chat` dialect. Count: 25→32.
- **7 new capability rules** — one `Matcher::Any` fallback per new
  provider (text-only, `json_schema` where applicable). Count: 42→49.
- `mock-full` rule updated with all 13 `ParamFlag` variants.
- Cross-catalog overlap allowlist: replicate + cloudflare (dual-role).

### ⚡ Phase D Session 4A — Catalog structural enrichment (2026-04-16)

Context-window + output-limit + JSON mode enrichment. First structural
expansion of capabilities beyond the Session 2a/2b foundation.

- **3 new CapPatch fields** — `context_window_tokens: Option<u32>`,
  `max_output_tokens: Option<u32>`, `json_mode: Option<JsonMode>`.
  Per-model context windows and output limits are now expressible in the
  TOML-driven capability resolver.
- **`JsonMode` enum** — `Schema` (tool_use enforcement) / `Object`
  (unstructured json_object mode). Per-provider granularity.
- **`ContainsAny` matcher** — word-boundary-anchored substring matching
  with left/right boundary chars (`-`, `_`, `/`, `.`, `@`). Prevents
  "sonnet-4" from matching "sonnet-4-60" (the `6` after "sonnet-4" is
  not a boundary character).
- **`#[non_exhaustive]` on 20 mock structs** — all `nika-kernel-mock`
  types now enforce invariant #19 (attribute + `pub fn new()`).
- **`HttpStreamResponse::new()`** — invariant #19 compliance for the
  only `#[non_exhaustive]` struct that was missing a constructor.
- **12-field merge_with regression guard** — all CapPatch fields covered
  by a single test with confirmed RED on removal.
- **estimate_cost edge cases** — zero tokens → $0.00, nonexistent model → None.
- **MemoryId deserialize error paths** — missing `mem-` prefix and invalid
  UUID now have dedicated tests.
- Token count: 625 → **630 lib tests** (+5).

### 🛡️ Phase C Wave 3 — Stabilization + review-swarm defense (2026-04-16)

Hardening pass after the foundational-types expansion. Mutation testing,
proptest campaigns, and a 3-agent review swarm closed all P0/P1 findings.

- **Seal `SecretResolver`** — `cargo-expand` verified private supertrait;
  community can't implement, allowing future method additions (P1-1).
- **`CancelCtx` Acquire/Release** — correctness fix for v0.95 DAG cancel
  semantics (P1-6). Drop guard prevents leaked tokens.
- **Reserve NIKA-700..819** + `Category::Memory` / `WasmPlugin` / `Sandbox`
  / `Observability` — error-code real estate for v0.95+ subsystems.
- **Cost stdlib arithmetic** — `Add`/`Sub`/`AddAssign`/`SubAssign` with
  panic-in-debug, wrap-in-release semantics. `checked_add` / `checked_sub`
  for fallible callers.
- **Remove `TrustLevel::Default`** — safe-by-default inversion (P1-2).
  All trust must be explicitly stated.
- **`InferResponse.cost: Option<Cost>`** — structured cost replaces the
  deprecated `cost_usd` float. Provider-side cost tracking now type-safe.
- **Structured `DenialKind`** — replaces `CapabilityDenied { reason: String }`
  with enum variants (`FsReadNotGranted`, `FsWriteNotGranted`, `NetEgressBlocked`,
  `ExecBlocked`, `EnvReadBlocked`, `Custom`).
- **20 proptest lattice/identity laws** — cost commutativity, associativity,
  identity; trust lattice meet/join; baggage merge idempotence (integration tests).
- **MemoryId UUIDv7** — `MemoryId(u128)` → `MemoryId { uuid: Uuid }`.
  Time-sortable, standard format, `Display`/`FromStr` roundtrip.
- **`#[deprecated]` cost_usd** on `InferResponse`, `AgentOutcome`,
  `AgentCheckpoint` + `Cost::to_usd_f64()` bridge for deprecation window.
- **Pin zeroize=1.8** — workspace-wide version lock for `SecretString`.
- **cargo-mutants 88.5% kill rate** on nika-error L0 (cost/trust/baggage).
- Token count: 572 → **585 lib / 621 total** (+13 lib, +49 total).

### ⚡ Phase C Wave 2 — L0 foundational types + L0.5 traits (2026-04-16)

23 pure-data types landed in L0 crates, 6 kernel traits in L0.5, plus
forward-compat seams for v0.95 Cortex and v0.100 WASM.

- **23 L0 value types** across nika-error and nika-kernel — cost, budget,
  trust, retry, schema versioning, baggage, resource URI, content hash,
  memory frame, deny kind, cancel context, plugin DTOs, sandbox policy,
  observability event.
- **6 L0.5 kernel traits** — `IdGenerator`, `SecretResolver`, `MetricsExporter`,
  `TracerProvider`, `EventSink`, `BillingSink`. Sealed: `SecretResolver`,
  `EventSink`, `BillingSink`. Open: `IdGenerator`, `MetricsExporter`,
  `TracerProvider`. All have mock implementations in nika-kernel-mock.
- **Sealing pattern** — `Provider`, `EventSink`, `BillingSink`,
  `SecretResolver` now sealed via `mod sealed { pub trait Sealed {} }`.
  Open traits (`MemoryStore`, `EmbeddingProvider`, `ToolExecutor`) remain
  community-implementable.
- **Forward-compat seams** — `cancel.rs`, `plugin.rs`, `sandbox.rs`,
  `observability.rs` in nika-kernel. `MemoryFrame` gains reserved
  `Option<_>` fields (`cipher`, `provenance`, `retention`, `redactions`).
- **ADRs 016-020** — cancellation, streaming, runtime, retry, WASM
  (Batch F part 1). **ADRs 033-034** — L0/L0.5 expansion plans.
- Token count: 416 → **572** (+156 tests).

### ⚡ Phase D Session 2a — TOML-driven model capabilities (2026-04-14)

Zero-allocation capability resolver migrated from hardcoded Rust to a
TOML-driven rule table. Zero-alloc, proptest-verified, forward-compatible.

- **`data/model-capabilities.toml`** — 9 ordered rules covering OpenAI o-series,
  GPT-5, Claude family, Anthropic catch-all, DeepSeek reasoner, DeepSeek any,
  and xAI Grok-4. Schema `nika/model-capabilities@1.0`. First-match-wins
  semantics with build-time FK checks (providers must exist in
  `llm-providers.toml`, api_dialect must be in the closed dialect set).
- **`src/types/capabilities.rs`** — `CapPatch` (5 `Option<T>` fields,
  `const fn merge_with`, `fn materialize`), `Matcher` (Any/Exact/ExactAny/PrefixAny,
  zero-alloc `eq_ignore_ascii_case`), `Rule` (providers + api_dialect scope + matcher + caps).
- **`build/capabilities.rs`** — extracted from `build.rs` (380 LOC) to stay under
  the 1500-LOC-per-file budget. Validates TOML schema, FK checks, closed-set
  enum validation, all-None rule prevention, emits static Rust arrays at compile time.
- **`api_dialect`** — `Option<&'static str>` added to all 21 providers in
  `llm-providers.toml`. Closed set: anthropic / openai-chat / openai-responses /
  gemini / cohere / ai21 / bedrock / voyage / mock. Reserved for Session 2b+
  dialect-scoped rule authoring.
- **`supports_thinking` → `reasoning` rename** — aligns with 2026 industry
  convention (LiteLLM `supports_reasoning`, models.dev `reasoning`, OpenRouter
  `reasoning`). No compat shim (forever-v0.x nuke-and-rebuild).
- **`TokenLimitParam::MaxOutputTokens`** — variant added (OpenAI Responses API
  future-proofing). No rule maps to it yet; the `#[non_exhaustive]` enum can
  grow without a schema bump.
- **Proptest parity harness** — 10,000 random (provider, model) pairs compared
  against frozen legacy body in `mod parity_tests`. Regex widened to cover slash
  syntax, uppercase, underscore (HF-style), long names.
- **Insta snapshot** — 31 golden (provider, model) pairs reviewable under
  `src/data/snapshots/`.
- **Invariant #19 FULL** — 15 `new()` constructors across the crate (every
  `#[non_exhaustive]` public struct). Includes: `ProviderModel`, `Provider`,
  `ProviderModel`, `McpServer`, `Embedding`, `TransformDef`, `Builtin`,
  `EnvVarSpec`, `McpPackage`, `McpRemote`, `ModelCapabilities`, `ModelPricing`,
  `CostEstimate`, `ParseTagError`, `ParseCategoryError`, `Suggestion`.
- **Gate 8 GREEN** — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.
  8+ broken intra-doc links fixed across the crate.
- **5-agent review** — rust-architect + rust-pro + rust-perf + spn-nika +
  feature-dev:code-reviewer. All P0/P1 findings addressed in same session
  across 2 hardening commits.

### 🏷️ Phase D Session 1 — Tag vocabulary + Cargo features (2026-04-14)

Typed tag system for catalog entries, Cargo feature gating, and Shield
safety invariant enforcement.

- **42-variant `Tag` enum** (`#[non_exhaustive]`) — model I/O modalities,
  reasoning/generation behaviour, economics, deployment/sovereignty,
  specialisation, domain, and MCP-server permissioning. Kebab-case wire
  format (`Tag::as_str()` + `FromStr`). Locked as enum (not `&str`) so
  pck authors get compile errors on typos.
- **`tags` + `extra_tags` fields** on `Provider`, `McpServer`, `Embedding` —
  `&'static [Tag]` (validated at build time) + `&'static [&'static str]`
  (passthrough escape hatch for community-specific vocabulary).
- **All 139 catalog entries tagged** (21 providers + 13 embeddings + 105 MCP
  servers). build.rs enforces: known tags only, sorted, deduplicated, and
  MCP entries MUST carry exactly one of `read-only` / `destructive` (Shield
  security-filter invariant, compile-time enforced).
- **Cargo features for subset compilation** — `full` (default), `minimal`,
  `mcp`, `providers`, `embeddings`, `pricing`, `capabilities`,
  `builtins-transforms`, `extension-author`. Community crates depend on
  `features = ["extension-author"]` for types-only (no bundled data).
- **7 runtime tag invariant tests** — XOR, Budget/Frontier mutex,
  Embedding/Reranker presence, sort/dedup codegen integrity, spot-checks
  (anthropic tags, stripe MCP tags).
- **COMMUNITY_EXTENSIONS.md** — pck-author pattern documentation for
  `nika-catalog-cn`, `nika-catalog-eu`, etc.
- **3-agent review** (spn-nika + feature-dev + rust-pro) — all P0/P1
  findings addressed: `f64::INFINITY` validation gap, `#[allow(dead_code)]`
  scoping, `tag_variant` drift guard, `Tag::Sandbox` doc clarification,
  `extra_tags` Gate 1 SAFETY note, version pin fix.

### ⚙️ Hygiene + automation (2026-04-14 PM)

Autonomous ecosystem hygiene stack added to prevent drift over the 11-12 month build:

- **15-vector hygiene dashboard** (`scripts/hygiene/check-all.sh`) — MEMORY HEAD,
  crate count, LOC, CHANGELOG, ROADMAP, crate specs, Linear, GitHub milestones,
  org profile, CITATION, unwraps, file LOC cap, Claude coauthor leak, private
  path leak, cargo audit. Green/yellow/red table, exit codes 0/1/2.
- **Claude Code hooks** — PreToolUse blocks 5 dangerous ops (force push,
  `git add -A`, `cargo test --test`, checkout main, `--no-verify`); PostToolUse
  inspects HEAD commit for Claude coauthor + auto-runs hygiene on admissions;
  SessionStart injects grep-verified HEAD + crate count + hygiene state.
- **Skills** — `/gate-check` and `/crate-admit` for 12-gate discipline;
  `review-swarm.md` subagent for parallel 3-agent review (Gate 11).
- **CI workflows** — `hygiene-nightly.yml` (cron 3h UTC, idempotent drift issue),
  `forward-compat.yml` (cargo-public-api + cargo-semver-checks on PR),
  `changelog-cliff.yml` (auto-PR prepend CHANGELOG on tag push).
- **git-cliff config** (`cliff.toml`) — groups match content pipeline.

## [0.80.0-alpha.4] - 2026-04-14

### 🆕 Crate admitted: nika-catalog-verify

The immune system.

Where `nika-catalog` answers "what do we know?" in O(1) from compile-time data,
`nika-catalog-verify` answers "is what we know still true?" It probes real
package registries (npm, PyPI, Docker) and remote MCP endpoints in parallel,
producing a JSON drift report. Binary, not library — runs nightly from CI or
on-demand via `cargo run -p nika-catalog-verify`.

This is the second catalog crate and the first L4 binary admitted. It exists
because static catalogs decay: a package gets deprecated, an API endpoint goes
away, a provider renames a model. Without verify, the catalog silently rots.

Exempted from Gate 5 (mutation ≥90%) because binary I/O code produces
tautological mutations. Gate 10 (legacy parity) is N/A — this is new tooling.

| Metric | Value |
|--------|-------|
| LOC | ~600 |
| Tests | partial (logic only, I/O excluded) |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

Commit `a977e35b1`. 🦋

---

## [Previously Unreleased] — moved to 0.80.0-alpha.4

### 🔨 Refactors

- **nika-catalog Phase C migration** — migrating catalog data from hardcoded
  Rust arrays to `data/*.toml` source files, compiled at build time via
  `build.rs` + `phf_codegen`. Same zero-runtime-overhead phf maps, but the
  source of truth is now human-readable TOML. This unblocks community
  contributions to the catalog (PR a TOML file, not a Rust array).

### 🐛 Fixes

- **nika-catalog Phase A cleanup** (db0bf8e3f) — a 5-agent deep audit
  discovered 29 of our 131 MCP aliases were broken. Some pointed to
  Anthropic reference servers that were quietly deprecated ("Package no
  longer supported" on npm). Others referenced npm packages that never
  existed — Python-only tools, Go binaries, or names we'd fabricated from
  incomplete documentation. Three were community forks with zero weekly
  downloads.

  We removed all 29 and added a regression test (`removed_broken_aliases_not_present`)
  so they can't sneak back. The catalog went from 131 to 102 aliases.
  Every remaining alias now resolves to a real, installable package.

---

## [0.80.0-alpha.3] - 2026-04-13

### 🆕 Crates admitted: nika-kernel + nika-kernel-mock

The nervous system.

`nika-kernel` defines the **trait contracts for every side effect** in Nika.
It sits at L0.5 — above the pure types (error, catalog) and below the
implementations (fs, http, process, provider). Zero implementations live here.
This crate is the constitution: it says what each organ *must* do, not how.

The design follows Interface Segregation Principle to the max: ~20 fine-grained
atomic traits (`FsRead`, `FsWrite`, `HttpGet`, `ShellRun`...) grouped into ~6
super-traits of convenience (`Fs`, `HttpClient`, `ShellExecutor`, `Provider`...).
Consumers depend on exactly the surface they need — a context loader imports
`FsRead` alone, not the entire filesystem umbrella.

All async traits use `trait_variant` (Rust 1.91 native AFIT) instead of
`async_trait`. Zero boxing on the static dispatch path. The kernel carries no
tokio dependency — pure trait definitions that any async runtime can implement.

We also planted the **Cortex + agent-v2 hooks** now: `MemoryStore`,
`EmbeddingProvider`, `ToolExecutor`, `ContextCompressor`, and agent checkpoint
types. These won't be implemented until v0.95, but defining them in Phase 1
means we won't need breaking changes to `#[non_exhaustive]` structs later.
Forward compatibility bought cheaply.

`nika-kernel-mock` is the companion: deterministic mocks for every kernel trait
(`MockClock`, `InMemoryFs`, `MockHttp`, `MockShell`, `MockProvider`...).
Test hermeticity from day one — no test in Nika will ever touch a real
filesystem, a real network, or a real LLM provider.

| Metric | nika-kernel | nika-kernel-mock |
|--------|-------------|------------------|
| LOC | 3,369 | 1,731 |
| Tests | 99 | 88 |
| Mutation killed | 100% | 95.7% |
| Clippy warnings | 0 | 0 |
| Unwraps in src/ | 0 | 0 |

### Key decisions

- **Clock is SYNC, everything else ASYNC** — YAGNI on network time. Hot paths
  stay simple.
- **`BTreeMap` over `HashMap`** — deterministic iteration order, no hasher
  dependency. Tests are reproducible.
- **Cancel as `fn` param, not in struct** — keeps `ShellCommand` free of
  tokio-util. The kernel stays runtime-agnostic.
- **Provider = Infer + Stream + Meta** — all providers MUST stream (even mock).
  Embed and Vision are opt-in traits.
- **Errors per subsystem** — `ProviderError`, `ShellError`, `ToolExecError`,
  `MemoryError`. No god-enum.

All 12 gates passed. Commit `ef8804371`. 🦋

---

## [0.80.0-alpha.2] - 2026-04-13

### 🆕 Crate admitted: nika-catalog

The memory.

`nika-catalog` is Nika's static knowledge of the world: every LLM provider it
can talk to, every MCP server it knows how to install, every builtin tool it
ships, every pipe transform it supports, and the pricing of every model it's
seen.

The catalog is compiled into the binary at build time. No runtime I/O, no
config files, no network calls. You ask "do you know `anthropic`?" and the
answer comes back in O(1) via a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function).

Why this matters: when a user writes `provider: claude` in their YAML, the
engine resolves the alias → canonical provider → model → capabilities → pricing
in a chain of zero-allocation lookups. No guessing, no fuzzy matching, no
"did you mean?" The catalog is the ground truth.

The lookup strategy is hybrid by design:
- **phf + unicase** for case-insensitive lookups (providers, MCP aliases) —
  because users write `Claude`, `claude`, `CLAUDE` and they all mean Anthropic.
- **Sorted arrays + binary_search** for case-sensitive lookups (builtins,
  transforms) — because `nika:read` and `nika:Read` are different things
  (actually `nika:Read` doesn't exist, and the catalog should say so clearly).

At admission: 16 providers, 105 MCP aliases, 63 builtins, 65 transforms,
61 model pricing entries. All from a single `cargo build`.

| Metric | Value |
|--------|-------|
| LOC | 2,235 |
| Tests | 85 |
| Mutation killed | 94.7% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `55a451695`. 🦋

---

## [0.80.0-alpha.1] - 2026-04-13

### 🆕 Crate admitted: nika-error

The DNA.

Every error in Nika carries a code. `NIKA-001` means schema validation failed.
`NIKA-053` means a blocked command was attempted. `NIKA-382` means a canary
token leaked (prompt injection detected). There are hundreds of these codes,
and every single one must roundtrip through Display, parse back from a string,
serialize to JSON, and match the exact same format across every provider, every
verb, every transport layer.

`nika-error` is the crate that makes this possible. It defines:

- **`NikaErrorCode`** — a trait that every per-crate error enum must implement.
  This is the contract: if you want to be a Nika error, you carry a code, a
  severity, a category, and you format yourself as `"NIKA-XXX: message"`.
- **`NikaError`** — a `Box<dyn NikaErrorCode>` wrapper. The unified error type
  that flows through `?` propagation across the entire codebase.
- **`NikaCode`** — the code itself. Dual format: Display gives you `"NIKA-140"`,
  serde gives you `{"num":140,"category":"ast","severity":"error","slug":"ast-analysis-failure"}`.
- **`CoreError`** — cross-cutting errors that don't belong to any specific crate
  (Validation, NotFound, Unsupported, Internal).

This is the L0 anchor. Zero `nika-*` dependencies. Reachable from every crate
in the workspace. The first cell of the organism.

It also resolves **shadow zone 6** from the pre-launch audit: every admitted
`NIKA-XXX` now ships with a Display parity golden test against the legacy
format. No silent drift.

| Metric | Value |
|--------|-------|
| LOC | 1,013 |
| Tests | 44 |
| Mutation killed | 100% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `42909b1c7`. 🦋

---

## [0.80.0-alpha.0] - 2026-04-13

### The beginning

Orphan branch `nika-diamond` (renamed `main` on 2026-05-06) created from scratch. No code inherited from legacy.
Clean slate, edition 2024, Rust 1.91.

From the start, the workspace enforces:
- `clippy::unwrap_used = "deny"` — zero unwraps, everywhere, always.
- `clippy::panic = "deny"` — if it can panic, it doesn't compile.
- `clippy::expect_used = "warn"` — we'll get there.

32 legacy crate directories excluded via `.gitignore` — they exist on disk
(the orphan branch inherits the working tree) but cargo ignores them. We read
legacy code via `git show main:path/to/file.rs` when we need guidance, but
nothing is copied verbatim. Every line is rewritten.

The organism's skeleton is in place. Now it grows. 🦋

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.4...HEAD
[0.80.0-alpha.4]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.3...v0.80.0-alpha.4
[0.80.0-alpha.3]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.2...v0.80.0-alpha.3
[0.80.0-alpha.2]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.1...v0.80.0-alpha.2
[0.80.0-alpha.1]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...v0.80.0-alpha.1
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
