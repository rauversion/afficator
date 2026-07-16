use crate::{settings, system};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

const DB_FILE: &str = "aifficator.sqlite3";
const PROFILE_ID: &str = "default";
const PCM_SAMPLE_RATE: usize = 44_100;
const PCM_CHANNELS: usize = 2;
const PCM_BYTES_PER_SAMPLE: usize = 2;
const SILENCE_CHUNK_MILLIS: usize = 250;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastProfileInput {
    host: String,
    port: u16,
    mount: String,
    username: String,
    station_name: String,
    description: String,
    bitrate_kbps: u16,
    tls: bool,
    public: bool,
    password: Option<String>,
    clear_password: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastProfile {
    id: String,
    host: String,
    port: u16,
    mount: String,
    username: String,
    station_name: String,
    description: String,
    bitrate_kbps: u16,
    tls: bool,
    public: bool,
    password_configured: bool,
    listener_url: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastQueueEntry {
    id: String,
    library_id: String,
    track_id: String,
    playlist_path: String,
    playlist_name: String,
    source_path: String,
    title: String,
    artist: Option<String>,
    duration_seconds: Option<u64>,
    position: i64,
    status: String,
    error: Option<String>,
    inserted_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastQueueAppendResult {
    appended_total: usize,
    skipped_missing_total: usize,
    queue: Vec<BroadcastQueueEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastPreflight {
    ffmpeg_available: bool,
    mp3_encoder_available: bool,
    icecast_protocol_available: bool,
    tls_protocol_available: bool,
    ready: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastStatus {
    status: String,
    message: String,
    now_playing: Option<BroadcastQueueEntry>,
    started_at: Option<String>,
    updated_at: String,
}

impl Default for BroadcastStatus {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            message: "Radio detenida.".to_string(),
            now_playing: None,
            started_at: None,
            updated_at: timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BroadcastProgressEvent {
    level: String,
    event: String,
    message: String,
    status: BroadcastStatus,
    timestamp: String,
}

struct RuntimeState {
    snapshot: Mutex<BroadcastStatus>,
}

impl RuntimeState {
    fn snapshot(&self) -> BroadcastStatus {
        self.snapshot
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    fn update(
        &self,
        app: &AppHandle,
        status: &str,
        message: impl Into<String>,
        now_playing: Option<BroadcastQueueEntry>,
        started_at: Option<String>,
        event_context: (&str, &str),
    ) {
        let (level, event) = event_context;
        let message = message.into();
        let snapshot = BroadcastStatus {
            status: status.to_string(),
            message: message.clone(),
            now_playing,
            started_at,
            updated_at: timestamp(),
        };
        if let Ok(mut current) = self.snapshot.lock() {
            *current = snapshot.clone();
        }
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: level.to_string(),
                event: event.to_string(),
                message,
                status: snapshot,
                timestamp: timestamp(),
            },
        );
    }

    fn log(&self, app: &AppHandle, level: &str, event: &str, message: impl Into<String>) {
        let message = message.into();
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: level.to_string(),
                event: event.to_string(),
                message,
                status: self.snapshot(),
                timestamp: timestamp(),
            },
        );
    }
}

enum WorkerCommand {
    Stop,
    Skip,
}

struct WorkerHandle {
    commands: Sender<WorkerCommand>,
    join: thread::JoinHandle<()>,
}

pub struct BroadcastManager {
    runtime: Arc<RuntimeState>,
    worker: Mutex<Option<WorkerHandle>>,
}

impl Default for BroadcastManager {
    fn default() -> Self {
        Self {
            runtime: Arc::new(RuntimeState {
                snapshot: Mutex::new(BroadcastStatus::default()),
            }),
            worker: Mutex::new(None),
        }
    }
}

impl BroadcastManager {
    fn cleanup_finished_worker(&self) {
        let finished = self
            .worker
            .lock()
            .ok()
            .and_then(|worker| worker.as_ref().map(|handle| handle.join.is_finished()))
            .unwrap_or(false);
        if !finished {
            return;
        }

        if let Ok(mut worker) = self.worker.lock() {
            if let Some(handle) = worker.take() {
                let _ = handle.join.join();
            }
        }
    }

    fn start(&self, app: AppHandle) -> Result<BroadcastStatus, String> {
        self.cleanup_finished_worker();
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| "No se pudo bloquear el motor de broadcast.".to_string())?;
        if worker.is_some() {
            return Err("El broadcast ya esta iniciado o deteniendose.".to_string());
        }

        let profile = load_profile(&app)?;
        let password = settings::load_icecast_source_password(&app)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Configura la contraseña source de Icecast.".to_string())?;
        let preflight = ffmpeg_preflight(&app, profile.tls);
        if !preflight.ready {
            return Err(preflight.message);
        }

        let mut conn = open_db(&app)?;
        reset_interrupted_entries(&mut conn)?;
        let (sender, receiver) = mpsc::channel();
        let runtime = Arc::clone(&self.runtime);
        let started_at = timestamp();
        runtime.update(
            &app,
            "connecting",
            format!(
                "Conectando con {}:{}{}...",
                profile.host, profile.port, profile.mount
            ),
            None,
            Some(started_at.clone()),
            ("info", "connecting"),
        );
        let join = thread::spawn(move || {
            run_worker(app, profile, password, runtime, receiver, started_at)
        });
        *worker = Some(WorkerHandle {
            commands: sender,
            join,
        });
        Ok(self.runtime.snapshot())
    }

    fn stop(&self, app: &AppHandle) -> Result<BroadcastStatus, String> {
        self.cleanup_finished_worker();
        let worker = self
            .worker
            .lock()
            .map_err(|_| "No se pudo bloquear el motor de broadcast.".to_string())?;
        let Some(worker) = worker.as_ref() else {
            return Ok(self.runtime.snapshot());
        };
        worker
            .commands
            .send(WorkerCommand::Stop)
            .map_err(|_| "El motor de broadcast ya se detuvo.".to_string())?;
        let current = self.runtime.snapshot();
        self.runtime.update(
            app,
            "stopping",
            "Deteniendo radio...",
            current.now_playing,
            current.started_at,
            ("info", "stopping"),
        );
        Ok(self.runtime.snapshot())
    }

    fn skip(&self) -> Result<BroadcastStatus, String> {
        self.cleanup_finished_worker();
        let worker = self
            .worker
            .lock()
            .map_err(|_| "No se pudo bloquear el motor de broadcast.".to_string())?;
        let Some(worker) = worker.as_ref() else {
            return Err("La radio no esta transmitiendo.".to_string());
        };
        worker
            .commands
            .send(WorkerCommand::Skip)
            .map_err(|_| "El motor de broadcast ya se detuvo.".to_string())?;
        Ok(self.runtime.snapshot())
    }
}

#[tauri::command]
pub fn broadcast_profile(app: AppHandle) -> Result<BroadcastProfile, String> {
    load_profile(&app)
}

#[tauri::command]
pub fn broadcast_save_profile(
    app: AppHandle,
    profile: BroadcastProfileInput,
) -> Result<BroadcastProfile, String> {
    let input = validate_profile(profile)?;
    let conn = open_db(&app)?;
    let now = timestamp();
    conn.execute(
        "INSERT INTO broadcast_profiles (
           id, host, port, mount, username, station_name, description,
           bitrate_kbps, tls, public, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
           host = excluded.host,
           port = excluded.port,
           mount = excluded.mount,
           username = excluded.username,
           station_name = excluded.station_name,
           description = excluded.description,
           bitrate_kbps = excluded.bitrate_kbps,
           tls = excluded.tls,
           public = excluded.public,
           updated_at = excluded.updated_at",
        params![
            PROFILE_ID,
            input.host,
            input.port,
            input.mount,
            input.username,
            input.station_name,
            input.description,
            input.bitrate_kbps,
            input.tls,
            input.public,
            now,
        ],
    )
    .map_err(|error| format!("No se pudo guardar perfil Icecast: {error}"))?;

    if input.clear_password {
        settings::save_icecast_source_password(&app, None)?;
    } else if let Some(password) = input.password {
        settings::save_icecast_source_password(&app, Some(password))?;
    }

    load_profile(&app)
}

#[tauri::command]
pub fn broadcast_preflight(app: AppHandle) -> BroadcastPreflight {
    let tls = load_profile(&app)
        .map(|profile| profile.tls)
        .unwrap_or(false);
    ffmpeg_preflight(&app, tls)
}

#[tauri::command]
pub fn broadcast_queue(app: AppHandle) -> Result<Vec<BroadcastQueueEntry>, String> {
    let conn = open_db(&app)?;
    list_queue(&conn)
}

#[tauri::command]
pub fn broadcast_append_playlist(
    app: AppHandle,
    library_id: String,
    playlist_path: String,
) -> Result<BroadcastQueueAppendResult, String> {
    let mut conn = open_db(&app)?;
    append_playlist(&mut conn, &library_id, &playlist_path)
}

#[tauri::command]
pub fn broadcast_remove_queue_entry(app: AppHandle, entry_id: String) -> Result<String, String> {
    let conn = open_db(&app)?;
    let deleted = conn
        .execute(
            "DELETE FROM broadcast_queue_entries WHERE id = ?1 AND status != 'playing'",
            params![entry_id],
        )
        .map_err(|error| format!("No se pudo quitar pista del broadcast: {error}"))?;
    if deleted == 0 {
        return Err("No se puede quitar la pista que esta sonando.".to_string());
    }
    Ok("Pista quitada de la cola.".to_string())
}

#[tauri::command]
pub fn broadcast_clear_queue(app: AppHandle) -> Result<usize, String> {
    let conn = open_db(&app)?;
    conn.execute(
        "DELETE FROM broadcast_queue_entries WHERE status != 'playing'",
        [],
    )
    .map_err(|error| format!("No se pudo limpiar cola de broadcast: {error}"))
}

#[tauri::command]
pub fn broadcast_status(manager: State<'_, BroadcastManager>) -> BroadcastStatus {
    manager.cleanup_finished_worker();
    manager.runtime.snapshot()
}

#[tauri::command]
pub fn broadcast_start(
    app: AppHandle,
    manager: State<'_, BroadcastManager>,
) -> Result<BroadcastStatus, String> {
    manager.start(app)
}

#[tauri::command]
pub fn broadcast_stop(
    app: AppHandle,
    manager: State<'_, BroadcastManager>,
) -> Result<BroadcastStatus, String> {
    manager.stop(&app)
}

#[tauri::command]
pub fn broadcast_skip(manager: State<'_, BroadcastManager>) -> Result<BroadcastStatus, String> {
    manager.skip()
}

fn validate_profile(mut input: BroadcastProfileInput) -> Result<BroadcastProfileInput, String> {
    input.host = input.host.trim().to_string();
    input.mount = input.mount.trim().to_string();
    input.username = input.username.trim().to_string();
    input.station_name = input.station_name.trim().to_string();
    input.description = input.description.trim().to_string();
    input.password = input
        .password
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if input.host.is_empty()
        || input.host.chars().any(char::is_whitespace)
        || input.host.contains('/')
        || input.host.contains('@')
    {
        return Err("Host Icecast invalido.".to_string());
    }
    if input.port == 0 {
        return Err("Puerto Icecast invalido.".to_string());
    }
    if !input.mount.starts_with('/')
        || input.mount.len() < 2
        || input.mount.chars().any(char::is_whitespace)
        || input.mount.contains('?')
        || input.mount.contains('#')
    {
        return Err("Mountpoint invalido. Usa un valor como /live.mp3.".to_string());
    }
    if input.username.is_empty()
        || input.username.chars().any(char::is_whitespace)
        || input.username.contains(['@', ':', '/', '\\'])
    {
        return Err("Usuario source de Icecast invalido.".to_string());
    }
    if input.station_name.is_empty() || input.station_name.len() > 120 {
        return Err("Nombre de estación invalido.".to_string());
    }
    if !(64..=320).contains(&input.bitrate_kbps) {
        return Err("El bitrate MP3 debe estar entre 64 y 320 kbps.".to_string());
    }
    Ok(input)
}

fn load_profile(app: &AppHandle) -> Result<BroadcastProfile, String> {
    let conn = open_db(app)?;
    let stored = conn
        .query_row(
            "SELECT id, host, port, mount, username, station_name, description,
                    bitrate_kbps, tls, public, updated_at
             FROM broadcast_profiles WHERE id = ?1",
            params![PROFILE_ID],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u16>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, u16>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("No se pudo leer perfil Icecast: {error}"))?;
    let (
        id,
        host,
        port,
        mount,
        username,
        station_name,
        description,
        bitrate,
        tls,
        public,
        updated_at,
    ) = stored.unwrap_or_else(|| {
        (
            PROFILE_ID.to_string(),
            "127.0.0.1".to_string(),
            8000,
            "/live.mp3".to_string(),
            "source".to_string(),
            "Rau Studio Radio".to_string(),
            "Broadcast local desde Rau Studio".to_string(),
            128,
            false,
            false,
            timestamp(),
        )
    });
    let password_configured = settings::load_icecast_source_password(app)?.is_some();
    let scheme = if tls { "https" } else { "http" };
    let listener_url = format!("{scheme}://{host}:{port}{mount}");
    Ok(BroadcastProfile {
        id,
        host,
        port,
        mount,
        username,
        station_name,
        description,
        bitrate_kbps: bitrate,
        tls,
        public,
        password_configured,
        listener_url,
        updated_at,
    })
}

fn ffmpeg_preflight(app: &AppHandle, tls_required: bool) -> BroadcastPreflight {
    let encoders = system::ffmpeg_command(app)
        .args(["-hide_banner", "-encoders"])
        .output();
    let protocols = system::ffmpeg_command(app)
        .args(["-hide_banner", "-protocols"])
        .output();
    let ffmpeg_available = encoders.is_ok() && protocols.is_ok();
    let encoder_text = encoders
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let protocol_text = protocols
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let mp3_encoder_available = encoder_text.lines().any(|line| line.contains("libmp3lame"));
    let icecast_protocol_available = protocol_list_contains(&protocol_text, "icecast");
    let tls_protocol_available = protocol_list_contains(&protocol_text, "tls");
    let ready = ffmpeg_available
        && mp3_encoder_available
        && icecast_protocol_available
        && (!tls_required || tls_protocol_available);
    let message = if ready {
        "FFmpeg esta listo para transmitir MP3 a Icecast.".to_string()
    } else if !ffmpeg_available {
        "FFmpeg no esta disponible.".to_string()
    } else if !mp3_encoder_available {
        "FFmpeg no incluye el encoder libmp3lame requerido para MP3.".to_string()
    } else if !icecast_protocol_available {
        "FFmpeg no incluye el protocolo de salida icecast.".to_string()
    } else {
        "FFmpeg no incluye TLS, pero el perfil Icecast exige conexión segura.".to_string()
    };
    BroadcastPreflight {
        ffmpeg_available,
        mp3_encoder_available,
        icecast_protocol_available,
        tls_protocol_available,
        ready,
        message,
    }
}

fn protocol_list_contains(output: &str, protocol: &str) -> bool {
    output.lines().any(|line| line.trim() == protocol)
}

fn publisher_args(profile: &BroadcastProfile, password: &str) -> Vec<String> {
    let destination = format!(
        "icecast://{}@{}:{}{}",
        profile.username, profile.host, profile.port, profile.mount
    );
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-nostdin".to_string(),
        "-re".to_string(),
        "-f".to_string(),
        "s16le".to_string(),
        "-ar".to_string(),
        PCM_SAMPLE_RATE.to_string(),
        "-ac".to_string(),
        PCM_CHANNELS.to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-map".to_string(),
        "0:a:0".to_string(),
        "-c:a".to_string(),
        "libmp3lame".to_string(),
        "-b:a".to_string(),
        format!("{}k", profile.bitrate_kbps),
        "-content_type".to_string(),
        "audio/mpeg".to_string(),
        "-ice_name".to_string(),
        profile.station_name.clone(),
        "-ice_description".to_string(),
        profile.description.clone(),
        "-ice_public".to_string(),
        if profile.public { "1" } else { "0" }.to_string(),
        "-password".to_string(),
        password.to_string(),
    ];
    if profile.tls {
        args.extend(["-tls".to_string(), "1".to_string()]);
    }
    args.extend([
        "-flush_packets".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "mp3".to_string(),
        destination,
    ]);
    args
}

fn decoder_args(path: &str) -> Vec<String> {
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        path.to_string(),
        "-map".to_string(),
        "0:a:0".to_string(),
        "-vn".to_string(),
        "-sn".to_string(),
        "-dn".to_string(),
        "-c:a".to_string(),
        "pcm_s16le".to_string(),
        "-ar".to_string(),
        PCM_SAMPLE_RATE.to_string(),
        "-ac".to_string(),
        PCM_CHANNELS.to_string(),
        "-f".to_string(),
        "s16le".to_string(),
        "pipe:1".to_string(),
    ]
}

