use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::fmt;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
use tokio::net::TcpListener;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use tauri::Manager;
use iroh_tickets::endpoint::EndpointTicket;
use crate::p2p::network::{local_endpoint_async, local_store_async};
use std::str::FromStr;
use std::io::SeekFrom;

pub(super) const STREAM_ALPN: &[u8] = b"/rau/stream/1";

#[derive(Clone)]
pub(super) struct StreamProtocol {
    app: AppHandle,
    endpoint_id: String,
}

impl StreamProtocol {
    pub(super) fn new(app: AppHandle, endpoint_id: String) -> Self {
        Self { app, endpoint_id }
    }
}

impl fmt::Debug for StreamProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamProtocol")
            .field("endpoint_id", &self.endpoint_id)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StreamRequest {
    #[serde(rename = "list_tracks")]
    ListTracks,
}

impl ProtocolHandler for StreamProtocol {
    async fn accept(
        &self,
        conn: Connection,
    ) -> Result<(), AcceptError> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let app = self.app.clone();

        tokio::spawn(async move {
            let buf = match recv.read_to_end(1024 * 1024).await {
                Ok(b) => b,
                Err(_) => return,
            };

            if let Ok(req) = serde_json::from_slice::<StreamRequest>(&buf) {
                match req {
                    StreamRequest::ListTracks => {
                        let state = app.state::<BroadcasterState>();
                        let tracks = state.tracks.read().await.clone();
                        if let Ok(json) = serde_json::to_vec(&tracks) {
                            let _ = send.write_all(&json).await;
                            let _ = send.finish();
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrack {
    pub hash: String,
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub async fn scan_audio_directory(path: String) -> Result<Vec<AudioTrack>, String> {
    let mut tracks = Vec::new();
    let root = std::path::Path::new(&path);
    println!("Scanning directory: {:?}", root);
    if !root.exists() || !root.is_dir() {
        println!("Invalid directory: {:?}", root);
        return Err("Invalid directory".into());
    }

    let store = local_store_async().await.map_err(|e| format!("Store no inicializado: {}", e))?;
    println!("Store initialized successfully.");

    let entries: Vec<_> = WalkDir::new(root).follow_links(false).into_iter().collect();
    println!("WalkDir found {} total entries (including errors)", entries.len());

    for entry_result in entries {
        match entry_result {
            Ok(entry) => {
                if entry.file_type().is_file() {
                    let file_path = entry.path();
                    println!("Checking file: {:?}", file_path);
            if let Some(ext) = file_path.extension().and_then(|s| s.to_str()) {
                let ext = ext.to_lowercase();
                if matches!(ext.as_str(), "mp3" | "wav" | "flac" | "aiff" | "m4a") {
                    let path_buf = file_path.to_path_buf();
                    println!("Starting hashing for {:?}", path_buf);
                    
                    // Workaround: read bytes manually to avoid Iroh FsStore add_path deadlock
                    match tokio::fs::read(&path_buf).await {
                        Ok(bytes) => {
                            match store.blobs().add_bytes(bytes).await {
                                Ok(tag) => {
                                    let hash = tag.hash.to_string();
                                    let name = entry.file_name().to_string_lossy().into_owned();
                                    println!("Added track: {} ({})", name, hash);
                                    tracks.push(AudioTrack {
                                        hash,
                                        name,
                                        path: file_path.to_string_lossy().into_owned(),
                                    });
                                }
                                Err(e) => {
                                    println!("Failed to add bytes {:?}: {}", file_path, e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Failed to read file {:?}: {}", file_path, e);
                        }
                    }
                }
            }
                }
            }
            Err(e) => {
                println!("WalkDir entry error: {:?}", e);
            }
        }
    }
    
    Ok(tracks)
}

pub struct BroadcasterState {
    pub tracks: Arc<AsyncRwLock<Vec<AudioTrack>>>,
}

#[tauri::command]
pub async fn set_broadcaster_tracks(
    app: tauri::AppHandle,
    tracks: Vec<AudioTrack>,
) -> Result<(), String> {
    let state = app.state::<BroadcasterState>();
    let mut current = state.tracks.write().await;
    *current = tracks;
    Ok(())
}

#[tauri::command]
pub async fn get_broadcaster_tracks(app: tauri::AppHandle) -> Result<Vec<AudioTrack>, String> {
    let state = app.state::<BroadcasterState>();
    let tracks = state.tracks.read().await.clone();
    Ok(tracks)
}

pub struct ListenerState {
    pub connection: Arc<tokio::sync::Mutex<Option<Connection>>>,
}

#[tauri::command]
pub async fn connect_to_broadcaster(
    app: tauri::AppHandle,
    ticket: String,
) -> Result<Vec<AudioTrack>, String> {
    let endpoint = local_endpoint_async().await.map_err(|e| e.to_string())?;
    let ticket_parsed = ticket
        .parse::<EndpointTicket>()
        .map_err(|e| format!("Invalid ticket: {e}"))?;
        
    let broadcaster_id = ticket_parsed.endpoint_addr().id;

    let conn = endpoint
        .connect(ticket_parsed.endpoint_addr().clone(), STREAM_ALPN)
        .await
        .map_err(|e| e.to_string())?;
        
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    
    let req_json = serde_json::to_vec(&StreamRequest::ListTracks).unwrap();
    send.write_all(&req_json).await.map_err(|e| e.to_string())?;
    send.finish().map_err(|e| e.to_string())?;
    
    let buf = recv.read_to_end(1024 * 1024).await.map_err(|e| e.to_string())?;
    let tracks: Vec<AudioTrack> = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;
    
    let state = app.state::<ListenerState>();
    *state.connection.lock().await = Some(conn.clone());
    
    tauri::async_runtime::spawn(async move {
        let store = match local_store_async().await {
            Ok(s) => s,
            Err(_) => return,
        };
        let downloader = store.downloader(&endpoint);
        
        if let Ok(listener) = TcpListener::bind("127.0.0.1:4000").await {
            while let Ok((mut tcp_stream, _)) = listener.accept().await {
                let store = store.clone();
                let downloader = downloader.clone();
                
                tokio::spawn(async move {
                    let mut http_buf = [0u8; 1024];
                    if let Ok(n) = tcp_stream.read(&mut http_buf).await {
                        let request_str = String::from_utf8_lossy(&http_buf[..n]);
                        if let Some(hash_str) = request_str.split("hash=").nth(1).and_then(|s| s.split_whitespace().next()) {
                            if let Ok(hash) = iroh_blobs::Hash::from_str(hash_str) {
                                let _ = downloader.download(hash, Some(broadcaster_id)).await;
                                
                                let mut blob_reader = store.blobs().reader(hash);
                                    let total_size = match blob_reader.seek(SeekFrom::End(0)).await {
                                        Ok(s) => s,
                                        Err(_) => return,
                                    };
                                    
                                    let mut start_offset = 0;
                                    let mut is_partial = false;
                                    
                                    if let Some(range_line) = request_str.lines().find(|l| l.to_lowercase().starts_with("range:")) {
                                        if let Some(bytes_val) = range_line.split("bytes=").nth(1) {
                                            if let Some(start_str) = bytes_val.split('-').next() {
                                                if let Ok(s) = start_str.trim().parse::<u64>() {
                                                    start_offset = s;
                                                    is_partial = true;
                                                }
                                            }
                                        }
                                    }
                                    
                                    let headers = if is_partial {
                                        format!("HTTP/1.1 206 Partial Content\r\n\
                                                 Content-Type: audio/mpeg\r\n\
                                                 Accept-Ranges: bytes\r\n\
                                                 Content-Range: bytes {}-{}/{}\r\n\
                                                 Content-Length: {}\r\n\
                                                 Connection: close\r\n\r\n",
                                                 start_offset, total_size - 1, total_size, total_size - start_offset)
                                    } else {
                                        format!("HTTP/1.1 200 OK\r\n\
                                                 Content-Type: audio/mpeg\r\n\
                                                 Accept-Ranges: bytes\r\n\
                                                 Content-Length: {}\r\n\
                                                 Connection: close\r\n\r\n", total_size)
                                    };
                                    
                                    if tcp_stream.write_all(headers.as_bytes()).await.is_ok() {
                                        let _ = blob_reader.seek(SeekFrom::Start(start_offset)).await;
                                        let _ = tokio::io::copy(&mut blob_reader, &mut tcp_stream).await;
                                    }
                            }
                        }
                    }
                });
            }
        }
    });
    
    Ok(tracks)
}

