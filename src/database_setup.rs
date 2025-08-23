use anyhow::{Context, Result};
use tokio_postgres::{Config as PgConfig, NoTls};
use tracing::{error, info};
use url::Url;
use regex::Regex;

/// Database setup and validation utilities
pub struct DatabaseSetup {
    database_url: String,
}

impl DatabaseSetup {
    pub fn new(database_url: String) -> Self {
        Self { database_url }
    }

    /// Validate and sanitize database identifier to prevent injection attacks
    /// PostgreSQL identifiers must:
    /// - Start with letter or underscore
    /// - Contain only letters, digits, underscores, dollar signs  
    /// - Be 1-63 characters long
    /// - Not be a reserved keyword
    fn validate_database_identifier(identifier: &str) -> Result<String> {
        if identifier.is_empty() {
            return Err(anyhow::anyhow!("Database identifier cannot be empty"));
        }

        if identifier.len() > 63 {
            return Err(anyhow::anyhow!(
                "Database identifier too long (max 63 characters): {}",
                identifier.len()
            ));
        }

        // Check for valid PostgreSQL identifier pattern
        let identifier_regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_$]*$")
            .expect("Invalid regex for database identifier validation");
        
        if !identifier_regex.is_match(identifier) {
            return Err(anyhow::anyhow!(
                "Invalid database identifier '{}': must start with letter/underscore and contain only letters, digits, underscores, and dollar signs", 
                identifier
            ));
        }

        // Check against PostgreSQL reserved keywords
        let reserved_keywords = [
            "ALL", "ANALYSE", "ANALYZE", "AND", "ANY", "ARRAY", "AS", "ASC", "ASYMMETRIC",
            "AUTHORIZATION", "BINARY", "BOTH", "CASE", "CAST", "CHECK", "COLLATE", "COLLATION",
            "COLUMN", "CONCURRENTLY", "CONSTRAINT", "CREATE", "CROSS", "CURRENT_CATALOG",
            "CURRENT_DATE", "CURRENT_ROLE", "CURRENT_SCHEMA", "CURRENT_TIME", "CURRENT_TIMESTAMP",
            "CURRENT_USER", "DEFAULT", "DEFERRABLE", "DESC", "DISTINCT", "DO", "ELSE", "END",
            "EXCEPT", "FALSE", "FETCH", "FOR", "FOREIGN", "FREEZE", "FROM", "FULL", "GRANT",
            "GROUP", "HAVING", "ILIKE", "IN", "INITIALLY", "INNER", "INTERSECT", "INTO", "IS",
            "ISNULL", "JOIN", "LATERAL", "LEADING", "LEFT", "LIKE", "LIMIT", "LOCALTIME",
            "LOCALTIMESTAMP", "NATURAL", "NOT", "NOTNULL", "NULL", "OFFSET", "ON", "ONLY",
            "OR", "ORDER", "OUTER", "OVERLAPS", "PLACING", "PRIMARY", "REFERENCES", "RETURNING",
            "RIGHT", "SELECT", "SESSION_USER", "SIMILAR", "SOME", "SYMMETRIC", "TABLE", "TABLESAMPLE",
            "THEN", "TO", "TRAILING", "TRUE", "UNION", "UNIQUE", "USER", "USING", "VARIADIC",
            "VERBOSE", "WHEN", "WHERE", "WINDOW", "WITH"
        ];

        let upper_identifier = identifier.to_uppercase();
        if reserved_keywords.contains(&upper_identifier.as_str()) {
            return Err(anyhow::anyhow!(
                "Database identifier '{}' is a reserved PostgreSQL keyword",
                identifier
            ));
        }

