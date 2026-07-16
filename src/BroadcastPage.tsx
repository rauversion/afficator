import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Library,
  LoaderCircle,
  Mic,
  MicOff,
  Play,
  Plus,
  Radio,
  RefreshCcw,
  Save,
  SkipForward,
  Square,
  Trash2,
  Wifi
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

type BroadcastProfile = {
  id: string;
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
  password_configured: boolean;
  listener_url: string;
  updated_at: string;
};

type BroadcastPreflight = {
  ffmpeg_available: boolean;
  mp3_encoder_available: boolean;
  icecast_protocol_available: boolean;
  tls_protocol_available: boolean;
  microphone_input_available: boolean;
  ready: boolean;
  message: string;
};

type BroadcastMicrophoneDevice = {
  id: string;
  label: string;
  is_default: boolean;
};

type BroadcastMicrophoneStatus = {
  configured: boolean;
  ready: boolean;
  live: boolean;
  device?: string | null;
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
  microphone: BroadcastMicrophoneStatus;
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

type QueueAppendResult = {
  appended_total: number;
  skipped_missing_total: number;
  queue: BroadcastQueueEntry[];
};

type BusyAction = "loading" | "saving" | "starting" | "stopping" | "skipping" | "appending" | "clearing" | string | null;

const fieldClass =
  "h-10 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none transition focus:border-foreground/35 focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

export function BroadcastPage() {
  const { locale, t } = useI18n();
  const [profile, setProfile] = useState<BroadcastProfile | null>(null);
  const [preflight, setPreflight] = useState<BroadcastPreflight | null>(null);
  const [status, setStatus] = useState<BroadcastStatus | null>(null);
  const [queue, setQueue] = useState<BroadcastQueueEntry[]>([]);
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [playlists, setPlaylists] = useState<PlaylistIndexPlaylist[]>([]);
  const [microphoneDevices, setMicrophoneDevices] = useState<BroadcastMicrophoneDevice[]>([]);
  const [libraryId, setLibraryId] = useState("");
  const [playlistPath, setPlaylistPath] = useState("");
  const [terminalLogs, setTerminalLogs] = useState<TerminalLogEntry[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [busy, setBusy] = useState<BusyAction>("loading");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

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
  const [microphoneEnabled, setMicrophoneEnabled] = useState(false);
  const [microphoneDevice, setMicrophoneDevice] = useState("default");
  const [microphoneGain, setMicrophoneGain] = useState("100");
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);

  const running = status ? ["connecting", "live", "reconnecting", "stopping"].includes(status.status) : false;
  const queuedTotal = queue.filter((entry) => entry.status === "queued").length;
  const completedTotal = queue.filter((entry) => entry.status === "played").length;
  const failedTotal = queue.filter((entry) => entry.status === "failed").length;

  const hydrateProfile = useCallback((next: BroadcastProfile) => {
    setProfile(next);
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
      invoke<PlaylistIndexLibrary[]>("playlist_index_libraries"),
      invoke<BroadcastMicrophoneDevice[]>("broadcast_microphone_devices")
    ])
      .then(([nextProfile, nextStatus, nextQueue, nextPreflight, nextLibraries, nextMicrophones]) => {
        if (disposed) return;
        hydrateProfile(nextProfile);
        setStatus(nextStatus);
        setQueue(nextQueue);
        setPreflight(nextPreflight);
        setLibraries(nextLibraries);
        setMicrophoneDevices(nextMicrophones);
        setLibraryId(nextLibraries[0]?.id ?? "");
      })
      .catch((cause) => setError(errorMessage(cause, locale)))
      .finally(() => setBusy(null));

    void listen<BroadcastProgressEvent>("broadcast-progress", ({ payload }) => {
      setStatus(payload.status);
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
      void invoke<BroadcastQueueEntry[]>("broadcast_queue").then(setQueue).catch(() => undefined);
    })
      .then((stopListening) => {
        if (disposed) stopListeningOnce(stopListening);
        else unlisten = stopListening;
      })
      .catch(() => undefined);

    const timer = window.setInterval(() => {
      void Promise.all([
        invoke<BroadcastStatus>("broadcast_status").then(setStatus),
        invoke<BroadcastQueueEntry[]>("broadcast_queue").then(setQueue)
      ]).catch(() => undefined);
    }, 2500);

    return () => {
      disposed = true;
      window.clearInterval(timer);
      stopListeningOnce();
    };
  }, [hydrateProfile, locale]);

  useEffect(() => {
    setPlaylistPath("");
    if (!libraryId) {
      setPlaylists([]);
      return;
    }
    void invoke<PlaylistIndexPlaylist[]>("playlist_index_library_playlists", { libraryId })
      .then((items) => {
        const playable = items.filter((item) => item.track_count > 0);
        setPlaylists(playable);
        setPlaylistPath(playable[0]?.path ?? "");
      })
      .catch((cause) => setError(errorMessage(cause, locale)));
  }, [libraryId, locale]);

  const activePlaylist = useMemo(
    () => playlists.find((playlist) => playlist.path === playlistPath) ?? null,
    [playlistPath, playlists]
  );

  async function saveProfile(event: FormEvent) {
    event.preventDefault();
    setBusy("saving");
    setError(null);
    setNotice(null);
    try {
      const saved = await invoke<BroadcastProfile>("broadcast_save_profile", {
        profile: {
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
          password: password || null,
          clearPassword
        }
      });
      hydrateProfile(saved);
      const nextPreflight = await invoke<BroadcastPreflight>("broadcast_preflight");
      setPreflight(nextPreflight);
      setNotice(t("Perfil Icecast guardado."));
    } catch (cause) {
      setError(errorMessage(cause, locale));
    } finally {
      setBusy(null);
    }
  }

  async function appendPlaylist() {
    if (!libraryId || !playlistPath) return;
    setBusy("appending");
    setError(null);
    setNotice(null);
    try {
      const result = await invoke<QueueAppendResult>("broadcast_append_playlist", {
        libraryId,
        playlistPath
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
      setStatus(await invoke<BroadcastStatus>("broadcast_start"));
      setNotice(t("Iniciando transmisión a Icecast."));
    });
  }

  async function stopBroadcast() {
    await runAction("stopping", async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_stop"));
    });
  }

  async function skipTrack() {
    await runAction("skipping", async () => {
      setStatus(await invoke<BroadcastStatus>("broadcast_skip"));
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

  async function refreshMicrophones() {
    await runAction("microphones", async () => {
      const devices = await invoke<BroadcastMicrophoneDevice[]>("broadcast_microphone_devices");
      setMicrophoneDevices(devices);
      if (!devices.some((device) => device.id === microphoneDevice)) {
        setMicrophoneDevice("default");
      }
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
    <main className={cn("min-h-screen bg-background p-4 pb-20 text-foreground lg:p-6", terminalExpanded && "pb-72")}>
      <div className="mx-auto grid w-full max-w-[1480px] gap-4">
        <header className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-muted-foreground">
              <Radio className="h-4 w-4" />
              <span className="text-xs font-semibold uppercase tracking-[0.18em]">{t("Broadcast")}</span>
            </div>
            <h1 className="mt-1 text-2xl font-semibold tracking-tight">{t("Radio desde casa")}</h1>
            <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
              {t("Rau Studio reproduce tu cola local y mantiene un stream MP3 persistente hacia Icecast.")}
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
              <CardTitle>{t("Destino Icecast")}</CardTitle>
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
                <Field label={t("Nombre de estación")}>
                  <input className={fieldClass} value={stationName} required maxLength={120} disabled={running} onChange={(event) => setStationName(event.target.value)} />
                </Field>
                <Field label={t("Descripción")}>
                  <input className={fieldClass} value={description} maxLength={240} disabled={running} onChange={(event) => setDescription(event.target.value)} />
                </Field>
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
                <div className="grid gap-3 rounded-md border border-border p-3">
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
                          {microphoneDevices.map((device) => <option key={device.id} value={device.id}>{device.label}</option>)}
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
                      </p>
                    </>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      {preflight?.microphone_input_available
                        ? t("Activa esta opción para seleccionar un micrófono.")
                        : t("El FFmpeg actual no incluye entrada AVFoundation.")}
                    </p>
                  )}
                </div>
                <div className="rounded-md bg-secondary/60 px-3 py-2 text-xs text-muted-foreground">
                  <strong className="block text-foreground">{profile?.listener_url ?? "—"}</strong>
                  <span>{translateBackendMessage(locale, preflight?.message ?? t("Revisando motor FFmpeg..."))}</span>
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
                    <Button size="sm" disabled={!preflight?.ready || (microphoneEnabled && !preflight?.microphone_input_available) || busy !== null} onClick={() => void startBroadcast()}>
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
                  {running && profile?.microphone_enabled ? (
                    <Button
                      size="sm"
                      variant={status?.microphone?.live ? "destructive" : "secondary"}
                      disabled={!status?.microphone?.ready || busy === "microphone"}
                      onClick={() => void toggleMicrophone()}
                    >
                      {status?.microphone?.live ? <MicOff className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
                      {status?.microphone?.live ? t("Silenciar micrófono") : t("Micrófono al aire")}
                    </Button>
                  ) : null}
                </div>
              </CardHeader>
              <CardContent className="p-3">
                {status?.now_playing ? (
                  <div className="rounded-md border border-emerald-500/25 bg-emerald-500/5 p-4">
                    <span className="text-xs font-semibold uppercase tracking-[0.15em] text-emerald-700 dark:text-emerald-300">{t("Ahora al aire")}</span>
                    <strong className="mt-2 block text-lg">{entryTitle(status.now_playing)}</strong>
                    <span className="mt-1 block text-xs text-muted-foreground">{status.now_playing.playlist_name}</span>
                  </div>
                ) : (
                  <div className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
                    {running ? t("La conexión sigue viva transmitiendo silencio hasta que haya una pista.") : t("Configura Icecast, agrega una playlist y sal al aire.")}
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
                    {translateBackendMessage(locale, status?.microphone?.message ?? t("Micrófono esperando inicio."))}
                  </div>
                ) : null}
              </CardContent>
            </Card>

            <Card className="min-h-[360px]">
              <CardHeader>
                <CardTitle>{t("Cola de broadcast")}</CardTitle>
                <Button size="sm" variant="ghost" disabled={queue.every((entry) => entry.status === "playing") || busy === "clearing"} onClick={() => void clearQueue()}>
                  <Trash2 className="h-4 w-4" />
                  {t("Limpiar")}
                </Button>
              </CardHeader>
              <CardContent>
                <div className="grid gap-2 border-b border-border p-3 md:grid-cols-[minmax(140px,0.75fr)_minmax(180px,1fr)_auto]">
                  <select className={fieldClass} value={libraryId} onChange={(event) => setLibraryId(event.target.value)}>
                    <option value="">{t("Selecciona biblioteca")}</option>
                    {libraries.map((library) => <option key={library.id} value={library.id}>{library.source_name}</option>)}
                  </select>
                  <select className={fieldClass} value={playlistPath} disabled={!libraryId} onChange={(event) => setPlaylistPath(event.target.value)}>
                    <option value="">{t("Selecciona playlist")}</option>
                    {playlists.map((playlist) => <option key={playlist.path} value={playlist.path}>{playlist.name} · {playlist.track_count}</option>)}
                  </select>
                  <Button disabled={!activePlaylist || busy === "appending"} onClick={() => void appendPlaylist()}>
                    {busy === "appending" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
                    {t("Agregar")}
                  </Button>
                </div>
                {queue.length === 0 ? (
                  <div className="grid min-h-48 place-items-center p-6 text-sm text-muted-foreground">{t("La cola está vacía.")}</div>
                ) : (
                  <div className="divide-y divide-border">
                    {queue.map((entry) => (
                      <div key={entry.id} className={cn("grid grid-cols-[minmax(0,1fr)_auto] gap-3 px-3 py-2.5", entry.status === "playing" && "bg-emerald-500/5")}>
                        <div className="min-w-0">
                          <div className="flex min-w-0 items-center gap-2">
                            <span className="truncate text-sm font-medium">{entryTitle(entry)}</span>
                            <QueueStatus status={entry.status} />
                          </div>
                          <span className="mt-0.5 block truncate text-xs text-muted-foreground">{entry.playlist_name} · {formatDuration(entry.duration_seconds)}</span>
                          {entry.error ? <span className="mt-1 block text-xs text-destructive">{entry.error}</span> : null}
                        </div>
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
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        </section>

      </div>
      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle={t("ffmpeg / icecast / micrófono")}
        emptyMessage="Sin eventos todavía."
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
    </main>
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
