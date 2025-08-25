use crate::{
    backup::BackupManager,
    manager::ServerManager,
    mcp_server::{MCPServer, MCPServerConfig},
    memory::{
        connection::create_pool, silent_harvester::SilentHarvesterService,
        tier_manager::TierManager,
    },
    monitoring::HealthChecker,
    Config, DatabaseSetup, MemoryRepository, SetupManager, SimpleEmbedder,
};

#[cfg(feature = "codex-dreams")]
use crate::insights::{
    processor::{InsightsProcessor, ProcessorConfig},
    storage::InsightStorage,
    ollama_client::{OllamaClient, OllamaConfig},
};
use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

/// Dependency injection container for the application
pub struct DependencyContainer {
    // Core configuration
    pub config: Config,

    // Database layer
    pub db_pool: Arc<PgPool>,

    // Repository layer
    pub memory_repository: Arc<MemoryRepository>,

    // Service layer
    pub embedder: Arc<SimpleEmbedder>,
    pub setup_manager: Arc<SetupManager>,
    pub database_setup: Arc<DatabaseSetup>,
    pub backup_manager: Option<Arc<BackupManager>>,
    pub tier_manager: Option<Arc<TierManager>>,
    pub harvester_service: Option<Arc<SilentHarvesterService>>,

    // Infrastructure layer
    pub health_checker: Arc<HealthChecker>,
    pub mcp_server: Option<Arc<MCPServer>>,
    pub server_manager: Arc<ServerManager>,

    // Codex Dreams components (feature gated)
    #[cfg(feature = "codex-dreams")]
    pub ollama_client: Option<Arc<OllamaClient>>,
    #[cfg(feature = "codex-dreams")]
    pub insight_storage: Option<Arc<InsightStorage>>,
    #[cfg(feature = "codex-dreams")]
    pub insights_processor: Option<Arc<InsightsProcessor>>,
}

impl DependencyContainer {
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing dependency container...");

        // Load configuration
        let config = Config::from_env().unwrap_or_else(|_| {
            info!("⚠️  No configuration found, using defaults");
            Config::default()
        });

        // Create database connection pool
        let db_pool = Arc::new(
            create_pool(&config.database_url, config.operational.max_db_connections).await?,
        );

        // Repository layer
        let memory_repository = Arc::new(MemoryRepository::with_config(
            (*db_pool).clone(),
            config.clone(),
        ));

        // Service layer
        let embedder = Arc::new(Self::create_embedder(&config)?);
        let setup_manager = Arc::new(SetupManager::new(config.clone()));
        let database_setup = Arc::new(DatabaseSetup::new(config.database_url.clone()));

        // Infrastructure layer
        let health_checker = Arc::new(HealthChecker::new(db_pool.clone()));
        let server_manager = Arc::new(ServerManager::new());

        // Optional services
        let backup_manager = if config.backup.enabled {
            let backup_config = crate::backup::BackupConfig::default(); // Use default config for now
            Some(Arc::new(BackupManager::new(backup_config, db_pool.clone())))
        } else {
            None
        };

        let tier_manager = if config.tier_manager.enabled {
            Some(Arc::new(TierManager::new(
                memory_repository.clone(),
                config.tier_manager.clone(),
            )?))
        } else {
            None
        };

        // TODO: Harvest service requires additional dependencies that need to be properly configured
        // For now, disable until we can implement proper dependency injection
        let harvester_service = None;
        // let harvester_service = Some(Arc::new(SilentHarvesterService::new(
        //     memory_repository.clone(),
        //     importance_pipeline,
        //     embedder.clone(),
        //     Some(config.harvester.clone()),
        //     &prometheus_registry,
        // )?));

