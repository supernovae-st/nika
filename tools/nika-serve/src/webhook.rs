//! HMAC-signed webhook notifications for job completion.
//!
//! When `NIKA_WEBHOOK_URL` is set, sends a POST request to the configured URL
//! when a job reaches a terminal state (completed, failed, cancelled).
//!
//! The request includes an `X-Nika-Signature` header with an HMAC-SHA256
//! signature of the body, using `NIKA_WEBHOOK_SECRET` as the key.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;

/// Webhook configuration, loaded from environment variables.
#[derive(Clone, Debug)]
pub struct WebhookConfig {
    pub url: String,
    pub secret: String,
}

impl WebhookConfig {
    /// Load webhook config from environment variables.
    ///
    /// Returns `None` if `NIKA_WEBHOOK_URL` is not set.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("NIKA_WEBHOOK_URL").ok()?;
        let secret = std::env::var("NIKA_WEBHOOK_SECRET").unwrap_or_default();
        if secret.is_empty() {
            warn!("NIKA_WEBHOOK_URL is set but NIKA_WEBHOOK_SECRET is empty — signatures will be weak");
        }
        Some(Self { url, secret })
    }
}

/// Compute HMAC-SHA256 signature for a payload.
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body);
    let result = mac.finalize();
    let bytes = result.into_bytes();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256={hex}")
}

/// Send a webhook notification for a completed job.
///
/// Non-blocking: spawns a tokio task to send the request.
/// Failures are logged but do not affect job status.
pub fn notify(config: &WebhookConfig, job_id: &str, status: &str, output: Option<&str>) {
    let url = config.url.clone();
    let secret = config.secret.clone();
    let body = serde_json::json!({
        "job_id": job_id,
        "status": status,
        "output": output,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    tokio::spawn(async move {
        let signature = sign(&secret, &body_bytes);

        let client = reqwest::Client::new();
        match client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-nika-signature", &signature)
            .body(body_bytes)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                debug!(
                    url = %url,
                    status = resp.status().as_u16(),
                    "webhook delivered"
                );
            }
            Err(e) => {
                warn!(url = %url, error = %e, "webhook delivery failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_sha256_prefix() {
        let sig = sign("my-secret", b"hello world");
        assert!(sig.starts_with("sha256="), "signature must start with sha256=");
        // sha256= prefix + 64 hex chars
        assert_eq!(sig.len(), 7 + 64);
    }

    #[test]
    fn sign_deterministic() {
        let a = sign("key", b"data");
        let b = sign("key", b"data");
        assert_eq!(a, b, "same key+data must produce same signature");
    }

    #[test]
    fn sign_different_keys_produce_different_sigs() {
        let a = sign("key1", b"data");
        let b = sign("key2", b"data");
        assert_ne!(a, b);
    }

    #[test]
    fn webhook_config_from_env_none_when_unset() {
        // No NIKA_WEBHOOK_URL set — should return None
        std::env::remove_var("NIKA_WEBHOOK_URL");
        assert!(WebhookConfig::from_env().is_none());
    }
}
