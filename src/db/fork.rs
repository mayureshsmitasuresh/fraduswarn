use sqlx::PgPool;
use anyhow::Result;

pub struct ForkManager {
    main_pool: PgPool,
}

impl ForkManager {
    pub fn new(pool: PgPool) -> Self {
        Self { main_pool: pool }
    }
    
    /// Create a new database fork for user analysis
    pub async fn create_fork(&self, fork_name: &str) -> Result<String> {
        tracing::info!("Creating fork: {}", fork_name);
        
        // Tiger Cloud fork creation
        sqlx::query("SELECT create_fork($1)")
            .bind(fork_name)
            .execute(&self.main_pool)
            .await?;
        
        tracing::info!("✅ Fork created: {}", fork_name);
        Ok(fork_name.to_string())
    }
    
    /// Connect to a specific fork
    pub async fn connect_to_fork(&self, fork_name: &str) -> Result<PgPool> {
        let base_url = std::env::var("DATABASE_URL")?;
        
        // Modify connection to use the fork
        // Tiger Cloud uses schema-based forks
        let fork_pool = PgPool::connect(&base_url).await?;
        
        // Set search path to the fork schema
        sqlx::query(&format!("SET search_path TO {}", fork_name))
            .execute(&fork_pool)
            .await?;
        
        tracing::info!("✅ Connected to fork: {}", fork_name);
        Ok(fork_pool)
    }
    
    /// Delete a fork after analysis
    pub async fn cleanup_fork(&self, fork_name: &str) -> Result<()> {
        tracing::info!("Cleaning up fork: {}", fork_name);
        
        sqlx::query("SELECT delete_fork($1)")
            .bind(fork_name)
            .execute(&self.main_pool)
            .await?;
        
        tracing::info!("✅ Fork deleted: {}", fork_name);
        Ok(())
    }
    
    /// Generate unique fork name for transaction
    pub fn generate_fork_name(user_id: &str, transaction_id: &str) -> String {
        format!("user_{}_txn_{}", 
            user_id.replace("-", ""), 
            &transaction_id[..8]
        )
    }
}