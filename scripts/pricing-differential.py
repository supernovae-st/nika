#!/usr/bin/env python3
# pricing-differential.py — two price catalogs, one verdict (NOBODY WINS).
#
# models.dev (our vendored snapshot) and LiteLLM (MIT · community-maintained)
# price the same models independently. Where both carry a row, they must
# agree; a disagreement is a FINDING with a name and a date, never a silent
# pick. This is oracle-differential applied to DATA (wave-1.5 ruling §3bis).
#
#   python3 scripts/pricing-differential.py [litellm.json]   # live fetch if absent
#   python3 scripts/pricing-differential.py --selftest       # prove the teeth
#
# Exit 0 · every comparable row agrees (or its skew is a named ledger row)
# Exit 2 · environment (fetch failed · file unreadable)
# Exit 4 · an unexplained skew — triage it: fix a catalog or name the row
#
# Scope is honest, never silent: only providers with an unambiguous
# LiteLLM mapping are compared; aggregators (openrouter · huggingface —
# route-dependent pricing) and nvidia (their NIM catalog is a different
# animal) are SKIPPED and the report says so. A model present in one
# catalog only is NOT a skew — catalogs cover different frontiers.
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
OURS = ROOT / "crates/nika-catalog/data/model-pricing.toml"
LITELLM_URL = (
    "https://raw.githubusercontent.com/BerriAI/litellm/main/"
    "model_prices_and_context_window.json"
)

# our engine provider id → LiteLLM's litellm_provider
PROVIDER_MAP = {
    "anthropic": "anthropic",
    "openai": "openai",
    "google": "gemini",
    "groq": "groq",
    "mistral": "mistral",
    "xai": "xai",
    "deepseek": "deepseek",
}
SKIPPED = ("openrouter", "huggingface", "nvidia")

# Relative tolerance for float dust; a real pricing skew is 1.5x-4x,
# never 0.5%.
TOL = 0.005

# (provider, pattern, field) → (ours, theirs, why) — tolerated skews.
# DIRECTIONAL like the oracle ledger: the row forgives exactly these two
# values; if either catalog moves, the row re-fails and gets re-triaged.
# Keep it EMPTY unless a skew has a name; a row here is a debt, not a
# dispensation.
_DEEPSEEK = (
    "three-way disagreement 2026-07-29: ours (models.dev) 0.14/0.28 · "
    "LiteLLM 0.28/0.42 · the primary page (api-docs.deepseek.com) shows "
    "TWO columns (0.14/0.28 and 0.435/0.87) whose model assignment needs "
    "a rendered read — triage owed, neither catalog trusted yet."
)
_ALIAS = (
    "floating-alias lag 2026-07-29: models.dev resolves the -latest "
    "alias against the 3.5 generation (GA on ai.google.dev), LiteLLM "
    "still against 2.5 — rows die when LiteLLM re-resolves."
)
_NONTEXT = (
    "non-text output priced as text by LiteLLM 2026-07-29: the audio/"
    "image output token rate is the expensive one (ai.google.dev TTS "
    "audio-out $10/M · openai image-out $30/M) and models.dev carries "
    "it — rows die when LiteLLM prices the real modality."
)
LEDGER: dict[tuple[str, str, str], tuple[float, float, str]] = {
    ("deepseek", "deepseek-reasoner", "input_per_million"): (0.14, 0.28, _DEEPSEEK),
    ("deepseek", "deepseek-reasoner", "output_per_million"): (0.28, 0.42, _DEEPSEEK),
    ("deepseek", "deepseek-chat", "input_per_million"): (0.14, 0.28, _DEEPSEEK),
    ("deepseek", "deepseek-chat", "output_per_million"): (0.28, 0.42, _DEEPSEEK),
    ("google", "gemini-flash-latest", "input_per_million"): (1.5, 0.3, _ALIAS),
    ("google", "gemini-flash-latest", "output_per_million"): (9.0, 2.5, _ALIAS),
    ("google", "gemini-flash-lite-latest", "input_per_million"): (0.25, 0.1, _ALIAS),
    ("google", "gemini-flash-lite-latest", "output_per_million"): (1.5, 0.4, _ALIAS),
    ("google", "gemini-2.5-flash-preview-tts", "input_per_million"): (0.5, 0.3, _NONTEXT),
    ("google", "gemini-2.5-flash-preview-tts", "output_per_million"): (10.0, 2.5, _NONTEXT),
    ("openai", "gpt-image-2", "output_per_million"): (30.0, 10.0, _NONTEXT),
}


def load_ours() -> list[dict]:
    return tomllib.load(open(OURS, "rb"))["rules"]


