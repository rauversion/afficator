use super::{
    catalog, chat, stream, mark_peer_seen, open_db, peer_endpoint_ticket, unlocked_network_identity,
};
use tauri::Manager;
use chrono::Utc;
use iroh::{
    endpoint::{presets, Connection},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointAddr, SecretKey,
};
use iroh_tickets::endpoint::EndpointTicket;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

pub(super) const DIAGNOSTIC_ALPN: &[u8] = b"/rau/diagnostic/1";
const PROTOCOL_VERSION: u8 = 1;
const MAX_FRAME_BYTES: usize = 4096;
const MAX_TICKET_LENGTH: usize = 8192;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const NETWORK_EVENT: &str = "p2p-network-event";

struct NetworkRuntime {
    endpoint: Endpoint,
    router: Router,
    _gossip: iroh_gossip::net::Gossip,
    pub(crate) store: iroh_blobs::store::fs::FsStore,
    display_name: String,
    started_at: String,
}

#[derive(Clone)]
struct DiagnosticProtocol {
    app: Option<AppHandle>,
    display_name: String,
    endpoint_id: String,
}

impl fmt::Debug for DiagnosticProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticProtocol")
            .field("display_name", &self.display_name)
            .field("endpoint_id", &self.endpoint_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct DiagnosticRequest {
    version: u8,
    kind: String,
    nonce: String,
    display_name: String,
    sent_at_ms: i64,
    #[serde(default)]
    endpoint_ticket: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DiagnosticResponse {
    version: u8,
    kind: String,
    nonce: String,
    endpoint_id: String,
    display_name: String,
    received_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NetworkStatus {
    running: bool,
    endpoint_id: Option<String>,
    ticket: Option<String>,
    relay_ready: bool,
    address_count: usize,
    bound_sockets: Vec<String>,
    started_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NetworkEvent {
    kind: String,
    peer_endpoint_id: Option<String>,
    peer_display_name: Option<String>,
    message: String,
    rtt_ms: Option<f64>,
    occurred_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PingResult {
    remote_endpoint_id: String,
    remote_display_name: String,
    rtt_ms: f64,
    protocol_version: u8,
    received_at: String,
}

fn network_state() -> &'static Mutex<Option<NetworkRuntime>> {
    static STATE: OnceLock<Mutex<Option<NetworkRuntime>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub(crate) async fn local_endpoint_async() -> Result<iroh::Endpoint, String> {
    network_state()
        .lock()
        .await
        .as_ref()
        .map(|runtime| runtime.endpoint.clone())
        .ok_or_else(|| "La red P2P no está iniciada.".to_string())
}


pub(crate) async fn local_store_async() -> Result<iroh_blobs::store::fs::FsStore, String> {
    network_state()
        .lock()
        .await
        .as_ref()
        .map(|runtime| runtime.store.clone())
        .ok_or_else(|| "La red P2P no está iniciada.".to_string())
}

impl ProtocolHandler for DiagnosticProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let (mut send, mut recv) = connection.accept_bi().await?;
        let request_bytes = recv
            .read_to_end(MAX_FRAME_BYTES)
            .await
            .map_err(stream_error)?;
        let request = match serde_json::from_slice::<DiagnosticRequest>(&request_bytes) {
            Ok(request) if valid_request(&request) => request,
            _ => {
                connection.close(1u32.into(), b"invalid rau diagnostic request");
                return Ok(());
            }
        };
        let remote_display_name = safe_peer_name(&request.display_name, &remote_endpoint_id);
        let remote_ticket = request
            .endpoint_ticket
            .as_deref()
            .and_then(|ticket| validated_endpoint_ticket(ticket, &remote_endpoint_id));
        let response = DiagnosticResponse {
            version: PROTOCOL_VERSION,
            kind: "pong".to_string(),
            nonce: request.nonce,
            endpoint_id: self.endpoint_id.clone(),
            display_name: self.display_name.clone(),
            received_at_ms: unix_millis(),
        };
        let response_bytes = match serde_json::to_vec(&response) {
            Ok(bytes) => bytes,
            Err(_) => {
                connection.close(2u32.into(), b"could not encode rau diagnostic response");
                return Ok(());
            }
        };
        send.write_all(&response_bytes)
            .await
            .map_err(stream_error)?;
        send.finish()?;

        if let Some(app) = self.app.clone() {
            observe_peer_async(
                app.clone(),
                remote_endpoint_id.clone(),
                remote_display_name.clone(),
                remote_ticket,
            )
            .await;
            emit_network_event(
                &app,
                NetworkEvent {
                    kind: "incoming_ping".to_string(),
                    peer_endpoint_id: Some(remote_endpoint_id),
                    peer_display_name: Some(remote_display_name),
                    message: "Ping P2P recibido y respondido.".to_string(),
                    rtt_ms: None,
                    occurred_at: timestamp(),
                },
            );
        }

        connection.closed().await;
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn p2p_network_status() -> Result<NetworkStatus, String> {
    let state = network_state().lock().await;
    Ok(state
        .as_ref()
        .map(network_status)
        .unwrap_or_else(stopped_status))
}

#[tauri::command]
pub(crate) async fn p2p_network_start(app: AppHandle) -> Result<NetworkStatus, String> {
    let identity = unlocked_network_identity()?;
    {
        let state = network_state().lock().await;
        if let Some(runtime) = state.as_ref() {
            return Ok(network_status(runtime));
        }
    }

    let secret_key = SecretKey::from_bytes(&identity.secret);
    let endpoint_id = secret_key.public().to_string();
    if endpoint_id != identity.endpoint_id {
        return Err("La identidad desbloqueada no coincide con el endpoint Iroh.".to_string());
    }

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .map_err(|error| format!("No se pudo iniciar endpoint Iroh: {error}"))?;
    let protocol = DiagnosticProtocol {
        app: Some(app.clone()),
        display_name: identity.display_name.clone(),
        endpoint_id: endpoint_id.clone(),
    };
    let gossip = iroh_gossip::net::Gossip::builder().spawn(endpoint.clone());
    
    let blobs_dir = app.path().app_data_dir()
        .map_err(|e| format!("Error con app_data_dir: {e}"))?
        .join("blobs");
    tokio::fs::create_dir_all(&blobs_dir).await.map_err(|e| format!("Error creando blobs dir: {e}"))?;
    let store = iroh_blobs::store::fs::FsStore::load(&blobs_dir).await.map_err(|e| format!("Error store: {e}"))?;
    let blobs_protocol = iroh_blobs::BlobsProtocol::new(&store, None);

    let router = Router::builder(endpoint.clone())
        .accept(DIAGNOSTIC_ALPN, protocol)
        .accept(
            catalog::CATALOG_ALPN,
            catalog::CatalogProtocol::new(app.clone(), endpoint_id.clone()),
        )
        .accept(catalog::FILE_ALPN, catalog::FileProtocol::new(app.clone()))
        .accept(
            chat::CHAT_ALPN,
            chat::ChatProtocol::new(app.clone(), endpoint_id.clone()),
        )
        .accept(
            stream::STREAM_ALPN,
            stream::StreamProtocol::new(app.clone(), endpoint_id.clone()),
        )
        .accept(iroh_gossip::net::GOSSIP_ALPN, gossip.clone())
        .accept(iroh_blobs::ALPN, blobs_protocol)
        .spawn();
    let runtime = NetworkRuntime {
        endpoint,
        router,
        _gossip: gossip,
        store,
        display_name: identity.display_name,
        started_at: timestamp(),
    };
    let status = network_status(&runtime);
    let mut state = network_state().lock().await;
    *state = Some(runtime);
    drop(state);

    emit_network_event(
        &app,
        NetworkEvent {
            kind: "started".to_string(),
            peer_endpoint_id: None,
            peer_display_name: None,
            message: "Endpoint Iroh iniciado.".to_string(),
            rtt_ms: None,
            occurred_at: timestamp(),
        },
    );
    Ok(status)
}

#[tauri::command]
pub(crate) async fn p2p_network_stop(app: AppHandle) -> Result<NetworkStatus, String> {
    stop_if_running(app).await?;
    Ok(stopped_status())
}

pub(super) async fn stop_if_running(app: AppHandle) -> Result<(), String> {
    let runtime = network_state().lock().await.take();
    let Some(runtime) = runtime else {
        return Ok(());
    };
    runtime
        .router
        .shutdown()
        .await
        .map_err(|error| format!("No se pudo detener endpoint Iroh: {error}"))?;
    emit_network_event(
        &app,
        NetworkEvent {
            kind: "stopped".to_string(),
            peer_endpoint_id: None,
            peer_display_name: None,
            message: "Endpoint Iroh detenido.".to_string(),
            rtt_ms: None,
            occurred_at: timestamp(),
        },
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn p2p_network_ping_ticket(
    app: AppHandle,
    ticket: String,
) -> Result<PingResult, String> {
    let ticket_text = ticket.trim();
    if ticket_text.is_empty() || ticket_text.len() > MAX_TICKET_LENGTH {
        return Err("El ticket Iroh esta vacio o es demasiado largo.".to_string());
    }
    let ticket = ticket_text
        .parse::<EndpointTicket>()
        .map_err(|error| format!("Ticket Iroh invalido: {error}"))?;
    let remote_addr = ticket.endpoint_addr().clone();

    let (endpoint, display_name) = {
        let state = network_state().lock().await;
        let runtime = state
            .as_ref()
            .ok_or_else(|| "Inicia la red P2P antes de conectar otro dispositivo.".to_string())?;
        (runtime.endpoint.clone(), runtime.display_name.clone())
    };
    if remote_addr.id == endpoint.id() {
        return Err("Ese ticket pertenece a este mismo dispositivo.".to_string());
    }

    let result = ping_endpoint(&endpoint, remote_addr, &display_name).await?;
    observe_peer_async(
        app.clone(),
        result.remote_endpoint_id.clone(),
        result.remote_display_name.clone(),
        Some(ticket_text.to_string()),
    )
    .await;
    emit_network_event(
        &app,
        NetworkEvent {
            kind: "ping_succeeded".to_string(),
            peer_endpoint_id: Some(result.remote_endpoint_id.clone()),
            peer_display_name: Some(result.remote_display_name.clone()),
            message: format!("Conexion P2P confirmada en {:.1} ms.", result.rtt_ms),
            rtt_ms: Some(result.rtt_ms),
            occurred_at: timestamp(),
        },
    );
    Ok(result)
}

pub(super) async fn connect_known_peer(
    app: &AppHandle,
    peer_endpoint_id: &str,
    alpn: &'static [u8],
) -> Result<Connection, String> {
    let ticket_text = peer_endpoint_ticket(app, peer_endpoint_id)?;
    let ticket = ticket_text
        .parse::<EndpointTicket>()
        .map_err(|error| format!("El ticket guardado del peer no es valido: {error}"))?;
    if ticket.endpoint_addr().id.to_string() != peer_endpoint_id {
        return Err("El ticket guardado no coincide con la identidad del peer.".to_string());
    }
    let endpoint = {
        let state = network_state().lock().await;
        state
            .as_ref()
            .ok_or_else(|| "Inicia la red P2P antes de usar servicios remotos.".to_string())?
            .endpoint
            .clone()
    };
    let connection = tokio::time::timeout(
        CONNECT_TIMEOUT,
        endpoint.connect(ticket.endpoint_addr().clone(), alpn),
    )
    .await
    .map_err(|_| "La conexion P2P excedio 15 segundos.".to_string())?
    .map_err(|error| format!("No se pudo conectar al peer: {error}"))?;
    if connection.remote_id().to_string() != peer_endpoint_id {
        connection.close(4u32.into(), b"unexpected peer identity");
        return Err(
            "La conexion respondio con una identidad distinta al peer elegido.".to_string(),
        );
    }
    mark_peer_seen(app, peer_endpoint_id)?;
    Ok(connection)
}

pub(super) async fn local_display_name() -> Result<String, String> {
    let state = network_state().lock().await;
    state
        .as_ref()
        .map(|runtime| runtime.display_name.clone())
        .ok_or_else(|| "Inicia la red P2P antes de enviar mensajes.".to_string())
}

pub(super) async fn local_endpoint_ticket() -> Result<String, String> {
    let state = network_state().lock().await;
    state
        .as_ref()
        .map(|runtime| EndpointTicket::new(runtime.endpoint.addr()).to_string())
        .ok_or_else(|| "Inicia la red P2P antes de enviar mensajes.".to_string())
}

pub(super) fn observe_return_ticket(
    app: &AppHandle,
    endpoint_id: &str,
    display_name: &str,
    ticket: &str,
) {
    if let Some(ticket) = validated_endpoint_ticket(ticket, endpoint_id) {
        let _ = observe_peer(app, endpoint_id, display_name, Some(&ticket));
    }
}

async fn ping_endpoint(
    endpoint: &Endpoint,
    remote_addr: EndpointAddr,
    display_name: &str,
) -> Result<PingResult, String> {
    let started = Instant::now();
    let connection = tokio::time::timeout(
        CONNECT_TIMEOUT,
        endpoint.connect(remote_addr, DIAGNOSTIC_ALPN),
    )
    .await
    .map_err(|_| "La conexion P2P excedio 15 segundos.".to_string())?
    .map_err(|error| format!("No se pudo conectar al peer: {error}"))?;
    let remote_endpoint_id = connection.remote_id().to_string();
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("No se pudo abrir stream P2P: {error}"))?;
    let nonce = Uuid::new_v4().to_string();
    let request = DiagnosticRequest {
        version: PROTOCOL_VERSION,
        kind: "ping".to_string(),
        nonce: nonce.clone(),
        display_name: display_name.to_string(),
        sent_at_ms: unix_millis(),
        endpoint_ticket: Some(EndpointTicket::new(endpoint.addr()).to_string()),
    };
    let request_bytes = serde_json::to_vec(&request)
        .map_err(|error| format!("No se pudo codificar ping P2P: {error}"))?;
    send.write_all(&request_bytes)
        .await
        .map_err(|error| format!("No se pudo enviar ping P2P: {error}"))?;
    send.finish()
        .map_err(|error| format!("No se pudo finalizar ping P2P: {error}"))?;
    let response_bytes = tokio::time::timeout(RESPONSE_TIMEOUT, recv.read_to_end(MAX_FRAME_BYTES))
        .await
        .map_err(|_| "El peer no respondio el ping dentro de 10 segundos.".to_string())?
        .map_err(|error| format!("No se pudo leer pong P2P: {error}"))?;
    let response = serde_json::from_slice::<DiagnosticResponse>(&response_bytes)
        .map_err(|error| format!("El peer respondio un pong invalido: {error}"))?;
    if response.version != PROTOCOL_VERSION
        || response.kind != "pong"
        || response.nonce != nonce
        || response.endpoint_id != remote_endpoint_id
    {
        connection.close(3u32.into(), b"invalid rau diagnostic response");
        return Err("El pong P2P no coincide con la conexion autenticada.".to_string());
    }
    connection.close(0u32.into(), b"rau diagnostic complete");

    Ok(PingResult {
        remote_endpoint_id: remote_endpoint_id.clone(),
        remote_display_name: safe_peer_name(&response.display_name, &remote_endpoint_id),
        rtt_ms: started.elapsed().as_secs_f64() * 1000.0,
        protocol_version: response.version,
        received_at: timestamp(),
    })
}

fn network_status(runtime: &NetworkRuntime) -> NetworkStatus {
    let addr = runtime.endpoint.addr();
    let relay_ready = addr.relay_urls().next().is_some();
    let address_count = addr.addrs.len();
    let ticket = EndpointTicket::new(addr).to_string();
    NetworkStatus {
        running: !runtime.endpoint.is_closed(),
        endpoint_id: Some(runtime.endpoint.id().to_string()),
        ticket: Some(ticket),
        relay_ready,
        address_count,
        bound_sockets: runtime
            .endpoint
            .bound_sockets()
            .into_iter()
            .map(|socket| socket.to_string())
            .collect(),
        started_at: Some(runtime.started_at.clone()),
    }
}

fn stopped_status() -> NetworkStatus {
    NetworkStatus {
        running: false,
        endpoint_id: None,
        ticket: None,
        relay_ready: false,
        address_count: 0,
        bound_sockets: Vec::new(),
        started_at: None,
    }
}

fn valid_request(request: &DiagnosticRequest) -> bool {
    request.version == PROTOCOL_VERSION
        && request.kind == "ping"
        && request.nonce.len() == 36
        && request.display_name.chars().count() >= 2
        && request.display_name.chars().count() <= 64
        && request.sent_at_ms > 0
        && request
            .endpoint_ticket
            .as_ref()
            .is_none_or(|ticket| ticket.len() <= MAX_TICKET_LENGTH)
}

fn safe_peer_name(value: &str, endpoint_id: &str) -> String {
    let value = value.trim();
    if (2..=64).contains(&value.chars().count()) {
        value.to_string()
    } else {
        format!("Peer {}", &endpoint_id[..endpoint_id.len().min(12)])
    }
}

fn validated_endpoint_ticket(ticket: &str, endpoint_id: &str) -> Option<String> {
    let ticket = ticket.trim();
    if ticket.is_empty() || ticket.len() > MAX_TICKET_LENGTH {
        return None;
    }
    let parsed = ticket.parse::<EndpointTicket>().ok()?;
    (parsed.endpoint_addr().id.to_string() == endpoint_id).then(|| ticket.to_string())
}

async fn observe_peer_async(
    app: AppHandle,
    endpoint_id: String,
    display_name: String,
    endpoint_ticket: Option<String>,
) {
    let _ = tokio::task::spawn_blocking(move || {
        observe_peer(
            &app,
            &endpoint_id,
            &display_name,
            endpoint_ticket.as_deref(),
        )
    })
    .await;
}

fn observe_peer(
    app: &AppHandle,
    endpoint_id: &str,
    display_name: &str,
    endpoint_ticket: Option<&str>,
) -> Result<(), String> {
    let conn = open_db(app)?;
    let now = timestamp();
    conn.execute(
        "INSERT INTO p2p_peers (
           endpoint_id, display_name, trust_state, presence_status,
           last_endpoint_addr, last_seen_at, paired_at
         ) VALUES (?1, ?2, 'observed', 'online', ?3, ?4, ?4)
         ON CONFLICT(endpoint_id) DO UPDATE SET
           display_name = CASE
             WHEN p2p_peers.trust_state = 'observed' THEN excluded.display_name
             ELSE p2p_peers.display_name
           END,
           presence_status = 'online',
           last_endpoint_addr = COALESCE(excluded.last_endpoint_addr, p2p_peers.last_endpoint_addr),
           last_seen_at = excluded.last_seen_at",
        params![endpoint_id, display_name, endpoint_ticket, now],
    )
    .map_err(|error| format!("No se pudo registrar peer observado: {error}"))?;
    Ok(())
}

fn emit_network_event(app: &AppHandle, event: NetworkEvent) {
    let _ = app.emit(NETWORK_EVENT, event);
}

fn stream_error(error: impl fmt::Display) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_requests_are_versioned_and_bounded() {
        let mut request = DiagnosticRequest {
            version: PROTOCOL_VERSION,
            kind: "ping".to_string(),
            nonce: Uuid::new_v4().to_string(),
            display_name: "Rau Test".to_string(),
            sent_at_ms: unix_millis(),
            endpoint_ticket: None,
        };
        assert!(valid_request(&request));
        request.version = 2;
        assert!(!valid_request(&request));
        request.version = PROTOCOL_VERSION;
        request.display_name = "x".repeat(65);
        assert!(!valid_request(&request));
    }

    #[tokio::test]
    async fn diagnostic_protocol_moves_real_bytes_between_local_endpoints() {
        let receiver_secret = SecretKey::generate();
        let receiver_id = receiver_secret.public().to_string();
        let receiver = Endpoint::builder(presets::Minimal)
            .secret_key(receiver_secret)
            .bind()
            .await
            .expect("bind receiver");
        let router = Router::builder(receiver.clone())
            .accept(
                DIAGNOSTIC_ALPN,
                DiagnosticProtocol {
                    app: None,
                    display_name: "Receiver".to_string(),
                    endpoint_id: receiver_id.clone(),
                },
            )
            .spawn();
        let sender = Endpoint::bind(presets::Minimal).await.expect("bind sender");

        let result = ping_endpoint(&sender, receiver.addr(), "Sender")
            .await
            .expect("ping local receiver");
        assert_eq!(result.remote_endpoint_id, receiver_id);
        assert_eq!(result.remote_display_name, "Receiver");
        assert_eq!(result.protocol_version, PROTOCOL_VERSION);

        sender.close().await;
        router.shutdown().await.expect("shutdown receiver");
    }
}
