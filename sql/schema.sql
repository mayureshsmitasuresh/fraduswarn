-- Enable extensions
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Users table
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT PRIMARY KEY,
    email TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    average_transaction_amount DECIMAL(10,2) DEFAULT 0,
    common_categories TEXT[] DEFAULT ARRAY[]::TEXT[],
    home_location JSONB
);

-- Transactions table
CREATE TABLE IF NOT EXISTS transactions (
    transaction_id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(user_id),
    amount DECIMAL(10,2) NOT NULL,
    merchant TEXT NOT NULL,
    merchant_category TEXT NOT NULL,
    location JSONB,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    payment_method TEXT,
    device_fingerprint TEXT,
    
    -- Fraud detection results
    fraud_label BOOLEAN,
    risk_score DECIMAL(3,2),
    decision TEXT,
    
    -- Agent scores
    pattern_score DECIMAL(3,2),
    anomaly_score DECIMAL(3,2),
    geographic_score DECIMAL(3,2),
    merchant_score DECIMAL(3,2),
    
    -- Vector embedding for semantic search
    transaction_embedding vector(768),
    
    -- Full-text search
    description_tsv tsvector GENERATED ALWAYS AS (
        to_tsvector('english', merchant || ' ' || merchant_category)
    ) STORED
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_transactions_user ON transactions(user_id);
CREATE INDEX IF NOT EXISTS idx_transactions_timestamp ON transactions(timestamp);
CREATE INDEX IF NOT EXISTS idx_transactions_merchant ON transactions(merchant);
CREATE INDEX IF NOT EXISTS idx_transactions_embedding ON transactions 
    USING ivfflat (transaction_embedding vector_cosine_ops) 
    WITH (lists = 100);
CREATE INDEX IF NOT EXISTS idx_transactions_tsv ON transactions USING gin(description_tsv);

-- Merchants table
CREATE TABLE IF NOT EXISTS merchants (
    merchant_id SERIAL PRIMARY KEY,
    merchant_name TEXT UNIQUE NOT NULL,
    category TEXT,
    fraud_rate DECIMAL(5,4) DEFAULT 0,
    total_transactions INTEGER DEFAULT 0,
    fraud_transactions INTEGER DEFAULT 0,
    merchant_embedding vector(768),
    last_updated TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_merchants_embedding ON merchants 
    USING ivfflat (merchant_embedding vector_cosine_ops)
    WITH (lists = 100);

-- Appeals table
CREATE TABLE IF NOT EXISTS appeals (
    appeal_id SERIAL PRIMARY KEY,
    transaction_id TEXT REFERENCES transactions(transaction_id),
    user_id TEXT REFERENCES users(user_id),
    user_feedback TEXT NOT NULL,
    feedback_embedding vector(768),
    resolution TEXT,
    was_fraud BOOLEAN,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_appeals_feedback_embedding ON appeals 
    USING ivfflat (feedback_embedding vector_cosine_ops)
    WITH (lists = 100);

-- Fraud rings table
CREATE TABLE IF NOT EXISTS fraud_rings (
    ring_id SERIAL PRIMARY KEY,
    merchant TEXT,
    detected_at TIMESTAMPTZ DEFAULT NOW(),
    victim_count INTEGER,
    total_amount DECIMAL(12,2),
    pattern_description TEXT,
    status TEXT DEFAULT 'ACTIVE'
);

-- Agent performance tracking
CREATE TABLE IF NOT EXISTS agent_performance (
    id SERIAL PRIMARY KEY,
    agent_name TEXT NOT NULL,
    date DATE DEFAULT CURRENT_DATE,
    total_predictions INTEGER DEFAULT 0,
    false_positives INTEGER DEFAULT 0,
    false_negatives INTEGER DEFAULT 0,
    accuracy DECIMAL(5,4),
    UNIQUE(agent_name, date)
);