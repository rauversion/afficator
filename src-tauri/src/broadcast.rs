use crate::{application_audio, settings, system};
use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, SampleFormat};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

const DB_FILE: &str = "aifficator.sqlite3";
const PROFILE_ID: &str = "default";
const MANUAL_QUEUE_PATH: &str = "__manual__";
const MANUAL_QUEUE_NAME: &str = "Agregadas manualmente";
const OUTPUT_KIND_ICECAST: &str = "icecast";
const OUTPUT_KIND_RTMP: &str = "rtmp";
const RTMP_PLATFORM_INSTAGRAM: &str = "instagram";
const RTMP_PLATFORM_CUSTOM: &str = "custom";
const RTMP_VIDEO_WIDTH: usize = 720;
const RTMP_VIDEO_HEIGHT: usize = 1280;
const RTMP_VIDEO_FPS: usize = 30;
const PCM_SAMPLE_RATE: usize = 44_100;
const PCM_CHANNELS: usize = 2;
const PCM_BYTES_PER_SAMPLE: usize = 2;
const SILENCE_CHUNK_MILLIS: usize = 250;
const MICROPHONE_BUFFER_SECONDS: usize = 2;
const MICROPHONE_PREBUFFER_MILLIS: usize = 250;
const MICROPHONE_MAX_LATENCY_MILLIS: usize = 750;
const MICROPHONE_DUCKING_PERCENT: f32 = 35.0;
const MICROPHONE_DUCKING_THRESHOLD: f32 = 0.01;
const MICROPHONE_ENVELOPE_ATTACK: f32 = 0.01;
const MICROPHONE_ENVELOPE_RELEASE: f32 = 0.0002;
const MICROPHONE_DUCKING_ATTACK: f32 = 0.002;
const MICROPHONE_DUCKING_RELEASE: f32 = 0.00008;
const LINE_INPUT_CHUNK_MILLIS: usize = 50;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastProfileInput {
    output_kind: String,
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
    line_input_enabled: bool,
    line_input_device: String,
    line_input_channel: u16,
    line_input_stereo: bool,
    line_input_gain_percent: u16,
    application_audio_enabled: bool,
    application_audio_bundle_id: String,
    application_audio_gain_percent: u16,
    rtmp_platform: String,
    rtmp_server_url: String,
    rtmp_video_bitrate_kbps: u16,
    rtmp_audio_bitrate_kbps: u16,
    password: Option<String>,
    clear_password: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastProfile {
    id: String,
    output_kind: String,
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
    line_input_enabled: bool,
    line_input_device: String,
    line_input_channel: u16,
    line_input_stereo: bool,
    line_input_gain_percent: u16,
    application_audio_enabled: bool,
    application_audio_bundle_id: String,
    application_audio_gain_percent: u16,
    rtmp_platform: String,
    rtmp_server_url: String,
    rtmp_video_bitrate_kbps: u16,
    rtmp_audio_bitrate_kbps: u16,
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
    h264_encoder_available: bool,
    aac_encoder_available: bool,
    rtmp_protocol_available: bool,
    rtmps_protocol_available: bool,
    flv_muxer_available: bool,
    visualizer_filter_available: bool,
    microphone_input_available: bool,
    ready: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastMicrophoneDevice {
    id: String,
    label: String,
    is_default: bool,
    input_channels: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastApplicationAudioDevice {
    id: String,
    label: String,
    process_id: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastMicrophoneStatus {
    configured: bool,
    ready: bool,
    live: bool,
    receiving_audio: bool,
    level_percent: u8,
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
            receiving_audio: false,
            level_percent: 0,
            device: None,
            gain_percent: 100,
            message: "Micrófono desactivado.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastLineInputStatus {
    configured: bool,
    ready: bool,
    live: bool,
    receiving_audio: bool,
    level_percent: u8,
    device: Option<String>,
    channel: u16,
    stereo: bool,
    gain_percent: u16,
    message: String,
}

impl Default for BroadcastLineInputStatus {
    fn default() -> Self {
        Self {
            configured: false,
            ready: false,
            live: false,
            receiving_audio: false,
            level_percent: 0,
            device: None,
            channel: 1,
            stereo: true,
            gain_percent: 100,
            message: "Línea directa desactivada.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastApplicationAudioStatus {
    configured: bool,
    ready: bool,
    live: bool,
    receiving_audio: bool,
    level_percent: u8,
    application: Option<String>,
    label: Option<String>,
    gain_percent: u16,
    message: String,
}

impl Default for BroadcastApplicationAudioStatus {
    fn default() -> Self {
        Self {
            configured: false,
            ready: false,
            live: false,
            receiving_audio: false,
            level_percent: 0,
            application: None,
            label: None,
            gain_percent: 100,
            message: "Audio del Mac desactivado.".to_string(),
        }
    }
}

fn application_audio_title(target_id: Option<&str>, label: Option<&str>) -> String {
    if target_id == Some(application_audio::SYSTEM_AUDIO_TARGET_ID) {
        return label
            .unwrap_or(application_audio::SYSTEM_AUDIO_LABEL)
            .to_string();
    }
    label
        .map(|label| format!("Audio de {label}"))
        .unwrap_or_else(|| "Audio del Mac".to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct BroadcastStatus {
    status: String,
    message: String,
    now_playing: Option<BroadcastQueueEntry>,
    started_at: Option<String>,
    source_mode: String,
    microphone: BroadcastMicrophoneStatus,
    line_input: BroadcastLineInputStatus,
    application_audio: BroadcastApplicationAudioStatus,
    updated_at: String,
}

impl Default for BroadcastStatus {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            message: "Radio detenida.".to_string(),
            now_playing: None,
            started_at: None,
            source_mode: "playlist".to_string(),
            microphone: BroadcastMicrophoneStatus::default(),
            line_input: BroadcastLineInputStatus::default(),
            application_audio: BroadcastApplicationAudioStatus::default(),
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
        let (source_mode, microphone, line_input, application_audio) = self
            .snapshot
            .lock()
            .map(|current| {
                (
                    current.source_mode.clone(),
                    current.microphone.clone(),
                    current.line_input.clone(),
                    current.application_audio.clone(),
                )
            })
            .unwrap_or_else(|_| {
                (
                    "playlist".to_string(),
                    BroadcastMicrophoneStatus::default(),
                    BroadcastLineInputStatus::default(),
                    BroadcastApplicationAudioStatus::default(),
                )
            });
        let snapshot = BroadcastStatus {
            status: status.to_string(),
            message: message.clone(),
            now_playing,
            started_at,
            source_mode,
            microphone,
            line_input,
            application_audio,
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

    fn update_line_input(
        &self,
        app: &AppHandle,
        line_input: BroadcastLineInputStatus,
        level: &str,
        event: &str,
    ) {
        let snapshot = if let Ok(mut current) = self.snapshot.lock() {
            current.source_mode = if line_input.live {
                "line_input".to_string()
            } else if current.application_audio.live {
                "application_audio".to_string()
            } else {
                "playlist".to_string()
            };
            if line_input.live {
                current.now_playing = None;
                current.message = "Línea directa al aire.".to_string();
            } else if current.status == "live" && !current.application_audio.live {
                current.message = "Radio en vivo · fuente Playlist.".to_string();
            }
            current.line_input = line_input.clone();
            current.updated_at = timestamp();
            current.clone()
        } else {
            BroadcastStatus {
                source_mode: if line_input.live {
                    "line_input".to_string()
                } else {
                    "playlist".to_string()
                },
                line_input: line_input.clone(),
                ..BroadcastStatus::default()
            }
        };
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: level.to_string(),
                event: event.to_string(),
                message: line_input.message,
                status: snapshot,
                timestamp: timestamp(),
            },
        );
    }

    fn update_application_audio(
        &self,
        app: &AppHandle,
        application_audio: BroadcastApplicationAudioStatus,
        level: &str,
        event: &str,
    ) {
        let snapshot = if let Ok(mut current) = self.snapshot.lock() {
            current.source_mode = if application_audio.live {
                "application_audio".to_string()
            } else if current.line_input.live {
                "line_input".to_string()
            } else {
                "playlist".to_string()
            };
            if application_audio.live {
                current.now_playing = None;
                current.message = format!(
                    "{} al aire.",
                    application_audio_title(
                        application_audio.application.as_deref(),
                        application_audio.label.as_deref()
                    )
                );
            } else if current.status == "live" && !current.line_input.live {
                current.message = "Radio en vivo · fuente Playlist.".to_string();
            }
            current.application_audio = application_audio.clone();
            current.updated_at = timestamp();
            current.clone()
        } else {
            BroadcastStatus {
                source_mode: if application_audio.live {
                    "application_audio".to_string()
                } else {
                    "playlist".to_string()
                },
                application_audio: application_audio.clone(),
                ..BroadcastStatus::default()
            }
        };
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: level.to_string(),
                event: event.to_string(),
                message: application_audio.message,
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

    fn mark_output_ready(&self, app: &AppHandle, message: impl Into<String>) {
        let message = message.into();
        let snapshot = if let Ok(mut current) = self.snapshot.lock() {
            current.status = "live".to_string();
            current.message = current
                .now_playing
                .as_ref()
                .map(|entry| format!("En vivo: {}", display_title(entry)))
                .unwrap_or_else(|| message.clone());
            current.updated_at = timestamp();
            current.clone()
        } else {
            BroadcastStatus {
                status: "live".to_string(),
                message: message.clone(),
                ..BroadcastStatus::default()
            }
        };
        let _ = app.emit(
            "broadcast-progress",
            BroadcastProgressEvent {
                level: "info".to_string(),
                event: "connected".to_string(),
                message,
                status: snapshot,
                timestamp: timestamp(),
            },
        );
    }
}

enum WorkerCommand {
    Stop,
    Skip,
    SetMicrophoneLive(bool),
    SetLineInputLive(bool),
    SetApplicationAudioLive(bool),
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

    fn start(&self, app: AppHandle, stream_key: Option<String>) -> Result<BroadcastStatus, String> {
        self.cleanup_finished_worker();
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| "No se pudo bloquear el motor de broadcast.".to_string())?;
        if worker.is_some() {
            return Err("El broadcast ya esta iniciado o deteniendose.".to_string());
        }

        let profile = load_profile(&app)?;
        let credential = if profile.output_kind == OUTPUT_KIND_RTMP {
            validate_stream_key(stream_key)?
        } else {
            settings::load_icecast_source_password(&app)?
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Configura la contraseña source de Icecast.".to_string())?
        };
        let preflight = ffmpeg_preflight(&app, &profile);
        if !preflight.ready {
            return Err(preflight.message);
        }
        if (profile.microphone_enabled || profile.line_input_enabled)
            && !preflight.microphone_input_available
        {
            return Err("No hay un dispositivo de entrada de audio disponible.".to_string());
        }

        let mut conn = open_db(&app)?;
        reset_interrupted_entries(&mut conn)?;
        let (sender, receiver) = mpsc::channel();
        let runtime = Arc::clone(&self.runtime);
        let started_at = timestamp();
        runtime.update(
            &app,
            "connecting",
            connecting_message(&profile),
            None,
            Some(started_at.clone()),
            ("info", "connecting"),
        );
        let join = thread::spawn(move || {
            run_worker(app, profile, credential, runtime, receiver, started_at)
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

    fn set_line_input_live(&self, live: bool) -> Result<BroadcastStatus, String> {
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
            .send(WorkerCommand::SetLineInputLive(live))
            .map_err(|_| "El motor de broadcast ya se detuvo.".to_string())?;
        Ok(self.runtime.snapshot())
    }

    fn set_application_audio_live(&self, live: bool) -> Result<BroadcastStatus, String> {
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
            .send(WorkerCommand::SetApplicationAudioLive(live))
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
           id, output_kind, host, port, mount, username, station_name, description,
           bitrate_kbps, tls, public, microphone_enabled, microphone_device,
           microphone_gain_percent, line_input_enabled, line_input_device,
           line_input_channel, line_input_stereo, line_input_gain_percent,
           application_audio_enabled, application_audio_bundle_id,
           application_audio_gain_percent, rtmp_platform, rtmp_server_url,
           rtmp_video_bitrate_kbps, rtmp_audio_bitrate_kbps, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
         ON CONFLICT(id) DO UPDATE SET
           output_kind = excluded.output_kind,
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
           line_input_enabled = excluded.line_input_enabled,
           line_input_device = excluded.line_input_device,
           line_input_channel = excluded.line_input_channel,
           line_input_stereo = excluded.line_input_stereo,
           line_input_gain_percent = excluded.line_input_gain_percent,
           application_audio_enabled = excluded.application_audio_enabled,
           application_audio_bundle_id = excluded.application_audio_bundle_id,
           application_audio_gain_percent = excluded.application_audio_gain_percent,
           rtmp_platform = excluded.rtmp_platform,
           rtmp_server_url = excluded.rtmp_server_url,
           rtmp_video_bitrate_kbps = excluded.rtmp_video_bitrate_kbps,
           rtmp_audio_bitrate_kbps = excluded.rtmp_audio_bitrate_kbps,
           updated_at = excluded.updated_at",
        params![
            PROFILE_ID,
            input.output_kind,
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
            input.line_input_enabled,
            input.line_input_device,
            input.line_input_channel,
            input.line_input_stereo,
            input.line_input_gain_percent,
            input.application_audio_enabled,
            input.application_audio_bundle_id,
            input.application_audio_gain_percent,
            input.rtmp_platform,
            input.rtmp_server_url,
            input.rtmp_video_bitrate_kbps,
            input.rtmp_audio_bitrate_kbps,
            now,
        ],
    )
    .map_err(|error| format!("No se pudo guardar perfil de broadcast: {error}"))?;

    if input.clear_password {
        settings::save_icecast_source_password(&app, None)?;
    } else if let Some(password) = input.password {
        settings::save_icecast_source_password(&app, Some(password))?;
    }

    load_profile(&app)
}

#[tauri::command]
pub fn broadcast_preflight(app: AppHandle) -> BroadcastPreflight {
    let profile = load_profile(&app).unwrap_or_else(|_| default_profile(&app));
    ffmpeg_preflight(&app, &profile)
}

#[tauri::command]
pub fn broadcast_microphone_devices(
    app: AppHandle,
) -> Result<Vec<BroadcastMicrophoneDevice>, String> {
    microphone_devices(&app)
}

#[tauri::command]
pub fn broadcast_application_audio_devices() -> Result<Vec<BroadcastApplicationAudioDevice>, String>
{
    application_audio::list_applications().map(|applications| {
        applications
            .into_iter()
            .map(|application| BroadcastApplicationAudioDevice {
                id: application.id,
                label: application.label,
                process_id: application.process_id,
            })
            .collect()
    })
}

#[tauri::command]
pub fn broadcast_open_application_audio_settings() -> Result<(), String> {
    application_audio::open_permission_settings()
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
pub fn broadcast_append_draft(
    app: AppHandle,
    draft_id: String,
) -> Result<BroadcastQueueAppendResult, String> {
    let mut conn = open_db(&app)?;
    append_draft(&mut conn, &draft_id)
}

#[tauri::command]
pub fn broadcast_append_track(
    app: AppHandle,
    library_id: String,
    track_id: String,
) -> Result<BroadcastQueueEntry, String> {
    let mut conn = open_db(&app)?;
    append_track(&mut conn, &library_id, &track_id)
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
    stream_key: Option<String>,
) -> Result<BroadcastStatus, String> {
    manager.start(app, stream_key)
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

#[tauri::command]
pub fn broadcast_set_line_input_live(
    manager: State<'_, BroadcastManager>,
    live: bool,
) -> Result<BroadcastStatus, String> {
    manager.set_line_input_live(live)
}

#[tauri::command]
pub fn broadcast_set_application_audio_live(
    manager: State<'_, BroadcastManager>,
    live: bool,
) -> Result<BroadcastStatus, String> {
    manager.set_application_audio_live(live)
}

fn validate_profile(mut input: BroadcastProfileInput) -> Result<BroadcastProfileInput, String> {
    input.output_kind = input.output_kind.trim().to_lowercase();
    input.host = input.host.trim().to_string();
    input.mount = input.mount.trim().to_string();
    input.username = input.username.trim().to_string();
    input.station_name = input.station_name.trim().to_string();
    input.description = input.description.trim().to_string();
    input.microphone_device = input.microphone_device.trim().to_string();
    input.line_input_device = input.line_input_device.trim().to_string();
    input.application_audio_bundle_id = input.application_audio_bundle_id.trim().to_string();
    input.rtmp_platform = input.rtmp_platform.trim().to_lowercase();
    input.rtmp_server_url = input.rtmp_server_url.trim().to_string();
    input.password = input
        .password
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if !matches!(
        input.output_kind.as_str(),
        OUTPUT_KIND_ICECAST | OUTPUT_KIND_RTMP
    ) {
        return Err("Tipo de destino de broadcast inválido.".to_string());
    }
    if input.output_kind == OUTPUT_KIND_ICECAST {
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
        if !(64..=320).contains(&input.bitrate_kbps) {
            return Err("El bitrate MP3 debe estar entre 64 y 320 kbps.".to_string());
        }
    } else {
        validate_rtmp_profile(&input)?;
    }
    if input.station_name.is_empty() || input.station_name.len() > 120 {
        return Err("Nombre de estación invalido.".to_string());
    }
    if input.microphone_device.is_empty()
        || input.microphone_device.len() > 512
        || input.microphone_device.chars().any(char::is_control)
    {
        return Err("Dispositivo de micrófono invalido.".to_string());
    }
    if input.microphone_gain_percent > 200 {
        return Err("La ganancia del micrófono debe estar entre 0% y 200%.".to_string());
    }
    if input.line_input_device.is_empty()
        || input.line_input_device.len() > 512
        || input.line_input_device.chars().any(char::is_control)
    {
        return Err("Dispositivo de línea directa inválido.".to_string());
    }
    if !(1..=64).contains(&input.line_input_channel) {
        return Err("El canal de línea debe estar entre 1 y 64.".to_string());
    }
    if input.line_input_gain_percent > 200 {
        return Err("La ganancia de línea debe estar entre 0% y 200%.".to_string());
    }
    if input.application_audio_bundle_id.len() > 512
        || input
            .application_audio_bundle_id
            .chars()
            .any(char::is_control)
    {
        return Err("La fuente de audio del Mac seleccionada es inválida.".to_string());
    }
    if input.application_audio_enabled && input.application_audio_bundle_id.is_empty() {
        return Err(
            "Selecciona la salida completa del Mac o una aplicación específica.".to_string(),
        );
    }
    if input.application_audio_gain_percent > 200 {
        return Err("La ganancia del audio del Mac debe estar entre 0% y 200%.".to_string());
    }
    Ok(input)
}

fn validate_rtmp_profile(input: &BroadcastProfileInput) -> Result<(), String> {
    if !matches!(
        input.rtmp_platform.as_str(),
        RTMP_PLATFORM_INSTAGRAM | RTMP_PLATFORM_CUSTOM
    ) {
        return Err("Plataforma RTMP inválida.".to_string());
    }
    let secure = input.rtmp_server_url.starts_with("rtmps://");
    let plain = input.rtmp_server_url.starts_with("rtmp://");
    if input.rtmp_server_url.len() > 2048
        || input.rtmp_server_url.chars().any(char::is_whitespace)
        || input.rtmp_server_url.chars().any(char::is_control)
        || (!secure && !plain)
    {
        return Err("La URL debe comenzar con rtmp:// o rtmps://.".to_string());
    }
    let without_scheme = input
        .rtmp_server_url
        .split_once("://")
        .map(|(_, value)| value)
        .unwrap_or_default();
    if without_scheme.is_empty() || without_scheme.starts_with('/') || without_scheme.contains('@')
    {
        return Err("URL de servidor RTMP inválida.".to_string());
    }
    if input.rtmp_platform == RTMP_PLATFORM_INSTAGRAM && !secure {
        return Err("Instagram requiere una URL RTMPS segura.".to_string());
    }
    let video_range = if input.rtmp_platform == RTMP_PLATFORM_INSTAGRAM {
        2_250..=6_000
    } else {
        250..=20_000
    };
    if !video_range.contains(&input.rtmp_video_bitrate_kbps) {
        return Err("Bitrate de video RTMP fuera del rango permitido.".to_string());
    }
    if !(32..=256).contains(&input.rtmp_audio_bitrate_kbps) {
        return Err("El bitrate de audio RTMP debe estar entre 32 y 256 kbps.".to_string());
    }
    Ok(())
}

fn validate_stream_key(stream_key: Option<String>) -> Result<String, String> {
    let stream_key = stream_key.unwrap_or_default().trim().to_string();
    let normalized = stream_key.to_ascii_lowercase();
    if normalized.starts_with("rtmp://") || normalized.starts_with("rtmps://") {
        return Err(
            "En Clave de transmisión pega solo la clave, no la URL RTMP completa.".to_string(),
        );
    }
    if stream_key.is_empty()
        || stream_key.len() > 4096
        || stream_key.chars().any(char::is_whitespace)
        || stream_key.chars().any(char::is_control)
    {
        return Err("Pega una clave de transmisión RTMP válida para esta sesión.".to_string());
    }
    Ok(stream_key)
}

fn load_profile(app: &AppHandle) -> Result<BroadcastProfile, String> {
    let conn = open_db(app)?;
    let stored = conn
        .query_row(
            "SELECT id, output_kind, host, port, mount, username, station_name, description,
                    bitrate_kbps, tls, public, microphone_enabled,
                    microphone_device, microphone_gain_percent, line_input_enabled,
                    line_input_device, line_input_channel, line_input_stereo,
                    line_input_gain_percent, application_audio_enabled,
                    application_audio_bundle_id, application_audio_gain_percent,
                    rtmp_platform, rtmp_server_url, rtmp_video_bitrate_kbps,
                    rtmp_audio_bitrate_kbps, updated_at
             FROM broadcast_profiles WHERE id = ?1",
            params![PROFILE_ID],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u16>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, u16>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, bool>(10)?,
                    row.get::<_, bool>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, u16>(13)?,
                    row.get::<_, bool>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, u16>(16)?,
                    row.get::<_, bool>(17)?,
                    row.get::<_, u16>(18)?,
                    row.get::<_, bool>(19)?,
                    row.get::<_, String>(20)?,
                    row.get::<_, u16>(21)?,
                    row.get::<_, String>(22)?,
                    row.get::<_, String>(23)?,
                    row.get::<_, u16>(24)?,
                    row.get::<_, u16>(25)?,
                    row.get::<_, String>(26)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("No se pudo leer perfil de broadcast: {error}"))?;
    let (
        id,
        output_kind,
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
        line_input_enabled,
        line_input_device,
        line_input_channel,
        line_input_stereo,
        line_input_gain_percent,
        application_audio_enabled,
        application_audio_bundle_id,
        application_audio_gain_percent,
        rtmp_platform,
        rtmp_server_url,
        rtmp_video_bitrate_kbps,
        rtmp_audio_bitrate_kbps,
        updated_at,
    ) = stored.unwrap_or_else(|| {
        (
            PROFILE_ID.to_string(),
            OUTPUT_KIND_ICECAST.to_string(),
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
            false,
            "default".to_string(),
            1,
            true,
            100,
            false,
            String::new(),
            100,
            RTMP_PLATFORM_INSTAGRAM.to_string(),
            String::new(),
            3_500,
            128,
            timestamp(),
        )
    });
    let password_configured = settings::load_icecast_source_password(app)?.is_some();
    let scheme = if tls { "https" } else { "http" };
    let listener_url = format!("{scheme}://{host}:{port}{mount}");
    Ok(BroadcastProfile {
        id,
        output_kind,
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
        line_input_enabled,
        line_input_device,
        line_input_channel,
        line_input_stereo,
        line_input_gain_percent,
        application_audio_enabled,
        application_audio_bundle_id,
        application_audio_gain_percent,
        rtmp_platform,
        rtmp_server_url,
        rtmp_video_bitrate_kbps,
        rtmp_audio_bitrate_kbps,
        password_configured,
        listener_url,
        updated_at,
    })
}

fn default_profile(app: &AppHandle) -> BroadcastProfile {
    BroadcastProfile {
        id: PROFILE_ID.to_string(),
        output_kind: OUTPUT_KIND_ICECAST.to_string(),
        host: "127.0.0.1".to_string(),
        port: 8000,
        mount: "/live.mp3".to_string(),
        username: "source".to_string(),
        station_name: "Rau Studio Radio".to_string(),
        description: "Broadcast local desde Rau Studio".to_string(),
        bitrate_kbps: 128,
        tls: false,
        public: false,
        microphone_enabled: false,
        microphone_device: "default".to_string(),
        microphone_gain_percent: 100,
        line_input_enabled: false,
        line_input_device: "default".to_string(),
        line_input_channel: 1,
        line_input_stereo: true,
        line_input_gain_percent: 100,
        application_audio_enabled: false,
        application_audio_bundle_id: String::new(),
        application_audio_gain_percent: 100,
        rtmp_platform: RTMP_PLATFORM_INSTAGRAM.to_string(),
        rtmp_server_url: String::new(),
        rtmp_video_bitrate_kbps: 3_500,
        rtmp_audio_bitrate_kbps: 128,
        password_configured: settings::load_icecast_source_password(app)
            .ok()
            .flatten()
            .is_some(),
        listener_url: "http://127.0.0.1:8000/live.mp3".to_string(),
        updated_at: timestamp(),
    }
}

fn ffmpeg_preflight(app: &AppHandle, profile: &BroadcastProfile) -> BroadcastPreflight {
    let encoders = system::ffmpeg_command(app)
        .args(["-hide_banner", "-encoders"])
        .output();
    let protocols = system::ffmpeg_command(app)
        .args(["-hide_banner", "-protocols"])
        .output();
    let muxers = system::ffmpeg_command(app)
        .args(["-hide_banner", "-muxers"])
        .output();
    let filters = system::ffmpeg_command(app)
        .args(["-hide_banner", "-filters"])
        .output();
    let ffmpeg_available =
        encoders.is_ok() && protocols.is_ok() && muxers.is_ok() && filters.is_ok();
    let encoder_text = encoders
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let protocol_text = protocols
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let muxer_text = muxers
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let filter_text = filters
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let mp3_encoder_available = encoder_text.lines().any(|line| line.contains("libmp3lame"));
    let icecast_protocol_available = protocol_list_contains(&protocol_text, "icecast");
    let tls_protocol_available = protocol_list_contains(&protocol_text, "tls");
    let h264_encoder_available = list_contains_token(&encoder_text, "libx264");
    let aac_encoder_available = list_contains_token(&encoder_text, "aac");
    let rtmp_protocol_available = protocol_list_contains(&protocol_text, "rtmp");
    let rtmps_protocol_available = protocol_list_contains(&protocol_text, "rtmps");
    let flv_muxer_available = list_contains_token(&muxer_text, "flv");
    let visualizer_filter_available = list_contains_token(&filter_text, "testsrc2");
    let microphone_input_available = cpal::default_host().default_input_device().is_some();
    let rtmps_required = profile.rtmp_server_url.starts_with("rtmps://");
    let ready = if profile.output_kind == OUTPUT_KIND_RTMP {
        ffmpeg_available
            && h264_encoder_available
            && aac_encoder_available
            && flv_muxer_available
            && visualizer_filter_available
            && if rtmps_required {
                rtmps_protocol_available
            } else {
                rtmp_protocol_available
            }
    } else {
        ffmpeg_available
            && mp3_encoder_available
            && icecast_protocol_available
            && (!profile.tls || tls_protocol_available)
    };
    let message = if !ffmpeg_available {
        "FFmpeg no esta disponible.".to_string()
    } else if ready && profile.output_kind == OUTPUT_KIND_RTMP {
        "FFmpeg está listo para transmitir video H.264 y audio AAC por RTMP.".to_string()
    } else if ready {
        "FFmpeg esta listo para transmitir MP3 a Icecast.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && !h264_encoder_available {
        "FFmpeg no incluye el encoder libx264 requerido para RTMP.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && !aac_encoder_available {
        "FFmpeg no incluye el encoder AAC requerido para RTMP.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && !flv_muxer_available {
        "FFmpeg no incluye el muxer FLV requerido para RTMP.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && !visualizer_filter_available {
        "FFmpeg no incluye el filtro requerido para la carta de prueba RTMP.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && rtmps_required && !rtmps_protocol_available
    {
        "FFmpeg no incluye el protocolo RTMPS requerido por este destino.".to_string()
    } else if profile.output_kind == OUTPUT_KIND_RTMP && !rtmp_protocol_available {
        "FFmpeg no incluye el protocolo RTMP requerido por este destino.".to_string()
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
        h264_encoder_available,
        aac_encoder_available,
        rtmp_protocol_available,
        rtmps_protocol_available,
        flv_muxer_available,
        visualizer_filter_available,
        microphone_input_available,
        ready,
        message,
    }
}

fn protocol_list_contains(output: &str, protocol: &str) -> bool {
    output.lines().any(|line| line.trim() == protocol)
}

fn list_contains_token(output: &str, capability: &str) -> bool {
    output
        .lines()
        .any(|line| line.split_whitespace().any(|token| token == capability))
}

fn microphone_devices(_app: &AppHandle) -> Result<Vec<BroadcastMicrophoneDevice>, String> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let mut result = Vec::new();
    if let Some(default_device) = default_device.as_ref() {
        result.push(BroadcastMicrophoneDevice {
            id: "default".to_string(),
            label: "Entrada predeterminada del sistema".to_string(),
            is_default: true,
            input_channels: default_device
                .default_input_config()
                .map(|config| config.channels())
                .unwrap_or(0),
        });
    }
    let devices = host
        .input_devices()
        .map_err(|error| format!("No se pudieron consultar micrófonos: {error}"))?;
    for device in devices {
        let id = device
            .id()
            .map_err(|error| format!("No se pudo identificar un micrófono: {error}"))?
            .to_string();
        let label = device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| device.to_string());
        result.push(BroadcastMicrophoneDevice {
            id,
            label,
            is_default: false,
            input_channels: device
                .default_input_config()
                .map(|config| config.channels())
                .unwrap_or(0),
        });
    }
    Ok(result)
}

fn publisher_args(profile: &BroadcastProfile, credential: &str) -> Vec<String> {
    if profile.output_kind == OUTPUT_KIND_RTMP {
        return rtmp_publisher_args(profile, credential);
    }
    icecast_publisher_args(profile, credential)
}

fn icecast_publisher_args(profile: &BroadcastProfile, password: &str) -> Vec<String> {
    let destination = format!(
        "icecast://{}@{}:{}{}",
        profile.username, profile.host, profile.port, profile.mount
    );
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-nostdin".to_string(),
        "-f".to_string(),
        "s16le".to_string(),
        "-ar".to_string(),
        PCM_SAMPLE_RATE.to_string(),
        "-ac".to_string(),
        PCM_CHANNELS.to_string(),
        "-channel_layout".to_string(),
        "stereo".to_string(),
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

fn rtmp_publisher_args(profile: &BroadcastProfile, stream_key: &str) -> Vec<String> {
    let destination = rtmp_destination_url(&profile.rtmp_server_url, stream_key);
    let video_bitrate = format!("{}k", profile.rtmp_video_bitrate_kbps);
    let video_buffer = format!("{}k", u32::from(profile.rtmp_video_bitrate_kbps) * 2);
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "debug".to_string(),
        "-nostdin".to_string(),
        "-nostats".to_string(),
        "-progress".to_string(),
        "pipe:2".to_string(),
        "-stats_period".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "s16le".to_string(),
        "-ar".to_string(),
        PCM_SAMPLE_RATE.to_string(),
        "-ac".to_string(),
        PCM_CHANNELS.to_string(),
        "-channel_layout".to_string(),
        "stereo".to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-re".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        format!(
            "testsrc2=size={}x{}:rate={}",
            RTMP_VIDEO_WIDTH, RTMP_VIDEO_HEIGHT, RTMP_VIDEO_FPS
        ),
        "-map".to_string(),
        "1:v:0".to_string(),
        "-map".to_string(),
        "0:a:0".to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "veryfast".to_string(),
        "-tune".to_string(),
        "zerolatency".to_string(),
        "-profile:v".to_string(),
        "main".to_string(),
        "-level:v".to_string(),
        "4.1".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-r".to_string(),
        RTMP_VIDEO_FPS.to_string(),
        "-g".to_string(),
        (RTMP_VIDEO_FPS * 2).to_string(),
        "-keyint_min".to_string(),
        (RTMP_VIDEO_FPS * 2).to_string(),
        "-sc_threshold".to_string(),
        "0".to_string(),
        "-b:v".to_string(),
        video_bitrate.clone(),
        "-maxrate".to_string(),
        video_bitrate,
        "-bufsize".to_string(),
        video_buffer,
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        format!("{}k", profile.rtmp_audio_bitrate_kbps),
        "-ar".to_string(),
        PCM_SAMPLE_RATE.to_string(),
        "-ac".to_string(),
        PCM_CHANNELS.to_string(),
        "-flvflags".to_string(),
        "no_duration_filesize".to_string(),
        "-flush_packets".to_string(),
        "1".to_string(),
        "-rtmp_flush_interval".to_string(),
        "1".to_string(),
        "-tcp_nodelay".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "flv".to_string(),
        destination,
    ]
}

fn rtmp_destination_url(server_url: &str, stream_key: &str) -> String {
    format!(
        "{}/{}",
        server_url.trim_end_matches('/'),
        stream_key.trim_start_matches('/')
    )
}

fn decoder_args(path: &str) -> Vec<String> {
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-nostdin".to_string(),
        "-re".to_string(),
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

enum AudioInputOwner {
    Device { _stream: cpal::Stream },
    Application(application_audio::ApplicationAudioCapture),
}

struct AudioInputCapture {
    owner: AudioInputOwner,
    label: String,
    buffer: Arc<Mutex<VecDeque<[i16; 2]>>>,
    stream_error: Arc<Mutex<Option<String>>>,
    input_sample_rate: u32,
    resample_position: f64,
    buffering: bool,
    microphone_envelope: f32,
    music_gain_percent: f32,
}

struct AudioInputMix {
    mixed_frames: usize,
    peak_percent: u8,
    buffering: bool,
}

impl AudioInputCapture {
    fn mix_into(&mut self, output: &mut [u8], gain_percent: u16) -> Result<AudioInputMix, String> {
        if let Some(error) = self
            .stream_error
            .lock()
            .map_err(|_| format!("No se pudo revisar la {}.", self.label))?
            .take()
        {
            return Err(error);
        }
        let mut microphone = self
            .buffer
            .lock()
            .map_err(|_| format!("No se pudo leer el buffer de {}.", self.label))?;
        let output_frames = output.len() / 4;
        let input_per_output = self.input_sample_rate as f64 / PCM_SAMPLE_RATE as f64;
        let required_frames = (output_frames as f64 * input_per_output).ceil() as usize + 1;
        let prebuffer_frames =
            self.input_sample_rate as usize * MICROPHONE_PREBUFFER_MILLIS / 1_000;
        let target_frames = required_frames + prebuffer_frames;
        if self.buffering {
            if microphone.len() < target_frames {
                return Ok(AudioInputMix {
                    mixed_frames: 0,
                    peak_percent: 0,
                    buffering: true,
                });
            }
            self.buffering = false;
        } else if microphone.len() < required_frames {
            self.buffering = true;
            self.resample_position = 0.0;
            self.microphone_envelope = 0.0;
            self.music_gain_percent = 100.0;
            return Ok(AudioInputMix {
                mixed_frames: 0,
                peak_percent: 0,
                buffering: true,
            });
        }
        let maximum_latency_frames =
            self.input_sample_rate as usize * MICROPHONE_MAX_LATENCY_MILLIS / 1_000;
        if microphone.len() > maximum_latency_frames {
            let excess = microphone.len().saturating_sub(target_frames);
            microphone.drain(..excess);
            self.resample_position = 0.0;
        }
        let mut position = self.resample_position;
        let mut mixed_frames = 0usize;
        let mut peak = 0u16;
        for output_frame in output.chunks_exact_mut(4) {
            let input_index = position.floor() as usize;
            let Some([left, right]) = microphone.get(input_index).copied() else {
                break;
            };
            let music_left = i16::from_le_bytes([output_frame[0], output_frame[1]]);
            let music_right = i16::from_le_bytes([output_frame[2], output_frame[3]]);
            let microphone_level = f32::from(left.unsigned_abs().max(right.unsigned_abs()))
                / f32::from(i16::MAX as u16);
            let envelope_rate = if microphone_level > self.microphone_envelope {
                MICROPHONE_ENVELOPE_ATTACK
            } else {
                MICROPHONE_ENVELOPE_RELEASE
            };
            self.microphone_envelope +=
                (microphone_level - self.microphone_envelope) * envelope_rate;
            let target_music_gain = if self.microphone_envelope >= MICROPHONE_DUCKING_THRESHOLD {
                MICROPHONE_DUCKING_PERCENT
            } else {
                100.0
            };
            let ducking_rate = if target_music_gain < self.music_gain_percent {
                MICROPHONE_DUCKING_ATTACK
            } else {
                MICROPHONE_DUCKING_RELEASE
            };
            self.music_gain_percent += (target_music_gain - self.music_gain_percent) * ducking_rate;
            let music_gain_percent = self.music_gain_percent.round() as u16;
            let mixed_left = mix_pcm_sample(music_left, left, gain_percent, music_gain_percent);
            let mixed_right = mix_pcm_sample(music_right, right, gain_percent, music_gain_percent);
            output_frame[..2].copy_from_slice(&mixed_left.to_le_bytes());
            output_frame[2..].copy_from_slice(&mixed_right.to_le_bytes());
            peak = peak.max(left.unsigned_abs()).max(right.unsigned_abs());
            mixed_frames += 1;
            position += input_per_output;
        }
        let consumed = (position.floor() as usize).min(microphone.len());
        microphone.drain(..consumed);
        self.resample_position = position - consumed as f64;
        Ok(AudioInputMix {
            mixed_frames,
            peak_percent: ((u32::from(peak) * 100 / i16::MAX as u32).min(100)) as u8,
            buffering: false,
        })
    }

    fn write_direct(
        &mut self,
        output: &mut [u8],
        gain_percent: u16,
    ) -> Result<AudioInputMix, String> {
        output.fill(0);
        self.microphone_envelope = 0.0;
        self.music_gain_percent = 100.0;
        self.mix_into(output, gain_percent)
    }

    fn clear(&mut self) {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.clear();
        }
        self.resample_position = 0.0;
        self.buffering = true;
        self.microphone_envelope = 0.0;
        self.music_gain_percent = 100.0;
    }

    fn terminate(self) {
        if let AudioInputOwner::Application(capture) = self.owner {
            capture.stop();
        }
    }
}

fn mix_pcm_sample(music: i16, microphone: i16, gain_percent: u16, music_gain_percent: u16) -> i16 {
    (music as i32)
        .saturating_mul(i32::from(music_gain_percent))
        .saturating_div(100)
        .saturating_add((microphone as i32).saturating_mul(gain_percent as i32) / 100)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn spawn_audio_input_capture(
    _app: &AppHandle,
    device: &str,
    first_channel: u16,
    stereo: Option<bool>,
    input_label: &str,
    _runtime: &Arc<RuntimeState>,
) -> Result<AudioInputCapture, String> {
    let host = cpal::default_host();
    let selected = if device == "default" {
        host.default_input_device()
            .ok_or_else(|| format!("No hay una {input_label} predeterminada disponible."))?
    } else {
        host.input_devices()
            .map_err(|error| format!("No se pudieron consultar entradas de audio: {error}"))?
            .find(|candidate| {
                candidate
                    .id()
                    .map(|id| id.to_string() == device)
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("La {input_label} seleccionada ya no está disponible."))?
    };
    let supported = selected
        .default_input_config()
        .map_err(|error| format!("No se pudo obtener el formato de {input_label}: {error}"))?;
    let sample_format = supported.sample_format();
    if !matches!(
        sample_format,
        SampleFormat::F32
            | SampleFormat::F64
            | SampleFormat::I8
            | SampleFormat::I16
            | SampleFormat::I32
            | SampleFormat::U8
            | SampleFormat::U16
            | SampleFormat::U32
    ) {
        return Err(format!(
            "El formato {sample_format} de {input_label} no está soportado."
        ));
    }
    let config: cpal::StreamConfig = supported.into();
    let input_sample_rate = config.sample_rate;
    let input_channels = usize::from(config.channels);
    let first_channel_index = usize::from(first_channel.saturating_sub(1));
    let stereo = stereo.unwrap_or(input_channels > 1);
    if first_channel == 0
        || first_channel_index >= input_channels
        || (stereo && first_channel_index + 1 >= input_channels)
    {
        let requested = if stereo {
            format!("{first_channel}–{}", first_channel.saturating_add(1))
        } else {
            first_channel.to_string()
        };
        return Err(format!(
            "La {input_label} tiene {input_channels} canal(es); no se puede usar {requested}."
        ));
    }
    let buffer = Arc::new(Mutex::new(VecDeque::new()));
    let callback_buffer = Arc::clone(&buffer);
    let stream_error = Arc::new(Mutex::new(None));
    let callback_error = Arc::clone(&stream_error);
    let callback_input_label = input_label.to_string();
    let maximum_frames = input_sample_rate as usize * MICROPHONE_BUFFER_SECONDS;
    let stream = selected
        .build_input_stream_raw(
            config,
            sample_format,
            move |data, _| {
                push_audio_input_data(
                    data,
                    input_channels,
                    first_channel_index,
                    stereo,
                    maximum_frames,
                    &callback_buffer,
                )
            },
            move |error| {
                if let Ok(mut target) = callback_error.lock() {
                    *target = Some(format!(
                        "La captura de {callback_input_label} falló: {error}"
                    ));
                }
            },
            None,
        )
        .map_err(|error| {
            format!(
                "No se pudo abrir la {input_label}. Revisa el permiso de audio de macOS para Rau Studio: {error}"
            )
        })?;
    stream.play().map_err(|error| {
        format!(
            "No se pudo activar la {input_label}. Revisa el permiso de audio de macOS para Rau Studio: {error}"
        )
    })?;
    Ok(AudioInputCapture {
        owner: AudioInputOwner::Device { _stream: stream },
        label: input_label.to_string(),
        buffer,
        stream_error,
        input_sample_rate,
        resample_position: 0.0,
        buffering: true,
        microphone_envelope: 0.0,
        music_gain_percent: 100.0,
    })
}

fn spawn_application_audio_capture(
    bundle_id: &str,
    label: &str,
) -> Result<AudioInputCapture, String> {
    let parts = application_audio::start_capture(bundle_id)?;
    Ok(AudioInputCapture {
        owner: AudioInputOwner::Application(parts.capture),
        label: if bundle_id == application_audio::SYSTEM_AUDIO_TARGET_ID {
            "salida de audio del Mac".to_string()
        } else {
            format!("audio de {label}")
        },
        buffer: parts.buffer,
        stream_error: parts.stream_error,
        input_sample_rate: application_audio::APPLICATION_AUDIO_SAMPLE_RATE,
        resample_position: 0.0,
        buffering: true,
        microphone_envelope: 0.0,
        music_gain_percent: 100.0,
    })
}

fn push_audio_input_data(
    data: &Data,
    channels: usize,
    first_channel: usize,
    stereo: bool,
    maximum_frames: usize,
    buffer: &Arc<Mutex<VecDeque<[i16; 2]>>>,
) {
    let Ok(mut target) = buffer.lock() else {
        return;
    };
    match data.sample_format() {
        SampleFormat::F32 => append_audio_input_frames(
            data.as_slice::<f32>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16,
        ),
        SampleFormat::F64 => append_audio_input_frames(
            data.as_slice::<f64>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f64).round() as i16,
        ),
        SampleFormat::I8 => append_audio_input_frames(
            data.as_slice::<i8>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| i16::from(sample) << 8,
        ),
        SampleFormat::I16 => append_audio_input_frames(
            data.as_slice::<i16>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| sample,
        ),
        SampleFormat::I32 => append_audio_input_frames(
            data.as_slice::<i32>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| (sample >> 16) as i16,
        ),
        SampleFormat::U8 => append_audio_input_frames(
            data.as_slice::<u8>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| (i16::from(sample) - 128) << 8,
        ),
        SampleFormat::U16 => append_audio_input_frames(
            data.as_slice::<u16>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| (i32::from(sample) - 32_768) as i16,
        ),
        SampleFormat::U32 => append_audio_input_frames(
            data.as_slice::<u32>().unwrap_or_default(),
            channels,
            first_channel,
            stereo,
            &mut target,
            |sample| ((i64::from(sample) - 2_147_483_648) >> 16) as i16,
        ),
        _ => return,
    }
    if target.len() > maximum_frames {
        let excess = target.len() - maximum_frames;
        target.drain(..excess);
    }
}

fn append_audio_input_frames<T: Copy>(
    samples: &[T],
    channels: usize,
    first_channel: usize,
    stereo: bool,
    target: &mut VecDeque<[i16; 2]>,
    convert: impl Fn(T) -> i16,
) {
    if channels == 0 || first_channel >= channels {
        return;
    }
    for frame in samples.chunks_exact(channels) {
        let left = convert(frame[first_channel]);
        let right = if stereo && first_channel + 1 < channels {
            convert(frame[first_channel + 1])
        } else {
            left
        };
        target.push_back([left, right]);
    }
}

struct WorkerAudio {
    configured: bool,
    device: Option<String>,
    gain_percent: u16,
    microphone: Option<AudioInputCapture>,
    microphone_live: bool,
    microphone_receiving_audio: bool,
    microphone_level_percent: u8,
    last_meter_emit: Instant,
    line_configured: bool,
    line_device: Option<String>,
    line_channel: u16,
    line_stereo: bool,
    line_gain_percent: u16,
    line_input: Option<AudioInputCapture>,
    line_live: bool,
    line_receiving_audio: bool,
    line_level_percent: u8,
    last_line_meter_emit: Instant,
    application_configured: bool,
    application_bundle_id: Option<String>,
    application_label: Option<String>,
    application_gain_percent: u16,
    application_input: Option<AudioInputCapture>,
    application_live: bool,
    application_receiving_audio: bool,
    application_level_percent: u8,
    last_application_meter_emit: Instant,
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
            microphone_receiving_audio: false,
            microphone_level_percent: 0,
            last_meter_emit: Instant::now(),
            line_configured: profile.line_input_enabled,
            line_device: profile
                .line_input_enabled
                .then(|| profile.line_input_device.clone()),
            line_channel: profile.line_input_channel,
            line_stereo: profile.line_input_stereo,
            line_gain_percent: profile.line_input_gain_percent,
            line_input: None,
            line_live: false,
            line_receiving_audio: false,
            line_level_percent: 0,
            last_line_meter_emit: Instant::now(),
            application_configured: profile.application_audio_enabled,
            application_bundle_id: profile
                .application_audio_enabled
                .then(|| profile.application_audio_bundle_id.clone()),
            application_label: None,
            application_gain_percent: profile.application_audio_gain_percent,
            application_input: None,
            application_live: false,
            application_receiving_audio: false,
            application_level_percent: 0,
            last_application_meter_emit: Instant::now(),
        }
    }

    fn status(&self, message: impl Into<String>) -> BroadcastMicrophoneStatus {
        BroadcastMicrophoneStatus {
            configured: self.configured,
            ready: self.microphone.is_some(),
            live: self.microphone_live,
            receiving_audio: self.microphone_receiving_audio,
            level_percent: self.microphone_level_percent,
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
        if live && (self.line_live || self.application_live) {
            return Err(
                "El micrófono no puede activarse mientras una fuente directa está al aire."
                    .to_string(),
            );
        }
        if live && self.microphone.is_none() {
            return Err(
                "El micrófono no está preparado. Detén la radio y revisa su configuración."
                    .to_string(),
            );
        }
        self.microphone_live = live;
        self.microphone_receiving_audio = false;
        self.microphone_level_percent = 0;
        self.last_meter_emit = Instant::now();
        if !live {
            if let Some(microphone) = self.microphone.as_mut() {
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

    fn line_status(&self, message: impl Into<String>) -> BroadcastLineInputStatus {
        BroadcastLineInputStatus {
            configured: self.line_configured,
            ready: self.line_input.is_some(),
            live: self.line_live,
            receiving_audio: self.line_receiving_audio,
            level_percent: self.line_level_percent,
            device: self.line_device.clone(),
            channel: self.line_channel,
            stereo: self.line_stereo,
            gain_percent: self.line_gain_percent,
            message: message.into(),
        }
    }

    fn application_status(&self, message: impl Into<String>) -> BroadcastApplicationAudioStatus {
        BroadcastApplicationAudioStatus {
            configured: self.application_configured,
            ready: self.application_input.is_some(),
            live: self.application_live,
            receiving_audio: self.application_receiving_audio,
            level_percent: self.application_level_percent,
            application: self.application_bundle_id.clone(),
            label: self.application_label.clone(),
            gain_percent: self.application_gain_percent,
            message: message.into(),
        }
    }

    fn set_line_live(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        live: bool,
    ) -> Result<(), String> {
        if live && self.line_input.is_none() {
            return Err(
                "La línea directa no está preparada. Detén la radio y revisa su configuración."
                    .to_string(),
            );
        }
        if live && self.application_live {
            self.application_live = false;
            self.application_receiving_audio = false;
            self.application_level_percent = 0;
            if let Some(application_input) = self.application_input.as_mut() {
                application_input.clear();
            }
            runtime.update_application_audio(
                app,
                self.application_status("Audio del Mac detenido al activar línea directa."),
                "info",
                "application_audio_live",
            );
        }
        if live && self.microphone_live {
            self.microphone_live = false;
            self.microphone_receiving_audio = false;
            self.microphone_level_percent = 0;
            if let Some(microphone) = self.microphone.as_mut() {
                microphone.clear();
            }
            runtime.update_microphone(
                app,
                self.status("Micrófono silenciado al activar línea directa."),
                "info",
                "microphone_live",
            );
        }
        self.line_live = live;
        self.line_receiving_audio = false;
        self.line_level_percent = 0;
        self.last_line_meter_emit = Instant::now();
        if let Some(line_input) = self.line_input.as_mut() {
            line_input.clear();
        }
        let message = if live {
            "Línea directa al aire."
        } else {
            "Fuente Playlist al aire."
        };
        runtime.update_line_input(app, self.line_status(message), "info", "line_input_live");
        Ok(())
    }

    fn set_application_live(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        live: bool,
    ) -> Result<(), String> {
        if live && self.application_input.is_none() {
            return Err(
                "El audio del Mac no está preparado. Detén la radio y revisa su configuración."
                    .to_string(),
            );
        }
        if live && self.microphone_live {
            self.microphone_live = false;
            self.microphone_receiving_audio = false;
            self.microphone_level_percent = 0;
            if let Some(microphone) = self.microphone.as_mut() {
                microphone.clear();
            }
            runtime.update_microphone(
                app,
                self.status("Micrófono silenciado al activar audio del Mac."),
                "info",
                "microphone_live",
            );
        }
        if live && self.line_live {
            self.line_live = false;
            self.line_receiving_audio = false;
            self.line_level_percent = 0;
            if let Some(line_input) = self.line_input.as_mut() {
                line_input.clear();
            }
            runtime.update_line_input(
                app,
                self.line_status("Línea directa detenida al activar audio del Mac."),
                "info",
                "line_input_live",
            );
        }
        self.application_live = live;
        self.application_receiving_audio = false;
        self.application_level_percent = 0;
        self.last_application_meter_emit = Instant::now();
        if let Some(application_input) = self.application_input.as_mut() {
            application_input.clear();
        }
        let message = if live {
            format!(
                "{} al aire.",
                application_audio_title(
                    self.application_bundle_id.as_deref(),
                    self.application_label.as_deref()
                )
            )
        } else {
            "Fuente Playlist al aire.".to_string()
        };
        runtime.update_application_audio(
            app,
            self.application_status(message),
            "info",
            "application_audio_live",
        );
        Ok(())
    }

    fn direct_source_live(&self) -> bool {
        self.line_live || self.application_live
    }

    fn process_chunk(&mut self, app: &AppHandle, runtime: &Arc<RuntimeState>, output: &mut [u8]) {
        let Some(microphone) = self.microphone.as_mut() else {
            return;
        };
        if !self.microphone_live {
            microphone.clear();
            return;
        }
        match microphone.mix_into(output, self.gain_percent) {
            Ok(mixed) => {
                self.microphone_receiving_audio = mixed.mixed_frames > 0;
                self.microphone_level_percent = mixed.peak_percent;
                if self.last_meter_emit.elapsed() >= Duration::from_millis(500) {
                    let message = if mixed.buffering {
                        "Micrófono al aire · estabilizando señal.".to_string()
                    } else if self.microphone_receiving_audio {
                        format!(
                            "Micrófono al aire · señal {}%.",
                            self.microphone_level_percent
                        )
                    } else {
                        "Micrófono al aire · sin señal de entrada.".to_string()
                    };
                    runtime.update_microphone(
                        app,
                        self.status(message),
                        "info",
                        "microphone_level",
                    );
                    self.last_meter_emit = Instant::now();
                }
            }
            Err(error) => {
                self.microphone_live = false;
                self.microphone_receiving_audio = false;
                self.microphone_level_percent = 0;
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
    }

    fn process_line_chunk(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        output: &mut [u8],
    ) {
        let Some(line_input) = self.line_input.as_mut() else {
            output.fill(0);
            return;
        };
        match line_input.write_direct(output, self.line_gain_percent) {
            Ok(mixed) => {
                self.line_receiving_audio = mixed.mixed_frames > 0;
                self.line_level_percent = mixed.peak_percent;
                if self.last_line_meter_emit.elapsed() >= Duration::from_millis(500) {
                    let message = if mixed.buffering {
                        "Línea directa · estabilizando señal.".to_string()
                    } else if self.line_receiving_audio {
                        format!("Línea directa · señal {}%.", self.line_level_percent)
                    } else {
                        "Línea directa · sin señal de entrada.".to_string()
                    };
                    runtime.update_line_input(
                        app,
                        self.line_status(message),
                        "info",
                        "line_input_level",
                    );
                    self.last_line_meter_emit = Instant::now();
                }
            }
            Err(error) => {
                self.line_live = false;
                self.line_receiving_audio = false;
                self.line_level_percent = 0;
                runtime.log(app, "error", "line_input", error.clone());
                if let Some(line_input) = self.line_input.take() {
                    line_input.terminate();
                }
                runtime.update_line_input(
                    app,
                    self.line_status(format!("Línea directa no disponible: {error}")),
                    "error",
                    "line_input_failed",
                );
            }
        }
    }

    fn process_application_chunk(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        output: &mut [u8],
    ) {
        let Some(application_input) = self.application_input.as_mut() else {
            output.fill(0);
            return;
        };
        match application_input.write_direct(output, self.application_gain_percent) {
            Ok(mixed) => {
                self.application_receiving_audio = mixed.mixed_frames > 0;
                self.application_level_percent = mixed.peak_percent;
                if self.last_application_meter_emit.elapsed() >= Duration::from_millis(500) {
                    let prefix = application_audio_title(
                        self.application_bundle_id.as_deref(),
                        self.application_label.as_deref(),
                    );
                    let message = if mixed.buffering {
                        format!("{prefix} · estabilizando señal.")
                    } else if self.application_receiving_audio {
                        format!("{prefix} · señal {}%.", self.application_level_percent)
                    } else {
                        format!("{prefix} · sin señal. Reproduce audio en el Mac.")
                    };
                    runtime.update_application_audio(
                        app,
                        self.application_status(message),
                        "info",
                        "application_audio_level",
                    );
                    self.last_application_meter_emit = Instant::now();
                }
            }
            Err(error) => {
                self.application_live = false;
                self.application_receiving_audio = false;
                self.application_level_percent = 0;
                runtime.log(app, "error", "application_audio", error.clone());
                if let Some(application_input) = self.application_input.take() {
                    application_input.terminate();
                }
                runtime.update_application_audio(
                    app,
                    self.application_status(format!("Audio del Mac no disponible: {error}")),
                    "error",
                    "application_audio_failed",
                );
            }
        }
    }

    fn process_direct_chunk(
        &mut self,
        app: &AppHandle,
        runtime: &Arc<RuntimeState>,
        output: &mut [u8],
    ) {
        if self.application_live {
            self.process_application_chunk(app, runtime, output);
        } else {
            self.process_line_chunk(app, runtime, output);
        }
    }

    fn terminate(&mut self) {
        self.microphone_live = false;
        self.microphone_receiving_audio = false;
        self.microphone_level_percent = 0;
        if let Some(microphone) = self.microphone.take() {
            microphone.terminate();
        }
        self.line_live = false;
        self.line_receiving_audio = false;
        self.line_level_percent = 0;
        if let Some(line_input) = self.line_input.take() {
            line_input.terminate();
        }
        self.application_live = false;
        self.application_receiving_audio = false;
        self.application_level_percent = 0;
        if let Some(application_input) = self.application_input.take() {
            application_input.terminate();
        }
    }
}

struct Publisher {
    child: Child,
    stdin: ChildStdin,
    destination_label: String,
    opened: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl Publisher {
    fn is_opened(&self) -> bool {
        self.opened.load(Ordering::Acquire)
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| format!("No se pudo revisar publisher FFmpeg: {error}"))?
        {
            return Err(format!("Publisher FFmpeg termino con estado {status}."));
        }
        self.stdin.write_all(bytes).map_err(|error| {
            format!(
                "Se perdió la conexión con {}: {error}",
                self.destination_label
            )
        })
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
    credential: &str,
    runtime: &Arc<RuntimeState>,
) -> Result<Publisher, String> {
    let is_rtmp = profile.output_kind == OUTPUT_KIND_RTMP;
    let opened = Arc::new(AtomicBool::new(!is_rtmp));
    let ready = Arc::new(AtomicBool::new(!is_rtmp));
    let mut child = system::ffmpeg_command(app)
        .args(publisher_args(profile, credential))
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
        let credential = credential.to_string();
        let opened = Arc::clone(&opened);
        let ready = Arc::clone(&ready);
        let connected_message = connected_message(profile);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if is_rtmp && is_rtmp_output_open_line(&line) {
                    opened.store(true, Ordering::Release);
                    runtime.log(
                        &app,
                        "info",
                        "output_opened",
                        "Instagram aceptó la publicación · verificando flujo continuo...",
                    );
                } else if is_rtmp && is_rtmp_output_ready_line(&line) {
                    if !ready.swap(true, Ordering::AcqRel) {
                        runtime.mark_output_ready(&app, connected_message.clone());
                    }
                } else if is_publisher_warning_line(&line) {
                    runtime.log(
                        &app,
                        "warning",
                        "ffmpeg_publisher",
                        format!("FFmpeg: {}", redact_secret(&line, &credential)),
                    );
                } else if is_rtmp && is_rtmp_diagnostic_line(&line) {
                    runtime.log(
                        &app,
                        "info",
                        "ffmpeg_rtmp",
                        format!("RTMP: {}", redact_secret(&line, &credential)),
                    );
                }
            }
        });
    }
    Ok(Publisher {
        child,
        stdin,
        destination_label: destination_label(profile).to_string(),
        opened,
        ready,
    })
}

fn is_rtmp_output_ready_line(line: &str) -> bool {
    line.trim()
        .strip_prefix("out_time_us=")
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|value| value >= 2_000_000)
}

fn is_rtmp_output_open_line(line: &str) -> bool {
    line.contains("Output #0, flv, to ")
}

fn is_publisher_warning_line(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.contains("0 decode errors") {
        return false;
    }
    !normalized.is_empty()
        && [
            "error",
            "failed",
            "invalid",
            "broken pipe",
            "end of file",
            "resumed reading",
            "connection reset",
            "cannot",
            "refused",
            "denied",
            "unable",
            "timed out",
        ]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn is_rtmp_diagnostic_line(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.contains("[rtmps @")
        && [
            "handshaking",
            "server version",
            "window acknowledgement size",
            "max sent, unacked",
            "incoming chunk size",
            "releasing stream",
            "fcpublish stream",
            "creating stream",
            "sending publish command",
            "received acknowledgement",
            "ping request",
            "ping response",
            "goaway",
        ]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn fatal_publisher_failure_message(
    profile: &BroadcastProfile,
    publisher_opened: bool,
    publisher_ready: bool,
) -> Option<String> {
    if profile.output_kind != OUTPUT_KIND_RTMP || publisher_ready {
        return None;
    }
    if publisher_opened {
        return Some(if profile.rtmp_platform == RTMP_PLATFORM_INSTAGRAM {
            "Instagram aceptó la publicación, pero cerró antes de recibir dos segundos continuos de audio y video. Prueba otro motor FFmpeg o crea un Live nuevo."
                .to_string()
        } else {
            "El servidor RTMP aceptó la publicación, pero cerró antes de recibir un flujo multimedia continuo."
                .to_string()
        });
    }
    Some(if profile.rtmp_platform == RTMP_PLATFORM_INSTAGRAM {
        "Instagram rechazó la publicación antes de recibir la señal. Crea un Live nuevo y pega por separado la URL del servidor y la clave de esa misma sesión."
            .to_string()
    } else {
        "El servidor RTMP rechazó la publicación antes de recibir la señal. Revisa la URL y la clave de transmisión."
            .to_string()
    })
}

fn redact_secret(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        message.to_string()
    } else {
        message.replace(secret, "********")
    }
}

fn destination_label(profile: &BroadcastProfile) -> &'static str {
    if profile.output_kind == OUTPUT_KIND_RTMP {
        if profile.rtmp_platform == RTMP_PLATFORM_INSTAGRAM {
            "Instagram"
        } else {
            "RTMP"
        }
    } else {
        "Icecast"
    }
}

fn connecting_message(profile: &BroadcastProfile) -> String {
    if profile.output_kind == OUTPUT_KIND_RTMP {
        format!("Conectando la señal con {}...", destination_label(profile))
    } else {
        format!(
            "Conectando con {}:{}{}...",
            profile.host, profile.port, profile.mount
        )
    }
}

fn connected_message(profile: &BroadcastProfile) -> String {
    if profile.output_kind == OUTPUT_KIND_RTMP {
        if profile.rtmp_platform == RTMP_PLATFORM_INSTAGRAM {
            "Señal enviada a Instagram · revisa la vista previa y pulsa Go live en Live Producer."
                .to_string()
        } else {
            "Señal RTMP conectada · esperando audio.".to_string()
        }
    } else {
        "Radio en vivo · esperando audio.".to_string()
    }
}

enum PlayOutcome {
    Completed,
    Skipped,
    Stop,
    PublisherFailed(String),
}

enum DirectInputOutcome {
    ResumePlaylist,
    SourceChanged,
    Skipped,
    Stop,
    PublisherFailed(String),
}

struct BroadcastSession<'a> {
    profile: &'a BroadcastProfile,
    credential: &'a str,
    started_at: &'a str,
}

fn play_entry(
    app: &AppHandle,
    entry: &BroadcastQueueEntry,
    session: &BroadcastSession<'_>,
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
    let publisher_ready = publisher.is_ready();
    runtime.update(
        app,
        if publisher_ready {
            "live"
        } else {
            "connecting"
        },
        if publisher_ready {
            format!("En vivo: {}", display_title(&playing))
        } else {
            format!("Preparando señal: {}", display_title(&playing))
        },
        Some(playing.clone()),
        Some(session.started_at.to_string()),
        ("info", "track_started"),
    );
    update_output_metadata_async(
        session.profile.clone(),
        session.credential.to_string(),
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

        if worker_audio.direct_source_live() {
            let source_title = worker_audio
                .application_live
                .then(|| {
                    application_audio_title(
                        worker_audio.application_bundle_id.as_deref(),
                        worker_audio.application_label.as_deref(),
                    )
                })
                .unwrap_or_else(|| "Línea directa".to_string());
            update_output_metadata_value_async(
                session.profile.clone(),
                session.credential.to_string(),
                source_title,
                runtime,
                app.clone(),
            );
            match stream_direct_input(app, publisher, runtime, commands, worker_audio) {
                DirectInputOutcome::ResumePlaylist => {
                    runtime.update(
                        app,
                        "live",
                        format!("En vivo: {}", display_title(&playing)),
                        Some(playing.clone()),
                        Some(session.started_at.to_string()),
                        ("info", "track_resumed"),
                    );
                    update_output_metadata_async(
                        session.profile.clone(),
                        session.credential.to_string(),
                        playing.clone(),
                        runtime,
                        app.clone(),
                    );
                }
                DirectInputOutcome::SourceChanged => continue,
                DirectInputOutcome::Skipped => {
                    let _ = decoder.kill();
                    let _ = decoder.wait();
                    let _ = update_entry_status(app, &entry.id, "skipped", None);
                    return PlayOutcome::Skipped;
                }
                DirectInputOutcome::Stop => {
                    let _ = decoder.kill();
                    let _ = decoder.wait();
                    let _ = update_entry_status(app, &entry.id, "queued", None);
                    return PlayOutcome::Stop;
                }
                DirectInputOutcome::PublisherFailed(error) => {
                    let _ = decoder.kill();
                    let _ = decoder.wait();
                    let _ = update_entry_status(app, &entry.id, "queued", None);
                    return PlayOutcome::PublisherFailed(error);
                }
            }
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
    credential: String,
    runtime: Arc<RuntimeState>,
    commands: Receiver<WorkerCommand>,
    started_at: String,
) {
    let mut reconnect_attempt = 0u32;
    let mut publisher: Option<Publisher> = None;
    let mut terminal_error: Option<String> = None;
    let mut worker_audio = WorkerAudio::from_profile(&profile);
    if worker_audio.configured {
        let device = profile.microphone_device.clone();
        match spawn_audio_input_capture(&app, &device, 1, None, "entrada de micrófono", &runtime) {
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
    if worker_audio.line_configured {
        let device = profile.line_input_device.clone();
        match spawn_audio_input_capture(
            &app,
            &device,
            profile.line_input_channel,
            Some(profile.line_input_stereo),
            "entrada de línea",
            &runtime,
        ) {
            Ok(line_input) => {
                worker_audio.line_input = Some(line_input);
                runtime.update_line_input(
                    &app,
                    worker_audio.line_status("Línea directa preparada y en espera."),
                    "info",
                    "line_input_ready",
                );
            }
            Err(error) => {
                runtime.update_line_input(
                    &app,
                    worker_audio
                        .line_status(format!("No se pudo preparar la línea directa: {error}")),
                    "error",
                    "line_input_failed",
                );
            }
        }
    } else {
        runtime.update_line_input(
            &app,
            worker_audio.line_status("Línea directa desactivada."),
            "info",
            "line_input_disabled",
        );
    }
    if worker_audio.application_configured {
        let bundle_id = profile.application_audio_bundle_id.clone();
        let selected = if bundle_id == application_audio::SYSTEM_AUDIO_TARGET_ID {
            Ok(application_audio::SYSTEM_AUDIO_LABEL.to_string())
        } else {
            application_audio::list_applications().and_then(|applications| {
                applications
                    .into_iter()
                    .find(|application| application.id == bundle_id)
                    .map(|application| application.label)
                    .ok_or_else(|| {
                        "La aplicación seleccionada no está abierta o ya no está disponible."
                            .to_string()
                    })
            })
        };
        match selected.and_then(|label| {
            spawn_application_audio_capture(&bundle_id, &label).map(|capture| (capture, label))
        }) {
            Ok((application_input, label)) => {
                worker_audio.application_label = Some(label);
                worker_audio.application_input = Some(application_input);
                runtime.update_application_audio(
                    &app,
                    worker_audio.application_status("Audio del Mac preparado y en espera."),
                    "info",
                    "application_audio_ready",
                );
            }
            Err(error) => {
                runtime.update_application_audio(
                    &app,
                    worker_audio.application_status(format!(
                        "No se pudo preparar el audio del Mac: {error}"
                    )),
                    "error",
                    "application_audio_failed",
                );
            }
        }
    } else {
        runtime.update_application_audio(
            &app,
            worker_audio.application_status("Audio del Mac desactivado."),
            "info",
            "application_audio_disabled",
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
            match spawn_publisher(&app, &profile, &credential, &runtime) {
                Ok(candidate) => {
                    publisher = Some(candidate);
                    reconnect_attempt = 0;
                    let publisher_ready =
                        publisher.as_ref().map(Publisher::is_ready).unwrap_or(false);
                    if publisher_ready {
                        runtime.mark_output_ready(&app, connected_message(&profile));
                    } else {
                        runtime.update(
                            &app,
                            "connecting",
                            format!(
                                "FFmpeg inició la salida; esperando confirmación de {}...",
                                destination_label(&profile)
                            ),
                            None,
                            Some(started_at.clone()),
                            ("info", "publisher_started"),
                        );
                    }
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

        if worker_audio.direct_source_live() {
            let source_title = worker_audio
                .application_live
                .then(|| {
                    application_audio_title(
                        worker_audio.application_bundle_id.as_deref(),
                        worker_audio.application_label.as_deref(),
                    )
                })
                .unwrap_or_else(|| "Línea directa".to_string());
            update_output_metadata_value_async(
                profile.clone(),
                credential.clone(),
                source_title,
                &runtime,
                app.clone(),
            );
            match stream_direct_input(
                &app,
                publisher.as_mut().expect("publisher initialized"),
                &runtime,
                &commands,
                &mut worker_audio,
            ) {
                DirectInputOutcome::Stop => break,
                DirectInputOutcome::PublisherFailed(error) => {
                    let fatal_message = publisher.as_ref().and_then(|publisher| {
                        fatal_publisher_failure_message(
                            &profile,
                            publisher.is_opened(),
                            publisher.is_ready(),
                        )
                    });
                    if let Some(publisher) = publisher.take() {
                        publisher.terminate();
                    }
                    if let Some(message) = fatal_message {
                        terminal_error = Some(message);
                        break;
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
                DirectInputOutcome::ResumePlaylist => {
                    update_output_metadata_value_async(
                        profile.clone(),
                        credential.clone(),
                        "Playlist".to_string(),
                        &runtime,
                        app.clone(),
                    );
                }
                DirectInputOutcome::SourceChanged => {}
                DirectInputOutcome::Skipped => {}
            }
            continue;
        }

        let next = open_db(&app).and_then(|conn| next_queue_entry(&conn));
        match next {
            Ok(Some(entry)) => {
                let session = BroadcastSession {
                    profile: &profile,
                    credential: &credential,
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
                        let fatal_message = publisher.as_ref().and_then(|publisher| {
                            fatal_publisher_failure_message(
                                &profile,
                                publisher.is_opened(),
                                publisher.is_ready(),
                            )
                        });
                        if let Some(publisher) = publisher.take() {
                            publisher.terminate();
                        }
                        if let Some(message) = fatal_message {
                            terminal_error = Some(message);
                            break;
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
                    let fatal_message = publisher.as_ref().and_then(|publisher| {
                        fatal_publisher_failure_message(
                            &profile,
                            publisher.is_opened(),
                            publisher.is_ready(),
                        )
                    });
                    if let Some(publisher) = publisher.take() {
                        publisher.terminate();
                    }
                    if let Some(message) = fatal_message {
                        terminal_error = Some(message);
                        break;
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
                } else {
                    thread::sleep(Duration::from_millis(SILENCE_CHUNK_MILLIS as u64));
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
    runtime.update_line_input(
        &app,
        worker_audio.line_status(if worker_audio.line_configured {
            "Línea directa detenida."
        } else {
            "Línea directa desactivada."
        }),
        "info",
        "line_input_stopped",
    );
    runtime.update_application_audio(
        &app,
        worker_audio.application_status(if worker_audio.application_configured {
            "Audio del Mac detenido."
        } else {
            "Audio del Mac desactivado."
        }),
        "info",
        "application_audio_stopped",
    );
    if let Some(message) = terminal_error {
        runtime.update(
            &app,
            "error",
            message,
            None,
            None,
            ("error", "destination_rejected"),
        );
    } else {
        runtime.update(
            &app,
            "idle",
            "Radio detenida.",
            None,
            None,
            ("info", "stopped"),
        );
    }
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
        format!("Destino desconectado. Reintentando en {seconds}s: {reason}"),
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
            Ok(WorkerCommand::SetLineInputLive(live)) => {
                if let Err(error) = worker_audio.set_line_live(app, runtime, live) {
                    runtime.log(app, "error", "line_input", error);
                }
            }
            Ok(WorkerCommand::SetApplicationAudioLive(live)) => {
                if let Err(error) = worker_audio.set_application_live(app, runtime, live) {
                    runtime.log(app, "error", "application_audio", error);
                }
            }
            Err(TryRecvError::Empty) => return action,
        }
    }
}

fn stream_direct_input(
    app: &AppHandle,
    publisher: &mut Publisher,
    runtime: &Arc<RuntimeState>,
    commands: &Receiver<WorkerCommand>,
    worker_audio: &mut WorkerAudio,
) -> DirectInputOutcome {
    let started_as_application = worker_audio.application_live;
    while worker_audio.direct_source_live() {
        match poll_worker_commands(commands, app, runtime, worker_audio) {
            WorkerAction::Stop => return DirectInputOutcome::Stop,
            WorkerAction::Skip => return DirectInputOutcome::Skipped,
            WorkerAction::None => {}
        }
        if !worker_audio.direct_source_live() {
            break;
        }
        if worker_audio.application_live != started_as_application {
            return DirectInputOutcome::SourceChanged;
        }
        let mut output = silence_chunk_millis(LINE_INPUT_CHUNK_MILLIS);
        worker_audio.process_direct_chunk(app, runtime, &mut output);
        if let Err(error) = publisher.write(&output) {
            return DirectInputOutcome::PublisherFailed(error);
        }
        thread::sleep(Duration::from_millis(LINE_INPUT_CHUNK_MILLIS as u64));
    }
    DirectInputOutcome::ResumePlaylist
}

fn silence_chunk() -> Vec<u8> {
    silence_chunk_millis(SILENCE_CHUNK_MILLIS)
}

fn silence_chunk_millis(millis: usize) -> Vec<u8> {
    let bytes = PCM_SAMPLE_RATE * PCM_CHANNELS * PCM_BYTES_PER_SAMPLE * millis / 1000;
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

fn update_output_metadata_async(
    profile: BroadcastProfile,
    credential: String,
    entry: BroadcastQueueEntry,
    runtime: &Arc<RuntimeState>,
    app: AppHandle,
) {
    update_output_metadata_value_async(profile, credential, display_title(&entry), runtime, app);
}

fn update_output_metadata_value_async(
    profile: BroadcastProfile,
    credential: String,
    value: String,
    runtime: &Arc<RuntimeState>,
    app: AppHandle,
) {
    if profile.output_kind == OUTPUT_KIND_ICECAST {
        update_icecast_metadata_value_async(profile, credential, value, runtime, app);
    }
}

fn update_icecast_metadata_value_async(
    profile: BroadcastProfile,
    password: String,
    song: String,
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
                        ("song", song.as_str()),
                        ("charset", "UTF-8"),
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

    let (appended_total, skipped_missing_total) =
        append_track_snapshots(&tx, library_id, playlist_path, &playlist_name, tracks)?;
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar cola de broadcast: {error}"))?;
    Ok(BroadcastQueueAppendResult {
        appended_total,
        skipped_missing_total,
        queue: list_queue(conn)?,
    })
}

fn append_draft(
    conn: &mut Connection,
    draft_id: &str,
) -> Result<BroadcastQueueAppendResult, String> {
    let draft_id = draft_id.trim();
    if draft_id.is_empty() || draft_id.len() > 512 || draft_id.chars().any(char::is_control) {
        return Err("Selecciona una playlist local válida.".to_string());
    }
    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transacción de broadcast: {error}"))?;
    let (library_id, playlist_name) = tx
        .query_row(
            "SELECT library_id, name FROM playlist_drafts WHERE id = ?1",
            params![draft_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer playlist local: {error}"))?
        .ok_or_else(|| "Playlist local no encontrada.".to_string())?;
    let tracks = {
        let mut stmt = tx
            .prepare(
                "SELECT dt.track_id, t.source_path, t.name, t.artist, t.total_time, t.source_exists
                 FROM playlist_draft_tracks dt
                 JOIN playlist_index_tracks t
                   ON t.library_id = ?2 AND t.track_id = dt.track_id
                 WHERE dt.draft_id = ?1
                 ORDER BY dt.position, dt.track_id",
            )
            .map_err(|error| {
                format!("No se pudo preparar playlist local para broadcast: {error}")
            })?;
        let rows = stmt
            .query_map(params![draft_id, &library_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })
            .map_err(|error| format!("No se pudieron leer tracks de playlist local: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("No se pudieron mapear tracks de playlist local: {error}"))?
    };
    if tracks.is_empty() {
        return Err("La playlist local no contiene pistas indexadas.".to_string());
    }

    let playlist_path = format!("__local_draft__:{draft_id}");
    let (appended_total, skipped_missing_total) =
        append_track_snapshots(&tx, &library_id, &playlist_path, &playlist_name, tracks)?;
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar cola de broadcast: {error}"))?;
    Ok(BroadcastQueueAppendResult {
        appended_total,
        skipped_missing_total,
        queue: list_queue(conn)?,
    })
}

type BroadcastTrackSnapshot = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u64>,
    bool,
);

fn append_track_snapshots(
    tx: &rusqlite::Transaction<'_>,
    library_id: &str,
    playlist_path: &str,
    playlist_name: &str,
    tracks: Vec<BroadcastTrackSnapshot>,
) -> Result<(usize, usize), String> {
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
    Ok((appended_total, skipped_missing_total))
}

fn append_track(
    conn: &mut Connection,
    library_id: &str,
    track_id: &str,
) -> Result<BroadcastQueueEntry, String> {
    let library_id = library_id.trim();
    let track_id = track_id.trim();
    if library_id.is_empty() || track_id.is_empty() {
        return Err("No se pudo identificar la pista indexada.".to_string());
    }
    if library_id.len() > 512
        || track_id.len() > 512
        || library_id.chars().any(char::is_control)
        || track_id.chars().any(char::is_control)
    {
        return Err("La pista seleccionada es inválida.".to_string());
    }

    let tx = conn
        .transaction()
        .map_err(|error| format!("No se pudo iniciar transacción de broadcast: {error}"))?;
    let track = tx
        .query_row(
            "SELECT source_path, name, artist, total_time, source_exists
             FROM playlist_index_tracks
             WHERE library_id = ?1 AND track_id = ?2",
            params![library_id, track_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<u64>>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("No se pudo leer la pista para broadcast: {error}"))?
        .ok_or_else(|| "La pista indexada ya no existe.".to_string())?;
    let (source_path, title, artist, duration_seconds, source_exists) = track;
    let source_path = source_path
        .filter(|value| !value.trim().is_empty())
        .filter(|_| source_exists)
        .ok_or_else(|| "La pista no tiene un archivo local disponible.".to_string())?;
    let position = tx
        .query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM broadcast_queue_entries",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("No se pudo calcular posición de broadcast: {error}"))?;
    let now = timestamp();
    let entry = BroadcastQueueEntry {
        id: Uuid::new_v4().to_string(),
        library_id: library_id.to_string(),
        track_id: track_id.to_string(),
        playlist_path: MANUAL_QUEUE_PATH.to_string(),
        playlist_name: MANUAL_QUEUE_NAME.to_string(),
        source_path,
        title: title.unwrap_or_else(|| "Sin título".to_string()),
        artist,
        duration_seconds,
        position,
        status: "queued".to_string(),
        error: None,
        inserted_at: now.clone(),
        updated_at: now,
    };
    tx.execute(
        "INSERT INTO broadcast_queue_entries (
           id, library_id, track_id, playlist_path, playlist_name, source_path,
           title, artist, duration_seconds, position, status, error, inserted_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?12)",
        params![
            entry.id,
            entry.library_id,
            entry.track_id,
            entry.playlist_path,
            entry.playlist_name,
            entry.source_path,
            entry.title,
            entry.artist,
            entry.duration_seconds,
            entry.position,
            entry.status,
            entry.inserted_at,
        ],
    )
    .map_err(|error| format!("No se pudo agregar la pista al broadcast: {error}"))?;
    tx.commit()
        .map_err(|error| format!("No se pudo confirmar la cola de broadcast: {error}"))?;
    Ok(entry)
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
          output_kind TEXT NOT NULL DEFAULT 'icecast',
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
          line_input_enabled INTEGER NOT NULL DEFAULT 0,
          line_input_device TEXT NOT NULL DEFAULT 'default',
          line_input_channel INTEGER NOT NULL DEFAULT 1,
          line_input_stereo INTEGER NOT NULL DEFAULT 1,
          line_input_gain_percent INTEGER NOT NULL DEFAULT 100,
          application_audio_enabled INTEGER NOT NULL DEFAULT 0,
          application_audio_bundle_id TEXT NOT NULL DEFAULT '',
          application_audio_gain_percent INTEGER NOT NULL DEFAULT 100,
          rtmp_platform TEXT NOT NULL DEFAULT 'instagram',
          rtmp_server_url TEXT NOT NULL DEFAULT '',
          rtmp_video_bitrate_kbps INTEGER NOT NULL DEFAULT 3500,
          rtmp_audio_bitrate_kbps INTEGER NOT NULL DEFAULT 128,
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
    ensure_broadcast_profile_column(conn, "output_kind", "TEXT NOT NULL DEFAULT 'icecast'")?;
    ensure_broadcast_profile_column(conn, "microphone_enabled", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_broadcast_profile_column(conn, "microphone_device", "TEXT NOT NULL DEFAULT 'default'")?;
    ensure_broadcast_profile_column(
        conn,
        "microphone_gain_percent",
        "INTEGER NOT NULL DEFAULT 100",
    )?;
    ensure_broadcast_profile_column(conn, "line_input_enabled", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_broadcast_profile_column(conn, "line_input_device", "TEXT NOT NULL DEFAULT 'default'")?;
    ensure_broadcast_profile_column(conn, "line_input_channel", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_broadcast_profile_column(conn, "line_input_stereo", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_broadcast_profile_column(
        conn,
        "line_input_gain_percent",
        "INTEGER NOT NULL DEFAULT 100",
    )?;
    ensure_broadcast_profile_column(
        conn,
        "application_audio_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_broadcast_profile_column(
        conn,
        "application_audio_bundle_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_broadcast_profile_column(
        conn,
        "application_audio_gain_percent",
        "INTEGER NOT NULL DEFAULT 100",
    )?;
    ensure_broadcast_profile_column(conn, "rtmp_platform", "TEXT NOT NULL DEFAULT 'instagram'")?;
    ensure_broadcast_profile_column(conn, "rtmp_server_url", "TEXT NOT NULL DEFAULT ''")?;
    ensure_broadcast_profile_column(
        conn,
        "rtmp_video_bitrate_kbps",
        "INTEGER NOT NULL DEFAULT 3500",
    )?;
    ensure_broadcast_profile_column(
        conn,
        "rtmp_audio_bitrate_kbps",
        "INTEGER NOT NULL DEFAULT 128",
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
            output_kind: OUTPUT_KIND_ICECAST.to_string(),
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
            line_input_enabled: true,
            line_input_device: "default".to_string(),
            line_input_channel: 1,
            line_input_stereo: true,
            line_input_gain_percent: 100,
            application_audio_enabled: true,
            application_audio_bundle_id: application_audio::SYSTEM_AUDIO_TARGET_ID.to_string(),
            application_audio_gain_percent: 100,
            rtmp_platform: RTMP_PLATFORM_INSTAGRAM.to_string(),
            rtmp_server_url: "rtmps://live-upload.instagram.com:443/rtmp/".to_string(),
            rtmp_video_bitrate_kbps: 3_500,
            rtmp_audio_bitrate_kbps: 128,
            password: Some("secret".to_string()),
            clear_password: false,
        }
    }

    fn profile() -> BroadcastProfile {
        BroadcastProfile {
            id: PROFILE_ID.to_string(),
            output_kind: OUTPUT_KIND_ICECAST.to_string(),
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
            line_input_enabled: true,
            line_input_device: "default".to_string(),
            line_input_channel: 1,
            line_input_stereo: true,
            line_input_gain_percent: 100,
            application_audio_enabled: true,
            application_audio_bundle_id: application_audio::SYSTEM_AUDIO_TARGET_ID.to_string(),
            application_audio_gain_percent: 100,
            rtmp_platform: RTMP_PLATFORM_INSTAGRAM.to_string(),
            rtmp_server_url: "rtmps://live-upload.instagram.com:443/rtmp/".to_string(),
            rtmp_video_bitrate_kbps: 3_500,
            rtmp_audio_bitrate_kbps: 128,
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
        let mut invalid = profile_input();
        invalid.line_input_channel = 0;
        assert!(validate_profile(invalid).is_err());
        let mut invalid = profile_input();
        invalid.line_input_gain_percent = 201;
        assert!(validate_profile(invalid).is_err());
        let mut invalid = profile_input();
        invalid.application_audio_bundle_id.clear();
        assert!(validate_profile(invalid).is_err());
        let mut invalid = profile_input();
        invalid.application_audio_gain_percent = 201;
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
        assert!(!args.iter().any(|value| value == "-re"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-channel_layout", "stereo"]));
    }

    #[test]
    fn validates_instagram_rtmp_profile_boundaries() {
        let mut input = profile_input();
        input.output_kind = OUTPUT_KIND_RTMP.to_string();
        assert!(validate_profile(input.clone()).is_ok());

        input.rtmp_server_url = "rtmp://live-upload.instagram.com/rtmp/".to_string();
        assert!(validate_profile(input.clone()).is_err());

        input.rtmp_server_url = "rtmps://live-upload.instagram.com/rtmp/".to_string();
        input.rtmp_video_bitrate_kbps = 6_001;
        assert!(validate_profile(input).is_err());
    }

    #[test]
    fn validates_ephemeral_rtmp_stream_keys() {
        assert_eq!(
            validate_stream_key(Some("session-key".to_string())).unwrap(),
            "session-key"
        );
        assert!(validate_stream_key(None).is_err());
        assert!(validate_stream_key(Some("key with spaces".to_string())).is_err());
        assert!(validate_stream_key(Some(
            "rtmps://live-upload.instagram.com/rtmp/session-key".to_string()
        ))
        .is_err());
    }

    #[test]
    fn rtmp_publisher_uses_vertical_h264_aac_flv_output() {
        let mut profile = profile();
        profile.output_kind = OUTPUT_KIND_RTMP.to_string();
        let args = publisher_args(&profile, "session-key");

        assert!(args.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(args.windows(2).any(|pair| pair == ["-f", "flv"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-rtmp_flush_interval", "1"]));
        assert!(args.windows(2).any(|pair| pair == ["-tcp_nodelay", "1"]));
        assert!(args.windows(2).any(|pair| pair == ["-loglevel", "debug"]));
        assert!(args.iter().any(|value| value == "-nostats"));
        assert!(args.windows(2).any(|pair| pair == ["-progress", "pipe:2"]));
        assert_eq!(
            args.iter().filter(|value| value.as_str() == "-re").count(),
            1
        );
        assert!(args
            .windows(4)
            .any(|values| values == ["-re", "-f", "lavfi", "-i"]));
        assert!(!args.iter().any(|value| value == "-minrate"));
        assert!(!args.iter().any(|value| value == "-x264-params"));
        assert!(args.iter().any(|value| value.contains("720x1280")));
        assert!(args.iter().any(|value| value.contains("testsrc2")));
        assert!(!args.iter().any(|value| value.contains("overlay")));
        assert!(args.windows(2).any(|pair| pair == ["-map", "1:v:0"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "0:a:0"]));
        assert_eq!(
            args.last().unwrap(),
            "rtmps://live-upload.instagram.com:443/rtmp/session-key"
        );
    }

    #[test]
    fn publisher_logs_redact_ephemeral_stream_keys() {
        assert_eq!(
            redact_secret(
                "Failed to open rtmps://example.test/live/private-key",
                "private-key"
            ),
            "Failed to open rtmps://example.test/live/********"
        );
    }

    #[test]
    fn rtmp_readiness_waits_for_sustained_media_progress() {
        assert!(is_rtmp_output_open_line(
            "Output #0, flv, to 'rtmps://example.test/rtmp/private-key':"
        ));
        assert!(!is_rtmp_output_ready_line("out_time_us=700000"));
        assert!(is_rtmp_output_ready_line("out_time_us=2000000"));
        assert!(is_rtmp_output_ready_line("out_time_us=3123456"));
        assert!(!is_rtmp_output_ready_line(
            "Input #0, s16le, from 'pipe:0':"
        ));
        assert!(is_publisher_warning_line(
            "Error submitting a packet to the muxer: End of file"
        ));
        assert!(!is_publisher_warning_line(
            "Stream #0:1: Audio: aac (LC), 44100 Hz, stereo"
        ));
        assert!(!is_publisher_warning_line(
            "Input stream #0:0 (audio): 7 frames decoded; 0 decode errors"
        ));
        assert!(is_rtmp_diagnostic_line(
            "[rtmps @ 0x123] Sending publish command for 'private-key'"
        ));
        assert!(!is_rtmp_diagnostic_line(
            "[AVFilterGraph @ 0x123] query_formats: 7 queried"
        ));

        let mut rtmp_profile = profile();
        rtmp_profile.output_kind = OUTPUT_KIND_RTMP.to_string();
        assert!(fatal_publisher_failure_message(&rtmp_profile, false, false)
            .unwrap()
            .contains("rechazó la publicación"));
        assert!(fatal_publisher_failure_message(&rtmp_profile, true, false)
            .unwrap()
            .contains("dos segundos continuos"));
        assert!(fatal_publisher_failure_message(&rtmp_profile, true, true).is_none());
        assert!(fatal_publisher_failure_message(&profile(), false, false).is_none());
    }

    #[test]
    fn decoder_is_paced_in_real_time_before_mixing() {
        let args = decoder_args("track.wav");
        assert!(args.windows(2).any(|pair| pair == ["-re", "-i"]));
    }

    #[test]
    fn silence_chunk_is_exactly_a_quarter_second_of_pcm() {
        assert_eq!(silence_chunk().len(), 44_100);
    }

    #[test]
    fn audio_input_normalizes_mono_frames_to_stereo() {
        let mut target = VecDeque::new();
        append_audio_input_frames(&[-1.0_f32, 0.5, 1.0], 1, 0, false, &mut target, |sample| {
            (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
        });
        assert_eq!(target.len(), 3);
        assert_eq!(target[0], [-i16::MAX, -i16::MAX]);
        assert_eq!(target[2], [i16::MAX, i16::MAX]);
    }

    #[test]
    fn audio_input_selects_a_stereo_pair() {
        let mut target = VecDeque::new();
        append_audio_input_frames(
            &[10_i16, 20, 30, 40, 11, 21, 31, 41],
            4,
            2,
            true,
            &mut target,
            |sample| sample,
        );
        assert_eq!(target.into_iter().collect::<Vec<_>>(), [[30, 40], [31, 41]]);
    }

    #[test]
    fn microphone_mix_applies_gain_and_clamps() {
        assert_eq!(mix_pcm_sample(1_000, 2_000, 50, 35), 1_350);
        assert_eq!(mix_pcm_sample(30_000, 30_000, 100, 35), i16::MAX);
        assert_eq!(mix_pcm_sample(-30_000, -30_000, 100, 35), i16::MIN);
        assert_eq!(mix_pcm_sample(1_000, 0, 100, 100), 1_000);
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

    #[test]
    fn append_draft_snapshots_local_playlist_tracks_in_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE playlist_drafts (
              id TEXT PRIMARY KEY, library_id TEXT NOT NULL, name TEXT NOT NULL
            );
            CREATE TABLE playlist_draft_tracks (
              draft_id TEXT NOT NULL, track_id TEXT NOT NULL, position INTEGER NOT NULL
            );
            CREATE TABLE playlist_index_tracks (
              library_id TEXT NOT NULL, track_id TEXT NOT NULL, source_path TEXT,
              name TEXT, artist TEXT, total_time INTEGER, source_exists INTEGER NOT NULL,
              PRIMARY KEY(library_id, track_id)
            );
            INSERT INTO playlist_drafts VALUES ('draft-1', 'lib', 'Selección local');
            INSERT INTO playlist_index_tracks VALUES ('lib', '1', '/music/one.wav', 'One', 'Artist', 10, 1);
            INSERT INTO playlist_index_tracks VALUES ('lib', '2', NULL, 'Missing', NULL, 20, 0);
            INSERT INTO playlist_draft_tracks VALUES ('draft-1', '1', 0);
            INSERT INTO playlist_draft_tracks VALUES ('draft-1', '2', 1);
            ",
        )
        .unwrap();

        let result = append_draft(&mut conn, "draft-1").unwrap();
        assert_eq!(result.appended_total, 1);
        assert_eq!(result.skipped_missing_total, 1);
        assert_eq!(result.queue[0].playlist_name, "Selección local");
        assert_eq!(result.queue[0].playlist_path, "__local_draft__:draft-1");
        assert_eq!(result.queue[0].title, "One");
    }

    #[test]
    fn append_track_adds_one_available_track_to_the_end_of_the_queue() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE playlist_index_tracks (
              library_id TEXT NOT NULL, track_id TEXT NOT NULL, source_path TEXT,
              name TEXT, artist TEXT, total_time INTEGER, source_exists INTEGER NOT NULL,
              PRIMARY KEY(library_id, track_id)
            );
            INSERT INTO playlist_index_tracks VALUES
              ('lib', '1', '/music/one.wav', 'One', 'Artist', 10, 1),
              ('lib', '2', '/music/two.wav', 'Two', NULL, 20, 1),
              ('lib', '3', NULL, 'Missing', NULL, 30, 0);
            ",
        )
        .unwrap();

        let first = append_track(&mut conn, "lib", "1").unwrap();
        let second = append_track(&mut conn, "lib", "2").unwrap();

        assert_eq!(first.playlist_path, MANUAL_QUEUE_PATH);
        assert_eq!(first.playlist_name, MANUAL_QUEUE_NAME);
        assert_eq!(first.position, 1);
        assert_eq!(second.position, 2);
        assert_eq!(list_queue(&conn).unwrap().len(), 2);
        assert!(append_track(&mut conn, "lib", "3").is_err());
    }
}
