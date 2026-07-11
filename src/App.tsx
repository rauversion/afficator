import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ChevronRight,
  ClipboardList,
  Clock3,
  Copy,
  Database,
  Disc3,
  Download,
  FileOutput,
  FileAudio2,
  FolderOpen,
  Gauge,
  HardDrive,
  Info,
  KeyRound,
  Monitor,
  Moon,
  MoreHorizontal,
  Pause,
  Play,
  RefreshCcw,
  Settings,
  Square,
  Sun,
  Trash2,
  Upload,
  X
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { HashRouter, Navigate, NavLink, Outlet, Route, Routes, useOutletContext } from "react-router-dom";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "./components/ui/dropdown-menu";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { cn } from "./lib/utils";
import { FileConversionPage } from "./FileConversionPage";
import { MasteringPage } from "./MasteringPage";
import { TurnPage } from "./TurnPage";
import type * as React from "react";

type Issue = {
  severity: "info" | "warning" | "error";
  code: string;
  track_id?: string;
  playlist_path?: string;
  source_path?: string;
  message: string;
};

type Playlist = {
  path: string;
  name: string;
  node_type?: string;
  track_count: number;
  child_count: number;
  track_keys: string[];
};

type Validation = {
  tracks_total: number;
  playlists_total: number;
  convert_candidates: number;
  already_aiff: number;
  missing_files: number;
  unreadable_files: number;
  unsupported_tracks: number;
  duplicate_sources: number;
  playlist_reference_errors: number;
  format_counts: Record<string, number>;
  issues: Issue[];
};

type ImportResponse = {
  playlists: Playlist[];
  validation: Validation;
};

type PlanItem = {
  track_id: string;
  name?: string;
  artist?: string;
  kind?: string;
  source_path?: string;
  target_path?: string;
  action: "convert" | "reuse_existing" | "skip_already_aiff" | "blocked";
  issues: Issue[];
};

type Plan = {
  playlists_total: number;
  referenced_tracks_total: number;
  unique_tracks_total: number;
  convert_total: number;
  reuse_existing_total: number;
  skipped_total: number;
  blocked_total: number;
  items: PlanItem[];
  issues: Issue[];
};

type ConvertedFile = {
  track_id: string;
  name?: string;
  artist?: string;
  kind?: string;
  source_path: string;
  target_path: string;
  source_exists: boolean;
  target_exists: boolean;
};

type AudioFolderResponse = {
  root_path: string;
  recursive: boolean;
  files: AudioFile[];
  skipped_errors: string[];
};

type AudioFile = {
  name: string;
  extension: string;
  path: string;
  parent_path: string;
  size_bytes: number;
  modified_ms?: number;
};

type PlaylistTrackFile = {
  position: number;
  track_id: string;
  name?: string;
  artist?: string;
  album?: string;
  kind?: string;
  location?: string;
  size?: number;
  total_time?: number;
  sample_rate?: number;
  bitrate?: number;
  attributes?: Record<string, string>;
  source_path?: string;
  source_exists: boolean;
  target_path?: string;
  target_exists: boolean;
};

type ConversionStatus =
  | "queued"
  | "running"
  | "converted"
  | "already_converted"
  | "already_aiff"
  | "failed";

type ConversionProgressEvent = {
  track_id: string;
  name?: string;
  source_path?: string;
  target_path?: string;
  status: ConversionStatus;
  message?: string;
  percent?: number;
  elapsed_seconds?: number;
  speed?: string;
};

type ConversionLogEvent = {
  level: "info" | "warning" | "error";
  track_id?: string;
  name?: string;
  message: string;
};

type TerminalLog = TerminalLogEntry;

type ConversionItemResult = {
  track_id: string;
  name?: string;
  artist?: string;
  source_path?: string;
  target_path?: string;
  status: ConversionStatus;
  message?: string;
};

type ConversionBatchResult = {
  items: ConversionItemResult[];
  converted_total: number;
  already_converted_total: number;
  already_aiff_total: number;
  failed_total: number;
};

type ExportXmlResult = {
  output_path: string;
  selected_playlist_total: number;
  selected_track_total: number;
  replaced_track_total: number;
};

type PlayerState = {
  label: string;
  path: string;
  url: string;
};

type DetailTab = "playlist" | "converted" | "plan" | "report";

type AppShellContext = {
  darkMode: boolean;
  setDarkMode: React.Dispatch<React.SetStateAction<boolean>>;
};

type OpenAiApiKeyStatus = {
  configured: boolean;
  preview?: string | null;
};

const maxConcurrencyLimit = 4;
const themeModeKey = "aifficator.themeMode";
const savedXmlPathKey = "aifficator.savedXmlPath";
const recentXmlPathsKey = "aifficator.recentXmlPaths";

function detectLogicalCores() {
  if (typeof navigator === "undefined") return 1;
  const cores = navigator.hardwareConcurrency;
  return Number.isFinite(cores) && cores > 0 ? Math.floor(cores) : 1;
}

function recommendedConcurrencyForCores(cores: number) {
  return Math.max(1, Math.min(maxConcurrencyLimit, Math.floor(cores / 2) || 1));
}

function concurrencyOptionsForCores(cores: number) {
  const max = Math.max(1, Math.min(maxConcurrencyLimit, cores));
  return Array.from({ length: max }, (_, index) => index + 1);
}

function detectInitialDarkMode() {
  if (typeof window === "undefined") return false;
  const savedMode = localStorage.getItem(themeModeKey);
  if (savedMode === "dark") return true;
  if (savedMode === "light") return false;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route path="/" element={<Navigate to="/file-conversion/rekordbox-convert" replace />} />
          <Route path="/rekordbox-convert" element={<Navigate to="/file-conversion/rekordbox-convert" replace />} />
          <Route path="/file-conversion" element={<Navigate to="/file-conversion/rekordbox-convert" replace />} />
          <Route path="/file-conversion/local" element={<FileConversionPage />} />
          <Route path="/file-conversion/rekordbox-convert" element={<RekordboxConvertPage />} />
          <Route path="/turn" element={<TurnPage />} />
          <Route path="/mastering" element={<MasteringPage />} />
          <Route
            path="/settings"
            element={<SettingsPage />}
          />
          <Route path="*" element={<Navigate to="/file-conversion/rekordbox-convert" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}

function AppShell() {
  const [darkMode, setDarkMode] = useState(() => detectInitialDarkMode());
  const shellContext = useMemo<AppShellContext>(() => ({ darkMode, setDarkMode }), [darkMode]);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", darkMode);
    localStorage.setItem(themeModeKey, darkMode ? "dark" : "light");
  }, [darkMode]);

  return (
    <div className="min-h-screen bg-background text-foreground">
      <div className="flex min-h-screen max-lg:flex-col">
        <AppSidebar />
        <div className="min-w-0 flex-1">
          <Outlet context={shellContext} />
        </div>
      </div>
    </div>
  );
}

function PlaceholderPage({
  icon,
  title,
  description
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
}) {
  return (
    <main className="min-w-0 p-4 pb-20">
      <header className="mb-3 flex items-center gap-3 border-b border-border pb-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
          {icon}
        </span>
        <div className="min-w-0">
          <h1 className="m-0 text-2xl font-semibold tracking-normal">{title}</h1>
          <p className="mt-1 text-xs text-muted-foreground">{description}</p>
        </div>
      </header>

      <Card className="p-6">
        <CardTitle>{title}</CardTitle>
        <p className="mt-2 max-w-xl text-sm text-muted-foreground">
          Esta seccion ya esta registrada en el router y lista para recibir su flujo.
        </p>
      </Card>
    </main>
  );
}

