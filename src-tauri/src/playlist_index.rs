use crate::{settings, system};
use aifficator_core::exporter::export_with_new_playlist_xml;
use aifficator_core::rekordbox::{parse_rekordbox_xml_file, Track};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const DB_FILE: &str = "aifficator.sqlite3";
const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const EMBEDDING_DIMENSIONS: usize = 512;
const EMBEDDING_BATCH_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexLibrary {
    id: String,
    source_path: String,
    source_name: String,
    product_name: Option<String>,
    product_version: Option<String>,
    track_count: usize,
    playlist_count: usize,
    embedded_track_count: usize,
    missing_file_count: usize,
    indexed_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexPlaylist {
    library_id: String,
    path: String,
    name: String,
    node_type: Option<String>,
    track_count: usize,
    position: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexTrack {
    library_id: String,
    track_id: String,
    name: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    kind: Option<String>,
    location: Option<String>,
    source_path: Option<String>,
    size: Option<u64>,
    total_time: Option<u64>,
    sample_rate: Option<u32>,
    bitrate: Option<u32>,
    source_exists: bool,
    search_text: String,
    genre: Option<String>,
    comments: Option<String>,
    bpm: Option<String>,
    key: Option<String>,
    rating: Option<String>,
    year: Option<String>,
    label: Option<String>,
    date_added: Option<String>,
    attributes: BTreeMap<String, String>,
    embedding_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistSearchResult {
    track: PlaylistIndexTrack,
    score: f64,
    mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexGroup {
    library_id: String,
    kind: String,
    value: String,
    name: String,
    track_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistTaxonomyOverview {
    library: PlaylistIndexLibrary,
    track_count: usize,
    playlist_count: usize,
    genre_count: usize,
    artist_count: usize,
    album_count: usize,
    key_count: usize,
    bpm_known_count: usize,
    bpm_missing_count: usize,
    bpm_average: Option<f64>,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
    genre_missing_count: usize,
    key_missing_count: usize,
    source_missing_count: usize,
    genres: Vec<TaxonomyCount>,
    bpm_buckets: Vec<TaxonomyCount>,
    keys: Vec<TaxonomyCount>,
    formats: Vec<TaxonomyCount>,
    years: Vec<TaxonomyCount>,
    metadata_gaps: Vec<TaxonomyCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyCount {
    kind: String,
    value: String,
    name: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistTaxonomyGraph {
    nodes: Vec<TaxonomyGraphNode>,
    edges: Vec<TaxonomyGraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyGraphNode {
    id: String,
    kind: String,
    value: String,
    label: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyGraphEdge {
    id: String,
    source: String,
    target: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistDraft {
    id: String,
    library_id: String,
    name: String,
    description: Option<String>,
    track_count: usize,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexImportResponse {
    library: PlaylistIndexLibrary,
    playlists: Vec<PlaylistIndexPlaylist>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexPreviewResponse {
    source_path: String,
    source_name: String,
    product_name: Option<String>,
    product_version: Option<String>,
    tracks_total: usize,
    playlists: Vec<PlaylistIndexPreviewPlaylist>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistIndexPreviewPlaylist {
    path: String,
    name: String,
    track_count: usize,
    position: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistEmbeddingResult {
    library_id: String,
    generated_total: usize,
    skipped_total: usize,
    model: String,
    dimensions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistExportResult {
    draft_id: String,
    output_path: String,
    track_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistCopilotRequest {
    library_id: String,
    prompt: String,
    target_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotResponse {
    message: String,
    interpreted: PlaylistCopilotInterpretation,
    questions: Vec<String>,
    candidates: Vec<PlaylistCopilotCandidate>,
    used_openai: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotCandidate {
    track: PlaylistIndexTrack,
    score: f64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlaylistCopilotInterpretation {
    genres: Vec<String>,
    artists: Vec<String>,
    keys: Vec<String>,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
    mood: Option<String>,
    energy: Option<String>,
    exclude_terms: Vec<String>,
    target_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct PlaylistIndexProgressEvent {
    #[serde(rename = "type")]
    event_type: String,
    level: String,
    message: String,
    progress: Option<f64>,
    library_id: Option<String>,
    playlist_path: Option<String>,
    playlist_status: Option<String>,
    track_id: Option<String>,
    track_status: Option<String>,
    processed: Option<usize>,
    total: Option<usize>,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    model: String,
    usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    index: usize,
    embedding: Vec<f64>,
}

#[tauri::command]
pub fn playlist_index_libraries(app: AppHandle) -> Result<Vec<PlaylistIndexLibrary>, String> {
    let conn = open_db(&app)?;
    list_libraries(&conn)
}

#[tauri::command]
pub fn playlist_index_preview_xml(path: String) -> Result<PlaylistIndexPreviewResponse, String> {
    let source_path = PathBuf::from(&path);
    if !source_path.is_file() {
        return Err(format!("XML no encontrado: {}", source_path.display()));
    }

    let rekordbox_library =
        parse_rekordbox_xml_file(&source_path).map_err(|error| error.to_string())?;
    let source_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("rekordbox.xml")
        .to_string();
    let product_name = rekordbox_library
        .product
        .as_ref()
        .and_then(|product| product.name.clone());
    let product_version = rekordbox_library
        .product
        .as_ref()
        .and_then(|product| product.version.clone());
    let playlists = rekordbox_library
        .playlists_flat()
        .into_iter()
        .enumerate()
        .filter(|(_, playlist)| playlist.node_type.as_deref() == Some("1"))
        .map(|(position, playlist)| PlaylistIndexPreviewPlaylist {
            path: playlist.path,
            name: playlist.name,
            track_count: playlist.track_count,
            position,
        })
        .collect();

    Ok(PlaylistIndexPreviewResponse {
        source_path: source_path.to_string_lossy().into_owned(),
        source_name,
        product_name,
        product_version,
        tracks_total: rekordbox_library.tracks.len(),
        playlists,
    })
}

#[tauri::command]
pub fn playlist_index_import_xml(
    app: AppHandle,
    path: String,
    playlist_paths: Option<Vec<String>>,
) -> Result<PlaylistIndexImportResponse, String> {
    let source_path = PathBuf::from(&path);
    if !source_path.is_file() {
        return Err(format!("XML no encontrado: {}", source_path.display()));
    }

    emit_progress(
        &app,
        "info",
        "Indexando XML de Rekordbox.",
        Some(0.0),
        None,
        None,
        None,
    );

    let rekordbox_library =
        parse_rekordbox_xml_file(&source_path).map_err(|error| error.to_string())?;
    let all_playlists = rekordbox_library.playlists_flat();
    let requested_playlist_paths = playlist_paths
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>();
    let selected_mode = !requested_playlist_paths.is_empty();
    let playlists = all_playlists
        .iter()
        .filter(|playlist| playlist.node_type.as_deref() == Some("1"))
        .filter(|playlist| !selected_mode || requested_playlist_paths.contains(&playlist.path))
        .cloned()
        .collect::<Vec<_>>();

    if selected_mode {
        let available_paths = all_playlists
            .iter()
            .filter(|playlist| playlist.node_type.as_deref() == Some("1"))
            .map(|playlist| playlist.path.clone())
            .collect::<BTreeSet<_>>();
        let missing_paths = requested_playlist_paths
            .difference(&available_paths)
            .cloned()
            .collect::<Vec<_>>();
        if !missing_paths.is_empty() {
            return Err(format!(
                "Playlist(s) no encontrada(s): {}",
                missing_paths.join(", ")
            ));
        }
    }

    let selected_track_ids = if selected_mode {
        playlists
            .iter()
            .flat_map(|playlist| playlist.track_keys.iter().cloned())
            .collect::<BTreeSet<_>>()
    } else {
        rekordbox_library
            .tracks
            .iter()
            .map(|track| track.track_id.clone())
            .collect::<BTreeSet<_>>()
    };
    let collection_track_ids = rekordbox_library
        .tracks
        .iter()
        .map(|track| track.track_id.clone())
        .collect::<BTreeSet<_>>();
    let indexed_track_ids = selected_track_ids
        .intersection(&collection_track_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    let playlist_paths_by_track = playlist_paths_by_track(&all_playlists);
    let now = timestamp();
    let mut conn = open_db(&app)?;
    let existing_id = conn
        .query_row(
            "SELECT id FROM playlist_index_libraries WHERE source_path = ?1",
            params![source_path.to_string_lossy().as_ref()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer libreria indexada: {error}"))?;
    let library_id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let source_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("rekordbox.xml")
        .to_string();
    let playlist_count = playlists.len();
    let product_name = rekordbox_library
        .product
        .as_ref()
        .and_then(|product| product.name.clone());
    let product_version = rekordbox_library
        .product
        .as_ref()
        .and_then(|product| product.version.clone());
    let indexed_tracks = rekordbox_library
        .tracks
        .iter()
        .filter(|track| indexed_track_ids.contains(&track.track_id))
        .collect::<Vec<_>>();
    let total_work = indexed_tracks.len() + playlists.len() + 1;
    let mut processed_work = 0usize;

    emit_progress(
        &app,
        "info",
        "Preparando indice SQLite.",
        Some(2.0),
        Some(library_id.clone()),
        Some(processed_work),
        Some(total_work),
    );

    {
        let tx = conn
            .transaction()
            .map_err(|error| format!("No se pudo iniciar transaccion SQLite: {error}"))?;
        tx.execute(
            "INSERT INTO playlist_index_libraries (
                id, source_path, source_name, product_name, product_version,
                track_count, playlist_count, indexed_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(source_path) DO UPDATE SET
                source_name = excluded.source_name,
                product_name = excluded.product_name,
                product_version = excluded.product_version,
                track_count = excluded.track_count,
                playlist_count = excluded.playlist_count,
                updated_at = excluded.updated_at",
            params![
                &library_id,
                source_path.to_string_lossy().as_ref(),
                &source_name,
                &product_name,
                &product_version,
                indexed_track_ids.len() as i64,
                playlist_count as i64,
                &now
            ],
        )
        .map_err(|error| format!("No se pudo guardar libreria indexada: {error}"))?;
        if selected_mode {
            for playlist in &playlists {
                tx.execute(
                    "DELETE FROM playlist_index_memberships WHERE library_id = ?1 AND playlist_path = ?2",
                    params![&library_id, &playlist.path],
                )
                .map_err(|error| format!("No se pudieron limpiar memberships de {}: {error}", playlist.path))?;
                tx.execute(
                    "DELETE FROM playlist_index_playlists WHERE library_id = ?1 AND path = ?2",
                    params![&library_id, &playlist.path],
                )
                .map_err(|error| {
                    format!("No se pudo limpiar playlist {}: {error}", playlist.path)
                })?;
            }
        } else {
            tx.execute(
                "DELETE FROM playlist_index_memberships WHERE library_id = ?1",
                params![&library_id],
            )
            .map_err(|error| format!("No se pudieron limpiar memberships: {error}"))?;
            tx.execute(
                "DELETE FROM playlist_index_playlists WHERE library_id = ?1",
                params![&library_id],
            )
            .map_err(|error| format!("No se pudieron limpiar playlists indexadas: {error}"))?;
            tx.execute(
                "DELETE FROM playlist_track_embeddings WHERE library_id = ?1",
                params![&library_id],
            )
            .map_err(|error| format!("No se pudieron limpiar embeddings de tracks: {error}"))?;
            tx.execute(
                "DELETE FROM playlist_index_tracks WHERE library_id = ?1",
                params![&library_id],
            )
            .map_err(|error| format!("No se pudieron limpiar tracks indexados: {error}"))?;
        }

        for (index, track) in indexed_tracks.iter().enumerate() {
            insert_track(&tx, &library_id, track, &playlist_paths_by_track, &now)?;
            processed_work += 1;

            if should_emit_index_progress(index + 1, indexed_tracks.len()) {
                emit_index_work_progress(
                    &app,
                    &library_id,
                    "Indexando tracks en SQLite.",
                    processed_work,
                    total_work,
                );
            }
        }

        for (position, playlist) in playlists.iter().enumerate() {
            emit_playlist_index_progress(
                &app,
                &library_id,
                &playlist.path,
                "indexing",
                &format!("Indexando playlist: {}", playlist.path),
                processed_work,
                total_work,
            );
            tx.execute(
                "INSERT INTO playlist_index_playlists (
                    library_id, path, name, node_type, track_count, position, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    &library_id,
                    &playlist.path,
                    &playlist.name,
                    &playlist.node_type,
                    playlist.track_count as i64,
                    position as i64,
                    &now
                ],
            )
            .map_err(|error| format!("No se pudo guardar playlist {}: {error}", playlist.path))?;

            for (track_position, track_id) in playlist.track_keys.iter().enumerate() {
                if !indexed_track_ids.contains(track_id) {
                    continue;
                }

                tx.execute(
                    "INSERT INTO playlist_index_memberships (
                        library_id, playlist_path, track_id, position
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![&library_id, &playlist.path, track_id, track_position as i64],
                )
                .map_err(|error| {
                    format!(
                        "No se pudo guardar track en playlist {}: {error}",
                        playlist.path
                    )
                })?;
            }

            processed_work += 1;
            emit_playlist_index_progress(
                &app,
                &library_id,
                &playlist.path,
                "indexed",
                &format!("Playlist indexada: {}", playlist.path),
                processed_work,
                total_work,
            );
            if should_emit_index_progress(position + 1, playlists.len()) {
                emit_index_work_progress(
                    &app,
                    &library_id,
                    "Indexando playlists y relaciones.",
                    processed_work,
                    total_work,
                );
            }
        }

        tx.execute(
            "UPDATE playlist_index_libraries
             SET track_count = (
                   SELECT COUNT(*) FROM playlist_index_tracks WHERE library_id = ?1
                 ),
                 playlist_count = (
                   SELECT COUNT(*) FROM playlist_index_playlists WHERE library_id = ?1 AND node_type = '1'
                 ),
                 updated_at = ?2
             WHERE id = ?1",
            params![&library_id, &now],
        )
        .map_err(|error| format!("No se pudieron actualizar contadores de libreria: {error}"))?;

        tx.commit()
            .map_err(|error| format!("No se pudo confirmar indice SQLite: {error}"))?;
    }

    emit_progress(
        &app,
        "info",
        "Reconstruyendo indice de busqueda FTS.",
        Some(98.0),
        Some(library_id.clone()),
        Some(processed_work),
        Some(total_work),
    );
    rebuild_fts(&conn)?;
    processed_work += 1;
    emit_progress(
        &app,
        "info",
        "Indice de playlists actualizado.",
        Some(100.0),
        Some(library_id.clone()),
        Some(processed_work),
        Some(total_work),
    );

    let library = get_library(&conn, &library_id)?
        .ok_or_else(|| "No se pudo leer libreria indexada.".to_string())?;
    let playlists = list_playlists(&conn, &library_id)?;
    Ok(PlaylistIndexImportResponse { library, playlists })
}

#[tauri::command]
pub fn playlist_index_library_playlists(
    app: AppHandle,
    library_id: String,
) -> Result<Vec<PlaylistIndexPlaylist>, String> {
    let conn = open_db(&app)?;
    list_playlists(&conn, &library_id)
}

#[tauri::command]
pub fn playlist_index_delete_library(app: AppHandle, library_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    let deleted = conn
        .execute(
            "DELETE FROM playlist_index_libraries WHERE id = ?1",
            params![&library_id],
        )
        .map_err(|error| format!("No se pudo eliminar libreria indexada: {error}"))?;
    if deleted == 0 {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }
    rebuild_fts(&conn)?;
    emit_progress(
        &app,
        "info",
        "Indice de libreria eliminado.",
        Some(100.0),
        Some(library_id.clone()),
        None,
        None,
    );
    Ok(library_id)
}

#[tauri::command]
pub fn playlist_index_delete_playlists(
    app: AppHandle,
    library_id: String,
    playlist_paths: Vec<String>,
) -> Result<PlaylistIndexImportResponse, String> {
    let mut conn = open_db(&app)?;
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let paths = playlist_paths
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>();
    if paths.is_empty() {
        return Err("Selecciona al menos una playlist indexada para eliminar.".to_string());
    }

    let now = timestamp();
    {
        let tx = conn
            .transaction()
            .map_err(|error| format!("No se pudo iniciar transaccion SQLite: {error}"))?;
        let mut affected_track_ids = BTreeSet::new();

        for playlist_path in &paths {
            {
                let mut stmt = tx
                    .prepare(
                        "SELECT DISTINCT track_id
                         FROM playlist_index_memberships
                         WHERE library_id = ?1 AND playlist_path = ?2",
                    )
                    .map_err(|error| {
                        format!("No se pudieron leer tracks de {playlist_path}: {error}")
                    })?;
                let rows = stmt
                    .query_map(params![&library_id, playlist_path], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|error| {
                        format!("No se pudieron mapear tracks de {playlist_path}: {error}")
                    })?;
                for track_id in rows {
                    affected_track_ids.insert(track_id.map_err(|error| {
                        format!("No se pudo leer track de {playlist_path}: {error}")
                    })?);
                }
            }

            tx.execute(
                "DELETE FROM playlist_index_playlists WHERE library_id = ?1 AND path = ?2",
                params![&library_id, playlist_path],
            )
            .map_err(|error| format!("No se pudo eliminar indice de {playlist_path}: {error}"))?;
        }

        for track_id in &affected_track_ids {
            tx.execute(
                "DELETE FROM playlist_index_tracks
                 WHERE library_id = ?1 AND track_id = ?2
                   AND NOT EXISTS (
                     SELECT 1 FROM playlist_index_memberships m
                     WHERE m.library_id = playlist_index_tracks.library_id
                       AND m.track_id = playlist_index_tracks.track_id
                   )",
                params![&library_id, track_id],
            )
            .map_err(|error| format!("No se pudo limpiar track huerfano {track_id}: {error}"))?;
        }

        tx.execute(
            "DELETE FROM playlist_draft_tracks
             WHERE draft_id IN (SELECT id FROM playlist_drafts WHERE library_id = ?1)
               AND NOT EXISTS (
                 SELECT 1 FROM playlist_index_tracks t
                 WHERE t.library_id = ?1 AND t.track_id = playlist_draft_tracks.track_id
               )",
            params![&library_id],
        )
        .map_err(|error| format!("No se pudieron limpiar drafts huerfanos: {error}"))?;

        tx.execute(
            "UPDATE playlist_index_libraries
             SET track_count = (
                   SELECT COUNT(*) FROM playlist_index_tracks WHERE library_id = ?1
                 ),
                 playlist_count = (
                   SELECT COUNT(*) FROM playlist_index_playlists WHERE library_id = ?1 AND node_type = '1'
                 ),
                 updated_at = ?2
             WHERE id = ?1",
            params![&library_id, &now],
        )
        .map_err(|error| format!("No se pudieron actualizar contadores de libreria: {error}"))?;

        tx.commit()
            .map_err(|error| format!("No se pudo confirmar eliminacion de indices: {error}"))?;
    }

    rebuild_fts(&conn)?;
    emit_progress(
        &app,
        "info",
        &format!("Indices de playlists eliminados: {}", paths.len()),
        Some(100.0),
        Some(library_id.clone()),
        Some(paths.len()),
        Some(paths.len()),
    );

    let library = get_library(&conn, &library_id)?
        .ok_or_else(|| "No se pudo leer libreria indexada.".to_string())?;
    let playlists = list_playlists(&conn, &library_id)?;
    Ok(PlaylistIndexImportResponse { library, playlists })
}

#[tauri::command]
pub fn playlist_index_delete_tracks(
    app: AppHandle,
    library_id: String,
    track_ids: Vec<String>,
) -> Result<PlaylistIndexImportResponse, String> {
    let mut conn = open_db(&app)?;
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let ids = track_ids
        .into_iter()
        .map(|track_id| track_id.trim().to_string())
        .filter(|track_id| !track_id.is_empty())
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Err("Selecciona al menos un track indexado para eliminar.".to_string());
    }

    let now = timestamp();
    {
        let tx = conn
            .transaction()
            .map_err(|error| format!("No se pudo iniciar transaccion SQLite: {error}"))?;
        for track_id in &ids {
            tx.execute(
                "DELETE FROM playlist_index_tracks WHERE library_id = ?1 AND track_id = ?2",
                params![&library_id, track_id],
            )
            .map_err(|error| format!("No se pudo eliminar track indexado {track_id}: {error}"))?;
        }

        tx.execute(
            "DELETE FROM playlist_draft_tracks
             WHERE draft_id IN (SELECT id FROM playlist_drafts WHERE library_id = ?1)
               AND NOT EXISTS (
                 SELECT 1 FROM playlist_index_tracks t
                 WHERE t.library_id = ?1 AND t.track_id = playlist_draft_tracks.track_id
               )",
            params![&library_id],
        )
        .map_err(|error| format!("No se pudieron limpiar drafts huerfanos: {error}"))?;

        tx.execute(
            "UPDATE playlist_index_playlists
             SET track_count = (
                   SELECT COUNT(DISTINCT track_id)
                   FROM playlist_index_memberships
                   WHERE library_id = playlist_index_playlists.library_id
                     AND playlist_path = playlist_index_playlists.path
                 ),
                 updated_at = ?2
             WHERE library_id = ?1",
            params![&library_id, &now],
        )
        .map_err(|error| format!("No se pudieron actualizar contadores de playlists: {error}"))?;

        tx.execute(
            "UPDATE playlist_index_libraries
             SET track_count = (
                   SELECT COUNT(*) FROM playlist_index_tracks WHERE library_id = ?1
                 ),
                 playlist_count = (
                   SELECT COUNT(*) FROM playlist_index_playlists WHERE library_id = ?1 AND node_type = '1'
                 ),
                 updated_at = ?2
             WHERE id = ?1",
            params![&library_id, &now],
        )
        .map_err(|error| format!("No se pudieron actualizar contadores de libreria: {error}"))?;

        tx.commit()
            .map_err(|error| format!("No se pudo confirmar eliminacion de tracks: {error}"))?;
    }

    rebuild_fts(&conn)?;
    emit_progress(
        &app,
        "info",
        &format!("Tracks indexados eliminados: {}", ids.len()),
        Some(100.0),
        Some(library_id.clone()),
        Some(ids.len()),
        Some(ids.len()),
    );

    let library = get_library(&conn, &library_id)?
        .ok_or_else(|| "No se pudo leer libreria indexada.".to_string())?;
    let playlists = list_playlists(&conn, &library_id)?;
    Ok(PlaylistIndexImportResponse { library, playlists })
}

#[tauri::command]
pub fn playlist_index_playlist_tracks(
    app: AppHandle,
    library_id: String,
    playlist_path: String,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    list_playlist_tracks(&conn, &library_id, &playlist_path)
}

#[tauri::command]
pub fn playlist_index_search_tracks(
    app: AppHandle,
    library_id: Option<String>,
    query: String,
    limit: Option<usize>,
    semantic: Option<bool>,
) -> Result<Vec<PlaylistSearchResult>, String> {
    let conn = open_db(&app)?;
    let limit = limit.unwrap_or(80).clamp(1, 250);

    if semantic.unwrap_or(false) && !query.trim().is_empty() {
        return semantic_search(&app, &conn, library_id.as_deref(), &query, limit);
    }

    lexical_search(&conn, library_id.as_deref(), &query, limit)
}

#[tauri::command]
pub fn playlist_index_track_groups(
    app: AppHandle,
    library_id: String,
    kind: String,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<PlaylistIndexGroup>, String> {
    let conn = open_db(&app)?;
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    list_track_groups(&conn, &library_id, &kind, query.as_deref(), limit)
}

#[tauri::command]
pub fn playlist_index_group_tracks(
    app: AppHandle,
    library_id: String,
    kind: String,
    value: String,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    let limit = limit.unwrap_or(500).clamp(1, 3000);
    list_group_tracks(&conn, &library_id, &kind, &value, query.as_deref(), limit)
}

#[tauri::command]
pub fn playlist_index_taxonomy_overview(
    app: AppHandle,
    library_id: String,
) -> Result<PlaylistTaxonomyOverview, String> {
    let conn = open_db(&app)?;
    taxonomy_overview(&conn, &library_id)
}

#[tauri::command]
pub fn playlist_index_taxonomy_graph(
    app: AppHandle,
    library_id: String,
    limit: Option<usize>,
) -> Result<PlaylistTaxonomyGraph, String> {
    let conn = open_db(&app)?;
    taxonomy_graph(&conn, &library_id, limit.unwrap_or(12).clamp(4, 30))
}

#[tauri::command]
pub fn playlist_index_taxonomy_tracks(
    app: AppHandle,
    library_id: String,
    kind: String,
    value: String,
    limit: Option<usize>,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    taxonomy_tracks(
        &conn,
        &library_id,
        &kind,
        &value,
        limit.unwrap_or(250).clamp(1, 2000),
    )
}

#[tauri::command]
pub async fn playlist_copilot_generate(
    app: AppHandle,
    request: PlaylistCopilotRequest,
) -> Result<PlaylistCopilotResponse, String> {
    let app_for_error = app.clone();
    tauri::async_runtime::spawn_blocking(move || playlist_copilot_generate_blocking(app, request))
        .await
        .map_err(|error| {
            settings::localized(
                &app_for_error,
                &format!("Playlist Copilot fallo inesperadamente: {error}"),
                &format!("Playlist Copilot failed unexpectedly: {error}"),
            )
        })?
}

#[tauri::command]
pub fn playlist_index_track_cover(
    app: AppHandle,
    source_path: String,
) -> Result<Option<String>, String> {
    extract_track_cover(&app, &source_path)
}

#[tauri::command]
pub async fn playlist_index_generate_embeddings(
    app: AppHandle,
    library_id: String,
    limit: Option<usize>,
    track_ids: Option<Vec<String>>,
) -> Result<PlaylistEmbeddingResult, String> {
    let app_for_error = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        generate_embeddings_blocking(app, library_id, limit, track_ids)
    })
    .await
    .map_err(|error| {
        settings::localized(
            &app_for_error,
            &format!("La indexacion vectorial fallo inesperadamente: {error}"),
            &format!("Vector indexing failed unexpectedly: {error}"),
        )
    })?
}

#[tauri::command]
pub fn playlist_index_drafts(
    app: AppHandle,
    library_id: Option<String>,
) -> Result<Vec<PlaylistDraft>, String> {
    let conn = open_db(&app)?;
    list_drafts(&conn, library_id.as_deref())
}

#[tauri::command]
pub fn playlist_index_create_draft(
    app: AppHandle,
    library_id: String,
    name: String,
    description: Option<String>,
) -> Result<PlaylistDraft, String> {
    let conn = open_db(&app)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("Ingresa un nombre para la playlist.".to_string());
    }
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let id = Uuid::new_v4().to_string();
    let now = timestamp();
    conn.execute(
        "INSERT INTO playlist_drafts (id, library_id, name, description, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![&id, &library_id, name, clean_optional(description), &now],
    )
    .map_err(|error| format!("No se pudo crear playlist draft: {error}"))?;

    get_draft(&conn, &id)?.ok_or_else(|| "No se pudo leer playlist creada.".to_string())
}

#[tauri::command]
pub fn playlist_index_add_tracks_to_draft(
    app: AppHandle,
    draft_id: String,
    track_ids: Vec<String>,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let mut conn = open_db(&app)?;
    let draft = get_draft(&conn, &draft_id)?
        .ok_or_else(|| format!("Playlist draft no encontrada: {draft_id}"))?;
    let mut seen = BTreeSet::new();
    let unique_track_ids = track_ids
        .into_iter()
        .filter(|track_id| seen.insert(track_id.clone()))
        .collect::<Vec<_>>();
    let now = timestamp();

    {
        let tx = conn
            .transaction()
            .map_err(|error| format!("No se pudo iniciar transaccion SQLite: {error}"))?;
        let mut position = tx
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM playlist_draft_tracks WHERE draft_id = ?1",
                params![&draft_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("No se pudo leer posicion de draft: {error}"))?;

        for track_id in unique_track_ids {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM playlist_index_tracks WHERE library_id = ?1 AND track_id = ?2",
                    params![&draft.library_id, &track_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|error| format!("No se pudo validar track {track_id}: {error}"))?
                .is_some();
            if !exists {
                continue;
            }

            let already_added = tx
                .query_row(
                    "SELECT 1 FROM playlist_draft_tracks WHERE draft_id = ?1 AND track_id = ?2",
                    params![&draft_id, &track_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(|error| format!("No se pudo validar draft track {track_id}: {error}"))?
                .is_some();
            if already_added {
                continue;
            }

            position += 1;
            tx.execute(
                "INSERT INTO playlist_draft_tracks (draft_id, track_id, position, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![&draft_id, &track_id, position, &now],
            )
            .map_err(|error| format!("No se pudo agregar track al draft: {error}"))?;
        }

        tx.execute(
            "UPDATE playlist_drafts SET updated_at = ?2 WHERE id = ?1",
            params![&draft_id, &now],
        )
        .map_err(|error| format!("No se pudo actualizar draft: {error}"))?;

        tx.commit()
            .map_err(|error| format!("No se pudo confirmar playlist draft: {error}"))?;
    }

    draft_tracks(&conn, &draft_id)
}

#[tauri::command]
pub fn playlist_index_remove_draft_track(
    app: AppHandle,
    draft_id: String,
    track_id: String,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    conn.execute(
        "DELETE FROM playlist_draft_tracks WHERE draft_id = ?1 AND track_id = ?2",
        params![&draft_id, &track_id],
    )
    .map_err(|error| format!("No se pudo quitar track del draft: {error}"))?;
    conn.execute(
        "UPDATE playlist_drafts SET updated_at = ?2 WHERE id = ?1",
        params![&draft_id, timestamp()],
    )
    .map_err(|error| format!("No se pudo actualizar draft: {error}"))?;
    draft_tracks(&conn, &draft_id)
}

#[tauri::command]
pub fn playlist_index_delete_draft(app: AppHandle, draft_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    conn.execute(
        "DELETE FROM playlist_drafts WHERE id = ?1",
        params![&draft_id],
    )
    .map_err(|error| format!("No se pudo borrar playlist draft: {error}"))?;
    Ok(draft_id)
}

#[tauri::command]
pub fn playlist_index_draft_tracks(
    app: AppHandle,
    draft_id: String,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    draft_tracks(&conn, &draft_id)
}

#[tauri::command]
pub fn playlist_index_export_draft_xml(
    app: AppHandle,
    draft_id: String,
    output_path: String,
) -> Result<PlaylistExportResult, String> {
    let conn = open_db(&app)?;
    let draft = get_draft(&conn, &draft_id)?
        .ok_or_else(|| format!("Playlist draft no encontrada: {draft_id}"))?;
    let library = get_library(&conn, &draft.library_id)?
        .ok_or_else(|| format!("Libreria indexada no encontrada: {}", draft.library_id))?;
    let tracks = draft_tracks(&conn, &draft_id)?;
    if tracks.is_empty() {
        return Err("La playlist draft no tiene tracks para exportar.".to_string());
    }

    let xml = fs::read_to_string(&library.source_path)
        .map_err(|error| format!("No se pudo leer XML original: {error}"))?;
    let track_ids = tracks
        .iter()
        .map(|track| track.track_id.clone())
        .collect::<Vec<_>>();
    let exported_xml = export_with_new_playlist_xml(&xml, &draft.name, &track_ids)
        .map_err(|error| error.to_string())?;
    fs::write(&output_path, exported_xml)
        .map_err(|error| format!("No se pudo escribir XML exportado: {error}"))?;

    emit_progress(
        &app,
        "info",
        &format!("Playlist exportada: {output_path}"),
        Some(100.0),
        Some(draft.library_id),
        Some(tracks.len()),
        Some(tracks.len()),
    );

    Ok(PlaylistExportResult {
        draft_id,
        output_path,
        track_count: tracks.len(),
    })
}

fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join(DB_FILE))
        .map_err(|error| format!("No se pudo abrir SQLite playlists: {error}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| format!("No se pudo habilitar foreign keys SQLite: {error}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS playlist_index_libraries (
          id TEXT PRIMARY KEY,
          source_path TEXT NOT NULL UNIQUE,
          source_name TEXT NOT NULL,
          product_name TEXT,
          product_version TEXT,
          track_count INTEGER NOT NULL DEFAULT 0,
          playlist_count INTEGER NOT NULL DEFAULT 0,
          indexed_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_index_libraries_updated_at
          ON playlist_index_libraries(updated_at);

        CREATE TABLE IF NOT EXISTS playlist_index_tracks (
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          name TEXT,
          artist TEXT,
          album TEXT,
          kind TEXT,
          location TEXT,
          source_path TEXT,
          size_bytes INTEGER,
          total_time INTEGER,
          sample_rate INTEGER,
          bitrate INTEGER,
          source_exists INTEGER NOT NULL DEFAULT 0,
          search_text TEXT NOT NULL,
          attributes_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY(library_id, track_id),
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_index_tracks_artist_name
          ON playlist_index_tracks(library_id, artist, name);
        CREATE INDEX IF NOT EXISTS idx_playlist_index_tracks_source_path
          ON playlist_index_tracks(source_path);

        CREATE TABLE IF NOT EXISTS playlist_index_playlists (
          library_id TEXT NOT NULL,
          path TEXT NOT NULL,
          name TEXT NOT NULL,
          node_type TEXT,
          track_count INTEGER NOT NULL DEFAULT 0,
          position INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY(library_id, path),
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_index_playlists_position
          ON playlist_index_playlists(library_id, position);

        CREATE TABLE IF NOT EXISTS playlist_index_memberships (
          library_id TEXT NOT NULL,
          playlist_path TEXT NOT NULL,
          track_id TEXT NOT NULL,
          position INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY(library_id, playlist_path, track_id, position),
          FOREIGN KEY(library_id, playlist_path)
            REFERENCES playlist_index_playlists(library_id, path) ON DELETE CASCADE,
          FOREIGN KEY(library_id, track_id)
            REFERENCES playlist_index_tracks(library_id, track_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_index_memberships_track
          ON playlist_index_memberships(library_id, track_id);

        CREATE TABLE IF NOT EXISTS playlist_track_embeddings (
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          model TEXT NOT NULL,
          dimensions INTEGER NOT NULL,
          text_hash TEXT NOT NULL,
          embedding_json TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY(library_id, track_id, model, dimensions),
          FOREIGN KEY(library_id, track_id)
            REFERENCES playlist_index_tracks(library_id, track_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_track_embeddings_lookup
          ON playlist_track_embeddings(library_id, model, dimensions);

        CREATE TABLE IF NOT EXISTS playlist_drafts (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          name TEXT NOT NULL,
          description TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_drafts_library
          ON playlist_drafts(library_id, updated_at);

        CREATE TABLE IF NOT EXISTS playlist_draft_tracks (
          draft_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          position INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          PRIMARY KEY(draft_id, track_id),
          FOREIGN KEY(draft_id) REFERENCES playlist_drafts(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_draft_tracks_position
          ON playlist_draft_tracks(draft_id, position);

        CREATE VIRTUAL TABLE IF NOT EXISTS playlist_track_fts USING fts5(
          library_id UNINDEXED,
          track_id UNINDEXED,
          name,
          artist,
          album,
          kind,
          location,
          source_path,
          search_text
        );
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite playlist index: {error}"))?;

    ensure_playlist_index_track_column(conn, "attributes_json", "TEXT NOT NULL DEFAULT '{}'")?;

    Ok(())
}

fn ensure_playlist_index_track_column(
    conn: &Connection,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(playlist_index_tracks)")
        .map_err(|error| format!("No se pudo inspeccionar playlist_index_tracks: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| {
            format!("No se pudieron leer columnas de playlist_index_tracks: {error}")
        })?;
    let columns = rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        format!("No se pudieron mapear columnas de playlist_index_tracks: {error}")
    })?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    let sql = format!("ALTER TABLE playlist_index_tracks ADD COLUMN {column} {definition}");
    conn.execute(&sql, [])
        .map_err(|error| format!("No se pudo agregar columna {column}: {error}"))?;
    Ok(())
}

fn rebuild_fts(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM playlist_track_fts", [])
        .map_err(|error| format!("No se pudo limpiar FTS de playlists: {error}"))?;
    conn.execute(
        "INSERT INTO playlist_track_fts (
            library_id, track_id, name, artist, album, kind, location, source_path, search_text
         )
         SELECT library_id, track_id, COALESCE(name, ''), COALESCE(artist, ''), COALESCE(album, ''),
                COALESCE(kind, ''), COALESCE(location, ''), COALESCE(source_path, ''), search_text
         FROM playlist_index_tracks",
        [],
    )
    .map_err(|error| format!("No se pudo reconstruir FTS de playlists: {error}"))?;
    Ok(())
}

fn insert_track(
    conn: &Connection,
    library_id: &str,
    track: &Track,
    playlist_paths_by_track: &HashMap<String, Vec<String>>,
    now: &str,
) -> Result<(), String> {
    let source_path = track
        .file_path
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let source_exists = track.file_path.as_ref().is_some_and(|path| path.is_file());
    let search_text = track_search_text(track, playlist_paths_by_track);
    let attributes_json = serde_json::to_string(&track.attributes).map_err(|error| {
        format!(
            "No se pudo serializar metadata XML del track {}: {error}",
            track.track_id
        )
    })?;

    conn.execute(
        "INSERT INTO playlist_index_tracks (
            library_id, track_id, name, artist, album, kind, location, source_path,
            size_bytes, total_time, sample_rate, bitrate, source_exists, search_text,
            attributes_json, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)
         ON CONFLICT(library_id, track_id) DO UPDATE SET
            name = excluded.name,
            artist = excluded.artist,
            album = excluded.album,
            kind = excluded.kind,
            location = excluded.location,
            source_path = excluded.source_path,
            size_bytes = excluded.size_bytes,
            total_time = excluded.total_time,
            sample_rate = excluded.sample_rate,
            bitrate = excluded.bitrate,
            source_exists = excluded.source_exists,
            search_text = excluded.search_text,
            attributes_json = excluded.attributes_json,
            updated_at = excluded.updated_at",
        params![
            library_id,
            &track.track_id,
            &track.name,
            &track.artist,
            &track.album,
            &track.kind,
            &track.location,
            &source_path,
            track.size.map(|value| value as i64),
            track.total_time.map(|value| value as i64),
            track.sample_rate.map(|value| value as i64),
            track.bitrate.map(|value| value as i64),
            if source_exists { 1_i64 } else { 0_i64 },
            &search_text,
            &attributes_json,
            now
        ],
    )
    .map_err(|error| format!("No se pudo guardar track {}: {error}", track.track_id))?;

    Ok(())
}

fn list_libraries(conn: &Connection) -> Result<Vec<PlaylistIndexLibrary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.id, l.source_path, l.source_name, l.product_name, l.product_version,
                    l.track_count, l.playlist_count,
                    COUNT(e.track_id) AS embedded_track_count,
                    SUM(CASE WHEN t.source_exists = 0 THEN 1 ELSE 0 END) AS missing_file_count,
                    l.indexed_at, l.updated_at
             FROM playlist_index_libraries l
             LEFT JOIN playlist_index_tracks t ON t.library_id = l.id
             LEFT JOIN playlist_track_embeddings e ON e.library_id = t.library_id
                AND e.track_id = t.track_id
                AND e.model = ?1
                AND e.dimensions = ?2
             GROUP BY l.id
             ORDER BY l.updated_at DESC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de librerias: {error}"))?;
    let rows = stmt
        .query_map(
            params![EMBEDDING_MODEL, EMBEDDING_DIMENSIONS as i64],
            row_to_library,
        )
        .map_err(|error| format!("No se pudieron leer librerias: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear librerias: {error}"))
}

fn get_library(
    conn: &Connection,
    library_id: &str,
) -> Result<Option<PlaylistIndexLibrary>, String> {
    conn.query_row(
        "SELECT l.id, l.source_path, l.source_name, l.product_name, l.product_version,
                l.track_count, l.playlist_count,
                COUNT(e.track_id) AS embedded_track_count,
                SUM(CASE WHEN t.source_exists = 0 THEN 1 ELSE 0 END) AS missing_file_count,
                l.indexed_at, l.updated_at
         FROM playlist_index_libraries l
         LEFT JOIN playlist_index_tracks t ON t.library_id = l.id
         LEFT JOIN playlist_track_embeddings e ON e.library_id = t.library_id
            AND e.track_id = t.track_id
            AND e.model = ?2
            AND e.dimensions = ?3
         WHERE l.id = ?1
         GROUP BY l.id",
        params![library_id, EMBEDDING_MODEL, EMBEDDING_DIMENSIONS as i64],
        row_to_library,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer libreria indexada: {error}"))
}

fn row_to_library(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistIndexLibrary> {
    Ok(PlaylistIndexLibrary {
        id: row.get(0)?,
        source_path: row.get(1)?,
        source_name: row.get(2)?,
        product_name: row.get(3)?,
        product_version: row.get(4)?,
        track_count: i64_to_usize(row.get(5)?),
        playlist_count: i64_to_usize(row.get(6)?),
        embedded_track_count: i64_to_usize(row.get(7)?),
        missing_file_count: i64_to_usize(row.get::<_, Option<i64>>(8)?.unwrap_or_default()),
        indexed_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn list_playlists(
    conn: &Connection,
    library_id: &str,
) -> Result<Vec<PlaylistIndexPlaylist>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT library_id, path, name, node_type, track_count, position
             FROM playlist_index_playlists
             WHERE library_id = ?1 AND node_type = '1'
             ORDER BY position ASC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de playlists: {error}"))?;
    let rows = stmt
        .query_map(params![library_id], row_to_playlist)
        .map_err(|error| format!("No se pudieron leer playlists: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear playlists: {error}"))
}

fn row_to_playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistIndexPlaylist> {
    Ok(PlaylistIndexPlaylist {
        library_id: row.get(0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        node_type: row.get(3)?,
        track_count: i64_to_usize(row.get(4)?),
        position: i64_to_usize(row.get(5)?),
    })
}

fn list_playlist_tracks(
    conn: &Connection,
    library_id: &str,
    playlist_path: &str,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {}
             FROM playlist_index_memberships m
             JOIN playlist_index_tracks t
               ON t.library_id = m.library_id AND t.track_id = m.track_id
             WHERE m.library_id = ?1 AND m.playlist_path = ?2
             ORDER BY m.position ASC",
            track_select_clause()
        ))
        .map_err(|error| format!("No se pudo preparar tracks de playlist: {error}"))?;
    let rows = stmt
        .query_map(params![library_id, playlist_path], row_to_track)
        .map_err(|error| format!("No se pudieron leer tracks de playlist: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear tracks de playlist: {error}"))
}

fn lexical_search(
    conn: &Connection,
    library_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<PlaylistSearchResult>, String> {
    let Some(fts_query) = fts_query(query) else {
        return list_tracks(conn, library_id, limit, "library");
    };

    let library_filter = if library_id.is_some() {
        "AND t.library_id = ?2"
    } else {
        ""
    };
    let limit_param_index = if library_id.is_some() { "?3" } else { "?2" };
    let sql = format!(
        "SELECT {}, bm25(playlist_track_fts) AS score
         FROM playlist_track_fts f
         JOIN playlist_index_tracks t
           ON t.library_id = f.library_id AND t.track_id = f.track_id
         WHERE playlist_track_fts MATCH ?1
         {library_filter}
         ORDER BY score ASC
         LIMIT {limit_param_index}",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar busqueda FTS: {error}"))?;

    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<PlaylistSearchResult> {
        Ok(PlaylistSearchResult {
            track: row_to_track(row)?,
            score: row.get::<_, f64>(16)?,
            mode: "lexical".to_string(),
        })
    };

    if let Some(library_id) = library_id {
        let rows = stmt
            .query_map(params![fts_query, library_id, limit as i64], map_row)
            .map_err(|error| format!("No se pudo ejecutar busqueda FTS: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear resultados FTS: {error}"))
    } else {
        let rows = stmt
            .query_map(params![fts_query, limit as i64], map_row)
            .map_err(|error| format!("No se pudo ejecutar busqueda FTS: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear resultados FTS: {error}"))
    }
}

fn list_tracks(
    conn: &Connection,
    library_id: Option<&str>,
    limit: usize,
    mode: &str,
) -> Result<Vec<PlaylistSearchResult>, String> {
    let library_filter = if library_id.is_some() {
        "WHERE t.library_id = ?1"
    } else {
        ""
    };
    let limit_param_index = if library_id.is_some() { "?2" } else { "?1" };
    let sql = format!(
        "SELECT {}
         FROM playlist_index_tracks t
         {library_filter}
         ORDER BY COALESCE(t.artist, ''), COALESCE(t.name, ''), t.track_id
         LIMIT {limit_param_index}",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar listado de tracks: {error}"))?;

    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<PlaylistSearchResult> {
        Ok(PlaylistSearchResult {
            track: row_to_track(row)?,
            score: 0.0,
            mode: mode.to_string(),
        })
    };

    if let Some(library_id) = library_id {
        let rows = stmt
            .query_map(params![library_id, limit as i64], map_row)
            .map_err(|error| format!("No se pudieron listar tracks: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks: {error}"))
    } else {
        let rows = stmt
            .query_map(params![limit as i64], map_row)
            .map_err(|error| format!("No se pudieron listar tracks: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks: {error}"))
    }
}

fn list_track_groups(
    conn: &Connection,
    library_id: &str,
    kind: &str,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<PlaylistIndexGroup>, String> {
    let column = track_group_column(kind)?;
    let value_expression = format!("COALESCE(NULLIF(TRIM(t.{column}), ''), '')");
    let query_filter = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|_| format!("AND LOWER({value_expression}) LIKE ?2"))
        .unwrap_or_default();
    let limit_param = if query_filter.is_empty() { "?2" } else { "?3" };
    let sql = format!(
        "SELECT {value_expression} AS value, COUNT(*) AS track_count
         FROM playlist_index_tracks t
         WHERE t.library_id = ?1
         {query_filter}
         GROUP BY value
         ORDER BY CASE WHEN value = '' THEN 1 ELSE 0 END, value COLLATE NOCASE
         LIMIT {limit_param}"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar navegador de {kind}: {error}"))?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<PlaylistIndexGroup> {
        let value: String = row.get(0)?;
        Ok(PlaylistIndexGroup {
            library_id: library_id.to_string(),
            kind: kind.to_string(),
            name: track_group_name(kind, &value),
            value,
            track_count: i64_to_usize(row.get(1)?),
        })
    };

    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = like_pattern(query);
        let rows = stmt
            .query_map(params![library_id, pattern, limit as i64], map_row)
            .map_err(|error| format!("No se pudieron leer grupos de {kind}: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear grupos de {kind}: {error}"))
    } else {
        let rows = stmt
            .query_map(params![library_id, limit as i64], map_row)
            .map_err(|error| format!("No se pudieron leer grupos de {kind}: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear grupos de {kind}: {error}"))
    }
}

fn list_group_tracks(
    conn: &Connection,
    library_id: &str,
    kind: &str,
    value: &str,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let column = track_group_column(kind)?;
    let value_expression = format!("COALESCE(NULLIF(TRIM(t.{column}), ''), '')");
    let query_filter = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|_| "AND LOWER(t.search_text) LIKE ?3")
        .unwrap_or_default();
    let limit_param = if query_filter.is_empty() { "?3" } else { "?4" };
    let sql = format!(
        "SELECT {}
         FROM playlist_index_tracks t
         WHERE t.library_id = ?1
           AND {value_expression} = ?2
         {query_filter}
         ORDER BY COALESCE(t.artist, ''), COALESCE(t.album, ''), COALESCE(t.name, ''), t.track_id
         LIMIT {limit_param}",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar tracks de {kind}: {error}"))?;

    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = like_pattern(query);
        let rows = stmt
            .query_map(
                params![library_id, value, pattern, limit as i64],
                row_to_track,
            )
            .map_err(|error| format!("No se pudieron leer tracks de {kind}: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks de {kind}: {error}"))
    } else {
        let rows = stmt
            .query_map(params![library_id, value, limit as i64], row_to_track)
            .map_err(|error| format!("No se pudieron leer tracks de {kind}: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks de {kind}: {error}"))
    }
}

fn taxonomy_overview(
    conn: &Connection,
    library_id: &str,
) -> Result<PlaylistTaxonomyOverview, String> {
    let library = get_library(conn, library_id)?
        .ok_or_else(|| format!("Libreria indexada no encontrada: {library_id}"))?;
    let tracks = list_taxonomy_tracks(conn, library_id)?;

    let mut genre_counts = BTreeMap::<String, usize>::new();
    let mut artist_counts = BTreeMap::<String, usize>::new();
    let mut album_counts = BTreeMap::<String, usize>::new();
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut format_counts = BTreeMap::<String, usize>::new();
    let mut year_counts = BTreeMap::<String, usize>::new();
    let mut bpm_bucket_counts = BTreeMap::<String, usize>::new();
    let mut bpm_known_count = 0_usize;
    let mut bpm_missing_count = 0_usize;
    let mut bpm_sum = 0_f64;
    let mut bpm_min: Option<f64> = None;
    let mut bpm_max: Option<f64> = None;
    let mut genre_missing_count = 0_usize;
    let mut key_missing_count = 0_usize;
    let mut source_missing_count = 0_usize;

    for track in &tracks {
        let genre = taxonomy_value(track.genre.as_deref());
        if genre.is_empty() {
            genre_missing_count += 1;
        }
        increment_count(&mut genre_counts, genre);

        increment_count(&mut artist_counts, taxonomy_value(track.artist.as_deref()));
        increment_count(&mut album_counts, taxonomy_value(track.album.as_deref()));

        let key = taxonomy_value(track.key.as_deref());
        if key.is_empty() {
            key_missing_count += 1;
        }
        increment_count(&mut key_counts, key);
        increment_count(&mut format_counts, taxonomy_value(track.kind.as_deref()));
        increment_count(&mut year_counts, taxonomy_value(track.year.as_deref()));

        let bpm = track_bpm_value(track);
        let (bucket, _) = bpm_bucket_for(bpm);
        increment_count(&mut bpm_bucket_counts, bucket.to_string());
        if let Some(value) = bpm {
            bpm_known_count += 1;
            bpm_sum += value;
            bpm_min = Some(bpm_min.map_or(value, |current| current.min(value)));
            bpm_max = Some(bpm_max.map_or(value, |current| current.max(value)));
        } else {
            bpm_missing_count += 1;
        }

        if !track.source_exists {
            source_missing_count += 1;
        }
    }

    Ok(PlaylistTaxonomyOverview {
        playlist_count: library.playlist_count,
        track_count: tracks.len(),
        genre_count: non_empty_count(&genre_counts),
        artist_count: non_empty_count(&artist_counts),
        album_count: non_empty_count(&album_counts),
        key_count: non_empty_count(&key_counts),
        bpm_known_count,
        bpm_missing_count,
        bpm_average: (bpm_known_count > 0).then_some(bpm_sum / bpm_known_count as f64),
        bpm_min,
        bpm_max,
        genre_missing_count,
        key_missing_count,
        source_missing_count,
        genres: counts_to_taxonomy("genre", &genre_counts, "Sin genero", 40, true),
        bpm_buckets: bpm_counts_to_taxonomy(&bpm_bucket_counts),
        keys: counts_to_taxonomy("key", &key_counts, "Sin key", 40, true),
        formats: counts_to_taxonomy("format", &format_counts, "Formato desconocido", 20, true),
        years: counts_to_taxonomy("year", &year_counts, "Sin ano", 30, true),
        metadata_gaps: vec![
            TaxonomyCount {
                kind: "metadata_gap".to_string(),
                value: "missing_genre".to_string(),
                name: "Sin genero".to_string(),
                count: genre_missing_count,
            },
            TaxonomyCount {
                kind: "metadata_gap".to_string(),
                value: "missing_bpm".to_string(),
                name: "Sin BPM".to_string(),
                count: bpm_missing_count,
            },
            TaxonomyCount {
                kind: "metadata_gap".to_string(),
                value: "missing_key".to_string(),
                name: "Sin key".to_string(),
                count: key_missing_count,
            },
            TaxonomyCount {
                kind: "metadata_gap".to_string(),
                value: "source_missing".to_string(),
                name: "Archivo no encontrado".to_string(),
                count: source_missing_count,
            },
        ],
        library,
    })
}

fn taxonomy_graph(
    conn: &Connection,
    library_id: &str,
    limit: usize,
) -> Result<PlaylistTaxonomyGraph, String> {
    let tracks = list_taxonomy_tracks(conn, library_id)?;

    let mut genre_counts = BTreeMap::<String, usize>::new();
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut bpm_bucket_counts = BTreeMap::<String, usize>::new();

    for track in &tracks {
        increment_count(&mut genre_counts, taxonomy_value(track.genre.as_deref()));
        increment_count(&mut key_counts, taxonomy_value(track.key.as_deref()));
        let (bucket, _) = bpm_bucket_for(track_bpm_value(track));
        increment_count(&mut bpm_bucket_counts, bucket.to_string());
    }

    let top_genres = top_count_values(&genre_counts, limit, true);
    let top_keys = top_count_values(&key_counts, limit.min(10), true);
    let top_bpm_buckets = bpm_bucket_counts
        .iter()
        .filter(|(value, _)| value.as_str() != "missing")
        .map(|(value, _)| value.clone())
        .collect::<BTreeSet<_>>();

    let mut nodes = Vec::<TaxonomyGraphNode>::new();
    let mut edge_counts = HashMap::<(String, String), usize>::new();

    for value in &top_genres {
        let count = genre_counts.get(value).copied().unwrap_or_default();
        nodes.push(taxonomy_node("genre", value, value, count));
    }

    for bucket in &top_bpm_buckets {
        let (_, label) = bpm_bucket_for_value(bucket);
        let count = bpm_bucket_counts.get(bucket).copied().unwrap_or_default();
        nodes.push(taxonomy_node("bpm", bucket, label, count));
    }

    for value in &top_keys {
        let count = key_counts.get(value).copied().unwrap_or_default();
        nodes.push(taxonomy_node("key", value, value, count));
    }

    for track in &tracks {
        let genre = taxonomy_value(track.genre.as_deref());
        if !top_genres.contains(&genre) {
            continue;
        }

        let genre_id = taxonomy_node_id("genre", &genre);
        let (bucket, _) = bpm_bucket_for(track_bpm_value(track));
        if top_bpm_buckets.contains(bucket) {
            increment_edge(
                &mut edge_counts,
                genre_id.clone(),
                taxonomy_node_id("bpm", bucket),
            );
        }

        let key = taxonomy_value(track.key.as_deref());
        if top_keys.contains(&key) {
            increment_edge(
                &mut edge_counts,
                genre_id.clone(),
                taxonomy_node_id("key", &key),
            );
            let (bucket, _) = bpm_bucket_for(track_bpm_value(track));
            if top_bpm_buckets.contains(bucket) {
                increment_edge(
                    &mut edge_counts,
                    taxonomy_node_id("bpm", bucket),
                    taxonomy_node_id("key", &key),
                );
            }
        }
    }

    let min_edge_count = (tracks.len() / 750).max(2);
    let mut edges = edge_counts
        .into_iter()
        .filter(|(_, count)| *count >= min_edge_count)
        .map(|((source, target), count)| TaxonomyGraphEdge {
            id: format!("{source}->{target}"),
            source,
            target,
            count,
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.id.cmp(&right.id))
    });
    edges.truncate(140);

    Ok(PlaylistTaxonomyGraph { nodes, edges })
}

fn taxonomy_tracks(
    conn: &Connection,
    library_id: &str,
    kind: &str,
    value: &str,
    limit: usize,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    if kind == "playlist" {
        return list_playlist_tracks(conn, library_id, value)
            .map(|tracks| tracks.into_iter().take(limit).collect());
    }

    let tracks = list_taxonomy_tracks(conn, library_id)?;
    Ok(tracks
        .into_iter()
        .filter(|track| taxonomy_track_matches(track, kind, value))
        .take(limit)
        .collect())
}

fn list_taxonomy_tracks(
    conn: &Connection,
    library_id: &str,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {}
             FROM playlist_index_tracks t
             WHERE t.library_id = ?1
             ORDER BY COALESCE(t.artist, ''), COALESCE(t.album, ''), COALESCE(t.name, ''), t.track_id",
            track_select_clause()
        ))
        .map_err(|error| format!("No se pudo preparar tracks de taxonomia: {error}"))?;
    let rows = stmt
        .query_map(params![library_id], row_to_track)
        .map_err(|error| format!("No se pudieron leer tracks de taxonomia: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear tracks de taxonomia: {error}"))
}

fn taxonomy_track_matches(track: &PlaylistIndexTrack, kind: &str, value: &str) -> bool {
    match kind {
        "genre" => taxonomy_value(track.genre.as_deref()) == value,
        "artist" => taxonomy_value(track.artist.as_deref()) == value,
        "album" => taxonomy_value(track.album.as_deref()) == value,
        "key" => taxonomy_value(track.key.as_deref()) == value,
        "format" => taxonomy_value(track.kind.as_deref()) == value,
        "year" => taxonomy_value(track.year.as_deref()) == value,
        "bpm" => {
            let (bucket, _) = bpm_bucket_for(track_bpm_value(track));
            bucket == value
        }
        "metadata_gap" => match value {
            "missing_genre" => taxonomy_value(track.genre.as_deref()).is_empty(),
            "missing_bpm" => track_bpm_value(track).is_none(),
            "missing_key" => taxonomy_value(track.key.as_deref()).is_empty(),
            "source_missing" => !track.source_exists,
            _ => false,
        },
        _ => false,
    }
}

fn counts_to_taxonomy(
    kind: &str,
    counts: &BTreeMap<String, usize>,
    missing_name: &str,
    limit: usize,
    include_empty: bool,
) -> Vec<TaxonomyCount> {
    let mut items = counts
        .iter()
        .filter(|(value, _)| include_empty || !value.is_empty())
        .map(|(value, count)| TaxonomyCount {
            kind: kind.to_string(),
            value: value.clone(),
            name: if value.is_empty() {
                missing_name.to_string()
            } else {
                value.clone()
            },
            count: *count,
        })
        .collect::<Vec<_>>();
    sort_taxonomy_counts(&mut items);
    items.truncate(limit);
    items
}

fn bpm_counts_to_taxonomy(counts: &BTreeMap<String, usize>) -> Vec<TaxonomyCount> {
    bpm_bucket_order()
        .iter()
        .filter_map(|(value, name)| {
            let count = counts.get(*value).copied().unwrap_or_default();
            (count > 0).then(|| TaxonomyCount {
                kind: "bpm".to_string(),
                value: (*value).to_string(),
                name: (*name).to_string(),
                count,
            })
        })
        .collect()
}

fn top_count_values(
    counts: &BTreeMap<String, usize>,
    limit: usize,
    exclude_empty: bool,
) -> BTreeSet<String> {
    let mut items = counts
        .iter()
        .filter(|(value, _)| !exclude_empty || !value.is_empty())
        .map(|(value, count)| TaxonomyCount {
            kind: String::new(),
            value: value.clone(),
            name: value.clone(),
            count: *count,
        })
        .collect::<Vec<_>>();
    sort_taxonomy_counts(&mut items);
    items
        .into_iter()
        .take(limit)
        .map(|item| item.value)
        .collect()
}

fn sort_taxonomy_counts(items: &mut [TaxonomyCount]) {
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

fn taxonomy_node(kind: &str, value: &str, label: &str, count: usize) -> TaxonomyGraphNode {
    TaxonomyGraphNode {
        id: taxonomy_node_id(kind, value),
        kind: kind.to_string(),
        value: value.to_string(),
        label: label.to_string(),
        count,
    }
}

fn taxonomy_node_id(kind: &str, value: &str) -> String {
    format!("{kind}:{}", stable_hash(value))
}

fn increment_count(counts: &mut BTreeMap<String, usize>, value: String) {
    *counts.entry(value).or_insert(0) += 1;
}

fn increment_edge(edges: &mut HashMap<(String, String), usize>, source: String, target: String) {
    *edges.entry((source, target)).or_insert(0) += 1;
}

fn non_empty_count(counts: &BTreeMap<String, usize>) -> usize {
    counts.keys().filter(|value| !value.is_empty()).count()
}

fn taxonomy_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn track_bpm_value(track: &PlaylistIndexTrack) -> Option<f64> {
    track
        .bpm
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.replace(',', ".").parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn bpm_bucket_for(bpm: Option<f64>) -> (&'static str, &'static str) {
    match bpm {
        None => ("missing", "Sin BPM"),
        Some(value) if value < 90.0 => ("lt90", "< 90"),
        Some(value) if value < 100.0 => ("90_100", "90-100"),
        Some(value) if value < 110.0 => ("100_110", "100-110"),
        Some(value) if value < 120.0 => ("110_120", "110-120"),
        Some(value) if value < 128.0 => ("120_128", "120-128"),
        Some(value) if value < 135.0 => ("128_135", "128-135"),
        Some(_) => ("gte135", "135+"),
    }
}

fn bpm_bucket_for_value(value: &str) -> (&'static str, &'static str) {
    bpm_bucket_order()
        .iter()
        .find(|(bucket, _)| *bucket == value)
        .copied()
        .unwrap_or(("missing", "Sin BPM"))
}

fn bpm_bucket_order() -> &'static [(&'static str, &'static str)] {
    &[
        ("lt90", "< 90"),
        ("90_100", "90-100"),
        ("100_110", "100-110"),
        ("110_120", "110-120"),
        ("120_128", "120-128"),
        ("128_135", "128-135"),
        ("gte135", "135+"),
        ("missing", "Sin BPM"),
    ]
}

fn track_group_column(kind: &str) -> Result<&'static str, String> {
    match kind {
        "artist" => Ok("artist"),
        "album" => Ok("album"),
        _ => Err(format!("Tipo de navegador no soportado: {kind}")),
    }
}

fn track_group_name(kind: &str, value: &str) -> String {
    if !value.trim().is_empty() {
        return value.to_string();
    }

    match kind {
        "artist" => "Sin artista".to_string(),
        "album" => "Sin album".to_string(),
        _ => "Sin metadata".to_string(),
    }
}

fn like_pattern(query: &str) -> String {
    format!("%{}%", query.trim().to_ascii_lowercase())
}

fn extract_track_cover(app: &AppHandle, source_path: &str) -> Result<Option<String>, String> {
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Ok(None);
    }

    let metadata = fs::metadata(&source)
        .map_err(|error| format!("No se pudo leer metadata del audio: {error}"))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or_default();
    let cache_key = stable_hash(&format!(
        "{}:{}:{}",
        source.to_string_lossy(),
        metadata.len(),
        modified
    ));
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))?
        .join("cover-cache");
    fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("No se pudo crear cache de portadas: {error}"))?;

    let cover_path = cache_dir.join(format!("{cache_key}.jpg"));
    let miss_path = cache_dir.join(format!("{cache_key}.none"));
    if cover_path.is_file() {
        return Ok(Some(cover_path.to_string_lossy().into_owned()));
    }
    if miss_path.is_file() {
        return Ok(None);
    }

    let output = system::ffmpeg_command(app)
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&source)
        .arg("-an")
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg(&cover_path)
        .output();

    match output {
        Ok(output)
            if output.status.success() && cover_path.is_file() && file_has_content(&cover_path) =>
        {
            Ok(Some(cover_path.to_string_lossy().into_owned()))
        }
        _ => {
            let _ = fs::remove_file(&cover_path);
            let _ = fs::write(&miss_path, b"");
            Ok(None)
        }
    }
}

fn file_has_content(path: &PathBuf) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct PlaylistCopilotProfile {
    track_count: usize,
    genres: Vec<String>,
    artists: Vec<String>,
    keys: Vec<String>,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
}

fn playlist_copilot_generate_blocking(
    app: AppHandle,
    request: PlaylistCopilotRequest,
) -> Result<PlaylistCopilotResponse, String> {
    let prompt = request.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("Describe la playlist que quieres generar.".to_string());
    }

    let target_count = request.target_count.unwrap_or(30).clamp(5, 120);
    let conn = open_db(&app)?;
    let library = get_library(&conn, &request.library_id)?
        .ok_or_else(|| format!("Libreria indexada no encontrada: {}", request.library_id))?;
    let tracks = list_taxonomy_tracks(&conn, &request.library_id)?;
    if tracks.is_empty() {
        return Err("La libreria indexada no tiene tracks para sugerir.".to_string());
    }

    let profile = playlist_copilot_profile(&tracks);
    let api_key = settings::load_openai_api_key(&app)?;
    let mut used_openai = false;
    let mut openai_error = None;
    let mut interpreted = if let Some(api_key) = api_key.as_deref() {
        match request_copilot_interpretation(api_key, &prompt, &profile, target_count) {
            Ok(interpretation) => {
                used_openai = true;
                interpretation
            }
            Err(error) => {
                openai_error = Some(error);
                local_copilot_interpretation(&prompt, &profile, target_count)
            }
        }
    } else {
        local_copilot_interpretation(&prompt, &profile, target_count)
    };
    interpreted.target_count = Some(target_count);
    interpreted = normalize_copilot_interpretation(interpreted);

    let semantic_scores = if api_key.is_some() && tracks.iter().any(|track| track.embedding_ready) {
        playlist_copilot_semantic_scores(&app, &conn, &request.library_id, &prompt)
    } else {
        HashMap::new()
    };
    let candidates = rank_copilot_candidates(
        &tracks,
        &interpreted,
        &prompt,
        target_count,
        &semantic_scores,
    );
    let questions = playlist_copilot_questions(&interpreted, candidates.len());
    let mut message = if used_openai {
        format!(
            "Interprete el brief con OpenAI y encontre {} candidato(s) en {}.",
            candidates.len(),
            library.source_name
        )
    } else {
        format!(
            "Use ranking local y encontre {} candidato(s) en {}.",
            candidates.len(),
            library.source_name
        )
    };
    if let Some(error) = openai_error {
        message.push_str(&format!(" OpenAI no respondio correctamente: {error}"));
    }

    Ok(PlaylistCopilotResponse {
        message,
        interpreted,
        questions,
        candidates,
        used_openai,
    })
}

fn playlist_copilot_profile(tracks: &[PlaylistIndexTrack]) -> PlaylistCopilotProfile {
    let mut genre_counts = BTreeMap::<String, usize>::new();
    let mut artist_counts = BTreeMap::<String, usize>::new();
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut bpm_min: Option<f64> = None;
    let mut bpm_max: Option<f64> = None;

    for track in tracks {
        increment_count(&mut genre_counts, taxonomy_value(track.genre.as_deref()));
        increment_count(&mut artist_counts, taxonomy_value(track.artist.as_deref()));
        increment_count(&mut key_counts, taxonomy_value(track.key.as_deref()));
        if let Some(bpm) = track_bpm_value(track) {
            bpm_min = Some(bpm_min.map_or(bpm, |current| current.min(bpm)));
            bpm_max = Some(bpm_max.map_or(bpm, |current| current.max(bpm)));
        }
    }

    PlaylistCopilotProfile {
        track_count: tracks.len(),
        genres: top_profile_values(&genre_counts, 80),
        artists: top_profile_values(&artist_counts, 160),
        keys: top_profile_values(&key_counts, 32),
        bpm_min,
        bpm_max,
    }
}

fn top_profile_values(counts: &BTreeMap<String, usize>, limit: usize) -> Vec<String> {
    let mut items = counts
        .iter()
        .filter(|(value, _)| !value.is_empty())
        .map(|(value, count)| TaxonomyCount {
            kind: String::new(),
            value: value.clone(),
            name: value.clone(),
            count: *count,
        })
        .collect::<Vec<_>>();
    sort_taxonomy_counts(&mut items);
    items
        .into_iter()
        .take(limit)
        .map(|item| item.value)
        .collect()
}

fn request_copilot_interpretation(
    api_key: &str,
    prompt: &str,
    profile: &PlaylistCopilotProfile,
    target_count: usize,
) -> Result<PlaylistCopilotInterpretation, String> {
    let system_prompt = [
        "You are a DJ playlist planning assistant.",
        "Return only JSON. Do not include markdown.",
        "Infer filters from the user's brief using the provided local library profile.",
        "Use this JSON shape:",
        r#"{"genres":[],"artists":[],"keys":[],"bpm_min":null,"bpm_max":null,"mood":null,"energy":null,"exclude_terms":[],"target_count":30}"#,
        "Keep arrays short and use values that can match the library profile when possible.",
    ]
    .join(" ");
    let user_prompt = format!(
        "Library profile:\n{}\n\nTarget track count: {}\nBrief: {}",
        playlist_copilot_profile_summary(profile),
        target_count,
        prompt
    );
    let body = json!({
        "model": "gpt-4o-mini",
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ]
    });
    let response = reqwest::blocking::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|error| format!("OpenAI chat request fallo: {error}"))?
        .error_for_status()
        .map_err(|error| format!("OpenAI chat retorno error: {error}"))?
        .json::<Value>()
        .map_err(|error| format!("OpenAI chat retorno JSON invalido: {error}"))?;
    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| "OpenAI no retorno contenido para el Copilot.".to_string())?;

    parse_copilot_interpretation_json(content)
}

fn playlist_copilot_profile_summary(profile: &PlaylistCopilotProfile) -> String {
    format!(
        "tracks: {}\ngenres: {}\nartists: {}\nkeys: {}\nbpm_range: {}-{}",
        profile.track_count,
        profile
            .genres
            .iter()
            .take(32)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        profile
            .artists
            .iter()
            .take(48)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        profile
            .keys
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        profile
            .bpm_min
            .map(|value| format!("{value:.0}"))
            .unwrap_or_else(|| "unknown".to_string()),
        profile
            .bpm_max
            .map(|value| format!("{value:.0}"))
            .unwrap_or_else(|| "unknown".to_string())
    )
}

fn parse_copilot_interpretation_json(
    content: &str,
) -> Result<PlaylistCopilotInterpretation, String> {
    let value = serde_json::from_str::<Value>(content)
        .map_err(|error| format!("OpenAI retorno interpretacion invalida: {error}"))?;
    let interpretation_value = value
        .get("interpretation")
        .or_else(|| value.get("interpreted"))
        .unwrap_or(&value)
        .clone();
    let interpretation =
        serde_json::from_value::<PlaylistCopilotInterpretation>(interpretation_value)
            .map_err(|error| format!("OpenAI retorno campos invalidos: {error}"))?;
    Ok(normalize_copilot_interpretation(interpretation))
}

fn local_copilot_interpretation(
    prompt: &str,
    profile: &PlaylistCopilotProfile,
    target_count: usize,
) -> PlaylistCopilotInterpretation {
    let normalized_prompt = normalize_for_match(prompt);
    let bpm_values = prompt_numbers(prompt)
        .into_iter()
        .filter(|value| (50.0..=220.0).contains(value))
        .collect::<Vec<_>>();
    let (bpm_min, bpm_max) = match bpm_values.as_slice() {
        [single] if normalized_prompt.contains("bpm") => {
            let value = *single;
            (Some((value - 4.0).max(50.0)), Some(value + 4.0))
        }
        [first, second, ..] => (Some((*first).min(*second)), Some((*first).max(*second))),
        _ => (None, None),
    };

    normalize_copilot_interpretation(PlaylistCopilotInterpretation {
        genres: profile_matches(&normalized_prompt, &profile.genres, 6),
        artists: profile_matches(&normalized_prompt, &profile.artists, 8),
        keys: profile_matches(&normalized_prompt, &profile.keys, 6),
        bpm_min,
        bpm_max,
        mood: prompt_mood(&normalized_prompt),
        energy: prompt_energy(&normalized_prompt),
        exclude_terms: prompt_exclude_terms(prompt),
        target_count: Some(target_count),
    })
}

fn normalize_copilot_interpretation(
    mut interpretation: PlaylistCopilotInterpretation,
) -> PlaylistCopilotInterpretation {
    interpretation.genres = clean_copilot_terms(interpretation.genres, 8);
    interpretation.artists = clean_copilot_terms(interpretation.artists, 10);
    interpretation.keys = clean_copilot_terms(interpretation.keys, 8);
    interpretation.exclude_terms = clean_copilot_terms(interpretation.exclude_terms, 12);
    interpretation.mood = clean_optional_string(interpretation.mood);
    interpretation.energy = clean_optional_string(interpretation.energy);
    interpretation.bpm_min = clean_bpm_filter(interpretation.bpm_min);
    interpretation.bpm_max = clean_bpm_filter(interpretation.bpm_max);
    if let (Some(min), Some(max)) = (interpretation.bpm_min, interpretation.bpm_max) {
        if min > max {
            interpretation.bpm_min = Some(max);
            interpretation.bpm_max = Some(min);
        }
    }
    interpretation.target_count = interpretation.target_count.map(|value| value.clamp(5, 120));
    interpretation
}

fn clean_copilot_terms(values: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(normalize_for_match(value)))
        .take(limit)
        .collect()
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_bpm_filter(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && (50.0..=220.0).contains(value))
}

fn profile_matches(prompt: &str, values: &[String], limit: usize) -> Vec<String> {
    values
        .iter()
        .filter(|value| normalized_contains_phrase(prompt, &normalize_for_match(value)))
        .take(limit)
        .cloned()
        .collect()
}

fn prompt_numbers(prompt: &str) -> Vec<f64> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for character in prompt.chars() {
        if character.is_ascii_digit() || character == '.' || character == ',' {
            current.push(if character == ',' { '.' } else { character });
        } else if !current.is_empty() {
            if let Ok(value) = current.parse::<f64>() {
                numbers.push(value);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Ok(value) = current.parse::<f64>() {
            numbers.push(value);
        }
    }
    numbers
}

fn prompt_mood(prompt: &str) -> Option<String> {
    let moods = [
        ("dark", "dark"),
        ("oscuro", "oscuro"),
        ("melodic", "melodic"),
        ("melodico", "melodico"),
        ("warm", "warm"),
        ("calido", "calido"),
        ("vocal", "vocal"),
        ("funk", "funk"),
        ("deep", "deep"),
        ("groove", "groove"),
        ("hypnotic", "hypnotic"),
        ("hipnotico", "hipnotico"),
    ];
    moods
        .iter()
        .find(|(needle, _)| normalized_contains_phrase(prompt, needle))
        .map(|(_, mood)| (*mood).to_string())
}

fn prompt_energy(prompt: &str) -> Option<String> {
    if ["warmup", "opening", "abrir", "inicio", "suave"]
        .iter()
        .any(|needle| normalized_contains_phrase(prompt, needle))
    {
        return Some("warmup".to_string());
    }
    if ["peak", "alto", "alta", "fuerte", "club", "main"]
        .iter()
        .any(|needle| normalized_contains_phrase(prompt, needle))
    {
        return Some("peak".to_string());
    }
    if ["closing", "cierre", "after", "late"]
        .iter()
        .any(|needle| normalized_contains_phrase(prompt, needle))
    {
        return Some("closing".to_string());
    }
    None
}

fn prompt_exclude_terms(prompt: &str) -> Vec<String> {
    let tokens = prompt
        .split_whitespace()
        .map(|value| value.trim_matches(|character: char| !character.is_alphanumeric()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut excludes = Vec::new();
    for pair in tokens.windows(2) {
        let marker = pair[0].to_lowercase();
        if matches!(marker.as_str(), "sin" | "no" | "without" | "exclude") {
            excludes.push(pair[1].to_string());
        }
    }
    clean_copilot_terms(excludes, 8)
}

fn playlist_copilot_semantic_scores(
    app: &AppHandle,
    conn: &Connection,
    library_id: &str,
    prompt: &str,
) -> HashMap<String, f64> {
    semantic_search(app, conn, Some(library_id), prompt, 220)
        .unwrap_or_default()
        .into_iter()
        .map(|result| (result.track.track_id, result.score))
        .collect()
}

fn rank_copilot_candidates(
    tracks: &[PlaylistIndexTrack],
    interpreted: &PlaylistCopilotInterpretation,
    prompt: &str,
    target_count: usize,
    semantic_scores: &HashMap<String, f64>,
) -> Vec<PlaylistCopilotCandidate> {
    let prompt_terms = copilot_prompt_terms(prompt);
    let normalized_excludes = interpreted
        .exclude_terms
        .iter()
        .map(|value| normalize_for_match(value))
        .collect::<Vec<_>>();
    let mut candidates = tracks
        .iter()
        .filter_map(|track| {
            let normalized_text = normalize_for_match(&track.search_text);
            if normalized_excludes
                .iter()
                .any(|term| !term.is_empty() && normalized_contains_phrase(&normalized_text, term))
            {
                return None;
            }

            let mut score = 0.0_f64;
            let mut reasons = Vec::<String>::new();

            if let Some(semantic_score) = semantic_scores.get(&track.track_id).copied() {
                let boost = ((semantic_score - 0.45).max(0.0) * 80.0).min(36.0);
                if boost > 2.0 {
                    score += boost;
                    reasons.push("Match semantico con el brief".to_string());
                }
            }

            score += score_terms(
                "Genero",
                track.genre.as_deref(),
                &interpreted.genres,
                34.0,
                &mut reasons,
            );
            score += score_terms(
                "Artista",
                track.artist.as_deref(),
                &interpreted.artists,
                26.0,
                &mut reasons,
            );
            score += score_terms(
                "Key",
                track.key.as_deref(),
                &interpreted.keys,
                14.0,
                &mut reasons,
            );
            score += score_bpm(track, interpreted, &mut reasons);

            let matched_terms = prompt_terms
                .iter()
                .filter(|term| normalized_contains_phrase(&normalized_text, term))
                .take(8)
                .cloned()
                .collect::<Vec<_>>();
            if !matched_terms.is_empty() {
                score += (matched_terms.len() as f64 * 2.0).min(14.0);
                reasons.push(format!("Coincide con: {}", matched_terms.join(", ")));
            }

            if let Some(mood) = interpreted.mood.as_deref() {
                let normalized_mood = normalize_for_match(mood);
                if normalized_contains_phrase(&normalized_text, &normalized_mood) {
                    score += 8.0;
                    reasons.push(format!("Mood: {mood}"));
                }
            }
            if let Some(energy) = interpreted.energy.as_deref() {
                if energy == "peak" && track_bpm_value(track).is_some_and(|bpm| bpm >= 124.0) {
                    score += 5.0;
                    reasons.push("Energia alta por BPM".to_string());
                } else if energy == "warmup"
                    && track_bpm_value(track).is_some_and(|bpm| bpm <= 124.0)
                {
                    score += 5.0;
                    reasons.push("Apto para warmup".to_string());
                }
            }

            score += metadata_quality_score(track);
            if !track.source_exists {
                score -= 25.0;
                reasons.push("Archivo no encontrado".to_string());
            }
            if reasons.is_empty() {
                reasons.push("Buen fit general por metadata disponible".to_string());
            }

            Some(PlaylistCopilotCandidate {
                track: track.clone(),
                score: (score * 10.0).round() / 10.0,
                reasons,
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.track
                    .artist
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(
                        &right
                            .track
                            .artist
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase(),
                    )
            })
            .then_with(|| {
                left.track
                    .name
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(
                        &right
                            .track
                            .name
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase(),
                    )
            })
    });

    diversify_copilot_candidates(candidates, target_count)
}

fn score_terms(
    label: &str,
    value: Option<&str>,
    terms: &[String],
    points: f64,
    reasons: &mut Vec<String>,
) -> f64 {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0.0;
    };
    let normalized_value = normalize_for_match(value);
    if terms
        .iter()
        .map(|term| normalize_for_match(term))
        .any(|term| {
            normalized_contains_phrase(&normalized_value, &term)
                || normalized_contains_phrase(&term, &normalized_value)
        })
    {
        reasons.push(format!("{label}: {value}"));
        return points;
    }
    0.0
}

fn score_bpm(
    track: &PlaylistIndexTrack,
    interpreted: &PlaylistCopilotInterpretation,
    reasons: &mut Vec<String>,
) -> f64 {
    let Some(bpm) = track_bpm_value(track) else {
        return 0.0;
    };
    let min = interpreted.bpm_min.unwrap_or(50.0);
    let max = interpreted.bpm_max.unwrap_or(220.0);
    if interpreted.bpm_min.is_none() && interpreted.bpm_max.is_none() {
        return 0.0;
    }
    if bpm >= min && bpm <= max {
        reasons.push(format!("BPM {bpm:.0} dentro del rango"));
        return 24.0;
    }
    let distance = if bpm < min { min - bpm } else { bpm - max };
    if distance <= 5.0 {
        reasons.push(format!("BPM {bpm:.0} cerca del rango"));
        return 8.0;
    }
    -distance.min(18.0) * 0.45
}

fn metadata_quality_score(track: &PlaylistIndexTrack) -> f64 {
    [
        track.name.as_ref(),
        track.artist.as_ref(),
        track.album.as_ref(),
        track.genre.as_ref(),
        track.bpm.as_ref(),
        track.key.as_ref(),
    ]
    .into_iter()
    .filter(|value| value.is_some_and(|value| !value.trim().is_empty()))
    .count() as f64
}

fn diversify_copilot_candidates(
    candidates: Vec<PlaylistCopilotCandidate>,
    target_count: usize,
) -> Vec<PlaylistCopilotCandidate> {
    let artist_soft_limit = if target_count <= 20 { 2 } else { 3 };
    let mut artist_counts = HashMap::<String, usize>::new();
    let mut selected = Vec::new();
    let mut deferred = Vec::new();

    for candidate in candidates {
        let artist = normalize_for_match(candidate.track.artist.as_deref().unwrap_or("unknown"));
        let count = artist_counts.get(&artist).copied().unwrap_or_default();
        if selected.len() < target_count && (artist == "unknown" || count < artist_soft_limit) {
            *artist_counts.entry(artist).or_insert(0) += 1;
            selected.push(candidate);
        } else {
            deferred.push(candidate);
        }
    }

    for candidate in deferred {
        if selected.len() >= target_count {
            break;
        }
        selected.push(candidate);
    }

    selected
}

fn copilot_prompt_terms(prompt: &str) -> Vec<String> {
    let stopwords = [
        "para", "con", "sin", "que", "una", "uno", "los", "las", "the", "and", "for", "from",
        "playlist", "lista", "temas", "tracks", "quiero", "generar", "crear", "algo", "entre",
        "bpm", "del", "por", "como",
    ];
    let normalized = normalize_for_match(prompt);
    let mut seen = BTreeSet::new();
    normalized
        .split_whitespace()
        .filter(|term| term.len() >= 3 && !stopwords.contains(term))
        .filter(|term| seen.insert((*term).to_string()))
        .take(24)
        .map(ToOwned::to_owned)
        .collect()
}

fn playlist_copilot_questions(
    interpreted: &PlaylistCopilotInterpretation,
    candidate_count: usize,
) -> Vec<String> {
    let mut questions = Vec::new();
    if interpreted.genres.is_empty() {
        questions.push("Quieres priorizar algun genero o subgenero?".to_string());
    }
    if interpreted.bpm_min.is_none() && interpreted.bpm_max.is_none() {
        questions.push("Quieres acotar un rango BPM?".to_string());
    }
    if interpreted.keys.is_empty() {
        questions.push("Mantengo compatibilidad armonica por key?".to_string());
    }
    if candidate_count < interpreted.target_count.unwrap_or(30).min(30) {
        questions
            .push("Quieres abrir criterios o incluir tracks con metadata incompleta?".to_string());
    }
    questions.truncate(4);
    questions
}

fn normalize_for_match(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_contains_phrase(haystack: &str, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    if needle.len() <= 2 {
        return haystack.split_whitespace().any(|token| token == needle);
    }
    haystack.contains(needle)
}

fn semantic_search(
    app: &AppHandle,
    conn: &Connection,
    library_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<PlaylistSearchResult>, String> {
    let api_key = settings::load_openai_api_key(app)?.ok_or_else(|| {
        "OpenAI API key no configurada. Guardala en Settings o usa busqueda normal.".to_string()
    })?;
    let query_embedding = request_embeddings(&api_key, &[query.trim().to_string()])?
        .into_iter()
        .next()
        .ok_or_else(|| "OpenAI no retorno embedding para la busqueda.".to_string())?;
    let mut candidates = load_embedded_tracks(conn, library_id)?;
    if candidates.is_empty() {
        return lexical_search(conn, library_id, query, limit);
    }

    for (_, score, embedding) in &mut candidates {
        *score = cosine_similarity(&query_embedding, embedding);
    }

    candidates.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(candidates
        .into_iter()
        .take(limit)
        .map(|(track, score, _)| PlaylistSearchResult {
            score,
            track,
            mode: "semantic".to_string(),
        })
        .collect())
}

fn load_embedded_tracks(
    conn: &Connection,
    library_id: Option<&str>,
) -> Result<Vec<(PlaylistIndexTrack, f64, Vec<f64>)>, String> {
    let library_filter = if library_id.is_some() {
        "AND t.library_id = ?3"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {}, e.embedding_json
         FROM playlist_track_embeddings e
         JOIN playlist_index_tracks t
           ON t.library_id = e.library_id AND t.track_id = e.track_id
         WHERE e.model = ?1 AND e.dimensions = ?2
         {library_filter}",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar busqueda vectorial: {error}"))?;

    let map_row =
        |row: &rusqlite::Row<'_>| -> rusqlite::Result<(PlaylistIndexTrack, f64, Vec<f64>)> {
            let track = row_to_track(row)?;
            let embedding_json: String = row.get(16)?;
            let embedding = serde_json::from_str::<Vec<f64>>(&embedding_json).unwrap_or_default();
            Ok((track, 0.0, embedding))
        };

    let rows = if let Some(library_id) = library_id {
        stmt.query_map(
            params![EMBEDDING_MODEL, EMBEDDING_DIMENSIONS as i64, library_id],
            map_row,
        )
        .map_err(|error| format!("No se pudo ejecutar busqueda vectorial: {error}"))?
    } else {
        stmt.query_map(
            params![EMBEDDING_MODEL, EMBEDDING_DIMENSIONS as i64],
            map_row,
        )
        .map_err(|error| format!("No se pudo ejecutar busqueda vectorial: {error}"))?
    };

    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear embeddings: {error}"))?;
    items.retain(|(_, _, embedding)| !embedding.is_empty());
    Ok(items)
}

fn generate_embeddings_blocking(
    app: AppHandle,
    library_id: String,
    limit: Option<usize>,
    track_ids: Option<Vec<String>>,
) -> Result<PlaylistEmbeddingResult, String> {
    let api_key = settings::load_openai_api_key(&app)?
        .ok_or_else(|| "OpenAI API key no configurada. Guardala en Settings.".to_string())?;
    let conn = open_db(&app)?;
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let max_items = limit.unwrap_or(500).clamp(1, 5000);
    let selected_track_ids = track_ids
        .unwrap_or_default()
        .into_iter()
        .map(|track_id| track_id.trim().to_string())
        .filter(|track_id| !track_id.is_empty())
        .collect::<BTreeSet<_>>();
    let pending = embedding_candidates(&conn, &library_id, max_items, &selected_track_ids)?;
    let total = pending.len();
    if total == 0 {
        emit_progress(
            &app,
            "info",
            "Todos los tracks ya tienen embeddings actualizados.",
            Some(100.0),
            Some(library_id.clone()),
            Some(0),
            Some(0),
        );
        return Ok(PlaylistEmbeddingResult {
            library_id,
            generated_total: 0,
            skipped_total: 0,
            model: EMBEDDING_MODEL.to_string(),
            dimensions: EMBEDDING_DIMENSIONS,
        });
    }

    emit_progress(
        &app,
        "info",
        &format!("Generando embeddings para {total} track(s)."),
        Some(0.0),
        Some(library_id.clone()),
        Some(0),
        Some(total),
    );

    let mut generated_total = 0;
    for chunk in pending.chunks(EMBEDDING_BATCH_SIZE) {
        for candidate in chunk {
            emit_track_embedding_progress(
                &app,
                &library_id,
                &candidate.track_id,
                "embedding",
                &format!("Generando embedding: {}", candidate.track_id),
                generated_total,
                total,
            );
        }

        let inputs = chunk
            .iter()
            .map(|candidate| candidate.search_text.clone())
            .collect::<Vec<_>>();
        let embeddings = request_embeddings(&api_key, &inputs)?;
        let now = timestamp();

        for (candidate, embedding) in chunk.iter().zip(embeddings.into_iter()) {
            conn.execute(
                "INSERT INTO playlist_track_embeddings (
                    library_id, track_id, model, dimensions, text_hash, embedding_json, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(library_id, track_id, model, dimensions) DO UPDATE SET
                    text_hash = excluded.text_hash,
                    embedding_json = excluded.embedding_json,
                    updated_at = excluded.updated_at",
                params![
                    &library_id,
                    &candidate.track_id,
                    EMBEDDING_MODEL,
                    EMBEDDING_DIMENSIONS as i64,
                    &candidate.text_hash,
                    serde_json::to_string(&embedding)
                        .map_err(|error| format!("No se pudo serializar embedding: {error}"))?,
                    &now
                ],
            )
            .map_err(|error| format!("No se pudo guardar embedding: {error}"))?;
            generated_total += 1;
            emit_track_embedding_progress(
                &app,
                &library_id,
                &candidate.track_id,
                "embedded",
                &format!("Embedding listo: {}", candidate.track_id),
                generated_total,
                total,
            );
        }

        emit_progress(
            &app,
            "info",
            &format!("Embeddings {generated_total}/{total}"),
            Some((generated_total as f64 / total as f64) * 100.0),
            Some(library_id.clone()),
            Some(generated_total),
            Some(total),
        );
    }

    Ok(PlaylistEmbeddingResult {
        library_id,
        generated_total,
        skipped_total: 0,
        model: EMBEDDING_MODEL.to_string(),
        dimensions: EMBEDDING_DIMENSIONS,
    })
}

#[derive(Debug)]
struct EmbeddingCandidate {
    track_id: String,
    search_text: String,
    text_hash: String,
}

fn embedding_candidates(
    conn: &Connection,
    library_id: &str,
    limit: usize,
    selected_track_ids: &BTreeSet<String>,
) -> Result<Vec<EmbeddingCandidate>, String> {
    let limit_clause = if selected_track_ids.is_empty() {
        "LIMIT ?4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT t.track_id, t.search_text, e.text_hash
         FROM playlist_index_tracks t
         LEFT JOIN playlist_track_embeddings e ON e.library_id = t.library_id
            AND e.track_id = t.track_id
            AND e.model = ?2
            AND e.dimensions = ?3
         WHERE t.library_id = ?1
         ORDER BY COALESCE(t.artist, ''), COALESCE(t.name, ''), t.track_id
         {limit_clause}"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar candidatos de embeddings: {error}"))?;

    let mut candidates = Vec::new();
    let mut push_row =
        |row: rusqlite::Result<(String, String, String, Option<String>)>| -> Result<(), String> {
            let (track_id, search_text, text_hash, existing_hash) =
                row.map_err(|error| format!("No se pudo mapear candidato: {error}"))?;
            if !selected_track_ids.is_empty() && !selected_track_ids.contains(&track_id) {
                return Ok(());
            }
            if existing_hash.as_deref() == Some(text_hash.as_str()) {
                return Ok(());
            }
            candidates.push(EmbeddingCandidate {
                track_id,
                search_text,
                text_hash,
            });
            Ok(())
        };

    let map_row =
        |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String, String, Option<String>)> {
            let track_id: String = row.get(0)?;
            let search_text: String = row.get(1)?;
            let existing_hash: Option<String> = row.get(2)?;
            let text_hash = stable_hash(&search_text);
            Ok((track_id, search_text, text_hash, existing_hash))
        };

    if selected_track_ids.is_empty() {
        let rows = stmt
            .query_map(
                params![
                    library_id,
                    EMBEDDING_MODEL,
                    EMBEDDING_DIMENSIONS as i64,
                    limit as i64
                ],
                map_row,
            )
            .map_err(|error| format!("No se pudieron leer candidatos de embeddings: {error}"))?;
        for row in rows {
            push_row(row)?;
        }
    } else {
        let rows = stmt
            .query_map(
                params![library_id, EMBEDDING_MODEL, EMBEDDING_DIMENSIONS as i64],
                map_row,
            )
            .map_err(|error| format!("No se pudieron leer candidatos de embeddings: {error}"))?;
        for row in rows {
            push_row(row)?;
        }
    }

    Ok(candidates)
}

fn request_embeddings(api_key: &str, inputs: &[String]) -> Result<Vec<Vec<f64>>, String> {
    let client = reqwest::blocking::Client::new();
    let body = json!({
        "model": EMBEDDING_MODEL,
        "input": inputs,
        "dimensions": EMBEDDING_DIMENSIONS,
        "encoding_format": "float"
    });
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|error| format!("OpenAI embeddings request fallo: {error}"))?
        .error_for_status()
        .map_err(|error| format!("OpenAI embeddings retorno error: {error}"))?
        .json::<EmbeddingResponse>()
        .map_err(|error| format!("OpenAI embeddings retorno JSON invalido: {error}"))?;

    let EmbeddingResponse {
        mut data,
        model,
        usage,
    } = response;
    let _ = (model, usage);
    data.sort_by_key(|item| item.index);
    let embeddings = data
        .into_iter()
        .map(|item| item.embedding)
        .collect::<Vec<_>>();
    if embeddings.len() != inputs.len() {
        return Err(format!(
            "OpenAI retorno {} embeddings para {} input(s).",
            embeddings.len(),
            inputs.len()
        ));
    }

    Ok(embeddings)
}

fn list_drafts(conn: &Connection, library_id: Option<&str>) -> Result<Vec<PlaylistDraft>, String> {
    let library_filter = if library_id.is_some() {
        "WHERE d.library_id = ?1"
    } else {
        ""
    };
    let sql = format!(
        "SELECT d.id, d.library_id, d.name, d.description, COUNT(dt.track_id) AS track_count,
                d.created_at, d.updated_at
         FROM playlist_drafts d
         LEFT JOIN playlist_draft_tracks dt ON dt.draft_id = d.id
         {library_filter}
         GROUP BY d.id
         ORDER BY d.updated_at DESC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar consulta de drafts: {error}"))?;

    if let Some(library_id) = library_id {
        let rows = stmt
            .query_map(params![library_id], row_to_draft)
            .map_err(|error| format!("No se pudieron leer drafts: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear drafts: {error}"))
    } else {
        let rows = stmt
            .query_map([], row_to_draft)
            .map_err(|error| format!("No se pudieron leer drafts: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear drafts: {error}"))
    }
}

fn get_draft(conn: &Connection, draft_id: &str) -> Result<Option<PlaylistDraft>, String> {
    conn.query_row(
        "SELECT d.id, d.library_id, d.name, d.description, COUNT(dt.track_id) AS track_count,
                d.created_at, d.updated_at
         FROM playlist_drafts d
         LEFT JOIN playlist_draft_tracks dt ON dt.draft_id = d.id
         WHERE d.id = ?1
         GROUP BY d.id",
        params![draft_id],
        row_to_draft,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer playlist draft: {error}"))
}

fn row_to_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistDraft> {
    Ok(PlaylistDraft {
        id: row.get(0)?,
        library_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        track_count: i64_to_usize(row.get(4)?),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn draft_tracks(conn: &Connection, draft_id: &str) -> Result<Vec<PlaylistIndexTrack>, String> {
    let draft = get_draft(conn, draft_id)?
        .ok_or_else(|| format!("Playlist draft no encontrada: {draft_id}"))?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {}
             FROM playlist_draft_tracks dt
             JOIN playlist_index_tracks t
               ON t.library_id = ?2 AND t.track_id = dt.track_id
             WHERE dt.draft_id = ?1
             ORDER BY dt.position ASC",
            track_select_clause()
        ))
        .map_err(|error| format!("No se pudo preparar tracks de draft: {error}"))?;
    let rows = stmt
        .query_map(params![draft_id, &draft.library_id], row_to_track)
        .map_err(|error| format!("No se pudieron leer tracks de draft: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear tracks de draft: {error}"))
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistIndexTrack> {
    let attributes = parse_track_attributes_json(row.get::<_, Option<String>>(14)?);

    Ok(PlaylistIndexTrack {
        library_id: row.get(0)?,
        track_id: row.get(1)?,
        name: row.get(2)?,
        artist: row.get(3)?,
        album: row.get(4)?,
        kind: row.get(5)?,
        location: row.get(6)?,
        source_path: row.get(7)?,
        size: option_i64_to_u64(row.get(8)?),
        total_time: option_i64_to_u64(row.get(9)?),
        sample_rate: option_i64_to_u32(row.get(10)?),
        bitrate: option_i64_to_u32(row.get(11)?),
        source_exists: row.get::<_, i64>(12)? == 1,
        search_text: row.get(13)?,
        genre: attribute_value(&attributes, &["Genre"]),
        comments: attribute_value(&attributes, &["Comments", "Comment"]),
        bpm: attribute_value(&attributes, &["AverageBpm", "Bpm", "BPM"]),
        key: attribute_value(&attributes, &["Tonality", "Key"]),
        rating: attribute_value(&attributes, &["Rating"]),
        year: attribute_value(&attributes, &["Year"]),
        label: attribute_value(&attributes, &["Label"]),
        date_added: attribute_value(&attributes, &["DateAdded", "Date"]),
        attributes,
        embedding_ready: row.get::<_, i64>(15)? == 1,
    })
}

fn track_select_clause() -> &'static str {
    "t.library_id, t.track_id, t.name, t.artist, t.album, t.kind, t.location, t.source_path,
     t.size_bytes, t.total_time, t.sample_rate, t.bitrate, t.source_exists, t.search_text,
     t.attributes_json,
     EXISTS(
       SELECT 1 FROM playlist_track_embeddings e
       WHERE e.library_id = t.library_id
         AND e.track_id = t.track_id
         AND e.model = 'text-embedding-3-small'
         AND e.dimensions = 512
     ) AS embedding_ready"
}

fn parse_track_attributes_json(value: Option<String>) -> BTreeMap<String, String> {
    value
        .as_deref()
        .and_then(|json| serde_json::from_str::<BTreeMap<String, String>>(json).ok())
        .unwrap_or_default()
}

fn attribute_value(attributes: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = attributes
            .get(*key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return Some(value.to_string());
        }
    }

    attributes.iter().find_map(|(name, value)| {
        keys.iter()
            .any(|key| name.eq_ignore_ascii_case(key))
            .then(|| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn playlist_paths_by_track(
    playlists: &[aifficator_core::rekordbox::PlaylistSummary],
) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::<String, Vec<String>>::new();
    for playlist in playlists {
        if playlist.node_type.as_deref() != Some("1") {
            continue;
        }
        for track_id in &playlist.track_keys {
            map.entry(track_id.clone())
                .or_default()
                .push(playlist.path.clone());
        }
    }
    map
}

fn track_search_text(
    track: &Track,
    playlist_paths_by_track: &HashMap<String, Vec<String>>,
) -> String {
    let mut parts = BTreeMap::new();
    parts.insert("title", track.name.as_deref().unwrap_or(""));
    parts.insert("artist", track.artist.as_deref().unwrap_or(""));
    parts.insert("album", track.album.as_deref().unwrap_or(""));
    parts.insert("kind", track.kind.as_deref().unwrap_or(""));
    parts.insert("location", track.location.as_deref().unwrap_or(""));

    let playlists = playlist_paths_by_track
        .get(&track.track_id)
        .map(|paths| paths.join(" | "))
        .unwrap_or_default();
    let metadata = track
        .attributes
        .iter()
        .filter_map(|(key, value)| {
            let value = value.trim();
            (!value.is_empty()).then(|| format!("{key}: {value}"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "title: {}\nartist: {}\nalbum: {}\nkind: {}\nplaylists: {}\nlocation: {}\nmetadata:\n{}",
        parts["title"],
        parts["artist"],
        parts["album"],
        parts["kind"],
        playlists,
        parts["location"],
        metadata
    )
}

fn fts_query(query: &str) -> Option<String> {
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.len() >= 2)
        .map(|term| format!("{}*", term.to_ascii_lowercase()))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
    let length = left.len().min(right.len());
    if length == 0 {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for index in 0..length {
        dot += left[index] * right[index];
        left_norm += left[index] * left[index];
        right_norm += right[index] * right[index];
    }

    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn stable_hash(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn emit_progress(
    app: &AppHandle,
    level: &str,
    message: &str,
    progress: Option<f64>,
    library_id: Option<String>,
    processed: Option<usize>,
    total: Option<usize>,
) {
    let _ = app.emit(
        "playlist-index-progress",
        PlaylistIndexProgressEvent {
            event_type: "playlist_index_progress".to_string(),
            level: level.to_string(),
            message: message.to_string(),
            progress,
            library_id,
            playlist_path: None,
            playlist_status: None,
            track_id: None,
            track_status: None,
            processed,
            total,
            timestamp: timestamp(),
        },
    );
}

fn emit_index_work_progress(
    app: &AppHandle,
    library_id: &str,
    message: &str,
    processed: usize,
    total: usize,
) {
    emit_progress(
        app,
        "info",
        message,
        Some(index_work_percent(processed, total)),
        Some(library_id.to_string()),
        Some(processed),
        Some(total),
    );
}

fn emit_playlist_index_progress(
    app: &AppHandle,
    library_id: &str,
    playlist_path: &str,
    playlist_status: &str,
    message: &str,
    processed: usize,
    total: usize,
) {
    let _ = app.emit(
        "playlist-index-progress",
        PlaylistIndexProgressEvent {
            event_type: "playlist_index_progress".to_string(),
            level: "info".to_string(),
            message: message.to_string(),
            progress: Some(index_work_percent(processed, total)),
            library_id: Some(library_id.to_string()),
            playlist_path: Some(playlist_path.to_string()),
            playlist_status: Some(playlist_status.to_string()),
            track_id: None,
            track_status: None,
            processed: Some(processed),
            total: Some(total),
            timestamp: timestamp(),
        },
    );
}

fn emit_track_embedding_progress(
    app: &AppHandle,
    library_id: &str,
    track_id: &str,
    track_status: &str,
    message: &str,
    processed: usize,
    total: usize,
) {
    let progress = if total == 0 {
        100.0
    } else {
        (processed as f64 / total as f64) * 100.0
    };
    let _ = app.emit(
        "playlist-index-progress",
        PlaylistIndexProgressEvent {
            event_type: "playlist_index_progress".to_string(),
            level: "info".to_string(),
            message: message.to_string(),
            progress: Some(progress),
            library_id: Some(library_id.to_string()),
            playlist_path: None,
            playlist_status: None,
            track_id: Some(track_id.to_string()),
            track_status: Some(track_status.to_string()),
            processed: Some(processed),
            total: Some(total),
            timestamp: timestamp(),
        },
    );
}

fn index_work_percent(processed: usize, total: usize) -> f64 {
    if total == 0 {
        return 100.0;
    }

    ((processed as f64 / total as f64) * 96.0).clamp(2.0, 98.0)
}

fn should_emit_index_progress(done: usize, total: usize) -> bool {
    done == total || total <= 25 || done % 100 == 0
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

fn i64_to_usize(value: i64) -> usize {
    usize::try_from(value.max(0)).unwrap_or_default()
}

fn option_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn option_i64_to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}
