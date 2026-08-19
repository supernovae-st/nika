// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bare `nika` on a TTY — one thread over the existing workflow surfaces.

// The thread owns a live terminal, like `run`: environment failures must
// reach that terminal immediately rather than being deferred as a value.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Theme;
use crate::verbs::{VerbOutput, exit};

const DEFAULT_MODEL: &str = "xai/grok-4";
const HELP: &str = "text                 talk in this thread\n\
/model provider/model choose the thread model\n\
/list                list workflows below here\n\
/workflow <path>     post a workflow card\n\
/run <path>          run a workflow in this thread\n\
/quit                close the thread\n";
const HISTORY_LIMIT: usize = 12;
static TURN_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default)]
struct Turn {
    answer: String,
    interrupted: bool,
}

impl Turn {
    fn answered(answer: String) -> Self {
        Self {
            answer,
            interrupted: false,
        }
    }

    fn interrupted() -> Self {
        Self {
            answer: String::new(),
            interrupted: true,
        }
    }
}

trait ThreadRuntime {
    fn talk(&mut self, prompt: &str, model: &str, theme: Theme) -> Turn;
    fn post(&mut self, path: &str, theme: Theme) -> VerbOutput;
    fn run_workflow(&mut self, path: &str, theme: Theme) -> Turn;
    fn list(&mut self) -> VerbOutput;
}

struct LiveRuntime;

impl ThreadRuntime for LiveRuntime {
    fn talk(&mut self, prompt: &str, model: &str, theme: Theme) -> Turn {
        let path = turn_path();
        let source = turn_workflow(prompt, model);
        if let Err(error) = std::fs::write(&path, source) {
            eprintln!("nika: cannot stage thread turn: {error}");
            return Turn::answered(String::new());
        }
        let result = super::run::run_in_thread(&path.to_string_lossy(), theme);
        let _ = std::fs::remove_file(path);
        if result.interrupted {
            Turn::interrupted()
        } else {
            Turn::answered(result.answer.unwrap_or_default())
        }
    }

    fn post(&mut self, path: &str, theme: Theme) -> VerbOutput {
        super::inspect::run(path, theme)
    }

    fn run_workflow(&mut self, path: &str, theme: Theme) -> Turn {
        let result = super::run::run_in_thread(path, theme);
        if result.interrupted {
            Turn::interrupted()
        } else {
            Turn::answered(String::new())
        }
    }

    fn list(&mut self) -> VerbOutput {
        super::list::run(Path::new("."))
    }
}

struct Thread {
    model: String,
    history: Vec<(String, String)>,
}

impl Thread {
    fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_owned(),
            history: Vec::new(),
        }
    }

    fn prompt(&self, message: &str) -> String {
        let mut prompt = String::from(
            "Continue this conversation. Answer directly and conversationally, without a ceremonial preamble.\n\n",
        );
        let start = self.history.len().saturating_sub(HISTORY_LIMIT);
        for (user, assistant) in &self.history[start..] {
            let _ = writeln!(prompt, "user: {user}\nassistant: {assistant}");
        }
        let _ = write!(prompt, "user: {message}\nassistant:");
        prompt
    }
}

fn turn_path() -> std::path::PathBuf {
    let id = TURN_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("nika-thread-{}-{id}.nika.yaml", std::process::id()))
}

fn turn_workflow(prompt: &str, model: &str) -> String {
    let model = serde_json::to_string(model).unwrap_or_else(|_| format!("\"{DEFAULT_MODEL}\""));
    let prompt = serde_json::to_string(prompt).unwrap_or_else(|_| "\"\"".to_owned());
    format!(
        "nika: interactive-turn\nmodel: {model}\npermits: {{}}\ntasks:\n  reply:\n    agent:\n      prompt: {prompt}\n      tools: []\n      max_turns: 4\n      max_tokens_total: 4096\noutputs:\n  reply: ${{{{ tasks.reply.output }}}}\n"
    )
}

fn show<W: Write>(output: &mut W, value: &VerbOutput) -> std::io::Result<()> {
    if !value.text.is_empty() {
        writeln!(output, "{}", value.text.trim_end())?;
    }
    Ok(())
}

fn finish_turn<W: Write>(output: &mut W, turn: &Turn) -> std::io::Result<()> {
    if turn.interrupted {
        writeln!(output, "interrupted · thread stays open")
    } else if turn.answer.is_empty() {
        Ok(())
    } else {
        writeln!(output, "\n◂ {}", turn.answer)
    }
}

