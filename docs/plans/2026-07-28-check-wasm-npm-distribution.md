# nika-check-wasm · distribution — the checker becomes an npm package

> Status: decision + implementation plan. Every claim below was measured
> against the tree or the live registry on 2026-07-28 (commits, tags, npm
> versions cited inline). The crate itself was admitted the same day
> (ADR-107 · 12 gates); this document is about how its artifact leaves
> the repo, not about what the crate is.

Companions: `adr/adr-107-nika-check-wasm-admission.md` (the admission),
`crates/nika-check-wasm/README.md` (build + differential gates),
`RELEASING.md` (the release train this plan extends).

---

## 0 · Why this document exists

The browser half of `nika check` is merged and live on nika.sh, which
vendors the built artifact by hand (a pinned `pkg/` + PROVENANCE record).
That works for one consumer. The moment there are two — docs playgrounds,
third-party CI, anyone's editor — hand-vendoring becomes N private copies
of a build nobody can verify. The distribution answer must preserve the
property the admission fought for: **one source of truth, provable
lineage, no second assembly that can drift.**

Two questions were asked, and both get their answer here:

1. **npm?** Yes — `@supernovae-st/nika-check-wasm`, published by the
   engine's own release workflow with npm provenance attestations.
2. **A new ecosystem repo, like nika-client?** No — and the reason is a
   philosophy, not a convenience (§2, Q2).

---

## 1 · The version-honesty finding (what forced a patch release)

The naive move — "publish 0.106.0 to npm now, from `main`" — is a
lineage lie, measured:

- `v0.106.0` was tagged and released **2026-07-27** (GitHub release
  `Latest`).
- `main` has moved since: `a77078707` (`fix(check): the audited card
  stops calling a ceiling a floor`) landed **after** the tag and changes
  checker output.
- The wasm crate itself is not in the `v0.106.0` tree at all — it merges
  after.

So an npm `0.106.0` built from `main` would disagree with the released
`0.106.0` binaries on real verdicts — exactly the drift class the
differential gates exist to kill. The npm-pack script therefore makes the
lie unrepresentable rather than merely forbidden: an untagged checkout
(`git describe --exact-match`) packs as an explicit `-dev.g<sha>`
prerelease that cannot impersonate a release, and the publish step
refuses any `-dev` tarball outright. The debut ships as:

**`v0.106.1`** — a patch tag cut from `main`, carrying the wasm crate,
the audited-card fix, and the resource-algebra research docs. Binaries
and npm package are born from the same tree, and every surface that says
"0.106" means the same commit family.

---

## 2 · The socratic pass (ten questions, ten verdicts)

**Q1 — panic strategy: `abort` or unwind?** `abort`. The house doctrine
is already "a trapped wasm instance never comes back" (the jq-bomb work
made that literal: the shadow stack does not restore). Unwinding
machinery in the artifact would be dead weight that *implies* recovery
we refuse to promise. Measured: no `catch_unwind` anywhere in the wasm
dependency tree (only a doc comment in `nika-infer-local`, which is not
in-tree for wasm). Shipped: `panic = "abort"` on `[profile.wasm-release]`.

**Q2 — a dedicated repo, like nika-client?** No. `nika-client` earns its
repo because it *houses source* — hand-written TypeScript that exists
nowhere else. A `nika-check-wasm` repo would house only generated
artifacts (wasm-bindgen output), i.e. a second assembly able to drift
from the crate — the exact shape ADR-107 declined twice (typed verdict,
span clamping). The served artifact must stay a *projection* of the
engine tree. If a showcase repo is ever wanted, it can graduate later
without breaking anyone: the npm name does not move.

**Q3 — ship a typed TS wrapper over the verdict?** No runtime wrapper
(second-assembly law again). The generated `.d.ts` ships as-is; the row
shape is documented by the spec's report format, which is the SSOT the
README points at. A hand-maintained interface would be a drift surface
with no gate.

