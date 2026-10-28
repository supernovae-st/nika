- **The absent-block repair of `nika:image_fx` and `nika:image_compose`
  names both sides.** Under an absent `permits:` block, `NIKA-AUTH-006`
  said to add the tool and "the path to permits.fs", which named the output
  and no side, so following it as worded still refused the input read. The
  repair now sends every path the call reads to `permits.fs.read` and the
  path it writes to `permits.fs.write`, and points at
  `nika check --infer-permits` for the exact block.
