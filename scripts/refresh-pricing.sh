#!/usr/bin/env bash
# Refresh data/model-pricing.toml from models.dev (MIT · sst/models.dev).
#
# The pricing catalog is a VENDORED SNAPSHOT, refreshed at maintenance
# time — never a runtime fetch (supernovae-alignment Rule 1: a static
# report must not depend on a vendor endpoint being up). models.dev is
# the one machine-readable source covering every cloud provider in the
# engine catalog (verified 2026-07-06: openrouter 336 priced models ·
# huggingface 50 · nvidia 84 — LiteLLM carries zero huggingface).
#
# Usage:
#   bash scripts/refresh-pricing.sh              # fetch live
#   bash scripts/refresh-pricing.sh api.json     # from a local snapshot
#
# Emission laws (matched to src/data/models.rs lookup semantics):
# - provider strings = the ENGINE's canonical ids (anthropic · google ·
#   …) so `find_pricing_scoped(engine_id, …)` matches (the hand era
#   wrote display names — "Gemini" — which eq_ignore_ascii_case never
#   matched against "google").
# - within a provider, rules sort by pattern length DESC: the contains()
#   fallback takes the FIRST hit, so `claude-opus-4-5` must outrank any
#   shorter pattern a versioned id also contains.
# - values always carry a decimal point (TOML integers do not
#   deserialize into f64 fields).
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="${1:-}"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
if [ -z "$SRC" ]; then
  curl -sL --max-time 120 https://models.dev/api.json -o "$TMP"
  SRC="$TMP"
fi

python3 - "$SRC" <<'PYEOF'
import hashlib
import json
import sys
from datetime import date

src = sys.argv[1]
raw = open(src, "rb").read()
sha = hashlib.sha256(raw).hexdigest()[:16]
data = json.loads(raw)

# Engine canonical id → models.dev provider id (identical today; the
# map exists so a future rename upstream is one line here).
PROVIDERS = {
    "anthropic": "anthropic",
    "openai": "openai",
    "google": "google",
    "deepseek": "deepseek",
    "mistral": "mistral",
    "xai": "xai",
    "groq": "groq",
    "openrouter": "openrouter",
    "huggingface": "huggingface",
    "nvidia": "nvidia",
    "moonshot": "moonshotai",
}

def toml_float(v: float) -> str:
    s = f"{v:.10g}"
    return s if ("." in s or "e" in s) else s + ".0"

# Referenced-but-retired: models the PROVIDER CATALOG still points at
# (default_model rows) that upstream dropped — priced from LiteLLM's
# historical table (values verified 2026-07-06). The integrity gate
# (validate::provider_models_have_pricing) holds through upstream churn;
# retiring these requires the default_model canon to move first.
SUPPLEMENTS = [
    ("anthropic", "claude-sonnet-4", 3.0, 15.0),
    ("xai", "grok-3", 3.0, 15.0),
    ("xai", "grok-3-mini-fast", 0.6, 4.0),
]

rules = []
seen = set()
for engine_id, upstream_id in PROVIDERS.items():
    prov = data.get(upstream_id)
    if not prov:
        print(f"WARN: {upstream_id} missing upstream", file=sys.stderr)
        continue
    for mid, m in prov["models"].items():
        cost = m.get("cost")
        if not cost or "input" not in cost or "output" not in cost:
            continue  # unpriced (previews · embeddings without rates) — absent ≠ free
        key = (engine_id, mid)
        if key in seen:
            continue
        seen.add(key)
        # Cache axes when upstream discloses them (2026-07: cache_read on
        # 2209 models · cache_write on 756). The cost math prices the
        # cache subsets at these rates; without them it falls back to the
        # input rate (documented approximation).
        cr = cost.get("cache_read")
        cw = cost.get("cache_write")
        rules.append((
            engine_id, mid, float(cost["input"]), float(cost["output"]),
            None if cr is None else float(cr),
            None if cw is None else float(cw),
        ))

for prov, mid, inp, outp in SUPPLEMENTS:
    if (prov, mid) not in seen:
        seen.add((prov, mid))
        rules.append((prov, mid, inp, outp, None, None))

# Contains-fallback safety: longest pattern first within each provider.
rules.sort(key=lambda r: (r[0], -len(r[1]), r[1]))

# Shrink guard (the LiteLLM pattern): a refresh that loses more than half
# the previous rules is a corrupted/partial upstream, not a real catalog
# move — refuse instead of silently shipping a gutted snapshot.
target = "crates/nika-catalog/data/model-pricing.toml"
try:
    prev = open(target).read().count("[[rules]]")
except OSError:
    prev = 0
if prev and len(rules) < prev // 2:
    print(
        f"ERROR: refreshed catalog shrank {prev} → {len(rules)} rules "
        f"(more than half) — corrupted upstream? refusing to write. "
        f"Set NIKA_PRICING_FORCE=1 to override.",
        file=sys.stderr,
    )
    import os
    if not os.environ.get("NIKA_PRICING_FORCE"):
        sys.exit(1)

out = []
out.append('schema = "nika/model-pricing@1.1"')
out.append("")
out.append("[meta]")
out.append('source = "https://models.dev/api.json"')
out.append(f'as_of = "{date.today().isoformat()}"')
out.append(f'source_sha256_16 = "{sha}"')
out.append("")
out.append("# ── GENERATED · do not hand-edit ────────────────────────────")
out.append("# Source: models.dev (MIT · anomalyco/models.dev) — see [meta].")
out.append("# Refresh: bash scripts/refresh-pricing.sh")
out.append("# Values: USD per 1M tokens. Lookup: exact match, then contains()")
out.append("# fallback (first hit wins — longest patterns emitted first).")
out.append("# Cache axes ride along when upstream discloses them; the cost")
out.append("# math prices cache subsets at these rates (input-rate fallback).")

current = None
for prov, mid, inp, outp, cr, cw in rules:
    if prov != current:
        out.append("")
        out.append(f"# ── {prov} ({sum(1 for r in rules if r[0] == prov)} models) ──")
        current = prov
    out.append("")
    out.append("[[rules]]")
    out.append(f'provider = "{prov}"')
    out.append(f'model_pattern = "{mid}"')
    out.append(f"input_per_million = {toml_float(inp)}")
    out.append(f"output_per_million = {toml_float(outp)}")
    if cr is not None:
        out.append(f"cache_read_per_million = {toml_float(cr)}")
    if cw is not None:
        out.append(f"cache_write_per_million = {toml_float(cw)}")

open(target, "w").write("\n".join(out) + "\n")
per = {}
cached = sum(1 for r in rules if r[4] is not None)
for r in rules:
    per[r[0]] = per.get(r[0], 0) + 1
print(
    f"wrote {target}: {len(rules)} rules ({cached} with cache rates) · "
    + " · ".join(f"{k}:{v}" for k, v in sorted(per.items()))
)
PYEOF
