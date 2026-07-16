use super::{mark_peer_seen, network, open_db, require_unlocked_identity};
use chrono::Utc;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub(super) const CHAT_ALPN: &[u8] = b"/rau/chat/1";
const PROTOCOL_VERSION: u8 = 1;
const MAX_CHAT_FRAME_BYTES: usize = 32 * 1024;
const MAX_CHAT_BODY_CHARS: usize = 4_000;
const CHAT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const CHAT_EVENT: &str = "p2p-chat-event";

#[derive(Clone)]
pub(super) struct ChatProtocol {
    app: AppHandle,
    endpoint_id: String,
}

impl ChatProtocol {
    pub(super) fn new(app: AppHandle, endpoint_id: String) -> Self {
        Self { app, endpoint_id }
    }
}

impl fmt::Debug for ChatProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChatProtocol")
            .field("endpoint_id", &self.endpoint_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChatWireMessage {
    version: u8,
    id: String,
    room: String,
    sender_endpoint_id: String,
    sender_display_name: String,
    body: String,
    sent_at: String,
    endpoint_ticket: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatAck {
    version: u8,
    message_id: String,
    receiver_endpoint_id: String,
    accepted: bool,
    error: Option<String>,
    received_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChatMessage {
    id: String,
    room: String,
    peer_endpoint_id: String,
    sender_endpoint_id: String,
    sender_display_name: String,
    body: String,
    direction: String,
    delivery_status: String,
    sent_at: String,
    received_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChatSendResult {
    message: ChatMessage,
    attempted_recipients: usize,
    delivered_recipients: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ChatEvent {
    kind: String,
    message: ChatMessage,
}

impl ProtocolHandler for ChatProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let (mut send, mut recv) = connection.accept_bi().await?;
        let bytes = recv
            .read_to_end(MAX_CHAT_FRAME_BYTES)
            .await
            .map_err(stream_error)?;
        let request = serde_json::from_slice::<ChatWireMessage>(&bytes).ok();
        let mut ack = ChatAck {
            version: PROTOCOL_VERSION,
            message_id: request
                .as_ref()
                .map(|message| message.id.clone())
                .unwrap_or_default(),
            receiver_endpoint_id: self.endpoint_id.clone(),
            accepted: false,
            error: None,
            received_at: timestamp(),
        };

        match request {
            Some(message)
                if valid_wire_message(&message)
                    && message.sender_endpoint_id == remote_endpoint_id =>
            {
                let app = self.app.clone();
                let peer = remote_endpoint_id.clone();
                let stored = message.clone();
                match tokio::task::spawn_blocking(move || {
                    persist_incoming_message(&app, &peer, &stored)
                })
                .await
                {
                    Ok(Ok(chat_message)) => {
                        if let Some(ticket) = message.endpoint_ticket.as_deref() {
                            network::observe_return_ticket(
                                &self.app,
                                &remote_endpoint_id,
                                &message.sender_display_name,
                                ticket,
                            );
                        }
                        ack.accepted = true;
                        let _ = self.app.emit(
                            CHAT_EVENT,
                            ChatEvent {
                                kind: "message_received".to_string(),
                                message: chat_message,
                            },
                        );
                    }
                    Ok(Err(error)) => ack.error = Some(error),
                    Err(error) => {
                        ack.error = Some(format!("No se pudo guardar mensaje recibido: {error}"))
                    }
                }
            }
            _ => ack.error = Some("Mensaje P2P invalido o identidad no coincidente.".to_string()),
        }

        let response = serde_json::to_vec(&ack).map_err(stream_error)?;
        send.write_all(&response).await.map_err(stream_error)?;
        send.finish()?;
        connection.closed().await;
        Ok(())
    }
}

#[tauri::command]
pub(crate) fn p2p_chat_list(
    app: AppHandle,
    room: String,
    peer_endpoint_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<ChatMessage>, String> {
    require_unlocked_identity()?;
    let room = validate_room(&room)?;
    let limit = limit.unwrap_or(200).clamp(1, 500);
    let conn = open_db(&app)?;
    let mut sql = String::from(
        "SELECT id, room, peer_endpoint_id, sender_endpoint_id, sender_display_name,
                body, direction, delivery_status, sent_at, received_at
         FROM p2p_chat_messages WHERE room = ?1",
    );
    let peer = if room == "private" {
        let peer = peer_endpoint_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Selecciona un peer para abrir el chat privado.".to_string())?;
        sql.push_str(" AND peer_endpoint_id = ?2");
        Some(peer)
    } else {
        None
    };
    sql.push_str(&format!(
        " ORDER BY sent_at DESC, created_at DESC LIMIT {limit}"
    ));
    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar historial de chat: {error}"))?;
    let mapper = |row: &rusqlite::Row<'_>| {
        Ok(ChatMessage {
            id: row.get(0)?,
            room: row.get(1)?,
            peer_endpoint_id: row.get(2)?,
            sender_endpoint_id: row.get(3)?,
            sender_display_name: row.get(4)?,
            body: row.get(5)?,
            direction: row.get(6)?,
            delivery_status: row.get(7)?,
            sent_at: row.get(8)?,
            received_at: row.get(9)?,
        })
    };
    let rows = if let Some(peer) = peer.as_deref() {
        statement.query_map(params![room, peer], mapper)
    } else {
        statement.query_map(params![room], mapper)
    }
    .map_err(|error| format!("No se pudo consultar historial de chat: {error}"))?;
    let mut messages = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo leer historial de chat: {error}"))?;
    messages.reverse();
    Ok(messages)
}

#[tauri::command]
pub(crate) async fn p2p_chat_send(
    app: AppHandle,
    room: String,
    peer_endpoint_id: Option<String>,
    body: String,
) -> Result<ChatSendResult, String> {
    let local_endpoint_id = require_unlocked_identity()?;
    let room = validate_room(&room)?;
    let body = validate_body(&body)?;
    let display_name = network::local_display_name().await?;
    let endpoint_ticket = network::local_endpoint_ticket().await?;
    let id = Uuid::new_v4().to_string();
    let sent_at = timestamp();
    let targets = if room == "private" {
        vec![peer_endpoint_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Selecciona un peer para enviar el mensaje privado.".to_string())?]
    } else {
        list_general_targets(&app)?
    };
    if targets.is_empty() {
        return Err("No hay peers con ticket de retorno para recibir el mensaje.".to_string());
    }
    let storage_peer = if room == "private" {
        targets[0].clone()
    } else {
        "*".to_string()
    };
    let message = ChatWireMessage {
        version: PROTOCOL_VERSION,
        id: id.clone(),
        room: room.clone(),
        sender_endpoint_id: local_endpoint_id.clone(),
        sender_display_name: display_name.clone(),
        body: body.clone(),
        sent_at: sent_at.clone(),
        endpoint_ticket: Some(endpoint_ticket),
    };
    persist_outgoing_message(&app, &storage_peer, &message, "pending")?;

    let mut delivered = 0usize;
    let mut last_error = None;
    for target in &targets {
        match send_to_peer(&app, target, &message).await {
            Ok(()) => delivered += 1,
            Err(error) => last_error = Some(error),
        }
    }
    let status = if delivered == targets.len() {
        "delivered"
    } else if delivered > 0 {
        "partial"
    } else {
        "failed"
    };
    update_delivery_status(&app, &id, status)?;
    let stored = load_message(&app, &id)?;
    let _ = app.emit(
        CHAT_EVENT,
        ChatEvent {
            kind: "message_sent".to_string(),
            message: stored.clone(),
        },
    );
    if delivered == 0 {
        return Err(last_error.unwrap_or_else(|| "Ningun peer recibio el mensaje.".to_string()));
    }
    Ok(ChatSendResult {
        message: stored,
        attempted_recipients: targets.len(),
        delivered_recipients: delivered,
    })
}

async fn send_to_peer(
    app: &AppHandle,
    peer_endpoint_id: &str,
    message: &ChatWireMessage,
) -> Result<(), String> {
    let connection = network::connect_known_peer(app, peer_endpoint_id, CHAT_ALPN).await?;
    let authenticated_peer = connection.remote_id().to_string();
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("No se pudo abrir chat con peer: {error}"))?;
    let bytes = serde_json::to_vec(message)
        .map_err(|error| format!("No se pudo codificar mensaje: {error}"))?;
    send.write_all(&bytes)
        .await
        .map_err(|error| format!("No se pudo enviar mensaje: {error}"))?;
    send.finish()
        .map_err(|error| format!("No se pudo finalizar mensaje: {error}"))?;
    let response = tokio::time::timeout(
        CHAT_RESPONSE_TIMEOUT,
        recv.read_to_end(MAX_CHAT_FRAME_BYTES),
    )
    .await
    .map_err(|_| "El peer no confirmo el mensaje dentro de 15 segundos.".to_string())?
    .map_err(|error| format!("No se pudo leer confirmacion del chat: {error}"))?;
    let ack = serde_json::from_slice::<ChatAck>(&response)
        .map_err(|error| format!("El peer respondio una confirmacion invalida: {error}"))?;
    if ack.version != PROTOCOL_VERSION
        || ack.message_id != message.id
        || ack.receiver_endpoint_id != authenticated_peer
    {
        return Err("La confirmacion no coincide con el peer autenticado.".to_string());
    }
    if !ack.accepted {
        return Err(ack
            .error
            .unwrap_or_else(|| "El peer rechazo el mensaje.".to_string()));
    }
    connection.close(0u32.into(), b"rau chat complete");
    Ok(())
}

fn persist_incoming_message(
    app: &AppHandle,
    remote_endpoint_id: &str,
    message: &ChatWireMessage,
) -> Result<ChatMessage, String> {
    mark_peer_seen(app, remote_endpoint_id)?;
    let conn = open_db(app)?;
    let received_at = timestamp();
    conn.execute(
        "INSERT OR IGNORE INTO p2p_chat_messages (
           id, room, peer_endpoint_id, sender_endpoint_id, sender_display_name,
           body, direction, delivery_status, sent_at, received_at, created_at
         ) VALUES (?1, ?2, ?3, ?3, ?4, ?5, 'incoming', 'delivered', ?6, ?7, ?7)",
        params![
            message.id,
            message.room,
            remote_endpoint_id,
            message.sender_display_name,
            message.body,
            message.sent_at,
            received_at,
        ],
    )
    .map_err(|error| format!("No se pudo guardar mensaje recibido: {error}"))?;
    load_message(app, &message.id)
}

fn persist_outgoing_message(
    app: &AppHandle,
    peer_endpoint_id: &str,
    message: &ChatWireMessage,
    status: &str,
) -> Result<(), String> {
    let conn = open_db(app)?;
    conn.execute(
        "INSERT INTO p2p_chat_messages (
           id, room, peer_endpoint_id, sender_endpoint_id, sender_display_name,
           body, direction, delivery_status, sent_at, received_at, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'outgoing', ?7, ?8, NULL, ?8)",
        params![
            message.id,
            message.room,
            peer_endpoint_id,
            message.sender_endpoint_id,
            message.sender_display_name,
            message.body,
            status,
            message.sent_at,
        ],
    )
    .map_err(|error| format!("No se pudo guardar mensaje saliente: {error}"))?;
    Ok(())
}

fn update_delivery_status(app: &AppHandle, id: &str, status: &str) -> Result<(), String> {
    let conn = open_db(app)?;
    conn.execute(
        "UPDATE p2p_chat_messages SET delivery_status = ?2 WHERE id = ?1",
        params![id, status],
    )
    .map_err(|error| format!("No se pudo actualizar entrega del mensaje: {error}"))?;
    Ok(())
}

fn load_message(app: &AppHandle, id: &str) -> Result<ChatMessage, String> {
    let conn = open_db(app)?;
    conn.query_row(
        "SELECT id, room, peer_endpoint_id, sender_endpoint_id, sender_display_name,
                body, direction, delivery_status, sent_at, received_at
         FROM p2p_chat_messages WHERE id = ?1",
        params![id],
        |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                room: row.get(1)?,
                peer_endpoint_id: row.get(2)?,
                sender_endpoint_id: row.get(3)?,
                sender_display_name: row.get(4)?,
                body: row.get(5)?,
                direction: row.get(6)?,
                delivery_status: row.get(7)?,
                sent_at: row.get(8)?,
                received_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| format!("No se pudo leer mensaje: {error}"))?
    .ok_or_else(|| "El mensaje ya no existe.".to_string())
}

fn list_general_targets(app: &AppHandle) -> Result<Vec<String>, String> {
    let conn = open_db(app)?;
    let mut statement = conn
        .prepare(
            "SELECT endpoint_id FROM p2p_peers
             WHERE blocked_at IS NULL AND last_endpoint_addr IS NOT NULL AND last_endpoint_addr != ''
             ORDER BY endpoint_id",
        )
        .map_err(|error| format!("No se pudo preparar destinatarios generales: {error}"))?;
    let rows = statement
        .query_map([], |row| row.get(0))
        .map_err(|error| format!("No se pudo consultar destinatarios generales: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo leer destinatarios generales: {error}"))
}

fn valid_wire_message(message: &ChatWireMessage) -> bool {
    message.version == PROTOCOL_VERSION
        && Uuid::parse_str(&message.id).is_ok()
        && matches!(message.room.as_str(), "private" | "general")
        && (2..=64).contains(&message.sender_display_name.trim().chars().count())
        && !message.body.trim().is_empty()
        && message.body.chars().count() <= MAX_CHAT_BODY_CHARS
        && DateTimeCheck::valid(&message.sent_at)
}

fn validate_room(room: &str) -> Result<String, String> {
    let room = room.trim().to_ascii_lowercase();
    if matches!(room.as_str(), "private" | "general") {
        Ok(room)
    } else {
        Err("La sala de chat no es valida.".to_string())
    }
}

fn validate_body(body: &str) -> Result<String, String> {
    let body = body.trim();
    if body.is_empty() || body.chars().count() > MAX_CHAT_BODY_CHARS {
        return Err("El mensaje debe tener entre 1 y 4000 caracteres.".to_string());
    }
    Ok(body.to_string())
}

struct DateTimeCheck;

impl DateTimeCheck {
    fn valid(value: &str) -> bool {
        chrono::DateTime::parse_from_rfc3339(value).is_ok()
    }
}

fn stream_error(error: impl fmt::Display) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_messages_are_versioned_and_bounded() {
        let mut message = ChatWireMessage {
            version: PROTOCOL_VERSION,
            id: Uuid::new_v4().to_string(),
            room: "private".to_string(),
            sender_endpoint_id: "endpoint".to_string(),
            sender_display_name: "Rau Test".to_string(),
            body: "hola".to_string(),
            sent_at: timestamp(),
            endpoint_ticket: None,
        };
        assert!(valid_wire_message(&message));
        message.body = "x".repeat(MAX_CHAT_BODY_CHARS + 1);
        assert!(!valid_wire_message(&message));
        assert!(validate_room("general").is_ok());
        assert!(validate_room("public-internet").is_err());
    }
}
