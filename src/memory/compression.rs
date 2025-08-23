use crate::memory::error::{MemoryError, Result};
use serde_json::Value;
use tracing::{debug, warn};

/// Compression engine for frozen memory tier using zstd
/// Optimized for 5:1 compression ratio target with fast decompression
pub struct ZstdCompressionEngine {
    compression_level: i32,
}

impl ZstdCompressionEngine {
    /// Create a new compression engine with optimal settings for memory data
    pub fn new() -> Self {
        Self {
            // Level 3 provides good balance between compression ratio and speed
            // Targeting 5:1 compression ratio as specified
            compression_level: 3,
        }
    }

    /// Create compression engine with custom level
    pub fn with_level(level: i32) -> Self {
        Self {
            compression_level: level.clamp(1, 22), // zstd supports levels 1-22
        }
    }

    /// Compress memory content and metadata into a single compressed blob
    /// Returns compressed bytes and compression metrics
    pub fn compress_memory_data(
        &self,
        content: &str,
        metadata: &Value,
    ) -> Result<CompressionResult> {
        debug!(
            "Compressing memory data: content_length={}, compression_level={}",
            content.len(),
            self.compression_level
        );

        // Serialize the combined data structure
        let memory_data = MemoryData {
            content: content.to_string(),
            metadata: metadata.clone(),
            compressed_at: chrono::Utc::now(),
            original_size: content.len() as u64,
        };

        let serialized =
            serde_json::to_vec(&memory_data).map_err(|e| MemoryError::SerializationError {
                message: format!("Failed to serialize memory data: {e}"),
            })?;

        // Compress using zstd
        let compressed =
            zstd::encode_all(std::io::Cursor::new(&serialized), self.compression_level).map_err(
                |e| MemoryError::CompressionError {
                    message: format!("zstd compression failed: {e}"),
                },
            )?;

        let original_size = serialized.len() as u64;
        let compressed_size = compressed.len() as u64;
        let compression_ratio = original_size as f64 / compressed_size as f64;

        debug!(
            "Compression completed: original={}B, compressed={}B, ratio={:.2}:1",
            original_size, compressed_size, compression_ratio
        );

        // Warn if compression ratio is below target
        if compression_ratio < 5.0 {
            warn!(
                "Compression ratio {:.2}:1 is below target 5:1 for content length {}",
                compression_ratio,
                content.len()
            );
        }

        Ok(CompressionResult {
            compressed_data: compressed,
            original_size,
            compressed_size,
            compression_ratio,
        })
    }

    /// Decompress memory data back to original content and metadata
    /// Includes integrity validation
    pub fn decompress_memory_data(&self, compressed_data: &[u8]) -> Result<MemoryData> {
        debug!(
            "Decompressing memory data: compressed_size={}B",
            compressed_data.len()
        );

        // Decompress using zstd
        let decompressed =
            zstd::decode_all(std::io::Cursor::new(compressed_data)).map_err(|e| {
                MemoryError::DecompressionError {
                    message: format!("zstd decompression failed: {e}"),
                }
            })?;

        // Deserialize the memory data
        let memory_data: MemoryData =
            serde_json::from_slice(&decompressed).map_err(|e| MemoryError::SerializationError {
                message: format!("Failed to deserialize memory data: {e}"),
            })?;

        // Validate integrity
        if memory_data.content.len() != memory_data.original_size as usize {
            return Err(MemoryError::IntegrityError {
                message: format!(
                    "Content size mismatch: expected {}, got {}",
                    memory_data.original_size,
                    memory_data.content.len()
                ),
            });
        }

        debug!(
            "Decompression completed: content_length={}B",
            memory_data.content.len()
        );

        Ok(memory_data)
    }

    /// Batch compress multiple memories for efficient processing
    pub fn batch_compress(&self, memories: Vec<(&str, &Value)>) -> Result<Vec<CompressionResult>> {
        debug!("Starting batch compression of {} memories", memories.len());

        let mut results = Vec::with_capacity(memories.len());
        let mut total_original = 0u64;
        let mut total_compressed = 0u64;

        for (content, metadata) in memories {
            match self.compress_memory_data(content, metadata) {
                Ok(result) => {
                    total_original += result.original_size;
                    total_compressed += result.compressed_size;
                    results.push(result);
                }
                Err(e) => {
                    warn!("Failed to compress memory in batch: {}", e);
                    return Err(e);
                }
            }
        }

        let overall_ratio = total_original as f64 / total_compressed as f64;
        debug!(
            "Batch compression completed: {} memories, overall ratio {:.2}:1",
            results.len(),
            overall_ratio
        );

        Ok(results)
    }

