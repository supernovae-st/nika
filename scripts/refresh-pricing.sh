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
#   deserialize into f64 fields). This applies to the RATES only — the
#   token limits are u32 and must stay integers.
# - a field upstream does not carry is OMITTED, never defaulted. Absent
#   means "not disclosed"; a zero would mean "bounded by nothing", which
#   is the opposite claim. Upstream writes 0 for models where a limit
#   does not apply (image · video · speech) — those are dropped too.
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
}

def toml_float(v: float) -> str:
    s = f"{v:.10g}"
    return s if ("." in s or "e" in s) else s + ".0"

def limit(m: dict, key: str):
    """A token limit, or None when upstream is silent OR says 0.

    models.dev writes 0 for models where the limit is not a thing —
    image, video and speech models. Carrying that 0 through would claim
    a ceiling of nothing, so 0 and absent collapse to the same answer:
    we do not know. The schema stores that as an omitted field."""
    v = (m.get("limit") or {}).get(key)
    if not isinstance(v, int) or v <= 0:
        return None
    return v

def status_of(m: dict):
    """Upstream lifecycle marker (deprecated · beta · whatever they add
    next). Shape-checked only — the vocabulary is theirs to grow, and a
    refresh must not fail because they coined a new word."""
    s = m.get("status")
    if not isinstance(s, str):
        return None
    s = s.strip().lower()
    # Byte rule kept identical to validate_status() in the codegen crate:
    # [a-z0-9-]{1,32}. Anything else is upstream garbage, not a status.
    ok = s and len(s) <= 32 and all(
        ("a" <= c <= "z") or ("0" <= c <= "9") or c == "-" for c in s
    )
    return s if ok else None

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
        ow = m.get("open_weights")
        rules.append({
            "provider": engine_id,
            "pattern": mid,
            "input": float(cost["input"]),
            "output": float(cost["output"]),
            "cache_read": None if cr is None else float(cr),
            "cache_write": None if cw is None else float(cw),
            # Upstream model facts (schema @1.2) — carried verbatim, or
            # omitted. Never defaulted: see the emission laws above.
            "max_output_tokens": limit(m, "output"),
            "context_window_tokens": limit(m, "context"),
            "open_weights": ow if isinstance(ow, bool) else None,
            "status": status_of(m),
        })

# Supplements are hand-priced from LiteLLM's historical table and carry
# NO model facts — upstream dropped these models, so there is nothing to
# mirror. Every @1.2 field stays absent rather than guessed.
for prov, mid, inp, outp in SUPPLEMENTS:
    if (prov, mid) not in seen:
        seen.add((prov, mid))
        rules.append({
            "provider": prov, "pattern": mid, "input": inp, "output": outp,
            "cache_read": None, "cache_write": None,
            "max_output_tokens": None, "context_window_tokens": None,
            "open_weights": None, "status": None,
        })

# Contains-fallback safety: longest pattern first within each provider.
rules.sort(key=lambda r: (r["provider"], -len(r["pattern"]), r["pattern"]))

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
out.append('schema = "nika/model-pricing@1.2"')
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
out.append("# Token limits · open_weights · status are upstream's own words,")
out.append("# carried verbatim. An OMITTED field means upstream did not say —")
out.append("# read it as unknown, never as 0 / false.")

current = None
for r in rules:
    prov = r["provider"]
    if prov != current:
        n = sum(1 for x in rules if x["provider"] == prov)
        out.append("")
        out.append(f"# ── {prov} ({n} models) ──")
        current = prov
    out.append("")
    out.append("[[rules]]")
    out.append(f'provider = "{prov}"')
    out.append(f'model_pattern = "{r["pattern"]}"')
    out.append(f"input_per_million = {toml_float(r['input'])}")
    out.append(f"output_per_million = {toml_float(r['output'])}")
    if r["cache_read"] is not None:
        out.append(f"cache_read_per_million = {toml_float(r['cache_read'])}")
    if r["cache_write"] is not None:
        out.append(f"cache_write_per_million = {toml_float(r['cache_write'])}")
    # u32 fields — emitted as TOML integers (no decimal point).
    if r["max_output_tokens"] is not None:
        out.append(f"max_output_tokens = {r['max_output_tokens']}")
    if r["context_window_tokens"] is not None:
        out.append(f"context_window_tokens = {r['context_window_tokens']}")
    if r["open_weights"] is not None:
        out.append(f"open_weights = {str(r['open_weights']).lower()}")
    if r["status"] is not None:
        out.append(f'status = "{r["status"]}"')

open(target, "w").write("\n".join(out) + "\n")
per = {}
for r in rules:
    per[r["provider"]] = per.get(r["provider"], 0) + 1
def n_with(field):
    return sum(1 for r in rules if r[field] is not None)
print(
    f"wrote {target}: {len(rules)} rules · "
    + " · ".join(f"{k}:{v}" for k, v in sorted(per.items()))
)
# Coverage per field — the honest report of what upstream disclosed. A
# number well below the rule count is upstream being silent, not a bug.
print(
    "  facts: "
    f"cache_read {n_with('cache_read')} · "
    f"max_output_tokens {n_with('max_output_tokens')} · "
    f"context_window_tokens {n_with('context_window_tokens')} · "
    f"open_weights {n_with('open_weights')} · "
    f"status {n_with('status')}"
    f"  (of {len(rules)})"
)
PYEOF
