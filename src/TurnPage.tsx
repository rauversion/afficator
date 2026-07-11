import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  Disc3,
  FolderOpen,
  ImagePlus,
  Loader2,
  Music,
  Pause,
  Palette,
  Play,
  RefreshCw,
  RotateCcw,
  RotateCw,
  Timer,
  Trash2,
  Video,
  X
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { cn } from "./lib/utils";

type TurnJob = {
  id: string;
  cover_image_path: string;
  cover_image_name: string;
  audio_file_path: string;
  audio_file_name: string;
  output_path?: string | null;
  state: "pending" | "running" | "completed" | "failed";
  duration_seconds: number;
  loop_speed: number;
  audio_start?: number | null;
  audio_end?: number | null;
  background_color: string;
  disc_size: number;
  error_message?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  failed_at?: string | null;
  created_at: string;
  updated_at: string;
  ready: boolean;
};

type TurnProgressEvent = {
  type: "turn_progress";
  id: string;
  job_id: string;
  event: string;
  step: string;
  level: "info" | "warning" | "error";
  message: string;
  progress?: number | null;
  timestamp: string;
  job: TurnJob;
  payload: Record<string, unknown>;
};

type TimelineEvent = TurnProgressEvent & {
  key: string;
};

type TurnTerminalLog = TerminalLogEntry;
type TurnTab = "editor" | "history";

