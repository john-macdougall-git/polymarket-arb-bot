use crate::types::{OrderBook, OrderSide, PriceLevel, WsMessage};
use anyhow::Result;
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, warn};

/// Thread-safe order book manager using DashMap
pub struct OrderBookManager {
    /// Map of token_id -> OrderBook
    books: Arc<DashMap<String, OrderBook>>,
}

impl OrderBookManager {
    pub fn new() -> Self {
        Self {
            books: Arc::new(DashMap::new()),
        }
    }

    /// Get a reference to an order book
    pub fn get_book(&self, token_id: &str) -> Option<OrderBook> {
        self.books.get(token_id).map(|entry| entry.clone())
    }

    /// Initialize an order book for a token
    pub fn init_book(&self, token_id: String) {
        if !self.books.contains_key(&token_id) {
            self.books.insert(token_id.clone(), OrderBook::new(token_id));
            debug!("Initialized order book for token");
        }
    }

    /// Process a WebSocket message and update the order book
    pub fn process_message(&self, msg: WsMessage) -> Result<()> {
        match msg {
            WsMessage::Book {
                market,
                asset_id: _,
                bids,
                asks,
                ..
            } => {
                self.handle_book_snapshot(&market, bids, asks)?;
            }
            WsMessage::PriceChange {
                market: _,
                price_changes,
                ..
            } => {
                // Process each price change in the array
                for change in price_changes {
                    self.handle_price_change(
                        &change.asset_id,
                        change.side(),
                        &change.price,
                        &change.size,
                    )?;
                }
            }
            WsMessage::LastTradePrice { .. } => {
                // We don't need to track last trade price for arbitrage detection
            }
            WsMessage::Error { message } => {
                warn!("WebSocket error message: {}", message);
            }
        }

        Ok(())
    }

    /// Handle a full order book snapshot
    fn handle_book_snapshot(
        &self,
        token_id: &str,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
    ) -> Result<()> {
        self.init_book(token_id.to_string());

        let mut book_entry = self.books.get_mut(token_id).unwrap();

        // Parse and replace bids
        let bid_levels: Vec<(Decimal, Decimal)> = bids
            .iter()
            .filter_map(|level| level.parse().ok())
            .collect();

        book_entry.replace_side(OrderSide::Bid, bid_levels);

        // Parse and replace asks
        let ask_levels: Vec<(Decimal, Decimal)> = asks
            .iter()
            .filter_map(|level| level.parse().ok())
            .collect();

        book_entry.replace_side(OrderSide::Ask, ask_levels);

        debug!(
            "Updated order book snapshot for token {} ({} bids, {} asks)",
            token_id,
            book_entry.bids.len(),
            book_entry.asks.len()
        );

        Ok(())
    }

    /// Handle a price change update
    fn handle_price_change(
        &self,
        token_id: &str,
        side: OrderSide,
        price_str: &str,
        size_str: &str,
    ) -> Result<()> {
        self.init_book(token_id.to_string());

        let price = Decimal::from_str(price_str)?;
        let size = Decimal::from_str(size_str)?;

        let mut book_entry = self.books.get_mut(token_id).unwrap();
        book_entry.update(side, price, size);

        debug!(
            "Updated order book for token {} (side: {:?}, price: {}, size: {})",
            token_id, side, price, size
        );

        Ok(())
    }

    /// Get a pair of order books for Yes and No tokens
    pub fn get_pair(
        &self,
        yes_token_id: &str,
        no_token_id: &str,
    ) -> Option<(OrderBook, OrderBook)> {
        let yes_book = self.get_book(yes_token_id)?;
        let no_book = self.get_book(no_token_id)?;
        Some((yes_book, no_book))
    }
}

impl Clone for OrderBookManager {
    fn clone(&self) -> Self {
        Self {
            books: Arc::clone(&self.books),
        }
    }
}
