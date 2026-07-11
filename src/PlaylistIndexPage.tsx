import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ChevronRight,
  Database,
  FileOutput,
  FolderOpen,
  Play,
  Plus,
  RefreshCcw,
  Search,
  Sparkles,
  Square,
  Trash2,
  Upload
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type * as React from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { cn } from "./lib/utils";
import { translateBackendMessage, useI18n } from "./i18n";

type PlaylistIndexLibrary = {
  id: string;
  source_path: string;
  source_name: string;
  product_name?: string | null;
  product_version?: string | null;
  track_count: number;
  playlist_count: number;
  embedded_track_count: number;
  missing_file_count: number;
  indexed_at: string;
  updated_at: string;
};

type PlaylistIndexPlaylist = {
  library_id: string;
  path: string;
  name: string;
  node_type?: string | null;
  track_count: number;
  position: number;
};

type PlaylistIndexTrack = {
  library_id: string;
  track_id: string;
  name?: string | null;
  artist?: string | null;
  album?: string | null;
  kind?: string | null;
  location?: string | null;
  source_path?: string | null;
  size?: number | null;
  total_time?: number | null;
  sample_rate?: number | null;
  bitrate?: number | null;
  source_exists: boolean;
  search_text: string;
  embedding_ready: boolean;
};

type PlaylistSearchResult = {
  track: PlaylistIndexTrack;
  score: number;
  mode: "library" | "lexical" | "semantic" | string;
};

type PlaylistDraft = {
  id: string;
  library_id: string;
  name: string;
  description?: string | null;
  track_count: number;
  created_at: string;
  updated_at: string;
};

type PlaylistIndexImportResponse = {
  library: PlaylistIndexLibrary;
  playlists: PlaylistIndexPlaylist[];
};

type PlaylistIndexPreviewPlaylist = {
  path: string;
  name: string;
  track_count: number;
  position: number;
};

type PlaylistIndexPreviewResponse = {
  source_path: string;
  source_name: string;
  product_name?: string | null;
  product_version?: string | null;
  tracks_total: number;
  playlists: PlaylistIndexPreviewPlaylist[];
};

type PlaylistEmbeddingResult = {
  library_id: string;
  generated_total: number;
  skipped_total: number;
  model: string;
  dimensions: number;
};

type PlaylistExportResult = {
  draft_id: string;
  output_path: string;
  track_count: number;
};

type PlaylistIndexProgressEvent = {
  type: "playlist_index_progress";
  level: "info" | "warning" | "error" | string;
  message: string;
  progress?: number | null;
  library_id?: string | null;
  playlist_path?: string | null;
  playlist_status?: "indexing" | "indexed" | string | null;
  processed?: number | null;
  total?: number | null;
  timestamp: string;
};

type PlayerState = {
  label: string;
  path: string;
  url: string;
};

type PlaylistIndexTab = "index" | "search" | "playlist";
type PlaylistIndexPlaylistStatus = "pending" | "queued" | "indexing" | "indexed";