fn drive<R: BufRead, W: Write, T: ThreadRuntime>(
    input: &mut R,
    output: &mut W,
    theme: Theme,
    runtime: &mut T,
) -> std::io::Result<u8> {
    let mut thread = Thread::new();
    writeln!(output, "nika · thread")?;
    writeln!(output, "model {} · /help", thread.model)?;
    loop {
        write!(output, "\nnika › ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(exit::OK);
        }
        let message = line.trim();
        if message.is_empty() {
            continue;
        }
        match message.split_once(' ') {
            _ if matches!(message, "/quit" | "/exit") => return Ok(exit::OK),
            _ if message == "/help" => write!(output, "{HELP}")?,
            _ if message == "/list" => show(output, &runtime.list())?,
            Some(("/model", model)) if model.contains('/') => {
                model.trim().clone_into(&mut thread.model);
                writeln!(output, "model {}", thread.model)?;
            }
            _ if message == "/model" => writeln!(output, "model {}", thread.model)?,
            Some(("/model", _)) => writeln!(output, "use /model provider/model")?,
            Some(("/workflow", path)) if !path.trim().is_empty() => {
                show(output, &runtime.post(path.trim(), theme))?;
            }
            Some(("/run", path)) if !path.trim().is_empty() => {
                finish_turn(output, &runtime.run_workflow(path.trim(), theme))?;
            }
            _ if message.starts_with('/') => writeln!(output, "unknown thread command · /help")?,
            _ => {
                let prompt = thread.prompt(message);
                let turn = runtime.talk(&prompt, &thread.model, theme);
                finish_turn(output, &turn)?;
                if !turn.interrupted && !turn.answer.is_empty() {
                    thread
                        .history
                        .push((message.to_owned(), turn.answer.clone()));
                }
            }
        }
    }
}

/// Open one local interactive thread over Nika's existing run surfaces.
#[must_use]
pub fn run(theme: Theme) -> u8 {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout();
    let mut runtime = LiveRuntime;
    match drive(&mut input, &mut output, theme, &mut runtime) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("nika: thread I/O failed: {error}");
            exit::ENV
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[derive(Default)]
    struct FakeRuntime {
        prompts: Vec<(String, String)>,
        posted: Vec<String>,
        runs: Vec<String>,
        interrupt_first: bool,
    }

    impl ThreadRuntime for FakeRuntime {
        fn talk(&mut self, prompt: &str, model: &str, _theme: Theme) -> Turn {
            self.prompts.push((prompt.to_owned(), model.to_owned()));
            if self.interrupt_first {
                self.interrupt_first = false;
                return Turn::interrupted();
            }
            Turn::answered(format!("answer-{}", self.prompts.len()))
        }

        fn post(&mut self, path: &str, _theme: Theme) -> VerbOutput {
            self.posted.push(path.to_owned());
            VerbOutput::ok(format!("card {path}\n"))
        }

        fn run_workflow(&mut self, path: &str, _theme: Theme) -> Turn {
            self.runs.push(path.to_owned());
            Turn::answered(String::new())
        }

        fn list(&mut self) -> VerbOutput {
            VerbOutput::ok("a.nika.yaml\nnested/b.nika.yaml\n".to_owned())
        }
    }

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    #[test]
    fn one_thread_routes_talk_list_post_run_and_model_without_modes() {
        let script = concat!(
            "/model xai/grok-4\n",
            "hello\n",
            "/list\n",
            "/workflow nested/b.nika.yaml\n",
            "/run a.nika.yaml\n",
            "again\n",
            "/quit\n",
        );
        let mut input = Cursor::new(script.as_bytes());
        let mut output = Vec::new();
        let mut runtime = FakeRuntime::default();

        let code = drive(&mut input, &mut output, plain(), &mut runtime).expect("thread");
        let shown = String::from_utf8(output).expect("utf8");

        assert_eq!(code, exit::OK);
        assert_eq!(runtime.posted, ["nested/b.nika.yaml"]);
        assert_eq!(runtime.runs, ["a.nika.yaml"]);
        assert_eq!(runtime.prompts.len(), 2);
        assert_eq!(runtime.prompts[0].1, "xai/grok-4");
        assert!(runtime.prompts[1].0.contains("assistant: answer-1"));
        assert!(shown.contains("a.nika.yaml\nnested/b.nika.yaml"));
        assert!(shown.contains("card nested/b.nika.yaml"));
    }

    #[test]
    fn an_interrupted_turn_returns_to_the_same_prompt() {
        let mut input = Cursor::new(b"first\nsecond\n/quit\n");
        let mut output = Vec::new();
        let mut runtime = FakeRuntime {
            interrupt_first: true,
            ..FakeRuntime::default()
        };

        let code = drive(&mut input, &mut output, plain(), &mut runtime).expect("thread");
        let shown = String::from_utf8(output).expect("utf8");

        assert_eq!(code, exit::OK);
        assert_eq!(runtime.prompts.len(), 2);
        assert_eq!(runtime.prompts[0].1, "xai/grok-4");
        assert!(shown.contains("interrupted · thread stays open"));
        assert!(shown.contains("answer-2"));
    }
}
