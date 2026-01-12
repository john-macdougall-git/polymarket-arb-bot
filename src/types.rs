use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ============================================================================
// INPUT/STATE STRUCTURES (WebSocket & Order Book)
// ============================================================================

/// WebSocket subscription request
#[derive(Debug, Clone, Serialize)]
pub struct SubscribeMessage {
    pub assets_ids: Vec<String>, // Array of token IDs
    #[serde(rename = "type")]
    pub msg_type: String,        // "market"
}

/// WebSocket message types from Polymarket CLOB
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event_type")]
pub enum WsMessage {
    #[serde(rename = "book")]
    Book {
        market: String,
        asset_id: String,
        #[serde(default)]
        bids: Vec<PriceLevel>,
        #[serde(default)]
        asks: Vec<PriceLevel>,
        timestamp: Option<String>,
        hash: Option<String>,
        last_trade_price: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    #[serde(rename = "price_change")]
    PriceChange {
        market: String,
        price_changes: Vec<PriceChangeItem>,
        timestamp: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    #[serde(rename = "last_trade_price")]
    LastTradePrice {
        market: String,
        asset_id: String,
        price: String,
        timestamp: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}

/// Price level in order book (price and size)
#[derive(Debug, Clone, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

/// Individual price change item in a price_change message
#[derive(Debug, Clone, Deserialize)]
pub struct PriceChangeItem {
    pub asset_id: String,
    pub price: String,
    pub size: String,
    #[serde(rename = "side")]
    pub side_str: String, // "BUY" or "SELL" (uppercase)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_bid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_ask: Option<String>,
}

impl PriceChangeItem {
    /// Convert side string to OrderSide enum
    pub fn side(&self) -> OrderSide {
        match self.side_str.to_uppercase().as_str() {
            "BUY" | "BID" => OrderSide::Bid,
            "SELL" | "ASK" => OrderSide::Ask,
            _ => OrderSide::Ask, // Default fallback
        }
    }
}

/// Order side (bid or ask)
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Bid,
    Ask,
}

/// Order book state for a single token
#[derive(Debug, Clone)]
pub struct OrderBook {
    pub token_id: String,
    /// Asks: price -> size (sorted ascending by price)
    pub asks: BTreeMap<Decimal, Decimal>,
    /// Bids: price -> size (sorted descending by price)
    pub bids: BTreeMap<Decimal, Decimal>,
    pub last_updated: DateTime<Utc>,
    /// Last processed sequence number (for gap detection)
    pub last_seq: Option<u64>,
    /// Flag indicating if the order book is out of sync
    pub out_of_sync: bool,
}

impl OrderBook {
    pub fn new(token_id: String) -> Self {
        Self {
            token_id,
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
            last_updated: Utc::now(),
            last_seq: None,
            out_of_sync: false,
        }
    }

    /// Get the best (lowest) ask price and size
    pub fn get_best_ask(&self) -> Option<Order> {
        self.asks
            .iter()
            .next()
            .map(|(price, size)| Order {
                price: *price,
                size: *size,
            })
    }

    /// Get the best (highest) bid price and size
    pub fn get_best_bid(&self) -> Option<Order> {
        self.bids
            .iter()
            .next_back()
            .map(|(price, size)| Order {
                price: *price,
                size: *size,
            })
    }

    /// Validate sequence number and check for gaps
    /// Returns true if sequence is valid, false if there's a gap
    pub fn validate_sequence(&mut self, seq: Option<u64>) -> bool {
        if let Some(new_seq) = seq {
            if let Some(last_seq) = self.last_seq {
                // Check for sequence gap (missed packets)
                if new_seq != last_seq + 1 {
                    self.out_of_sync = true;
                    return false;
                }
            }
            self.last_seq = Some(new_seq);
        }
        true
    }

    /// Reset sync flag when receiving a full snapshot
    pub fn reset_sync(&mut self) {
        self.out_of_sync = false;
        self.last_seq = None;
    }

    /// Update order book with new price level
    pub fn update(&mut self, side: OrderSide, price: Decimal, size: Decimal) {
        let book = match side {
            OrderSide::Ask => &mut self.asks,
            OrderSide::Bid => &mut self.bids,
        };

        if size.is_zero() {
            book.remove(&price);
        } else {
            book.insert(price, size);
        }
        self.last_updated = Utc::now();
    }

    /// Replace entire side of book (used for snapshots)
    pub fn replace_side(&mut self, side: OrderSide, levels: Vec<(Decimal, Decimal)>) {
        let book = match side {
            OrderSide::Ask => &mut self.asks,
            OrderSide::Bid => &mut self.bids,
        };

        book.clear();
        for (price, size) in levels {
            if !size.is_zero() {
                book.insert(price, size);
            }
        }
        self.last_updated = Utc::now();
        // Snapshots reset the sync state
        self.reset_sync();
    }
}

/// A single order in the order book
#[derive(Debug, Clone)]
pub struct Order {
    pub price: Decimal,
    pub size: Decimal,
}

// ============================================================================
// OUTPUT/LOGGING STRUCTURES (Markets & Opportunities)
// ============================================================================

/// Represents a market on Polymarket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub condition_id: String,
    pub question: String,
    pub tokens: Vec<Token>,
    pub volume: Decimal,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date_iso: Option<String>,
}

/// Represents a token (Yes or No outcome) in a market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub token_id: String,
    pub outcome: String, // "Yes" or "No"
    pub price: Option<Decimal>,
}

/// Represents an arbitrage opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub market_id: String,
    pub market_question: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub yes_price: Decimal,
    pub no_price: Decimal,
    pub total_cost: Decimal,
    pub profit_per_unit: Decimal,
    pub profit_percentage: Decimal,
    pub max_size: Decimal,
    pub potential_profit: Decimal,
    pub timestamp: DateTime<Utc>,
}

impl Opportunity {
    /// Create CSV headers for logging
    pub fn csv_headers() -> Vec<&'static str> {
        vec![
            "timestamp",
            "market_id",
            "market_question",
            "yes_token_id",
            "no_token_id",
            "yes_price",
            "no_price",
            "total_cost",
            "profit_per_unit",
            "profit_percentage",
            "max_size",
            "potential_profit",
        ]
    }

    /// Convert to CSV row
    pub fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.timestamp.to_rfc3339(),
            self.market_id.clone(),
            self.market_question.clone(),
            self.yes_token_id.clone(),
            self.no_token_id.clone(),
            self.yes_price.to_string(),
            self.no_price.to_string(),
            self.total_cost.to_string(),
            self.profit_per_unit.to_string(),
            self.profit_percentage.to_string(),
            self.max_size.to_string(),
            self.potential_profit.to_string(),
        ]
    }
}
