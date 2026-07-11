use crate::system;
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const OPENAI_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, Serialize)]
pub struct MasteringProfile {
    key: String,
    label_es: String,
    target_lufs: f64,
    true_peak_ceiling_db: f64,
    style_es: String,
    highpass_frequency_hz: f64,
    limiter_enabled: bool,
    limiter_max_gain_reduction_db: f64,
    already_mastered_limiter_max_gain_reduction_db: f64,
    loudness_correction_limit_db: f64,
    max_loudness_correction_passes: usize,
    minimum_crest_factor_db: Option<f64>,
    max_positive_gain_db: f64,
    loud_source_gain_cap_db: f64,
    true_peak_safety_margin_db: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringJob {
    id: String,
    source_path: String,
    source_name: String,
    target_profile: String,
    state: String,
    feedback: Option<String>,
    reference_notes: Option<String>,
    output_format: String,
    metadata: Value,
    cover_art_path: Option<String>,
    output_path: Option<String>,
    package_report: Value,
    recipe: Value,
    analysis_before: Value,
    analysis_after: Value,
    error_message: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    failed_at: Option<String>,
    created_at: String,
    updated_at: String,
    ready: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MasteringMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    year: Option<String>,
    track_number: Option<String>,
    composer: Option<String>,
    label: Option<String>,
    copyright: Option<String>,
    bpm: Option<String>,
    musical_key: Option<String>,
    isrc: Option<String>,
    comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MasteringProgressEvent {
    #[serde(rename = "type")]
    event_type: String,
    id: String,
    job_id: String,
    event: String,
    step: String,
    level: String,
    message: String,
    progress: Option<f64>,
    timestamp: String,
    job: MasteringJob,
    payload: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AudioAnalysis {
    duration_sec: Option<f64>,
    sample_rate_hz: Option<u32>,
    channels: Option<u32>,
    integrated_lufs: Option<f64>,
    true_peak_dbfs: Option<f64>,
    sample_peak_dbfs: Option<f64>,
    clipping_detected: Option<bool>,
    dc_offset: Option<f64>,
    crest_factor_db: Option<f64>,
    spectral_notes: BTreeMap<String, Value>,
}

#[tauri::command]
pub fn mastering_profiles() -> Vec<MasteringProfile> {
    target_profiles()
}

#[tauri::command]
pub fn mastering_list_jobs(app: AppHandle) -> Result<Vec<MasteringJob>, String> {
    let conn = open_db(&app)?;
    list_jobs(&conn)
}

#[tauri::command]
pub fn mastering_get_job(app: AppHandle, job_id: String) -> Result<MasteringJob, String> {
    let conn = open_db(&app)?;
    get_job(&conn, &job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))
}

#[tauri::command]
pub fn mastering_job_events(
    app: AppHandle,
    job_id: String,
) -> Result<Vec<MasteringProgressEvent>, String> {
    let conn = open_db(&app)?;
    let job =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    list_job_events(&conn, &job)
}

#[tauri::command]
pub fn mastering_start_job(
    app: AppHandle,
    source_path: String,
    target_profile: String,
    feedback: Option<String>,
    reference_notes: Option<String>,
    output_format: Option<String>,
    metadata: Option<MasteringMetadata>,
    cover_art_path: Option<String>,
    use_ai: Option<bool>,
) -> Result<MasteringJob, String> {
    let source = PathBuf::from(&source_path);
    if !source.is_file() {
        return Err(format!(
            "Archivo de audio no encontrado: {}",
            source.display()
        ));
    }

    let target_profile = normalize_profile_key(&target_profile);
    let conn = open_db(&app)?;
    let now = timestamp();
    let id = Uuid::new_v4().to_string();
    let source_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio")
        .to_string();
    let output_format = normalize_output_format(output_format.as_deref());
    let metadata_json = normalized_metadata_json(&source_name, metadata);
    let cover_art_path = clean_cover_art_path(cover_art_path)?;

    conn.execute(
        "INSERT INTO mastering_jobs (
            id, source_path, source_name, target_profile, state, feedback, reference_notes,
            output_format, metadata_json, cover_art_path,
            recipe_json, analysis_before_json, analysis_after_json, package_report_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, '{}', '{}', '{}', '{}', ?10, ?10)",
        params![
            id,
            source_path,
            source_name,
            target_profile,
            clean_optional(feedback),
            clean_optional(reference_notes),
            output_format,
            metadata_json.to_string(),
            cover_art_path,
            now
        ],
    )
    .map_err(|error| format!("No se pudo crear mastering job: {error}"))?;

    let job =
        get_job(&conn, &id)?.ok_or_else(|| "No se pudo leer mastering job creado.".to_string())?;
    spawn_mastering(app, id, use_ai.unwrap_or(true));
    Ok(job)
}

#[tauri::command]
pub fn mastering_retry_job(
    app: AppHandle,
    job_id: String,
    feedback: Option<String>,
    reference_notes: Option<String>,
    output_format: Option<String>,
    metadata: Option<MasteringMetadata>,
    cover_art_path: Option<String>,
    use_ai: Option<bool>,
) -> Result<MasteringJob, String> {
    let conn = open_db(&app)?;
    let current =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    if current.state == "pending" || current.state == "running" {
        return Err("El master ya esta en proceso.".to_string());
    }

    let _ = fs::remove_dir_all(job_dir(&app, &job_id)?);
    let now = timestamp();
    let next_output_format = output_format
        .as_deref()
        .map(|value| normalize_output_format(Some(value)))
        .unwrap_or_else(|| normalize_output_format(Some(&current.output_format)));
    let next_metadata = metadata
        .map(|metadata| normalized_metadata_json(&current.source_name, Some(metadata)))
        .unwrap_or_else(|| current.metadata.clone());
    let next_cover_art_path = match cover_art_path {
        Some(value) => clean_cover_art_path(Some(value))?,
        None => current.cover_art_path.clone(),
    };
    conn.execute(
        "DELETE FROM mastering_events WHERE job_id = ?1",
        params![&job_id],
    )
    .map_err(|error| format!("No se pudieron limpiar eventos del master: {error}"))?;
    conn.execute(
        "UPDATE mastering_jobs SET
            state = 'pending',
            feedback = COALESCE(?2, feedback),
            reference_notes = COALESCE(?3, reference_notes),
            output_format = ?4,
            metadata_json = ?5,
            cover_art_path = ?6,
            output_path = NULL,
            recipe_json = '{}',
            analysis_before_json = '{}',
            analysis_after_json = '{}',
            package_report_json = '{}',
            error_message = NULL,
            started_at = NULL,
            completed_at = NULL,
            failed_at = NULL,
            updated_at = ?7
         WHERE id = ?1",
        params![
            &job_id,
            clean_optional(feedback),
            clean_optional(reference_notes),
            next_output_format,
            next_metadata.to_string(),
            next_cover_art_path,
            now
        ],
    )
    .map_err(|error| format!("No se pudo reintentar master: {error}"))?;

    let job =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    spawn_mastering(app, job_id, use_ai.unwrap_or(true));
    Ok(job)
}

#[tauri::command]
pub fn mastering_delete_job(app: AppHandle, job_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    let job =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    if job.state == "pending" || job.state == "running" {
        return Err("No se puede eliminar un master en proceso.".to_string());
    }

    let _ = fs::remove_dir_all(job_dir(&app, &job_id)?);
    conn.execute(
        "DELETE FROM mastering_events WHERE job_id = ?1",
        params![job_id],
    )
    .map_err(|error| format!("No se pudieron borrar eventos del master: {error}"))?;
    conn.execute("DELETE FROM mastering_jobs WHERE id = ?1", params![job_id])
        .map_err(|error| format!("No se pudo borrar master: {error}"))?;

    Ok(job.id)
}

fn spawn_mastering(app: AppHandle, job_id: String, use_ai: bool) {
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = run_mastering_job(&app, &job_id, use_ai) {
            if let Ok(conn) = open_db(&app) {
                let _ = mark_failed(&conn, &job_id, &error);
                if let Ok(Some(job)) = get_job(&conn, &job_id) {
                    let _ = emit_event(
                        &app,
                        &conn,
                        &job,
                        EventMeta {
                            event: "failed",
                            step: "failed",
                            level: "error",
                            progress: None,
                            message: format!("El master fallo: {error}"),
                            payload: json!({ "error": error }),
                        },
                    );
                }
            }
        }
    });
}

