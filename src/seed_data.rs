use anyhow::Result;
use chrono::{Utc, Duration};
use crate::AppState;

pub async fn seed_database(app_state: &AppState) -> Result<()> {
    println!("🌱 Seeding FraudSwarm database...\n");
    
    println!("1️⃣ Creating test users...");
    seed_users(app_state).await?;
    println!("   -->Created 5 test users\n");
    
    println!("2️⃣ Creating merchants...");
    seed_merchants(app_state).await?;
    println!("   -->Created 10 merchants\n");
    
    println!("3️⃣ Creating sample transactions...");
    seed_transactions(app_state).await?;
    println!("   -->Created 30 sample transactions\n");
    
    println!("🎉 Database seeded successfully!");
    println!("\nSample users created:");
    println!("  - user_normal_123 (normal spending)");
    println!("  - user_frequent_456 (frequent buyer)");
    println!("  - user_fraud_789 (fraudulent activity)");
    println!("  - user_traveler_321 (business traveler)");
    println!("  - user_business_654 (high-value transactions)");
    
    Ok(())
}

async fn seed_users(app_state: &AppState) -> Result<()> {
    let users = vec![
        ("user_normal_123", "normal@example.com", 150.0, vec!["groceries", "gas"]),
        ("user_frequent_456", "frequent@example.com", 500.0, vec!["electronics", "clothing"]),
        ("user_fraud_789", "fraud@example.com", 200.0, vec!["electronics"]),
        ("user_traveler_321", "traveler@example.com", 300.0, vec!["hotels", "restaurants"]),
        ("user_business_654", "business@example.com", 800.0, vec!["software", "office"]),
    ];
    
    for (user_id, email, avg_amount, categories) in users {
        sqlx::query(
            r#"
            INSERT INTO users (user_id, email, average_transaction_amount, common_categories)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id) DO UPDATE
            SET average_transaction_amount = EXCLUDED.average_transaction_amount,
                common_categories = EXCLUDED.common_categories
            "#
        )
        .bind(user_id)
        .bind(email)
        .bind(avg_amount)
        .bind(categories)
        .execute(&app_state.pool)
        .await?;
    }
    
    Ok(())
}

async fn seed_merchants(app_state: &AppState) -> Result<()> {
    let merchants = vec![
        ("BestBuy Electronics", "electronics", 0.05),
        ("Amazon Online", "general", 0.02),
        ("Shell Gas Station", "gas", 0.01),
        ("Walmart Superstore", "groceries", 0.03),
        ("ScamElectronics Inc", "electronics", 0.45), // High fraud rate!
        ("Apple Store", "electronics", 0.01),
        ("Starbucks Coffee", "food", 0.02),
        ("Hilton Hotel", "hotels", 0.03),
        ("SuspiciousShop", "general", 0.38), // High fraud rate!
        ("Target Store", "retail", 0.02),
    ];
    
    for (name, category, fraud_rate) in merchants {
        let embedding = crate::embedding::generate_embedding_internal(
            app_state,
            format!("Merchant: {} Category: {}", name, category)
        ).await
        .map_err(|e| anyhow::anyhow!("Embedding generation failed: {}", e))?;
        
        let embedding_str = crate::embedding::embedding_to_pgvector(&embedding);
        
        sqlx::query(
            r#"
            INSERT INTO merchants (merchant_name, category, fraud_rate, merchant_embedding)
            VALUES ($1, $2, $3, $4::vector)
            ON CONFLICT (merchant_name) DO UPDATE
            SET fraud_rate = EXCLUDED.fraud_rate,
                merchant_embedding = EXCLUDED.merchant_embedding,
                last_updated = NOW()
            "#
        )
        .bind(name)
        .bind(category)
        .bind(fraud_rate)
        .bind(embedding_str)
        .execute(&app_state.pool)
        .await?;
    }
    
    Ok(())
}

async fn seed_transactions(app_state: &AppState) -> Result<()> {
    let scenarios = vec![
        // Normal user transactions
        ("user_normal_123", "Walmart Superstore", 85.50, "groceries", false, 5),
        ("user_normal_123", "Shell Gas Station", 45.00, "gas", false, 10),
        ("user_normal_123", "Starbucks Coffee", 12.50, "food", false, 15),
        ("user_normal_123", "Target Store", 65.00, "retail", false, 20),
        
        // Frequent buyer - normal high spending
        ("user_frequent_456", "Amazon Online", 250.00, "general", false, 2),
        ("user_frequent_456", "BestBuy Electronics", 899.99, "electronics", false, 7),
        ("user_frequent_456", "Apple Store", 1299.00, "electronics", false, 12),
        ("user_frequent_456", "Target Store", 450.00, "retail", false, 18),
        
        // Fraud cases - high risk
        ("user_fraud_789", "ScamElectronics Inc", 2500.00, "electronics", true, 1),
        ("user_fraud_789", "SuspiciousShop", 1800.00, "general", true, 3),
        ("user_fraud_789", "ScamElectronics Inc", 3200.00, "electronics", true, 8),
        
        // Business traveler - legitimate high transactions
        ("user_traveler_321", "Hilton Hotel", 450.00, "hotels", false, 4),
        ("user_traveler_321", "Starbucks Coffee", 15.00, "food", false, 6),
        ("user_traveler_321", "Shell Gas Station", 60.00, "gas", false, 9),
        ("user_traveler_321", "Hilton Hotel", 520.00, "hotels", false, 14),
        
        // Business user - high value legitimate
        ("user_business_654", "Apple Store", 2500.00, "electronics", false, 11),
        ("user_business_654", "Amazon Online", 800.00, "general", false, 16),
        ("user_business_654", "BestBuy Electronics", 1200.00, "electronics", false, 19),
        
        // Mixed scenarios for testing
        ("user_normal_123", "Apple Store", 2000.00, "electronics", false, 25), // Unusual high amount but legit
        ("user_frequent_456", "SuspiciousShop", 1500.00, "general", true, 22), // Caught fraud
        ("user_traveler_321", "ScamElectronics Inc", 2800.00, "electronics", true, 27), // Travel + fraud
    ];
    
    for (user_id, merchant, amount, category, is_fraud, days_ago) in scenarios {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now() - Duration::days(days_ago);
        
        let description = format!("{} spending ${} at {} in {}", user_id, amount, merchant, category);
        let embedding = crate::embedding::generate_embedding_internal(app_state, description).await
            .map_err(|e| anyhow::anyhow!("Embedding generation failed: {}", e))?;
        let embedding_str = crate::embedding::embedding_to_pgvector(&embedding);
        
        // Random device fingerprint
        let device_fp = format!("fp_{}", &txn_id[..8]);
        
        sqlx::query(
            r#"
            INSERT INTO transactions (
                transaction_id, user_id, merchant, amount,
                merchant_category, timestamp, fraud_label,
                transaction_embedding, payment_method, device_fingerprint
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::vector, 'credit_card', $9)
            ON CONFLICT (transaction_id) DO NOTHING
            "#
        )
        .bind(&txn_id)
        .bind(user_id)
        .bind(merchant)
        .bind(amount)
        .bind(category)
        .bind(timestamp)
        .bind(is_fraud)
        .bind(embedding_str)
        .bind(device_fp)
        .execute(&app_state.pool)
        .await?;
    }
    
    Ok(())
}