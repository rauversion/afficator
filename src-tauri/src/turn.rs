use crate::system;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const DEFAULT_BACKGROUND_COLOR: &str = "#faafc8";
const MASK_SIZE: usize = 1080;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnJob {
    id: String,
    cover_image_path: String,
    cover_image_name: String,
    audio_file_path: String,
    audio_file_name: String,
    output_path: Option<String>,
    state: String,
    duration_seconds: f64,
    loop_speed: f64,
    audio_start: Option<f64>,
    audio_end: Option<f64>,
    background_color: String,
    disc_size: f64,
    error_message: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    failed_at: Option<String>,
    created_at: String,
    updated_at: String,
    ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnProgressEvent {
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
    job: TurnJob,
    payload: Value,
}

struct EventMeta {
    event: &'static str,
    step: &'static str,
    level: &'static str,
    progress: Option<f64>,
    message: String,
    payload: Value,
}

#[tauri::command]
pub fn turn_list_jobs(app: AppHandle) -> Result<Vec<TurnJob>, String> {
    let conn = open_db(&app)?;
    list_jobs(&conn)
}

#[tauri::command]
pub fn turn_get_job(app: AppHandle, job_id: String) -> Result<TurnJob, String> {
    let conn = open_db(&app)?;
    get_job(&conn, &job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))
}

#[tauri::command]
pub fn turn_job_events(app: AppHandle, job_id: String) -> Result<Vec<TurnProgressEvent>, String> {
    let conn = open_db(&app)?;
    let job =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))?;
    list_job_events(&conn, &job)
}

#[tauri::command]
pub fn turn_start_job(
    app: AppHandle,
    cover_image_path: String,
    audio_file_path: String,
    duration_seconds: f64,
    loop_speed: f64,
    audio_start: Option<f64>,
    audio_end: Option<f64>,
    background_color: String,
    disc_size: f64,
) -> Result<TurnJob, String> {
    let cover = PathBuf::from(&cover_image_path);
    if !cover.is_file() {
        return Err(format!("Cover no encontrado: {}", cover.display()));
    }

    let audio = PathBuf::from(&audio_file_path);
    if !audio.is_file() {
        return Err(format!("Audio no encontrado: {}", audio.display()));
    }

    let conn = open_db(&app)?;
    let now = timestamp();
    let id = Uuid::new_v4().to_string();
    let cover_image_name = file_name(&cover, "cover");
    let audio_file_name = file_name(&audio, "audio");
    let audio_start = normalize_optional_seconds(audio_start);
    let audio_end = normalize_audio_end(audio_start, audio_end);
    let duration_seconds = normalize_duration(duration_seconds, audio_start, audio_end);
    let loop_speed = normalize_loop_speed(loop_speed);
    let background_color = normalize_color(&background_color);
    let disc_size = normalize_disc_size(disc_size);

    conn.execute(
        "INSERT INTO turn_jobs (
            id, cover_image_path, cover_image_name, audio_file_path, audio_file_name,
            state, duration_seconds, loop_speed, audio_start, audio_end, background_color, disc_size,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            id,
            cover_image_path,
            cover_image_name,
            audio_file_path,
            audio_file_name,
            duration_seconds,
            loop_speed,
            audio_start,
            audio_end,
            background_color,
            disc_size,
            now
        ],
    )
    .map_err(|error| format!("No se pudo crear turn job: {error}"))?;

    let job = get_job(&conn, &id)?.ok_or_else(|| "No se pudo leer turn job creado.".to_string())?;
    spawn_turn(app, id);
    Ok(job)
}

#[tauri::command]
pub fn turn_retry_job(app: AppHandle, job_id: String) -> Result<TurnJob, String> {
    let conn = open_db(&app)?;
    let current =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))?;
    if current.state == "pending" || current.state == "running" {
        return Err("El video ya esta en proceso.".to_string());
    }

    let _ = fs::remove_dir_all(job_dir(&app, &job_id)?);
    let now = timestamp();
    conn.execute(
        "UPDATE turn_jobs SET
          state = 'pending',
          output_path = NULL,
          error_message = NULL,
          started_at = NULL,
          completed_at = NULL,
          failed_at = NULL,
          updated_at = ?2
         WHERE id = ?1",
        params![job_id, now],
    )
    .map_err(|error| format!("No se pudo reintentar turn job: {error}"))?;
    conn.execute("DELETE FROM turn_events WHERE job_id = ?1", params![job_id])
        .map_err(|error| format!("No se pudieron limpiar eventos turn: {error}"))?;

    let job = get_job(&conn, &job_id)?
        .ok_or_else(|| "No se pudo leer turn job actualizado.".to_string())?;
    spawn_turn(app, job_id);
    Ok(job)
}

