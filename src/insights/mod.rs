//! Codex Dreams insights feature module.
//!
//! This module contains all components for automated insight generation
//! from stored memories using AI models. All functionality is feature-gated
//! behind the `codex-dreams` feature flag.

#[cfg(feature = "codex-dreams")]
pub mod models;

#[cfg(feature = "codex-dreams")]
pub use models::*;