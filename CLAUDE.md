# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a high-frequency trading bot written in Rust that detects arbitrage opportunities on Polymarket's Central Limit Order Book (CLOB). Currently operates in dry-run mode (detection only, no execution).

**Target Opportunity**: Synthetic Long Arbitrage where `Ask(Yes) + Ask(No) < $1.00`

## Development Commands

### Build
```bash
cargo build --release
```

### Run
```bash
cargo run --release
```

### Run with debug logging
```bash
RUST_LOG=arbitrage_bot=debug cargo run
```

### Test
```bash
cargo test
```

### Check without building
```bash
cargo check
```

### Format code
```bash
cargo fmt
```

### Lint
```bash
cargo clippy
```

### Test WebSocket Latency (Server Location Selection)
```bash
cargo run --release --bin latency_test
```

This tool measures round-trip ping/pong latency to Polymarket's matching engine. Run this from different server locations (AWS us-east-1, eu-west-1, etc.) to determine optimal deployment location for HFT. Results include min/max/avg/p95/p99 statistics over 100 pings.

## Architecture

### High-Level Design

The bot is an **event-driven system** built around WebSocket streams:

1. **Market Discovery** (REST API): Fetches top N markets by volume from Polymarket Gamma API
2. **WebSocket Layer**: Maintains a single persistent connection to Polymarket CLOB WebSocket, subscribing to all token IDs in one subscription message (efficient, avoids rate limiting)
3. **Order Book Manager**: Thread-safe (DashMap) in-memory order books for all monitored tokens
4. **Arbitrage Detector**: Triggered on every order book update to check for opportunities
5. **CSV Logger**: Records all detected opportunities with full details

### Module Breakdown

- **src/main.rs**: Orchestration layer, event loop, CSV logging
- **src/config.rs**: Environment variable loading and configuration management
- **src/types.rs**: All shared types including:
  - WebSocket message types (`WsMessage`, `SubscribeMessage`, `PriceLevel`)
  - Order book types (`OrderBook`, `Order`, `OrderSide`)
  - Domain types (`Market`, `Token`, `Opportunity`)
- **src/markets/discovery.rs**: REST API client for fetching active markets from Polymarket
  - **Pagination**: Automatically fetches ALL pages from Gamma API (100 markets per page)
  - **Rate limiting**: 100ms delay between API requests
  - **Filtering**: Applies volume filters after fetching all markets
- **src/websocket/client.rs**: WebSocket client with auto-reconnect logic
- **src/websocket/messages.rs**: WebSocket message parsing and serialization
- **src/orderbook/book.rs**: `OrderBookManager` using DashMap for concurrent access
- **src/detector/arbitrage.rs**: Core arbitrage detection logic
- **src/bin/latency_test.rs**: Standalone tool for measuring WebSocket ping/pong latency to Polymarket's matching engine (used for server location selection)

### Key Design Decisions

#### 1. DashMap for Order Books
**Why**: Standard `HashMap` requires `RwLock` or `Mutex`. DashMap provides lock-free reads and fine-grained write locking (per-shard), critical for HFT performance.

**Usage**: Each token has its own `OrderBook` stored in `DashMap<String, OrderBook>`.

#### 2. Rust Decimal for Financial Math
**Why**: `f64` has floating-point precision issues. With arbitrage margins of 0.5-2%, even tiny errors are unacceptable.

**Example**:
```rust
let total_cost = yes_price + no_price; // Both are Decimal
let profit = Decimal::from(1) - total_cost;
```

#### 3. Single WebSocket Connection for All Tokens
**Why**: Polymarket's `SubscribeMessage` accepts an array of `assets_ids`. Using one connection for all tokens (instead of N connections) prevents rate limiting and reduces overhead.

**How**: `WebSocketClient` accepts `Vec<String>` of token IDs and subscribes to all in one message. Messages are sent to a shared `mpsc::unbounded_channel` that the main loop processes sequentially.

**Before**: 10 markets × 2 tokens = 20 TCP connections (inefficient, triggers abuse detection)
**After**: 10 markets × 2 tokens = 1 TCP connection (efficient, scalable)

#### 4. BTreeMap Inside OrderBook
**Why**: Order books need sorted price levels (best bid = highest price, best ask = lowest price). `BTreeMap` maintains sorted order automatically.

**Structure**:
```rust
pub struct OrderBook {
    pub asks: BTreeMap<Decimal, Decimal>, // price -> size
    pub bids: BTreeMap<Decimal, Decimal>,
}
```

#### 5. Event-Driven, Not Polling
**Why**: Polymarket has strict rate limits (500 req/sec). WebSocket provides push-based updates with millisecond latency.

**Flow**:
```
WebSocket Message → OrderBookManager.process_message() →
  Update BTreeMap → ArbitrageDetector.check_opportunity() →
    Log to CSV if profitable
```

### Critical Code Paths

#### Order Book Update Flow
```rust
// 1. WebSocket receives message
WsMessage::PriceChange { market, side, price, size, .. }

// 2. OrderBookManager updates the book
book.update(side, price, size) // Inserts/removes from BTreeMap

// 3. Main loop checks all markets
detector.check_opportunity(market, &yes_book, &no_book)

// 4. If profitable, log to CSV
csv_logger.log_opportunity(&opportunity)
```

