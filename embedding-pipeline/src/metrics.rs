use crate::{EmbeddingProvider, ProviderMetrics};
use parking_lot::RwLock;
use prometheus::{
    register_counter_vec, register_histogram_vec, CounterVec, HistogramVec,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct MetricsCollector {
    request_counter: CounterVec,
    latency_histogram: HistogramVec,
    cost_counter: CounterVec,
    provider_metrics: Arc<RwLock<HashMap<EmbeddingProvider, ProviderMetrics>>>,
}

impl MetricsCollector {
    pub fn new() -> Result<Self, prometheus::Error> {
        let request_counter = register_counter_vec!(
            "embedding_requests_total",
            "Total number of embedding requests",
            &["provider", "status"]
        )?;
        
        let latency_histogram = register_histogram_vec!(
            "embedding_latency_seconds",
            "Embedding generation latency in seconds",
            &["provider"]
        )?;
        
        let cost_counter = register_counter_vec!(
            "embedding_cost_usd_total",
            "Total embedding cost in USD",
            &["provider"]
        )?;
        
        Ok(Self {
            request_counter,
            latency_histogram,
            cost_counter,
            provider_metrics: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub fn new_mock() -> Self {
        use prometheus::{Opts, HistogramOpts};
        
        let request_counter = prometheus::CounterVec::new(
            Opts::new("test_embedding_requests_total", "Total number of embedding requests"),
            &["provider", "status"]
        ).unwrap();
        
        let latency_histogram = prometheus::HistogramVec::new(
            HistogramOpts::new("test_embedding_latency_seconds", "Embedding generation latency in seconds"),
            &["provider"]
        ).unwrap();
        
        let cost_counter = prometheus::CounterVec::new(
            Opts::new("test_embedding_cost_usd_total", "Total embedding cost in USD"),
            &["provider"]
        ).unwrap();
        
        Self {
            request_counter,
            latency_histogram,
            cost_counter,
            provider_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn record_request(
        &self,
        provider: EmbeddingProvider,
        success: bool,
        latency: Duration,
        cost_usd: Option<f64>,
    ) {
        let provider_str = match provider {
            EmbeddingProvider::Claude => "claude",
            EmbeddingProvider::Gpu => "gpu",
            EmbeddingProvider::LocalCpu => "local",
        };
        
        let status = if success { "success" } else { "failure" };
        
        self.request_counter
            .with_label_values(&[provider_str, status])
            .inc();
        
        self.latency_histogram
            .with_label_values(&[provider_str])
            .observe(latency.as_secs_f64());
        
        if let Some(cost) = cost_usd {
            self.cost_counter
                .with_label_values(&[provider_str])
                .inc_by(cost);
        }
        
        // Update internal metrics
        let mut metrics = self.provider_metrics.write();
        let provider_metric = metrics.entry(provider).or_insert_with(ProviderMetrics::default);
        
        provider_metric.total_requests += 1;
        if success {
            provider_metric.successful_requests += 1;
        } else {
            provider_metric.failed_requests += 1;
        }
        
        provider_metric.total_latency_ms += latency.as_millis() as u64;
        provider_metric.avg_latency_ms = 
            provider_metric.total_latency_ms as f64 / provider_metric.total_requests as f64;
        
        if let Some(cost) = cost_usd {
            provider_metric.total_cost_usd += cost;
        }
    }
    
    pub fn get_provider_metrics(&self, provider: EmbeddingProvider) -> Option<ProviderMetrics> {
        self.provider_metrics.read().get(&provider).cloned()
    }
    
    pub fn get_all_metrics(&self) -> HashMap<EmbeddingProvider, ProviderMetrics> {
        self.provider_metrics.read().clone()
    }
}