struct Publisher {
    child: Child,
    stdin: ChildStdin,
}

impl Publisher {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| format!("No se pudo revisar publisher FFmpeg: {error}"))?
        {
            return Err(format!("Publisher FFmpeg termino con estado {status}."));
        }
        self.stdin
            .write_all(bytes)
            .map_err(|error| format!("Se perdio la conexión con Icecast: {error}"))
    }

    fn terminate(mut self) {
        drop(self.stdin);
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn spawn_publisher(
    app: &AppHandle,
    profile: &BroadcastProfile,
    password: &str,
    runtime: &Arc<RuntimeState>,
) -> Result<Publisher, String> {
    let mut child = system::ffmpeg_command(app)
        .args(publisher_args(profile, password))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("No se pudo iniciar publisher FFmpeg: {error}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "No se pudo abrir stdin del publisher FFmpeg.".to_string())?;
    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        let runtime = Arc::clone(runtime);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    runtime.log(
                        &app,
                        "warning",
                        "ffmpeg_publisher",
                        format!("FFmpeg: {line}"),
                    );
                }
            }
        });
    }
    Ok(Publisher { child, stdin })
}

enum PlayOutcome {
    Completed,
    Skipped,
    Stop,
    PublisherFailed(String),
}

struct IcecastSession<'a> {
    profile: &'a BroadcastProfile,
    password: &'a str,
    started_at: &'a str,
}

