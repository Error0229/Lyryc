use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMessage {
    pub message_type: String,
    pub data: serde_json::Value,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackUpdate {
    pub title: String,
    pub artist: String,
    pub thumbnail: Option<String>,
    pub source: String,
    pub url: String,
    pub timestamp: u64,
    pub is_playing: bool,
    #[serde(rename = "currentTime")]
    pub current_time: Option<f64>,
    pub duration: Option<f64>,
}

use tokio::sync::mpsc;

type ClientConnections = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Message>>>>;

pub struct WebSocketServer {
    port: u16,
    pub clients: ClientConnections,
    track_callback: Option<Arc<dyn Fn(TrackUpdate) + Send + Sync>>,
}

impl WebSocketServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            clients: Arc::new(Mutex::new(HashMap::new())),
            track_callback: None,
        }
    }

    pub fn set_track_callback<F>(&mut self, callback: F)
    where
        F: Fn(TrackUpdate) + Send + Sync + 'static,
    {
        self.track_callback = Some(Arc::new(callback));
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("127.0.0.1:{}", self.port);
        
        match TcpListener::bind(&addr).await {
            Ok(listener) => {
                info!("WebSocket server listening on: {}", addr);
                
                let clients = Arc::clone(&self.clients);
                let track_callback = self.track_callback.clone();

                while let Ok((stream, addr)) = listener.accept().await {
                    let clients = Arc::clone(&clients);
                    let callback = track_callback.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, addr, clients, callback).await {
                            error!("Error handling connection from {}: {}", addr, e);
                        }
                    });
                }

                Ok(())
            }
            Err(e) => {
                error!("Failed to bind to {}: {}", addr, e);
                error!("This usually means:");
                error!("  1. Another instance of the application is running");
                error!("  2. The port is occupied by another process");
                error!("  3. Permission denied (try running as administrator)");
                Err(e.into())
            }
        }
    }

    pub async fn broadcast_to_extension(&self, message: ExtensionMessage) -> Result<(), String> {
        let mut clients = self.clients.lock().await;
        let message_text = serde_json::to_string(&message).map_err(|e| e.to_string())?;
        let ws_message = Message::Text(message_text.clone());

        info!("Broadcasting to {} clients: {}", clients.len(), message_text);

        if clients.is_empty() {
            warn!("No WebSocket clients connected - message not sent");
            return Ok(());
        }

        // Keep track of disconnected clients to remove them
        let mut disconnected_clients = Vec::new();
        let mut successful_sends = 0;

        for (client_id, sender) in clients.iter() {
            match sender.send(ws_message.clone()) {
                Ok(_) => {
                    info!("✅ Sent message to client {}", client_id);
                    successful_sends += 1;
                }
                Err(_) => {
                    error!("❌ Failed to send to client {}: channel closed", client_id);
                    disconnected_clients.push(client_id.clone());
                }
            }
        }

        // Remove disconnected clients
        for client_id in disconnected_clients {
            clients.remove(&client_id);
            warn!("Removed disconnected client: {}", client_id);
        }

        info!("Message broadcast complete: {} successful, {} failed", successful_sends, clients.len() - successful_sends);
        Ok(())
    }

    pub async fn send_playback_command(
        &self,
        command: String,
        seek_time: Option<f64>,
    ) -> Result<(), String> {
        let message = ExtensionMessage {
            message_type: "PLAYBACK_COMMAND".to_string(),
            data: serde_json::json!({
                "command": command,
                "seekTime": seek_time
            }),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        self.broadcast_to_extension(message).await
    }
}

async fn handle_connection(
    raw_stream: TcpStream,
    addr: SocketAddr,
    clients: ClientConnections,
    track_callback: Option<Arc<dyn Fn(TrackUpdate) + Send + Sync>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(raw_stream).await?;
    let client_id = Uuid::new_v4().to_string();

    info!("New WebSocket connection: {} with ID: {}", addr, client_id);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send welcome message
    let welcome_msg = ExtensionMessage {
        message_type: "connected".to_string(),
        data: serde_json::json!({
            "client_id": client_id,
            "status": "ready"
        }),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    let welcome_text = serde_json::to_string(&welcome_msg)?;
    ws_sender.send(Message::Text(welcome_text)).await?;

    // Create mpsc channel for this client
    let (tx, mut rx) = mpsc::unbounded_channel();
    
    // Add client to connections after welcome message
    {
        let mut clients_guard = clients.lock().await;
        clients_guard.insert(client_id.clone(), tx);
        info!("✅ Client {} added to connections with mpsc channel. Total clients: {}", client_id, clients_guard.len());
    }

    // Spawn a task to forward messages from the channel to the WebSocket
    let client_id_for_sender = client_id.clone();
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = ws_sender.send(message).await {
                error!("❌ Failed to send message to client {}: {}", client_id_for_sender, e);
                break;
            }
        }
        debug!("Message forwarding task ended for client {}", client_id_for_sender);
    });

    // Handle incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(extension_msg) = serde_json::from_str::<ExtensionMessage>(&text) {
                    // println!("📨 WebSocket received: {} with data: {}", extension_msg.message_type, extension_msg.data);

                    match extension_msg.message_type.as_str() {
                        "TRACK_DETECTED" | "TRACK_PAUSED" | "TRACK_STOPPED" | "TRACK_PROGRESS" => {
                            if let Ok(track_update) =
                                serde_json::from_value::<TrackUpdate>(extension_msg.data.clone())
                            {
                                // println!("✅ Successfully parsed TrackUpdate: {:?}", track_update);
                                // Call the track callback if available
                                if let Some(ref callback) = track_callback {
                                    callback(track_update);
                                }
                            } else {
                                error!(
                                    "Failed to parse TrackUpdate from: {}",
                                    extension_msg.data
                                );
                            }
                        }
                        "ping" => {
                            let pong_msg = ExtensionMessage {
                                message_type: "pong".to_string(),
                                data: serde_json::json!({}),
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as u64,
                            };
                            let pong_text = serde_json::to_string(&pong_msg)?;

                            // Get sender from connections to send pong
                            let clients_guard = clients.lock().await;
                            if let Some(sender) = clients_guard.get(&client_id) {
                                let _ = sender.send(Message::Text(pong_text));
                            }
                        }
                        _ => {
                            debug!("Unknown message type: {}", extension_msg.message_type);
                        }
                    }
                } else {
                    warn!("Failed to parse message: {}", text);
                }
            }
            Message::Binary(_) => {
                debug!("Received binary message (not supported)");
            }
            Message::Close(_) => {
                info!("Client {} disconnected", client_id);
                break;
            }
            _ => {}
        }
    }

    // Remove client from connections
    {
        let mut clients_guard = clients.lock().await;
        clients_guard.remove(&client_id);
        info!("❌ Client {} disconnected and removed. Total clients: {}", client_id, clients_guard.len());
    }

    Ok(())
}

// Helper function to create a default WebSocket server instance
pub fn create_websocket_server() -> WebSocketServer {
    WebSocketServer::new(8765) // Default port
}