fn run_mastering_job(app: &AppHandle, job_id: &str, use_ai: bool) -> Result<(), String> {
    let conn = open_db(app)?;
    get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;

    mark_running(&conn, job_id)?;
    let mut job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "started",
            step: "queue",
            level: "info",
            progress: Some(5.0),
            message: format!("Master iniciado para {}.", job.source_name),
            payload: json!({}),
        },
    )?;

    if !Path::new(&job.source_path).is_file() {
        return Err(format!("Audio fuente no encontrado: {}", job.source_path));
    }
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "source_ready",
            step: "source",
            level: "info",
            progress: Some(10.0),
            message: format!("Audio fuente localizado: {}.", job.source_name),
            payload: json!({}),
        },
    )?;

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "analysis_before_started",
            step: "analysis_before",
            level: "info",
            progress: Some(20.0),
            message: "Analizando loudness, peaks, rango dinamico y clipping del archivo original."
                .to_string(),
            payload: json!({}),
        },
    )?;
    let analysis_before = analyze_audio(app, Path::new(&job.source_path))?;
    let analysis_before_json = serde_json::to_value(&analysis_before)
        .map_err(|error| format!("No se pudo serializar analisis inicial: {error}"))?;
    save_json_field(&conn, job_id, "analysis_before_json", &analysis_before_json)?;
    job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "analysis_before_finished",
            step: "analysis_before",
            level: "info",
            progress: Some(35.0),
            message: analysis_summary(&analysis_before),
            payload: json!({ "analysis_before": analysis_before_json }),
        },
    )?;

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "recipe_started",
            step: "recipe",
            level: "info",
            progress: Some(40.0),
            message: "Generando receta de mastering con presets y feedback.".to_string(),
            payload: json!({ "use_ai": use_ai }),
        },
    )?;
    let mut recipe = generate_recipe(app, &conn, &job, &analysis_before, use_ai)?;
    save_json_field(&conn, job_id, "recipe_json", &recipe)?;
    job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    let recipe_message = recipe
        .pointer("/diagnosis/summary_es")
        .and_then(Value::as_str)
        .unwrap_or("Receta generada.")
        .to_string();
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "recipe_finished",
            step: "recipe",
            level: "info",
            progress: Some(50.0),
            message: recipe_message,
            payload: json!({ "recipe": recipe }),
        },
    )?;

    let dir = job_dir(app, job_id)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("No se pudo crear carpeta de master: {error}"))?;

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "render_started",
            step: "render",
            level: "info",
            progress: Some(60.0),
            message: "Aplicando cadena DSP y renderizando WAV 24-bit.".to_string(),
            payload: json!({}),
        },
    )?;
    let mut output_path = render_master(
        app,
        Path::new(&job.source_path),
        &dir,
        0,
        &recipe,
        &analysis_before,
    )?;
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "render_finished",
            step: "render",
            level: "info",
            progress: Some(75.0),
            message: "Render WAV completado; verificando resultado.".to_string(),
            payload: json!({ "output_path": output_path.to_string_lossy() }),
        },
    )?;

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "analysis_after_started",
            step: "analysis_after",
            level: "info",
            progress: Some(82.0),
            message: "Reanalizando master para confirmar LUFS, true peak y clipping.".to_string(),
            payload: json!({}),
        },
    )?;
    let mut analysis_after = analyze_audio(app, &output_path)?;
    let mut analysis_after_json = serde_json::to_value(&analysis_after)
        .map_err(|error| format!("No se pudo serializar analisis final: {error}"))?;
    save_json_field(&conn, job_id, "analysis_after_json", &analysis_after_json)?;
    job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "analysis_after_finished",
            step: "analysis_after",
            level: "info",
            progress: Some(90.0),
            message: analysis_summary(&analysis_after),
            payload: json!({ "analysis_after": analysis_after_json }),
        },
    )?;

    let mut correction_pass = 0usize;
    while correction_pass < max_loudness_correction_passes(&recipe) {
        let correction_db = loudness_correction_db(&recipe, &analysis_after);
        if correction_db <= 0.0 {
            break;
        }

        correction_pass += 1;
        let loudness_offset_db = current_loudness_offset_db(&recipe) + correction_db;
        emit_event(
            app,
            &conn,
            &job,
            EventMeta {
                event: "loudness_correction_started",
                step: "loudness_correction",
                level: "info",
                progress: Some(92.0),
                message: format!(
                    "El master quedo bajo el target; aplicando pasada {correction_pass} con offset +{:.2} dB.",
                    loudness_offset_db
                ),
                payload: json!({ "loudness_offset_db": loudness_offset_db }),
            },
        )?;

        let corrected_recipe =
            recipe_with_loudness_offset(&recipe, loudness_offset_db, correction_pass);
        let corrected_output = render_master(
            app,
            Path::new(&job.source_path),
            &dir,
            correction_pass,
            &corrected_recipe,
            &analysis_before,
        )?;
        let corrected_analysis = analyze_audio(app, &corrected_output)?;

        if unsafe_analysis(&corrected_recipe, &corrected_analysis)
            || overcompressed_analysis(&corrected_recipe, &analysis_before, &corrected_analysis)
        {
            let _ = fs::remove_file(&corrected_output);
            emit_event(
                app,
                &conn,
                &job,
                EventMeta {
                    event: "loudness_correction_stopped",
                    step: "loudness_correction",
                    level: "warning",
                    progress: Some(94.0),
                    message: format!(
                        "Correccion detenida para preservar true peak y transientes: {}.",
                        analysis_summary(&corrected_analysis)
                    ),
                    payload: json!({}),
                },
            )?;
            break;
        }

        let _ = fs::remove_file(&output_path);
        output_path = corrected_output;
        recipe = corrected_recipe;
        analysis_after = corrected_analysis;
        analysis_after_json = serde_json::to_value(&analysis_after)
            .map_err(|error| format!("No se pudo serializar correccion: {error}"))?;
        save_json_field(&conn, job_id, "recipe_json", &recipe)?;
        save_json_field(&conn, job_id, "analysis_after_json", &analysis_after_json)?;
        emit_event(
            app,
            &conn,
            &job,
            EventMeta {
                event: "loudness_correction_finished",
                step: "loudness_correction",
                level: "info",
                progress: Some(94.0),
                message: format!(
                    "Pasada {correction_pass} completada: {}.",
                    analysis_summary(&analysis_after)
                ),
                payload: json!({ "analysis_after": analysis_after_json }),
            },
        )?;
    }

    job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    let final_path = final_output_path(&dir, &job);
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "packaging_started",
            step: "packaging",
            level: "info",
            progress: Some(96.0),
            message: format!(
                "Empaquetando master final como {} con metadata.",
                output_format_label(&job.output_format)
            ),
            payload: json!({
                "output_format": job.output_format.clone(),
                "cover_art_path": job.cover_art_path.clone()
            }),
        },
    )?;
    let package_report = package_master(app, &output_path, &final_path, &job, &recipe)?;
    let _ = fs::remove_file(&output_path);
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "packaging_finished",
            step: "packaging",
            level: package_event_level(&package_report),
            progress: Some(98.0),
            message: package_summary(&package_report),
            payload: json!({ "package_report": package_report.clone() }),
        },
    )?;
    write_sidecar_json(&dir, "recipe.json", &recipe)?;
    write_sidecar_json(&dir, "analysis_before.json", &analysis_before_json)?;
    write_sidecar_json(&dir, "analysis_after.json", &analysis_after_json)?;
    write_sidecar_json(&dir, "metadata.json", &job.metadata)?;
    write_sidecar_json(&dir, "package_report.json", &package_report)?;
    mark_completed(
        &conn,
        job_id,
        &final_path.to_string_lossy(),
        &recipe,
        &analysis_before_json,
        &analysis_after_json,
        &package_report,
    )?;
    job =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Mastering job no encontrado: {job_id}"))?;
    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "completed",
            step: "completed",
            level: "info",
            progress: Some(100.0),
            message: "Master listo para reproducir y descargar.".to_string(),
            payload: json!({}),
        },
    )?;

    Ok(())
}

struct EventMeta {
    event: &'static str,
    step: &'static str,
    level: &'static str,
    progress: Option<f64>,
    message: String,
    payload: Value,
}

fn emit_event(
    app: &AppHandle,
    conn: &Connection,
    job: &MasteringJob,
    meta: EventMeta,
) -> Result<(), String> {
    let now = timestamp();
    let event_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO mastering_events (id, job_id, event, step, level, message, progress, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &event_id,
            &job.id,
            meta.event,
            meta.step,
            meta.level,
            meta.message,
            meta.progress,
            meta.payload.to_string(),
            now
        ],
    )
    .map_err(|error| format!("No se pudo guardar evento de mastering: {error}"))?;

    let payload = MasteringProgressEvent {
        event_type: "mastering_progress".to_string(),
        id: event_id,
        job_id: job.id.clone(),
        event: meta.event.to_string(),
        step: meta.step.to_string(),
        level: meta.level.to_string(),
        message: meta.message,
        progress: meta.progress,
        timestamp: now,
        job: job.clone(),
        payload: meta.payload,
    };

    app.emit("mastering-progress", payload)
        .map_err(|error| format!("No se pudo emitir evento mastering-progress: {error}"))
}

fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join("aifficator.sqlite3"))
        .map_err(|error| format!("No se pudo abrir SQLite: {error}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS mastering_jobs (
          id TEXT PRIMARY KEY,
          source_path TEXT NOT NULL,
          source_name TEXT NOT NULL,
          target_profile TEXT NOT NULL,
          state TEXT NOT NULL,
          feedback TEXT,
          reference_notes TEXT,
          output_format TEXT NOT NULL DEFAULT 'aiff_24',
          metadata_json TEXT NOT NULL DEFAULT '{}',
          cover_art_path TEXT,
          output_path TEXT,
          package_report_json TEXT NOT NULL DEFAULT '{}',
          recipe_json TEXT NOT NULL DEFAULT '{}',
          analysis_before_json TEXT NOT NULL DEFAULT '{}',
          analysis_after_json TEXT NOT NULL DEFAULT '{}',
          error_message TEXT,
          started_at TEXT,
          completed_at TEXT,
          failed_at TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mastering_jobs_created_at ON mastering_jobs(created_at);
        CREATE INDEX IF NOT EXISTS idx_mastering_jobs_state ON mastering_jobs(state);
        CREATE INDEX IF NOT EXISTS idx_mastering_jobs_target_profile ON mastering_jobs(target_profile);

        CREATE TABLE IF NOT EXISTS mastering_events (
          id TEXT PRIMARY KEY,
          job_id TEXT NOT NULL,
          event TEXT NOT NULL,
          step TEXT NOT NULL,
          level TEXT NOT NULL,
          message TEXT NOT NULL,
          progress REAL,
          payload_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          FOREIGN KEY(job_id) REFERENCES mastering_jobs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_mastering_events_job_created_at ON mastering_events(job_id, created_at);
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite mastering: {error}"))?;

    ensure_mastering_column(conn, "output_format", "TEXT NOT NULL DEFAULT 'aiff_24'")?;
    ensure_mastering_column(conn, "metadata_json", "TEXT NOT NULL DEFAULT '{}'")?;
    ensure_mastering_column(conn, "cover_art_path", "TEXT")?;
    ensure_mastering_column(conn, "package_report_json", "TEXT NOT NULL DEFAULT '{}'")?;

    Ok(())
}

fn ensure_mastering_column(
    conn: &Connection,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(mastering_jobs)")
        .map_err(|error| format!("No se pudo inspeccionar mastering_jobs: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("No se pudieron leer columnas de mastering_jobs: {error}"))?;
    let columns = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear columnas de mastering_jobs: {error}"))?;

    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    let sql = format!("ALTER TABLE mastering_jobs ADD COLUMN {column} {definition}");
    conn.execute(&sql, [])
        .map_err(|error| format!("No se pudo agregar columna {column}: {error}"))?;
    Ok(())
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn job_dir(app: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?
        .join("mastering")
        .join("jobs")
        .join(job_id))
}

fn list_jobs(conn: &Connection) -> Result<Vec<MasteringJob>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_path, source_name, target_profile, state, feedback, reference_notes,
                    output_format, metadata_json, cover_art_path, output_path, package_report_json,
                    recipe_json, analysis_before_json, analysis_after_json, error_message,
                    started_at, completed_at, failed_at, created_at, updated_at
             FROM mastering_jobs
             ORDER BY created_at DESC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de masters: {error}"))?;

    let rows = stmt
        .query_map([], row_to_job)
        .map_err(|error| format!("No se pudo leer masters: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo mapear masters: {error}"))
}

fn get_job(conn: &Connection, job_id: &str) -> Result<Option<MasteringJob>, String> {
    conn.query_row(
        "SELECT id, source_path, source_name, target_profile, state, feedback, reference_notes,
                output_format, metadata_json, cover_art_path, output_path, package_report_json,
                recipe_json, analysis_before_json, analysis_after_json, error_message,
                started_at, completed_at, failed_at, created_at, updated_at
         FROM mastering_jobs
         WHERE id = ?1",
        params![job_id],
        row_to_job,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer mastering job: {error}"))
}

fn list_job_events(
    conn: &Connection,
    job: &MasteringJob,
) -> Result<Vec<MasteringProgressEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_id, event, step, level, message, progress, payload_json, created_at
             FROM mastering_events
             WHERE job_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de eventos: {error}"))?;

    let rows = stmt
        .query_map(params![&job.id], |row| {
            let payload_json: String = row.get(7)?;
            Ok(MasteringProgressEvent {
                event_type: "mastering_progress".to_string(),
                id: row.get(0)?,
                job_id: row.get(1)?,
                event: row.get(2)?,
                step: row.get(3)?,
                level: row.get(4)?,
                message: row.get(5)?,
                progress: row.get(6)?,
                payload: parse_json_text(payload_json.as_str()),
                timestamp: row.get(8)?,
                job: job.clone(),
            })
        })
        .map_err(|error| format!("No se pudieron leer eventos de mastering: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear eventos de mastering: {error}"))
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<MasteringJob> {
    let output_path: Option<String> = row.get(10)?;
    let state: String = row.get(4)?;
    Ok(MasteringJob {
        id: row.get(0)?,
        source_path: row.get(1)?,
        source_name: row.get(2)?,
        target_profile: row.get(3)?,
        state: state.clone(),
        feedback: row.get(5)?,
        reference_notes: row.get(6)?,
        output_format: row.get(7)?,
        metadata: parse_json_text(row.get::<_, String>(8)?.as_str()),
        cover_art_path: row.get(9)?,
        ready: state == "completed" && output_path.is_some(),
        output_path,
        package_report: parse_json_text(row.get::<_, String>(11)?.as_str()),
        recipe: parse_json_text(row.get::<_, String>(12)?.as_str()),
        analysis_before: parse_json_text(row.get::<_, String>(13)?.as_str()),
        analysis_after: parse_json_text(row.get::<_, String>(14)?.as_str()),
        error_message: row.get(15)?,
        started_at: row.get(16)?,
        completed_at: row.get(17)?,
        failed_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn parse_json_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({}))
}

fn save_json_field(
    conn: &Connection,
    job_id: &str,
    field: &str,
    value: &Value,
) -> Result<(), String> {
    let field = match field {
        "recipe_json" => "recipe_json",
        "analysis_before_json" => "analysis_before_json",
        "analysis_after_json" => "analysis_after_json",
        _ => return Err("Campo JSON no permitido.".to_string()),
    };
    let sql = format!("UPDATE mastering_jobs SET {field} = ?2, updated_at = ?3 WHERE id = ?1");
    conn.execute(&sql, params![job_id, value.to_string(), timestamp()])
        .map_err(|error| format!("No se pudo guardar JSON de mastering: {error}"))?;
    Ok(())
}

fn mark_running(conn: &Connection, job_id: &str) -> Result<(), String> {
    let now = timestamp();
    conn.execute(
        "UPDATE mastering_jobs SET state = 'running', started_at = ?2, failed_at = NULL, error_message = NULL, updated_at = ?2 WHERE id = ?1",
        params![job_id, now],
    )
    .map_err(|error| format!("No se pudo marcar master running: {error}"))?;
    Ok(())
}

fn mark_completed(
    conn: &Connection,
    job_id: &str,
    output_path: &str,
    recipe: &Value,
    analysis_before: &Value,
    analysis_after: &Value,
    package_report: &Value,
) -> Result<(), String> {
    let now = timestamp();
    conn.execute(
        "UPDATE mastering_jobs SET
          state = 'completed',
          output_path = ?2,
          recipe_json = ?3,
          analysis_before_json = ?4,
          analysis_after_json = ?5,
          package_report_json = ?6,
          completed_at = ?7,
          failed_at = NULL,
          error_message = NULL,
          updated_at = ?7
         WHERE id = ?1",
        params![
            job_id,
            output_path,
            recipe.to_string(),
            analysis_before.to_string(),
            analysis_after.to_string(),
            package_report.to_string(),
            now
        ],
    )
    .map_err(|error| format!("No se pudo marcar master completed: {error}"))?;
    Ok(())
}

fn mark_failed(conn: &Connection, job_id: &str, message: &str) -> Result<(), String> {
    let now = timestamp();
    let bounded = message.chars().take(1000).collect::<String>();
    conn.execute(
        "UPDATE mastering_jobs SET state = 'failed', failed_at = ?2, error_message = ?3, updated_at = ?2 WHERE id = ?1",
        params![job_id, now, bounded],
    )
    .map_err(|error| format!("No se pudo marcar master failed: {error}"))?;
    Ok(())
}

fn analyze_audio(app: &AppHandle, input_path: &Path) -> Result<AudioAnalysis, String> {
    let metadata = probe_metadata(app, input_path)?;
    let (integrated_lufs, ebur_true_peak) = analyze_ebur128(app, input_path)?;
    let (dc_offset, sample_peak_dbfs, crest_factor_db) = analyze_astats(app, input_path)?;
    let true_peak_dbfs = ebur_true_peak.or(sample_peak_dbfs);
    let clipping_detected = [sample_peak_dbfs, true_peak_dbfs]
        .into_iter()
        .flatten()
        .any(|peak| peak >= -0.1);

    Ok(AudioAnalysis {
        duration_sec: metadata.duration_sec,
        sample_rate_hz: metadata.sample_rate_hz,
        channels: metadata.channels,
        integrated_lufs,
        true_peak_dbfs,
        sample_peak_dbfs,
        clipping_detected: Some(clipping_detected),
        dc_offset,
        crest_factor_db,
        spectral_notes: BTreeMap::new(),
    })
}

#[derive(Default)]
struct ProbeMetadata {
    duration_sec: Option<f64>,
    sample_rate_hz: Option<u32>,
    channels: Option<u32>,
}

fn probe_metadata(app: &AppHandle, input_path: &Path) -> Result<ProbeMetadata, String> {
    let output = system::ffprobe_command(app)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,sample_rate,channels",
            "-of",
            "json",
        ])
        .arg(input_path)
        .output()
        .map_err(|error| format!("No se pudo ejecutar ffprobe: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe no pudo leer metadata: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("ffprobe retorno JSON invalido: {error}"))?;
    let audio_stream = parsed
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"))
        });
    let format = parsed.get("format");

    Ok(ProbeMetadata {
        duration_sec: format
            .and_then(|value| value.get("duration"))
            .and_then(value_to_f64)
            .map(round2),
        sample_rate_hz: audio_stream
            .and_then(|stream| stream.get("sample_rate"))
            .and_then(value_to_u32),
        channels: audio_stream
            .and_then(|stream| stream.get("channels"))
            .and_then(value_to_u32),
    })
}

fn analyze_ebur128(
    app: &AppHandle,
    input_path: &Path,
) -> Result<(Option<f64>, Option<f64>), String> {
    let output = system::ffmpeg_command(app)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(input_path)
        .args(["-filter_complex", "ebur128=peak=true", "-f", "null", "-"])
        .output()
        .map_err(|error| format!("No se pudo ejecutar ffmpeg ebur128: {error}"))?;

    if !output.status.success() {
        return Ok((None, None));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = stderr.split("Summary:").last().unwrap_or_default();
    let integrated = Regex::new(r"I:\s*(-?\d+(?:\.\d+)?)\s*LUFS")
        .ok()
        .and_then(|regex| regex.captures(summary))
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<f64>().ok());
    let peak_regex = Regex::new(r"Peak:\s*(-?\d+(?:\.\d+)?)\s*dBFS")
        .map_err(|error| format!("Regex ebur128 invalida: {error}"))?;
    let true_peak = peak_regex
        .captures_iter(summary)
        .filter_map(|captures| {
            captures
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok())
        })
        .last();

    Ok((integrated, true_peak))
}

fn analyze_astats(
    app: &AppHandle,
    input_path: &Path,
) -> Result<(Option<f64>, Option<f64>, Option<f64>), String> {
    let output = system::ffmpeg_command(app)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(input_path)
        .args(["-af", "astats=metadata=1:reset=0", "-f", "null", "-"])
        .output()
        .map_err(|error| format!("No se pudo ejecutar ffmpeg astats: {error}"))?;

    if !output.status.success() {
        return Ok((None, None, None));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let dc_offset = last_number(&stderr, r"DC offset:\s*(-?\d+(?:\.\d+)?(?:e[+-]?\d+)?)")
        .map(|value| (value * 1_000_000.0).round() / 1_000_000.0);
    let sample_peak = last_number(&stderr, r"Peak level dB:\s*(-?(?:inf|\d+(?:\.\d+)?))");
    let crest_factor = last_number(&stderr, r"Crest factor:\s*(\d+(?:\.\d+)?)");
    let crest_factor_db = crest_factor
        .filter(|value| *value > 0.0)
        .map(|value| round2(20.0 * value.log10()));

    Ok((dc_offset, sample_peak, crest_factor_db))
}

fn last_number(text: &str, pattern: &str) -> Option<f64> {
    let regex = Regex::new(pattern).ok()?;
    regex
        .captures_iter(text)
        .filter_map(|captures| captures.get(1))
        .filter_map(|value| {
            let text = value.as_str();
            if text.to_ascii_lowercase().ends_with("inf") {
                None
            } else {
                text.parse::<f64>().ok()
            }
        })
        .last()
}

fn generate_recipe(
    app: &AppHandle,
    conn: &Connection,
    job: &MasteringJob,
    analysis: &AudioAnalysis,
    use_ai: bool,
) -> Result<Value, String> {
    let profile = fetch_profile(&job.target_profile);
    let feedback = job.feedback.clone().unwrap_or_default();
    let reference_notes = job.reference_notes.clone().unwrap_or_default();
    let feedback_interpretation = feedback_interpretation(
        app,
        conn,
        job,
        &feedback,
        &reference_notes,
        analysis,
        &profile,
        use_ai,
    )?;
    let policy = mastering_policy(
        app,
        conn,
        job,
        &feedback,
        &reference_notes,
        analysis,
        &profile,
        use_ai,
    )?;
    let already_mastered = already_mastered(analysis);
    let risk = risk_level(analysis, &profile);
    let eq_bands = feedback_interpretation
        .get("eq_bands")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let lowpass = feedback_interpretation
        .get("lowpass_filter")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let saturation = feedback_interpretation
        .get("saturation")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let compressor_enabled =
        !already_mastered && analysis.crest_factor_db.is_some_and(|crest| crest > 13.0);
    let requested_saturation = saturation
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let saturation_enabled =
        requested_saturation && profile.key != "vinyl_premaster" && risk != "high";
    let limiter_gr = policy
        .get("limiter_max_gain_reduction_db")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| limiter_gain_reduction(&profile, already_mastered));

    let warnings = recipe_warnings(analysis, &profile, &feedback_interpretation, &policy);
    let main_issues = main_issues(analysis, &feedback);
    let sample_rate = analysis.sample_rate_hz.unwrap_or(44_100);

    Ok(json!({
        "diagnosis": {
            "summary_es": diagnosis_summary(analysis),
            "risk_level": risk,
            "already_mastered": already_mastered,
            "main_issues": main_issues
        },
        "target": {
            "profile": profile.key,
            "target_lufs": profile.target_lufs,
            "true_peak_ceiling_db": profile.true_peak_ceiling_db
        },
        "mastering_policy": policy,
        "feedback_interpretation": feedback_interpretation,
        "processing_chain": [
            {
                "type": "highpass_filter",
                "enabled": true,
                "frequency_hz": profile.highpass_frequency_hz,
                "slope_db_per_oct": 12,
                "reason_es": "Limpiar DC y energia subsonica sin adelgazar el bajo."
            },
            {
                "type": "eq",
                "enabled": eq_bands.as_array().is_some_and(|items| !items.is_empty()),
                "bands": eq_bands,
                "reason_es": "Ajustes amplios y conservadores guiados por el feedback del track."
            },
            {
                "type": "lowpass_filter",
                "enabled": lowpass.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                "frequency_hz": if lowpass.get("enabled").and_then(Value::as_bool).unwrap_or(false) { lowpass.get("frequency_hz").and_then(Value::as_f64).unwrap_or(12_000.0) } else { 18_000.0 },
                "slope_db_per_oct": if lowpass.get("enabled").and_then(Value::as_bool).unwrap_or(false) { lowpass.get("slope_db_per_oct").and_then(Value::as_f64).unwrap_or(12.0) } else { 6.0 },
                "reason_es": lowpass.get("reason_es").and_then(Value::as_str).unwrap_or("Sin low-pass adicional.")
            },
            {
                "type": "bus_compressor",
                "enabled": compressor_enabled,
                "threshold_db": -18,
                "ratio": 1.4,
                "attack_ms": 30,
                "release_ms": 160,
                "makeup_gain_db": 0,
                "mix_percent": 40,
                "reason_es": if compressor_enabled { "Control paralelo muy leve para estabilizar dinamica sin perder transientes." } else { "La dinamica medida no justifica compresion adicional." }
            },
            {
                "type": "saturation",
                "enabled": saturation_enabled,
                "drive_db": if saturation_enabled { saturation.get("drive_db").and_then(Value::as_f64).unwrap_or(0.4) } else { 0.0 },
                "mix_percent": if saturation_enabled { saturation.get("mix_percent").and_then(Value::as_f64).unwrap_or(12.0) } else { 0.0 },
                "reason_es": saturation.get("reason_es").and_then(Value::as_str).unwrap_or("No se aplica saturacion para preservar el mix.")
            },
            {
                "type": "limiter",
                "enabled": profile.limiter_enabled,
                "ceiling_db": profile.true_peak_ceiling_db,
                "target_lufs": profile.target_lufs,
                "max_gain_reduction_db": limiter_gr,
                "reason_es": if profile.limiter_enabled { "Controlar true peak como ultimo paso y evitar inter-sample clipping." } else { "Para premaster de vinilo se prioriza headroom y se evita hard limiting." }
            }
        ],
        "export": {
            "format": "wav",
            "bit_depth": 24,
            "sample_rate_hz": sample_rate,
            "dither": true
        },
        "artist_message_es": format!("Prepararemos un master {} para {}, preservando el caracter del mix y priorizando margen de true peak seguro.", profile.label_es, job.source_name),
        "warnings_es": warnings
    }))
}

fn feedback_interpretation(
    app: &AppHandle,
    conn: &Connection,
    job: &MasteringJob,
    feedback: &str,
    reference_notes: &str,
    analysis: &AudioAnalysis,
    profile: &MasteringProfile,
    use_ai: bool,
) -> Result<Value, String> {
    if normalized_feedback(feedback, reference_notes)
        .trim()
        .is_empty()
    {
        if use_ai {
            emit_event(
                app,
                conn,
                job,
                EventMeta {
                    event: "ai_feedback_skipped",
                    step: "ai_feedback",
                    level: "info",
                    progress: Some(42.0),
                    message: "Sin feedback ni referencia; se omite parametrizacion AI de feedback."
                        .to_string(),
                    payload: json!({ "reason": "empty_feedback" }),
                },
            )?;
        }

        return Ok(empty_feedback_result(feedback, reference_notes));
    }

    if use_ai {
        emit_event(
            app,
            conn,
            job,
            EventMeta {
                event: "ai_feedback_started",
                step: "ai_feedback",
                level: "info",
                progress: Some(42.0),
                message:
                    "Enviando feedback y metricas a OpenAI para parametrizar EQ, filtros y textura."
                        .to_string(),
                payload: json!({
                    "model": OPENAI_MODEL,
                    "profile": profile.key,
                    "feedback_present": !feedback.trim().is_empty(),
                    "reference_present": !reference_notes.trim().is_empty()
                }),
            },
        )?;

        match ai_feedback(app, feedback, reference_notes, analysis, profile) {
            Ok(value) => {
                if let Some(result) =
                    normalize_feedback_result(&value, "ai_tool", feedback, reference_notes)
                {
                    let summary = result
                        .get("summary_es")
                        .and_then(Value::as_str)
                        .unwrap_or("Feedback AI parametrizado.")
                        .to_string();
                    emit_event(
                        app,
                        conn,
                        job,
                        EventMeta {
                            event: "ai_feedback_finished",
                            step: "ai_feedback",
                            level: "info",
                            progress: Some(44.0),
                            message: summary,
                            payload: json!({ "feedback_interpretation": result.clone() }),
                        },
                    )?;

                    return Ok(result);
                }

                emit_event(
                    app,
                    conn,
                    job,
                    EventMeta {
                        event: "ai_fallback_used",
                        step: "ai_feedback",
                        level: "warning",
                        progress: Some(44.0),
                        message:
                            "OpenAI respondio feedback fuera del contrato; usando reglas locales."
                                .to_string(),
                        payload: json!({ "raw_response": value }),
                    },
                )?;
            }
            Err(error) => {
                emit_event(
                    app,
                    conn,
                    job,
                    EventMeta {
                        event: "ai_fallback_used",
                        step: "ai_feedback",
                        level: "warning",
                        progress: Some(44.0),
                        message: format!(
                            "OpenAI feedback no disponible; usando reglas locales: {error}"
                        ),
                        payload: json!({ "error": error }),
                    },
                )?;
            }
        }
    }

    Ok(deterministic_feedback_result(feedback, reference_notes))
}

fn mastering_policy(
    app: &AppHandle,
    conn: &Connection,
    job: &MasteringJob,
    feedback: &str,
    reference_notes: &str,
    analysis: &AudioAnalysis,
    profile: &MasteringProfile,
    use_ai: bool,
) -> Result<Value, String> {
    if use_ai {
        emit_event(
            app,
            conn,
            job,
            EventMeta {
                event: "ai_policy_started",
                step: "ai_policy",
                level: "info",
                progress: Some(46.0),
                message: "Consultando OpenAI para definir guardrails de loudness, limiter y seguridad de true peak."
                    .to_string(),
                payload: json!({
                    "model": OPENAI_MODEL,
                    "profile": profile.key,
                    "target_lufs": profile.target_lufs,
                    "true_peak_ceiling_db": profile.true_peak_ceiling_db
                }),
            },
        )?;

        match ai_policy(app, feedback, reference_notes, analysis, profile) {
            Ok(value) => {
                if let Some(policy) = normalize_policy_result(&value, "ai_tool", profile) {
                    let summary = policy
                        .get("summary_es")
                        .and_then(Value::as_str)
                        .unwrap_or("Politica AI definida.")
                        .to_string();
                    emit_event(
                        app,
                        conn,
                        job,
                        EventMeta {
                            event: "ai_policy_finished",
                            step: "ai_policy",
                            level: "info",
                            progress: Some(48.0),
                            message: summary,
                            payload: json!({ "mastering_policy": policy.clone() }),
                        },
                    )?;

                    return Ok(policy);
                }

                emit_event(
                    app,
                    conn,
                    job,
                    EventMeta {
                        event: "ai_fallback_used",
                        step: "ai_policy",
                        level: "warning",
                        progress: Some(48.0),
                        message: "OpenAI respondio policy fuera del contrato; usando politica del preset."
                            .to_string(),
                        payload: json!({ "raw_response": value }),
                    },
                )?;
            }
            Err(error) => {
                emit_event(
                    app,
                    conn,
                    job,
                    EventMeta {
                        event: "ai_fallback_used",
                        step: "ai_policy",
                        level: "warning",
                        progress: Some(48.0),
                        message: format!(
                            "OpenAI policy no disponible; usando politica del preset: {error}"
                        ),
                        payload: json!({ "error": error }),
                    },
                )?;
            }
        }
    }

    Ok(default_policy(profile, analysis))
}

fn deterministic_feedback_result(feedback: &str, reference_notes: &str) -> Value {
    let normalized = normalized_feedback(feedback, reference_notes);
    let mut bands = Vec::new();
    let mut warnings = Vec::new();

    if low_cut_requested(&normalized) {
        bands.push(json!({
            "filter": "low_shelf",
            "frequency_hz": 90,
            "gain_db": -0.8,
            "q": 0.7,
            "reason_es": "Recorte leve de graves/subgrave pedido en el feedback."
        }));
    }

    if harshness_requested(&normalized) {
        bands.push(json!({
            "filter": "bell",
            "frequency_hz": 3600,
            "gain_db": -0.8,
            "q": 1.0,
            "reason_es": "Suavizar aspereza o presencia agresiva sin apagar el mix."
        }));
    }

    if high_cut_requested(&normalized) {
        let extreme = extreme_request(&normalized);
        bands.push(json!({
            "filter": "high_shelf",
            "frequency_hz": if extreme { 8500 } else { 10_000 },
            "gain_db": if extreme { -2.5 } else { -1.2 },
            "q": 0.7,
            "reason_es": "Recortar agudos de forma audible pero segura."
        }));
        if extreme {
            warnings.push("La peticion extrema sobre agudos se limito a un recorte seguro para no destruir el balance del mix.");
        }
    } else if high_boost_requested(&normalized) {
        bands.push(json!({
            "filter": "high_shelf",
            "frequency_hz": 11_000,
            "gain_db": 0.5,
            "q": 0.7,
            "reason_es": "Abrir un poco el extremo alto sin volverlo filoso."
        }));
    }

    json!({
        "source": "rules",
        "summary_es": if bands.is_empty() { "Feedback recibido, pero no requiere cambios tonales claros." } else { "Feedback convertido en ajustes de mastering con limites seguros." },
        "eq_bands": normalize_bands(&Value::Array(bands)),
        "lowpass_filter": deterministic_lowpass_filter(&normalized),
        "saturation": deterministic_saturation(&normalized),
        "compression": disabled_compression(),
        "warnings_es": warnings
    })
}

fn empty_feedback_result(feedback: &str, reference_notes: &str) -> Value {
    let normalized = normalized_feedback(feedback, reference_notes);
    json!({
        "source": "none",
        "summary_es": "Sin feedback de mastering para parametrizar.",
        "eq_bands": [],
        "lowpass_filter": deterministic_lowpass_filter(&normalized),
        "saturation": deterministic_saturation(&normalized),
        "compression": disabled_compression(),
        "warnings_es": []
    })
}

fn ai_feedback(
    app: &AppHandle,
    feedback: &str,
    reference_notes: &str,
    analysis: &AudioAnalysis,
    profile: &MasteringProfile,
) -> Result<Value, String> {
    let api_key = openai_api_key(app)?;
    let body = json!({
        "model": OPENAI_MODEL,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content": "You are Aifficator's mastering feedback parameterizer for independent electronic music. Convert artist feedback into safe DSP parameters only. Preserve the mix and cap EQ moves to +/-3 dB. Return only by calling submit_mastering_feedback_parameters."
            },
            {
                "role": "user",
                "content": format!(
                    "Target profile: {}\nArtist feedback: {}\nReference notes: {}\nMeasured audio stats: {}",
                    profile.key,
                    if feedback.trim().is_empty() { "none" } else { feedback },
                    if reference_notes.trim().is_empty() { "none" } else { reference_notes },
                    serde_json::to_string(analysis).unwrap_or_else(|_| "{}".to_string())
                )
            }
        ],
        "tools": [feedback_tool_schema()],
        "tool_choice": {
            "type": "function",
            "function": { "name": "submit_mastering_feedback_parameters" }
        }
    });
    openai_tool_arguments(&api_key, body, "submit_mastering_feedback_parameters")
}