export function TurnPage() {
  const [activeTab, setActiveTab] = useState<TurnTab>("editor");
  const [jobs, setJobs] = useState<TurnJob[]>([]);
  const [activeJobId, setActiveJobId] = useState("");
  const [coverImagePath, setCoverImagePath] = useState("");
  const [audioFilePath, setAudioFilePath] = useState("");
  const [backgroundColor, setBackgroundColor] = useState("#faafc8");
  const [discSize, setDiscSize] = useState(75);
  const [loopSpeed, setLoopSpeed] = useState(33);
  const [audioStart, setAudioStart] = useState(0);
  const [audioEnd, setAudioEnd] = useState(15);
  const [durationSeconds, setDurationSeconds] = useState(15);
  const [audioDuration, setAudioDuration] = useState<number | null>(null);
  const [previewPlaying, setPreviewPlaying] = useState(false);
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);
  const [terminalLogs, setTerminalLogs] = useState<TurnTerminalLog[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [eventsLoadingJobId, setEventsLoadingJobId] = useState("");
  const [busy, setBusy] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");
  const previewAudioRef = useRef<HTMLAudioElement | null>(null);
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);
  const activeJobIdRef = useRef("");

  useEffect(() => {
    activeJobIdRef.current = activeJobId;
  }, [activeJobId]);

  useEffect(() => {
    void loadTurnJobs();

    const unlisteners: UnlistenFn[] = [];
    listen<TurnProgressEvent>("turn-progress", (event) => {
      const payload = event.payload;
      setJobs((current) => upsertJob(current, payload.job));
      setActiveJobId(payload.job_id);
      setTimeline((current) => mergeTimelineEvents(current, [payload]));
      appendTerminalLog(payload);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  useEffect(() => {
    if (!activeJobId) {
      setTerminalLogs([]);
      return;
    }

    void loadJobEvents(activeJobId);
  }, [activeJobId]);

  const activeJob = jobs.find((job) => job.id === activeJobId) ?? jobs[0] ?? null;
  const activeTimeline = activeJob
    ? timeline.filter((event) => event.job_id === activeJob.id).slice().reverse()
    : [];
  const activeProgress = activeJob?.state === "completed"
    ? 100
    : activeTimeline[0]?.progress ?? (activeJob?.state === "running" ? 8 : 0);
  const progressByJobId = useMemo(() => {
    const map = new Map<string, number>();
    for (const event of timeline) {
      if (typeof event.progress === "number") map.set(event.job_id, event.progress);
    }
    return map;
  }, [timeline]);
  const coverPreviewPath = coverImagePath || activeJob?.cover_image_path || "";
  const coverPreviewUrl = coverPreviewPath ? convertFileSrc(coverPreviewPath) : "";
  const audioUrl = audioFilePath ? convertFileSrc(audioFilePath) : "";
  const videoUrl = activeJob?.output_path ? convertFileSrc(activeJob.output_path) : "";
  const spinDuration = Math.max(0.4, 60 / Math.max(1, loopSpeed));
  const previewIsSpinning = previewPlaying || activeJob?.state === "running";

  useEffect(() => {
    const audio = previewAudioRef.current;
    if (!audio) return;

    if (!previewPlaying) {
      if (!audio.paused) audio.pause();
      return;
    }

    const rangeStart = Math.max(0, audioStart);
    const rangeEnd = audioEnd > rangeStart ? audioEnd : rangeStart + 0.01;
    if (!Number.isFinite(audio.currentTime) || audio.currentTime < rangeStart || audio.currentTime >= rangeEnd) {
      audio.currentTime = rangeStart;
    }

    audio.play().catch((error) => {
      setPreviewPlaying(false);
      setErrorMessage(`No se pudo reproducir el preview: ${String(error)}`);
    });
  }, [audioEnd, audioStart, audioUrl, previewPlaying]);

  useEffect(() => {
    const audio = previewAudioRef.current;
    if (!audio || previewPlaying) return;
    audio.currentTime = Math.max(0, audioStart);
  }, [audioStart, previewPlaying]);

  async function loadTurnJobs() {
    setErrorMessage("");
    try {
      const rows = await invoke<TurnJob[]>("turn_list_jobs");
      setJobs(rows);
      const nextActiveJob =
        rows.find((job) => job.id === activeJobIdRef.current) ?? rows[0] ?? null;
      setActiveJobId(nextActiveJob?.id ?? "");
      if (nextActiveJob) syncEditorFromJob(nextActiveJob);
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function loadJobEvents(jobId: string) {
    setEventsLoadingJobId(jobId);
    try {
      const events = await invoke<TurnProgressEvent[]>("turn_job_events", { jobId });
      setTimeline((current) => mergeTimelineEvents(current, events));

      if (activeJobIdRef.current === jobId) {
        const logs = events.map((event, index) => eventToTerminalLog(event, index + 1));
        nextTerminalLogId.current = logs.length + 1;
        setTerminalLogs(logs);
      }
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setEventsLoadingJobId((current) => (current === jobId ? "" : current));
    }
  }

  async function chooseCoverImage() {
    setErrorMessage("");
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Cover",
          extensions: ["jpg", "jpeg", "png", "webp"]
        }
      ]
    });
    if (typeof selected === "string") {
      activeJobIdRef.current = "";
      setActiveJobId("");
      setCoverImagePath(selected);
      setActiveTab("editor");
    }
  }

  async function chooseAudioFile() {
    setErrorMessage("");
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Audio",
          extensions: ["wav", "wave", "aif", "aiff", "flac", "mp3", "m4a", "aac", "alac"]
        }
      ]
    });
    if (typeof selected === "string") {
      activeJobIdRef.current = "";
      setActiveJobId("");
      setAudioFilePath(selected);
      setAudioDuration(null);
      setPreviewPlaying(false);
      setAudioStart(0);
      setAudioEnd(0);
      setDurationSeconds(0);
      setActiveTab("editor");
    }
  }

  async function startTurn() {
    if (!coverImagePath || !audioFilePath) return;
    setBusy(true);
    setErrorMessage("");
    try {
      const job = await invoke<TurnJob>("turn_start_job", {
        coverImagePath,
        audioFilePath,
        durationSeconds,
        loopSpeed,
        audioStart,
        audioEnd,
        backgroundColor,
        discSize
      });
      setJobs((current) => upsertJob(current, job));
      setActiveJobId(job.id);
      setActiveTab("editor");
      setTimeline((current) => current.filter((event) => event.job_id !== job.id));
      clearTerminal();
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function retryJob(job: TurnJob) {
    setBusy(true);
    setErrorMessage("");
    try {
      const updated = await invoke<TurnJob>("turn_retry_job", { jobId: job.id });
      setJobs((current) => upsertJob(current, updated));
      setActiveJobId(updated.id);
      setActiveTab("editor");
      syncEditorFromJob(updated);
      setTimeline((current) => current.filter((event) => event.job_id !== updated.id));
      clearTerminal();
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function deleteJob(job: TurnJob) {
    setBusy(true);
    setErrorMessage("");
    try {
      const deletedId = await invoke<string>("turn_delete_job", { jobId: job.id });
      setJobs((current) => current.filter((item) => item.id !== deletedId));
      setTimeline((current) => current.filter((event) => event.job_id !== deletedId));
      if (activeJobId === deletedId) {
        const nextJob = jobs.find((item) => item.id !== deletedId);
        setActiveJobId(nextJob?.id ?? "");
        if (nextJob) syncEditorFromJob(nextJob);
        clearTerminal();
      }
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function openFolder(path?: string | null) {
    if (!path) return;
    try {
      await invoke("open_parent_folder", { path });
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  function selectJob(job: TurnJob) {
    setActiveJobId(job.id);
    setActiveTab("editor");
    syncEditorFromJob(job);
  }

  function syncEditorFromJob(job: TurnJob) {
    setPreviewPlaying(false);
    setCoverImagePath(job.cover_image_path);
    setAudioFilePath(job.audio_file_path);
    setBackgroundColor(job.background_color);
    setDiscSize(job.disc_size);
    setLoopSpeed(job.loop_speed);
    setAudioStart(job.audio_start ?? 0);
    setAudioEnd(job.audio_end ?? job.duration_seconds);
    setDurationSeconds(job.duration_seconds);
  }

  function updateTrimRange(start: number, end: number) {
    const totalDuration = audioDuration ?? Math.max(end, audioEnd, durationSeconds, 1);
    const safeStart = Math.max(0, Math.min(start, totalDuration - 0.01));
    const safeEnd = Math.min(totalDuration, Math.max(end, safeStart + 0.01));

    setAudioStart(round2(safeStart));
    setAudioEnd(round2(safeEnd));
    setDurationSeconds(round2(safeEnd - safeStart));
  }

  function handleAudioMetadata(event: React.SyntheticEvent<HTMLAudioElement>) {
    const duration = event.currentTarget.duration;
    if (!Number.isFinite(duration) || duration <= 0) return;
    const rounded = round2(duration);
    setAudioDuration(rounded);
    if (!activeJobIdRef.current) {
      setAudioStart(0);
      setAudioEnd(rounded);
      setDurationSeconds(rounded);
    }
  }

  function handlePreviewTimeUpdate(event: React.SyntheticEvent<HTMLAudioElement>) {
    const audio = event.currentTarget;
    const rangeStart = Math.max(0, audioStart);
    const rangeEnd = audioEnd > rangeStart ? audioEnd : rangeStart + 0.01;

    if (audio.currentTime >= rangeEnd) {
      audio.pause();
      audio.currentTime = rangeStart;
      setPreviewPlaying(false);
    }
  }

  function handlePreviewPlay() {
    const audio = previewAudioRef.current;
    if (!audio) {
      setPreviewPlaying(true);
      return;
    }

    const rangeStart = Math.max(0, audioStart);
    const rangeEnd = audioEnd > rangeStart ? audioEnd : rangeStart + 0.01;
    if (!Number.isFinite(audio.currentTime) || audio.currentTime < rangeStart || audio.currentTime >= rangeEnd) {
      audio.currentTime = rangeStart;
    }
    setPreviewPlaying(true);
  }

  function handlePreviewPause() {
    setPreviewPlaying(false);
  }

  function appendTerminalLog(event: TurnProgressEvent) {
    const log = eventToTerminalLog(event, nextTerminalLogId.current);

    nextTerminalLogId.current += 1;
    setTerminalLogs((current) => [...current, log].slice(-1200));
    window.requestAnimationFrame(() => {
      if (terminalElement.current) {
        terminalElement.current.scrollTop = terminalElement.current.scrollHeight;
      }
    });
  }

  function clearTerminal() {
    setTerminalLogs([]);
  }

  return (
    <main className={cn("min-w-0 p-4 pb-20", terminalExpanded && "pb-72")}>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
            <Disc3 className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-semibold tracking-normal">Turn</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {activeJob?.output_path ?? (coverImagePath || "Mockups de discos girando en MP4")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" onClick={() => void loadTurnJobs()} disabled={busy}>
            <RefreshCw className="h-4 w-4" />
            Refrescar
          </Button>
          <Button onClick={() => void startTurn()} disabled={busy || !coverImagePath || !audioFilePath}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Video className="h-4 w-4" />}
            Generar video
          </Button>
        </div>
      </header>

      {errorMessage ? (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}

      <div className="mb-3 inline-flex rounded-md border border-border bg-secondary p-1">
        <TurnTabButton active={activeTab === "editor"} onClick={() => setActiveTab("editor")}>
          Editor
        </TurnTabButton>
        <TurnTabButton active={activeTab === "history"} onClick={() => setActiveTab("history")}>
          Historial {jobs.length > 0 ? `(${jobs.length})` : ""}
        </TurnTabButton>
      </div>

      {activeTab === "editor" ? (
        <section className="grid gap-3">
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <RotateCw className="h-4 w-4" />
                <CardTitle>Nuevo turn</CardTitle>
              </div>
              <span className="text-xs text-muted-foreground">MP4 1080x1080</span>
            </CardHeader>
            <CardContent className="grid gap-4 p-3">
              <div className="grid gap-4 lg:grid-cols-[minmax(280px,420px)_minmax(0,1fr)]">
                <div className="grid gap-3">
                  <div
                    className="relative aspect-square overflow-hidden rounded-md border border-border"
                    style={{ backgroundColor }}
                  >
                    <div
                      className="absolute left-1/2 top-1/2 aspect-square -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-full bg-secondary bg-cover bg-center shadow-2xl ring-1 ring-black/15"
                      style={{
                        width: `${discSize}%`,
                        backgroundImage: coverPreviewUrl ? `url("${coverPreviewUrl}")` : undefined,
                        animation: previewIsSpinning ? `turn-spin ${spinDuration}s linear infinite` : undefined
                      }}
                    >
                      <div className="absolute left-1/2 top-1/2 h-[16%] w-[16%] -translate-x-1/2 -translate-y-1/2 rounded-full border border-white/40 bg-black/70 shadow-inner" />
                    </div>
                    {!coverPreviewUrl ? (
                      <div className="absolute inset-0 grid place-items-center text-sm text-muted-foreground">
                        Elige una portada
                      </div>
                    ) : null}
                  </div>
                  <div className="flex items-center gap-2">
                    <Button type="button" variant="secondary" onClick={() => setPreviewPlaying((current) => !current)}>
                      {previewPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                      {previewPlaying ? "Pausar preview" : "Play preview"}
                    </Button>
                    <span className="text-xs text-muted-foreground">
                      {loopSpeed} RPM / disco {discSize}%
                    </span>
                  </div>
                </div>

                <div className="grid gap-3">
                  <PathPicker
                    icon={<ImagePlus className="h-4 w-4" />}
                    label="Cover"
                    value={coverImagePath}
                    placeholder="JPG, PNG o WEBP"
                    onChoose={chooseCoverImage}
                    onClear={() => setCoverImagePath("")}
                    disabled={busy}
                  />
                  <PathPicker
                    icon={<Music className="h-4 w-4" />}
                    label="Audio"
                    value={audioFilePath}
                    placeholder="WAV, AIFF, FLAC, MP3, M4A"
                    onChoose={chooseAudioFile}
                    onClear={() => setAudioFilePath("")}
                    disabled={busy}
                  />

                  {audioUrl ? (
                    <audio
                      ref={previewAudioRef}
                      className="w-full"
                      controls
                      src={audioUrl}
                      onLoadedMetadata={handleAudioMetadata}
                      onTimeUpdate={handlePreviewTimeUpdate}
                      onPlay={handlePreviewPlay}
                      onPause={handlePreviewPause}
                    />
                  ) : null}

                  <div className="grid gap-3 rounded-md border border-border bg-background/60 p-3">
                    <div className="flex items-center gap-2 text-sm font-semibold">
                      <Palette className="h-4 w-4" />
                      Visual
                    </div>
                    <div className="grid gap-3 md:grid-cols-3">
                      <label className="grid gap-1 text-sm font-medium">
                        Fondo
                        <input
                          className="h-10 rounded-md border border-input bg-background px-2"
                          type="color"
                          value={backgroundColor}
                          onChange={(event) => setBackgroundColor(event.currentTarget.value)}
                        />
                      </label>
                      <RangeInput
                        label="Disco"
                        value={discSize}
                        min={20}
                        max={100}
                        step={1}
                        suffix="%"
                        onChange={setDiscSize}
                      />
                      <RangeInput
                        label="Velocidad"
                        value={loopSpeed}
                        min={1}
                        max={78}
                        step={1}
                        suffix="RPM"
                        onChange={setLoopSpeed}
                      />
                    </div>
                  </div>

                  <div className="grid gap-3 rounded-md border border-border bg-background/60 p-3">
                    <div className="flex items-center gap-2 text-sm font-semibold">
                      <Timer className="h-4 w-4" />
                      Audio y duracion
                    </div>
                    <AudioTrimSlider
                      totalDuration={audioDuration ?? Math.max(audioEnd, durationSeconds, 1)}
                      startTime={audioStart}
                      endTime={audioEnd > audioStart ? audioEnd : Math.max(audioStart + 0.01, audioDuration ?? 1)}
                      isPlaying={previewPlaying}
                      setIsPlaying={setPreviewPlaying}
                      onValueChange={updateTrimRange}
                    />
                    <span className="text-xs text-muted-foreground">
                      {audioDuration
                        ? `Audio: ${formatSeconds(audioDuration)}. Video: ${formatSeconds(durationSeconds)}.`
                        : "La duracion del audio se detecta al cargar el archivo."}
                    </span>
                  </div>

                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <Button variant="secondary" onClick={() => void openFolder(audioFilePath)} disabled={!audioFilePath}>
                      <FolderOpen className="h-4 w-4" />
                      Abrir audio
                    </Button>
                    <Button onClick={() => void startTurn()} disabled={busy || !coverImagePath || !audioFilePath}>
                      {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Video className="h-4 w-4" />}
                      Generar video
                    </Button>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {activeJob ? (
            <TurnDetail
              job={activeJob}
              progress={activeProgress}
              timeline={activeTimeline}
              eventsLoading={eventsLoadingJobId === activeJob.id}
              videoUrl={videoUrl}
              busy={busy}
              onRetry={retryJob}
              onDelete={deleteJob}
              onOpenFolder={openFolder}
            />
          ) : (
            <Card className="p-6">
              <CardTitle>Sin videos todavia</CardTitle>
              <p className="mt-2 text-sm text-muted-foreground">
                Genera un turn para ver el MP4, sus eventos y el historial.
              </p>
            </Card>
          )}
        </section>
      ) : (
        <TurnHistoryPanel
          jobs={jobs}
          activeJobId={activeJob?.id ?? ""}
          eventsLoadingJobId={eventsLoadingJobId}
          progressByJobId={progressByJobId}
          onSelectJob={selectJob}
        />
      )}

      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle="ffmpeg / turn"
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
    </main>
  );
}

function TurnTabButton({
  active,
  onClick,
  children
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className={cn(
        "min-h-8 rounded-md px-3 text-sm font-semibold transition-colors",
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground"
      )}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function TurnHistoryPanel({
  jobs,
  activeJobId,
  eventsLoadingJobId,
  progressByJobId,
  onSelectJob
}: {
  jobs: TurnJob[];
  activeJobId: string;
  eventsLoadingJobId: string;
  progressByJobId: Map<string, number>;
  onSelectJob: (job: TurnJob) => void;
}) {
  return (
    <Card className="min-h-[520px] overflow-hidden">
      <CardHeader>
        <CardTitle>Historial</CardTitle>
        <span className="text-xs text-muted-foreground">{jobs.length} videos</span>
      </CardHeader>
      <CardContent className="p-0">
        {jobs.length === 0 ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">Sin jobs.</div>
        ) : null}
        {jobs.length > 0 ? (
          <div className="sticky top-0 z-10 grid min-h-10 grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)_116px_160px] items-center gap-3 border-b border-border bg-secondary px-3 text-xs font-semibold text-muted-foreground max-lg:hidden">
            <span>Cover</span>
            <span>Audio</span>
            <span>Estado</span>
            <span>Progreso</span>
          </div>
        ) : null}
        {jobs.map((job) => {
          const progress = job.state === "completed"
            ? 100
            : Math.max(0, Math.min(100, progressByJobId.get(job.id) ?? 0));

          return (
            <button
              key={job.id}
              type="button"
              className={cn(
                "grid w-full gap-3 border-b border-border px-3 py-3 text-left hover:bg-secondary lg:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)_116px_160px] lg:items-center",
                activeJobId === job.id && "bg-secondary"
              )}
              onClick={() => onSelectJob(job)}
            >
              <div className="min-w-0">
                <span className="block truncate text-sm font-semibold" title={job.cover_image_name}>
                  {job.cover_image_name}
                </span>
                <span className="mt-1 block text-xs text-muted-foreground">{formatDate(job.created_at)}</span>
              </div>
              <span className="min-w-0 truncate text-xs text-muted-foreground" title={job.audio_file_name}>
                {job.audio_file_name}
              </span>
              <TurnStatusPill state={job.state} />
              <div className="grid gap-2">
                <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
                  <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <HistoryChip active={job.ready}>MP4</HistoryChip>
                  <HistoryChip active={job.state === "running" || job.state === "pending"}>Procesando</HistoryChip>
                  <HistoryChip active={eventsLoadingJobId === job.id}>Eventos</HistoryChip>
                </div>
              </div>
            </button>
          );
        })}
      </CardContent>
    </Card>
  );
}

function TurnDetail({
  job,
  progress,
  timeline,
  eventsLoading,
  videoUrl,
  busy,
  onRetry,
  onDelete,
  onOpenFolder
}: {
  job: TurnJob;
  progress: number;
  timeline: TimelineEvent[];
  eventsLoading: boolean;
  videoUrl: string;
  busy: boolean;
  onRetry: (job: TurnJob) => Promise<void>;
  onDelete: (job: TurnJob) => Promise<void>;
  onOpenFolder: (path?: string | null) => Promise<void>;
}) {
  return (
    <Card>
      <CardHeader>
        <div className="min-w-0">
          <CardTitle className="truncate">{job.cover_image_name}</CardTitle>
          <span className="block truncate text-xs text-muted-foreground" title={job.audio_file_path}>
            {job.audio_file_name}
          </span>
        </div>
        <TurnStatusPill state={job.state} />
      </CardHeader>
      <CardContent className="grid gap-4 p-3">
        {job.state === "running" || job.state === "pending" ? (
          <div>
            <div className="mb-1 flex items-center justify-between text-xs text-muted-foreground">
              <span>{timeline[0]?.step ?? "ffmpeg"}</span>
              <span>{Math.round(progress)}%</span>
            </div>
            <div className="h-2 overflow-hidden rounded-full bg-secondary">
              <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${Math.max(0, Math.min(100, progress))}%` }} />
            </div>
          </div>
        ) : null}

        {job.state === "failed" ? (
          <div className="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
            <div className="flex items-start gap-2">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <span>{job.error_message ?? "No se pudo generar el video."}</span>
            </div>
          </div>
        ) : null}

        <section className="grid gap-3 lg:grid-cols-[minmax(280px,420px)_minmax(0,1fr)]">
          <div className="grid gap-2">
            <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Output
            </span>
            {videoUrl ? (
              <video className="aspect-square w-full rounded-md border border-border bg-black" src={videoUrl} controls />
            ) : (
              <div className="grid aspect-square place-items-center rounded-md border border-dashed border-border text-sm text-muted-foreground">
                El MP4 aparecera cuando termine el render.
              </div>
            )}
          </div>
          <div className="grid gap-3">
            <div className="grid gap-2 rounded-md border border-border bg-background/60 p-3 text-sm">
              <InfoRow label="Duracion" value={formatSeconds(job.duration_seconds)} />
              <InfoRow label="Trim" value={`${formatSeconds(job.audio_start ?? 0)} - ${formatSeconds(job.audio_end ?? job.duration_seconds)}`} />
              <InfoRow label="Velocidad" value={`${job.loop_speed} RPM`} />
              <InfoRow label="Disco" value={`${job.disc_size}%`} />
              <InfoRow label="Fondo" value={job.background_color} />
              <InfoRow label="Eventos" value={eventsLoading ? "Cargando" : `${timeline.length}`} />
            </div>

            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" onClick={() => void onOpenFolder(job.cover_image_path)}>
                <FolderOpen className="h-4 w-4" />
                Cover
              </Button>
              <Button variant="secondary" onClick={() => void onOpenFolder(job.audio_file_path)}>
                <FolderOpen className="h-4 w-4" />
                Audio
              </Button>
              <Button variant="secondary" disabled={!job.output_path} onClick={() => void onOpenFolder(job.output_path)}>
                <FolderOpen className="h-4 w-4" />
                Abrir carpeta MP4
              </Button>
              <Button
                variant="secondary"
                disabled={busy || job.state === "pending" || job.state === "running"}
                onClick={() => void onRetry(job)}
              >
                <RefreshCw className="h-4 w-4" />
                Reintentar
              </Button>
              <Button
                variant="destructive"
                disabled={busy || job.state === "pending" || job.state === "running"}
                onClick={() => void onDelete(job)}
              >
                <Trash2 className="h-4 w-4" />
                Eliminar
              </Button>
            </div>
          </div>
        </section>

        <section className="grid gap-2">
          <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Eventos recientes
          </span>
          <div className="max-h-56 overflow-auto rounded-md border border-border">
            {timeline.length === 0 ? (
              <div className="px-3 py-4 text-sm text-muted-foreground">Sin eventos.</div>
            ) : null}
            {timeline.slice(0, 30).map((event) => (
              <div key={event.key} className="grid gap-1 border-b border-border px-3 py-2 text-xs last:border-b-0">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-semibold">{event.step}</span>
                  <span className="text-muted-foreground">{formatDate(event.timestamp)}</span>
                </div>
                <span className={cn(event.level === "error" && "text-red-700 dark:text-red-300")}>
                  {event.message}
                </span>
              </div>
            ))}
          </div>
        </section>
      </CardContent>
    </Card>
  );
}

function PathPicker({
  icon,
  label,
  value,
  placeholder,
  disabled,
  onChoose,
  onClear
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  placeholder: string;
  disabled: boolean;
  onChoose: () => Promise<void>;
  onClear: () => void;
}) {
  return (
    <div className="grid gap-2">
      <span className="flex items-center gap-2 text-xs font-semibold text-muted-foreground">
        {icon}
        {label}
      </span>
      <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] gap-2">
        <div className="truncate rounded-md border border-border bg-secondary px-3 py-2 text-sm" title={value}>
          {value || placeholder}
        </div>
        <Button type="button" variant="secondary" onClick={() => void onChoose()} disabled={disabled}>
          Elegir
        </Button>
        <Button type="button" variant="secondary" onClick={onClear} disabled={disabled || !value}>
          Limpiar
        </Button>
      </div>
    </div>
  );
}

function RangeInput({
  label,
  value,
  min,
  max,
  step,
  suffix,
  onChange
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="grid gap-1 text-sm font-medium">
      <span className="flex justify-between gap-2">
        <span>{label}</span>
        <span className="text-muted-foreground">{value}{suffix}</span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
      />
    </label>
  );
}

function AudioTrimSlider({
  totalDuration,
  startTime,
  endTime,
  isPlaying,
  setIsPlaying,
  onValueChange
}: {
  totalDuration: number;
  startTime: number;
  endTime: number;
  isPlaying: boolean;
  setIsPlaying: (playing: boolean) => void;
  onValueChange: (start: number, end: number) => void;
}) {
  const [isDragging, setIsDragging] = useState<"start" | "end" | "center" | null>(null);
  const [dragOffset, setDragOffset] = useState(0);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const safeTotalDuration = Math.max(0.01, totalDuration);
  const safeStart = Math.max(0, Math.min(startTime, safeTotalDuration - 0.01));
  const safeEnd = Math.min(safeTotalDuration, Math.max(endTime, safeStart + 0.01));
  const selectedDuration = safeEnd - safeStart;
  const getPercentage = useCallback(
    (value: number) => (value / safeTotalDuration) * 100,
    [safeTotalDuration]
  );
  const getValueFromPercentage = useCallback(
    (percentage: number) => (percentage / 100) * safeTotalDuration,
    [safeTotalDuration]
  );

  const handleMouseDown = useCallback(
    (type: "start" | "end" | "center", event: React.MouseEvent) => {
      event.preventDefault();
      setIsDragging(type);

      if (type === "center" && trackRef.current) {
        const rect = trackRef.current.getBoundingClientRect();
        const centerX =
          rect.left +
          (getPercentage(safeStart) / 100) * rect.width +
          (((getPercentage(safeEnd) - getPercentage(safeStart)) / 100) * rect.width) / 2;
        setDragOffset(event.clientX - centerX);
      }
    },
    [getPercentage, safeEnd, safeStart]
  );

  const handleMouseMove = useCallback(
    (event: MouseEvent) => {
      if (!isDragging || !trackRef.current) return;

      const rect = trackRef.current.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const percentage = Math.max(0, Math.min(100, (x / rect.width) * 100));
      const value = getValueFromPercentage(percentage);

      if (isDragging === "start") {
        const nextStart = Math.max(0, Math.min(value, safeEnd - 0.01));
        onValueChange(nextStart, safeEnd);
        return;
      }

      if (isDragging === "end") {
        const nextEnd = Math.max(safeStart + 0.01, Math.min(value, safeTotalDuration));
        onValueChange(safeStart, nextEnd);
        return;
      }

      const centerX = event.clientX - dragOffset - rect.left;
      const centerPercentage = (centerX / rect.width) * 100;
      const centerValue = getValueFromPercentage(centerPercentage);
      const halfDuration = selectedDuration / 2;
      let nextStart = centerValue - halfDuration;
      let nextEnd = centerValue + halfDuration;

      if (nextStart < 0) {
        nextStart = 0;
        nextEnd = selectedDuration;
      } else if (nextEnd > safeTotalDuration) {
        nextEnd = safeTotalDuration;
        nextStart = safeTotalDuration - selectedDuration;
      }

      onValueChange(nextStart, nextEnd);
    },
    [
      dragOffset,
      getValueFromPercentage,
      isDragging,
      onValueChange,
      safeEnd,
      safeStart,
      safeTotalDuration,
      selectedDuration
    ]
  );

  const handleMouseUp = useCallback(() => {
    setIsDragging(null);
    setDragOffset(0);
  }, []);

  useEffect(() => {
    if (!isDragging) return;

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [handleMouseMove, handleMouseUp, isDragging]);

  function moveRange(delta: number) {
    let nextStart = safeStart + delta;
    let nextEnd = safeEnd + delta;

    if (nextStart < 0) {
      nextStart = 0;
      nextEnd = selectedDuration;
    } else if (nextEnd > safeTotalDuration) {
      nextEnd = safeTotalDuration;
      nextStart = safeTotalDuration - selectedDuration;
    }

    onValueChange(nextStart, nextEnd);
  }

  return (
    <div className="grid gap-3">
      <div ref={trackRef} className="relative h-12 rounded-md bg-secondary">
        <div
          className="absolute bottom-0 top-0 rounded-sm border-x-2 border-primary bg-primary/20"
          style={{
            left: `${getPercentage(safeStart)}%`,
            width: `${getPercentage(safeEnd) - getPercentage(safeStart)}%`
          }}
        >
          <button
            type="button"
            className="absolute left-1/2 top-1/2 grid h-6 w-6 -translate-x-1/2 -translate-y-1/2 cursor-grab place-items-center rounded-md border border-border bg-background active:cursor-grabbing"
            onMouseDown={(event) => handleMouseDown("center", event)}
            aria-label="Mover rango"
          >
            <span className="grid grid-cols-2 gap-0.5">
              <span className="h-1 w-1 rounded-full bg-muted-foreground" />
              <span className="h-1 w-1 rounded-full bg-muted-foreground" />
              <span className="h-1 w-1 rounded-full bg-muted-foreground" />
              <span className="h-1 w-1 rounded-full bg-muted-foreground" />
            </span>
          </button>
        </div>
        <button
          type="button"
          className="absolute bottom-0 top-0 w-2 cursor-ew-resize rounded-l bg-primary"
          style={{ left: `${getPercentage(safeStart)}%` }}
          onMouseDown={(event) => handleMouseDown("start", event)}
          aria-label="Inicio trim"
        />
        <button
          type="button"
          className="absolute bottom-0 top-0 w-2 cursor-ew-resize rounded-r bg-primary"
          style={{ left: `${getPercentage(safeEnd)}%` }}
          onMouseDown={(event) => handleMouseDown("end", event)}
          aria-label="Fin trim"
        />
      </div>

      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
        <label className="grid gap-1 text-sm font-medium">
          Inicio
          <input
            className="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            type="number"
            value={formatTrimInput(safeStart)}
            min={0}
            max={safeTotalDuration}
            step={0.1}
            onChange={(event) => {
              const value = Number.parseFloat(event.currentTarget.value) || 0;
              onValueChange(Math.max(0, Math.min(value, safeEnd - 0.01)), safeEnd);
            }}
          />
        </label>
        <label className="grid gap-1 text-sm font-medium">
          Fin
          <input
            className="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            type="number"
            value={formatTrimInput(safeEnd)}
            min={0}
            max={safeTotalDuration}
            step={0.1}
            onChange={(event) => {
              const value = Number.parseFloat(event.currentTarget.value) || 0;
              onValueChange(safeStart, Math.min(safeTotalDuration, Math.max(value, safeStart + 0.01)));
            }}
          />
        </label>
        <div className="flex items-end gap-2">
          <Button type="button" variant="secondary" size="icon" onClick={() => setIsPlaying(!isPlaying)}>
            {isPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          </Button>
          <Button type="button" variant="secondary" size="icon" onClick={() => moveRange(-1)}>
            <RotateCcw className="h-4 w-4" />
          </Button>
          <Button type="button" variant="secondary" size="icon" onClick={() => moveRange(1)}>
            <RotateCw className="h-4 w-4" />
          </Button>
          <Button type="button" variant="secondary" size="icon" onClick={() => onValueChange(0, safeTotalDuration)}>
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}

function formatTrimInput(value: number) {
  return value.toFixed(1);
}

function TurnStatusPill({ state }: { state: TurnJob["state"] }) {
  const labels = {
    pending: "Pendiente",
    running: "Procesando",
    completed: "Listo",
    failed: "Error"
  };
  const classes = {
    pending: "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200",
    running: "border-blue-300 bg-blue-50 text-blue-800 dark:border-blue-900 dark:bg-blue-950/40 dark:text-blue-200",
    completed: "border-emerald-300 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200",
    failed: "border-red-300 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200"
  };
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-semibold", classes[state])}>
      {state === "completed" ? <CheckCircle2 className="h-3 w-3" /> : null}
      {state === "failed" ? <AlertTriangle className="h-3 w-3" /> : null}
      {state === "running" ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
      {labels[state]}
    </span>
  );
}

function HistoryChip({ active, children }: { active: boolean; children: React.ReactNode }) {
  return (
    <span className={cn(
      "rounded-full border px-2 py-0.5 text-[11px] font-semibold",
      active
        ? "border-primary/30 bg-primary/10 text-foreground"
        : "border-border bg-background text-muted-foreground"
    )}>
      {children}
    </span>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[96px_minmax(0,1fr)] gap-2 border-b border-border py-1 last:border-b-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate font-semibold" title={value}>{value}</span>
    </div>
  );
}

function eventToTimelineEvent(event: TurnProgressEvent): TimelineEvent {
  return {
    ...event,
    key: event.id
  };
}

function mergeTimelineEvents(current: TimelineEvent[], events: TurnProgressEvent[]) {
  const byKey = new Map(current.map((event) => [event.key, event]));
  for (const event of events) {
    byKey.set(event.id, eventToTimelineEvent(event));
  }
  return Array.from(byKey.values()).sort((left, right) => left.timestamp.localeCompare(right.timestamp));
}

function eventToTerminalLog(event: TurnProgressEvent, id: number): TurnTerminalLog {
  return {
    id,
    time: formatTime(event.timestamp),
    level: event.level,
    track_id: event.job_id.slice(0, 8),
    name: event.job.cover_image_name,
    message: event.message
  };
}

function upsertJob(jobs: TurnJob[], job: TurnJob) {
  const next = jobs.filter((item) => item.id !== job.id);
  return [job, ...next].sort((left, right) => right.created_at.localeCompare(left.created_at));
}

function round2(value: number) {
  return Math.round(value * 100) / 100;
}

function formatSeconds(value: number) {
  if (!Number.isFinite(value)) return "n/d";
  if (value >= 60) {
    const minutes = Math.floor(value / 60);
    const seconds = Math.round(value % 60).toString().padStart(2, "0");
    return `${minutes}:${seconds}`;
  }
  return `${round2(value)}s`;
}

function formatTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}
