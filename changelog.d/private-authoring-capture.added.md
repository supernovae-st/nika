Add explicit, bounded private capture for returned Compile authoring Text.
Keep capture off by default, withhold resolved credentials, preserve compiler
outcomes when storage is unavailable, and retain reservation and close facts.
`CompileCommand` gains a `capture` field; its existing constructor keeps
capture off. `CompileArgs` stays unchanged.
