use sqlx::postgres::{PgPool, PgPoolOptions};
use anyhow::Result;

pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;
    
    tracing::info!("-->Connected to Tiger Cloud database");
    
    Ok(pool)
}

pub async fn test_connection(pool: &PgPool) -> Result<()> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await?;
    
    tracing::info!("-->Database connection test successful");
    
    Ok(())
}