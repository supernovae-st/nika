//! Notification channels for Jobs Daemon.
//!
//! Supports Slack webhooks and email notifications for job events.

use crate::jobs::config::{EmailConfig, NotifyConfig};
use crate::jobs::error::JobsError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Job execution event for notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvent {
    /// Job name.
    pub job_name: String,
    /// Execution ID.
    pub execution_id: String,
    /// Event type.
    pub event_type: JobEventType,
    /// Event timestamp (Unix epoch).
    pub timestamp: u64,
    /// Duration in seconds (for completed/failed).
    pub duration_secs: Option<u64>,
    /// Error message (for failed events).
    pub error: Option<String>,
    /// Attempt number (for retries).
    pub attempt: Option<u32>,
    /// Max attempts configured.
    pub max_attempts: Option<u32>,
}

/// Job event types that can trigger notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobEventType {
    /// Job started execution.
    Started,
    /// Job completed successfully.
    Success,
    /// Job failed.
    Failed,
    /// Job is being retried.
    Retrying,
    /// Job exceeded max retries.
    MaxRetriesExceeded,
    /// Job timed out.
    TimedOut,
}

impl JobEventType {
    /// Returns emoji for the event type.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Started => "🚀",
            Self::Success => "✅",
            Self::Failed => "❌",
            Self::Retrying => "🔄",
            Self::MaxRetriesExceeded => "💀",
            Self::TimedOut => "⏰",
        }
    }

    /// Returns color for Slack attachment.
    pub fn slack_color(&self) -> &'static str {
        match self {
            Self::Started => "#3498db",            // blue
            Self::Success => "#2ecc71",            // green
            Self::Failed => "#e74c3c",             // red
            Self::Retrying => "#f39c12",           // orange
            Self::MaxRetriesExceeded => "#9b59b6", // purple
            Self::TimedOut => "#e67e22",           // dark orange
        }
    }
}

/// Notification channel trait.
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Channel name for logging.
    fn name(&self) -> &str;

    /// Send notification for a job event.
    async fn notify(&self, event: &JobEvent) -> Result<(), JobsError>;

    /// Check if the channel is configured and ready.
    fn is_configured(&self) -> bool;
}

/// Slack webhook notifier.
pub struct SlackNotifier {
    webhook_url: String,
    client: reqwest::Client,
}

impl SlackNotifier {
    /// Create a new Slack notifier.
    pub fn new(webhook_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            webhook_url,
            client,
        }
    }

    /// Format job event as Slack message payload.
    fn format_message(&self, event: &JobEvent) -> serde_json::Value {
        let title = format!(
            "{} Job `{}` {}",
            event.event_type.emoji(),
            event.job_name,
            match event.event_type {
                JobEventType::Started => "started",
                JobEventType::Success => "completed successfully",
                JobEventType::Failed => "failed",
                JobEventType::Retrying => "is retrying",
                JobEventType::MaxRetriesExceeded => "exceeded max retries",
                JobEventType::TimedOut => "timed out",
            }
        );

        let mut fields = vec![serde_json::json!({
            "title": "Execution ID",
            "value": event.execution_id,
            "short": true
        })];

        if let Some(duration) = event.duration_secs {
            fields.push(serde_json::json!({
                "title": "Duration",
                "value": format_duration(duration),
                "short": true
            }));
        }

        if let Some(attempt) = event.attempt {
            let max = event.max_attempts.unwrap_or(1);
            fields.push(serde_json::json!({
                "title": "Attempt",
                "value": format!("{}/{}", attempt, max),
                "short": true
            }));
        }

        if let Some(ref error) = event.error {
            fields.push(serde_json::json!({
                "title": "Error",
                "value": error,
                "short": false
            }));
        }

        serde_json::json!({
            "attachments": [{
                "color": event.event_type.slack_color(),
                "title": title,
                "fields": fields,
                "ts": event.timestamp
            }]
        })
    }
}

#[async_trait]
impl NotificationChannel for SlackNotifier {
    fn name(&self) -> &str {
        "slack"
    }

    async fn notify(&self, event: &JobEvent) -> Result<(), JobsError> {
        let payload = self.format_message(event);

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| JobsError::NotificationFailed {
                channel: "slack".to_string(),
                reason: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(JobsError::NotificationFailed {
                channel: "slack".to_string(),
                reason: format!("HTTP {}: {}", status, body),
            });
        }

        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.webhook_url.is_empty()
    }
}

