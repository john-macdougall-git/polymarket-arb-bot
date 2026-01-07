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

## Architecture

### High-Level Design

The bot is an **event-driven system** built around WebSocket streams:

1. **Market Discovery** (REST API): Fetches top N markets by volume from Polymarket Gamma API
2. **WebSocket Layer**: Maintains persistent connections to Polymarket CLOB WebSocket for real-time order book updates
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
- **src/websocket/client.rs**: WebSocket client with auto-reconnect logic
- **src/websocket/messages.rs**: WebSocket message parsing and serialization
- **src/orderbook/book.rs**: `OrderBookManager` using DashMap for concurrent access
- **src/detector/arbitrage.rs**: Core arbitrage detection logic

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

#### 3. Separate WebSocket Task per Token
**Why**: Polymarket CLOB requires subscribing to individual token IDs. We spawn one task per token to parallelize connections.

**How**: Each `WebSocketClient` sends messages to a shared `mpsc::unbounded_channel`, which the main loop processes sequentially.

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

- `MIN_PROFIT_THRESHOLD`: Minimum profit per unit (default: 0.01 = 1 cent)
- `MAX_POSITION_SIZE`: Max size to consider (default: 100)
- `CSV_OUTPUT_PATH`: Where to log opportunities (default: `./opportunities.csv`)
- `RUST_LOG`: Log level (e.g., `arbitrage_bot=debug,info`)

### Adding New Features

#### To add live trading (future):
1. Enable `live-trading` feature in Cargo.toml (includes Alloy for Ethereum)
2. Add L2 authentication in `src/config.rs` (POLY_API_KEY, POLY_SIGNATURE)
3. Create `src/execution/` module with FOK order logic
4. Add leg risk mitigation (emergency sell if one order fails)

#### To add more markets:
Change `get_top_markets(10)` to higher number in `src/main.rs`.

#### To add fee calculation:
Modify `src/detector/arbitrage.rs` to include taker fees in `total_cost`:
```rust
let fee = calculate_taker_fee(market); // Implement based on market type
let total_cost = yes_ask.price + no_ask.price + fee;
```

### Common Gotchas

1. **WebSocket Disconnects**: The client auto-reconnects with 5-second delay. Check logs if you see gaps.
2. **Empty Order Books**: Markets with low liquidity may have empty asks/bids. The code uses `Option<>` to handle this.
3. **CSV Append Mode**: The CSV logger appends to existing files. Delete `opportunities.csv` to start fresh.
4. **Time Zones**: All timestamps are UTC (via `chrono::Utc::now()`).

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
