use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
const NUM_PINGS: usize = 100;
const PING_INTERVAL_MS: u64 = 500; // 500ms between pings

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "latency_test=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🔍 Polymarket WebSocket Latency Test");
    info!("Target: {}", WS_URL);
    info!("Sending {} pings with {}ms interval...\n", NUM_PINGS, PING_INTERVAL_MS);

    // Connect to WebSocket
    info!("Connecting to WebSocket...");
    let (ws_stream, response) = connect_async(WS_URL)
        .await
        .context("Failed to connect to WebSocket")?;

    info!("Connected! HTTP status: {}", response.status());

    let (mut write, mut read) = ws_stream.split();

    let mut latencies: Vec<Duration> = Vec::with_capacity(NUM_PINGS);
    let mut successful_pings = 0;
    let mut failed_pings = 0;

    for i in 0..NUM_PINGS {
        let ping_data = format!("ping_{}", i).into_bytes();
        let start = Instant::now();

        // Send ping
        if let Err(e) = write.send(Message::Ping(ping_data.clone().into())).await {
            error!("Failed to send ping {}: {:?}", i, e);
            failed_pings += 1;
            continue;
        }

        // Wait for pong (with timeout)
        let timeout = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Pong(data)) if data == ping_data => {
                        return Ok(());
                    }
                    Ok(Message::Pong(_)) => {
                        // Different pong, keep waiting
                        continue;
                    }
                    Ok(Message::Ping(data)) => {
                        // Server sent ping, respond with pong
                        if let Err(e) = write.send(Message::Pong(data)).await {
                            error!("Failed to send pong: {:?}", e);
                        }
                        continue;
                    }
                    Ok(Message::Close(_)) => {
                        return Err(anyhow::anyhow!("Connection closed"));
                    }
                    Ok(_) => {
                        // Ignore other message types (text, binary, etc.)
                        continue;
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("WebSocket error: {:?}", e));
                    }
                }
            }
            Err(anyhow::anyhow!("Stream ended"))
        })
        .await;

        match timeout {
            Ok(Ok(())) => {
                let latency = start.elapsed();
                latencies.push(latency);
                successful_pings += 1;

                if (i + 1) % 10 == 0 {
                    info!("Progress: {}/{} pings completed", i + 1, NUM_PINGS);
                }
            }
            Ok(Err(e)) => {
                error!("Ping {} failed: {:?}", i, e);
                failed_pings += 1;
            }
            Err(_) => {
                error!("Ping {} timed out (>5s)", i);
                failed_pings += 1;
            }
        }

        // Wait before next ping
        if i < NUM_PINGS - 1 {
            tokio::time::sleep(Duration::from_millis(PING_INTERVAL_MS)).await;
        }
    }

    // Calculate statistics
    if latencies.is_empty() {
        error!("\n❌ No successful pings! Cannot calculate statistics.");
        return Ok(());
    }

    latencies.sort();

    let total: Duration = latencies.iter().sum();
    let avg = total / latencies.len() as u32;
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let median = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    // Print results
    info!("\n{}", "=".repeat(60));
    info!("📊 LATENCY TEST RESULTS");
    info!("{}\n", "=".repeat(60));

    info!("Success Rate: {}/{} ({:.1}%)",
          successful_pings,
          NUM_PINGS,
          (successful_pings as f64 / NUM_PINGS as f64) * 100.0);
    info!("Failed Pings: {}\n", failed_pings);

    info!("Latency Statistics (Round-Trip Time):");
    info!("  Minimum:    {:>8.2}ms", min.as_secs_f64() * 1000.0);
    info!("  Maximum:    {:>8.2}ms", max.as_secs_f64() * 1000.0);
    info!("  Average:    {:>8.2}ms", avg.as_secs_f64() * 1000.0);
    info!("  Median:     {:>8.2}ms", median.as_secs_f64() * 1000.0);
    info!("  P95:        {:>8.2}ms", p95.as_secs_f64() * 1000.0);
    info!("  P99:        {:>8.2}ms", p99.as_secs_f64() * 1000.0);

    info!("\n{}", "=".repeat(60));

    // Provide interpretation
    let avg_ms = avg.as_secs_f64() * 1000.0;
    info!("\n💡 Interpretation:");
    if avg_ms < 10.0 {
        info!("  ✅ EXCELLENT - Sub-10ms latency ideal for HFT");
    } else if avg_ms < 50.0 {
        info!("  ✅ GOOD - Suitable for HFT with <50ms latency");
    } else if avg_ms < 100.0 {
        info!("  ⚠️  ACCEPTABLE - 50-100ms may lose edge in competitive markets");
    } else {
        info!("  ❌ HIGH - >100ms latency too slow for HFT arbitrage");
    }

    Ok(())
}