fn ai_policy(
    app: &AppHandle,
    feedback: &str,
    reference_notes: &str,
    analysis: &AudioAnalysis,
    profile: &MasteringProfile,
) -> Result<Value, String> {
    let api_key = openai_api_key(app)?;
    let body = json!({
        "model": OPENAI_MODEL,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content": "You are Aifficator's mastering policy advisor. Choose conservative guardrails for a mastering render, not raw DSP. Stay inside selected target profile limits. Preserve transients and true peak safety over loudness. Return only by calling submit_mastering_policy."
            },
            {
                "role": "user",
                "content": format!(
                    "Target profile: {}\nAudio analysis before mastering: {}\nArtist feedback: {}\nReference notes: {}",
                    serde_json::to_string(profile).unwrap_or_else(|_| "{}".to_string()),
                    serde_json::to_string(analysis).unwrap_or_else(|_| "{}".to_string()),
                    if feedback.trim().is_empty() { "none" } else { feedback },
                    if reference_notes.trim().is_empty() { "none" } else { reference_notes }
                )
            }
        ],
        "tools": [policy_tool_schema()],
        "tool_choice": {
            "type": "function",
            "function": { "name": "submit_mastering_policy" }
        }
    });
    openai_tool_arguments(&api_key, body, "submit_mastering_policy")
}

