use crate::types::{PriceLevel, SubscribeMessage, WsMessage};
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::str::FromStr;

impl SubscribeMessage {
    /// Create a new market subscription message for multiple tokens
    pub fn new_market_subscription(token_ids: Vec<String>) -> Self {
        Self {
            assets_ids: token_ids,
            msg_type: "market".to_string(),
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize subscription message")
    }
}

impl PriceLevel {
    /// Parse price and size into Decimals
    pub fn parse(&self) -> Result<(Decimal, Decimal)> {
        let price = Decimal::from_str(&self.price)
            .context("Failed to parse price")?;
        let size = Decimal::from_str(&self.size)
            .context("Failed to parse size")?;
        Ok((price, size))
    }
}

impl WsMessage {
    /// Parse WebSocket messages from JSON text
    /// Handles both single messages and arrays of messages
    pub fn from_json(text: &str) -> Result<Vec<Self>> {
        // Try parsing as a single message first (most common case)
        if let Ok(single_msg) = serde_json::from_str::<WsMessage>(text) {
            return Ok(vec![single_msg]);
        }

        // Fall back to array format (for backwards compatibility)
        serde_json::from_str::<Vec<WsMessage>>(text)
            .context("Failed to parse WebSocket message as single object or array")
    }
}
