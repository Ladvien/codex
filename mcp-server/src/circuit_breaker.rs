use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout: Duration,
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }
    }
}

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    half_open_calls: Arc<RwLock<u32>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            half_open_calls: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
        E: std::fmt::Display,
    {
        let state = self.get_state().await;

        match state {
            CircuitState::Open => {
                if self.should_attempt_reset().await {
                    self.transition_to_half_open().await;
                } else {
                    error!("Circuit breaker is open, rejecting call");
                    return Err(self.create_circuit_open_error());
                }
            }
            CircuitState::HalfOpen => {
                let calls = *self.half_open_calls.read().await;
                if calls >= self.config.half_open_max_calls {
                    warn!("Circuit breaker half-open limit reached");
                    return Err(self.create_circuit_open_error());
                }
                *self.half_open_calls.write().await += 1;
            }
            CircuitState::Closed => {}
        }

        match f() {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(error) => {
                self.on_failure().await;
                error!("Circuit breaker call failed: {}", error);
                Err(error)
            }
        }
    }

    async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    async fn on_success(&self) {
        let mut state = self.state.write().await;
        let mut success_count = self.success_count.write().await;
        let mut failure_count = self.failure_count.write().await;

        match *state {
            CircuitState::HalfOpen => {
                *success_count += 1;
                if *success_count >= self.config.success_threshold {
                    *state = CircuitState::Closed;
                    *failure_count = 0;
                    *success_count = 0;
                    *self.half_open_calls.write().await = 0;
                    info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Closed => {
                *failure_count = 0;
            }
            _ => {}
        }
    }

    async fn on_failure(&self) {
        let mut state = self.state.write().await;
        let mut failure_count = self.failure_count.write().await;
        let mut last_failure_time = self.last_failure_time.write().await;

        *failure_count += 1;
        *last_failure_time = Some(Instant::now());

        match *state {
            CircuitState::Closed => {
                if *failure_count >= self.config.failure_threshold {
                    *state = CircuitState::Open;
                    warn!("Circuit breaker opened after {} failures", failure_count);
                }
            }
            CircuitState::HalfOpen => {
                *state = CircuitState::Open;
                *self.success_count.write().await = 0;
                *self.half_open_calls.write().await = 0;
                warn!("Circuit breaker reopened from half-open state");
            }
            _ => {}
        }
    }

    async fn should_attempt_reset(&self) -> bool {
        if let Some(last_failure) = *self.last_failure_time.read().await {
            last_failure.elapsed() >= self.config.timeout
        } else {
            false
        }
    }

    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::HalfOpen;
        *self.half_open_calls.write().await = 0;
        info!("Circuit breaker transitioned to half-open");
    }

    fn create_circuit_open_error<E>(&self) -> E
    where
        E: std::fmt::Display,
    {
        // This is a placeholder - in real implementation, you'd create proper error type
        panic!("Circuit breaker is open")
    }

    pub async fn get_stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read().await,
            failure_count: *self.failure_count.read().await,
            success_count: *self.success_count.read().await,
            half_open_calls: *self.half_open_calls.read().await,
        }
    }

    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        *self.failure_count.write().await = 0;
        *self.success_count.write().await = 0;
        *self.last_failure_time.write().await = None;
        *self.half_open_calls.write().await = 0;
        debug!("Circuit breaker manually reset");
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub half_open_calls: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_transitions() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            half_open_max_calls: 3,
        };

        let cb = CircuitBreaker::new(config);

        // Initially closed
        assert_eq!(cb.get_state().await, CircuitState::Closed);

        // Simulate failures to open the circuit
        for _ in 0..2 {
            cb.on_failure().await;
        }
        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Wait for timeout and check half-open
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(cb.should_attempt_reset().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        
        let stats = cb.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);
    }
}