; Runnables — inline "Run ▶" buttons for Nika workflows
;
; Detects `workflow: my-name` declarations and shows a run button.
; Maps to the `nika-run` task tag in .zed/tasks.json.

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key))
  value: (flow_node
    (plain_scalar
      (string_scalar) @run))
  (#eq? @_key "workflow"))