#### Arbitrage Detection Logic
```rust
// From src/detector/arbitrage.rs
let yes_ask = yes_book.get_best_ask()?; // Lowest ask in BTreeMap
let no_ask = no_book.get_best_ask()?;

let total_cost = yes_ask.price + no_ask.price;
let profit_per_unit = Decimal::from(1) - total_cost;

if profit_per_unit > MIN_PROFIT_THRESHOLD {
    // Arbitrage opportunity!
}
```

### WebSocket Message Types

Polymarket CLOB sends several message types (see `src/types.rs`):

- **`subscribed`**: Confirmation of subscription
- **`book`**: Full order book snapshot (sent on first subscription)
- **`price_change`**: Incremental update (most common)
- **`last_trade_price`**: Ignored for arbitrage detection
- **`error`**: Error messages

### Configuration

All configuration is via environment variables (`.env` file):

**Trading Parameters:**
- `MIN_PROFIT_THRESHOLD`: Minimum profit per unit (default: 0.01 = 1 cent)
- `MAX_POSITION_SIZE`: Max size to consider (default: 100)

**Market Selection Filters (all optional):**
- `MIN_MARKET_VOLUME`: Minimum 24h volume to monitor (default: None = no minimum)
- `MAX_MARKET_VOLUME`: Maximum 24h volume to monitor (default: None = no maximum)
- `MAX_MARKETS`: Maximum number of markets to track (default: None = unlimited)

**Output & Logging:**
- `CSV_OUTPUT_PATH`: Where to log opportunities (default: `./opportunities.csv`)
- `RUST_LOG`: Log level (e.g., `arbitrage_bot=debug,info`)

**Market Selection Strategy:**

If all filters are unset, the bot monitors **all active markets** on Polymarket.

Common configurations:
- **Cast wide net (recommended for arbitrage)**: Leave all filters unset or set `MAX_MARKETS=500`
- **Target mid-tier markets**: Set `MIN_MARKET_VOLUME=1000` and `MAX_MARKET_VOLUME=500000`
- **Avoid whale markets**: Set `MAX_MARKET_VOLUME=100000`
- **Resource-limited server**: Set `MAX_MARKETS=50`

**Why volume filtering matters for HFT:**
High-volume markets (top 10-20 by volume) are dominated by professional market makers. Arbitrage spreads >1% rarely exist because:
- Multiple HFT bots compete for the same opportunities
- Market makers maintain tight spreads (<0.5%)
- Any mispricing is corrected in microseconds

Mid-tier and long-tail markets have:
- Less competition from professional traders
- Wider spreads due to lower liquidity
- More frequent arbitrage opportunities (1-5% spreads)
- Longer opportunity windows (seconds vs microseconds)

**How pagination ensures you reach mid-tier markets:**
The bot uses automatic pagination to fetch ALL active markets from Polymarket (not just the first page of 100 results). This means:
- Top 100 markets (pages 1-2) = whale-dominated, hyper-competitive
- Markets 100-500 (pages 2-5) = mid-tier, optimal for arbitrage
- Markets 500+ (pages 5+) = long-tail, wider spreads but less liquidity

Volume filters are applied AFTER fetching all pages, ensuring you can access any tier of markets regardless of where they appear in the API results.

### Adding New Features

#### To add live trading (future):
1. Enable `live-trading` feature in Cargo.toml (includes Alloy for Ethereum)
2. Add L2 authentication in `src/config.rs` (POLY_API_KEY, POLY_SIGNATURE)
3. Create `src/execution/` module with FOK order logic
4. Add leg risk mitigation (emergency sell if one order fails)

#### To monitor more/fewer markets:
Edit your `.env` file and adjust the market selection filters:
- Remove all filters to monitor all active markets
- Set `MAX_MARKETS=200` to limit resource usage
- Set `MAX_MARKET_VOLUME=100000` to filter out whale-dominated markets
- Set `MIN_MARKET_VOLUME=5000` to filter out illiquid markets

No code changes or recompilation required.

#### To add fee calculation:
Modify `src/detector/arbitrage.rs` to include taker fees in `total_cost`:
```rust
let fee = calculate_taker_fee(market); // Implement based on market type
let total_cost = yes_ask.price + no_ask.price + fee;
```

### Common Gotchas

1. **Startup Time**: The bot fetches ALL active markets from Polymarket using pagination (100 per page with 100ms delay). Expect 5-15 seconds startup time depending on how many markets are active. Watch the logs for "Fetching page (offset=X)" messages.
2. **WebSocket Disconnects**: The client auto-reconnects with 5-second delay. Check logs if you see gaps.
3. **Empty Order Books**: Markets with low liquidity may have empty asks/bids. The code uses `Option<>` to handle this.
4. **CSV Append Mode**: The CSV logger appends to existing files. Delete `opportunities.csv` to start fresh.
5. **Time Zones**: All timestamps are UTC (via `chrono::Utc::now()`).
6. **API Pagination**: The bot automatically fetches all pages from the Gamma API. If you set volume filters, filtering happens AFTER fetching all markets, so you'll always have access to mid-tier and long-tail markets.

### Dependencies

- **tokio**: Async runtime (with `parking_lot` for faster mutexes)
- **tokio-tungstenite**: WebSocket client (using rustls for pure-Rust SSL)
- **reqwest**: HTTP client for REST API (also using rustls)
- **dashmap**: Concurrent HashMap
- **rust_decimal**: Precise decimal arithmetic
- **serde/serde_json**: Serialization
- **tracing**: Structured logging
- **csv**: CSV writing
- **dotenvy**: `.env` file loading
- **anyhow/thiserror**: Error handling
- **chrono**: Timestamps
