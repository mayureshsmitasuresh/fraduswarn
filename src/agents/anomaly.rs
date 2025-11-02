use sqlx::PgPool;
use anyhow::Result;
use chrono::{ Timelike, Utc};

use crate::models::transaction::{AgentScore, Transaction};


pub struct AnomalyAgent;

impl AnomalyAgent {
    pub fn new() -> Self {
        Self
    }
    
    /// Detect anomalies in transaction timing, frequency, and amount patterns
    pub async fn analyze(
        &self,
        pool: &PgPool,
        transaction: &Transaction,
    ) -> Result<AgentScore> {
        tracing::info!("🔍 Anomaly Agent analyzing {}", transaction.transaction_id);
        
        // Get user's recent transaction history
        let recent_txns = self.get_recent_transactions(pool, &transaction.user_id).await?;
        
        let mut risk_score: f64 = 0.0;
        let mut reasons = Vec::new();
        
        // 1. Check transaction frequency (velocity)
        let txns_last_hour = recent_txns.iter()
            .filter(|t| t.minutes_ago <= 60.0)
            .count();
        
        if txns_last_hour >= 5 {
            risk_score += 0.3;
            reasons.push(format!("{} transactions in last hour (high velocity)", txns_last_hour));
        } else if txns_last_hour >= 3 {
            risk_score += 0.15;
        }
        
        // 2. Check unusual time (late night transactions)
        let hour = Utc::now().time().hour();  // Fixed: use .time().hour()
        if hour >= 2 && hour <= 5 {
            risk_score += 0.2;
            reasons.push(format!("Transaction at unusual hour: {}:00", hour));
        }
        
        // 3. Check for rapid successive transactions
        if let Some(last_txn) = recent_txns.first() {
            if last_txn.minutes_ago < 5.0 {
                risk_score += 0.25;
                reasons.push(format!("Transaction only {:.0} minutes after previous", last_txn.minutes_ago));
            }
        }
        
        // 4. Check amount spike pattern
        if !recent_txns.is_empty() {
            let avg_amount: f64 = recent_txns.iter()
                .map(|t| t.amount)
                .sum::<f64>() / recent_txns.len() as f64;
            
            if transaction.amount > avg_amount * 3.0 {
                risk_score += 0.25;
                reasons.push(format!("Amount ${:.2} is 3x recent average ${:.2}", transaction.amount, avg_amount));
            }
        }
        
        risk_score = risk_score.clamp(0.0, 1.0);
        
        let reason = if reasons.is_empty() {
            "Normal transaction timing and frequency".to_string()
        } else {
            reasons.join("; ")
        };
        
        tracing::info!("✅ Anomaly Agent: {:.2} - {}", risk_score, reason);
        
        Ok(AgentScore {
            risk_score,
            reason,
            details: serde_json::json!({
                "transactions_last_hour": txns_last_hour,
                "hour_of_day": hour,
                "recent_transaction_count": recent_txns.len()
            }),
        })
    }
    
    async fn get_recent_transactions(
        &self,
        pool: &PgPool,
        user_id: &str,
    ) -> Result<Vec<RecentTransaction>> {
        let txns = sqlx::query_as::<_, RecentTransaction>(
            r#"
            SELECT 
                amount::float8 as amount,
                EXTRACT(EPOCH FROM (NOW() - timestamp)) / 60 as minutes_ago
            FROM transactions
            WHERE user_id = $1
            AND timestamp > NOW() - INTERVAL '24 hours'
            ORDER BY timestamp DESC
            LIMIT 20
            "#
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        
        Ok(txns)
    }
}

#[derive(sqlx::FromRow, Debug)]
struct RecentTransaction {
    amount: f64,
    minutes_ago: f64,
}