    /// Estimate compression ratio for planning purposes
    pub fn estimate_compression_ratio(&self, content: &str) -> f64 {
        // Quick heuristic based on content characteristics
        let content_len = content.len() as f64;

        // Base ratio estimates for different content types
        let estimated_ratio = if content_len < 100.0 {
            // Very short content compresses poorly
            2.0
        } else if content.chars().all(|c| c.is_ascii() && !c.is_control()) {
            // Text content typically compresses well
            if content.contains("  ") || content.contains("\n\n") {
                // Whitespace-heavy content compresses very well
                7.0
            } else {
                5.5
            }
        } else {
            // Mixed content
            4.0
        };

        // Adjust based on content length (longer content often compresses better)
        let length_factor = (content_len / 1000.0).min(2.0).max(0.5);
        estimated_ratio * length_factor
    }
}

impl Default for ZstdCompressionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined memory data structure for compression
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryData {
    pub content: String,
    pub metadata: Value,
    pub compressed_at: chrono::DateTime<chrono::Utc>,
    pub original_size: u64,
}

/// Result of compression operation with metrics
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub compressed_data: Vec<u8>,
    pub original_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
}

/// Compression statistics for monitoring
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompressionStats {
    pub total_memories_compressed: u64,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
    pub average_compression_ratio: f64,
    pub total_space_saved_bytes: u64,
    pub compression_efficiency_percent: f64,
}

impl CompressionStats {
    pub fn new() -> Self {
        Self {
            total_memories_compressed: 0,
            total_original_bytes: 0,
            total_compressed_bytes: 0,
            average_compression_ratio: 0.0,
            total_space_saved_bytes: 0,
            compression_efficiency_percent: 0.0,
        }
    }

    pub fn add_compression(&mut self, result: &CompressionResult) {
        self.total_memories_compressed += 1;
        self.total_original_bytes += result.original_size;
        self.total_compressed_bytes += result.compressed_size;
        self.total_space_saved_bytes = self.total_original_bytes - self.total_compressed_bytes;

        self.average_compression_ratio = if self.total_compressed_bytes > 0 {
            self.total_original_bytes as f64 / self.total_compressed_bytes as f64
        } else {
            0.0
        };

        self.compression_efficiency_percent = if self.total_original_bytes > 0 {
            (self.total_space_saved_bytes as f64 / self.total_original_bytes as f64) * 100.0
        } else {
            0.0
        };
    }
}

impl Default for CompressionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Frozen memory compression utilities
pub struct FrozenMemoryCompression;

impl FrozenMemoryCompression {
    /// Convert compression result to database-ready format
    pub fn to_database_format(result: CompressionResult) -> (Vec<u8>, i32, i32, f64) {
        (
            result.compressed_data,
            result.original_size as i32,
            result.compressed_size as i32,
            result.compression_ratio,
        )
    }

    /// Validate compression meets frozen tier requirements
    pub fn validate_compression_quality(ratio: f64, content_length: usize) -> Result<()> {
        const MIN_COMPRESSION_RATIO: f64 = 2.0; // Absolute minimum
        const TARGET_COMPRESSION_RATIO: f64 = 5.0; // Target ratio
        const MIN_CONTENT_LENGTH: usize = 50; // Don't compress very short content

        if content_length < MIN_CONTENT_LENGTH {
            return Err(MemoryError::CompressionError {
                message: format!(
                    "Content too short for compression: {content_length} bytes (minimum: {MIN_CONTENT_LENGTH})"
                ),
            });
        }

        if ratio < MIN_COMPRESSION_RATIO {
            return Err(MemoryError::CompressionError {
                message: format!(
                    "Compression ratio {ratio:.2}:1 is below minimum {MIN_COMPRESSION_RATIO:.1}:1"
                ),
            });
        }

        if ratio < TARGET_COMPRESSION_RATIO {
            warn!(
                "Compression ratio {:.2}:1 is below target {:.1}:1",
                ratio, TARGET_COMPRESSION_RATIO
            );
        }

        Ok(())
    }