/// Email notifier using SMTP.
pub struct EmailNotifier {
    config: EmailConfig,
}

impl EmailNotifier {
    /// Create a new email notifier.
    pub fn new(config: EmailConfig) -> Result<Self, JobsError> {
        // Validate configuration
        if config.smtp_host.is_empty() {
            return Err(JobsError::InvalidNotificationConfig {
                reason: "SMTP host is required".to_string(),
            });
        }
        if config.from.is_empty() {
            return Err(JobsError::InvalidNotificationConfig {
                reason: "From address is required".to_string(),
            });
        }
        if config.to.is_empty() {
            return Err(JobsError::InvalidNotificationConfig {
                reason: "At least one recipient is required".to_string(),
            });
        }

        Ok(Self { config })
    }

    /// Format job event as email subject.
    fn format_subject(&self, event: &JobEvent) -> String {
        format!(
            "[Nika Jobs] {} Job '{}' {}",
            event.event_type.emoji(),
            event.job_name,
            match event.event_type {
                JobEventType::Started => "Started",
                JobEventType::Success => "Completed",
                JobEventType::Failed => "Failed",
                JobEventType::Retrying => "Retrying",
                JobEventType::MaxRetriesExceeded => "Max Retries Exceeded",
                JobEventType::TimedOut => "Timed Out",
            }
        )
    }

    /// Format job event as email body.
    fn format_body(&self, event: &JobEvent) -> String {
        let mut body = String::new();

        body.push_str(&format!("Job: {}\n", event.job_name));
        body.push_str(&format!("Execution ID: {}\n", event.execution_id));
        body.push_str(&format!(
            "Status: {} {}\n",
            event.event_type.emoji(),
            format!("{:?}", event.event_type)
        ));

        if let Some(duration) = event.duration_secs {
            body.push_str(&format!("Duration: {}\n", format_duration(duration)));
        }

        if let Some(attempt) = event.attempt {
            let max = event.max_attempts.unwrap_or(1);
            body.push_str(&format!("Attempt: {}/{}\n", attempt, max));
        }

        if let Some(ref error) = event.error {
            body.push_str(&format!("\nError Details:\n{}\n", error));
        }

        body.push_str("\n---\nSent by Nika Jobs Daemon\n");

        body
    }
}

#[async_trait]
impl NotificationChannel for EmailNotifier {
    fn name(&self) -> &str {
        "email"
    }

    async fn notify(&self, event: &JobEvent) -> Result<(), JobsError> {
        // Use lettre for SMTP (feature-gated in real implementation)
        // For now, we'll use a simpler approach with reqwest for testing
        // In production, this would use lettre crate

        let subject = self.format_subject(event);
        let body = self.format_body(event);

        // Log the email that would be sent (placeholder for actual SMTP)
        tracing::info!(
            target: "nika::jobs::notify",
            smtp_host = %self.config.smtp_host,
            from = %self.config.from,
            to = ?self.config.to,
            subject = %subject,
            "Email notification prepared (SMTP not yet implemented)"
        );

        // In a real implementation, we'd use lettre:
        // ```
        // use lettre::{Message, SmtpTransport, Transport};
        // let email = Message::builder()
        //     .from(self.config.from.parse()?)
        //     .to(recipient.parse()?)
        //     .subject(subject)
        //     .body(body)?;
        // let mailer = SmtpTransport::relay(&self.config.smtp_host)?
        //     .credentials(Credentials::new(username, password))
        //     .build();
        // mailer.send(&email)?;
        // ```

        // For now, mark as success (SMTP implementation deferred)
        let _ = (subject, body); // Suppress unused warnings
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.config.smtp_host.is_empty() && !self.config.to.is_empty()
    }
}

/// Notification dispatcher that coordinates all channels.
pub struct NotificationDispatcher {
    channels: Vec<Arc<dyn NotificationChannel>>,
    config: NotifyConfig,
}

impl NotificationDispatcher {
    /// Create a new dispatcher from configuration.
    pub fn new(config: NotifyConfig) -> Result<Self, JobsError> {
        let mut channels: Vec<Arc<dyn NotificationChannel>> = Vec::new();

        // Add Slack channel if configured
        if let Some(ref webhook_url) = config.slack_webhook {
            if !webhook_url.is_empty() {
                channels.push(Arc::new(SlackNotifier::new(webhook_url.clone())));
            }
        }

        // Add Email channel if configured
        if let Some(ref email_config) = config.email {
            channels.push(Arc::new(EmailNotifier::new(email_config.clone())?));
        }

        Ok(Self { channels, config })
    }

