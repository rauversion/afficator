use crate::{settings, system};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
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
const MICROPHONE_BUFFER_SECONDS: usize = 2;

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
    microphone_enabled: bool,
    microphone_device: String,
    microphone_gain_percent: u16,
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
    microphone_enabled: bool,
    microphone_device: String,
    microphone_gain_percent: u16,
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
    microphone_input_available: bool,
    ready: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastMicrophoneDevice {
    id: String,
    label: String,
    is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastMicrophoneStatus {
    configured: bool,
    ready: bool,
    live: bool,
    device: Option<String>,
    gain_percent: u16,
    message: String,
}

impl Default for BroadcastMicrophoneStatus {
    fn default() -> Self {
        Self {
            configured: false,
            ready: false,
            live: false,
            device: None,
            gain_percent: 100,
            message: "Micrófono desactivado.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastStatus {
    status: String,
    message: String,
    now_playing: Option<BroadcastQueueEntry>,
    started_at: Option<String>,
    microphone: BroadcastMicrophoneStatus,
    updated_at: String,
}

impl Default for BroadcastStatus {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            message: "Radio detenida.".to_string(),
            now_playing: None,
            started_at: None,
            microphone: BroadcastMicrophoneStatus::default(),
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
        let microphone = self
            .snapshot
            .lock()
            .map(|current| current.microphone.clone())
            .unwrap_or_default();
        let snapshot = BroadcastStatus {
            status: status.to_string(),
            message: message.clone(),
            now_playing,
            started_at,
            microphone,
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

    fn update_microphone(
        &self,
        app: &AppHandle,
        microphone: BroadcastMicrophoneStatus,
        level: &str,
        event: &str,
    ) {
        let snapshot = if let Ok(mut current) = self.snapshot.lock() {
            current.microphone = microphone.clone();
            current.updated_at = timestamp();
            current.clone()
        } else {
            BroadcastStatus {
                microphone: microphone.clone(),
                ..BroadcastStatus::default()
            }
        };
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: level.to_string(),
                event: event.to_string(),
                message: microphone.message,
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
    SetMicrophoneLive(bool),
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
        if profile.microphone_enabled && !preflight.microphone_input_available {
            return Err(
                "FFmpeg no incluye la entrada AVFoundation requerida para el micrófono."
                    .to_string(),
            );
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

    fn set_microphone_live(&self, live: bool) -> Result<BroadcastStatus, String> {
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
            .send(WorkerCommand::SetMicrophoneLive(live))
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
           bitrate_kbps, tls, public, microphone_enabled, microphone_device,
           microphone_gain_percent, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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
           microphone_enabled = excluded.microphone_enabled,
           microphone_device = excluded.microphone_device,
           microphone_gain_percent = excluded.microphone_gain_percent,
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
            input.microphone_enabled,
            input.microphone_device,
            input.microphone_gain_percent,
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
pub fn broadcast_microphone_devices(
    app: AppHandle,
) -> Result<Vec<BroadcastMicrophoneDevice>, String> {
    microphone_devices(&app)
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

#[tauri::command]
pub fn broadcast_set_microphone_live(
    manager: State<'_, BroadcastManager>,
    live: bool,
) -> Result<BroadcastStatus, String> {
    manager.set_microphone_live(live)
}

fn validate_profile(mut input: BroadcastProfileInput) -> Result<BroadcastProfileInput, String> {
    input.host = input.host.trim().to_string();
    input.mount = input.mount.trim().to_string();
    input.username = input.username.trim().to_string();
    input.station_name = input.station_name.trim().to_string();
    input.description = input.description.trim().to_string();
    input.microphone_device = input.microphone_device.trim().to_string();
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
    if input.microphone_device != "default"
        && !input
            .microphone_device
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err("Dispositivo de micrófono invalido.".to_string());
    }
    if input.microphone_gain_percent > 200 {
        return Err("La ganancia del micrófono debe estar entre 0% y 200%.".to_string());
    }
    Ok(input)
}

fn load_profile(app: &AppHandle) -> Result<BroadcastProfile, String> {
    let conn = open_db(app)?;
    let stored = conn
        .query_row(
            "SELECT id, host, port, mount, username, station_name, description,
                    bitrate_kbps, tls, public, microphone_enabled,
                    microphone_device, microphone_gain_percent, updated_at
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
                    row.get::<_, bool>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, u16>(12)?,
                    row.get::<_, String>(13)?,
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
        microphone_enabled,
        microphone_device,
        microphone_gain_percent,
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
            false,
            "default".to_string(),
            100,
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
        microphone_enabled,
        microphone_device,
        microphone_gain_percent,
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
    let devices = system::ffmpeg_command(app)
        .args(["-hide_banner", "-devices"])
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
    let device_text = devices
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let mp3_encoder_available = encoder_text.lines().any(|line| line.contains("libmp3lame"));
    let icecast_protocol_available = protocol_list_contains(&protocol_text, "icecast");
    let tls_protocol_available = protocol_list_contains(&protocol_text, "tls");
    let microphone_input_available = device_text
        .lines()
        .any(|line| line.contains("avfoundation") && line.trim_start().starts_with('D'));
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
        microphone_input_available,
        ready,
        message,
    }
}

fn protocol_list_contains(output: &str, protocol: &str) -> bool {
    output.lines().any(|line| line.trim() == protocol)
}

fn microphone_devices(app: &AppHandle) -> Result<Vec<BroadcastMicrophoneDevice>, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        return Err("La selección de micrófono está disponible en macOS.".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let output = system::ffmpeg_command(app)
            .args([
                "-hide_banner",
                "-f",
                "avfoundation",
                "-list_devices",
                "true",
                "-i",
                "",
            ])
            .output()
            .map_err(|error| format!("No se pudieron consultar micrófonos: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut devices = vec![BroadcastMicrophoneDevice {
            id: "default".to_string(),
            label: "Micrófono predeterminado de macOS".to_string(),
            is_default: true,
        }];
        let line_pattern = Regex::new(r"\]\s+\[(\d+)\]\s+(.+)$")
            .map_err(|error| format!("No se pudo preparar parser de micrófonos: {error}"))?;
        let mut reading_audio = false;
        for line in stderr.lines() {
            if line.contains("AVFoundation audio devices:") {
                reading_audio = true;
                continue;
            }
            if !reading_audio {
                continue;
            }
            if line.contains(" devices:") || line.contains("Error opening input") {
                break;
            }
            let Some(captures) = line_pattern.captures(line) else {
                continue;
            };
            let Some(id) = captures.get(1).map(|value| value.as_str().to_string()) else {
                continue;
            };
            let Some(label) = captures
                .get(2)
                .map(|value| value.as_str().trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            devices.push(BroadcastMicrophoneDevice {
                id,
                label,
                is_default: false,
            });
        }
        Ok(devices)
    }
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

fn microphone_capture_args(device: &str) -> Vec<String> {
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-nostdin".to_string(),
        "-thread_queue_size".to_string(),
        "512".to_string(),
        "-fflags".to_string(),
        "+nobuffer".to_string(),
        "-f".to_string(),
        "avfoundation".to_string(),
        "-i".to_string(),
        format!(":{device}"),
        "-map".to_string(),
        "0:a:0".to_string(),
        "-vn".to_string(),
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

struct MicrophoneCapture {
    child: Child,
    buffer: Arc<Mutex<VecDeque<u8>>>,
}

impl MicrophoneCapture {
    fn mix_into(&mut self, output: &mut [u8], gain_percent: u16) -> Result<(), String> {
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| format!("No se pudo revisar el micrófono: {error}"))?
        {
            return Err(format!(
                "La captura de micrófono terminó con estado {status}."
            ));
        }
        let mut microphone = self
            .buffer
            .lock()
            .map_err(|_| "No se pudo leer el buffer del micrófono.".to_string())?;
        if microphone.len() > output.len() {
            let excess = microphone.len() - output.len();
            let aligned = excess / 4 * 4;
            microphone.drain(..aligned);
        }
        let sample_bytes = output.len().min(microphone.len()) & !1;
        for sample in output[..sample_bytes].chunks_exact_mut(2) {
            let low = microphone.pop_front().unwrap_or_default();
            let high = microphone.pop_front().unwrap_or_default();
            let music = i16::from_le_bytes([sample[0], sample[1]]) as i32;
            let mic = i16::from_le_bytes([low, high]) as i32;
            let mixed = mix_pcm_sample(music as i16, mic as i16, gain_percent);
            sample.copy_from_slice(&mixed.to_le_bytes());
        }
        Ok(())
    }

    fn clear(&self) {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.clear();
        }
    }

    fn terminate(mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn mix_pcm_sample(music: i16, microphone: i16, gain_percent: u16) -> i16 {
    (music as i32)
        .saturating_add((microphone as i32).saturating_mul(gain_percent as i32) / 100)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn spawn_microphone_capture(
    app: &AppHandle,
    device: &str,
    runtime: &Arc<RuntimeState>,
) -> Result<MicrophoneCapture, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, device, runtime);
        return Err("La captura de micrófono está disponible en macOS.".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let mut child = system::ffmpeg_command(app)
            .args(microphone_capture_args(device))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("No se pudo iniciar el micrófono: {error}"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| "No se pudo leer audio del micrófono.".to_string())?;
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let reader_buffer = Arc::clone(&buffer);
        thread::spawn(move || {
            let mut chunk = [0u8; 16 * 1024];
            let maximum_bytes =
                PCM_SAMPLE_RATE * PCM_CHANNELS * PCM_BYTES_PER_SAMPLE * MICROPHONE_BUFFER_SECONDS;
            while let Ok(read) = stdout.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let Ok(mut target) = reader_buffer.lock() else {
                    break;
                };
                target.extend(&chunk[..read]);
                if target.len() > maximum_bytes {
                    let excess = target.len() - maximum_bytes;
                    let aligned = excess.div_ceil(4) * 4;
                    let drop_bytes = aligned.min(target.len());
                    target.drain(..drop_bytes);
                }
            }
        });
        if let Some(stderr) = child.stderr.take() {
            let app = app.clone();
            let runtime = Arc::clone(runtime);
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        runtime.log(
                            &app,
                            "warning",
                            "ffmpeg_microphone",
                            format!("Micrófono: {line}"),
                        );
                    }
                }
            });
        }
        Ok(MicrophoneCapture { child, buffer })
    }
}

struct WorkerAudio {
    configured: bool,
    device: Option<String>,
    gain_percent: u16,
    microphone: Option<MicrophoneCapture>,
    microphone_live: bool,
}

impl WorkerAudio {
    fn from_profile(profile: &BroadcastProfile) -> Self {
        Self {
            configured: profile.microphone_enabled,
            device: profile
                .microphone_enabled
                .then(|| profile.microphone_device.clone()),
            gain_percent: profile.microphone_gain_percent,
            microphone: None,
            microphone_live: false,
        }
    }

    fn status(&self, message: impl Into<String>) -> BroadcastMicrophoneStatus {
        BroadcastMicrophoneStatus {
            configured: self.configured,
            ready: self.microphone.is_some(),
            live: self.microphone_live,
            device: self.device.clone(),
            gain_percent: self.gain_percent,
            message: message.into(),
        }
    }

    fn set_live(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        live: bool,
    ) -> Result<(), String> {
        if live && self.microphone.is_none() {
            return Err(
                "El micrófono no está preparado. Detén la radio y revisa su configuración."
                    .to_string(),
            );
        }
        self.microphone_live = live;
        if !live {
            if let Some(microphone) = self.microphone.as_ref() {
                microphone.clear();
            }
        }
        let message = if live {
            "Micrófono al aire."
        } else {
            "Micrófono silenciado."
        };
        runtime.update_microphone(app, self.status(message), "info", "microphone_live");
        Ok(())
    }

    fn process_chunk(&mut self, app: &AppHandle, runtime: &Arc<RuntimeState>, output: &mut [u8]) {
        let Some(microphone) = self.microphone.as_mut() else {
            return;
        };
        if !self.microphone_live {
            microphone.clear();
            return;
        }
        if let Err(error) = microphone.mix_into(output, self.gain_percent) {
            self.microphone_live = false;
            runtime.log(app, "error", "microphone", error.clone());
            if let Some(microphone) = self.microphone.take() {
                microphone.terminate();
            }
            runtime.update_microphone(
                app,
                self.status(format!("Micrófono no disponible: {error}")),
                "error",
                "microphone_failed",
            );
        }
    }

    fn terminate(&mut self) {
        self.microphone_live = false;
        if let Some(microphone) = self.microphone.take() {
            microphone.terminate();
        }
    }
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
    worker_audio: &mut WorkerAudio,
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
        match poll_worker_commands(commands, app, runtime, worker_audio) {
            WorkerAction::Stop => {
                let _ = decoder.kill();
                let _ = decoder.wait();
                let _ = update_entry_status(app, &entry.id, "queued", None);
                return PlayOutcome::Stop;
            }
            WorkerAction::Skip => {
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
            WorkerAction::None => {}
        }

        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let output = &mut buffer[..read];
                worker_audio.process_chunk(app, runtime, output);
                if let Err(error) = publisher.write(output) {
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
    let mut worker_audio = WorkerAudio::from_profile(&profile);
    if worker_audio.configured {
        let device = profile.microphone_device.clone();
        match spawn_microphone_capture(&app, &device, &runtime) {
            Ok(microphone) => {
                worker_audio.microphone = Some(microphone);
                runtime.update_microphone(
                    &app,
                    worker_audio.status("Micrófono preparado y silenciado."),
                    "info",
                    "microphone_ready",
                );
            }
            Err(error) => {
                runtime.update_microphone(
                    &app,
                    worker_audio.status(format!("No se pudo preparar el micrófono: {error}")),
                    "error",
                    "microphone_failed",
                );
            }
        }
    } else {
        runtime.update_microphone(
            &app,
            worker_audio.status("Micrófono desactivado."),
            "info",
            "microphone_disabled",
        );
    }

    loop {
        if matches!(
            poll_worker_commands(&commands, &app, &runtime, &mut worker_audio),
            WorkerAction::Stop
        ) {
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
                        &mut worker_audio,
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
                    &mut worker_audio,
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
                            &mut worker_audio,
                        ) {
                            break;
                        }
                    }
                    PlayOutcome::Completed | PlayOutcome::Skipped => {}
                }
            }
            Ok(None) => {
                let mut silence = silence_chunk();
                worker_audio.process_chunk(&app, &runtime, &mut silence);
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
                        &mut worker_audio,
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
    worker_audio.terminate();
    runtime.update_microphone(
        &app,
        worker_audio.status(if worker_audio.configured {
            "Micrófono detenido."
        } else {
            "Micrófono desactivado."
        }),
        "info",
        "microphone_stopped",
    );
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
    worker_audio: &mut WorkerAudio,
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
        if matches!(
            poll_worker_commands(commands, app, runtime, worker_audio),
            WorkerAction::Stop
        ) {
            return false;
        }
        thread::sleep(Duration::from_millis(250));
    }
    true
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum WorkerAction {
    None,
    Stop,
    Skip,
}

fn poll_worker_commands(
    commands: &Receiver<WorkerCommand>,
    app: &AppHandle,
    runtime: &Arc<RuntimeState>,
    worker_audio: &mut WorkerAudio,
) -> WorkerAction {
    let mut action = WorkerAction::None;
    loop {
        match commands.try_recv() {
            Ok(WorkerCommand::Stop) | Err(TryRecvError::Disconnected) => {
                return WorkerAction::Stop;
            }
            Ok(WorkerCommand::Skip) => action = WorkerAction::Skip,
            Ok(WorkerCommand::SetMicrophoneLive(live)) => {
                if let Err(error) = worker_audio.set_live(app, runtime, live) {
                    runtime.log(app, "error", "microphone", error);
                }
            }
            Err(TryRecvError::Empty) => return action,
        }
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
          microphone_enabled INTEGER NOT NULL DEFAULT 0,
          microphone_device TEXT NOT NULL DEFAULT 'default',
          microphone_gain_percent INTEGER NOT NULL DEFAULT 100,
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
    .map_err(|error| format!("No se pudo inicializar SQLite broadcast: {error}"))?;
    ensure_broadcast_profile_column(conn, "microphone_enabled", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_broadcast_profile_column(conn, "microphone_device", "TEXT NOT NULL DEFAULT 'default'")?;
    ensure_broadcast_profile_column(
        conn,
        "microphone_gain_percent",
        "INTEGER NOT NULL DEFAULT 100",
    )?;
    Ok(())
}

fn ensure_broadcast_profile_column(
    conn: &Connection,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(broadcast_profiles)")
        .map_err(|error| format!("No se pudo revisar perfil de broadcast: {error}"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("No se pudieron leer columnas de broadcast: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("No se pudieron mapear columnas de broadcast: {error}"))?;
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE broadcast_profiles ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| format!("No se pudo agregar columna {column} a broadcast: {error}"))?;
    Ok(())
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
            microphone_enabled: true,
            microphone_device: "default".to_string(),
            microphone_gain_percent: 100,
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
            microphone_enabled: true,
            microphone_device: "default".to_string(),
            microphone_gain_percent: 100,
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
    fn microphone_capture_uses_avfoundation_and_normalized_pcm() {
        let args = microphone_capture_args("2");
        assert!(args.windows(2).any(|pair| pair == ["-f", "avfoundation"]));
        assert!(args.windows(2).any(|pair| pair == ["-i", ":2"]));
        assert!(args.windows(2).any(|pair| pair == ["-ar", "44100"]));
        assert!(args.windows(2).any(|pair| pair == ["-ac", "2"]));
    }

    #[test]
    fn microphone_mix_applies_gain_and_clamps() {
        assert_eq!(mix_pcm_sample(1_000, 2_000, 50), 2_000);
        assert_eq!(mix_pcm_sample(30_000, 30_000, 100), i16::MAX);
        assert_eq!(mix_pcm_sample(-30_000, -30_000, 100), i16::MIN);
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
