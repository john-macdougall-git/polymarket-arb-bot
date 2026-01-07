# Polymarket Arbitrage Bot

A high-frequency trading bot for detecting and (eventually) executing "risk-free" arbitrage opportunities on Polymarket's Central Limit Order Book (CLOB).

## Overview

This bot identifies synthetic long arbitrage opportunities where:
```
Ask(Yes) + Ask(No) < $1.00
```

Since Yes + No shares always redeem for exactly $1.00 when a market resolves, buying both at a combined price below $1.00 creates a risk-free profit.

## Current Status: Dry-Run Mode

The bot currently operates in **dry-run mode**, which means it:
- ✅ Connects to Polymarket's WebSocket API in real-time
- ✅ Maintains live order books for top markets
- ✅ Detects arbitrage opportunities
- ✅ Logs opportunities to console and CSV
- ❌ Does NOT execute any trades

## Features

- **Real-time WebSocket Integration**: Subscribes to Polymarket CLOB WebSocket for millisecond-level order book updates
- **Concurrent Order Book Management**: Uses DashMap for thread-safe, high-performance order book tracking
- **Decimal Precision**: All financial calculations use `rust_decimal` to avoid floating-point errors
- **Automatic Market Discovery**: Fetches and monitors the top 10 markets by volume
- **CSV Logging**: Records all detected opportunities with timestamp, prices, and profit calculations
- **Graceful Error Handling**: Automatically reconnects WebSocket connections with exponential backoff

## Prerequisites

- Rust 1.70+ (2021 edition)
- Internet connection for Polymarket API access

## Installation

1. Clone the repository:
```bash
cd arbitrage-bot
```

2. Copy the example environment file:
```bash
cp .env.example .env
```

3. (Optional) Edit `.env` to adjust parameters:
```bash
MIN_PROFIT_THRESHOLD=0.01
MAX_POSITION_SIZE=100.0
LOG_LEVEL=info
CSV_OUTPUT_PATH=./opportunities.csv
```

## Usage

### Build the project:
```bash
cargo build --release
```

### Run in dry-run mode:
```bash
cargo run --release
```

### Run with debug logging:
```bash
RUST_LOG=arbitrage_bot=debug cargo run
```

## Handling sessions with tmux
```bash
# Detach while running
Ctrl + B, then D

# Re-attach while running
tmux attach
```

## Output

The bot will:
1. Fetch the top 10 active markets by volume
2. Establish WebSocket connections for all Yes/No token pairs
3. Print detected opportunities to the console:
   ```
   🎯 ARBITRAGE OPPORTUNITY DETECTED!
   Market: Will Bitcoin reach $100k by March?
   Yes Price: $0.48 | No Price: $0.51
   Total Cost: $0.99 | Profit/Unit: $0.01 (1.01%)
   Max Size: 50 | Potential Profit: $0.50
   ```
4. Log all opportunities to `opportunities.csv`

## Configuration

Environment variables (see `.env.example`):

| Variable | Description | Default |
|----------|-------------|---------|
| `MIN_PROFIT_THRESHOLD` | Minimum profit per unit to trigger alert | `0.01` ($0.01) |
| `MAX_POSITION_SIZE` | Maximum size to consider per trade | `100.0` |
| `LOG_LEVEL` | Logging verbosity (error, warn, info, debug) | `info` |
| `CSV_OUTPUT_PATH` | Path to CSV output file | `./opportunities.csv` |

## Architecture

```
src/
├── main.rs              # Event loop and orchestration
├── config.rs            # Configuration management
├── types.rs             # Shared data types
├── markets/
│   └── discovery.rs     # Market discovery via REST API
├── websocket/
│   ├── client.rs        # WebSocket connection handler
│   └── messages.rs      # Message parsing
├── orderbook/
│   ├── book.rs          # Order book manager (DashMap)
│   └── types.rs         # Order book types
└── detector/
    └── arbitrage.rs     # Arbitrage detection logic
```

### Data Flow

1. **Market Discovery**: Fetches top markets via Polymarket REST API
2. **WebSocket Connections**: Spawns WebSocket clients for each token
3. **Order Book Updates**: Messages routed to `OrderBookManager` (thread-safe DashMap)
4. **Arbitrage Detection**: On every update, checks all markets for opportunities
5. **Logging**: Opportunities logged to console and CSV

## Future Enhancements

- [ ] Live trading mode with FOK (Fill-Or-Kill) orders
- [ ] L2 authentication for faster order submission
- [ ] Leg risk mitigation (emergency liquidation)
- [ ] Settlement sweeper (automatic redemption)
- [ ] Fee calculation for crypto 15-min markets
- [ ] Rate limiting with token bucket algorithm
- [ ] Redis-backed order book for multi-instance deployment
- [ ] Performance metrics dashboard

## Risk Disclosure

This bot is for educational purposes. When implementing live trading:
- **Leg Risk**: Partial fills can leave you exposed
- **Race Conditions**: Other bots may execute faster
- **Fee Changes**: Polymarket may adjust fee structures
- **Market Risk**: Always possible for extraordinary events

Always test thoroughly and understand the risks before trading with real capital.

## License

MIT
