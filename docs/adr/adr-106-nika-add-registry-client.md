---
id: ADR-106
title: "nika add — the registry client verb (resolve · verify · install)"
status: proposed
date: 2026-07-06
phase: "post-0.95 · registry track"
deciders: ["@ThibautMelen"]
tags: ["nika-cli", "registry", "supply-chain", "content-addressed", "sovereignty", "conformance-as-trust", "lockfile"]
affects_crates: ["nika-cli", "nika-blob", "nika-schema"]
affects_layers: ["L1", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-092", "ADR-099", "ADR-105"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-REG-001", "NIKA-REG-002", "NIKA-REG-003", "NIKA-REG-004", "NIKA-REG-005"]
timeline: "design locked 2026-07-06 · implementation post-launch train"
follow_ups: ["nika sign (minisign · registry-v0.2 signature block)", "nika publish (pack + entry scaffold)", "model pointers via the blob CAS"]
---

# ADR-106: nika add — the registry client verb (resolve · verify · install)

## Context

The sharing contract is normative (nika-spec `registry/registry-v0.1.md`)
and the reference registry is live and CI-proven
(supernovae-st/nika-registry): entries pin an author's artifact by full
commit + full sha256, CI re-proves hash + conformance + secret-scan, and
the engine's own static certificate (exec · tools · cost · permits) is
projected per entry. Today the consume path is the registry's `get.py` —
correct but external: it requires a repo clone and Python. The engine owes
the native verb the contract was designed for. Every mechanism `nika add`
needs already exists in the binary: the check ladder (ADR-092) proves
artifacts locally, `nika-blob` is a blake3 CAS, and the run journal
(ADR-099) set the dot-dir conventions.

## Decision

Ship `nika add <ref>` as a THIN client of the registry-v0.1 contract —
the four `get.py` steps, native, offline-verifiable, zero trust in any
index:

```
nika add meeting-actions                      # resolve in the default index
nika add acme/price-watch@1.2.0               # publisher-scoped + pinned version
nika add --index github.com/acme/registry x   # org/private index (a git repo)
nika add --dry-run <ref>                      # print the cert · install nothing
```

1. **Resolve** · fetch the index's `index.json` (contract §4) · match
   `[publisher/]name[@version]` · newest SemVer wins ties. An
   LLM-suggested name that resolves nowhere fails loudly
   (`NIKA-REG-001` · the slopsquatting guard the contract mandates).
2. **Refuse on advisory** · a matching advisory (contract §3) refuses
   BEFORE any bytes move (`NIKA-REG-002`) — override only with an
   explicit `--yanked-ok` (mirrors the contract's MUST-refuse).
3. **Fetch + verify** · raw-fetch `source.repo@rev:path` (https only ·
   1 MB cap) · `sha256(bytes)` MUST equal the entry digest
   (`NIKA-REG-003` on mismatch · nothing is written). The blob lands in
   the `nika-blob` CAS keyed by digest; the workfile is a link/copy into
   the cwd — two installs of the same artifact share one blob.
4. **Audit + hand off** · run the ADR-092 ladder on the verified bytes
   (the local re-proof the contract centers on) · print the certificate
   summary (exec flag + cost bound first — the contract's display rule) ·
   the next-step hand-off is CERT-DRIVEN (llm-only → mock preview ·
   fetch-only → "set vars: first" · the get.py lesson, kept).
5. **Record** · append to `nika.lock` (TOML · ref → index · entry digest
   · artifact sha256 · installed path). `nika add` with a lockfile entry
   re-installs byte-identical or fails (`NIKA-REG-004`); a tampered or
   vanished index cannot change what a locked ref resolves to.

Structural refusals, all before write: install-time execution does not
exist (artifacts are data — there is nothing to execute by construction);
an entry whose schema the client does not understand is refused
(`NIKA-REG-005` · unknown-field = smuggling channel, same closed-set law
as the contract's R-rules).

## Consequences

- The registry story completes: publish (PR to an index) · discover
  (index.json / llms.txt) · install (`nika add`) · verify (every step
  re-provable offline). One binary does the whole consumer side.
- `nika.lock` makes shared workflows reproducible across a team the way
  Cargo.lock does — and makes index compromise a non-event for existing
  installs (hashes pin, names do not).
- A new error namespace (NIKA-REG) enters spec/05 — additive.
- Org/private registries work day one: `--index` takes any git-hosted
  index implementing the contract; nothing distinguishes ours.
- Deliberately NOT in v1 of the verb: signature verification (waits for
  the registry-v0.2 `[signature]` block + the operator key ceremony) ·
  transitive pack dependency resolution (packs are v0.2 registry
  territory) · model-pointer downloads (GB blobs · the CAS is ready but
  the UX needs its own pass).

## Alternatives

- **Keep get.py only** — rejected: a Python-and-clone dependency for the
  ecosystem's front door contradicts "single static binary", and the
  engine already contains every verification primitive.
- **A separate `nika-registry` binary** — rejected: the check ladder IS
  the trust layer; splitting it duplicates the oracle or weakens the
  client.
- **Full lockfile-first design (à la cargo add + resolution)** —
  deferred: workflows have no transitive graph in v0.1 (packs will);
  a flat lock covers the reproducibility need today.
- **Auto-update semantics (`nika add --upgrade`)** — deferred to the
  first real upgrade pain; immutable entries make upgrades explicit
  new-version installs by construction.

## Related

- nika-spec `registry/registry-v0.1.md` — the contract this implements.
- the sovereign-hub canon (identity ⊥ discovery · conformance-as-trust) —
  studio-internal strategy; the public contract it produced is the
  nika-spec registry chapter above.
- ADR-092 (check ladder · the local re-proof) · ADR-099 (dot-dir + trace
  conventions) · supernovae-st/nika-registry `scripts/get.py` (the
  behavioral reference · cert-driven hand-off included).

## Notes

Numbering: 106 allocated against both homes + index.toml (105 = engine
image_generate · 104 = spec provider catalog). Status `proposed` per the
status law — flips to `accepted` when the verb ships.