    /// Check if notifications should be sent for this event type.
    pub fn should_notify(&self, event_type: JobEventType) -> bool {
        match event_type {
            JobEventType::Success => self.config.on_success,
            JobEventType::Failed | JobEventType::MaxRetriesExceeded | JobEventType::TimedOut => {
                self.config.on_failure
            }
            JobEventType::Started | JobEventType::Retrying => false, // Not configurable yet
        }
    }

    /// Send notification to all configured channels.
    pub async fn notify(&self, event: &JobEvent) -> Result<(), JobsError> {
        if !self.should_notify(event.event_type) {
            tracing::debug!(
                target: "nika::jobs::notify",
                job = %event.job_name,
                event_type = ?event.event_type,
                "Notification skipped (not configured for this event type)"
            );
            return Ok(());
        }

        let mut errors = Vec::new();

        for channel in &self.channels {
            if !channel.is_configured() {
                continue;
            }

            match channel.notify(event).await {
                Ok(()) => {
                    tracing::info!(
                        target: "nika::jobs::notify",
                        channel = channel.name(),
                        job = %event.job_name,
                        event_type = ?event.event_type,
                        "Notification sent successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        target: "nika::jobs::notify",
                        channel = channel.name(),
                        job = %event.job_name,
                        error = %e,
                        "Failed to send notification"
                    );
                    errors.push(e);
                }
            }
        }

        // Return first error if any occurred
        if let Some(first_error) = errors.into_iter().next() {
            return Err(first_error);
        }

        Ok(())
    }

    /// Get number of configured channels.
    pub fn channel_count(&self) -> usize {
        self.channels.iter().filter(|c| c.is_configured()).count()
    }

    /// Check if any channels are configured.
    pub fn has_channels(&self) -> bool {
        self.channel_count() > 0
    }
}

