# Legacy Lookup — safe brouillon branch reference

Look up code from the legacy `brouillon` branch (v0.79 · `tools/` layout)
WITHOUT checking out. `main` is the Diamond production branch — legacy
lives on `brouillon` (renamed 2026-05-06).
Usage: `/legacy-lookup tools/nika-core/src/error.rs`

## Argument

$ARGUMENTS = path to look up on brouillon, OR a search query

## Behavior

If $ARGUMENTS looks like a file path:
```bash
git show "brouillon:$ARGUMENTS"
```

If $ARGUMENTS looks like a crate name (e.g., "nika-core"):
```bash
# Show the crate's lib.rs
git show "brouillon:tools/$ARGUMENTS/src/lib.rs" | head -50
# Show crate structure
git ls-tree -r brouillon "tools/$ARGUMENTS/src/" | awk '{print $4}'
# Show LOC
git ls-tree -r brouillon "tools/$ARGUMENTS/src/" | awk '{print $4}' | while read f; do
  git show "brouillon:$f" | wc -l
done | awk '{s+=$1} END {print "Total:", s, "LOC"}'
```

If $ARGUMENTS looks like a function name:
```bash
# Find where this function lives on brouillon
git grep -n "fn $ARGUMENTS" brouillon -- '*.rs' | head -10
```

## Rules

- NEVER `git checkout brouillon`
- NEVER modify anything on brouillon
- Read-only reference via `git show` and `git grep`
- This is for understanding how legacy did things, to guide the REWRITE (not copy-paste)
