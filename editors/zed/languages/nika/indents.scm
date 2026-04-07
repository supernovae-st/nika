; Nika Workflow Language — Indentation Rules for Zed
;
; YAML is indentation-based. Increase indent after mapping keys
; that introduce nested content.

; Block mappings increase indent
(block_mapping) @indent

; Block sequences increase indent
(block_sequence) @indent

; Flow constructs indent their contents
(flow_mapping "}" @end) @indent
(flow_sequence "]" @end) @indent
