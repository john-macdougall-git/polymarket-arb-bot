use crate::config::Config;
use crate::types::{Market, Opportunity, OrderBook};
use chrono::Utc;
use rust_decimal::Decimal;
use std::cmp::min;
use tracing::info;

pub struct ArbitrageDetector {
    config: Config,
}

impl ArbitrageDetector {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Check for arbitrage opportunity between Yes and No order books
    pub fn check_opportunity(
        &self,
        market: &Market,
        yes_book: &OrderBook,
        no_book: &OrderBook,
    ) -> Option<Opportunity> {
        // Get best ask prices
        let yes_ask = yes_book.get_best_ask()?;
        let no_ask = no_book.get_best_ask()?;

        // Calculate total cost
        let total_cost = yes_ask.price + no_ask.price;

        // Calculate profit per unit
        let profit_per_unit = Decimal::from(1) - total_cost;

        // Check if profit exceeds threshold
        if profit_per_unit <= self.config.min_profit_threshold {
            return None;
        }

        // Calculate profit percentage
        let profit_percentage = (profit_per_unit / total_cost) * Decimal::from(100);

        // Calculate max size we can trade
        let max_size = min(yes_ask.size, no_ask.size);
        let max_size = min(max_size, self.config.max_position_size);

        // Calculate potential profit
        let potential_profit = profit_per_unit * max_size;

        // Find token IDs
        let yes_token = market.tokens.iter()
            .find(|t| t.outcome.to_lowercase() == "yes")?;
        let no_token = market.tokens.iter()
            .find(|t| t.outcome.to_lowercase() == "no")?;

        let opportunity = Opportunity {
            market_id: market.condition_id.clone(),
            market_question: market.question.clone(),
            yes_token_id: yes_token.token_id.clone(),
            no_token_id: no_token.token_id.clone(),
            yes_price: yes_ask.price,
            no_price: no_ask.price,
            total_cost,
            profit_per_unit,
            profit_percentage,
            max_size,
            potential_profit,
            timestamp: Utc::now(),
        };

        info!(
            "🎯 ARBITRAGE OPPORTUNITY DETECTED!\n\
             Market: {}\n\
             Yes Price: ${} | No Price: ${}\n\
             Total Cost: ${} | Profit/Unit: ${} ({:.2}%)\n\
             Max Size: {} | Potential Profit: ${}",
            opportunity.market_question,
            opportunity.yes_price,
            opportunity.no_price,
            opportunity.total_cost,
            opportunity.profit_per_unit,
            opportunity.profit_percentage,
            opportunity.max_size,
            opportunity.potential_profit
        );

        Some(opportunity)
    }
}
