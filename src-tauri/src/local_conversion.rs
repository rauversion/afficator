use crate::system;
use aifficator_core::conversion::{ffmpeg_args, ConversionSettings};
use aifficator_core::validation::default_target_path;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const DB_FILE: &str = "aifficator.sqlite3";

#[derive(Debug, Clone, Serialize)]
pub struct LocalConversionItem {
    id: String,
    source_path: String,
    source_name: String,
    source_parent: String,
    extension: String,
    target_path: String,
    state: String,
    size_bytes: Option<u64>,
    modified_ms: Option<u128>,
    message: Option<String>,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    source_exists: bool,
    target_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalConversionGroup {
    id: String,
    kind: String,
    name: String,
    root_path: Option<String>,
    recursive: bool,
    item_count: usize,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct LocalConversionImportResponse {
    group: Option<LocalConversionGroup>,
    root_path: Option<String>,
    recursive: bool,
    items: Vec<LocalConversionItem>,
    skipped_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LocalConversionBatchResult {
    items: Vec<LocalConversionItem>,
    converted_total: usize,
    already_converted_total: usize,
    already_aiff_total: usize,
    failed_total: usize,
}

#[derive(Clone, Debug, Serialize)]
struct LocalConversionProgressEvent {
    item_id: String,
    name: String,
    source_path: String,
    target_path: String,
    status: String,
    message: Option<String>,
    percent: Option<f64>,
    elapsed_seconds: Option<f64>,
    speed: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct LocalConversionLogEvent {
    level: String,
    item_id: Option<String>,
    name: Option<String>,
    message: String,
}

#[tauri::command]
pub fn local_conversion_list_items(app: AppHandle) -> Result<Vec<LocalConversionItem>, String> {
    let conn = open_db(&app)?;
    list_items(&conn)
}

#[tauri::command]
pub fn local_conversion_list_groups(app: AppHandle) -> Result<Vec<LocalConversionGroup>, String> {
    let conn = open_db(&app)?;
    list_groups(&conn)
}

#[tauri::command]
pub fn local_conversion_group_items(
    app: AppHandle,
    group_id: String,
) -> Result<Vec<LocalConversionItem>, String> {
    let conn = open_db(&app)?;
    list_group_items(&conn, &group_id)
}

#[tauri::command]
pub fn local_conversion_add_files(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<LocalConversionImportResponse, String> {
    let conn = open_db(&app)?;
    let group = create_files_group(&conn, paths.len())?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    let mut skipped_errors = Vec::new();

    for raw_path in paths {
        if !seen.insert(raw_path.clone()) {
            continue;
        }

        match register_source_path(&conn, PathBuf::from(raw_path)) {
            Ok(Some(item)) => {
                link_group_item(&conn, &group.id, &item.id)?;
                items.push(item);
            }
            Ok(None) => {}
            Err(error) => skipped_errors.push(error),
        }
    }

    let group = get_group(&conn, &group.id)?.unwrap_or(group);
    Ok(LocalConversionImportResponse {
        group: Some(group),
        root_path: None,
        recursive: false,
        items,
        skipped_errors,
    })
}

#[tauri::command]
pub fn local_conversion_scan_folder(
    app: AppHandle,
    folder_path: String,
    recursive: bool,
) -> Result<LocalConversionImportResponse, String> {
    let root = PathBuf::from(folder_path);
    let metadata = fs::metadata(&root)
        .map_err(|error| format!("No se pudo leer la carpeta {}: {error}", root.display()))?;

    if !metadata.is_dir() {
        return Err(format!("El path no es una carpeta: {}", root.display()));
    }

    let conn = open_db(&app)?;
    let group = upsert_folder_group(&conn, &root, recursive)?;
    clear_group_items(&conn, &group.id)?;
    let mut paths = Vec::new();
    let mut skipped_errors = Vec::new();
    collect_audio_paths(&root, &root, recursive, &mut paths, &mut skipped_errors);
    paths.sort();

    let mut items = Vec::new();
    for path in paths {
        match register_source_path(&conn, path) {
            Ok(Some(item)) => {
                link_group_item(&conn, &group.id, &item.id)?;
                items.push(item);
            }
            Ok(None) => {}
            Err(error) => skipped_errors.push(error),
        }
    }

    let group = get_group(&conn, &group.id)?.unwrap_or(group);
    Ok(LocalConversionImportResponse {
        group: Some(group),
        root_path: Some(root.to_string_lossy().into_owned()),
        recursive,
        items,
        skipped_errors,
    })
}

#[tauri::command]
pub async fn local_conversion_convert_items(
    app: AppHandle,
    item_ids: Vec<String>,
    max_concurrency: Option<usize>,
) -> Result<LocalConversionBatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        convert_items_blocking(app, item_ids, max_concurrency)
    })
    .await
    .map_err(|error| format!("La conversion local fallo inesperadamente: {error}"))?
}

#[tauri::command]
pub fn local_conversion_delete_item(app: AppHandle, item_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    conn.execute(
        "DELETE FROM local_conversion_events WHERE item_id = ?1",
        params![&item_id],
    )
    .map_err(|error| format!("No se pudieron borrar eventos de conversion local: {error}"))?;
    conn.execute(
        "DELETE FROM local_conversion_group_items WHERE item_id = ?1",
        params![&item_id],
    )
    .map_err(|error| format!("No se pudo remover archivo de grupos locales: {error}"))?;
    conn.execute(
        "DELETE FROM local_conversion_items WHERE id = ?1",
        params![&item_id],
    )
    .map_err(|error| format!("No se pudo olvidar archivo local: {error}"))?;
    Ok(item_id)
}

fn convert_items_blocking(
    app: AppHandle,
    item_ids: Vec<String>,
    max_concurrency: Option<usize>,
) -> Result<LocalConversionBatchResult, String> {
    let conn = open_db(&app)?;
    let max_concurrency = max_concurrency.unwrap_or(1).clamp(1, 4);
    let mut seen = BTreeSet::new();
    let ordered_ids = item_ids
        .into_iter()
        .filter(|item_id| seen.insert(item_id.clone()))
        .collect::<Vec<_>>();
    let mut input_items = Vec::new();
    let mut output_items = Vec::new();

    emit_log(
        &app,
        LocalConversionLogEvent {
            level: "info".to_string(),
            item_id: None,
            name: None,
            message: format!(
                "Conversion local iniciada: {} archivo(s), concurrencia maxima {}",
                ordered_ids.len(),
                max_concurrency
            ),
        },
    );

    for item_id in ordered_ids {
        match get_item(&conn, &item_id)? {
            Some(item) => input_items.push(item),
            None => {
                emit_log(
                    &app,
                    LocalConversionLogEvent {
                        level: "error".to_string(),
                        item_id: Some(item_id.clone()),
                        name: None,
                        message: format!("Archivo local no encontrado en SQLite: {item_id}"),
                    },
                );
            }
        }
    }

    for chunk in input_items.chunks(max_concurrency) {
        let mut handles = Vec::new();

        for item in chunk.iter().cloned() {
            let app_handle = app.clone();
            handles.push((
                item.id.clone(),
                thread::spawn(move || convert_item(&app_handle, item)),
            ));
        }

        for (item_id, handle) in handles {
            match handle.join() {
                Ok(item) => output_items.push(item),
                Err(_) => {
                    let _ = update_item_state(
                        &app,
                        &item_id,
                        "failed",
                        Some("La conversion fallo por un panic interno"),
                        None,
                    );
                    if let Ok(conn) = open_db(&app) {
                        if let Ok(Some(item)) = get_item(&conn, &item_id) {
                            output_items.push(item);
                        }
                    }
                    emit_log(
                        &app,
                        LocalConversionLogEvent {
                            level: "error".to_string(),
                            item_id: Some(item_id),
                            name: None,
                            message: "La conversion fallo por un panic interno".to_string(),
                        },
                    );
                }
            }
        }
    }

    let result = LocalConversionBatchResult {
        converted_total: output_items
            .iter()
            .filter(|item| item.state == "converted")
            .count(),
        already_converted_total: output_items
            .iter()
            .filter(|item| item.state == "already_converted")
            .count(),
        already_aiff_total: output_items
            .iter()
            .filter(|item| item.state == "already_aiff")
            .count(),
        failed_total: output_items
            .iter()
            .filter(|item| item.state == "failed")
            .count(),
        items: output_items,
    };

    emit_log(
        &app,
        LocalConversionLogEvent {
            level: if result.failed_total > 0 {
                "warning".to_string()
            } else {
                "info".to_string()
            },
            item_id: None,
            name: None,
            message: format!(
                "Conversion local terminada: {} convertidos, {} existentes, {} AIFF originales, {} errores",
                result.converted_total,
                result.already_converted_total,
                result.already_aiff_total,
                result.failed_total
            ),
        },
    );

    Ok(result)
}

fn convert_item(app: &AppHandle, mut item: LocalConversionItem) -> LocalConversionItem {
    let _ = update_item_state(app, &item.id, "queued", Some("En cola"), None);
    item.state = "queued".to_string();
    item.message = Some("En cola".to_string());
    emit_progress(app, item_progress_event(&item, Some(0.0), None, None));
    emit_log(
        app,
        LocalConversionLogEvent {
            level: "info".to_string(),
            item_id: Some(item.id.clone()),
            name: Some(item.source_name.clone()),
            message: "En cola".to_string(),
        },
    );

    let source_path = PathBuf::from(&item.source_path);
    let target_path = PathBuf::from(&item.target_path);

    if !source_path.is_file() {
        item = fail_item(app, item, "Archivo fuente no encontrado");
        return item;
    }

    if is_aiff_path(&source_path) {
        let _ = update_item_state(
            app,
            &item.id,
            "already_aiff",
            Some("El original ya es AIFF"),
            None,
        );
        item.state = "already_aiff".to_string();
        item.message = Some("El original ya es AIFF".to_string());
        item.target_path = item.source_path.clone();
        item.target_exists = true;
        emit_progress(app, item_progress_event(&item, Some(100.0), None, None));
        emit_log(
            app,
            LocalConversionLogEvent {
                level: "info".to_string(),
                item_id: Some(item.id.clone()),
                name: Some(item.source_name.clone()),
                message: "Omitido: el original ya es AIFF".to_string(),
            },
        );
        return item;
    }

    if target_path.exists() {
        let _ = update_item_state(
            app,
            &item.id,
            "already_converted",
            Some("AIFF convertido ya existe"),
            Some(&target_path),
        );
        item.state = "already_converted".to_string();
        item.message = Some("AIFF convertido ya existe".to_string());
        item.target_exists = true;
        emit_progress(app, item_progress_event(&item, Some(100.0), None, None));
        emit_log(
            app,
            LocalConversionLogEvent {
                level: "info".to_string(),
                item_id: Some(item.id.clone()),
                name: Some(item.source_name.clone()),
                message: format!("Reutilizando AIFF existente: {}", target_path.display()),
            },
        );
        return item;
    }

    if let Some(parent) = target_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            item = fail_item(
                app,
                item,
                &format!("No se pudo crear la carpeta {}: {error}", parent.display()),
            );
            return item;
        }
    }

