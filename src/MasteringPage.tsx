import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  AudioLines,
  CheckCircle2,
  Download,
  FolderOpen,
  ImagePlus,
  Loader2,
  Play,
  RefreshCw,
  SlidersHorizontal,
  Sparkles,
  Tags,
  Trash2,
  Wand2
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

type MasteringProfile = {
  key: string;
  label_es: string;
  target_lufs: number;
  true_peak_ceiling_db: number;
  style_es: string;
};

type MasteringJob = {
  id: string;
  source_path: string;
  source_name: string;
  target_profile: string;
  state: "pending" | "running" | "completed" | "failed";
  feedback?: string | null;
  reference_notes?: string | null;
  output_format: MasteringOutputFormat;
  metadata: MasteringMetadata;
  cover_art_path?: string | null;
  output_path?: string | null;
  package_report: Record<string, unknown>;
  recipe: Record<string, unknown>;
  analysis_before: Record<string, unknown>;
  analysis_after: Record<string, unknown>;
  error_message?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  failed_at?: string | null;
  created_at: string;
  updated_at: string;
  ready: boolean;
};

type MasteringOutputFormat = "aiff_24" | "aiff_cdj16" | "wav_24";

type MasteringMetadata = {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  genre?: string | null;
  year?: string | null;
  track_number?: string | null;
  composer?: string | null;
  label?: string | null;
  copyright?: string | null;
  bpm?: string | null;
  musical_key?: string | null;
  isrc?: string | null;
  comment?: string | null;
};

type MasteringProgressEvent = {
  type: "mastering_progress";
  id: string;
  job_id: string;
  event: string;
  step: string;
  level: "info" | "warning" | "error";
  message: string;
  progress?: number | null;
  timestamp: string;
  job: MasteringJob;
  payload: Record<string, unknown>;
};

type OpenAiApiKeyStatus = {
  configured: boolean;
  preview?: string | null;
};

type TimelineEvent = MasteringProgressEvent & {
  key: string;
};

type MasteringTerminalLog = TerminalLogEntry;