fn openai_tool_arguments(api_key: &str, body: Value, function_name: &str) -> Result<Value, String> {
    let client = reqwest::blocking::Client::new();
    let response: Value = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|error| format!("OpenAI request fallo: {error}"))?
        .error_for_status()
        .map_err(|error| format!("OpenAI retorno error: {error}"))?
        .json()
        .map_err(|error| format!("OpenAI retorno JSON invalido: {error}"))?;

    let calls = response
        .pointer("/choices/0/message/tool_calls")
        .and_then(Value::as_array)
        .ok_or_else(|| "OpenAI no retorno tool calls.".to_string())?;

    let arguments = calls
        .iter()
        .find(|call| call.pointer("/function/name").and_then(Value::as_str) == Some(function_name))
        .and_then(|call| call.pointer("/function/arguments").and_then(Value::as_str))
        .ok_or_else(|| "OpenAI no retorno argumentos de funcion.".to_string())?;

    serde_json::from_str(arguments)
        .map_err(|error| format!("OpenAI retorno argumentos invalidos: {error}"))
}

fn openai_api_key(app: &AppHandle) -> Result<String, String> {
    crate::settings::load_openai_api_key(app)?
        .ok_or_else(|| "OpenAI API key no configurada.".to_string())
}

fn feedback_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "submit_mastering_feedback_parameters",
            "description": "Submit safe mastering parameters interpreted from artist feedback.",
            "parameters": {
                "type": "object",
                "required": ["summary_es", "eq_bands", "lowpass_filter", "saturation", "compression", "warnings_es"],
                "additionalProperties": false,
                "properties": {
                    "summary_es": { "type": "string" },
                    "eq_bands": {
                        "type": "array",
                        "maxItems": 4,
                        "items": {
                            "type": "object",
                            "required": ["filter", "frequency_hz", "gain_db", "q", "reason_es"],
                            "additionalProperties": false,
                            "properties": {
                                "filter": { "type": "string", "enum": ["low_shelf", "bell", "high_shelf"] },
                                "frequency_hz": { "type": "number", "minimum": 20, "maximum": 18000 },
                                "gain_db": { "type": "number", "minimum": -3, "maximum": 3 },
                                "q": { "type": "number", "minimum": 0.2, "maximum": 3.0 },
                                "reason_es": { "type": "string" }
                            }
                        }
                    },
                    "lowpass_filter": {
                        "type": "object",
                        "required": ["enabled", "frequency_hz", "slope_db_per_oct", "reason_es"],
                        "additionalProperties": false,
                        "properties": {
                            "enabled": { "type": "boolean" },
                            "frequency_hz": { "type": "number", "minimum": 8000, "maximum": 18000 },
                            "slope_db_per_oct": { "type": "number", "enum": [6, 12] },
                            "reason_es": { "type": "string" }
                        }
                    },
                    "saturation": {
                        "type": "object",
                        "required": ["enabled", "drive_db", "mix_percent", "reason_es"],
                        "additionalProperties": false,
                        "properties": {
                            "enabled": { "type": "boolean" },
                            "drive_db": { "type": "number", "minimum": 0, "maximum": 1 },
                            "mix_percent": { "type": "number", "minimum": 0, "maximum": 20 },
                            "reason_es": { "type": "string" }
                        }
                    },
                    "compression": {
                        "type": "object",
                        "required": ["enabled", "ratio", "mix_percent", "reason_es"],
                        "additionalProperties": false,
                        "properties": {
                            "enabled": { "type": "boolean" },
                            "ratio": { "type": "number", "minimum": 1.1, "maximum": 1.6 },
                            "mix_percent": { "type": "number", "minimum": 0, "maximum": 45 },
                            "reason_es": { "type": "string" }
                        }
                    },
                    "warnings_es": { "type": "array", "maxItems": 4, "items": { "type": "string" } }
                }
            }
        }
    })
}

fn policy_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "submit_mastering_policy",
            "description": "Submit mastering safety and loudness policy values.",
            "parameters": {
                "type": "object",
                "required": [
                    "summary_es",
                    "limiter_max_gain_reduction_db",
                    "loudness_correction_limit_db",
                    "max_loudness_correction_passes",
                    "minimum_crest_factor_db",
                    "max_positive_gain_db",
                    "loud_source_gain_cap_db",
                    "true_peak_safety_margin_db",
                    "warnings_es"
                ],
                "additionalProperties": false,
                "properties": {
                    "summary_es": { "type": "string" },
                    "limiter_max_gain_reduction_db": { "type": "number" },
                    "loudness_correction_limit_db": { "type": "number" },
                    "max_loudness_correction_passes": { "type": "number" },
                    "minimum_crest_factor_db": { "type": ["number", "null"] },
                    "max_positive_gain_db": { "type": "number" },
                    "loud_source_gain_cap_db": { "type": "number" },
                    "true_peak_safety_margin_db": { "type": "number" },
                    "warnings_es": { "type": "array", "maxItems": 4, "items": { "type": "string" } }
                }
            }
        }
    })
}

fn normalize_feedback_result(
    value: &Value,
    source: &str,
    feedback: &str,
    reference_notes: &str,
) -> Option<Value> {
    let normalized = normalized_feedback(feedback, reference_notes);
    let mut lowpass = normalize_lowpass(value.get("lowpass_filter"));
    if high_cut_requested(&normalized)
        && extreme_request(&normalized)
        && !lowpass
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        lowpass = deterministic_lowpass_filter(&normalized);
    }

    Some(json!({
        "source": source,
        "summary_es": value.get("summary_es").and_then(Value::as_str).filter(|text| !text.trim().is_empty()).unwrap_or("Feedback interpretado como ajustes conservadores de mastering."),
        "eq_bands": normalize_bands(value.get("eq_bands").unwrap_or(&Value::Null)),
        "lowpass_filter": lowpass,
        "saturation": normalize_saturation(value.get("saturation")),
        "compression": normalize_compression(value.get("compression")),
        "warnings_es": normalize_string_array(value.get("warnings_es"), 4)
    }))
}

fn normalize_policy_result(
    value: &Value,
    source: &str,
    profile: &MasteringProfile,
) -> Option<Value> {
    let minimum_crest = match profile.minimum_crest_factor_db {
        Some(minimum) => Some(clamp(
            value
                .get("minimum_crest_factor_db")
                .and_then(Value::as_f64)
                .unwrap_or(minimum),
            minimum,
            minimum + 2.0,
        )),
        None => None,
    };
    Some(json!({
        "source": source,
        "summary_es": value.get("summary_es").and_then(Value::as_str).filter(|text| !text.trim().is_empty()).unwrap_or("Politica AI de mastering."),
        "limiter_max_gain_reduction_db": clamp(value.get("limiter_max_gain_reduction_db").and_then(Value::as_f64).unwrap_or(profile.limiter_max_gain_reduction_db), 0.0, profile.limiter_max_gain_reduction_db),
        "loudness_correction_limit_db": clamp(value.get("loudness_correction_limit_db").and_then(Value::as_f64).unwrap_or(profile.loudness_correction_limit_db), 0.0, profile.loudness_correction_limit_db),
        "max_loudness_correction_passes": clamp(value.get("max_loudness_correction_passes").and_then(Value::as_f64).unwrap_or(profile.max_loudness_correction_passes as f64), 0.0, profile.max_loudness_correction_passes as f64).round() as usize,
        "minimum_crest_factor_db": minimum_crest,
        "max_positive_gain_db": clamp(value.get("max_positive_gain_db").and_then(Value::as_f64).unwrap_or(profile.max_positive_gain_db), 0.0, profile.max_positive_gain_db),
        "loud_source_gain_cap_db": clamp(value.get("loud_source_gain_cap_db").and_then(Value::as_f64).unwrap_or(profile.loud_source_gain_cap_db), 0.0, profile.loud_source_gain_cap_db),
        "true_peak_safety_margin_db": clamp(value.get("true_peak_safety_margin_db").and_then(Value::as_f64).unwrap_or(profile.true_peak_safety_margin_db), 0.3, profile.true_peak_safety_margin_db.max(0.8)),
        "warnings_es": normalize_string_array(value.get("warnings_es"), 4)
    }))
}

fn normalize_bands(value: &Value) -> Value {
    let bands = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|band| {
            let filter = band.get("filter").and_then(Value::as_str)?;
            if !matches!(filter, "low_shelf" | "bell" | "high_shelf") {
                return None;
            }
            Some(json!({
                "filter": filter,
                "frequency_hz": clamp(band.get("frequency_hz").and_then(Value::as_f64).unwrap_or(1000.0), 20.0, 18_000.0).round(),
                "gain_db": round2(clamp(band.get("gain_db").and_then(Value::as_f64).unwrap_or(0.0), -3.0, 3.0)),
                "q": round2(clamp(band.get("q").and_then(Value::as_f64).unwrap_or(0.7), 0.2, 3.0)),
                "reason_es": band.get("reason_es").and_then(Value::as_str).unwrap_or("Ajuste derivado del feedback.")
            }))
        })
        .take(4)
        .collect::<Vec<_>>();
    Value::Array(bands)
}

fn normalize_lowpass(value: Option<&Value>) -> Value {
    let value = value.unwrap_or(&Value::Null);
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({
        "enabled": enabled,
        "frequency_hz": if enabled { clamp(value.get("frequency_hz").and_then(Value::as_f64).unwrap_or(12_000.0), 8_000.0, 18_000.0).round() } else { 18_000.0 },
        "slope_db_per_oct": if enabled && value.get("slope_db_per_oct").and_then(Value::as_f64).unwrap_or(12.0) >= 12.0 { 12 } else { 6 },
        "reason_es": value.get("reason_es").and_then(Value::as_str).unwrap_or("Filtro low-pass controlado segun feedback extremo.")
    })
}

