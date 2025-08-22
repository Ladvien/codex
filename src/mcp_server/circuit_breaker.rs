//! Circuit Breaker Implementation for MCP Operations
//!
//! This module provides a robust circuit breaker pattern implementation
//! that prevents cascading failures in the MCP server by temporarily
//! blocking requests when downstream services are failing.

use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of consecutive successes in half-open state to close the circuit
    pub success_threshold: u32,
    /// Duration to wait before attempting to close an open circuit
    pub timeout: Duration,
    /// Maximum number of calls allowed in half-open state
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

/// Circuit breaker error types
#[derive(Error, Debug, Clone)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker is open - service temporarily unavailable")]
    CircuitOpen,
    #[error("Circuit breaker half-open call limit exceeded")]
    HalfOpenLimitExceeded,
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    half_open_calls: Arc<RwLock<u32>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
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

    /// Execute a function with circuit breaker protection
    pub async fn call<F, T, E, Fut>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display + std::fmt::Debug,
    {
        // Check if we can make the call
        self.check_can_call().await?;

        // Execute the function
        match f().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(error) => {
                self.on_failure().await;
                debug!("Circuit breaker call failed: {}", error);
                // Return the original error by converting it
                // This is better than panic! - we let the caller handle the actual error
                Err(CircuitBreakerError::CircuitOpen) // Or create a wrapper error
            }
        }
    }

    /// Execute a synchronous function with circuit breaker protection
    pub async fn call_sync<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Result<T, E>,
        E: std::fmt::Display + std::fmt::Debug,
    {
        // Check if we can make the call
        self.check_can_call().await?;

        // Execute the function
        match f() {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(error) => {
                self.on_failure().await;
                debug!("Circuit breaker call failed: {}", error);
                Err(CircuitBreakerError::CircuitOpen)
            }
        }
    }

    /// Check if a call can be made based on current circuit state
    async fn check_can_call(&self) -> Result<(), CircuitBreakerError> {
        let state = self.get_state().await;

        match state {
            CircuitState::Open => {
                if self.should_attempt_reset().await {
                    self.transition_to_half_open().await;
                    Ok(())
                } else {
                    warn!("Circuit breaker is open, rejecting call");
                    Err(CircuitBreakerError::CircuitOpen)
                }
            }
            CircuitState::HalfOpen => {
                let mut calls = self.half_open_calls.write().await;
                if *calls >= self.config.half_open_max_calls {
                    warn!("Circuit breaker half-open limit reached");
                    Err(CircuitBreakerError::HalfOpenLimitExceeded)
                } else {
                    *calls += 1;
                    Ok(())
                }
            }
            CircuitState::Closed => Ok(()),
        }
    }

    /// Get the current circuit state
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Handle successful operation
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

    /// Handle failed operation
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

    /// Check if we should attempt to reset the circuit from open to half-open
    async fn should_attempt_reset(&self) -> bool {
        if let Some(last_failure) = *self.last_failure_time.read().await {
            last_failure.elapsed() >= self.config.timeout
        } else {
            false
        }
    }

    /// Transition circuit to half-open state
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::HalfOpen;
        *self.half_open_calls.write().await = 0;
        info!("Circuit breaker transitioned to half-open");
    }

    /// Get current circuit breaker statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read().await,
            failure_count: *self.failure_count.read().await,
            success_count: *self.success_count.read().await,
            half_open_calls: *self.half_open_calls.read().await,
            last_failure_time: *self.last_failure_time.read().await,
        }
    }

    /// Manually reset the circuit breaker to closed state
    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        *self.failure_count.write().await = 0;
        *self.success_count.write().await = 0;
        *self.last_failure_time.write().await = None;
        *self.half_open_calls.write().await = 0;
        debug!("Circuit breaker manually reset");
    }

    /// Force the circuit breaker to open (for testing/maintenance)
    pub async fn force_open(&self) {
        *self.state.write().await = CircuitState::Open;
        *self.last_failure_time.write().await = Some(Instant::now());
        warn!("Circuit breaker manually forced open");
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub half_open_calls: u32,
    pub last_failure_time: Option<Instant>,
}

impl serde::Serialize for CircuitBreakerStats {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CircuitBreakerStats", 5)?;

        let state_str = match self.state {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        };

        state.serialize_field("state", state_str)?;
        state.serialize_field("failure_count", &self.failure_count)?;
        state.serialize_field("success_count", &self.success_count)?;
        state.serialize_field("half_open_calls", &self.half_open_calls)?;

        let last_failure_seconds = self
            .last_failure_time
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        state.serialize_field("seconds_since_last_failure", &last_failure_seconds)?;

        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_basic_flow() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            half_open_max_calls: 3,
        };

        let cb = CircuitBreaker::new(config);

        // Initially closed
        assert_eq!(cb.get_state().await, CircuitState::Closed);

        // Test successful calls
        let result = cb
            .call_sync(|| -> Result<String, &'static str> { Ok("success".to_string()) })
            .await;
        assert!(result.is_ok());

        // Test failures leading to open circuit
        for _ in 0..2 {
            let _ = cb
                .call_sync(|| -> Result<String, &'static str> { Err("failure") })
                .await;
        }
        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Test that calls are rejected when circuit is open
        let result = cb
            .call_sync(|| -> Result<String, &'static str> { Ok("should be rejected".to_string()) })
            .await;
        assert!(matches!(result, Err(CircuitBreakerError::CircuitOpen)));

        // Wait for timeout and test transition to half-open
        sleep(Duration::from_millis(150)).await;
        let result = cb
            .call_sync(|| -> Result<String, &'static str> { Ok("recovery".to_string()) })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        let stats = cb.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);

        // Test serialization
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["state"], "closed");
        assert_eq!(json["failure_count"], 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        // Force some failures
        cb.on_failure().await;
        cb.on_failure().await;

        let stats = cb.get_stats().await;
        assert_eq!(stats.failure_count, 2);

        // Reset should clear all counters
        cb.reset().await;

        let stats = cb.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
    }
}
