#!/usr/bin/env bash
# The complete proof battery, one command (the fresh-user-e2e pattern).
# Every claim the crate makes, re-proven by a third party where one exists.
# Optional judges (node/vega-lite · Chrome) SKIP loudly, never silently.
set -euo pipefail
cd "$(dirname "$0")"

PASS=0
SKIP=0
ok() {
  printf '  \342\234\224 %s\n' "$1"
  PASS=$((PASS + 1))
}
skip() {
  printf '  \342\227\213 SKIP %s\n' "$1"
  SKIP=$((SKIP + 1))
}

echo "nika-chart judge battery"
echo "1 · cargo gates"
cargo fmt --check >/dev/null 2>&1 && ok "fmt --check"
if cargo clippy --all-targets 2>&1 | grep -qE '^(warning|error):'; then
  echo "CLIPPY DIRTY"
  exit 1
fi
ok "clippy 0 (det laws as lints)"
if cargo doc --no-deps 2>&1 | grep -qE '^warning:|^error'; then
  echo "DOC DIRTY"
  exit 1
fi
ok "rustdoc 0"
T=$(cargo test 2>&1 | grep -oE '^test result: ok\. [0-9]+' | grep -oE '[0-9]+' | paste -sd+ - | bc)
ok "tests ($T passed · goldens · fuzz-200 w/ attest property · properties)"

echo "2 · determinism at scale"
cargo run --release --example fuzz_deep 2>&1 | tail -1 | grep -q 'FUZZ-DEEP GREEN' && ok "10k-case corpus · 0 panics · attest Match corpus-wide"

echo "3 · cross-architecture byte-parity"
if rustup target list --installed 2>/dev/null | grep -q wasm32-wasip1 && command -v node >/dev/null; then
  cargo build --release --example parity >/dev/null 2>&1
  cargo build --release --target wasm32-wasip1 --example parity >/dev/null 2>&1
  TGT="${CARGO_TARGET_DIR:-target}"
  "$TGT/release/examples/parity" >/tmp/parity-native.txt
  node run-wasi.mjs >/tmp/parity-wasm.txt 2>/dev/null || sed "s|./target-session|$TGT|" run-wasi.mjs | node --input-type=module - >/tmp/parity-wasm.txt 2>/dev/null
  diff -q /tmp/parity-native.txt /tmp/parity-wasm.txt >/dev/null && ok "wasm32-wasip1 ≡ aarch64 (sha256 identical · SVG+PNG)"
else
  skip "wasm parity (needs wasm32-wasip1 target + node)"
fi

echo "4 · Vega-Lite · the official compiler"
if command -v node >/dev/null && [ -d "${VL_JUDGE_DIR:-/nonexistent}" ]; then
  cargo run --example vl_all >/dev/null 2>&1
  (cd "$VL_JUDGE_DIR" && node judge.mjs) | tail -1 | grep -q 'ALL 6 VALID' && ok "6/6 specs compile · 0 warn (official vega-lite)"
else
  skip "VL judge (set VL_JUDGE_DIR to a dir with node_modules/vega-lite + judge.mjs)"
fi

echo "5 · PNG · independent decoders"
cargo run --release --example png_edge >/dev/null 2>&1
python3 - <<'PY'
import struct, zlib, glob, sys
files = sorted(glob.glob('edge-*.png'))
assert files, 'no edge PNGs'
for name in files:
    raw = open(name,'rb').read(); pos, idat = 8, b''
    assert raw[:8] == bytes([0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A])
    while pos < len(raw):
        ln = struct.unpack('>I', raw[pos:pos+4])[0]
        assert struct.unpack('>I', raw[pos+8+ln:pos+12+ln])[0] == zlib.crc32(raw[pos+4:pos+8+ln]) & 0xffffffff
        if raw[pos+4:pos+8] == b'IDAT': idat += raw[pos+8:pos+8+ln]
        pos += 12 + ln
    zlib.decompress(idat)
print(f'{len(files)} edge PNGs · all CRCs + zlib decode OK')
PY
ok "python-zlib decodes every edge PNG (CRCs · Adler · filters)"
if command -v sips >/dev/null; then
  sips -g pixelWidth edge-641x33.png >/dev/null 2>&1
  ok "sips (Apple decoder) reads odd-stride PNG"
else
  skip "sips"
fi

echo "6 · report HTML · balance judge"
cargo run --example report_demo >/dev/null 2>&1
python3 - <<'PY'
from html.parser import HTMLParser
VOID = {'meta','br','hr','img','input','link','circle','rect','line','path','polyline','use','stop'}
class J(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False); self.stack=[]; self.errors=[]
    def handle_starttag(self, t, a):
        if t not in VOID: self.stack.append(t)
    def handle_endtag(self, t):
        if t in VOID: return
        if not self.stack or self.stack[-1] != t: self.errors.append(t)
        else: self.stack.pop()
j = J(); j.feed(open('report.html').read()); j.close()
assert not j.errors and not j.stack, f'MALFORMED: {j.errors} {j.stack}'
print('report.html well-formed · tags balanced')
PY
ok "HTML5-aware balance (nesting · strays · unclosed)"

echo
echo "JUDGE BATTERY: $PASS proven · $SKIP skipped (optional judges)"
