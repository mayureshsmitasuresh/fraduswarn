use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub city: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub transaction_id: String,
    pub user_id: String,
    pub amount: f64,
    pub merchant: String,
    pub merchant_category: String,
    pub location: Location,
    pub timestamp: DateTime<Utc>,
    pub payment_method: String,
    pub device_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub user_id: String,
    pub amount: f64,
    pub merchant: String,
    pub merchant_category: String,
    pub location: Location,
    pub payment_method: String,
    pub device_fingerprint: String,
}

impl TransactionRequest {
    pub fn to_transaction(&self) -> Transaction {
        Transaction {
            transaction_id: uuid::Uuid::new_v4().to_string(),
            user_id: self.user_id.clone(),
            amount: self.amount,
            merchant: self.merchant.clone(),
            merchant_category: self.merchant_category.clone(),
            location: self.location.clone(),
            timestamp: Utc::now(),
            payment_method: self.payment_method.clone(),
            device_fingerprint: self.device_fingerprint.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentScores {
    pub pattern: f64,
    pub anomaly: f64,
    pub geographic: f64,
    pub merchant: f64,
}

#[derive(Debug, Serialize)]
pub struct AnalysisResult {
    pub decision: String,
    pub confidence: f64,
    pub latency_ms: u64,
    pub agent_scores: AgentScores,
    pub fraud_ring_detected: bool,
    pub reasoning: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct AgentScore {
    pub risk_score: f64,
    pub reason: String,
    pub details: serde_json::Value,
}