//! TaskBox Management for Chat View
//!
//! Contains TaskBox CRUD operations, completion handlers, and streaming updates.
//! Extracted from mod.rs as part of Phase A1 refactoring.

use super::{
    ActivityItem, BoxState, ChatTaskState, ChatTaskVerb, ChatView, InlineContent, TaskBox,
};

// ═══════════════════════════════════════════════════════════════════════════════
// TaskBox Management (v0.12.1)
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Add a TaskBox to inline content
    ///
    /// This is the unified way to display verb-specific boxes in the chat.
    /// Each verb type (infer, exec, fetch, invoke, agent) has its own visual treatment.
    pub fn add_task_box(&mut self, task_box: TaskBox) {
        // Add activity based on verb type
        let (verb_name, chat_verb) = match &task_box {
            TaskBox::Infer(_) => ("infer", ChatTaskVerb::Infer),
            TaskBox::Exec(_) => ("exec", ChatTaskVerb::Exec),
            TaskBox::Fetch(_) => ("fetch", ChatTaskVerb::Fetch),
            TaskBox::Invoke(_) => ("invoke", ChatTaskVerb::Invoke),
            TaskBox::Agent(_) => ("agent", ChatTaskVerb::Agent),
        };

        // v0.12.1: Generate unique task ID for queue tracking
        let task_id = format!("task-{}-{}", verb_name, self.inline_content.len());

        self.activity_items
            .push(ActivityItem::hot(task_id.clone(), verb_name));

        // v0.12.1: Add to task queue and set state to Running
        self.add_task_to_queue(&task_id, chat_verb);
        self.update_task_state(&task_id, ChatTaskState::Running, None);

        self.inline_content.push(InlineContent::Task(Box::new(task_box)));
        self.auto_scroll_to_bottom();
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
        // v0.12.1: Determine verb type for queue update
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

        // v0.12.1: Update task queue state
        if let Some(v) = verb {
            self.complete_last_running_task(v, duration_ms);
        }
        self.transition_activity_to_warm("task");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Infer
    pub fn complete_last_infer_box(&mut self, duration_ms: u64) {
        // v0.12.1: Transfer partial_response to InferBox before completing
        // Option A: InferBox REPLACES the AI message bubble
        let response = std::mem::take(&mut self.partial_response);

        // Find last Infer box that is Running
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Infer(infer)) = content {
                if infer.state.is_running() {
                    // v0.12.1: Set the response content so it displays in the widget
                    if !response.is_empty() {
                        infer.response = response.clone();
                    }
                    infer.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
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
        // v0.12.1: Update task queue state
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
        // v0.12.1: Update task queue state
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
        // v0.12.1: Update task queue state
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
                    invoke.result = Some(result);
                    invoke.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
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
        // v0.12.1: Update task queue state
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
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Agent, duration_ms);
        self.transition_activity_to_warm("agent");
        self.auto_scroll_to_bottom();
    }

    /// Fail the last TaskBox with error
    pub fn fail_last_task_box(&mut self, error: &str, duration_ms: u64) {
        // v0.12.1: Determine verb type before failing for queue update
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

        // v0.12.1: Update task queue state
        if let Some(v) = verb {
            self.fail_last_running_task(v, duration_ms);
        }
        self.auto_scroll_to_bottom();
    }

    /// Start an inference stream
    ///
    /// v0.8 FIX: Don't create InferStream box - use streaming_decrypt for visual effect instead.
    /// The streaming decrypt provides the matrix reveal effect, while InferStream boxes
    /// were redundant and blocked the decrypt from showing.
    pub fn start_infer_stream(&mut self, model: &str, _tokens_in: u32, _max_tokens: u32) {
        // v0.8 FIX: Don't add to inline_content - let streaming_decrypt handle the visual
        // The matrix decrypt effect is the WOW feature, not the INFER boxes

        // Add to activity stack as hot (for Mission Control panel)
        self.activity_items.push(ActivityItem::hot(
            format!("infer-{}-{}", model, self.frame),
            "infer",
        ));
    }

    /// Append content to current inference stream
    /// v0.9.1: Updated to handle TaskBox::Infer (tokens only) - Matrix effect handles text
    pub fn append_infer_content(&mut self, chunk: &str, tokens_out: u32) {
        // Update TaskBox::Infer token count (if present)
        // v0.9.1 FIX: Handle Task(TaskBox::Infer) instead of legacy InferStream
        for content in self.inline_content.iter_mut().rev() {
            match content {
                InlineContent::Task(TaskBox::Infer(infer)) if infer.state.is_running() => {
                    infer.tokens_out = tokens_out;
                    break;
                }
                InlineContent::InferStream(data) => {
                    // Legacy support - update if still used
                    data.append_content(chunk);
                    data.update_tokens(tokens_out);
                    break;
                }
                _ => continue,
            }
        }
        // Matrix effect handles the actual text display via partial_response
        self.partial_response.push_str(chunk);
    }

    /// Complete current inference stream
    pub fn complete_infer_stream(&mut self) {
        if let Some(InlineContent::InferStream(data)) = self.inline_content.last_mut() {
            data.complete();
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("infer");
    }
}
