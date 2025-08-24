use super::models::{Memory, MemoryTier};
use super::repository::MemoryRepository;
use super::error::Result;
use regex::Regex;
use std::sync::Arc;
use tracing::{info, debug};
use uuid::Uuid;

/// Automatic tiering rules for keeping working memory clean
pub struct AutoTieringEngine {
    repository: Arc<MemoryRepository>,
    test_pattern: Regex,
    dev_pattern: Regex,
}

impl AutoTieringEngine {
    pub fn new(repository: Arc<MemoryRepository>) -> Self {
        Self {
            repository,
            test_pattern: Regex::new(r"(?i)(test|health check|concurrent.*thread|binary size)").unwrap(),
            dev_pattern: Regex::new(r"(?i)(jira|story \d+|status:\s*completed|development.*summary|creating rust)").unwrap(),
        }
    }

    /// Analyze a memory and determine its appropriate tier
    pub fn classify_memory(&self, memory: &Memory) -> (MemoryTier, f32) {
        let content_lower = memory.content.to_lowercase();
        
        // Test data should go to cold storage with very low importance
        if self.test_pattern.is_match(&content_lower) {
            return (MemoryTier::Cold, 0.1);
        }
        
        // Development artifacts go to warm tier
        if self.dev_pattern.is_match(&content_lower) {
            return (MemoryTier::Warm, 0.3);
        }
        
        // Check for other patterns that indicate low-value content
        if content_lower.len() < 20 || // Very short memories
           content_lower.contains("thread") && content_lower.contains("item") || // Test patterns
           content_lower.starts_with("##") && content_lower.contains(".md") // Markdown headers
        {
            return (MemoryTier::Warm, 0.4);
        }
        
        // Default: keep current tier and importance
        (memory.tier.clone(), memory.importance_score)
    }

    /// Process all memories and apply auto-tiering rules
    pub async fn apply_auto_tiering(&self) -> Result<TieringReport> {
        info!("Starting auto-tiering process to clean working memory");
        
        let memories = self.repository.get_memories_by_tier(MemoryTier::Working, Some(100), Some(0)).await?;
        let mut moved_to_warm = 0;
        let mut moved_to_cold = 0;
        
        for memory in memories {
            let (new_tier, new_importance) = self.classify_memory(&memory);
            
            if new_tier != memory.tier || (new_importance - memory.importance_score).abs() > 0.01 {
                debug!(
                    "Moving memory {} from {:?} to {:?} with importance {} -> {}",
                    memory.id, memory.tier, new_tier, memory.importance_score, new_importance
                );
                
                // Update the memory's tier and importance using a full update
                let mut updated_memory = memory.clone();
                updated_memory.tier = new_tier.clone();
                updated_memory.importance_score = new_importance;
                self.repository.update_memory(updated_memory).await?;
                
                match new_tier {
                    MemoryTier::Warm => moved_to_warm += 1,
                    MemoryTier::Cold => moved_to_cold += 1,
                    _ => {}
                }
            }
        }
        
        // Ensure working memory doesn't exceed capacity (7±2 items)
        self.enforce_working_memory_limit().await?;
        
        Ok(TieringReport {
            moved_to_warm,
            moved_to_cold,
            working_memory_count: self.repository.get_memories_by_tier(MemoryTier::Working, Some(1), Some(0))
                .await?
                .len(),
        })
    }
    
    /// Keep only the most important memories in working tier
    async fn enforce_working_memory_limit(&self) -> Result<()> {
        const MAX_WORKING_MEMORIES: usize = 9; // Miller's 7±2
        
        let working_memories = self.repository.get_memories_by_tier(MemoryTier::Working, Some(100), Some(0)).await?;
        
        if working_memories.len() > MAX_WORKING_MEMORIES {
            // Sort by combined score and importance
            let mut sorted = working_memories;
            sorted.sort_by(|a, b| {
                b.combined_score.partial_cmp(&a.combined_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.importance_score.partial_cmp(&a.importance_score)
                        .unwrap_or(std::cmp::Ordering::Equal))
            });
            
            // Move excess memories to warm tier
            for memory in sorted.iter().skip(MAX_WORKING_MEMORIES) {
                info!("Moving excess memory {} from working to warm tier", memory.id);
                let mut updated = memory.clone();
                updated.tier = MemoryTier::Warm;
                self.repository.update_memory(updated).await?;
            }
        }
        
        Ok(())
    }
}

pub struct TieringReport {
    pub moved_to_warm: usize,
    pub moved_to_cold: usize,
    pub working_memory_count: usize,
}

impl TieringReport {
    pub fn summary(&self) -> String {
        format!(
            "Auto-tiering complete: {} → warm, {} → cold. Working memory now has {} items.",
            self.moved_to_warm, self.moved_to_cold, self.working_memory_count
        )
    }
}