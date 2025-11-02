use anyhow::Result;
use sqlx::PgPool;

use crate::{
    AppState,
    models::transaction::{AgentScore, Transaction},
};

#[derive(sqlx::FromRow, Debug)]
struct SimilarTxn {
    pub fraud_label: Option<bool>,
}

pub struct PatternAgent;

impl PatternAgent {
    pub fn new() -> Self {
        Self
    }

    /// Analyze if transaction matches user's normal spending pattern
    pub async fn analyze(
        &self,
        pool: &PgPool,
        state: &AppState,
        transaction: &Transaction,
    ) -> Result<AgentScore> {
        tracing::info!("🔍 Pattern Agent analyzing {}", transaction.transaction_id);

        // Get user's baseline spending
        let baseline = self.get_user_baseline(pool, &transaction.user_id).await?;

        // Log the baseline
        tracing::info!(
            "Baseline for {}: avg=${:.2}, categories={:?}",
            transaction.user_id,
            baseline.average_amount,
            baseline.common_categories
        );

        // Calculate amount deviation
        let amount_deviation = if baseline.average_amount > 0.0 {
            (transaction.amount - baseline.average_amount).abs() / baseline.average_amount
        } else {
            0.0
        };

        // Log the calculation
        tracing::info!(
            "Transaction ${:.2} vs Average ${:.2} = Deviation {:.2}",
            transaction.amount,
            baseline.average_amount,
            amount_deviation
        );

        // Check category familiarity
        let category_familiar = baseline
            .common_categories
            .contains(&transaction.merchant_category);

        // Generate embedding and find similar transactions
        let description = format!(
            "User {} spending ${} at {} in category {}",
            transaction.user_id,
            transaction.amount,
            transaction.merchant,
            transaction.merchant_category
        );

        let embedding = crate::embedding::generate_embedding_internal(state, description)
            .await
            .map_err(|e| anyhow::anyhow!("Embedding failed: {}", e))?;

        // Find similar past transactions
        let similar_txns = self
            .find_similar_transactions(pool, &embedding, &transaction.user_id, 10)
            .await?;

        // Calculate fraud rate in similar transactions
        let fraud_in_similar = if !similar_txns.is_empty() {
            similar_txns
                .iter()
                .filter(|t| t.fraud_label.unwrap_or(false))
                .count() as f64
                / similar_txns.len() as f64
        } else {
            0.0
        };

        // Combine scores
        let mut risk_score = 0.0;
        let mut reasons = Vec::new();

        // Amount deviation (30% weight)
        if amount_deviation > 3.0 {
            risk_score += 0.3;
            reasons.push(format!(
                "Amount ${:.2} is {:.1}x user's average ${:.2}",
                transaction.amount,
                transaction.amount / baseline.average_amount,
                baseline.average_amount
            ));
        } else if amount_deviation > 1.5 {
            risk_score += 0.15;
        }

        // Category unfamiliarity (20% weight)
        if !category_familiar {
            risk_score += 0.2;
            reasons.push(format!("New category '{}'", transaction.merchant_category));
        }

        // Similar fraud patterns (50% weight)
        risk_score += fraud_in_similar * 0.5;
        if fraud_in_similar > 0.3 {
            reasons.push(format!(
                "{:.0}% of similar transactions were fraud",
                fraud_in_similar * 100.0
            ));
        }

        risk_score = risk_score.clamp(0.0, 1.0);

        let reason = if reasons.is_empty() {
            "Normal spending pattern".to_string()
        } else {
            reasons.join("; ")
        };

        tracing::info!("-->Pattern Agent: {:.2} - {}", risk_score, reason);

        Ok(AgentScore {
            risk_score,
            reason,
            details: serde_json::json!({
                "amount_deviation": amount_deviation,
                "category_familiar": category_familiar,
                "fraud_in_similar": fraud_in_similar,
                "similar_count": similar_txns.len()
            }),
        })
    }

    async fn get_user_baseline(&self, pool: &PgPool, user_id: &str) -> Result<UserBaseline> {
        // First, try to get actual transaction history
        let result = sqlx::query_as::<_, UserBaseline>(
            r#"
            SELECT 
                COALESCE(AVG(amount), 0) as average_amount,
                COALESCE(ARRAY_AGG(DISTINCT merchant_category), ARRAY[]::TEXT[]) as common_categories
            FROM transactions
            WHERE user_id = $1
            AND timestamp > NOW() - INTERVAL '90 days'
            AND (fraud_label = false OR fraud_label IS NULL)
            "#
        )
        .bind(user_id)
        .fetch_one(pool)
        .await;

        match result {
            Ok(baseline) => {
                // If no transactions found, use user profile data
                if baseline.average_amount == 0.0 {
                    tracing::warn!("No transaction history for {}, using user profile", user_id);
                    return self.get_user_profile_baseline(pool, user_id).await;
                }
                tracing::info!(
                    "User {} baseline: avg=${:.2}, categories={:?}",
                    user_id,
                    baseline.average_amount,
                    baseline.common_categories
                );
                Ok(baseline)
            }
            Err(e) => {
                tracing::warn!("Failed to get baseline: {}, using user profile", e);
                self.get_user_profile_baseline(pool, user_id).await
            }
        }
    }

    // Add this new method to get baseline from user profile
    async fn get_user_profile_baseline(
        &self,
        pool: &PgPool,
        user_id: &str,
    ) -> Result<UserBaseline> {
        let result = sqlx::query_as::<_, UserBaseline>(
            r#"
            SELECT 
                AVG(amount)::float8 as average_amount,
                ARRAY_AGG(DISTINCT merchant_category) as common_categories
            FROM transactions
            WHERE user_id = $1
            AND timestamp > NOW() - INTERVAL '90 days'
            GROUP BY user_id
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap();

        Ok(result)

        // Ok(UserBaseline {
        //     average_amount: profile.average_transaction_amount.unwrap_or(0.0),
        //     common_categories: profile.common_categories.unwrap_or_default(),
        // })

        // Ok(UserBaseline {
        //     average_amount:125.00,
        //     common_categories: vec!["groceries".to_string(), "entertainment".to_string(), "utilities".to_string()],
        // })
    }

    async fn find_similar_transactions(
        &self,
        pool: &PgPool,
        embedding: &[f32],
        user_id: &str,
        limit: i32,
    ) -> Result<Vec<SimilarTxn>> {
        let embedding_str = crate::embedding::embedding_to_pgvector(embedding);

        let rows = sqlx::query_as::<_, SimilarTxn>(
            r#"
            SELECT 
                transaction_id,
                fraud_label,
                (1 - (transaction_embedding <=> $1::vector)) as similarity
            FROM transactions
            WHERE user_id = $2
            AND transaction_embedding IS NOT NULL
            ORDER BY transaction_embedding <=> $1::vector
            LIMIT $3
            "#,
        )
        .bind(embedding_str)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }
}

#[derive(sqlx::FromRow, Debug, Default)]
struct UserBaseline {
    average_amount: f64,
    common_categories: Vec<String>,
}