        // Return the validated identifier - we'll quote it during query construction
        Ok(identifier.to_string())
    }

    /// Complete database setup process
    pub async fn setup(&self) -> Result<()> {
        info!("🗄️  Starting database setup...");

        // 1. Parse and validate the database URL
        let db_info = self.parse_database_url()?;
        info!(
            "Database: {} on {}:{}",
            db_info.database, db_info.host, db_info.port
        );

        // 2. Check if PostgreSQL is running
        self.check_postgresql_running(&db_info).await?;

        // 3. Check if the database exists, create if not
        self.ensure_database_exists(&db_info).await?;

        // 4. Check for pgvector extension availability
        self.check_pgvector_availability(&db_info).await?;

        // 5. Install pgvector extension
        self.install_pgvector_extension().await?;

        // 6. Run migrations
        self.run_migrations().await?;

        // 7. Verify setup
        self.verify_setup().await?;

        info!("✅ Database setup completed successfully!");
        Ok(())
    }

    /// Parse database URL and extract connection info
    fn parse_database_url(&self) -> Result<DatabaseInfo> {
        let url = Url::parse(&self.database_url).context("Invalid database URL format")?;

        if url.scheme() != "postgresql" && url.scheme() != "postgres" {
            return Err(anyhow::anyhow!(
                "Database URL must use postgresql:// or postgres:// scheme"
            ));
        }

        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Database URL missing host"))?
            .to_string();

        let port = url.port().unwrap_or(5432);

        let username = url.username();
        if username.is_empty() {
            return Err(anyhow::anyhow!("Database URL missing username"));
        }

        let password = url.password().unwrap_or("");

        let database = url.path().trim_start_matches('/');
        if database.is_empty() {
            return Err(anyhow::anyhow!("Database URL missing database name"));
        }

        Ok(DatabaseInfo {
            host,
            port,
            username: username.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        })
    }

    /// Check if PostgreSQL is running and accessible
    async fn check_postgresql_running(&self, db_info: &DatabaseInfo) -> Result<()> {
        info!("🔍 Checking PostgreSQL connectivity...");

        // Try to connect to the 'postgres' system database first
        let system_url = format!(
            "postgresql://{}:{}@{}:{}/postgres",
            db_info.username, db_info.password, db_info.host, db_info.port
        );

        let config: PgConfig = system_url
            .parse()
            .context("Failed to parse system database URL")?;

        match config.connect(NoTls).await {
            Ok((client, connection)) => {
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        error!("System database connection error: {}", e);
                    }
                });

                // Test basic connectivity
                client
                    .query("SELECT version()", &[])
                    .await
                    .context("Failed to query PostgreSQL version")?;

                info!("✅ PostgreSQL is running and accessible");
                Ok(())
            }
            Err(e) => {
                error!("❌ Cannot connect to PostgreSQL: {}", e);
                info!("💡 Please ensure PostgreSQL is installed and running");
                info!("💡 Common solutions:");
                info!("   - Start PostgreSQL: brew services start postgresql");
                info!("   - Or: sudo systemctl start postgresql");
                info!("   - Check connection details in DATABASE_URL");
                Err(anyhow::anyhow!("PostgreSQL is not accessible: {}", e))
            }
        }
    }

    /// Ensure the target database exists, create if necessary
    async fn ensure_database_exists(&self, db_info: &DatabaseInfo) -> Result<()> {
        info!("🔍 Checking if database '{}' exists...", db_info.database);

        // Connect to system database to check/create target database
        let system_url = format!(
            "postgresql://{}:{}@{}:{}/postgres",
            db_info.username, db_info.password, db_info.host, db_info.port
        );

        let config: PgConfig = system_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("System database connection error: {}", e);
            }
        });

        // Check if database exists
        let rows = client
            .query(
                "SELECT 1 FROM pg_database WHERE datname = $1",
                &[&db_info.database],
            )
            .await?;

        if rows.is_empty() {
            info!(
                "📋 Database '{}' does not exist, creating...",
                db_info.database
            );

            // Validate database name before creating to prevent injection
            let validated_db_name = Self::validate_database_identifier(&db_info.database)
                .context("Invalid database name for creation")?;

            // CREATE DATABASE cannot use parameters, so we use validated identifier with proper quoting
            // The validation ensures no injection is possible
            let create_query = format!("CREATE DATABASE \"{}\"", validated_db_name);
            client
                .execute(&create_query, &[])
                .await
                .context("Failed to create database")?;

            info!("✅ Database '{}' created successfully", db_info.database);
        } else {
            info!("✅ Database '{}' already exists", db_info.database);
        }

        Ok(())
    }

    /// Check if pgvector extension is available
    async fn check_pgvector_availability(&self, _db_info: &DatabaseInfo) -> Result<()> {
        info!("🔍 Checking pgvector extension availability...");

        let config: PgConfig = self.database_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check if pgvector is available in pg_available_extensions
        let rows = client
            .query(
                "SELECT name, default_version FROM pg_available_extensions WHERE name = 'vector'",
                &[],
            )
            .await?;

        if rows.is_empty() {
            error!("❌ pgvector extension is not available");
            info!("💡 Please install pgvector extension:");
            info!("   📋 On macOS (Homebrew): brew install pgvector");
            info!("   📋 On Ubuntu/Debian: apt install postgresql-15-pgvector");
            info!("   📋 From source: https://github.com/pgvector/pgvector");
            return Err(anyhow::anyhow!("pgvector extension not available"));
        } else {
            let row = &rows[0];
            let version: String = row.get(1);
            info!("✅ pgvector extension available (version: {})", version);
        }

        Ok(())
    }

    /// Install pgvector extension in the database
    async fn install_pgvector_extension(&self) -> Result<()> {
        info!("🔧 Installing pgvector extension...");

        let config: PgConfig = self.database_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check if extension is already installed
        let rows = client
            .query(
                "SELECT extname FROM pg_extension WHERE extname = 'vector'",
                &[],
            )
            .await?;

        if !rows.is_empty() {
            info!("✅ pgvector extension already installed");
            return Ok(());
        }

        // Install the extension
        client
            .execute("CREATE EXTENSION vector", &[])
            .await
            .context("Failed to install pgvector extension")?;

        info!("✅ pgvector extension installed successfully");

        // Verify installation by testing basic functionality
        client
            .query("SELECT vector_dims('[1,2,3]'::vector)", &[])
            .await
            .context("pgvector extension installation verification failed")?;

        info!("✅ pgvector extension verification passed");
        Ok(())
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        info!("📋 Running database migrations...");

        let config: PgConfig = self.database_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check if migration tracking table exists
        let tracking_exists = client
            .query(
                "SELECT 1 FROM information_schema.tables WHERE table_name = 'migration_history'",
                &[],
            )
            .await?;

        if tracking_exists.is_empty() {
            info!("📋 Creating migration tracking table...");
            client
                .execute(
                    r#"
                    CREATE TABLE migration_history (
                        id SERIAL PRIMARY KEY,
                        migration_name VARCHAR(255) NOT NULL UNIQUE,
                        applied_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                    )
                    "#,
                    &[],
                )
                .await?;
        }

        // Check if main tables exist
        let tables_exist = client
            .query(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('memories', 'memory_tiers')",
                &[],
            )
            .await?;

        if tables_exist.len() < 2 {
            info!("📋 Creating main schema tables...");
            self.create_main_schema(&client).await?;
        } else {
            info!("✅ Main schema tables already exist");
        }

        Ok(())
    }

    /// Create the main database schema
    async fn create_main_schema(&self, client: &tokio_postgres::Client) -> Result<()> {
        info!("📋 Creating main database schema...");

        // Create memories table
        client
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS memories (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    content TEXT NOT NULL,
                    embedding VECTOR(768),
                    metadata JSONB DEFAULT '{}',
                    tier VARCHAR(20) NOT NULL DEFAULT 'working',
                    importance_score FLOAT DEFAULT 0.0,
                    access_count INTEGER DEFAULT 0,
                    last_accessed TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                )
                "#,
                &[],
            )
            .await
            .context("Failed to create memories table")?;

        // Create index on embedding for vector similarity search
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS memories_embedding_idx ON memories USING hnsw (embedding vector_cosine_ops)",
                &[],
            )
            .await
            .context("Failed to create embedding index")?;

        // Create indexes for common queries
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS memories_tier_idx ON memories (tier)",
                &[],
            )
            .await
            .context("Failed to create tier index")?;

        client
            .execute(
                "CREATE INDEX IF NOT EXISTS memories_last_accessed_idx ON memories (last_accessed DESC)",
                &[],
            )
            .await
            .context("Failed to create last_accessed index")?;

        client
            .execute(
                "CREATE INDEX IF NOT EXISTS memories_importance_idx ON memories (importance_score DESC)",
                &[],
            )
            .await
            .context("Failed to create importance index")?;

        // Create memory_tiers table for tier management
        client
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS memory_tiers (
                    id SERIAL PRIMARY KEY,
                    tier_name VARCHAR(20) NOT NULL UNIQUE,
                    max_capacity INTEGER,
                    current_count INTEGER DEFAULT 0,
                    retention_days INTEGER,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                )
                "#,
                &[],
            )
            .await
            .context("Failed to create memory_tiers table")?;

        // Insert default tiers
        client
            .execute(
                r#"
                INSERT INTO memory_tiers (tier_name, max_capacity, retention_days)
                VALUES 
                    ('working', 1000, 7),
                    ('warm', 10000, 30),
                    ('cold', NULL, NULL)
                ON CONFLICT (tier_name) DO NOTHING
                "#,
                &[],
            )
            .await
            .context("Failed to insert default tiers")?;

        // Record migration
        client
            .execute(
                "INSERT INTO migration_history (migration_name) VALUES ('001_initial_schema') ON CONFLICT (migration_name) DO NOTHING",
                &[],
            )
            .await?;

        info!("✅ Main database schema created successfully");
        Ok(())
    }

    /// Verify the complete database setup
    async fn verify_setup(&self) -> Result<()> {
        info!("🔍 Verifying database setup...");

        let config: PgConfig = self.database_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check that all required tables exist
        let required_tables = ["memories", "memory_tiers", "migration_history"];
        for table in &required_tables {
            let rows = client
                .query(
                    "SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1",
                    &[table],
                )
                .await?;

            if rows.is_empty() {
                return Err(anyhow::anyhow!("Required table '{}' not found", table));
            }
        }

        // Check that pgvector extension is working
        client
            .query("SELECT vector_dims('[1,2,3]'::vector)", &[])
            .await
            .context("pgvector extension not working")?;

        // Test inserting and querying a sample memory
        info!("🧪 Testing vector operations...");

        // Insert a test memory using 768-dimensional vector (matching schema)
        let test_vector = vec![0.1f32; 768]
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        client
            .execute(
                &format!("INSERT INTO memories (content, embedding) VALUES ($1, '[{test_vector}]'::vector) ON CONFLICT DO NOTHING"),
                &[&"Setup test memory"],
            )
            .await
            .context("Failed to insert test memory")?;

        // Test vector similarity search using 768-dimensional vector
        let query_vector = vec![0.1f32; 768]
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        client
            .query(
                &format!("SELECT content FROM memories ORDER BY embedding <-> '[{query_vector}]'::vector LIMIT 1"),
                &[],
            )
            .await
            .context("Failed to perform vector similarity search")?;

        // Clean up test data
        client
            .execute(
                "DELETE FROM memories WHERE content = 'Setup test memory'",
                &[],
            )
            .await?;

        info!("✅ Database setup verification passed");
        Ok(())
    }

    /// Quick database health check
    pub async fn health_check(&self) -> Result<DatabaseHealth> {
        let config: PgConfig = self.database_url.parse()?;
        let (client, connection) = config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        let mut health = DatabaseHealth::default();

        // Check basic connectivity
        match client.query("SELECT 1", &[]).await {
            Ok(_) => health.connectivity = true,
            Err(e) => {
                health.connectivity = false;
                health.issues.push(format!("Connectivity failed: {e}"));
            }
        }

        // Check pgvector extension
        match client
            .query("SELECT 1 FROM pg_extension WHERE extname = 'vector'", &[])
            .await
        {
            Ok(rows) => {
                health.pgvector_installed = !rows.is_empty();
                if !health.pgvector_installed {
                    health
                        .issues
                        .push("pgvector extension not installed".to_string());
                }
            }
            Err(e) => {
                health.issues.push(format!("Failed to check pgvector: {e}"));
            }
        }

        // Check required tables
        let required_tables = ["memories", "memory_tiers"];
        let mut tables_found = 0;
        for table in &required_tables {
            match client
                .query(
                    "SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1",
                    &[table],
                )
                .await
            {
                Ok(rows) => {
                    if rows.is_empty() {
                        health.issues.push(format!("Table '{table}' missing"));
                    } else {
                        tables_found += 1;
                    }
                }
                Err(e) => {
                    health.issues.push(format!("Failed to check table {table}: {e}"));
                }
            }
        }
        health.schema_ready = tables_found == required_tables.len();

        // Get memory count
        match client.query("SELECT COUNT(*) FROM memories", &[]).await {
            Ok(rows) => {
                let count: i64 = rows[0].get(0);
                health.memory_count = count as usize;
            }
            Err(e) => {
                health
                    .issues
                    .push(format!("Failed to get memory count: {e}"));
            }
        }

        Ok(health)
    }
}

