- **`nika:image_compose` assembles several PNG images into one.** The 30th
  builtin (stdlib §Media graduate #5) lays an ordered list of 1 to 64 inputs
  out as a `grid` (columns and rows derived from the count when absent), a
  `row` or a `column`; each image keeps its own size, is scaled to fit its
  cell with the aspect kept (`fit`), or is scaled and center-cropped to fill
  it (`fill`), with `gap`, `padding` and a `background` colour. Like
  `nika:image_fx` it is pure, zero-dependency pixel work: the same input
  bytes and args give the same PNG bytes, the `image_compose/v1` recipe rides
  the tEXt chunk, and the output carries path, sha256, width and height,
  never bytes. Every input is an `fs.read` that `nika check` judges, infers
  and counts as a use, and `out:` an `fs.write`; caps on input count, summed
  input pixels and canvas size fail `NIKA-BUILTIN-IMAGE_COMPOSE-005` from the
  PNG headers before any decompression, and each of the six codes has its
  `nika explain` lesson. A contact sheet no longer needs `exec magick
  montage`.