#[tauri::command]
pub fn turn_delete_job(app: AppHandle, job_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    let current =
        get_job(&conn, &job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))?;
    if current.state == "pending" || current.state == "running" {
        return Err("No se puede eliminar un video en proceso.".to_string());
    }

    conn.execute("DELETE FROM turn_events WHERE job_id = ?1", params![job_id])
        .map_err(|error| format!("No se pudieron eliminar eventos turn: {error}"))?;
    conn.execute("DELETE FROM turn_jobs WHERE id = ?1", params![job_id])
        .map_err(|error| format!("No se pudo eliminar turn job: {error}"))?;
    let _ = fs::remove_dir_all(job_dir(&app, &job_id)?);
    Ok(job_id)
}

fn spawn_turn(app: AppHandle, job_id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = process_turn(&app, &job_id) {
            if let Ok(conn) = open_db(&app) {
                let _ = mark_failed(&conn, &job_id, &error);
                if let Ok(Some(job)) = get_job(&conn, &job_id) {
                    let _ = emit_event(
                        &app,
                        &conn,
                        &job,
                        EventMeta {
                            event: "failed",
                            step: "render",
                            level: "error",
                            progress: None,
                            message: error,
                            payload: json!({}),
                        },
                    );
                }
            }
        }
    });
}

fn process_turn(app: &AppHandle, job_id: &str) -> Result<(), String> {
    let conn = open_db(app)?;
    mark_running(&conn, job_id)?;
    let job = get_job(&conn, job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))?;

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "started",
            step: "prepare",
            level: "info",
            progress: Some(5.0),
            message: "Preparando video turn.".to_string(),
            payload: json!({}),
        },
    )?;

    let dir = job_dir(app, job_id)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("No se pudo crear carpeta del video: {error}"))?;
    let mask_path = ensure_turn_mask(app)?;
    let output_path = final_output_path(&dir, &job);
    let _ = fs::remove_file(&output_path);

    emit_event(
        app,
        &conn,
        &job,
        EventMeta {
            event: "render_started",
            step: "ffmpeg",
            level: "info",
            progress: Some(12.0),
            message: "Renderizando MP4 1080x1080 con ffmpeg.".to_string(),
            payload: json!({
                "cover": job.cover_image_path,
                "audio": job.audio_file_path,
                "output": output_path.to_string_lossy(),
            }),
        },
    )?;

    run_ffmpeg_turn(app, &conn, &job, &mask_path, &output_path)?;

    if !output_path.is_file() {
        return Err("ffmpeg termino sin generar el archivo MP4.".to_string());
    }

    let output_path_text = output_path.to_string_lossy().into_owned();
    mark_completed(&conn, job_id, &output_path_text)?;
    let completed =
        get_job(&conn, job_id)?.ok_or_else(|| format!("Turn job no encontrado: {job_id}"))?;
    emit_event(
        app,
        &conn,
        &completed,
        EventMeta {
            event: "completed",
            step: "done",
            level: "info",
            progress: Some(100.0),
            message: "Video listo.".to_string(),
            payload: json!({ "output_path": output_path_text }),
        },
    )?;

    Ok(())
}