fn play_entry(
    app: &AppHandle,
    entry: &BroadcastQueueEntry,
    session: &IcecastSession<'_>,
    publisher: &mut Publisher,
    runtime: &Arc<RuntimeState>,
    commands: &Receiver<WorkerCommand>,
) -> PlayOutcome {
    if let Err(error) = update_entry_status(app, &entry.id, "playing", None) {
        runtime.log(app, "error", "queue", error);
        return PlayOutcome::Completed;
    }
    let mut playing = entry.clone();
    playing.status = "playing".to_string();
    playing.updated_at = timestamp();
    runtime.update(
        app,
        "live",
        format!("En vivo: {}", display_title(&playing)),
        Some(playing.clone()),
        Some(session.started_at.to_string()),
        ("info", "track_started"),
    );
    update_icecast_metadata_async(
        session.profile.clone(),
        session.password.to_string(),
        playing.clone(),
        runtime,
        app.clone(),
    );

    let mut decoder = match system::ffmpeg_command(app)
        .args(decoder_args(&entry.source_path))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let message = format!("No se pudo decodificar {}: {error}", entry.source_path);
            let _ = update_entry_status(app, &entry.id, "failed", Some(&message));
            runtime.log(app, "error", "decoder", message);
            return PlayOutcome::Completed;
        }
    };
    let mut stdout = match decoder.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = decoder.kill();
            let message = "No se pudo leer audio decodificado desde FFmpeg.";
            let _ = update_entry_status(app, &entry.id, "failed", Some(message));
            return PlayOutcome::Completed;
        }
    };
    if let Some(stderr) = decoder.stderr.take() {
        let app = app.clone();
        let runtime = Arc::clone(runtime);
        let title = display_title(entry);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    runtime.log(&app, "error", "ffmpeg_decoder", format!("{title}: {line}"));
                }
            }
        });
    }

    let mut buffer = [0u8; 16 * 1024];
    loop {
        match poll_command(commands) {
            Some(WorkerCommand::Stop) => {
                let _ = decoder.kill();
                let _ = decoder.wait();
                let _ = update_entry_status(app, &entry.id, "queued", None);
                return PlayOutcome::Stop;
            }
            Some(WorkerCommand::Skip) => {
                let _ = decoder.kill();
                let _ = decoder.wait();
                let _ = update_entry_status(app, &entry.id, "skipped", None);
                runtime.log(
                    app,
                    "info",
                    "track_skipped",
                    format!("Saltada: {}", display_title(entry)),
                );
                return PlayOutcome::Skipped;
            }
            None => {}
        }

        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if let Err(error) = publisher.write(&buffer[..read]) {
                    let _ = decoder.kill();
                    let _ = decoder.wait();
                    let _ = update_entry_status(app, &entry.id, "queued", None);
                    return PlayOutcome::PublisherFailed(error);
                }
            }
            Err(error) => {
                let _ = decoder.kill();
                let _ = decoder.wait();
                let message = format!("No se pudo leer audio de {}: {error}", display_title(entry));
                let _ = update_entry_status(app, &entry.id, "failed", Some(&message));
                runtime.log(app, "error", "decoder", message);
                return PlayOutcome::Completed;
            }
        }
    }

    match decoder.wait() {
        Ok(status) if status.success() => {
            let _ = update_entry_status(app, &entry.id, "played", None);
            runtime.log(
                app,
                "info",
                "track_completed",
                format!("Reproducida: {}", display_title(entry)),
            );
        }
        Ok(status) => {
            let message = format!(
                "FFmpeg no pudo reproducir {}: {status}",
                display_title(entry)
            );
            let _ = update_entry_status(app, &entry.id, "failed", Some(&message));
            runtime.log(app, "error", "decoder", message);
        }
        Err(error) => {
            let message = format!("No se pudo esperar decoder FFmpeg: {error}");
            let _ = update_entry_status(app, &entry.id, "failed", Some(&message));
            runtime.log(app, "error", "decoder", message);
        }
    }
    PlayOutcome::Completed
}

