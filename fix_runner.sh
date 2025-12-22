#!/bin/bash
# Fix script for runner.rs execute methods
# This updates all execute methods to use the new Format 4 nested structure

cat > runner_fixes.txt << 'EOF'
# Fix execute_subagent
s/let (subagent_prompt, model, system_prompt, allowed_tools) = match &task.action {/let subagent_def = match \&task.action {/
s/TaskAction::Subagent { subagent, model, system_prompt, allowed_tools, .. } => {/TaskAction::Subagent { subagent } => subagent,/
s/(subagent.as_str(), model.as_deref(), system_prompt.as_deref(), allowed_tools.clone())/PLACEHOLDER/
s/let prompt = resolve_templates(subagent_prompt, ctx)/let prompt = resolve_templates(\&subagent_def.prompt, ctx)/
s/PromptRequest::new(&prompt, model.unwrap_or(&workflow.agent.model))/PromptRequest::new(\&prompt, subagent_def.model.as_deref().unwrap_or(\&workflow.agent.model))/
s/system_prompt$/subagent_def.system_prompt.as_deref()/
s/.with_tools(allowed_tools.unwrap_or_default())/.with_tools(subagent_def.allowed_tools.clone().unwrap_or_default())/

# Fix execute_shell
s/let shell_cmd = match &task.action {/let shell_def = match \&task.action {/
s/TaskAction::Shell { shell, .. } => shell.as_str(),/TaskAction::Shell { shell } => shell,/
s/let cmd_str = resolve_templates(shell_cmd, ctx)/let cmd_str = resolve_templates(\&shell_def.command, ctx)/

# Fix execute_http
s/let (url, method) = match &task.action {/let http_def = match \&task.action {/
s/TaskAction::Http { http_config } => match http_config {/TaskAction::Http { http } => http,/
s/HttpConfig::Simple { http, method, .. } => (http.as_str(), method.as_deref().unwrap_or("GET")),/PLACEHOLDER/
s/HttpConfig::Complex { http } => (&http.url\[..\], http.method.as_deref().unwrap_or("GET")),/PLACEHOLDER/
s/let resolved_url = resolve_templates(url, ctx)/let resolved_url = resolve_templates(\&http_def.url, ctx)/
s/format!("\[http\] Would {} {}", method, resolved_url)/format!("[http] Would {} {}", http_def.method.as_deref().unwrap_or("GET"), resolved_url)/

# Fix execute_mcp
s/let (mcp, args) = match &task.action {/let mcp_def = match \&task.action {/
s/TaskAction::Mcp { mcp, args } => (mcp.as_str(), args.as_ref()),/TaskAction::Mcp { mcp } => mcp,/
s/let args_str = resolve_args(args, ctx)/let args_str = resolve_args(mcp_def.args.as_ref(), ctx)/
s/format!("\[mcp\] Would call {} with args: {}", mcp, args_str)/format!("[mcp] Would call {} with args: {}", mcp_def.reference, args_str)/

# Fix execute_function
s/let (func, args) = match &task.action {/let func_def = match \&task.action {/
s/TaskAction::Function { function, args } => (function.as_str(), args.as_ref()),/TaskAction::Function { function } => function,/
s/let args_str = resolve_args(args, ctx)/let args_str = resolve_args(func_def.args.as_ref(), ctx)/
s/format!("\[function\] Would call {} with args: {}", func, args_str)/format!("[function] Would call {} with args: {}", func_def.reference, args_str)/

# Fix execute_llm
s/let llm_prompt = match &task.action {/let llm_def = match \&task.action {/
s/TaskAction::Llm { llm, .. } => llm.as_str(),/TaskAction::Llm { llm } => llm,/
s/let prompt = resolve_templates(llm_prompt, ctx)/let prompt = resolve_templates(\&llm_def.prompt, ctx)/
EOF

echo "This script would fix runner.rs but needs more work. Doing it manually instead."