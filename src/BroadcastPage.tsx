import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  AudioLines,
  Camera,
  Check,
  ChevronsUpDown,
  GripVertical,
  Library,
  LoaderCircle,
  Mic,
  MicOff,
  Monitor,
  Play,
  Plus,
  Radio,
  RefreshCcw,
  Save,
  SlidersHorizontal,
  SkipForward,
  Square,
  Trash2,
  Wifi
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList
} from "./components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "./components/ui/popover";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

const SYSTEM_AUDIO_TARGET_ID = "__system_audio__";

type BroadcastProfile = {
  id: string;
  output_kind: "icecast" | "rtmp" | string;
  host: string;
  port: number;
  mount: string;
  username: string;
  station_name: string;
  description: string;
  bitrate_kbps: number;
  tls: boolean;
  public: boolean;
  microphone_enabled: boolean;
  microphone_device: string;
  microphone_gain_percent: number;
  line_input_enabled: boolean;
  line_input_device: string;
  line_input_channel: number;
  line_input_stereo: boolean;
  line_input_gain_percent: number;
  application_audio_enabled: boolean;
  application_audio_bundle_id: string;
  application_audio_gain_percent: number;
  rtmp_platform: "instagram" | "custom" | string;
  rtmp_server_url: string;
  rtmp_video_bitrate_kbps: number;
  rtmp_audio_bitrate_kbps: number;
  video_compositor: BroadcastVideoCompositor;
  password_configured: boolean;
  listener_url: string;
  updated_at: string;
};

type BroadcastVideoCompositor = {
  enabled: boolean;
  captureMode: "native" | "browser" | string;
  cameraEnabled: boolean;
  cameraDevice: string;
  cameraPosition: "top_left" | "top_right" | "center" | "bottom_left" | "bottom_right" | string;
  cameraSize: "small" | "medium" | "large" | string;
  cameraEffect: "clean" | "mono" | "contrast" | "dream" | string;
  cameraMirror: boolean;
  cameraRotationDegrees: 0 | 90 | 180 | 270 | number;
  cameraFraming: "contain" | "cover" | string;
  cameraLayout: "card" | "wide" | "background" | string;
  cameraOpacityPercent: number;
  screenEnabled: boolean;
  screenLabel: string;
  screenPosition: "top_left" | "top_right" | "center" | "bottom_left" | "bottom_right" | string;
  screenSize: "small" | "medium" | "large" | string;
  screenEffect: "clean" | "mono" | "contrast" | "dream" | string;
  screenMirror: boolean;
  screenRotationDegrees: number;
  screenFraming: "contain" | "cover" | string;
  screenLayout: "card" | "wide" | "background" | string;
  screenOpacityPercent: number;
  transitionMillis: number;
};

type BroadcastPreflight = {
  ffmpeg_available: boolean;
  mp3_encoder_available: boolean;
  icecast_protocol_available: boolean;
  tls_protocol_available: boolean;
  h264_encoder_available: boolean;
  aac_encoder_available: boolean;
  rtmp_protocol_available: boolean;
  rtmps_protocol_available: boolean;
  flv_muxer_available: boolean;
  visualizer_filter_available: boolean;
  overlay_filter_available: boolean;
  camera_input_available: boolean;
  camera_filter_available: boolean;
  microphone_input_available: boolean;
  ready: boolean;
  message: string;
};

type BroadcastMicrophoneDevice = {
  id: string;
  label: string;
  is_default: boolean;
  input_channels: number;
};

type BroadcastApplicationAudioDevice = {
  id: string;
  label: string;
  process_id: number;
};

type BroadcastCameraDevice = {
  id: string;
  label: string;
  kind: "camera" | "screen" | string;
};

type BroadcastMicrophoneStatus = {
  configured: boolean;
  ready: boolean;
  live: boolean;
  receiving_audio: boolean;
  level_percent: number;
  device?: string | null;
  gain_percent: number;
  message: string;
};

type BroadcastLineInputStatus = {
  configured: boolean;
  ready: boolean;
  live: boolean;
  receiving_audio: boolean;
  level_percent: number;
  device?: string | null;
  channel: number;
  stereo: boolean;
  gain_percent: number;
  message: string;
};

type BroadcastApplicationAudioStatus = {
  configured: boolean;
  ready: boolean;
  live: boolean;
  receiving_audio: boolean;
  level_percent: number;
  application?: string | null;
  label?: string | null;
  gain_percent: number;
  message: string;
};

type BroadcastQueueEntry = {
  id: string;
  library_id: string;
  track_id: string;
  playlist_path: string;
  playlist_name: string;
  source_path: string;
  title: string;
  artist?: string | null;
  duration_seconds?: number | null;
  position: number;
  status: "queued" | "playing" | "played" | "skipped" | "failed" | string;
  error?: string | null;
  inserted_at: string;
  updated_at: string;
};

type BroadcastStatus = {
  status: "idle" | "connecting" | "live" | "reconnecting" | "stopping" | "error" | string;
  message: string;
  now_playing?: BroadcastQueueEntry | null;
  started_at?: string | null;
  source_mode: "playlist" | "line_input" | "application_audio" | string;
  microphone: BroadcastMicrophoneStatus;
  line_input: BroadcastLineInputStatus;
  application_audio: BroadcastApplicationAudioStatus;
  camera: {
    configured: boolean;
    ready: boolean;
    live: boolean;
    mix_percent: number;
    device?: string | null;
    label?: string | null;
    transition_millis: number;
    message: string;
  };
  updated_at: string;
};

type BroadcastProgressEvent = {
  level: "info" | "warning" | "error" | string;
  event: string;
  message: string;
  status: BroadcastStatus;
  timestamp: string;
};

type PlaylistIndexLibrary = {
  id: string;
  source_name: string;
  track_count: number;
  playlist_count: number;
};

type PlaylistIndexPlaylist = {
  library_id: string;
  path: string;
  name: string;
  track_count: number;
  position: number;
};

type PlaylistDraft = {
  id: string;
  library_id: string;
  name: string;
  description?: string | null;
  track_count: number;
};

type BroadcastPlaylistSource = {
  key: string;
  kind: "local" | "rekordbox";
  id: string;
  library_id: string;
  library_name: string;
  name: string;
  track_count: number;
};

type QueueAppendResult = {
  appended_total: number;
  skipped_missing_total: number;
  queue: BroadcastQueueEntry[];
};

type BusyAction = "loading" | "saving" | "starting" | "stopping" | "skipping" | "appending" | "clearing" | string | null;
type BroadcastSourceTab = "microphone" | "line_input" | "system_audio";
type BroadcastOutputKind = "icecast" | "rtmp";
type RtmpPlatform = "instagram" | "custom";

const defaultVideoCompositor: BroadcastVideoCompositor = {
  enabled: false,
  captureMode: "browser",
  cameraEnabled: true,
  cameraDevice: "default",
  cameraPosition: "top_right",
  cameraSize: "medium",
  cameraEffect: "mono",
  cameraMirror: true,
  cameraRotationDegrees: 180,
  cameraFraming: "contain",
  cameraLayout: "wide",
  cameraOpacityPercent: 100,
  screenEnabled: false,
  screenLabel: "",
  screenPosition: "top_left",
  screenSize: "large",
  screenEffect: "clean",
  screenMirror: false,
  screenRotationDegrees: 0,
  screenFraming: "contain",
  screenLayout: "background",
  screenOpacityPercent: 100,
  transitionMillis: 800
};

const fieldClass =
  "h-10 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none transition focus:border-foreground/35 focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

async function loadBroadcastPlaylistSources(): Promise<BroadcastPlaylistSource[]> {
  const [libraries, drafts] = await Promise.all([
    invoke<PlaylistIndexLibrary[]>("playlist_index_libraries"),
    invoke<PlaylistDraft[]>("playlist_index_drafts", { libraryId: null })
  ]);
  const indexedPlaylists = await Promise.all(
    libraries.map((library) => invoke<PlaylistIndexPlaylist[]>("playlist_index_library_playlists", { libraryId: library.id }))
  );
  const libraryNames = new Map(libraries.map((library) => [library.id, library.source_name]));
  const localSources = drafts
    .filter((draft) => draft.track_count > 0)
    .map((draft): BroadcastPlaylistSource => ({
      key: `local:${draft.id}`,
      kind: "local",
      id: draft.id,
      library_id: draft.library_id,
      library_name: libraryNames.get(draft.library_id) ?? draft.library_id,
      name: draft.name,
      track_count: draft.track_count
    }));
  const rekordboxSources = libraries.flatMap((library, libraryIndex) =>
    indexedPlaylists[libraryIndex]
      .filter((playlist) => playlist.track_count > 0)
      .map((playlist): BroadcastPlaylistSource => ({
        key: `rekordbox:${library.id}:${playlist.path}`,
        kind: "rekordbox",
        id: playlist.path,
        library_id: library.id,
        library_name: library.source_name,
        name: playlist.name,
        track_count: playlist.track_count
      }))
  );
  return [...localSources, ...rekordboxSources];
}

