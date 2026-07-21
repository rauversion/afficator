use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{Ed25519KeyPair, KeyPair};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};
use zeroize::Zeroizing;

pub(crate) mod catalog;
pub(crate) mod chat;
pub(crate) mod network;
pub(crate) mod stream;
const DB_FILE: &str = "aifficator.sqlite3";
const IDENTITY_ROW_ID: i64 = 1;
const IDENTITY_CIPHER_VERSION: i64 = 1;
const IDENTITY_SECRET_LEN: usize = 32;
const IDENTITY_SALT_LEN: usize = 16;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_ITERATIONS: u32 = 3;
const KDF_PARALLELISM: u32 = 1;
const MAX_SHARED_FILES: usize = 100_000;
const MAX_SEARCH_TOKENS: usize = 8;
const MAX_SEARCH_RESULTS: usize = 200;
const PRESENCE_FRESH_FOR_SECONDS: i64 = 120;

#[derive(Debug)]
struct UnlockedIdentity {
    endpoint_id: String,
    display_name: String,
    _secret: Zeroizing<[u8; IDENTITY_SECRET_LEN]>,
}

pub(super) struct NetworkIdentity {
    pub endpoint_id: String,
    pub display_name: String,
    pub secret: Zeroizing<[u8; IDENTITY_SECRET_LEN]>,
}

#[derive(Debug)]
struct StoredIdentity {
    display_name: String,
    endpoint_id: String,
    secret_ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    salt: Vec<u8>,
    kdf_memory_kib: u32,
    kdf_iterations: u32,
    kdf_parallelism: u32,
}

#[derive(Debug)]
struct IndexedFile {
    file_id: String,
    relative_path: String,
    name: String,
    extension: String,
    size_bytes: u64,
    modified_ms: Option<i64>,
}