    let _ = update_item_state(
        app,
        &item.id,
        "running",
        Some("Convirtiendo con ffmpeg"),
        Some(&target_path),
    );
    item.state = "running".to_string();
    item.message = Some("Convirtiendo con ffmpeg".to_string());
    emit_progress(app, item_progress_event(&item, Some(0.0), Some(0.0), None));
    emit_log(
        app,
        LocalConversionLogEvent {
            level: "info".to_string(),
            item_id: Some(item.id.clone()),
            name: Some(item.source_name.clone()),
            message: format!(
                "ffmpeg iniciado: {} -> {}",
                source_path.display(),
                target_path.display()
            ),
        },
    );

    match run_ffmpeg_conversion(app, &item, &source_path, &target_path) {
        Ok(()) => {
            let _ = update_item_state(
                app,
                &item.id,
                "converted",
                Some("Conversion completada"),
                Some(&target_path),
            );
            item.state = "converted".to_string();
            item.message = Some("Conversion completada".to_string());
            item.target_exists = true;
            emit_progress(app, item_progress_event(&item, Some(100.0), None, None));
            emit_log(
                app,
                LocalConversionLogEvent {
                    level: "info".to_string(),
                    item_id: Some(item.id.clone()),
                    name: Some(item.source_name.clone()),
                    message: format!("Conversion completada: {}", target_path.display()),
                },
            );
        }
        Err(error) => {
            item = fail_item(app, item, &error);
        }
    }

