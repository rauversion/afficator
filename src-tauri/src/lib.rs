use aifficator_core::conversion::{ffmpeg_args, ConversionSettings};
use aifficator_core::exporter::{
    export_replacement_xml, path_to_rekordbox_location, ExportTrackReplacement,
};
use aifficator_core::planner::{build_conversion_plan, ConversionPlan, PlanOptions};
use aifficator_core::rekordbox::{
    parse_rekordbox_xml_file, PlaylistSummary, RekordboxLibrary, Track,
};
use aifficator_core::validation::{
    default_target_path, track_action, validate_library, validate_track, IssueSeverity,
    TrackAction, ValidationReport,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

mod local_conversion;
mod mastering;
mod playlist_index;
mod settings;
mod system;
mod turn;

#[derive(Debug, Serialize)]
struct ImportResponse {
    library: RekordboxLibrary,
    playlists: Vec<PlaylistSummary>,
    validation: ValidationReport,
}

#[derive(Debug, Serialize)]
struct ConvertedFile {
    track_id: String,
    name: Option<String>,
    artist: Option<String>,
    kind: Option<String>,
    source_path: String,
    target_path: String,
    source_exists: bool,
    target_exists: bool,
}

#[derive(Debug, Serialize)]
struct AudioFolderResponse {
    root_path: String,
    recursive: bool,
    files: Vec<AudioFile>,
    skipped_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AudioFile {
    name: String,
    extension: String,
    path: String,
    parent_path: String,
    size_bytes: u64,
    modified_ms: Option<u128>,
}

#[derive(Debug, Serialize)]
struct PlaylistTrackFile {
    position: usize,
    track_id: String,
    name: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    kind: Option<String>,
    location: Option<String>,
    size: Option<u64>,
    total_time: Option<u64>,
    sample_rate: Option<u32>,
    bitrate: Option<u32>,
    attributes: BTreeMap<String, String>,
    source_path: Option<String>,
    source_exists: bool,
    target_path: Option<String>,
    target_exists: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ConversionStatus {
    Queued,
    Running,
    Converted,
    AlreadyConverted,
    AlreadyAiff,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
struct ConversionProgressEvent {
    track_id: String,
    name: Option<String>,
    source_path: Option<String>,
    target_path: Option<String>,
    status: ConversionStatus,
    message: Option<String>,
    percent: Option<f64>,
    elapsed_seconds: Option<f64>,
    speed: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConversionLogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
struct ConversionLogEvent {
    level: ConversionLogLevel,
    track_id: Option<String>,
    name: Option<String>,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
struct ConversionItemResult {
    track_id: String,
    name: Option<String>,
    artist: Option<String>,
    source_path: Option<String>,
    target_path: Option<String>,
    status: ConversionStatus,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConversionBatchResult {
    items: Vec<ConversionItemResult>,
    converted_total: usize,
    already_converted_total: usize,
    already_aiff_total: usize,
    failed_total: usize,
}

#[derive(Debug, Serialize)]
struct ExportXmlResult {
    output_path: String,
    selected_playlist_total: usize,
    selected_track_total: usize,
    replaced_track_total: usize,
}

#[derive(Debug, Serialize)]
struct SystemStatus {
    ffmpeg: system::BinaryStatus,
    ffprobe: system::BinaryStatus,
    checked_at_ms: u128,
}

#[tauri::command]
fn system_status(app: AppHandle) -> SystemStatus {
    SystemStatus {
        ffmpeg: system::binary_status(&app, "ffmpeg"),
        ffprobe: system::binary_status(&app, "ffprobe"),
        checked_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
    }
}

#[tauri::command]
fn get_openai_api_key_status(app: AppHandle) -> Result<settings::OpenAiApiKeyStatus, String> {
    settings::get_openai_api_key_status(&app)
}

#[tauri::command]
fn save_openai_api_key(
    app: AppHandle,
    api_key: String,
) -> Result<settings::OpenAiApiKeyStatus, String> {
    settings::save_openai_api_key(&app, api_key)
}

#[tauri::command]
fn clear_openai_api_key(app: AppHandle) -> Result<settings::OpenAiApiKeyStatus, String> {
    settings::clear_openai_api_key(&app)
}

#[tauri::command]
fn get_audio_tool_settings(app: AppHandle) -> Result<settings::AudioToolSettings, String> {
    settings::get_audio_tool_settings(&app)
}

#[tauri::command]
fn save_audio_tool_settings(
    app: AppHandle,
    ffmpeg_path: Option<String>,
    ffprobe_path: Option<String>,
) -> Result<settings::AudioToolSettings, String> {
    settings::save_audio_tool_settings(&app, ffmpeg_path, ffprobe_path)
}

#[tauri::command]
fn get_language_settings(app: AppHandle) -> Result<settings::LanguageSettings, String> {
    settings::get_language_settings(&app)
}

#[tauri::command]
fn save_language_settings(
    app: AppHandle,
    language: String,
) -> Result<settings::LanguageSettings, String> {
    settings::save_language_settings(&app, language)
}

#[tauri::command]
fn import_rekordbox_xml(path: String) -> Result<ImportResponse, String> {
    let library = parse_rekordbox_xml_file(path).map_err(|error| error.to_string())?;
    let playlists = library.playlists_flat();
    let validation = validate_library(&library);

    Ok(ImportResponse {
        library,
        playlists,
        validation,
    })
}

#[tauri::command]
fn plan_conversion(path: String, playlist_paths: Vec<String>) -> Result<ConversionPlan, String> {
    let library = parse_rekordbox_xml_file(path).map_err(|error| error.to_string())?;

    Ok(build_conversion_plan(
        &library,
        PlanOptions {
            playlist_paths,
            reuse_existing: true,
        },
    ))
}

#[tauri::command]
fn export_rekordbox_xml(
    path: String,
    playlist_paths: Vec<String>,
    output_path: String,
) -> Result<ExportXmlResult, String> {
    let xml = fs::read_to_string(&path).map_err(|error| format!("No se pudo leer XML: {error}"))?;
    let library = parse_rekordbox_xml_file(&path).map_err(|error| error.to_string())?;
    let selected_track_ids = export_track_ids(&library, &playlist_paths)?;
    let selected_replacements = export_replacements(&library, &selected_track_ids)?;
    let mut replacements = existing_converted_replacements(&library);
    replacements.extend(selected_replacements.clone());
    let exported_xml =
        export_replacement_xml(&xml, &replacements).map_err(|error| error.to_string())?;

    fs::write(&output_path, exported_xml)
        .map_err(|error| format!("No se pudo escribir XML exportado: {error}"))?;

    Ok(ExportXmlResult {
        output_path,
        selected_playlist_total: playlist_paths.len(),
        selected_track_total: selected_track_ids.len(),
        replaced_track_total: selected_replacements.len(),
    })
}

fn export_track_ids(
    library: &RekordboxLibrary,
    playlist_paths: &[String],
) -> Result<BTreeSet<String>, String> {
    if playlist_paths.is_empty() {
        return Err("Selecciona al menos una playlist para exportar.".to_string());
    }

    let playlists = library.playlists_flat();
    let mut selected_track_ids = BTreeSet::new();
    let mut errors = Vec::new();

    for requested_path in playlist_paths {
        match playlists.iter().find(|playlist| {
            playlist.path == *requested_path && playlist.node_type.as_deref() == Some("1")
        }) {
            Some(playlist) => {
                selected_track_ids.extend(playlist.track_keys.iter().cloned());
            }
            None => errors.push(format!("Playlist no encontrada: {requested_path}")),
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    if selected_track_ids.is_empty() {
        return Err("Las playlists seleccionadas no tienen tracks.".to_string());
    }

    Ok(selected_track_ids)
}

fn export_replacements(
    library: &RekordboxLibrary,
    selected_track_ids: &BTreeSet<String>,
) -> Result<BTreeMap<String, ExportTrackReplacement>, String> {
    let track_index = library.track_by_id();
    let mut replacements = BTreeMap::new();
    let mut target_to_track = BTreeMap::<PathBuf, String>::new();
    let mut errors = Vec::new();

    for track_id in selected_track_ids {
        let Some(track) = track_index.get(track_id) else {
            errors.push(format!("TrackID no existe en COLLECTION: {track_id}"));
            continue;
        };

        let blocking_issues = validate_track(track)
            .into_iter()
            .filter(|issue| issue.severity == IssueSeverity::Error)
            .map(|issue| issue.message)
            .collect::<Vec<_>>();

        if !blocking_issues.is_empty() {
            errors.extend(blocking_issues);
            continue;
        }

        match track_action(track) {
            TrackAction::AlreadyAiff => {}
            TrackAction::Unsupported => {
                errors.push(format!(
                    "Formato no soportado para exportar TrackID {} ({:?})",
                    track.track_id, track.kind
                ));
            }
            TrackAction::Convert => {
                let Some(source_path) = &track.file_path else {
                    errors.push(format!(
                        "TrackID {} no tiene Location valida",
                        track.track_id
                    ));
                    continue;
                };
                let target_path = default_target_path(source_path);

                if let Some(previous_track_id) =
                    target_to_track.insert(target_path.clone(), track.track_id.clone())
                {
                    errors.push(format!(
                        "Colision de salida: TrackID {} y {} apuntan a {}",
                        previous_track_id,
                        track.track_id,
                        target_path.display()
                    ));
                    continue;
                }

                let metadata = match fs::metadata(&target_path) {
                    Ok(metadata) if metadata.is_file() => metadata,
                    Ok(_) => {
                        errors.push(format!(
                            "El AIFF convertido no es un archivo regular: {}",
                            target_path.display()
                        ));
                        continue;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        errors.push(format!(
                            "Falta convertir TrackID {}: {}",
                            track.track_id,
                            target_path.display()
                        ));
                        continue;
                    }
                    Err(error) => {
                        errors.push(format!(
                            "No se pudo leer AIFF convertido {}: {error}",
                            target_path.display()
                        ));
                        continue;
                    }
                };

                match replacement_for_converted_file(&target_path, &metadata) {
                    Ok(replacement) => {
                        replacements.insert(track.track_id.clone(), replacement);
                    }
                    Err(error) => {
                        errors.push(error);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(replacements)
    } else {
        Err(errors.join("\n"))
    }
}

fn existing_converted_replacements(
    library: &RekordboxLibrary,
) -> BTreeMap<String, ExportTrackReplacement> {
    let mut replacements = BTreeMap::new();

    for track in &library.tracks {
        if track_action(track) != TrackAction::Convert {
            continue;
        }

        let Some(source_path) = &track.file_path else {
            continue;
        };
        let target_path = default_target_path(source_path);
        let Ok(metadata) = fs::metadata(&target_path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        if let Ok(replacement) = replacement_for_converted_file(&target_path, &metadata) {
            replacements.insert(track.track_id.clone(), replacement);
        }
    }

    replacements
}

fn replacement_for_converted_file(
    target_path: &Path,
    metadata: &fs::Metadata,
) -> Result<ExportTrackReplacement, String> {
    let location = path_to_rekordbox_location(target_path).map_err(|error| error.to_string())?;

    Ok(ExportTrackReplacement {
        location,
        kind: "AIFF File".to_string(),
        size: Some(metadata.len()),
        sample_rate: Some(44_100),
        bit_rate: Some(1411),
    })
}

#[tauri::command]
async fn convert_tracks(
    app: tauri::AppHandle,
    path: String,
    track_ids: Vec<String>,
    max_concurrency: Option<usize>,
) -> Result<ConversionBatchResult, String> {
    let app_for_error = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        convert_tracks_blocking(app, path, track_ids, max_concurrency)
    })
    .await
    .map_err(|error| {
        settings::localized(
            &app_for_error,
            &format!("La conversion fallo inesperadamente: {error}"),
            &format!("Conversion failed unexpectedly: {error}"),
        )
    })?
}

fn convert_tracks_blocking(
    app: tauri::AppHandle,
    path: String,
    track_ids: Vec<String>,
    max_concurrency: Option<usize>,
) -> Result<ConversionBatchResult, String> {
    let library = parse_rekordbox_xml_file(path).map_err(|error| error.to_string())?;
    let track_index = library.track_by_id();
    let max_concurrency = max_concurrency.unwrap_or(1).clamp(1, 4);
    let mut seen = BTreeSet::new();
    let ordered_track_ids = track_ids
        .into_iter()
        .filter(|track_id| seen.insert(track_id.clone()))
        .collect::<Vec<_>>();

    let mut items = Vec::new();

    emit_conversion_log(
        &app,
        ConversionLogEvent {
            level: ConversionLogLevel::Info,
            track_id: None,
            name: None,
            message: settings::localized(
                &app,
                &format!(
                    "Conversion iniciada: {} track(s), concurrencia maxima {}",
                    ordered_track_ids.len(),
                    max_concurrency
                ),
                &format!(
                    "Conversion started: {} track(s), max concurrency {}",
                    ordered_track_ids.len(),
                    max_concurrency
                ),
            ),
        },
    );

    for chunk in ordered_track_ids.chunks(max_concurrency) {
        let mut handles = Vec::new();

        for track_id in chunk {
            let Some(track) = track_index.get(track_id).cloned() else {
                let item = ConversionItemResult {
                    track_id: track_id.clone(),
                    name: None,
                    artist: None,
                    source_path: None,
                    target_path: None,
                    status: ConversionStatus::Failed,
                    message: Some(settings::localized(
                        &app,
                        &format!("TrackID no existe en COLLECTION: {track_id}"),
                        &format!("TrackID does not exist in COLLECTION: {track_id}"),
                    )),
                };
                emit_conversion_progress(&app, item_progress_event(&item, None, None, None));
                emit_conversion_log(
                    &app,
                    ConversionLogEvent {
                        level: ConversionLogLevel::Error,
                        track_id: Some(track_id.clone()),
                        name: None,
                        message: settings::localized(
                            &app,
                            &format!("TrackID no existe en COLLECTION: {track_id}"),
                            &format!("TrackID does not exist in COLLECTION: {track_id}"),
                        ),
                    },
                );
                items.push(item);
                continue;
            };

            let app_handle = app.clone();
            let track_id = track.track_id.clone();
            handles.push((
                track_id,
                thread::spawn(move || convert_track(&app_handle, &track)),
            ));
        }

        for (track_id, handle) in handles {
            match handle.join() {
                Ok(item) => items.push(item),
                Err(_) => {
                    let item = ConversionItemResult {
                        track_id: track_id.clone(),
                        name: None,
                        artist: None,
                        source_path: None,
                        target_path: None,
                        status: ConversionStatus::Failed,
                        message: Some(settings::localized(
                            &app,
                            "La conversion fallo por un panic interno",
                            "Conversion failed because of an internal panic",
                        )),
                    };
                    emit_conversion_progress(&app, item_progress_event(&item, None, None, None));
                    emit_conversion_log(
                        &app,
                        ConversionLogEvent {
                            level: ConversionLogLevel::Error,
                            track_id: Some(track_id),
                            name: None,
                            message: settings::localized(
                                &app,
                                "La conversion fallo por un panic interno",
                                "Conversion failed because of an internal panic",
                            ),
                        },
                    );
                    items.push(item);
                }
            }
        }
    }

    let result = ConversionBatchResult {
        converted_total: items
            .iter()
            .filter(|item| item.status == ConversionStatus::Converted)
            .count(),
        already_converted_total: items
            .iter()
            .filter(|item| item.status == ConversionStatus::AlreadyConverted)
            .count(),
        already_aiff_total: items
            .iter()
            .filter(|item| item.status == ConversionStatus::AlreadyAiff)
            .count(),
        failed_total: items
            .iter()
            .filter(|item| item.status == ConversionStatus::Failed)
            .count(),
        items,
    };

    emit_conversion_log(
        &app,
        ConversionLogEvent {
            level: if result.failed_total > 0 {
                ConversionLogLevel::Warning
            } else {
                ConversionLogLevel::Info
            },
            track_id: None,
            name: None,
            message: settings::localized(
                &app,
                &format!(
                    "Conversion terminada: {} convertidos, {} existentes, {} AIFF originales, {} errores",
                    result.converted_total,
                    result.already_converted_total,
                    result.already_aiff_total,
                    result.failed_total
                ),
                &format!(
                    "Conversion finished: {} converted, {} existing, {} original AIFF, {} errors",
                    result.converted_total,
                    result.already_converted_total,
                    result.already_aiff_total,
                    result.failed_total
                ),
            ),
        },
    );

    Ok(result)
}

fn convert_track(app: &tauri::AppHandle, track: &Track) -> ConversionItemResult {
    let source_path = track.file_path.clone();
    let target_path = source_path.as_deref().map(default_target_path);
    let mut item = ConversionItemResult {
        track_id: track.track_id.clone(),
        name: track.name.clone(),
        artist: track.artist.clone(),
        source_path: source_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        target_path: target_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        status: ConversionStatus::Queued,
        message: Some(settings::localized(app, "En cola", "Queued")),
    };

    emit_conversion_progress(app, item_progress_event(&item, Some(0.0), None, None));
    emit_conversion_log(
        app,
        ConversionLogEvent {
            level: ConversionLogLevel::Info,
            track_id: Some(item.track_id.clone()),
            name: item.name.clone(),
            message: settings::localized(app, "En cola", "Queued"),
        },
    );

    let blocking_issue = validate_track(track)
        .into_iter()
        .find(|issue| issue.severity == IssueSeverity::Error);
    if let Some(issue) = blocking_issue {
        item.status = ConversionStatus::Failed;
        item.message = Some(issue.message);
        emit_conversion_progress(app, item_progress_event(&item, None, None, None));
        emit_conversion_log(
            app,
            ConversionLogEvent {
                level: ConversionLogLevel::Error,
                track_id: Some(item.track_id.clone()),
                name: item.name.clone(),
                message: item.message.clone().unwrap_or_default(),
            },
        );
        return item;
    }

    if track_action(track) == TrackAction::AlreadyAiff {
        item.target_path = item.source_path.clone();
        item.status = ConversionStatus::AlreadyAiff;
        item.message = Some(settings::localized(
            app,
            "El original ya es AIFF",
            "Original file is already AIFF",
        ));
        emit_conversion_progress(app, item_progress_event(&item, Some(100.0), None, None));
        emit_conversion_log(
            app,
            ConversionLogEvent {
                level: ConversionLogLevel::Info,
                track_id: Some(item.track_id.clone()),
                name: item.name.clone(),
                message: settings::localized(
                    app,
                    "Omitido: el original ya es AIFF",
                    "Skipped: original file is already AIFF",
                ),
            },
        );
        return item;
    }

    let Some(source_path) = source_path else {
        item.status = ConversionStatus::Failed;
        item.message = Some(settings::localized(
            app,
            "El track no tiene Location valida",
            "Track does not have a valid Location",
        ));
        emit_conversion_progress(app, item_progress_event(&item, None, None, None));
        emit_conversion_log(
            app,
            ConversionLogEvent {
                level: ConversionLogLevel::Error,
                track_id: Some(item.track_id.clone()),
                name: item.name.clone(),
                message: item.message.clone().unwrap_or_default(),
            },
        );
        return item;
    };
    let Some(target_path) = target_path else {
        item.status = ConversionStatus::Failed;
        item.message = Some(settings::localized(
            app,
            "No se pudo resolver la ruta de salida",
            "Could not resolve output path",
        ));
        emit_conversion_progress(app, item_progress_event(&item, None, None, None));
        emit_conversion_log(
            app,
            ConversionLogEvent {
                level: ConversionLogLevel::Error,
                track_id: Some(item.track_id.clone()),
                name: item.name.clone(),
                message: item.message.clone().unwrap_or_default(),
            },
        );
        return item;
    };

    if target_path.exists() {
        item.status = ConversionStatus::AlreadyConverted;
        item.message = Some(settings::localized(
            app,
            "AIFF convertido ya existe",
            "Converted AIFF already exists",
        ));
        emit_conversion_progress(app, item_progress_event(&item, Some(100.0), None, None));
        emit_conversion_log(
            app,
            ConversionLogEvent {
                level: ConversionLogLevel::Info,
                track_id: Some(item.track_id.clone()),
                name: item.name.clone(),
                message: settings::localized(
                    app,
                    &format!("Reutilizando AIFF existente: {}", target_path.display()),
                    &format!("Reusing existing AIFF: {}", target_path.display()),
                ),
            },
        );
        return item;
    }

    if let Some(parent) = target_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            item.status = ConversionStatus::Failed;
            item.message = Some(settings::localized(
                app,
                &format!("No se pudo crear la carpeta {}: {error}", parent.display()),
                &format!("Could not create folder {}: {error}", parent.display()),
            ));
            emit_conversion_progress(app, item_progress_event(&item, None, None, None));
            emit_conversion_log(
                app,
                ConversionLogEvent {
                    level: ConversionLogLevel::Error,
                    track_id: Some(item.track_id.clone()),
                    name: item.name.clone(),
                    message: item.message.clone().unwrap_or_default(),
                },
            );
            return item;
        }
    }

    item.status = ConversionStatus::Running;
    item.message = Some(settings::localized(
        app,
        "Convirtiendo con ffmpeg",
        "Converting with ffmpeg",
    ));
    emit_conversion_progress(app, item_progress_event(&item, Some(0.0), Some(0.0), None));
    emit_conversion_log(
        app,
        ConversionLogEvent {
            level: ConversionLogLevel::Info,
            track_id: Some(item.track_id.clone()),
            name: item.name.clone(),
            message: settings::localized(
                app,
                &format!(
                    "ffmpeg iniciado: {} -> {}",
                    source_path.display(),
                    target_path.display()
                ),
                &format!(
                    "ffmpeg started: {} -> {}",
                    source_path.display(),
                    target_path.display()
                ),
            ),
        },
    );

    match run_ffmpeg_conversion(app, track, &source_path, &target_path) {
        Ok(()) => {
            item.status = ConversionStatus::Converted;
            item.message = Some(settings::localized(
                app,
                "Conversion completada",
                "Conversion completed",
            ));
            emit_conversion_progress(app, item_progress_event(&item, Some(100.0), None, None));
            emit_conversion_log(
                app,
                ConversionLogEvent {
                    level: ConversionLogLevel::Info,
                    track_id: Some(item.track_id.clone()),
                    name: item.name.clone(),
                    message: settings::localized(
                        app,
                        &format!("Conversion completada: {}", target_path.display()),
                        &format!("Conversion completed: {}", target_path.display()),
                    ),
                },
            );
        }
        Err(error) => {
            item.status = ConversionStatus::Failed;
            item.message = Some(error);
            emit_conversion_progress(app, item_progress_event(&item, None, None, None));
            emit_conversion_log(
                app,
                ConversionLogEvent {
                    level: ConversionLogLevel::Error,
                    track_id: Some(item.track_id.clone()),
                    name: item.name.clone(),
                    message: item.message.clone().unwrap_or_default(),
                },
            );
        }
    }

    item
}

fn run_ffmpeg_conversion(
    app: &tauri::AppHandle,
    track: &Track,
    source_path: &Path,
    target_path: &Path,
) -> Result<(), String> {
    let settings = ConversionSettings::default();
    let args = ffmpeg_args(source_path, target_path, &settings);
    let mut child = system::ffmpeg_command(app)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            settings::localized(
                app,
                &format!("No se pudo ejecutar ffmpeg. Revisa que este instalado en PATH: {error}"),
                &format!("Could not run ffmpeg. Check that it is installed in PATH: {error}"),
            )
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        settings::localized(
            app,
            "No se pudo leer el progreso de ffmpeg",
            "Could not read ffmpeg progress",
        )
    })?;
    let stderr = child.stderr.take();
    let stderr_handle = stderr.map(|stderr| {
        let app = app.clone();
        let track_id = track.track_id.clone();
        let name = track.name.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut lines = Vec::new();

            for line in reader.lines() {
                let Ok(line) = line else {
                    continue;
                };
                if line.trim().is_empty() {
                    continue;
                }

                emit_conversion_log(
                    &app,
                    ConversionLogEvent {
                        level: ConversionLogLevel::Info,
                        track_id: Some(track_id.clone()),
                        name: name.clone(),
                        message: format!("ffmpeg: {line}"),
                    },
                );
                lines.push(line);
            }

            lines.join("\n")
        })
    });

    let total_seconds = track
        .total_time
        .map(|seconds| seconds as f64)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0);
    let reader = BufReader::new(stdout);
    let mut elapsed_seconds = None;
    let mut speed = None;

    for line in reader.lines() {
        let Ok(line) = line else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key {
            "out_time_us" | "out_time_ms" => {
                elapsed_seconds = parse_ffmpeg_progress_seconds(value);
            }
            "speed" => {
                speed = Some(value.to_string());
            }
            "progress" => {
                let percent =
                    conversion_percent(elapsed_seconds, total_seconds).map(|value| value.min(99.0));
                emit_conversion_progress(
                    app,
                    ConversionProgressEvent {
                        track_id: track.track_id.clone(),
                        name: track.name.clone(),
                        source_path: Some(source_path.to_string_lossy().into_owned()),
                        target_path: Some(target_path.to_string_lossy().into_owned()),
                        status: ConversionStatus::Running,
                        message: Some(if value == "end" {
                            settings::localized(app, "Finalizando", "Finalizing")
                        } else {
                            settings::localized(
                                app,
                                "Convirtiendo con ffmpeg",
                                "Converting with ffmpeg",
                            )
                        }),
                        percent,
                        elapsed_seconds,
                        speed: speed.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    let status = child.wait().map_err(|error| {
        settings::localized(
            app,
            &format!("No se pudo esperar a ffmpeg: {error}"),
            &format!("Could not wait for ffmpeg: {error}"),
        )
    })?;
    let stderr_output = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if !status.success() {
        return Err(settings::localized(
            app,
            &format!(
                "ffmpeg fallo con estado {status}. {}",
                stderr_tail(&stderr_output)
            ),
            &format!(
                "ffmpeg failed with status {status}. {}",
                stderr_tail(&stderr_output)
            ),
        ));
    }

    if !target_path.exists() {
        return Err(settings::localized(
            app,
            &format!(
                "ffmpeg termino sin generar el archivo {}",
                target_path.display()
            ),
            &format!("ffmpeg finished without creating {}", target_path.display()),
        ));
    }

    Ok(())
}

fn emit_conversion_progress(app: &tauri::AppHandle, event: ConversionProgressEvent) {
    let _ = app.emit("conversion-progress", event);
}

fn emit_conversion_log(app: &tauri::AppHandle, event: ConversionLogEvent) {
    let _ = app.emit("conversion-log", event);
}

fn item_progress_event(
    item: &ConversionItemResult,
    percent: Option<f64>,
    elapsed_seconds: Option<f64>,
    speed: Option<String>,
) -> ConversionProgressEvent {
    ConversionProgressEvent {
        track_id: item.track_id.clone(),
        name: item.name.clone(),
        source_path: item.source_path.clone(),
        target_path: item.target_path.clone(),
        status: item.status.clone(),
        message: item.message.clone(),
        percent,
        elapsed_seconds,
        speed,
    }
}

fn parse_ffmpeg_progress_seconds(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .map(|microseconds| microseconds / 1_000_000.0)
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
}

fn conversion_percent(elapsed_seconds: Option<f64>, total_seconds: Option<f64>) -> Option<f64> {
    let elapsed_seconds = elapsed_seconds?;
    let total_seconds = total_seconds?;

    Some(((elapsed_seconds / total_seconds) * 100.0).clamp(0.0, 100.0))
}

fn stderr_tail(stderr_output: &str) -> String {
    let lines = stderr_output
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(6)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return "Sin detalle adicional de ffmpeg.".to_string();
    }

    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

#[tauri::command]
fn list_converted_files(path: String) -> Result<Vec<ConvertedFile>, String> {
    let library = parse_rekordbox_xml_file(path).map_err(|error| error.to_string())?;
    let mut converted_files = library
        .tracks
        .iter()
        .filter(|track| matches!(track_action(track), TrackAction::Convert))
        .filter_map(|track| {
            let source_path = track.file_path.as_ref()?;
            let target_path = default_target_path(source_path);

            if !target_path.exists() {
                return None;
            }

            Some(ConvertedFile {
                track_id: track.track_id.clone(),
                name: track.name.clone(),
                artist: track.artist.clone(),
                kind: track.kind.clone(),
                source_path: source_path.to_string_lossy().into_owned(),
                target_path: target_path.to_string_lossy().into_owned(),
                source_exists: source_path.exists(),
                target_exists: true,
            })
        })
        .collect::<Vec<_>>();

    converted_files.sort_by(|left, right| {
        left.artist
            .cmp(&right.artist)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.target_path.cmp(&right.target_path))
    });

    Ok(converted_files)
}

#[tauri::command]
fn list_audio_files(folder_path: String, recursive: bool) -> Result<AudioFolderResponse, String> {
    let root = PathBuf::from(folder_path);
    let metadata = fs::metadata(&root)
        .map_err(|error| format!("No se pudo leer la carpeta {}: {error}", root.display()))?;

    if !metadata.is_dir() {
        return Err(format!("El path no es una carpeta: {}", root.display()));
    }

    let mut files = Vec::new();
    let mut skipped_errors = Vec::new();
    collect_audio_files(&root, &root, recursive, &mut files, &mut skipped_errors);
    files.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(AudioFolderResponse {
        root_path: root.to_string_lossy().into_owned(),
        recursive,
        files,
        skipped_errors,
    })
}

#[tauri::command]
fn playlist_tracks(path: String, playlist_path: String) -> Result<Vec<PlaylistTrackFile>, String> {
    let library = parse_rekordbox_xml_file(path).map_err(|error| error.to_string())?;
    let playlists = library.playlists_flat();
    let playlist = playlists
        .iter()
        .find(|playlist| playlist.path == playlist_path)
        .ok_or_else(|| format!("No se encontro la playlist: {playlist_path}"))?;
    let track_index = library.track_by_id();

    Ok(playlist
        .track_keys
        .iter()
        .enumerate()
        .map(|(position, track_id)| {
            let track = track_index.get(track_id);
            let source_path = track.and_then(|track| track.file_path.as_ref());
            let target_path = source_path.map(|path| default_target_path(path));

            PlaylistTrackFile {
                position: position + 1,
                track_id: track_id.clone(),
                name: track.and_then(|track| track.name.clone()),
                artist: track.and_then(|track| track.artist.clone()),
                album: track.and_then(|track| track.album.clone()),
                kind: track.and_then(|track| track.kind.clone()),
                location: track.and_then(|track| track.location.clone()),
                size: track.and_then(|track| track.size),
                total_time: track.and_then(|track| track.total_time),
                sample_rate: track.and_then(|track| track.sample_rate),
                bitrate: track.and_then(|track| track.bitrate),
                attributes: track
                    .map(|track| track.attributes.clone())
                    .unwrap_or_default(),
                source_path: source_path.map(|path| path.to_string_lossy().into_owned()),
                source_exists: source_path.is_some_and(|path| path.exists()),
                target_exists: target_path.as_ref().is_some_and(|path| path.exists()),
                target_path: target_path.map(|path| path.to_string_lossy().into_owned()),
            }
        })
        .collect())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    reveal_in_file_manager(PathBuf::from(path))
}

#[tauri::command]
fn open_parent_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let folder = if path.is_dir() {
        path
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "No se pudo resolver la carpeta del archivo".to_string())?
    };

    open_path(&folder)
}

fn reveal_in_file_manager(path: PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("El path no existe: {}", path.display()));
    }

    #[cfg(target_os = "macos")]
    {
        run_command(Command::new("open").arg("-R").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        run_command(Command::new("explorer").arg("/select,").arg(path))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let folder = if path.is_dir() {
            path
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "No se pudo resolver la carpeta del archivo".to_string())?
        };
        run_command(Command::new("xdg-open").arg(folder))
    }
}

fn open_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("El path no existe: {}", path.display()));
    }

    #[cfg(target_os = "macos")]
    {
        run_command(Command::new("open").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        run_command(Command::new("explorer").arg(path))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_command(Command::new("xdg-open").arg(path))
    }
}

fn run_command(command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("No se pudo abrir el path: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("El comando del sistema fallo con estado: {status}"))
    }
}

fn collect_audio_files(
    root: &Path,
    current: &Path,
    recursive: bool,
    files: &mut Vec<AudioFile>,
    skipped_errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) => {
            skipped_errors.push(format!("No se pudo leer {}: {error}", current.display()));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                skipped_errors.push(format!(
                    "No se pudo leer una entrada en {}: {error}",
                    current.display()
                ));
                continue;
            }
        };

        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                skipped_errors.push(format!(
                    "No se pudo leer metadata de {}: {error}",
                    path.display()
                ));
                continue;
            }
        };

        if metadata.is_dir() {
            if recursive && !is_converted_subfolder(root, &path) {
                collect_audio_files(root, &path, recursive, files, skipped_errors);
            }
            continue;
        }

        if !metadata.is_file() || !is_audio_path(&path) || is_inside_converted_folder(root, &path) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let parent_path = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis());

        files.push(AudioFile {
            name: name.to_string(),
            extension,
            path: path.to_string_lossy().into_owned(),
            parent_path,
            size_bytes: metadata.len(),
            modified_ms,
        });
    }
}

