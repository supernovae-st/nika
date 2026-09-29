# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

def scan(re; flags): matches(re; "g" + flags)[] | .[0].string;
def scan(re): scan(re; "");

# Each operand is one number, never the zero-or-many stream from fromjson.
# Keep fromjson itself unchanged; an explicit try still owns its skip policy.
def tonumber:
  if type == "number" then .
  elif type == "string" then
    [fromjson] | if length == 1 and (.[0] | type) == "number"
      then .[0] else error("cannot parse as one number") end
  else error("cannot parse as one number") end;
