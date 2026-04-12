// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shell execution and HTTP fetch commands
//!
//! Handles the `exec:` and `fetch:` TUI commands for running shell commands
//! and making HTTP requests.

use nika_engine::error::NikaError;

use super::ChatAgent;

impl ChatAgent {
    /// Execute a shell command
    ///
    /// Uses `tokio::process::Command` for non-blocking execution.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    ///
    /// # Returns
    ///
    /// The command output (stdout) on success, or formatted error on failure.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ExecError` if the command fails to execute.
    ///
    /// # Safety
    ///
    /// This executes arbitrary shell commands. Use with caution.
    pub async fn exec_command(&self, command: &str) -> Result<String, NikaError> {
        use tokio::process::Command as TokioCommand;

        // SEC: Validate command against blocklist before shell execution.
        // Uses shell-mode validation since we execute via `sh -c`.
        nika_security::validate_exec_command_with_shell(command, true)?;

        let output = TokioCommand::new("sh")
            .arg("-c")
            .arg(command)
            .kill_on_drop(true) // invariant #11
            .output()
            .await
            .map_err(|e| NikaError::ExecError {
                reason: format!("Failed to execute command: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.trim().to_string())
        } else {
            // Return formatted output including exit code and stderr
            let exit_code = output.status.code().unwrap_or(-1);
            Ok(format!(
                "Exit code: {}\n{}\n{}",
                exit_code,
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    /// Execute a fetch command (HTTP request)
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch
    /// * `method` - HTTP method (GET, POST, PUT, DELETE)
    ///
    /// # Returns
    ///
    /// The response body as text.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::FetchError` if the HTTP request fails.
    pub async fn fetch(&self, url: &str, method: &str) -> Result<String, NikaError> {
        let request = match method.to_uppercase().as_str() {
            "POST" => self.http_client.post(url),
            "PUT" => self.http_client.put(url),
            "DELETE" => self.http_client.delete(url),
            "PATCH" => self.http_client.patch(url),
            "HEAD" => self.http_client.head(url),
            _ => self.http_client.get(url), // Default to GET
        };

        let response = request.send().await.map_err(|e| NikaError::FetchError {
            reason: format!("HTTP request failed: {}", e),
        })?;

        let status = response.status();
        let text = response.text().await.map_err(|e| NikaError::FetchError {
            reason: format!("Failed to read response: {}", e),
        })?;

        // Include status code for non-2xx responses
        if !status.is_success() {
            Ok(format!(
                "HTTP {} {}\n{}",
                status.as_u16(),
                status.as_str(),
                text
            ))
        } else {
            Ok(text)
        }
    }
}