fn is_audio_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "aif" | "aiff" | "flac" | "mp3" | "wav" | "wave" | "m4a" | "alac" | "aac"
    )
}

fn is_converted_subfolder(root: &Path, path: &Path) -> bool {
    path != root
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("converted"))
}

fn is_inside_converted_folder(root: &Path, path: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| is_converted_subfolder(root, parent))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            system_status,
            import_rekordbox_xml,
            plan_conversion,
            export_rekordbox_xml,
            convert_tracks,
            list_converted_files,
            list_audio_files,
            playlist_tracks,
            reveal_path,
            open_parent_folder,
            get_openai_api_key_status,
            save_openai_api_key,
            clear_openai_api_key,
            get_audio_tool_settings,
            save_audio_tool_settings,
            get_language_settings,
            save_language_settings,
            local_conversion::local_conversion_list_items,
            local_conversion::local_conversion_list_groups,
            local_conversion::local_conversion_group_items,
            local_conversion::local_conversion_add_files,
            local_conversion::local_conversion_scan_folder,
            local_conversion::local_conversion_convert_items,
            local_conversion::local_conversion_delete_item,
            mastering::mastering_profiles,
            mastering::mastering_list_jobs,
            mastering::mastering_get_job,
            mastering::mastering_job_events,
            mastering::mastering_start_job,
            mastering::mastering_retry_job,
            mastering::mastering_delete_job,
            playlist_index::playlist_index_libraries,
            playlist_index::playlist_index_preview_xml,
            playlist_index::playlist_index_import_xml,
            playlist_index::playlist_index_library_playlists,
            playlist_index::playlist_index_delete_library,
            playlist_index::playlist_index_delete_playlists,
            playlist_index::playlist_index_delete_tracks,
            playlist_index::playlist_index_playlist_tracks,
            playlist_index::playlist_index_search_tracks,
            playlist_index::playlist_index_generate_embeddings,
            playlist_index::playlist_index_drafts,
            playlist_index::playlist_index_create_draft,
            playlist_index::playlist_index_add_tracks_to_draft,
            playlist_index::playlist_index_remove_draft_track,
            playlist_index::playlist_index_delete_draft,
            playlist_index::playlist_index_draft_tracks,
            playlist_index::playlist_index_export_draft_xml,
            turn::turn_list_jobs,
            turn::turn_get_job,
            turn::turn_job_events,
            turn::turn_start_job,
            turn::turn_retry_job,
            turn::turn_delete_job
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rau Studio");
}
