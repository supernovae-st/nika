- **One strict admission for authoring knowledge on every door, against a trusted identity.**
  `nika compile`, the Session and `nika serve` admit a Foundry knowledge release
  (`nika-knowledge-release/2`, profile `nika-knowledge-release-profile/r1`, the contract the
  producer shares, with the same vectors) or refuse it whole with one typed cause. The host
  names the identity it trusts for the release (its manifest's sha256 and its policy) from its
  own release record; without one nothing is collected, so a root `--knowledge` or
  `NIKA_KNOWLEDGE` names is refused (`ADMISSION_UNTRUSTED`) until a qualified identity source is
  wired. The release is collected on held directory descriptors (the root's final component and
  every entry beneath it are never followed as links, the root's ancestors are outside this check;
  no FIFO blocking; Unix only) in a closed layout, bounded before any read; its manifest is closed
  and pins every file; every row is a canonical line of its kind's closed schema pinned to the
  target; only a block claims CHECKED, with a check receipt bound to the verifier that checked its
  bytes; lineage names retained sources; relations, licence texts and notices close. No permissive
  reader remains, nothing is read in part, and a historical snapshot directory is refused. A pack
  composed elsewhere (`--knowledge-pack`, `NIKA_KNOWLEDGE_PACK`) is refused before it is read.
  `--no-knowledge` (now on `nika serve` too) and the exact word `NIKA_KNOWLEDGE=off` turn the
  knowledge off; off beside a source on the same settings layer is refused; the explicit layer
  wins over the environment. With nothing named and authoring enabled, the
  default is an embedded release of three patterns and three workflow blocks,
  admitted against an identity built into Nika. Recall follows matching
  patterns to their blocks; an intent with no matching words gets no references.
  A named source that fails admission never falls back to this default.
  The pack builder is `nika-compile/knowledge-door-v4`. For Rust callers this
  is a breaking change in the 0.122 minor: `Snapshot::open` and `KnowledgePin::open` take the
  trusted identity (`None` is refused), `KnowledgePin::dir` is replaced by `origin`
  (`KnowledgeOrigin::Disk` or `Embedded`), `KnowledgeSource::Snapshot` carries `identity`,
  `KnowledgeError::NotASnapshot` and `KnowledgeError::Stale` are removed. Strict snapshot-admission
  refusals use `KnowledgeError::Unavailable` with its code. `CompileArgs` and `NativeAuthoringArgs`
  gain `no_knowledge`; callers constructing these structs must supply it.