function SettingsPage() {
  const { darkMode, setDarkMode } = useOutletContext<AppShellContext>();
  const [apiKey, setApiKey] = useState("");
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const [apiKeyStatus, setApiKeyStatus] = useState<OpenAiApiKeyStatus | null>(null);
  const [loadingApiKey, setLoadingApiKey] = useState(true);
  const [savingApiKey, setSavingApiKey] = useState(false);
  const [settingsMessage, setSettingsMessage] = useState("");
  const [settingsError, setSettingsError] = useState("");

  useEffect(() => {
    void loadApiKeyStatus();
  }, []);

  async function loadApiKeyStatus() {
    setLoadingApiKey(true);
    setSettingsError("");

    try {
      const status = await invoke<OpenAiApiKeyStatus>("get_openai_api_key_status");
      setApiKeyStatus(status);
    } catch (error) {
      setSettingsError(String(error));
    } finally {
      setLoadingApiKey(false);
    }
  }

  async function saveApiKey(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSavingApiKey(true);
    setSettingsMessage("");
    setSettingsError("");

    try {
      const status = await invoke<OpenAiApiKeyStatus>("save_openai_api_key", { apiKey });
      setApiKeyStatus(status);
      setApiKey("");
      setApiKeyVisible(false);
      setSettingsMessage("OpenAI API key guardada.");
    } catch (error) {
      setSettingsError(String(error));
    } finally {
      setSavingApiKey(false);
    }
  }

  async function clearApiKey() {
    setSavingApiKey(true);
    setSettingsMessage("");
    setSettingsError("");

    try {
      const status = await invoke<OpenAiApiKeyStatus>("clear_openai_api_key");
      setApiKeyStatus(status);
      setApiKey("");
      setSettingsMessage("OpenAI API key eliminada.");
    } catch (error) {
      setSettingsError(String(error));
    } finally {
      setSavingApiKey(false);
    }
  }

  return (
    <main className="min-w-0 p-4 pb-20">
      <header className="mb-3 flex items-center gap-3 border-b border-border pb-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
          <Settings className="h-5 w-5" />
        </span>
        <div className="min-w-0">
          <h1 className="m-0 text-2xl font-semibold tracking-normal">Settings</h1>
          <p className="mt-1 text-xs text-muted-foreground">Preferencias generales de Aifficator.</p>
        </div>
      </header>

      <section className="grid max-w-3xl gap-3">
        {settingsError ? (
          <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
            {settingsError}
          </div>
        ) : null}
        {settingsMessage ? (
          <div className="rounded-md border border-border bg-muted px-3 py-2 text-sm text-foreground">
            {settingsMessage}
          </div>
        ) : null}

        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Monitor className="h-4 w-4" />
              <CardTitle>Apariencia</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <div className="inline-flex rounded-md border border-border bg-secondary p-1">
              <Button
                type="button"
                variant={!darkMode ? "default" : "ghost"}
                size="sm"
                onClick={() => setDarkMode(false)}
              >
                <Sun className="h-4 w-4" />
                Claro
              </Button>
              <Button
                type="button"
                variant={darkMode ? "default" : "ghost"}
                size="sm"
                onClick={() => setDarkMode(true)}
              >
                <Moon className="h-4 w-4" />
                Oscuro
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <KeyRound className="h-4 w-4" />
              <CardTitle>OpenAI API key</CardTitle>
            </div>
            <span className="text-xs text-muted-foreground">
              {loadingApiKey
                ? "Revisando estado..."
                : apiKeyStatus?.configured
                  ? `Guardada: ${apiKeyStatus.preview}`
                  : "No configurada"}
            </span>
          </CardHeader>
          <CardContent>
            <form className="grid gap-3" onSubmit={saveApiKey}>
              <label className="grid gap-1 text-sm font-medium">
                API key
                <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2">
                  <input
                    className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                    type={apiKeyVisible ? "text" : "password"}
                    value={apiKey}
                    autoComplete="off"
                    placeholder="sk-..."
                    onChange={(event) => setApiKey(event.currentTarget.value)}
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setApiKeyVisible((current) => !current)}
                  >
                    {apiKeyVisible ? "Ocultar" : "Mostrar"}
                  </Button>
                </div>
              </label>

              <div className="flex flex-wrap items-center gap-2">
                <Button type="submit" disabled={savingApiKey || apiKey.trim().length === 0}>
                  Guardar key
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  disabled={savingApiKey || loadingApiKey || !apiKeyStatus?.configured}
                  onClick={() => void clearApiKey()}
                >
                  Eliminar key
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={savingApiKey || loadingApiKey}
                  onClick={() => void loadApiKeyStatus()}
                >
                  Refrescar
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      </section>
    </main>
  );
}

