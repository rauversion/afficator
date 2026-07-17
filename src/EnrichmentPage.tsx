import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  RefreshCcw,
  Search,
  Sparkles,
  Tags,
  Trash2
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type * as React from "react";
import { Link } from "react-router-dom";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { BroadcastAddButton } from "./components/tracks/BroadcastAddButton";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";
import type { EnrichmentProviderDescriptor } from "./enrichmentProviders";

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

type PlaylistIndexTrack = {
  library_id: string;
  track_id: string;
  name?: string | null;
  artist?: string | null;
  album?: string | null;
  kind?: string | null;
  location?: string | null;
  source_path?: string | null;
  source_exists: boolean;
  genre?: string | null;
  comments?: string | null;
  bpm?: string | null;
  key?: string | null;
  year?: string | null;
  label?: string | null;
};

type EnrichmentOverview = {
  library: PlaylistIndexLibrary;
  track_count: number;
  missing_genre_count: number;
  missing_year_count: number;
  missing_label_count: number;
  missing_comments_count: number;
  missing_key_count: number;
  missing_bpm_count: number;
  enriched_track_count: number;
  matched_result_count: number;
  failed_result_count: number;
  last_run_at?: string | null;
};

type EnrichmentItem = {
  id: string;
  library_id: string;
  track_id: string;
  provider: string;
  provider_key?: string | null;
  status: "matched" | "no_match" | "failed" | string;
  confidence: number;
  fields: Record<string, string>;
  message?: string | null;
  source_url?: string | null;
  updated_at: string;
  applied_at?: string | null;
  track: PlaylistIndexTrack;
};

type EnrichmentRunResult = {
  run_id: string;
  library_id: string;
  processed_total: number;
  matched_total: number;
  no_match_total: number;
  failed_total: number;
  providers: string[];
};

type EnrichmentApplyResult = {
  library_id: string;
  applied_total: number;
  skipped_total: number;
};

type EnrichmentProgressEvent = {
  type: "track_enrichment_progress";
  level: "info" | "warning" | "error" | string;
  message: string;
  progress?: number | null;
  library_id: string;
  track_id?: string | null;
  provider?: string | null;
  status?: string | null;
  processed: number;
  total: number;
  timestamp: string;
};

type GapFilter =
  | "missing_metadata"
  | "missing_genre"
  | "missing_year"
  | "missing_label"
  | "missing_comments"
  | "missing_key"
  | "missing_bpm"
  | "all";

const gapOptions: Array<{ value: GapFilter; label: string }> = [
  { value: "missing_metadata", label: "Metadata incompleta" },
  { value: "missing_genre", label: "Sin genero" },
  { value: "missing_year", label: "Sin ano" },
  { value: "missing_label", label: "Sin label" },
  { value: "missing_comments", label: "Sin comentarios" },
  { value: "missing_key", label: "Sin key" },
  { value: "missing_bpm", label: "Sin BPM" },
  { value: "all", label: "Todos" }
];

