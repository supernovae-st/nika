# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

# The runtime's corrections to jaq's std, chained after jaq-json by every jq consumer: the
# nika:jq builtin, output bindings, the static checker and the compile verifier. One text, so no
# consumer drifts; nika_cap::JQ_STD_SHADOW_PROBES is the probe set each of them runs.

# jq defines scan as global (match(re; "g" + flags)). jaq-std 3.0.x omits the flag and yields the
# first match only: a green check and a green run, every number wrong (the 2026-07-29 finding).
# This shadow retires the day a jaq release carries the correction.
def scan(re; flags): matches(re; "g" + flags)[] | .[0].string;
def scan(re): scan(re; "");

# Each operand is one number, never the zero-or-many stream from fromjson.
# Keep fromjson itself unchanged; an explicit try still owns its skip policy.
# The number is finite: the reader accepts NaN, Infinity and -Infinity and overflows 1e400 to an
# infinity, and jaq orders NaN below every number, so the refusal comes before any predicate,
# comparison, sort or aggregate reads the value. fromjson and computed values stay unguarded.
def tonumber:
  if type == "number" then .
  elif type == "string" then
    [fromjson] | if length == 1 and (.[0] | type) == "number"
      then .[0] else error("cannot parse as one number") end
  else error("cannot parse as one number") end
  | if isinfinite or isnan then error("tonumber: not a finite number") else . end;
