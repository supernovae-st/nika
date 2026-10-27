- **The embedded language pack follows nika-spec `5cd95cd`.** `SPEC_PIN`
  and `crates/nika-pack/pack` advance together from `c5dcd97`, the commit
  0.123.2 embeds. nika-spec#397 (`aac8de5`) adds the contract of
  `nika:image_compose` and the keyless `system` voice: `canon.yaml` counts 30
  builtins, the schema's builtin enum and the stdlib gain
  `nika:image_compose`, its caps list each input side, the summed pixels,
  each scaled box and the canvas, the `system` voice speaks its text as text
  with its audio capped at 64 MiB, and the `image-compose-sheet` example
  joins the pack. nika-spec#399 (`ebc6448`) makes the images an agent
  application delivers on a seat a value of the `agent:` task: the schema
  gains `images: { save: }`, chapter 02 its §Harness images, chapter 17 the
  `saved_path` of a saved image and the error registry `NIKA-AGENT-007` (112
  codes), and a seat's `infer:` meets a JSON or JSON Schema need when the
  engine validates its answer. nika-spec#400 (`479b0bc`) makes an `infer:`
  `vision:` file a read under `permits.fs.read` (`NIKA-AUTH-006` under an
  absent block, `NIKA-SEC-004` outside a declared one, and a refusal at
  check, whatever the path's form, under a boundary that admits no read; a
  `source: url` entry needs no grant), makes every `vision:` entry a
  provider-egress sink of its task (`NIKA-SEC-006`, `NIKA-SEC-009`), and
  bounds an `images: { save: }` to four files and names it in the same-path
  law (`NIKA-SEC-012`). nika-spec#401 (`5cd95cd`) lets the `cost_usd` of
  `nika:image_generate` and `nika:tts_generate` come from the engine's dated
  price catalog, marked by `cost_basis` with the rates' date in
  `cost_as_of`, and keeps it `null` when neither the provider's report nor
  the catalog yields a figure.
  <!-- TODO(0.124.0): #1851 merged as 14073c081 and set this pin, so the
  rebased branch carries `5cd95cd` with no pin move of its own. After the
  rebase, delete the last sentence of `vision-file-reads.security.md` (or
  the #400 sentence here), which says the same, then delete this comment. -->