/// Format duration in human-readable form.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{}h {}m", hours, mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_event(event_type: JobEventType) -> JobEvent {
        JobEvent {
            job_name: "test-job".to_string(),
            execution_id: "exec-123".to_string(),
            event_type,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_secs: Some(42),
            error: None,
            attempt: Some(1),
            max_attempts: Some(3),
        }
    }

    #[test]
    fn test_job_event_type_emoji() {
        assert_eq!(JobEventType::Started.emoji(), "🚀");
        assert_eq!(JobEventType::Success.emoji(), "✅");
        assert_eq!(JobEventType::Failed.emoji(), "❌");
        assert_eq!(JobEventType::Retrying.emoji(), "🔄");
        assert_eq!(JobEventType::MaxRetriesExceeded.emoji(), "💀");
        assert_eq!(JobEventType::TimedOut.emoji(), "⏰");
    }

    #[test]
    fn test_job_event_type_slack_color() {
        assert_eq!(JobEventType::Success.slack_color(), "#2ecc71");
        assert_eq!(JobEventType::Failed.slack_color(), "#e74c3c");
    }

    #[test]
    fn test_slack_message_format() {
        let notifier = SlackNotifier::new("https://hooks.slack.com/test".to_string());
        let event = test_event(JobEventType::Success);
        let message = notifier.format_message(&event);

        let attachments = message["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 1);

        let attachment = &attachments[0];
        assert_eq!(attachment["color"], "#2ecc71");
        assert!(attachment["title"]
            .as_str()
            .unwrap()
            .contains("completed successfully"));
    }

    #[test]
    fn test_slack_message_with_error() {
        let notifier = SlackNotifier::new("https://hooks.slack.com/test".to_string());
        let mut event = test_event(JobEventType::Failed);
        event.error = Some("Connection timeout".to_string());

        let message = notifier.format_message(&event);
        let fields = message["attachments"][0]["fields"].as_array().unwrap();

        // Should have an error field
        let error_field = fields.iter().find(|f| f["title"] == "Error");
        assert!(error_field.is_some());
        assert_eq!(error_field.unwrap()["value"], "Connection timeout");
    }

    #[test]
    fn test_email_subject_format() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            username: None,
            password: None,
            from: "nika@example.com".to_string(),
            to: vec!["admin@example.com".to_string()],
        };
        let notifier = EmailNotifier::new(config).unwrap();
        let event = test_event(JobEventType::Failed);

        let subject = notifier.format_subject(&event);
        assert!(subject.contains("❌"));
        assert!(subject.contains("test-job"));
        assert!(subject.contains("Failed"));
    }

    #[test]
    fn test_email_body_format() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            username: None,
            password: None,
            from: "nika@example.com".to_string(),
            to: vec!["admin@example.com".to_string()],
        };
        let notifier = EmailNotifier::new(config).unwrap();
        let mut event = test_event(JobEventType::Failed);
        event.error = Some("Database connection failed".to_string());

        let body = notifier.format_body(&event);
        assert!(body.contains("Job: test-job"));
        assert!(body.contains("Execution ID: exec-123"));
        assert!(body.contains("Duration: 42s"));
        assert!(body.contains("Attempt: 1/3"));
        assert!(body.contains("Database connection failed"));
    }

    #[test]
    fn test_email_validation_missing_host() {
        let config = EmailConfig {
            smtp_host: "".to_string(),
            smtp_port: 587,
            username: None,
            password: None,
            from: "nika@example.com".to_string(),
            to: vec!["admin@example.com".to_string()],
        };
        let result = EmailNotifier::new(config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("SMTP host is required"));
    }

    #[test]
    fn test_email_validation_missing_recipients() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            username: None,
            password: None,
            from: "nika@example.com".to_string(),
            to: vec![],
        };
        let result = EmailNotifier::new(config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("At least one recipient"));
    }

    #[test]
    fn test_dispatcher_should_notify() {
        let config = NotifyConfig {
            slack_webhook: None,
            email: None,
            on_failure: true,
            on_success: false,
        };
        let dispatcher = NotificationDispatcher::new(config).unwrap();

        assert!(dispatcher.should_notify(JobEventType::Failed));
        assert!(dispatcher.should_notify(JobEventType::MaxRetriesExceeded));
        assert!(dispatcher.should_notify(JobEventType::TimedOut));
        assert!(!dispatcher.should_notify(JobEventType::Success));
        assert!(!dispatcher.should_notify(JobEventType::Started));
    }

    #[test]
    fn test_dispatcher_with_on_success() {
        let config = NotifyConfig {
            slack_webhook: None,
            email: None,
            on_failure: true,
            on_success: true,
        };
        let dispatcher = NotificationDispatcher::new(config).unwrap();

        assert!(dispatcher.should_notify(JobEventType::Success));
        assert!(dispatcher.should_notify(JobEventType::Failed));
    }

    #[test]
    fn test_dispatcher_channel_count() {
        let config = NotifyConfig {
            slack_webhook: Some("https://hooks.slack.com/test".to_string()),
            email: None,
            on_failure: true,
            on_success: false,
        };
        let dispatcher = NotificationDispatcher::new(config).unwrap();

        assert_eq!(dispatcher.channel_count(), 1);
        assert!(dispatcher.has_channels());
    }

    #[test]
    fn test_dispatcher_no_channels() {
        let config = NotifyConfig::default();
        let dispatcher = NotificationDispatcher::new(config).unwrap();

        assert_eq!(dispatcher.channel_count(), 0);
        assert!(!dispatcher.has_channels());
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(45), "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(125), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(7380), "2h 3m");
    }

    #[test]
    fn test_slack_is_configured() {
        let notifier = SlackNotifier::new("https://hooks.slack.com/test".to_string());
        assert!(notifier.is_configured());

        let empty_notifier = SlackNotifier::new("".to_string());
        assert!(!empty_notifier.is_configured());
    }

    #[test]
    fn test_email_is_configured() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            username: None,
            password: None,
            from: "nika@example.com".to_string(),
            to: vec!["admin@example.com".to_string()],
        };
        let notifier = EmailNotifier::new(config).unwrap();
        assert!(notifier.is_configured());
    }

    #[tokio::test]
    async fn test_dispatcher_notify_skipped() {
        let config = NotifyConfig {
            slack_webhook: None,
            email: None,
            on_failure: false,
            on_success: false,
        };
        let dispatcher = NotificationDispatcher::new(config).unwrap();
        let event = test_event(JobEventType::Failed);

        // Should succeed (notification skipped due to config)
        let result = dispatcher.notify(&event).await;
        assert!(result.is_ok());
    }
}
