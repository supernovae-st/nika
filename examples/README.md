# Nika examples — live in the binary

New to Nika? Start with the [first-run walkthrough](../README.md#start-here).
It turns your meeting transcript into a saved action list using your existing
API or Codex access. You do not need to choose from the full catalog first.

> One source of truth: the [`nika-spec`](https://github.com/supernovae-st/nika-spec)
> examples pack, vendored into the binary (`crates/nika-pack/pack/`). No
> loose copies live here — a second copy is a future lie.

```sh
nika try                         # the showroom: the numbered path + the jobs
nika try 01-hello                # prove one, offline (mock rehearsal · zero keys)
nika new 01-hello                # make one yours (ingredients included)
```

`try` previews an embedded example. `new` copies one into your folder so you
can edit and keep it. The hello rehearsal uses `mock/echo`, not a real AI answer.

The corpus is organized as **the path** (numbered foundation lessons ·
complete construct coverage, including extract-then-law) then **the jobs**
(real showcase workflows).
Skeletons to start your own: `nika new '?'` lists the set.
