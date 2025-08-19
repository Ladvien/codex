use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub exponential_base: f64,
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            exponential_base: 2.0,
            jitter: true,
        }
    }
}

pub struct RetryPolicy {
    config: RetryConfig,
}

impl RetryPolicy {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    pub async fn execute<F, Fut, T, E>(&self, mut f: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut attempt = 0;
        let mut delay = self.config.initial_delay;

        loop {
            attempt += 1;

            match f().await {
                Ok(result) => {
                    if attempt > 1 {
                        debug!("Retry succeeded on attempt {}", attempt);
                    }
                    return Ok(result);
                }
                Err(error) if attempt >= self.config.max_attempts => {
                    warn!("All {} retry attempts exhausted", self.config.max_attempts);
                    return Err(error);
                }
                Err(error) => {
                    warn!(
                        "Attempt {} failed: {}. Retrying in {:?}",
                        attempt, error, delay
                    );

                    sleep(delay).await;

                    // Calculate next delay with exponential backoff
                    delay = self.calculate_next_delay(delay);
                }
            }
        }
    }

    fn calculate_next_delay(&self, current_delay: Duration) -> Duration {
        let mut next_delay =
            Duration::from_secs_f64(current_delay.as_secs_f64() * self.config.exponential_base);

        // Apply jitter if enabled
        if self.config.jitter {
            let jitter_amount = next_delay.as_secs_f64() * 0.1 * rand::random::<f64>();
            next_delay = Duration::from_secs_f64(next_delay.as_secs_f64() + jitter_amount);
        }

        // Cap at max delay
        if next_delay > self.config.max_delay {
            next_delay = self.config.max_delay;
        }

        next_delay
    }

    pub async fn execute_with_circuit_breaker<F, Fut, T, E>(
        &self,
        _circuit_breaker: &crate::mcp::circuit_breaker::CircuitBreaker,
        f: F,
    ) -> Result<T, E>
    where
        F: Fn() -> Fut + Clone,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        // For now, bypass circuit breaker and just use retry policy
        // TODO: Implement proper async circuit breaker integration
        self.execute(|| {
            let f = f.clone();
            f()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_on_second_attempt() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let policy = RetryPolicy::new(config);

        let result = policy
            .execute(|| {
                let counter = counter_clone.clone();
                async move {
                    let count = counter.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        Err("First attempt fails")
                    } else {
                        Ok("Success")
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_exhausts_attempts() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let config = RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let policy = RetryPolicy::new(config);

        let result: Result<(), &str> = policy
            .execute(|| {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err("Always fails")
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_calculate_next_delay() {
        let config = RetryConfig {
            exponential_base: 2.0,
            max_delay: Duration::from_secs(5),
            jitter: false,
            ..Default::default()
        };

        let policy = RetryPolicy::new(config);

        let delay1 = Duration::from_secs(1);
        let delay2 = policy.calculate_next_delay(delay1);
        assert_eq!(delay2, Duration::from_secs(2));

        let delay3 = policy.calculate_next_delay(Duration::from_secs(3));
        assert_eq!(delay3, Duration::from_secs(5)); // Capped at max_delay
    }
}