export function PlaylistIndexPage() {
  const { locale, t } = useI18n();
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [xmlPath, setXmlPath] = useState("");
  const [xmlPreview, setXmlPreview] = useState<PlaylistIndexPreviewResponse | null>(null);
  const [selectedPreviewPlaylistPaths, setSelectedPreviewPlaylistPaths] = useState<Set<string>>(new Set());
  const [playlists, setPlaylists] = useState<PlaylistIndexPlaylist[]>([]);
  const [activePlaylistPath, setActivePlaylistPath] = useState("");
  const [playlistTracks, setPlaylistTracks] = useState<PlaylistIndexTrack[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [semanticSearch, setSemanticSearch] = useState(false);
  const [searchResults, setSearchResults] = useState<PlaylistSearchResult[]>([]);
  const [selectedTrackIds, setSelectedTrackIds] = useState<Set<string>>(new Set());
  const [drafts, setDrafts] = useState<PlaylistDraft[]>([]);
  const [activeDraftId, setActiveDraftId] = useState("");
  const [draftTracks, setDraftTracks] = useState<PlaylistIndexTrack[]>([]);
  const [draftName, setDraftName] = useState("");
  const [draftDescription, setDraftDescription] = useState("");
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [terminalLogs, setTerminalLogs] = useState<TerminalLogEntry[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [indexProgress, setIndexProgress] = useState<PlaylistIndexProgressEvent | null>(null);
  const [playlistIndexStatuses, setPlaylistIndexStatuses] = useState<Record<string, PlaylistIndexPlaylistStatus>>({});
  const [player, setPlayer] = useState<PlayerState | null>(null);
  const [playerPlaying, setPlayerPlaying] = useState(false);
  const [playerCurrentTime, setPlayerCurrentTime] = useState(0);
  const [playerDuration, setPlayerDuration] = useState(0);
  const [activeTab, setActiveTab] = useState<PlaylistIndexTab>("index");
  const [createDraftSheetOpen, setCreateDraftSheetOpen] = useState(false);
  const [detailTrack, setDetailTrack] = useState<PlaylistIndexTrack | null>(null);
  const [detailSheetOpen, setDetailSheetOpen] = useState(false);

  const audioElement = useRef<HTMLAudioElement | null>(null);
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);

  const activeLibrary = useMemo(
    () => libraries.find((library) => library.id === activeLibraryId) ?? null,
    [activeLibraryId, libraries]
  );
  const activePlaylist = playlists.find((playlist) => playlist.path === activePlaylistPath);
  const activeDraft = drafts.find((draft) => draft.id === activeDraftId) ?? null;
  const indexablePlaylists = useMemo<PlaylistIndexPreviewPlaylist[]>(() => {
    if (xmlPreview) return xmlPreview.playlists;
    if (!activeLibrary) return [];

    return playlists.map((playlist) => ({
      path: playlist.path,
      name: playlist.name,
      track_count: playlist.track_count,
      position: playlist.position
    }));
  }, [activeLibrary, playlists, xmlPreview]);
  const indexSourcePath = xmlPreview?.source_path || xmlPath || activeLibrary?.source_path || "";
  const indexedPlaylistPaths = useMemo(() => {
    if (!activeLibrary || activeLibrary.source_path !== indexSourcePath) return new Set<string>();
    return new Set(playlists.map((playlist) => playlist.path));
  }, [activeLibrary, indexSourcePath, playlists]);
  const indexTrackCount = xmlPreview?.tracks_total ?? activeLibrary?.track_count ?? 0;
  const allPreviewPlaylistsSelected =
    indexablePlaylists.length > 0 &&
    selectedPreviewPlaylistPaths.size === indexablePlaylists.length;
  const embeddedPercent = activeLibrary && activeLibrary.track_count > 0
    ? Math.round((activeLibrary.embedded_track_count / activeLibrary.track_count) * 100)
    : 0;
  const playerProgress =
    playerDuration > 0 ? Math.min(100, (playerCurrentTime / playerDuration) * 100) : 0;

  useEffect(() => {
    void loadLibraries();

    const unlisteners: UnlistenFn[] = [];
    listen<PlaylistIndexProgressEvent>("playlist-index-progress", (event) => {
      setIndexProgress(event.payload);
      if (event.payload.playlist_path && event.payload.playlist_status) {
        const status = event.payload.playlist_status === "indexed" ? "indexed" : "indexing";
        setPlaylistIndexStatuses((current) => ({
          ...current,
          [event.payload.playlist_path as string]: status
        }));
      }
      appendTerminalLog({
        level: normalizeLogLevel(event.payload.level),
        message: event.payload.message,
        name: event.payload.library_id ?? "playlist-index"
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  async function loadLibraries(selectId?: string) {
    setErrorMessage("");

    try {
      const response = await invoke<PlaylistIndexLibrary[]>("playlist_index_libraries");
      setLibraries(response);
      const nextId = selectId || activeLibraryId || response[0]?.id || "";
      setActiveLibraryId(nextId);
      if (nextId) {
        await loadLibraryDetails(nextId);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function loadLibraryDetails(libraryId: string) {
    if (!libraryId) return;

    setErrorMessage("");
    setSelectedTrackIds(new Set());
    setSearchResults([]);
    setPlaylistTracks([]);
    setActivePlaylistPath("");

    try {
      const [playlistRows, draftRows] = await Promise.all([
        invoke<PlaylistIndexPlaylist[]>("playlist_index_library_playlists", { libraryId }),
        invoke<PlaylistDraft[]>("playlist_index_drafts", { libraryId })
      ]);
      setPlaylists(playlistRows);
      setDrafts(draftRows);
      const nextDraftId = draftRows.find((draft) => draft.id === activeDraftId)?.id ?? draftRows[0]?.id ?? "";
      setActiveDraftId(nextDraftId);
      if (nextDraftId) {
        await loadDraftTracks(nextDraftId);
      } else {
        setDraftTracks([]);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function chooseXml() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Rekordbox XML", extensions: ["xml"] }]
    });
    if (typeof selected !== "string") return;
    setXmlPath(selected);
    setActiveTab("index");
    await previewXml(selected);
  }

  async function previewXml(path: string) {
    setBusy(true);
    setMessage("");
    setErrorMessage("");
    setXmlPreview(null);
    setPlaylistIndexStatuses({});
    setSelectedPreviewPlaylistPaths(new Set());

    try {
      const preview = await invoke<PlaylistIndexPreviewResponse>("playlist_index_preview_xml", {
        path
      });
      setXmlPreview(preview);
      setSelectedPreviewPlaylistPaths(new Set(preview.playlists.map((playlist) => playlist.path)));
      setMessage(t("XML cargado: {tracks} tracks, {playlists} playlists. Elige que indexar.", {
        tracks: preview.tracks_total,
        playlists: preview.playlists.length
      }));
    } catch (error) {
      const message = translateBackendMessage(locale, String(error));
      setErrorMessage(message);
      appendTerminalLog({ level: "error", message });
    } finally {
      setBusy(false);
    }
  }

  async function indexXml(pathOverride?: string, playlistPaths?: string[]) {
    const path = pathOverride ?? xmlPath;
    if (!path.trim()) return;

    setBusy(true);
    setMessage("");
    setErrorMessage("");
    const pathsForStatus = playlistPaths && playlistPaths.length > 0
      ? playlistPaths
      : indexablePlaylists.map((playlist) => playlist.path);
    if (pathsForStatus.length > 0) {
      setPlaylistIndexStatuses((current) => {
        const next = { ...current };
        for (const playlistPath of pathsForStatus) {
          next[playlistPath] = "queued";
        }
        return next;
      });
    }
    setIndexProgress({
      type: "playlist_index_progress",
      level: "info",
      message: t("Indexando XML de Rekordbox."),
      progress: 0,
      processed: 0,
      total: undefined,
      timestamp: new Date().toISOString()
    });
    appendTerminalLog({ level: "info", message: `${t("Indexando")} ${path}` });
    await waitForNextPaint();

    try {
      const response = await invoke<PlaylistIndexImportResponse>("playlist_index_import_xml", {
        path,
        playlistPaths: playlistPaths ?? []
      });
      setPlaylists(response.playlists);
      setPlaylistIndexStatuses((current) => {
        const next = { ...current };
        for (const playlist of response.playlists) {
          next[playlist.path] = "indexed";
        }
        return next;
      });
      setActiveLibraryId(response.library.id);
      setMessage(t("Indice actualizado: {tracks} tracks, {playlists} playlists.", {
        tracks: response.library.track_count,
        playlists: response.library.playlist_count
      }));
      await loadLibraries(response.library.id);
    } catch (error) {
      const message = translateBackendMessage(locale, String(error));
      setErrorMessage(message);
      appendTerminalLog({ level: "error", message });
    } finally {
      setBusy(false);
    }
  }

  function togglePreviewPlaylist(path: string) {
    setSelectedPreviewPlaylistPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  function toggleAllPreviewPlaylists() {
    setSelectedPreviewPlaylistPaths(() => {
      if (allPreviewPlaylistsSelected) return new Set();
      return new Set(indexablePlaylists.map((playlist) => playlist.path));
    });
  }

  async function indexSelectedPreviewPlaylists() {
    await indexXml(indexSourcePath, Array.from(selectedPreviewPlaylistPaths));
  }

  async function indexAllPreviewPlaylists() {
    await indexXml(indexSourcePath, []);
  }

  async function selectLibrary(libraryId: string) {
    const library = libraries.find((library) => library.id === libraryId);
    setXmlPreview(null);
    setPlaylistIndexStatuses({});
    setXmlPath(library?.source_path ?? "");
    setSelectedPreviewPlaylistPaths(new Set());
    setActiveLibraryId(libraryId);
    await loadLibraryDetails(libraryId);
  }

  async function selectPlaylist(playlistPath: string) {
    if (!activeLibraryId) return;

    if (activeTab === "index") {
      togglePreviewPlaylist(playlistPath);
      return;
    }

    setBusy(true);
    setErrorMessage("");
    setActivePlaylistPath(playlistPath);

    try {
      const tracks = await invoke<PlaylistIndexTrack[]>("playlist_index_playlist_tracks", {
        libraryId: activeLibraryId,
        playlistPath
      });
      setPlaylistTracks(tracks);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function searchTracks(event?: React.FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    if (!activeLibraryId) return;

    setBusy(true);
    setErrorMessage("");
    setSelectedTrackIds(new Set());

    try {
      const results = await invoke<PlaylistSearchResult[]>("playlist_index_search_tracks", {
        libraryId: activeLibraryId,
        query: searchQuery,
        limit: 120,
        semantic: semanticSearch
      });
      setSearchResults(results);
      setMessage(t("{count} resultados.", { count: results.length }));
    } catch (error) {
      const message = translateBackendMessage(locale, String(error));
      setErrorMessage(message);
      appendTerminalLog({ level: "error", message });
    } finally {
      setBusy(false);
    }
  }

  async function generateEmbeddings() {
    if (!activeLibraryId) return;

    setBusy(true);
    setErrorMessage("");
    setMessage("");
    appendTerminalLog({ level: "info", message: t("Generando embeddings de tracks.") });

    try {
      const result = await invoke<PlaylistEmbeddingResult>("playlist_index_generate_embeddings", {
        libraryId: activeLibraryId,
        limit: 2000
      });
      setMessage(t("Embeddings listos: {count} generados con {model}.", {
        count: result.generated_total,
        model: result.model
      }));
      await loadLibraries(activeLibraryId);
      if (searchResults.length > 0 || searchQuery.trim()) {
        await searchTracks();
      }
    } catch (error) {
      const message = translateBackendMessage(locale, String(error));
      setErrorMessage(message);
      appendTerminalLog({ level: "error", message });
    } finally {
      setBusy(false);
    }
  }

  async function createDraft(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeLibraryId || !draftName.trim()) return;

    setBusy(true);
    setErrorMessage("");

    try {
      const draft = await invoke<PlaylistDraft>("playlist_index_create_draft", {
        libraryId: activeLibraryId,
        name: draftName,
        description: draftDescription || null
      });
      setDraftName("");
      setDraftDescription("");
      setCreateDraftSheetOpen(false);
      setActiveDraftId(draft.id);
      await loadDrafts(activeLibraryId, draft.id);
      setMessage(t("Playlist creada: {name}", { name: draft.name }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function loadDrafts(libraryId = activeLibraryId, selectDraftId = activeDraftId) {
    if (!libraryId) return;
    const response = await invoke<PlaylistDraft[]>("playlist_index_drafts", { libraryId });
    setDrafts(response);
    const nextDraftId = response.find((draft) => draft.id === selectDraftId)?.id ?? response[0]?.id ?? "";
    setActiveDraftId(nextDraftId);
    if (nextDraftId) {
      await loadDraftTracks(nextDraftId);
    } else {
      setDraftTracks([]);
    }
  }

  async function selectDraft(draftId: string) {
    setActiveDraftId(draftId);
    await loadDraftTracks(draftId);
  }

  async function loadDraftTracks(draftId = activeDraftId) {
    if (!draftId) return;

    try {
      const tracks = await invoke<PlaylistIndexTrack[]>("playlist_index_draft_tracks", { draftId });
      setDraftTracks(tracks);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function addSelectedToDraft() {
    if (!activeDraftId || selectedTrackIds.size === 0) return;
    await addTrackIdsToDraft(Array.from(selectedTrackIds));
    setSelectedTrackIds(new Set());
  }

  async function addPlaylistToDraft() {
    if (!activeDraftId || playlistTracks.length === 0) return;
    await addTrackIdsToDraft(playlistTracks.map((track) => track.track_id));
  }

  async function addTrackIdsToDraft(trackIds: string[]) {
    setBusy(true);
    setErrorMessage("");

    try {
      const tracks = await invoke<PlaylistIndexTrack[]>("playlist_index_add_tracks_to_draft", {
        draftId: activeDraftId,
        trackIds
      });
      setDraftTracks(tracks);
      await loadDrafts(activeLibraryId, activeDraftId);
      setMessage(t("{count} tracks en la playlist.", { count: tracks.length }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function removeDraftTrack(trackId: string) {
    if (!activeDraftId) return;
    setBusy(true);
    setErrorMessage("");

    try {
      const tracks = await invoke<PlaylistIndexTrack[]>("playlist_index_remove_draft_track", {
        draftId: activeDraftId,
        trackId
      });
      setDraftTracks(tracks);
      await loadDrafts(activeLibraryId, activeDraftId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function deleteDraft() {
    if (!activeDraftId) return;

    setBusy(true);
    setErrorMessage("");

    try {
      await invoke<string>("playlist_index_delete_draft", { draftId: activeDraftId });
      setActiveDraftId("");
      setDraftTracks([]);
      await loadDrafts(activeLibraryId, "");
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function exportDraft() {
    if (!activeDraft || !activeLibrary) return;

    const outputPath = await save({
      defaultPath: defaultExportPath(activeLibrary.source_path, activeDraft.name),
      filters: [{ name: "Rekordbox XML", extensions: ["xml"] }]
    });
    if (typeof outputPath !== "string") return;

    setBusy(true);
    setErrorMessage("");
    appendTerminalLog({ level: "info", message: `${t("Exportando XML")} ${outputPath}` });

    try {
      const result = await invoke<PlaylistExportResult>("playlist_index_export_draft_xml", {
        draftId: activeDraft.id,
        outputPath
      });
      setMessage(t("XML exportado: {count} tracks.", { count: result.track_count }));
      appendTerminalLog({ level: "info", message: result.output_path });
    } catch (error) {
      const message = translateBackendMessage(locale, String(error));
      setErrorMessage(message);
      appendTerminalLog({ level: "error", message });
    } finally {
      setBusy(false);
    }
  }

  function toggleSearchTrack(trackId: string) {
    setSelectedTrackIds((current) => {
      const next = new Set(current);
      if (next.has(trackId)) {
        next.delete(trackId);
      } else {
        next.add(trackId);
      }
      return next;
    });
  }

  function selectAllSearchResults() {
    setSelectedTrackIds((current) => {
      if (current.size === searchResults.length) return new Set();
      return new Set(searchResults.map((result) => result.track.track_id));
    });
  }

  function openTrackDetail(track: PlaylistIndexTrack) {
    setDetailTrack(track);
    setDetailSheetOpen(true);
  }

  function playlistIndexStatus(path: string): PlaylistIndexPlaylistStatus {
    return playlistIndexStatuses[path] ?? (indexedPlaylistPaths.has(path) ? "indexed" : "pending");
  }

  async function togglePathPlayback(path: string, label: string) {
    if (player?.path === path && playerPlaying) {
      stopPlayer();
      return;
    }

    setPlayer({ path, label, url: convertFileSrc(path) });
    setPlayerPlaying(false);
    setPlayerCurrentTime(0);
    setPlayerDuration(0);

    await new Promise((resolve) => window.requestAnimationFrame(resolve));

    try {
      audioElement.current?.load();
      await audioElement.current?.play();
      setPlayerPlaying(true);
    } catch (error) {
      setErrorMessage(`${t("No se pudo reproducir")} ${label}: ${String(error)}`);
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
      setErrorMessage(`${t("No se pudo controlar el player")}: ${String(error)}`);
    }
  }

  function stopPlayer() {
    if (audioElement.current) {
      audioElement.current.pause();
      audioElement.current.currentTime = 0;
    }
    setPlayerPlaying(false);
    setPlayerCurrentTime(0);
  }

  function syncPlayerTime(audio: HTMLAudioElement | null = audioElement.current) {
    if (!audio) return;
    setPlayerCurrentTime(audio.currentTime || 0);
    setPlayerDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
  }

  function appendTerminalLog(log: Omit<TerminalLogEntry, "id" | "time">) {
    const nextLog: TerminalLogEntry = {
      ...log,
      message: translateBackendMessage(locale, log.message),
      id: nextTerminalLogId.current,
      time: new Date().toLocaleTimeString()
    };
    nextTerminalLogId.current += 1;
    setTerminalLogs((current) => [...current, nextLog].slice(-1000));
  }

  async function reveal(path?: string | null) {
    if (!path) return;
    try {
      await invoke("reveal_path", { path });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function openFolder(path?: string | null) {
    if (!path) return;
    try {
      await invoke("open_parent_folder", { path });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  return (
    <main className={cn("min-w-0 p-4 pb-20", terminalExpanded && "pb-72")}>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="min-w-0">
          <h1 className="m-0 text-2xl font-semibold tracking-normal">{t("Playlist Library")}</h1>
          <p className="mt-1 max-w-[72vw] truncate text-xs text-muted-foreground lg:max-w-[56vw]">
            {activeLibrary?.source_path ?? t("Sin XML indexado")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={chooseXml} disabled={busy}>
            <Upload className="h-4 w-4" />
            {t("Elegir XML")}
          </Button>
          <Button variant="secondary" onClick={() => void loadLibraries()} disabled={busy}>
            <RefreshCcw className="h-4 w-4" />
            {t("Refrescar")}
          </Button>
        </div>
      </header>

      {errorMessage ? (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}
      {message ? (
        <div className="mb-3 rounded-md border border-border bg-muted px-3 py-2 text-sm text-foreground">
          {message}
        </div>
      ) : null}

      {indexProgress ? (
        <Card className="mb-3 p-3">
          <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0">
              <strong className="block truncate text-sm">{t("Resumen general")}</strong>
              <span className="block truncate text-xs text-muted-foreground">
                {translateBackendMessage(locale, indexProgress.message)}
              </span>
            </div>
            <span className="text-xs tabular-nums text-muted-foreground">
              {typeof indexProgress.processed === "number" && typeof indexProgress.total === "number"
                ? t("{processed} de {total}", {
                    processed: indexProgress.processed,
                    total: indexProgress.total
                  })
                : `${Math.round(indexProgress.progress ?? 0)}%`}
            </span>
          </div>
          <Progress value={indexProgress.progress ?? 0} />
        </Card>
      ) : null}

      {activeLibrary ? (
        <section className="mb-3 grid grid-cols-2 gap-2 lg:grid-cols-5">
          <IndexMetric label={t("Tracks")} value={activeLibrary.track_count} />
          <IndexMetric label={t("Playlists")} value={activeLibrary.playlist_count} />
          <IndexMetric label={t("Vectores")} value={`${activeLibrary.embedded_track_count} / ${activeLibrary.track_count}`} />
          <IndexMetric label={t("Vector %")} value={`${embeddedPercent}%`} />
          <IndexMetric label={t("No encontrados")} value={activeLibrary.missing_file_count} danger={activeLibrary.missing_file_count > 0} />
        </section>
      ) : null}

      <Card className="mb-3 grid grid-cols-[74px_minmax(180px,320px)_minmax(220px,1fr)_84px] items-center gap-3 p-3 max-lg:grid-cols-1">
        <Button disabled={!player} onClick={() => void togglePlayer()} className="w-[74px] px-0">
          {playerPlaying ? <Square className="h-4 w-4" /> : <Play className="h-4 w-4" />}
          {playerPlaying ? t("Stop") : t("Play")}
        </Button>
        <div className="min-w-0">
          <span className="block text-xs text-muted-foreground">Player</span>
          <strong className="block truncate text-sm" title={player?.path ?? ""}>
            {player?.label ?? t("Sin archivo cargado")}
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
            onEnded={() => setPlayerPlaying(false)}
          />
        ) : null}
        <Button variant="secondary" disabled={!player} onClick={() => player && void reveal(player.path)}>
          Finder
        </Button>
      </Card>

      <div className="mb-3 flex min-w-0 flex-wrap items-center gap-1 rounded-md border border-border bg-card p-1">
        <PlaylistTabButton active={activeTab === "index"} onClick={() => setActiveTab("index")}>
          {t("Indexar XML")}
        </PlaylistTabButton>
        <PlaylistTabButton active={activeTab === "search"} onClick={() => setActiveTab("search")}>
          {t("Buscar")}
        </PlaylistTabButton>
        <PlaylistTabButton active={activeTab === "playlist"} onClick={() => setActiveTab("playlist")}>
          {t("Playlist")}
        </PlaylistTabButton>
      </div>

      <section className="grid h-[calc(100vh-390px)] min-h-[560px] grid-cols-[280px_minmax(0,1fr)] gap-3 max-lg:h-auto max-lg:grid-cols-1">
        <aside className="grid min-h-0 grid-rows-[minmax(0,180px)_minmax(0,1fr)] gap-3 max-lg:h-[520px]">
          <Card className="flex min-h-0 flex-col overflow-hidden">
            <CardHeader>
              <CardTitle>{t("Librerias")}</CardTitle>
              <span className="text-xs text-muted-foreground">{libraries.length}</span>
            </CardHeader>
            <CardContent className="overflow-y-auto">
              {libraries.length === 0 ? <EmptyRow>{t("Indexa un XML para empezar.")}</EmptyRow> : null}
              {libraries.map((library) => (
                <button
                  key={library.id}
                  type="button"
                  className={cn(
                    "grid w-full min-w-0 gap-1 border-b border-border px-3 py-2 text-left text-xs hover:bg-secondary",
                    library.id === activeLibraryId && "bg-muted"
                  )}
                  onClick={() => void selectLibrary(library.id)}
                >
                  <strong className="truncate text-sm">{library.source_name}</strong>
                  <span className="truncate text-muted-foreground" title={library.source_path}>
                    {library.track_count} tracks · {library.playlist_count} playlists
                  </span>
                </button>
              ))}
            </CardContent>
          </Card>

          <Card className="flex min-h-0 flex-col overflow-hidden">
            <CardHeader>
              <CardTitle>{t("Playlists origen")}</CardTitle>
              <span className="text-xs text-muted-foreground">{playlists.length}</span>
            </CardHeader>
            <CardContent className="overflow-y-auto">
              {!activeLibrary ? <EmptyRow>{t("Sin libreria activa")}</EmptyRow> : null}
              {activeLibrary && playlists.length === 0 ? <EmptyRow>{t("Sin playlists indexadas")}</EmptyRow> : null}
              {playlists.map((playlist) => (
                <button
                  key={playlist.path}
                  type="button"
                  className={cn(
                    "grid w-full grid-cols-[10px_minmax(0,1fr)_48px] items-center gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-secondary",
                    playlist.path === activePlaylistPath && activeTab !== "index" && "bg-muted",
                    activeTab === "index" && selectedPreviewPlaylistPaths.has(playlist.path) && "bg-muted"
                  )}
                  onClick={() => void selectPlaylist(playlist.path)}
                  title={playlist.path}
                >
                  <PlaylistIndexStatusDot status={playlistIndexStatus(playlist.path)} />
                  <span className="truncate">{playlist.path}</span>
                  <strong className="text-right tabular-nums">{playlist.track_count}</strong>
                </button>
              ))}
            </CardContent>
          </Card>
        </aside>

        <section className="min-h-0">
          {activeTab === "index" ? (
            <Card className="flex h-full min-h-0 flex-col overflow-hidden">
              <CardHeader>
                <div className="min-w-0">
                  <CardTitle>{t("Playlists del XML")}</CardTitle>
                  <span className="block truncate text-xs text-muted-foreground" title={indexSourcePath}>
                    {indexablePlaylists.length > 0
                      ? t("{tracks} tracks en coleccion · {playlists} playlists disponibles", {
                          tracks: indexTrackCount,
                          playlists: indexablePlaylists.length
                        })
                      : indexSourcePath || t("Sin XML indexado")}
                  </span>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-2">
                  <Button variant="secondary" size="sm" disabled={busy || indexablePlaylists.length === 0} onClick={toggleAllPreviewPlaylists}>
                    {allPreviewPlaylistsSelected ? t("Deseleccionar") : t("Todos")}
                  </Button>
                  <Button
                    size="sm"
                    disabled={busy || !indexSourcePath || selectedPreviewPlaylistPaths.size === 0}
                    onClick={() => void indexSelectedPreviewPlaylists()}
                  >
                    <Database className="h-3.5 w-3.5" />
                    {t("Indexar {count} playlists", { count: selectedPreviewPlaylistPaths.size })}
                  </Button>
                  <Button variant="secondary" size="sm" disabled={busy || !indexSourcePath || indexablePlaylists.length === 0} onClick={() => void indexAllPreviewPlaylists()}>
                    {t("Indexar todo")}
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="overflow-y-auto">
                {indexablePlaylists.length === 0 ? <EmptyRow>{t("Elige un XML para revisar sus playlists antes de indexar.")}</EmptyRow> : null}
                {indexablePlaylists.map((playlist) => {
                  const status = playlistIndexStatus(playlist.path);

                  return (
                    <div
                      key={playlist.path}
                      className={cn(
                        "relative grid min-h-9 cursor-pointer grid-cols-[22px_14px_minmax(0,1fr)_64px_112px_auto] items-center gap-2 overflow-hidden border-b border-border px-3 text-xs hover:bg-secondary max-md:grid-cols-[22px_14px_minmax(0,1fr)_64px]",
                        status === "indexing" && "bg-primary/5"
                      )}
                      title={playlist.path}
                      onClick={() => togglePreviewPlaylist(playlist.path)}
                    >
                      <input
                        type="checkbox"
                        checked={selectedPreviewPlaylistPaths.has(playlist.path)}
                        onChange={() => togglePreviewPlaylist(playlist.path)}
                        onClick={(event) => event.stopPropagation()}
                      />
                      <PlaylistIndexStatusDot status={status} />
                      <span className="truncate">{playlist.path}</span>
                      <strong className="text-right tabular-nums">{playlist.track_count}</strong>
                      <PlaylistIndexStatusBadge status={status} />
                      <Button
                        variant="secondary"
                        size="sm"
                        className="max-md:hidden"
                        disabled={busy || status === "indexing"}
                        onClick={(event) => {
                          event.stopPropagation();
                          void indexXml(indexSourcePath, [playlist.path]);
                        }}
                      >
                        <Database className="h-3.5 w-3.5" />
                        {t("Indexar")}
                      </Button>
                      {status === "indexing" ? (
                        <span className="absolute inset-x-0 bottom-0 h-0.5 overflow-hidden bg-primary/15">
                          <span className="block h-full w-1/3 animate-[playlist-index-row_1s_ease-in-out_infinite] rounded-full bg-primary" />
                        </span>
                      ) : null}
                    </div>
                  );
                })}
              </CardContent>
            </Card>
          ) : null}

          {activeTab === "search" ? (
            <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-3">
              <Card className="p-3">
                <form className="grid grid-cols-[minmax(0,1fr)_auto_auto_auto] gap-2 max-lg:grid-cols-1" onSubmit={searchTracks}>
                  <input
                    className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                    value={searchQuery}
                    placeholder={t("Buscar por titulo, artista, album, mood...")}
                    onChange={(event) => setSearchQuery(event.currentTarget.value)}
                  />
                  <Button type="submit" disabled={busy || !activeLibraryId}>
                    <Search className="h-4 w-4" />
                    {t("Buscar")}
                  </Button>
                  <InfoPopover
                    title={t("Modo Vector")}
                    body={t("Al buscar, envia solo el texto de busqueda a OpenAI para generar un embedding temporal y compara contra vectores guardados en SQLite local. No reindexa tracks.")}
                  >
                    <Button
                      type="button"
                      variant={semanticSearch ? "default" : "secondary"}
                      disabled={!activeLibraryId}
                      onClick={() => setSemanticSearch((current) => !current)}
                    >
                      <Sparkles className="h-4 w-4" />
                      {t("Vector")}
                    </Button>
                  </InfoPopover>
                  <InfoPopover
                    title={t("Indexar vectores")}
                    body={t("Genera embeddings de metadata de tracks con OpenAI y los guarda en SQLite local. No sube audio; solo texto como titulo, artista, album, playlists y location.")}
                  >
                    <Button type="button" variant="secondary" disabled={busy || !activeLibraryId} onClick={() => void generateEmbeddings()}>
                      <Database className="h-4 w-4" />
                      {t("Indexar vectores")}
                    </Button>
                  </InfoPopover>
                </form>
              </Card>

              <Card className="flex min-h-0 flex-col overflow-hidden">
                <CardHeader>
                  <div className="min-w-0">
                    <CardTitle>{t("Busqueda")}</CardTitle>
                    <span className="block truncate text-xs text-muted-foreground">
                      {searchResults.length} {t("resultados")} · {selectedTrackIds.size} {t("seleccionados")}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <Button variant="secondary" size="sm" disabled={searchResults.length === 0} onClick={selectAllSearchResults}>
                      {selectedTrackIds.size === searchResults.length ? t("Deseleccionar") : t("Todos")}
                    </Button>
                    <Button size="sm" disabled={!activeDraftId || selectedTrackIds.size === 0 || busy} onClick={() => void addSelectedToDraft()}>
                      <Plus className="h-3.5 w-3.5" />
                      {t("Agregar")}
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="overflow-x-hidden overflow-y-auto">
                  <div className="playlist-index-track-grid sticky top-0 z-10 bg-secondary text-xs font-semibold text-muted-foreground">
                    <span />
                    <span />
                    <span>{t("Tema")}</span>
                    <span>{t("Artista")}</span>
                    <span>{t("Album")}</span>
                    <span>{t("Formato")}</span>
                    <span>{t("Score")}</span>
                    <span>{t("Acciones")}</span>
                  </div>
                  {searchResults.length === 0 ? <EmptyRow>{t("Busca tracks indexados o deja la busqueda vacia para listar.")}</EmptyRow> : null}
                  {searchResults.map((result) => (
                    <TrackRow
                      key={`${result.track.library_id}-${result.track.track_id}`}
                      track={result.track}
                      selected={selectedTrackIds.has(result.track.track_id)}
                      score={scoreLabel(result)}
                      onToggle={() => toggleSearchTrack(result.track.track_id)}
                      onPlay={() => result.track.source_path && void togglePathPlayback(result.track.source_path, result.track.name ?? result.track.source_path)}
                      onDetails={() => openTrackDetail(result.track)}
                      onReveal={() => void reveal(result.track.source_path)}
                      onOpenFolder={() => void openFolder(result.track.source_path)}
                      playing={Boolean(result.track.source_path && player?.path === result.track.source_path && playerPlaying)}
                    />
                  ))}
                </CardContent>
              </Card>
            </section>
          ) : null}

          {activeTab === "playlist" ? (
            <section className="grid h-full min-h-0 grid-cols-[260px_minmax(0,1fr)] gap-3 max-lg:grid-cols-1">
              <Card className="flex min-h-0 flex-col overflow-hidden">
                <CardHeader>
                  <CardTitle>{t("Drafts")}</CardTitle>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">{drafts.length}</span>
                    <Button size="sm" disabled={!activeLibraryId} onClick={() => setCreateDraftSheetOpen(true)}>
                      <Plus className="h-3.5 w-3.5" />
                      {t("Nueva")}
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="overflow-y-auto">
                  {drafts.length === 0 ? <EmptyRow>{t("Sin playlists nuevas.")}</EmptyRow> : null}
                  {drafts.map((draft) => (
                    <button
                      key={draft.id}
                      type="button"
                      className={cn(
                        "grid w-full grid-cols-[minmax(0,1fr)_48px] items-center gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-secondary",
                        draft.id === activeDraftId && "bg-muted"
                      )}
                      onClick={() => void selectDraft(draft.id)}
                    >
                      <span className="truncate font-semibold">{draft.name}</span>
                      <span className="text-right tabular-nums">{draft.track_count}</span>
                    </button>
                  ))}
                </CardContent>
              </Card>

              <section className="grid min-h-0 grid-rows-[minmax(0,1fr)_minmax(0,240px)] gap-3">
                <Card className="flex min-h-0 flex-col overflow-hidden">
                  <CardHeader>
                    <div className="min-w-0">
                      <CardTitle>{activeDraft?.name ?? t("Playlist nueva")}</CardTitle>
                      <span className="block truncate text-xs text-muted-foreground">
                        {draftTracks.length} {t("tracks")}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <Button variant="secondary" size="sm" disabled={!activeDraftId || draftTracks.length === 0 || busy} onClick={() => void exportDraft()}>
                        <FileOutput className="h-3.5 w-3.5" />
                        {t("Exportar")}
                      </Button>
                      <Button variant="secondary" size="icon" disabled={!activeDraftId || busy} title={t("Eliminar")} onClick={() => void deleteDraft()}>
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </CardHeader>
                  <CardContent className="overflow-y-auto">
                    {!activeDraftId ? <EmptyRow>{t("Crea o selecciona una playlist.")}</EmptyRow> : null}
                    {activeDraftId && draftTracks.length === 0 ? <EmptyRow>{t("Agrega tracks desde la busqueda o desde una playlist origen.")}</EmptyRow> : null}
                    {draftTracks.map((track, index) => (
                      <div key={`${track.track_id}-${index}`} className="grid min-h-10 grid-cols-[28px_minmax(0,1fr)_96px_28px] items-center gap-2 border-b border-border px-3 text-xs">
                        <span className="text-muted-foreground">{index + 1}</span>
                        <div className="min-w-0">
                          <strong className="block truncate">{track.name ?? track.track_id}</strong>
                          <span className="block truncate text-muted-foreground">{track.artist ?? ""}</span>
                        </div>
                        <TrackIndexBadges track={track} />
                        <Button variant="ghost" size="icon" title={t("Quitar")} onClick={() => void removeDraftTrack(track.track_id)}>
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    ))}
                  </CardContent>
                </Card>

                <Card className="flex min-h-0 flex-col overflow-hidden">
                  <CardHeader>
                    <div className="min-w-0">
                      <CardTitle>{t("Playlist origen")}</CardTitle>
                      <span className="block truncate text-xs text-muted-foreground" title={activePlaylistPath}>
                        {activePlaylist?.path ?? t("Sin playlist seleccionada")}
                      </span>
                    </div>
                    <Button size="sm" disabled={!activeDraftId || playlistTracks.length === 0 || busy} onClick={() => void addPlaylistToDraft()}>
                      <Plus className="h-3.5 w-3.5" />
                      {t("Agregar playlist")}
                    </Button>
                  </CardHeader>
                  <CardContent className="overflow-y-auto">
                    {!activePlaylistPath ? <EmptyRow>{t("Elige una playlist origen.")}</EmptyRow> : null}
                    {activePlaylistPath && playlistTracks.length === 0 ? <EmptyRow>{t("Playlist sin tracks.")}</EmptyRow> : null}
                    {playlistTracks.map((track) => (
                      <CompactTrackRow
                        key={`${track.track_id}-${track.source_path ?? ""}`}
                        track={track}
                        onPlay={() => track.source_path && void togglePathPlayback(track.source_path, track.name ?? track.source_path)}
                        playing={Boolean(track.source_path && player?.path === track.source_path && playerPlaying)}
                      />
                    ))}
                  </CardContent>
                </Card>
              </section>
            </section>
          ) : null}
        </section>
      </section>

      {createDraftSheetOpen ? (
        <div className="fixed inset-0 z-[65]">
          <div className="absolute inset-0 bg-black/25 backdrop-blur-[1px]" onClick={() => setCreateDraftSheetOpen(false)} />
          <aside className="absolute right-0 top-0 z-[70] flex h-full w-[420px] max-w-[calc(100vw-16px)] flex-col border-l border-border bg-background shadow-2xl">
            <header className="flex min-h-14 items-center justify-between gap-3 border-b border-border bg-card px-4">
              <div className="min-w-0">
                <h2 className="truncate text-base font-semibold">{t("Crear playlist")}</h2>
                <p className="truncate text-xs text-muted-foreground">{activeLibrary?.source_name ?? t("Sin libreria activa")}</p>
              </div>
              <Button variant="ghost" size="sm" onClick={() => setCreateDraftSheetOpen(false)}>
                {t("Cerrar")}
              </Button>
            </header>
            <form className="grid gap-3 p-4" onSubmit={createDraft}>
              <label className="grid gap-1 text-sm font-medium">
                {t("Nombre")}
                <input
                  className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                  value={draftName}
                  placeholder={t("Nueva playlist")}
                  onChange={(event) => setDraftName(event.currentTarget.value)}
                />
              </label>
              <label className="grid gap-1 text-sm font-medium">
                {t("Descripcion")}
                <textarea
                  className="min-h-28 min-w-0 resize-none rounded-md border border-input bg-background px-3 py-2 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                  value={draftDescription}
                  placeholder={t("Descripcion opcional")}
                  onChange={(event) => setDraftDescription(event.currentTarget.value)}
                />
              </label>
              <Button type="submit" disabled={busy || !activeLibraryId || !draftName.trim()}>
                <Plus className="h-4 w-4" />
                {t("Crear playlist")}
              </Button>
            </form>
          </aside>
        </div>
      ) : null}

      <PlaylistTrackDetailSheet
        open={detailSheetOpen}
        track={detailTrack}
        onClose={() => setDetailSheetOpen(false)}
        onPlay={(track) =>
          track.source_path &&
          void togglePathPlayback(track.source_path, track.name ?? track.source_path)
        }
        onReveal={(track) => void reveal(track.source_path)}
        onOpenFolder={(track) => void openFolder(track.source_path)}
      />

      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle={t("playlist index / embeddings / export")}
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={() => setTerminalLogs([])}
      />
    </main>
  );
}

function TrackRow({
  track,
  selected,
  score,
  playing,
  onToggle,
  onPlay,
  onDetails,
  onReveal,
  onOpenFolder
}: {
  track: PlaylistIndexTrack;
  selected: boolean;
  score: string;
  playing: boolean;
  onToggle: () => void;
  onPlay: () => void;
  onDetails: () => void;
  onReveal: () => void;
  onOpenFolder: () => void;
}) {
  const { t } = useI18n();

  return (
    <div className={cn("playlist-index-track-grid border-b border-border text-xs", !track.source_exists && "bg-red-50 dark:bg-red-950/30")}>
      <input type="checkbox" checked={selected} onChange={onToggle} />
      <Button variant={playing ? "default" : "secondary"} size="icon" disabled={!track.source_exists || !track.source_path} onClick={onPlay}>
        {playing ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </Button>
      <span className="flex min-w-0 items-center gap-2" title={track.name ?? track.track_id}>
        <button
          type="button"
          className="min-w-0 truncate text-left font-medium underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onClick={onDetails}
        >
          {track.name ?? track.track_id}
        </button>
        <TrackIndexBadges track={track} />
      </span>
      <span className="truncate" title={track.artist ?? ""}>{track.artist ?? ""}</span>
      <span className="truncate" title={track.album ?? ""}>{track.album ?? ""}</span>
      <span className="truncate">{track.kind ?? ""}</span>
      <span className="truncate tabular-nums">{score}</span>
      <div className="flex justify-end gap-1">
        <Button variant="secondary" size="icon" disabled={!track.source_path} title={t("Mostrar en Finder")} onClick={onReveal}>
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
        <Button variant="secondary" size="icon" disabled={!track.source_path} title={t("Abrir carpeta")} onClick={onOpenFolder}>
          <FolderOpen className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

function InfoPopover({
  title,
  body,
  children
}: {
  title: string;
  body: string;
  children: React.ReactNode;
}) {
  return (
    <div className="group relative inline-flex min-w-0">
      {children}
      <div className="pointer-events-none absolute right-0 top-[calc(100%+8px)] z-50 hidden w-80 rounded-md border border-border bg-card p-3 text-card-foreground shadow-lg group-hover:block group-focus-within:block">
        <strong className="block text-sm">{title}</strong>
        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{body}</p>
      </div>
    </div>
  );
}

function TrackIndexBadges({ track }: { track: PlaylistIndexTrack }) {
  const { t } = useI18n();

  return (
    <span className="inline-flex shrink-0 items-center gap-1">
      <span
        className="rounded-sm border border-emerald-200 bg-emerald-50 px-1 py-0.5 text-[10px] font-semibold leading-none text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200"
        title={t("Track indexado en SQLite")}
      >
        SQL
      </span>
      <span
        className={cn(
          "rounded-sm border px-1 py-0.5 text-[10px] font-semibold leading-none",
          track.embedding_ready
            ? "border-primary bg-primary text-primary-foreground"
            : "border-border bg-secondary text-muted-foreground"
        )}
        title={track.embedding_ready ? t("Vector indexado") : t("Vector pendiente")}
      >
        VEC
      </span>
    </span>
  );
}

function PlaylistIndexStatusDot({ status }: { status: PlaylistIndexPlaylistStatus }) {
  return (
    <span
      className={cn(
        "inline-block h-2.5 w-2.5 shrink-0 rounded-full border",
        status === "pending" && "border-border bg-muted",
        status === "queued" && "border-amber-400 bg-amber-400 shadow-[0_0_0_3px_rgba(251,191,36,0.16)]",
        status === "indexing" && "animate-pulse border-amber-400 bg-amber-400 shadow-[0_0_0_4px_rgba(251,191,36,0.22)]",
        status === "indexed" && "border-emerald-500 bg-emerald-500"
      )}
    />
  );
}

function PlaylistIndexStatusBadge({ status }: { status: PlaylistIndexPlaylistStatus }) {
  const { t } = useI18n();

  return (
    <span
      className={cn(
        "inline-flex h-6 items-center justify-center gap-1 rounded-md border px-2 text-[11px] font-semibold max-md:hidden",
        status === "pending" && "border-border bg-secondary text-muted-foreground",
        status === "queued" && "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/50 dark:text-amber-200",
        status === "indexing" && "border-primary/40 bg-primary/10 text-primary",
        status === "indexed" && "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200"
      )}
    >
      <PlaylistIndexStatusDot status={status} />
      {status === "indexed"
        ? t("Indexada")
        : status === "indexing"
          ? t("Indexando")
          : status === "queued"
            ? t("En cola")
            : t("Pendiente")}
    </span>
  );
}

function PlaylistTrackDetailSheet({
  open,
  track,
  onClose,
  onPlay,
  onReveal,
  onOpenFolder
}: {
  open: boolean;
  track: PlaylistIndexTrack | null;
  onClose: () => void;
  onPlay: (track: PlaylistIndexTrack) => void;
  onReveal: (track: PlaylistIndexTrack) => void;
  onOpenFolder: (track: PlaylistIndexTrack) => void;
}) {
  const { t } = useI18n();

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open || !track) return null;

  const rows: Array<[string, React.ReactNode]> = [
    ["Track ID", track.track_id],
    [t("Titulo"), track.name],
    [t("Artista"), track.artist],
    ["Album", track.album],
    [t("Formato"), track.kind],
    ["Location", track.location]
  ];

  return (
    <div className="fixed inset-0 z-[65]">
      <div className="absolute inset-0 bg-black/25 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="absolute right-0 top-0 z-[70] flex h-full w-[500px] max-w-[calc(100vw-16px)] flex-col border-l border-border bg-background shadow-2xl">
        <header className="border-b border-border bg-card px-4 py-4">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <h2 className="truncate text-base font-semibold">{track.name ?? track.track_id}</h2>
              <p className="mt-1 truncate text-sm text-muted-foreground">{track.artist ?? t("Sin artista")}</p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <TrackIndexBadges track={track} />
                <StatusPill tone={track.source_exists ? "ok" : "error"}>
                  {track.source_exists ? t("Original encontrado") : t("Original no encontrado")}
                </StatusPill>
              </div>
            </div>
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("Cerrar")}
            </Button>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <section className="grid grid-cols-2 gap-2">
            <DetailStat label={t("Duracion")} value={track.total_time ? formatTime(track.total_time) : "n/d"} />
            <DetailStat label={t("Tamano")} value={track.size ? formatBytes(track.size) : "n/d"} />
            <DetailStat label="Sample rate" value={track.sample_rate ? `${track.sample_rate} Hz` : "n/d"} />
            <DetailStat label="Bitrate" value={track.bitrate ? `${track.bitrate} kbps` : "n/d"} />
          </section>

          <SheetBlock title={t("Metadata")}>
            <div className="grid gap-2">
              {rows.map(([label, value]) => (
                <DetailRow key={label} label={label} value={value} />
              ))}
            </div>
          </SheetBlock>

          <SheetBlock title={t("Rutas")}>
            <PathBlock label={t("Original")} value={track.source_path} missing={!track.source_exists} />
          </SheetBlock>

          <SheetBlock title={t("Acciones")}>
            <div className="flex flex-wrap gap-2">
              <Button disabled={!track.source_exists || !track.source_path} onClick={() => onPlay(track)}>
                <Play className="h-4 w-4" />
                {t("Play")}
              </Button>
              <Button variant="secondary" disabled={!track.source_path} onClick={() => onReveal(track)}>
                <ChevronRight className="h-4 w-4" />
                Finder
              </Button>
              <Button variant="secondary" disabled={!track.source_path} onClick={() => onOpenFolder(track)}>
                <FolderOpen className="h-4 w-4" />
                {t("Abrir carpeta")}
              </Button>
            </div>
          </SheetBlock>
        </div>
      </aside>
    </div>
  );
}

function DetailStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-card p-3">
      <span className="block text-[11px] font-semibold uppercase text-muted-foreground">{label}</span>
      <strong className="mt-2 block truncate text-sm">{value}</strong>
    </div>
  );
}

function SheetBlock({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-4 rounded-md border border-border bg-card">
      <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{title}</h3>
      <div className="p-3">{children}</div>
    </section>
  );
}

function DetailRow({ label, value }: { label: string; value: React.ReactNode }) {
  if (value === undefined || value === null || value === "") return null;

  return (
    <div className="grid grid-cols-[112px_minmax(0,1fr)] gap-3 rounded-md bg-secondary/60 px-3 py-2 text-xs">
      <span className="truncate font-semibold text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words">{value}</span>
    </div>
  );
}

function PathBlock({ label, value, missing }: { label: string; value?: string | null; missing: boolean }) {
  return (
    <div className={cn("rounded-md border p-3", missing ? "border-red-200 bg-red-50 dark:border-red-900 dark:bg-red-950/40" : "border-border bg-secondary/60")}>
      <span className="mb-2 block text-xs font-semibold text-muted-foreground">{label}</span>
      <p className="break-words font-mono text-[11px] leading-relaxed text-foreground">
        {value || "n/d"}
      </p>
    </div>
  );
}

function StatusPill({ tone, children }: { tone: "ok" | "error"; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border px-2 py-0.5 text-[11px] font-semibold",
        tone === "ok" && "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200",
        tone === "error" && "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200"
      )}
    >
      {children}
    </span>
  );
}

function PlaylistTabButton({
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

function CompactTrackRow({
  track,
  playing,
  onPlay
}: {
  track: PlaylistIndexTrack;
  playing: boolean;
  onPlay: () => void;
}) {
  return (
    <div className="grid min-h-10 grid-cols-[32px_minmax(0,1fr)_96px_72px] items-center gap-2 border-b border-border px-3 text-xs">
      <Button variant={playing ? "default" : "secondary"} size="icon" disabled={!track.source_exists || !track.source_path} onClick={onPlay}>
        {playing ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </Button>
      <div className="min-w-0">
        <strong className="block truncate">{track.name ?? track.track_id}</strong>
        <span className="block truncate text-muted-foreground">{track.artist ?? ""}</span>
      </div>
      <TrackIndexBadges track={track} />
      <span className="truncate text-right text-muted-foreground">{track.kind ?? ""}</span>
    </div>
  );
}

function IndexMetric({ label, value, danger = false }: { label: string; value: React.ReactNode; danger?: boolean }) {
  return (
    <Card className={cn("p-3", danger && "border-red-300 text-red-800 dark:border-red-900 dark:text-red-200")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block truncate text-xl">{value}</strong>
    </Card>
  );
}

function EmptyRow({ children }: { children: React.ReactNode }) {
  return <div className="flex min-h-11 items-center px-3 text-sm text-muted-foreground">{children}</div>;
}

function Progress({ value }: { value: number }) {
  return (
    <div className="h-2 overflow-hidden rounded-full bg-slate-200 dark:bg-slate-800">
      <div className="h-full rounded-full bg-primary" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

function normalizeLogLevel(level: string): TerminalLogEntry["level"] {
  if (level === "error") return "error";
  if (level === "warning") return "warning";
  return "info";
}

function scoreLabel(result: PlaylistSearchResult) {
  if (result.mode === "semantic") return result.score.toFixed(3);
  if (result.mode === "lexical") return result.score.toFixed(2);
  return "-";
}

function defaultExportPath(sourcePath: string, playlistName: string) {
  const cleanName = playlistName
    .trim()
    .replace(/[^\w.-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLowerCase() || "playlist";
  return sourcePath.replace(/\.xml$/i, "") + `.rau-studio.${cleanName}.xml`;
}

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => resolve());
    });
  });
}

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${remainingSeconds}`;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes < 0) return "n/d";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}