fn run_worker(
    app: AppHandle,
    profile: BroadcastProfile,
    password: String,
    runtime: Arc<RuntimeState>,
    commands: Receiver<WorkerCommand>,
    started_at: String,
) {
    let mut reconnect_attempt = 0u32;
    let mut publisher: Option<Publisher> = None;

    loop {
        if matches!(poll_command(&commands), Some(WorkerCommand::Stop)) {
            break;
        }
        if publisher.is_none() {
            match spawn_publisher(&app, &profile, &password, &runtime) {
                Ok(candidate) => {
                    publisher = Some(candidate);
                    reconnect_attempt = 0;
                    runtime.update(
                        &app,
                        "live",
                        "Radio en vivo · esperando audio.",
                        None,
                        Some(started_at.clone()),
                        ("info", "connected"),
                    );
                }
                Err(error) => {
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    if !wait_before_reconnect(
                        &app,
                        &runtime,
                        &commands,
                        &started_at,
                        reconnect_attempt,
                        &error,
                    ) {
                        break;
                    }
                    continue;
                }
            }
        }

        let next = open_db(&app).and_then(|conn| next_queue_entry(&conn));
        match next {
            Ok(Some(entry)) => {
                let session = IcecastSession {
                    profile: &profile,
                    password: &password,
                    started_at: &started_at,
                };
                let outcome = play_entry(
                    &app,
                    &entry,
                    &session,
                    publisher.as_mut().expect("publisher initialized"),
                    &runtime,
                    &commands,
                );
                match outcome {
                    PlayOutcome::Stop => break,
                    PlayOutcome::PublisherFailed(error) => {
                        if let Some(publisher) = publisher.take() {
                            publisher.terminate();
                        }
                        reconnect_attempt = reconnect_attempt.saturating_add(1);
                        if !wait_before_reconnect(
                            &app,
                            &runtime,
                            &commands,
                            &started_at,
                            reconnect_attempt,
                            &error,
                        ) {
                            break;
                        }
                    }
                    PlayOutcome::Completed | PlayOutcome::Skipped => {}
                }
            }
            Ok(None) => {
                let silence = silence_chunk();
                let result = publisher
                    .as_mut()
                    .expect("publisher initialized")
                    .write(&silence);
                if let Err(error) = result {
                    if let Some(publisher) = publisher.take() {
                        publisher.terminate();
                    }
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    if !wait_before_reconnect(
                        &app,
                        &runtime,
                        &commands,
                        &started_at,
                        reconnect_attempt,
                        &error,
                    ) {
                        break;
                    }
                }
            }
            Err(error) => {
                runtime.log(&app, "error", "queue", error);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }

    if let Some(publisher) = publisher.take() {
        publisher.terminate();
    }
    runtime.update(
        &app,
        "idle",
        "Radio detenida.",
        None,
        None,
        ("info", "stopped"),
    );
}

fn wait_before_reconnect(
    app: &AppHandle,
    runtime: &Arc<RuntimeState>,
    commands: &Receiver<WorkerCommand>,
    started_at: &str,
    attempt: u32,
    reason: &str,
) -> bool {
    let seconds = 2u64.saturating_pow(attempt.min(3)).clamp(1, 15);
    runtime.update(
        app,
        "reconnecting",
        format!("Icecast desconectado. Reintentando en {seconds}s: {reason}"),
        None,
        Some(started_at.to_string()),
        ("warning", "reconnecting"),
    );
    for _ in 0..seconds * 4 {
        if matches!(poll_command(commands), Some(WorkerCommand::Stop)) {
            return false;
        }
        thread::sleep(Duration::from_millis(250));
    }
    true
}

fn poll_command(commands: &Receiver<WorkerCommand>) -> Option<WorkerCommand> {
    match commands.try_recv() {
        Ok(command) => Some(command),
        Err(TryRecvError::Disconnected) => Some(WorkerCommand::Stop),
        Err(TryRecvError::Empty) => None,
    }
}

fn silence_chunk() -> Vec<u8> {
    let bytes = PCM_SAMPLE_RATE * PCM_CHANNELS * PCM_BYTES_PER_SAMPLE * SILENCE_CHUNK_MILLIS / 1000;
    vec![0; bytes]
}

fn display_title(entry: &BroadcastQueueEntry) -> String {
    entry
        .artist
        .as_deref()
        .filter(|artist| !artist.trim().is_empty())
        .map(|artist| format!("{artist} — {}", entry.title))
        .unwrap_or_else(|| entry.title.clone())
}

fn update_icecast_metadata_async(
    profile: BroadcastProfile,
    password: String,
    entry: BroadcastQueueEntry,
    runtime: &Arc<RuntimeState>,
    app: AppHandle,
) {
    let runtime = Arc::clone(runtime);
    thread::spawn(move || {
        let scheme = if profile.tls { "https" } else { "http" };
        let url = format!(
            "{scheme}://{}:{}/admin/metadata",
            profile.host, profile.port
        );
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .and_then(|client| {
                client
                    .get(url)
                    .basic_auth(&profile.username, Some(password))
                    .query(&[
                        ("mount", profile.mount.as_str()),
                        ("mode", "updinfo"),
                        ("song", display_title(&entry).as_str()),
                    ])
                    .send()
            });
        match response {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => runtime.log(
                &app,
                "warning",
                "metadata",
                format!("Icecast rechazo metadata con HTTP {}.", response.status()),
            ),
            Err(error) => runtime.log(
                &app,
                "warning",
                "metadata",
                format!("No se pudo actualizar metadata Icecast: {error}"),
            ),
        }
    });
}

fn append_playlist(
    conn: &mut Connection,
    library_id: &str,
    playlist_path: &str,
) -> Result<BroadcastQueueAppendResult, String> {
    let library_id = library_id.trim();
    let playlist_path = playlist_path.trim();
    if library_id.is_empty() || playlist_path.is_empty() {
        return Err("Selecciona una playlist indexada.".to_string());
    }
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transaccion de broadcast: {error}"))?;
    let playlist_name = tx
        .query_row(
            "SELECT name FROM playlist_index_playlists WHERE library_id = ?1 AND path = ?2",
            params![library_id, playlist_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer playlist: {error}"))?
        .ok_or_else(|| "Playlist indexada no encontrada.".to_string())?;
    let tracks = {
        let mut stmt = tx
            .prepare(
                "SELECT m.track_id, t.source_path, t.name, t.artist, t.total_time, t.source_exists
                 FROM playlist_index_memberships m
                 JOIN playlist_index_tracks t
                   ON t.library_id = m.library_id AND t.track_id = m.track_id
                 WHERE m.library_id = ?1 AND m.playlist_path = ?2
                 ORDER BY m.position, m.track_id",
            )
            .map_err(|error| format!("No se pudo preparar playlist para broadcast: {error}"))?;
        let rows = stmt
            .query_map(params![library_id, playlist_path], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })
            .map_err(|error| format!("No se pudieron leer tracks de playlist: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks de playlist: {error}"))?
    };
    if tracks.is_empty() {
        return Err("La playlist no contiene pistas indexadas.".to_string());
    }

    let mut position = tx
        .query_row(
            "SELECT COALESCE(MAX(position), 0) FROM broadcast_queue_entries",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("No se pudo calcular posición de broadcast: {error}"))?;
    let now = timestamp();
    let mut appended_total = 0usize;
    let mut skipped_missing_total = 0usize;
    for (track_id, source_path, title, artist, duration_seconds, source_exists) in tracks {
        let Some(source_path) = source_path.filter(|value| !value.trim().is_empty()) else {
            skipped_missing_total += 1;
            continue;
        };
        if !source_exists {
            skipped_missing_total += 1;
            continue;
        }
        position += 1;
        tx.execute(
            "INSERT INTO broadcast_queue_entries (
               id, library_id, track_id, playlist_path, playlist_name, source_path,
               title, artist, duration_seconds, position, status, error, inserted_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', NULL, ?11, ?11)",
            params![
                Uuid::new_v4().to_string(),
                library_id,
                track_id,
                playlist_path,
                playlist_name,
                source_path,
                title.unwrap_or_else(|| "Sin título".to_string()),
                artist,
                duration_seconds,
                position,
                now,
            ],
        )
        .map_err(|error| format!("No se pudo agregar pista al broadcast: {error}"))?;
        appended_total += 1;
    }
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar cola de broadcast: {error}"))?;
    Ok(BroadcastQueueAppendResult {
        appended_total,
        skipped_missing_total,
        queue: list_queue(conn)?,
    })
}

fn list_queue(conn: &Connection) -> Result<Vec<BroadcastQueueEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, library_id, track_id, playlist_path, playlist_name, source_path,
                    title, artist, duration_seconds, position, status, error, inserted_at, updated_at
             FROM broadcast_queue_entries ORDER BY position",
        )
        .map_err(|error| format!("No se pudo preparar cola de broadcast: {error}"))?;
    let rows = stmt
        .query_map([], row_to_queue_entry)
        .map_err(|error| format!("No se pudo leer cola de broadcast: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudo mapear cola de broadcast: {error}"))
}

fn next_queue_entry(conn: &Connection) -> Result<Option<BroadcastQueueEntry>, String> {
    conn.query_row(
        "SELECT id, library_id, track_id, playlist_path, playlist_name, source_path,
                title, artist, duration_seconds, position, status, error, inserted_at, updated_at
         FROM broadcast_queue_entries WHERE status = 'queued' ORDER BY position LIMIT 1",
        [],
        row_to_queue_entry,
    )
    .optional()
    .map_err(|error| format!("No se pudo leer siguiente pista de broadcast: {error}"))
}

fn row_to_queue_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<BroadcastQueueEntry> {
    Ok(BroadcastQueueEntry {
        id: row.get(0)?,
        library_id: row.get(1)?,
        track_id: row.get(2)?,
        playlist_path: row.get(3)?,
        playlist_name: row.get(4)?,
        source_path: row.get(5)?,
        title: row.get(6)?,
        artist: row.get(7)?,
        duration_seconds: row.get(8)?,
        position: row.get(9)?,
        status: row.get(10)?,
        error: row.get(11)?,
        inserted_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn update_entry_status(
    app: &AppHandle,
    entry_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let conn = open_db(app)?;
    conn.execute(
        "UPDATE broadcast_queue_entries SET status = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
        params![entry_id, status, error, timestamp()],
    )
    .map_err(|error| format!("No se pudo actualizar pista de broadcast: {error}"))?;
    Ok(())
}

fn reset_interrupted_entries(conn: &mut Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE broadcast_queue_entries SET status = 'queued', updated_at = ?1 WHERE status = 'playing'",
        params![timestamp()],
    )
    .map_err(|error| format!("No se pudo recuperar cola interrumpida: {error}"))?;
    Ok(())
}

fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join(DB_FILE))
        .map_err(|error| format!("No se pudo abrir SQLite broadcast: {error}"))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| format!("No se pudo configurar SQLite broadcast: {error}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS broadcast_profiles (
          id TEXT PRIMARY KEY,
          host TEXT NOT NULL,
          port INTEGER NOT NULL,
          mount TEXT NOT NULL,
          username TEXT NOT NULL,
          station_name TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          bitrate_kbps INTEGER NOT NULL DEFAULT 128,
          tls INTEGER NOT NULL DEFAULT 0,
          public INTEGER NOT NULL DEFAULT 0,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS broadcast_queue_entries (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          track_id TEXT NOT NULL,
          playlist_path TEXT NOT NULL,
          playlist_name TEXT NOT NULL,
          source_path TEXT NOT NULL,
          title TEXT NOT NULL,
          artist TEXT,
          duration_seconds INTEGER,
          position INTEGER NOT NULL,
          status TEXT NOT NULL,
          error TEXT,
          inserted_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          CHECK(status IN ('queued', 'playing', 'played', 'skipped', 'failed'))
        );
        CREATE INDEX IF NOT EXISTS idx_broadcast_queue_status_position
          ON broadcast_queue_entries(status, position);
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite broadcast: {error}"))
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_input() -> BroadcastProfileInput {
        BroadcastProfileInput {
            host: "radio.example.com".to_string(),
            port: 8443,
            mount: "/live.mp3".to_string(),
            username: "source".to_string(),
            station_name: "Test Radio".to_string(),
            description: "Test".to_string(),
            bitrate_kbps: 128,
            tls: true,
            public: false,
            password: Some("secret".to_string()),
            clear_password: false,
        }
    }

    fn profile() -> BroadcastProfile {
        BroadcastProfile {
            id: PROFILE_ID.to_string(),
            host: "radio.example.com".to_string(),
            port: 8443,
            mount: "/live.mp3".to_string(),
            username: "source".to_string(),
            station_name: "Test Radio".to_string(),
            description: "Test".to_string(),
            bitrate_kbps: 128,
            tls: true,
            public: false,
            password_configured: true,
            listener_url: "https://radio.example.com:8443/live.mp3".to_string(),
            updated_at: timestamp(),
        }
    }

    #[test]
    fn validates_icecast_profile_boundaries() {
        assert!(validate_profile(profile_input()).is_ok());
        let mut invalid = profile_input();
        invalid.mount = "live.mp3".to_string();
        assert!(validate_profile(invalid).is_err());
        let mut invalid = profile_input();
        invalid.bitrate_kbps = 32;
        assert!(validate_profile(invalid).is_err());
    }

    #[test]
    fn publisher_uses_persistent_pcm_input_and_mp3_icecast_output() {
        let args = publisher_args(&profile(), "secret");
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "libmp3lame"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-content_type", "audio/mpeg"]));
        assert!(args.windows(2).any(|pair| pair == ["-tls", "1"]));
        assert_eq!(
            args.last().unwrap(),
            "icecast://source@radio.example.com:8443/live.mp3"
        );
    }

    #[test]
    fn silence_chunk_is_exactly_a_quarter_second_of_pcm() {
        assert_eq!(silence_chunk().len(), 44_100);
    }

    #[test]
    fn append_playlist_snapshots_available_tracks_in_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE playlist_index_playlists (
              library_id TEXT NOT NULL, path TEXT NOT NULL, name TEXT NOT NULL,
              PRIMARY KEY(library_id, path)
            );
            CREATE TABLE playlist_index_tracks (
              library_id TEXT NOT NULL, track_id TEXT NOT NULL, source_path TEXT,
              name TEXT, artist TEXT, total_time INTEGER, source_exists INTEGER NOT NULL,
              PRIMARY KEY(library_id, track_id)
            );
            CREATE TABLE playlist_index_memberships (
              library_id TEXT NOT NULL, playlist_path TEXT NOT NULL,
              track_id TEXT NOT NULL, position INTEGER NOT NULL
            );
            INSERT INTO playlist_index_playlists VALUES ('lib', '/set', 'Set');
            INSERT INTO playlist_index_tracks VALUES ('lib', '1', '/music/one.wav', 'One', 'Artist', 10, 1);
            INSERT INTO playlist_index_tracks VALUES ('lib', '2', NULL, 'Missing', NULL, 20, 0);
            INSERT INTO playlist_index_memberships VALUES ('lib', '/set', '1', 0);
            INSERT INTO playlist_index_memberships VALUES ('lib', '/set', '2', 1);
            ",
        )
        .unwrap();

        let result = append_playlist(&mut conn, "lib", "/set").unwrap();
        assert_eq!(result.appended_total, 1);
        assert_eq!(result.skipped_missing_total, 1);
        assert_eq!(result.queue[0].title, "One");
        assert_eq!(result.queue[0].position, 1);
    }
}