fn normalize_saturation(value: Option<&Value>) -> Value {
    let value = value.unwrap_or(&Value::Null);
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({
        "enabled": enabled,
        "drive_db": if enabled { round2(clamp(value.get("drive_db").and_then(Value::as_f64).unwrap_or(0.4), 0.0, 1.0)) } else { 0.0 },
        "mix_percent": if enabled { clamp(value.get("mix_percent").and_then(Value::as_f64).unwrap_or(10.0), 0.0, 20.0).round() } else { 0.0 },
        "reason_es": value.get("reason_es").and_then(Value::as_str).unwrap_or("Saturacion sutil segun feedback.")
    })
}

fn normalize_compression(value: Option<&Value>) -> Value {
    let value = value.unwrap_or(&Value::Null);
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({
        "enabled": enabled,
        "ratio": if enabled { round2(clamp(value.get("ratio").and_then(Value::as_f64).unwrap_or(1.3), 1.1, 1.6)) } else { 1.3 },
        "mix_percent": if enabled { clamp(value.get("mix_percent").and_then(Value::as_f64).unwrap_or(30.0), 0.0, 45.0).round() } else { 0.0 },
        "reason_es": value.get("reason_es").and_then(Value::as_str).unwrap_or("Compresion paralela leve segun feedback.")
    })
}

fn deterministic_lowpass_filter(normalized: &str) -> Value {
    let enabled = high_cut_requested(normalized) && extreme_request(normalized);
    json!({
        "enabled": enabled,
        "frequency_hz": if enabled { 12_000 } else { 18_000 },
        "slope_db_per_oct": if enabled { 12 } else { 6 },
        "reason_es": if enabled { "Low-pass suave para que el pedido extremo de cortar agudos sea perceptible sin apagar completamente el master." } else { "Sin low-pass adicional." }
    })
}

fn deterministic_saturation(normalized: &str) -> Value {
    let enabled = saturation_requested(normalized);
    json!({
        "enabled": enabled,
        "drive_db": if enabled { 0.4 } else { 0.0 },
        "mix_percent": if enabled { 12 } else { 0 },
        "reason_es": if enabled { "Agregar densidad sutil pedida en el feedback." } else { "Sin saturacion solicitada." }
    })
}

fn disabled_compression() -> Value {
    json!({
        "enabled": false,
        "ratio": 1.3,
        "mix_percent": 0,
        "reason_es": "Sin compresion adicional solicitada."
    })
}

fn default_policy(profile: &MasteringProfile, analysis: &AudioAnalysis) -> Value {
    json!({
        "source": "profile",
        "summary_es": format!("Politica base del preset {}.", profile.label_es),
        "limiter_max_gain_reduction_db": limiter_gain_reduction(profile, already_mastered(analysis)),
        "loudness_correction_limit_db": profile.loudness_correction_limit_db,
        "max_loudness_correction_passes": profile.max_loudness_correction_passes,
        "minimum_crest_factor_db": profile.minimum_crest_factor_db,
        "max_positive_gain_db": profile.max_positive_gain_db,
        "loud_source_gain_cap_db": profile.loud_source_gain_cap_db,
        "true_peak_safety_margin_db": profile.true_peak_safety_margin_db,
        "warnings_es": []
    })
}

fn render_master(
    app: &AppHandle,
    input_path: &Path,
    output_dir: &Path,
    pass: usize,
    recipe: &Value,
    analysis_before: &AudioAnalysis,
) -> Result<PathBuf, String> {
    fs::create_dir_all(output_dir)
        .map_err(|error| format!("No se pudo crear carpeta de render: {error}"))?;
    let output_path = output_dir.join(format!("render-{pass}.wav"));
    let filters = filter_chain(recipe, analysis_before);
    let mut command = system::ffmpeg_command(app);
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input_path)
        .arg("-vn");

    if !filters.is_empty() {
        command.args(["-filter:a", &filters]);
    }

    command
        .args([
            "-ar",
            &export_sample_rate(recipe).to_string(),
            "-ac",
            "2",
            "-c:a",
            "pcm_s24le",
        ])
        .arg(&output_path);

    let output = command
        .output()
        .map_err(|error| format!("No se pudo ejecutar ffmpeg render: {error}"))?;

    if !output.status.success() || !output_path.is_file() {
        return Err(format!(
            "ffmpeg no pudo renderizar el master: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(output_path)
}

fn package_master(
    app: &AppHandle,
    rendered_path: &Path,
    final_path: &Path,
    job: &MasteringJob,
    recipe: &Value,
) -> Result<Value, String> {
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("No se pudo crear carpeta de salida: {error}"))?;
    }
    let _ = fs::remove_file(final_path);

    let cover_path = job
        .cover_art_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file() && is_cover_art_path(path));
    let mut warnings = Vec::new();
    let mut cover_embedded = false;

    if let Some(cover) = cover_path.as_ref() {
        match run_package_command(
            app,
            rendered_path,
            final_path,
            job,
            recipe,
            Some(cover.as_path()),
        ) {
            Ok(()) => cover_embedded = true,
            Err(error) => {
                warnings.push(format!(
                    "No se pudo incrustar la caratula; se genero audio sin cover: {error}"
                ));
                let _ = fs::remove_file(final_path);
                run_package_command(app, rendered_path, final_path, job, recipe, None)?;
            }
        }
    } else {
        if job.cover_art_path.is_some() {
            warnings.push(
                "Caratula omitida porque el archivo ya no existe o no es JPG/PNG.".to_string(),
            );
        }
        run_package_command(app, rendered_path, final_path, job, recipe, None)?;
    }

    let validation = validate_packaged_file(app, final_path)?;
    let validation_cover = validation
        .get("cover_detected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if cover_embedded && !validation_cover {
        warnings
            .push("ffprobe no detecto la caratula como attached_pic en el AIFF final.".to_string());
    }

    Ok(json!({
        "output_format": job.output_format.clone(),
        "output_label": output_format_label(&job.output_format),
        "output_path": final_path.to_string_lossy(),
        "metadata_written": !metadata_pairs(&job.metadata, &job.source_name).is_empty(),
        "cover_requested": job.cover_art_path.is_some(),
        "cover_embedded": cover_embedded && validation_cover,
        "warnings": warnings,
        "validation": validation
    }))
}

fn run_package_command(
    app: &AppHandle,
    rendered_path: &Path,
    final_path: &Path,
    job: &MasteringJob,
    recipe: &Value,
    cover_path: Option<&Path>,
) -> Result<(), String> {
    let output_format = normalize_output_format(Some(&job.output_format));
    let is_aiff = output_format != "wav_24";
    let mut command = system::ffmpeg_command(app);
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(rendered_path);

    if let Some(cover) = cover_path {
        command.arg("-i").arg(cover);
    }

    command.args(["-map", "0:a:0"]);
    if cover_path.is_some() {
        command
            .args([
                "-map",
                "1:v:0",
                "-c:v",
                "png",
                "-disposition:v:0",
                "attached_pic",
            ])
            .args(["-metadata:s:v", "title=Cover"])
            .args(["-metadata:s:v", "comment=Cover (front)"]);
    } else {
        command.arg("-vn");
    }

    command.args(["-map_metadata", "-1"]);
    for (key, value) in metadata_pairs(&job.metadata, &job.source_name) {
        command.arg("-metadata").arg(format!("{key}={value}"));
    }

    match output_format.as_str() {
        "aiff_cdj16" => {
            command.args(["-ar", "44100", "-ac", "2", "-c:a", "pcm_s16be"]);
        }
        "wav_24" => {
            command.args([
                "-ar",
                &export_sample_rate(recipe).to_string(),
                "-ac",
                "2",
                "-c:a",
                "pcm_s24le",
            ]);
        }
        _ => {
            command.args([
                "-ar",
                &export_sample_rate(recipe).to_string(),
                "-ac",
                "2",
                "-c:a",
                "pcm_s24be",
            ]);
        }
    }

    if is_aiff {
        command.args(["-write_id3v2", "1", "-id3v2_version", "3"]);
    }

    command.arg(final_path);
    let output = command
        .output()
        .map_err(|error| format!("No se pudo ejecutar ffmpeg packaging: {error}"))?;

    if !output.status.success() || !final_path.is_file() {
        return Err(format!(
            "ffmpeg no pudo empaquetar el master: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn validate_packaged_file(app: &AppHandle, path: &Path) -> Result<Value, String> {
    let output = system::ffprobe_command(app)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=format_name:format_tags:stream=index,codec_type,codec_name:stream_disposition=attached_pic",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("No se pudo validar metadata con ffprobe: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe no pudo validar el master final: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("ffprobe retorno JSON invalido al validar metadata: {error}"))?;
    let format_name = parsed
        .pointer("/format/format_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let tag_count = parsed
        .pointer("/format/tags")
        .and_then(Value::as_object)
        .map(|tags| tags.len())
        .unwrap_or(0);
    let cover_detected = parsed
        .get("streams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|stream| {
            stream.get("codec_type").and_then(Value::as_str) == Some("video")
                && stream
                    .pointer("/disposition/attached_pic")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    == 1
        });

    Ok(json!({
        "format_name": format_name,
        "tag_count": tag_count,
        "cover_detected": cover_detected
    }))
}

fn metadata_pairs(metadata: &Value, source_name: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    push_metadata_pair(
        &mut pairs,
        "title",
        metadata_text(metadata, "title").or_else(|| Some(safe_stem(source_name))),
    );
    push_metadata_pair(&mut pairs, "artist", metadata_text(metadata, "artist"));
    push_metadata_pair(&mut pairs, "album", metadata_text(metadata, "album"));
    push_metadata_pair(&mut pairs, "genre", metadata_text(metadata, "genre"));
    push_metadata_pair(&mut pairs, "date", metadata_text(metadata, "year"));
    push_metadata_pair(&mut pairs, "track", metadata_text(metadata, "track_number"));
    push_metadata_pair(&mut pairs, "composer", metadata_text(metadata, "composer"));
    push_metadata_pair(&mut pairs, "publisher", metadata_text(metadata, "label"));
    push_metadata_pair(
        &mut pairs,
        "copyright",
        metadata_text(metadata, "copyright"),
    );
    push_metadata_pair(&mut pairs, "bpm", metadata_text(metadata, "bpm"));
    push_metadata_pair(
        &mut pairs,
        "initialkey",
        metadata_text(metadata, "musical_key"),
    );
    push_metadata_pair(&mut pairs, "isrc", metadata_text(metadata, "isrc"));
    push_metadata_pair(&mut pairs, "comment", metadata_text(metadata, "comment"));
    pairs
}

fn metadata_text(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(500).collect())
}

fn push_metadata_pair(pairs: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        pairs.push((key.to_string(), value));
    }
}

fn output_format_label(output_format: &str) -> &'static str {
    match output_format {
        "aiff_cdj16" => "AIFF CDJ safe 16-bit",
        "wav_24" => "WAV 24-bit",
        _ => "AIFF 24-bit",
    }
}

