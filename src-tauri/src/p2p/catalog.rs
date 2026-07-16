use super::{
    network, resolve_shared_file_for_peer, search_shared_files_for_peer, SharedFileSearchResult,
};
use chrono::Utc;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

pub(super) const CATALOG_ALPN: &[u8] = b"/rau/catalog/1";
pub(super) const FILE_ALPN: &[u8] = b"/rau/file/1";
const PROTOCOL_VERSION: u8 = 1;
const MAX_CATALOG_REQUEST_BYTES: usize = 16 * 1024;
const MAX_CATALOG_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILE_REQUEST_BYTES: usize = 8 * 1024;
const MAX_FILE_HEADER_BYTES: usize = 16 * 1024;
const FILE_BUFFER_BYTES: usize = 64 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(15);
const TRANSFER_EVENT: &str = "p2p-transfer-event";

#[derive(Clone)]
pub(super) struct CatalogProtocol {
    app: AppHandle,
    endpoint_id: String,
}

#[derive(Clone)]
pub(super) struct FileProtocol {
    app: AppHandle,
}

impl CatalogProtocol {
    pub(super) fn new(app: AppHandle, endpoint_id: String) -> Self {
        Self { app, endpoint_id }
    }
}

impl FileProtocol {
    pub(super) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl fmt::Debug for CatalogProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogProtocol")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for FileProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileProtocol")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CatalogRequest {
    version: u8,
    query: String,
    limit: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct CatalogResponse {
    version: u8,
    provider_endpoint_id: String,
    query: String,
    results: Vec<SharedFileSearchResult>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RemoteCatalogResponse {
    peer_endpoint_id: String,
    peer_display_name: String,
    query: String,
    results: Vec<SharedFileSearchResult>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileRequest {
    version: u8,
    share_id: String,
    file_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileResponseHeader {
    version: u8,
    ok: bool,
    error: Option<String>,
    name: Option<String>,
    size_bytes: u64,
    modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadResult {
    peer_endpoint_id: String,
    name: String,
    destination_path: String,
    size_bytes: u64,
    elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
struct TransferEvent {
    transfer_id: String,
    peer_endpoint_id: String,
    file_name: String,
    destination_path: String,
    received_bytes: u64,
    total_bytes: u64,
    status: String,
    message: String,
    occurred_at: String,
}

impl ProtocolHandler for CatalogProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let (mut send, mut recv) = connection.accept_bi().await?;
        let request_bytes = recv
            .read_to_end(MAX_CATALOG_REQUEST_BYTES)
            .await
            .map_err(stream_error)?;
        let request = serde_json::from_slice::<CatalogRequest>(&request_bytes).ok();
        let mut response = match request {
            Some(request) if valid_catalog_request(&request) => {
                let app = self.app.clone();
                let peer = remote_endpoint_id.clone();
                let query = request.query.clone();
                match tokio::task::spawn_blocking(move || {
                    search_shared_files_for_peer(&app, &peer, query, Some(request.limit))
                })
                .await
                {
                    Ok(Ok(response)) => CatalogResponse {
                        version: PROTOCOL_VERSION,
                        provider_endpoint_id: self.endpoint_id.clone(),
                        query: response.query,
                        results: response.results,
                        error: None,
                    },
                    Ok(Err(error)) => catalog_error(error),
                    Err(error) => catalog_error(format!("No se pudo consultar catalogo: {error}")),
                }
            }
            _ => catalog_error("Solicitud de catalogo invalida.".to_string()),
        };
        if response.provider_endpoint_id.is_empty() {
            response.provider_endpoint_id = self.endpoint_id.clone();
        }
        write_json(&mut send, &response).await?;
        send.finish()?;
        connection.closed().await;
        Ok(())
    }
}

impl ProtocolHandler for FileProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_endpoint_id = connection.remote_id().to_string();
        let (mut send, mut recv) = connection.accept_bi().await?;
        let request_bytes = recv
            .read_to_end(MAX_FILE_REQUEST_BYTES)
            .await
            .map_err(stream_error)?;
        let request = match serde_json::from_slice::<FileRequest>(&request_bytes) {
            Ok(request) if valid_file_request(&request) => request,
            _ => {
                write_file_header(
                    &mut send,
                    &file_error("Solicitud de archivo invalida.".to_string()),
                )
                .await?;
                send.finish()?;
                return Ok(());
            }
        };

        let app = self.app.clone();
        let peer = remote_endpoint_id.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            resolve_shared_file_for_peer(&app, &peer, &request.share_id, &request.file_id)
        })
        .await;
        let resolved = match resolved {
            Ok(Ok(file)) => file,
            Ok(Err(error)) => {
                write_file_header(&mut send, &file_error(error)).await?;
                send.finish()?;
                return Ok(());
            }
            Err(error) => {
                write_file_header(
                    &mut send,
                    &file_error(format!("No se pudo preparar archivo: {error}")),
                )
                .await?;
                send.finish()?;
                return Ok(());
            }
        };

        let mut file = match tokio::fs::File::open(&resolved.path).await {
            Ok(file) => file,
            Err(error) => {
                write_file_header(
                    &mut send,
                    &file_error(format!("No se pudo abrir archivo compartido: {error}")),
                )
                .await?;
                send.finish()?;
                return Ok(());
            }
        };
        let opened_metadata = file.metadata().await.map_err(stream_error)?;
        if !opened_metadata.is_file() {
            write_file_header(
                &mut send,
                &file_error("El recurso compartido ya no es un archivo regular.".to_string()),
            )
            .await?;
            send.finish()?;
            return Ok(());
        }
        let opened_size = opened_metadata.len();
        let header = FileResponseHeader {
            version: PROTOCOL_VERSION,
            ok: true,
            error: None,
            name: Some(resolved.name),
            size_bytes: opened_size,
            modified_ms: resolved.modified_ms,
        };
        write_file_header(&mut send, &header).await?;

        let mut buffer = vec![0u8; FILE_BUFFER_BYTES];
        loop {
            let read = file.read(&mut buffer).await.map_err(stream_error)?;
            if read == 0 {
                break;
            }
            send.write_all(&buffer[..read])
                .await
                .map_err(stream_error)?;
        }
        send.finish()?;
        connection.closed().await;
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn p2p_remote_search(
    app: AppHandle,
    peer_endpoint_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<RemoteCatalogResponse, String> {
    let limit = limit.unwrap_or(100).clamp(1, 200);
    let connection = network::connect_known_peer(&app, &peer_endpoint_id, CATALOG_ALPN).await?;
    let authenticated_peer = connection.remote_id().to_string();
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("No se pudo abrir catalogo remoto: {error}"))?;
    let request = CatalogRequest {
        version: PROTOCOL_VERSION,
        query: query.trim().to_string(),
        limit,
    };
    let bytes = serde_json::to_vec(&request)
        .map_err(|error| format!("No se pudo codificar busqueda remota: {error}"))?;
    send.write_all(&bytes)
        .await
        .map_err(|error| format!("No se pudo enviar busqueda remota: {error}"))?;
    send.finish()
        .map_err(|error| format!("No se pudo finalizar busqueda remota: {error}"))?;
    let response_bytes =
        tokio::time::timeout(HEADER_TIMEOUT, recv.read_to_end(MAX_CATALOG_RESPONSE_BYTES))
            .await
            .map_err(|_| "El catalogo remoto no respondio dentro de 15 segundos.".to_string())?
            .map_err(|error| format!("No se pudo leer catalogo remoto: {error}"))?;
    let response = serde_json::from_slice::<CatalogResponse>(&response_bytes)
        .map_err(|error| format!("El peer respondio un catalogo invalido: {error}"))?;
    if response.version != PROTOCOL_VERSION
        || response.provider_endpoint_id != authenticated_peer
        || response
            .results
            .iter()
            .any(|result| result.provider_endpoint_id != authenticated_peer)
    {
        return Err("El catalogo no coincide con la identidad autenticada del peer.".to_string());
    }
    if let Some(error) = response.error {
        return Err(error);
    }
    let peer_display_name = super::peer_display_name(&app, &authenticated_peer)?;
    connection.close(0u32.into(), b"rau catalog complete");
    Ok(RemoteCatalogResponse {
        peer_endpoint_id: authenticated_peer,
        peer_display_name,
        query: response.query,
        results: response.results,
    })
}

#[tauri::command]
pub(crate) async fn p2p_download_remote_file(
    app: AppHandle,
    peer_endpoint_id: String,
    share_id: String,
    file_id: String,
    destination_path: String,
) -> Result<DownloadResult, String> {
    if share_id.len() > 128 || file_id.len() > 128 {
        return Err("El identificador del archivo remoto no es valido.".to_string());
    }
    let destination = PathBuf::from(destination_path.trim());
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Selecciona un destino de archivo valido.".to_string())?;
    let parent = destination
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| "La carpeta de destino ya no existe.".to_string())?;
    let transfer_id = Uuid::new_v4().to_string();
    let temporary = parent.join(format!(".{file_name}.rau-part-{transfer_id}"));
    let backup = parent.join(format!(".{file_name}.rau-backup-{transfer_id}"));
    let started = Instant::now();

    let result = download_to_temporary(
        &app,
        &peer_endpoint_id,
        &share_id,
        &file_id,
        &destination,
        &temporary,
        &transfer_id,
    )
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    let (authenticated_peer, header) = result?;
    let had_existing_destination = destination.exists();
    if had_existing_destination {
        tokio::fs::rename(&destination, &backup)
            .await
            .map_err(|error| format!("No se pudo resguardar archivo de destino: {error}"))?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        if had_existing_destination {
            let _ = tokio::fs::rename(&backup, &destination).await;
        }
        return Err(format!("No se pudo completar descarga: {error}"));
    }
    if had_existing_destination {
        let _ = tokio::fs::remove_file(&backup).await;
    }
    let destination_path = destination.to_string_lossy().into_owned();
    emit_transfer(
        &app,
        TransferEvent {
            transfer_id,
            peer_endpoint_id: authenticated_peer.clone(),
            file_name: header.name.clone().unwrap_or_else(|| file_name.to_string()),
            destination_path: destination_path.clone(),
            received_bytes: header.size_bytes,
            total_bytes: header.size_bytes,
            status: "completed".to_string(),
            message: "Descarga P2P completada.".to_string(),
            occurred_at: timestamp(),
        },
    );
    Ok(DownloadResult {
        peer_endpoint_id: authenticated_peer,
        name: header.name.unwrap_or_else(|| file_name.to_string()),
        destination_path,
        size_bytes: header.size_bytes,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

async fn download_to_temporary(
    app: &AppHandle,
    peer_endpoint_id: &str,
    share_id: &str,
    file_id: &str,
    destination: &Path,
    temporary: &Path,
    transfer_id: &str,
) -> Result<(String, FileResponseHeader), String> {
    let connection = network::connect_known_peer(app, peer_endpoint_id, FILE_ALPN).await?;
    let authenticated_peer = connection.remote_id().to_string();
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("No se pudo abrir descarga P2P: {error}"))?;
    let request = FileRequest {
        version: PROTOCOL_VERSION,
        share_id: share_id.to_string(),
        file_id: file_id.to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)
        .map_err(|error| format!("No se pudo preparar descarga P2P: {error}"))?;
    send.write_all(&request_bytes)
        .await
        .map_err(|error| format!("No se pudo solicitar archivo P2P: {error}"))?;
    send.finish()
        .map_err(|error| format!("No se pudo finalizar solicitud P2P: {error}"))?;
    let header = read_file_header(&mut recv).await?;
    if !header.ok {
        return Err(header
            .error
            .unwrap_or_else(|| "El peer rechazo la descarga.".to_string()));
    }
    if header.version != PROTOCOL_VERSION {
        return Err("La version del protocolo de descarga no es compatible.".to_string());
    }
    if header.size_bytes > MAX_DOWNLOAD_BYTES {
        return Err("El archivo remoto excede el limite de descarga de 100 GB.".to_string());
    }

    let mut output = tokio::fs::File::create(temporary)
        .await
        .map_err(|error| format!("No se pudo crear archivo temporal: {error}"))?;
    let mut received = 0u64;
    let mut last_event = Instant::now();
    let mut buffer = vec![0u8; FILE_BUFFER_BYTES];
    while received < header.size_bytes {
        let remaining = header.size_bytes.saturating_sub(received);
        let capacity =
            usize::try_from(remaining.min(FILE_BUFFER_BYTES as u64)).unwrap_or(FILE_BUFFER_BYTES);
        let read = recv
            .read(&mut buffer[..capacity])
            .await
            .map_err(|error| format!("No se pudo recibir archivo P2P: {error}"))?
            .ok_or_else(|| "La descarga termino antes de recibir todos los bytes.".to_string())?;
        output
            .write_all(&buffer[..read])
            .await
            .map_err(|error| format!("No se pudo guardar descarga P2P: {error}"))?;
        received = received.saturating_add(read as u64);
        if last_event.elapsed() >= Duration::from_millis(250) {
            emit_transfer(
                app,
                TransferEvent {
                    transfer_id: transfer_id.to_string(),
                    peer_endpoint_id: authenticated_peer.clone(),
                    file_name: header.name.clone().unwrap_or_else(|| "archivo".to_string()),
                    destination_path: destination.to_string_lossy().into_owned(),
                    received_bytes: received,
                    total_bytes: header.size_bytes,
                    status: "downloading".to_string(),
                    message: "Recibiendo archivo P2P.".to_string(),
                    occurred_at: timestamp(),
                },
            );
            last_event = Instant::now();
        }
    }
    output
        .flush()
        .await
        .map_err(|error| format!("No se pudo finalizar archivo temporal: {error}"))?;
    output
        .sync_all()
        .await
        .map_err(|error| format!("No se pudo sincronizar archivo temporal: {error}"))?;
    connection.close(0u32.into(), b"rau file complete");
    Ok((authenticated_peer, header))
}

fn valid_catalog_request(request: &CatalogRequest) -> bool {
    request.version == PROTOCOL_VERSION
        && request.query.chars().count() <= 512
        && (1..=200).contains(&request.limit)
}

fn valid_file_request(request: &FileRequest) -> bool {
    request.version == PROTOCOL_VERSION
        && (1..=128).contains(&request.share_id.len())
        && (1..=128).contains(&request.file_id.len())
}

fn catalog_error(error: String) -> CatalogResponse {
    CatalogResponse {
        version: PROTOCOL_VERSION,
        provider_endpoint_id: String::new(),
        query: String::new(),
        results: Vec::new(),
        error: Some(error),
    }
}

fn file_error(error: String) -> FileResponseHeader {
    FileResponseHeader {
        version: PROTOCOL_VERSION,
        ok: false,
        error: Some(error),
        name: None,
        size_bytes: 0,
        modified_ms: None,
    }
}

async fn write_json(
    send: &mut iroh::endpoint::SendStream,
    value: &impl Serialize,
) -> Result<(), AcceptError> {
    let bytes = serde_json::to_vec(value).map_err(stream_error)?;
    send.write_all(&bytes).await.map_err(stream_error)?;
    Ok(())
}

async fn write_file_header(
    send: &mut iroh::endpoint::SendStream,
    header: &FileResponseHeader,
) -> Result<(), AcceptError> {
    let bytes = serde_json::to_vec(header).map_err(stream_error)?;
    let length = u32::try_from(bytes.len()).map_err(stream_error)?;
    send.write_all(&length.to_be_bytes())
        .await
        .map_err(stream_error)?;
    send.write_all(&bytes).await.map_err(stream_error)?;
    Ok(())
}

async fn read_file_header(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<FileResponseHeader, String> {
    let mut length_bytes = [0u8; 4];
    tokio::time::timeout(HEADER_TIMEOUT, recv.read_exact(&mut length_bytes))
        .await
        .map_err(|_| "El peer no respondio la descarga dentro de 15 segundos.".to_string())?
        .map_err(|error| format!("No se pudo leer cabecera de descarga: {error}"))?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FILE_HEADER_BYTES {
        return Err("La cabecera de descarga excede el limite permitido.".to_string());
    }
    let mut bytes = vec![0u8; length];
    recv.read_exact(&mut bytes)
        .await
        .map_err(|error| format!("No se pudo leer cabecera de descarga: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("El peer respondio una cabecera invalida: {error}"))
}

fn emit_transfer(app: &AppHandle, event: TransferEvent) {
    let _ = app.emit(TRANSFER_EVENT, event);
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
    fn catalog_and_file_requests_are_bounded() {
        assert!(valid_catalog_request(&CatalogRequest {
            version: PROTOCOL_VERSION,
            query: "house".to_string(),
            limit: 100,
        }));
        assert!(!valid_catalog_request(&CatalogRequest {
            version: PROTOCOL_VERSION,
            query: "x".repeat(513),
            limit: 100,
        }));
        assert!(valid_file_request(&FileRequest {
            version: PROTOCOL_VERSION,
            share_id: Uuid::new_v4().to_string(),
            file_id: Uuid::new_v4().to_string(),
        }));
    }
}
