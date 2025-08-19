use crate::EmbeddingProvider;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CostEntry {
    pub provider: EmbeddingProvider,
    pub cost_usd: f64,
    pub tokens: u32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone)]
pub struct CostTracker {
    entries: Arc<RwLock<Vec<CostEntry>>>,
    provider_totals: Arc<RwLock<HashMap<EmbeddingProvider, f64>>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            provider_totals: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn track(&self, provider: EmbeddingProvider, cost_usd: f64, tokens: u32) {
        let entry = CostEntry {
            provider,
            cost_usd,
            tokens,
            timestamp: Utc::now(),
        };
        
        self.entries.write().push(entry);
        
        let mut totals = self.provider_totals.write();
        *totals.entry(provider).or_insert(0.0) += cost_usd;
    }
    
    pub fn get_total_cost(&self) -> f64 {
        self.provider_totals.read().values().sum()
    }
    
    pub fn get_provider_cost(&self, provider: EmbeddingProvider) -> f64 {
        self.provider_totals.read().get(&provider).copied().unwrap_or(0.0)
    }
    
    pub fn get_cost_breakdown(&self) -> HashMap<EmbeddingProvider, f64> {
        self.provider_totals.read().clone()
    }
    
    pub fn get_recent_entries(&self, limit: usize) -> Vec<CostEntry> {
        let entries = self.entries.read();
        let start = if entries.len() > limit {
            entries.len() - limit
        } else {
            0
        };
        entries[start..].to_vec()
    }
    
    pub fn clear(&self) {
        self.entries.write().clear();
        self.provider_totals.write().clear();
    }
}