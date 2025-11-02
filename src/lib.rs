pub mod agents;
pub mod analysis;
pub mod db;
pub mod embedding;
pub mod models;
pub mod seed_data;

pub use agents::*;
pub use analysis::FraudAnalyzer;
pub use db::pool::create_pool;
pub use models::*;

// Re-export AppState
use candle_core::{Device, Tensor};
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokenizers::Tokenizer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tensors: Arc<HashMap<String, Tensor>>,
    pub tokenizer: Arc<Tokenizer>,
    pub device: Device,
}
