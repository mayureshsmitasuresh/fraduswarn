use anyhow::Result;
use sqlx::PgPool;
use std::time::Instant;

use crate::{AppState, agents::{anomaly::AnomalyAgent, geographic::GeographicAgent, merchant::MerchantAgent, network::NetworkAgent, pattern::PatternAgent}, models::transaction::{AgentScores, AnalysisResult, TransactionRequest}};


/// Orchestrates fraud analysis using multiple agents
pub struct FraudAnalyzer {
    pattern_agent: PatternAgent,
    anomaly_agent: AnomalyAgent,
    geographic_agent: GeographicAgent,
    merchant_agent: MerchantAgent,
    network_agent: NetworkAgent,
}

impl FraudAnalyzer {
    pub fn new(_pool: PgPool) -> Self {
        Self {
            pattern_agent: PatternAgent::new(),
            anomaly_agent: AnomalyAgent::new(),
            geographic_agent: GeographicAgent::new(),
            merchant_agent: MerchantAgent::new(),
            network_agent: NetworkAgent::new(),
        }
    }

    /// Analyze a transaction for fraud using all 5 agents
    pub async fn analyze_transaction(
        &self,
        pool: &PgPool,
        state: &AppState,
        request: TransactionRequest,
    ) -> Result<AnalysisResult> {
        let start = Instant::now();
        let transaction = request.to_transaction();

        tracing::info!("🔍 Analyzing transaction: {}", transaction.transaction_id);
        tracing::info!("🤖 Running all 5 fraud detection agents in parallel...");

        // Run all agents in parallel for maximum performance
        let (pattern_result, anomaly_result, geo_result, merchant_result, network_result) = tokio::join!(
            self.pattern_agent.analyze(pool, state, &transaction),
            self.anomaly_agent.analyze(pool, &transaction),
            self.geographic_agent.analyze(pool, &transaction),
            self.merchant_agent.analyze(pool, state, &transaction),
            self.network_agent.analyze(pool, &transaction),
        );

        // Unwrap all results
        let pattern_score = pattern_result?;
        let anomaly_score = anomaly_result?;
        let geographic_score = geo_result?;
        let merchant_score = merchant_result?;
        let network_score = network_result?;

        tracing::info!(
            "📊 Agent Scores - Pattern: {:.2}, Anomaly: {:.2}, Geographic: {:.2}, Merchant: {:.2}, Network: {:.2}",
            pattern_score.risk_score,
            anomaly_score.risk_score,
            geographic_score.risk_score,
            merchant_score.risk_score,
            network_score.risk_score
        );

        // Weighted average of all agents
        // Pattern (25%) + Anomaly (20%) + Geographic (15%) + Merchant (25%) + Network (15%)
        let avg_score = (
            pattern_score.risk_score * 0.25 +
            anomaly_score.risk_score * 0.20 +
            geographic_score.risk_score * 0.15 +
            merchant_score.risk_score * 0.25 +
            network_score.risk_score * 0.15
        );

        // Check if fraud ring detected by network agent
        let fraud_ring_detected = network_score.reason.contains("FRAUD RING DETECTED");

        // Make decision based on aggregated score
        let (decision, confidence) = if fraud_ring_detected {
            // Always block fraud rings with high confidence
            ("BLOCK".to_string(), 0.95)
        } else if avg_score > 0.7 {
            ("BLOCK".to_string(), 0.90)
        } else if avg_score > 0.4 {
            ("CHALLENGE".to_string(), 0.75)
        } else {
            ("APPROVE".to_string(), 0.85)
        };

        let total_latency = start.elapsed();

        // Build comprehensive reasoning from all agents
        let reasoning = format!(
            "Pattern: {} | Anomaly: {} | Geographic: {} | Merchant: {} | Network: {}",
            pattern_score.reason,
            anomaly_score.reason,
            geographic_score.reason,
            merchant_score.reason,
            network_score.reason
        );

        tracing::info!(
            "✅ Analysis complete in {:.2}ms - Decision: {} (confidence: {:.0}%) - Avg Risk: {:.2}",
            total_latency.as_millis(),
            decision,
            confidence * 100.0,
            avg_score
        );

        if fraud_ring_detected {
            tracing::warn!("⚠️ FRAUD RING DETECTED!");
        }

        Ok(AnalysisResult {
            decision,
            confidence,
            latency_ms: total_latency.as_millis() as u64,
            agent_scores: AgentScores {
                pattern: pattern_score.risk_score,
                anomaly: anomaly_score.risk_score,
                geographic: geographic_score.risk_score,
                merchant: merchant_score.risk_score,
            },
            fraud_ring_detected,
            reasoning,
        })
    }
}