**Q4 — one build target or two (web + node)?** One: `--target web`.
Node ≥ 18 consumes the same glue via explicit bytes (`init({
module_or_path })` — necessary because Node's `fetch` refuses `file://`
URLs, so the glue's self-loading path is browser-only) — proven daily by
the site's own oracle tests, which run the web-target artifact under
Node and byte-compare rows against fixtures. The field standard among
the big wasm shippers is separate `-web`/`-nodejs`/`-bundler` packages;
that is a door we can open later without breaking anyone (§7), but a
second artifact family to attest is not paid for by any consumer we
have today.

**Q5 — size diet now?** No semantic diet, ever: any change that alters
verdicts is caught by the differential legs, and "smaller but different"
is a regression, not a diet. The honest levers shipped now: `panic=abort`,
`wasm-opt -Oz`, plus the two strip passes the adversarial review measured
safe (`--strip-producers --strip-target-features` — the artifact's only
custom sections, 269 bytes of toolchain disclosure; a `--converge` pass
was measured at −39 bytes for +14 s wall and declined). The size anatomy
is now known, not guessed: ~7.7% of the artifact is the IANA timezone
database (`jiff ← jaq-std ← nika-check` — the jq stdlib's date filters),
and trimming it would change which jq programs compile — a semantic
change, locked behind both differential legs and the native CLI moving in
lockstep (§7).

**Q6 — cap wasm memory (`--max-memory`)?** No. An alloc failure inside
the instance panics → traps → poisons the instance; that is a worse
failure mode than a large tab. A JS-side input cap was then considered —
and **declined on measurement** (Node, the shipped artifact, 2026-07-28):
a 623 KB workflow of 20 000 tasks checks in 68.5 ms, a 4 MB single-string
value in 12.5 ms — linear, no cliff, an order of magnitude under any size
a human or agent pastes. A cap would be a guessed guard against a cost
that does not exist; the depth-shaped attacks are already refused at the
source (jq nesting 128, suggestion budget 256). The README documents the
measured scale instead.

**Q7 — npm auth: token or trusted publishing?** What ships today is the
`NPM_TOKEN` path, stated plainly: the publish step checks for the token,
publishes the tarball with `--provenance` when it exists, and
**loud-skips** when it does not — still attaching the exact `npm pack`
tarball (+ sha256) to the GitHub release, so a credential-less release
ships the artifact and its proof, publishable later without a rebuild.
Trusted publishing (OIDC, zero long-lived secret) stays the destination,
but it is a §7 named follow-up, not a claim: the runner's stock npm does
not negotiate OIDC, and wiring an npm upgrade into the release train
deserves its own verified pass rather than a hopeful branch here.

**Q8 — publish cadence?** Lockstep with the engine: every release tag
publishes the matching npm version. The version is never typed anywhere
— it is projected from the crate manifest, and the existing release gate
(manifest must equal tag) makes a mismatch unshippable.

**Q9 — what goes in the package?** The minimum that runs: glue JS,
`.d.ts`, the optimized `.wasm`, README, LICENSE. No fixtures, no build
scripts, no tests — provenance and the engine repo carry the proof. The
publish directory is assembled from scratch on every pack; nothing in it
is checked in.

**Q10 — reproducibility (and the leak it hid)?** `--locked` everywhere
(already law), the release job pins its toolchain (`wasm-bindgen-cli` at
the lock's minor, binaryen by sha), **and the paths are remapped** — the
adversarial pass measured 266 absolute host paths embedded in the
artifact as panic-Location strings, including the building machine's
private directory layout, shipped whether or not a panic ever fires.
`--remap-path-prefix` ×2 (cargo home → `/cargo`, workspace → `/build`)
zeroes them — verified by rebuild, with verdict rows proven
byte-identical across the remap — and removes the machine variable from
the bytes at the same time (`trim-paths` was tried first and is not on
stable 1.91). Attested builds remain the outer guarantee, same as the
rest of the release.

---

## 3 · The artifact (what `npm install` gets)

```
@supernovae-st/nika-check-wasm/
├── nika_check_wasm.js        wasm-bindgen web-target glue (ESM)
├── nika_check_wasm.d.ts      generated types
├── nika_check_wasm_bg.wasm   the checker (wasm-opt -Oz, panic=abort)
├── README.md                 quickstart + gates + honesty contract
└── LICENSE                   AGPL-3.0-or-later (copied from the tree)
```

`package.json` is **projected, never authored**: `npm-pack.sh` reads
`cargo metadata` for name, version, description and license, and writes
the manifest at pack time. There is no hand-maintained template to
drift. Key fields: `"type": "module"`, an `exports` map (`.` → types +
default, plus a passthrough export for the raw `.wasm`), `sideEffects:
false`, `publishConfig: { access: "public", provenance: true }`.

The license is AGPL-3.0-or-later like the engine it is compiled from —
that is the anti-extraction stance, unchanged by the compilation target.
The README says so plainly and points at the source.

In-band honesty is unchanged from the admission: the verdict carries
`"wasm": true` and the closed `"legs": ["PARSE", "CONFORM"]` — a browser
checker that cannot run the environmental legs must not claim them.

---

## 4 · The release train (what the machine does per tag)

`release.yml` gains two jobs, `npm-wasm-pack` → `npm-wasm-publish`,
alongside the existing build → release → provenance → bump-formula →
docker chain — **two**, because the supply-chain pass caught the shape
the file's own build → release split exists to prevent: the packing job
compiles third-party code (`cargo install`, build scripts, proc macros),
and compiled code must never share a job with `id-token: write` or a
writable token — a hostile `build.rs` there could mint Sigstore
attestations in the repo's name. So the pack job holds `contents: read`
with `persist-credentials: false` and hands the tarball over as an
artifact; the publish job holds the elevated scopes and runs zero
third-party code. Between them, the packed version is asserted equal to
the released version **before** anything is uploaded or attested, and
every downstream checkout uses the sha the release job resolved once —
never a re-resolution of the (mutable) tag:

1. Check out the tag (same discipline as the build matrix).
2. `rustup target add wasm32-unknown-unknown`; install `wasm-bindgen-cli`
   at the Cargo.lock minor; install pinned binaryen (wasm-opt is
   **mandatory** here — the local script tolerates its absence, the
   release does not).
3. `crates/nika-check-wasm/npm-pack.sh` — tests (against the spec at
   `SPEC_PIN`, checked out like diamond-ci's legs), builds `wasm-release`,
   binds, optimizes, assembles the publish dir, projects `package.json`,
   `npm pack` → tarball + sha256 (bare-name sidecar — release assets
   attach flat). An untagged tree packs a `-dev.g<sha>` prerelease.
4. Publish with `--provenance` (OIDC id-token) when credentials exist;
   otherwise loud-skip with the instruction in the step summary.
5. Attach tarball + sha256 to the GitHub release either way.

The site keeps vendoring (CSP self-containment + offline builds), but
its PROVENANCE record gains the npm coordinate once published, and
re-vendoring becomes "unpack the release tarball" instead of a local
build — the artifact consumed is the artifact attested.

---

## 5 · The 0.106 alignment train (surfaces, in order)

| Surface | State measured 2026-07-28 | Action |
|---|---|---|
| engine | `v0.106.0` released 07-27; wasm merges post-tag | cut **`v0.106.1`** per RELEASING.md (changelog splice → bump → tag push) |
| npm (new) | package does not exist | debuts at `0.106.1` from the tag (§4) |
| homebrew | auto (`bump-formula` job) | nothing — follows the tag |
| docker | auto (`docker` job) | nothing — follows the tag |
| nika.sh | vendors a pre-admission build from the feature branch | re-vendor from the `v0.106.1` artifact; PROVENANCE → tag + main sha |
| nika-client | `0.105.0` on npm | its own repo's ceremony: coverage gate against 0.106, then version-align release |
| docs.nika.sh & friends | no wasm mention | front-doors cascade (drafts exist; maintainer merges) |

---

## 6 · What only the operator can do

One move: add an `NPM_TOKEN` secret to the engine repo — a **granular**
token for the `@supernovae-st` scope (classic tokens no longer exist;
granular write tokens expire within 90 days, which is fine for what this
is: a bootstrap, since npm's own tracker says a package's first-ever
publish cannot use OIDC anyway). Everything else in this plan is
automated and lands without it — tarball-on-release is the no-credential
fallback. After the debut, the §7 OIDC follow-up retires the token
class entirely; the publish job already pins the OIDC-capable toolchain
(node 24 → npm ≥ 11.5.1, the component that generates the provenance).

---

## 7 · Named follow-ups (not this plan)

- **Wasm diet, differential-gated**: the anatomy is measured (§2 Q5 —
  ~7.7% is the IANA tzdb riding `jiff ← jaq-std`); any trim changes
  which jq programs compile, so it moves the native CLI and the wasm in
  lockstep behind both differential legs, or it does not move at all.
- **npm trusted publishing (OIDC)**: upgrade the job's npm to an
  OIDC-capable version (the runner's stock npm is too old), configure
  the package on npmjs.com, retire the long-lived token — its own
  verified pass (§2 Q7). Note the sequencing constraint from npm's own
  tracker: a package's **first-ever** publish cannot use OIDC, so the
  token bootstrap this plan ships is required regardless.
- **A `-nodejs` target package**: the field standard is per-target
  packages from one crate; ours stays single web-target + documented
  bytes-init until a Node consumer who can't pass bytes actually
  appears (§2 Q4).
- **Remaining static legs**: extract the CLI's other non-environmental
  legs behind the seam the crate already documents — never re-assembled.
- **`nika test` in wasm**: the measured wall is the runtime tree
  (tokio ×7, mio, reqwest); the kernel-mock path has zero async/io and
  is the door.
- **Showcase repo**: only if a real audience appears; the npm name is
  stable either way (§2 Q2).
