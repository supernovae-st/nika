# Exact decimal order laws (R4 A8), run by the one runtime jq: the exact order key, the rank cut
# and the transport guard. A value here is one the unchanged number law accepted (a finite JSON
# number or a text its grammar accepts); nothing collapses to f64, rounds or guesses.

# Leading and trailing zeros of a digit text, removed without regular expressions (they dominate
# the runtime cost of a law run once per value): one slice for a single zero, one pass for a run.
def _dlead0:
  if startswith("0") | not then .
  elif startswith("00") | not then .[1:]
  else explode | (map(. != 48) | index(true)) as $i | if $i == null then "" else .[$i:] | implode end
  end;
def _dtrail0:
  if endswith("0") | not then .
  elif endswith("00") | not then .[:-1]
  else explode | (map(. != 48) | rindex(true)) as $i | if $i == null then "" else .[:$i + 1] | implode end
  end;

# Digits of a non-negative integer text without leading zeros ("0" when none).
def _dint: _dlead0 | if . == "" then "0" else . end;

# Sign, integer digits, fraction digits and exponent (an integer) of an accepted number: a text
# the law accepted is read as jq reads it (blanks dropped), then its own digits are split. The
# text is read with fromjson: the runtime's tonumber refuses a decimal beyond f64 (1e400),
# whose digits these laws still read exactly.
def _dparts:
  (if type == "string" then fromjson else . end)
  | tojson
  | startswith("-") as $neg
  | ltrimstr("-") | ascii_downcase | split("e") as $me
  | ($me[0] | split(".")) as $if
  | {s: (if $neg then "-" else "" end), i: $if[0], f: ($if[1] // ""),
     x: (($me[1] // "0") | ltrimstr("+") | if startswith("-") then -(ltrimstr("-") | _dint | tonumber) else _dint | tonumber end)};

# The exact order key: [0] for zero, [1, E, D] for a positive 0.D x 10^E (D the significant
# digits, no leading or trailing zero), [-1, -E, C] for a negative one (C complements every digit
# of D and ends with ":", so a longer mantissa sorts first). jq's own array order compares keys;
# the work is linear in the value's own digits, never in its exponent.
def dkey:
  _dparts
  | (.i + .f) as $d
  | ($d | _dlead0) as $t
  | if $t == "" then [0]
    else ($t | _dtrail0) as $D
      | ((.i | length) - (($d | length) - ($t | length)) + .x) as $E
      | if .s == "-" then [-1, -$E, ($D | explode | map(105 - .) | implode) + ":"] else [1, $E, $D] end
    end;

# An exact value written back as text: plain decimal for moderate exponents, else scientific
# (a run of zeros is never expanded beyond forty places).
def _dshow:
  dkey as $k
  | if $k == [0] then "0"
    else (if $k[0] < 0 then "-" else "" end) as $s
      | (if $k[0] < 0 then ($k[2] | rtrimstr(":") | explode | map(105 - .) | implode) else $k[2] end) as $D
      | ($k[1] * $k[0]) as $E
      | if $E > 0 and $E <= 40 then
          $s + (if ($D | length) > $E then $D[:$E] + "." + $D[$E:] else $D + ("0" * ($E - ($D | length) + 1))[1:] end)
        elif $E <= 0 and $E > -40 then $s + "0." + ("0" * (1 - $E))[1:] + $D
        else $s + $D[:1] + (if ($D | length) > 1 then "." + $D[1:] else "" end) + "e" + (($E - 1) | tostring)
        end
    end;

# The number as the JSON transport between tasks writes it (serde_json with float_roundtrip): an
# integer within [-2^63, 2^64-1] as itself, anything else as the shortest text of its f64; null
# when no finite f64 carries it, an infinity or NaN included (its text reads as no number).
def _dcarried:
  tojson as $t
  | if ($t | contains(".") or contains("e") or contains("E")) then . + 0.0
    elif . >= -9223372036854775808 and . <= 18446744073709551615 then .
    else ($t + ".0" | try fromjson catch infinite) + 0.0
    end
  | if isinfinite or isnan then null else tojson end;

# Whether the transport carries this number with its exact value (string tests first: most
# numbers are short integers or decimals whose shortest f64 text is their own).
def _dkept:
  tojson as $t
  | if ($t | contains(".") or contains("e") or contains("E")) | not then
      ($t | length) <= 18 or (. >= -9223372036854775808 and . <= 18446744073709551615)
        or (_dcarried as $c | $c != null and ($c | dkey) == ($t | dkey))
    else _dcarried as $c | $c != null and ($c == $t or ($c | dkey) == ($t | dkey))
    end;

# Every number of the document the next task may read or write keeps its exact value, or the run
# stops before any effect naming the first that would not. The scope is every number ($fields
# null), else the named fields of each record: the check and the name it gives obey the same
# scope, so a field outside it never decides nor appears in the refusal.
def _dscoped($fields): $fields == null or (length >= 2 and (.[1] as $f | any($fields[]; . == $f)));
def _dlost($fields):
  (if $fields == null then .. else (.[]? | objects | .[$fields[]]? | ..) end) | numbers | select(_dkept | not);
def dguard($fields):
  if ([limit(1; _dlost($fields))] | length) == 0 then .
  else first(paths(numbers) as $p | select(($p | _dscoped($fields)) and (getpath($p) | _dkept | not)) | $p) as $p
    | getpath($p) as $v
    | error("the number at " + ($p | map(tostring) | join(".")) + " is " + ($v | tojson) + ": the JSON transport between tasks would carry it as " + (($v | _dcarried) // "no finite number") + ", so it cannot pass exactly and nothing is written")
  end;

# The cut of a ranking keeping n rows (the slice stays the rule's own `.[:n]`): rows the cut
# separates that tie on the key must be copies of one another as written; otherwise input order
# would choose, and the run stops naming the tie. Copies, and ties the cut does not separate, pass.
def dtie($n; key; out; $what):
  if length > $n and $n > 0 and ((.[$n - 1] | key) == (.[$n] | key)) then
    (.[$n - 1] | key) as $k
    | ([.[] | select((key) == $k) | out] | group_by(.) | length) as $distinct
    | if $distinct > 1 then
        error("rows " + ($n | tostring) + " and " + ($n + 1 | tostring) + " of the ranking by " + $what + " tie between " + ($distinct | tostring) + " different records: keeping " + ($n | tostring) + " would choose among them by input order, so nothing is written")
      else . end
  else . end;