export function EnrichmentPage() {
  const { locale, t } = useI18n();
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [overview, setOverview] = useState<EnrichmentOverview | null>(null);
  const [candidates, setCandidates] = useState<PlaylistIndexTrack[]>([]);
  const [results, setResults] = useState<EnrichmentItem[]>([]);
  const [selectedTrackIds, setSelectedTrackIds] = useState<Set<string>>(new Set());
  const [gap, setGap] = useState<GapFilter>("missing_metadata");
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(100);
  const [providers, setProviders] = useState<EnrichmentProviderDescriptor[]>([]);
  const [selectedProviderIds, setSelectedProviderIds] = useState<Set<string>>(
    new Set(["musicbrainz"])
  );
  const [lastResult, setLastResult] = useState<EnrichmentRunResult | null>(null);
  const [progress, setProgress] = useState<EnrichmentProgressEvent | null>(null);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const [loading, setLoading] = useState(false);
  const [running, setRunning] = useState(false);
  const [applying, setApplying] = useState(false);
  const [terminalLogs, setTerminalLogs] = useState<TerminalLogEntry[]>([]);
  const [terminalExpanded, setTerminalExpanded] = useState(false);

  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const matchedResults = useMemo(() => results.filter((result) => result.status === "matched"), [results]);
  const unappliedMatchedResults = useMemo(
    () => matchedResults.filter((result) => !result.applied_at),
    [matchedResults]
  );
  const allCandidatesSelected = candidates.length > 0 && selectedTrackIds.size === candidates.length;

  useEffect(() => {
    void loadLibraries();
    void loadProviders();
    const unlisteners: UnlistenFn[] = [];
    listen<EnrichmentProgressEvent>("track-enrichment-progress", (event) => {
      setProgress(event.payload);
      appendTerminalLog({
        level: event.payload.level === "error" ? "error" : "info",
        track_id: event.payload.track_id ?? undefined,
        name: event.payload.provider ?? undefined,
        message: event.payload.message
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  async function loadProviders() {
    try {
      const response = await invoke<EnrichmentProviderDescriptor[]>("enrichment_providers");
      setProviders(response);
      setSelectedProviderIds((current) => {
        const available = new Set(response.filter((provider) => provider.ready).map((provider) => provider.id));
        const selected = new Set(Array.from(current).filter((providerId) => available.has(providerId)));
        if (selected.size === 0 && available.has("musicbrainz")) selected.add("musicbrainz");
        return selected;
      });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

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
        await loadEnrichment(nextLibraryId);
      } else {
        setOverview(null);
        setCandidates([]);
        setResults([]);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function loadEnrichment(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const [overviewResponse, candidateResponse, resultResponse] = await Promise.all([
      invoke<EnrichmentOverview>("playlist_enrichment_overview", { libraryId }),
      invoke<PlaylistIndexTrack[]>("playlist_enrichment_candidates", {
        libraryId,
        gap,
        query,
        limit
      }),
      invoke<EnrichmentItem[]>("playlist_enrichment_results", {
        libraryId,
        limit: 200
      })
    ]);
    setOverview(overviewResponse);
    setCandidates(candidateResponse);
    setResults(resultResponse);
    setSelectedTrackIds((current) => {
      const available = new Set(candidateResponse.map((track) => track.track_id));
      return new Set(Array.from(current).filter((trackId) => available.has(trackId)));
    });
  }

  async function changeLibrary(libraryId: string) {
    setActiveLibraryId(libraryId);
    setSelectedTrackIds(new Set());
    setMessage("");
    setErrorMessage("");
    setLoading(true);
    try {
      await loadEnrichment(libraryId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function refresh() {
    setLoading(true);
    setErrorMessage("");
    try {
      await loadEnrichment();
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function runEnrichment() {
    if (!activeLibraryId) return;
    const providers = selectedProviders();
    if (providers.length === 0) {
      setErrorMessage(t("Selecciona al menos un proveedor."));
      return;
    }

    setRunning(true);
    setMessage("");
    setErrorMessage("");
    setLastResult(null);

    try {
      const response = await invoke<EnrichmentRunResult>("playlist_enrichment_run", {
        libraryId: activeLibraryId,
        providers,
        limit,
        trackIds: selectedTrackIds.size > 0 ? Array.from(selectedTrackIds) : null
      });
      setLastResult(response);
      setMessage(
        t("Enrichment listo: {matched} matches, {failed} errores.", {
          matched: response.matched_total,
          failed: response.failed_total
        })
      );
      await loadEnrichment(activeLibraryId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setRunning(false);
    }
  }

  async function applyResults(resultIds = unappliedMatchedResults.map((result) => result.id)) {
    if (!activeLibraryId || resultIds.length === 0) return;
    setApplying(true);
    setMessage("");
    setErrorMessage("");

    try {
      const response = await invoke<EnrichmentApplyResult>("playlist_enrichment_apply", {
        libraryId: activeLibraryId,
        resultIds
      });
      setMessage(
        t("Sugerencias aplicadas: {applied}. Saltadas: {skipped}.", {
          applied: response.applied_total,
          skipped: response.skipped_total
        })
      );
      await loadEnrichment(activeLibraryId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setApplying(false);
    }
  }

  async function clearResults() {
    if (!activeLibraryId) return;
    setLoading(true);
    setMessage("");
    setErrorMessage("");
    try {
      const deleted = await invoke<number>("playlist_enrichment_clear", {
        libraryId: activeLibraryId,
        trackIds: null
      });
      setMessage(t("Resultados eliminados: {count}.", { count: deleted }));
      await loadEnrichment(activeLibraryId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  function selectedProviders() {
    return providers
      .filter((provider) => provider.ready && selectedProviderIds.has(provider.id))
      .map((provider) => provider.id);
  }

  function toggleProvider(providerId: string) {
    setSelectedProviderIds((current) => {
      const next = new Set(current);
      if (next.has(providerId)) next.delete(providerId);
      else next.add(providerId);
      return next;
    });
  }

  function appendTerminalLog(entry: Omit<TerminalLogEntry, "id" | "time">) {
    setTerminalLogs((current) => [
      ...current.slice(-250),
      {
        ...entry,
        id: nextTerminalLogId.current++,
        time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })
      }
    ]);
    window.setTimeout(() => {
      terminalElement.current?.scrollTo({ top: terminalElement.current.scrollHeight });
    }, 0);
  }

  function toggleTrack(trackId: string) {
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

  function toggleAllCandidates() {
    setSelectedTrackIds(() => {
      if (allCandidatesSelected) return new Set();
      return new Set(candidates.map((track) => track.track_id));
    });
  }

  return (
    <main className="min-w-0 p-4">
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
            <Tags className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-semibold tracking-normal">{t("Enrichment")}</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {activeLibrary?.source_name ?? t("Sin librerias indexadas")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" disabled={loading || running} onClick={() => void refresh()}>
            <RefreshCcw className="h-4 w-4" />
            {t("Refrescar")}
          </Button>
          <Button disabled={!activeLibraryId || running || selectedProviders().length === 0} onClick={() => void runEnrichment()}>
            <Sparkles className="h-4 w-4" />
            {running ? t("Enriqueciendo") : t("Ejecutar")}
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

      <section className="grid gap-3">
        <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <Database className="h-4 w-4" />
                <CardTitle>{t("Libreria")}</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="grid gap-3">
              <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_160px_140px]">
                <select
                  className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                  value={activeLibraryId}
                  disabled={loading || running || libraries.length === 0}
                  onChange={(event) => void changeLibrary(event.currentTarget.value)}
                >
                  {libraries.length === 0 ? <option value="">{t("Sin librerias indexadas")}</option> : null}
                  {libraries.map((library) => (
                    <option key={library.id} value={library.id}>
                      {library.source_name} · {library.track_count} tracks
                    </option>
                  ))}
                </select>
                <select
                  className="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                  value={gap}
                  disabled={loading || running}
                  onChange={(event) => setGap(event.currentTarget.value as GapFilter)}
                >
                  {gapOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {t(option.label)}
                    </option>
                  ))}
                </select>
                <input
                  className="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                  type="number"
                  min={1}
                  max={1000}
                  value={limit}
                  disabled={loading || running}
                  onChange={(event) => setLimit(clampLimit(event.currentTarget.value))}
                />
              </div>

              <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_auto]">
                <label className="relative block">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <input
                    className="h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 text-sm outline-none ring-offset-background transition-shadow focus-visible:ring-2 focus-visible:ring-ring"
                    value={query}
                    disabled={loading || running}
                    placeholder={t("Buscar")}
                    onChange={(event) => setQuery(event.currentTarget.value)}
                  />
                </label>
                <Button variant="secondary" disabled={loading || running || !activeLibraryId} onClick={() => void refresh()}>
                  <Search className="h-4 w-4" />
                  {t("Buscar")}
                </Button>
              </div>

              <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                <Metric label={t("Tracks")} value={overview?.track_count ?? 0} />
                <Metric label={t("Sin genero")} value={overview?.missing_genre_count ?? 0} danger={(overview?.missing_genre_count ?? 0) > 0} />
                <Metric label={t("Sin ano")} value={overview?.missing_year_count ?? 0} danger={(overview?.missing_year_count ?? 0) > 0} />
                <Metric label={t("Matches")} value={overview?.matched_result_count ?? 0} />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <div className="flex items-center gap-2">
                <Sparkles className="h-4 w-4" />
                <CardTitle>{t("Proveedores")}</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="grid gap-3">
              {providers.map((provider) => (
                <label
                  key={provider.id}
                  className={cn(
                    "grid min-h-14 grid-cols-[auto_minmax(0,1fr)] items-start gap-2 rounded-md border border-border px-3 py-2 text-sm",
                    !provider.ready && "opacity-60"
                  )}
                >
                  <input
                    className="mt-1"
                    type="checkbox"
                    checked={selectedProviderIds.has(provider.id)}
                    disabled={running || !provider.ready}
                    onChange={() => toggleProvider(provider.id)}
                  />
                  <span className="min-w-0">
                    <span className="flex items-center justify-between gap-2">
                      <strong>{provider.label}</strong>
                      <span className={cn("text-[10px] font-semibold uppercase", provider.ready ? "text-emerald-600" : "text-amber-600")}>
                        {provider.ready ? t("Lista") : t("Requiere credencial")}
                      </span>
                    </span>
                    <span className="mt-0.5 block text-xs text-muted-foreground">{provider.description}</span>
                    <span className="mt-1 block truncate font-mono text-[10px] text-muted-foreground">
                      {provider.capabilities.join(" · ")}
                    </span>
                  </span>
                </label>
              ))}
              {providers.some((provider) => !provider.ready) ? (
                <Link className="text-xs font-medium text-primary hover:underline" to="/settings">
                  {t("Configurar credenciales en Settings")}
                </Link>
              ) : null}
              <div className="h-2 rounded-full bg-secondary">
                <div
                  className="h-2 rounded-full bg-primary transition-all"
                  style={{ width: `${Math.max(0, Math.min(100, progress?.progress ?? 0))}%` }}
                />
              </div>
              <div className="grid gap-1 text-xs text-muted-foreground">
                <span>{progress ? `${progress.processed}/${progress.total} · ${progress.message}` : t("Sin eventos todavia.")}</span>
                {lastResult ? (
                  <span>
                    {lastResult.matched_total} {t("matches")} · {lastResult.no_match_total} {t("sin match")} · {lastResult.failed_total} {t("errores")}
                  </span>
                ) : null}
              </div>
            </CardContent>
          </Card>
        </div>

        <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(420px,0.8fr)]">
          <Card className="overflow-hidden">
            <CardHeader>
              <div className="min-w-0">
                <CardTitle>{t("Candidatos")}</CardTitle>
                <span className="block text-xs text-muted-foreground">
                  {selectedTrackIds.size} / {candidates.length}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Button variant="secondary" size="sm" disabled={candidates.length === 0 || running} onClick={toggleAllCandidates}>
                  {allCandidatesSelected ? t("Deseleccionar") : t("Todos")}
                </Button>
              </div>
            </CardHeader>
            <CardContent className="p-0">
              <div className="max-h-[520px] overflow-auto">
                <div className="grid min-w-[840px] grid-cols-[36px_minmax(220px,1.4fr)_minmax(160px,1fr)_160px_220px_44px] border-b border-border bg-secondary px-3 py-2 text-xs font-semibold text-muted-foreground">
                  <span />
                  <span>{t("Track")}</span>
                  <span>{t("Album")}</span>
                  <span>{t("Metadata")}</span>
                  <span>{t("Gaps")}</span>
                  <span className="text-right">{t("Acciones")}</span>
                </div>
                {candidates.length === 0 ? <EmptyRow>{t("Sin candidatos.")}</EmptyRow> : null}
                {candidates.map((track) => (
                  <div key={track.track_id} className="grid min-h-12 min-w-[840px] grid-cols-[36px_minmax(220px,1.4fr)_minmax(160px,1fr)_160px_220px_44px] items-center gap-2 border-b border-border px-3 py-2 text-xs">
                    <input
                      type="checkbox"
                      checked={selectedTrackIds.has(track.track_id)}
                      disabled={running}
                      onChange={() => toggleTrack(track.track_id)}
                    />
                    <div className="min-w-0">
                      <strong className="block truncate">{track.name ?? track.track_id}</strong>
                      <span className="block truncate text-muted-foreground">{track.artist ?? ""}</span>
                    </div>
                    <span className="truncate text-muted-foreground">{track.album ?? ""}</span>
                    <span className="truncate text-muted-foreground">{trackMetadata(track)}</span>
                    <span className="truncate text-amber-700 dark:text-amber-300">{trackGaps(track).join(" · ")}</span>
                    <BroadcastAddButton track={track} />
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          <Card className="overflow-hidden">
            <CardHeader>
              <div className="min-w-0">
                <CardTitle>{t("Resultados")}</CardTitle>
                <span className="block text-xs text-muted-foreground">{unappliedMatchedResults.length} {t("pendientes")}</span>
              </div>
              <div className="flex items-center gap-2">
                <Button size="sm" disabled={applying || unappliedMatchedResults.length === 0} onClick={() => void applyResults()}>
                  <CheckCircle2 className="h-4 w-4" />
                  {t("Aplicar")}
                </Button>
                <Button variant="secondary" size="sm" disabled={loading || running || results.length === 0} onClick={() => void clearResults()}>
                  <Trash2 className="h-4 w-4" />
                  {t("Limpiar")}
                </Button>
              </div>
            </CardHeader>
            <CardContent className="p-0">
              <div className="max-h-[520px] overflow-auto">
                {results.length === 0 ? <EmptyRow>{t("Sin resultados todavia.")}</EmptyRow> : null}
                {results.map((result) => (
                  <div key={result.id} className="grid gap-2 border-b border-border px-3 py-2 text-xs">
                    <div className="flex min-w-0 items-center justify-between gap-2">
                      <div className="min-w-0">
                        <strong className="block truncate">{result.track.name ?? result.track_id}</strong>
                        <span className="block truncate text-muted-foreground">{result.track.artist ?? ""}</span>
                      </div>
                      <StatusPill status={result.status} applied={Boolean(result.applied_at)} />
                    </div>
                    <div className="flex flex-wrap items-center gap-2 text-muted-foreground">
                      <span className="font-semibold text-foreground">{providerLabel(result.provider)}</span>
                      <span>{formatPercent(result.confidence)}</span>
                      <span>{formatDate(result.updated_at)}</span>
                      {result.source_url ? (
                        <a className="text-primary underline-offset-2 hover:underline" href={result.source_url} target="_blank" rel="noreferrer">
                          {t("Fuente")}
                        </a>
                      ) : null}
                    </div>
                    <p className="line-clamp-2 text-muted-foreground">{result.message ?? fieldSummary(result.fields)}</p>
                    {Object.keys(result.fields).length > 0 ? (
                      <div className="grid gap-1 rounded-md border border-border bg-muted/40 p-2 font-mono text-[11px]">
                        {Object.entries(result.fields).slice(0, 6).map(([key, value]) => (
                          <span key={key} className="truncate" title={`${key}: ${value}`}>
                            <strong>{key}</strong>: {value}
                          </span>
                        ))}
                      </div>
                    ) : null}
                    {result.status === "matched" && !result.applied_at ? (
                      <div>
                        <Button size="sm" variant="secondary" disabled={applying} onClick={() => void applyResults([result.id])}>
                          {t("Aplicar")}
                        </Button>
                      </div>
                    ) : null}
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </div>
      </section>

      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle="track enrichment"
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={() => setTerminalLogs([])}
      />
    </main>
  );
}

function Metric({ label, value, danger = false }: { label: string; value: number; danger?: boolean }) {
  return (
    <div className={cn("rounded-md border border-border p-3", danger && "border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-900/70 dark:bg-amber-950/25 dark:text-amber-100")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block text-xl">{value}</strong>
    </div>
  );
}

function EmptyRow({ children }: { children: React.ReactNode }) {
  return <div className="flex min-h-11 items-center px-3 text-sm text-muted-foreground">{children}</div>;
}

function StatusPill({ status, applied }: { status: string; applied: boolean }) {
  const tone = applied ? "ok" : status === "matched" ? "ok" : status === "failed" ? "error" : "pending";
  return (
    <span
      className={cn(
        "inline-flex h-7 shrink-0 items-center gap-1 rounded-md border px-2 text-[11px] font-semibold",
        tone === "ok" && "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200",
        tone === "pending" && "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200",
        tone === "error" && "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200"
      )}
    >
      {tone === "error" ? <AlertTriangle className="h-3.5 w-3.5" /> : <CheckCircle2 className="h-3.5 w-3.5" />}
      {applied ? "applied" : status}
    </span>
  );
}

function trackMetadata(track: PlaylistIndexTrack) {
  return [track.genre, track.bpm ? `${track.bpm} BPM` : null, track.key, track.year, track.label]
    .filter(Boolean)
    .join(" · ");
}

function trackGaps(track: PlaylistIndexTrack) {
  const gaps: string[] = [];
  if (!track.genre?.trim()) gaps.push("genre");
  if (!track.year?.trim()) gaps.push("year");
  if (!track.label?.trim()) gaps.push("label");
  if (!track.comments?.trim()) gaps.push("comments");
  if (!track.key?.trim()) gaps.push("key");
  if (!track.bpm?.trim()) gaps.push("bpm");
  return gaps.length > 0 ? gaps : ["ok"];
}

function providerLabel(provider: string) {
  if (provider === "musicbrainz") return "MusicBrainz";
  if (provider === "lastfm") return "Last.fm";
  return provider;
}

function fieldSummary(fields: Record<string, string>) {
  return Object.entries(fields)
    .slice(0, 5)
    .map(([key, value]) => `${key}: ${value}`)
    .join(" · ");
}

function formatPercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], { month: "short", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function clampLimit(value: string) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) return 100;
  return Math.max(1, Math.min(1000, parsed));
}
