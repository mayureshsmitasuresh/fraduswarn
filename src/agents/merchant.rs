use sqlx::PgPool;
use anyhow::Result;

use crate::{AppState, models::transaction::{AgentScore, Transaction}};

pub struct MerchantAgent;

impl MerchantAgent {
    pub fn new() -> Self {
        Self
    }
    
    /// Analyze merchant reputation using pg_text search + pgvector similarity
    pub async fn analyze(
        &self,
        pool: &PgPool,
        state: &AppState,
        transaction: &Transaction,
    ) -> Result<AgentScore> {
        tracing::info!("🔍 Merchant Agent analyzing {}", transaction.transaction_id);
        
        let mut risk_score: f64 = 0.0;
        let mut reasons = Vec::new();
        
        // 1. Get merchant from database
        let merchant_info = self.get_merchant_info(pool, &transaction.merchant).await?;
        
        if let Some(ref merchant) = merchant_info {
            // Check fraud rate
            if merchant.fraud_rate > 0.3 {
                risk_score += 0.5;
                reasons.push(format!(
                    "High-risk merchant: {:.0}% fraud rate",
                    merchant.fraud_rate * 100.0
                ));
            } else if merchant.fraud_rate > 0.1 {
                risk_score += 0.25;
                reasons.push(format!("Elevated risk merchant: {:.0}% fraud rate", merchant.fraud_rate * 100.0));
            }
            
            // Check if merchant is new (low transaction count)
            if merchant.total_transactions < 10 {
                risk_score += 0.2;
                reasons.push("New/unknown merchant".to_string());
            }
        } else {
            // Merchant not in database - could be new or suspicious
            risk_score += 0.3;
            reasons.push("Unrecognized merchant".to_string());
        }
        
        // 2. Use pg_text to search for similar merchant fraud patterns
        let fraud_patterns = self.search_merchant_fraud_patterns(
            pool,
            &transaction.merchant,
            &transaction.merchant_category
        ).await?;
        
        if fraud_patterns > 0 {
            risk_score += 0.25;
            reasons.push(format!("Found {} similar fraud cases via pg_text search", fraud_patterns));
        }
        
        // 3. Use pgvector to find similar merchants (if merchant has embedding)
        if merchant_info.is_some() {
            let similar_risky_merchants = self.find_similar_risky_merchants(
                pool,
                &transaction.merchant
            ).await?;
            
            if similar_risky_merchants > 0 {
                risk_score += 0.2;
                reasons.push(format!("{} similar high-risk merchants found", similar_risky_merchants));
            }
        }
        
        risk_score = risk_score.clamp(0.0, 1.0);
        
        let reason = if reasons.is_empty() {
            format!("Trusted merchant: {}", transaction.merchant)
        } else {
            reasons.join("; ")
        };
        
        tracing::info!("✅ Merchant Agent: {:.2} - {}", risk_score, reason);
        
        Ok(AgentScore {
            risk_score,
            reason,
            details: serde_json::json!({
                "merchant": transaction.merchant,
                "category": transaction.merchant_category,
                "fraud_patterns_found": fraud_patterns,
            }),
        })
    }
    
    async fn get_merchant_info(
        &self,
        pool: &PgPool,
        merchant_name: &str,
    ) -> Result<Option<MerchantInfo>> {
        let merchant = sqlx::query_as::<_, MerchantInfo>(
            r#"
            SELECT 
                merchant_name,
                fraud_rate::float8 as fraud_rate,
                total_transactions
            FROM merchants
            WHERE merchant_name = $1
            "#
        )
        .bind(merchant_name)
        .fetch_optional(pool)
        .await?;
        
        Ok(merchant)
    }
    
    /// Use pg_text to search for fraud patterns mentioning this merchant
    async fn search_merchant_fraud_patterns(
        &self,
        pool: &PgPool,
        merchant_name: &str,
        category: &str,
    ) -> Result<i64> {
        let search_query = format!("{} {} fraud scam suspicious", merchant_name, category);
        
        let result = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM transactions
            WHERE description_tsv @@ plainto_tsquery('english', $1)
            AND fraud_label = true
            "#
        )
        .bind(search_query)
        .fetch_one(pool)
        .await?;
        
        Ok(result)
    }
    
    /// Use pgvector to find similar merchants with high fraud rates
    async fn find_similar_risky_merchants(
        &self,
        pool: &PgPool,
        merchant_name: &str,
    ) -> Result<i64> {
        let result = sqlx::query_scalar::<_, i64>(
            r#"
            WITH current_merchant AS (
                SELECT merchant_embedding
                FROM merchants
                WHERE merchant_name = $1
                AND merchant_embedding IS NOT NULL
            )
            SELECT COUNT(*)
            FROM merchants m, current_merchant cm
            WHERE m.fraud_rate > 0.3
            AND m.merchant_embedding IS NOT NULL
            AND (1 - (m.merchant_embedding <=> cm.merchant_embedding)) > 0.7
            LIMIT 10
            "#
        )
        .bind(merchant_name)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
        
        Ok(result)
    }
}

#[derive(sqlx::FromRow, Debug)]
struct MerchantInfo {
    merchant_name: String,
    fraud_rate: f64,
    total_transactions: i32,
    // Removed merchant_embedding - we'll query it separately if needed
}