//! Codex Dreams insights feature module.
//!
//! This module contains all components for automated insight generation
//! from stored memories using AI models. All functionality is feature-gated
//! behind the `codex-dreams` feature flag.

#[cfg(feature = "codex-dreams")]
pub mod models;

#[cfg(feature = "codex-dreams")]
pub mod ollama_client;

#[cfg(feature = "codex-dreams")]
pub mod storage;

#[cfg(feature = "codex-dreams")]
pub mod processor;

#[cfg(feature = "codex-dreams")]
pub mod export;

#[cfg(feature = "codex-dreams")]
pub mod scheduler;

#[cfg(feature = "codex-dreams")]
pub use models::*;

#[cfg(feature = "codex-dreams")]
pub use ollama_client::{OllamaClient, OllamaConfig, OllamaClientError};

#[cfg(feature = "codex-dreams")]
pub use storage::InsightStorage;

#[cfg(feature = "codex-dreams")]
pub use processor::{InsightsProcessor, ProcessorConfig, ProcessingResult, ProcessingStats};

#[cfg(feature = "codex-dreams")]
pub use export::InsightExporter;

#[cfg(feature = "codex-dreams")]
pub use scheduler::{InsightScheduler, SchedulerConfig, SchedulerStatistics, SchedulerStatus, SchedulerRunResult};