export function BroadcastPage() {
  const { locale, t } = useI18n();
  const [profile, setProfile] = useState<BroadcastProfile | null>(null);
  const [preflight, setPreflight] = useState<BroadcastPreflight | null>(null);
  const [status, setStatus] = useState<BroadcastStatus | null>(null);
  const [queue, setQueue] = useState<BroadcastQueueEntry[]>([]);
  const [playlistSources, setPlaylistSources] = useState<BroadcastPlaylistSource[]>([]);
  const [microphoneDevices, setMicrophoneDevices] = useState<BroadcastMicrophoneDevice[]>([]);
  const [applicationAudioDevices, setApplicationAudioDevices] = useState<BroadcastApplicationAudioDevice[]>([]);
  const [cameraDevices, setCameraDevices] = useState<BroadcastCameraDevice[]>([]);
  const [playlistSourceKey, setPlaylistSourceKey] = useState("");
  const [playlistComboboxOpen, setPlaylistComboboxOpen] = useState(false);
  const [terminalLogs, setTerminalLogs] = useState<TerminalLogEntry[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [busy, setBusy] = useState<BusyAction>("loading");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [draggedQueueEntryId, setDraggedQueueEntryId] = useState<string | null>(null);

  const [outputKind, setOutputKind] = useState<BroadcastOutputKind>("icecast");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("8000");
  const [mount, setMount] = useState("/live.mp3");
  const [username, setUsername] = useState("source");
  const [stationName, setStationName] = useState("Rau Studio Radio");
  const [description, setDescription] = useState("");
  const [bitrate, setBitrate] = useState("128");
  const [tls, setTls] = useState(false);
  const [isPublic, setIsPublic] = useState(false);
  const [password, setPassword] = useState("");
  const [clearPassword, setClearPassword] = useState(false);
  const [rtmpPlatform, setRtmpPlatform] = useState<RtmpPlatform>("instagram");
  const [rtmpServerUrl, setRtmpServerUrl] = useState("");
  const [rtmpVideoBitrate, setRtmpVideoBitrate] = useState("3500");
  const [rtmpAudioBitrate, setRtmpAudioBitrate] = useState("128");
  const [streamKey, setStreamKey] = useState("");
  const [microphoneEnabled, setMicrophoneEnabled] = useState(false);
  const [microphoneDevice, setMicrophoneDevice] = useState("default");
  const [microphoneGain, setMicrophoneGain] = useState("100");
  const [lineInputEnabled, setLineInputEnabled] = useState(false);
  const [lineInputDevice, setLineInputDevice] = useState("default");
  const [lineInputChannel, setLineInputChannel] = useState("1");
  const [lineInputStereo, setLineInputStereo] = useState(true);
  const [lineInputGain, setLineInputGain] = useState("100");
  const [applicationAudioEnabled, setApplicationAudioEnabled] = useState(false);
  const [applicationAudioBundleId, setApplicationAudioBundleId] = useState("");
  const [applicationAudioGain, setApplicationAudioGain] = useState("100");
  const [videoCompositor, setVideoCompositor] = useState<BroadcastVideoCompositor>(defaultVideoCompositor);
  const [videoStudioOpen, setVideoStudioOpen] = useState(false);
  const [cameraMix, setCameraMix] = useState(0);
  const [sourceTab, setSourceTab] = useState<BroadcastSourceTab>("microphone");
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);

  const running = status ? ["connecting", "live", "reconnecting", "stopping"].includes(status.status) : false;
  const destinationNeedsSave = !profile
    || outputKind !== (profile.output_kind === "rtmp" ? "rtmp" : "icecast")
    || (outputKind === "rtmp" && (
      rtmpPlatform !== (profile.rtmp_platform === "custom" ? "custom" : "instagram")
      || rtmpServerUrl.trim() !== profile.rtmp_server_url
      || Number(rtmpVideoBitrate) !== profile.rtmp_video_bitrate_kbps
      || Number(rtmpAudioBitrate) !== profile.rtmp_audio_bitrate_kbps
      || JSON.stringify(videoCompositor) !== JSON.stringify(profile.video_compositor)
    ));
  const queuedEntries = queue.filter((entry) => entry.status === "queued");
  const queuedTotal = queuedEntries.length;
  const completedTotal = queue.filter((entry) => entry.status === "played").length;
  const failedTotal = queue.filter((entry) => entry.status === "failed").length;
  const applicationAudioDetail = translateBackendMessage(
    locale,
    status?.application_audio?.message ?? t("Audio del Mac esperando inicio.")
  );
  const applicationAudioNeedsAttention = Boolean(
    running &&
    profile?.application_audio_enabled &&
    status?.application_audio?.configured &&
    !status.application_audio.ready
  );
  const applicationAudioPermissionMissing = /autoriz|permiso|tcc|capture/i.test(
    status?.application_audio?.message ?? ""
  );

  const hydrateProfile = useCallback((next: BroadcastProfile) => {
    setProfile(next);
    setOutputKind(next.output_kind === "rtmp" ? "rtmp" : "icecast");
    setHost(next.host);
    setPort(String(next.port));
    setMount(next.mount);
    setUsername(next.username);
    setStationName(next.station_name);
    setDescription(next.description);
    setBitrate(String(next.bitrate_kbps));
    setTls(next.tls);
    setIsPublic(next.public);
    setMicrophoneEnabled(next.microphone_enabled);
    setMicrophoneDevice(next.microphone_device || "default");
    setMicrophoneGain(String(next.microphone_gain_percent));
    setLineInputEnabled(next.line_input_enabled);
    setLineInputDevice(next.line_input_device || "default");
    setLineInputChannel(String(next.line_input_channel || 1));
    setLineInputStereo(next.line_input_stereo);
    setLineInputGain(String(next.line_input_gain_percent));
    setApplicationAudioEnabled(next.application_audio_enabled);
    setApplicationAudioBundleId(next.application_audio_bundle_id || SYSTEM_AUDIO_TARGET_ID);
    setApplicationAudioGain(String(next.application_audio_gain_percent));
    setRtmpPlatform(next.rtmp_platform === "custom" ? "custom" : "instagram");
    setRtmpServerUrl(next.rtmp_server_url);
    setRtmpVideoBitrate(String(next.rtmp_video_bitrate_kbps));
    setRtmpAudioBitrate(String(next.rtmp_audio_bitrate_kbps));
    setVideoCompositor(next.video_compositor ?? defaultVideoCompositor);
    setSourceTab(next.application_audio_enabled ? "system_audio" : next.line_input_enabled ? "line_input" : "microphone");
    setPassword("");
    setClearPassword(false);
  }, []);

  const refreshRuntime = useCallback(async () => {
    const [nextStatus, nextQueue, nextPreflight] = await Promise.all([
      invoke<BroadcastStatus>("broadcast_status"),
      invoke<BroadcastQueueEntry[]>("broadcast_queue"),
      invoke<BroadcastPreflight>("broadcast_preflight")
    ]);
    setStatus(nextStatus);
    setCameraMix(nextStatus.camera?.mix_percent ?? 0);
    setQueue(nextQueue);
    setPreflight(nextPreflight);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    let unlistenCalled = false;
    const stopListeningOnce = (candidate = unlisten) => {
      if (!candidate || unlistenCalled) return;
      unlistenCalled = true;
      safelyUnlisten(candidate);
    };
    void Promise.all([
      invoke<BroadcastProfile>("broadcast_profile"),
      invoke<BroadcastStatus>("broadcast_status"),
      invoke<BroadcastQueueEntry[]>("broadcast_queue"),
      invoke<BroadcastPreflight>("broadcast_preflight"),
      loadBroadcastPlaylistSources(),
      invoke<BroadcastMicrophoneDevice[]>("broadcast_microphone_devices"),
      invoke<BroadcastCameraDevice[]>("broadcast_camera_devices").catch(() => [])
    ])
      .then(([nextProfile, nextStatus, nextQueue, nextPreflight, nextPlaylistSources, nextMicrophones, nextCameras]) => {
        if (disposed) return;
        hydrateProfile(nextProfile);
        setStatus(nextStatus);
        setQueue(nextQueue);
        setPreflight(nextPreflight);
        setPlaylistSources(nextPlaylistSources);
        setMicrophoneDevices(nextMicrophones);
        setCameraDevices(nextCameras);
        setCameraMix(nextStatus.camera?.mix_percent ?? 0);
        if (!nextMicrophones.some((device) => device.id === nextProfile.microphone_device)) {
          setMicrophoneDevice(nextMicrophones[0]?.id ?? "default");
        }
        if (!nextMicrophones.some((device) => device.id === nextProfile.line_input_device)) {
          setLineInputDevice(nextMicrophones[0]?.id ?? "default");
        }
      })
      .catch((cause) => setError(errorMessage(cause, locale)))
      .finally(() => setBusy(null));

    void listen<BroadcastProgressEvent>("broadcast-progress", ({ payload }) => {
      setStatus(payload.status);
      setCameraMix(payload.status.camera?.mix_percent ?? 0);
      if (!payload.event.endsWith("_level")) {
        const level: TerminalLogEntry["level"] = payload.level === "error"
          ? "error"
          : payload.level === "warning"
            ? "warning"
            : "info";
        setTerminalLogs((current) => [...current, {
          id: nextTerminalLogId.current++,
          time: new Date(payload.timestamp).toLocaleTimeString(),
          level,
          name: payload.event,
          message: translateBackendMessage(locale, payload.message)
        }].slice(-1200));
        window.requestAnimationFrame(() => {
          if (terminalElement.current) {
            terminalElement.current.scrollTop = terminalElement.current.scrollHeight;
          }
        });
      }
      void invoke<BroadcastQueueEntry[]>("broadcast_queue").then(setQueue).catch(() => undefined);
    })
      .then((stopListening) => {
        if (disposed) stopListeningOnce(stopListening);
        else unlisten = stopListening;
      })
      .catch(() => undefined);

    const timer = window.setInterval(() => {
      void Promise.all([
        invoke<BroadcastStatus>("broadcast_status").then((nextStatus) => {
          setStatus(nextStatus);
          setCameraMix(nextStatus.camera?.mix_percent ?? 0);
        }),
        invoke<BroadcastQueueEntry[]>("broadcast_queue").then(setQueue)
      ]).catch(() => undefined);
    }, 2500);

    return () => {
      disposed = true;
      window.clearInterval(timer);
      stopListeningOnce();
    };
  }, [hydrateProfile, locale]);

  const selectedPlaylistSource = useMemo(
    () => playlistSources.find((source) => source.key === playlistSourceKey) ?? null,
    [playlistSourceKey, playlistSources]
  );
  const localPlaylistSources = useMemo(
    () => playlistSources.filter((source) => source.kind === "local"),
    [playlistSources]
  );
  const rekordboxPlaylistSources = useMemo(
    () => playlistSources.filter((source) => source.kind === "rekordbox"),
    [playlistSources]
  );
  const selectedLineInputDevice = useMemo(
    () => microphoneDevices.find((device) => device.id === lineInputDevice) ?? null,
    [lineInputDevice, microphoneDevices]
  );
  const lineInputChannels = Math.max(1, selectedLineInputDevice?.input_channels ?? 1);

  function changeLineInputDevice(deviceId: string) {
    const nextDevice = microphoneDevices.find((device) => device.id === deviceId);
    const channels = Math.max(1, nextDevice?.input_channels ?? 1);
    setLineInputDevice(deviceId);
    if (Number(lineInputChannel) > channels || (lineInputStereo && Number(lineInputChannel) >= channels)) {
      setLineInputChannel("1");
    }
    if (channels < 2) setLineInputStereo(false);
  }

  async function saveProfile(event: FormEvent) {
    event.preventDefault();
    await persistProfile();
  }

  async function persistProfile(): Promise<boolean> {
    setBusy("saving");
    setError(null);
    setNotice(null);
    try {
      const saved = await invoke<BroadcastProfile>("broadcast_save_profile", {
        profile: {
          outputKind,
          host,
          port: Number(port),
          mount,
          username,
          stationName,
          description,
          bitrateKbps: Number(bitrate),
          tls,
          public: isPublic,
          microphoneEnabled,
          microphoneDevice,
          microphoneGainPercent: Number(microphoneGain),
          lineInputEnabled,
          lineInputDevice,
          lineInputChannel: Number(lineInputChannel),
          lineInputStereo,
          lineInputGainPercent: Number(lineInputGain),
          applicationAudioEnabled,
          applicationAudioBundleId,
          applicationAudioGainPercent: Number(applicationAudioGain),
          rtmpPlatform,
          rtmpServerUrl,
          rtmpVideoBitrateKbps: Number(rtmpVideoBitrate),
          rtmpAudioBitrateKbps: Number(rtmpAudioBitrate),
          videoCompositor,
          password: password || null,
          clearPassword
        }
      });
      hydrateProfile(saved);
      const nextPreflight = await invoke<BroadcastPreflight>("broadcast_preflight");
      setPreflight(nextPreflight);
      setNotice(t("Perfil de broadcast guardado."));
      return true;
    } catch (cause) {
      setError(errorMessage(cause, locale));
      return false;
    } finally {
      setBusy(null);
    }
  }

  async function appendPlaylist() {
    if (!selectedPlaylistSource) return;
    setBusy("appending");
    setError(null);
    setNotice(null);
    try {
      const result = selectedPlaylistSource.kind === "local"
        ? await invoke<QueueAppendResult>("broadcast_append_draft", {
            draftId: selectedPlaylistSource.id
          })
        : await invoke<QueueAppendResult>("broadcast_append_playlist", {
            libraryId: selectedPlaylistSource.library_id,
            playlistPath: selectedPlaylistSource.id
          });
      setQueue(result.queue);
      setNotice(t("Se agregaron {count} pistas al broadcast. {skipped} omitidas.", {
        count: result.appended_total,
        skipped: result.skipped_missing_total
      }));
    } catch (cause) {
      setError(errorMessage(cause, locale));
    } finally {
      setBusy(null);
    }
  }

  async function startBroadcast() {
    await runAction("starting", async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_start", {
        streamKey: outputKind === "rtmp" ? streamKey : null
      }));
      setNotice(outputKind === "rtmp"
        ? t("Enviando señal RTMP. Revisa la vista previa antes de salir al aire.")
        : t("Iniciando transmisión a Icecast."));
    });
  }

  async function stopBroadcast() {
    await runAction("stopping", async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_stop"));
      if (outputKind === "rtmp") setStreamKey("");
    });
  }

  async function skipTrack() {
    await runAction("skipping", async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_skip"));
    });
  }

  async function playQueueEntry(entry: BroadcastQueueEntry) {
    await runAction(`play:${entry.id}`, async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_play_queue_entry", { entryId: entry.id }));
      setNotice(t("Cambiando a {track}...", { track: entryTitle(entry) }));
    });
  }

  async function toggleMicrophone() {
    const live = !(status?.microphone?.live ?? false);
    await runAction("microphone", async () => {
      await invoke<BroadcastStatus>("broadcast_set_microphone_live", { live });
      setStatus((current) => current ? {
        ...current,
        microphone: {
          ...(current.microphone ?? {
            configured: true,
            ready: true,
            receiving_audio: false,
            level_percent: 0,
            device: profile?.microphone_device ?? "default",
            gain_percent: profile?.microphone_gain_percent ?? 100,
            message: ""
          }),
          live,
          message: live ? t("Micrófono al aire.") : t("Micrófono silenciado.")
        }
      } : current);
    });
  }

  async function toggleLineInput() {
    const live = !(status?.line_input?.live ?? false);
    await runAction("line-input", async () => {
      await invoke<BroadcastStatus>("broadcast_set_line_input_live", { live });
      setStatus((current) => current ? {
        ...current,
        source_mode: live ? "line_input" : "playlist",
        now_playing: live ? null : current.now_playing,
        message: live ? t("Línea directa al aire.") : t("Radio en vivo · fuente Playlist."),
        line_input: {
          ...(current.line_input ?? {
            configured: true,
            ready: true,
            receiving_audio: false,
            level_percent: 0,
            device: profile?.line_input_device ?? "default",
            channel: profile?.line_input_channel ?? 1,
            stereo: profile?.line_input_stereo ?? true,
            gain_percent: profile?.line_input_gain_percent ?? 100,
            message: ""
          }),
          live,
          message: live ? t("Línea directa al aire.") : t("Fuente Playlist al aire.")
        }
      } : current);
    });
  }

  async function toggleApplicationAudio() {
    const live = !(status?.application_audio?.live ?? false);
    await runAction("application-audio", async () => {
      await invoke<BroadcastStatus>("broadcast_set_application_audio_live", { live });
      const label = applicationAudioBundleId === SYSTEM_AUDIO_TARGET_ID
        ? t("Salida completa del Mac")
        : applicationAudioDevices.find((application) => application.id === applicationAudioBundleId)?.label;
      const liveMessage = applicationAudioBundleId === SYSTEM_AUDIO_TARGET_ID
        ? t("Salida completa del Mac al aire.")
        : t("Audio de {application} al aire.", { application: label ?? t("aplicación") });
      setStatus((current) => current ? {
        ...current,
        source_mode: live ? "application_audio" : "playlist",
        now_playing: live ? null : current.now_playing,
        message: live
          ? liveMessage
          : t("Radio en vivo · fuente Playlist."),
        application_audio: {
          ...(current.application_audio ?? {
            configured: true,
            ready: true,
            receiving_audio: false,
            level_percent: 0,
            application: profile?.application_audio_bundle_id ?? "",
            label: label ?? null,
            gain_percent: profile?.application_audio_gain_percent ?? 100,
            message: ""
          }),
          live,
          message: live
            ? liveMessage
            : t("Fuente Playlist al aire.")
        }
      } : current);
    });
  }

  async function sendCameraMix(mixPercent: number, transitionMillis: number) {
    const normalized = Math.max(0, Math.min(100, Math.round(mixPercent)));
    setCameraMix(normalized);
    await runAction("camera-mix", async () => {
      const nextStatus = await invoke<BroadcastStatus>("broadcast_set_camera_mix", {
        mixPercent: normalized,
        transitionMillis
      });
      setStatus(nextStatus);
    });
  }

  async function changeVideoCompositor(next: BroadcastVideoCompositor) {
    const previous = videoCompositor;
    setVideoCompositor(next);
    if (!running) return;
    try {
      const nextStatus = await invoke<BroadcastStatus>("broadcast_update_camera_settings", {
        config: next
      });
      setStatus(nextStatus);
      setProfile((current) => current ? { ...current, video_compositor: next } : current);
    } catch (cause) {
      setVideoCompositor(previous);
      setError(errorMessage(cause, locale));
    }
  }

  async function refreshMicrophones() {
    await runAction("microphones", async () => {
      const devices = await invoke<BroadcastMicrophoneDevice[]>("broadcast_microphone_devices");
      setMicrophoneDevices(devices);
      if (!devices.some((device) => device.id === microphoneDevice)) {
        setMicrophoneDevice("default");
      }
      if (!devices.some((device) => device.id === lineInputDevice)) {
        setLineInputDevice("default");
      }
    });
  }

  async function refreshApplications() {
    await runAction("applications", async () => {
      const applications = await invoke<BroadcastApplicationAudioDevice[]>("broadcast_application_audio_devices");
      setApplicationAudioDevices(applications);
      if (!applicationAudioBundleId) {
        setApplicationAudioBundleId(SYSTEM_AUDIO_TARGET_ID);
      }
    });
  }

  async function openApplicationAudioSettings() {
    await runAction("application-settings", async () => {
      await invoke("broadcast_open_application_audio_settings");
      setNotice(t("Activa Rau Studio, cierra completamente la app y vuelve a abrirla."));
    });
  }

  async function clearQueue() {
    await runAction("clearing", async () => {
      const deleted = await invoke<number>("broadcast_clear_queue");
      await refreshRuntime();
      setNotice(t("Se quitaron {count} entradas de la cola.", { count: deleted }));
    });
  }

  async function removeEntry(entryId: string) {
    await runAction(`remove:${entryId}`, async () => {
      await invoke("broadcast_remove_queue_entry", { entryId });
      setQueue(await invoke<BroadcastQueueEntry[]>("broadcast_queue"));
    });
  }

  async function persistQueuedOrder(entryIds: string[]) {
    await runAction("reordering", async () => {
      setQueue(await invoke<BroadcastQueueEntry[]>("broadcast_reorder_queue", { entryIds }));
    });
  }

  async function moveQueuedEntry(entryId: string, direction: -1 | 1) {
    const entryIds = queue.filter((entry) => entry.status === "queued").map((entry) => entry.id);
    const currentIndex = entryIds.indexOf(entryId);
    const nextIndex = currentIndex + direction;
    if (currentIndex < 0 || nextIndex < 0 || nextIndex >= entryIds.length) return;
    [entryIds[currentIndex], entryIds[nextIndex]] = [entryIds[nextIndex], entryIds[currentIndex]];
    await persistQueuedOrder(entryIds);
  }

  async function moveQueuedEntryToTarget(entryId: string, targetId: string) {
    if (entryId === targetId) return;
    const entryIds = queue.filter((entry) => entry.status === "queued").map((entry) => entry.id);
    const currentIndex = entryIds.indexOf(entryId);
    const targetIndex = entryIds.indexOf(targetId);
    if (currentIndex < 0 || targetIndex < 0) return;
    entryIds.splice(currentIndex, 1);
    const adjustedTargetIndex = entryIds.indexOf(targetId);
    entryIds.splice(currentIndex < targetIndex ? adjustedTargetIndex + 1 : adjustedTargetIndex, 0, entryId);
    await persistQueuedOrder(entryIds);
  }

  async function sortQueuedEntries(sort: "title" | "artist" | "duration") {
    const collator = new Intl.Collator(locale, { numeric: true, sensitivity: "base" });
    const sorted = queue.filter((entry) => entry.status === "queued").sort((left, right) => {
      if (sort === "duration") {
        return (left.duration_seconds ?? Number.MAX_SAFE_INTEGER) - (right.duration_seconds ?? Number.MAX_SAFE_INTEGER)
          || collator.compare(entryTitle(left), entryTitle(right));
      }
      const leftValue = sort === "artist" ? left.artist ?? left.title : left.title;
      const rightValue = sort === "artist" ? right.artist ?? right.title : right.title;
      return collator.compare(leftValue, rightValue) || collator.compare(entryTitle(left), entryTitle(right));
    });
    await persistQueuedOrder(sorted.map((entry) => entry.id));
  }

  async function runAction(action: BusyAction, callback: () => Promise<void>) {
    setBusy(action);
    setError(null);
    setNotice(null);
    try {
      await callback();
    } catch (cause) {
      setError(errorMessage(cause, locale));
    } finally {
      setBusy(null);
    }
  }

  function clearTerminal() {
    setTerminalLogs([]);
  }

  if (busy === "loading" && !profile) {
    return (
      <main className="grid min-h-screen place-items-center p-6">
        <LoaderCircle className="h-7 w-7 animate-spin text-muted-foreground" aria-label={t("Cargando")} />
      </main>
    );
  }

  return (
    <main
      className={cn(
        "overflow-y-auto bg-background p-4 text-foreground lg:p-6",
        terminalExpanded ? "h-[calc(100vh-17.25rem)]" : "h-[calc(100vh-4.75rem)]"
      )}
    >
      <div className="mx-auto grid w-full max-w-[1480px] gap-4">
        <header className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-muted-foreground">
              <Radio className="h-4 w-4" />
              <span className="text-xs font-semibold uppercase tracking-[0.18em]">{t("Broadcast")}</span>
            </div>
            <h1 className="mt-1 text-2xl font-semibold tracking-tight">{t("Broadcast desde casa")}</h1>
            <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
              {t("Rau Studio mezcla tu cola y entradas locales para transmitir por Icecast o RTMP.")}
            </p>
          </div>
          <StatusBadge status={status?.status ?? "idle"} label={status?.message ?? t("Radio detenida.")} />
        </header>

        {error ? (
          <div role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        ) : null}
        {notice ? (
          <div className="rounded-md border border-emerald-500/25 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-800 dark:text-emerald-200">
            {notice}
          </div>
        ) : null}

        <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <Metric label={t("Estado")} value={statusLabel(status?.status ?? "idle", t)} icon={<Wifi className="h-4 w-4" />} />
          <Metric label={t("En cola")} value={String(queuedTotal)} icon={<Library className="h-4 w-4" />} />
          <Metric label={t("Reproducidas")} value={String(completedTotal)} icon={<Play className="h-4 w-4" />} />
          <Metric label={t("Fallidas")} value={String(failedTotal)} icon={<RefreshCcw className="h-4 w-4" />} danger={failedTotal > 0} />
        </section>

        <section className="grid gap-4 xl:grid-cols-[minmax(360px,0.8fr)_minmax(520px,1.2fr)]">
          <Card>
            <CardHeader>
              <CardTitle>{t("Destino de salida")}</CardTitle>
              <span className={cn(
                "rounded-full px-2 py-1 text-[11px] font-semibold",
                preflight?.ready
                  ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200"
                  : "bg-amber-500/10 text-amber-800 dark:text-amber-200"
              )}>
                {preflight?.ready ? t("FFmpeg listo") : t("Revisar FFmpeg")}
              </span>
            </CardHeader>
            <CardContent className="p-3">
              <form className="grid gap-3" onSubmit={saveProfile}>
                <Field label={t("Tipo de destino")}>
                  <select className={fieldClass} value={outputKind} disabled={running} onChange={(event) => setOutputKind(event.target.value as BroadcastOutputKind)}>
                    <option value="icecast">Icecast · MP3</option>
                    <option value="rtmp">RTMP / RTMPS · {t("Video en vivo")}</option>
                  </select>
                </Field>
                {outputKind === "icecast" ? (
                  <>
                    <div className="grid gap-3 sm:grid-cols-[1fr_110px]">
                      <Field label={t("Host")}>
                        <input className={fieldClass} value={host} required disabled={running} onChange={(event) => setHost(event.target.value)} />
                      </Field>
                      <Field label={t("Puerto")}>
                        <input className={fieldClass} type="number" min={1} max={65535} value={port} required disabled={running} onChange={(event) => setPort(event.target.value)} />
                      </Field>
                    </div>
                    <Field label={t("Mountpoint MP3")}>
                      <input className={fieldClass} value={mount} required disabled={running} placeholder="/live.mp3" onChange={(event) => setMount(event.target.value)} />
                    </Field>
                    <div className="grid gap-3 sm:grid-cols-2">
                      <Field label={t("Usuario source")}>
                        <input className={fieldClass} value={username} required disabled={running} onChange={(event) => setUsername(event.target.value)} />
                      </Field>
                      <Field label={t("Bitrate MP3")}>
                        <select className={fieldClass} value={bitrate} disabled={running} onChange={(event) => setBitrate(event.target.value)}>
                          {[96, 128, 160, 192, 256, 320].map((value) => <option key={value} value={value}>{value} kbps</option>)}
                        </select>
                      </Field>
                    </div>
                    <Field label={profile?.password_configured ? t("Nueva contraseña source (opcional)") : t("Contraseña source")}>
                      <input
                        className={fieldClass}
                        type="password"
                        value={password}
                        required={!profile?.password_configured && !clearPassword}
                        disabled={running || clearPassword}
                        autoComplete="new-password"
                        onChange={(event) => setPassword(event.target.value)}
                      />
                    </Field>
                    <div className="grid gap-2 text-sm sm:grid-cols-2">
                      <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2">
                        <input type="checkbox" checked={tls} disabled={running} onChange={(event) => setTls(event.target.checked)} />
                        {t("Usar TLS")}
                      </label>
                      <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2">
                        <input type="checkbox" checked={isPublic} disabled={running} onChange={(event) => setIsPublic(event.target.checked)} />
                        {t("Listar públicamente")}
                      </label>
                      {profile?.password_configured ? (
                        <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2 sm:col-span-2">
                          <input type="checkbox" checked={clearPassword} disabled={running} onChange={(event) => setClearPassword(event.target.checked)} />
                          {t("Eliminar contraseña guardada")}
                        </label>
                      ) : null}
                    </div>
                  </>
                ) : (
                  <>
                    <Field label={t("Plataforma")}>
                      <select className={fieldClass} value={rtmpPlatform} disabled={running} onChange={(event) => setRtmpPlatform(event.target.value as RtmpPlatform)}>
                        <option value="instagram">Instagram Live</option>
                        <option value="custom">{t("RTMP personalizado")}</option>
                      </select>
                    </Field>
                    <Field label={t("URL del servidor RTMP")}>
                      <input
                        className={fieldClass}
                        type="url"
                        value={rtmpServerUrl}
                        required
                        disabled={running}
                        placeholder="rtmps://live-upload.instagram.com:443/rtmp/"
                        onChange={(event) => setRtmpServerUrl(event.target.value)}
                      />
                    </Field>
                    <Field label={t("Clave de transmisión · solo esta sesión")}>
                      <input
                        className={fieldClass}
                        type="password"
                        value={streamKey}
                        disabled={running}
                        autoComplete="off"
                        placeholder={t("Pégala antes de enviar la señal")}
                        onChange={(event) => setStreamKey(event.target.value)}
                      />
                    </Field>
                    <div className="grid gap-3 sm:grid-cols-2">
                      <Field label={t("Bitrate de video")}>
                        <select className={fieldClass} value={rtmpVideoBitrate} disabled={running} onChange={(event) => setRtmpVideoBitrate(event.target.value)}>
                          {[2250, 3000, 3500, 4500, 6000].map((value) => <option key={value} value={value}>{value} kbps</option>)}
                        </select>
                      </Field>
                      <Field label={t("Bitrate AAC")}>
                        <select className={fieldClass} value={rtmpAudioBitrate} disabled={running} onChange={(event) => setRtmpAudioBitrate(event.target.value)}>
                          {[96, 128, 160, 192, 256].map((value) => <option key={value} value={value}>{value} kbps</option>)}
                        </select>
                      </Field>
                    </div>
                    <div className="rounded-md border border-violet-500/25 bg-violet-500/5 px-3 py-2 text-xs text-muted-foreground">
                      <div className="flex items-start justify-between gap-3">
                        <div>
                          <strong className="block text-foreground">720 × 1280 · 30 fps · H.264/AAC</strong>
                          <span>{t("Rau genera una señal visual monocroma con identidad de la radio y la pista actual, actualizada sin cortar el Live.")}</span>
                        </div>
                        <Button type="button" size="sm" variant="secondary" onClick={() => setVideoStudioOpen(true)}>
                          <SlidersHorizontal className="h-4 w-4" />
                          {t("Video Studio")}
                        </Button>
                      </div>
                    </div>
                    {rtmpPlatform === "instagram" ? (
                      <div className="rounded-md border border-amber-500/25 bg-amber-500/5 px-3 py-2 text-xs text-amber-900 dark:text-amber-100">
                        {t("Crea el Live en Instagram.com, copia su URL y clave, envía la señal desde Rau y confirma la vista previa en Live Producer. Para terminar, finaliza primero en Instagram.")}
                      </div>
                    ) : null}
                  </>
                )}
                <Field label={t("Nombre de estación")}>
                  <input className={fieldClass} value={stationName} required maxLength={120} disabled={running} onChange={(event) => setStationName(event.target.value)} />
                </Field>
                <Field label={t("Descripción")}>
                  <input className={fieldClass} value={description} maxLength={240} disabled={running} onChange={(event) => setDescription(event.target.value)} />
                </Field>
                <div className="overflow-hidden rounded-lg border border-border bg-muted/15">
                  <div className="grid grid-cols-3 gap-1 border-b border-border bg-secondary/70 p-1" role="tablist" aria-label={t("Fuentes de audio")}>
                    <SourceTabButton
                      id="broadcast-source-microphone-tab"
                      controls="broadcast-source-microphone-panel"
                      active={sourceTab === "microphone"}
                      enabled={microphoneEnabled}
                      icon={<Mic className="h-3.5 w-3.5" />}
                      label={t("Micrófono")}
                      onClick={() => setSourceTab("microphone")}
                    />
                    <SourceTabButton
                      id="broadcast-source-line-tab"
                      controls="broadcast-source-line-panel"
                      active={sourceTab === "line_input"}
                      enabled={lineInputEnabled}
                      icon={<Radio className="h-3.5 w-3.5" />}
                      label={t("Línea")}
                      onClick={() => setSourceTab("line_input")}
                    />
                    <SourceTabButton
                      id="broadcast-source-system-tab"
                      controls="broadcast-source-system-panel"
                      active={sourceTab === "system_audio"}
                      enabled={applicationAudioEnabled}
                      icon={<AudioLines className="h-3.5 w-3.5" />}
                      label={t("Sistema")}
                      onClick={() => setSourceTab("system_audio")}
                    />
                  </div>
                  <div className="min-h-[250px]">
                <div
                  id="broadcast-source-microphone-panel"
                  role="tabpanel"
                  aria-labelledby="broadcast-source-microphone-tab"
                  hidden={sourceTab !== "microphone"}
                  className={cn("grid gap-3 p-3", sourceTab !== "microphone" && "hidden")}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <Mic className="h-4 w-4" />
                      <strong className="text-sm">{t("Entrada de micrófono")}</strong>
                    </div>
                    <Button type="button" size="sm" variant="ghost" disabled={running || busy === "microphones"} onClick={() => void refreshMicrophones()}>
                      <RefreshCcw className={cn("h-4 w-4", busy === "microphones" && "animate-spin")} />
                      {t("Refrescar")}
                    </Button>
                  </div>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={microphoneEnabled}
                      disabled={running || !preflight?.microphone_input_available}
                      onChange={(event) => setMicrophoneEnabled(event.target.checked)}
                    />
                    {t("Preparar micrófono al iniciar")}
                  </label>
                  {microphoneEnabled ? (
                    <>
                      <Field label={t("Dispositivo de entrada")}>
                        <select className={fieldClass} value={microphoneDevice} disabled={running} onChange={(event) => setMicrophoneDevice(event.target.value)}>
                          {microphoneDevices.map((device) => <option key={device.id} value={device.id}>{device.is_default ? t(device.label) : device.label}</option>)}
                        </select>
                      </Field>
                      <Field label={t("Ganancia del micrófono: {gain}%", { gain: microphoneGain })}>
                        <input
                          className="w-full accent-foreground"
                          type="range"
                          min={0}
                          max={200}
                          step={5}
                          value={microphoneGain}
                          disabled={running}
                          onChange={(event) => setMicrophoneGain(event.target.value)}
                        />
                      </Field>
                      <p className="text-xs text-muted-foreground">
                        {t("Se prepara silenciado. Actívalo desde Control de transmisión cuando quieras hablar.")}
                        {" "}{t("Cuando detecta tu voz, la música baja automáticamente y vuelve a subir al terminar.")}
                      </p>
                    </>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      {preflight?.microphone_input_available
                        ? t("Activa esta opción para seleccionar un micrófono.")
                        : t("No hay un dispositivo de entrada de audio disponible.")}
                    </p>
                  )}
                </div>
                <div
                  id="broadcast-source-line-panel"
                  role="tabpanel"
                  aria-labelledby="broadcast-source-line-tab"
                  hidden={sourceTab !== "line_input"}
                  className={cn("grid gap-3 p-3", sourceTab !== "line_input" && "hidden")}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <Radio className="h-4 w-4" />
                      <strong className="text-sm">{t("Entrada de línea directa")}</strong>
                    </div>
                    <Button type="button" size="sm" variant="ghost" disabled={running || busy === "microphones"} onClick={() => void refreshMicrophones()}>
                      <RefreshCcw className={cn("h-4 w-4", busy === "microphones" && "animate-spin")} />
                      {t("Refrescar")}
                    </Button>
                  </div>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={lineInputEnabled}
                      disabled={running || !preflight?.microphone_input_available}
                      onChange={(event) => setLineInputEnabled(event.target.checked)}
                    />
                    {t("Preparar línea directa al iniciar")}
                  </label>
                  {lineInputEnabled ? (
                    <>
                      <Field label={t("Dispositivo de línea")}>
                        <select className={fieldClass} value={lineInputDevice} disabled={running} onChange={(event) => changeLineInputDevice(event.target.value)}>
                          {microphoneDevices.map((device) => <option key={device.id} value={device.id}>{device.is_default ? t(device.label) : device.label} · {device.input_channels} ch</option>)}
                        </select>
                      </Field>
                      <Field label={t("Canal de entrada")}>
                        <select
                          className={fieldClass}
                          value={`${lineInputStereo ? "stereo" : "mono"}:${lineInputChannel}`}
                          disabled={running}
                          onChange={(event) => {
                            const [mode, channel] = event.target.value.split(":");
                            setLineInputStereo(mode === "stereo");
                            setLineInputChannel(channel);
                          }}
                        >
                          <optgroup label={t("Mono")}>
                            {Array.from({ length: lineInputChannels }, (_, index) => index + 1).map((channel) => (
                              <option key={`mono:${channel}`} value={`mono:${channel}`}>{t("Canal {channel} mono", { channel })}</option>
                            ))}
                          </optgroup>
                          {lineInputChannels > 1 ? (
                            <optgroup label={t("Estéreo")}>
                              {Array.from({ length: lineInputChannels - 1 }, (_, index) => index + 1).map((channel) => (
                                <option key={`stereo:${channel}`} value={`stereo:${channel}`}>{t("Canales {left}–{right} estéreo", { left: channel, right: channel + 1 })}</option>
                              ))}
                            </optgroup>
                          ) : null}
                        </select>
                      </Field>
                      <Field label={t("Ganancia de línea: {gain}%", { gain: lineInputGain })}>
                        <input
                          className="w-full accent-foreground"
                          type="range"
                          min={0}
                          max={200}
                          step={5}
                          value={lineInputGain}
                          disabled={running}
                          onChange={(event) => setLineInputGain(event.target.value)}
                        />
                      </Field>
                      <p className="text-xs text-muted-foreground">
                        {t("La línea reemplaza temporalmente la playlist y pasa directo al destino, sin ducking.")}
                      </p>
                    </>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      {preflight?.microphone_input_available
                        ? t("Activa esta opción para preparar una interfaz o entrada de línea.")
                        : t("No hay un dispositivo de entrada de audio disponible.")}
                    </p>
                  )}
                </div>
                <div
                  id="broadcast-source-system-panel"
                  role="tabpanel"
                  aria-labelledby="broadcast-source-system-tab"
                  hidden={sourceTab !== "system_audio"}
                  className={cn("grid gap-3 p-3", sourceTab !== "system_audio" && "hidden")}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <AudioLines className="h-4 w-4" />
                      <strong className="text-sm">{t("Salida del Mac")}</strong>
                    </div>
                    <div className="flex flex-wrap justify-end gap-1">
                      {applicationAudioDevices.length === 0 ? (
                        <Button type="button" size="sm" variant="ghost" disabled={running || busy === "application-settings"} onClick={() => void openApplicationAudioSettings()}>
                          {t("Abrir ajustes")}
                        </Button>
                      ) : null}
                      <Button type="button" size="sm" variant="ghost" disabled={running || busy === "applications"} onClick={() => void refreshApplications()}>
                        <RefreshCcw className={cn("h-4 w-4", busy === "applications" && "animate-spin")} />
                        {applicationAudioDevices.length === 0 ? t("Solicitar acceso") : t("Refrescar")}
                      </Button>
                    </div>
                  </div>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={applicationAudioEnabled}
                      disabled={running}
                      onChange={(event) => {
                        const enabled = event.target.checked;
                        setApplicationAudioEnabled(enabled);
                        if (enabled && !applicationAudioBundleId) {
                          setApplicationAudioBundleId(SYSTEM_AUDIO_TARGET_ID);
                        }
                      }}
                    />
                    {t("Preparar salida del Mac al iniciar")}
                  </label>
                  {applicationAudioEnabled ? (
                    <>
                      <Field label={t("Fuente de audio") }>
                        <select className={fieldClass} value={applicationAudioBundleId} disabled={running} onChange={(event) => setApplicationAudioBundleId(event.target.value)}>
                          <option value={SYSTEM_AUDIO_TARGET_ID}>{t("Toda la salida del Mac")}</option>
                          {applicationAudioBundleId && applicationAudioBundleId !== SYSTEM_AUDIO_TARGET_ID && !applicationAudioDevices.some((application) => application.id === applicationAudioBundleId) ? (
                            <option value={applicationAudioBundleId}>{applicationAudioBundleId} · {t("no está abierta")}</option>
                          ) : null}
                          {applicationAudioDevices.length > 0 ? (
                            <optgroup label={t("Aplicación específica (opcional)")}>
                              {applicationAudioDevices.map((application) => (
                                <option key={`${application.id}:${application.process_id}`} value={application.id}>{application.label}</option>
                              ))}
                            </optgroup>
                          ) : null}
                        </select>
                      </Field>
                      <Field label={t("Ganancia de salida: {gain}%", { gain: applicationAudioGain })}>
                        <input
                          className="w-full accent-foreground"
                          type="range"
                          min={0}
                          max={200}
                          step={5}
                          value={applicationAudioGain}
                          disabled={running}
                          onChange={(event) => setApplicationAudioGain(event.target.value)}
                        />
                      </Field>
                      <p className="text-xs text-muted-foreground">
                        {t("Reemplaza temporalmente la playlist por todo lo que suena en el Mac, sin micrófono ni ducking. Rau Studio excluye su propio audio para evitar realimentación.")}
                        {" "}{t("macOS pedirá permiso de Grabación de pantalla y audio del sistema.")}
                      </p>
                    </>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      {t("Activa esta opción para enviar toda la salida normal del computador al broadcast. También puedes limitarla a una aplicación.")}
                    </p>
                  )}
                </div>
                  </div>
                </div>
                <div className="rounded-md bg-secondary/60 px-3 py-2 text-xs text-muted-foreground">
                  <strong className="block break-all text-foreground">
                    {outputKind === "rtmp" ? (rtmpServerUrl || t("Configura la URL RTMP")) : (profile?.listener_url ?? "—")}
                  </strong>
                  <span>{translateBackendMessage(locale, preflight?.message ?? t("Revisando motor FFmpeg..."))}</span>
                  {destinationNeedsSave ? (
                    <span className="mt-1 block font-medium text-amber-700 dark:text-amber-300">{t("Guarda los cambios del destino antes de iniciar.")}</span>
                  ) : null}
                </div>
                <Button type="submit" disabled={busy === "saving" || running}>
                  {busy === "saving" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
                  {t("Guardar perfil")}
                </Button>
              </form>
            </CardContent>
          </Card>

          <div className="grid min-h-0 gap-4">
            <Card>
              <CardHeader>
                <CardTitle>{t("Control de transmisión")}</CardTitle>
                <div className="flex flex-wrap gap-2">
                  {!running ? (
                    <Button size="sm" disabled={destinationNeedsSave || !preflight?.ready || (outputKind === "rtmp" && !streamKey.trim()) || ((microphoneEnabled || lineInputEnabled) && !preflight?.microphone_input_available) || busy !== null} onClick={() => void startBroadcast()}>
                      {busy === "starting" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                      {t("Salir al aire")}
                    </Button>
                  ) : (
                    <Button size="sm" variant="destructive" disabled={busy === "stopping" || status?.status === "stopping"} onClick={() => void stopBroadcast()}>
                      {busy === "stopping" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Square className="h-4 w-4" />}
                      {t("Detener")}
                    </Button>
                  )}
                  <Button size="sm" variant="secondary" disabled={!status?.now_playing || busy === "skipping"} onClick={() => void skipTrack()}>
                    <SkipForward className="h-4 w-4" />
                    {t("Saltar")}
                  </Button>
                  {outputKind === "rtmp" ? (
                    <Button
                      size="sm"
                      variant={status?.camera?.live ? "default" : "secondary"}
                      onClick={() => setVideoStudioOpen(true)}
                    >
                      {videoCompositor.screenEnabled
                        ? <Monitor className={cn("h-4 w-4", status?.camera?.live && "animate-pulse")} />
                        : <Camera className={cn("h-4 w-4", status?.camera?.live && "animate-pulse")} />}
                      {status?.camera?.live
                        ? t("Fuentes en Program")
                        : t("Video Studio")}
                    </Button>
                  ) : null}
                  {running && profile?.microphone_enabled ? (
                    <Button
                      size="sm"
                      variant={status?.microphone?.live ? "destructive" : "secondary"}
                      disabled={!status?.microphone?.ready || ["line_input", "application_audio"].includes(status?.source_mode ?? "") || busy === "microphone"}
                      onClick={() => void toggleMicrophone()}
                    >
                      {status?.microphone?.live ? <MicOff className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
                      {status?.microphone?.live ? t("Silenciar micrófono") : t("Micrófono al aire")}
                    </Button>
                  ) : null}
                  {running && profile?.line_input_enabled ? (
                    <Button
                      size="sm"
                      variant={status?.source_mode === "line_input" ? "default" : "secondary"}
                      disabled={!status?.line_input?.ready || status?.source_mode === "application_audio" || busy === "line-input"}
                      onClick={() => void toggleLineInput()}
                    >
                      <Radio className={cn("h-4 w-4", status?.source_mode === "line_input" && "animate-pulse")} />
                      {status?.source_mode === "line_input" ? t("Volver a Playlist") : t("Línea directa al aire")}
                    </Button>
                  ) : null}
                  {running && profile?.application_audio_enabled ? (
                    <Button
                      size="sm"
                      variant={status?.source_mode === "application_audio" ? "default" : "secondary"}
                      disabled={!status?.application_audio?.ready || status?.source_mode === "line_input" || busy === "application-audio"}
                      onClick={() => void toggleApplicationAudio()}
                    >
                      <AudioLines className={cn("h-4 w-4", status?.source_mode === "application_audio" && "animate-pulse")} />
                      {status?.source_mode === "application_audio" ? t("Volver a Playlist") : t("Salida del Mac al aire")}
                    </Button>
                  ) : null}
                </div>
              </CardHeader>
              <CardContent className="p-3">
                {status?.source_mode === "application_audio" ? (
                  <div className="rounded-md border border-violet-500/25 bg-violet-500/5 p-4">
                    <span className="text-xs font-semibold uppercase tracking-[0.15em] text-violet-700 dark:text-violet-300">{t("Fuente principal al aire")}</span>
                    <strong className="mt-2 block text-lg">{status.application_audio.label ?? t("Salida del Mac")}</strong>
                    <span className="mt-1 block text-xs text-muted-foreground">
                      {t("Audio estéreo del sistema")} · {t("Playlist en espera")}
                    </span>
                  </div>
                ) : status?.source_mode === "line_input" ? (
                  <div className="rounded-md border border-cyan-500/25 bg-cyan-500/5 p-4">
                    <span className="text-xs font-semibold uppercase tracking-[0.15em] text-cyan-700 dark:text-cyan-300">{t("Fuente principal al aire")}</span>
                    <strong className="mt-2 block text-lg">{t("Línea directa")}</strong>
                    <span className="mt-1 block text-xs text-muted-foreground">
                      {status.line_input.stereo
                        ? t("Canales {left}–{right} estéreo", { left: status.line_input.channel, right: status.line_input.channel + 1 })
                        : t("Canal {channel} mono", { channel: status.line_input.channel })}
                      {" · "}{t("Playlist en espera")}
                    </span>
                  </div>
                ) : status?.now_playing ? (
                  <div className="rounded-md border border-emerald-500/25 bg-emerald-500/5 p-4">
                    <span className="text-xs font-semibold uppercase tracking-[0.15em] text-emerald-700 dark:text-emerald-300">{t("Ahora al aire")}</span>
                    <strong className="mt-2 block text-lg">{entryTitle(status.now_playing)}</strong>
                    <span className="mt-1 block text-xs text-muted-foreground">{status.now_playing.playlist_name}</span>
                  </div>
                ) : (
                  <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
                    {running ? t("La conexión sigue viva transmitiendo silencio hasta que haya una pista.") : t("Configura un destino, agrega una playlist y sal al aire.")}
                  </div>
                )}
                {profile?.microphone_enabled ? (
                  <div className={cn(
                    "mt-3 flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
                    status?.microphone?.live
                      ? "border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-200"
                      : "border-border text-muted-foreground"
                  )}>
                    {status?.microphone?.live ? <Mic className="h-4 w-4 animate-pulse" /> : <MicOff className="h-4 w-4" />}
                    <span className="min-w-0 flex-1 truncate">
                      {translateBackendMessage(locale, status?.microphone?.message ?? t("Micrófono esperando inicio."))}
                    </span>
                    {status?.microphone?.live ? (
                      <div className="flex shrink-0 items-center gap-2" title={t("Nivel de entrada")}>
                        <span className="w-14 text-right tabular-nums">
                          {status.microphone.receiving_audio ? `${status.microphone.level_percent}%` : t("Sin señal")}
                        </span>
                        <div className="h-2 w-20 overflow-hidden rounded-full bg-background/70 ring-1 ring-current/15">
                          <div
                            className={cn(
                              "h-full rounded-full transition-[width] duration-150",
                              status.microphone.level_percent > 80 ? "bg-red-500" : "bg-emerald-500"
                            )}
                            style={{ width: `${status.microphone.level_percent}%` }}
                          />
                        </div>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                {profile?.line_input_enabled ? (
                  <div className={cn(
                    "mt-3 flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
                    status?.line_input?.live
                      ? "border-cyan-500/30 bg-cyan-500/10 text-cyan-800 dark:text-cyan-200"
                      : "border-border text-muted-foreground"
                  )}>
                    <Radio className={cn("h-4 w-4", status?.line_input?.live && "animate-pulse")} />
                    <span className="min-w-0 flex-1 truncate">
                      {translateBackendMessage(locale, status?.line_input?.message ?? t("Línea directa esperando inicio."))}
                    </span>
                    {status?.line_input?.live ? (
                      <div className="flex shrink-0 items-center gap-2" title={t("Nivel de entrada")}>
                        <span className="w-14 text-right tabular-nums">
                          {status.line_input.receiving_audio ? `${status.line_input.level_percent}%` : t("Sin señal")}
                        </span>
                        <div className="h-2 w-20 overflow-hidden rounded-full bg-background/70 ring-1 ring-current/15">
                          <div
                            className={cn(
                              "h-full rounded-full transition-[width] duration-150",
                              status.line_input.level_percent > 80 ? "bg-red-500" : "bg-cyan-500"
                            )}
                            style={{ width: `${status.line_input.level_percent}%` }}
                          />
                        </div>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                {profile?.application_audio_enabled && applicationAudioNeedsAttention ? (
                  <Popover>
                    <PopoverTrigger asChild>
                      <button
                        type="button"
                        className="mt-3 flex w-full min-w-0 items-center gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-left text-xs text-amber-800 transition-colors hover:bg-amber-500/15 dark:text-amber-200"
                      >
                        <AlertTriangle className="h-4 w-4 shrink-0" />
                        <span className="min-w-0 flex-1 truncate font-medium">
                          {t(applicationAudioPermissionMissing ? "Audio del Mac sin acceso." : "Audio del Mac requiere atención.")}
                        </span>
                        <span className="shrink-0 text-[11px] font-semibold">{t("Ver detalle")}</span>
                      </button>
                    </PopoverTrigger>
                    <PopoverContent align="end" side="top" className="w-80 max-w-[calc(100vw-2rem)] p-3">
                      <div className="flex items-start gap-2.5">
                        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
                        <div className="min-w-0">
                          <h4 className="text-sm font-semibold">{t("Audio del Mac requiere atención")}</h4>
                          <p className="mt-1 break-words text-xs leading-relaxed text-muted-foreground">
                            {applicationAudioDetail}
                          </p>
                        </div>
                      </div>
                      <div className="mt-3 flex justify-end border-t border-border pt-3">
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={busy === "application-settings"}
                          onClick={() => void openApplicationAudioSettings()}
                        >
                          {t("Abrir ajustes")}
                        </Button>
                      </div>
                    </PopoverContent>
                  </Popover>
                ) : profile?.application_audio_enabled ? (
                  <div className={cn(
                    "mt-3 flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
                    status?.application_audio?.live
                      ? "border-violet-500/30 bg-violet-500/10 text-violet-800 dark:text-violet-200"
                      : "border-border text-muted-foreground"
                  )}>
                    <AudioLines className={cn("h-4 w-4", status?.application_audio?.live && "animate-pulse")} />
                    <span className="min-w-0 flex-1 truncate">
                      {applicationAudioDetail}
                    </span>
                    {status?.application_audio?.live ? (
                      <div className="flex shrink-0 items-center gap-2" title={t("Nivel de entrada")}>
                        <span className="w-14 text-right tabular-nums">
                          {status.application_audio.receiving_audio ? `${status.application_audio.level_percent}%` : t("Sin señal")}
                        </span>
                        <div className="h-2 w-20 overflow-hidden rounded-full bg-background/70 ring-1 ring-current/15">
                          <div
                            className={cn(
                              "h-full rounded-full transition-[width] duration-150",
                              status.application_audio.level_percent > 80 ? "bg-red-500" : "bg-violet-500"
                            )}
                            style={{ width: `${status.application_audio.level_percent}%` }}
                          />
                        </div>
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </CardContent>
            </Card>

            <Card className="flex h-[calc(100vh-3rem)] min-h-[420px] max-h-[860px] flex-col overflow-hidden">
              <CardHeader className="flex-wrap py-2">
                <CardTitle>{t("Cola de broadcast")}</CardTitle>
                <div className="ml-auto flex items-center gap-2">
                  <select
                    aria-label={t("Ordenar pistas")}
                    className="h-8 max-w-36 rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none disabled:opacity-50"
                    value=""
                    disabled={queuedTotal === 0 || busy === "reordering"}
                    onChange={(event) => void sortQueuedEntries(event.currentTarget.value as "title" | "artist" | "duration")}
                  >
                    <option value="" disabled>{t("Ordenar próximas...")}</option>
                    <option value="title">{t("Título A–Z")}</option>
                    <option value="artist">{t("Artista A–Z")}</option>
                    <option value="duration">{t("Duración menor primero")}</option>
                  </select>
                  <Button size="sm" variant="ghost" disabled={queue.every((entry) => entry.status === "playing") || busy === "clearing"} onClick={() => void clearQueue()}>
                    <Trash2 className="h-4 w-4" />
                    {t("Limpiar")}
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="flex min-h-0 flex-1 flex-col overflow-hidden">
                <div className="grid shrink-0 gap-2 border-b border-border p-3 md:grid-cols-[minmax(260px,1fr)_auto]">
                  <Popover open={playlistComboboxOpen} onOpenChange={setPlaylistComboboxOpen}>
                    <PopoverTrigger asChild>
                      <Button
                        variant="secondary"
                        role="combobox"
                        aria-expanded={playlistComboboxOpen}
                        className="h-10 min-w-0 justify-between border border-input bg-background px-3 font-normal hover:bg-accent"
                      >
                        {selectedPlaylistSource ? (
                          <span className="min-w-0 truncate">
                            <span className="font-medium">{selectedPlaylistSource.name}</span>
                            <span className="text-muted-foreground">
                              {" · "}{t(selectedPlaylistSource.kind === "local" ? "Local" : "Rekordbox")}{" · "}{selectedPlaylistSource.track_count} {t("tracks")}
                            </span>
                          </span>
                        ) : (
                          <span className="truncate text-muted-foreground">{t("Buscar una playlist para agregar...")}</span>
                        )}
                        <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 text-muted-foreground" />
                      </Button>
                    </PopoverTrigger>
                    <PopoverContent
                      align="start"
                      className="w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-2rem)]"
                    >
                      <Command>
                        <CommandInput placeholder={t("Buscar por nombre, biblioteca u origen...")} />
                        <CommandList>
                          <CommandEmpty>{t("No se encontraron playlists.")}</CommandEmpty>
                          {[
                            { label: t("Playlists locales"), items: localPlaylistSources },
                            { label: t("Playlists de Rekordbox"), items: rekordboxPlaylistSources }
                          ].map((group) => group.items.length > 0 ? (
                            <CommandGroup key={group.label} heading={group.label}>
                              {group.items.map((source) => (
                                <CommandItem
                                  key={source.key}
                                  value={`${source.key} ${source.name} ${source.library_name} ${source.kind}`}
                                  onSelect={() => {
                                    setPlaylistSourceKey(source.key);
                                    setPlaylistComboboxOpen(false);
                                  }}
                                >
                                  <Check className={cn("mr-2 h-4 w-4 shrink-0", playlistSourceKey === source.key ? "opacity-100" : "opacity-0")} />
                                  <span className="min-w-0 flex-1">
                                    <span className="block truncate font-medium">{source.name}</span>
                                    <span className="block truncate text-xs text-muted-foreground">
                                      {source.library_name} · {source.track_count} {t("tracks")}
                                    </span>
                                  </span>
                                  <span className={cn(
                                    "ml-3 shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide",
                                    source.kind === "local"
                                      ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                                      : "bg-blue-500/10 text-blue-700 dark:text-blue-300"
                                  )}>
                                    {t(source.kind === "local" ? "Local" : "Rekordbox")}
                                  </span>
                                </CommandItem>
                              ))}
                            </CommandGroup>
                          ) : null)}
                        </CommandList>
                      </Command>
                    </PopoverContent>
                  </Popover>
                  <Button disabled={!selectedPlaylistSource || busy === "appending"} onClick={() => void appendPlaylist()}>
                    {busy === "appending" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
                    {t("Agregar")}
                  </Button>
                </div>
                {queue.length === 0 ? (
                  <div className="grid min-h-0 flex-1 place-items-center p-6 text-sm text-muted-foreground">{t("La cola está vacía.")}</div>
                ) : (
                  <div className="min-h-0 flex-1 divide-y divide-border overflow-y-auto overscroll-contain">
                    {queue.map((entry) => {
                      const queuedIndex = queuedEntries.findIndex((queuedEntry) => queuedEntry.id === entry.id);
                      const canSelectTrack = running
                        && status?.status !== "stopping"
                        && status?.source_mode === "playlist"
                        && entry.status !== "playing";
                      return (
                      <div
                        key={entry.id}
                        className={cn(
                          "grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 px-2 py-2.5 transition-colors",
                          entry.status === "playing" && "bg-emerald-500/5",
                          draggedQueueEntryId && entry.status === "queued" && entry.id !== draggedQueueEntryId && "hover:bg-accent/60"
                        )}
                        onDragOver={(event) => {
                          if (entry.status !== "queued" || !draggedQueueEntryId) return;
                          event.preventDefault();
                          event.dataTransfer.dropEffect = "move";
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          const sourceId = draggedQueueEntryId ?? event.dataTransfer.getData("text/plain");
                          setDraggedQueueEntryId(null);
                          if (sourceId && entry.status === "queued") void moveQueuedEntryToTarget(sourceId, entry.id);
                        }}
                      >
                        <button
                          type="button"
                          draggable={entry.status === "queued" && busy === null}
                          className={cn(
                            "grid h-8 w-6 shrink-0 place-items-center rounded text-muted-foreground",
                            entry.status === "queued" ? "cursor-grab hover:bg-accent hover:text-foreground active:cursor-grabbing" : "cursor-not-allowed opacity-25"
                          )}
                          aria-label={t("Arrastrar para reordenar")}
                          title={t("Arrastrar para reordenar")}
                          onDragStart={(event) => {
                            if (entry.status !== "queued") return;
                            setDraggedQueueEntryId(entry.id);
                            event.dataTransfer.effectAllowed = "move";
                            event.dataTransfer.setData("text/plain", entry.id);
                          }}
                          onDragEnd={() => setDraggedQueueEntryId(null)}
                        >
                          <GripVertical className="h-4 w-4" />
                        </button>
                        <div className="min-w-0">
                          <div className="flex min-w-0 items-center gap-2">
                            <span className="truncate text-sm font-medium">{entryTitle(entry)}</span>
                            <QueueStatus status={entry.status} />
                          </div>
                          <span className="mt-0.5 block truncate text-xs text-muted-foreground">{t(entry.playlist_name)} · {formatDuration(entry.duration_seconds)}</span>
                          {entry.error ? <span className="mt-1 block text-xs text-destructive">{entry.error}</span> : null}
                        </div>
                        <div className="flex items-center gap-0.5">
                          <Button
                            size="icon"
                            variant={entry.status === "playing" ? "secondary" : "ghost"}
                            aria-label={entry.status === "playing" ? t("Pista al aire") : t("Reproducir ahora")}
                            title={entry.status === "playing" ? t("Pista al aire") : t("Reproducir ahora")}
                            disabled={!canSelectTrack || busy === `play:${entry.id}`}
                            onClick={() => void playQueueEntry(entry)}
                          >
                            {busy === `play:${entry.id}` ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            aria-label={t("Mover hacia arriba")}
                            title={t("Mover hacia arriba")}
                            disabled={queuedIndex <= 0 || busy === "reordering"}
                            onClick={() => void moveQueuedEntry(entry.id, -1)}
                          >
                            <ArrowUp className="h-3.5 w-3.5" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            aria-label={t("Mover hacia abajo")}
                            title={t("Mover hacia abajo")}
                            disabled={queuedIndex < 0 || queuedIndex >= queuedEntries.length - 1 || busy === "reordering"}
                            onClick={() => void moveQueuedEntry(entry.id, 1)}
                          >
                            <ArrowDown className="h-3.5 w-3.5" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            aria-label={t("Quitar de la cola")}
                            disabled={entry.status === "playing" || busy === `remove:${entry.id}`}
                            onClick={() => void removeEntry(entry.id)}
                          >
                            {busy === `remove:${entry.id}` ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
                          </Button>
                        </div>
                      </div>
                    )})}
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        </section>

      </div>
      <VideoStudioModal
        open={videoStudioOpen}
        config={videoCompositor}
        devices={cameraDevices}
        stationName={stationName}
        trackTitle={status?.now_playing ? entryTitle(status.now_playing) : t("WAITING FOR NEXT TRACK")}
        running={running}
        cameraReady={status?.camera?.ready ?? false}
        mixPercent={cameraMix}
        busy={busy}
        onClose={() => setVideoStudioOpen(false)}
        onChange={(next) => void changeVideoCompositor(next)}
        onMix={sendCameraMix}
        onSave={async () => {
          if (await persistProfile()) setVideoStudioOpen(false);
        }}
      />
      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle={t("ffmpeg / destinos / entradas de audio")}
        emptyMessage="Sin eventos todavía."
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
    </main>
  );
}

function VideoStudioModal({
  open,
  config,
  devices,
  stationName,
  trackTitle,
  running,
  cameraReady,
  mixPercent,
  busy,
  onClose,
  onChange,
  onMix,
  onSave
}: {
  open: boolean;
  config: BroadcastVideoCompositor;
  devices: BroadcastCameraDevice[];
  stationName: string;
  trackTitle: string;
  running: boolean;
  cameraReady: boolean;
  mixPercent: number;
  busy: BusyAction;
  onClose: () => void;
  onChange: (config: BroadcastVideoCompositor) => void;
  onMix: (mixPercent: number, transitionMillis: number) => Promise<void>;
  onSave: () => Promise<void>;
}) {
  const { locale, t } = useI18n();
  const cameraVideo = useRef<HTMLVideoElement | null>(null);
  const screenVideo = useRef<HTMLVideoElement | null>(null);
  const previewCanvas = useRef<HTMLCanvasElement | null>(null);
  const programCanvas = useRef<HTMLCanvasElement | null>(null);
  const cameraStream = useRef<MediaStream | null>(null);
  const screenStream = useRef<MediaStream | null>(null);
  const offscreenCanvas = useRef<HTMLCanvasElement | null>(null);
  const uploadPending = useRef(false);
  const visualSequence = useRef(0);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [draftMix, setDraftMix] = useState(mixPercent);
  const [handoffPending, setHandoffPending] = useState(false);
  const [activeLayer, setActiveLayer] = useState<"camera" | "screen">("camera");
  const [cameraAvailable, setCameraAvailable] = useState(false);
  const [screenAvailable, setScreenAvailable] = useState(false);
  const [browserCameras, setBrowserCameras] = useState<BroadcastCameraDevice[]>([]);
  const cameraDevices = [...devices.filter((device) => device.kind === "camera"), ...browserCameras]
    .filter((device, index, all) => all.findIndex((candidate) => candidate.label === device.label) === index);

  const update = useCallback((patch: Partial<BroadcastVideoCompositor>) => {
    onChange({ ...config, ...patch, captureMode: "browser" });
  }, [config, onChange]);

  const stopCamera = useCallback(() => {
    cameraStream.current?.getTracks().forEach((track) => track.stop());
    cameraStream.current = null;
    if (cameraVideo.current) cameraVideo.current.srcObject = null;
    setCameraAvailable(false);
  }, []);

  const stopScreen = useCallback(() => {
    screenStream.current?.getTracks().forEach((track) => track.stop());
    screenStream.current = null;
    if (screenVideo.current) screenVideo.current.srcObject = null;
    setScreenAvailable(false);
  }, []);

  const startCamera = useCallback(async (preferredLabel = config.cameraDevice) => {
    try {
      setPreviewError(null);
      stopCamera();
      let stream = await navigator.mediaDevices.getUserMedia({
        audio: false,
        video: { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30, max: 30 } }
      });
      const enumerated = await navigator.mediaDevices.enumerateDevices();
      const cameras = enumerated
        .filter((device) => device.kind === "videoinput")
        .map((device, index) => ({
          id: device.label || device.deviceId || `camera-${index + 1}`,
          label: device.label || `${t("Cámara")} ${index + 1}`,
          kind: "camera"
        }));
      setBrowserCameras(cameras);
      const preferred = cameras.find((device) => device.label === preferredLabel || device.id === preferredLabel);
      const browserDevice = enumerated.find((device) => device.kind === "videoinput" && (
        device.label === preferred?.label || device.deviceId === preferred?.id
      ));
      if (browserDevice && stream.getVideoTracks()[0]?.label !== browserDevice.label) {
        stream.getTracks().forEach((track) => track.stop());
        stream = await navigator.mediaDevices.getUserMedia({
          audio: false,
          video: {
            deviceId: { exact: browserDevice.deviceId },
            width: { ideal: 1280 },
            height: { ideal: 720 },
            frameRate: { ideal: 30, max: 30 }
          }
        });
      }
      cameraStream.current = stream;
      if (cameraVideo.current) {
        cameraVideo.current.srcObject = stream;
        await cameraVideo.current.play().catch(() => undefined);
      }
      setCameraAvailable(true);
      const selectedLabel = stream.getVideoTracks()[0]?.label || preferredLabel || "default";
      if (config.captureMode !== "browser") {
        update({ captureMode: "browser", cameraEnabled: true, cameraDevice: selectedLabel, cameraRotationDegrees: 0 });
      }
      return selectedLabel;
    } catch (cause) {
      setPreviewError(cause instanceof Error ? cause.message : String(cause));
      return null;
    }
  }, [config.cameraDevice, config.captureMode, stopCamera, t, update]);

  const chooseScreenOrWindow = useCallback(async () => {
    try {
      setPreviewError(null);
      stopScreen();
      const stream = await navigator.mediaDevices.getDisplayMedia({
        audio: false,
        video: { frameRate: { ideal: 30, max: 30 } }
      });
      const track = stream.getVideoTracks()[0];
      if (!track) throw new Error(t("No se recibió video de la pantalla o ventana seleccionada."));
      screenStream.current = stream;
      track.addEventListener("ended", () => {
        screenStream.current = null;
        setScreenAvailable(false);
      }, { once: true });
      if (screenVideo.current) {
        screenVideo.current.srcObject = stream;
        await screenVideo.current.play().catch(() => undefined);
      }
      const surface = track.getSettings().displaySurface;
      const surfaceLabel = surface === "window" ? t("Ventana") : surface === "monitor" ? t("Pantalla") : t("Pantalla o ventana");
      const label = track.label ? `${surfaceLabel} · ${track.label}` : surfaceLabel;
      setScreenAvailable(true);
      update({ screenEnabled: true, screenLabel: label });
    } catch (cause) {
      setPreviewError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [stopScreen, t, update]);

  useEffect(() => setDraftMix(mixPercent), [mixPercent]);

  useEffect(() => {
    if (open && config.enabled && config.cameraEnabled && !cameraStream.current) {
      void startCamera();
    }
  }, [config.cameraEnabled, config.enabled, open, startCamera]);

  useEffect(() => {
    if (!config.cameraEnabled) stopCamera();
  }, [config.cameraEnabled, stopCamera]);

  useEffect(() => {
    if (!config.screenEnabled) stopScreen();
  }, [config.screenEnabled, stopScreen]);

  useEffect(() => () => {
    cameraStream.current?.getTracks().forEach((track) => track.stop());
    screenStream.current?.getTracks().forEach((track) => track.stop());
  }, []);

  useEffect(() => {
    if (!offscreenCanvas.current) {
      offscreenCanvas.current = document.createElement("canvas");
      offscreenCanvas.current.width = 360;
      offscreenCanvas.current.height = 640;
    }
    let frameRequest = 0;
    let lastUploadAt = 0;
    const render = (now: number) => {
      const canvas = offscreenCanvas.current;
      const context = canvas?.getContext("2d");
      if (canvas && context) {
        context.clearRect(0, 0, canvas.width, canvas.height);
        if (config.screenEnabled && screenAvailable && screenVideo.current) {
          drawVisualLayer(context, screenVideo.current, visualLayerConfig(config, "screen"));
        }
        if (config.cameraEnabled && cameraAvailable && cameraVideo.current) {
          drawVisualLayer(context, cameraVideo.current, visualLayerConfig(config, "camera"));
        }
        for (const monitor of [previewCanvas.current, programCanvas.current]) {
          const monitorContext = monitor?.getContext("2d");
          if (!monitor || !monitorContext) continue;
          monitorContext.clearRect(0, 0, monitor.width, monitor.height);
          monitorContext.drawImage(canvas, 0, 0);
        }
        if (running && config.enabled && now - lastUploadAt >= 1000 / 24 && !uploadPending.current) {
          lastUploadAt = now;
          uploadPending.current = true;
          canvas.toBlob((blob) => {
            if (!blob) {
              uploadPending.current = false;
              return;
            }
            void blob.arrayBuffer()
              .then((buffer) => invoke("broadcast_push_visual_frame", {
                sequence: ++visualSequence.current,
                frameBase64: arrayBufferToBase64(buffer)
              }))
              .catch((cause) => setPreviewError(errorMessage(cause, locale)))
              .finally(() => { uploadPending.current = false; });
          }, "image/webp", 0.9);
        }
      }
      frameRequest = requestAnimationFrame(render);
    };
    frameRequest = requestAnimationFrame(render);
    return () => cancelAnimationFrame(frameRequest);
  }, [cameraAvailable, config, locale, running, screenAvailable]);

  const faderEnabled = running && config.enabled && cameraReady;
  const take = async (nextMix: number, transitionMillis: number) => {
    if (handoffPending || busy === "camera-mix") return;
    setDraftMix(nextMix);
    setHandoffPending(true);
    try {
      await onMix(nextMix, transitionMillis);
    } finally {
      setHandoffPending(false);
    }
  };

  const layer = visualLayerConfig(config, activeLayer);
  const updateLayer = (patch: Partial<VisualLayerConfig>) => {
    if (activeLayer === "camera") {
      update({
        cameraLayout: patch.layout ?? config.cameraLayout,
        cameraPosition: patch.position ?? config.cameraPosition,
        cameraSize: patch.size ?? config.cameraSize,
        cameraEffect: patch.effect ?? config.cameraEffect,
        cameraMirror: patch.mirror ?? config.cameraMirror,
        cameraRotationDegrees: patch.rotationDegrees ?? config.cameraRotationDegrees,
        cameraFraming: patch.framing ?? config.cameraFraming,
        cameraOpacityPercent: patch.opacityPercent ?? config.cameraOpacityPercent
      });
    } else {
      update({
        screenLayout: patch.layout ?? config.screenLayout,
        screenPosition: patch.position ?? config.screenPosition,
        screenSize: patch.size ?? config.screenSize,
        screenEffect: patch.effect ?? config.screenEffect,
        screenMirror: patch.mirror ?? config.screenMirror,
        screenRotationDegrees: patch.rotationDegrees ?? config.screenRotationDegrees,
        screenFraming: patch.framing ?? config.screenFraming,
        screenOpacityPercent: patch.opacityPercent ?? config.screenOpacityPercent
      });
    }
  };

  const captureElements = (
    <div className="pointer-events-none fixed -left-4 top-0 h-px w-px overflow-hidden opacity-0" aria-hidden="true">
      <video ref={screenVideo} muted playsInline />
      <video ref={cameraVideo} muted playsInline />
    </div>
  );

  if (!open) return <>{captureElements}</>;

  return (
    <>{captureElements}<div className="fixed inset-0 z-[80] flex items-center justify-center p-3" role="dialog" aria-modal="true" aria-labelledby="video-studio-title">
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={onClose} />
      <section className="relative z-[85] flex max-h-[94vh] w-full max-w-6xl flex-col overflow-hidden rounded-xl border border-white/15 bg-[#090b0a] text-white shadow-2xl">
        <header className="flex items-start justify-between gap-4 border-b border-white/10 px-5 py-4">
          <div>
            <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.22em] text-white/45">
              <SlidersHorizontal className="h-4 w-4" /> Rau Broadcast System
            </div>
            <h2 id="video-studio-title" className="mt-1 text-xl font-semibold">{t("Video Studio · Preview / Program")}</h2>
            <p className="mt-1 text-xs text-white/50">{t("Prepara la fuente y usa el fader para enviarla sin reiniciar RTMP.")}</p>
          </div>
          <Button type="button" size="sm" variant="secondary" onClick={onClose}>{t("Cerrar")}</Button>
        </header>

        <div className="grid min-h-0 flex-1 overflow-y-auto lg:grid-cols-[minmax(0,1fr)_310px]">
          <div className="grid gap-4 p-4 sm:grid-cols-2">
            <StudioMonitor
              label="PREVIEW"
              stationName={stationName}
              trackTitle={trackTitle}
              visualVisible={config.enabled}
              canvasRef={previewCanvas}
              visualAvailable={(config.cameraEnabled && cameraAvailable) || (config.screenEnabled && screenAvailable)}
              visualPlaceholder={previewError ? t("Vista previa no disponible") : t("Activa una cámara o elige una pantalla o ventana")}
            />
            <StudioMonitor
              label="PROGRAM"
              stationName={stationName}
              trackTitle={trackTitle}
              visualVisible={config.enabled && draftMix > 0}
              visualOpacity={draftMix / 100}
              canvasRef={programCanvas}
              visualAvailable={(config.cameraEnabled && cameraAvailable) || (config.screenEnabled && screenAvailable)}
              visualPlaceholder={t("Fuentes en Program")}
              transitionMillis={config.transitionMillis}
            />

            <div className="rounded-lg border border-white/10 bg-white/[0.035] p-4 sm:col-span-2">
              <div className="flex items-center justify-between gap-3 text-xs font-semibold uppercase tracking-[0.16em] text-white/55">
                <span>GRAPHIC</span><span>{draftMix}%</span><span>VISUAL LAYERS</span>
              </div>
              <input
                aria-label={t("Fader Preview a Program")}
                className="mt-3 h-3 w-full cursor-ew-resize accent-white disabled:cursor-not-allowed disabled:opacity-35"
                type="range"
                min={0}
                max={100}
                step={1}
                value={draftMix}
                disabled={!faderEnabled || busy === "camera-mix" || handoffPending}
                onChange={(event) => setDraftMix(Number(event.currentTarget.value))}
                onPointerUp={() => void take(draftMix, 0)}
                onKeyUp={() => void take(draftMix, 0)}
              />
              <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
                <span className="text-xs text-white/45">
                  {running
                    ? cameraReady ? t("El fader controla la señal que recibe Instagram.") : t("Esperando que el compositor quede listo...")
                    : t("El fader se habilita al iniciar el broadcast; la fuente visual comienza fuera de Program.")}
                </span>
                <Button
                  type="button"
                  disabled={!faderEnabled || busy === "camera-mix" || handoffPending}
                  onClick={() => void take(draftMix > 0 ? 0 : 100, config.transitionMillis)}
                >
                  {busy === "camera-mix" || handoffPending ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                  {draftMix > 0 ? t("Volver a gráfica") : t("AUTO · Enviar a Program")}
                </Button>
              </div>
            </div>
          </div>

          <aside className="grid content-start gap-4 border-t border-white/10 bg-black/20 p-4 lg:border-l lg:border-t-0">
            <div className="flex items-center justify-between gap-3">
              <div>
                <strong className="text-sm">{t("Fuente visual")}</strong>
                <span className="block text-xs text-white/40">{config.enabled ? running ? t("Capturando · fuera de Program") : t("Preparada · inicia fuera de Program") : t("Desactivada")}</span>
              </div>
              <label className="flex items-center gap-2 text-xs font-semibold">
                <input
                  type="checkbox"
                  checked={config.enabled}
                  disabled={running}
                  onChange={(event) => update({
                    enabled: event.currentTarget.checked,
                    captureMode: "browser",
                    cameraDevice: config.cameraDevice || "default"
                  })}
                />
                {t("Usar fuente")}
              </label>
            </div>

            <div className="grid grid-cols-2 gap-2">
              <button type="button" className={cn("rounded-md border p-3 text-left", activeLayer === "camera" ? "border-emerald-400/60 bg-emerald-400/10" : "border-white/10 bg-white/[.03]")} onClick={() => setActiveLayer("camera")}>
                <span className="flex items-center gap-2 text-xs font-semibold"><Camera className="h-4 w-4" />{t("Cámara")}</span>
                <span className="mt-1 block text-[10px] text-white/40">{cameraAvailable ? t("Activa") : t("Sin señal")}</span>
              </button>
              <button type="button" className={cn("rounded-md border p-3 text-left", activeLayer === "screen" ? "border-emerald-400/60 bg-emerald-400/10" : "border-white/10 bg-white/[.03]")} onClick={() => setActiveLayer("screen")}>
                <span className="flex items-center gap-2 text-xs font-semibold"><Monitor className="h-4 w-4" />{t("Pantalla / ventana")}</span>
                <span className="mt-1 block truncate text-[10px] text-white/40">{screenAvailable ? config.screenLabel : t("Sin señal")}</span>
              </button>
            </div>

            {activeLayer === "camera" ? <Field label={t("Cámara")}>
              <div className="flex gap-2">
                <select
                  className={cn(fieldClass, "border-white/15 bg-white/5 text-white")}
                  value={config.cameraDevice}
                  disabled={!config.enabled}
                  onChange={(event) => {
                    const cameraDevice = event.currentTarget.value;
                    update({ cameraDevice, cameraEnabled: true, cameraRotationDegrees: 0 });
                    void startCamera(cameraDevice);
                  }}
                >
                  <option value="default">{t("Cámara predeterminada")}</option>
                  {cameraDevices.map((device) => <option key={`${device.kind}:${device.id}`} value={device.label}>{device.label}</option>)}
                </select>
                <Button type="button" size="icon" variant="secondary" onClick={() => void startCamera()} aria-label={t("Refrescar fuentes") }>
                  <RefreshCcw className="h-4 w-4" />
                </Button>
              </div>
              <label className="mt-2 flex items-center gap-2 text-xs text-white/65">
                <input type="checkbox" checked={config.cameraEnabled} disabled={!config.enabled} onChange={(event) => {
                  if (event.currentTarget.checked) {
                    update({ cameraEnabled: true, cameraRotationDegrees: config.captureMode === "native" ? 0 : config.cameraRotationDegrees });
                    void startCamera();
                  } else {
                    stopCamera();
                    update({ cameraEnabled: false });
                  }
                }} />
                {t("Cámara activa")}
              </label>
            </Field> : <Field label={t("Pantalla o ventana")}>
              <Button type="button" variant="secondary" disabled={!config.enabled} onClick={() => void chooseScreenOrWindow()}>
                <Monitor className="h-4 w-4" />{screenAvailable ? t("Cambiar pantalla o ventana") : t("Elegir pantalla o ventana")}
              </Button>
              <label className="mt-2 flex items-center gap-2 text-xs text-white/65">
                <input type="checkbox" checked={config.screenEnabled} disabled={!config.enabled} onChange={(event) => {
                  if (event.currentTarget.checked) void chooseScreenOrWindow();
                  else {
                    stopScreen();
                    update({ screenEnabled: false });
                  }
                }} />
                {t("Pantalla o ventana activa")}
              </label>
              <span className="mt-2 block text-[11px] leading-relaxed text-white/40">{t("El selector del sistema permite compartir una pantalla completa o una ventana de aplicación.")}</span>
            </Field>}

            <Field label={t("Composición") }>
              <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.layout} disabled={!config.enabled} onChange={(event) => updateLayer({ layout: event.currentTarget.value })}>
                <option value="card">{t("Tarjeta")}</option>
                <option value="wide">{t("Ancho completo")}</option>
                <option value="background">{t("Fondo")}</option>
              </select>
            </Field>

            <div className="grid grid-cols-2 gap-3">
              <Field label={t("Posición") }>
                <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.position} disabled={!config.enabled || layer.layout !== "card"} onChange={(event) => updateLayer({ position: event.currentTarget.value })}>
                  <option value="top_left">{t("Arriba izquierda")}</option>
                  <option value="top_right">{t("Arriba derecha")}</option>
                  <option value="center">{t("Centro")}</option>
                  <option value="bottom_left">{t("Abajo izquierda")}</option>
                  <option value="bottom_right">{t("Abajo derecha")}</option>
                </select>
              </Field>
              <Field label={t("Tamaño") }>
                <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.size} disabled={!config.enabled || layer.layout !== "card"} onChange={(event) => updateLayer({ size: event.currentTarget.value })}>
                  <option value="small">{t("Pequeña")}</option>
                  <option value="medium">{t("Mediana")}</option>
                  <option value="large">{t("Grande")}</option>
                </select>
              </Field>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <Field label={t("Efecto") }>
                <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.effect} disabled={!config.enabled} onChange={(event) => updateLayer({ effect: event.currentTarget.value })}>
                  <option value="clean">{t("Limpio")}</option>
                  <option value="mono">{t("Monocromo")}</option>
                  <option value="contrast">{t("Contraste editorial")}</option>
                  <option value="dream">{t("Dream blur")}</option>
                </select>
              </Field>
              <Field label={t("Orientación") }>
                <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.rotationDegrees} disabled={!config.enabled} onChange={(event) => updateLayer({ rotationDegrees: Number(event.currentTarget.value) })}>
                  <option value={0}>{t("Normal · 0°")}</option>
                  <option value={90}>{t("Girar 90°")}</option>
                  <option value={180}>{t("Girar 180°")}</option>
                  <option value={270}>{t("Girar 270°")}</option>
                </select>
              </Field>
            </div>

            <label className="flex items-center gap-2 text-xs text-white/65">
              <input type="checkbox" checked={layer.mirror} disabled={!config.enabled} onChange={(event) => updateLayer({ mirror: event.currentTarget.checked })} />
              {t("Espejar fuente")}
            </label>

            <Field label={t("Encuadre") }>
              <select className={cn(fieldClass, "border-white/15 bg-white/5 text-white")} value={layer.framing} disabled={!config.enabled} onChange={(event) => updateLayer({ framing: event.currentTarget.value })}>
                <option value="contain">{t("Ajustar · mostrar imagen completa")}</option>
                <option value="cover">{t("Rellenar · recortar bordes")}</option>
              </select>
            </Field>

            <Field label={t("Opacidad máxima: {value}%", { value: layer.opacityPercent })}>
              <input type="range" min={20} max={100} step={5} value={layer.opacityPercent} disabled={!config.enabled} onChange={(event) => updateLayer({ opacityPercent: Number(event.currentTarget.value) })} />
            </Field>
            <Field label={t("Duración AUTO: {value} ms", { value: config.transitionMillis })}>
              <input type="range" min={0} max={3000} step={100} value={config.transitionMillis} disabled={!config.enabled} onChange={(event) => update({ transitionMillis: Number(event.currentTarget.value) })} />
            </Field>

            {previewError ? <p className="rounded-md border border-amber-400/20 bg-amber-400/10 p-2 text-xs text-amber-100">{previewError}</p> : null}
            {!running ? (
              <Button type="button" disabled={busy === "saving" || (config.enabled && !config.cameraEnabled && !config.screenEnabled)} onClick={() => void onSave()}>
                {busy === "saving" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
                {t("Guardar composición")}
              </Button>
            ) : (
              <p className="text-xs text-white/40">{t("Los cambios de fuente visual se aplican y guardan en vivo sin reiniciar RTMP.")}</p>
            )}
          </aside>
        </div>
      </section>
    </div></>
  );
}

type VisualLayerConfig = {
  layout: string;
  position: string;
  size: string;
  effect: string;
  mirror: boolean;
  rotationDegrees: number;
  framing: string;
  opacityPercent: number;
};

function visualLayerConfig(config: BroadcastVideoCompositor, layer: "camera" | "screen"): VisualLayerConfig {
  return layer === "camera" ? {
    layout: config.cameraLayout,
    position: config.cameraPosition,
    size: config.cameraSize,
    effect: config.cameraEffect,
    mirror: config.cameraMirror,
    rotationDegrees: config.cameraRotationDegrees,
    framing: config.cameraFraming,
    opacityPercent: config.cameraOpacityPercent
  } : {
    layout: config.screenLayout,
    position: config.screenPosition,
    size: config.screenSize,
    effect: config.screenEffect,
    mirror: config.screenMirror,
    rotationDegrees: config.screenRotationDegrees,
    framing: config.screenFraming,
    opacityPercent: config.screenOpacityPercent
  };
}

function visualLayerRect(config: VisualLayerConfig) {
  if (config.layout === "background") return { x: 0, y: 110, width: 360, height: 340 };
  if (config.layout === "wide") return { x: 0, y: 120, width: 360, height: 225 };
  const size = config.size === "small" ? 105 : config.size === "large" ? 205 : 150;
  const margin = 24;
  const positions: Record<string, { x: number; y: number }> = {
    top_left: { x: margin, y: 120 },
    top_right: { x: 360 - size - margin, y: 120 },
    center: { x: (360 - size) / 2, y: (640 - size) / 2 },
    bottom_left: { x: margin, y: 640 - size - margin },
    bottom_right: { x: 360 - size - margin, y: 640 - size - margin }
  };
  return { ...(positions[config.position] ?? positions.top_right), width: size, height: size };
}

function drawVisualLayer(context: CanvasRenderingContext2D, video: HTMLVideoElement, config: VisualLayerConfig) {
  if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth || !video.videoHeight) return;
  const box = visualLayerRect(config);
  const quarterTurn = config.rotationDegrees === 90 || config.rotationDegrees === 270;
  const targetWidth = quarterTurn ? box.height : box.width;
  const targetHeight = quarterTurn ? box.width : box.height;
  const scale = config.framing === "cover"
    ? Math.max(targetWidth / video.videoWidth, targetHeight / video.videoHeight)
    : Math.min(targetWidth / video.videoWidth, targetHeight / video.videoHeight);
  const drawWidth = video.videoWidth * scale;
  const drawHeight = video.videoHeight * scale;
  const filters: Record<string, string> = {
    clean: "none",
    mono: "grayscale(1)",
    contrast: "contrast(1.35) saturate(.82)",
    dream: "blur(1.5px) brightness(1.08) saturate(.72)"
  };

  context.save();
  context.globalAlpha = config.opacityPercent / 100;
  context.beginPath();
  context.rect(box.x, box.y, box.width, box.height);
  context.clip();
  context.fillStyle = "black";
  context.fillRect(box.x, box.y, box.width, box.height);
  context.translate(box.x + box.width / 2, box.y + box.height / 2);
  context.rotate(config.rotationDegrees * Math.PI / 180);
  context.scale(config.mirror ? -1 : 1, 1);
  context.filter = filters[config.effect] ?? "none";
  context.drawImage(video, -drawWidth / 2, -drawHeight / 2, drawWidth, drawHeight);
  context.restore();
}

function arrayBufferToBase64(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 32_768));
  }
  return btoa(binary);
}

function StudioMonitor({
  label,
  stationName,
  trackTitle,
  visualVisible,
  visualOpacity = 1,
  visualAvailable,
  visualPlaceholder,
  canvasRef,
  transitionMillis = 0
}: {
  label: string;
  stationName: string;
  trackTitle: string;
  visualVisible: boolean;
  visualOpacity?: number;
  visualAvailable: boolean;
  visualPlaceholder: string;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  transitionMillis?: number;
}) {
  return (
    <div>
      <div className="mb-2 flex items-center justify-between text-[11px] font-bold tracking-[0.2em] text-white/55">
        <span>{label}</span>
        <span className={cn("h-2 w-2 rounded-full", label === "PROGRAM" ? "bg-red-500" : "bg-emerald-400")} />
      </div>
      <div
        className="relative mx-auto aspect-[9/16] max-h-[58vh] overflow-hidden border border-white/15 bg-[#080b09] shadow-inner"
        style={{
          backgroundImage: "linear-gradient(rgba(255,255,255,.055) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,.055) 1px, transparent 1px)",
          backgroundSize: "12.5% 7.03%"
        }}
      >
        <div className="absolute inset-x-0 top-0 z-30 h-[1.6%] bg-white" />
        <div className="absolute left-[5%] right-[5%] top-[3%] z-30 h-[11%] border border-white/40 bg-black/70 px-[2.5%] py-[1.5%]">
          <strong className="block truncate text-[clamp(8px,1.8vw,17px)] font-medium uppercase">{stationName}</strong>
          <span className="absolute bottom-[10%] left-[2.5%] font-mono text-[clamp(4px,.65vw,7px)] tracking-[0.16em] text-white/55">LIVE / RAU BROADCAST SYSTEM</span>
        </div>
        <div className="absolute left-[5%] top-[20%] z-10 h-[35%] w-[1.2%] bg-white/85" />
        <div className="absolute left-[9.5%] right-[5%] top-[20%] z-10 h-[35%] border border-white/30 bg-gradient-to-br from-white/55 via-white/5 to-transparent" />
        <div className="absolute inset-x-[5%] top-[70%] z-30 border-t border-white/55 bg-black/90 pt-[4%]">
          <span className="font-mono text-[clamp(5px,.8vw,9px)] tracking-[0.13em] text-white/50">NOW PLAYING / CURRENT AUDIO</span>
          <strong className="mt-[3%] block line-clamp-3 text-[clamp(9px,1.8vw,18px)] font-medium uppercase leading-tight">{trackTitle}</strong>
        </div>
        {visualVisible ? <canvas ref={canvasRef} width={360} height={640} className="absolute inset-0 z-20 h-full w-full" style={{ opacity: visualOpacity, transition: `opacity ${transitionMillis}ms linear` }} /> : null}
        {visualVisible && !visualAvailable ? (
          <div className="absolute inset-x-[15%] top-[30%] z-20 grid h-[18%] place-items-center border border-white/20 bg-black/70 p-2 text-center">
            <div><Monitor className="mx-auto h-5 w-5 text-white/70" /><span className="mt-1 block text-[8px] uppercase tracking-wider text-white/55">{visualPlaceholder}</span></div>
          </div>
        ) : null}
        <span className="absolute bottom-[3%] left-[5%] z-30 font-mono text-[clamp(5px,.7vw,8px)] tracking-[0.12em] text-white/35">H264 / AAC / 720X1280 / 30FPS</span>
      </div>
    </div>
  );
}

function SourceTabButton({
  id,
  controls,
  active,
  enabled,
  icon,
  label,
  onClick
}: {
  id: string;
  controls: string;
  active: boolean;
  enabled: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      id={id}
      type="button"
      role="tab"
      aria-selected={active}
      aria-controls={controls}
      className={cn(
        "relative flex min-w-0 items-center justify-center gap-1.5 rounded-md px-2 py-2 text-xs font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
      )}
      onClick={onClick}
    >
      {icon}
      <span className="truncate">{label}</span>
      {enabled ? <span className="absolute right-1.5 top-1.5 h-1.5 w-1.5 rounded-full bg-emerald-500" aria-hidden="true" /> : null}
    </button>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="grid gap-1.5 text-xs font-medium text-muted-foreground"><span>{label}</span>{children}</label>;
}

function Metric({ label, value, icon, danger = false }: { label: string; value: string; icon: React.ReactNode; danger?: boolean }) {
  return (
    <Card className={cn("p-3", danger && "border-destructive/35")}>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">{icon}{label}</div>
      <strong className={cn("mt-2 block text-xl", danger && "text-destructive")}>{value}</strong>
    </Card>
  );
}

function StatusBadge({ status, label }: { status: string; label: string }) {
  const live = status === "live";
  const warning = ["connecting", "reconnecting", "stopping"].includes(status);
  return (
    <div className={cn(
      "flex max-w-xl items-center gap-2 rounded-full border px-3 py-1.5 text-xs font-medium",
      live && "border-emerald-500/25 bg-emerald-500/10 text-emerald-800 dark:text-emerald-200",
      warning && "border-amber-500/25 bg-amber-500/10 text-amber-800 dark:text-amber-200",
      !live && !warning && "border-border bg-secondary text-muted-foreground"
    )}>
      <span className={cn("h-2 w-2 shrink-0 rounded-full", live ? "animate-pulse bg-emerald-500" : warning ? "bg-amber-500" : "bg-muted-foreground")} />
      <span className="truncate">{label}</span>
    </div>
  );
}

function QueueStatus({ status }: { status: string }) {
  const labels: Record<string, string> = { queued: "cola", playing: "aire", played: "lista", skipped: "saltada", failed: "falló" };
  return (
    <span className={cn(
      "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide",
      status === "playing" && "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
      status === "failed" && "bg-destructive/10 text-destructive",
      !["playing", "failed"].includes(status) && "bg-secondary text-muted-foreground"
    )}>{labels[status] ?? status}</span>
  );
}

function entryTitle(entry: BroadcastQueueEntry) {
  return entry.artist ? `${entry.artist} — ${entry.title}` : entry.title;
}

function formatDuration(seconds?: number | null) {
  if (!seconds || seconds < 1) return "—";
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.floor(seconds % 60);
  return `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function statusLabel(status: string, t: (key: string) => string) {
  const labels: Record<string, string> = {
    idle: t("Detenida"),
    connecting: t("Conectando"),
    live: t("En vivo"),
    reconnecting: t("Reconectando"),
    stopping: t("Deteniendo"),
    error: t("Error")
  };
  return labels[status] ?? status;
}

function errorMessage(cause: unknown, locale: "es" | "en") {
  return translateBackendMessage(locale, cause instanceof Error ? cause.message : String(cause));
}

function safelyUnlisten(unlisten: UnlistenFn) {
  try {
    void Promise.resolve(unlisten()).catch(() => undefined);
  } catch {
    // Tauri may already have removed the listener during a dev reload.
  }
}