export function MasteringPage() {
  const { locale, t } = useI18n();
  const [profiles, setProfiles] = useState<MasteringProfile[]>([]);
  const [jobs, setJobs] = useState<MasteringJob[]>([]);
  const [sourcePath, setSourcePath] = useState("");
  const [targetProfile, setTargetProfile] = useState("demo_balanced");
  const [outputFormat, setOutputFormat] = useState<MasteringOutputFormat>("aiff_24");
  const [metadata, setMetadata] = useState<MasteringMetadata>({});
  const [coverArtPath, setCoverArtPath] = useState("");
  const [feedback, setFeedback] = useState("");
  const [referenceNotes, setReferenceNotes] = useState("");
  const [useAi, setUseAi] = useState(true);
  const [apiKeyStatus, setApiKeyStatus] = useState<OpenAiApiKeyStatus | null>(null);
  const [activeJobId, setActiveJobId] = useState("");
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);
  const [terminalLogs, setTerminalLogs] = useState<MasteringTerminalLog[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [eventsLoadingJobId, setEventsLoadingJobId] = useState("");
  const [busy, setBusy] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);
  const activeJobIdRef = useRef("");

  useEffect(() => {
    activeJobIdRef.current = activeJobId;
  }, [activeJobId]);

  useEffect(() => {
    void loadMastering();

    const unlisteners: UnlistenFn[] = [];
    listen<MasteringProgressEvent>("mastering-progress", (event) => {
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

  const profileByKey = useMemo(
    () => new Map(profiles.map((profile) => [profile.key, profile])),
    [profiles]
  );
  const activeJob = jobs.find((job) => job.id === activeJobId) ?? jobs[0] ?? null;
  const activeProfile = activeJob
    ? profileByKey.get(activeJob.target_profile)
    : profileByKey.get(targetProfile);
  const activeTimeline = activeJob
    ? timeline.filter((event) => event.job_id === activeJob.id).slice().reverse()
    : [];
  const activeProgress = activeJob?.state === "completed"
    ? 100
    : activeTimeline[0]?.progress ?? (activeJob?.state === "running" ? 5 : 0);

  async function loadMastering() {
    setErrorMessage("");
    try {
      const [profileRows, jobRows, openAiStatus] = await Promise.all([
        invoke<MasteringProfile[]>("mastering_profiles"),
        invoke<MasteringJob[]>("mastering_list_jobs"),
        invoke<OpenAiApiKeyStatus>("get_openai_api_key_status")
      ]);
      setProfiles(profileRows);
      setJobs(jobRows);
      setApiKeyStatus(openAiStatus);
      setUseAi(openAiStatus.configured);
      if (profileRows.length > 0 && !profileRows.some((profile) => profile.key === targetProfile)) {
        setTargetProfile(profileRows[0].key);
      }
      const nextActiveJob =
        jobRows.find((job) => job.id === activeJobIdRef.current) ?? jobRows[0] ?? null;
      setActiveJobId(nextActiveJob?.id ?? "");
      if (nextActiveJob) syncEditorFromJob(nextActiveJob);
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function loadJobEvents(jobId: string) {
    setEventsLoadingJobId(jobId);
    try {
      const events = await invoke<MasteringProgressEvent[]>("mastering_job_events", { jobId });
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

  async function chooseSourceFile() {
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
      setSourcePath(selected);
      setMetadata((current) => ({
        ...current,
        title: current.title?.trim() ? current.title : fileStem(selected)
      }));
    }
  }

  async function chooseCoverArt() {
    setErrorMessage("");
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Cover",
          extensions: ["jpg", "jpeg", "png"]
        }
      ]
    });
    if (typeof selected === "string") setCoverArtPath(selected);
  }

  async function startMastering() {
    if (!sourcePath) return;
    setBusy(true);
    setErrorMessage("");
    try {
      const job = await invoke<MasteringJob>("mastering_start_job", {
        sourcePath,
        targetProfile,
        feedback,
        referenceNotes,
        outputFormat,
        metadata: normalizedMetadataForSubmit(metadata, sourcePath),
        coverArtPath: coverArtPath || null,
        useAi
      });
      setJobs((current) => upsertJob(current, job));
      setActiveJobId(job.id);
      syncEditorFromJob(job);
      setTimeline((current) => current.filter((event) => event.job_id !== job.id));
      clearTerminal();
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function retryJob(job: MasteringJob) {
    setBusy(true);
    setErrorMessage("");
    try {
      const updated = await invoke<MasteringJob>("mastering_retry_job", {
        jobId: job.id,
        feedback: feedback.trim() || job.feedback,
        referenceNotes: referenceNotes.trim() || job.reference_notes,
        outputFormat,
        metadata: normalizedMetadataForSubmit(metadata, sourcePath || job.source_path),
        coverArtPath: coverArtPath || null,
        useAi
      });
      setJobs((current) => upsertJob(current, updated));
      setActiveJobId(updated.id);
      syncEditorFromJob(updated);
      setTimeline((current) => current.filter((event) => event.job_id !== updated.id));
      clearTerminal();
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function deleteJob(job: MasteringJob) {
    setBusy(true);
    setErrorMessage("");
    try {
      const deletedId = await invoke<string>("mastering_delete_job", { jobId: job.id });
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

  function selectJob(job: MasteringJob) {
    setActiveJobId(job.id);
    syncEditorFromJob(job);
  }

  function syncEditorFromJob(job: MasteringJob) {
    setSourcePath(job.source_path);
    setTargetProfile(job.target_profile);
    setOutputFormat(normalizeEditableOutputFormat(job.output_format));
    setMetadata(job.metadata ?? {});
    setCoverArtPath(job.cover_art_path ?? "");
    setFeedback(job.feedback ?? "");
    setReferenceNotes(job.reference_notes ?? "");
  }

  function updateMetadataField(field: keyof MasteringMetadata, value: string) {
    setMetadata((current) => ({ ...current, [field]: value }));
  }

  function appendTerminalLog(event: MasteringProgressEvent) {
    const log = eventToTerminalLog(event, nextTerminalLogId.current);
    log.message = translateBackendMessage(locale, log.message);

    nextTerminalLogId.current += 1;
    setTerminalLogs((current) => [...current, log].slice(-1000));
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
            <Wand2 className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-semibold tracking-normal">Mastering</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {sourcePath || t("Sin archivo seleccionado")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" onClick={() => void loadMastering()} disabled={busy}>
            <RefreshCw className="h-4 w-4" />
            {t("Refrescar")}
          </Button>
          <Button onClick={() => void chooseSourceFile()} disabled={busy}>
            <AudioLines className="h-4 w-4" />
            {t("Elegir audio")}
          </Button>
        </div>
      </header>

      {errorMessage ? (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}

      <section className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="grid gap-3">
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <SlidersHorizontal className="h-4 w-4" />
                <CardTitle>{t("Nuevo master")}</CardTitle>
              </div>
              <span className="text-xs text-muted-foreground">
                {apiKeyStatus?.configured ? `AI ${apiKeyStatus.preview}` : t("AI sin key")}
              </span>
            </CardHeader>
            <CardContent className="grid gap-4 p-3">
              <div className="grid gap-2">
                <span className="text-xs font-semibold text-muted-foreground">{t("Archivo")}</span>
                <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2">
                  <div className="truncate rounded-md border border-border bg-secondary px-3 py-2 text-sm" title={sourcePath}>
                    {sourcePath || t("Elige un archivo de audio")}
                  </div>
                  <Button variant="secondary" onClick={() => void chooseSourceFile()} disabled={busy}>
                    {t("Elegir")}
                  </Button>
                </div>
              </div>

              <div className="grid gap-2">
                <span className="text-xs font-semibold text-muted-foreground">{t("Presets")}</span>
                <div className="grid gap-2 md:grid-cols-2">
                  {profiles.map((profile) => (
                    <button
                      key={profile.key}
                      type="button"
                      className={cn(
                        "min-h-[104px] rounded-md border p-3 text-left transition-colors",
                        targetProfile === profile.key
                          ? "border-primary bg-primary/10"
                          : "border-border bg-background hover:bg-secondary"
                      )}
                      onClick={() => setTargetProfile(profile.key)}
                    >
                      <span className="block text-sm font-semibold">{profile.label_es}</span>
                      <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
                        {profile.style_es}
                      </span>
                      <span className="mt-3 block text-xs font-semibold">
                        {profile.target_lufs} LUFS / TP {profile.true_peak_ceiling_db} dB
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="grid gap-3 rounded-md border border-border bg-background/60 p-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Tags className="h-4 w-4" />
                    <span className="text-sm font-semibold">{t("Formato y metadata")}</span>
                  </div>
                  <select
                    className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
                    value={outputFormat}
                    onChange={(event) => setOutputFormat(normalizeOutputFormat(event.currentTarget.value))}
                  >
                    <option value="aiff_24">AIFF 24-bit</option>
                    <option value="aiff_cdj16">AIFF CDJ safe 16-bit</option>
                  </select>
                </div>

                <div className="grid gap-2 md:grid-cols-3">
                  <MetadataInput label={t("Titulo")} value={metadata.title} onChange={(value) => updateMetadataField("title", value)} />
                  <MetadataInput label={t("Artista")} value={metadata.artist} onChange={(value) => updateMetadataField("artist", value)} />
                  <MetadataInput label="Album" value={metadata.album} onChange={(value) => updateMetadataField("album", value)} />
                  <MetadataInput label={t("Genero")} value={metadata.genre} onChange={(value) => updateMetadataField("genre", value)} />
                  <MetadataInput label={t("Ano")} value={metadata.year} onChange={(value) => updateMetadataField("year", value)} />
                  <MetadataInput label="Track" value={metadata.track_number} onChange={(value) => updateMetadataField("track_number", value)} />
                  <MetadataInput label="BPM" value={metadata.bpm} onChange={(value) => updateMetadataField("bpm", value)} />
                  <MetadataInput label="Key" value={metadata.musical_key} onChange={(value) => updateMetadataField("musical_key", value)} />
                  <MetadataInput label="ISRC" value={metadata.isrc} onChange={(value) => updateMetadataField("isrc", value)} />
                  <MetadataInput label="Compositor" value={metadata.composer} onChange={(value) => updateMetadataField("composer", value)} />
                  <MetadataInput label="Label" value={metadata.label} onChange={(value) => updateMetadataField("label", value)} />
                  <MetadataInput label="Copyright" value={metadata.copyright} onChange={(value) => updateMetadataField("copyright", value)} />
                </div>

                <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_220px]">
                  <label className="grid gap-1 text-sm font-medium">
                    {t("Comentario")}
                    <textarea
                      className="min-h-20 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      value={metadata.comment ?? ""}
                      onChange={(event) => updateMetadataField("comment", event.currentTarget.value)}
                      placeholder={t("Notas que quedaran embebidas en el AIFF...")}
                    />
                  </label>
                  <div className="grid gap-2">
                    <span className="text-sm font-medium">Cover</span>
                    <div className="flex gap-2">
                      <Button type="button" variant="secondary" onClick={() => void chooseCoverArt()} disabled={busy}>
                        <ImagePlus className="h-4 w-4" />
                        {t("Elegir")}
                      </Button>
                      <Button type="button" variant="secondary" onClick={() => setCoverArtPath("")} disabled={!coverArtPath || busy}>
                        {t("Limpiar")}
                      </Button>
                    </div>
                    {coverArtPath ? (
                      <div className="grid grid-cols-[52px_minmax(0,1fr)] gap-2 rounded-md border border-border p-2">
                        <img className="h-12 w-12 rounded-sm object-cover" src={convertFileSrc(coverArtPath)} alt="" />
                        <span className="min-w-0 truncate text-xs text-muted-foreground" title={coverArtPath}>{coverArtPath}</span>
                      </div>
                    ) : (
                      <div className="rounded-md border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">
                        {t("JPG o PNG opcional.")}
                      </div>
                    )}
                  </div>
                </div>
              </div>

              <div className="grid gap-3 md:grid-cols-2">
                <label className="grid gap-1 text-sm font-medium">
                  {t("Feedback")}
                  <textarea
                    className="min-h-28 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    value={feedback}
                    onChange={(event) => setFeedback(event.currentTarget.value)}
                    placeholder={t("Mantener pegada, limpiar subgrave, suavizar hats...")}
                  />
                </label>
                <label className="grid gap-1 text-sm font-medium">
                  {t("Referencia")}
                  <textarea
                    className="min-h-28 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    value={referenceNotes}
                    onChange={(event) => setReferenceNotes(event.currentTarget.value)}
                    placeholder={t("Club, streaming, demo, vinilo, referencia sonora...")}
                  />
                </label>
              </div>

              <div className="flex flex-wrap items-center justify-between gap-3">
                <label className="inline-flex items-center gap-2 text-sm font-medium">
                  <input
                    type="checkbox"
                    checked={useAi}
                    disabled={!apiKeyStatus?.configured}
                    onChange={(event) => setUseAi(event.currentTarget.checked)}
                  />
                  <Sparkles className="h-4 w-4" />
                  AI
                </label>
                <Button disabled={busy || !sourcePath} onClick={() => void startMastering()}>
                  {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Wand2 className="h-4 w-4" />}
                  {t("Generar master")}
                </Button>
              </div>
            </CardContent>
          </Card>

          {activeJob ? (
            <MasteringDetail
              job={activeJob}
              profile={activeProfile}
              progress={activeProgress}
              timeline={activeTimeline}
              eventsLoading={eventsLoadingJobId === activeJob.id}
              busy={busy}
              onRetry={retryJob}
              onDelete={deleteJob}
              onOpenFolder={openFolder}
            />
          ) : (
            <Card className="p-6">
              <CardTitle>{t("Sin masters todavia")}</CardTitle>
              <p className="mt-2 text-sm text-muted-foreground">
                {t("El historial aparece cuando generes el primer master.")}
              </p>
            </Card>
          )}
        </div>

        <Card className="h-[calc(100vh-112px)] min-h-[520px] overflow-hidden max-xl:h-[420px]">
          <CardHeader>
            <CardTitle>{t("Historial")}</CardTitle>
            <span className="text-xs text-muted-foreground">{t("{count} masters", { count: jobs.length })}</span>
          </CardHeader>
          <CardContent>
            {jobs.length === 0 ? (
              <div className="px-3 py-4 text-sm text-muted-foreground">{t("Sin jobs.")}</div>
            ) : null}
            {jobs.map((job) => (
              <button
                key={job.id}
                type="button"
                className={cn(
                  "grid w-full gap-2 border-b border-border px-3 py-3 text-left hover:bg-secondary",
                  activeJob?.id === job.id && "bg-secondary"
                )}
                onClick={() => selectJob(job)}
              >
                <div className="flex min-w-0 items-center justify-between gap-2">
                  <span className="truncate text-sm font-semibold" title={job.source_name}>
                    {job.source_name}
                  </span>
                  <StatusPill state={job.state} />
                </div>
                <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
                  <span>{profileByKey.get(job.target_profile)?.label_es ?? job.target_profile} / {outputFormatLabel(job.output_format)}</span>
                  <span>{formatDate(job.created_at)}</span>
                </div>
                <div className="flex min-w-0 items-center justify-between gap-2 text-xs">
                  <span className="truncate text-muted-foreground" title={job.output_path ?? job.source_path}>
                    {job.output_path ? t("Master disponible") : job.source_path}
                  </span>
                  <span className="font-semibold text-foreground">
                    {activeJob?.id === job.id
                      ? eventsLoadingJobId === job.id
                        ? t("Cargando")
                        : t("Abierto")
                      : t("Ver detalle")}
                  </span>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <HistoryChip active={Boolean(job.feedback?.trim())}>Feedback</HistoryChip>
                  <HistoryChip active={Boolean(job.reference_notes?.trim())}>Referencia</HistoryChip>
                  <HistoryChip active={Boolean(job.metadata?.title || job.metadata?.artist)}>Tags</HistoryChip>
                  <HistoryChip active={Boolean(job.cover_art_path)}>Cover</HistoryChip>
                </div>
              </button>
            ))}
          </CardContent>
        </Card>
      </section>
      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle={t("ffmpeg / ai / mastering")}
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
    </main>
  );
}

function MasteringDetail({
  job,
  profile,
  progress,
  timeline,
  eventsLoading,
  busy,
  onRetry,
  onDelete,
  onOpenFolder
}: {
  job: MasteringJob;
  profile?: MasteringProfile;
  progress: number;
  timeline: TimelineEvent[];
  eventsLoading: boolean;
  busy: boolean;
  onRetry: (job: MasteringJob) => Promise<void>;
  onDelete: (job: MasteringJob) => Promise<void>;
  onOpenFolder: (path?: string | null) => Promise<void>;
}) {
  const recipe = job.recipe ?? {};
  const diagnosis = getObject(recipe, "diagnosis");
  const feedback = getObject(recipe, "feedback_interpretation");
  const target = getObject(recipe, "target");
  const chain = Array.isArray(recipe.processing_chain) ? recipe.processing_chain : [];
  const warnings = Array.isArray(recipe.warnings_es) ? recipe.warnings_es : [];
  const originalFeedback = job.feedback?.trim() ?? "";
  const originalReference = job.reference_notes?.trim() ?? "";
  const packageWarnings = Array.isArray(job.package_report?.warnings) ? job.package_report.warnings : [];

  return (
    <div className="grid gap-3">
      <Card>
        <CardHeader>
          <div className="min-w-0">
            <CardTitle className="truncate">{job.source_name}</CardTitle>
            <span className="block truncate text-xs text-muted-foreground" title={job.source_path}>
              {profile?.label_es ?? job.target_profile} / {outputFormatLabel(job.output_format)}
            </span>
          </div>
          <StatusPill state={job.state} />
        </CardHeader>
        <CardContent className="grid gap-4 p-3">
          {job.state === "running" || job.state === "pending" ? (
            <div>
              <div className="mb-1 flex items-center justify-between text-xs text-muted-foreground">
                <span>{timeline[0]?.step ?? "pipeline"}</span>
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
                <span>{job.error_message ?? "No se pudo generar el master."}</span>
              </div>
            </div>
          ) : null}

          <div className="grid gap-3 lg:grid-cols-2">
            <AudioPanel title="Original" path={job.source_path} />
            <AudioPanel title="Master" path={job.output_path} />
          </div>

          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={() => void onOpenFolder(job.source_path)}>
              <FolderOpen className="h-4 w-4" />
              Abrir original
            </Button>
            <Button variant="secondary" disabled={!job.output_path} onClick={() => void onOpenFolder(job.output_path)}>
              <FolderOpen className="h-4 w-4" />
              Abrir master
            </Button>
            {job.output_path ? (
              <Button variant="secondary" onClick={() => void onOpenFolder(job.output_path)}>
                <Download className="h-4 w-4" />
                Carpeta {downloadLabel(job.output_format)}
              </Button>
            ) : null}
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
        </CardContent>
      </Card>

      <section className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="grid gap-3">
          <Card>
            <CardHeader>
              <CardTitle>Feedback guardado</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-3 p-3 md:grid-cols-2">
              <BriefBlock label="Feedback" value={originalFeedback} />
              <BriefBlock label="Referencia" value={originalReference} />
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Metadata</CardTitle>
              <span className="text-xs text-muted-foreground">{outputFormatLabel(job.output_format)}</span>
            </CardHeader>
            <CardContent className="grid gap-3 p-3">
              <div className="grid gap-2 md:grid-cols-2">
                <MetadataLine label="Titulo" value={job.metadata?.title} />
                <MetadataLine label="Artista" value={job.metadata?.artist} />
                <MetadataLine label="Album" value={job.metadata?.album} />
                <MetadataLine label="Genero" value={job.metadata?.genre} />
                <MetadataLine label="Ano" value={job.metadata?.year} />
                <MetadataLine label="BPM" value={job.metadata?.bpm} />
                <MetadataLine label="Key" value={job.metadata?.musical_key} />
                <MetadataLine label="ISRC" value={job.metadata?.isrc} />
              </div>
              {job.cover_art_path ? (
                <div className="grid grid-cols-[56px_minmax(0,1fr)] gap-2 rounded-md border border-border p-2">
                  <img className="h-14 w-14 rounded-sm object-cover" src={convertFileSrc(job.cover_art_path)} alt="" />
                  <div className="min-w-0">
                    <span className="block text-xs font-semibold">Cover</span>
                    <span className="mt-1 block truncate text-xs text-muted-foreground" title={job.cover_art_path}>{job.cover_art_path}</span>
                  </div>
                </div>
              ) : null}
              {job.package_report && Object.keys(job.package_report).length > 0 ? (
                <div className="grid gap-2 rounded-md bg-secondary p-3 text-xs">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-semibold">Packaging</span>
                    <span className="text-muted-foreground">
                      {stringValue(getNested(job.package_report, ["validation", "tag_count"]), "0")} tag(s)
                    </span>
                  </div>
                  <div className="text-muted-foreground">
                    Cover: {job.package_report.cover_embedded ? "embebido" : job.package_report.cover_requested ? "omitido" : "sin cover"}
                  </div>
                  {packageWarnings.map((warning, index) => (
                    <div key={`${String(warning)}-${index}`} className="text-yellow-800 dark:text-yellow-200">
                      {String(warning)}
                    </div>
                  ))}
                </div>
              ) : null}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Diagnostico</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-3 p-3">
              <p className="text-sm text-muted-foreground">{stringValue(diagnosis.summary_es, "Sin diagnostico todavia.")}</p>
              {stringValue(feedback.summary_es, "") ? (
                <div className="rounded-md bg-secondary p-3 text-sm">
                  <span className="block text-xs font-semibold text-muted-foreground">Feedback interpretado</span>
                  <span className="mt-1 block">{stringValue(feedback.summary_es, "")}</span>
                </div>
              ) : null}
              <div className="grid gap-2 md:grid-cols-3">
                <MiniMetric label="Riesgo" value={stringValue(diagnosis.risk_level, "n/d")} />
                <MiniMetric label="Target LUFS" value={stringValue(target.target_lufs, "n/d")} />
                <MiniMetric label="True peak" value={`${stringValue(target.true_peak_ceiling_db, "n/d")} dB`} />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Cadena aplicada</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-2 p-3">
              {chain.length === 0 ? (
                <div className="text-sm text-muted-foreground">La cadena aparece despues de generar la receta.</div>
              ) : null}
              {chain.map((stage, index) => {
                const item = stage as Record<string, unknown>;
                return (
                  <div key={`${stringValue(item.type, "stage")}-${index}`} className="rounded-md border border-border p-3">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-semibold">{stringValue(item.type, "stage")}</span>
                      <span className={cn("rounded-full px-2 py-0.5 text-[11px] font-semibold", item.enabled ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200" : "bg-secondary text-muted-foreground")}>
                        {item.enabled ? "activo" : "omitido"}
                      </span>
                    </div>
                    <p className="mt-2 text-xs leading-relaxed text-muted-foreground">{stringValue(item.reason_es, "")}</p>
                  </div>
                );
              })}
            </CardContent>
          </Card>
        </div>

        <div className="grid gap-3">
          <MetricsCard title="Antes" analysis={job.analysis_before} />
          <MetricsCard title="Despues" analysis={job.analysis_after} />
          {warnings.length > 0 ? (
            <Card className="border-yellow-500/40 bg-yellow-500/10">
              <CardHeader>
                <CardTitle>Advertencias</CardTitle>
              </CardHeader>
              <CardContent className="grid gap-2 p-3 text-sm text-yellow-900 dark:text-yellow-100">
                {warnings.map((warning, index) => (
                  <div key={`${String(warning)}-${index}`}>{String(warning)}</div>
                ))}
              </CardContent>
            </Card>
          ) : null}
        </div>
      </section>

      <Card>
        <CardHeader>
          <CardTitle>Eventos</CardTitle>
          <span className="text-xs text-muted-foreground">
            {eventsLoading ? "Cargando..." : `${timeline.length} eventos`}
          </span>
        </CardHeader>
        <CardContent className="max-h-72 p-0">
          {timeline.length === 0 && !eventsLoading ? (
            <div className="px-3 py-4 text-sm text-muted-foreground">Sin eventos guardados para este master.</div>
          ) : null}
          {timeline.map((event) => (
            <div key={event.key} className="grid grid-cols-[84px_92px_minmax(0,1fr)] gap-2 border-b border-border px-3 py-2 text-xs">
              <span className="text-muted-foreground">{formatTime(event.timestamp)}</span>
              <span className="font-semibold">{event.step}</span>
              <span className="min-w-0 truncate" title={event.message}>{event.message}</span>
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function AudioPanel({ title, path }: { title: string; path?: string | null }) {
  return (
    <div className="rounded-md border border-border bg-background/60 p-3">
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
        {path ? <Play className="h-4 w-4" /> : <AudioLines className="h-4 w-4" />}
        {title}
      </div>
      {path ? (
        <>
          <audio className="w-full" controls src={convertFileSrc(path)} />
          <div className="mt-2 truncate text-xs text-muted-foreground" title={path}>{path}</div>
        </>
      ) : (
        <div className="text-sm text-muted-foreground">No disponible.</div>
      )}
    </div>
  );
}

function MetadataInput({
  label,
  value,
  onChange
}: {
  label: string;
  value?: string | null;
  onChange: (value: string) => void;
}) {
  return (
    <label className="grid gap-1 text-xs font-semibold text-muted-foreground">
      {label}
      <input
        className="h-9 rounded-md border border-input bg-background px-2 text-sm font-normal text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
        value={value ?? ""}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
    </label>
  );
}

function MetadataLine({ label, value }: { label: string; value?: unknown }) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border/60 py-1.5 last:border-b-0">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-xs font-semibold">{stringValue(value, "n/d")}</span>
    </div>
  );
}

function BriefBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-background/60 p-3">
      <span className="block text-xs font-semibold text-muted-foreground">{label}</span>
      <p className="mt-2 whitespace-pre-wrap break-words text-sm leading-relaxed">
        {value || "No registrado."}
      </p>
    </div>
  );
}

function MetricsCard({ title, analysis }: { title: string; analysis: Record<string, unknown> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-1 p-3">
        <MetricLine label="LUFS" value={analysis.integrated_lufs} />
        <MetricLine label="True peak" value={analysis.true_peak_dbfs} suffix=" dB" />
        <MetricLine label="Sample peak" value={analysis.sample_peak_dbfs} suffix=" dB" />
        <MetricLine label="Crest" value={analysis.crest_factor_db} suffix=" dB" />
        <MetricLine label="Clipping" value={analysis.clipping_detected} />
      </CardContent>
    </Card>
  );
}

function MetricLine({ label, value, suffix = "" }: { label: string; value: unknown; suffix?: string }) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border/60 py-1.5 last:border-b-0">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs font-semibold">{stringValue(value, "n/d")}{value === undefined || value === null || value === "" ? "" : suffix}</span>
    </div>
  );
}

function MiniMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md bg-secondary p-3">
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block text-sm">{value}</strong>
    </div>
  );
}

function HistoryChip({ active, children }: { active: boolean; children: ReactNode }) {
  return (
    <span
      className={cn(
        "rounded-full border px-2 py-0.5 text-[11px] font-semibold",
        active
          ? "border-primary/20 bg-primary/10 text-foreground"
          : "border-border bg-secondary text-muted-foreground"
      )}
    >
      {children}
    </span>
  );
}

function StatusPill({ state }: { state: MasteringJob["state"] }) {
  const { t } = useI18n();
  const labels: Record<MasteringJob["state"], string> = {
    pending: "Pendiente",
    running: "Procesando",
    completed: "Listo",
    failed: "Error"
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold",
        state === "completed" && "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200",
        state === "failed" && "bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-200",
        (state === "pending" || state === "running") && "bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-200"
      )}
    >
      {state === "completed" ? <CheckCircle2 className="h-3 w-3" /> : null}
      {state === "running" || state === "pending" ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
      {state === "failed" ? <AlertTriangle className="h-3 w-3" /> : null}
      {t(labels[state])}
    </span>
  );
}

function eventToTimelineEvent(event: MasteringProgressEvent): TimelineEvent {
  return {
    ...event,
    key: event.id || `${event.timestamp}-${event.job_id}-${event.event}-${event.step}`
  };
}

function mergeTimelineEvents(current: TimelineEvent[], events: MasteringProgressEvent[]) {
  const byKey = new Map(current.map((event) => [event.key, event]));

  for (const event of events) {
    const timelineEvent = eventToTimelineEvent(event);
    byKey.set(timelineEvent.key, timelineEvent);
  }

  return Array.from(byKey.values())
    .sort((left, right) => left.timestamp.localeCompare(right.timestamp))
    .slice(-500);
}

function eventToTerminalLog(event: MasteringProgressEvent, id: number): MasteringTerminalLog {
  const progressPrefix = typeof event.progress === "number" ? `[${Math.round(event.progress)}%] ` : "";

  return {
    id,
    time: formatTime(event.timestamp),
    level: event.level,
    track_id: event.job_id,
    name: event.job.source_name || event.step,
    message: `${progressPrefix}${event.step}: ${event.message}`
  };
}

function upsertJob(jobs: MasteringJob[], job: MasteringJob) {
  const next = jobs.filter((item) => item.id !== job.id);
  next.unshift(job);
  return next.sort((left, right) => right.created_at.localeCompare(left.created_at));
}

function normalizeOutputFormat(value?: string | null): MasteringOutputFormat {
  if (value === "aiff_cdj16") return "aiff_cdj16";
  if (value === "wav_24") return "wav_24";
  return "aiff_24";
}

function normalizeEditableOutputFormat(value?: string | null): MasteringOutputFormat {
  return value === "aiff_cdj16" ? "aiff_cdj16" : "aiff_24";
}

function outputFormatLabel(value?: string | null) {
  const outputFormat = normalizeOutputFormat(value);
  if (outputFormat === "aiff_cdj16") return "AIFF CDJ safe";
  if (outputFormat === "wav_24") return "WAV 24-bit";
  return "AIFF 24-bit";
}

function downloadLabel(value?: string | null) {
  return normalizeOutputFormat(value) === "wav_24" ? "WAV" : "AIFF";
}

function normalizedMetadataForSubmit(metadata: MasteringMetadata, sourcePath: string): MasteringMetadata {
  return {
    title: cleanMetadataValue(metadata.title) || fileStem(sourcePath),
    artist: cleanMetadataValue(metadata.artist),
    album: cleanMetadataValue(metadata.album),
    genre: cleanMetadataValue(metadata.genre),
    year: cleanMetadataValue(metadata.year),
    track_number: cleanMetadataValue(metadata.track_number),
    composer: cleanMetadataValue(metadata.composer),
    label: cleanMetadataValue(metadata.label),
    copyright: cleanMetadataValue(metadata.copyright),
    bpm: cleanMetadataValue(metadata.bpm),
    musical_key: cleanMetadataValue(metadata.musical_key),
    isrc: cleanMetadataValue(metadata.isrc),
    comment: cleanMetadataValue(metadata.comment)
  };
}

function cleanMetadataValue(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function fileStem(path: string) {
  const filename = path.split(/[\\/]/).pop() || "track";
  const index = filename.lastIndexOf(".");
  return index > 0 ? filename.slice(0, index) : filename;
}

function getObject(value: Record<string, unknown>, key: string): Record<string, unknown> {
  const child = value[key];
  return child && typeof child === "object" && !Array.isArray(child) ? child as Record<string, unknown> : {};
}

function getNested(value: Record<string, unknown>, keys: string[]) {
  let current: unknown = value;
  for (const key of keys) {
    if (!current || typeof current !== "object" || Array.isArray(current)) return undefined;
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function stringValue(value: unknown, fallback: string) {
  if (value === null || value === undefined || value === "") return fallback;
  if (typeof value === "number") return Number.isInteger(value) ? String(value) : String(Math.round(value * 100) / 100);
  if (typeof value === "boolean") return String(value);
  return String(value);
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Sin fecha";
  return date.toLocaleDateString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function formatTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}