fn run_ffmpeg_turn(
    app: &AppHandle,
    conn: &Connection,
    job: &TurnJob,
    mask_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let duration = format_seconds(job.duration_seconds);
    let background = ffmpeg_color(&job.background_color);
    let background_input = format!("color=c={background}:s=1080x1080");
    let filter_complex = turn_filter(job);

    let mut command = system::ffmpeg_command(app);
    command
        .args(["-hide_banner", "-f", "lavfi", "-t"])
        .arg(&duration)
        .args(["-i"])
        .arg(background_input)
        .args(["-loop", "1", "-t"])
        .arg(&duration)
        .args(["-i"])
        .arg(&job.cover_image_path)
        .args(["-i"])
        .arg(mask_path);

    if let Some(start) = job.audio_start {
        command.args(["-ss", &format_seconds(start)]);
    }
    if let Some(end) = job.audio_end {
        command.args(["-to", &format_seconds(end)]);
    }

    command
        .args(["-i"])
        .arg(&job.audio_file_path)
        .args(["-filter_complex"])
        .arg(filter_complex)
        .args([
            "-map",
            "[vout]",
            "-map",
            "3:a:0",
            "-t",
            &duration,
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
        ])
        .arg(output_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|error| {
        format!("No se pudo ejecutar ffmpeg. Revisa que este instalado en PATH: {error}")
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "No se pudo leer progreso de ffmpeg.".to_string())?;
    let stderr = child.stderr.take();
    let stderr_handle = stderr.map(|stderr| {
        let app = app.clone();
        let job = job.clone();
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

                if let Ok(conn) = open_db(&app) {
                    let _ = emit_event(
                        &app,
                        &conn,
                        &job,
                        EventMeta {
                            event: "ffmpeg_log",
                            step: "ffmpeg",
                            level: "info",
                            progress: None,
                            message: format!("ffmpeg: {line}"),
                            payload: json!({}),
                        },
                    );
                }
                lines.push(line);
            }

            lines.join("\n")
        })
    });

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
                let progress = turn_progress(elapsed_seconds, job.duration_seconds);
                emit_event(
                    app,
                    conn,
                    job,
                    EventMeta {
                        event: "render_progress",
                        step: "ffmpeg",
                        level: "info",
                        progress,
                        message: if value == "end" {
                            "Finalizando render.".to_string()
                        } else {
                            "Renderizando video.".to_string()
                        },
                        payload: json!({
                            "elapsed_seconds": elapsed_seconds,
                            "speed": speed,
                        }),
                    },
                )?;
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

    Ok(())
}

fn turn_filter(job: &TurnJob) -> String {
    let mask_px = ((MASK_SIZE as f64) * (job.disc_size / 100.0))
        .round()
        .clamp(1.0, MASK_SIZE as f64) as usize;
    let cover_px = ((mask_px as f64) * 1.05)
        .round()
        .clamp(1.0, MASK_SIZE as f64) as usize;
    let loop_speed = format_float(job.loop_speed);

    format!(
        "[0:v]scale=1296:1296,crop=1080:1080:((in_w-out_w)/2):((in_h-out_h)/2)[bg];\
         [1:v]scale={cover_px}:{cover_px},pad=1080:1080:(ow-iw)/2:(oh-ih)/2,format=rgba[scaled];\
         [scaled]rotate=2*PI*{loop_speed}*t/60:c=none:ow=rotw(1080):oh=roth(1080),scale=1080:1080[rotated];\
         [2:v]scale={mask_px}:{mask_px},pad=1080:1080:(ow-iw)/2:(oh-ih)/2,format=gray[mask];\
         [rotated][mask]alphamerge[masked];\
         [bg][masked]overlay=0:0:format=auto,format=yuv420p[vout]"
    )
}

