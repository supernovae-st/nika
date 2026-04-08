// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TaskBox Management for Chat View
//!
//! Contains TaskBox CRUD operations, completion handlers, and streaming updates.

use super::{
    ActivityItem, BoxState, ChatTaskState, ChatTaskVerb, ChatView, InlineContent, TaskBox,
};

/// Maximum number of inline content items to retain (oldest evicted on overflow)
const MAX_INLINE_CONTENT: usize = 100;
/// Maximum number of activity items to retain (oldest evicted on overflow)
const MAX_ACTIVITY_ITEMS: usize = 50;

// ═══════════════════════════════════════════════════════════════════════════════
// TaskBox Management
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Add a TaskBox to inline content
    ///
    /// This is the unified way to display verb-specific boxes in the chat.
    /// Each verb type (infer, exec, fetch, invoke, agent) has its own visual treatment.
    pub fn add_task_box(&mut self, task_box: TaskBox) {
        let (verb_name, chat_verb) = match &task_box {
            TaskBox::Infer(_) => ("infer", ChatTaskVerb::Infer),
            TaskBox::Exec(_) => ("exec", ChatTaskVerb::Exec),
            TaskBox::Fetch(_) => ("fetch", ChatTaskVerb::Fetch),
            TaskBox::Invoke(_) => ("invoke", ChatTaskVerb::Invoke),
            TaskBox::Agent(_) => ("agent", ChatTaskVerb::Agent),
        };
        let task_id = format!("task-{}-{}", verb_name, self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(task_id.clone(), verb_name));
        Self::enforce_cap(&mut self.activity_items, MAX_ACTIVITY_ITEMS);
        self.add_task_to_queue(&task_id, chat_verb);
        self.update_task_state(&task_id, ChatTaskState::Running, None);
        self.inline_content.push(InlineContent::Task(task_box));
        Self::enforce_cap(&mut self.inline_content, MAX_INLINE_CONTENT);
        self.auto_scroll_to_bottom();
    }

    /// Evict oldest items from a Vec when it exceeds the given cap
    pub(in crate::views::chat) fn enforce_cap<T>(vec: &mut Vec<T>, cap: usize) {
        if vec.len() > cap {
            vec.drain(..vec.len() - cap);
        }
    }

    /// Update the last TaskBox (for streaming updates)
    pub fn update_last_task_box<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TaskBox),
    {
        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            f(task_box);
        }
    }

    /// Complete the last TaskBox with success (generic fallback)
    pub fn complete_last_task_box(&mut self, duration_ms: u64) {
        let verb = if let Some(InlineContent::Task(task_box)) = self.inline_content.last() {
            Some(match task_box {
                TaskBox::Infer(_) => ChatTaskVerb::Infer,
                TaskBox::Exec(_) => ChatTaskVerb::Exec,
                TaskBox::Fetch(_) => ChatTaskVerb::Fetch,
                TaskBox::Invoke(_) => ChatTaskVerb::Invoke,
                TaskBox::Agent(_) => ChatTaskVerb::Agent,
            })
        } else {
            None
        };
        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            *task_box.state_mut() = BoxState::success(duration_ms);
        }
        if let Some(v) = verb {
            self.complete_last_running_task(v, duration_ms);
        }
        self.transition_activity_to_warm("task");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Infer
    pub fn complete_last_infer_box(&mut self, duration_ms: u64) {
        let response = std::mem::take(&mut self.partial_response);
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Infer(infer)) = content {
                if infer.state.is_running() {
                    if !response.is_empty() {
                        infer.response = response.clone();
                    }
                    infer.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Infer, duration_ms);
        self.transition_activity_to_warm("infer");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Exec
    pub fn complete_last_exec_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Exec(exec)) = content {
                if exec.state.is_running() {
                    exec.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Exec, duration_ms);
        self.transition_activity_to_warm("exec");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Fetch
    pub fn complete_last_fetch_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Fetch(fetch)) = content {
                if fetch.state.is_running() {
                    fetch.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Fetch, duration_ms);
        self.transition_activity_to_warm("fetch");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Invoke
    pub fn complete_last_invoke_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.transition_activity_to_warm("invoke");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Invoke with result
    pub fn complete_last_invoke_box_with_result(
        &mut self,
        result: serde_json::Value,
        duration_ms: u64,
    ) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.set_result(result);
                    invoke.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.transition_activity_to_warm("invoke");
        self.auto_scroll_to_bottom();
    }

    /// Fail the last RUNNING TaskBox::Invoke with error
    pub fn fail_last_invoke_box(&mut self, error: &str, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.error = Some(error.to_string());
                    invoke.state = BoxState::failed(error, duration_ms);
                    break;
                }
            }
        }
        self.fail_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Agent
    pub fn complete_last_agent_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Agent(agent)) = content {
                if agent.state.is_running() {
                    agent.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        self.complete_last_running_task(ChatTaskVerb::Agent, duration_ms);
        self.transition_activity_to_warm("agent");
        self.auto_scroll_to_bottom();
    }

    /// Fail the last TaskBox with error
    pub fn fail_last_task_box(&mut self, error: &str, duration_ms: u64) {
        let verb = if let Some(InlineContent::Task(task_box)) = self.inline_content.last() {
            Some(match task_box {
                TaskBox::Infer(_) => ChatTaskVerb::Infer,
                TaskBox::Exec(_) => ChatTaskVerb::Exec,
                TaskBox::Fetch(_) => ChatTaskVerb::Fetch,
                TaskBox::Invoke(_) => ChatTaskVerb::Invoke,
                TaskBox::Agent(_) => ChatTaskVerb::Agent,
            })
        } else {
            None
        };
        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            *task_box.state_mut() = BoxState::failed(error, duration_ms);
        }
        if let Some(v) = verb {
            self.fail_last_running_task(v, duration_ms);
        }
        self.auto_scroll_to_bottom();
    }

    /// Start an inference stream
    pub fn start_infer_stream(&mut self, model: &str, _tokens_in: u64, _max_tokens: u64) {
        self.activity_items.push(ActivityItem::hot(
            format!("infer-{}-{}", model, self.frame),
            "infer",
        ));
        Self::enforce_cap(&mut self.activity_items, MAX_ACTIVITY_ITEMS);
    }

    /// Append content to current inference stream
    pub fn append_infer_content(&mut self, chunk: &str, tokens_out: u64) {
        for content in self.inline_content.iter_mut().rev() {
            match content {
                InlineContent::Task(TaskBox::Infer(infer)) if infer.state.is_running() => {
                    infer.tokens_out = tokens_out;
                    break;
                }
                InlineContent::InferStream(data) => {
                    data.append_content(chunk);
                    data.update_tokens(tokens_out);
                    break;
                }
                _ => continue,
            }
        }
        self.partial_response.push_str(chunk);
    }

    /// Complete current inference stream
    pub fn complete_infer_stream(&mut self) {
        if let Some(InlineContent::InferStream(data)) = self.inline_content.last_mut() {
            data.complete();
        }
        self.transition_activity_to_warm("infer");
    }
}
