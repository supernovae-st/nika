# Nika examples — live in the binary

> One source of truth: the [`nika-spec`](https://github.com/supernovae-st/nika-spec)
> examples pack, vendored into the binary (`crates/nika-pack/pack/`). No
> loose copies live here — a second copy is a future lie.

```sh
nika examples                    # the path (01-07) + the showcase (T1→T4)
nika examples show 01-hello      # read one
nika examples run 01-hello --model mock/echo   # prove one, offline
nika examples copy 01-hello      # make one yours
```

The corpus is organized as **the path** (7 foundation steps · complete
v0.1 construct coverage) then **showcase tiers T1→T4** (real jobs).
Skeletons to start your own live under `nika new --from <template>`.
