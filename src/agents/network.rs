use sqlx::PgPool;
use anyhow::Result;

use crate::models::transaction::{AgentScore, Transaction};


pub struct NetworkAgent;

impl NetworkAgent {
    pub fn new() -> Self {
        Self
    }
    
    /// Detect fraud rings - multiple users sharing devices/locations
    pub async fn analyze(
        &self,
        pool: &PgPool,
        transaction: &Transaction,
    ) -> Result<AgentScore> {
        tracing::info!("🔍 Network Agent analyzing {}", transaction.transaction_id);
        
        let mut risk_score:f64 = 0.0;
        let mut reasons = Vec::new();
        let mut fraud_ring_detected = false;
        
        // 1. Check device fingerprint sharing
        let users_sharing_device = self.check_device_sharing(
            pool,
            &transaction.device_fingerprint,
            &transaction.user_id
        ).await?;
        
        if users_sharing_device > 3 {
            risk_score += 0.4;
            fraud_ring_detected = true;
            reasons.push(format!("Device shared by {} users (fraud ring)", users_sharing_device));
        } else if users_sharing_device > 1 {
            risk_score += 0.2;
            reasons.push(format!("Device used by {} users", users_sharing_device));
        }
        
        // 2. Check for coordinated fraud (same merchant, multiple users, short time)
        let coordinated_transactions = self.check_coordinated_fraud(
            pool,
            &transaction.merchant,
            &transaction.timestamp.to_rfc3339()
        ).await?;
        
        if coordinated_transactions > 5 {
            risk_score += 0.3;
            fraud_ring_detected = true;
            reasons.push(format!("{} coordinated transactions at same merchant", coordinated_transactions));
        }
        
        // 3. Check for velocity fraud ring
        let velocity_ring = self.check_velocity_ring(
            pool,
            &transaction.device_fingerprint
        ).await?;
        
        if velocity_ring > 10 {
            risk_score += 0.3;
            fraud_ring_detected = true;
            reasons.push(format!("{} rapid transactions from this device", velocity_ring));
        }
        
        risk_score = risk_score.clamp(0.0, 1.0);
        
        let reason = if reasons.is_empty() {
            "No fraud ring indicators".to_string()
        } else {
            reasons.join("; ")
        };
        
        tracing::info!("✅ Network Agent: {:.2} - {} - Ring: {}", risk_score, reason, fraud_ring_detected);
        
        Ok(AgentScore {
            risk_score,
            reason: if fraud_ring_detected {
                format!("⚠️ FRAUD RING DETECTED: {}", reason)
            } else {
                reason
            },
            details: serde_json::json!({
                "fraud_ring_detected": fraud_ring_detected,
                "users_sharing_device": users_sharing_device,
                "coordinated_transactions": coordinated_transactions,
            }),
        })
    }
    
    async fn check_device_sharing(
        &self,
        pool: &PgPool,
        device_fingerprint: &str,
        current_user_id: &str,
    ) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(DISTINCT user_id)
            FROM transactions
            WHERE device_fingerprint = $1
            AND user_id != $2
            AND timestamp > NOW() - INTERVAL '30 days'
            "#
        )
        .bind(device_fingerprint)
        .bind(current_user_id)
        .fetch_one(pool)
        .await?;
        
        Ok(count)
    }
    
    async fn check_coordinated_fraud(
        &self,
        pool: &PgPool,
        merchant: &str,
        timestamp: &str,
    ) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(DISTINCT user_id)
            FROM transactions
            WHERE merchant = $1
            AND ABS(EXTRACT(EPOCH FROM (timestamp - $2::timestamptz))) < 3600
            "#
        )
        .bind(merchant)
        .bind(timestamp)
        .fetch_one(pool)
        .await?;
        
        Ok(count)
    }
    
    async fn check_velocity_ring(
        &self,
        pool: &PgPool,
        device_fingerprint: &str,
    ) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM transactions
            WHERE device_fingerprint = $1
            AND timestamp > NOW() - INTERVAL '1 hour'
            "#
        )
        .bind(device_fingerprint)
        .fetch_one(pool)
        .await?;
        
        Ok(count)
    }
}