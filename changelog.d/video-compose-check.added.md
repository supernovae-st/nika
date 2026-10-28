- **`nika check` judges `nika:video_compose` before anything runs.** The
  31st builtin (stdlib §Media graduate #6) joins the catalog, the tool
  definitions and `nika explain`, which teaches its eleven codes
  `NIKA-BUILTIN-VIDEO_COMPOSE-001..011` with their categories. Every media
  input (each `segments[k].image`, each `segments[k].audio`, the top-level
  `audio` · a path, or a producer's `{ path, sha256 }` object) is an
  `fs.read` the check judges, infers and counts as a use, and `out:` an
  `fs.write`; a side that admits no path refuses the call whatever form its
  paths take, and an absent block refuses it with `NIKA-AUTH-006`. No
  `permits.exec` is required or implied. Every literal argument is judged as
  the run parses it, from one table: the closed argument set and segment
  map, both keys of a media reference and a digest of 64 lowercase hex
  digits, 1 to 200 segments, the duration and pad rules, even sides inside
  64..=3840 × 64..=2160, the closed sets (`transition: fade` is reserved) and
  `metadata` up to 16 KiB as canonical JSON. The trifecta counts the call as
  an fs-write sink and never an ingress source, its outputs keep the taint of
  what they echo, and the order law keeps binding `exec:` only. The plan and
  the review name the program the call starts — the host's ffmpeg, from
  `NIKA_FFMPEG` or three fixed paths, 6.0 or later with libx264, under the
  exec sandbox — statically, without probing the host and without a network
  claim. `nika test` refuses it like every other media effect. This build
  carries no video backend: a run judges the same arguments, then stops
  with `NIKA-BUILTIN-VIDEO_COMPOSE-010` before it reads, stages or writes
  anything.