def load_litellm(path: str | None):
    if path:
        return json.load(open(path))
    try:
        raw = subprocess.run(
            ["curl", "-sL", "--max-time", "60", LITELLM_URL],
            capture_output=True, check=True,
        ).stdout
        return json.loads(raw)
    except (subprocess.CalledProcessError, json.JSONDecodeError) as e:
        print(f"environment: cannot fetch LiteLLM table ({e})", file=sys.stderr)
        sys.exit(2)


def compare(ours: list[dict], theirs: dict) -> tuple[int, int, list[str], int]:
    """(compared, agreed, unexplained-lines, ledger-hits)."""
    # index LiteLLM by (provider, bare key)
    idx = {}
    for key, v in theirs.items():
        if isinstance(v, dict) and "litellm_provider" in v:
            idx[(v["litellm_provider"], key)] = v
    compared = agreed = ledgered = 0
    bad: list[str] = []
    for r in ours:
        lp = PROVIDER_MAP.get(r["provider"])
        if lp is None:
            continue
        t = idx.get((lp, r["model_pattern"]))
        if t is None:
            continue
        for field, their_key in (
            ("input_per_million", "input_cost_per_token"),
            ("output_per_million", "output_cost_per_token"),
        ):
            mine = r.get(field)
            their_tok = t.get(their_key)
            if mine is None or their_tok is None:
                continue
            their_m = their_tok * 1_000_000
            compared += 1
            if their_m == 0 and mine == 0:
                agreed += 1
                continue
            denom = max(abs(mine), abs(their_m))
            if denom == 0 or abs(mine - their_m) / denom <= TOL:
                agreed += 1
                continue
            row = LEDGER.get((r["provider"], r["model_pattern"], field))
            if row is not None and abs(row[0] - mine) < 1e-12 and abs(row[1] - their_m) / max(their_m, 1e-12) <= TOL:
                ledgered += 1
                print(f"LEDGER {r['provider']}/{r['model_pattern']} {field} · {row[2]}")
                continue
            bad.append(
                f"SKEW {r['provider']}/{r['model_pattern']} {field}: "
                f"ours {mine} · litellm {their_m:.6g}"
            )
    return compared, agreed, bad, ledgered


def main() -> int:
    args = sys.argv[1:]
    if "--selftest" in args:
        return selftest()
    ours = load_ours()
    theirs = load_litellm(args[0] if args else None)
    compared, agreed, bad, ledgered = compare(ours, theirs)
    for line in bad:
        print(line)
    print(
        f"\npricing-differential · {compared} prices compared · {agreed} agree · "
        f"{ledgered} ledgered · {len(bad)} unexplained · "
        f"skipped providers: {', '.join(SKIPPED)} (aggregators/NIM · by design)"
    )
    return 4 if bad else 0


def selftest() -> int:
    ours = [
        {"provider": "openai", "model_pattern": "m1",
         "input_per_million": 2.5, "output_per_million": 10.0},
        {"provider": "openai", "model_pattern": "m2",
         "input_per_million": 1.0, "output_per_million": 4.0},
        {"provider": "openrouter", "model_pattern": "m3",
         "input_per_million": 9.0, "output_per_million": 9.0},
    ]
    theirs = {
        "m1": {"litellm_provider": "openai",
               "input_cost_per_token": 2.5e-6, "output_cost_per_token": 1e-5},
        "m2": {"litellm_provider": "openai",
               "input_cost_per_token": 2e-6, "output_cost_per_token": 4.000001e-6},
        "m3": {"litellm_provider": "openrouter",
               "input_cost_per_token": 1e-6, "output_cost_per_token": 1e-6},
    }
    cases = []
    c, a, bad, led = compare(ours, theirs)
    cases.append(("agree + tolerance + skew + skip", c == 4 and a == 3 and len(bad) == 1 and led == 0))
    cases.append(("the skew names the field", bad and "m2 input_per_million" in bad[0]))
    LEDGER[("openai", "m2", "input_per_million")] = (1.0, 2.0, "selftest row")
    c, a, bad, led = compare(ours, theirs)
    cases.append(("ledger row absorbs the named skew", led == 1 and not bad))
    LEDGER[("openai", "m2", "input_per_million")] = (1.0, 3.0, "stale row")
    c, a, bad, led = compare(ours, theirs)
    cases.append(("a moved value re-fails its ledger row", led == 0 and len(bad) == 1))
    del LEDGER[("openai", "m2", "input_per_million")]
    ok = all(v for _, v in cases)
    for name, v in cases:
        print(f"{'ok  ' if v else 'FAIL'}  {name}")
    print(f"selftest · {sum(v for _, v in cases)}/{len(cases)}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