fn emit_event(
    app: &AppHandle,
    conn: &Connection,
    job: &TurnJob,
    meta: EventMeta,
) -> Result<(), String> {
    let now = timestamp();
    let event_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO turn_events (id, job_id, event, step, level, message, progress, payload_json, created_at)
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
    .map_err(|error| format!("No se pudo guardar evento turn: {error}"))?;

    let payload = TurnProgressEvent {
        event_type: "turn_progress".to_string(),
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

    app.emit("turn-progress", payload)
        .map_err(|error| format!("No se pudo emitir evento turn-progress: {error}"))
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
        CREATE TABLE IF NOT EXISTS turn_jobs (
          id TEXT PRIMARY KEY,
          cover_image_path TEXT NOT NULL,
          cover_image_name TEXT NOT NULL,
          audio_file_path TEXT NOT NULL,
          audio_file_name TEXT NOT NULL,
          output_path TEXT,
          state TEXT NOT NULL,
          duration_seconds REAL NOT NULL,
          loop_speed REAL NOT NULL,
          audio_start REAL,
          audio_end REAL,
          background_color TEXT NOT NULL,
          disc_size REAL NOT NULL,
          error_message TEXT,
          started_at TEXT,
          completed_at TEXT,
          failed_at TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_turn_jobs_created_at ON turn_jobs(created_at);
        CREATE INDEX IF NOT EXISTS idx_turn_jobs_state ON turn_jobs(state);

        CREATE TABLE IF NOT EXISTS turn_events (
          id TEXT PRIMARY KEY,
          job_id TEXT NOT NULL,
          event TEXT NOT NULL,
          step TEXT NOT NULL,
          level TEXT NOT NULL,
          message TEXT NOT NULL,
          progress REAL,
          payload_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          FOREIGN KEY(job_id) REFERENCES turn_jobs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_turn_events_job_created_at ON turn_events(job_id, created_at);
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite turn: {error}"))
}

fn list_jobs(conn: &Connection) -> Result<Vec<TurnJob>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, cover_image_path, cover_image_name, audio_file_path, audio_file_name,
                    output_path, state, duration_seconds, loop_speed, audio_start, audio_end,
                    background_color, disc_size, error_message, started_at, completed_at,
                    failed_at, created_at, updated_at
             FROM turn_jobs
             ORDER BY created_at DESC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de turn jobs: {error}"))?;

    let rows = stmt
        .query_map([], row_to_job)
        .map_err(|error| format!("No se pudo leer turn jobs: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear turn jobs: {error}"))
}

fn get_job(conn: &Connection, job_id: &str) -> Result<Option<TurnJob>, String> {
    conn.query_row(
        "SELECT id, cover_image_path, cover_image_name, audio_file_path, audio_file_name,
                output_path, state, duration_seconds, loop_speed, audio_start, audio_end,
                background_color, disc_size, error_message, started_at, completed_at,
                failed_at, created_at, updated_at
         FROM turn_jobs
         WHERE id = ?1",
        params![job_id],
        row_to_job,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer turn job: {error}"))
}

fn list_job_events(conn: &Connection, job: &TurnJob) -> Result<Vec<TurnProgressEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_id, event, step, level, message, progress, payload_json, created_at
             FROM turn_events
             WHERE job_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|error| format!("No se pudo preparar consulta de eventos turn: {error}"))?;

    let rows = stmt
        .query_map(params![&job.id], |row| {
            let payload_json: String = row.get(7)?;
            Ok(TurnProgressEvent {
                event_type: "turn_progress".to_string(),
                id: row.get(0)?,
                job_id: row.get(1)?,
                event: row.get(2)?,
                step: row.get(3)?,
                level: row.get(4)?,
                message: row.get(5)?,
                progress: row.get(6)?,
                payload: parse_json_text(&payload_json),
                timestamp: row.get(8)?,
                job: job.clone(),
            })
        })
        .map_err(|error| format!("No se pudieron leer eventos turn: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear eventos turn: {error}"))
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<TurnJob> {
    let output_path: Option<String> = row.get(5)?;
    let state: String = row.get(6)?;
    Ok(TurnJob {
        id: row.get(0)?,
        cover_image_path: row.get(1)?,
        cover_image_name: row.get(2)?,
        audio_file_path: row.get(3)?,
        audio_file_name: row.get(4)?,
        ready: state == "completed" && output_path.is_some(),
        output_path,
        state,
        duration_seconds: row.get(7)?,
        loop_speed: row.get(8)?,
        audio_start: row.get(9)?,
        audio_end: row.get(10)?,
        background_color: row.get(11)?,
        disc_size: row.get(12)?,
        error_message: row.get(13)?,
        started_at: row.get(14)?,
        completed_at: row.get(15)?,
        failed_at: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn mark_running(conn: &Connection, job_id: &str) -> Result<(), String> {
    let now = timestamp();
    conn.execute(
        "UPDATE turn_jobs SET state = 'running', started_at = ?2, completed_at = NULL, failed_at = NULL, error_message = NULL, updated_at = ?2 WHERE id = ?1",
        params![job_id, now],
    )
    .map_err(|error| format!("No se pudo marcar turn running: {error}"))?;
    Ok(())
}

fn mark_completed(conn: &Connection, job_id: &str, output_path: &str) -> Result<(), String> {
    let now = timestamp();
    conn.execute(
        "UPDATE turn_jobs SET state = 'completed', output_path = ?2, completed_at = ?3, failed_at = NULL, error_message = NULL, updated_at = ?3 WHERE id = ?1",
        params![job_id, output_path, now],
    )
    .map_err(|error| format!("No se pudo marcar turn completed: {error}"))?;
    Ok(())
}

fn mark_failed(conn: &Connection, job_id: &str, message: &str) -> Result<(), String> {
    let now = timestamp();
    let bounded = message.chars().take(1200).collect::<String>();
    conn.execute(
        "UPDATE turn_jobs SET state = 'failed', failed_at = ?2, error_message = ?3, updated_at = ?2 WHERE id = ?1",
        params![job_id, now, bounded],
    )
    .map_err(|error| format!("No se pudo marcar turn failed: {error}"))?;
    Ok(())
}

fn ensure_turn_mask(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?.join("turn");
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear carpeta turn: {error}"))?;
    let path = dir.join("alpha-mask.pgm");
    if path.is_file() {
        return Ok(path);
    }

    let mut file = fs::File::create(&path)
        .map_err(|error| format!("No se pudo crear mascara turn: {error}"))?;
    file.write_all(format!("P5\n{MASK_SIZE} {MASK_SIZE}\n255\n").as_bytes())
        .map_err(|error| format!("No se pudo escribir header de mascara turn: {error}"))?;

    let center = (MASK_SIZE as f64 - 1.0) / 2.0;
    let radius = center;
    let radius_sq = radius * radius;
    let mut row = Vec::with_capacity(MASK_SIZE);
    for y in 0..MASK_SIZE {
        row.clear();
        for x in 0..MASK_SIZE {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            row.push(if dx * dx + dy * dy <= radius_sq {
                255
            } else {
                0
            });
        }
        file.write_all(&row)
            .map_err(|error| format!("No se pudo escribir mascara turn: {error}"))?;
    }

    Ok(path)
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn job_dir(app: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("turn").join("jobs").join(job_id))
}

fn final_output_path(dir: &Path, job: &TurnJob) -> PathBuf {
    let stem = Path::new(&job.cover_image_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(safe_file_stem)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "turn".to_string());
    dir.join(format!("turn-{stem}.mp4"))
}

fn file_name(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(80)
        .collect()
}

fn normalize_optional_seconds(value: Option<f64>) -> Option<f64> {
    value
        .filter(|number| number.is_finite() && *number >= 0.0)
        .map(|number| round3(number.min(3600.0)))
}

fn normalize_audio_end(audio_start: Option<f64>, audio_end: Option<f64>) -> Option<f64> {
    let end = normalize_optional_seconds(audio_end)?;
    if let Some(start) = audio_start {
        if end <= start {
            return None;
        }
    }
    Some(end)
}

fn normalize_duration(
    duration_seconds: f64,
    audio_start: Option<f64>,
    audio_end: Option<f64>,
) -> f64 {
    if let (Some(start), Some(end)) = (audio_start, audio_end) {
        return round3((end - start).clamp(1.0, 900.0));
    }
    if !duration_seconds.is_finite() {
        return 15.0;
    }
    round3(duration_seconds.clamp(1.0, 900.0))
}

fn normalize_loop_speed(loop_speed: f64) -> f64 {
    if !loop_speed.is_finite() {
        return 33.0;
    }
    round3(loop_speed.clamp(1.0, 78.0))
}

fn normalize_disc_size(disc_size: f64) -> f64 {
    if !disc_size.is_finite() {
        return 75.0;
    }
    round3(disc_size.clamp(20.0, 100.0))
}

fn normalize_color(value: &str) -> String {
    let trimmed = value.trim();
    if is_hex_color(trimmed) {
        return trimmed.to_string();
    }
    if !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return trimmed.to_string();
    }
    DEFAULT_BACKGROUND_COLOR.to_string()
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn ffmpeg_color(value: &str) -> String {
    if is_hex_color(value) {
        format!("0x{}", &value[1..])
    } else {
        value.to_string()
    }
}

fn format_seconds(value: f64) -> String {
    format_float(value.max(0.0))
}

fn format_float(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn parse_ffmpeg_progress_seconds(value: &str) -> Option<f64> {
    let micros = value.trim().parse::<f64>().ok()?;
    if !micros.is_finite() || micros < 0.0 {
        return None;
    }
    Some(micros / 1_000_000.0)
}

fn turn_progress(elapsed_seconds: Option<f64>, total_seconds: f64) -> Option<f64> {
    let elapsed_seconds = elapsed_seconds?;
    if total_seconds <= 0.0 {
        return None;
    }
    let render_progress = ((elapsed_seconds / total_seconds) * 100.0).clamp(0.0, 100.0);
    Some((12.0 + render_progress * 0.84).clamp(12.0, 96.0))
}

fn stderr_tail(stderr: &str) -> String {
    let lines = stderr
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(6)
        .collect::<Vec<_>>();

    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn parse_json_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({}))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}