fn package_event_level(report: &Value) -> &'static str {
    if report
        .get("warnings")
        .and_then(Value::as_array)
        .is_some_and(|warnings| !warnings.is_empty())
    {
        "warning"
    } else {
        "info"
    }
}

fn package_summary(report: &Value) -> String {
    let label = report
        .get("output_label")
        .and_then(Value::as_str)
        .unwrap_or("master");
    let tag_count = report
        .pointer("/validation/tag_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cover = if report
        .get("cover_embedded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "cover embebido"
    } else if report
        .get("cover_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "cover omitido"
    } else {
        "sin cover"
    };
    format!("{label} listo con {tag_count} tag(s), {cover}.")
}

fn filter_chain(recipe: &Value, analysis_before: &AudioAnalysis) -> String {
    let mut filters = Vec::new();
    filters.extend(highpass_filters(recipe));
    filters.extend(eq_filters(recipe));
    filters.extend(lowpass_filters(recipe));
    filters.extend(compressor_filters(recipe));
    filters.extend(saturation_filters(recipe));
    filters.extend(gain_filters(recipe, analysis_before));
    filters.extend(limiter_filters(recipe));
    if recipe
        .pointer("/export/dither")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        filters.push("aresample=dither_method=triangular".to_string());
    }
    filters.join(",")
}

fn highpass_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "highpass_filter") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    vec![format!(
        "highpass=f={}",
        number(stage.get("frequency_hz"), 25.0)
    )]
}

fn eq_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "eq") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    stage
        .get("bands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|band| {
            let frequency = round2(number(band.get("frequency_hz"), 1000.0));
            let gain = round2(number(band.get("gain_db"), 0.0));
            let q = round2(number(band.get("q"), 0.7));
            match band.get("filter").and_then(Value::as_str) {
                Some("low_shelf") => Some(format!(
                    "bass=f={frequency}:g={gain}:width_type=q:width={q}"
                )),
                Some("high_shelf") => Some(format!(
                    "treble=f={frequency}:g={gain}:width_type=q:width={q}"
                )),
                Some("bell") => Some(format!("equalizer=f={frequency}:t=q:w={q}:g={gain}")),
                _ => None,
            }
        })
        .collect()
}

fn lowpass_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "lowpass_filter") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    vec![format!(
        "lowpass=f={}",
        round2(number(stage.get("frequency_hz"), 12_000.0))
    )]
}

fn compressor_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "bus_compressor") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let threshold = round6(db_to_amplitude(number(stage.get("threshold_db"), -18.0)));
    let ratio = round2(number(stage.get("ratio"), 1.4));
    let attack = round2(number(stage.get("attack_ms"), 30.0));
    let release = round2(number(stage.get("release_ms"), 160.0));
    let makeup = round6(db_to_amplitude(number(stage.get("makeup_gain_db"), 0.0)));
    let mix = round2((number(stage.get("mix_percent"), 40.0) / 100.0).clamp(0.0, 1.0));
    vec![format!(
        "acompressor=threshold={threshold}:ratio={ratio}:attack={attack}:release={release}:makeup={makeup}:mix={mix}"
    )]
}

fn saturation_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "saturation") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let drive = round6(db_to_amplitude(number(stage.get("drive_db"), 0.4)));
    vec![format!("asoftclip=type=tanh:param={drive}")]
}

fn gain_filters(recipe: &Value, analysis_before: &AudioAnalysis) -> Vec<String> {
    let gain = loudness_gain_db(recipe, analysis_before);
    if gain.abs() < 0.05 {
        Vec::new()
    } else {
        vec![format!("volume={}dB", round2(gain))]
    }
}

fn limiter_filters(recipe: &Value) -> Vec<String> {
    let Some(stage) = stage(recipe, "limiter") else {
        return Vec::new();
    };
    if !stage
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let limit = round6(db_to_amplitude(limiter_sample_ceiling_db(recipe, stage)));
    vec![
        "aresample=192000".to_string(),
        format!("alimiter=limit={limit}:level=false"),
        format!("aresample={}", export_sample_rate(recipe)),
    ]
}

fn loudness_gain_db(recipe: &Value, analysis_before: &AudioAnalysis) -> f64 {
    let limiter = stage(recipe, "limiter");
    let target_lufs = recipe
        .pointer("/target/target_lufs")
        .and_then(Value::as_f64)
        .or_else(|| limiter.and_then(|stage| stage.get("target_lufs").and_then(Value::as_f64)));
    let Some(target_lufs) = target_lufs else {
        return 0.0;
    };
    let Some(current_lufs) = analysis_before.integrated_lufs else {
        return 0.0;
    };
    let gain = target_lufs - current_lufs;
    if gain <= 0.0 {
        return gain + loudness_offset_db(recipe);
    }

    gain.min(max_positive_gain_db(recipe, analysis_before, limiter)) + loudness_offset_db(recipe)
}

fn max_positive_gain_db(
    recipe: &Value,
    analysis_before: &AudioAnalysis,
    limiter: Option<&Value>,
) -> f64 {
    if recipe.pointer("/target/profile").and_then(Value::as_str) == Some("vinyl_premaster") {
        return 0.0;
    }
    let policy = recipe.get("mastering_policy").unwrap_or(&Value::Null);
    let profile_cap = number(policy.get("max_positive_gain_db"), 10.0);

    if analysis_before
        .integrated_lufs
        .is_some_and(|lufs| lufs >= -10.0)
    {
        return profile_cap.min(number(policy.get("loud_source_gain_cap_db"), 1.5));
    }

    match peak_safe_gain_cap(recipe, analysis_before, limiter) {
        Some(cap) => profile_cap.min(cap),
        None => profile_cap,
    }
}

fn peak_safe_gain_cap(
    recipe: &Value,
    analysis_before: &AudioAnalysis,
    limiter: Option<&Value>,
) -> Option<f64> {
    let limiter = limiter?;
    if !limiter
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let current_true_peak = analysis_before.true_peak_dbfs?;
    let max_gain_reduction = number(limiter.get("max_gain_reduction_db"), 6.0);
    Some(limiter_sample_ceiling_db(recipe, limiter) - current_true_peak + max_gain_reduction)
}

fn limiter_sample_ceiling_db(recipe: &Value, limiter: &Value) -> f64 {
    number(limiter.get("ceiling_db"), -1.0) - true_peak_safety_margin_db(recipe)
}

fn true_peak_safety_margin_db(recipe: &Value) -> f64 {
    number(
        recipe.pointer("/mastering_policy/true_peak_safety_margin_db"),
        0.5,
    )
}

fn loudness_offset_db(recipe: &Value) -> f64 {
    number(
        recipe.pointer("/render_adjustments/loudness_offset_db"),
        0.0,
    )
    .clamp(-6.0, 6.0)
}

fn stage<'a>(recipe: &'a Value, stage_type: &str) -> Option<&'a Value> {
    recipe
        .get("processing_chain")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| item.get("type").and_then(Value::as_str) == Some(stage_type))
}

fn export_sample_rate(recipe: &Value) -> u32 {
    number(recipe.pointer("/export/sample_rate_hz"), 44_100.0) as u32
}

fn loudness_correction_db(recipe: &Value, analysis_after: &AudioAnalysis) -> f64 {
    if unsafe_analysis(recipe, analysis_after) {
        return 0.0;
    }
    if recipe.pointer("/target/profile").and_then(Value::as_str) == Some("vinyl_premaster") {
        return 0.0;
    }
    let Some(target_lufs) = recipe
        .pointer("/target/target_lufs")
        .and_then(Value::as_f64)
    else {
        return 0.0;
    };
    let Some(current_lufs) = analysis_after.integrated_lufs else {
        return 0.0;
    };
    if let (Some(true_peak), Some(ceiling)) = (
        analysis_after.true_peak_dbfs,
        recipe
            .pointer("/target/true_peak_ceiling_db")
            .and_then(Value::as_f64),
    ) {
        if true_peak > ceiling {
            return 0.0;
        }
    }

    let correction = target_lufs - current_lufs;
    if correction <= 0.3 {
        return 0.0;
    }
    round2(correction.min(number(
        recipe.pointer("/mastering_policy/loudness_correction_limit_db"),
        0.0,
    )))
}

fn recipe_with_loudness_offset(
    recipe: &Value,
    loudness_offset_db: f64,
    correction_pass: usize,
) -> Value {
    let mut corrected = recipe.clone();
    corrected["render_adjustments"] = json!({
        "loudness_offset_db": round2(loudness_offset_db),
        "correction_passes": correction_pass,
        "reason_es": "Correccion iterativa para acercar el master al LUFS objetivo sin perder el ceiling de true peak."
    });
    corrected
}

fn current_loudness_offset_db(recipe: &Value) -> f64 {
    number(
        recipe.pointer("/render_adjustments/loudness_offset_db"),
        0.0,
    )
}

fn unsafe_analysis(recipe: &Value, analysis: &AudioAnalysis) -> bool {
    if analysis.clipping_detected.unwrap_or(false) {
        return true;
    }
    match (
        analysis.true_peak_dbfs,
        recipe
            .pointer("/target/true_peak_ceiling_db")
            .and_then(Value::as_f64),
    ) {
        (Some(true_peak), Some(ceiling)) => true_peak > ceiling,
        _ => false,
    }
}

fn overcompressed_analysis(recipe: &Value, before: &AudioAnalysis, after: &AudioAnalysis) -> bool {
    let minimum_crest = recipe
        .pointer("/mastering_policy/minimum_crest_factor_db")
        .and_then(Value::as_f64);
    let Some(minimum_crest) = minimum_crest else {
        return false;
    };
    let Some(after_crest) = after.crest_factor_db else {
        return false;
    };
    if before
        .crest_factor_db
        .is_some_and(|crest| crest < minimum_crest)
    {
        return false;
    }
    after_crest < minimum_crest
}

