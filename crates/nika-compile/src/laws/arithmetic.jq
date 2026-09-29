# Exact decimal arithmetic laws (R4 A8), after order.jq: bounded exact sums, averages and the
# roundings a request states, then the output rule. A result is exact, or the run stops naming
# what cannot be done; nothing is computed in f64 or rounded unasked, and no allocation grows with
# an exponent.

# Exact arithmetic is bounded before anything is built: the common scale, and every operand
# rescaled to it, need at most this many digits, or the run stops naming what it needed. Past the
# operands the growth is arithmetic: a sum of n operands carries into at most ceil(log10 n) more
# digits, an average multiplies that sum's quotient by at most 2^63 or 5^63 (the count's own twos
# and fives), a stated rounding appends at most six digits. The work is O(n * 1000) digit
# operations for n values; a bare exponent (1e-1000000000) never sizes anything.
def _dbound: 1000;

# The operands as signed integers at their common scale: {ns, k}, value_i = ns[i] / 10^k.
def _dscaled($what):
  map(_dparts | ((.i + .f) | _dlead0) as $t | {s, t: $t, e: (.x - (.f | length))})
  | (map(select(.t != "") | -.e) | max // 0 | if . < 0 then 0 else . end) as $k
  | if $k > _dbound then
      error($what + " needs " + ($k | tostring) + " decimal places exactly, beyond the " + (_dbound | tostring) + "-digit bound of exact arithmetic: it is not computed")
    else
      {k: $k, ns: map(if .t == "" then 0
        else (.e + $k) as $z
          | if ($z + (.t | length)) > _dbound then
              error($what + " needs a " + (($z + (.t | length)) | tostring) + "-digit integer exactly, beyond the " + (_dbound | tostring) + "-digit bound of exact arithmetic: it is not computed")
            else (.s + .t + (if $z > 0 then "0" * $z else "" end)) | tonumber end
        end)}
    end;

# The exact decimal text of n / 10^k (n an integer): compact, trailing zeros folded.
def _dtext($n; $k):
  if $n == 0 then "0"
  else ($n | tostring) as $s | ($s | _dtrail0) as $m
    | $m + "e" + ((($s | length) - ($m | length) - $k) | tostring)
  end;

# Quotient digits and remainder of a non-negative integer (given as digits) by a small integer.
def _ddivmod($c):
  reduce (explode[] - 48) as $d ({q: "", r: 0};
    (.r * 10 + $d) as $v | .q += (($v / $c | floor) | tostring) | .r = ($v % $c))
  | .q |= _dint;

# The exact sum.
def dsum($what): _dscaled($what) as $x | _dtext($x.ns | add // 0; $x.k);

# The exact average when its decimal expansion is finite; otherwise the run stops, naming the sum
# and the count (no rounding is invented).
def davg($what):
  length as $c
  | _dscaled($what) as $x
  | ($x.ns | add // 0) as $n
  | ($n | tostring | ltrimstr("-")) as $abs
  | {m: $c, a: 0, b: 0}
  | until(.m % 2 != 0; .m = (.m / 2 | floor) | .a += 1)
  | until(.m % 5 != 0; .m = (.m / 5 | floor) | .b += 1)
  | . as $f
  | ($abs | _ddivmod($f.m)) as $qr
  | if $qr.r != 0 then
      error($what + " is " + (_dtext($n; $x.k) | _dshow) + " divided by " + ($c | tostring) + ", which has no finite decimal expansion: state a rounding to write it")
    else ([$f.a, $f.b] | max) as $g
      | (reduce range(0; $g - $f.a) as $_ (1; . * 2)) as $p2
      | (reduce range(0; $g - $f.b) as $_ (1; . * 5)) as $p5
      | (((if $n < 0 then "-" else "" end) + $qr.q | tonumber) * $p2 * $p5) as $q
      | _dtext($q; $x.k + $g)
    end;

# Rounded to $d decimals, half away from zero (the mode of jq's round), on the exact value
# n / (c * 10^k) of a sum (c = 1) or an average (c = the count).
def _dround($n; $c; $k; $d):
  ($n | tostring | ltrimstr("-")) as $abs
  | (if $k <= $d then ($abs + ("0" * ($d - $k) // "")) | _ddivmod($c) | {q, up: (2 * .r >= $c)}
     else ($k - $d) as $j
       | (if ($abs | length) > $j then {hi: $abs[:($abs | length) - $j], lo: $abs[($abs | length) - $j:]}
          else {hi: "0", lo: (("0" * ($j - ($abs | length)) // "") + $abs)} end) as $s
       | ($s.hi | _ddivmod($c)) as $u
       | ($c - 2 * $u.r) as $gap
       | {q: $u.q, up: ($gap <= 0 or ($gap == 1 and ($s.lo[:1] | tonumber) >= 5))}
     end) as $r
  | (($r.q | tonumber) + (if $r.up then 1 else 0 end)) as $m
  | _dtext(if $n < 0 then -$m else $m end; $d);
def dsum_round($what; $d): _dscaled($what) as $x | _dround($x.ns | add // 0; 1; $x.k; $d);
def davg_round($what; $d): length as $c | _dscaled($what) as $x | _dround($x.ns | add // 0; $c; $x.k; $d);

# The exact integer a value is, when it has at most 20 digits (null otherwise).
def _dinteger:
  _dparts
  | ((.i + .f) | _dlead0) as $t
  | if $t == "" then 0
    else ($t | _dtrail0) as $D
      | (.x - (.f | length) + (($t | length) - ($D | length))) as $e
      | if $e >= 0 and (($D | length) + $e) <= 20 then (.s + $D + (if $e > 0 then "0" * $e else "" end)) | tonumber
        else null end
    end;

# The number a computed exact value is written as, keeping the type jq's own arithmetic gives it
# ($mode "float": a float, as an average or a decimal sum; "integer": an integer, as a sum of
# integers; "own": the selected value itself, as a minimum or a maximum), else the exact integer or
# float that carries it; a value no JSON number carries exactly stops the run, naming it and what
# it would have become.
def dout($mode; $what):
  . as $x
  | ($x | dkey) as $key
  | (if type == "number" then . else tonumber end) as $own
  | (if $mode == "float" then [$own + 0.0, ($x | _dinteger)]
     elif $mode == "integer" then [($x | _dinteger), $own + 0.0]
     else [$own, ($x | _dinteger)] end) as $candidates
  | ($candidates | map(select(. != null and (_dcarried as $c | $c != null and ($c | dkey) == $key)))) as $ok
  | if ($ok | length) > 0 then $ok[0]
    else error($what + " is exactly " + ($x | _dshow) + ", which no JSON number carries here (it would be written as " + (($candidates[0] // $own) | _dcarried // "no finite number") + "), so nothing is written")
    end;

# Whether jq's own arithmetic over these values would give an integer (every value an integer).
def _dintegers: all(.[]; tojson | (contains(".") or contains("e") or contains("E")) | not);

# The aggregates as written: the exact law, then the representation rule.
def dsum_out($what): (if _dintegers then "integer" else "float" end) as $m | dsum($what) | dout($m; $what);
def dsum_out($what; $d): dsum_round($what; $d) | dout("float"; $what);
def davg_out($what): if length == 0 then error($what + " has no value to average") else davg($what) | dout("float"; $what) end;
def davg_out($what; $d): if length == 0 then error($what + " has no value to average") else davg_round($what; $d) | dout("float"; $what) end;
def dmin_out($what): if length == 0 then null else min_by(dkey) | dout("own"; $what) end;
def dmin_out($what; $d): if length == 0 then null else [min_by(dkey)] | dsum_round($what; $d) | dout("float"; $what) end;
def dmax_out($what): if length == 0 then null else max_by(dkey) | dout("own"; $what) end;
def dmax_out($what; $d): if length == 0 then null else [max_by(dkey)] | dsum_round($what; $d) | dout("float"; $what) end;

# A column the request writes as a JSON number (E38 S2): the value the number law read, written as
# itself only where a JSON number carries it exactly, else the run stops naming it.
def dnum_out($what): dout("own"; $what);
