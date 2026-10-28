- **`nika check` no longer judges a templated `nika:image_compose`
  `inputs` list as a file path.** A whole `inputs: "${{ const.frames }}"`
  naming a string was refused as a read outside `permits.fs.read`, and
  `--infer-permits` printed that string as a read grant, although the run
  refuses such a value before it reads anything. The list is now the run's,
  as every templated list is, and only a side that admits no read refuses it
  at check. A literal `inputs` that is not a list keeps its
  `NIKA-BUILTIN-001` and no longer adds a `NIKA-SEC-004` or a read grant
  beside it.
