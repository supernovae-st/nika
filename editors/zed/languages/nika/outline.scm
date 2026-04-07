; Nika Workflow Language — Code Outline for Zed
;
; Shows workflow structure in the outline panel:
; - Workflow name
; - Task IDs
; - Top-level sections (tasks, inputs, context, skills, etc.)

; Workflow name at the top level
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "workflow")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @name))
  (#set! item.kind "module")) @item

; Task ID entries — the most important outline items
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "id")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @name))
  (#set! item.kind "function")) @item

; Top-level sections
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @name
      (#match? @name "^(tasks|inputs|context|skills|artifacts|mcp|agents|schedule)$")))
  (#set! item.kind "interface")) @item
