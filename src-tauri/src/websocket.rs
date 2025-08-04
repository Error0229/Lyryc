use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
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
}

type ClientConnections = Arc<Mutex<HashMap<String, tokio_tungstenite::WebSocketStream<TcpStream>>>>;

pub struct WebSocketServer {
    port: u16,
    clients: ClientConnections,
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
        let listener = TcpListener::bind(&addr).await?;
        
        println!("WebSocket server listening on: {}", addr);

        let clients = Arc::clone(&self.clients);
        let track_callback = self.track_callback.clone();

        while let Ok((stream, addr)) = listener.accept().await {
            let clients = Arc::clone(&clients);
            let callback = track_callback.clone();
            
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, addr, clients, callback).await {
                    eprintln!("Error handling connection: {}", e);
                }
            });
        }

        Ok(())
    }

    pub async fn broadcast_to_extension(&self, message: ExtensionMessage) -> Result<(), String> {
        let clients = self.clients.lock().map_err(|e| e.to_string())?;
        let message_text = serde_json::to_string(&message).map_err(|e| e.to_string())?;
        
        for (client_id, _ws_stream) in clients.iter() {
            // Note: In a real implementation, you'd need to handle sending properly
            // This is a simplified version
            println!("Would send to client {}: {}", client_id, message_text);
        }
        
        Ok(())
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
    
    println!("New WebSocket connection: {} with ID: {}", addr, client_id);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Add client to connections
    {
        let _clients_guard = clients.lock().unwrap();
        // Note: This is simplified - in practice you'd store the sender part
        println!("Client {} connected", client_id);
    }

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

    // Handle incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(extension_msg) = serde_json::from_str::<ExtensionMessage>(&text) {
                    match extension_msg.message_type.as_str() {
                        "TRACK_DETECTED" | "TRACK_PAUSED" | "TRACK_STOPPED" => {
                            if let Ok(track_update) = serde_json::from_value::<TrackUpdate>(extension_msg.data.clone()) {
                                // Call the track callback if available
                                if let Some(ref callback) = track_callback {
                                    callback(track_update);
                                }
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
                            ws_sender.send(Message::Text(pong_text)).await?;
                        }
                        _ => {
                            println!("Unknown message type: {}", extension_msg.message_type);
                        }
                    }
                } else {
                    println!("Failed to parse message: {}", text);
                }
            }
            Message::Binary(_) => {
                println!("Received binary message (not supported)");
            }
            Message::Close(_) => {
                println!("Client {} disconnected", client_id);
                break;
            }
            _ => {}
        }
    }

    // Remove client from connections
    {
        let _clients_guard = clients.lock().unwrap();
        println!("Client {} disconnected", client_id);
    }

    Ok(())
}

// Helper function to create a default WebSocket server instance
pub fn create_websocket_server() -> WebSocketServer {
    WebSocketServer::new(8765) // Default port
}