    item
}

fn fail_item(app: &AppHandle, mut item: LocalConversionItem, message: &str) -> LocalConversionItem {
    let _ = update_item_state(app, &item.id, "failed", Some(message), None);
    item.state = "failed".to_string();
    item.message = Some(message.to_string());
    emit_progress(app, item_progress_event(&item, None, None, None));
    emit_log(
        app,
        LocalConversionLogEvent {
            level: "error".to_string(),
            item_id: Some(item.id.clone()),
            name: Some(item.source_name.clone()),
            message: message.to_string(),
        },
    );
    item
}

fn run_ffmpeg_conversion(
    app: &AppHandle,
    item: &LocalConversionItem,
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
            format!("No se pudo ejecutar ffmpeg. Revisa que este instalado en PATH: {error}")
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "No se pudo leer el progreso de ffmpeg".to_string())?;
    let stderr = child.stderr.take();
    let stderr_handle = stderr.map(|stderr| {
        let app = app.clone();
        let item_id = item.id.clone();
        let name = item.source_name.clone();
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

                emit_log(
                    &app,
                    LocalConversionLogEvent {
                        level: "info".to_string(),
                        item_id: Some(item_id.clone()),
                        name: Some(name.clone()),
                        message: format!("ffmpeg: {line}"),
                    },
                );
                lines.push(line);
            }

            lines.join("\n")
        })
    });

    let total_seconds = probe_duration_seconds(app, source_path);
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
                emit_progress(
                    app,
                    LocalConversionProgressEvent {
                        item_id: item.id.clone(),
                        name: item.source_name.clone(),
                        source_path: source_path.to_string_lossy().into_owned(),
                        target_path: target_path.to_string_lossy().into_owned(),
                        status: "running".to_string(),
                        message: Some(if value == "end" {
                            "Finalizando".to_string()
                        } else {
                            "Convirtiendo con ffmpeg".to_string()
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

    let status = child
        .wait()
        .map_err(|error| format!("No se pudo esperar a ffmpeg: {error}"))?;
    let stderr_output = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if !status.success() {
        return Err(format!(
            "ffmpeg fallo con estado {status}. {}",
            stderr_tail(&stderr_output)
        ));
    }

    if !target_path.exists() {
        return Err(format!(
            "ffmpeg termino sin generar el archivo {}",
            target_path.display()
        ));
    }

    Ok(())
}

fn register_source_path(
    conn: &Connection,
    source_path: PathBuf,
) -> Result<Option<LocalConversionItem>, String> {
    let metadata = fs::metadata(&source_path)
        .map_err(|error| format!("No se pudo leer {}: {error}", source_path.display()))?;

    if !metadata.is_file() {
        return Err(format!(
            "El path no es un archivo: {}",
            source_path.display()
        ));
    }

    if !is_audio_path(&source_path) {
        return Err(format!("Formato no soportado: {}", source_path.display()));
    }

    if is_inside_any_converted_folder(&source_path) {
        return Ok(None);
    }

    let source_path_text = source_path.to_string_lossy().into_owned();
    let source_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio")
        .to_string();
    let source_parent = source_path
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let target_path = if is_aiff_path(&source_path) {
        source_path.clone()
    } else {
        default_target_path(&source_path)
    };
    let target_path_text = target_path.to_string_lossy().into_owned();
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis());
    let now = timestamp();

    conn.execute(
        "INSERT INTO local_conversion_items (
            id, source_path, source_name, source_parent, extension, target_path, state,
            size_bytes, modified_ms, message, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, 'Pendiente', ?9, ?9)
         ON CONFLICT(source_path) DO UPDATE SET
            source_name = excluded.source_name,
            source_parent = excluded.source_parent,
            extension = excluded.extension,
            target_path = excluded.target_path,
            size_bytes = excluded.size_bytes,
            modified_ms = excluded.modified_ms,
            updated_at = excluded.updated_at",
        params![
            Uuid::new_v4().to_string(),
            source_path_text,
            source_name,
            source_parent,
            extension,
            target_path_text,
            metadata.len() as i64,
            modified_ms.map(|value| value as i64),
            now
        ],
    )
    .map_err(|error| format!("No se pudo guardar referencia local: {error}"))?;

    get_item_by_source(conn, &source_path.to_string_lossy())
}

fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join(DB_FILE))
        .map_err(|error| format!("No se pudo abrir SQLite local conversion: {error}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS local_conversion_items (
          id TEXT PRIMARY KEY,
          source_path TEXT NOT NULL UNIQUE,
          source_name TEXT NOT NULL,
          source_parent TEXT NOT NULL,
          extension TEXT NOT NULL,
          target_path TEXT NOT NULL,
          state TEXT NOT NULL,
          size_bytes INTEGER,
          modified_ms INTEGER,
          message TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_local_conversion_items_updated_at ON local_conversion_items(updated_at);
        CREATE INDEX IF NOT EXISTS idx_local_conversion_items_state ON local_conversion_items(state);

        CREATE TABLE IF NOT EXISTS local_conversion_groups (
          id TEXT PRIMARY KEY,
          kind TEXT NOT NULL,
          name TEXT NOT NULL,
          root_path TEXT,
          recursive INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_local_conversion_groups_updated_at ON local_conversion_groups(updated_at);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_local_conversion_groups_folder_unique
          ON local_conversion_groups(root_path, recursive)
          WHERE kind = 'folder';

        CREATE TABLE IF NOT EXISTS local_conversion_group_items (
          group_id TEXT NOT NULL,
          item_id TEXT NOT NULL,
          created_at TEXT NOT NULL,
          PRIMARY KEY (group_id, item_id),
          FOREIGN KEY(group_id) REFERENCES local_conversion_groups(id) ON DELETE CASCADE,
          FOREIGN KEY(item_id) REFERENCES local_conversion_items(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_local_conversion_group_items_item ON local_conversion_group_items(item_id);

        CREATE TABLE IF NOT EXISTS local_conversion_events (
          id TEXT PRIMARY KEY,
          item_id TEXT,
          level TEXT NOT NULL,
          message TEXT NOT NULL,
          payload_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          FOREIGN KEY(item_id) REFERENCES local_conversion_items(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_local_conversion_events_item_created_at ON local_conversion_events(item_id, created_at);
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite local conversion: {error}"))?;

    conn.execute(
        "UPDATE local_conversion_items SET state = 'pending', message = 'Pendiente' WHERE state = 'queued' AND message = 'Registrado'",
        [],
    )
    .map_err(|error| format!("No se pudo migrar estados locales pendientes: {error}"))?;

    Ok(())
}

fn create_files_group(
    conn: &Connection,
    requested_count: usize,
) -> Result<LocalConversionGroup, String> {
    let now = timestamp();
    let id = Uuid::new_v4().to_string();
    let name = format!("Seleccion manual - {} archivo(s)", requested_count);

    conn.execute(
        "INSERT INTO local_conversion_groups (id, kind, name, root_path, recursive, created_at, updated_at)
         VALUES (?1, 'files', ?2, NULL, 0, ?3, ?3)",
        params![id, name, now],
    )
    .map_err(|error| format!("No se pudo crear grupo de archivos: {error}"))?;

    get_group(conn, &id)?.ok_or_else(|| "No se pudo leer grupo creado.".to_string())
}

fn upsert_folder_group(
    conn: &Connection,
    root: &Path,
    recursive: bool,
) -> Result<LocalConversionGroup, String> {
    let root_path = root.to_string_lossy().into_owned();
    let recursive_i64 = if recursive { 1_i64 } else { 0_i64 };
    if let Some(group) = conn
        .query_row(
            "SELECT g.id, g.kind, g.name, g.root_path, g.recursive, COUNT(gi.item_id), g.created_at, g.updated_at
             FROM local_conversion_groups g
             LEFT JOIN local_conversion_group_items gi ON gi.group_id = g.id
             WHERE g.kind = 'folder' AND g.root_path = ?1 AND g.recursive = ?2
             GROUP BY g.id",
            params![&root_path, recursive_i64],
            row_to_group,
        )
        .optional()
        .map_err(|error| format!("No se pudo buscar grupo de carpeta: {error}"))?
    {
        let now = timestamp();
        conn.execute(
            "UPDATE local_conversion_groups SET updated_at = ?2 WHERE id = ?1",
            params![&group.id, now],
        )
        .map_err(|error| format!("No se pudo actualizar grupo de carpeta: {error}"))?;
        return get_group(conn, &group.id)?.ok_or_else(|| "No se pudo releer grupo.".to_string());
    }

    let now = timestamp();
    let id = Uuid::new_v4().to_string();
    let folder_name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Carpeta")
        .to_string();
    let name = if recursive {
        format!("{folder_name} (recursivo)")
    } else {
        folder_name
    };

    conn.execute(
        "INSERT INTO local_conversion_groups (id, kind, name, root_path, recursive, created_at, updated_at)
         VALUES (?1, 'folder', ?2, ?3, ?4, ?5, ?5)",
        params![&id, &name, &root_path, recursive_i64, &now],
    )
    .map_err(|error| format!("No se pudo crear grupo de carpeta: {error}"))?;

    get_group(conn, &id)?.ok_or_else(|| "No se pudo leer grupo de carpeta creado.".to_string())
}

fn clear_group_items(conn: &Connection, group_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM local_conversion_group_items WHERE group_id = ?1",
        params![group_id],
    )
    .map_err(|error| format!("No se pudo refrescar grupo local: {error}"))?;
    Ok(())
}

fn link_group_item(conn: &Connection, group_id: &str, item_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO local_conversion_group_items (group_id, item_id, created_at)
         VALUES (?1, ?2, ?3)",
        params![group_id, item_id, timestamp()],
    )
    .map_err(|error| format!("No se pudo asociar archivo al grupo: {error}"))?;
    Ok(())
}

fn list_groups(conn: &Connection) -> Result<Vec<LocalConversionGroup>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT g.id, g.kind, g.name, g.root_path, g.recursive, COUNT(gi.item_id), g.created_at, g.updated_at
             FROM local_conversion_groups g
             LEFT JOIN local_conversion_group_items gi ON gi.group_id = g.id
             GROUP BY g.id
             ORDER BY g.updated_at DESC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de grupos locales: {error}"))?;
    let rows = stmt
        .query_map([], row_to_group)
        .map_err(|error| format!("No se pudieron leer grupos locales: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear grupos locales: {error}"))
}

fn get_group(conn: &Connection, group_id: &str) -> Result<Option<LocalConversionGroup>, String> {
    conn.query_row(
        "SELECT g.id, g.kind, g.name, g.root_path, g.recursive, COUNT(gi.item_id), g.created_at, g.updated_at
         FROM local_conversion_groups g
         LEFT JOIN local_conversion_group_items gi ON gi.group_id = g.id
         WHERE g.id = ?1
         GROUP BY g.id",
        params![group_id],
        row_to_group,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer grupo local: {error}"))
}

fn list_group_items(conn: &Connection, group_id: &str) -> Result<Vec<LocalConversionItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.source_path, i.source_name, i.source_parent, i.extension, i.target_path, i.state,
                    i.size_bytes, i.modified_ms, i.message, i.created_at, i.updated_at, i.completed_at
             FROM local_conversion_items i
             INNER JOIN local_conversion_group_items gi ON gi.item_id = i.id
             WHERE gi.group_id = ?1
             ORDER BY i.source_path ASC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de archivos del grupo: {error}"))?;
    let rows = stmt
        .query_map(params![group_id], row_to_item)
        .map_err(|error| format!("No se pudieron leer archivos del grupo: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear archivos del grupo: {error}"))
}

fn row_to_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalConversionGroup> {
    let recursive: i64 = row.get(4)?;
    let item_count: i64 = row.get(5)?;
    Ok(LocalConversionGroup {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        root_path: row.get(3)?,
        recursive: recursive != 0,
        item_count: item_count.max(0) as usize,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn list_items(conn: &Connection) -> Result<Vec<LocalConversionItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_path, source_name, source_parent, extension, target_path, state,
                    size_bytes, modified_ms, message, created_at, updated_at, completed_at
             FROM local_conversion_items
             ORDER BY updated_at DESC",
        )
        .map_err(|error| format!("No se pudo preparar consulta local conversion: {error}"))?;
    let rows = stmt
        .query_map([], row_to_item)
        .map_err(|error| format!("No se pudo leer conversiones locales: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear conversiones locales: {error}"))
}

fn get_item(conn: &Connection, item_id: &str) -> Result<Option<LocalConversionItem>, String> {
    conn.query_row(
        "SELECT id, source_path, source_name, source_parent, extension, target_path, state,
                size_bytes, modified_ms, message, created_at, updated_at, completed_at
         FROM local_conversion_items
         WHERE id = ?1",
        params![item_id],
        row_to_item,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer item local: {error}"))
}

fn get_item_by_source(
    conn: &Connection,
    source_path: &str,
) -> Result<Option<LocalConversionItem>, String> {
    conn.query_row(
        "SELECT id, source_path, source_name, source_parent, extension, target_path, state,
                size_bytes, modified_ms, message, created_at, updated_at, completed_at
         FROM local_conversion_items
         WHERE source_path = ?1",
        params![source_path],
        row_to_item,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer item local por source: {error}"))
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalConversionItem> {
    let source_path: String = row.get(1)?;
    let target_path: String = row.get(5)?;
    let size_bytes: Option<i64> = row.get(7)?;
    let modified_ms: Option<i64> = row.get(8)?;

    Ok(LocalConversionItem {
        id: row.get(0)?,
        source_exists: Path::new(&source_path).is_file(),
        target_exists: Path::new(&target_path).is_file(),
        source_path,
        source_name: row.get(2)?,
        source_parent: row.get(3)?,
        extension: row.get(4)?,
        target_path,
        state: row.get(6)?,
        size_bytes: size_bytes.map(|value| value.max(0) as u64),
        modified_ms: modified_ms.map(|value| value.max(0) as u128),
        message: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

fn update_item_state(
    app: &AppHandle,
    item_id: &str,
    state: &str,
    message: Option<&str>,
    target_path: Option<&Path>,
) -> Result<(), String> {
    let conn = open_db(app)?;
    let now = timestamp();
    let completed_at = if matches!(state, "converted" | "already_converted" | "already_aiff") {
        Some(now.clone())
    } else {
        None
    };
    let target_path = target_path.map(|path| path.to_string_lossy().into_owned());

    conn.execute(
        "UPDATE local_conversion_items SET
          state = ?2,
          message = ?3,
          target_path = COALESCE(?4, target_path),
          updated_at = ?5,
          completed_at = CASE WHEN ?6 IS NULL THEN completed_at ELSE ?6 END
         WHERE id = ?1",
        params![item_id, state, message, target_path, now, completed_at],
    )
    .map_err(|error| format!("No se pudo actualizar estado local: {error}"))?;

    Ok(())
}

fn item_progress_event(
    item: &LocalConversionItem,
    percent: Option<f64>,
    elapsed_seconds: Option<f64>,
    speed: Option<String>,
) -> LocalConversionProgressEvent {
    LocalConversionProgressEvent {
        item_id: item.id.clone(),
        name: item.source_name.clone(),
        source_path: item.source_path.clone(),
        target_path: item.target_path.clone(),
        status: item.state.clone(),
        message: item.message.clone(),
        percent,
        elapsed_seconds,
        speed,
    }
}

fn emit_progress(app: &AppHandle, event: LocalConversionProgressEvent) {
    let _ = app.emit("local-conversion-progress", event);
}

fn emit_log(app: &AppHandle, event: LocalConversionLogEvent) {
    if let Ok(conn) = open_db(app) {
        let _ = conn.execute(
            "INSERT INTO local_conversion_events (id, item_id, level, message, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                event.item_id.clone(),
                event.level.clone(),
                event.message.clone(),
                json!({ "name": event.name }).to_string(),
                timestamp()
            ],
        );
    }

    let _ = app.emit("local-conversion-log", event);
}

fn collect_audio_paths(
    root: &Path,
    current: &Path,
    recursive: bool,
    paths: &mut Vec<PathBuf>,
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
                collect_audio_paths(root, &path, recursive, paths, skipped_errors);
            }
            continue;
        }

        if metadata.is_file() && is_audio_path(&path) && !is_inside_converted_folder(root, &path) {
            paths.push(path);
        }
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

fn is_aiff_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(extension.as_str(), "aif" | "aiff")
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

fn is_inside_any_converted_folder(path: &Path) -> bool {
    path.ancestors().skip(1).any(|ancestor| {
        ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("converted"))
    })
}

fn probe_duration_seconds(app: &AppHandle, source_path: &Path) -> Option<f64> {
    let output = system::ffprobe_command(app)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(source_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn parse_ffmpeg_progress_seconds(value: &str) -> Option<f64> {
    let micros = value.trim().parse::<f64>().ok()?;
    if !micros.is_finite() || micros < 0.0 {
        return None;
    }
    Some(micros / 1_000_000.0)
}

fn conversion_percent(elapsed_seconds: Option<f64>, total_seconds: Option<f64>) -> Option<f64> {
    let elapsed_seconds = elapsed_seconds?;
    let total_seconds = total_seconds?;
    if total_seconds <= 0.0 {
        return None;
    }
    Some(((elapsed_seconds / total_seconds) * 100.0).clamp(0.0, 100.0))
}

fn stderr_tail(stderr: &str) -> String {
    let lines = stderr
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

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}