        // Initialize Codex Dreams components (feature gated)
        #[cfg(feature = "codex-dreams")]
        let (ollama_client, insight_storage, insights_processor) = {
            info!("🧠 Initializing Codex Dreams components...");
            
            // Create Ollama client with configuration from environment
            let ollama_config = OllamaConfig {
                base_url: std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://192.168.1.110:11434".to_string()),
                model: std::env::var("OLLAMA_MODEL")
                    .unwrap_or_else(|_| "gpt-oss:20b".to_string()),
                timeout_seconds: std::env::var("OLLAMA_TIMEOUT")
                    .unwrap_or_else(|_| "30".to_string())
                    .parse().unwrap_or(30),
                max_retries: 3,
                initial_retry_delay_ms: 1000,
                max_retry_delay_ms: 10000,
            };
            
            let ollama_client = match OllamaClient::new(ollama_config) {
                Ok(client) => {
                    info!("✅ Ollama client initialized");
                    Some(Arc::new(client))
                },
                Err(e) => {
                    info!("⚠️  Ollama client initialization failed: {}. Insights will be disabled.", e);
                    None
                }
            };
            
            // Create insight storage
            let insight_storage = Some(Arc::new(InsightStorage::new(
                db_pool.clone(),
                embedder.clone(),
            )));
            info!("✅ Insight storage initialized");
            
            // Create insights processor (only if Ollama client is available)
            let insights_processor = if let (Some(ollama_client), Some(insight_storage)) = 
                (ollama_client.as_ref(), insight_storage.as_ref()) {
                
                let processor_config = ProcessorConfig {
                    batch_size: std::env::var("INSIGHTS_BATCH_SIZE")
                        .unwrap_or_else(|_| "50".to_string())
                        .parse().unwrap_or(50),
                    max_retries: 3,
                    timeout_seconds: 120,
                    circuit_breaker_threshold: 5,
                    circuit_breaker_recovery_timeout: 60,
                    min_confidence_threshold: std::env::var("INSIGHTS_MIN_CONFIDENCE")
                        .unwrap_or_else(|_| "0.6".to_string())
                        .parse().unwrap_or(0.6),
                    max_insights_per_batch: 10,
                };
                
                let processor = InsightsProcessor::new(
                    memory_repository.clone(),
                    ollama_client.clone(),
                    insight_storage.clone(),
                    processor_config,
                );
                
                info!("✅ Insights processor initialized");
                Some(Arc::new(processor))
            } else {
                info!("⚠️  Insights processor disabled (missing dependencies)");
                None
            };
            
            (ollama_client, insight_storage, insights_processor)
        };

        info!("✅ Dependency container initialized successfully");

        Ok(Self {
            config,
            db_pool,
            memory_repository,
            embedder,
            setup_manager,
            database_setup,
            backup_manager,
            tier_manager,
            harvester_service,
            health_checker,
            mcp_server: None, // Created on demand
            server_manager,
            #[cfg(feature = "codex-dreams")]
            ollama_client,
            #[cfg(feature = "codex-dreams")]
            insight_storage,
            #[cfg(feature = "codex-dreams")]
            insights_processor,
        })
    }

    fn create_embedder(config: &Config) -> Result<SimpleEmbedder> {
        match config.embedding.provider.as_str() {
            "openai" => Ok(SimpleEmbedder::new(config.embedding.api_key.clone())
                .with_model(config.embedding.model.clone())
                .with_base_url(config.embedding.base_url.clone())),
            "ollama" => Ok(SimpleEmbedder::new_ollama(
                config.embedding.base_url.clone(),
                config.embedding.model.clone(),
            )),
            "mock" => Ok(SimpleEmbedder::new_mock()),
            _ => Err(anyhow::anyhow!(
                "Unsupported embedding provider: {}",
                config.embedding.provider
            )),
        }
    }

    pub async fn create_mcp_server(&self) -> Result<MCPServer> {
        let mcp_config = MCPServerConfig::default();

        #[cfg(feature = "codex-dreams")]
        {
            let server = MCPServer::new_with_insights(
                self.memory_repository.clone(),
                self.embedder.clone(),
                mcp_config,
                self.insights_processor.clone(),
                self.insight_storage.clone(),
            )?;
            Ok(server)
        }

        #[cfg(not(feature = "codex-dreams"))]
        {
            let server = MCPServer::new(
                self.memory_repository.clone(),
                self.embedder.clone(),
                mcp_config,
            )?;
            Ok(server)
        }
    }

    pub async fn health_check(&self) -> Result<bool> {
        // Quick health check using our services
        match self.database_setup.health_check().await {
            Ok(health) => Ok(health.is_healthy()),
            Err(_) => Ok(false),
        }
    }
}
