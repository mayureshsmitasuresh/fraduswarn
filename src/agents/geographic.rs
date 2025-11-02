

use sqlx::PgPool;
use anyhow::Result;

use crate::models::transaction::{AgentScore, Location, Transaction};


pub struct GeographicAgent;

impl GeographicAgent {
    pub fn new() -> Self {
        Self
    }
    
    /// Validate transaction location against user's typical locations
    pub async fn analyze(
        &self,
        pool: &PgPool,
        transaction: &Transaction,
    ) -> Result<AgentScore> {
        tracing::info!("🔍 Geographic Agent analyzing {}", transaction.transaction_id);
        
        // Get user's recent locations
        let recent_locations = self.get_recent_locations(pool, &transaction.user_id).await?;
        
        let mut risk_score:f64 = 0.0;
        let mut reasons = Vec::new();
        
        // 1. Check if location is unknown/suspicious
        if transaction.location.country == "XX" || 
           transaction.location.city == "Unknown" ||
           (transaction.location.lat == 0.0 && transaction.location.lon == 0.0) {
            risk_score += 0.4;
            reasons.push("Unknown or suspicious location".to_string());
        }
        
        // 2. Check impossible travel (if we have recent location)
        if let Some(last_location) = recent_locations.first() {
            let distance_km = self.calculate_distance(
                &transaction.location,
                &Location {
                    city: last_location.city.clone(),
                    country: last_location.country.clone(),
                    lat: last_location.lat,
                    lon: last_location.lon,
                }
            );
            
            let time_hours = last_location.hours_ago;
            
            // If distance > 500km and time < 1 hour, likely fraud
            if distance_km > 500.0 && time_hours < 1.0 {
                risk_score += 0.5;
                reasons.push(format!(
                    "Impossible travel: {:.0}km in {:.1} hours",
                    distance_km, time_hours
                ));
            } else if distance_km > 1000.0 && time_hours < 3.0 {
                risk_score += 0.3;
                reasons.push(format!("Unlikely travel pattern: {:.0}km", distance_km));
            }
        }
        
        // 3. Check for new country
        let known_countries: Vec<String> = recent_locations.iter()
            .map(|l| l.country.clone())
            .collect();
        
        if !known_countries.contains(&transaction.location.country) {
            risk_score += 0.2;
            reasons.push(format!("First transaction in {}", transaction.location.country));
        }
        
        risk_score = risk_score.clamp(0.0, 1.0);
        
        let reason = if reasons.is_empty() {
            format!("Normal location: {}, {}", transaction.location.city, transaction.location.country)
        } else {
            reasons.join("; ")
        };
        
        tracing::info!("✅ Geographic Agent: {:.2} - {}", risk_score, reason);
        
        Ok(AgentScore {
            risk_score,
            reason,
            details: serde_json::json!({
                "current_location": {
                    "city": transaction.location.city,
                    "country": transaction.location.country
                },
                "recent_countries": known_countries,
            }),
        })
    }
    
    async fn get_recent_locations(
        &self,
        pool: &PgPool,
        user_id: &str,
    ) -> Result<Vec<RecentLocation>> {
        let locations = sqlx::query_as::<_, RecentLocation>(
            r#"
            SELECT 
                COALESCE(location->>'city', 'Unknown') as city,
                COALESCE(location->>'country', 'Unknown') as country,
                COALESCE((location->>'lat')::float8, 0.0) as lat,
                COALESCE((location->>'lon')::float8, 0.0) as lon,
                EXTRACT(EPOCH FROM (NOW() - timestamp)) / 3600 as hours_ago
            FROM transactions
            WHERE user_id = $1
            AND timestamp > NOW() - INTERVAL '7 days'
            AND location IS NOT NULL
            ORDER BY timestamp DESC
            LIMIT 10
            "#
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        
        Ok(locations)
    }
    
    fn calculate_distance(&self, loc1: &Location, loc2: &Location) -> f64 {
        // Haversine formula for distance between two lat/lon points
        let r = 6371.0; // Earth radius in km
        
        let lat1 = loc1.lat.to_radians();
        let lat2 = loc2.lat.to_radians();
        let delta_lat = (loc2.lat - loc1.lat).to_radians();
        let delta_lon = (loc2.lon - loc1.lon).to_radians();
        
        let a = (delta_lat / 2.0).sin().powi(2) +
                lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        
        r * c
    }
}

#[derive(sqlx::FromRow, Debug)]
struct RecentLocation {
    city: String,
    country: String,
    lat: f64,
    lon: f64,
    hours_ago: f64,
}