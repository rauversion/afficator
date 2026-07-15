use crate::{
    enrichment,
    playlist_copilot::{
        apply_guided_answer, rank_and_sequence_with_seed, DiscoveryMode, EnergyCurve, GuidedAnswer,
        HarmonicPolicy, PlaylistIntent as PlaylistCopilotInterpretation, SourcePolicy, TempoPolicy,
        TrackFeatures,
    },
    settings, system,
};
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
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const DB_FILE: &str = "aifficator.sqlite3";
const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const EMBEDDING_DIMENSIONS: usize = 512;
const EMBEDDING_BATCH_SIZE: usize = 32;
const COPILOT_RANKER_VERSION: &str = "multi-probe-sequence-v2";
const COPILOT_PROBE_RESULT_LIMIT: usize = 160;
const TRACK_COVER_CACHE_VERSION: &str = "v2";
const TRACK_COVER_FILTER: &str =
    "scale=256:256:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=fast_bilinear";

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
    user_rating: Option<u8>,
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

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaylistCatalogFilters {
    genres: Vec<String>,
    artists: Vec<String>,
    albums: Vec<String>,
    keys: Vec<String>,
    years: Vec<String>,
    formats: Vec<String>,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
    rating_min: Option<u8>,
    metadata_gaps: Vec<String>,
    availability: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistCatalogRequest {
    library_id: String,
    query: Option<String>,
    filters: Option<PlaylistCatalogFilters>,
    sort: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCatalogFacetValue {
    value: String,
    name: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCatalogFacets {
    genres: Vec<PlaylistCatalogFacetValue>,
    artists: Vec<PlaylistCatalogFacetValue>,
    albums: Vec<PlaylistCatalogFacetValue>,
    keys: Vec<PlaylistCatalogFacetValue>,
    years: Vec<PlaylistCatalogFacetValue>,
    formats: Vec<PlaylistCatalogFacetValue>,
    ratings: Vec<PlaylistCatalogFacetValue>,
    metadata_gaps: Vec<PlaylistCatalogFacetValue>,
    availability: Vec<PlaylistCatalogFacetValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCatalogResponse {
    items: Vec<PlaylistIndexTrack>,
    total: usize,
    page: usize,
    page_size: usize,
    total_pages: usize,
    facets: PlaylistCatalogFacets,
    query_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCatalogSavedSearch {
    id: String,
    library_id: String,
    name: String,
    description: Option<String>,
    query: String,
    filters: PlaylistCatalogFilters,
    sort: String,
    result_count: usize,
    last_evaluated_at: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistCatalogSaveRequest {
    id: Option<String>,
    library_id: String,
    name: String,
    description: Option<String>,
    query: Option<String>,
    filters: Option<PlaylistCatalogFilters>,
    sort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCatalogSelectionResponse {
    items: Vec<PlaylistIndexTrack>,
    total: usize,
    truncated: bool,
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
    message: String,
    request_id: Option<String>,
    target_count: Option<usize>,
    session_id: Option<String>,
    mode: Option<String>,
    language: Option<String>,
    answered_question_ids: Option<Vec<String>>,
    guided_answer: Option<GuidedAnswer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotResponse {
    session_id: String,
    candidate_set_id: String,
    message: String,
    interpreted: PlaylistCopilotInterpretation,
    questions: Vec<String>,
    guided_questions: Vec<PlaylistCopilotQuestion>,
    steps: Vec<PlaylistCopilotStep>,
    brief_changes: Vec<String>,
    search_trace: Vec<PlaylistCopilotSearchTrace>,
    reasoning_summary: Vec<String>,
    title_suggestions: Vec<PlaylistCopilotTitleSuggestion>,
    coverage: PlaylistCopilotCoverage,
    candidates: Vec<PlaylistCopilotCandidate>,
    used_openai: bool,
    ranker_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotSearchTrace {
    id: String,
    label: String,
    candidate_count: usize,
    top_similarity: Option<f64>,
    detail: String,
}

#[derive(Debug, Clone)]
struct PlaylistCopilotSearchProbe {
    id: String,
    label: String,
    query: String,
    weight: f64,
}

#[derive(Debug, Clone, Default)]
struct CopilotSemanticEvidence {
    score: f64,
    probes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PlaylistCopilotProgressEvent {
    request_id: String,
    stage: String,
    status: String,
    message: String,
    detail: Option<String>,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotCandidate {
    track: PlaylistIndexTrack,
    score: f64,
    reasons: Vec<String>,
    score_components: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotStep {
    label: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotQuestion {
    id: String,
    question: String,
    options: Vec<PlaylistCopilotQuestionOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotQuestionOption {
    label: String,
    value: String,
    description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotTitleSuggestion {
    title: String,
    rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistCopilotCoverage {
    track_count: usize,
    source_missing_count: usize,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
    bpm_average: Option<f64>,
    genres: Vec<TaxonomyCount>,
    keys: Vec<TaxonomyCount>,
    formats: Vec<TaxonomyCount>,
    top_artists: Vec<TaxonomyCount>,
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

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistEnrichmentOverview {
    library: PlaylistIndexLibrary,
    track_count: usize,
    missing_genre_count: usize,
    missing_year_count: usize,
    missing_label_count: usize,
    missing_comments_count: usize,
    missing_key_count: usize,
    missing_bpm_count: usize,
    enriched_track_count: usize,
    matched_result_count: usize,
    failed_result_count: usize,
    last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistEnrichmentItem {
    id: String,
    library_id: String,
    track_id: String,
    provider: String,
    provider_key: Option<String>,
    status: String,
    confidence: f64,
    fields: BTreeMap<String, String>,
    message: Option<String>,
    source_url: Option<String>,
    updated_at: String,
    applied_at: Option<String>,
    track: PlaylistIndexTrack,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistEnrichmentRunResult {
    run_id: String,
    library_id: String,
    processed_total: usize,
    matched_total: usize,
    no_match_total: usize,
    failed_total: usize,
    providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistEnrichmentApplyResult {
    library_id: String,
    applied_total: usize,
    skipped_total: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TrackEnrichmentProgressEvent {
    #[serde(rename = "type")]
    event_type: String,
    level: String,
    message: String,
    progress: Option<f64>,
    library_id: String,
    track_id: Option<String>,
    provider: Option<String>,
    status: Option<String>,
    processed: usize,
    total: usize,
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
pub fn playlist_index_set_track_rating(
    app: AppHandle,
    library_id: String,
    track_id: String,
    rating: u8,
) -> Result<PlaylistIndexTrack, String> {
    let conn = open_db(&app)?;
    set_track_rating(&conn, &library_id, &track_id, rating)
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
pub fn playlist_catalog_search(
    app: AppHandle,
    request: PlaylistCatalogRequest,
) -> Result<PlaylistCatalogResponse, String> {
    let conn = open_db(&app)?;
    catalog_search(&conn, request)
}

#[tauri::command]
pub fn playlist_catalog_select_all(
    app: AppHandle,
    request: PlaylistCatalogRequest,
    limit: Option<usize>,
) -> Result<PlaylistCatalogSelectionResponse, String> {
    let conn = open_db(&app)?;
    catalog_select_all(&conn, request, limit.unwrap_or(5_000).clamp(1, 5_000))
}

#[tauri::command]
pub fn playlist_catalog_saved_searches(
    app: AppHandle,
    library_id: String,
) -> Result<Vec<PlaylistCatalogSavedSearch>, String> {
    let conn = open_db(&app)?;
    list_catalog_saved_searches(&conn, &library_id)
}

#[tauri::command]
pub fn playlist_catalog_save_search(
    app: AppHandle,
    request: PlaylistCatalogSaveRequest,
) -> Result<PlaylistCatalogSavedSearch, String> {
    let conn = open_db(&app)?;
    save_catalog_search(&conn, request)
}

#[tauri::command]
pub fn playlist_catalog_delete_saved_search(
    app: AppHandle,
    library_id: String,
    saved_search_id: String,
) -> Result<String, String> {
    let conn = open_db(&app)?;
    delete_catalog_saved_search(&conn, &library_id, &saved_search_id)
}

#[tauri::command]
pub fn playlist_catalog_set_rating(
    app: AppHandle,
    library_id: String,
    track_ids: Vec<String>,
    rating: u8,
) -> Result<usize, String> {
    let mut conn = open_db(&app)?;
    set_catalog_tracks_rating(&mut conn, &library_id, track_ids, rating)
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
pub async fn playlist_index_track_cover(
    app: AppHandle,
    source_path: String,
) -> Result<Option<String>, String> {
    let app_for_error = app.clone();
    tauri::async_runtime::spawn_blocking(move || extract_track_cover(&app, &source_path))
        .await
        .map_err(|error| {
            settings::localized(
                &app_for_error,
                &format!("La portada fallo inesperadamente: {error}"),
                &format!("The cover extraction failed unexpectedly: {error}"),
            )
        })?
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
pub fn playlist_enrichment_overview(
    app: AppHandle,
    library_id: String,
) -> Result<PlaylistEnrichmentOverview, String> {
    let conn = open_db(&app)?;
    enrichment_overview(&conn, &library_id)
}

#[tauri::command]
pub fn playlist_enrichment_candidates(
    app: AppHandle,
    library_id: String,
    gap: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let conn = open_db(&app)?;
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let gap = gap.unwrap_or_else(|| "missing_metadata".to_string());
    let max_items = limit.unwrap_or(150).clamp(1, 1000);
    let tracks = load_enrichment_tracks(&conn, &library_id, query.as_deref(), max_items * 5)?;
    Ok(tracks
        .into_iter()
        .filter(|track| enrichment_gap_matches(track, &gap))
        .take(max_items)
        .collect())
}

#[tauri::command]
pub fn playlist_enrichment_results(
    app: AppHandle,
    library_id: String,
    provider: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<PlaylistEnrichmentItem>, String> {
    let conn = open_db(&app)?;
    list_enrichment_results(
        &conn,
        &library_id,
        provider.as_deref(),
        status.as_deref(),
        limit.unwrap_or(200).clamp(1, 1000),
    )
}

#[tauri::command]
pub async fn playlist_enrichment_run(
    app: AppHandle,
    library_id: String,
    providers: Vec<String>,
    limit: Option<usize>,
    track_ids: Option<Vec<String>>,
) -> Result<PlaylistEnrichmentRunResult, String> {
    let app_for_error = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_enrichment_blocking(app, library_id, providers, limit, track_ids)
    })
    .await
    .map_err(|error| {
        settings::localized(
            &app_for_error,
            &format!("Enrichment fallo inesperadamente: {error}"),
            &format!("Enrichment failed unexpectedly: {error}"),
        )
    })?
}

#[tauri::command]
pub fn playlist_enrichment_apply(
    app: AppHandle,
    library_id: String,
    result_ids: Vec<String>,
) -> Result<PlaylistEnrichmentApplyResult, String> {
    let mut conn = open_db(&app)?;
    apply_enrichment_results(&mut conn, &library_id, result_ids)
}

#[tauri::command]
pub fn playlist_enrichment_clear(
    app: AppHandle,
    library_id: String,
    track_ids: Option<Vec<String>>,
) -> Result<usize, String> {
    let conn = open_db(&app)?;
    clear_enrichment_results(&conn, &library_id, track_ids)
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
          user_rating INTEGER CHECK(user_rating BETWEEN 0 AND 5),
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

        CREATE TABLE IF NOT EXISTS playlist_track_enrichments (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          provider TEXT NOT NULL,
          provider_key TEXT,
          status TEXT NOT NULL,
          confidence REAL NOT NULL DEFAULT 0,
          fields_json TEXT NOT NULL DEFAULT '{}',
          payload_json TEXT NOT NULL DEFAULT '{}',
          message TEXT,
          source_url TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          applied_at TEXT,
          UNIQUE(library_id, track_id, provider),
          FOREIGN KEY(library_id, track_id)
            REFERENCES playlist_index_tracks(library_id, track_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_track_enrichments_library
          ON playlist_track_enrichments(library_id, updated_at);
        CREATE INDEX IF NOT EXISTS idx_playlist_track_enrichments_status
          ON playlist_track_enrichments(library_id, status, provider);

        CREATE TABLE IF NOT EXISTS playlist_enrichment_runs (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          status TEXT NOT NULL,
          providers_json TEXT NOT NULL DEFAULT '[]',
          requested_fields_json TEXT NOT NULL DEFAULT '[]',
          total_work INTEGER NOT NULL DEFAULT 0,
          processed_total INTEGER NOT NULL DEFAULT 0,
          matched_total INTEGER NOT NULL DEFAULT 0,
          no_match_total INTEGER NOT NULL DEFAULT 0,
          failed_total INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          started_at TEXT NOT NULL,
          completed_at TEXT,
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_enrichment_runs_library
          ON playlist_enrichment_runs(library_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS playlist_enrichment_tasks (
          id TEXT PRIMARY KEY,
          run_id TEXT NOT NULL,
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          provider TEXT NOT NULL,
          status TEXT NOT NULL,
          attempts INTEGER NOT NULL DEFAULT 1,
          error_kind TEXT,
          message TEXT,
          started_at TEXT NOT NULL,
          finished_at TEXT,
          UNIQUE(run_id, track_id, provider),
          FOREIGN KEY(run_id) REFERENCES playlist_enrichment_runs(id) ON DELETE CASCADE,
          FOREIGN KEY(library_id, track_id)
            REFERENCES playlist_index_tracks(library_id, track_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_enrichment_tasks_run
          ON playlist_enrichment_tasks(run_id, status, provider);

        CREATE TABLE IF NOT EXISTS playlist_enrichment_observations (
          id TEXT PRIMARY KEY,
          task_id TEXT NOT NULL,
          run_id TEXT NOT NULL,
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          provider TEXT NOT NULL,
          field TEXT NOT NULL,
          value TEXT NOT NULL,
          confidence REAL NOT NULL DEFAULT 0,
          provider_key TEXT,
          source_url TEXT,
          observed_at TEXT NOT NULL,
          FOREIGN KEY(task_id) REFERENCES playlist_enrichment_tasks(id) ON DELETE CASCADE,
          FOREIGN KEY(run_id) REFERENCES playlist_enrichment_runs(id) ON DELETE CASCADE,
          FOREIGN KEY(library_id, track_id)
            REFERENCES playlist_index_tracks(library_id, track_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_enrichment_observations_track
          ON playlist_enrichment_observations(library_id, track_id, field, observed_at DESC);

        CREATE TABLE IF NOT EXISTS playlist_catalog_saved_searches (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          name TEXT NOT NULL,
          description TEXT,
          query TEXT NOT NULL DEFAULT '',
          filters_json TEXT NOT NULL DEFAULT '{}',
          sort TEXT NOT NULL DEFAULT 'relevance',
          result_count INTEGER NOT NULL DEFAULT 0,
          last_evaluated_at TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(library_id, name),
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_catalog_saved_searches_library
          ON playlist_catalog_saved_searches(library_id, updated_at DESC);

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

        CREATE TABLE IF NOT EXISTS playlist_copilot_sessions (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          title TEXT NOT NULL,
          intent_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          FOREIGN KEY(library_id) REFERENCES playlist_index_libraries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_copilot_sessions_library
          ON playlist_copilot_sessions(library_id, updated_at);

        CREATE TABLE IF NOT EXISTS playlist_copilot_messages (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL,
          role TEXT NOT NULL,
          content TEXT NOT NULL,
          created_at TEXT NOT NULL,
          FOREIGN KEY(session_id) REFERENCES playlist_copilot_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_copilot_messages_session
          ON playlist_copilot_messages(session_id, created_at);

        CREATE TABLE IF NOT EXISTS playlist_copilot_candidate_sets (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL,
          prompt TEXT NOT NULL,
          interpretation_json TEXT NOT NULL,
          reasoning_json TEXT NOT NULL,
          coverage_json TEXT NOT NULL,
          ranker_version TEXT NOT NULL DEFAULT 'legacy',
          created_at TEXT NOT NULL,
          FOREIGN KEY(session_id) REFERENCES playlist_copilot_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_copilot_candidate_sets_session
          ON playlist_copilot_candidate_sets(session_id, created_at);

        CREATE TABLE IF NOT EXISTS playlist_copilot_candidate_tracks (
          candidate_set_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          position INTEGER NOT NULL,
          score REAL NOT NULL,
          reasons_json TEXT NOT NULL,
          score_components_json TEXT NOT NULL DEFAULT '{}',
          PRIMARY KEY(candidate_set_id, track_id),
          FOREIGN KEY(candidate_set_id) REFERENCES playlist_copilot_candidate_sets(id) ON DELETE CASCADE
        );

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

    ensure_table_column(
        conn,
        "playlist_index_tracks",
        "attributes_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_table_column(
        conn,
        "playlist_index_tracks",
        "user_rating",
        "INTEGER CHECK(user_rating BETWEEN 0 AND 5)",
    )?;
    ensure_table_column(
        conn,
        "playlist_copilot_sessions",
        "intent_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_table_column(
        conn,
        "playlist_copilot_candidate_tracks",
        "score_components_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_table_column(
        conn,
        "playlist_copilot_candidate_sets",
        "ranker_version",
        "TEXT NOT NULL DEFAULT 'legacy'",
    )?;

    Ok(())
}

fn ensure_table_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    if !table
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !column
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("Nombre de tabla o columna invalido.".to_string());
    }
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("No se pudo inspeccionar {table}: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("No se pudieron leer columnas de {table}: {error}"))?;
    let columns = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear columnas de {table}: {error}"))?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
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

fn set_track_rating(
    conn: &Connection,
    library_id: &str,
    track_id: &str,
    rating: u8,
) -> Result<PlaylistIndexTrack, String> {
    if rating > 5 {
        return Err("El rating debe estar entre 0 y 5 estrellas.".to_string());
    }

    let updated = conn
        .execute(
            "UPDATE playlist_index_tracks
             SET user_rating = ?3, updated_at = ?4
             WHERE library_id = ?1 AND track_id = ?2",
            params![library_id, track_id, i64::from(rating), timestamp()],
        )
        .map_err(|error| format!("No se pudo guardar el rating del track: {error}"))?;
    if updated == 0 {
        return Err(format!("Track indexado no encontrado: {track_id}"));
    }

    get_index_track(conn, library_id, track_id)?
        .ok_or_else(|| format!("No se pudo recargar el track indexado: {track_id}"))
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
            score: row.get::<_, f64>(17)?,
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

#[derive(Debug, Clone)]
struct CatalogCriteria {
    filters: PlaylistCatalogFilters,
    query_terms: Vec<String>,
}

fn catalog_search(
    conn: &Connection,
    request: PlaylistCatalogRequest,
) -> Result<PlaylistCatalogResponse, String> {
    if request.library_id.trim().is_empty() {
        return Err("Selecciona una libreria para buscar tracks.".to_string());
    }

    let tracks = list_taxonomy_tracks(conn, request.library_id.trim())?;
    let mut filters = request.filters.unwrap_or_default();
    let query_terms =
        parse_catalog_query(request.query.as_deref().unwrap_or_default(), &mut filters);
    let criteria = CatalogCriteria {
        filters,
        query_terms,
    };

    let facets = catalog_facets(&tracks, &criteria);
    let mut matches = tracks
        .iter()
        .filter(|track| catalog_track_matches(track, &criteria, None))
        .cloned()
        .collect::<Vec<_>>();
    sort_catalog_tracks(
        &mut matches,
        request.sort.as_deref().unwrap_or("relevance"),
        &criteria.query_terms,
    );

    let total = matches.len();
    let page_size = request.page_size.unwrap_or(50).clamp(10, 100);
    let total_pages = total.div_ceil(page_size).max(1);
    let page = request.page.unwrap_or(1).max(1).min(total_pages);
    let offset = (page - 1) * page_size;
    let items = matches.into_iter().skip(offset).take(page_size).collect();

    Ok(PlaylistCatalogResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
        facets,
        query_terms: criteria.query_terms,
    })
}

fn catalog_select_all(
    conn: &Connection,
    request: PlaylistCatalogRequest,
    limit: usize,
) -> Result<PlaylistCatalogSelectionResponse, String> {
    if request.library_id.trim().is_empty() {
        return Err("Selecciona una libreria para buscar tracks.".to_string());
    }

    let tracks = list_taxonomy_tracks(conn, request.library_id.trim())?;
    let mut filters = request.filters.unwrap_or_default();
    let query_terms =
        parse_catalog_query(request.query.as_deref().unwrap_or_default(), &mut filters);
    let criteria = CatalogCriteria {
        filters,
        query_terms,
    };
    let mut matches = tracks
        .into_iter()
        .filter(|track| catalog_track_matches(track, &criteria, None))
        .collect::<Vec<_>>();
    sort_catalog_tracks(
        &mut matches,
        request.sort.as_deref().unwrap_or("relevance"),
        &criteria.query_terms,
    );
    let total = matches.len();
    matches.truncate(limit);

    Ok(PlaylistCatalogSelectionResponse {
        truncated: matches.len() < total,
        items: matches,
        total,
    })
}

fn list_catalog_saved_searches(
    conn: &Connection,
    library_id: &str,
) -> Result<Vec<PlaylistCatalogSavedSearch>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, library_id, name, description, query, filters_json, sort,
                    result_count, last_evaluated_at, created_at, updated_at
             FROM playlist_catalog_saved_searches
             WHERE library_id = ?1
             ORDER BY updated_at DESC, name COLLATE NOCASE",
        )
        .map_err(|error| format!("No se pudieron preparar las busquedas guardadas: {error}"))?;
    let rows = stmt
        .query_map(params![library_id], row_to_catalog_saved_search)
        .map_err(|error| format!("No se pudieron leer las busquedas guardadas: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear las busquedas guardadas: {error}"))
}

fn save_catalog_search(
    conn: &Connection,
    request: PlaylistCatalogSaveRequest,
) -> Result<PlaylistCatalogSavedSearch, String> {
    let library_id = request.library_id.trim();
    if library_id.is_empty() || get_library(conn, library_id)?.is_none() {
        return Err("La libreria de la busqueda guardada no existe.".to_string());
    }
    let name = request.name.trim();
    if name.is_empty() {
        return Err("Escribe un nombre para la busqueda guardada.".to_string());
    }
    if name.chars().count() > 100 {
        return Err("El nombre de la busqueda no puede superar 100 caracteres.".to_string());
    }

    let query = request.query.unwrap_or_default().trim().to_string();
    let filters = request.filters.unwrap_or_default();
    let sort = normalize_catalog_sort(request.sort.as_deref());
    let tracks = list_taxonomy_tracks(conn, library_id)?;
    let mut evaluated_filters = filters.clone();
    let query_terms = parse_catalog_query(&query, &mut evaluated_filters);
    let criteria = CatalogCriteria {
        filters: evaluated_filters,
        query_terms,
    };
    let result_count = tracks
        .iter()
        .filter(|track| catalog_track_matches(track, &criteria, None))
        .count();
    let filters_json = serde_json::to_string(&filters)
        .map_err(|error| format!("No se pudieron serializar los filtros: {error}"))?;
    let description = request
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = timestamp();
    let updating = request.id.is_some();
    let id = request.id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let existing = conn
        .query_row(
            "SELECT 1 FROM playlist_catalog_saved_searches WHERE id = ?1 AND library_id = ?2",
            params![&id, library_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| format!("No se pudo validar la busqueda guardada: {error}"))?
        .is_some();
    if updating && !existing {
        return Err("Busqueda guardada no encontrada.".to_string());
    }

    let result = if existing {
        conn.execute(
            "UPDATE playlist_catalog_saved_searches
             SET name = ?3, description = ?4, query = ?5, filters_json = ?6,
                 sort = ?7, result_count = ?8, last_evaluated_at = ?9, updated_at = ?9
             WHERE id = ?1 AND library_id = ?2",
            params![
                &id,
                library_id,
                name,
                description,
                query,
                filters_json,
                sort,
                result_count as i64,
                now
            ],
        )
    } else {
        conn.execute(
            "INSERT INTO playlist_catalog_saved_searches (
               id, library_id, name, description, query, filters_json, sort,
               result_count, last_evaluated_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9)",
            params![
                &id,
                library_id,
                name,
                description,
                query,
                filters_json,
                sort,
                result_count as i64,
                now
            ],
        )
    };
    result.map_err(|error| {
        if error.to_string().contains("UNIQUE constraint failed") {
            format!("Ya existe una busqueda guardada llamada '{name}'.")
        } else {
            format!("No se pudo guardar la busqueda: {error}")
        }
    })?;

    get_catalog_saved_search(conn, library_id, &id)?
        .ok_or_else(|| "No se pudo recargar la busqueda guardada.".to_string())
}

fn delete_catalog_saved_search(
    conn: &Connection,
    library_id: &str,
    saved_search_id: &str,
) -> Result<String, String> {
    let deleted = conn
        .execute(
            "DELETE FROM playlist_catalog_saved_searches WHERE id = ?1 AND library_id = ?2",
            params![saved_search_id, library_id],
        )
        .map_err(|error| format!("No se pudo borrar la busqueda guardada: {error}"))?;
    if deleted == 0 {
        return Err("Busqueda guardada no encontrada.".to_string());
    }
    Ok(saved_search_id.to_string())
}

fn get_catalog_saved_search(
    conn: &Connection,
    library_id: &str,
    saved_search_id: &str,
) -> Result<Option<PlaylistCatalogSavedSearch>, String> {
    conn.query_row(
        "SELECT id, library_id, name, description, query, filters_json, sort,
                result_count, last_evaluated_at, created_at, updated_at
         FROM playlist_catalog_saved_searches
         WHERE id = ?1 AND library_id = ?2",
        params![saved_search_id, library_id],
        row_to_catalog_saved_search,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer la busqueda guardada: {error}"))
}

fn row_to_catalog_saved_search(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PlaylistCatalogSavedSearch> {
    let filters_json = row.get::<_, String>(5)?;
    Ok(PlaylistCatalogSavedSearch {
        id: row.get(0)?,
        library_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        query: row.get(4)?,
        filters: serde_json::from_str(&filters_json).unwrap_or_default(),
        sort: row.get(6)?,
        result_count: i64_to_usize(row.get(7)?),
        last_evaluated_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn normalize_catalog_sort(sort: Option<&str>) -> String {
    match sort.unwrap_or("relevance") {
        "recent" | "rating" | "bpm" | "title" => sort.unwrap_or_default().to_string(),
        _ => "relevance".to_string(),
    }
}

fn set_catalog_tracks_rating(
    conn: &mut Connection,
    library_id: &str,
    track_ids: Vec<String>,
    rating: u8,
) -> Result<usize, String> {
    if rating > 5 {
        return Err("El rating debe estar entre 0 y 5 estrellas.".to_string());
    }
    let track_ids = track_ids
        .into_iter()
        .filter(|track_id| !track_id.trim().is_empty())
        .collect::<BTreeSet<_>>();
    if track_ids.len() > 5_000 {
        return Err("Puedes actualizar hasta 5000 tracks por operacion.".to_string());
    }
    let now = timestamp();
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar el rating masivo: {error}"))?;
    let mut updated = 0;
    for track_id in track_ids {
        updated += tx
            .execute(
                "UPDATE playlist_index_tracks
                 SET user_rating = ?3, updated_at = ?4
                 WHERE library_id = ?1 AND track_id = ?2",
                params![library_id, track_id, i64::from(rating), &now],
            )
            .map_err(|error| format!("No se pudo actualizar el rating masivo: {error}"))?;
    }
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar el rating masivo: {error}"))?;
    Ok(updated)
}

fn parse_catalog_query(query: &str, filters: &mut PlaylistCatalogFilters) -> Vec<String> {
    let mut query_terms = Vec::new();

    for token in catalog_query_tokens(query) {
        let Some((field, raw_value)) = token.split_once(':') else {
            push_catalog_filter_value(&mut query_terms, &token);
            continue;
        };
        let value = raw_value.trim();
        if value.is_empty() {
            push_catalog_filter_value(&mut query_terms, &token);
            continue;
        }

        match field.trim().to_ascii_lowercase().as_str() {
            "genre" | "genero" | "género" => {
                push_catalog_filter_values(&mut filters.genres, value)
            }
            "artist" | "artista" => push_catalog_filter_values(&mut filters.artists, value),
            "album" | "álbum" => push_catalog_filter_values(&mut filters.albums, value),
            "key" | "tonality" | "tonalidad" => {
                push_catalog_filter_values(&mut filters.keys, value)
            }
            "year" | "ano" | "año" => push_catalog_filter_values(&mut filters.years, value),
            "format" | "formato" | "kind" => {
                push_catalog_filter_values(&mut filters.formats, value)
            }
            "bpm" => apply_catalog_number_range(value, &mut filters.bpm_min, &mut filters.bpm_max),
            "rating" | "stars" | "estrellas" => {
                if let Some(rating) = parse_catalog_minimum(value) {
                    filters.rating_min = Some(
                        filters
                            .rating_min
                            .unwrap_or_default()
                            .max(rating.round() as u8),
                    );
                }
            }
            "missing" | "falta" => {
                for missing in value.split([',', '|']) {
                    let normalized = match missing.trim().to_ascii_lowercase().as_str() {
                        "genre" | "genero" | "género" => "missing_genre",
                        "bpm" => "missing_bpm",
                        "key" | "tonality" | "tonalidad" => "missing_key",
                        "label" | "sello" => "missing_label",
                        "year" | "ano" | "año" => "missing_year",
                        "artist" | "artista" => "missing_artist",
                        "album" | "álbum" => "missing_album",
                        _ => missing.trim(),
                    };
                    push_catalog_filter_value(&mut filters.metadata_gaps, normalized);
                }
            }
            "source" | "archivo" => {
                for source in value.split([',', '|']) {
                    let normalized = match source.trim().to_ascii_lowercase().as_str() {
                        "available" | "disponible" | "ok" => "available",
                        "missing" | "faltante" | "no" => "missing",
                        _ => source.trim(),
                    };
                    push_catalog_filter_value(&mut filters.availability, normalized);
                }
            }
            _ => push_catalog_filter_value(&mut query_terms, &token),
        }
    }

    query_terms
}

fn catalog_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for character in query.chars() {
        match character {
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            value => current.push(value),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn push_catalog_filter_values(target: &mut Vec<String>, raw_values: &str) {
    for value in raw_values.split([',', '|']) {
        push_catalog_filter_value(target, value);
    }
}

fn push_catalog_filter_value(target: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || target.iter().any(|item| item.eq_ignore_ascii_case(value)) {
        return;
    }
    target.push(value.to_string());
}

fn apply_catalog_number_range(value: &str, min: &mut Option<f64>, max: &mut Option<f64>) {
    let value = value.trim().replace(',', ".");
    if let Some((start, end)) = value.split_once("..") {
        if let Ok(parsed) = start.trim().parse::<f64>() {
            *min = Some(min.unwrap_or(parsed).max(parsed));
        }
        if let Ok(parsed) = end.trim().parse::<f64>() {
            *max = Some(max.unwrap_or(parsed).min(parsed));
        }
        return;
    }

    if let Some(parsed) = parse_catalog_minimum(&value) {
        if value.starts_with('>') {
            *min = Some(min.unwrap_or(parsed).max(parsed));
        } else if value.starts_with('<') {
            *max = Some(max.unwrap_or(parsed).min(parsed));
        } else {
            *min = Some(min.unwrap_or(parsed).max(parsed));
            *max = Some(max.unwrap_or(parsed).min(parsed));
        }
    }
}

fn parse_catalog_minimum(value: &str) -> Option<f64> {
    value
        .trim()
        .trim_start_matches(['>', '<', '='])
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn catalog_track_matches(
    track: &PlaylistIndexTrack,
    criteria: &CatalogCriteria,
    skip_facet: Option<&str>,
) -> bool {
    let filters = &criteria.filters;
    if !criteria
        .query_terms
        .iter()
        .all(|term| catalog_track_contains(track, term))
    {
        return false;
    }
    if skip_facet != Some("genres")
        && !catalog_value_matches(track.genre.as_deref(), &filters.genres)
    {
        return false;
    }
    if skip_facet != Some("artists")
        && !catalog_value_matches(track.artist.as_deref(), &filters.artists)
    {
        return false;
    }
    if skip_facet != Some("albums")
        && !catalog_value_matches(track.album.as_deref(), &filters.albums)
    {
        return false;
    }
    if skip_facet != Some("keys") && !catalog_value_matches(track.key.as_deref(), &filters.keys) {
        return false;
    }
    if skip_facet != Some("years") && !catalog_value_matches(track.year.as_deref(), &filters.years)
    {
        return false;
    }
    if skip_facet != Some("formats")
        && !catalog_value_matches(track.kind.as_deref(), &filters.formats)
    {
        return false;
    }

    if skip_facet != Some("bpm") && (filters.bpm_min.is_some() || filters.bpm_max.is_some()) {
        let Some(bpm) = track_bpm_value(track) else {
            return false;
        };
        if filters.bpm_min.is_some_and(|min| bpm < min)
            || filters.bpm_max.is_some_and(|max| bpm > max)
        {
            return false;
        }
    }
    if skip_facet != Some("ratings")
        && filters
            .rating_min
            .is_some_and(|minimum| catalog_track_rating(track) < minimum)
    {
        return false;
    }
    if skip_facet != Some("metadata_gaps")
        && !filters.metadata_gaps.is_empty()
        && !filters
            .metadata_gaps
            .iter()
            .any(|gap| catalog_track_has_gap(track, gap))
    {
        return false;
    }
    if skip_facet != Some("availability")
        && !filters.availability.is_empty()
        && !filters
            .availability
            .iter()
            .any(|availability| match availability.as_str() {
                "available" => track.source_exists,
                "missing" => !track.source_exists,
                _ => false,
            })
    {
        return false;
    }

    true
}

fn catalog_track_contains(track: &PlaylistIndexTrack, term: &str) -> bool {
    let needle = term.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let values = [
        Some(track.track_id.as_str()),
        track.name.as_deref(),
        track.artist.as_deref(),
        track.album.as_deref(),
        track.genre.as_deref(),
        track.comments.as_deref(),
        track.bpm.as_deref(),
        track.key.as_deref(),
        track.year.as_deref(),
        track.label.as_deref(),
        track.kind.as_deref(),
        Some(track.search_text.as_str()),
    ];
    values
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(&needle))
        || track.attributes.iter().any(|(name, value)| {
            name.to_lowercase().contains(&needle) || value.to_lowercase().contains(&needle)
        })
}

fn catalog_value_matches(value: Option<&str>, selected: &[String]) -> bool {
    selected.is_empty()
        || value.is_some_and(|value| {
            selected
                .iter()
                .any(|selected| selected.trim().eq_ignore_ascii_case(value.trim()))
        })
}

fn catalog_track_rating(track: &PlaylistIndexTrack) -> u8 {
    if let Some(rating) = track.user_rating {
        return rating.min(5);
    }
    track
        .rating
        .as_deref()
        .and_then(|rating| rating.trim().parse::<f64>().ok())
        .map(|rating| {
            if rating <= 5.0 {
                rating.round() as u8
            } else {
                (rating / 51.0).round() as u8
            }
        })
        .unwrap_or_default()
        .min(5)
}

fn catalog_track_has_gap(track: &PlaylistIndexTrack, gap: &str) -> bool {
    match gap {
        "missing_genre" => taxonomy_value(track.genre.as_deref()).is_empty(),
        "missing_bpm" => track_bpm_value(track).is_none(),
        "missing_key" => taxonomy_value(track.key.as_deref()).is_empty(),
        "missing_label" => taxonomy_value(track.label.as_deref()).is_empty(),
        "missing_year" => taxonomy_value(track.year.as_deref()).is_empty(),
        "missing_artist" => taxonomy_value(track.artist.as_deref()).is_empty(),
        "missing_album" => taxonomy_value(track.album.as_deref()).is_empty(),
        _ => false,
    }
}

fn catalog_facets(
    tracks: &[PlaylistIndexTrack],
    criteria: &CatalogCriteria,
) -> PlaylistCatalogFacets {
    PlaylistCatalogFacets {
        genres: catalog_value_facets(tracks, criteria, "genres", 18),
        artists: catalog_value_facets(tracks, criteria, "artists", 12),
        albums: catalog_value_facets(tracks, criteria, "albums", 12),
        keys: catalog_value_facets(tracks, criteria, "keys", 24),
        years: catalog_value_facets(tracks, criteria, "years", 16),
        formats: catalog_value_facets(tracks, criteria, "formats", 12),
        ratings: (1..=5)
            .rev()
            .map(|rating| PlaylistCatalogFacetValue {
                value: rating.to_string(),
                name: format!("{rating} estrellas o mas"),
                count: tracks
                    .iter()
                    .filter(|track| {
                        catalog_track_matches(track, criteria, Some("ratings"))
                            && catalog_track_rating(track) >= rating
                    })
                    .count(),
            })
            .collect(),
        metadata_gaps: [
            ("missing_genre", "Sin genero"),
            ("missing_bpm", "Sin BPM"),
            ("missing_key", "Sin tonalidad"),
            ("missing_label", "Sin label"),
            ("missing_year", "Sin ano"),
            ("missing_artist", "Sin artista"),
            ("missing_album", "Sin album"),
        ]
        .into_iter()
        .map(|(value, name)| PlaylistCatalogFacetValue {
            value: value.to_string(),
            name: name.to_string(),
            count: tracks
                .iter()
                .filter(|track| {
                    catalog_track_matches(track, criteria, Some("metadata_gaps"))
                        && catalog_track_has_gap(track, value)
                })
                .count(),
        })
        .collect(),
        availability: [
            ("available", "Disponible"),
            ("missing", "Archivo no encontrado"),
        ]
        .into_iter()
        .map(|(value, name)| PlaylistCatalogFacetValue {
            value: value.to_string(),
            name: name.to_string(),
            count: tracks
                .iter()
                .filter(|track| {
                    catalog_track_matches(track, criteria, Some("availability"))
                        && if value == "available" {
                            track.source_exists
                        } else {
                            !track.source_exists
                        }
                })
                .count(),
        })
        .collect(),
    }
}

fn catalog_value_facets(
    tracks: &[PlaylistIndexTrack],
    criteria: &CatalogCriteria,
    facet: &str,
    limit: usize,
) -> Vec<PlaylistCatalogFacetValue> {
    let mut counts = BTreeMap::<String, usize>::new();
    for track in tracks {
        if !catalog_track_matches(track, criteria, Some(facet)) {
            continue;
        }
        let value = match facet {
            "genres" => track.genre.as_deref(),
            "artists" => track.artist.as_deref(),
            "albums" => track.album.as_deref(),
            "keys" => track.key.as_deref(),
            "years" => track.year.as_deref(),
            "formats" => track.kind.as_deref(),
            _ => None,
        }
        .map(str::trim)
        .filter(|value| !value.is_empty());
        if let Some(value) = value {
            increment_count(&mut counts, value.to_string());
        }
    }

    let mut values = counts
        .into_iter()
        .map(|(value, count)| PlaylistCatalogFacetValue {
            name: value.clone(),
            value,
            count,
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    let selected = match facet {
        "genres" => &criteria.filters.genres,
        "artists" => &criteria.filters.artists,
        "albums" => &criteria.filters.albums,
        "keys" => &criteria.filters.keys,
        "years" => &criteria.filters.years,
        "formats" => &criteria.filters.formats,
        _ => &criteria.filters.genres,
    };
    let mut result = values
        .iter()
        .filter(|item| {
            selected
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&item.value))
        })
        .cloned()
        .collect::<Vec<_>>();
    for item in values {
        if result.len() >= limit {
            break;
        }
        if !result.iter().any(|existing| existing.value == item.value) {
            result.push(item);
        }
    }
    result
}

fn sort_catalog_tracks(tracks: &mut [PlaylistIndexTrack], sort: &str, query_terms: &[String]) {
    tracks.sort_by(|left, right| match sort {
        "recent" => catalog_text(right.date_added.as_deref())
            .cmp(&catalog_text(left.date_added.as_deref()))
            .then_with(|| catalog_track_title(left).cmp(&catalog_track_title(right))),
        "rating" => catalog_track_rating(right)
            .cmp(&catalog_track_rating(left))
            .then_with(|| catalog_track_title(left).cmp(&catalog_track_title(right))),
        "bpm" => track_bpm_value(left)
            .unwrap_or(f64::MAX)
            .total_cmp(&track_bpm_value(right).unwrap_or(f64::MAX))
            .then_with(|| catalog_track_title(left).cmp(&catalog_track_title(right))),
        "title" => catalog_track_title(left).cmp(&catalog_track_title(right)),
        _ => catalog_relevance_score(right, query_terms)
            .cmp(&catalog_relevance_score(left, query_terms))
            .then_with(|| {
                catalog_text(left.artist.as_deref()).cmp(&catalog_text(right.artist.as_deref()))
            })
            .then_with(|| {
                catalog_text(left.album.as_deref()).cmp(&catalog_text(right.album.as_deref()))
            })
            .then_with(|| catalog_track_title(left).cmp(&catalog_track_title(right))),
    });
}

fn catalog_text(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_lowercase()
}

fn catalog_track_title(track: &PlaylistIndexTrack) -> String {
    catalog_text(track.name.as_deref()).to_string()
}

fn catalog_relevance_score(track: &PlaylistIndexTrack, query_terms: &[String]) -> usize {
    query_terms
        .iter()
        .map(|term| {
            let term = term.trim().to_lowercase();
            let title = catalog_text(track.name.as_deref());
            let artist = catalog_text(track.artist.as_deref());
            let album = catalog_text(track.album.as_deref());
            if title == term {
                100
            } else if title.starts_with(&term) {
                60
            } else if artist == term {
                50
            } else if artist.starts_with(&term) {
                35
            } else if title.contains(&term) {
                25
            } else if artist.contains(&term) || album.contains(&term) {
                15
            } else {
                5
            }
        })
        .sum()
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
        .join("cover-cache")
        .join(TRACK_COVER_CACHE_VERSION);
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

    let temporary_cover_path = cache_dir.join(format!("{cache_key}.{}.tmp.jpg", Uuid::new_v4()));
    let output = system::ffmpeg_command(app)
        .arg("-y")
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&source)
        .arg("-map")
        .arg("0:v:0?")
        .arg("-an")
        .arg("-vf")
        .arg(TRACK_COVER_FILTER)
        .arg("-filter_threads")
        .arg("1")
        .arg("-frames:v")
        .arg("1")
        .arg("-threads")
        .arg("1")
        .arg("-q:v")
        .arg("4")
        .arg(&temporary_cover_path)
        .output();

    match output {
        Ok(output) if output.status.success() && file_has_content(&temporary_cover_path) => {
            if !cover_path.is_file() {
                if let Err(error) = fs::rename(&temporary_cover_path, &cover_path) {
                    let _ = fs::remove_file(&temporary_cover_path);
                    if !cover_path.is_file() {
                        return Err(format!("No se pudo guardar thumbnail de portada: {error}"));
                    }
                }
            } else {
                let _ = fs::remove_file(&temporary_cover_path);
            }
            Ok(Some(cover_path.to_string_lossy().into_owned()))
        }
        _ => {
            let _ = fs::remove_file(&temporary_cover_path);
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
    bpm_anchor: Option<f64>,
}

fn playlist_copilot_generate_blocking(
    app: AppHandle,
    request: PlaylistCopilotRequest,
) -> Result<PlaylistCopilotResponse, String> {
    let user_message = request.message.trim().to_string();
    if user_message.is_empty() {
        return Err("Describe la playlist que quieres generar.".to_string());
    }
    let request_id = request
        .request_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let english = copilot_uses_english(request.language.as_deref());
    emit_copilot_progress(
        &app,
        &request_id,
        "brief",
        "working",
        if english {
            "Interpreting your latest instruction"
        } else {
            "Interpretando tu ultima instruccion"
        },
        None,
    );

    let target_count = request.target_count.unwrap_or(30).clamp(5, 120);
    let mut conn = open_db(&app)?;
    let library = get_library(&conn, &request.library_id)?
        .ok_or_else(|| format!("Libreria indexada no encontrada: {}", request.library_id))?;
    let tracks = list_taxonomy_tracks(&conn, &request.library_id)?;
    if tracks.is_empty() {
        return Err("La libreria indexada no tiene tracks para sugerir.".to_string());
    }

    let profile = playlist_copilot_profile(&tracks);
    let previous_intent =
        load_previous_copilot_intent(&conn, &request.library_id, request.session_id.as_deref())?;
    let api_key = settings::load_openai_api_key(&app)?;
    let mut used_openai = false;
    let mut openai_error = None;
    let mut interpreted = if request.guided_answer.is_some() && previous_intent.is_some() {
        previous_intent.clone().unwrap_or_default()
    } else if let Some(api_key) = api_key.as_deref() {
        match request_copilot_interpretation(
            api_key,
            &user_message,
            &profile,
            target_count,
            previous_intent.as_ref(),
        ) {
            Ok(interpretation) => {
                used_openai = true;
                interpretation
            }
            Err(error) => {
                openai_error = Some(error);
                local_copilot_interpretation(
                    &user_message,
                    &profile,
                    target_count,
                    previous_intent.as_ref(),
                )
            }
        }
    } else {
        local_copilot_interpretation(
            &user_message,
            &profile,
            target_count,
            previous_intent.as_ref(),
        )
    };
    if let Some(answer) = request.guided_answer.as_ref() {
        apply_guided_answer(&mut interpreted, answer, profile.bpm_anchor);
    }
    interpreted.target_count = Some(target_count);
    interpreted = normalize_copilot_interpretation(interpreted);
    let brief_changes =
        playlist_copilot_brief_changes(previous_intent.as_ref(), &interpreted, english);
    emit_copilot_progress(
        &app,
        &request_id,
        "brief",
        "done",
        if previous_intent.is_some() {
            if english {
                "Updated the working brief implicitly"
            } else {
                "Actualice el brief de trabajo implicitamente"
            }
        } else if english {
            "Created the working brief"
        } else {
            "Cree el brief de trabajo"
        },
        Some(brief_changes.join(" · ")),
    );

    let guided_mode = request.mode.as_deref() == Some("guided");
    let answered_question_ids = request.answered_question_ids.clone().unwrap_or_default();
    let planned_guided_questions =
        playlist_copilot_guided_questions(&interpreted, &profile, english);
    if guided_mode {
        if let Some(next_question) =
            next_unanswered_copilot_question(&planned_guided_questions, &answered_question_ids)
        {
            let candidates = Vec::<PlaylistCopilotCandidate>::new();
            let coverage = playlist_copilot_coverage(&candidates);
            let reasoning_summary = if english {
                vec![
                    "I am collecting the playlist brief one decision at a time before searching SQLite."
                        .to_string(),
                    format!(
                        "{} guided decision(s) captured, next decision: {}.",
                        answered_question_ids.len(),
                        next_question.question
                    ),
                ]
            } else {
                vec![
                    "Estoy reuniendo el brief de la playlist una decision a la vez antes de buscar en SQLite."
                        .to_string(),
                    format!(
                        "{} decision(es) guiadas capturadas; siguiente decision: {}.",
                        answered_question_ids.len(),
                        next_question.question
                    ),
                ]
            };
            let steps = playlist_copilot_brief_steps(
                &profile,
                &interpreted,
                &answered_question_ids,
                &next_question,
                used_openai,
                english,
            );
            let mut message = if english {
                format!(
                    "I am going step by step. Before searching tracks I need to define: {}",
                    next_question.question
                )
            } else {
                format!(
                    "Voy paso a paso. Antes de buscar tracks necesito definir: {}",
                    next_question.question
                )
            };
            if let Some(error) = openai_error {
                message.push_str(&format!(" OpenAI no respondio correctamente: {error}"));
            }
            let session_id = persist_playlist_copilot_brief_turn(
                &mut conn,
                &request.library_id,
                request.session_id.as_deref(),
                &user_message,
                &message,
                &interpreted,
            )?;
            emit_copilot_progress(
                &app,
                &request_id,
                "brief-question",
                "waiting",
                if english {
                    "The brief needs one more decision"
                } else {
                    "El brief necesita una decision mas"
                },
                Some(next_question.question.clone()),
            );

            return Ok(PlaylistCopilotResponse {
                session_id,
                candidate_set_id: String::new(),
                message,
                interpreted,
                questions: Vec::new(),
                guided_questions: vec![next_question],
                steps,
                brief_changes,
                search_trace: Vec::new(),
                reasoning_summary,
                title_suggestions: Vec::new(),
                coverage,
                candidates,
                used_openai,
                ranker_version: COPILOT_RANKER_VERSION.to_string(),
            });
        }
    }

    let search_probes = playlist_copilot_search_probes(&user_message, &interpreted, english);
    emit_copilot_progress(
        &app,
        &request_id,
        "search-plan",
        "done",
        if english {
            "Planned several focused library searches"
        } else {
            "Planifique varias busquedas focalizadas en la libreria"
        },
        Some(format!(
            "{}: {}",
            search_probes.len(),
            search_probes
                .iter()
                .map(|probe| probe.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    );
    let vector_search_available =
        api_key.is_some() && tracks.iter().any(|track| track.embedding_ready);
    let mut search_warning = None;
    let (semantic_evidence, search_trace) = if vector_search_available {
        let api_key = api_key
            .as_deref()
            .expect("vector search requires an OpenAI API key");
        match playlist_copilot_semantic_evidence(
            &app,
            &conn,
            &request.library_id,
            api_key,
            &search_probes,
            &request_id,
            english,
        ) {
            Ok(result) => result,
            Err(error) => {
                search_warning = Some(error);
                playlist_copilot_local_evidence(&app, &tracks, &search_probes, &request_id, english)
            }
        }
    } else {
        playlist_copilot_local_evidence(&app, &tracks, &search_probes, &request_id, english)
    };
    let recent_suggestion_counts = recent_copilot_suggestion_counts(&conn, &request.library_id, 8)?;
    let exploration_seed = playlist_copilot_exploration_seed(
        &conn,
        &request.library_id,
        request.session_id.as_deref(),
        &user_message,
    )?;
    emit_copilot_progress(
        &app,
        &request_id,
        "ranking",
        "working",
        if english {
            "Fusing search evidence and rotating recent results"
        } else {
            "Fusionando evidencia y rotando resultados recientes"
        },
        Some(format!(
            "{} tracks con evidencia; historial de 8 corridas",
            semantic_evidence.len()
        )),
    );
    let candidates = rank_copilot_candidates(
        &tracks,
        &interpreted,
        &user_message,
        target_count,
        &semantic_evidence,
        &recent_suggestion_counts,
        exploration_seed,
    );
    let coverage = playlist_copilot_coverage(&candidates);
    emit_copilot_progress(
        &app,
        &request_id,
        "ranking",
        "done",
        if english {
            "Fused search evidence and rotated recent results"
        } else {
            "Fusione evidencia y rote resultados recientes"
        },
        Some(format!(
            "{} candidatos seleccionados desde {} tracks con evidencia",
            candidates.len(),
            semantic_evidence.len()
        )),
    );
    emit_copilot_progress(
        &app,
        &request_id,
        "sequencing",
        "done",
        if english {
            "Sequenced the final candidates"
        } else {
            "Secuencie los candidatos finales"
        },
        Some(playlist_copilot_coverage_sentence(&coverage, &interpreted)),
    );
    let questions = if guided_mode {
        Vec::new()
    } else {
        playlist_copilot_questions(&interpreted, candidates.len())
    };
    let guided_questions = if guided_mode {
        Vec::new()
    } else {
        playlist_copilot_guided_questions(&interpreted, &profile, english)
    };
    let reasoning_summary = playlist_copilot_reasoning_summary(
        &interpreted,
        &coverage,
        &candidates,
        used_openai,
        vector_search_available && search_warning.is_none(),
        search_trace.len(),
    );
    let steps = playlist_copilot_steps(
        &profile,
        &interpreted,
        &coverage,
        &candidates,
        used_openai,
        vector_search_available && search_warning.is_none(),
    );
    let title_suggestions =
        playlist_copilot_title_suggestions(&user_message, &interpreted, &coverage);
    let mut message = if used_openai && english {
        format!(
            "I interpreted the brief, searched SQLite, and built {} candidate(s) in {}.",
            candidates.len(),
            library.source_name
        )
    } else if used_openai {
        format!(
            "Interprete el brief con OpenAI, revise SQLite y arme {} candidato(s) en {}.",
            candidates.len(),
            library.source_name
        )
    } else if english {
        format!(
            "I used the local planner and built {} candidate(s) in {}.",
            candidates.len(),
            library.source_name
        )
    } else {
        format!(
            "Use ranking local por pasos y arme {} candidato(s) en {}.",
            candidates.len(),
            library.source_name
        )
    };
    if let Some(error) = openai_error {
        message.push_str(&format!(" OpenAI no respondio correctamente: {error}"));
    }
    if let Some(error) = search_warning {
        message.push_str(&format!(
            " La busqueda vectorial fallo y use probes locales: {error}"
        ));
    }
    let (session_id, candidate_set_id) = persist_playlist_copilot_run(
        &mut conn,
        &request.library_id,
        request.session_id.as_deref(),
        &user_message,
        &message,
        &interpreted,
        &reasoning_summary,
        &coverage,
        &candidates,
    )?;

    Ok(PlaylistCopilotResponse {
        session_id,
        candidate_set_id,
        message,
        interpreted,
        questions,
        guided_questions,
        steps,
        brief_changes,
        search_trace,
        reasoning_summary,
        title_suggestions,
        coverage,
        candidates,
        used_openai,
        ranker_version: COPILOT_RANKER_VERSION.to_string(),
    })
}

fn playlist_copilot_profile(tracks: &[PlaylistIndexTrack]) -> PlaylistCopilotProfile {
    let mut genre_counts = BTreeMap::<String, usize>::new();
    let mut artist_counts = BTreeMap::<String, usize>::new();
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut bpm_counts = BTreeMap::<i64, usize>::new();
    let mut bpm_min: Option<f64> = None;
    let mut bpm_max: Option<f64> = None;

    for track in tracks {
        increment_count(&mut genre_counts, taxonomy_value(track.genre.as_deref()));
        increment_count(&mut artist_counts, taxonomy_value(track.artist.as_deref()));
        increment_count(&mut key_counts, taxonomy_value(track.key.as_deref()));
        if let Some(bpm) = track_bpm_value(track) {
            bpm_min = Some(bpm_min.map_or(bpm, |current| current.min(bpm)));
            bpm_max = Some(bpm_max.map_or(bpm, |current| current.max(bpm)));
            *bpm_counts.entry(bpm.round() as i64).or_default() += 1;
        }
    }
    let bpm_anchor = bpm_counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(bpm, _)| bpm as f64);

    PlaylistCopilotProfile {
        track_count: tracks.len(),
        genres: top_profile_values(&genre_counts, 80),
        artists: top_profile_values(&artist_counts, 160),
        keys: top_profile_values(&key_counts, 32),
        bpm_min,
        bpm_max,
        bpm_anchor,
    }
}

fn playlist_copilot_coverage(candidates: &[PlaylistCopilotCandidate]) -> PlaylistCopilotCoverage {
    let mut genre_counts = BTreeMap::<String, usize>::new();
    let mut key_counts = BTreeMap::<String, usize>::new();
    let mut format_counts = BTreeMap::<String, usize>::new();
    let mut artist_counts = BTreeMap::<String, usize>::new();
    let mut source_missing_count = 0_usize;
    let mut bpm_known_count = 0_usize;
    let mut bpm_sum = 0.0_f64;
    let mut bpm_min: Option<f64> = None;
    let mut bpm_max: Option<f64> = None;

    for candidate in candidates {
        let track = &candidate.track;
        increment_count(&mut genre_counts, taxonomy_value(track.genre.as_deref()));
        increment_count(&mut key_counts, taxonomy_value(track.key.as_deref()));
        increment_count(&mut format_counts, taxonomy_value(track.kind.as_deref()));
        increment_count(&mut artist_counts, taxonomy_value(track.artist.as_deref()));
        if !track.source_exists {
            source_missing_count += 1;
        }
        if let Some(bpm) = track_bpm_value(track) {
            bpm_known_count += 1;
            bpm_sum += bpm;
            bpm_min = Some(bpm_min.map_or(bpm, |current| current.min(bpm)));
            bpm_max = Some(bpm_max.map_or(bpm, |current| current.max(bpm)));
        }
    }

    PlaylistCopilotCoverage {
        track_count: candidates.len(),
        source_missing_count,
        bpm_min,
        bpm_max,
        bpm_average: (bpm_known_count > 0).then_some(bpm_sum / bpm_known_count as f64),
        genres: counts_to_taxonomy("genre", &genre_counts, "Sin genero", 8, false),
        keys: counts_to_taxonomy("key", &key_counts, "Sin key", 10, false),
        formats: counts_to_taxonomy("format", &format_counts, "Formato desconocido", 8, false),
        top_artists: counts_to_taxonomy("artist", &artist_counts, "Sin artista", 10, false),
    }
}

fn playlist_copilot_steps(
    profile: &PlaylistCopilotProfile,
    interpreted: &PlaylistCopilotInterpretation,
    coverage: &PlaylistCopilotCoverage,
    candidates: &[PlaylistCopilotCandidate],
    used_openai: bool,
    used_vectors: bool,
) -> Vec<PlaylistCopilotStep> {
    vec![
        PlaylistCopilotStep {
            label: "Library scan".to_string(),
            status: "done".to_string(),
            detail: format!(
                "Read {} indexed tracks, {} genre signals and {} key signals from SQLite.",
                profile.track_count,
                profile.genres.len(),
                profile.keys.len()
            ),
        },
        PlaylistCopilotStep {
            label: "Brief interpretation".to_string(),
            status: "done".to_string(),
            detail: if used_openai {
                "OpenAI structured the brief; local rules normalized the filters.".to_string()
            } else {
                "Local parser inferred BPM, genre, artist, key, mood and exclusions.".to_string()
            },
        },
        PlaylistCopilotStep {
            label: "Search tools".to_string(),
            status: "done".to_string(),
            detail: if used_vectors {
                "Combined metadata scoring with available vector similarity.".to_string()
            } else {
                "Used local SQLite metadata, text terms, BPM and key matching.".to_string()
            },
        },
        PlaylistCopilotStep {
            label: "Ranking and diversity".to_string(),
            status: "done".to_string(),
            detail: format!(
                "Ranked candidates, softened repeated artists and kept {} selected tracks.",
                candidates.len()
            ),
        },
        PlaylistCopilotStep {
            label: "Coverage check".to_string(),
            status: if coverage.source_missing_count > 0 {
                "warning".to_string()
            } else {
                "done".to_string()
            },
            detail: playlist_copilot_coverage_sentence(coverage, interpreted),
        },
    ]
}

fn playlist_copilot_brief_steps(
    profile: &PlaylistCopilotProfile,
    interpreted: &PlaylistCopilotInterpretation,
    answered_question_ids: &[String],
    next_question: &PlaylistCopilotQuestion,
    used_openai: bool,
    english: bool,
) -> Vec<PlaylistCopilotStep> {
    vec![
        PlaylistCopilotStep {
            label: if english { "Library scan" } else { "Lectura de libreria" }.to_string(),
            status: "done".to_string(),
            detail: if english {
                format!(
                    "Read {} indexed tracks and prepared local genre, artist, key and BPM signals.",
                    profile.track_count
                )
            } else {
                format!(
                    "Lei {} tracks indexados y prepare senales locales de genero, artista, key y BPM.",
                    profile.track_count
                )
            },
        },
        PlaylistCopilotStep {
            label: if english { "Brief interpretation" } else { "Interpretacion del brief" }
                .to_string(),
            status: "done".to_string(),
            detail: if used_openai && english {
                "Interpreted the conversation with OpenAI before asking the next question."
                    .to_string()
            } else if used_openai {
                "Interprete la conversacion con OpenAI antes de preguntar el siguiente paso."
                    .to_string()
            } else if english {
                "Interpreted the conversation with local metadata matching before asking the next question."
                    .to_string()
            } else {
                "Interprete la conversacion con metadata local antes de preguntar el siguiente paso."
                    .to_string()
            },
        },
        PlaylistCopilotStep {
            label: if english { "Guided brief" } else { "Brief guiado" }.to_string(),
            status: "warning".to_string(),
            detail: if english {
                format!(
                    "Waiting for one answer: {}. Captured {} decision(s) so far.",
                    next_question.question,
                    answered_question_ids.len()
                )
            } else {
                format!(
                    "Esperando una respuesta: {}. He capturado {} decision(es) hasta ahora.",
                    next_question.question,
                    answered_question_ids.len()
                )
            },
        },
        PlaylistCopilotStep {
            label: if english { "Search" } else { "Busqueda" }.to_string(),
            status: "warning".to_string(),
            detail: if english {
                "Paused candidate ranking until the guided brief has enough context."
            } else {
                "Pause el ranking de candidatos hasta que el brief guiado tenga suficiente contexto."
            }
            .to_string(),
        },
        PlaylistCopilotStep {
            label: if english { "Current signals" } else { "Senales actuales" }.to_string(),
            status: "done".to_string(),
            detail: format!(
                "{}: {}; {}: {}; BPM: {}.",
                if english { "Genres" } else { "Generos" },
                interpreted.genres.join(", "),
                if english { "artists" } else { "artistas" },
                interpreted.artists.join(", "),
                match (interpreted.bpm_min, interpreted.bpm_max) {
                    (Some(min), Some(max)) => format!("{min:.0}-{max:.0}"),
                    (Some(min), None) => format!("{min:.0}+"),
                    (None, Some(max)) if english => format!("up to {max:.0}"),
                    (None, Some(max)) => format!("hasta {max:.0}"),
                    (None, None) if english => "not set".to_string(),
                    (None, None) => "sin definir".to_string(),
                }
            ),
        },
    ]
}

fn playlist_copilot_reasoning_summary(
    interpreted: &PlaylistCopilotInterpretation,
    coverage: &PlaylistCopilotCoverage,
    candidates: &[PlaylistCopilotCandidate],
    used_openai: bool,
    used_vectors: bool,
    search_count: usize,
) -> Vec<String> {
    let mut summary = Vec::new();
    summary.push(if used_openai {
        "The brief was interpreted with OpenAI, then normalized and executed against local SQLite data.".to_string()
    } else {
        "The brief was interpreted locally, so no AI request was required for planning.".to_string()
    });
    if used_vectors {
        summary.push(format!(
            "Fused {search_count} focused vector searches instead of relying on one broad query."
        ));
    } else {
        summary.push(format!(
            "Fused {search_count} focused local metadata searches instead of relying on one broad query."
        ));
    }
    summary.push(
        "Recent suggestions were penalized and close matches were rotated with a per-run exploration seed."
            .to_string(),
    );
    if let (Some(min), Some(max)) = (interpreted.bpm_min, interpreted.bpm_max) {
        summary.push(format!(
            "BPM matching prioritized tracks between {min:.0} and {max:.0}."
        ));
    }
    if !interpreted.genres.is_empty() {
        summary.push(format!(
            "Genre matching prioritized: {}.",
            interpreted.genres.join(", ")
        ));
    }
    if !interpreted.keys.is_empty() {
        summary.push(format!(
            "Key matching prioritized: {}.",
            interpreted.keys.join(", ")
        ));
    }
    if coverage.source_missing_count > 0 {
        summary.push(format!(
            "{} candidate(s) have missing source files and should be reviewed before export.",
            coverage.source_missing_count
        ));
    }
    if let Some(top) = candidates.first() {
        summary.push(format!(
            "Top candidate: {} because {}.",
            top.track.name.as_deref().unwrap_or(&top.track.track_id),
            top.reasons
                .first()
                .map(String::as_str)
                .unwrap_or("it matched the strongest metadata signals")
        ));
    }
    summary
}

fn playlist_copilot_guided_questions(
    interpreted: &PlaylistCopilotInterpretation,
    profile: &PlaylistCopilotProfile,
    english: bool,
) -> Vec<PlaylistCopilotQuestion> {
    let mut questions = Vec::new();

    if interpreted.genres.is_empty()
        && interpreted.artists.is_empty()
        && interpreted.mood.is_none()
        && interpreted.energy.is_none()
    {
        let first_genre = profile.genres.first().cloned().unwrap_or_else(|| {
            if english {
                "the strongest local genre cluster".to_string()
            } else {
                "el cluster de genero local mas fuerte".to_string()
            }
        });
        let second_genre = profile.genres.get(1).cloned().unwrap_or_else(|| {
            if english {
                "a contrasting but compatible direction".to_string()
            } else {
                "una direccion contrastante pero compatible".to_string()
            }
        });
        questions.push(PlaylistCopilotQuestion {
            id: "style_focus".to_string(),
            question: if english {
                "What musical direction should I prioritize?"
            } else {
                "Que direccion musical deberia priorizar?"
            }
            .to_string(),
            options: if english {
                vec![
                    copilot_option(
                        &format!("Lean into {first_genre}"),
                        &format!("genre:{first_genre}"),
                        "Uses a strong genre cluster from your indexed library.",
                    ),
                    copilot_option(
                        &format!("Explore {second_genre}"),
                        &format!("genre:{second_genre}"),
                        "Keeps the brief focused but opens a second lane.",
                    ),
                    copilot_option(
                        "Mood first",
                        "mood_first",
                        "Good when the playlist is about a feeling more than a genre.",
                    ),
                ]
            } else {
                vec![
                    copilot_option(
                        &format!("Ir hacia {first_genre}"),
                        &format!("genre:{first_genre}"),
                        "Usa un cluster de genero fuerte de tu libreria indexada.",
                    ),
                    copilot_option(
                        &format!("Explorar {second_genre}"),
                        &format!("genre:{second_genre}"),
                        "Mantiene el brief enfocado pero abre una segunda direccion.",
                    ),
                    copilot_option(
                        "Primero el mood",
                        "mood_first",
                        "Sirve cuando la playlist va mas por sensacion que por genero.",
                    ),
                ]
            },
        });
    }

    questions.push(PlaylistCopilotQuestion {
        id: "set_shape".to_string(),
        question: if english {
            "What shape should the playlist have?"
        } else {
            "Que forma deberia tener la playlist?"
        }
        .to_string(),
        options: if english {
            vec![
                copilot_option(
                    "Slow build",
                    "slow_build",
                    "Good for opening sets and long transitions.",
                ),
                copilot_option(
                    "Flat warmup",
                    "flat",
                    "Keeps the room stable instead of pushing too early.",
                ),
                copilot_option(
                    "Energy ramp",
                    "energy_ramp",
                    "Useful when preparing a handoff into a harder set.",
                ),
            ]
        } else {
            vec![
                copilot_option(
                    "Construccion lenta",
                    "slow_build",
                    "Bueno para openings y transiciones largas.",
                ),
                copilot_option(
                    "Warmup estable",
                    "flat",
                    "Mantiene la pista estable sin empujar demasiado temprano.",
                ),
                copilot_option(
                    "Rampa de energia",
                    "energy_ramp",
                    "Util para preparar una entrega hacia un set mas fuerte.",
                ),
            ]
        },
    });

    if interpreted.keys.is_empty() {
        questions.push(PlaylistCopilotQuestion {
            id: "harmony".to_string(),
            question: if english {
                "How strict should harmonic compatibility be?"
            } else {
                "Que tan estricta debe ser la compatibilidad armonica?"
            }
            .to_string(),
            options: if english {
                vec![
                    copilot_option(
                        "Strict key flow",
                        "strict",
                        "Best when key metadata is reliable.",
                    ),
                    copilot_option(
                        "Loose key flow",
                        "soft",
                        "Balanced for imperfect Rekordbox key analysis.",
                    ),
                    copilot_option(
                        "Ignore key",
                        "ignore",
                        "Useful when key metadata is incomplete.",
                    ),
                ]
            } else {
                vec![
                    copilot_option(
                        "Key estricta",
                        "strict",
                        "Mejor cuando la metadata de key es confiable.",
                    ),
                    copilot_option(
                        "Key flexible",
                        "soft",
                        "Balanceado para analisis imperfecto de Rekordbox.",
                    ),
                    copilot_option(
                        "Ignorar key",
                        "ignore",
                        "Util cuando la metadata de key esta incompleta.",
                    ),
                ]
            },
        });
    }

    questions.push(PlaylistCopilotQuestion {
        id: "discovery".to_string(),
        question: if english {
            "Should the assistant favor known anchors or discoveries?"
        } else {
            "Deberia favorecer anclas conocidas o descubrimientos?"
        }
        .to_string(),
        options: if english {
            vec![
                copilot_option(
                    "Balanced",
                    "balanced",
                    "A reliable default for exportable playlists.",
                ),
                copilot_option(
                    "More known artists",
                    "known",
                    "Makes the result feel safer and more recognizable.",
                ),
                copilot_option(
                    "Discovery mode",
                    "discovery",
                    "Better for finding material outside the obvious picks.",
                ),
            ]
        } else {
            vec![
                copilot_option(
                    "Balanceado",
                    "balanced",
                    "Default confiable para playlists exportables.",
                ),
                copilot_option(
                    "Mas conocidos",
                    "known",
                    "Hace que el resultado se sienta mas seguro y reconocible.",
                ),
                copilot_option(
                    "Modo descubrimiento",
                    "discovery",
                    "Mejor para encontrar material fuera de lo obvio.",
                ),
            ]
        },
    });

    if interpreted.bpm_min.is_none() && interpreted.bpm_max.is_none() {
        questions.push(PlaylistCopilotQuestion {
            id: "tempo".to_string(),
            question: if english {
                "Should tempo be constrained?"
            } else {
                "Deberiamos acotar el tempo?"
            }
            .to_string(),
            options: if english {
                vec![
                    copilot_option("Tight BPM range", "tight", "Makes mixing easier."),
                    copilot_option(
                        "Flexible tempo",
                        "flexible",
                        "Finds better musical matches with looser mixing constraints.",
                    ),
                ]
            } else {
                vec![
                    copilot_option("Rango BPM cerrado", "tight", "Hace la mezcla mas facil."),
                    copilot_option(
                        "Tempo flexible",
                        "flexible",
                        "Encuentra mejores matches musicales con restricciones mas abiertas.",
                    ),
                ]
            },
        });
    }

    questions.truncate(5);
    questions
}

fn playlist_copilot_brief_changes(
    previous: Option<&PlaylistCopilotInterpretation>,
    current: &PlaylistCopilotInterpretation,
    english: bool,
) -> Vec<String> {
    let Some(previous) = previous else {
        let mut signals = Vec::new();
        if !current.genres.is_empty() {
            signals.push(format!(
                "{}: {}",
                if english { "genres" } else { "generos" },
                current.genres.join(", ")
            ));
        }
        if !current.artists.is_empty() {
            signals.push(format!(
                "{}: {}",
                if english { "artists" } else { "artistas" },
                current.artists.join(", ")
            ));
        }
        if let Some(mood) = current.mood.as_deref() {
            signals.push(format!("mood: {mood}"));
        }
        if let Some(energy) = current.energy.as_deref() {
            signals.push(format!(
                "{}: {energy}",
                if english { "energy" } else { "energia" }
            ));
        }
        if current.bpm_min.is_some() || current.bpm_max.is_some() {
            signals.push(format!(
                "BPM: {}",
                copilot_bpm_value(current.bpm_min, current.bpm_max)
            ));
        }
        return vec![if signals.is_empty() {
            if english {
                "Initialized an open brief; the next messages can refine it without starting over."
                    .to_string()
            } else {
                "Inicie un brief abierto; los siguientes mensajes pueden ajustarlo sin partir de cero."
                    .to_string()
            }
        } else if english {
            format!("Initial brief: {}.", signals.join("; "))
        } else {
            format!("Brief inicial: {}.", signals.join("; "))
        }];
    };

    let mut changes = Vec::new();
    push_copilot_list_change(
        &mut changes,
        if english { "Genres" } else { "Generos" },
        &previous.genres,
        &current.genres,
        english,
    );
    push_copilot_list_change(
        &mut changes,
        if english { "Artists" } else { "Artistas" },
        &previous.artists,
        &current.artists,
        english,
    );
    push_copilot_list_change(&mut changes, "Keys", &previous.keys, &current.keys, english);
    push_copilot_list_change(
        &mut changes,
        if english { "Exclusions" } else { "Exclusiones" },
        &previous.exclude_terms,
        &current.exclude_terms,
        english,
    );
    if (previous.bpm_min, previous.bpm_max) != (current.bpm_min, current.bpm_max) {
        changes.push(format!(
            "BPM -> {}",
            copilot_bpm_value(current.bpm_min, current.bpm_max)
        ));
    }
    push_copilot_optional_change(&mut changes, "Mood", &previous.mood, &current.mood, english);
    push_copilot_optional_change(
        &mut changes,
        if english { "Energy" } else { "Energia" },
        &previous.energy,
        &current.energy,
        english,
    );

    let policies = [
        (
            if english {
                "Energy curve"
            } else {
                "Curva de energia"
            },
            copilot_policy_value(&previous.energy_curve),
            copilot_policy_value(&current.energy_curve),
        ),
        (
            if english { "Harmony" } else { "Armonia" },
            copilot_policy_value(&previous.harmonic_policy),
            copilot_policy_value(&current.harmonic_policy),
        ),
        (
            "Discovery",
            copilot_policy_value(&previous.discovery_mode),
            copilot_policy_value(&current.discovery_mode),
        ),
        (
            "Tempo",
            copilot_policy_value(&previous.tempo_policy),
            copilot_policy_value(&current.tempo_policy),
        ),
        (
            if english { "Files" } else { "Archivos" },
            copilot_policy_value(&previous.source_policy),
            copilot_policy_value(&current.source_policy),
        ),
        (
            if english { "Focus" } else { "Foco" },
            copilot_policy_value(&previous.focus_policy),
            copilot_policy_value(&current.focus_policy),
        ),
    ];
    for (label, before, after) in policies {
        if before != after {
            changes.push(format!("{label} -> {}", after.replace('_', " ")));
        }
    }

    if changes.is_empty() {
        changes.push(if english {
            "Kept the existing brief and treated the message as additional context.".to_string()
        } else {
            "Mantuve el brief existente y tome el mensaje como contexto adicional.".to_string()
        });
    }
    changes
}

fn push_copilot_list_change(
    changes: &mut Vec<String>,
    label: &str,
    previous: &[String],
    current: &[String],
    english: bool,
) {
    if previous != current {
        changes.push(format!(
            "{label} -> {}",
            if current.is_empty() {
                if english { "no filter" } else { "sin filtro" }.to_string()
            } else {
                current.join(", ")
            }
        ));
    }
}

fn push_copilot_optional_change(
    changes: &mut Vec<String>,
    label: &str,
    previous: &Option<String>,
    current: &Option<String>,
    english: bool,
) {
    if previous != current {
        changes.push(format!(
            "{label} -> {}",
            current
                .as_deref()
                .unwrap_or(if english { "no filter" } else { "sin filtro" })
        ));
    }
}

fn copilot_policy_value<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn copilot_bpm_value(min: Option<f64>, max: Option<f64>) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!("{min:.0}-{max:.0}"),
        (Some(min), None) => format!("{min:.0}+"),
        (None, Some(max)) => format!("<={max:.0}"),
        (None, None) => "open".to_string(),
    }
}

fn copilot_uses_english(language: Option<&str>) -> bool {
    language
        .map(|value| value.eq_ignore_ascii_case("en"))
        .unwrap_or(false)
}

fn next_unanswered_copilot_question(
    questions: &[PlaylistCopilotQuestion],
    answered_question_ids: &[String],
) -> Option<PlaylistCopilotQuestion> {
    questions
        .iter()
        .find(|question| {
            !answered_question_ids
                .iter()
                .any(|answered| answered == &question.id)
        })
        .cloned()
}

fn copilot_option(label: &str, value: &str, description: &str) -> PlaylistCopilotQuestionOption {
    PlaylistCopilotQuestionOption {
        label: label.to_string(),
        value: value.to_string(),
        description: description.to_string(),
    }
}

fn playlist_copilot_title_suggestions(
    prompt: &str,
    interpreted: &PlaylistCopilotInterpretation,
    coverage: &PlaylistCopilotCoverage,
) -> Vec<PlaylistCopilotTitleSuggestion> {
    let mood = interpreted
        .mood
        .as_deref()
        .or(interpreted.energy.as_deref())
        .unwrap_or("Session");
    let genre = interpreted
        .genres
        .first()
        .cloned()
        .or_else(|| coverage.genres.first().map(|item| item.name.clone()))
        .unwrap_or_else(|| "Selections".to_string());
    let bpm = match (coverage.bpm_min, coverage.bpm_max) {
        (Some(min), Some(max)) => format!("{min:.0}-{max:.0} BPM"),
        _ => "Open Tempo".to_string(),
    };
    let compact_prompt = prompt
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");

    vec![
        PlaylistCopilotTitleSuggestion {
            title: format!("{genre} {mood}"),
            rationale: "Uses the dominant genre and mood from the brief.".to_string(),
        },
        PlaylistCopilotTitleSuggestion {
            title: format!("{bpm} Run"),
            rationale: "Names the playlist around its tempo corridor.".to_string(),
        },
        PlaylistCopilotTitleSuggestion {
            title: format!("{genre} Draft {}", coverage.track_count),
            rationale: "Practical export name with track count context.".to_string(),
        },
        PlaylistCopilotTitleSuggestion {
            title: if compact_prompt.is_empty() {
                "Copilot Session".to_string()
            } else {
                title_case(&compact_prompt)
            },
            rationale: "Condenses the original prompt into a short working title.".to_string(),
        },
    ]
}

fn playlist_copilot_coverage_sentence(
    coverage: &PlaylistCopilotCoverage,
    interpreted: &PlaylistCopilotInterpretation,
) -> String {
    let bpm = match (coverage.bpm_min, coverage.bpm_max, coverage.bpm_average) {
        (Some(min), Some(max), Some(avg)) => format!("BPM {min:.0}-{max:.0}, avg {avg:.0}"),
        _ => "BPM coverage is incomplete".to_string(),
    };
    let genre = coverage
        .genres
        .first()
        .map(|item| item.name.clone())
        .or_else(|| interpreted.genres.first().cloned())
        .unwrap_or_else(|| "mixed genres".to_string());
    format!(
        "{} candidate(s), top genre {}, {}, {} missing source file(s).",
        coverage.track_count, genre, bpm, coverage.source_missing_count
    )
}

fn persist_playlist_copilot_run(
    conn: &mut Connection,
    library_id: &str,
    session_id: Option<&str>,
    user_message: &str,
    assistant_message: &str,
    interpreted: &PlaylistCopilotInterpretation,
    reasoning_summary: &[String],
    coverage: &PlaylistCopilotCoverage,
    candidates: &[PlaylistCopilotCandidate],
) -> Result<(String, String), String> {
    let now = timestamp();
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transaccion Copilot: {error}"))?;
    let session_id =
        upsert_copilot_session(&tx, library_id, session_id, user_message, interpreted, &now)?;

    insert_copilot_message(&tx, &session_id, "user", user_message, &now)?;
    insert_copilot_message(&tx, &session_id, "assistant", assistant_message, &now)?;

    let candidate_set_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO playlist_copilot_candidate_sets (
            id, session_id, prompt, interpretation_json, reasoning_json, coverage_json,
            ranker_version, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &candidate_set_id,
            &session_id,
            user_message,
            serde_json::to_string(interpreted).map_err(|error| format!(
                "No se pudo serializar interpretacion Copilot: {error}"
            ))?,
            serde_json::to_string(reasoning_summary)
                .map_err(|error| format!("No se pudo serializar reasoning Copilot: {error}"))?,
            serde_json::to_string(coverage)
                .map_err(|error| format!("No se pudo serializar coverage Copilot: {error}"))?,
            COPILOT_RANKER_VERSION,
            &now
        ],
    )
    .map_err(|error| format!("No se pudo guardar candidate set Copilot: {error}"))?;

    for (position, candidate) in candidates.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_copilot_candidate_tracks (
                candidate_set_id, track_id, position, score, reasons_json, score_components_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &candidate_set_id,
                &candidate.track.track_id,
                position as i64,
                candidate.score,
                serde_json::to_string(&candidate.reasons)
                    .map_err(|error| format!("No se pudo serializar razones Copilot: {error}"))?,
                serde_json::to_string(&candidate.score_components).map_err(|error| format!(
                    "No se pudieron serializar componentes Copilot: {error}"
                ))?
            ],
        )
        .map_err(|error| format!("No se pudo guardar track candidato Copilot: {error}"))?;
    }

    tx.commit()
        .map_err(|error| format!("No se pudo confirmar transaccion Copilot: {error}"))?;
    Ok((session_id, candidate_set_id))
}

fn persist_playlist_copilot_brief_turn(
    conn: &mut Connection,
    library_id: &str,
    session_id: Option<&str>,
    user_message: &str,
    assistant_message: &str,
    interpreted: &PlaylistCopilotInterpretation,
) -> Result<String, String> {
    let now = timestamp();
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transaccion Copilot: {error}"))?;
    let session_id =
        upsert_copilot_session(&tx, library_id, session_id, user_message, interpreted, &now)?;
    insert_copilot_message(&tx, &session_id, "user", user_message, &now)?;
    insert_copilot_message(&tx, &session_id, "assistant", assistant_message, &now)?;
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar transaccion Copilot: {error}"))?;
    Ok(session_id)
}

fn upsert_copilot_session(
    conn: &Connection,
    library_id: &str,
    session_id: Option<&str>,
    user_message: &str,
    interpreted: &PlaylistCopilotInterpretation,
    now: &str,
) -> Result<String, String> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_library_id = conn
        .query_row(
            "SELECT library_id FROM playlist_copilot_sessions WHERE id = ?1",
            params![&session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer sesion Copilot: {error}"))?;
    if let Some(existing_library_id) = existing_library_id.as_deref() {
        if existing_library_id != library_id {
            return Err("La sesion Copilot pertenece a otra libreria.".to_string());
        }
    }
    let intent_json = serde_json::to_string(interpreted)
        .map_err(|error| format!("No se pudo serializar intent Copilot: {error}"))?;
    if existing_library_id.is_some() {
        conn.execute(
            "UPDATE playlist_copilot_sessions
             SET intent_json = ?2, updated_at = ?3
             WHERE id = ?1",
            params![&session_id, &intent_json, now],
        )
        .map_err(|error| format!("No se pudo actualizar sesion Copilot: {error}"))?;
    } else {
        conn.execute(
            "INSERT INTO playlist_copilot_sessions (
                id, library_id, title, intent_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                &session_id,
                library_id,
                copilot_session_title(user_message),
                &intent_json,
                now
            ],
        )
        .map_err(|error| format!("No se pudo crear sesion Copilot: {error}"))?;
    }
    Ok(session_id)
}

fn load_previous_copilot_intent(
    conn: &Connection,
    library_id: &str,
    session_id: Option<&str>,
) -> Result<Option<PlaylistCopilotInterpretation>, String> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let session = conn
        .query_row(
            "SELECT library_id, intent_json FROM playlist_copilot_sessions WHERE id = ?1",
            params![session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer intent de sesion Copilot: {error}"))?
        .ok_or_else(|| "Sesion Copilot no encontrada.".to_string())?;
    if session.0 != library_id {
        return Err("La sesion Copilot pertenece a otra libreria.".to_string());
    }
    if !session.1.trim().is_empty() && session.1.trim() != "{}" {
        return serde_json::from_str(&session.1)
            .map(Some)
            .map_err(|error| format!("Intent Copilot persistido invalido: {error}"));
    }

    let previous_json = conn
        .query_row(
            "SELECT interpretation_json
             FROM playlist_copilot_candidate_sets
             WHERE session_id = ?1
             ORDER BY created_at DESC, rowid DESC
             LIMIT 1",
            params![session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo recuperar intent Copilot anterior: {error}"))?;
    previous_json
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| format!("Intent Copilot anterior invalido: {error}"))
        })
        .transpose()
}

fn insert_copilot_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO playlist_copilot_messages (id, session_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![Uuid::new_v4().to_string(), session_id, role, content, now],
    )
    .map_err(|error| format!("No se pudo guardar mensaje Copilot: {error}"))?;
    Ok(())
}

fn copilot_session_title(prompt: &str) -> String {
    let compact = prompt
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        "Copilot Session".to_string()
    } else {
        format!("Copilot - {}", title_case(&compact))
    }
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    user_message: &str,
    profile: &PlaylistCopilotProfile,
    target_count: usize,
    previous_intent: Option<&PlaylistCopilotInterpretation>,
) -> Result<PlaylistCopilotInterpretation, String> {
    let system_prompt = [
        "You are a DJ playlist planning assistant.",
        "Return only JSON. Do not include markdown.",
        "Update the previous intent from the user's new message and the local library profile.",
        "Preserve prior decisions unless the new message explicitly changes them.",
        "Use canonical values from the library profile whenever possible.",
        "Use this JSON shape:",
        r#"{"genres":[],"artists":[],"keys":[],"bpm_min":null,"bpm_max":null,"mood":null,"energy":null,"exclude_terms":[],"target_count":30,"energy_curve":"flat","harmonic_policy":"soft","discovery_mode":"balanced","tempo_policy":"flexible","source_policy":"prefer_available","focus_policy":"balanced","max_tracks_per_artist":3}"#,
        "Keep arrays short and use values that can match the library profile when possible.",
    ]
    .join(" ");
    let previous_intent = previous_intent
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("No se pudo serializar intent Copilot: {error}"))?
        .unwrap_or_else(|| "null".to_string());
    let user_prompt = format!(
        "Library profile:\n{}\n\nPrevious intent: {}\nTarget track count: {}\nNew user message: {}",
        playlist_copilot_profile_summary(profile),
        previous_intent,
        target_count,
        user_message
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
    previous_intent: Option<&PlaylistCopilotInterpretation>,
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

    let mut intent = previous_intent.cloned().unwrap_or_default();
    let genres = profile_matches(&normalized_prompt, &profile.genres, 6);
    let artists = profile_matches(&normalized_prompt, &profile.artists, 8);
    let keys = profile_matches(&normalized_prompt, &profile.keys, 6);
    if !genres.is_empty() {
        intent.genres = genres;
    }
    if !artists.is_empty() {
        intent.artists = artists;
    }
    if !keys.is_empty() {
        intent.keys = keys;
    }
    if bpm_min.is_some() || bpm_max.is_some() {
        intent.bpm_min = bpm_min;
        intent.bpm_max = bpm_max;
    }
    if let Some(mood) = prompt_mood(&normalized_prompt) {
        intent.mood = Some(mood);
    }
    if let Some(energy) = prompt_energy(&normalized_prompt) {
        intent.energy = Some(energy);
    }
    intent.exclude_terms.extend(prompt_exclude_terms(prompt));
    if normalized_prompt.contains("subida")
        || normalized_prompt.contains("energy ramp")
        || normalized_prompt.contains("mas energia")
        || normalized_prompt.contains("sube la energia")
        || normalized_prompt.contains("push the energy")
    {
        intent.energy_curve = EnergyCurve::Ramp;
        intent.energy = Some("peak".to_string());
    } else if normalized_prompt.contains("construccion lenta")
        || normalized_prompt.contains("slow build")
    {
        intent.energy_curve = EnergyCurve::SlowBuild;
    } else if normalized_prompt.contains("menos energia")
        || normalized_prompt.contains("baja la energia")
        || normalized_prompt.contains("reduce the energy")
    {
        intent.energy_curve = EnergyCurve::Flat;
        intent.energy = Some("warmup".to_string());
    }
    if normalized_prompt.contains("key estricta")
        || normalized_prompt.contains("strict key")
        || normalized_prompt.contains("strict harmonic")
    {
        intent.harmonic_policy = HarmonicPolicy::Strict;
    } else if normalized_prompt.contains("ignora key") || normalized_prompt.contains("ignore key") {
        intent.harmonic_policy = HarmonicPolicy::Ignore;
    }
    if normalized_prompt.contains("descubrimiento")
        || normalized_prompt.contains("discovery mode")
        || normalized_prompt.contains("mas variedad")
        || normalized_prompt.contains("no repitas")
        || normalized_prompt.contains("menos obvio")
        || normalized_prompt.contains("otros generos")
        || normalized_prompt.contains("sorprendeme")
    {
        intent.discovery_mode = DiscoveryMode::Discovery;
    } else if normalized_prompt.contains("mas conocidos")
        || normalized_prompt.contains("known artists")
    {
        intent.discovery_mode = DiscoveryMode::Known;
    }
    if normalized_prompt.contains("rango bpm cerrado") || normalized_prompt.contains("tight bpm") {
        intent.tempo_policy = TempoPolicy::Tight;
    }
    if normalized_prompt.contains("sin archivos faltantes")
        || normalized_prompt.contains("avoid missing files")
        || normalized_prompt.contains("available files only")
    {
        intent.source_policy = SourcePolicy::AvailableOnly;
    }
    intent.target_count = Some(target_count);
    normalize_copilot_interpretation(intent)
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
    interpretation.max_tracks_per_artist = interpretation.max_tracks_per_artist.clamp(1, 10);
    if interpretation.harmonic_policy == HarmonicPolicy::Ignore {
        interpretation.keys.clear();
    }
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
        if matches!(
            marker.as_str(),
            "sin" | "no" | "quita" | "evita" | "without" | "exclude" | "avoid" | "remove"
        ) {
            excludes.push(pair[1].to_string());
        }
    }
    clean_copilot_terms(excludes, 8)
}

fn playlist_copilot_semantic_query(
    user_message: &str,
    intent: &PlaylistCopilotInterpretation,
) -> String {
    let bpm = match (intent.bpm_min, intent.bpm_max) {
        (Some(min), Some(max)) => format!("{min:.0}-{max:.0} BPM"),
        (Some(min), None) => format!("at least {min:.0} BPM"),
        (None, Some(max)) => format!("up to {max:.0} BPM"),
        (None, None) => String::new(),
    };
    [
        Some(format!("DJ playlist request: {user_message}")),
        (!intent.genres.is_empty()).then(|| format!("genres: {}", intent.genres.join(", "))),
        (!intent.artists.is_empty()).then(|| format!("artists: {}", intent.artists.join(", "))),
        (!intent.keys.is_empty()).then(|| format!("keys: {}", intent.keys.join(", "))),
        (!bpm.is_empty()).then(|| format!("tempo: {bpm}")),
        intent.mood.as_ref().map(|value| format!("mood: {value}")),
        intent
            .energy
            .as_ref()
            .map(|value| format!("energy: {value}")),
        (!intent.exclude_terms.is_empty())
            .then(|| format!("exclude: {}", intent.exclude_terms.join(", "))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
}

fn playlist_copilot_search_probes(
    user_message: &str,
    intent: &PlaylistCopilotInterpretation,
    english: bool,
) -> Vec<PlaylistCopilotSearchProbe> {
    let mut probes = Vec::new();
    push_copilot_probe(
        &mut probes,
        "brief",
        if english {
            "Complete brief"
        } else {
            "Brief completo"
        },
        playlist_copilot_semantic_query(user_message, intent),
        1.35,
    );

    if !intent.genres.is_empty() || !intent.artists.is_empty() {
        push_copilot_probe(
            &mut probes,
            "style",
            if english {
                "Style and references"
            } else {
                "Estilo y referencias"
            },
            format!(
                "DJ tracks. Genres and subgenres: {}. Artist references: {}.",
                intent.genres.join(", "),
                intent.artists.join(", ")
            ),
            1.15,
        );
    }

    if intent.mood.is_some() || intent.energy.is_some() {
        push_copilot_probe(
            &mut probes,
            "feel",
            if english {
                "Mood and energy"
            } else {
                "Mood y energia"
            },
            format!(
                "DJ tracks with {} mood and {} energy, suitable for a {:?} energy curve.",
                intent.mood.as_deref().unwrap_or("compatible"),
                intent.energy.as_deref().unwrap_or("balanced"),
                intent.energy_curve
            ),
            1.0,
        );
    }

    if intent.bpm_min.is_some() || intent.bpm_max.is_some() || !intent.keys.is_empty() {
        push_copilot_probe(
            &mut probes,
            "mix",
            if english {
                "Tempo and harmonic fit"
            } else {
                "Tempo y mezcla armonica"
            },
            format!(
                "DJ mixing candidates around {} with compatible musical keys {}.",
                copilot_bpm_value(intent.bpm_min, intent.bpm_max),
                intent.keys.join(", ")
            ),
            0.9,
        );
    }

    push_copilot_probe(
        &mut probes,
        "adjacent",
        if english {
            "Adjacent discoveries"
        } else {
            "Descubrimientos adyacentes"
        },
        format!(
            "Less obvious deep cuts and adjacent subgenres compatible with this DJ brief: {}. Keep the same mood, energy and mixability but avoid repeating the obvious anchors.",
            user_message
        ),
        match intent.discovery_mode {
            DiscoveryMode::Known => 0.55,
            DiscoveryMode::Balanced => 1.0,
            DiscoveryMode::Discovery => 1.4,
        },
    );
    probes.truncate(5);
    probes
}

fn push_copilot_probe(
    probes: &mut Vec<PlaylistCopilotSearchProbe>,
    id: &str,
    label: &str,
    query: String,
    weight: f64,
) {
    let query = query.trim().to_string();
    if query.is_empty() {
        return;
    }
    let normalized = normalize_for_match(&query);
    if probes
        .iter()
        .any(|probe| normalize_for_match(&probe.query) == normalized)
    {
        return;
    }
    probes.push(PlaylistCopilotSearchProbe {
        id: id.to_string(),
        label: label.to_string(),
        query,
        weight,
    });
}

fn playlist_copilot_semantic_evidence(
    app: &AppHandle,
    conn: &Connection,
    library_id: &str,
    api_key: &str,
    probes: &[PlaylistCopilotSearchProbe],
    request_id: &str,
    english: bool,
) -> Result<
    (
        HashMap<String, CopilotSemanticEvidence>,
        Vec<PlaylistCopilotSearchTrace>,
    ),
    String,
> {
    let inputs = probes
        .iter()
        .map(|probe| probe.query.clone())
        .collect::<Vec<_>>();
    let embeddings = request_embeddings(api_key, &inputs)?;
    let tracks = load_embedded_tracks(conn, Some(library_id))?;
    let mut evidence = HashMap::<String, CopilotSemanticEvidence>::new();
    let mut traces = Vec::new();

    for (probe, query_embedding) in probes.iter().zip(embeddings.iter()) {
        let mut ranked = tracks
            .iter()
            .map(|(track, _, embedding)| {
                (
                    track.track_id.clone(),
                    cosine_similarity(query_embedding, embedding),
                )
            })
            .filter(|(_, score)| score.is_finite())
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
        ranked.truncate(COPILOT_PROBE_RESULT_LIMIT);
        merge_copilot_probe_evidence(&mut evidence, probe, &ranked);
        let top_similarity = ranked.first().map(|(_, score)| round_similarity(*score));
        let detail = if english {
            format!(
                "Vector probe retained {} candidates; top similarity {}.",
                ranked.len(),
                top_similarity
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "n/a".to_string())
            )
        } else {
            format!(
                "El probe vectorial retuvo {} candidatos; similitud maxima {}.",
                ranked.len(),
                top_similarity
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "n/d".to_string())
            )
        };
        emit_copilot_progress(
            app,
            request_id,
            &format!("search-{}", probe.id),
            "done",
            &probe.label,
            Some(detail.clone()),
        );
        traces.push(PlaylistCopilotSearchTrace {
            id: probe.id.clone(),
            label: probe.label.clone(),
            candidate_count: ranked.len(),
            top_similarity,
            detail,
        });
    }

    Ok((evidence, traces))
}

fn playlist_copilot_local_evidence(
    app: &AppHandle,
    tracks: &[PlaylistIndexTrack],
    probes: &[PlaylistCopilotSearchProbe],
    request_id: &str,
    english: bool,
) -> (
    HashMap<String, CopilotSemanticEvidence>,
    Vec<PlaylistCopilotSearchTrace>,
) {
    let mut evidence = HashMap::<String, CopilotSemanticEvidence>::new();
    let mut traces = Vec::new();
    for probe in probes {
        let terms = copilot_probe_terms(&probe.query);
        let mut ranked = tracks
            .iter()
            .filter_map(|track| {
                let text = normalize_for_match(&track.search_text);
                let matched = terms
                    .iter()
                    .filter(|term| normalized_contains_phrase(&text, term))
                    .count();
                (matched > 0).then(|| (track.track_id.clone(), matched as f64))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        ranked.truncate(COPILOT_PROBE_RESULT_LIMIT);
        merge_copilot_probe_evidence(&mut evidence, probe, &ranked);
        let detail = if english {
            format!("Local metadata probe found {} candidates.", ranked.len())
        } else {
            format!(
                "El probe de metadata local encontro {} candidatos.",
                ranked.len()
            )
        };
        emit_copilot_progress(
            app,
            request_id,
            &format!("search-{}", probe.id),
            "done",
            &probe.label,
            Some(detail.clone()),
        );
        traces.push(PlaylistCopilotSearchTrace {
            id: probe.id.clone(),
            label: probe.label.clone(),
            candidate_count: ranked.len(),
            top_similarity: None,
            detail,
        });
    }
    (evidence, traces)
}

fn merge_copilot_probe_evidence(
    evidence: &mut HashMap<String, CopilotSemanticEvidence>,
    probe: &PlaylistCopilotSearchProbe,
    ranked: &[(String, f64)],
) {
    for (rank, (track_id, _)) in ranked.iter().enumerate() {
        let item = evidence.entry(track_id.clone()).or_default();
        item.score += probe.weight / (60.0 + rank as f64 + 1.0);
        if !item.probes.iter().any(|label| label == &probe.label) {
            item.probes.push(probe.label.clone());
        }
    }
}

fn copilot_probe_terms(query: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "and",
        "con",
        "para",
        "this",
        "that",
        "the",
        "tracks",
        "playlist",
        "brief",
        "candidates",
        "compatible",
        "same",
        "keep",
        "around",
        "pero",
        "mantener",
        "bpm",
    ];
    normalize_for_match(query)
        .split_whitespace()
        .filter(|term| term.len() >= 3 && !STOPWORDS.contains(term))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(32)
        .collect()
}

fn round_similarity(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn playlist_copilot_exploration_seed(
    conn: &Connection,
    library_id: &str,
    session_id: Option<&str>,
    user_message: &str,
) -> Result<u64, String> {
    let run_index = conn
        .query_row(
            "SELECT COUNT(*)
             FROM playlist_copilot_candidate_sets sets
             JOIN playlist_copilot_sessions sessions ON sessions.id = sets.session_id
             WHERE sessions.library_id = ?1",
            params![library_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("No se pudo calcular rotacion Copilot: {error}"))?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    library_id.hash(&mut hasher);
    session_id.unwrap_or_default().hash(&mut hasher);
    normalize_for_match(user_message).hash(&mut hasher);
    run_index.hash(&mut hasher);
    Ok(hasher.finish())
}

fn recent_copilot_suggestion_counts(
    conn: &Connection,
    library_id: &str,
    candidate_set_limit: usize,
) -> Result<HashMap<String, usize>, String> {
    let mut stmt = conn
        .prepare(
            "WITH recent_sets AS (
                SELECT candidate_sets.id
                FROM playlist_copilot_candidate_sets candidate_sets
                JOIN playlist_copilot_sessions sessions
                  ON sessions.id = candidate_sets.session_id
                WHERE sessions.library_id = ?1
                ORDER BY candidate_sets.created_at DESC, candidate_sets.rowid DESC
                LIMIT ?2
             )
             SELECT tracks.track_id, COUNT(DISTINCT tracks.candidate_set_id)
             FROM playlist_copilot_candidate_tracks tracks
             JOIN recent_sets ON recent_sets.id = tracks.candidate_set_id
             GROUP BY tracks.track_id",
        )
        .map_err(|error| format!("No se pudo preparar historial reciente Copilot: {error}"))?;
    let rows = stmt
        .query_map(params![library_id, candidate_set_limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, i64_to_usize(row.get(1)?)))
        })
        .map_err(|error| format!("No se pudo consultar historial reciente Copilot: {error}"))?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| format!("No se pudo mapear historial reciente Copilot: {error}"))
}

fn rank_copilot_candidates(
    tracks: &[PlaylistIndexTrack],
    interpreted: &PlaylistCopilotInterpretation,
    prompt: &str,
    target_count: usize,
    semantic_evidence: &HashMap<String, CopilotSemanticEvidence>,
    recent_suggestion_counts: &HashMap<String, usize>,
    exploration_seed: u64,
) -> Vec<PlaylistCopilotCandidate> {
    let features = tracks
        .iter()
        .map(|track| TrackFeatures {
            track_id: track.track_id.clone(),
            title: track.name.clone().unwrap_or_default(),
            artist: track.artist.clone().unwrap_or_default(),
            genre: track.genre.clone().unwrap_or_default(),
            key: track.key.clone().unwrap_or_default(),
            bpm: track_bpm_value(track),
            duration_seconds: track.total_time,
            source_exists: track.source_exists,
            search_text: track.search_text.clone(),
            metadata_quality: [
                track.name.as_ref(),
                track.artist.as_ref(),
                track.album.as_ref(),
                track.genre.as_ref(),
                track.bpm.as_ref(),
                track.key.as_ref(),
            ]
            .into_iter()
            .filter(|value| value.is_some_and(|value| !value.trim().is_empty()))
            .count(),
            semantic_score: semantic_evidence
                .get(&track.track_id)
                .map(|item| item.score),
            semantic_probes: semantic_evidence
                .get(&track.track_id)
                .map(|item| item.probes.clone())
                .unwrap_or_default(),
            prior_suggestion_count: recent_suggestion_counts
                .get(&track.track_id)
                .copied()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let tracks_by_id = tracks
        .iter()
        .map(|track| (track.track_id.as_str(), track))
        .collect::<HashMap<_, _>>();

    rank_and_sequence_with_seed(
        &features,
        interpreted,
        prompt,
        target_count,
        exploration_seed,
    )
    .into_iter()
    .filter_map(|ranked| {
        tracks_by_id
            .get(ranked.track_id.as_str())
            .map(|track| PlaylistCopilotCandidate {
                track: (*track).clone(),
                score: ranked.score,
                reasons: ranked.reasons,
                score_components: ranked.components,
            })
    })
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
        .map(|character| match character {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            character if character.is_alphanumeric() => character,
            _ => ' ',
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
            let embedding_json: String = row.get(17)?;
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

    let max_items = limit.unwrap_or(500).clamp(1, 10000);
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

fn enrichment_overview(
    conn: &Connection,
    library_id: &str,
) -> Result<PlaylistEnrichmentOverview, String> {
    let library = get_library(conn, library_id)?
        .ok_or_else(|| format!("Libreria indexada no encontrada: {library_id}"))?;
    let tracks = load_enrichment_tracks(conn, library_id, None, usize::MAX)?;
    let mut missing_genre_count = 0_usize;
    let mut missing_year_count = 0_usize;
    let mut missing_label_count = 0_usize;
    let mut missing_comments_count = 0_usize;
    let mut missing_key_count = 0_usize;
    let mut missing_bpm_count = 0_usize;

    for track in &tracks {
        if taxonomy_value(track.genre.as_deref()).is_empty() {
            missing_genre_count += 1;
        }
        if taxonomy_value(track.year.as_deref()).is_empty() {
            missing_year_count += 1;
        }
        if taxonomy_value(track.label.as_deref()).is_empty() {
            missing_label_count += 1;
        }
        if taxonomy_value(track.comments.as_deref()).is_empty() {
            missing_comments_count += 1;
        }
        if taxonomy_value(track.key.as_deref()).is_empty() {
            missing_key_count += 1;
        }
        if track_bpm_value(track).is_none() {
            missing_bpm_count += 1;
        }
    }

    let enriched_track_count = conn
        .query_row(
            "SELECT COUNT(DISTINCT track_id)
             FROM playlist_track_enrichments
             WHERE library_id = ?1 AND status = 'matched'",
            params![library_id],
            |row| row.get::<_, i64>(0),
        )
        .map(i64_to_usize)
        .map_err(|error| format!("No se pudo contar enrichment: {error}"))?;
    let matched_result_count = enrichment_status_count(conn, library_id, "matched")?;
    let failed_result_count = enrichment_status_count(conn, library_id, "failed")?;
    let last_run_at = conn
        .query_row(
            "SELECT MAX(updated_at) FROM playlist_track_enrichments WHERE library_id = ?1",
            params![library_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer ultima corrida de enrichment: {error}"))?
        .flatten();

    Ok(PlaylistEnrichmentOverview {
        library,
        track_count: tracks.len(),
        missing_genre_count,
        missing_year_count,
        missing_label_count,
        missing_comments_count,
        missing_key_count,
        missing_bpm_count,
        enriched_track_count,
        matched_result_count,
        failed_result_count,
        last_run_at,
    })
}

fn enrichment_status_count(
    conn: &Connection,
    library_id: &str,
    status: &str,
) -> Result<usize, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM playlist_track_enrichments WHERE library_id = ?1 AND status = ?2",
        params![library_id, status],
        |row| row.get::<_, i64>(0),
    )
    .map(i64_to_usize)
    .map_err(|error| format!("No se pudo contar resultados de enrichment: {error}"))
}

fn load_enrichment_tracks(
    conn: &Connection,
    library_id: &str,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<PlaylistIndexTrack>, String> {
    let query_filter = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|_| "AND LOWER(t.search_text) LIKE ?2")
        .unwrap_or_default();
    let limit_clause = if limit == usize::MAX {
        "".to_string()
    } else if query_filter.is_empty() {
        "LIMIT ?2".to_string()
    } else {
        "LIMIT ?3".to_string()
    };
    let sql = format!(
        "SELECT {}
         FROM playlist_index_tracks t
         WHERE t.library_id = ?1
         {query_filter}
         ORDER BY COALESCE(t.artist, ''), COALESCE(t.name, ''), t.track_id
         {limit_clause}",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar tracks para enrichment: {error}"))?;

    let rows = if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = like_pattern(query);
        if limit == usize::MAX {
            stmt.query_map(params![library_id, pattern], row_to_track)
                .map_err(|error| format!("No se pudieron leer tracks para enrichment: {error}"))?
        } else {
            stmt.query_map(params![library_id, pattern, limit as i64], row_to_track)
                .map_err(|error| format!("No se pudieron leer tracks para enrichment: {error}"))?
        }
    } else if limit == usize::MAX {
        stmt.query_map(params![library_id], row_to_track)
            .map_err(|error| format!("No se pudieron leer tracks para enrichment: {error}"))?
    } else {
        stmt.query_map(params![library_id, limit as i64], row_to_track)
            .map_err(|error| format!("No se pudieron leer tracks para enrichment: {error}"))?
    };

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear tracks para enrichment: {error}"))
}

fn enrichment_gap_matches(track: &PlaylistIndexTrack, gap: &str) -> bool {
    match gap {
        "all" => true,
        "missing_genre" => taxonomy_value(track.genre.as_deref()).is_empty(),
        "missing_year" => taxonomy_value(track.year.as_deref()).is_empty(),
        "missing_label" => taxonomy_value(track.label.as_deref()).is_empty(),
        "missing_comments" => taxonomy_value(track.comments.as_deref()).is_empty(),
        "missing_key" => taxonomy_value(track.key.as_deref()).is_empty(),
        "missing_bpm" => track_bpm_value(track).is_none(),
        "missing_metadata" | _ => {
            taxonomy_value(track.genre.as_deref()).is_empty()
                || taxonomy_value(track.year.as_deref()).is_empty()
                || taxonomy_value(track.label.as_deref()).is_empty()
                || taxonomy_value(track.comments.as_deref()).is_empty()
        }
    }
}

fn list_enrichment_results(
    conn: &Connection,
    library_id: &str,
    provider: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<PlaylistEnrichmentItem>, String> {
    let sql = format!(
        "SELECT e.id, e.library_id, e.track_id, e.provider, e.provider_key, e.status,
                e.confidence, e.fields_json, e.message, e.source_url, e.updated_at, e.applied_at,
                {}
         FROM playlist_track_enrichments e
         JOIN playlist_index_tracks t
           ON t.library_id = e.library_id AND t.track_id = e.track_id
         WHERE e.library_id = ?1
         ORDER BY e.updated_at DESC
         LIMIT ?2",
        track_select_clause()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| format!("No se pudo preparar resultados de enrichment: {error}"))?;
    let rows = stmt
        .query_map(params![library_id, limit as i64], row_to_enrichment_item)
        .map_err(|error| format!("No se pudieron leer resultados de enrichment: {error}"))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear resultados de enrichment: {error}"))?;

    if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        items.retain(|item| item.provider == provider);
    }
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        items.retain(|item| item.status == status);
    }
    Ok(items)
}

fn row_to_enrichment_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistEnrichmentItem> {
    let fields_json: String = row.get(7)?;
    let fields = serde_json::from_str::<BTreeMap<String, String>>(&fields_json).unwrap_or_default();

    Ok(PlaylistEnrichmentItem {
        id: row.get(0)?,
        library_id: row.get(1)?,
        track_id: row.get(2)?,
        provider: row.get(3)?,
        provider_key: row.get(4)?,
        status: row.get(5)?,
        confidence: row.get(6)?,
        fields,
        message: row.get(8)?,
        source_url: row.get(9)?,
        updated_at: row.get(10)?,
        applied_at: row.get(11)?,
        track: row_to_track_at(row, 12)?,
    })
}

fn run_enrichment_blocking(
    app: AppHandle,
    library_id: String,
    providers: Vec<String>,
    limit: Option<usize>,
    track_ids: Option<Vec<String>>,
) -> Result<PlaylistEnrichmentRunResult, String> {
    let conn = open_db(&app)?;
    if get_library(&conn, &library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }

    let providers = enrichment::normalize_provider_ids(providers)?;
    let provider_clients = enrichment::load_provider_clients(&app, &providers)?;
    let selected_track_ids = track_ids
        .unwrap_or_default()
        .into_iter()
        .map(|track_id| track_id.trim().to_string())
        .filter(|track_id| !track_id.is_empty())
        .collect::<BTreeSet<_>>();
    let max_items = limit.unwrap_or(100).clamp(1, 1000);
    let mut tracks = load_enrichment_tracks(&conn, &library_id, None, max_items * 5)?;
    if selected_track_ids.is_empty() {
        tracks.retain(|track| enrichment_gap_matches(track, "missing_metadata"));
        tracks.truncate(max_items);
    } else {
        tracks.retain(|track| selected_track_ids.contains(&track.track_id));
    }

    let force_selected = !selected_track_ids.is_empty();
    let total_work = tracks
        .iter()
        .map(enrichment_track_input)
        .map(|track| {
            enrichment::planned_provider_ids(&track, &provider_clients, force_selected).len()
        })
        .sum::<usize>();
    let run_id = create_enrichment_run(
        &conn,
        &library_id,
        &providers,
        &provider_clients,
        total_work,
    )?;
    if total_work == 0 {
        emit_enrichment_progress(
            &app,
            &library_id,
            None,
            None,
            None,
            "info",
            "No hay tracks para enriquecer.",
            0,
            0,
        );
        update_enrichment_run_progress(&conn, &run_id, 0, 0, 0, 0, true)?;
        return Ok(PlaylistEnrichmentRunResult {
            run_id,
            library_id,
            processed_total: 0,
            matched_total: 0,
            no_match_total: 0,
            failed_total: 0,
            providers,
        });
    }

    emit_enrichment_progress(
        &app,
        &library_id,
        None,
        None,
        None,
        "info",
        &format!("Iniciando enrichment para {} track(s).", tracks.len()),
        0,
        total_work,
    );

    let mut processed_total = 0_usize;
    let mut matched_total = 0_usize;
    let mut no_match_total = 0_usize;
    let mut failed_total = 0_usize;

    let mut last_provider_calls = HashMap::<String, Instant>::new();
    for track in &tracks {
        let mut enrichment_track = enrichment_track_input(track);
        let planned_providers =
            enrichment::planned_provider_ids(&enrichment_track, &provider_clients, force_selected)
                .into_iter()
                .collect::<BTreeSet<_>>();
        for provider_client in &provider_clients {
            let provider = provider_client.id();
            if !planned_providers.contains(provider) {
                continue;
            }
            emit_enrichment_progress(
                &app,
                &library_id,
                Some(track.track_id.clone()),
                Some(provider.to_string()),
                Some("running".to_string()),
                "info",
                &format!(
                    "Consultando {provider}: {}",
                    track.name.as_deref().unwrap_or(&track.track_id)
                ),
                processed_total,
                total_work,
            );

            let task_id =
                create_enrichment_task(&conn, &run_id, &library_id, &track.track_id, provider)?;
            wait_for_provider_rate_limit(provider_client, &last_provider_calls);
            let suggestion = provider_client.enrich(&enrichment_track);
            last_provider_calls.insert(provider.to_string(), Instant::now());

            if suggestion.status == "matched" {
                for key in ["musicbrainz_recording_id", "musicbrainz_release_id", "isrc"] {
                    if let Some(value) = suggestion.fields.get(key) {
                        enrichment_track
                            .external_ids
                            .insert(key.to_string(), value.clone());
                    }
                }
            }

            match suggestion.status.as_str() {
                "matched" => matched_total += 1,
                "failed" => failed_total += 1,
                _ => no_match_total += 1,
            }

            upsert_enrichment_result(&conn, &library_id, &track.track_id, &suggestion)?;
            finish_enrichment_task(
                &conn,
                &task_id,
                &run_id,
                &library_id,
                &track.track_id,
                &suggestion,
            )?;
            processed_total += 1;
            update_enrichment_run_progress(
                &conn,
                &run_id,
                processed_total,
                matched_total,
                no_match_total,
                failed_total,
                false,
            )?;

            emit_enrichment_progress(
                &app,
                &library_id,
                Some(track.track_id.clone()),
                Some(provider.to_string()),
                Some(suggestion.status.clone()),
                if suggestion.status == "failed" {
                    "error"
                } else {
                    "info"
                },
                suggestion
                    .message
                    .as_deref()
                    .unwrap_or("Resultado de enrichment guardado."),
                processed_total,
                total_work,
            );
        }
    }

    emit_enrichment_progress(
        &app,
        &library_id,
        None,
        None,
        Some("done".to_string()),
        "info",
        "Enrichment terminado.",
        processed_total,
        total_work,
    );
    update_enrichment_run_progress(
        &conn,
        &run_id,
        processed_total,
        matched_total,
        no_match_total,
        failed_total,
        true,
    )?;

    Ok(PlaylistEnrichmentRunResult {
        run_id,
        library_id,
        processed_total,
        matched_total,
        no_match_total,
        failed_total,
        providers,
    })
}

fn wait_for_provider_rate_limit(
    provider: &enrichment::ProviderClient,
    last_calls: &HashMap<String, Instant>,
) {
    let Some(last_call) = last_calls.get(provider.id()) else {
        return;
    };
    let minimum = Duration::from_millis(provider.definition().min_interval_ms);
    let elapsed = last_call.elapsed();
    if elapsed < minimum {
        thread::sleep(minimum - elapsed);
    }
}

fn enrichment_track_input(track: &PlaylistIndexTrack) -> enrichment::EnrichmentTrack {
    let mut external_ids = BTreeMap::new();
    for (field, aliases) in [
        (
            "musicbrainz_recording_id",
            &["MusicBrainzRecordingID", "MusicBrainz Track Id"][..],
        ),
        ("musicbrainz_release_id", &["MusicBrainzReleaseID"][..]),
        ("isrc", &["ISRC", "Isrc"][..]),
    ] {
        if let Some(value) = attribute_value(&track.attributes, aliases) {
            external_ids.insert(field.to_string(), value);
        }
    }
    enrichment::EnrichmentTrack {
        track_id: track.track_id.clone(),
        title: track.name.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        total_time: track.total_time,
        genre: track.genre.clone(),
        comments: track.comments.clone(),
        bpm: track.bpm.clone(),
        key: track.key.clone(),
        year: track.year.clone(),
        label: track.label.clone(),
        external_ids,
    }
}

fn create_enrichment_run(
    conn: &Connection,
    library_id: &str,
    providers: &[String],
    provider_clients: &[enrichment::ProviderClient],
    total_work: usize,
) -> Result<String, String> {
    let run_id = Uuid::new_v4().to_string();
    let now = timestamp();
    let providers_json = serde_json::to_string(providers)
        .map_err(|error| format!("No se pudieron serializar proveedores: {error}"))?;
    let requested_fields = provider_clients
        .iter()
        .flat_map(|provider| provider.definition().capabilities.iter().copied())
        .collect::<BTreeSet<_>>();
    let requested_fields_json = serde_json::to_string(&requested_fields)
        .map_err(|error| format!("No se pudieron serializar capabilities: {error}"))?;
    conn.execute(
        "INSERT INTO playlist_enrichment_runs (
            id, library_id, status, providers_json, requested_fields_json,
            total_work, created_at, started_at
         ) VALUES (?1, ?2, 'running', ?3, ?4, ?5, ?6, ?6)",
        params![
            &run_id,
            library_id,
            &providers_json,
            &requested_fields_json,
            total_work as i64,
            &now,
        ],
    )
    .map_err(|error| format!("No se pudo crear corrida de enrichment: {error}"))?;
    Ok(run_id)
}

fn create_enrichment_task(
    conn: &Connection,
    run_id: &str,
    library_id: &str,
    track_id: &str,
    provider: &str,
) -> Result<String, String> {
    let task_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO playlist_enrichment_tasks (
            id, run_id, library_id, track_id, provider, status, started_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
        params![
            &task_id,
            run_id,
            library_id,
            track_id,
            provider,
            timestamp(),
        ],
    )
    .map_err(|error| format!("No se pudo crear tarea de enrichment: {error}"))?;
    Ok(task_id)
}

fn finish_enrichment_task(
    conn: &Connection,
    task_id: &str,
    run_id: &str,
    library_id: &str,
    track_id: &str,
    suggestion: &enrichment::ProviderSuggestion,
) -> Result<(), String> {
    let now = timestamp();
    let error_kind = suggestion.payload.get("error_kind").and_then(Value::as_str);
    conn.execute(
        "UPDATE playlist_enrichment_tasks
         SET status = ?2, error_kind = ?3, message = ?4, finished_at = ?5
         WHERE id = ?1",
        params![
            task_id,
            &suggestion.status,
            error_kind,
            &suggestion.message,
            &now,
        ],
    )
    .map_err(|error| format!("No se pudo finalizar tarea de enrichment: {error}"))?;

    if suggestion.status == "matched" {
        for (field, value) in &suggestion.fields {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO playlist_enrichment_observations (
                    id, task_id, run_id, library_id, track_id, provider, field, value,
                    confidence, provider_key, source_url, observed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    Uuid::new_v4().to_string(),
                    task_id,
                    run_id,
                    library_id,
                    track_id,
                    &suggestion.provider,
                    field,
                    value,
                    suggestion.confidence,
                    &suggestion.provider_key,
                    &suggestion.source_url,
                    &now,
                ],
            )
            .map_err(|error| format!("No se pudo guardar observacion de enrichment: {error}"))?;
        }
    }
    Ok(())
}

fn update_enrichment_run_progress(
    conn: &Connection,
    run_id: &str,
    processed_total: usize,
    matched_total: usize,
    no_match_total: usize,
    failed_total: usize,
    finished: bool,
) -> Result<(), String> {
    let status = if !finished {
        "running"
    } else if failed_total > 0 {
        "partial"
    } else {
        "completed"
    };
    let completed_at = finished.then(timestamp);
    conn.execute(
        "UPDATE playlist_enrichment_runs
         SET status = ?2, processed_total = ?3, matched_total = ?4,
             no_match_total = ?5, failed_total = ?6, completed_at = ?7
         WHERE id = ?1",
        params![
            run_id,
            status,
            processed_total as i64,
            matched_total as i64,
            no_match_total as i64,
            failed_total as i64,
            completed_at,
        ],
    )
    .map_err(|error| format!("No se pudo actualizar corrida de enrichment: {error}"))?;
    Ok(())
}

fn upsert_enrichment_result(
    conn: &Connection,
    library_id: &str,
    track_id: &str,
    suggestion: &enrichment::ProviderSuggestion,
) -> Result<(), String> {
    let now = timestamp();
    let fields_json = serde_json::to_string(&suggestion.fields)
        .map_err(|error| format!("No se pudo serializar campos de enrichment: {error}"))?;
    let payload_json = serde_json::to_string(&suggestion.payload)
        .map_err(|error| format!("No se pudo serializar payload de enrichment: {error}"))?;

    conn.execute(
        "INSERT INTO playlist_track_enrichments (
            id, library_id, track_id, provider, provider_key, status, confidence,
            fields_json, payload_json, message, source_url, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
         ON CONFLICT(library_id, track_id, provider) DO UPDATE SET
            provider_key = excluded.provider_key,
            status = excluded.status,
            confidence = excluded.confidence,
            fields_json = excluded.fields_json,
            payload_json = excluded.payload_json,
            message = excluded.message,
            source_url = excluded.source_url,
            updated_at = excluded.updated_at,
            applied_at = NULL",
        params![
            Uuid::new_v4().to_string(),
            library_id,
            track_id,
            &suggestion.provider,
            &suggestion.provider_key,
            &suggestion.status,
            suggestion.confidence,
            &fields_json,
            &payload_json,
            &suggestion.message,
            &suggestion.source_url,
            &now
        ],
    )
    .map_err(|error| format!("No se pudo guardar enrichment de track {track_id}: {error}"))?;

    Ok(())
}

fn apply_enrichment_results(
    conn: &mut Connection,
    library_id: &str,
    result_ids: Vec<String>,
) -> Result<PlaylistEnrichmentApplyResult, String> {
    if get_library(conn, library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }
    let ids = result_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Err("Selecciona al menos un resultado para aplicar.".to_string());
    }

    let now = timestamp();
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transaccion de enrichment: {error}"))?;
    let mut applied_total = 0_usize;
    let mut skipped_total = 0_usize;
    let mut by_track =
        BTreeMap::<String, Vec<(String, String, f64, BTreeMap<String, String>)>>::new();

    for result_id in ids {
        let result = tx
            .query_row(
                "SELECT e.id, e.provider, e.fields_json, e.status, e.track_id, e.confidence
                 FROM playlist_track_enrichments e
                 WHERE e.library_id = ?1 AND e.id = ?2",
                params![library_id, &result_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, f64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("No se pudo leer resultado de enrichment: {error}"))?;
        let Some((id, provider, fields_json, status, track_id, confidence)) = result else {
            skipped_total += 1;
            continue;
        };
        if status != "matched" {
            skipped_total += 1;
            continue;
        }

        let fields =
            serde_json::from_str::<BTreeMap<String, String>>(&fields_json).unwrap_or_default();
        by_track
            .entry(track_id)
            .or_default()
            .push((id, provider, confidence, fields));
    }

    let mut metadata_changed = false;
    for (track_id, results) in by_track {
        let track = get_index_track(&tx, library_id, &track_id)?
            .ok_or_else(|| format!("Track indexado no encontrado: {track_id}"))?;
        let mut attributes = track.attributes.clone();
        let mut changed = false;
        for (_, provider, _, fields) in &results {
            changed |= merge_enrichment_provenance(&mut attributes, provider, fields);
        }
        let resolution_inputs = results
            .iter()
            .map(
                |(_, provider, confidence, fields)| enrichment::ResolutionInput {
                    provider: provider.clone(),
                    confidence: *confidence,
                    fields: fields.clone(),
                },
            )
            .collect::<Vec<_>>();
        changed |= apply_resolved_enrichment_fields(
            &mut attributes,
            &enrichment::resolve_fields(&resolution_inputs),
        );

        if changed {
            let playlist_paths = indexed_playlist_paths_for_track(&tx, library_id, &track_id)?;
            let search_text = indexed_track_search_text(&track, &attributes, &playlist_paths);
            let attributes_json = serde_json::to_string(&attributes)
                .map_err(|error| format!("No se pudo serializar attributes_json: {error}"))?;
            tx.execute(
                "UPDATE playlist_index_tracks
                 SET attributes_json = ?3,
                     search_text = ?4,
                     updated_at = ?5
                 WHERE library_id = ?1 AND track_id = ?2",
                params![library_id, &track_id, &attributes_json, &search_text, &now],
            )
            .map_err(|error| format!("No se pudo aplicar enrichment en {track_id}: {error}"))?;
            metadata_changed = true;
        }

        for (id, _, _, _) in &results {
            tx.execute(
                "UPDATE playlist_track_enrichments
                 SET applied_at = ?3, updated_at = ?3
                 WHERE library_id = ?1 AND id = ?2",
                params![library_id, id, &now],
            )
            .map_err(|error| format!("No se pudo marcar enrichment aplicado: {error}"))?;
        }
        applied_total += results.len();
    }

    tx.execute(
        "UPDATE playlist_index_libraries SET updated_at = ?2 WHERE id = ?1",
        params![library_id, &now],
    )
    .map_err(|error| format!("No se pudo actualizar libreria: {error}"))?;
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar enrichment aplicado: {error}"))?;
    if metadata_changed {
        rebuild_fts(conn)?;
    }

    Ok(PlaylistEnrichmentApplyResult {
        library_id: library_id.to_string(),
        applied_total,
        skipped_total,
    })
}

fn clear_enrichment_results(
    conn: &Connection,
    library_id: &str,
    track_ids: Option<Vec<String>>,
) -> Result<usize, String> {
    if get_library(conn, library_id)?.is_none() {
        return Err(format!("Libreria indexada no encontrada: {library_id}"));
    }
    let track_ids = track_ids
        .unwrap_or_default()
        .into_iter()
        .map(|track_id| track_id.trim().to_string())
        .filter(|track_id| !track_id.is_empty())
        .collect::<BTreeSet<_>>();
    if track_ids.is_empty() {
        let deleted = conn
            .execute(
                "DELETE FROM playlist_track_enrichments WHERE library_id = ?1",
                params![library_id],
            )
            .map_err(|error| format!("No se pudieron limpiar resultados de enrichment: {error}"))?;
        return Ok(deleted);
    }

    let mut deleted = 0_usize;
    for track_id in track_ids {
        deleted += conn
            .execute(
                "DELETE FROM playlist_track_enrichments WHERE library_id = ?1 AND track_id = ?2",
                params![library_id, &track_id],
            )
            .map_err(|error| format!("No se pudo limpiar enrichment de {track_id}: {error}"))?;
    }
    Ok(deleted)
}

fn merge_enrichment_provenance(
    attributes: &mut BTreeMap<String, String>,
    provider: &str,
    fields: &BTreeMap<String, String>,
) -> bool {
    let mut changed = false;

    for (key, value) in fields {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let source_key = enrichment_attribute_key(provider, key);
        if attributes.get(&source_key).map(String::as_str) != Some(value) {
            attributes.insert(source_key, value.to_string());
            changed = true;
        }
    }

    changed
}

fn apply_resolved_enrichment_fields(
    attributes: &mut BTreeMap<String, String>,
    resolved: &BTreeMap<String, enrichment::ResolvedField>,
) -> bool {
    let mut changed = false;
    for (field, value) in resolved {
        let (lookup_keys, canonical_key): (&[&str], &str) = match field.as_str() {
            "genre" => (&["Genre"], "Genre"),
            "year" => (&["Year"], "Year"),
            "label" => (&["Label"], "Label"),
            "isrc" => (&["ISRC", "Isrc"], "ISRC"),
            "comments" => (&["Comments", "Comment"], "Comments"),
            "bpm" => (&["AverageBpm", "Bpm", "BPM"], "AverageBpm"),
            "key" => (&["Tonality", "Key"], "Tonality"),
            "musicbrainz_recording_id" => (
                &["MusicBrainzRecordingID", "MusicBrainz Track Id"],
                "MusicBrainzRecordingID",
            ),
            "musicbrainz_release_id" => (&["MusicBrainzReleaseID"], "MusicBrainzReleaseID"),
            _ => continue,
        };
        changed |=
            set_attribute_if_missing(attributes, lookup_keys, canonical_key, Some(&value.value));
    }
    changed
}

fn set_attribute_if_missing(
    attributes: &mut BTreeMap<String, String>,
    lookup_keys: &[&str],
    canonical_key: &str,
    value: Option<&String>,
) -> bool {
    let Some(value) = value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if attribute_value(attributes, lookup_keys).is_some() {
        return false;
    }
    attributes.insert(canonical_key.to_string(), value.to_string());
    true
}

fn enrichment_attribute_key(provider: &str, key: &str) -> String {
    format!(
        "Enrichment{}{}",
        pascal_fragment(provider),
        pascal_fragment(key)
    )
}

fn pascal_fragment(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn get_index_track(
    conn: &Connection,
    library_id: &str,
    track_id: &str,
) -> Result<Option<PlaylistIndexTrack>, String> {
    conn.query_row(
        &format!(
            "SELECT {}
             FROM playlist_index_tracks t
             WHERE t.library_id = ?1 AND t.track_id = ?2",
            track_select_clause()
        ),
        params![library_id, track_id],
        row_to_track,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer track indexado: {error}"))
}

fn indexed_playlist_paths_for_track(
    conn: &Connection,
    library_id: &str,
    track_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT playlist_path
             FROM playlist_index_memberships
             WHERE library_id = ?1 AND track_id = ?2
             ORDER BY playlist_path COLLATE NOCASE",
        )
        .map_err(|error| format!("No se pudieron preparar playlists del track: {error}"))?;
    let rows = stmt
        .query_map(params![library_id, track_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("No se pudieron leer playlists del track: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear playlists del track: {error}"))
}

fn indexed_track_search_text(
    track: &PlaylistIndexTrack,
    attributes: &BTreeMap<String, String>,
    playlist_paths: &[String],
) -> String {
    let metadata = attributes
        .iter()
        .filter_map(|(key, value)| {
            let value = value.trim();
            (!value.is_empty()).then(|| format!("{key}: {value}"))
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "title: {}\nartist: {}\nalbum: {}\nkind: {}\nplaylists: {}\nlocation: {}\nmetadata:\n{}",
        track.name.as_deref().unwrap_or(""),
        track.artist.as_deref().unwrap_or(""),
        track.album.as_deref().unwrap_or(""),
        track.kind.as_deref().unwrap_or(""),
        playlist_paths.join(" | "),
        track.location.as_deref().unwrap_or(""),
        metadata
    )
}

fn emit_enrichment_progress(
    app: &AppHandle,
    library_id: &str,
    track_id: Option<String>,
    provider: Option<String>,
    status: Option<String>,
    level: &str,
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
        "track-enrichment-progress",
        TrackEnrichmentProgressEvent {
            event_type: "track_enrichment_progress".to_string(),
            level: level.to_string(),
            message: message.to_string(),
            progress: Some(progress),
            library_id: library_id.to_string(),
            track_id,
            provider,
            status,
            processed,
            total,
            timestamp: timestamp(),
        },
    );
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
    row_to_track_at(row, 0)
}

fn row_to_track_at(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<PlaylistIndexTrack> {
    let attributes = parse_track_attributes_json(row.get::<_, Option<String>>(offset + 14)?);

    Ok(PlaylistIndexTrack {
        library_id: row.get(offset)?,
        track_id: row.get(offset + 1)?,
        name: row.get(offset + 2)?,
        artist: row.get(offset + 3)?,
        album: row.get(offset + 4)?,
        kind: row.get(offset + 5)?,
        location: row.get(offset + 6)?,
        source_path: row.get(offset + 7)?,
        size: option_i64_to_u64(row.get(offset + 8)?),
        total_time: option_i64_to_u64(row.get(offset + 9)?),
        sample_rate: option_i64_to_u32(row.get(offset + 10)?),
        bitrate: option_i64_to_u32(row.get(offset + 11)?),
        source_exists: row.get::<_, i64>(offset + 12)? == 1,
        search_text: row.get(offset + 13)?,
        genre: attribute_value(&attributes, &["Genre"]),
        comments: attribute_value(&attributes, &["Comments", "Comment"]),
        bpm: attribute_value(&attributes, &["AverageBpm", "Bpm", "BPM"]),
        key: attribute_value(&attributes, &["Tonality", "Key"]),
        rating: attribute_value(&attributes, &["Rating"]),
        user_rating: option_i64_to_u8(row.get(offset + 15)?),
        year: attribute_value(&attributes, &["Year"]),
        label: attribute_value(&attributes, &["Label"]),
        date_added: attribute_value(&attributes, &["DateAdded", "Date"]),
        attributes,
        embedding_ready: row.get::<_, i64>(offset + 16)? == 1,
    })
}

fn track_select_clause() -> &'static str {
    "t.library_id, t.track_id, t.name, t.artist, t.album, t.kind, t.location, t.source_path,
     t.size_bytes, t.total_time, t.sample_rate, t.bitrate, t.source_exists, t.search_text,
     t.attributes_json, t.user_rating,
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

fn emit_copilot_progress(
    app: &AppHandle,
    request_id: &str,
    stage: &str,
    status: &str,
    message: &str,
    detail: Option<String>,
) {
    let _ = app.emit(
        "playlist-copilot-progress",
        PlaylistCopilotProgressEvent {
            request_id: request_id.to_string(),
            stage: stage.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            detail,
            timestamp: timestamp(),
        },
    );
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

fn option_i64_to_u8(value: Option<i64>) -> Option<u8> {
    value.and_then(|value| u8::try_from(value).ok())
}

#[cfg(test)]
mod playlist_index_tests {
    use super::*;

    #[test]
    fn copilot_follow_up_updates_the_persisted_brief_implicitly() {
        let profile = PlaylistCopilotProfile {
            track_count: 6_000,
            genres: vec!["House".to_string(), "Techno".to_string()],
            artists: Vec::new(),
            keys: vec!["8A".to_string()],
            bpm_min: Some(80.0),
            bpm_max: Some(160.0),
            bpm_anchor: Some(124.0),
        };
        let previous = PlaylistCopilotInterpretation {
            genres: vec!["House".to_string()],
            bpm_min: Some(120.0),
            bpm_max: Some(126.0),
            ..PlaylistCopilotInterpretation::default()
        };

        let updated = local_copilot_interpretation(
            "Manten house, pero mas energia, no repitas y evita vocal",
            &profile,
            30,
            Some(&previous),
        );
        let changes = playlist_copilot_brief_changes(Some(&previous), &updated, false);
        let probes = playlist_copilot_search_probes("mas energia y variedad", &updated, false);

        assert_eq!(updated.genres, vec!["House"]);
        assert_eq!(updated.energy.as_deref(), Some("peak"));
        assert_eq!(updated.energy_curve, EnergyCurve::Ramp);
        assert_eq!(updated.discovery_mode, DiscoveryMode::Discovery);
        assert!(updated.exclude_terms.iter().any(|term| term == "vocal"));
        assert!(changes.iter().any(|change| change.contains("Energia")));
        assert!(changes.iter().any(|change| change.contains("Discovery")));
        assert!(probes.len() >= 4);
        assert!(probes.iter().any(|probe| probe.id == "adjacent"));
    }

    #[test]
    fn copilot_schema_upgrades_existing_tables() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE playlist_copilot_sessions (
                id TEXT PRIMARY KEY,
                library_id TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE playlist_copilot_candidate_sets (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                prompt TEXT NOT NULL,
                interpretation_json TEXT NOT NULL,
                reasoning_json TEXT NOT NULL,
                coverage_json TEXT NOT NULL,
                created_at TEXT NOT NULL
             );
             CREATE TABLE playlist_copilot_candidate_tracks (
                candidate_set_id TEXT NOT NULL,
                track_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                score REAL NOT NULL,
                reasons_json TEXT NOT NULL,
                PRIMARY KEY(candidate_set_id, track_id)
             );",
        )
        .expect("create legacy schema");

        init_db(&conn).expect("upgrade legacy schema");

        assert!(table_columns(&conn, "playlist_copilot_sessions").contains(&"intent_json".into()));
        assert!(table_columns(&conn, "playlist_copilot_candidate_sets")
            .contains(&"ranker_version".into()));
        assert!(table_columns(&conn, "playlist_copilot_candidate_tracks")
            .contains(&"score_components_json".into()));
        assert!(table_columns(&conn, "playlist_enrichment_runs").contains(&"providers_json".into()));
        assert!(table_columns(&conn, "playlist_enrichment_tasks").contains(&"error_kind".into()));
        assert!(
            table_columns(&conn, "playlist_enrichment_observations").contains(&"confidence".into())
        );
        assert!(table_columns(&conn, "playlist_index_tracks").contains(&"user_rating".into()));
    }

    #[test]
    fn track_rating_is_validated_and_persisted() {
        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_db(&conn).expect("initialize schema");
        let now = timestamp();
        conn.execute(
            "INSERT INTO playlist_index_libraries (
                id, source_path, source_name, indexed_at, updated_at
             ) VALUES ('library-1', '/tmp/library.xml', 'library.xml', ?1, ?1)",
            params![&now],
        )
        .expect("insert library");
        conn.execute(
            "INSERT INTO playlist_index_tracks (
                library_id, track_id, name, source_exists, search_text,
                attributes_json, created_at, updated_at
             ) VALUES ('library-1', 'track-1', 'Track', 1, 'Track',
                       '{\"Rating\":\"204\"}', ?1, ?1)",
            params![&now],
        )
        .expect("insert track");

        let rated = set_track_rating(&conn, "library-1", "track-1", 5).expect("save rating");
        assert_eq!(rated.user_rating, Some(5));
        assert_eq!(rated.rating.as_deref(), Some("204"));

        let cleared = set_track_rating(&conn, "library-1", "track-1", 0).expect("clear rating");
        assert_eq!(cleared.user_rating, Some(0));
        assert!(set_track_rating(&conn, "library-1", "track-1", 6).is_err());

        let updated = set_catalog_tracks_rating(
            &mut conn,
            "library-1",
            vec!["track-1".to_string(), "track-1".to_string()],
            3,
        )
        .expect("bulk rating");
        assert_eq!(updated, 1);
        assert_eq!(
            get_index_track(&conn, "library-1", "track-1")
                .unwrap()
                .unwrap()
                .user_rating,
            Some(3)
        );
    }

    #[test]
    fn catalog_query_parses_operators_and_quoted_values() {
        let mut filters = PlaylistCatalogFilters::default();
        let terms = parse_catalog_query(
            "dance genre:\"Deep House\" bpm:120..130 key:8A|9A rating:>=4 missing:label",
            &mut filters,
        );

        assert_eq!(terms, vec!["dance"]);
        assert_eq!(filters.genres, vec!["Deep House"]);
        assert_eq!(filters.keys, vec!["8A", "9A"]);
        assert_eq!(filters.bpm_min, Some(120.0));
        assert_eq!(filters.bpm_max, Some(130.0));
        assert_eq!(filters.rating_min, Some(4));
        assert_eq!(filters.metadata_gaps, vec!["missing_label"]);
    }

    #[test]
    fn catalog_filters_combine_facets_with_and_semantics() {
        let mut track = test_track();
        track.user_rating = Some(4);
        let mut filters = PlaylistCatalogFilters::default();
        let query_terms = parse_catalog_query(
            "Test genre:House bpm:120..130 key:8A rating:>=4 missing:label source:available",
            &mut filters,
        );
        let criteria = CatalogCriteria {
            filters,
            query_terms,
        };

        assert!(catalog_track_matches(&track, &criteria, None));
        track.key = Some("2B".to_string());
        assert!(!catalog_track_matches(&track, &criteria, None));
    }

    #[test]
    fn catalog_saved_searches_persist_definitions_and_recalculate_results() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        init_db(&conn).expect("initialize schema");
        let now = timestamp();
        conn.execute(
            "INSERT INTO playlist_index_libraries (
               id, source_path, source_name, indexed_at, updated_at
             ) VALUES ('library-1', '/tmp/library.xml', 'library.xml', ?1, ?1)",
            params![&now],
        )
        .expect("insert library");
        conn.execute(
            "INSERT INTO playlist_index_tracks (
               library_id, track_id, name, artist, source_exists, search_text,
               attributes_json, created_at, updated_at
             ) VALUES ('library-1', 'track-1', 'First', 'Artist', 1, 'First Artist House',
                       '{\"Genre\":\"House\",\"AverageBpm\":\"124\"}', ?1, ?1)",
            params![&now],
        )
        .expect("insert first track");

        let saved = save_catalog_search(
            &conn,
            PlaylistCatalogSaveRequest {
                id: None,
                library_id: "library-1".to_string(),
                name: "House ready".to_string(),
                description: Some("Dynamic house selection".to_string()),
                query: Some("genre:House".to_string()),
                filters: Some(PlaylistCatalogFilters::default()),
                sort: Some("bpm".to_string()),
            },
        )
        .expect("save catalog search");
        assert_eq!(saved.result_count, 1);

        conn.execute(
            "INSERT INTO playlist_index_tracks (
               library_id, track_id, name, artist, source_exists, search_text,
               attributes_json, created_at, updated_at
             ) VALUES ('library-1', 'track-2', 'Second', 'Artist', 1, 'Second Artist House',
                       '{\"Genre\":\"House\",\"AverageBpm\":\"126\"}', ?1, ?1)",
            params![&now],
        )
        .expect("insert second track");

        let selection = catalog_select_all(
            &conn,
            PlaylistCatalogRequest {
                library_id: saved.library_id.clone(),
                query: Some(saved.query.clone()),
                filters: Some(saved.filters.clone()),
                sort: Some(saved.sort.clone()),
                page: None,
                page_size: None,
            },
            100,
        )
        .expect("recalculate saved search");
        assert_eq!(selection.total, 2);
        assert!(!selection.truncated);

        let updated = save_catalog_search(
            &conn,
            PlaylistCatalogSaveRequest {
                id: Some(saved.id.clone()),
                library_id: saved.library_id.clone(),
                name: saved.name.clone(),
                description: saved.description.clone(),
                query: Some(saved.query.clone()),
                filters: Some(saved.filters.clone()),
                sort: Some(saved.sort.clone()),
            },
        )
        .expect("update saved search count");
        assert_eq!(updated.result_count, 2);
        assert_eq!(
            list_catalog_saved_searches(&conn, "library-1")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            delete_catalog_saved_search(&conn, "library-1", &saved.id).unwrap(),
            saved.id
        );
        assert!(list_catalog_saved_searches(&conn, "library-1")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn copilot_schema_and_guided_turn_persistence_are_idempotent() {
        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_db(&conn).expect("initialize schema");
        init_db(&conn).expect("initialize schema twice");

        assert!(table_columns(&conn, "playlist_copilot_sessions").contains(&"intent_json".into()));
        assert!(table_columns(&conn, "playlist_copilot_candidate_sets")
            .contains(&"ranker_version".into()));
        assert!(table_columns(&conn, "playlist_copilot_candidate_tracks")
            .contains(&"score_components_json".into()));

        let now = timestamp();
        conn.execute(
            "INSERT INTO playlist_index_libraries (
                id, source_path, source_name, track_count, playlist_count, indexed_at, updated_at
             ) VALUES ('library-1', '/tmp/library.xml', 'library.xml', 0, 0, ?1, ?1)",
            params![&now],
        )
        .expect("insert library");
        let intent = PlaylistCopilotInterpretation {
            energy_curve: EnergyCurve::Ramp,
            harmonic_policy: HarmonicPolicy::Strict,
            ..PlaylistCopilotInterpretation::default()
        };
        let session_id = persist_playlist_copilot_brief_turn(
            &mut conn,
            "library-1",
            None,
            "Rampa de energia",
            "Siguiente pregunta",
            &intent,
        )
        .expect("persist guided turn");

        let user_message = conn
            .query_row(
                "SELECT content FROM playlist_copilot_messages
                 WHERE session_id = ?1 AND role = 'user'",
                params![&session_id],
                |row| row.get::<_, String>(0),
            )
            .expect("load user message");
        let candidate_sets = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_copilot_candidate_sets WHERE session_id = ?1",
                params![&session_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count candidate sets");
        let restored = load_previous_copilot_intent(&conn, "library-1", Some(&session_id))
            .expect("load intent")
            .expect("intent exists");

        assert_eq!(user_message, "Rampa de energia");
        assert_eq!(candidate_sets, 0);
        assert_eq!(restored.energy_curve, EnergyCurve::Ramp);
        assert_eq!(restored.harmonic_policy, HarmonicPolicy::Strict);

        let candidate = PlaylistCopilotCandidate {
            track: test_track(),
            score: 91.25,
            reasons: vec!["Genero: House".to_string()],
            score_components: BTreeMap::from([
                ("genre".to_string(), 38.0),
                ("semantic".to_string(), 31.25),
            ]),
        };
        let (_, candidate_set_id) = persist_playlist_copilot_run(
            &mut conn,
            "library-1",
            Some(&session_id),
            "Mantener la rampa",
            "Playlist lista",
            &intent,
            &["Ranking estructurado".to_string()],
            &playlist_copilot_coverage(std::slice::from_ref(&candidate)),
            &[candidate],
        )
        .expect("persist ranked run");
        let stored = conn
            .query_row(
                "SELECT s.ranker_version, t.score_components_json
                 FROM playlist_copilot_candidate_sets s
                 JOIN playlist_copilot_candidate_tracks t ON t.candidate_set_id = s.id
                 WHERE s.id = ?1",
                params![&candidate_set_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("load ranked run");
        assert_eq!(stored.0, COPILOT_RANKER_VERSION);
        assert!(stored.1.contains("semantic"));
        let recent = recent_copilot_suggestion_counts(&conn, "library-1", 8)
            .expect("load recent suggestions");
        assert_eq!(recent.get("track-1"), Some(&1));
    }

    #[test]
    fn applying_multiple_sources_resolves_each_field_and_marks_all_results() {
        let mut conn = Connection::open_in_memory().expect("open sqlite");
        init_db(&conn).expect("initialize schema");
        let now = timestamp();
        conn.execute(
            "INSERT INTO playlist_index_libraries (
                id, source_path, source_name, indexed_at, updated_at
             ) VALUES ('library-1', '/tmp/library.xml', 'library.xml', ?1, ?1)",
            params![&now],
        )
        .expect("insert library");
        conn.execute(
            "INSERT INTO playlist_index_tracks (
                library_id, track_id, name, artist, source_exists, search_text,
                attributes_json, created_at, updated_at
             ) VALUES ('library-1', 'track-1', 'Track', 'Artist', 1, 'Track Artist', '{}', ?1, ?1)",
            params![&now],
        )
        .expect("insert track");
        for (id, provider, confidence, fields) in [
            (
                "result-mb",
                "musicbrainz",
                0.94,
                json!({ "genre": "Electronic", "year": "2022", "label": "Label" }),
            ),
            (
                "result-lastfm",
                "lastfm",
                0.8,
                json!({ "genre": "Deep House", "tags": "deep house, club" }),
            ),
        ] {
            conn.execute(
                "INSERT INTO playlist_track_enrichments (
                    id, library_id, track_id, provider, status, confidence,
                    fields_json, payload_json, created_at, updated_at
                 ) VALUES (?1, 'library-1', 'track-1', ?2, 'matched', ?3, ?4, '{}', ?5, ?5)",
                params![id, provider, confidence, fields.to_string(), &now],
            )
            .expect("insert enrichment result");
        }

        let result = apply_enrichment_results(
            &mut conn,
            "library-1",
            vec!["result-mb".to_string(), "result-lastfm".to_string()],
        )
        .expect("apply enrichment");
        let attributes_json = conn
            .query_row(
                "SELECT attributes_json FROM playlist_index_tracks
                 WHERE library_id = 'library-1' AND track_id = 'track-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("read attributes");
        let attributes = parse_track_attributes_json(Some(attributes_json));
        let applied_count = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_track_enrichments
                 WHERE library_id = 'library-1' AND applied_at IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("count applied");

        assert_eq!(result.applied_total, 2);
        assert_eq!(applied_count, 2);
        assert_eq!(
            attributes.get("Genre").map(String::as_str),
            Some("Deep House")
        );
        assert_eq!(attributes.get("Year").map(String::as_str), Some("2022"));
        assert_eq!(attributes.get("Label").map(String::as_str), Some("Label"));
        assert_eq!(
            attributes.get("Comments").map(String::as_str),
            Some("Tags: deep house, club")
        );
    }

    #[test]
    fn enrichment_run_history_keeps_tasks_and_field_observations() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        init_db(&conn).expect("initialize schema");
        let now = timestamp();
        conn.execute(
            "INSERT INTO playlist_index_libraries (
                id, source_path, source_name, indexed_at, updated_at
             ) VALUES ('library-1', '/tmp/library.xml', 'library.xml', ?1, ?1)",
            params![&now],
        )
        .expect("insert library");
        conn.execute(
            "INSERT INTO playlist_index_tracks (
                library_id, track_id, name, artist, source_exists, search_text,
                attributes_json, created_at, updated_at
             ) VALUES ('library-1', 'track-1', 'Track', 'Artist', 1, 'Track Artist', '{}', ?1, ?1)",
            params![&now],
        )
        .expect("insert track");
        let provider_ids = vec!["musicbrainz".to_string()];
        let clients = enrichment::load_provider_clients_for_test(
            &provider_ids,
            enrichment::ProviderCredentials::default(),
        )
        .expect("provider clients");
        let run_id = create_enrichment_run(&conn, "library-1", &provider_ids, &clients, 1)
            .expect("create run");
        let task_id = create_enrichment_task(&conn, &run_id, "library-1", "track-1", "musicbrainz")
            .expect("create task");
        let suggestion = enrichment::ProviderSuggestion {
            provider: "musicbrainz".to_string(),
            provider_key: Some("mbid".to_string()),
            status: "matched".to_string(),
            confidence: 0.91,
            fields: BTreeMap::from([
                ("musicbrainz_recording_id".to_string(), "mbid".to_string()),
                ("year".to_string(), "2024".to_string()),
            ]),
            payload: json!({ "id": "mbid" }),
            message: Some("ok".to_string()),
            source_url: Some("https://musicbrainz.org/recording/mbid".to_string()),
        };
        finish_enrichment_task(
            &conn,
            &task_id,
            &run_id,
            "library-1",
            "track-1",
            &suggestion,
        )
        .expect("finish task");
        update_enrichment_run_progress(&conn, &run_id, 1, 1, 0, 0, true).expect("finish run");

        let status = conn
            .query_row(
                "SELECT status FROM playlist_enrichment_runs WHERE id = ?1",
                params![&run_id],
                |row| row.get::<_, String>(0),
            )
            .expect("read run");
        let observations = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_enrichment_observations WHERE run_id = ?1",
                params![&run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count observations");

        assert_eq!(status, "completed");
        assert_eq!(observations, 2);
    }

    fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare table info");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query table info")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns")
    }

    fn test_track() -> PlaylistIndexTrack {
        PlaylistIndexTrack {
            library_id: "library-1".to_string(),
            track_id: "track-1".to_string(),
            name: Some("Test Track".to_string()),
            artist: Some("Test Artist".to_string()),
            album: None,
            kind: Some("MP3 File".to_string()),
            location: None,
            source_path: None,
            size: None,
            total_time: None,
            sample_rate: None,
            bitrate: None,
            source_exists: true,
            search_text: "Test Track Test Artist House".to_string(),
            genre: Some("House".to_string()),
            comments: None,
            bpm: Some("124".to_string()),
            key: Some("8A".to_string()),
            rating: None,
            user_rating: None,
            year: None,
            label: None,
            date_added: None,
            attributes: BTreeMap::new(),
            embedding_ready: true,
        }
    }
}
