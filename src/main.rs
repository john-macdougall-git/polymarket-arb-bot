mod config;
mod detector;
mod markets;
mod orderbook;
mod types;
mod websocket;

use anyhow::Result;
use config::Config;
use detector::ArbitrageDetector;
use markets::MarketDiscovery;
use orderbook::OrderBookManager;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use types::{Market, Opportunity, WsMessage};
use websocket::WebSocketClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "arbitrage_bot=debug,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🚀 Starting Polymarket Arbitrage Bot (Dry-Run Mode)");

    // Load configuration
    let config = Config::from_env()?;
    info!(
        "Configuration loaded: min_profit_threshold={}, max_position_size={}",
        config.min_profit_threshold, config.max_position_size
    );

    // Initialize CSV logger
    let csv_logger = CsvLogger::new(&config.csv_output_path)?;
    info!("CSV logger initialized: {}", config.csv_output_path);

    // Discover top markets
    let discovery = MarketDiscovery::new();
    let markets = discovery.get_top_markets(10).await?;

    if markets.is_empty() {
        error!("No active markets found!");
        return Ok(());
    }

    // Create order book manager
    let order_book_manager = OrderBookManager::new();

    // Create arbitrage detector
    let detector = ArbitrageDetector::new(config);

    // Create a channel for WebSocket messages
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<WsMessage>();

    // Spawn WebSocket clients for each token
    for market in &markets {
        for token in &market.tokens {
            let client = WebSocketClient::new(token.token_id.clone(), ws_tx.clone());
            let token_id = token.token_id.clone();

            tokio::spawn(async move {
                if let Err(e) = client.run().await {
                    error!("WebSocket client error for token {}: {:?}", token_id, e);
                }
            });
        }
    }

    // Create a map of condition_id -> Market for quick lookup
    let market_map: HashMap<String, Market> = markets
        .into_iter()
        .map(|m| (m.condition_id.clone(), m))
        .collect();

    info!("All WebSocket connections established. Starting main event loop...");

    // Main event loop
    while let Some(msg) = ws_rx.recv().await {
        // Process the message and update order books
        if let Err(e) = order_book_manager.process_message(msg) {
            error!("Error processing WebSocket message: {:?}", e);
            continue;
        }

        // Check for arbitrage opportunities across all markets
        for market in market_map.values() {
            // Get Yes and No tokens
            let yes_token = market
                .tokens
                .iter()
                .find(|t| t.outcome.to_lowercase() == "yes");
            let no_token = market
                .tokens
                .iter()
                .find(|t| t.outcome.to_lowercase() == "no");

            if let (Some(yes_token), Some(no_token)) = (yes_token, no_token) {
                // Get order books
                if let Some((yes_book, no_book)) =
                    order_book_manager.get_pair(&yes_token.token_id, &no_token.token_id)
                {
                    // Check for arbitrage
                    if let Some(opportunity) = detector.check_opportunity(market, &yes_book, &no_book) {
                        // Log opportunity to CSV
                        if let Err(e) = csv_logger.log_opportunity(&opportunity) {
                            error!("Failed to log opportunity to CSV: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    info!("Event loop terminated");
    Ok(())
}

/// CSV logger for arbitrage opportunities
struct CsvLogger {
    writer: std::sync::Mutex<csv::Writer<std::fs::File>>,
}

impl CsvLogger {
    fn new(path: &str) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let mut writer = csv::Writer::from_writer(file);

        // Write headers if file is empty
        if std::fs::metadata(path)?.len() == 0 {
            writer.write_record(Opportunity::csv_headers())?;
            writer.flush()?;
        }

        Ok(Self {
            writer: std::sync::Mutex::new(writer),
        })
    }

    fn log_opportunity(&self, opportunity: &Opportunity) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_record(opportunity.to_csv_row())?;
        writer.flush()?;
        Ok(())
    }
}
