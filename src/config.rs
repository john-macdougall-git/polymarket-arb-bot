use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub min_profit_threshold: Decimal,
    pub max_position_size: Decimal,
    pub csv_output_path: String,
    pub polymarket_api_key: Option<String>,
    pub polymarket_private_key: Option<String>,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        // Load .env file if it exists
        let _ = dotenvy::dotenv();

        let min_profit_threshold = env::var("MIN_PROFIT_THRESHOLD")
            .unwrap_or_else(|_| "0.01".to_string())
            .parse()
            .context("Failed to parse MIN_PROFIT_THRESHOLD")?;

        let max_position_size = env::var("MAX_POSITION_SIZE")
            .unwrap_or_else(|_| "100.0".to_string())
            .parse()
            .context("Failed to parse MAX_POSITION_SIZE")?;

        let csv_output_path = env::var("CSV_OUTPUT_PATH")
            .unwrap_or_else(|_| "./opportunities.csv".to_string());

        let polymarket_api_key = env::var("POLYMARKET_API_KEY").ok();
        let polymarket_private_key = env::var("POLYMARKET_PRIVATE_KEY").ok();

        Ok(Config {
            min_profit_threshold,
            max_position_size,
            csv_output_path,
            polymarket_api_key,
            polymarket_private_key,
        })
    }
}
