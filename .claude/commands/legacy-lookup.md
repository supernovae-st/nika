# Legacy Lookup — safe main branch reference

Look up code from the legacy brouillon branch WITHOUT checking out.
Usage: `/legacy-lookup tools/nika-core/src/error.rs`

## Argument

$ARGUMENTS = path to look up on main, OR a search query

## Behavior

If $ARGUMENTS looks like a file path:
```bash
git show "main:$ARGUMENTS"
```

If $ARGUMENTS looks like a crate name (e.g., "nika-core"):
```bash
# Show the crate's lib.rs
git show "main:tools/$ARGUMENTS/src/lib.rs" | head -50
# Show crate structure
git ls-tree -r main "tools/$ARGUMENTS/src/" | awk '{print $4}'
# Show LOC
git ls-tree -r main "tools/$ARGUMENTS/src/" | awk '{print $4}' | while read f; do
  git show "main:$f" | wc -l
done | awk '{s+=$1} END {print "Total:", s, "LOC"}'
```

If $ARGUMENTS looks like a function name:
```bash
# Find where this function lives on main
git grep -n "fn $ARGUMENTS" main -- '*.rs' | head -10
```

## Rules

- NEVER `git checkout brouillon`
- NEVER modify anything on main
- Read-only reference via `git show` and `git grep`
- This is for understanding how legacy did things, to guide the REWRITE (not copy-paste)