fn max_loudness_correction_passes(recipe: &Value) -> usize {
    recipe
        .pointer("/mastering_policy/max_loudness_correction_passes")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn final_output_path(dir: &Path, job: &MasteringJob) -> PathBuf {
    let suffix = if job.target_profile == "vinyl_premaster" {
        "premaster"
    } else {
        "master"
    };
    let extension = if normalize_output_format(Some(&job.output_format)) == "wav_24" {
        "wav"
    } else {
        "aiff"
    };
    dir.join(format!(
        "{}-{}-{suffix}.{extension}",
        safe_stem(&job.source_name),
        job.target_profile
    ))
}

fn write_sidecar_json(dir: &Path, name: &str, value: &Value) -> Result<(), String> {
    fs::write(
        dir.join(name),
        serde_json::to_string_pretty(value)
            .map_err(|error| format!("No se pudo serializar {name}: {error}"))?,
    )
    .map_err(|error| format!("No se pudo escribir {name}: {error}"))
}

fn target_profiles() -> Vec<MasteringProfile> {
    vec![
        MasteringProfile {
            key: "streaming_clean".to_string(),
            label_es: "Streaming clean".to_string(),
            target_lufs: -14.0,
            true_peak_ceiling_db: -1.0,
            style_es: "Limpio, dinamico y seguro para plataformas.".to_string(),
            highpass_frequency_hz: 25.0,
            limiter_enabled: true,
            limiter_max_gain_reduction_db: 4.0,
            already_mastered_limiter_max_gain_reduction_db: 1.0,
            loudness_correction_limit_db: 1.5,
            max_loudness_correction_passes: 2,
            minimum_crest_factor_db: Some(11.5),
            max_positive_gain_db: 10.0,
            loud_source_gain_cap_db: 1.5,
            true_peak_safety_margin_db: 0.5,
        },
        MasteringProfile {
            key: "club_loud".to_string(),
            label_es: "Club loud".to_string(),
            target_lufs: -9.0,
            true_peak_ceiling_db: -0.7,
            style_es: "Fuerte y energetico, cuidando transientes y evitando clipping.".to_string(),
            highpass_frequency_hz: 25.0,
            limiter_enabled: true,
            limiter_max_gain_reduction_db: 9.0,
            already_mastered_limiter_max_gain_reduction_db: 1.0,
            loudness_correction_limit_db: 4.0,
            max_loudness_correction_passes: 3,
            minimum_crest_factor_db: Some(10.0),
            max_positive_gain_db: 16.0,
            loud_source_gain_cap_db: 1.5,
            true_peak_safety_margin_db: 0.5,
        },
        MasteringProfile {
            key: "demo_balanced".to_string(),
            label_es: "Demo balanced".to_string(),
            target_lufs: -11.5,
            true_peak_ceiling_db: -1.0,
            style_es: "Presentable y balanceado, sin limitar de mas.".to_string(),
            highpass_frequency_hz: 25.0,
            limiter_enabled: true,
            limiter_max_gain_reduction_db: 5.0,
            already_mastered_limiter_max_gain_reduction_db: 1.0,
            loudness_correction_limit_db: 2.0,
            max_loudness_correction_passes: 2,
            minimum_crest_factor_db: Some(10.8),
            max_positive_gain_db: 12.0,
            loud_source_gain_cap_db: 1.5,
            true_peak_safety_margin_db: 0.5,
        },
        MasteringProfile {
            key: "vinyl_premaster".to_string(),
            label_es: "Vinyl premaster".to_string(),
            target_lufs: -15.0,
            true_peak_ceiling_db: -3.0,
            style_es: "Conservador, con headroom y sin hard limiting.".to_string(),
            highpass_frequency_hz: 30.0,
            limiter_enabled: false,
            limiter_max_gain_reduction_db: 0.0,
            already_mastered_limiter_max_gain_reduction_db: 0.0,
            loudness_correction_limit_db: 0.0,
            max_loudness_correction_passes: 0,
            minimum_crest_factor_db: None,
            max_positive_gain_db: 0.0,
            loud_source_gain_cap_db: 0.0,
            true_peak_safety_margin_db: 0.5,
        },
    ]
}

fn fetch_profile(key: &str) -> MasteringProfile {
    target_profiles()
        .into_iter()
        .find(|profile| profile.key == key)
        .unwrap_or_else(|| {
            target_profiles()
                .into_iter()
                .find(|profile| profile.key == "demo_balanced")
                .expect("demo_balanced profile exists")
        })
}

fn normalize_profile_key(key: &str) -> String {
    if target_profiles().iter().any(|profile| profile.key == key) {
        key.to_string()
    } else {
        "demo_balanced".to_string()
    }
}

fn limiter_gain_reduction(profile: &MasteringProfile, already_mastered: bool) -> f64 {
    if !profile.limiter_enabled {
        0.0
    } else if already_mastered {
        profile.already_mastered_limiter_max_gain_reduction_db
    } else {
        profile.limiter_max_gain_reduction_db
    }
}

fn recipe_warnings(
    analysis: &AudioAnalysis,
    profile: &MasteringProfile,
    feedback: &Value,
    policy: &Value,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if already_mastered(analysis) {
        warnings.push("No se recomienda subir mas el loudness sin perder dinamica.".to_string());
    }
    if analysis
        .true_peak_dbfs
        .is_some_and(|true_peak| true_peak > -1.0)
    {
        warnings.push(
            "El true peak medido esta cerca de 0 dBFS; el limitador debe trabajar con margen."
                .to_string(),
        );
    }
    if profile.key != "vinyl_premaster" {
        warnings.push("Para vinilo conviene preparar una version especifica con menos limitacion y mas headroom.".to_string());
    }
    warnings.extend(normalize_string_array(feedback.get("warnings_es"), 4));
    warnings.extend(normalize_string_array(policy.get("warnings_es"), 4));
    warnings
}

fn main_issues(analysis: &AudioAnalysis, feedback: &str) -> Vec<String> {
    let mut issues = Vec::new();
    if analysis
        .true_peak_dbfs
        .is_some_and(|true_peak| true_peak > -1.0)
    {
        issues.push("True peak cerca de 0 dBFS".to_string());
    }
    if already_mastered(analysis) {
        issues.push("Material ya muy fuerte para seguir aumentando loudness".to_string());
    }
    if analysis.clipping_detected.unwrap_or(false) {
        issues.push("Clipping o peak extremadamente alto detectado".to_string());
    }
    if !feedback.trim().is_empty() {
        issues.push("Feedback del artista requiere ajustes sutiles".to_string());
    }
    if analysis.integrated_lufs.is_none() {
        issues.push("Medicion de LUFS no disponible".to_string());
    }
    if issues.is_empty() {
        issues.push("No se detectan problemas tecnicos severos".to_string());
    }
    issues
}

fn diagnosis_summary(analysis: &AudioAnalysis) -> &'static str {
    if already_mastered(analysis) {
        "El track ya viene con bastante nivel. La recomendacion es limpiar subgrave/DC, cuidar true peak y evitar empujarlo mas de lo necesario."
    } else if analysis.integrated_lufs.is_some() {
        "El track conserva margen para un master controlado. Conviene trabajar con movimientos amplios, poca compresion y limitacion final segura."
    } else {
        "No se pudo medir loudness completo; se recomienda una cadena conservadora con limpieza subsonica y control de peak."
    }
}

fn risk_level(analysis: &AudioAnalysis, profile: &MasteringProfile) -> &'static str {
    if analysis.clipping_detected.unwrap_or(false)
        || analysis
            .true_peak_dbfs
            .is_some_and(|true_peak| true_peak >= -0.1)
    {
        "high"
    } else if already_mastered(analysis)
        || analysis
            .true_peak_dbfs
            .is_some_and(|true_peak| true_peak > profile.true_peak_ceiling_db)
    {
        "medium"
    } else {
        "low"
    }
}

fn already_mastered(analysis: &AudioAnalysis) -> bool {
    analysis.integrated_lufs.is_some_and(|lufs| lufs >= -10.0)
        || analysis.true_peak_dbfs.is_some_and(|peak| peak > -0.7)
}

fn analysis_summary(analysis: &AudioAnalysis) -> String {
    format!(
        "LUFS {}, true peak {} dB, clipping {}",
        display_option(analysis.integrated_lufs),
        display_option(analysis.true_peak_dbfs),
        analysis
            .clipping_detected
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/d".to_string())
    )
}

fn display_option(value: Option<f64>) -> String {
    value
        .map(|value| round2(value).to_string())
        .unwrap_or_else(|| "n/d".to_string())
}

fn low_cut_requested(text: &str) -> bool {
    contains_any(
        text,
        &[
            "grave", "bajo", "sub", "boomy", "retumba", "turbio", "muddy",
        ],
    )
}

fn harshness_requested(text: &str) -> bool {
    contains_any(
        text,
        &[
            "aspero", "áspero", "agresiv", "duro", "chillon", "chillón", "sibil", "filoso",
        ],
    )
}

fn high_cut_requested(text: &str) -> bool {
    contains_any(
        text,
        &[
            "agudo",
            "agudos",
            "alto",
            "altos",
            "treble",
            "brillo",
            "brillante",
        ],
    ) && contains_any(
        text,
        &[
            "corta", "cortar", "recorta", "recortar", "quita", "quitar", "saca", "sacar", "menos",
            "baja", "bajar", "reduce", "reducir", "oscuro", "oscurece",
        ],
    )
}

fn high_boost_requested(text: &str) -> bool {
    contains_any(
        text,
        &[
            "opaco",
            "apagado",
            "aire",
            "abierto",
            "abrir",
            "brillo",
            "brillante",
        ],
    ) && !high_cut_requested(text)
        && !harshness_requested(text)
}

fn saturation_requested(text: &str) -> bool {
    contains_any(
        text,
        &["calid", "cálid", "dens", "color", "analog", "satur"],
    )
}

fn extreme_request(text: &str) -> bool {
    contains_any(
        text,
        &[
            "maximo", "máximo", "full", "mucho", "todo", "total", "extremo", "agresivo",
        ],
    )
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn normalized_feedback(feedback: &str, reference_notes: &str) -> String {
    format!("{feedback} {reference_notes}").to_lowercase()
}

fn normalize_string_array(value: Option<&Value>, limit: usize) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .take(limit)
        .map(ToString::to_string)
        .collect()
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_output_format(value: Option<&str>) -> String {
    match value.unwrap_or("aiff_24") {
        "aiff_cdj16" => "aiff_cdj16".to_string(),
        _ => "aiff_24".to_string(),
    }
}

fn normalized_metadata_json(source_name: &str, metadata: Option<MasteringMetadata>) -> Value {
    let metadata = metadata.unwrap_or_default();
    let title = clean_optional(metadata.title).unwrap_or_else(|| safe_stem(source_name));

    json!({
        "title": title,
        "artist": clean_optional(metadata.artist),
        "album": clean_optional(metadata.album),
        "genre": clean_optional(metadata.genre),
        "year": clean_optional(metadata.year),
        "track_number": clean_optional(metadata.track_number),
        "composer": clean_optional(metadata.composer),
        "label": clean_optional(metadata.label),
        "copyright": clean_optional(metadata.copyright),
        "bpm": clean_optional(metadata.bpm),
        "musical_key": clean_optional(metadata.musical_key),
        "isrc": clean_optional(metadata.isrc),
        "comment": clean_optional(metadata.comment)
    })
}

fn clean_cover_art_path(value: Option<String>) -> Result<Option<String>, String> {
    let Some(path) = clean_optional(value) else {
        return Ok(None);
    };
    let cover = PathBuf::from(&path);
    if !cover.is_file() {
        return Err(format!("Caratula no encontrada: {}", cover.display()));
    }
    if !is_cover_art_path(&cover) {
        return Err(format!(
            "Formato de caratula no soportado: {}",
            cover.display()
        ));
    }
    Ok(Some(path))
}

fn is_cover_art_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(extension.as_str(), "jpg" | "jpeg" | "png")
}

fn value_to_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
}

fn value_to_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .map(|value| value as u32)
        .or_else(|| value.as_str().and_then(|value| value.parse::<u32>().ok()))
}

fn number(value: Option<&Value>, fallback: f64) -> f64 {
    value.and_then(value_to_f64).unwrap_or(fallback)
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn db_to_amplitude(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

fn safe_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("track");
    let safe = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if safe.is_empty() {
        "track".to_string()
    } else {
        safe
    }
}