function RekordboxConvertPage() {
  const { darkMode, setDarkMode } = useOutletContext<AppShellContext>();
  const [detectedLogicalCores] = useState(() => detectLogicalCores());
  const [xmlPath, setXmlPath] = useState("");
  const [recentXmlPaths, setRecentXmlPaths] = useState<string[]>([]);
  const [importResult, setImportResult] = useState<ImportResponse | null>(null);
  const [convertedFiles, setConvertedFiles] = useState<ConvertedFile[]>([]);
  const [folderPath, setFolderPath] = useState("");
  const [folderRecursive, setFolderRecursive] = useState(true);
  const [audioFiles, setAudioFiles] = useState<AudioFile[]>([]);
  const [folderSkippedErrors, setFolderSkippedErrors] = useState<string[]>([]);
  const [activePlaylistPath, setActivePlaylistPath] = useState("");
  const [playlistFiles, setPlaylistFiles] = useState<PlaylistTrackFile[]>([]);
  const [playlistLoading, setPlaylistLoading] = useState(false);
  const [selectedPlaylists, setSelectedPlaylists] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<Plan | null>(null);
  const [conversionProgress, setConversionProgress] = useState<Map<string, ConversionProgressEvent>>(new Map());
  const [conversionResults, setConversionResults] = useState<ConversionItemResult[]>([]);
  const [conversionQueue, setConversionQueue] = useState<string[]>([]);
  const [conversionBusy, setConversionBusy] = useState(false);
  const [conversionMessage, setConversionMessage] = useState("");
  const [maxConcurrency, setMaxConcurrency] = useState(() =>
    recommendedConcurrencyForCores(detectLogicalCores())
  );
  const [terminalLogs, setTerminalLogs] = useState<TerminalLog[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [activeDetailTab, setActiveDetailTab] = useState<DetailTab>("playlist");
  const [selectedTrackFile, setSelectedTrackFile] = useState<PlaylistTrackFile | null>(null);
  const [metadataSheetOpen, setMetadataSheetOpen] = useState(false);
  const [player, setPlayer] = useState<PlayerState | null>(null);
  const [playerPlaying, setPlayerPlaying] = useState(false);
  const [playerCurrentTime, setPlayerCurrentTime] = useState(0);
  const [playerDuration, setPlayerDuration] = useState(0);
  const [busy, setBusy] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  const audioElement = useRef<HTMLAudioElement | null>(null);
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);
  const terminalProgressBuckets = useRef(new Map<string, number>());
  const userSelectedConcurrency = useRef(false);
  const conversionQueueRef = useRef<string[]>([]);
  const conversionBusyRef = useRef(false);
  const conversionProgressRef = useRef(conversionProgress);
  const xmlPathRef = useRef(xmlPath);
  const activePlaylistPathRef = useRef(activePlaylistPath);
  const planRef = useRef(plan);
  const maxConcurrencyRef = useRef(maxConcurrency);

  useEffect(() => {
    conversionProgressRef.current = conversionProgress;
  }, [conversionProgress]);

  useEffect(() => {
    xmlPathRef.current = xmlPath;
  }, [xmlPath]);

  useEffect(() => {
    activePlaylistPathRef.current = activePlaylistPath;
  }, [activePlaylistPath]);

  useEffect(() => {
    planRef.current = plan;
  }, [plan]);

  useEffect(() => {
    maxConcurrencyRef.current = maxConcurrency;
  }, [maxConcurrency]);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const recent = readRecentXmlPaths();
    setRecentXmlPaths(recent);

    listen<ConversionProgressEvent>("conversion-progress", (event) => {
      setConversionProgress((current) => {
        const next = new Map(current);
        next.set(event.payload.track_id, event.payload);
        conversionProgressRef.current = next;
        return next;
      });
      logProgressMilestone(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<ConversionLogEvent>("conversion-log", (event) => {
      appendTerminalLog(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));

    const savedXmlPath = localStorage.getItem(savedXmlPathKey);
    if (savedXmlPath) {
      setXmlPath(savedXmlPath);
      void importXml(savedXmlPath);
    }

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  const playlistRows = useMemo(
    () => importResult?.playlists.filter((playlist) => playlist.node_type === "1") ?? [],
    [importResult]
  );
  const validation = importResult?.validation;
  const sortedIssues = validation?.issues ?? [];
  const plannedRows = plan?.items ?? [];
  const activePlaylist = playlistRows.find((playlist) => playlist.path === activePlaylistPath);
  const allPlaylistsSelected = playlistRows.length > 0 && selectedPlaylists.size === playlistRows.length;
  const somePlaylistsSelected = selectedPlaylists.size > 0 && !allPlaylistsSelected;
  const recommendedConcurrency = useMemo(
    () => recommendedConcurrencyForCores(detectedLogicalCores),
    [detectedLogicalCores]
  );
  const concurrencyOptions = useMemo(
    () => concurrencyOptionsForCores(detectedLogicalCores),
    [detectedLogicalCores]
  );

  useEffect(() => {
    if (!userSelectedConcurrency.current) {
      setMaxConcurrency(recommendedConcurrency);
      maxConcurrencyRef.current = recommendedConcurrency;
    }
  }, [recommendedConcurrency]);

  const playerProgress =
    playerDuration > 0 ? Math.min(100, (playerCurrentTime / playerDuration) * 100) : 0;
  const activeConvertibleTrackIds = playlistFiles.filter(canConvertPlaylistFile).map((file) => file.track_id);
  const processingTrackIds = useMemo(() => {
    const trackIds = new Set<string>();

    for (const [trackId, progress] of conversionProgress) {
      if (progress.status === "queued" || progress.status === "running") {
        trackIds.add(trackId);
      }
    }

    return trackIds;
  }, [conversionProgress]);
  const playlistProcessingCounts = useMemo(() => {
    const counts = new Map<string, number>();

    for (const playlist of playlistRows) {
      const processingCount = playlist.track_keys.reduce(
        (total, trackId) => total + (processingTrackIds.has(trackId) ? 1 : 0),
        0
      );
      counts.set(playlist.path, processingCount);
    }

    return counts;
  }, [playlistRows, processingTrackIds]);
  const convertedTrackIds = useMemo(() => {
    const trackIds = new Set(convertedFiles.map((file) => file.track_id));

    for (const [trackId, progress] of conversionProgress) {
      if (progress.status === "converted" || progress.status === "already_converted") {
        trackIds.add(trackId);
      }
    }

    for (const result of conversionResults) {
      if (result.status === "converted" || result.status === "already_converted") {
        trackIds.add(result.track_id);
      }
    }

    return trackIds;
  }, [convertedFiles, conversionProgress, conversionResults]);
  const playlistConvertedCounts = useMemo(() => {
    const counts = new Map<string, number>();

    for (const playlist of playlistRows) {
      const convertedCount = playlist.track_keys.reduce(
        (total, trackId) => total + (convertedTrackIds.has(trackId) ? 1 : 0),
        0
      );
      counts.set(playlist.path, convertedCount);
    }

    return counts;
  }, [playlistRows, convertedTrackIds]);
  const selectedPlaylistRows = useMemo(
    () => playlistRows.filter((playlist) => selectedPlaylists.has(playlist.path)),
    [playlistRows, selectedPlaylists]
  );
  const selectedPlaylistPendingTrackIds = useMemo(() => {
    if (selectedPlaylistRows.length === 0) return [];

    const seen = new Set<string>();
    const trackIds: string[] = [];

    for (const playlist of selectedPlaylistRows) {
      for (const trackId of playlist.track_keys) {
        if (seen.has(trackId) || convertedTrackIds.has(trackId) || processingTrackIds.has(trackId)) {
          continue;
        }

        seen.add(trackId);
        trackIds.push(trackId);
      }
    }

    return trackIds;
  }, [convertedTrackIds, processingTrackIds, selectedPlaylistRows]);
  const dynamicStats = validation
    ? {
        converted: convertedTrackIds.size,
        pending: Math.max(0, validation.convert_candidates - convertedTrackIds.size)
      }
    : null;
  const metadataTrack = selectedTrackFile
    ? playlistFiles.find(
        (file) =>
          file.track_id === selectedTrackFile.track_id &&
          file.position === selectedTrackFile.position
      ) ?? selectedTrackFile
    : null;

  async function chooseXml() {
    setErrorMessage("");
    setPlan(null);
    setPlayer(null);

    const selected = await open({
      multiple: false,
      filters: [{ name: "Rekordbox XML", extensions: ["xml"] }]
    });

    if (typeof selected !== "string") return;

    setXmlPath(selected);
    rememberXmlPath(selected);
    await importXml(selected);
  }

  async function importXml(path = xmlPathRef.current) {
    if (!path) return;

    setBusy(true);
    setErrorMessage("");
    setPlan(null);
    setPlayer(null);
    setConvertedFiles([]);
    setConversionProgress(new Map());
    conversionProgressRef.current = new Map();
    setConversionResults([]);
    conversionQueueRef.current = [];
    setConversionQueue([]);
    conversionBusyRef.current = false;
    setConversionBusy(false);
    setConversionMessage("");
    setTerminalLogs([]);
    terminalProgressBuckets.current = new Map();
    setActivePlaylistPath("");
    setPlaylistFiles([]);
    setSelectedPlaylists(new Set());
    setActiveDetailTab("playlist");
    setSelectedTrackFile(null);
    setMetadataSheetOpen(false);

    try {
      const response = await invoke<ImportResponse>("import_rekordbox_xml", { path });
      setImportResult(response);
      const firstPlaylist = response.playlists.find((playlist) => playlist.node_type === "1");
      if (firstPlaylist) {
        await selectPlaylist(firstPlaylist.path, path);
      }
      await refreshConvertedFiles(path);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function loadRecentXml(path: string) {
    setXmlPath(path);
    rememberXmlPath(path);
    await importXml(path);
  }

  function forgetSavedXml() {
    localStorage.removeItem(savedXmlPathKey);
    setXmlPath("");
    xmlPathRef.current = "";
    setImportResult(null);
    setConvertedFiles([]);
    setConversionProgress(new Map());
    conversionProgressRef.current = new Map();
    setConversionResults([]);
    conversionQueueRef.current = [];
    setConversionQueue([]);
    conversionBusyRef.current = false;
    setConversionBusy(false);
    setConversionMessage("");
    setTerminalLogs([]);
    terminalProgressBuckets.current = new Map();
    setActivePlaylistPath("");
    setPlaylistFiles([]);
    setSelectedPlaylists(new Set());
    setPlan(null);
    setActiveDetailTab("playlist");
    setSelectedTrackFile(null);
    setMetadataSheetOpen(false);
  }

  function rememberXmlPath(path: string) {
    localStorage.setItem(savedXmlPathKey, path);
    setRecentXmlPaths((current) => {
      const next = [path, ...current.filter((recentPath) => recentPath !== path)].slice(0, 8);
      localStorage.setItem(recentXmlPathsKey, JSON.stringify(next));
      return next;
    });
  }

  function readRecentXmlPaths() {
    const raw = localStorage.getItem(recentXmlPathsKey);
    if (!raw) return [];

    try {
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed)
        ? parsed.filter((path): path is string => typeof path === "string")
        : [];
    } catch {
      return [];
    }
  }

  async function chooseFolder() {
    setErrorMessage("");

    const selected = await open({
      multiple: false,
      directory: true
    });

    if (typeof selected !== "string") return;

    setFolderPath(selected);
    await refreshAudioFiles(selected);
  }

  function clearFolderExplorer() {
    setErrorMessage("");
    setFolderPath("");
    setAudioFiles([]);
    setFolderSkippedErrors([]);
  }

  async function refreshAudioFiles(path = folderPath, recursive = folderRecursive) {
    if (!path) return;

    setBusy(true);
    setErrorMessage("");

    try {
      const response = await invoke<AudioFolderResponse>("list_audio_files", {
        folderPath: path,
        recursive
      });
      setFolderPath(response.root_path);
      setAudioFiles(response.files);
      setFolderSkippedErrors(response.skipped_errors);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  function togglePlaylist(path: string) {
    setSelectedPlaylists((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  function toggleAllPlaylists() {
    setSelectedPlaylists(() => {
      if (allPlaylistsSelected) return new Set();
      return new Set(playlistRows.map((playlist) => playlist.path));
    });
  }

  async function selectPlaylist(path: string, xml = xmlPathRef.current) {
    if (!xml) return;

    setActivePlaylistPath(path);
    activePlaylistPathRef.current = path;
    setActiveDetailTab("playlist");
    setSelectedTrackFile(null);
    setMetadataSheetOpen(false);
    setPlaylistLoading(true);
    setErrorMessage("");

    try {
      const files = await invoke<PlaylistTrackFile[]>("playlist_tracks", {
        path: xml,
        playlistPath: path
      });
      setPlaylistFiles(files);
    } catch (error) {
      setPlaylistFiles([]);
      setErrorMessage(String(error));
    } finally {
      setPlaylistLoading(false);
    }
  }

  async function createPlan() {
    const path = xmlPathRef.current;
    if (!path) return;

    setBusy(true);
    setErrorMessage("");

    try {
      const response = await invoke<Plan>("plan_conversion", {
        path,
        playlistPaths: Array.from(selectedPlaylists)
      });
      setPlan(response);
      setActiveDetailTab("plan");
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  function convertTrackIds(trackIds: string[]) {
    const path = xmlPathRef.current;
    if (!path || trackIds.length === 0) return;

    const queuedIds = new Set(conversionQueueRef.current);
    const uniqueTrackIds = Array.from(new Set(trackIds)).filter((trackId) => {
      const status = conversionProgressRef.current.get(trackId)?.status;
      return status !== "queued" && status !== "running" && !queuedIds.has(trackId);
    });
    if (uniqueTrackIds.length === 0) return;

    conversionQueueRef.current = [...conversionQueueRef.current, ...uniqueTrackIds];
    setConversionQueue([...conversionQueueRef.current]);
    setConversionMessage(
      conversionBusyRef.current
        ? `${uniqueTrackIds.length} archivo(s) agregados a la cola. ${conversionQueueRef.current.length} pendientes.`
        : `${uniqueTrackIds.length} archivo(s) en cola.`
    );
    setErrorMessage("");

    setConversionProgress((current) => {
      const next = new Map(current);
      for (const trackId of uniqueTrackIds) {
        next.set(trackId, {
          track_id: trackId,
          status: "queued",
          message: "En cola",
          percent: 0
        });
      }
      conversionProgressRef.current = next;
      return next;
    });

    void drainConversionQueue();
  }

  async function drainConversionQueue() {
    const path = xmlPathRef.current;
    if (conversionBusyRef.current || !path) return;

    conversionBusyRef.current = true;
    setConversionBusy(true);
    setErrorMessage("");

    let convertedTotal = 0;
    let alreadyConvertedTotal = 0;
    let alreadyAiffTotal = 0;
    let failedTotal = 0;

    try {
      while (conversionQueueRef.current.length > 0) {
        const max = Math.max(1, Math.min(maxConcurrencyLimit, maxConcurrencyRef.current));
        const batch = conversionQueueRef.current.slice(0, max);
        conversionQueueRef.current = conversionQueueRef.current.slice(batch.length);
        setConversionQueue([...conversionQueueRef.current]);
        setConversionMessage(
          `Convirtiendo ${batch.length} archivo(s). ${conversionQueueRef.current.length} pendientes en cola.`
        );

        const result = await invoke<ConversionBatchResult>("convert_tracks", {
          path,
          trackIds: batch,
          maxConcurrency: max
        });

        setConversionResults((current) => [...current, ...result.items]);
        convertedTotal += result.converted_total;
        alreadyConvertedTotal += result.already_converted_total;
        alreadyAiffTotal += result.already_aiff_total;
        failedTotal += result.failed_total;
        await refreshConvertedFiles(path);
      }

      setConversionMessage(
        `${convertedTotal} convertidos, ${alreadyConvertedTotal} ya existian, ${alreadyAiffTotal} ya eran AIFF, ${failedTotal} con error.`
      );
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      conversionBusyRef.current = false;
      setConversionBusy(false);
      await refreshConvertedFiles(path);
      if (activePlaylistPathRef.current) {
        await selectPlaylist(activePlaylistPathRef.current, path);
      }
      if (planRef.current) {
        await createPlan();
      }
    }
  }

  async function exportXml() {
    const path = xmlPathRef.current;
    if (!path) return;

    const playlistPaths =
      selectedPlaylists.size > 0 ? Array.from(selectedPlaylists) : activePlaylistPath ? [activePlaylistPath] : [];

    if (playlistPaths.length === 0) {
      setErrorMessage("Selecciona una playlist o haz click en una playlist activa antes de exportar.");
      return;
    }

    const outputPath = await save({
      defaultPath: defaultExportPath(path),
      filters: [{ name: "Rekordbox XML", extensions: ["xml"] }]
    });

    if (typeof outputPath !== "string") return;

    setBusy(true);
    setErrorMessage("");
    appendTerminalLog({
      level: "info",
      message: `Exportando XML a ${outputPath}`
    });

    try {
      const result = await invoke<ExportXmlResult>("export_rekordbox_xml", {
        path,
        playlistPaths,
        outputPath
      });
      setConversionMessage(
        `XML exportado: ${result.output_path}. ${result.replaced_track_total}/${result.selected_track_total} tracks apuntan a AIFF.`
      );
      appendTerminalLog({
        level: "info",
        message: `XML exportado: ${result.output_path}. ${result.replaced_track_total}/${result.selected_track_total} tracks reemplazados.`
      });
    } catch (error) {
      setErrorMessage(String(error));
      appendTerminalLog({
        level: "error",
        message: `Error exportando XML: ${String(error)}`
      });
    } finally {
      setBusy(false);
    }
  }

  function defaultExportPath(path: string) {
    return path.replace(/\.xml$/i, "") + ".aifficator.aiff.xml";
  }

  function appendTerminalLog(log: ConversionLogEvent) {
    const nextLog: TerminalLog = {
      ...log,
      id: nextTerminalLogId.current,
      time: new Date().toLocaleTimeString()
    };
    nextTerminalLogId.current += 1;

    setTerminalLogs((current) => [...current, nextLog].slice(-1000));
    window.requestAnimationFrame(() => {
      if (terminalElement.current) {
        terminalElement.current.scrollTop = terminalElement.current.scrollHeight;
      }
    });
  }

  function logProgressMilestone(progress: ConversionProgressEvent) {
    if (progress.status !== "running" || typeof progress.percent !== "number") return;

    const bucket = Math.floor(progress.percent / 10) * 10;
    if (bucket <= 0) return;

    const previousBucket = terminalProgressBuckets.current.get(progress.track_id) ?? 0;
    if (bucket <= previousBucket) return;

    terminalProgressBuckets.current.set(progress.track_id, bucket);
    appendTerminalLog({
      level: "info",
      track_id: progress.track_id,
      name: progress.name,
      message: `Progreso ${bucket}%${progress.speed ? ` (${progress.speed})` : ""}`
    });
  }

  function clearTerminal() {
    setTerminalLogs([]);
  }

  function convertActivePlaylist() {
    convertTrackIds(playlistFiles.filter(canConvertPlaylistFile).map((file) => file.track_id));
  }

  function convertSelectedPlaylists() {
    convertTrackIds(selectedPlaylistPendingTrackIds);
  }

  function selectedPlaylistsConvertLabel() {
    return `Convertir ${selectedPlaylists.size} ${selectedPlaylists.size === 1 ? "playlist" : "playlists"}`;
  }

  function changeConcurrency(value: number) {
    userSelectedConcurrency.current = true;
    setMaxConcurrency(value);
  }

  function canConvertPlaylistFile(file: PlaylistTrackFile) {
    const status = conversionProgress.get(file.track_id)?.status;
    return Boolean(
      file.source_exists &&
        file.source_path &&
        !file.target_exists &&
        status !== "queued" &&
        status !== "running"
    );
  }

  function trackProgress(trackId: string) {
    return conversionProgress.get(trackId);
  }

  function isTrackConverting(trackId: string) {
    const status = trackProgress(trackId)?.status;
    return status === "queued" || status === "running";
  }

  function isTrackConverted(file: PlaylistTrackFile) {
    const status = trackProgress(file.track_id)?.status;
    return file.target_exists || status === "converted" || status === "already_converted";
  }

  function conversionDotClass(file: PlaylistTrackFile) {
    const status = trackProgress(file.track_id)?.status;
    if (isTrackConverted(file)) return "bg-emerald-500 border-emerald-700";
    if (status === "failed" || !file.source_exists) return "bg-red-500 border-red-700";
    if (status === "queued" || status === "running") return "bg-amber-400 border-amber-700";
    return "bg-slate-200 border-slate-400";
  }

  function conversionDotTitle(file: PlaylistTrackFile) {
    const progress = trackProgress(file.track_id);
    if (isTrackConverted(file)) return "Convertido";
    if (progress?.status === "failed") return progress.message ?? "Error de conversion";
    if (progress?.status === "queued") return "En cola";
    if (progress?.status === "running") return progress.message ?? "Convirtiendo";
    if (!file.source_exists) return "Archivo original no encontrado";
    return "Pendiente";
  }

  function conversionButtonLabel(file: PlaylistTrackFile) {
    const status = trackProgress(file.track_id)?.status;
    if (file.target_exists || status === "converted" || status === "already_converted") return "DONE";
    if (status === "failed") return "ERR";
    if (status === "queued" || status === "running") return "...";
    return "CONV";
  }

  function targetLabel(file: PlaylistTrackFile) {
    const progress = trackProgress(file.track_id);
    if (file.target_exists) return file.target_path ?? "Convertido";
    if (progress?.status === "converted" || progress?.status === "already_converted") {
      return progress.target_path ?? file.target_path ?? "Convertido";
    }
    if (progress?.status === "failed") return progress.message ?? "Error";
    if (progress?.status === "queued") return "En cola";
    if (progress?.status === "running") {
      const percentLabel = typeof progress.percent === "number" ? ` ${Math.round(progress.percent)}%` : "";
      const speedLabel = progress.speed ? ` ${progress.speed}` : "";
      return `Convirtiendo${percentLabel}${speedLabel}`;
    }
    return file.target_path ? "Pendiente" : "";
  }

  function progressPercent(trackId: string) {
    const percent = trackProgress(trackId)?.percent;
    return typeof percent === "number" && Number.isFinite(percent)
      ? Math.max(0, Math.min(100, percent))
      : 0;
  }

  function runPlaylistFileAction(file: PlaylistTrackFile, action: string) {
    switch (action) {
      case "aiff":
        if (file.target_path) void togglePathPlayback(file.target_path, file.name ?? file.target_path);
        break;
      case "convert":
        convertTrackIds([file.track_id]);
        break;
      case "find":
        if (file.source_path) void reveal(file.source_path);
        break;
      case "open":
        if (file.source_path) void openFolder(file.source_path);
        break;
    }
  }

  async function refreshConvertedFiles(path = xmlPathRef.current) {
    if (!path) return;

    try {
      const files = await invoke<ConvertedFile[]>("list_converted_files", { path });
      setConvertedFiles(files);
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function playPath(path: string, label: string) {
    setPlayer({
      label,
      path,
      url: convertFileSrc(path)
    });
    setPlayerPlaying(false);
    setPlayerCurrentTime(0);
    setPlayerDuration(0);

    await new Promise((resolve) => window.requestAnimationFrame(resolve));

    try {
      audioElement.current?.load();
      await audioElement.current?.play();
      setPlayerPlaying(true);
    } catch (error) {
      setErrorMessage(`No se pudo reproducir ${label}: ${String(error)}`);
    }
  }

  async function togglePlayer() {
    if (!audioElement.current || !player) return;

    try {
      if (audioElement.current.paused) {
        await audioElement.current.play();
        setPlayerPlaying(true);
      } else {
        audioElement.current.pause();
        setPlayerPlaying(false);
      }
    } catch (error) {
      setErrorMessage(`No se pudo controlar el player: ${String(error)}`);
    }
  }

  async function togglePathPlayback(path: string, label: string) {
    if (player?.path === path && playerPlaying) {
      stopPlayer();
      return;
    }

    if (player?.path === path) {
      await togglePlayer();
      return;
    }

    await playPath(path, label);
  }

  function stopPlayer() {
    if (audioElement.current) {
      audioElement.current.pause();
      audioElement.current.currentTime = 0;
    }
    setPlayerPlaying(false);
    setPlayerCurrentTime(0);
  }

  function playbackIcon(path?: string) {
    return path && player?.path === path && playerPlaying ? "stop" : "play";
  }

  function syncPlayerTime(audio: HTMLAudioElement | null = audioElement.current) {
    if (!audio) return;
    setPlayerCurrentTime(audio.currentTime || 0);
    setPlayerDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
  }

  function finishPlayback() {
    setPlayerPlaying(false);
    syncPlayerTime();
  }

  function formatTime(seconds: number) {
    if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = Math.floor(seconds % 60).toString().padStart(2, "0");
    return `${minutes}:${remainingSeconds}`;
  }

  function formatSize(bytes: number) {
    if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${bytes} B`;
  }

  async function reveal(path: string) {
    try {
      await invoke("reveal_path", { path });
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function openFolder(path: string) {
    try {
      await invoke("open_parent_folder", { path });
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  return (
    <main className={cn("min-w-0 p-4 pb-20", terminalExpanded && "pb-72")}>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="min-w-0">
          <h1 className="m-0 text-2xl font-semibold tracking-normal">Rekordbox Convert</h1>
          <p className="mt-1 max-w-[72vw] truncate text-xs text-muted-foreground lg:max-w-[56vw]">{xmlPath || "Sin XML cargado"}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={chooseXml} disabled={busy}>
            <Upload className="h-4 w-4" />
            Importar XML
          </Button>
          <Button variant="secondary" onClick={chooseFolder} disabled={busy}>
            <FolderOpen className="h-4 w-4" />
            Explorar carpeta
          </Button>
          <CreatePlanButton
            disabled={busy || !importResult}
            selectedCount={selectedPlaylists.size}
            onClick={createPlan}
          />
          <Button onClick={exportXml} disabled={busy || conversionBusy || !importResult}>
            <FileOutput className="h-4 w-4" />
            Exportar XML
          </Button>
          <Button
            variant="secondary"
            onClick={() => setDarkMode((current) => !current)}
            title={darkMode ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
          >
            {darkMode ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
            {darkMode ? "Claro" : "Oscuro"}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="secondary"
                disabled={conversionBusy}
                className="min-w-[168px] justify-between"
                title={`${detectedLogicalCores} core(s) logico(s) detectado(s). Default recomendado: ${recommendedConcurrency}.`}
              >
                <span className="text-muted-foreground">Concurrencia</span>
                <span>{maxConcurrency}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-[190px]">
              <div className="px-2 py-1.5 text-xs leading-relaxed text-muted-foreground">
                {detectedLogicalCores} core(s) detectado(s). Default: {recommendedConcurrency}.
              </div>
              {concurrencyOptions.map((value) => (
                <DropdownMenuItem
                  key={value}
                  onSelect={() => changeConcurrency(value)}
                  className={cn(value === maxConcurrency && "bg-secondary font-semibold")}
                >
                  {value} {value === 1 ? "archivo" : "archivos"}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          {xmlPath ? (
            <Button variant="secondary" onClick={forgetSavedXml} disabled={busy}>
              <Trash2 className="h-4 w-4" />
              Olvidar XML
            </Button>
          ) : null}
        </div>
      </header>

      {recentXmlPaths.length > 0 ? (
        <Card className="mb-3 flex items-center gap-2 overflow-x-auto p-2">
          <span className="shrink-0 text-xs font-semibold text-muted-foreground">XML recientes</span>
          {recentXmlPaths.map((recentPath) => (
            <Button
              key={recentPath}
              variant={recentPath === xmlPath ? "default" : "secondary"}
              size="sm"
              className="max-w-72 truncate"
              title={recentPath}
              onClick={() => void loadRecentXml(recentPath)}
              disabled={busy}
            >
              {recentPath}
            </Button>
          ))}
        </Card>
      ) : null}

      {errorMessage ? (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}

      {conversionMessage ? (
        <div className="mb-3 rounded-md border border-border bg-muted px-3 py-2 text-sm text-foreground">
          {conversionMessage}
        </div>
      ) : null}

      {validation && dynamicStats ? (
        <section className="mb-3 grid grid-cols-2 gap-2 lg:grid-cols-4 xl:grid-cols-8">
          <Metric label="Tracks" value={validation.tracks_total} />
          <Metric label="Convertibles" value={validation.convert_candidates} />
          <Metric label="Convertidos" value={dynamicStats.converted} />
          <Metric label="Pendientes" value={dynamicStats.pending} />
          <Metric label="AIFF origen" value={validation.already_aiff} />
          <Metric label="No encontrados" value={validation.missing_files} danger={validation.missing_files > 0} />
          <Metric label="No soportados" value={validation.unsupported_tracks} danger={validation.unsupported_tracks > 0} />
          <Metric label="Refs rotas" value={validation.playlist_reference_errors} danger={validation.playlist_reference_errors > 0} />
        </section>
      ) : null}

      {plan ? (
        <section className="mb-3 grid grid-cols-2 gap-2 lg:grid-cols-6">
          <PlanMetric>{plan.playlists_total} playlists</PlanMetric>
          <PlanMetric>{plan.unique_tracks_total} tracks unicos</PlanMetric>
          <PlanMetric>{plan.convert_total} conversiones</PlanMetric>
          <PlanMetric>{plan.reuse_existing_total} reutilizados</PlanMetric>
          <PlanMetric>{plan.skipped_total} omitidos</PlanMetric>
          <PlanMetric danger={plan.blocked_total > 0}>{plan.blocked_total} bloqueados</PlanMetric>
        </section>
      ) : null}

      <Card className="mb-3 grid grid-cols-[74px_minmax(180px,320px)_minmax(220px,1fr)_84px] items-center gap-3 p-3 max-lg:grid-cols-1">
        <Button disabled={!player} onClick={() => void togglePlayer()} className="w-[74px] px-0">
          {playerPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          {playerPlaying ? "Pause" : "Play"}
        </Button>
        <div className="min-w-0">
          <span className="block text-xs text-muted-foreground">Player</span>
          <strong className="block truncate text-sm" title={player?.path ?? ""}>
            {player?.label ?? "Sin archivo cargado"}
          </strong>
        </div>
        <div className="min-w-0">
          <div className="mb-1 flex justify-between text-xs text-muted-foreground">
            <span>{formatTime(playerCurrentTime)}</span>
            <span>{formatTime(playerDuration)}</span>
          </div>
          <Progress value={playerProgress} />
        </div>
        {player ? (
          <audio
            className="hidden"
            ref={audioElement}
            src={player.url}
            onLoadedMetadata={(event) => syncPlayerTime(event.currentTarget)}
            onTimeUpdate={(event) => syncPlayerTime(event.currentTarget)}
            onPlay={() => setPlayerPlaying(true)}
            onPause={() => setPlayerPlaying(false)}
            onEnded={finishPlayback}
          />
        ) : null}
        <Button variant="secondary" disabled={!player} onClick={() => player && void reveal(player.path)}>
          Finder
        </Button>
      </Card>

      <Card className="mb-3 max-h-[38vh] min-h-[220px]">
        <CardHeader>
          <div className="min-w-0">
            <CardTitle>Originales</CardTitle>
            <span className="block truncate text-xs text-muted-foreground" title={folderPath}>
              {folderPath || "Sin carpeta seleccionada"}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <label className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
              <input
                type="checkbox"
                checked={folderRecursive}
                onChange={(event) => {
                  const recursive = event.currentTarget.checked;
                  setFolderRecursive(recursive);
                  void refreshAudioFiles(folderPath, recursive);
                }}
              />
              Recursivo
            </label>
            <span className="text-xs text-muted-foreground">{audioFiles.length} archivos</span>
            <Button variant="secondary" size="sm" disabled={busy || !folderPath} onClick={() => void refreshAudioFiles()}>
              <RefreshCcw className="h-3.5 w-3.5" />
              Refrescar
            </Button>
            <Button variant="secondary" size="sm" disabled={busy || !folderPath} onClick={clearFolderExplorer}>
              <X className="h-3.5 w-3.5" />
              Cerrar
            </Button>
            <Button size="sm" disabled={busy} onClick={chooseFolder}>
              Elegir
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="file-grid file-grid-browser sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
            <span>Archivo</span>
            <span>Formato</span>
            <span>Tamano</span>
            <span>Carpeta</span>
            <span>Acciones</span>
          </div>
          {!folderPath ? <EmptyRow>Elige una carpeta para navegar archivos de audio originales.</EmptyRow> : null}
          {folderPath && audioFiles.length === 0 ? <EmptyRow>No se encontraron archivos de audio originales.</EmptyRow> : null}
          {audioFiles.map((file) => (
            <div key={file.path} className="file-grid file-grid-browser border-b border-border text-xs">
              <span className="truncate" title={file.path}>
                {file.name}
              </span>
              <span>{file.extension.toUpperCase()}</span>
              <span>{formatSize(file.size_bytes)}</span>
              <span className="truncate" title={file.parent_path}>
                {file.parent_path}
              </span>
              <div className="flex justify-end gap-1">
                <Button
                  variant="secondary"
                  size="icon"
                  title="Escuchar archivo"
                  onClick={() => void togglePathPlayback(file.path, file.name)}
                >
                  {playbackIcon(file.path) === "stop" ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                </Button>
                <Button variant="secondary" size="icon" title="Mostrar en Finder" onClick={() => void reveal(file.path)}>
                  <ChevronRight className="h-3.5 w-3.5" />
                </Button>
                <Button variant="secondary" size="icon" title="Abrir carpeta" onClick={() => void openFolder(file.path)}>
                  <FolderOpen className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
          ))}
        </CardContent>
        {folderSkippedErrors.length > 0 ? (
          <div className="border-t border-border px-3 py-2 text-xs text-red-700 dark:text-red-300">
            {folderSkippedErrors.length} carpetas o archivos no se pudieron leer.
          </div>
        ) : null}
      </Card>

      {importResult ? (
        <section className="grid min-h-0 grid-cols-[minmax(240px,300px)_minmax(0,1fr)] gap-3 max-lg:grid-cols-1">
          <Card className="flex h-[520px] min-h-0 flex-col overflow-hidden max-lg:h-[360px]">
            <CardHeader>
              <div className="flex min-w-0 items-center gap-2">
                <PlaylistSelectAllCheckbox
                  checked={allPlaylistsSelected}
                  indeterminate={somePlaylistsSelected}
                  disabled={playlistRows.length === 0}
                  onChange={toggleAllPlaylists}
                />
                <CardTitle>Playlists</CardTitle>
              </div>
              <div className="flex flex-wrap items-center justify-end gap-2">
                <span className="text-xs text-muted-foreground">{selectedPlaylists.size} seleccionadas</span>
                {selectedPlaylists.size > 0 ? (
                  <Button
                    size="sm"
                    className="bg-emerald-600 text-white hover:bg-emerald-700"
                    disabled={selectedPlaylistPendingTrackIds.length === 0}
                    onClick={convertSelectedPlaylists}
                    title={`${selectedPlaylistPendingTrackIds.length} track(s) unico(s) pendientes en ${selectedPlaylists.size} playlist(s)`}
                  >
                    <Download className="h-3.5 w-3.5" />
                    {selectedPlaylistsConvertLabel()}
                  </Button>
                ) : null}
              </div>
            </CardHeader>
            <CardContent className="overflow-x-hidden overflow-y-auto">
              {playlistRows.map((playlist) => {
                const processingCount = playlistProcessingCounts.get(playlist.path) ?? 0;
                const convertedCount = playlistConvertedCounts.get(playlist.path) ?? 0;

                return (
                  <div
                    key={playlist.path}
                    className={cn(
                      "grid min-h-9 grid-cols-[22px_minmax(0,1fr)_minmax(0,48px)_58px] items-center gap-2 border-b border-l-2 border-b-border border-l-transparent px-3",
                      processingCount > 0 && "border-l-amber-500 bg-amber-50/70 dark:bg-amber-950/30",
                      playlist.path === activePlaylistPath && "bg-muted"
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={selectedPlaylists.has(playlist.path)}
                      onChange={() => togglePlaylist(playlist.path)}
                    />
                    <button
                      type="button"
                      className="min-w-0 truncate text-left text-sm hover:text-primary"
                      title={playlist.path}
                      onClick={() => void selectPlaylist(playlist.path)}
                    >
                      {playlist.path}
                    </button>
                    <span className="flex justify-end">
                      {processingCount > 0 ? (
                        <span
                          className="inline-flex items-center gap-1 rounded-full border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold text-amber-800 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-200"
                          title={`${processingCount} archivo(s) procesandose en esta playlist`}
                        >
                          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-500" />
                          {processingCount}
                        </span>
                      ) : null}
                    </span>
                    <em
                      className={cn(
                        "text-right text-[11px] not-italic tabular-nums",
                        convertedCount > 0 ? "font-semibold text-foreground" : "text-muted-foreground"
                      )}
                      title={`${convertedCount} convertido(s) de ${playlist.track_count} track(s)`}
                    >
                      {convertedCount}/{playlist.track_count}
                    </em>
                  </div>
                );
              })}
            </CardContent>
          </Card>

          <section className="grid min-h-[520px] min-w-0 grid-rows-[auto_minmax(0,1fr)] gap-3">
            <div className="flex min-w-0 flex-wrap items-center gap-1 rounded-md border border-border bg-card p-1">
              <DetailTabButton active={activeDetailTab === "playlist"} onClick={() => setActiveDetailTab("playlist")}>
                Playlist
              </DetailTabButton>
              <DetailTabButton active={activeDetailTab === "converted"} onClick={() => setActiveDetailTab("converted")}>
                Convertidos ({convertedFiles.length})
              </DetailTabButton>
              <DetailTabButton active={activeDetailTab === "plan"} onClick={() => setActiveDetailTab("plan")}>
                Plan{plan ? ` (${plannedRows.length})` : ""}
              </DetailTabButton>
              <DetailTabButton active={activeDetailTab === "report"} onClick={() => setActiveDetailTab("report")}>
                Reporte ({sortedIssues.length})
              </DetailTabButton>
            </div>

            <Card className={cn("flex h-[520px] min-h-0 flex-col overflow-hidden", activeDetailTab !== "playlist" && "hidden")}>
              <CardHeader>
                <div className="min-w-0">
                  <CardTitle>Playlist</CardTitle>
                  <span className="block truncate text-xs text-muted-foreground" title={activePlaylistPath}>
                    {activePlaylistPath || "Sin playlist seleccionada"}
                  </span>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-2">
                  <span className="text-xs text-muted-foreground">{playlistFiles.length} archivos</span>
                  {selectedPlaylists.size > 0 ? (
                    <Button
                      size="sm"
                      className="bg-emerald-600 text-white hover:bg-emerald-700"
                      disabled={selectedPlaylistPendingTrackIds.length === 0}
                      onClick={convertSelectedPlaylists}
                      title={`${selectedPlaylistPendingTrackIds.length} track(s) unico(s) pendientes en ${selectedPlaylists.size} playlist(s)`}
                    >
                      <Download className="h-3.5 w-3.5" />
                      {selectedPlaylistsConvertLabel()}
                    </Button>
                  ) : null}
                  <Button size="sm" disabled={playlistLoading || activeConvertibleTrackIds.length === 0} onClick={convertActivePlaylist}>
                    Convertir playlist
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={playlistLoading || !activePlaylistPath}
                    onClick={() => void selectPlaylist(activePlaylistPath)}
                  >
                    <RefreshCcw className="h-3.5 w-3.5" />
                    Refrescar
                  </Button>
                  {activePlaylist && !selectedPlaylists.has(activePlaylist.path) ? (
                    <Button size="sm" onClick={() => togglePlaylist(activePlaylist.path)}>
                      Seleccionar
                    </Button>
                  ) : null}
                </div>
              </CardHeader>
              <CardContent className="overflow-x-hidden overflow-y-auto">
                <div className="playlist-track-grid sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
                  <span />
                  <span>#</span>
                  <span>Tema</span>
                  <span>Artista</span>
                  <span>Formato</span>
                  <span>Original</span>
                  <span>AIFF</span>
                  <span>Acciones</span>
                </div>
                {playlistLoading ? <EmptyRow>Cargando playlist...</EmptyRow> : null}
                {!playlistLoading && !activePlaylistPath ? <EmptyRow>Haz click en una playlist para ver sus archivos.</EmptyRow> : null}
                {!playlistLoading && activePlaylistPath && playlistFiles.length === 0 ? <EmptyRow>Esta playlist no tiene archivos.</EmptyRow> : null}
                {playlistFiles.map((file) => (
                  <div
                    key={`${file.track_id}-${file.position}`}
                    className={cn("playlist-track-grid border-b border-border text-xs", !file.source_exists && "bg-red-50 dark:bg-red-950/30")}
                  >
                    <span className="flex justify-center">
                      <Button
                        variant={file.source_path && player?.path === file.source_path && playerPlaying ? "default" : "secondary"}
                        size="icon"
                        title={playbackIcon(file.source_path) === "stop" ? "Detener original" : "Escuchar original"}
                        disabled={!file.source_exists || !file.source_path}
                        onClick={() => file.source_path && void togglePathPlayback(file.source_path, file.name ?? file.source_path)}
                      >
                        {playbackIcon(file.source_path) === "stop" ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                      </Button>
                    </span>
                    <span>{file.position}</span>
                    <span className="flex min-w-0 items-center gap-2" title={file.name ?? file.track_id}>
                      <span className={cn("h-2.5 w-2.5 shrink-0 rounded-full border", conversionDotClass(file))} title={conversionDotTitle(file)} />
                      <button
                        type="button"
                        className="min-w-0 truncate text-left font-medium underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        onClick={() => {
                          setSelectedTrackFile(file);
                          setMetadataSheetOpen(true);
                        }}
                      >
                        {file.name ?? file.track_id}
                      </button>
                    </span>
                    <span className="truncate" title={file.artist ?? ""}>{file.artist ?? ""}</span>
                    <span className="truncate">{file.kind ?? ""}</span>
                    <span className="truncate" title={file.source_path ?? ""}>{file.source_path ?? "No encontrado"}</span>
                    <span className="min-w-0" title={trackProgress(file.track_id)?.message ?? file.target_path ?? ""}>
                      <span className="block truncate">{targetLabel(file)}</span>
                      {isTrackConverting(file.track_id) ? <Progress value={progressPercent(file.track_id)} small /> : null}
                    </span>
                    <RowActions file={file} onAction={runPlaylistFileAction} canConvert={canConvertPlaylistFile(file)} label={conversionButtonLabel(file)} />
                  </div>
                ))}
              </CardContent>
            </Card>

            <Card className={cn("flex h-[520px] min-h-0 flex-col overflow-hidden", activeDetailTab !== "converted" && "hidden")}>
              <CardHeader>
                <CardTitle>Convertidos</CardTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">{convertedFiles.length} AIFF detectados</span>
                  <Button variant="secondary" size="sm" disabled={busy} onClick={() => void refreshConvertedFiles()}>
                    <RefreshCcw className="h-3.5 w-3.5" />
                    Refrescar
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="overflow-x-hidden overflow-y-auto">
                <div className="file-grid file-grid-converted sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
                  <span>Tema</span>
                  <span>Artista</span>
                  <span>Formato</span>
                  <span>AIFF</span>
                  <span>Acciones</span>
                </div>
                {convertedFiles.length === 0 ? <EmptyRow>No hay AIFF convertidos detectados para este XML.</EmptyRow> : null}
                {convertedFiles.map((file) => (
                  <div key={file.target_path} className="file-grid file-grid-converted border-b border-border text-xs">
                    <span className="flex min-w-0 items-center gap-2 truncate" title={file.name ?? file.track_id}>
                      <span className="h-2.5 w-2.5 shrink-0 rounded-full border border-emerald-700 bg-emerald-500" />
                      <span className="truncate">{file.name ?? file.track_id}</span>
                    </span>
                    <span className="truncate" title={file.artist ?? ""}>{file.artist ?? ""}</span>
                    <span className="truncate" title={file.source_path}>{file.kind ?? ""}</span>
                    <span className="truncate" title={file.target_path}>{file.target_path}</span>
                    <div className="flex justify-end gap-1">
                      <Button variant="secondary" size="icon" title="Escuchar AIFF" onClick={() => void togglePathPlayback(file.target_path, file.name ?? file.target_path)}>
                        {playbackIcon(file.target_path) === "stop" ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                      </Button>
                      <Button variant="secondary" size="icon" title="Mostrar en Finder" onClick={() => void reveal(file.target_path)}>
                        <ChevronRight className="h-3.5 w-3.5" />
                      </Button>
                      <Button variant="secondary" size="icon" title="Abrir carpeta" onClick={() => void openFolder(file.target_path)}>
                        <FolderOpen className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                ))}
              </CardContent>
            </Card>

            <Card className={cn("flex h-[520px] min-h-0 flex-col overflow-hidden", activeDetailTab !== "plan" && "hidden")}>
              <CardHeader>
                <CardTitle>Plan seleccionado</CardTitle>
                <span className="text-xs text-muted-foreground">{plannedRows.length} tracks</span>
              </CardHeader>
              <CardContent className="overflow-x-hidden overflow-y-auto">
                {!plan ? <EmptyRow>Crea un plan para ver los tracks seleccionados.</EmptyRow> : null}
                {plan ? (
                  <>
                  <div className="plan-grid sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
                    <span>Tema</span>
                    <span>Estado</span>
                    <span>Destino</span>
                    <span>Acciones</span>
                  </div>
                  {plannedRows.map((item) => (
                    <div key={item.track_id} className={cn("plan-grid border-b border-border text-xs", planRowClass(item.action))}>
                      <span className="truncate" title={item.name ?? item.track_id}>{item.name ?? item.track_id}</span>
                      <span>{item.action}</span>
                      <span className="truncate" title={item.target_path ?? item.source_path ?? ""}>{item.target_path ?? item.source_path ?? ""}</span>
                      <div className="flex justify-end gap-1">
                        <Button variant="secondary" size="icon" title="Escuchar original" disabled={!item.source_path} onClick={() => item.source_path && void togglePathPlayback(item.source_path, item.name ?? item.source_path)}>
                          <Play className="h-3.5 w-3.5" />
                        </Button>
                        <Button variant="secondary" size="icon" title="Abrir destino" disabled={!item.target_path} onClick={() => item.target_path && void openFolder(item.target_path)}>
                          <FolderOpen className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>
                  ))}
                  </>
                ) : null}
              </CardContent>
            </Card>

            <Card className={cn("flex h-[520px] min-h-0 flex-col overflow-hidden", activeDetailTab !== "report" && "hidden")}>
              <CardHeader>
                <CardTitle>Reporte</CardTitle>
                <span className="text-xs text-muted-foreground">{sortedIssues.length} hallazgos</span>
              </CardHeader>
              <CardContent className="overflow-x-hidden overflow-y-auto">
                <div className="issue-grid sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
                  <span>Severidad</span>
                  <span>Codigo</span>
                  <span>Track</span>
                  <span>Mensaje</span>
                </div>
                {sortedIssues.map((issue, index) => (
                  <div key={`${issue.code}-${issue.track_id ?? index}`} className={cn("issue-grid border-b border-border text-xs", issueRowClass(issue.severity))}>
                    <span>{issue.severity}</span>
                    <span>{issue.code}</span>
                    <span className="truncate">{issue.track_id ?? ""}</span>
                    <span className="truncate" title={issue.message}>{issue.message}</span>
                  </div>
                ))}
              </CardContent>
            </Card>
          </section>
        </section>
      ) : null}

      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle="ffmpeg / conversion / export"
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
      <TrackMetadataSheet
        open={metadataSheetOpen}
        track={metadataTrack}
        onClose={() => setMetadataSheetOpen(false)}
      />
    </main>
  );
}

function AppSidebar() {
  return (
    <aside className="sticky top-0 flex h-screen w-64 shrink-0 flex-col border-r border-border bg-card px-3 py-4 max-lg:static max-lg:h-auto max-lg:w-full max-lg:border-b max-lg:border-r-0">
      <div className="mb-5 flex items-center gap-3 px-2 max-lg:mb-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
          <Disc3 className="h-5 w-5" />
        </span>
        <div className="min-w-0">
          <strong className="block truncate text-sm font-semibold">Aifficator</strong>
          <span className="block truncate text-xs text-muted-foreground">Desktop</span>
        </div>
      </div>

      <nav className="grid gap-5 max-lg:flex max-lg:gap-3 max-lg:overflow-x-auto">
        <SidebarSection title="File Conversion">
          <SidebarLink to="/file-conversion/local" icon={<Upload className="h-4 w-4" />}>
            File Conversion
          </SidebarLink>
          <SidebarLink to="/file-conversion/rekordbox-convert" icon={<FileAudio2 className="h-4 w-4" />}>
            Rekordbox Convert
          </SidebarLink>
        </SidebarSection>

        <SidebarSection title="Turn">
          <SidebarLink to="/turn" icon={<Disc3 className="h-4 w-4" />}>
            Turn
          </SidebarLink>
        </SidebarSection>

        <SidebarSection title="Mastering">
          <SidebarLink to="/mastering" icon={<Gauge className="h-4 w-4" />}>
            Mastering
          </SidebarLink>
        </SidebarSection>

        <SidebarSection title="Settings">
          <SidebarLink to="/settings" icon={<Settings className="h-4 w-4" />}>
            Settings
          </SidebarLink>
        </SidebarSection>
      </nav>
    </aside>
  );
}

function SidebarSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid gap-1 max-lg:min-w-52">
      <span className="px-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </span>
      {children}
    </section>
  );
}

function SidebarLink({
  to,
  icon,
  children
}: {
  to: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        cn(
          "flex min-h-10 min-w-0 items-center gap-2 rounded-md px-3 text-left text-sm font-medium transition-colors",
          isActive
            ? "bg-primary text-primary-foreground shadow-sm"
            : "text-muted-foreground hover:bg-secondary hover:text-foreground"
        )
      }
    >
      <span className="shrink-0">{icon}</span>
      <span className="truncate">{children}</span>
    </NavLink>
  );
}

function Metric({ label, value, danger = false }: { label: string; value: number; danger?: boolean }) {
  return (
    <Card className={cn("p-3", danger && "border-red-300 text-red-800 dark:border-red-900 dark:text-red-200")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block text-xl">{value}</strong>
    </Card>
  );
}

function PlanMetric({ children, danger = false }: { children: React.ReactNode; danger?: boolean }) {
  return <Card className={cn("p-3 text-sm font-semibold", danger && "border-red-300 text-red-800 dark:border-red-900 dark:text-red-200")}>{children}</Card>;
}

function EmptyRow({ children }: { children: React.ReactNode }) {
  return <div className="flex min-h-11 items-center px-3 text-sm text-muted-foreground">{children}</div>;
}

function PlaylistSelectAllCheckbox({
  checked,
  indeterminate,
  disabled,
  onChange
}: {
  checked: boolean;
  indeterminate: boolean;
  disabled: boolean;
  onChange: () => void;
}) {
  const checkboxRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (checkboxRef.current) {
      checkboxRef.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <input
      ref={checkboxRef}
      type="checkbox"
      checked={checked}
      disabled={disabled}
      title={checked ? "Deseleccionar todas las playlists" : "Seleccionar todas las playlists"}
      onChange={onChange}
    />
  );
}

function Progress({ value, small = false }: { value: number; small?: boolean }) {
  return (
    <div className={cn("overflow-hidden rounded-full bg-slate-200 dark:bg-slate-800", small ? "mt-1 h-1.5" : "h-2")}>
      <div className="h-full rounded-full bg-primary" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

function CreatePlanButton({
  disabled,
  selectedCount,
  onClick
}: {
  disabled: boolean;
  selectedCount: number;
  onClick: () => void;
}) {
  return (
    <div className="group relative inline-flex">
      <Button onClick={onClick} disabled={disabled}>
        <ClipboardList className="h-4 w-4" />
        Crear plan
      </Button>
      <div className="pointer-events-none absolute right-0 top-[calc(100%+8px)] z-50 hidden w-80 rounded-md border border-border bg-card p-3 text-card-foreground shadow-lg group-hover:block group-focus-within:block">
        <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
          <ClipboardList className="h-4 w-4" />
          Preflight de conversion
        </div>
        <p className="text-xs leading-relaxed text-muted-foreground">
          El plan revisa las playlists seleccionadas antes de convertir. No modifica archivos ni exporta XML.
        </p>
        <div className="mt-3 grid gap-2 text-xs">
          <div className="rounded-md bg-secondary px-2 py-1.5">
            Detecta tracks a convertir, AIFF existentes, archivos faltantes y formatos bloqueados.
          </div>
          <div className="rounded-md bg-secondary px-2 py-1.5">
            Seleccionadas: <strong>{selectedCount}</strong>. Si no eliges ninguna, se planifica toda la libreria.
          </div>
        </div>
      </div>
    </div>
  );
}

function DetailTabButton({
  active,
  children,
  onClick
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <Button variant={active ? "default" : "ghost"} size="sm" className="h-8 px-3" onClick={onClick}>
      {children}
    </Button>
  );
}

function TrackMetadataSheet({
  open,
  track,
  onClose
}: {
  open: boolean;
  track: PlaylistTrackFile | null;
  onClose: () => void;
}) {
  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open || !track) return null;

  const attributes = Object.entries(track.attributes ?? {}).sort(([left], [right]) => left.localeCompare(right));
  const identityRows: Array<[string, React.ReactNode]> = [
    ["Track ID", track.track_id],
    ["Titulo", track.name],
    ["Artista", track.artist],
    ["Album", track.album],
    ["Formato", track.kind],
    ["Location XML", track.location]
  ];
  const technicalStats = [
    {
      icon: <Clock3 className="h-4 w-4" />,
      label: "Duracion",
      value: typeof track.total_time === "number" ? formatDuration(track.total_time) : "No disponible"
    },
    {
      icon: <HardDrive className="h-4 w-4" />,
      label: "Tamano",
      value: typeof track.size === "number" ? formatBytes(track.size) : "No disponible"
    },
    {
      icon: <Gauge className="h-4 w-4" />,
      label: "Sample rate",
      value: typeof track.sample_rate === "number" ? `${track.sample_rate} Hz` : "No disponible"
    },
    {
      icon: <FileAudio2 className="h-4 w-4" />,
      label: "Bitrate",
      value: typeof track.bitrate === "number" ? `${track.bitrate} kbps` : "No disponible"
    }
  ];

  return (
    <div className="fixed inset-0 z-[65]">
      <div className="absolute inset-0 bg-black/25 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="absolute right-0 top-0 z-[70] flex h-full w-[500px] max-w-[calc(100vw-16px)] flex-col border-l border-border bg-background shadow-2xl">
        <header className="border-b border-border bg-card px-4 py-4">
          <div className="flex items-start justify-between gap-3">
            <div className="flex min-w-0 gap-3">
              <span className="grid h-11 w-11 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
                <Disc3 className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <h2 className="truncate text-base font-semibold">{track.name ?? track.track_id}</h2>
                <p className="mt-1 truncate text-sm text-muted-foreground">{track.artist ?? "Sin artista"}</p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  <StatusPill tone={track.source_exists ? "ok" : "error"}>
                    {track.source_exists ? "Original encontrado" : "Original no encontrado"}
                  </StatusPill>
                  <StatusPill tone={track.target_exists ? "ok" : "muted"}>
                    {track.target_exists ? "AIFF convertido" : "AIFF pendiente"}
                  </StatusPill>
                  {track.kind ? <StatusPill tone="muted">{track.kind}</StatusPill> : null}
                </div>
              </div>
            </div>
            <Button variant="ghost" size="icon" title="Cerrar" onClick={onClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <section className="grid grid-cols-2 gap-2">
            {technicalStats.map((stat) => (
              <MetadataStat key={stat.label} icon={stat.icon} label={stat.label} value={stat.value} />
            ))}
          </section>

          <SheetSection icon={<Info className="h-4 w-4" />} title="Identidad">
            <div className="grid gap-2">
              {identityRows.map(([label, value]) => (
                <MetadataRow key={label} label={label} value={value} />
              ))}
            </div>
          </SheetSection>

          <SheetSection icon={<FileAudio2 className="h-4 w-4" />} title="Rutas">
            <div className="grid gap-3">
              <PathBlock label="Original" value={track.source_path} missing={!track.source_exists} />
              <PathBlock label="AIFF convertido" value={track.target_path} missing={false} />
            </div>
          </SheetSection>

          <SheetSection icon={<Database className="h-4 w-4" />} title={`Atributos XML (${attributes.length})`}>
            {attributes.length === 0 ? (
              <span className="text-xs text-muted-foreground">Sin atributos disponibles.</span>
            ) : (
              <div className="grid gap-2">
                {attributes.map(([key, value]) => (
                  <MetadataRow key={key} label={key} value={value} mono />
                ))}
              </div>
            )}
          </SheetSection>
        </div>
      </aside>
    </div>
  );
}

function StatusPill({ tone, children }: { tone: "ok" | "error" | "muted"; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-semibold",
        tone === "ok" && "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200",
        tone === "error" && "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200",
        tone === "muted" && "border-border bg-secondary text-secondary-foreground"
      )}
    >
      {tone === "error" ? <AlertTriangle className="h-3 w-3" /> : null}
      {children}
    </span>
  );
}

function MetadataStat({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-card p-3">
      <div className="flex items-center gap-2 text-muted-foreground">
        {icon}
        <span className="text-[11px] font-semibold uppercase">{label}</span>
      </div>
      <strong className="mt-2 block truncate text-sm">{value}</strong>
    </div>
  );
}

function SheetSection({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <section className="mt-4 rounded-md border border-border bg-card">
      <div className="flex min-h-10 items-center gap-2 border-b border-border px-3 text-sm font-semibold">
        {icon}
        <h3>{title}</h3>
      </div>
      <div className="p-3">{children}</div>
    </section>
  );
}

function MetadataRow({
  label,
  value,
  mono = false
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}) {
  if (value === undefined || value === null || value === "") return null;

  return (
    <div className="grid grid-cols-[120px_minmax(0,1fr)] gap-3 rounded-md bg-secondary/60 px-3 py-2 text-xs">
      <span className="truncate font-semibold text-muted-foreground">{label}</span>
      <span className={cn("min-w-0 break-words", mono && "font-mono text-[11px]")}>{value}</span>
    </div>
  );
}

function PathBlock({ label, value, missing }: { label: string; value?: string; missing: boolean }) {
  return (
    <div className={cn("rounded-md border p-3", missing ? "border-red-200 bg-red-50 dark:border-red-900 dark:bg-red-950/40" : "border-border bg-secondary/60")}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-muted-foreground">{label}</span>
        <Button variant="ghost" size="icon" title="Copiar path" disabled={!value} onClick={() => value && copyText(value)}>
          <Copy className="h-3.5 w-3.5" />
        </Button>
      </div>
      <p className="break-words font-mono text-[11px] leading-relaxed text-foreground">
        {value || "No disponible"}
      </p>
    </div>
  );
}

function copyText(value: string) {
  void navigator.clipboard?.writeText(value).catch(() => undefined);
}

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return "";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = Math.floor(seconds % 60).toString().padStart(2, "0");
  return hours > 0 ? `${hours}:${minutes.toString().padStart(2, "0")}:${remainingSeconds}` : `${minutes}:${remainingSeconds}`;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function RowActions({
  file,
  onAction,
  canConvert,
  label
}: {
  file: PlaylistTrackFile;
  onAction: (file: PlaylistTrackFile, action: string) => void;
  canConvert: boolean;
  label: string;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="secondary" size="icon" title="Acciones">
          <MoreHorizontal className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem disabled={!file.target_exists || !file.target_path} onSelect={() => onAction(file, "aiff")}>
          <Play className="h-4 w-4" />
          Escuchar AIFF
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!canConvert} onSelect={() => onAction(file, "convert")}>
          <Download className="h-4 w-4" />
          {label === "CONV" ? "Convertir" : label}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!file.source_exists || !file.source_path} onSelect={() => onAction(file, "find")}>
          <ChevronRight className="h-4 w-4" />
          Finder
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!file.source_path} onSelect={() => onAction(file, "open")}>
          <FolderOpen className="h-4 w-4" />
          Abrir carpeta
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function planRowClass(action: PlanItem["action"]) {
  if (action === "reuse_existing") return "bg-emerald-50 dark:bg-emerald-950/30";
  if (action === "blocked") return "bg-red-50 dark:bg-red-950/30";
  if (action === "skip_already_aiff") return "bg-slate-50 dark:bg-slate-900/40";
  return "";
}

function issueRowClass(severity: Issue["severity"]) {
  if (severity === "error") return "bg-red-50 dark:bg-red-950/30";
  if (severity === "warning") return "bg-amber-50 dark:bg-amber-950/30";
  return "bg-slate-50 dark:bg-slate-900/40";
}
