use crate::types::{SubscribeMessage, WsMessage};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message,
    MaybeTlsStream,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WebSocketClient {
    token_id: String,
    tx: mpsc::UnboundedSender<WsMessage>,
}

impl WebSocketClient {
    /// Create a new WebSocket client for a specific token
    pub fn new(
        token_id: String,
        tx: mpsc::UnboundedSender<WsMessage>,
    ) -> Self {
        Self { token_id, tx }
    }

    /// Connect to WebSocket and subscribe to market updates
    pub async fn run(&self) -> Result<()> {
        loop {
            info!("Connecting to WebSocket for token {}", self.token_id);

            match self.connect_and_listen().await {
                Ok(_) => {
                    info!("WebSocket connection closed normally for token {}", self.token_id);
                }
                Err(e) => {
                    error!(
                        "WebSocket error for token {}: {:?}",
                        self.token_id, e
                    );
                }
            }

            // Exponential backoff before reconnecting
            warn!(
                "Reconnecting to WebSocket for token {} in 5 seconds...",
                self.token_id
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    /// Connect to WebSocket and listen for messages
    async fn connect_and_listen(&self) -> Result<()> {
        debug!("Attempting WebSocket connection to: {}", WS_URL);

        // Connect to WebSocket
        let (ws_stream, response) = connect_async(WS_URL)
            .await
            .context("Failed to connect to WebSocket")?;

        debug!("WebSocket connected, HTTP status: {}", response.status());

        info!("WebSocket connected for token {}", self.token_id);

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to market updates
        let subscribe_msg = SubscribeMessage::new_market_subscription(self.token_id.clone());
        let subscribe_json = subscribe_msg.to_json()?;

        write
            .send(Message::Text(subscribe_json.into()))
            .await
            .context("Failed to send subscription message")?;

        info!("Subscribed to market updates for token {}", self.token_id);

        // Listen for messages
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    let text_str = text.to_string();
                    let preview = if text_str.len() > 200 {
                        format!("{}... ({} bytes)", &text_str[..200], text_str.len())
                    } else {
                        text_str.clone()
                    };
                    info!("📨 Received WebSocket message (token {}): {}",
                          &self.token_id[..20], preview);

                    match WsMessage::from_json(&text_str) {
                        Ok(ws_messages) => {
                            // Polymarket sends arrays of messages
                            for ws_msg in ws_messages {
                                if let Err(e) = self.tx.send(ws_msg) {
                                    error!("Failed to send message to channel: {:?}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse WebSocket message: {:?}", e);
                            debug!("Raw message: {}", text_str);
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping, sending pong");
                    write.send(Message::Pong(data)).await?;
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed by server for token {}", self.token_id);
                    break;
                }
                Ok(Message::Binary(_)) => {
                    warn!("Received unexpected binary message");
                }
                Ok(Message::Frame(_)) => {
                    // Ignore raw frames
                }
                Err(e) => {
                    error!("WebSocket error: {:?}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
