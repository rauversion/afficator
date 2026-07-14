import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Database,
  GitGraph,
  RefreshCcw,
  Tags
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Core, ElementDefinition, LayoutOptions, StylesheetJson } from "cytoscape";
import type * as React from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { PlaylistAddDialog, type PlaylistDraftOption } from "./components/tracks/PlaylistAddDialog";
import { TrackDetailSheet } from "./components/tracks/TrackDetailSheet";
import { TrackTable } from "./components/tracks/TrackList";
import { useTrackPlayer } from "./components/tracks/useTrackPlayer";
import type { TrackListItem } from "./components/tracks/types";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

type TaxonomyTab = "overview" | "genres" | "bpm" | "graph";

type PlaylistIndexLibrary = {
  id: string;
  source_path: string;
  source_name: string;
  track_count: number;
  playlist_count: number;
};

type TaxonomyCount = {
  kind: string;
  value: string;
  name: string;
  count: number;
};

type TaxonomyOverview = {
  library: PlaylistIndexLibrary;
  track_count: number;
  playlist_count: number;
  genre_count: number;
  artist_count: number;
  album_count: number;
  key_count: number;
  bpm_known_count: number;
  bpm_missing_count: number;
  bpm_average?: number | null;
  bpm_min?: number | null;
  bpm_max?: number | null;
  genre_missing_count: number;
  key_missing_count: number;
  source_missing_count: number;
  genres: TaxonomyCount[];
  bpm_buckets: TaxonomyCount[];
  keys: TaxonomyCount[];
  formats: TaxonomyCount[];
  years: TaxonomyCount[];
  metadata_gaps: TaxonomyCount[];
};

type TaxonomyGraph = {
  nodes: TaxonomyGraphNode[];
  edges: TaxonomyGraphEdge[];
};

type TaxonomyGraphNode = {
  id: string;
  kind: string;
  value: string;
  label: string;
  count: number;
};

type TaxonomyGraphEdge = {
  id: string;
  source: string;
  target: string;
  count: number;
};

type TaxonomyTrack = {
  library_id: string;
  track_id: string;
  name?: string | null;
  artist?: string | null;
  album?: string | null;
  kind?: string | null;
  location?: string | null;
  genre?: string | null;
  comments?: string | null;
  bpm?: string | null;
  key?: string | null;
  rating?: string | null;
  year?: string | null;
  label?: string | null;
  date_added?: string | null;
  source_path?: string | null;
  source_exists: boolean;
  total_time?: number | null;
  attributes?: Record<string, string>;
};

type TaxonomySelection = {
  kind: string;
  value: string;
  label: string;
  count: number;
};

