use sqlx::PgPool;
use anyhow::Result;

/// Search for similar transactions using pgvector
pub async fn find_similar_transactions(
    pool: &PgPool,
    embedding: &[f32],
    user_id: &str,
    limit: i32,
) -> Result<Vec<SimilarTransaction>> {
    let embedding_str = format!(
        "[{}]",
        embedding.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    
    let rows = sqlx::query_as::<_, SimilarTransaction>(
        r#"
        SELECT 
            transaction_id,
            merchant,
            amount::float8 as amount,
            fraud_label,
            (1 - (transaction_embedding <=> $1::vector)) as similarity
        FROM transactions
        WHERE user_id = $2
        AND transaction_embedding IS NOT NULL
        ORDER BY transaction_embedding <=> $1::vector
        LIMIT $3
        "#
    )
    .bind(embedding_str)
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    Ok(rows)
}

/// Hybrid search: Combine pg_text full-text search + pgvector similarity
pub async fn hybrid_search_transactions(
    pool: &PgPool,
    text_query: &str,
    embedding: &[f32],
    limit: i32,
) -> Result<Vec<HybridSearchResult>> {
    let embedding_str = format!(
        "[{}]",
        embedding.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    
    let rows = sqlx::query_as::<_, HybridSearchResult>(
        r#"
        WITH text_matches AS (
            SELECT 
                transaction_id,
                ts_rank(description_tsv, plainto_tsquery('english', $1)) as text_score
            FROM transactions
            WHERE description_tsv @@ plainto_tsquery('english', $1)
        ),
        vector_matches AS (
            SELECT 
                transaction_id,
                (1 - (transaction_embedding <=> $2::vector)) as vector_score
            FROM transactions
            WHERE transaction_embedding IS NOT NULL
            ORDER BY transaction_embedding <=> $2::vector
            LIMIT 50
        )
        SELECT 
            t.transaction_id,
            t.merchant,
            t.amount::float8 as amount,
            t.fraud_label,
            (COALESCE(tm.text_score, 0) * 0.3 + 
             COALESCE(vm.vector_score, 0) * 0.7) as combined_score,
            COALESCE(tm.text_score, 0) as text_score,
            COALESCE(vm.vector_score, 0) as vector_score
        FROM transactions t
        LEFT JOIN text_matches tm USING (transaction_id)
        LEFT JOIN vector_matches vm USING (transaction_id)
        WHERE tm.transaction_id IS NOT NULL OR vm.transaction_id IS NOT NULL
        ORDER BY combined_score DESC
        LIMIT $3
        "#
    )
    .bind(text_query)
    .bind(embedding_str)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    Ok(rows)
}

/// Search for similar merchants using pgvector
pub async fn find_similar_merchants(
    pool: &PgPool,
    embedding: &[f32],
    limit: i32,
) -> Result<Vec<SimilarMerchant>> {
    let embedding_str = format!(
        "[{}]",
        embedding.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    
    let rows = sqlx::query_as::<_, SimilarMerchant>(
        r#"
        SELECT 
            merchant_name,
            category,
            fraud_rate::float8 as fraud_rate,
            total_transactions,
            (1 - (merchant_embedding <=> $1::vector)) as similarity
        FROM merchants
        WHERE merchant_embedding IS NOT NULL
        ORDER BY merchant_embedding <=> $1::vector
        LIMIT $2
        "#
    )
    .bind(embedding_str)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    Ok(rows)
}

// Result types - using f64 instead of Decimal

#[derive(sqlx::FromRow, Debug)]
pub struct SimilarTransaction {
    pub transaction_id: String,
    pub merchant: String,
    pub amount: f64,
    pub fraud_label: Option<bool>,
    pub similarity: f64,
}

#[derive(sqlx::FromRow, Debug)]
pub struct HybridSearchResult {
    pub transaction_id: String,
    pub merchant: String,
    pub amount: f64,
    pub fraud_label: Option<bool>,
    pub combined_score: f64,
    pub text_score: f64,
    pub vector_score: f64,
}

#[derive(sqlx::FromRow, Debug)]
pub struct SimilarMerchant {
    pub merchant_name: String,
    pub category: String,
    pub fraud_rate: f64,
    pub total_transactions: i32,
    pub similarity: f64,
}