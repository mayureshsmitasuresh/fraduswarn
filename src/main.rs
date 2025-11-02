mod agents;
mod analysis;
mod db;
mod embedding;
mod models;
mod seed_data;
use axum::response::Html;
use axum::{Router, serve};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    routing::{get, post},
};
use candle_core::{Device, Tensor};
use std::fs;
use std::{collections::HashMap, env, sync::Arc};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

use sqlx::PgPool;
use tokenizers::Tokenizer;
use tokio::net::TcpListener;

use tracing_subscriber::prelude::*;

use crate::analysis::FraudAnalyzer;
use crate::models::transaction::AnalysisResult;
use crate::{
    agents::pattern::PatternAgent, embedding::load_model, models::transaction::TransactionRequest,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tensors: Arc<HashMap<String, Tensor>>,
    pub tokenizer: Arc<Tokenizer>,
    pub device: Device,
}

async fn test_pattern_agent(
    State(app_state): State<AppState>,
    Json(request): Json<TransactionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let transaction = request.to_transaction();
    let agent = PatternAgent::new();

    match agent
        .analyze(&app_state.pool, &app_state, &transaction)
        .await
    {
        Ok(score) => Ok(Json(serde_json::json!({
            "agent": "Pattern",
            "risk_score": score.risk_score,
            "reason": score.reason,
            "details": score.details
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

//main function to call orchestrator
async fn analyze_transaction(
    State(app_state): State<AppState>,
    Json(request): Json<TransactionRequest>,
) -> Result<Json<AnalysisResult>, (StatusCode, String)> {
    tracing::info!("📥 Received transaction for user: {}", request.user_id);

    let analyzer = FraudAnalyzer::new(app_state.pool.clone());

    match analyzer
        .analyze_transaction(&app_state.pool, &app_state, request)
        .await
    {
        Ok(result) => {
            tracing::info!("✅ Analysis complete: {}", result.decision);
            Ok(Json(result))
        }
        Err(e) => {
            tracing::error!("❌ Analysis failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Analysis failed: {}", e),
            ))
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    // Load .env file
    let _ = dotenvy::dotenv();

    // Load database pool
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = crate::db::pool::create_pool(&database_url).await?;

    //call function to load gemma model
    let (tensors, tokenizers, device) = load_model().await?;

    //declare the listener
    let port = env::var("PORT");
    let address = format!("0.0.0.0:{}", port.unwrap_or("2008".to_string()));
    let listener = TcpListener::bind(address.clone()).await.unwrap();

    //declare appstate
    let app_state = AppState {
        pool: pool.clone(),
        tensors: Arc::new(tensors),
        tokenizer: Arc::new(tokenizers),
        device,
    };
    //cors
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // println!("🚀 Starting database seeding...");
    // seed_data::seed_database(&app_state).await?;
    // println!("-->Database seeding completed!");

    //app router and handlers
    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/api/pattern", post(test_pattern_agent))
        .route("/api/analyze", post(analyze_transaction))
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(app_state);

    //server the api
    tracing::info!("Server listening on {}", address);

    serve(listener, app).await.unwrap();

    Ok(())
}



async fn serve_ui() -> Html<String> {
    let html = fs::read_to_string("src/index.html")
        .unwrap_or_else(|_| "<h1>Error: Could not load UI</h1>".to_string());
    Html(html)
}