    /// Calculate storage savings from compression
    pub fn calculate_storage_savings(original_size: u64, compressed_size: u64) -> StorageSavings {
        let space_saved = original_size.saturating_sub(compressed_size);
        let compression_ratio = if compressed_size > 0 {
            original_size as f64 / compressed_size as f64
        } else {
            0.0
        };
        let efficiency_percent = if original_size > 0 {
            (space_saved as f64 / original_size as f64) * 100.0
        } else {
            0.0
        };

        StorageSavings {
            original_size,
            compressed_size,
            space_saved,
            compression_ratio,
            efficiency_percent,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageSavings {
    pub original_size: u64,
    pub compressed_size: u64,
    pub space_saved: u64,
    pub compression_ratio: f64,
    pub efficiency_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compression_decompression_roundtrip() {
        let engine = ZstdCompressionEngine::new();
        let content = "This is a test memory content that should compress well because it has repetitive patterns and common English words.".repeat(10);
        let metadata = json!({
            "tag": "test",
            "importance": 0.8,
            "created_at": "2024-01-01T00:00:00Z"
        });

        // Compress
        let result = engine.compress_memory_data(&content, &metadata).unwrap();

        // Should achieve good compression ratio
        assert!(result.compression_ratio > 3.0);
        assert!(result.compressed_size < result.original_size);

        // Decompress
        let decompressed = engine
            .decompress_memory_data(&result.compressed_data)
            .unwrap();

        // Verify data integrity
        assert_eq!(decompressed.content, content);
        assert_eq!(decompressed.metadata, metadata);
        assert_eq!(decompressed.original_size, content.len() as u64);
    }

    #[test]
    fn test_compression_ratio_estimation() {
        let engine = ZstdCompressionEngine::new();

        // Long repetitive content should have high estimated ratio
        let repetitive_content = "Hello world! ".repeat(100);
        let ratio = engine.estimate_compression_ratio(&repetitive_content);
        assert!(ratio > 5.0);

        // Short content should have low estimated ratio
        let short_content = "Hi";
        let ratio = engine.estimate_compression_ratio(short_content);
        assert!(ratio < 3.0);
    }

    #[test]
    fn test_batch_compression() {
        let engine = ZstdCompressionEngine::new();
        let metadata = json!({"test": true});

        let memories = vec![
            ("First memory content", &metadata),
            ("Second memory content with different text", &metadata),
            (
                "Third memory content for testing batch processing",
                &metadata,
            ),
        ];

        let results = engine.batch_compress(memories).unwrap();

        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.compression_ratio > 1.0);
            assert!(result.compressed_size > 0);
        }
    }

    #[test]
    fn test_compression_validation() {
        // Valid compression should pass
        assert!(FrozenMemoryCompression::validate_compression_quality(5.0, 1000).is_ok());

        // Below minimum ratio should fail
        assert!(FrozenMemoryCompression::validate_compression_quality(1.5, 1000).is_err());

        // Too short content should fail
        assert!(FrozenMemoryCompression::validate_compression_quality(5.0, 10).is_err());
    }

    #[test]
    fn test_storage_savings_calculation() {
        let savings = FrozenMemoryCompression::calculate_storage_savings(1000, 200);

        assert_eq!(savings.original_size, 1000);
        assert_eq!(savings.compressed_size, 200);
        assert_eq!(savings.space_saved, 800);
        assert_eq!(savings.compression_ratio, 5.0);
        assert_eq!(savings.efficiency_percent, 80.0);
    }

    #[test]
    fn test_compression_stats_tracking() {
        let mut stats = CompressionStats::new();

        let result = CompressionResult {
            compressed_data: vec![1, 2, 3],
            original_size: 1000,
            compressed_size: 200,
            compression_ratio: 5.0,
        };

        stats.add_compression(&result);

        assert_eq!(stats.total_memories_compressed, 1);
        assert_eq!(stats.total_original_bytes, 1000);
        assert_eq!(stats.total_compressed_bytes, 200);
        assert_eq!(stats.average_compression_ratio, 5.0);
        assert_eq!(stats.total_space_saved_bytes, 800);
        assert_eq!(stats.compression_efficiency_percent, 80.0);
    }
}
