use std::{collections::HashMap};

use axum::{Json, extract::State, response::IntoResponse};
use candle_core::{Device, Tensor, safetensors};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

use crate::AppState;

#[derive(Deserialize)]
pub struct EmbeddingRequest {
    text: String,
}

#[derive(Serialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
    dimension: usize,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}


//load gemma model
pub async fn load_model() -> anyhow::Result<(HashMap<String, Tensor>, Tokenizer, Device)> {
    //declare device to use cpu
    let device = Device::Cpu;

    // Load model and tokenizers from local directory (note: embeddgemma with double 'd')
    let model_path = std::path::Path::new("src/embeddgemma");
    let tokenizer_file = model_path.join("tokenizer.json");

    // Check if tokenizer exists
    if !tokenizer_file.exists() {
        anyhow::bail!("Tokenizer file not found: {:?}", tokenizer_file);
    }

    //load tokenizers
    let tokenizer = match Tokenizer::from_file(&tokenizer_file) {
        std::result::Result::Ok(tok) => tok,
        Err(e) => {
            eprintln!("Failed to load tokenizer.json: {}", e);
            eprintln!("Trying tokenizer.model instead...");

            // Try the .model file as fallback
            let model_tokenizer_file = model_path.join("tokenizer.model");
            if model_tokenizer_file.exists() {
                // For SentencePiece tokenizers
                Tokenizer::from_file(&model_tokenizer_file)
                    .map_err(|e| anyhow::anyhow!("Failed to load tokenizer.model: {}", e))?
            } else {
                anyhow::bail!("Could not load any tokenizer file: {}", e);
            }
        }
    };
    //load safetensors
    //load a file
    let model_file = model_path.join("model.safetensors");
    if !model_file.exists() {
        anyhow::bail!("Model file not found: {:?}", model_file);
    }

    //load safe tensors
    let tensors = safetensors::load(model_file, &device)?;

    tracing::info!("Loaded {:?} tensors", tensors.len());

    //return loaded model tensors and edvice type
    Ok((tensors, tokenizer, device))
}

//function to generate embeddings
pub async fn generate_embedding(
    State(state): State<AppState>,
    Json(request): Json<EmbeddingRequest>,
) -> impl IntoResponse {
    match generate_embedding_internal(&state, request.text).await {
        Ok(embedding) => {
            let dimension = embedding.len();
            (
                StatusCode::OK,
                Json(EmbeddingResponse {
                    embedding,
                    dimension,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Embedding generation failed: {}", e),
            }),
        )
            .into_response(),
    }
}

//common function to generate embedding using gemma model
pub async fn generate_embedding_internal(
    state: &AppState,
    text: String,
) -> Result<Vec<f32>, String> {
    // Tokenize input text
    let tokens = state
        .tokenizer
        .encode(text.clone(), true)
        .map_err(|e| format!("Tokenization error: {}", e))?
        .get_ids()
        .to_vec();

    // Get embedding weights
    let embed_weights = state
        .tensors
        .get("embed_tokens.weight")
        .ok_or("embed_tokens.weight not found in model")?;

    // Create embeddings by indexing into embedding matrix
    let mut embeddings_vec = Vec::new();

    for &token_id in &tokens {
        let token_tensor = candle_core::Tensor::new(&[token_id as u32], &state.device)
            .map_err(|e| format!("Failed to create token tensor: {}", e))?;

        let token_embed = embed_weights
            .index_select(&token_tensor, 0)
            .map_err(|e| format!("Embedding lookup error: {}", e))?;

        embeddings_vec.push(token_embed);
    }

    // Stack embeddings (combine all token embeddings)
    let stacked = candle_core::Tensor::stack(&embeddings_vec, 0)
        .map_err(|e| format!("Stacking error: {}", e))?;

    // Mean pooling across tokens (average all token embeddings)
    let pooled = stacked
        .mean(0)
        .map_err(|e| format!("Pooling error: {}", e))?;

    // Convert to Vec<f32>
    let embedding_vec: Vec<f32> = pooled
        .squeeze(0)
        .map_err(|e| format!("Squeeze error: {}", e))?
        .to_vec1::<f32>()
        .map_err(|e| format!("Tensor conversion error: {}", e))?;

    // Normalize to unit vector (important for cosine similarity!)
    let length: f32 = embedding_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    let normalized: Vec<f32> = embedding_vec.iter().map(|x| x / length).collect();

    Ok(normalized)
}

pub fn embedding_to_pgvector(embedding: &[f32]) -> String {
    format!(
        "[{}]",
        embedding.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}