#!/usr/bin/env bash
# build-wasm.sh · the browser lot for nika-tui-core (law ② · --target web).
#
#   · wasm-pack builds (--target web · the published browser shape)
#   · wasm-opt from a CURRENT binaryen (law ④ · the bundled one predates
#     this rustc's wasm features and exits 1 on them)
#   · raw + gzip sizes printed (law ③ · a regression is visible)
#   · the built pkg is EXERCISED by the Node harness (law ⑧ · test.mjs
#     replays the parity fixtures through the wasm boundary)
set -euo pipefail
cd "$(dirname "$0")"

wasm-pack build --target web --out-dir pkg
# the Node harness rides its own target (law ② · the web pkg init()s by
# fetch(), which Node cannot do on a file path — measured)
wasm-pack build --target nodejs --out-dir pkg-node

wasm-opt -O3 pkg/nika_tui_core_bg.wasm -o pkg/nika_tui_core_bg.wasm

raw=$(stat -f%z pkg/nika_tui_core_bg.wasm)
gz=$(gzip -c pkg/nika_tui_core_bg.wasm | wc -c | tr -d ' ')
echo "wasm · raw ${raw} B · gzip ${gz} B"

node test.mjs
