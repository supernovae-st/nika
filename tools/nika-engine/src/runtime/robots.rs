//! robots.txt compliance via texting_robots (RFC 9309).
//!
//! Per-domain robots.txt fetching and caching. Checks if a URL is allowed
//! before the fetch executor makes the request.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::time::Duration;
use texting_robots::Robot;

/// Cached robots.txt compliance checker.
///
/// Lazily fetches and caches robots.txt per domain. Thread-safe via RwLock.
/// Cache entries persist for the lifetime of the workflow run.
pub struct RobotsCache {
    /// Domain → parsed Robot (None = 404 or error, meaning allow all)
    cache: RwLock<FxHashMap<String, Option<CachedRobot>>>,
    user_agent: String,
}

/// Wrapper around Robot.
struct CachedRobot {
    robot: Robot,
}

impl CachedRobot {
    fn allowed(&self, url: &str) -> bool {
        self.robot.allowed(url)
    }

    fn crawl_delay(&self) -> Option<f64> {
        self.robot.delay.map(|d| d as f64)
    }
}

impl RobotsCache {
    /// Create a new cache with the given user-agent string.
    pub fn new(user_agent: &str) -> Self {
        Self {
            cache: RwLock::new(FxHashMap::default()),
            user_agent: user_agent.to_string(),
        }
    }

    /// Check if the given URL is allowed by robots.txt.
    ///
    /// Fetches and caches robots.txt on first access per domain.
    /// Returns `true` if allowed (or if robots.txt is missing/errored).
    pub async fn is_allowed(&self, url: &url::Url, client: &reqwest::Client) -> bool {
        // Use host:port as cache key so non-standard ports are handled correctly
        let authority = match url.port() {
            Some(port) => format!("{}:{}", url.host_str().unwrap_or_default(), port),
            None => url.host_str().unwrap_or_default().to_string(),
        };

        // Check cache first (read lock)
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(&authority) {
                return match entry {
                    Some(robot) => robot.allowed(url.as_str()),
                    None => true, // 404 robots.txt = allow all
                };
            }
        }

        // Fetch and cache (write lock)
        let robots_url = format!("{}://{}/robots.txt", url.scheme(), authority);
        let robot = match client
            .get(&robots_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.bytes().await.unwrap_or_default();
                match Robot::new(&self.user_agent, &body) {
                    Ok(r) => Some(CachedRobot { robot: r }),
                    Err(_) => None,
                }
            }
            _ => None, // 404 or network error = allow all
        };

        let allowed = robot
            .as_ref()
            .map(|r| r.allowed(url.as_str()))
            .unwrap_or(true);
        self.cache.write().insert(authority, robot);
        allowed
    }

    /// Get the crawl-delay for a domain, if specified.
    pub async fn crawl_delay(&self, url: &url::Url, client: &reqwest::Client) -> Option<f64> {
        let authority = match url.port() {
            Some(port) => format!("{}:{}", url.host_str().unwrap_or_default(), port),
            None => url.host_str().unwrap_or_default().to_string(),
        };

        // Ensure robots.txt is fetched
        self.is_allowed(url, client).await;

        let cache = self.cache.read();
        cache
            .get(&authority)
            .and_then(|entry| entry.as_ref())
            .and_then(|r| r.crawl_delay())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_new_creates_empty() {
        let cache = RobotsCache::new("nika/0.63");
        assert!(cache.cache.read().is_empty());
    }

    #[tokio::test]
    async fn cache_allows_when_no_robots_txt() {
        // Use a client that will fail to connect (no actual server)
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let url = url::Url::parse("https://nonexistent.test.invalid/page").unwrap();

        let cache = RobotsCache::new("nika/0.63");
        // Should return true (allow) when robots.txt can't be fetched
        let allowed = cache.is_allowed(&url, &client).await;
        assert!(allowed, "Should allow when robots.txt is unreachable");
    }

    #[tokio::test]
    async fn cache_caches_result() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let url = url::Url::parse("https://nonexistent.test.invalid/page").unwrap();

        let cache = RobotsCache::new("nika/0.63");
        cache.is_allowed(&url, &client).await;

        // Second call should use cache (won't make HTTP request)
        assert!(cache.cache.read().contains_key("nonexistent.test.invalid"));
    }
}
