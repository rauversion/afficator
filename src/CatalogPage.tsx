import { invoke } from "@tauri-apps/api/core";
import {
  Bookmark,
  BookmarkPlus,
  ChevronLeft,
  ChevronRight,
  Columns3,
  Database,
  ListFilter,
  ListMusic,
  LoaderCircle,
  Layers3,
  Play,
  RefreshCcw,
  Search,
  Sparkles,
  Star,
  Trash2,
  X
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { PlaylistAddDialog, type PlaylistDraftOption } from "./components/tracks/PlaylistAddDialog";
import { TrackDetailSheet } from "./components/tracks/TrackDetailSheet";
import { TrackTable } from "./components/tracks/TrackList";
import type { TrackListColumn, TrackListItem } from "./components/tracks/types";
import { useTrackPlayer } from "./components/tracks/useTrackPlayer";
import type { EnrichmentProviderDescriptor } from "./enrichmentProviders";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

type PlaylistIndexLibrary = {
  id: string;
  source_name: string;
  source_path: string;
  track_count: number;
  playlist_count: number;
};

type CatalogFacetValue = {
  value: string;
  name: string;
  count: number;
};

type CatalogFacets = {
  genres: CatalogFacetValue[];
  artists: CatalogFacetValue[];
  albums: CatalogFacetValue[];
  keys: CatalogFacetValue[];
  years: CatalogFacetValue[];
  formats: CatalogFacetValue[];
  ratings: CatalogFacetValue[];
  metadata_gaps: CatalogFacetValue[];
  availability: CatalogFacetValue[];
};

type CatalogResponse = {
  items: TrackListItem[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
  facets: CatalogFacets;
  query_terms: string[];
};

type CatalogFilters = {
  genres: string[];
  artists: string[];
  albums: string[];
  keys: string[];
  years: string[];
  formats: string[];
  bpmMin?: number;
  bpmMax?: number;
  ratingMin?: number;
  metadataGaps: string[];
  availability: string[];
};

type EnrichmentRunResult = {
  processed_total: number;
  matched_total: number;
  no_match_total: number;
  failed_total: number;
};

type CatalogSavedSearch = {
  id: string;
  library_id: string;
  name: string;
  description?: string | null;
  query: string;
  filters: CatalogFilters;
  sort: string;
  result_count: number;
  last_evaluated_at: string;
  created_at: string;
  updated_at: string;
};

type CatalogSelectionResponse = {
  items: TrackListItem[];
  total: number;
  truncated: boolean;
};

const defaultColumns: TrackListColumn[] = ["artist", "album", "genre", "bpm", "key", "rating", "year", "label"];
const columnOptions: Array<{ value: TrackListColumn; label: string }> = [
  { value: "artist", label: "Artista" },
  { value: "album", label: "Album" },
  { value: "genre", label: "Genero" },
  { value: "bpm", label: "BPM" },
  { value: "key", label: "Key" },
  { value: "rating", label: "Rating" },
  { value: "year", label: "Ano" },
  { value: "label", label: "Label" },
  { value: "comments", label: "Comentarios" },
  { value: "kind", label: "Formato" }
];

function emptyFilters(): CatalogFilters {
  return {
    genres: [],
    artists: [],
    albums: [],
    keys: [],
    years: [],
    formats: [],
    metadataGaps: [],
    availability: []
  };
}

export function CatalogPage() {
  const { locale, t } = useI18n();
  const navigate = useNavigate();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const requestSequence = useRef(0);
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [filters, setFilters] = useState<CatalogFilters>(() => emptyFilters());
  const [sort, setSort] = useState("relevance");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(50);
  const [response, setResponse] = useState<CatalogResponse | null>(null);
  const [selectedTracks, setSelectedTracks] = useState<Map<string, TrackListItem>>(() => new Map());
  const [detailTrack, setDetailTrack] = useState<TrackListItem | null>(null);
  const [drafts, setDrafts] = useState<PlaylistDraftOption[]>([]);
  const [providers, setProviders] = useState<EnrichmentProviderDescriptor[]>([]);
  const [savedSearches, setSavedSearches] = useState<CatalogSavedSearch[]>([]);
  const [activeSavedSearchId, setActiveSavedSearchId] = useState("");
  const [activeSavedSearchBaseline, setActiveSavedSearchBaseline] = useState("");
  const [visibleColumns, setVisibleColumns] = useState<Set<TrackListColumn>>(() => new Set(defaultColumns));
  const [playlistDialogOpen, setPlaylistDialogOpen] = useState(false);
  const [saveSearchDialogOpen, setSaveSearchDialogOpen] = useState(false);
  const [deleteSearchDialogOpen, setDeleteSearchDialogOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [bootLoading, setBootLoading] = useState(true);
  const [playlistBusy, setPlaylistBusy] = useState(false);
  const [enrichmentBusy, setEnrichmentBusy] = useState(false);
  const [ratingBusy, setRatingBusy] = useState(false);
  const [selectionBusy, setSelectionBusy] = useState(false);
  const [savedSearchBusy, setSavedSearchBusy] = useState(false);
  const [refreshToken, setRefreshToken] = useState(0);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const trackPlayer = useTrackPlayer({ t, onError: setErrorMessage });

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const activeSavedSearch = savedSearches.find((savedSearch) => savedSearch.id === activeSavedSearchId) ?? null;
  const selectedTrackList = useMemo(() => Array.from(selectedTracks.values()), [selectedTracks]);
  const selectedTrackIds = useMemo(() => new Set(selectedTracks.keys()), [selectedTracks]);
  const orderedColumns = useMemo(
    () => columnOptions.map((option) => option.value).filter((column) => visibleColumns.has(column)),
    [visibleColumns]
  );
  const readyProviders = useMemo(() => providers.filter((provider) => provider.ready), [providers]);
  const visibleTracksSelected = Boolean(
    response?.items.length && response.items.every((track) => selectedTracks.has(track.track_id))
  );
  const filterChips = useMemo(() => buildFilterChips(filters), [filters]);
  const savedSearchDirty = Boolean(
    activeSavedSearch && activeSavedSearchBaseline !== catalogDefinitionKey(query, filters, sort)
  );

  useEffect(() => {
    void loadInitialData();
  }, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebouncedQuery(query), 250);
    return () => window.clearTimeout(timeout);
  }, [query]);

  useEffect(() => {
    if (!activeLibraryId) {
      setResponse(null);
      return;
    }
    void searchCatalog();
  }, [activeLibraryId, debouncedQuery, filters, page, pageSize, refreshToken, sort]);

  useEffect(() => {
    function focusSearch(event: KeyboardEvent) {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select, [contenteditable='true']")) return;
      event.preventDefault();
      searchInputRef.current?.focus();
    }
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  async function loadInitialData() {
    setBootLoading(true);
    setErrorMessage("");
    try {
      const [libraryResponse, providerResponse] = await Promise.all([
        invoke<PlaylistIndexLibrary[]>("playlist_index_libraries"),
        invoke<EnrichmentProviderDescriptor[]>("enrichment_providers")
      ]);
      setLibraries(libraryResponse);
      setProviders(providerResponse);
      const nextLibraryId = libraryResponse[0]?.id ?? "";
      setActiveLibraryId(nextLibraryId);
      if (nextLibraryId) {
        await Promise.all([loadDrafts(nextLibraryId), loadSavedSearches(nextLibraryId)]);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBootLoading(false);
    }
  }

  async function searchCatalog() {
    const requestId = ++requestSequence.current;
    setLoading(true);
    setErrorMessage("");
    try {
      const nextResponse = await invoke<CatalogResponse>("playlist_catalog_search", {
        request: {
          libraryId: activeLibraryId,
          query: debouncedQuery,
          filters,
          sort,
          page,
          pageSize
        }
      });
      if (requestId !== requestSequence.current) return;
      setResponse(nextResponse);
      if (nextResponse.page !== page) setPage(nextResponse.page);
      setSelectedTracks((current) => {
        if (current.size === 0) return current;
        const next = new Map(current);
        for (const track of nextResponse.items) {
          if (next.has(track.track_id)) next.set(track.track_id, track);
        }
        return next;
      });
    } catch (error) {
      if (requestId === requestSequence.current) {
        setErrorMessage(translateBackendMessage(locale, String(error)));
      }
    } finally {
      if (requestId === requestSequence.current) setLoading(false);
    }
  }

  async function loadDrafts(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const nextDrafts = await invoke<PlaylistDraftOption[]>("playlist_index_drafts", { libraryId });
    setDrafts(nextDrafts);
  }

  async function loadSavedSearches(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const searches = await invoke<CatalogSavedSearch[]>("playlist_catalog_saved_searches", { libraryId });
    setSavedSearches(searches);
  }

  async function changeLibrary(libraryId: string) {
    setActiveLibraryId(libraryId);
    setPage(1);
    setSelectedTracks(new Map());
    setDetailTrack(null);
    setActiveSavedSearchId("");
    setActiveSavedSearchBaseline("");
    setPlaylistDialogOpen(false);
    setSaveSearchDialogOpen(false);
    setMessage("");
    setErrorMessage("");
    try {
      await Promise.all([loadDrafts(libraryId), loadSavedSearches(libraryId)]);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function refreshCatalog() {
    setMessage("");
    setErrorMessage("");
    try {
      const [libraryResponse, providerResponse] = await Promise.all([
        invoke<PlaylistIndexLibrary[]>("playlist_index_libraries"),
        invoke<EnrichmentProviderDescriptor[]>("enrichment_providers")
      ]);
      setLibraries(libraryResponse);
      setProviders(providerResponse);
      if (activeLibraryId) {
        await Promise.all([loadDrafts(activeLibraryId), loadSavedSearches(activeLibraryId)]);
      }
      setRefreshToken((current) => current + 1);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  function updateQuery(value: string) {
    setQuery(value);
    setPage(1);
    setSelectedTracks(new Map());
  }

  function toggleArrayFilter(field: "genres" | "artists" | "albums" | "keys" | "years" | "formats" | "metadataGaps" | "availability", value: string) {
    setFilters((current) => {
      const values = current[field];
      return {
        ...current,
        [field]: values.includes(value) ? values.filter((item) => item !== value) : [...values, value]
      };
    });
    setPage(1);
    setSelectedTracks(new Map());
  }

  function setNumericFilter(field: "bpmMin" | "bpmMax" | "ratingMin", value?: number) {
    setFilters((current) => ({ ...current, [field]: value }));
    setPage(1);
    setSelectedTracks(new Map());
  }

  function clearFilters() {
    setFilters(emptyFilters());
    setPage(1);
    setSelectedTracks(new Map());
  }

  function toggleTrackSelection(track: TrackListItem) {
    setSelectedTracks((current) => {
      const next = new Map(current);
      if (next.has(track.track_id)) next.delete(track.track_id);
      else next.set(track.track_id, track);
      return next;
    });
  }

  function toggleVisibleSelection() {
    if (!response) return;
    setSelectedTracks((current) => {
      const next = new Map(current);
      if (visibleTracksSelected) {
        for (const track of response.items) next.delete(track.track_id);
      } else {
        for (const track of response.items) next.set(track.track_id, track);
      }
      return next;
    });
  }

  async function selectAllResults() {
    if (!activeLibraryId || !response?.total) return;
    setSelectionBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const selection = await invoke<CatalogSelectionResponse>("playlist_catalog_select_all", {
        request: {
          libraryId: activeLibraryId,
          query,
          filters,
          sort,
          page: 1,
          pageSize: 100
        },
        limit: 5000
      });
      setSelectedTracks(new Map(selection.items.map((track) => [track.track_id, track])));
      setMessage(
        selection.truncated
          ? t("Seleccionamos {count} de {total} resultados. El limite por operacion es 5000.", {
              count: selection.items.length,
              total: selection.total
            })
          : t("Seleccionamos los {count} resultados de esta busqueda.", { count: selection.total })
      );
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setSelectionBusy(false);
    }
  }

  function openSavedSearch(savedSearch: CatalogSavedSearch) {
    setQuery(savedSearch.query);
    setDebouncedQuery(savedSearch.query);
    setFilters(normalizeCatalogFilters(savedSearch.filters));
    setSort(savedSearch.sort);
    setPage(1);
    setSelectedTracks(new Map());
    setDetailTrack(null);
    setActiveSavedSearchId(savedSearch.id);
    setActiveSavedSearchBaseline(catalogDefinitionKey(savedSearch.query, savedSearch.filters, savedSearch.sort));
    setMessage(t("Smart collection cargada: {name}.", { name: savedSearch.name }));
    setErrorMessage("");
  }

  async function saveCurrentSearch(name: string, description: string, savedSearchId?: string) {
    if (!activeLibraryId || !name.trim()) return;
    setSavedSearchBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const saved = await invoke<CatalogSavedSearch>("playlist_catalog_save_search", {
        request: {
          id: savedSearchId || null,
          libraryId: activeLibraryId,
          name,
          description: description || null,
          query,
          filters,
          sort
        }
      });
      await loadSavedSearches(activeLibraryId);
      setActiveSavedSearchId(saved.id);
      setActiveSavedSearchBaseline(catalogDefinitionKey(query, filters, sort));
      setSaveSearchDialogOpen(false);
      setMessage(t("Smart collection guardada: {name}.", { name: saved.name }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setSavedSearchBusy(false);
    }
  }

  async function updateActiveSavedSearch() {
    if (!activeSavedSearch) return;
    await saveCurrentSearch(
      activeSavedSearch.name,
      activeSavedSearch.description ?? "",
      activeSavedSearch.id
    );
  }

  async function deleteActiveSavedSearch() {
    if (!activeSavedSearch || !activeLibraryId) return;
    setSavedSearchBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      await invoke("playlist_catalog_delete_saved_search", {
        libraryId: activeLibraryId,
        savedSearchId: activeSavedSearch.id
      });
      await loadSavedSearches(activeLibraryId);
      setActiveSavedSearchId("");
      setActiveSavedSearchBaseline("");
      setDeleteSearchDialogOpen(false);
      setMessage(t("Smart collection eliminada."));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setSavedSearchBusy(false);
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

  function playSelection() {
    const first = selectedTrackList.find((track) => track.source_exists && track.source_path);
    if (!first) return;
    void trackPlayer.toggleTrackPlayback(first, {
      id: `catalog-selection-${activeLibraryId}`,
      label: t("Seleccion del catalogo"),
      tracks: selectedTrackList
    });
  }

  async function addTracksToDraft(draftId: string, tracks = selectedTrackList) {
    if (!draftId || tracks.length === 0) return;
    setPlaylistBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const trackIds = uniqueTrackIds(tracks);
      const updated = await invoke<TrackListItem[]>("playlist_index_add_tracks_to_draft", { draftId, trackIds });
      await loadDrafts(activeLibraryId);
      setPlaylistDialogOpen(false);
      setMessage(t("{count} tracks agregados a la playlist.", { count: updated.length }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setPlaylistBusy(false);
    }
  }

  async function createPlaylist(name: string, description: string) {
    if (!activeLibraryId || selectedTrackList.length === 0) return;
    setPlaylistBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const draft = await invoke<PlaylistDraftOption>("playlist_index_create_draft", {
        libraryId: activeLibraryId,
        name,
        description: description || null
      });
      const trackIds = uniqueTrackIds(selectedTrackList);
      await invoke("playlist_index_add_tracks_to_draft", { draftId: draft.id, trackIds });
      await loadDrafts(activeLibraryId);
      setPlaylistDialogOpen(false);
      setMessage(t("Playlist creada: {name} con {count} tracks.", { name: draft.name, count: trackIds.length }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setPlaylistBusy(false);
    }
  }

  async function enrichSelection() {
    if (!activeLibraryId || selectedTrackList.length === 0) return;
    if (readyProviders.length === 0) {
      setErrorMessage(t("No hay fuentes de enrichment listas. Configuralas en Enrichment."));
      return;
    }
    setEnrichmentBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const trackIds = uniqueTrackIds(selectedTrackList);
      const result = await invoke<EnrichmentRunResult>("playlist_enrichment_run", {
        libraryId: activeLibraryId,
        providers: readyProviders.map((provider) => provider.id),
        limit: trackIds.length,
        trackIds
      });
      setMessage(
        t("Enrichment listo: {matched} matches, {missing} sin match y {failed} fallidos.", {
          matched: result.matched_total,
          missing: result.no_match_total,
          failed: result.failed_total
        })
      );
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setEnrichmentBusy(false);
    }
  }

  async function applyRating(rating: number) {
    if (!activeLibraryId || selectedTrackList.length === 0) return;
    setRatingBusy(true);
    setMessage("");
    setErrorMessage("");
    try {
      const trackIds = uniqueTrackIds(selectedTrackList);
      const updated = await invoke<number>("playlist_catalog_set_rating", {
        libraryId: activeLibraryId,
        trackIds,
        rating
      });
      setSelectedTracks((current) => {
        return new Map(
          Array.from(current.entries()).map(([trackId, track]) => [trackId, { ...track, user_rating: rating }])
        );
      });
      setResponse((current) => current ? { ...current, items: current.items.map((track) => (
        trackIds.includes(track.track_id) ? { ...track, user_rating: rating } : track
      )) } : current);
      setDetailTrack((current) => current && trackIds.includes(current.track_id)
        ? { ...current, user_rating: rating }
        : current);
      setRefreshToken((current) => current + 1);
      setMessage(t("Rating actualizado en {count} tracks.", { count: updated }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setRatingBusy(false);
    }
  }

  function toggleColumn(column: TrackListColumn) {
    setVisibleColumns((current) => {
      const next = new Set(current);
      if (next.has(column)) next.delete(column);
      else next.add(column);
      return next;
    });
  }

  return (
    <main className="min-w-0 p-4 pb-24">
      {trackPlayer.audio}
      <header className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-primary/10 text-primary">
            <Search className="h-5 w-5" />
          </span>
          <div>
            <h1 className="text-xl font-semibold">{t("Catalogo")}</h1>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("Busca toda tu libreria, combina atributos y convierte resultados en acciones.")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            className="h-9 max-w-72 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            value={activeLibraryId}
            onChange={(event) => void changeLibrary(event.currentTarget.value)}
            disabled={libraries.length === 0}
          >
            {libraries.map((library) => (
              <option key={library.id} value={library.id}>
                {library.source_name} · {library.track_count} tracks
              </option>
            ))}
          </select>
          <Button variant="secondary" size="icon" title={t("Actualizar")} onClick={() => void refreshCatalog()}>
            <RefreshCcw className={cn("h-4 w-4", loading && "animate-spin")} />
          </Button>
        </div>
      </header>

      {activeLibrary ? (
        <Card className="mb-4 overflow-visible">
          <CardHeader className="flex-wrap py-2">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Bookmark className="h-4 w-4 text-primary" />
                {t("Smart collections")}
              </CardTitle>
              <span className="mt-0.5 block text-xs text-muted-foreground">
                {t("Busquedas vivas que se recalculan cuando cambia tu libreria.")}
              </span>
            </div>
            <Button variant="secondary" size="sm" onClick={() => setSaveSearchDialogOpen(true)}>
              <BookmarkPlus className="h-3.5 w-3.5" />
              {t("Guardar busqueda")}
            </Button>
          </CardHeader>
          <CardContent className="overflow-x-auto p-3">
            <div className="flex min-w-max items-stretch gap-2">
              {savedSearches.length === 0 ? (
                <button
                  type="button"
                  className="flex min-h-16 min-w-64 items-center gap-3 rounded-md border border-dashed border-border px-3 text-left text-sm text-muted-foreground hover:border-primary/50 hover:text-foreground"
                  onClick={() => setSaveSearchDialogOpen(true)}
                >
                  <BookmarkPlus className="h-5 w-5" />
                  <span>
                    <strong className="block text-foreground">{t("Crea tu primera smart collection")}</strong>
                    <span className="mt-0.5 block text-xs">{t("Guarda la busqueda y sus filtros actuales.")}</span>
                  </span>
                </button>
              ) : null}
              {savedSearches.map((savedSearch) => {
                const active = savedSearch.id === activeSavedSearchId;
                const currentCount = active && !savedSearchDirty ? response?.total ?? savedSearch.result_count : savedSearch.result_count;
                return (
                  <button
                    key={savedSearch.id}
                    type="button"
                    className={cn(
                      "flex min-h-16 w-64 items-center gap-3 rounded-md border px-3 text-left transition",
                      active
                        ? "border-primary bg-primary/10 shadow-sm"
                        : "border-border bg-secondary/40 hover:border-primary/40 hover:bg-secondary/70"
                    )}
                    onClick={() => openSavedSearch(savedSearch)}
                  >
                    <span className={cn("grid h-9 w-9 shrink-0 place-items-center rounded-md", active ? "bg-primary text-primary-foreground" : "bg-background text-primary")}>
                      <Bookmark className="h-4 w-4" fill={active ? "currentColor" : "none"} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <strong className="block truncate text-sm">{savedSearch.name}</strong>
                      <span className="mt-0.5 block truncate text-xs text-muted-foreground">
                        {currentCount} tracks · {savedSearch.query || t("Solo filtros")}
                      </span>
                    </span>
                  </button>
                );
              })}
            </div>
          </CardContent>
        </Card>
      ) : null}

      {errorMessage ? (
        <div className="mb-3 flex items-start justify-between gap-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
          <span>{errorMessage}</span>
          <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => setErrorMessage("")}><X className="h-3.5 w-3.5" /></Button>
        </div>
      ) : null}
      {message ? (
        <div className="mb-3 flex items-start justify-between gap-3 rounded-md border border-emerald-300 bg-emerald-50 px-3 py-2 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200">
          <span>{message}</span>
          <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => setMessage("")}><X className="h-3.5 w-3.5" /></Button>
        </div>
      ) : null}

      <Card className="mb-4 overflow-visible bg-gradient-to-br from-primary/10 via-card to-card">
        <CardContent className="overflow-visible p-4">
          {activeSavedSearch ? (
            <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-primary/25 bg-background/80 px-3 py-2">
              <Bookmark className="h-4 w-4 text-primary" fill="currentColor" />
              <div className="min-w-0 flex-1">
                <strong className="block truncate text-sm">{activeSavedSearch.name}</strong>
                <span className="block text-xs text-muted-foreground">
                  {savedSearchDirty ? t("Hay cambios sin guardar en esta smart collection.") : t("Smart collection sincronizada.")}
                </span>
              </div>
              {savedSearchDirty ? (
                <Button size="sm" disabled={savedSearchBusy} onClick={() => void updateActiveSavedSearch()}>
                  {savedSearchBusy ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Bookmark className="h-3.5 w-3.5" />}
                  {t("Actualizar")}
                </Button>
              ) : null}
              <Button variant="ghost" size="icon" title={t("Eliminar smart collection")} disabled={savedSearchBusy} onClick={() => setDeleteSearchDialogOpen(true)}>
                <Trash2 className="h-4 w-4 text-destructive" />
              </Button>
            </div>
          ) : null}
          <div className="relative">
            <Search className="pointer-events-none absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 text-muted-foreground" />
            <input
              ref={searchInputRef}
              className="h-14 w-full rounded-md border border-input bg-background/95 pl-12 pr-16 text-base shadow-sm outline-none transition focus-visible:ring-2 focus-visible:ring-ring"
              placeholder={t("Busca titulo, artista, album, genero, label, comentarios...")}
              value={query}
              onChange={(event) => updateQuery(event.currentTarget.value)}
            />
            <kbd className="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 rounded border border-border bg-secondary px-2 py-1 text-[11px] text-muted-foreground">/</kbd>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>{t("Prueba")}</span>
            {["genre:House", "bpm:120..130", "rating:>=4", "missing:label"].map((example) => (
              <button
                key={example}
                type="button"
                className="rounded-full border border-border bg-background px-2.5 py-1 font-mono text-[11px] text-foreground transition hover:border-primary hover:text-primary"
                onClick={() => updateQuery(query.trim() ? `${query.trim()} ${example}` : example)}
              >
                {example}
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      {selectedTrackList.length > 0 ? (
        <section className="sticky top-2 z-30 mb-4 flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-background/95 p-2.5 shadow-lg backdrop-blur">
          <span className="mr-1 rounded-md bg-primary px-2.5 py-1.5 text-xs font-semibold text-primary-foreground">
            {selectedTrackList.length} {t("seleccionados")}
          </span>
          <Button size="sm" onClick={() => setPlaylistDialogOpen(true)}>
            <ListMusic className="h-3.5 w-3.5" /> {t("Playlist")}
          </Button>
          <Button variant="secondary" size="sm" disabled={!selectedTrackList.some((track) => track.source_exists)} onClick={playSelection}>
            <Play className="h-3.5 w-3.5" /> {t("Play")}
          </Button>
          <Button variant="secondary" size="sm" disabled={enrichmentBusy || readyProviders.length === 0} onClick={() => void enrichSelection()}>
            {enrichmentBusy ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Sparkles className="h-3.5 w-3.5" />}
            {t("Enriquecer")}
          </Button>
          <div className="flex items-center gap-0.5 rounded-md border border-border bg-secondary/50 px-2 py-1" aria-label={t("Aplicar rating")}>
            <span className="mr-1 text-[11px] font-semibold text-muted-foreground">{t("Rating")}</span>
            {[1, 2, 3, 4, 5].map((rating) => (
              <button
                key={rating}
                type="button"
                className="rounded p-0.5 text-amber-500 transition hover:scale-110 hover:bg-amber-100 disabled:opacity-50 dark:hover:bg-amber-950"
                title={t("Aplicar {count} estrellas", { count: rating })}
                disabled={ratingBusy}
                onClick={() => void applyRating(rating)}
              >
                <Star className="h-4 w-4" fill="currentColor" />
              </button>
            ))}
          </div>
          <Button variant="ghost" size="sm" className="ml-auto" onClick={() => setSelectedTracks(new Map())}>
            <X className="h-3.5 w-3.5" /> {t("Limpiar")}
          </Button>
        </section>
      ) : null}

      {libraries.length === 0 && !bootLoading ? (
        <Card>
          <CardContent className="grid place-items-center gap-3 p-10 text-center">
            <Database className="h-10 w-10 text-muted-foreground" />
            <div>
              <h2 className="font-semibold">{t("Aun no hay una libreria indexada")}</h2>
              <p className="mt-1 text-sm text-muted-foreground">{t("Importa un XML de Rekordbox para activar el catalogo.")}</p>
            </div>
            <Button onClick={() => navigate("/playlists")}>{t("Ir a Playlist Library")}</Button>
          </CardContent>
        </Card>
      ) : null}

      {activeLibrary ? (
        <div className="grid grid-cols-[250px_minmax(0,1fr)] items-start gap-4 max-xl:grid-cols-1">
          <Card className="sticky top-4 max-h-[calc(100vh-32px)] overflow-hidden max-xl:static max-xl:max-h-none">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><ListFilter className="h-4 w-4" />{t("Filtros")}</CardTitle>
              {filterChips.length > 0 ? <Button variant="ghost" size="sm" onClick={clearFilters}>{t("Limpiar")}</Button> : null}
            </CardHeader>
            <CardContent className="grid divide-y divide-border overflow-y-auto">
              <BpmFacet filters={filters} onChange={setNumericFilter} />
              <FacetSection title={t("Genero")} items={response?.facets.genres ?? []} selected={filters.genres} onToggle={(value) => toggleArrayFilter("genres", value)} />
              <FacetSection title={t("Artista")} items={response?.facets.artists ?? []} selected={filters.artists} onToggle={(value) => toggleArrayFilter("artists", value)} />
              <FacetSection title="Key" items={response?.facets.keys ?? []} selected={filters.keys} onToggle={(value) => toggleArrayFilter("keys", value)} />
              <RatingFacet items={response?.facets.ratings ?? []} selected={filters.ratingMin} onChange={(value) => setNumericFilter("ratingMin", value)} />
              <FacetSection title={t("Metadata faltante")} items={response?.facets.metadata_gaps ?? []} selected={filters.metadataGaps} onToggle={(value) => toggleArrayFilter("metadataGaps", value)} />
              <FacetSection title={t("Disponibilidad")} items={response?.facets.availability ?? []} selected={filters.availability} onToggle={(value) => toggleArrayFilter("availability", value)} />
              <FacetSection title={t("Formato")} items={response?.facets.formats ?? []} selected={filters.formats} onToggle={(value) => toggleArrayFilter("formats", value)} />
              <FacetSection title={t("Ano")} items={response?.facets.years ?? []} selected={filters.years} onToggle={(value) => toggleArrayFilter("years", value)} />
              <FacetSection title="Album" items={response?.facets.albums ?? []} selected={filters.albums} onToggle={(value) => toggleArrayFilter("albums", value)} />
            </CardContent>
          </Card>

          <section className="min-w-0">
            {filterChips.length > 0 ? (
              <div className="mb-3 flex flex-wrap items-center gap-2">
                {filterChips.map((chip) => (
                  <button
                    key={chip.key}
                    type="button"
                    className="inline-flex h-7 items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2.5 text-xs font-semibold text-primary hover:bg-primary/15"
                    onClick={() => {
                      removeFilterChip(chip, filters, setFilters, setPage);
                      setSelectedTracks(new Map());
                    }}
                  >
                    {chip.label}<X className="h-3 w-3" />
                  </button>
                ))}
              </div>
            ) : null}

            <Card className="min-w-0 overflow-hidden">
              <CardHeader className="flex-wrap py-2">
                <div className="min-w-0">
                  <CardTitle>{loading && !response ? t("Buscando...") : t("{count} tracks", { count: response?.total ?? 0 })}</CardTitle>
                  <span className="mt-0.5 block truncate text-xs text-muted-foreground">
                    {activeLibrary.source_name}{response?.query_terms.length ? ` · ${response.query_terms.join(" · ")}` : ""}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button variant="secondary" size="sm" disabled={!response?.items.length} onClick={toggleVisibleSelection}>
                    {visibleTracksSelected ? t("Quitar pagina") : t("Seleccionar pagina")}
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={!response?.total || selectionBusy}
                    onClick={() => void selectAllResults()}
                  >
                    {selectionBusy ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Layers3 className="h-3.5 w-3.5" />}
                    {t("Seleccionar todos ({count})", { count: Math.min(response?.total ?? 0, 5000) })}
                  </Button>
                  <label className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span>{t("Orden")}</span>
                    <select
                      className="h-8 rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none"
                      value={sort}
                      onChange={(event) => { setSort(event.currentTarget.value); setPage(1); }}
                    >
                      <option value="relevance">{t("Relevancia")}</option>
                      <option value="recent">{t("Mas recientes")}</option>
                      <option value="rating">{t("Mejor rating")}</option>
                      <option value="bpm">BPM</option>
                      <option value="title">{t("Titulo")}</option>
                    </select>
                  </label>
                  <ColumnChooser columns={visibleColumns} onToggle={toggleColumn} />
                </div>
              </CardHeader>
              <CardContent className="relative min-h-72 overflow-auto">
                {loading ? (
                  <div className="pointer-events-none absolute inset-x-0 top-0 z-20 h-0.5 overflow-hidden bg-primary/15">
                    <span className="block h-full w-1/3 animate-pulse bg-primary" />
                  </div>
                ) : null}
                <TrackTable
                  tracks={response?.items ?? []}
                  columns={orderedColumns}
                  selectedTrackIds={selectedTrackIds}
                  isPlaying={trackPlayer.isPlaying}
                  playbackContext={{ id: `catalog-${activeLibraryId}-${page}`, label: t("Catalogo") }}
                  onDetails={setDetailTrack}
                  onOpenFolder={openFolder}
                  onPlay={trackPlayer.toggleTrackPlayback}
                  onToggleTrack={toggleTrackSelection}
                  empty={
                    <div className="grid min-h-72 place-items-center p-8 text-center">
                      {loading ? (
                        <LoaderCircle className="h-7 w-7 animate-spin text-primary" />
                      ) : (
                        <div>
                          <Search className="mx-auto h-8 w-8 text-muted-foreground" />
                          <h3 className="mt-3 font-semibold">{t("No encontramos tracks")}</h3>
                          <p className="mt-1 text-sm text-muted-foreground">{t("Prueba quitando un filtro o usando una busqueda mas amplia.")}</p>
                        </div>
                      )}
                    </div>
                  }
                />
              </CardContent>
              <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-border px-3 py-2">
                <label className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span>{t("Por pagina")}</span>
                  <select
                    className="h-8 rounded-md border border-input bg-background px-2 text-xs text-foreground"
                    value={pageSize}
                    onChange={(event) => { setPageSize(Number(event.currentTarget.value)); setPage(1); }}
                  >
                    {[25, 50, 100].map((size) => <option key={size} value={size}>{size}</option>)}
                  </select>
                </label>
                <div className="flex items-center gap-2">
                  <Button variant="secondary" size="icon" disabled={(response?.page ?? 1) <= 1 || loading} onClick={() => setPage((current) => Math.max(1, current - 1))}>
                    <ChevronLeft className="h-4 w-4" />
                  </Button>
                  <span className="min-w-24 text-center text-xs text-muted-foreground">
                    {t("Pagina {page} de {total}", { page: response?.page ?? 1, total: response?.total_pages ?? 1 })}
                  </span>
                  <Button variant="secondary" size="icon" disabled={(response?.page ?? 1) >= (response?.total_pages ?? 1) || loading} onClick={() => setPage((current) => current + 1)}>
                    <ChevronRight className="h-4 w-4" />
                  </Button>
                </div>
              </footer>
            </Card>
          </section>
        </div>
      ) : null}

      <TrackDetailSheet
        track={detailTrack}
        onClose={() => setDetailTrack(null)}
        onOpenFolder={openFolder}
        onPlay={trackPlayer.toggleTrackPlayback}
      />
      <PlaylistAddDialog
        open={playlistDialogOpen}
        busy={playlistBusy}
        drafts={drafts}
        trackCount={selectedTrackList.length}
        contextLabel={t("Seleccion del catalogo")}
        defaultName={t("Seleccion del catalogo")}
        onClose={() => setPlaylistDialogOpen(false)}
        onAddExisting={(draftId) => void addTracksToDraft(draftId)}
        onCreate={(name, description) => void createPlaylist(name, description)}
      />
      <SaveSearchDialog
        open={saveSearchDialogOpen}
        busy={savedSearchBusy}
        defaultName={(query.trim() || t("Nueva smart collection")).slice(0, 100)}
        resultCount={response?.total ?? 0}
        onClose={() => setSaveSearchDialogOpen(false)}
        onSave={(name, description) => void saveCurrentSearch(name, description)}
      />
      <DeleteSavedSearchDialog
        open={deleteSearchDialogOpen}
        busy={savedSearchBusy}
        name={activeSavedSearch?.name ?? ""}
        onClose={() => setDeleteSearchDialogOpen(false)}
        onConfirm={() => void deleteActiveSavedSearch()}
      />
    </main>
  );
}

function DeleteSavedSearchDialog({
  open,
  busy,
  name,
  onClose,
  onConfirm
}: {
  open: boolean;
  busy: boolean;
  name: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const { t } = useI18n();

  useEffect(() => {
    if (!open) return;
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [busy, onClose, open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[75] flex items-center justify-center p-4" role="dialog" aria-modal="true" aria-labelledby="delete-saved-search-title">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px]" onClick={busy ? undefined : onClose} />
      <section className="relative z-[80] w-full max-w-md rounded-md border border-border bg-background shadow-2xl">
        <header className="flex items-start gap-3 border-b border-border bg-card px-4 py-4">
          <span className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-destructive/10 text-destructive">
            <Trash2 className="h-4 w-4" />
          </span>
          <div>
            <h2 id="delete-saved-search-title" className="text-base font-semibold">{t("Eliminar smart collection")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("Se eliminara '{name}'. Los tracks y playlists no se modificaran.", { name })}
            </p>
          </div>
        </header>
        <div className="flex justify-end gap-2 p-4">
          <Button type="button" variant="secondary" disabled={busy} onClick={onClose}>{t("Cancelar")}</Button>
          <Button type="button" variant="destructive" autoFocus disabled={busy} onClick={onConfirm}>
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
            {t("Eliminar")}
          </Button>
        </div>
      </section>
    </div>
  );
}

function SaveSearchDialog({
  open,
  busy,
  defaultName,
  resultCount,
  onClose,
  onSave
}: {
  open: boolean;
  busy: boolean;
  defaultName: string;
  resultCount: number;
  onClose: () => void;
  onSave: (name: string, description: string) => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState(defaultName);
  const [description, setDescription] = useState("");

  useEffect(() => {
    if (!open) return;
    setName(defaultName);
    setDescription("");
  }, [defaultName, open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[65] flex items-center justify-center p-4" role="dialog" aria-modal="true">
      <div className="absolute inset-0 bg-black/35 backdrop-blur-[1px]" onClick={onClose} />
      <form
        className="relative z-[70] w-full max-w-lg rounded-md border border-border bg-background shadow-2xl"
        onSubmit={(event) => {
          event.preventDefault();
          if (name.trim()) onSave(name.trim(), description.trim());
        }}
      >
        <header className="flex items-start justify-between gap-3 border-b border-border bg-card px-4 py-4">
          <div>
            <h2 className="text-base font-semibold">{t("Guardar como smart collection")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("La definicion se recalculara siempre. Hoy coincide con {count} tracks.", { count: resultCount })}
            </p>
          </div>
          <Button type="button" variant="ghost" size="sm" onClick={onClose}>{t("Cerrar")}</Button>
        </header>
        <div className="grid gap-4 p-4">
          <label className="grid gap-1 text-sm">
            <span className="font-semibold">{t("Nombre")}</span>
            <input
              autoFocus
              maxLength={100}
              className="h-10 rounded-md border border-input bg-background px-3 outline-none focus-visible:ring-2 focus-visible:ring-ring"
              value={name}
              onChange={(event) => setName(event.currentTarget.value)}
            />
          </label>
          <label className="grid gap-1 text-sm">
            <span className="font-semibold">{t("Descripcion opcional")}</span>
            <textarea
              className="min-h-20 rounded-md border border-input bg-background px-3 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring"
              value={description}
              onChange={(event) => setDescription(event.currentTarget.value)}
              placeholder={t("Ej: tracks listos para warm-up con metadata completa")}
            />
          </label>
          <div className="rounded-md border border-border bg-secondary/60 p-3 text-xs text-muted-foreground">
            <strong className="text-foreground">{t("Dinamica")}</strong>
            <span className="ml-2">{t("Guarda query, filtros y orden; no duplica tracks.")}</span>
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>{t("Cancelar")}</Button>
            <Button disabled={busy || !name.trim()}>
              {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <BookmarkPlus className="h-4 w-4" />}
              {t("Guardar")}
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
}

function FacetSection({
  title,
  items,
  selected,
  onToggle
}: {
  title: string;
  items: CatalogFacetValue[];
  selected: string[];
  onToggle: (value: string) => void;
}) {
  const visibleItems = items.filter((item) => item.count > 0 || selected.includes(item.value));
  return (
    <details className="group px-3 py-3" open={selected.length > 0 || ["Genero", "Genre", "Key"].includes(title)}>
      <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-semibold">
        <span>{title}</span>
        <span className="text-muted-foreground transition group-open:rotate-90">›</span>
      </summary>
      <div className="mt-2 grid max-h-48 gap-0.5 overflow-y-auto pr-1">
        {visibleItems.length === 0 ? <span className="py-2 text-xs text-muted-foreground">—</span> : null}
        {visibleItems.map((item) => (
          <label key={item.value} className="flex min-w-0 cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-accent">
            <input type="checkbox" checked={selected.includes(item.value)} onChange={() => onToggle(item.value)} />
            <span className="min-w-0 flex-1 truncate" title={item.name}>{item.name}</span>
            <span className="tabular-nums text-muted-foreground">{item.count}</span>
          </label>
        ))}
      </div>
    </details>
  );
}

function BpmFacet({
  filters,
  onChange
}: {
  filters: CatalogFilters;
  onChange: (field: "bpmMin" | "bpmMax", value?: number) => void;
}) {
  return (
    <details className="group px-3 py-3" open>
      <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-semibold">
        <span>BPM</span><span className="text-muted-foreground transition group-open:rotate-90">›</span>
      </summary>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <label className="grid gap-1 text-[11px] text-muted-foreground">
          Min
          <input
            type="number"
            min={0}
            step={1}
            className="h-8 min-w-0 rounded-md border border-input bg-background px-2 text-xs text-foreground"
            value={filters.bpmMin ?? ""}
            onChange={(event) => onChange("bpmMin", event.currentTarget.value === "" ? undefined : Number(event.currentTarget.value))}
          />
        </label>
        <label className="grid gap-1 text-[11px] text-muted-foreground">
          Max
          <input
            type="number"
            min={0}
            step={1}
            className="h-8 min-w-0 rounded-md border border-input bg-background px-2 text-xs text-foreground"
            value={filters.bpmMax ?? ""}
            onChange={(event) => onChange("bpmMax", event.currentTarget.value === "" ? undefined : Number(event.currentTarget.value))}
          />
        </label>
      </div>
    </details>
  );
}

function RatingFacet({
  items,
  selected,
  onChange
}: {
  items: CatalogFacetValue[];
  selected?: number;
  onChange: (value?: number) => void;
}) {
  return (
    <details className="group px-3 py-3" open>
      <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-semibold">
        <span>Rating</span><span className="text-muted-foreground transition group-open:rotate-90">›</span>
      </summary>
      <div className="mt-2 grid gap-0.5">
        {items.filter((item) => item.count > 0).map((item) => {
          const rating = Number(item.value);
          return (
            <button
              key={item.value}
              type="button"
              className={cn("flex items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-accent", selected === rating && "bg-primary/10 text-primary")}
              onClick={() => onChange(selected === rating ? undefined : rating)}
            >
              <span className="text-amber-500">{"★".repeat(rating)}</span>
              <span className="ml-auto tabular-nums text-muted-foreground">{item.count}</span>
            </button>
          );
        })}
      </div>
    </details>
  );
}

function ColumnChooser({ columns, onToggle }: { columns: Set<TrackListColumn>; onToggle: (column: TrackListColumn) => void }) {
  return (
    <details className="relative">
      <summary className="inline-flex h-8 cursor-pointer list-none items-center gap-2 rounded-md bg-secondary px-2.5 text-xs font-semibold hover:bg-secondary/80">
        <Columns3 className="h-3.5 w-3.5" /> Columnas
      </summary>
      <div className="absolute right-0 top-10 z-40 grid min-w-48 gap-0.5 rounded-md border border-border bg-card p-2 shadow-xl">
        {columnOptions.map((option) => (
          <label key={option.value} className="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-accent">
            <input type="checkbox" checked={columns.has(option.value)} onChange={() => onToggle(option.value)} />
            {option.label}
          </label>
        ))}
      </div>
    </details>
  );
}

type FilterChip = { key: string; field: keyof CatalogFilters; value?: string; label: string };

function buildFilterChips(filters: CatalogFilters): FilterChip[] {
  const chips: FilterChip[] = [];
  const fields: Array<[keyof CatalogFilters, string, string[]]> = [
    ["genres", "Genero", filters.genres],
    ["artists", "Artista", filters.artists],
    ["albums", "Album", filters.albums],
    ["keys", "Key", filters.keys],
    ["years", "Ano", filters.years],
    ["formats", "Formato", filters.formats],
    ["metadataGaps", "Falta", filters.metadataGaps],
    ["availability", "Archivo", filters.availability]
  ];
  for (const [field, label, values] of fields) {
    for (const value of values) chips.push({ key: `${field}:${value}`, field, value, label: `${label}: ${humanizeFilter(value)}` });
  }
  if (filters.bpmMin !== undefined || filters.bpmMax !== undefined) {
    chips.push({ key: "bpm", field: "bpmMin", label: `BPM: ${filters.bpmMin ?? "…"}–${filters.bpmMax ?? "…"}` });
  }
  if (filters.ratingMin !== undefined) chips.push({ key: "rating", field: "ratingMin", label: `Rating ≥ ${filters.ratingMin}` });
  return chips;
}

function removeFilterChip(
  chip: FilterChip,
  filters: CatalogFilters,
  setFilters: (value: CatalogFilters) => void,
  setPage: (value: number) => void
) {
  if (chip.key === "bpm") {
    setFilters({ ...filters, bpmMin: undefined, bpmMax: undefined });
  } else if (chip.field === "ratingMin") {
    setFilters({ ...filters, ratingMin: undefined });
  } else {
    const field = chip.field as "genres" | "artists" | "albums" | "keys" | "years" | "formats" | "metadataGaps" | "availability";
    setFilters({ ...filters, [field]: filters[field].filter((value) => value !== chip.value) });
  }
  setPage(1);
}

function humanizeFilter(value: string) {
  const labels: Record<string, string> = {
    missing_genre: "sin genero",
    missing_bpm: "sin BPM",
    missing_key: "sin key",
    missing_label: "sin label",
    missing_year: "sin ano",
    missing_artist: "sin artista",
    missing_album: "sin album",
    available: "disponible",
    missing: "faltante"
  };
  return labels[value] ?? value;
}

function normalizeCatalogFilters(filters?: Partial<CatalogFilters> | null): CatalogFilters {
  return {
    genres: filters?.genres ?? [],
    artists: filters?.artists ?? [],
    albums: filters?.albums ?? [],
    keys: filters?.keys ?? [],
    years: filters?.years ?? [],
    formats: filters?.formats ?? [],
    bpmMin: filters?.bpmMin,
    bpmMax: filters?.bpmMax,
    ratingMin: filters?.ratingMin,
    metadataGaps: filters?.metadataGaps ?? [],
    availability: filters?.availability ?? []
  };
}

function catalogDefinitionKey(query: string, filters: CatalogFilters, sort: string) {
  const normalized = normalizeCatalogFilters(filters);
  return JSON.stringify({
    query: query.trim(),
    sort,
    filters: {
      ...normalized,
      genres: [...normalized.genres].sort(),
      artists: [...normalized.artists].sort(),
      albums: [...normalized.albums].sort(),
      keys: [...normalized.keys].sort(),
      years: [...normalized.years].sort(),
      formats: [...normalized.formats].sort(),
      metadataGaps: [...normalized.metadataGaps].sort(),
      availability: [...normalized.availability].sort()
    }
  });
}

function uniqueTrackIds(tracks: TrackListItem[]) {
  return Array.from(new Set(tracks.map((track) => track.track_id)));
}
