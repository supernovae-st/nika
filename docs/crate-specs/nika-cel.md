# nika-cel — crate spec (L0 · the cel-subset/0.1 expression engine)

> Gate 1 artifact · 2026-06-13. The ONE CEL parser+evaluator the whole
> engine shares — the spec's "zero parser drift between engines" goal
> (03-dag §expression-language) made structural. nika-schema (the
> checker · static shape) and nika-runtime (the executor · the `expr`
> seam) both consume it · neither re-implements the grammar.
>
> SAFETY NOTE · this crate evaluates the `cel-subset/0.1` grammar, which
> is **non-Turing-complete, side-effect-free, and bounded** (the CEL
> design goal · used by Kubernetes/Envoy/gRPC). It is NOT a general
> code interpreter — no I/O, no loops, no recursion, no host calls; the
> callable set is closed (§3). The word "evaluate" below means "compute
> the value of a bounded expression", never "run arbitrary code".

## 1 · Role

Parse + compute the **`cel-subset/0.1`** grammar (the normative EBNF in
`nika-spec` 03-dag.md §formal-grammar) — the exact, bounded subset of
[CEL](https://cel.dev) a conformant Nika engine must support inside
`${{ }}`. Pure: zero I/O, zero async, zero runtime concepts — the host
injects namespace lookup through one trait.

```text
src (the inside of ${{ }})        e.g. tasks.test.status == 'success' && size(vars.tags) > 0
        │
        ▼
nika_cel::parse(src) -> Expr      lexer → recursive-descent → AST     (NIKA-VAR-005 · static)
        │
        ▼
nika_cel::compute(&Expr, res)     typed walk over a Resolver           (NIKA-VAR-001 · unresolved
        │                          (the host's namespace lookup)        NIKA-VAR-006 · type error)
        ▼
serde_json::Value                 the computed value (bool for when:)
```

Why L0 · pure parser+computer (no I/O · no async) · the L0 definition.
nika-schema (L0) and nika-runtime (L3) both depend downward onto it.
serde_json::Value is the value currency (the runtime's `Scope` already
produces it · the checker reasons over the same shapes · zero
conversion at either seam).

## 2 · Public API

```rust
/// Parse one cel-subset/0.1 expression (the inside of `${{ }}`, trimmed).
/// NIKA-VAR-005 on any grammar violation (chained relation · unknown
/// call · bad token · unclosed group).
pub fn parse(src: &str) -> Result<Expr, CelError>;

/// The host's namespace lookup — resolves a ROOT identifier to its
/// value (the parser owns `.field` / `[index]` navigation ON TOP).
/// `None` ⇒ NIKA-VAR-001 (unresolved root).
pub trait Resolver {
    fn resolve_root(&self, name: &str) -> Option<Value>;
}

/// Compute a parsed expression to a value (NIKA-VAR-001 unresolved ·
/// NIKA-VAR-006 type error / cross-type compare).
pub fn compute(expr: &Expr, resolver: &dyn Resolver) -> Result<Value, CelError>;

/// Compute as a boolean (the `when:` gate) — NIKA-VAR-006 if the
/// result is not a bool. Parse-time shape (a statically-non-boolean
/// root) is `Expr::is_boolean_shaped()` → the checker raises VAR-005.
pub fn compute_bool(expr: &Expr, resolver: &dyn Resolver) -> Result<bool, CelError>;

pub struct Expr { /* the AST root · #[non_exhaustive] node enum inside */ }
impl Expr {
    /// Static when:-shape (spec §when rules · side-constraint 5): a bare
    /// string/number literal or a bare reference with NO relation/boolean
    /// operator is not boolean-SHAPED → the checker raises VAR-005.
    pub fn is_boolean_shaped(&self) -> bool;
    /// Every namespace ROOT the expression references (the checker walks
    /// these to validate `tasks.<id>` ids + deep paths against schema).
    pub fn roots(&self) -> Vec<String>;
}

pub struct CelError { /* spec-plane code + message + byte span */ }
impl CelError {
    /// The spec wire code (`NIKA-VAR-005` · `NIKA-VAR-006` ·
    /// `NIKA-VAR-001`) — resolvable in `nika_pack::error_codes()`. CEL
    /// errors are CONFORMANCE codes (the spec 05 table is their canon ·
    /// NOT a nika-error registry range · same plane as NIKA-TIMEOUT-001).
    pub fn spec_code(&self) -> &'static str;
    pub fn message(&self) -> &str;
    pub fn span(&self) -> (usize, usize); // byte offsets into src
}
```

## 3 · The grammar (cel-subset/0.1 · normative · 03-dag §formal-grammar)

```ebnf
expr     = ternary ;
ternary  = or , [ "?" , expr , ":" , ternary ] ;   (* cond MUST be bool · right-assoc *)
or       = and , { "||" , and } ;
and      = rel , { "&&" , rel } ;
rel      = unary , [ relop , unary ] ;             (* at most ONE relop · non-associative *)
relop    = "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" ;
unary    = { "!" } , postfix ;
postfix  = primary , { "." , IDENT , [ "(" , [expr] , ")" ] | "[" , expr , "]" } ;
primary  = literal | list | call | IDENT | "(" , expr , ")" ;
call     = ( "size" | "has" ) , "(" , expr , ")" ;
list     = "[" , [ expr , { "," , expr } ] , "]" ;
literal  = INT | FLOAT | STRING | "true" | "false" | "null" ;
```

Precedence (tight→loose): postfix → `!` → relational → `&&` → `||` → `?:`.

### Closed callable set (side-constraint 1 · normative)
- free: `size(x)` · `has(x)` (each 1 arg)
- method (zero-arg): `x.size()`
- method (1 string arg): `x.contains(s)` · `x.startsWith(s)` · `x.endsWith(s)`
- `has(x)` = the presence macro · `true` iff `x` resolves to a defined
  non-`null` value (NEVER raises VAR-001 · the safe optional-field test)
- ANY other call suffix = parse error (VAR-005). No regex (`matches`
  reserved). No arithmetic. No `all`/`exists`.

### Typing rules (normative)
- Strong · no implicit coercion · `42 == "42"` is VAR-006 (NOT false).
- `<`/`<=`/`>`/`>=` only on (number, number) or (string, string) ·
  cross-type / on bool/list/map = VAR-006.
- `==`/`!=` across any types: different types ⇒ VAR-006 (per CEL · not
  silently false) EXCEPT a `null` operand (`x == null` is the canonical
  defined-null test · always typed bool · spec 04 §defined-null).
- `&&`/`||`/`!` operands MUST be bool (VAR-006 otherwise) · `in` =
  membership of the left in a right-side list/string.
- `size()` on string (char count) / list / map (len); on anything else
  VAR-006. The ONE v0.1 function (the empty-check idiom).
- ternary `cond ? a : b` · `cond` MUST be bool (VAR-006) · `a`/`b` any
  type (value-selection · not a relation · does NOT count against the
  one-relop rule).

### Resolution (side-constraint 6)
Roots resolve against the host's namespaces: `vars` · `with` · `tasks` ·
`env` · `secrets` + the `for_each` locals `item` · `index`. The
`Resolver` returns the ROOT's value; the computer navigates `.field` /
`[index]` over it. An unresolvable root (resolver `None`) OR a `.field`
on a null/absent OR an out-of-range `[index]` = VAR-001 — EXCEPT under
`has()`, which converts the whole lookup to a bool.

## 4 · Errors (spec-plane · the conformance codes)

| code | when |
|---|---|
| NIKA-VAR-001 | unresolved reference (unknown root · `.field` of null/absent · `[i]` out of range) |
| NIKA-VAR-005 | static grammar violation (bad token · chained relation · unknown call · non-bool `when:` root) |
| NIKA-VAR-006 | type error at compute (cross-type compare · non-bool `&&`/`!`/ternary cond · `size()` of a scalar) |

These are SPEC codes (the 05 table is the canon · `nika_pack::
error_codes()` resolves them) — `CelError::spec_code()` returns the
`&'static str`, doc-pinned like the runtime's `TIMEOUT_CODE`. nika-cel
takes NO nika-error registry range (it is not an engine-internal enum).

## 5 · Tests (the floor)

1. **Lexer** · every token class · string escapes (`\\ \' \" \n \t`) ·
   reserved words (`true`/`false`/`null`/`in` are not idents).
2. **Parser** · every EBNF production · precedence (`a || b && c` =
   `a || (b && c)`) · right-assoc ternary · non-associative rel
   (`a < b < c` = VAR-005) · unknown call = VAR-005 · the grammar's
   own worked examples (03-dag) parse.
3. **Computer** · the typing matrix (each VAR-006 case) · defined-null
   (`tasks.x.output == null` over a skipped task → true) · `has()`
   never raises VAR-001 · `size`/`in`/string-tests · ternary selection.
4. **Roots + shape** · `roots()` enumerates exactly the namespace roots ·
   `is_boolean_shaped()` true for relations/booleans, false for a bare
   ref/literal.
5. **Property** (proptest) · parse is total (never panics · Ok or
   VAR-005) · compute is total (never panics · Ok or a typed VAR error) ·
   parse stability on the canonical forms.
6. **Mutation** · `cargo mutants -p nika-cel` · **0 missed** (re-measured
   2026-06-14 post Gate-11 fixes · 272 mutants · 111 caught · 149 unviable
   · 12 timeouts · was 258/15 before the UTF-8 + recursion-guard fixes).

### Gate 5 budget — the non-termination timeouts (2026-06-14)

<!-- GATE5-EXEMPT: 15 -->

The run reports **0 MISSED** (zero silent survivors). The budgeted
survivors (12 observed 2026-06-14 · budget 15 keeps headroom for the
timing-variable count) are ONE class: scan-cursor mutations that break
the lexer's
forward progress, so its bounded `while i < bytes.len()` loop becomes
unbounded → **non-termination**. They ARE detected — the program hangs,
an observable behaviour change — but cargo-mutants classifies a hang as
TIMEOUT, which the 90% FLOOR ratio counts as a survivor. They are NOT
equivalent mutants and NOT test gaps: the property battery
(`parse_never_panics_*`) would loop forever on each. The 15 are all in
`lexer.rs` (`lex` · `one` · `lex_pair` · `lex_eq` · `lex_bang` ·
`lex_angle` · `lex_ident_or_keyword`) — every cursor `+=` / loop-condition
turned into a non-advancing form. Budget = the structural count of cursor
sites; a 16th survivor (a real MISSED, or a new cursor site) → RED, which
correctly forces a re-review.

## 6 · Non-goals (reserved · later minors · additive)

Arithmetic (`+ - * /`) · `matches()` regex (ReDoS · reserved) ·
`all`/`exists` macros · `map`/`filter` comprehensions · the jq plane
(data extraction is `nika:jq` + `output:` · NOT CEL · spec 04). The
grammar version is `cel-subset/0.1` — later minors only ADD, never
re-mean.

## 7 · Dependencies

`serde_json` (the value currency) · `thiserror` (the error derive).
Dev · `insta` · `proptest`. ZERO other crates (L0 · no kernel · no
async · no I/O). nika-schema (L0) + nika-runtime (L3) consume it; the
runtime's `Scope` implements `Resolver`, the checker walks `roots()` +
`is_boolean_shaped()`.
