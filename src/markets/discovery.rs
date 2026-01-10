use crate::types::Market;
use anyhow::{Context, Result};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::info;

const GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";

/// Market data from Polymarket API
#[derive(Debug, Deserialize)]
struct ApiMarket {
    #[serde(rename = "conditionId")]
    condition_id: String,
    question: String,
    #[serde(rename = "clobTokenIds")]
    clob_token_ids: String, // JSON string array like "[\"token1\", \"token2\"]"
    outcomes: String,        // JSON string array like "[\"Yes\", \"No\"]"
    #[serde(rename = "outcomePrices")]
    outcome_prices: String, // JSON string array like "[\"0.52\", \"0.48\"]"
    volume: String,
    active: bool,
    #[serde(rename = "endDateIso")]
    end_date_iso: Option<String>,
    #[serde(rename = "enableOrderBook")]
    enable_order_book: bool,
    #[serde(rename = "acceptingOrders")]
    accepting_orders: bool,
}

pub struct MarketDiscovery {
    client: Client,
}

impl MarketDiscovery {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Fetch active markets from Polymarket
    pub async fn fetch_active_markets(&self) -> Result<Vec<Market>> {
        info!("Fetching active markets from Polymarket...");

        let url = format!("{}/markets", GAMMA_API_URL);

        let response = self
            .client
            .get(&url)
            .query(&[("active", "true"), ("closed", "false")])
            .send()
            .await
            .context("Failed to fetch markets from Polymarket API")?;

        if !response.status().is_success() {
            anyhow::bail!("API returned error status: {}", response.status());
        }

        // API returns a direct array, not wrapped in an object
        let api_markets: Vec<ApiMarket> = response
            .json()
            .await
            .context("Failed to parse markets response")?;

        let mut markets: Vec<Market> = api_markets
            .into_iter()
            .filter_map(|api_market| {
                // Parse JSON string arrays
                let token_ids: Vec<String> = serde_json::from_str(&api_market.clob_token_ids).ok()?;
                let outcomes: Vec<String> = serde_json::from_str(&api_market.outcomes).ok()?;
                let prices: Vec<String> = serde_json::from_str(&api_market.outcome_prices).ok()?;

                // Only include markets with exactly 2 tokens (Yes/No)
                if token_ids.len() != 2 || outcomes.len() != 2 || prices.len() != 2 {
                    return None;
                }

                // Only include markets with order book enabled
                if !api_market.enable_order_book || !api_market.accepting_orders {
                    return None;
                }

                // Parse volume
                let volume = api_market.volume.parse().ok()?;

                // Create token objects
                let tokens = token_ids
                    .into_iter()
                    .zip(outcomes)
                    .zip(prices)
                    .map(|((token_id, outcome), price)| crate::types::Token {
                        token_id,
                        outcome,
                        price: price.parse().ok(),
                    })
                    .collect();

                Some(Market {
                    condition_id: api_market.condition_id,
                    question: api_market.question,
                    tokens,
                    volume,
                    active: api_market.active,
                    end_date_iso: api_market.end_date_iso,
                })
            })
            .collect();

        // Sort by volume (descending)
        markets.sort_by(|a, b| b.volume.cmp(&a.volume));

        info!("Fetched {} active markets", markets.len());

        Ok(markets)
    }

    /// Get top N markets by volume
    pub async fn get_top_markets(&self, limit: usize) -> Result<Vec<Market>> {
        let mut markets = self.fetch_active_markets().await?;
        markets.truncate(limit);

        info!(
            "Selected top {} markets by volume",
            markets.len()
        );

        for (i, market) in markets.iter().enumerate() {
            info!(
                "  {}. {} (volume: ${}, id: {})",
                i + 1,
                market.question,
                market.volume,
                market.condition_id
            );
        }

        Ok(markets)
    }

    /// Get filtered markets based on volume range and count limits
    pub async fn get_filtered_markets(
        &self,
        min_volume: Option<f64>,
        max_volume: Option<f64>,
        max_count: Option<usize>,
    ) -> Result<Vec<Market>> {
        let mut markets = self.fetch_active_markets().await?;

        // Apply volume filters
        if let Some(min) = min_volume {
            let min_decimal = Decimal::from_f64_retain(min)
                .context("Failed to convert MIN_MARKET_VOLUME to Decimal")?;
            markets.retain(|m| m.volume >= min_decimal);
            info!("Applied MIN_MARKET_VOLUME filter: ${}", min);
        }

        if let Some(max) = max_volume {
            let max_decimal = Decimal::from_f64_retain(max)
                .context("Failed to convert MAX_MARKET_VOLUME to Decimal")?;
            markets.retain(|m| m.volume <= max_decimal);
            info!("Applied MAX_MARKET_VOLUME filter: ${}", max);
        }

        // Apply count limit
        if let Some(limit) = max_count {
            markets.truncate(limit);
            info!("Applied MAX_MARKETS limit: {}", limit);
        }

        info!(
            "Selected {} markets (volume range: ${} - ${})",
            markets.len(),
            markets.last().map(|m| m.volume).unwrap_or(Decimal::ZERO),
            markets.first().map(|m| m.volume).unwrap_or(Decimal::ZERO)
        );

        for (i, market) in markets.iter().enumerate() {
            info!(
                "  {}. {} (volume: ${}, id: {})",
                i + 1,
                market.question,
                market.volume,
                market.condition_id
            );
        }

        Ok(markets)
    }
}