export function TaxonomyPage() {
  const { locale, t } = useI18n();
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [overview, setOverview] = useState<TaxonomyOverview | null>(null);
  const [graph, setGraph] = useState<TaxonomyGraph | null>(null);
  const [activeTab, setActiveTab] = useState<TaxonomyTab>("overview");
  const [selection, setSelection] = useState<TaxonomySelection | null>(null);
  const [selectionTracks, setSelectionTracks] = useState<TaxonomyTrack[]>([]);
  const [selectedTrackIds, setSelectedTrackIds] = useState<Set<string>>(new Set());
  const [drafts, setDrafts] = useState<PlaylistDraftOption[]>([]);
  const [detailTrack, setDetailTrack] = useState<TaxonomyTrack | null>(null);
  const [addPlaylistDialogOpen, setAddPlaylistDialogOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [playlistBusy, setPlaylistBusy] = useState(false);
  const [tracksLoading, setTracksLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const trackPlayer = useTrackPlayer({ t, onError: setErrorMessage });

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const topGenres = useMemo(() => (overview?.genres ?? []).filter((item) => item.count > 0).slice(0, 16), [overview]);
  const topKeys = useMemo(() => (overview?.keys ?? []).filter((item) => item.count > 0).slice(0, 14), [overview]);
  const metadataGaps = useMemo(() => (overview?.metadata_gaps ?? []).filter((item) => item.count > 0), [overview]);
  const selectedTracks = useMemo(
    () => selectionTracks.filter((track) => selectedTrackIds.has(track.track_id)),
    [selectedTrackIds, selectionTracks]
  );

  useEffect(() => {
    void loadLibraries();
  }, []);

  async function loadLibraries(preferredLibraryId = activeLibraryId) {
    setLoading(true);
    setErrorMessage("");

    try {
      const response = await invoke<PlaylistIndexLibrary[]>("playlist_index_libraries");
      setLibraries(response);
      const nextLibraryId = response.some((library) => library.id === preferredLibraryId)
        ? preferredLibraryId
        : response[0]?.id ?? "";
      setActiveLibraryId(nextLibraryId);
      if (nextLibraryId) {
        await Promise.all([loadTaxonomy(nextLibraryId), loadDrafts(nextLibraryId)]);
      } else {
        setOverview(null);
        setGraph(null);
        setDrafts([]);
        clearSelection();
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function changeLibrary(libraryId: string) {
    setActiveLibraryId(libraryId);
    clearSelection();
    setLoading(true);
    setErrorMessage("");

    try {
      await Promise.all([loadTaxonomy(libraryId), loadDrafts(libraryId)]);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function loadTaxonomy(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const [overviewResponse, graphResponse] = await Promise.all([
      invoke<TaxonomyOverview>("playlist_index_taxonomy_overview", { libraryId }),
      invoke<TaxonomyGraph>("playlist_index_taxonomy_graph", { libraryId, limit: 14 })
    ]);
    setOverview(overviewResponse);
    setGraph(graphResponse);
  }

  async function loadDrafts(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const response = await invoke<PlaylistDraftOption[]>("playlist_index_drafts", { libraryId });
    setDrafts(response);
  }

  function clearSelection() {
    setSelection(null);
    setSelectionTracks([]);
    setSelectedTrackIds(new Set());
    setDetailTrack(null);
    setAddPlaylistDialogOpen(false);
  }

  async function selectTaxonomy(nextSelection: TaxonomySelection) {
    if (!activeLibraryId) return;
    setSelection(nextSelection);
    setTracksLoading(true);
    setErrorMessage("");

    try {
      const tracks = await invoke<TaxonomyTrack[]>("playlist_index_taxonomy_tracks", {
        libraryId: activeLibraryId,
        kind: nextSelection.kind,
        value: nextSelection.value,
        limit: 500
      });
      setSelectionTracks(tracks);
      setSelectedTrackIds(new Set(tracks.map((track) => track.track_id)));
      setDetailTrack(null);
      setAddPlaylistDialogOpen(false);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setTracksLoading(false);
    }
  }

  async function openFolder(track: TrackListItem) {
    if (!track.source_path) return;
    try {
      await invoke("open_parent_folder", { path: track.source_path });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function addTracksToDraft(draftId: string, tracks = selectedTracks) {
    if (!draftId || tracks.length === 0) return;
    setPlaylistBusy(true);
    setErrorMessage("");
    setMessage("");

    try {
      const trackIds = uniqueTrackIds(tracks);
      const updatedTracks = await invoke<TaxonomyTrack[]>("playlist_index_add_tracks_to_draft", {
        draftId,
        trackIds
      });
      await loadDrafts(activeLibraryId);
      setAddPlaylistDialogOpen(false);
      setMessage(t("{count} tracks en la playlist.", { count: updatedTracks.length }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setPlaylistBusy(false);
    }
  }

  function toggleTrackSelection(track: TrackListItem) {
    setSelectedTrackIds((current) => {
      const next = new Set(current);
      if (next.has(track.track_id)) {
        next.delete(track.track_id);
      } else {
        next.add(track.track_id);
      }
      return next;
    });
  }

  function toggleAllSelection() {
    setSelectedTrackIds((current) => {
      if (selectionTracks.length > 0 && current.size === selectionTracks.length) {
        return new Set();
      }
      return new Set(selectionTracks.map((track) => track.track_id));
    });
  }

  async function createPlaylistFromTracks(name: string, description: string) {
    if (!activeLibraryId || selectedTracks.length === 0 || !name.trim()) return;
    setPlaylistBusy(true);
    setErrorMessage("");
    setMessage("");

    try {
      const draft = await invoke<PlaylistDraftOption>("playlist_index_create_draft", {
        libraryId: activeLibraryId,
        name,
        description: description || null
      });
      await addTracksToDraft(draft.id, selectedTracks);
      setMessage(t("Playlist creada: {name} con {count} tracks.", {
        name: draft.name,
        count: uniqueTrackIds(selectedTracks).length
      }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
      setPlaylistBusy(false);
    }
  }

  return (
    <main className="min-w-0 p-4 pb-20">
      {trackPlayer.audio}
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="min-w-0">
          <h1 className="m-0 text-2xl font-semibold tracking-normal">{t("Taxonomias")}</h1>
          <p className="mt-1 max-w-[72vw] truncate text-xs text-muted-foreground lg:max-w-[58vw]">
            {activeLibrary?.source_path ?? t("Indexa un XML para visualizar generos, BPM y relaciones.")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            className="h-10 max-w-80 rounded-md border border-input bg-background px-3 text-sm"
            value={activeLibraryId}
            onChange={(event) => void changeLibrary(event.currentTarget.value)}
            disabled={loading || libraries.length === 0}
          >
            {libraries.length === 0 ? <option value="">{t("Sin librerias indexadas")}</option> : null}
            {libraries.map((library) => (
              <option key={library.id} value={library.id}>
                {library.source_name}
              </option>
            ))}
          </select>
          <Button variant="secondary" disabled={loading || !activeLibraryId} onClick={() => void loadLibraries(activeLibraryId)}>
            <RefreshCcw className={cn("h-4 w-4", loading && "animate-spin")} />
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

      {!activeLibraryId ? (
        <Card className="p-6">
          <CardTitle>{t("Sin XML indexado")}</CardTitle>
          <p className="mt-2 max-w-xl text-sm text-muted-foreground">
            {t("Primero indexa una libreria en Playlist Library para crear taxonomias locales.")}
          </p>
        </Card>
      ) : null}

      {overview ? (
        <>
          <div className="mb-3 flex min-w-0 flex-wrap items-center gap-1 rounded-md border border-border bg-card p-1">
            <TaxonomyTabButton active={activeTab === "overview"} onClick={() => setActiveTab("overview")} icon={<Database className="h-4 w-4" />}>
              Overview
            </TaxonomyTabButton>
            <TaxonomyTabButton active={activeTab === "genres"} onClick={() => setActiveTab("genres")} icon={<Tags className="h-4 w-4" />}>
              {t("Generos")}
            </TaxonomyTabButton>
            <TaxonomyTabButton active={activeTab === "bpm"} onClick={() => setActiveTab("bpm")} icon={<Activity className="h-4 w-4" />}>
              BPM / Key
            </TaxonomyTabButton>
            <TaxonomyTabButton active={activeTab === "graph"} onClick={() => setActiveTab("graph")} icon={<GitGraph className="h-4 w-4" />}>
              {t("Grafo")}
            </TaxonomyTabButton>
          </div>

          {activeTab === "overview" ? (
            <section className="grid gap-3">
              <section className="grid grid-cols-2 gap-2 lg:grid-cols-4 xl:grid-cols-8">
                <TaxonomyMetric label={t("Tracks")} value={overview.track_count} />
                <TaxonomyMetric label={t("Playlists")} value={overview.playlist_count} />
                <TaxonomyMetric label={t("Generos")} value={overview.genre_count} />
                <TaxonomyMetric label={t("Artistas")} value={overview.artist_count} />
                <TaxonomyMetric label="Albums" value={overview.album_count} />
                <TaxonomyMetric label="Keys" value={overview.key_count} />
                <TaxonomyMetric label={t("BPM conocido")} value={overview.bpm_known_count} />
                <TaxonomyMetric label={t("Archivos faltantes")} value={overview.source_missing_count} danger={overview.source_missing_count > 0} />
              </section>

              <Card>
                <CardHeader>
                  <CardTitle>BPM</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="grid grid-cols-3 gap-2 text-sm max-sm:grid-cols-1">
                    <InlineStat label={t("Promedio")} value={formatBpm(overview.bpm_average)} />
                    <InlineStat label="Min" value={formatBpm(overview.bpm_min)} />
                    <InlineStat label="Max" value={formatBpm(overview.bpm_max)} />
                  </div>
                  <BpmHistogram
                    className="mt-4"
                    items={overview.bpm_buckets}
                    selected={selection}
                    onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                  />
                </CardContent>
              </Card>

              <section className="grid grid-cols-[minmax(0,1.35fr)_minmax(280px,0.65fr)] gap-3 max-xl:grid-cols-1">
                <Card>
                  <CardHeader>
                    <CardTitle>{t("Top generos")}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <BarList
                      items={topGenres}
                      selected={selection}
                      onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                    />
                  </CardContent>
                </Card>

                <div className="grid gap-3">
                  <Card>
                    <CardHeader>
                      <CardTitle>{t("Calidad metadata")}</CardTitle>
                    </CardHeader>
                    <CardContent>
                      <BarList
                        compact
                        items={metadataGaps}
                        selected={selection}
                        onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                      />
                    </CardContent>
                  </Card>
                </div>
              </section>

              <TaxonomyTracksPanel
                selection={selection}
                tracks={selectionTracks}
                selectedTrackIds={selectedTrackIds}
                loading={tracksLoading}
                isPlaying={trackPlayer.isPlaying}
                onClear={clearSelection}
                onDetails={(track) => setDetailTrack(track as TaxonomyTrack)}
                onOpenFolder={openFolder}
                onPlay={trackPlayer.toggleTrackPlayback}
                onOpenPlaylistDialog={() => setAddPlaylistDialogOpen(true)}
                onToggleAllTracks={toggleAllSelection}
                onToggleTrack={toggleTrackSelection}
              />
            </section>
          ) : null}

          {activeTab === "genres" ? (
            <section className="grid grid-cols-[minmax(0,1.15fr)_minmax(320px,0.85fr)] gap-3 max-xl:grid-cols-1">
              <Card>
                <CardHeader>
                  <CardTitle>{t("Distribucion de generos")}</CardTitle>
                </CardHeader>
                <CardContent>
                  <BarList
                    items={overview.genres}
                    selected={selection}
                    onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                  />
                </CardContent>
              </Card>

              <div className="grid gap-3">
                <Card>
                  <CardHeader>
                    <CardTitle>{t("Formatos")}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <BarList
                      compact
                      items={overview.formats}
                      selected={selection}
                      onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                    />
                  </CardContent>
                </Card>
                <Card>
                  <CardHeader>
                    <CardTitle>{t("Anos")}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <BarList
                      compact
                      items={overview.years}
                      selected={selection}
                      onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                    />
                  </CardContent>
                </Card>
              </div>

              <div className="xl:col-span-2">
                <TaxonomyTracksPanel
                  selection={selection}
                  tracks={selectionTracks}
                  selectedTrackIds={selectedTrackIds}
                  loading={tracksLoading}
                  isPlaying={trackPlayer.isPlaying}
                  onClear={clearSelection}
                  onDetails={(track) => setDetailTrack(track as TaxonomyTrack)}
                  onOpenFolder={openFolder}
                  onPlay={trackPlayer.toggleTrackPlayback}
                  onOpenPlaylistDialog={() => setAddPlaylistDialogOpen(true)}
                  onToggleAllTracks={toggleAllSelection}
                  onToggleTrack={toggleTrackSelection}
                />
              </div>
            </section>
          ) : null}

          {activeTab === "bpm" ? (
            <section className="grid grid-cols-[minmax(0,1fr)_minmax(320px,0.8fr)] gap-3 max-xl:grid-cols-1">
              <Card>
                <CardHeader>
                  <CardTitle>{t("Rangos de BPM")}</CardTitle>
                </CardHeader>
                <CardContent>
                  <BpmHistogram
                    tall
                    items={overview.bpm_buckets}
                    selected={selection}
                    onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                  />
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>Keys</CardTitle>
                </CardHeader>
                <CardContent>
                  <BarList
                    items={topKeys}
                    selected={selection}
                    onSelect={(item) => void selectTaxonomy(countToSelection(item))}
                  />
                </CardContent>
              </Card>

              <div className="xl:col-span-2">
                <TaxonomyTracksPanel
                  selection={selection}
                  tracks={selectionTracks}
                  selectedTrackIds={selectedTrackIds}
                  loading={tracksLoading}
                  isPlaying={trackPlayer.isPlaying}
                  onClear={clearSelection}
                  onDetails={(track) => setDetailTrack(track as TaxonomyTrack)}
                  onOpenFolder={openFolder}
                  onPlay={trackPlayer.toggleTrackPlayback}
                  onOpenPlaylistDialog={() => setAddPlaylistDialogOpen(true)}
                  onToggleAllTracks={toggleAllSelection}
                  onToggleTrack={toggleTrackSelection}
                />
              </div>
            </section>
          ) : null}

          {activeTab === "graph" ? (
            <section className="grid grid-cols-[minmax(0,1fr)_360px] gap-3 max-xl:grid-cols-1">
              <Card className="min-w-0 overflow-hidden">
                <CardHeader>
                  <div>
                    <CardTitle>{t("Relaciones")}</CardTitle>
                    <span className="mt-1 block text-xs text-muted-foreground">
                      {t("Genero, BPM y key conectados por co-ocurrencia. Haz click en un nodo para ver tracks.")}
                    </span>
                  </div>
                </CardHeader>
                <CardContent>
                  <TaxonomyGraphView
                    graph={graph}
                    selected={selection}
                    onSelect={(node) => void selectTaxonomy(nodeToSelection(node))}
                  />
                </CardContent>
              </Card>

              <TaxonomyTracksPanel
                selection={selection}
                tracks={selectionTracks}
                selectedTrackIds={selectedTrackIds}
                loading={tracksLoading}
                isPlaying={trackPlayer.isPlaying}
                onClear={clearSelection}
                onDetails={(track) => setDetailTrack(track as TaxonomyTrack)}
                onOpenFolder={openFolder}
                onPlay={trackPlayer.toggleTrackPlayback}
                onOpenPlaylistDialog={() => setAddPlaylistDialogOpen(true)}
                onToggleAllTracks={toggleAllSelection}
                onToggleTrack={toggleTrackSelection}
                compact
              />
            </section>
          ) : null}
        </>
      ) : null}

      <TrackDetailSheet
        track={detailTrack}
        onClose={() => setDetailTrack(null)}
        onOpenFolder={openFolder}
        onPlay={trackPlayer.toggleTrackPlayback}
      />
      <PlaylistAddDialog
        open={addPlaylistDialogOpen}
        busy={playlistBusy}
        drafts={drafts}
        trackCount={uniqueTrackIds(selectedTracks).length}
        contextLabel={selection ? t(selection.label) : t("Seleccion actual")}
        defaultName={selection ? t(selection.label) : t("Taxonomias")}
        onClose={() => setAddPlaylistDialogOpen(false)}
        onAddExisting={(draftId) => void addTracksToDraft(draftId, selectedTracks)}
        onCreate={(name, description) => void createPlaylistFromTracks(name, description)}
      />
    </main>
  );
}

function TaxonomyTabButton({
  active,
  icon,
  onClick,
  children
}: {
  active: boolean;
  icon: React.ReactNode;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex min-h-9 items-center gap-2 rounded-md px-3 text-sm font-semibold transition-colors",
        active ? "bg-primary text-primary-foreground shadow-sm" : "text-muted-foreground hover:bg-secondary hover:text-foreground"
      )}
    >
      {icon}
      {children}
    </button>
  );
}

function TaxonomyMetric({ label, value, danger = false }: { label: string; value: number; danger?: boolean }) {
  return (
    <Card className={cn("p-3", danger && "border-red-300 text-red-800 dark:border-red-900 dark:text-red-200")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block text-xl tabular-nums">{formatNumber(value)}</strong>
    </Card>
  );
}

function InlineStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-secondary px-3 py-2">
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block tabular-nums">{value}</strong>
    </div>
  );
}

function BarList({
  items,
  selected,
  compact = false,
  onSelect
}: {
  items: TaxonomyCount[];
  selected: TaxonomySelection | null;
  compact?: boolean;
  onSelect: (item: TaxonomyCount) => void;
}) {
  const { t } = useI18n();
  const max = Math.max(1, ...items.map((item) => item.count));

  if (items.length === 0) {
    return <EmptyTaxonomyState />;
  }

  return (
    <div className="w-full overflow-x-auto">
      <div className={cn("grid min-w-[560px] gap-1.5", compact && "min-w-[380px] gap-1")}>
        {items.map((item) => {
          const active = selected?.kind === item.kind && selected.value === item.value;
          return (
            <button
              key={`${item.kind}-${item.value}`}
              type="button"
              onClick={() => onSelect(item)}
              className={cn(
                "grid min-h-9 grid-cols-[minmax(0,1fr)_72px] items-center gap-3 rounded-md border border-transparent px-2 text-left text-sm hover:border-border hover:bg-secondary",
                active && "border-primary bg-primary/10"
              )}
            >
              <span className="min-w-0">
                <span className="block truncate font-medium">{t(item.name)}</span>
                <span className="mt-1 block h-1.5 overflow-hidden rounded-full bg-muted">
                  <span
                    className="block h-full rounded-full bg-primary"
                    style={{ width: `${Math.max(4, (item.count / max) * 100)}%` }}
                  />
                </span>
              </span>
              <strong className="text-right tabular-nums">{formatNumber(item.count)}</strong>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function BpmHistogram({
  items,
  selected,
  tall = false,
  className,
  onSelect
}: {
  items: TaxonomyCount[];
  selected: TaxonomySelection | null;
  tall?: boolean;
  className?: string;
  onSelect: (item: TaxonomyCount) => void;
}) {
  const { t } = useI18n();
  const max = Math.max(1, ...items.map((item) => item.count));
  const width = 760;
  const height = tall ? 300 : 180;
  const padding = 26;
  const chartHeight = height - padding * 2;
  const barGap = 10;
  const barWidth = items.length > 0 ? (width - padding * 2 - barGap * (items.length - 1)) / items.length : 0;

  if (items.length === 0) {
    return <EmptyTaxonomyState />;
  }

  return (
    <div className={cn("w-full overflow-x-auto", className)}>
      <svg viewBox={`0 0 ${width} ${height}`} className="h-auto min-w-[620px]">
        <line x1={padding} x2={width - padding} y1={height - padding} y2={height - padding} className="stroke-border" />
        {items.map((item, index) => {
          const barHeight = Math.max(4, (item.count / max) * chartHeight);
          const x = padding + index * (barWidth + barGap);
          const y = height - padding - barHeight;
          const active = selected?.kind === item.kind && selected.value === item.value;

          return (
            <g key={item.value} className="cursor-pointer" onClick={() => onSelect(item)}>
              <rect
                x={x}
                y={y}
                width={barWidth}
                height={barHeight}
                rx={6}
                className={cn(active ? "fill-primary" : "fill-muted-foreground/45 hover:fill-primary")}
              />
              <text x={x + barWidth / 2} y={y - 8} textAnchor="middle" className="fill-foreground text-[13px] font-semibold">
                {formatNumber(item.count)}
              </text>
              <text x={x + barWidth / 2} y={height - 7} textAnchor="middle" className="fill-muted-foreground text-[12px]">
                {t(item.name)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function TaxonomyGraphView({
  graph,
  selected,
  onSelect
}: {
  graph: TaxonomyGraph | null;
  selected: TaxonomySelection | null;
  onSelect: (node: TaxonomyGraphNode) => void;
}) {
  const { t } = useI18n();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const cyRef = useRef<Core | null>(null);
  const onSelectRef = useRef(onSelect);
  const stats = useMemo(() => graphStats(graph), [graph]);

  useEffect(() => {
    onSelectRef.current = onSelect;
  }, [onSelect]);

  useEffect(() => {
    if (!graph || graph.nodes.length === 0 || !containerRef.current) return;

    cyRef.current?.destroy();
    cyRef.current = null;
    const elements = taxonomyGraphElements(graph, t);
    let disposed = false;
    let createdCy: Core | null = null;

    void import("cytoscape").then(({ default: createCytoscape }) => {
      if (disposed || !containerRef.current) return;

      const cy = createCytoscape({
        container: containerRef.current,
        elements,
        minZoom: 0.28,
        maxZoom: 2.5,
        wheelSensitivity: 0.16,
        style: taxonomyGraphStyle(stats),
        layout: taxonomyGraphLayout()
      });

      cy.on("tap", "node", (event) => {
        const data = event.target.data() as TaxonomyGraphNode;
        onSelectRef.current(data);
      });

      cy.ready(() => {
        cy.fit(undefined, 48);
        cy.center();
        applyCytoscapeSelection(cy, selected);
      });

      createdCy = cy;
      cyRef.current = cy;
    });

    return () => {
      disposed = true;
      createdCy?.destroy();
      if (cyRef.current === createdCy) {
        cyRef.current = null;
      }
    };
  }, [graph, stats, t]);

  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) return;
    applyCytoscapeSelection(cy, selected);
  }, [selected]);

  if (!graph || graph.nodes.length === 0) {
    return <EmptyTaxonomyState />;
  }

  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-secondary px-3 py-2">
        <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
          <GraphLegend className="bg-emerald-500" label={t("Genero")} />
          <GraphLegend className="bg-amber-500" label="BPM" />
          <GraphLegend className="bg-violet-500" label="Key" />
          <span className="inline-flex items-center rounded-md border border-border bg-background px-2 py-1">
            {graph.nodes.length} {t("nodos")} · {graph.edges.length} {t("relaciones")}
          </span>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" size="sm" onClick={() => runCytoscapeLayout(cyRef.current)}>
            Re-layout
          </Button>
          <Button variant="secondary" size="sm" onClick={() => cyRef.current?.fit(undefined, 48)}>
            {t("Ajustar")}
          </Button>
        </div>
      </div>
      <div
        ref={containerRef}
        className="h-[620px] min-h-[460px] w-full overflow-hidden rounded-md border border-border bg-[#101114]"
      />
    </div>
  );
}

function GraphLegend({ className, label }: { className: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-md border border-border px-2 py-1">
      <span className={cn("h-2.5 w-2.5 rounded-full", className)} />
      {label}
    </span>
  );
}

function TaxonomyTracksPanel({
  selection,
  tracks,
  selectedTrackIds,
  loading,
  compact = false,
  isPlaying,
  onClear,
  onDetails,
  onOpenFolder,
  onOpenPlaylistDialog,
  onPlay,
  onToggleAllTracks,
  onToggleTrack
}: {
  selection: TaxonomySelection | null;
  tracks: TaxonomyTrack[];
  selectedTrackIds: Set<string>;
  loading: boolean;
  compact?: boolean;
  isPlaying: (track: TrackListItem) => boolean;
  onClear: () => void;
  onDetails: (track: TrackListItem) => void;
  onOpenFolder: (track: TrackListItem) => void;
  onOpenPlaylistDialog: () => void;
  onPlay: (track: TrackListItem) => void;
  onToggleAllTracks: () => void;
  onToggleTrack: (track: TrackListItem) => void;
}) {
  const { t } = useI18n();
  const selectedCount = tracks.reduce((total, track) => total + (selectedTrackIds.has(track.track_id) ? 1 : 0), 0);
  const allSelected = tracks.length > 0 && selectedCount === tracks.length;
  const someSelected = selectedCount > 0 && !allSelected;

  return (
    <Card className={cn("min-w-0", compact && "h-[560px] overflow-hidden")}>
      <CardHeader>
        <div className="min-w-0">
          <CardTitle>{selection ? t(selection.label) : t("Tracks")}</CardTitle>
          <span className="block text-xs text-muted-foreground">
            {selection
              ? t("{count} tracks en esta taxonomia", { count: selection.count })
              : t("Haz click en una barra o nodo para explorar tracks.")}
          </span>
        </div>
        {selection ? (
          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" size="sm" disabled={selectedCount === 0} onClick={onOpenPlaylistDialog}>
              {t("Agregar a playlist")}
            </Button>
            <Button variant="secondary" size="sm" onClick={onClear}>
              {t("Limpiar")}
            </Button>
          </div>
        ) : null}
      </CardHeader>
      <CardContent className={cn("min-h-32", compact && "h-[480px] overflow-y-auto")}>
        {selection && tracks.length > 0 ? (
          <div className="mb-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-secondary px-3 py-2 text-xs text-muted-foreground">
            <label className="flex items-center gap-2 font-semibold text-foreground">
              <IndeterminateCheckbox
                checked={allSelected}
                indeterminate={someSelected}
                onChange={onToggleAllTracks}
              />
              {allSelected ? t("Deseleccionar") : t("Seleccionar")}
            </label>
            <span>
              {selectedCount}/{tracks.length} {t("tracks seleccionados")}
            </span>
          </div>
        ) : null}
        {loading ? (
          <div className="flex min-h-24 items-center gap-2 text-sm text-muted-foreground">
            <RefreshCcw className="h-4 w-4 animate-spin" />
            {t("Cargando tracks")}
          </div>
        ) : null}
        {!loading && !selection ? <EmptyTaxonomyState /> : null}
        {!loading && selection && tracks.length === 0 ? <EmptyTaxonomyState /> : null}
        {!loading && tracks.length > 0 ? (
          <TrackTable
            tracks={tracks}
            columns={["artist", "album", "genre", "bpm", "key", "kind"]}
            playbackContext={{
              id: selection ? `taxonomy:${selection.kind}:${selection.value}` : "taxonomy:tracks",
              label: selection ? t(selection.label) : t("Tracks")
            }}
            selectedTrackIds={selectedTrackIds}
            isPlaying={isPlaying}
            onDetails={onDetails}
            onOpenFolder={onOpenFolder}
            onPlay={onPlay}
            onToggleTrack={onToggleTrack}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}

function EmptyTaxonomyState() {
  const { t } = useI18n();
  return <div className="flex min-h-16 items-center px-1 text-sm text-muted-foreground">{t("Sin datos para mostrar.")}</div>;
}

function IndeterminateCheckbox({
  checked,
  indeterminate,
  onChange
}: {
  checked: boolean;
  indeterminate: boolean;
  onChange: () => void;
}) {
  const ref = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <input
      ref={ref}
      type="checkbox"
      checked={checked}
      onChange={onChange}
    />
  );
}

function countToSelection(item: TaxonomyCount): TaxonomySelection {
  return {
    kind: item.kind,
    value: item.value,
    label: item.name,
    count: item.count
  };
}

function nodeToSelection(node: TaxonomyGraphNode): TaxonomySelection {
  return {
    kind: node.kind,
    value: node.value,
    label: node.label,
    count: node.count
  };
}

function uniqueTrackIds(tracks: Array<{ track_id: string }>) {
  return Array.from(new Set(tracks.map((track) => track.track_id)));
}

function graphStats(graph: TaxonomyGraph | null) {
  const nodeCounts = graph?.nodes.map((node) => node.count) ?? [1];
  const edgeCounts = graph?.edges.map((edge) => edge.count) ?? [1];
  const minNode = Math.min(...nodeCounts);
  const maxNode = Math.max(...nodeCounts);
  const minEdge = Math.min(...edgeCounts);
  const maxEdge = Math.max(...edgeCounts);

  return {
    minNode,
    maxNode: maxNode === minNode ? minNode + 1 : maxNode,
    minEdge,
    maxEdge: maxEdge === minEdge ? minEdge + 1 : maxEdge
  };
}

function taxonomyGraphElements(graph: TaxonomyGraph, t: (key: string) => string): ElementDefinition[] {
  return [
    ...graph.nodes.map((node) => ({
      data: {
        ...node,
        displayLabel: `${truncateGraphLabel(t(node.label), 20)}\n${formatShortNumber(node.count)}`
      }
    })),
    ...graph.edges.map((edge) => ({
      data: {
        ...edge,
        label: formatShortNumber(edge.count)
      }
    }))
  ];
}

function taxonomyGraphStyle(stats: ReturnType<typeof graphStats>): StylesheetJson {
  return [
    {
      selector: "node",
      style: {
        "background-color": "#10b981",
        "border-color": "#111827",
        "border-width": 2,
        color: "#f8fafc",
        "font-size": 10,
        "font-weight": 700,
        height: `mapData(count, ${stats.minNode}, ${stats.maxNode}, 36, 96)`,
        label: "data(displayLabel)",
        "min-zoomed-font-size": 7,
        "overlay-opacity": 0,
        "text-halign": "center",
        "text-outline-color": "#101114",
        "text-outline-width": 2,
        "text-valign": "center",
        "text-wrap": "wrap",
        "text-max-width": "92px",
        width: `mapData(count, ${stats.minNode}, ${stats.maxNode}, 36, 96)`
      }
    },
    {
      selector: "node[kind = 'genre']",
      style: {
        "background-color": "#10b981",
        shape: "round-rectangle"
      }
    },
    {
      selector: "node[kind = 'bpm']",
      style: {
        "background-color": "#f59e0b",
        shape: "ellipse"
      }
    },
    {
      selector: "node[kind = 'key']",
      style: {
        "background-color": "#8b5cf6",
        shape: "hexagon"
      }
    },
    {
      selector: "edge",
      style: {
        "curve-style": "bezier",
        "line-color": "#64748b",
        opacity: 0.42,
        "overlay-opacity": 0,
        "target-arrow-color": "#64748b",
        "target-arrow-shape": "triangle",
        width: `mapData(count, ${stats.minEdge}, ${stats.maxEdge}, 1.2, 8)`
      }
    },
    {
      selector: ".selected",
      style: {
        "border-color": "#f8fafc",
        "border-width": 5,
        "text-outline-color": "#020617",
        "z-index": 20
      }
    },
    {
      selector: ".selected-edge",
      style: {
        "line-color": "#f8fafc",
        opacity: 0.9,
        "target-arrow-color": "#f8fafc",
        "z-index": 18
      }
    },
    {
      selector: ".dimmed",
      style: {
        opacity: 0.16
      }
    }
  ];
}

function taxonomyGraphLayout(): LayoutOptions {
  return {
    name: "cose",
    animate: false,
    componentSpacing: 90,
    edgeElasticity: 140,
    fit: true,
    gravity: 0.18,
    idealEdgeLength: 118,
    nestingFactor: 1.1,
    nodeOverlap: 24,
    nodeRepulsion: 360000,
    numIter: 2600,
    padding: 48,
    randomize: true
  };
}

function runCytoscapeLayout(cy: Core | null) {
  if (!cy) return;
  cy.layout(taxonomyGraphLayout()).run();
  window.setTimeout(() => cy.fit(undefined, 48), 80);
}

function applyCytoscapeSelection(cy: Core, selected: TaxonomySelection | null) {
  cy.elements().removeClass("selected dimmed selected-edge");
  if (!selected) return;

  const matching = cy
    .nodes()
    .filter((node) => node.data("kind") === selected.kind && node.data("value") === selected.value);
  if (matching.empty()) return;

  const connectedEdges = matching.connectedEdges();
  const neighborhood = matching.closedNeighborhood();
  cy.elements().not(neighborhood).addClass("dimmed");
  matching.addClass("selected");
  connectedEdges.addClass("selected-edge");
}

function truncateGraphLabel(value: string, maxLength: number) {
  return value.length > maxLength ? `${value.slice(0, maxLength - 1)}...` : value;
}

function formatNumber(value: number) {
  return new Intl.NumberFormat().format(value);
}

function formatShortNumber(value: number) {
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
  return String(value);
}

function formatBpm(value?: number | null) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "n/d";
  return `${Math.round(value)} BPM`;
}