/// Database connection information
#[derive(Debug)]
struct DatabaseInfo {
    host: String,
    port: u16,
    username: String,
    password: String,
    database: String,
}

/// Database health status
#[derive(Debug, Default)]
pub struct DatabaseHealth {
    pub connectivity: bool,
    pub pgvector_installed: bool,
    pub schema_ready: bool,
    pub memory_count: usize,
    pub issues: Vec<String>,
}

impl DatabaseHealth {
    pub fn is_healthy(&self) -> bool {
        self.connectivity && self.pgvector_installed && self.schema_ready && self.issues.is_empty()
    }

    pub fn status_summary(&self) -> String {
        if self.is_healthy() {
            format!("✅ Healthy ({} memories)", self.memory_count)
        } else {
            format!("❌ Issues: {}", self.issues.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_database_url() {
        let setup = DatabaseSetup::new("postgresql://user:pass@localhost:5432/testdb".to_string());

        let info = setup
            .parse_database_url()
            .expect("Failed to parse valid database URL");
        assert_eq!(info.host, "localhost");
        assert_eq!(info.port, 5432);
        assert_eq!(info.username, "user");
        assert_eq!(info.password, "pass");
        assert_eq!(info.database, "testdb");
    }

    #[test]
    fn test_parse_database_url_default_port() {
        let setup = DatabaseSetup::new("postgresql://user:pass@localhost/testdb".to_string());

        let info = setup
            .parse_database_url()
            .expect("Failed to parse database URL with default port");
        assert_eq!(info.port, 5432); // Should default to 5432
    }

    #[test]
    fn test_parse_invalid_database_url() {
        let setup = DatabaseSetup::new("invalid-url".to_string());
        assert!(setup.parse_database_url().is_err());

        let setup = DatabaseSetup::new("http://localhost/db".to_string());
        assert!(setup.parse_database_url().is_err());
    }
}