#[derive(Debug)]
struct FolderIndex {
    files: Vec<IndexedFile>,
    total_size_bytes: u64,
    skipped_entries: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct IdentityStatus {
    configured: bool,
    unlocked: bool,
    display_name: Option<String>,
    endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PeerSummary {
    endpoint_id: String,
    display_name: String,
    trust_state: String,
    presence_status: String,
    last_seen_at: Option<String>,
    can_connect: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SharedFolder {
    id: String,
    name: String,
    root_path: String,
    visibility: String,
    enabled: bool,
    file_count: u64,
    total_size_bytes: u64,
    skipped_entries: u64,
    last_indexed_at: String,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SharedFileSearchResult {
    pub(super) provider_endpoint_id: String,
    pub(super) share_id: String,
    pub(super) share_name: String,
    pub(super) file_id: String,
    pub(super) name: String,
    pub(super) relative_path: String,
    pub(super) extension: String,
    pub(super) size_bytes: u64,
    pub(super) modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SharedFileSearchResponse {
    pub(super) query: String,
    pub(super) results: Vec<SharedFileSearchResult>,
}

pub(super) struct ResolvedSharedFile {
    pub name: String,
    pub path: PathBuf,
    pub modified_ms: Option<i64>,
}

fn identity_session() -> &'static Mutex<Option<UnlockedIdentity>> {
    static SESSION: OnceLock<Mutex<Option<UnlockedIdentity>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

#[tauri::command]
pub(crate) fn p2p_identity_status(app: AppHandle) -> Result<IdentityStatus, String> {
    let conn = open_db(&app)?;
    let stored = load_identity(&conn)?;
    let unlocked_endpoint = identity_session()
        .lock()
        .map_err(|_| "No se pudo leer la identidad P2P de esta sesion.".to_string())?
        .as_ref()
        .map(|identity| identity.endpoint_id.clone());

    Ok(match stored {
        Some(identity) => IdentityStatus {
            configured: true,
            unlocked: unlocked_endpoint.as_deref() == Some(identity.endpoint_id.as_str()),
            display_name: Some(identity.display_name),
            endpoint_id: Some(identity.endpoint_id),
        },
        None => IdentityStatus {
            configured: false,
            unlocked: false,
            display_name: None,
            endpoint_id: None,
        },
    })
}

#[tauri::command]
pub(crate) fn p2p_create_identity(
    app: AppHandle,
    display_name: String,
    password: String,
) -> Result<IdentityStatus, String> {
    let display_name = validate_display_name(&display_name)?;
    validate_password(&password)?;

    let mut conn = open_db(&app)?;
    if load_identity(&conn)?.is_some() {
        return Err("Ya existe una identidad P2P en este dispositivo.".to_string());
    }

    let secret = random_bytes::<IDENTITY_SECRET_LEN>()?;
    let endpoint_id = iroh::SecretKey::from_bytes(&secret).public().to_string();
    let salt = random_bytes::<IDENTITY_SALT_LEN>()?;
    let wrapping_key = derive_wrapping_key(
        &password,
        &salt,
        KDF_MEMORY_KIB,
        KDF_ITERATIONS,
        KDF_PARALLELISM,
    )?;
    let nonce = random_bytes::<NONCE_LEN>()?;
    let ciphertext = seal_identity_secret(&secret, &wrapping_key, &endpoint_id, nonce)?;
    let now = timestamp();

    let transaction = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar guardado de identidad P2P: {error}"))?;
    transaction
        .execute(
            "INSERT INTO p2p_identity (
               id, display_name, endpoint_id, secret_ciphertext, nonce, salt,
               kdf_memory_kib, kdf_iterations, kdf_parallelism, cipher_version,
               created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                IDENTITY_ROW_ID,
                display_name,
                endpoint_id,
                ciphertext,
                nonce.as_slice(),
                salt.as_slice(),
                KDF_MEMORY_KIB,
                KDF_ITERATIONS,
                KDF_PARALLELISM,
                IDENTITY_CIPHER_VERSION,
                now,
            ],
        )
        .map_err(|error| format!("No se pudo guardar identidad P2P cifrada: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("No se pudo confirmar identidad P2P: {error}"))?;

    *identity_session()
        .lock()
        .map_err(|_| "No se pudo desbloquear la identidad P2P creada.".to_string())? =
        Some(UnlockedIdentity {
            endpoint_id,
            display_name,
            _secret: Zeroizing::new(secret),
        });

    p2p_identity_status(app)
}

#[tauri::command]
pub(crate) fn p2p_unlock_identity(
    app: AppHandle,
    password: String,
) -> Result<IdentityStatus, String> {
    let conn = open_db(&app)?;
    let identity =
        load_identity(&conn)?.ok_or_else(|| "Primero crea una identidad P2P.".to_string())?;
    let wrapping_key = derive_wrapping_key(
        &password,
        &identity.salt,
        identity.kdf_memory_kib,
        identity.kdf_iterations,
        identity.kdf_parallelism,
    )?;
    let nonce: [u8; NONCE_LEN] = identity
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| "La identidad P2P tiene un nonce invalido.".to_string())?;
    let secret = open_identity_secret(
        &identity.secret_ciphertext,
        &wrapping_key,
        &identity.endpoint_id,
        nonce,
    )
    .map_err(|_| "No se pudo desbloquear la identidad. Revisa la contraseña.".to_string())?;
    let derived_endpoint_id = iroh::SecretKey::from_bytes(&secret).public().to_string();
    let endpoint_id = migrate_legacy_endpoint_id(
        &conn,
        &identity,
        &secret,
        &wrapping_key,
        &derived_endpoint_id,
    )?;

    *identity_session()
        .lock()
        .map_err(|_| "No se pudo actualizar la identidad P2P de esta sesion.".to_string())? =
        Some(UnlockedIdentity {
            endpoint_id,
            display_name: identity.display_name,
            _secret: Zeroizing::new(secret),
        });

    p2p_identity_status(app)
}

#[tauri::command]
pub(crate) async fn p2p_lock_identity(app: AppHandle) -> Result<IdentityStatus, String> {
    network::stop_if_running(app.clone()).await?;
    *identity_session()
        .lock()
        .map_err(|_| "No se pudo bloquear la identidad P2P.".to_string())? = None;
    p2p_identity_status(app)
}

#[tauri::command]
pub(crate) fn p2p_list_peers(app: AppHandle) -> Result<Vec<PeerSummary>, String> {
    let conn = open_db(&app)?;
    let mut statement = conn
        .prepare(
            "SELECT endpoint_id, display_name, trust_state, presence_status, last_seen_at,
                    CASE WHEN last_endpoint_addr IS NOT NULL AND last_endpoint_addr != '' THEN 1 ELSE 0 END
             FROM p2p_peers
             ORDER BY lower(display_name), endpoint_id",
        )
        .map_err(|error| format!("No se pudo preparar lista de contactos P2P: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(PeerSummary {
                endpoint_id: row.get(0)?,
                display_name: row.get(1)?,
                trust_state: row.get(2)?,
                presence_status: row.get(3)?,
                last_seen_at: row.get(4)?,
                can_connect: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|error| format!("No se pudo consultar contactos P2P: {error}"))?;

    let mut peers = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo leer contacto P2P: {error}"))?;
    let now = Utc::now();
    for peer in &mut peers {
        if peer.presence_status == "online"
            && !presence_observation_is_fresh(peer.last_seen_at.as_deref(), now)
        {
            peer.presence_status = "offline".to_string();
        }
    }
    Ok(peers)
}

fn presence_observation_is_fresh(value: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Ok(observed_at) = DateTime::parse_from_rfc3339(value) else {
        return false;
    };
    let age = now.signed_duration_since(observed_at.with_timezone(&Utc));
    age.num_seconds() >= -60 && age.num_seconds() <= PRESENCE_FRESH_FOR_SECONDS
}

#[tauri::command]
pub(crate) fn p2p_list_shares(app: AppHandle) -> Result<Vec<SharedFolder>, String> {
    let conn = open_db(&app)?;
    list_shares(&conn)
}

#[tauri::command]
pub(crate) fn p2p_add_share(
    app: AppHandle,
    path: String,
    name: String,
    visibility: String,
) -> Result<SharedFolder, String> {
    require_unlocked_identity()?;
    let name = validate_share_name(&name)?;
    let visibility = validate_visibility(&visibility)?;
    let root = canonical_shared_root(&path)?;
    let share_id = Uuid::new_v4().to_string();
    let folder_index = index_shared_folder(&root, &share_id)?;
    let now = timestamp();
    let root_path = root.to_string_lossy().into_owned();

    let mut conn = open_db(&app)?;
    let transaction = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar guardado de carpeta: {error}"))?;
    transaction
        .execute(
            "INSERT INTO p2p_shares (
               id, name, root_path, visibility, enabled, file_count,
               total_size_bytes, skipped_entries, last_indexed_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?8, ?8)",
            params![
                share_id,
                name,
                root_path,
                visibility,
                as_i64(folder_index.files.len() as u64, "cantidad de archivos")?,
                as_i64(folder_index.total_size_bytes, "tamaño compartido")?,
                as_i64(folder_index.skipped_entries, "entradas omitidas")?,
                now,
            ],
        )
        .map_err(|error| format!("No se pudo guardar carpeta compartida: {error}"))?;
    insert_indexed_files(&transaction, &share_id, &folder_index.files)?;
    transaction
        .commit()
        .map_err(|error| format!("No se pudo confirmar carpeta compartida: {error}"))?;

    load_share(&conn, &share_id)?
        .ok_or_else(|| "La carpeta compartida no se pudo volver a cargar.".to_string())
}

#[tauri::command]
pub(crate) fn p2p_reindex_share(app: AppHandle, share_id: String) -> Result<SharedFolder, String> {
    require_unlocked_identity()?;
    let mut conn = open_db(&app)?;
    let root_path: String = conn
        .query_row(
            "SELECT root_path FROM p2p_shares WHERE id = ?1",
            params![share_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer carpeta compartida: {error}"))?
        .ok_or_else(|| "La carpeta compartida ya no existe.".to_string())?;
    let root = canonical_shared_root(&root_path)?;
    let folder_index = index_shared_folder(&root, &share_id)?;
    let now = timestamp();

    let transaction = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar reindexacion: {error}"))?;
    transaction
        .execute(
            "DELETE FROM p2p_shared_files WHERE share_id = ?1",
            params![share_id],
        )
        .map_err(|error| format!("No se pudo limpiar indice anterior: {error}"))?;
    insert_indexed_files(&transaction, &share_id, &folder_index.files)?;
    transaction
        .execute(
            "UPDATE p2p_shares
             SET file_count = ?2, total_size_bytes = ?3, skipped_entries = ?4,
                 last_indexed_at = ?5, updated_at = ?5
             WHERE id = ?1",
            params![
                share_id,
                as_i64(folder_index.files.len() as u64, "cantidad de archivos")?,
                as_i64(folder_index.total_size_bytes, "tamaño compartido")?,
                as_i64(folder_index.skipped_entries, "entradas omitidas")?,
                now,
            ],
        )
        .map_err(|error| format!("No se pudo actualizar carpeta compartida: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("No se pudo confirmar reindexacion: {error}"))?;

    load_share(&conn, &share_id)?
        .ok_or_else(|| "La carpeta reindexada no se pudo volver a cargar.".to_string())
}

#[tauri::command]
pub(crate) fn p2p_set_share_enabled(
    app: AppHandle,
    share_id: String,
    enabled: bool,
) -> Result<SharedFolder, String> {
    require_unlocked_identity()?;
    let conn = open_db(&app)?;
    let changed = conn
        .execute(
            "UPDATE p2p_shares SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![share_id, enabled, timestamp()],
        )
        .map_err(|error| format!("No se pudo cambiar estado de carpeta: {error}"))?;
    if changed == 0 {
        return Err("La carpeta compartida ya no existe.".to_string());
    }
    load_share(&conn, &share_id)?
        .ok_or_else(|| "La carpeta actualizada no se pudo volver a cargar.".to_string())
}

#[tauri::command]
pub(crate) fn p2p_remove_share(app: AppHandle, share_id: String) -> Result<(), String> {
    require_unlocked_identity()?;
    let conn = open_db(&app)?;
    let changed = conn
        .execute("DELETE FROM p2p_shares WHERE id = ?1", params![share_id])
        .map_err(|error| format!("No se pudo quitar carpeta compartida: {error}"))?;
    if changed == 0 {
        return Err("La carpeta compartida ya no existe.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn p2p_search_shared_files(
    app: AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<SharedFileSearchResponse, String> {
    let endpoint_id = require_unlocked_identity()?;
    let conn = open_db(&app)?;
    search_shared_files(&conn, &endpoint_id, query, limit, false)
}

pub(super) fn search_shared_files_for_peer(
    app: &AppHandle,
    remote_endpoint_id: &str,
    query: String,
    limit: Option<usize>,
) -> Result<SharedFileSearchResponse, String> {
    let endpoint_id = require_unlocked_identity()?;
    let conn = open_db(app)?;
    require_authorized_peer(&conn, remote_endpoint_id)?;
    mark_peer_seen_in_connection(&conn, remote_endpoint_id)?;
    search_shared_files(&conn, &endpoint_id, query, limit, true)
}

fn search_shared_files(
    conn: &Connection,
    endpoint_id: &str,
    query: String,
    limit: Option<usize>,
    remote_request: bool,
) -> Result<SharedFileSearchResponse, String> {
    let query = query.trim().to_string();
    let tokens = query
        .split_whitespace()
        .take(MAX_SEARCH_TOKENS)
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let limit = limit.unwrap_or(50).clamp(1, MAX_SEARCH_RESULTS);

    let mut sql = String::from(
        "SELECT f.share_id, s.name, f.id, f.name, f.relative_path, f.extension,
                f.size_bytes, f.modified_ms
         FROM p2p_shared_files f
         JOIN p2p_shares s ON s.id = f.share_id
         WHERE s.enabled = 1",
    );
    if remote_request {
        sql.push_str(" AND s.visibility IN ('contacts', 'community', 'ticket')");
    }
    let mut values = Vec::new();
    for token in tokens {
        sql.push_str(
            " AND (lower(f.name) LIKE ? ESCAPE '\\' OR lower(f.relative_path) LIKE ? ESCAPE '\\' OR lower(f.extension) = ?)",
        );
        let pattern = format!("%{}%", escape_like(&token));
        values.push(pattern.clone());
        values.push(pattern);
        values.push(token);
    }
    sql.push_str(" ORDER BY lower(f.name), lower(f.relative_path)");
    sql.push_str(&format!(" LIMIT {limit}"));

    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar busqueda compartida: {error}"))?;
    let rows = statement
        .query_map(params_from_iter(values.iter()), |row| {
            Ok(SharedFileSearchResult {
                provider_endpoint_id: endpoint_id.to_string(),
                share_id: row.get(0)?,
                share_name: row.get(1)?,
                file_id: row.get(2)?,
                name: row.get(3)?,
                relative_path: row.get(4)?,
                extension: row.get(5)?,
                size_bytes: from_i64(row.get(6)?),
                modified_ms: row.get(7)?,
            })
        })
        .map_err(|error| format!("No se pudo ejecutar busqueda compartida: {error}"))?;
    let results = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo leer resultado compartido: {error}"))?;

    Ok(SharedFileSearchResponse { query, results })
}

pub(super) fn require_authorized_peer(conn: &Connection, endpoint_id: &str) -> Result<(), String> {
    let authorized = conn
        .query_row(
            "SELECT blocked_at IS NULL FROM p2p_peers WHERE endpoint_id = ?1",
            params![endpoint_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo verificar autorizacion del peer: {error}"))?
        .unwrap_or(false);
    if authorized {
        Ok(())
    } else {
        Err("El peer remoto no es un contacto P2P autorizado.".to_string())
    }
}

pub(super) fn mark_peer_seen(app: &AppHandle, endpoint_id: &str) -> Result<(), String> {
    let conn = open_db(app)?;
    require_authorized_peer(&conn, endpoint_id)?;
    mark_peer_seen_in_connection(&conn, endpoint_id)
}

fn mark_peer_seen_in_connection(conn: &Connection, endpoint_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE p2p_peers SET presence_status = 'online', last_seen_at = ?2
         WHERE endpoint_id = ?1 AND blocked_at IS NULL",
        params![endpoint_id, timestamp()],
    )
    .map_err(|error| format!("No se pudo actualizar presencia del peer: {error}"))?;
    Ok(())
}

pub(super) fn peer_endpoint_ticket(app: &AppHandle, endpoint_id: &str) -> Result<String, String> {
    let conn = open_db(app)?;
    require_authorized_peer(&conn, endpoint_id)?;
    conn.query_row(
        "SELECT last_endpoint_addr FROM p2p_peers WHERE endpoint_id = ?1",
        params![endpoint_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(|error| format!("No se pudo leer ticket del peer: {error}"))?
    .flatten()
    .filter(|ticket| !ticket.trim().is_empty())
    .ok_or_else(|| {
        "Ese peer no publico un ticket de retorno. Vuelve a probar la conexion en ambos dispositivos."
            .to_string()
    })
}

pub(super) fn peer_display_name(app: &AppHandle, endpoint_id: &str) -> Result<String, String> {
    let conn = open_db(app)?;
    conn.query_row(
        "SELECT display_name FROM p2p_peers WHERE endpoint_id = ?1",
        params![endpoint_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("No se pudo leer nombre del peer: {error}"))?
    .ok_or_else(|| "El peer ya no existe en la lista local.".to_string())
}

pub(super) fn resolve_shared_file_for_peer(
    app: &AppHandle,
    remote_endpoint_id: &str,
    share_id: &str,
    file_id: &str,
) -> Result<ResolvedSharedFile, String> {
    let conn = open_db(app)?;
    require_authorized_peer(&conn, remote_endpoint_id)?;
    mark_peer_seen_in_connection(&conn, remote_endpoint_id)?;
    let record = conn
        .query_row(
            "SELECT s.root_path, s.visibility, f.relative_path, f.name
             FROM p2p_shared_files f
             JOIN p2p_shares s ON s.id = f.share_id
             WHERE s.id = ?1 AND f.id = ?2 AND s.enabled = 1",
            params![share_id, file_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("No se pudo resolver archivo compartido: {error}"))?
        .ok_or_else(|| "El archivo compartido ya no esta disponible.".to_string())?;
    if !matches!(record.1.as_str(), "contacts" | "community" | "ticket") {
        return Err("La carpeta no autoriza descargas para este peer.".to_string());
    }

    let root = canonical_shared_root(&record.0)?;
    let path = safe_shared_file_path(&root, &record.2)?;
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("No se pudo leer archivo compartido: {error}"))?;
    if !metadata.is_file() {
        return Err("El archivo compartido ya no es un archivo regular.".to_string());
    }
    Ok(ResolvedSharedFile {
        name: record.3,
        path,
        modified_ms: metadata.modified().ok().and_then(system_time_millis),
    })
}

fn safe_shared_file_path(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("La ruta virtual del archivo no es valida.".to_string());
    }

    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err("La ruta virtual del archivo no es valida.".to_string());
        };
        candidate.push(part);
        let metadata = std::fs::symlink_metadata(&candidate)
            .map_err(|error| format!("No se pudo revalidar ruta compartida: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("La ruta compartida contiene un enlace simbolico.".to_string());
        }
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("No se pudo resolver archivo compartido: {error}"))?;
    if !canonical.starts_with(root) {
        return Err("El archivo solicitado escapa de la carpeta compartida.".to_string());
    }
    Ok(canonical)
}

pub(super) fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join(DB_FILE))
        .map_err(|error| format!("No se pudo abrir SQLite P2P: {error}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| format!("No se pudo habilitar integridad P2P: {error}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS p2p_identity (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          display_name TEXT NOT NULL,
          endpoint_id TEXT NOT NULL UNIQUE,
          secret_ciphertext BLOB NOT NULL,
          nonce BLOB NOT NULL,
          salt BLOB NOT NULL,
          kdf_memory_kib INTEGER NOT NULL,
          kdf_iterations INTEGER NOT NULL,
          kdf_parallelism INTEGER NOT NULL,
          cipher_version INTEGER NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS p2p_peers (
          endpoint_id TEXT PRIMARY KEY,
          display_name TEXT NOT NULL,
          trust_state TEXT NOT NULL DEFAULT 'paired',
          presence_status TEXT NOT NULL DEFAULT 'unknown',
          last_endpoint_addr TEXT,
          last_seen_at TEXT,
          paired_at TEXT NOT NULL,
          blocked_at TEXT
        );

        CREATE TABLE IF NOT EXISTS p2p_shares (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          root_path TEXT NOT NULL UNIQUE,
          visibility TEXT NOT NULL CHECK (visibility IN ('contacts', 'selected_contacts', 'community', 'ticket')),
          enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
          file_count INTEGER NOT NULL DEFAULT 0,
          total_size_bytes INTEGER NOT NULL DEFAULT 0,
          skipped_entries INTEGER NOT NULL DEFAULT 0,
          last_indexed_at TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS p2p_shared_files (
          id TEXT PRIMARY KEY,
          share_id TEXT NOT NULL REFERENCES p2p_shares(id) ON DELETE CASCADE,
          relative_path TEXT NOT NULL,
          name TEXT NOT NULL,
          extension TEXT NOT NULL,
          size_bytes INTEGER NOT NULL,
          modified_ms INTEGER,
          content_hash TEXT,
          UNIQUE (share_id, relative_path)
        );

        CREATE TABLE IF NOT EXISTS p2p_chat_messages (
          id TEXT PRIMARY KEY,
          room TEXT NOT NULL CHECK (room IN ('private', 'general')),
          peer_endpoint_id TEXT NOT NULL,
          sender_endpoint_id TEXT NOT NULL,
          sender_display_name TEXT NOT NULL,
          body TEXT NOT NULL,
          direction TEXT NOT NULL CHECK (direction IN ('incoming', 'outgoing')),
          delivery_status TEXT NOT NULL CHECK (delivery_status IN ('pending', 'delivered', 'partial', 'failed')),
          sent_at TEXT NOT NULL,
          received_at TEXT,
          created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS stream_favorites (
          id TEXT PRIMARY KEY,
          endpoint_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS p2p_shared_files_share_name_idx
          ON p2p_shared_files(share_id, name);
        CREATE INDEX IF NOT EXISTS p2p_peers_presence_idx
          ON p2p_peers(presence_status, display_name);
        CREATE INDEX IF NOT EXISTS p2p_chat_room_peer_sent_idx
          ON p2p_chat_messages(room, peer_endpoint_id, sent_at);
        ",
    )
    .map_err(|error| format!("No se pudo inicializar esquema P2P: {error}"))
}

fn load_identity(conn: &Connection) -> Result<Option<StoredIdentity>, String> {
    conn.query_row(
        "SELECT display_name, endpoint_id, secret_ciphertext, nonce, salt,
                kdf_memory_kib, kdf_iterations, kdf_parallelism, cipher_version
         FROM p2p_identity WHERE id = ?1",
        params![IDENTITY_ROW_ID],
        |row| {
            let cipher_version: i64 = row.get(8)?;
            if cipher_version != IDENTITY_CIPHER_VERSION {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok(StoredIdentity {
                display_name: row.get(0)?,
                endpoint_id: row.get(1)?,
                secret_ciphertext: row.get(2)?,
                nonce: row.get(3)?,
                salt: row.get(4)?,
                kdf_memory_kib: row.get(5)?,
                kdf_iterations: row.get(6)?,
                kdf_parallelism: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|error| format!("No se pudo leer identidad P2P: {error}"))
}

fn migrate_legacy_endpoint_id(
    conn: &Connection,
    identity: &StoredIdentity,
    secret: &[u8; IDENTITY_SECRET_LEN],
    wrapping_key: &[u8; IDENTITY_SECRET_LEN],
    canonical_endpoint_id: &str,
) -> Result<String, String> {
    if identity.endpoint_id == canonical_endpoint_id {
        return Ok(canonical_endpoint_id.to_string());
    }

    let key_pair = Ed25519KeyPair::from_seed_unchecked(secret)
        .map_err(|_| "La identidad P2P descifrada es invalida.".to_string())?;
    let legacy_endpoint_id = URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref());
    if identity.endpoint_id != legacy_endpoint_id {
        return Err("La identidad P2P no coincide con su clave publica.".to_string());
    }

    let nonce = random_bytes::<NONCE_LEN>()?;
    let ciphertext = seal_identity_secret(secret, wrapping_key, canonical_endpoint_id, nonce)?;
    conn.execute(
        "UPDATE p2p_identity
         SET endpoint_id = ?2, secret_ciphertext = ?3, nonce = ?4, updated_at = ?5
         WHERE id = ?1",
        params![
            IDENTITY_ROW_ID,
            canonical_endpoint_id,
            ciphertext,
            nonce.as_slice(),
            timestamp(),
        ],
    )
    .map_err(|error| format!("No se pudo migrar identidad P2P para Iroh: {error}"))?;

    Ok(canonical_endpoint_id.to_string())
}

fn list_shares(conn: &Connection) -> Result<Vec<SharedFolder>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, root_path, visibility, enabled, file_count,
                    total_size_bytes, skipped_entries, last_indexed_at, created_at
             FROM p2p_shares
             ORDER BY lower(name), created_at",
        )
        .map_err(|error| format!("No se pudo preparar lista de carpetas: {error}"))?;
    let rows = statement
        .query_map([], map_share)
        .map_err(|error| format!("No se pudo consultar carpetas compartidas: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo leer carpeta compartida: {error}"))
}

fn load_share(conn: &Connection, share_id: &str) -> Result<Option<SharedFolder>, String> {
    conn.query_row(
        "SELECT id, name, root_path, visibility, enabled, file_count,
                total_size_bytes, skipped_entries, last_indexed_at, created_at
         FROM p2p_shares WHERE id = ?1",
        params![share_id],
        map_share,
    )
    .optional()
    .map_err(|error| format!("No se pudo cargar carpeta compartida: {error}"))
}

fn map_share(row: &rusqlite::Row<'_>) -> rusqlite::Result<SharedFolder> {
    Ok(SharedFolder {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        visibility: row.get(3)?,
        enabled: row.get(4)?,
        file_count: from_i64(row.get(5)?),
        total_size_bytes: from_i64(row.get(6)?),
        skipped_entries: from_i64(row.get(7)?),
        last_indexed_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn insert_indexed_files(
    transaction: &rusqlite::Transaction<'_>,
    share_id: &str,
    files: &[IndexedFile],
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO p2p_shared_files (
               id, share_id, relative_path, name, extension, size_bytes, modified_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(|error| format!("No se pudo preparar indice compartido: {error}"))?;
    for file in files {
        statement
            .execute(params![
                file.file_id,
                share_id,
                file.relative_path,
                file.name,
                file.extension,
                as_i64(file.size_bytes, "tamaño de archivo")?,
                file.modified_ms,
            ])
            .map_err(|error| format!("No se pudo indexar {}: {error}", file.relative_path))?;
    }
    Ok(())
}

fn index_shared_folder(root: &Path, share_id: &str) -> Result<FolderIndex, String> {
    let mut files = Vec::new();
    let mut total_size_bytes = 0u64;
    let mut skipped_entries = 0u64;
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_hidden_entry(root, entry));

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                skipped_entries = skipped_entries.saturating_add(1);
                continue;
            }
        };
        if entry.path() == root || !entry.file_type().is_file() || entry.file_type().is_symlink() {
            continue;
        }
        if files.len() >= MAX_SHARED_FILES {
            return Err(format!(
                "La carpeta supera el limite inicial de {MAX_SHARED_FILES} archivos compartidos."
            ));
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped_entries = skipped_entries.saturating_add(1);
                continue;
            }
        };
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| "Un archivo indexado escapa de la carpeta compartida.".to_string())?;
        let relative_path = normalize_relative_path(relative);
        let name = entry.file_name().to_string_lossy().into_owned();
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let size_bytes = metadata.len();
        total_size_bytes = total_size_bytes
            .checked_add(size_bytes)
            .ok_or_else(|| "El tamaño total compartido excede el limite soportado.".to_string())?;
        files.push(IndexedFile {
            file_id: stable_file_id(share_id, &relative_path),
            relative_path,
            name,
            extension,
            size_bytes,
            modified_ms: metadata.modified().ok().and_then(system_time_millis),
        });
    }

    Ok(FolderIndex {
        files,
        total_size_bytes,
        skipped_entries,
    })
}

fn canonical_shared_root(path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Selecciona una carpeta para compartir.".to_string());
    }
    let root = std::fs::canonicalize(path)
        .map_err(|error| format!("No se pudo abrir carpeta compartida: {error}"))?;
    if !root.is_dir() {
        return Err("La ruta seleccionada no es una carpeta.".to_string());
    }
    Ok(root)
}

fn is_hidden_entry(root: &Path, entry: &DirEntry) -> bool {
    if entry.path() == root {
        return false;
    }
    if entry
        .file_name()
        .to_str()
        .is_some_and(|name| name.starts_with('.'))
    {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        return entry
            .metadata()
            .ok()
            .is_some_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0);
    }

    #[cfg(not(windows))]
    false
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn stable_file_id(share_id: &str, relative_path: &str) -> String {
    let mut input = Vec::with_capacity(share_id.len() + relative_path.len() + 1);
    input.extend_from_slice(share_id.as_bytes());
    input.push(0);
    input.extend_from_slice(relative_path.as_bytes());
    URL_SAFE_NO_PAD.encode(digest(&SHA256, &input).as_ref())
}

fn validate_display_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    let length = value.chars().count();
    if !(2..=64).contains(&length) {
        return Err("El nombre P2P debe tener entre 2 y 64 caracteres.".to_string());
    }
    Ok(value.to_string())
}

fn validate_password(value: &str) -> Result<(), String> {
    if value.chars().count() < 10 {
        return Err("La contraseña P2P debe tener al menos 10 caracteres.".to_string());
    }
    Ok(())
}

fn validate_share_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    let length = value.chars().count();
    if !(1..=80).contains(&length) {
        return Err("El nombre de la carpeta debe tener entre 1 y 80 caracteres.".to_string());
    }
    Ok(value.to_string())
}

fn validate_visibility(value: &str) -> Result<String, String> {
    match value.trim() {
        "contacts" | "selected_contacts" | "community" | "ticket" => Ok(value.trim().to_string()),
        _ => Err("Visibilidad de carpeta no soportada.".to_string()),
    }
}

fn derive_wrapping_key(
    password: &str,
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<Zeroizing<[u8; IDENTITY_SECRET_LEN]>, String> {
    let params = Params::new(
        memory_kib,
        iterations,
        parallelism,
        Some(IDENTITY_SECRET_LEN),
    )
    .map_err(|error| format!("Parametros Argon2id invalidos: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = Zeroizing::new([0u8; IDENTITY_SECRET_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, output.as_mut())
        .map_err(|error| format!("No se pudo derivar clave de identidad: {error}"))?;
    Ok(output)
}

fn seal_identity_secret(
    secret: &[u8; IDENTITY_SECRET_LEN],
    wrapping_key: &[u8; IDENTITY_SECRET_LEN],
    endpoint_id: &str,
    nonce_bytes: [u8; NONCE_LEN],
) -> Result<Vec<u8>, String> {
    let key = aead_key(wrapping_key)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = secret.to_vec();
    key.seal_in_place_append_tag(nonce, identity_aad(endpoint_id), &mut in_out)
        .map_err(|_| "No se pudo cifrar la identidad P2P.".to_string())?;
    Ok(in_out)
}

fn open_identity_secret(
    ciphertext: &[u8],
    wrapping_key: &[u8; IDENTITY_SECRET_LEN],
    endpoint_id: &str,
    nonce_bytes: [u8; NONCE_LEN],
) -> Result<[u8; IDENTITY_SECRET_LEN], String> {
    let key = aead_key(wrapping_key)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = ciphertext.to_vec();
    let plaintext = key
        .open_in_place(nonce, identity_aad(endpoint_id), &mut in_out)
        .map_err(|_| "No se pudo descifrar la identidad P2P.".to_string())?;
    plaintext
        .try_into()
        .map_err(|_| "La clave privada P2P tiene un tamaño invalido.".to_string())
}

fn aead_key(secret: &[u8; IDENTITY_SECRET_LEN]) -> Result<LessSafeKey, String> {
    let unbound = UnboundKey::new(&AES_256_GCM, secret)
        .map_err(|_| "La clave de cifrado P2P es invalida.".to_string())?;
    Ok(LessSafeKey::new(unbound))
}

fn identity_aad(endpoint_id: &str) -> Aad<Vec<u8>> {
    Aad::from(format!("rau-p2p-identity-v1:{endpoint_id}").into_bytes())
}

pub(super) fn require_unlocked_identity() -> Result<String, String> {
    identity_session()
        .lock()
        .map_err(|_| "No se pudo leer identidad P2P de esta sesion.".to_string())?
        .as_ref()
        .map(|identity| identity.endpoint_id.clone())
        .ok_or_else(|| "Desbloquea tu identidad P2P para continuar.".to_string())
}

pub(super) fn unlocked_network_identity() -> Result<NetworkIdentity, String> {
    let identity = identity_session()
        .lock()
        .map_err(|_| "No se pudo leer identidad P2P de esta sesion.".to_string())?;
    let identity = identity
        .as_ref()
        .ok_or_else(|| "Desbloquea tu identidad P2P para iniciar la red.".to_string())?;
    let mut secret = [0u8; IDENTITY_SECRET_LEN];
    secret.copy_from_slice(identity._secret.as_ref());
    Ok(NetworkIdentity {
        endpoint_id: identity.endpoint_id.clone(),
        display_name: identity.display_name.clone(),
        secret: Zeroizing::new(secret),
    })
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "No se pudieron generar bytes aleatorios seguros.".to_string())?;
    Ok(bytes)
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn system_time_millis(value: SystemTime) -> Option<i64> {
    let millis = value.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(millis).ok()
}

fn as_i64(value: u64, label: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("El {label} excede el limite soportado."))
}

fn from_i64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_secret_round_trips_and_rejects_wrong_key() {
        let secret = [7u8; IDENTITY_SECRET_LEN];
        let salt = [3u8; IDENTITY_SALT_LEN];
        let nonce = [9u8; NONCE_LEN];
        let key = derive_wrapping_key("correct horse battery staple", &salt, 1024, 1, 1)
            .expect("derive key");
        let ciphertext =
            seal_identity_secret(&secret, &key, "endpoint-a", nonce).expect("seal identity");
        let decrypted =
            open_identity_secret(&ciphertext, &key, "endpoint-a", nonce).expect("open identity");
        assert_eq!(decrypted, secret);

        let wrong_key =
            derive_wrapping_key("wrong password", &salt, 1024, 1, 1).expect("derive wrong key");
        assert!(open_identity_secret(&ciphertext, &wrong_key, "endpoint-a", nonce).is_err());
        assert!(open_identity_secret(&ciphertext, &key, "endpoint-b", nonce).is_err());
    }

    #[test]
    fn file_ids_are_stable_and_scoped_to_a_share() {
        let first = stable_file_id("share-a", "Artist/Track.aiff");
        assert_eq!(first, stable_file_id("share-a", "Artist/Track.aiff"));
        assert_ne!(first, stable_file_id("share-b", "Artist/Track.aiff"));
        assert_ne!(first, stable_file_id("share-a", "Artist/Other.aiff"));
    }

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(escape_like("100%_mix\\demo"), "100\\%\\_mix\\\\demo");
    }

    #[test]
    fn visibility_is_explicitly_bounded() {
        assert_eq!(validate_visibility("contacts").unwrap(), "contacts");
        assert_eq!(validate_visibility("ticket").unwrap(), "ticket");
        assert!(validate_visibility("public-everything").is_err());
    }

    #[test]
    fn presence_expires_without_a_recent_authenticated_observation() {
        let now = DateTime::parse_from_rfc3339("2026-07-16T12:02:00Z")
            .expect("parse now")
            .with_timezone(&Utc);
        assert!(presence_observation_is_fresh(
            Some("2026-07-16T12:00:01Z"),
            now
        ));
        assert!(!presence_observation_is_fresh(
            Some("2026-07-16T11:59:59Z"),
            now
        ));
        assert!(!presence_observation_is_fresh(None, now));
    }

    #[test]
    fn folder_index_uses_virtual_paths_and_skips_hidden_entries() {
        let root = std::env::temp_dir().join(format!("rau-p2p-index-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("Artist")).expect("create visible test directory");
        std::fs::create_dir_all(root.join(".private")).expect("create hidden test directory");
        std::fs::write(root.join("Artist").join("Track.aiff"), b"audio")
            .expect("write visible test file");
        std::fs::write(root.join(".private").join("Secret.wav"), b"hidden")
            .expect("write hidden test file");

        let index = index_shared_folder(&root, "share-a").expect("index shared folder");
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].relative_path, "Artist/Track.aiff");
        assert!(!index.files[0]
            .relative_path
            .contains(root.to_string_lossy().as_ref()));
        assert_eq!(index.total_size_bytes, 5);

        std::fs::remove_dir_all(root).expect("clean test directory");
    }

    #[test]
    fn shared_file_resolution_stays_beneath_its_canonical_root() {
        let root = std::env::temp_dir().join(format!("rau-p2p-resolve-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("Artist")).expect("create shared test directory");
        std::fs::write(root.join("Artist").join("Track.aiff"), b"audio")
            .expect("write shared test file");
        let canonical = root.canonicalize().expect("canonical test root");
        let resolved = safe_shared_file_path(&canonical, "Artist/Track.aiff")
            .expect("resolve safe shared path");
        assert!(resolved.starts_with(&canonical));
        assert!(safe_shared_file_path(&canonical, "../outside.aiff").is_err());
        assert!(safe_shared_file_path(&canonical, "/absolute.aiff").is_err());
        std::fs::remove_dir_all(root).expect("clean shared test directory");
    }

    #[test]
    fn p2p_schema_enforces_one_identity_and_cascades_share_files() {
        let conn = Connection::open_in_memory().expect("open memory database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        init_db(&conn).expect("initialize p2p schema");

        conn.execute(
            "INSERT INTO p2p_shares (
               id, name, root_path, visibility, enabled, file_count,
               total_size_bytes, skipped_entries, last_indexed_at, created_at, updated_at
             ) VALUES ('share-a', 'Masters', '/virtual/masters', 'contacts', 1, 1, 5, 0, 'now', 'now', 'now')",
            [],
        )
        .expect("insert share");
        conn.execute(
            "INSERT INTO p2p_shared_files (
               id, share_id, relative_path, name, extension, size_bytes
             ) VALUES ('file-a', 'share-a', 'Track.aiff', 'Track.aiff', 'aiff', 5)",
            [],
        )
        .expect("insert shared file");
        conn.execute("DELETE FROM p2p_shares WHERE id = 'share-a'", [])
            .expect("delete share");

        let remaining: i64 = conn
            .query_row("SELECT count(*) FROM p2p_shared_files", [], |row| {
                row.get(0)
            })
            .expect("count shared files");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn remote_catalog_requires_a_known_peer_and_hides_selected_contacts() {
        let conn = Connection::open_in_memory().expect("open memory database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        init_db(&conn).expect("initialize p2p schema");
        conn.execute(
            "INSERT INTO p2p_peers (
               endpoint_id, display_name, trust_state, presence_status, paired_at
             ) VALUES ('peer-a', 'Peer A', 'observed', 'online', 'now')",
            [],
        )
        .expect("insert known peer");
        for (share_id, visibility) in [
            ("share-contacts", "contacts"),
            ("share-selected", "selected_contacts"),
        ] {
            conn.execute(
                "INSERT INTO p2p_shares (
                   id, name, root_path, visibility, enabled, file_count,
                   total_size_bytes, skipped_entries, last_indexed_at, created_at, updated_at
                 ) VALUES (?1, ?1, ?2, ?3, 1, 1, 5, 0, 'now', 'now', 'now')",
                params![share_id, format!("/virtual/{share_id}"), visibility],
            )
            .expect("insert visibility share");
            conn.execute(
                "INSERT INTO p2p_shared_files (
                   id, share_id, relative_path, name, extension, size_bytes
                 ) VALUES (?1, ?2, 'Track.aiff', 'Track.aiff', 'aiff', 5)",
                params![format!("file-{share_id}"), share_id],
            )
            .expect("insert visibility file");
        }

        assert!(require_authorized_peer(&conn, "unknown").is_err());
        require_authorized_peer(&conn, "peer-a").expect("authorize known peer");
        let response = search_shared_files(&conn, "local-endpoint", String::new(), Some(100), true)
            .expect("search remote-visible files");
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].share_id, "share-contacts");
    }
}
