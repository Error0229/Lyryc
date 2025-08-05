use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
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
    #[serde(rename = "currentTime")]
    pub current_time: Option<f64>,
    pub duration: Option<f64>,
}

use futures_util::stream::SplitSink;
use tokio_tungstenite::WebSocketStream;

type ClientSender = SplitSink<WebSocketStream<TcpStream>, Message>;
type ClientConnections = Arc<Mutex<HashMap<String, ClientSender>>>;

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
        let mut clients = self.clients.lock().await;
        let message_text = serde_json::to_string(&message).map_err(|e| e.to_string())?;
        let ws_message = Message::Text(message_text.clone());
        
        // Keep track of disconnected clients to remove them
        let mut disconnected_clients = Vec::new();
        
        for (client_id, sender) in clients.iter_mut() {
            match sender.send(ws_message.clone()).await {
                Ok(_) => {
                    println!("Sent message to client {}: {}", client_id, message_text);
                }
                Err(e) => {
                    println!("Failed to send to client {}: {}", client_id, e);
                    disconnected_clients.push(client_id.clone());
                }
            }
        }
        
        // Remove disconnected clients
        for client_id in disconnected_clients {
            clients.remove(&client_id);
        }
        
        Ok(())
    }

    pub async fn send_playback_command(&self, command: String, seek_time: Option<f64>) -> Result<(), String> {
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
    
    println!("New WebSocket connection: {} with ID: {}", addr, client_id);

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

    // Add client to connections after welcome message
    {
        let mut clients_guard = clients.lock().await;
        clients_guard.insert(client_id.clone(), ws_sender);
        println!("Client {} added to connections", client_id);
    }

    // Handle incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(extension_msg) = serde_json::from_str::<ExtensionMessage>(&text) {
                    match extension_msg.message_type.as_str() {
                        "TRACK_DETECTED" | "TRACK_PAUSED" | "TRACK_STOPPED" | "TRACK_PROGRESS" => {
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
                            
                            // Get sender from connections to send pong
                            let mut clients_guard = clients.lock().await;
                            if let Some(sender) = clients_guard.get_mut(&client_id) {
                                let _ = sender.send(Message::Text(pong_text)).await;
                            }
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
        let mut clients_guard = clients.lock().await;
        clients_guard.remove(&client_id);
        println!("Client {} disconnected and removed", client_id);
    }

    Ok(())
}

// Helper function to create a default WebSocket server instance
pub fn create_websocket_server() -> WebSocketServer {
    WebSocketServer::new(8765) // Default port
}