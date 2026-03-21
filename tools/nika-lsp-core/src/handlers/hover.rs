//! Hover handler — documentation on hover.
use crate::analysis::context::CursorContext;

#[derive(Debug, Clone)]
pub struct HoverResult {
    pub contents: String,
    pub range: Option<(u32, u32)>,
}

pub fn hover(_text: &str, _offset: u32, context: &CursorContext) -> Option<HoverResult> {
    match context {
        CursorContext::VerbBlock { verb, .. } => verb_hover(verb),
        CursorContext::TaskField { prefix, .. } => field_hover(prefix),
        CursorContext::WorkflowRoot { prefix } => root_key_hover(prefix),
        CursorContext::ContentPart { focus, .. } => content_hover(focus),
        CursorContext::WithBlock { .. } => Some(HoverResult {
            contents: "## `with:` — Data Bindings\n\nBind outputs from upstream tasks.".to_string(),
            range: None,
        }),
        CursorContext::Template {
            in_transform_chain: true,
            partial_expr,
            ..
        } => transform_hover(partial_expr),
        CursorContext::Template { .. } => Some(HoverResult {
            contents: "## Template Expression\n\nAccess data: `{{with.alias}}`, `{{inputs.param}}`"
                .to_string(),
            range: None,
        }),
        _ => None,
    }
}

fn verb_hover(verb: &str) -> Option<HoverResult> {
    let doc = match verb {
    "infer" => "## `infer:` — LLM Generation\n\nSend prompt to LLM. Supports `content:` for vision.",
    "exec" => "## `exec:` — Shell Command\n\nRun shell command, capture output.",
    "fetch" => "## `fetch:` — HTTP Request\n\nMake HTTP request, capture response.",
    "invoke" => "## `invoke:` — MCP Tool Call\n\nCall tool on MCP server. Also supports `nika:*` builtins.",
    "agent" => "## `agent:` — Multi-Turn Agent\n\nAutonomous agent with tools and reasoning.",
    _ => return None,
  };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

fn field_hover(prefix: &str) -> Option<HoverResult> {
    let key = prefix.trim().trim_end_matches(':');
    let doc = match key {
        "id" => "**Task ID** — Unique identifier.",
        "with" => "**Data Bindings** — `alias: $task_id`",
        "depends_on" => "**Dependencies** — Pure ordering edges.",
        "content" => "**Vision Content** — Multimodal parts (text/image/image_url).",
        "for_each" => "**Parallel Loop** — Iterate over array.",
        "retry" => "**Retry Policy** — Retry with backoff.",
        "timeout" => "**Timeout** — Max seconds.",
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

fn root_key_hover(prefix: &str) -> Option<HoverResult> {
    let key = prefix.trim().trim_end_matches(':');
    let doc = match key {
        "schema" => "**Schema** — `nika/workflow@0.12`",
        "tasks" => "**Tasks** — DAG of tasks.",
        "mcp" => "**MCP** — Server configs.",
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

fn content_hover(focus: &crate::analysis::context::ContentFocus) -> Option<HoverResult> {
    use crate::analysis::context::ContentFocus;
    let doc = match focus {
        ContentFocus::PartType => "**Content Type** — text, image, image_url",
        ContentFocus::ImageDetail => "**Detail** — auto, low, high",
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

fn transform_hover(expr: &str) -> Option<HoverResult> {
    let t = expr.rsplit('|').next()?.trim();
    let doc = match t {
        "upper" => "UPPERCASE",
        "lower" => "lowercase",
        "trim" => "Strip whitespace",
        _ => return None,
    };
    Some(HoverResult {
        contents: format!("`{t}` — {doc}"),
        range: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_verbs() {
        for v in ["infer", "exec", "fetch", "invoke", "agent"] {
            assert!(verb_hover(v).is_some());
        }
    }
    #[test]
    fn unknown_none() {
        let c = CursorContext::Unknown {
            prefix: String::new(),
        };
        assert!(hover("", 0, &c).is_none());
    }
    #[test]
    fn content_hover_type() {
        use crate::analysis::context::ContentFocus;
        assert!(content_hover(&ContentFocus::PartType).is_some());
    }
}
