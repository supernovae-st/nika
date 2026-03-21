//! Semantic token handler — syntax coloring.

#[derive(Debug, Clone)]
pub struct RawToken {
    pub line: u32,
    pub start_char: u32,
    pub length: u32,
    pub token_type: TokenType,
    pub modifiers: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Keyword,
    Verb,
    Property,
    Variable,
    Function,
    Type,
    Comment,
}

impl TokenType {
    pub fn index(self) -> u32 {
        match self {
            Self::Keyword => 0,
            Self::Verb => 1,
            Self::Property => 2,
            Self::Variable => 3,
            Self::Function => 4,
            Self::Type => 5,
            Self::Comment => 6,
        }
    }
}

const VERBS: &[&str] = &["infer", "exec", "fetch", "invoke", "agent"];
const TOP_KEYS: &[&str] = &[
    "schema", "workflow", "tasks", "mcp", "context", "inputs", "include", "imports", "edges",
];
const TASK_KEYS: &[&str] = &[
    "id",
    "with",
    "depends_on",
    "content",
    "for_each",
    "retry",
    "timeout",
    "guardrails",
    "guard",
    "limits",
    "provider",
    "model",
    "description",
    "structured",
    "output",
    "on_error",
    "as",
    "concurrency",
    "fail_fast",
];
const VERB_SUBKEYS: &[&str] = &[
    "prompt",
    "system",
    "temperature",
    "max_tokens",
    "extended_thinking",
    "thinking_budget",
    "command",
    "shell",
    "cwd",
    "url",
    "method",
    "headers",
    "body",
    "extract",
    "selector",
    "response",
    "follow_redirects",
    "mcp",
    "tool",
    "params",
    "resource",
    "max_turns",
    "depth_limit",
    "tools",
    "goal",
    "detail",
    "source",
];

pub fn semantic_tokens(text: &str) -> Vec<RawToken> {
    let mut tokens = Vec::new();
    for (li, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let indent = (line.len() - trimmed.len()) as u32;
        if trimmed.starts_with('#') {
            tokens.push(RawToken {
                line: li as u32,
                start_char: indent,
                length: trimmed.len() as u32,
                token_type: TokenType::Comment,
                modifiers: 0,
            });
            continue;
        }
        // Templates
        let mut s = 0;
        while let Some(start) = line[s..].find("{{") {
            let abs = s + start;
            if let Some(end) = line[abs..].find("}}") {
                tokens.push(RawToken {
                    line: li as u32,
                    start_char: abs as u32,
                    length: (end + 2) as u32,
                    token_type: TokenType::Variable,
                    modifiers: 0,
                });
                if let Some(p) = line[abs + 2..abs + end].find('|') {
                    let t = line[abs + 2 + p + 1..abs + end].trim();
                    if !t.is_empty() {
                        let ts = abs
                            + 2
                            + p
                            + 1
                            + (line[abs + 2 + p + 1..abs + end].len()
                                - line[abs + 2 + p + 1..abs + end].trim_start().len());
                        tokens.push(RawToken {
                            line: li as u32,
                            start_char: ts as u32,
                            length: t.len() as u32,
                            token_type: TokenType::Function,
                            modifiers: 0,
                        });
                    }
                }
                s = abs + end + 2;
            } else {
                break;
            }
        }
        if let Some(cp) = trimmed.find(':') {
            let key = trimmed[..cp].trim().trim_start_matches("- ");
            if key.is_empty() {
                continue;
            }
            let ks = line.find(key).unwrap_or(indent as usize);
            let tt = if VERBS.contains(&key) {
                TokenType::Verb
            } else if indent == 0 && TOP_KEYS.contains(&key) {
                TokenType::Keyword
            } else if TASK_KEYS.contains(&key) {
                TokenType::Property
            } else if VERB_SUBKEYS.contains(&key) {
                TokenType::Property
            } else if key == "type" {
                TokenType::Type
            } else {
                continue;
            };
            tokens.push(RawToken {
                line: li as u32,
                start_char: ks as u32,
                length: key.len() as u32,
                token_type: tt,
                modifiers: 0,
            });
        }
    }
    tokens
}

pub fn encode_tokens(tokens: &[RawToken]) -> Vec<u32> {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let (mut pl, mut ps) = (0u32, 0u32);
    for t in tokens {
        let dl = t.line - pl;
        let ds = if dl == 0 {
            t.start_char - ps
        } else {
            t.start_char
        };
        data.extend_from_slice(&[dl, ds, t.length, t.token_type.index(), t.modifiers]);
        pl = t.line;
        ps = t.start_char;
    }
    data
}

pub fn token_legend() -> Vec<&'static str> {
    vec![
        "keyword", "keyword", "property", "variable", "function", "type", "comment",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments() {
        assert!(semantic_tokens("# hi\n")
            .iter()
            .any(|t| t.token_type == TokenType::Comment));
    }
    #[test]
    fn verbs() {
        assert!(semantic_tokens("    infer: x")
            .iter()
            .any(|t| t.token_type == TokenType::Verb));
    }
    #[test]
    fn templates() {
        assert!(semantic_tokens("  x: \"{{with.d}}\"")
            .iter()
            .any(|t| t.token_type == TokenType::Variable));
    }
    #[test]
    fn encode() {
        let d = encode_tokens(&[RawToken {
            line: 0,
            start_char: 0,
            length: 5,
            token_type: TokenType::Keyword,
            modifiers: 0,
        }]);
        assert_eq!(d.len(), 5);
    }
    #[test]
    fn empty() {
        assert!(semantic_tokens("").is_empty());
    }
    #[test]
    fn all_verbs() {
        for v in VERBS {
            assert!(semantic_tokens(&format!("    {v}: x"))
                .iter()
                .any(|t| t.token_type == TokenType::Verb));
        }
    }
}
