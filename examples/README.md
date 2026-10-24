# Nika examples — live in the binary

New to Nika? Start with the [first-run walkthrough](../README.md#try-it-in-two-minutes).
It starts with a file-filtering request in the Session, then shows how to use
DeepSeek and inspect the result. You do not need to choose from the full catalog first.

> One source of truth: the [`nika-spec` examples pack](https://github.com/supernovae-st/nika-spec/tree/main/examples),
> vendored into the binary at the spec commit [`SPEC_PIN`](../SPEC_PIN) names.
> No loose copies live here — a second copy is a future lie. To read the
> examples this engine embeds before you install anything, browse
> [`crates/nika-pack/pack/examples/`](../crates/nika-pack/pack/examples/).

```sh
nika try                         # the showroom: the numbered path + the jobs
nika try 01-hello                # prove one, offline (mock rehearsal · zero keys)
nika compile hello hello.nika    # create the offline lesson at an explicit path
```

`try` previews an embedded example. `compile` creates a workflow file you can
inspect, edit and keep. The hello rehearsal uses `mock/echo`, not a real AI answer.

The corpus is organized as **the path** (numbered foundation lessons ·
complete construct coverage, including extract-then-law) then **the jobs**
(real showcase workflows).
Skeletons to start your own: `nika compile --list` lists the set.
