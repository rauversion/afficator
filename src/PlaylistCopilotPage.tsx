import { invoke } from "@tauri-apps/api/core";
import {
  Bot,
  CheckSquare,
  ListPlus,
  RefreshCcw,
  Send,
  Sparkles
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
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

type PlaylistIndexLibrary = {
  id: string;
  source_path: string;
  source_name: string;
  track_count: number;
  playlist_count: number;
  embedded_track_count: number;
  missing_file_count: number;
};

type PlaylistCopilotTrack = TrackListItem & {
  search_text?: string;
  embedding_ready?: boolean;
};

type PlaylistCopilotCandidate = {
  track: PlaylistCopilotTrack;
  score: number;
  reasons: string[];
};

type PlaylistCopilotInterpretation = {
  genres: string[];
  artists: string[];
  keys: string[];
  bpm_min?: number | null;
  bpm_max?: number | null;
  mood?: string | null;
  energy?: string | null;
  exclude_terms: string[];
  target_count?: number | null;
};

type PlaylistCopilotResponse = {
  message: string;
  interpreted: PlaylistCopilotInterpretation;
  questions: string[];
  candidates: PlaylistCopilotCandidate[];
  used_openai: boolean;
};

type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
};

const copilotExamples = [
  "Warm up house 118-124 BPM con voces suaves",
  "Peak time techno melodico en Am o Em",
  "Funk, disco y hip hop old school para abrir",
  "Deep house nocturno, elegante, sin tracks muy agresivos"
];

export function PlaylistCopilotPage() {
  const { locale, t } = useI18n();
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [drafts, setDrafts] = useState<PlaylistDraftOption[]>([]);
  const [prompt, setPrompt] = useState("");
  const [targetCount, setTargetCount] = useState(30);
  const [messages, setMessages] = useState<ChatMessage[]>(() => [
    {
      id: "welcome",
      role: "assistant",
      text: "Describe la energia, generos, artistas, BPM, keys o contexto del set. Te sugiero tracks desde tu XML indexado."
    }
  ]);
  const [response, setResponse] = useState<PlaylistCopilotResponse | null>(null);
  const [selectedTrackIds, setSelectedTrackIds] = useState<Set<string>>(new Set());
  const [detailTrack, setDetailTrack] = useState<PlaylistCopilotTrack | null>(null);
  const [addPlaylistDialogOpen, setAddPlaylistDialogOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [playlistBusy, setPlaylistBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const messagesEndRef = useRef<HTMLDivElement | null>(null);
  const trackPlayer = useTrackPlayer({ t, onError: setErrorMessage });

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const candidates = response?.candidates ?? [];
  const candidateTracks = useMemo(() => candidates.map((candidate) => candidate.track), [candidates]);
  const selectedTracks = useMemo(
    () => candidateTracks.filter((track) => selectedTrackIds.has(track.track_id)),
    [candidateTracks, selectedTrackIds]
  );
  const selectedCount = selectedTracks.length;
  const allSelected = candidateTracks.length > 0 && selectedCount === candidateTracks.length;
  const someSelected = selectedCount > 0 && !allSelected;

  useEffect(() => {
    void loadLibraries();
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ block: "end" });
  }, [messages]);

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
        await loadDrafts(nextLibraryId);
      } else {
        setDrafts([]);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setLoading(false);
    }
  }

  async function changeLibrary(libraryId: string) {
    setActiveLibraryId(libraryId);
    setResponse(null);
    setSelectedTrackIds(new Set());
    setDetailTrack(null);
    setErrorMessage("");
    setMessage("");
    try {
      await loadDrafts(libraryId);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function loadDrafts(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const response = await invoke<PlaylistDraftOption[]>("playlist_index_drafts", { libraryId });
    setDrafts(response);
  }

  async function generateSuggestions(event?: React.FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    const currentPrompt = prompt.trim();
    if (!activeLibraryId) {
      setErrorMessage(t("Primero indexa una libreria XML."));
      return;
    }
    if (!currentPrompt) {
      setErrorMessage(t("Describe la playlist que quieres generar."));
      return;
    }

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      text: currentPrompt
    };
    setMessages((current) => [...current, userMessage]);
    setLoading(true);
    setErrorMessage("");
    setMessage("");

    try {
      const result = await invoke<PlaylistCopilotResponse>("playlist_copilot_generate", {
        request: {
          libraryId: activeLibraryId,
          prompt: currentPrompt,
          targetCount
        }
      });
      setResponse(result);
      setSelectedTrackIds(new Set(result.candidates.map((candidate) => candidate.track.track_id)));
      setMessages((current) => [
        ...current,
        {
          id: crypto.randomUUID(),
          role: "assistant",
          text: result.message
        }
      ]);
    } catch (error) {
      const translated = translateBackendMessage(locale, String(error));
      setErrorMessage(translated);
      setMessages((current) => [
        ...current,
        {
          id: crypto.randomUUID(),
          role: "system",
          text: translated
        }
      ]);
    } finally {
      setLoading(false);
    }
  }

  function appendQuestion(question: string) {
    setPrompt((current) => `${current.trim()} ${question}`.trim());
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
      if (candidateTracks.length > 0 && current.size === candidateTracks.length) {
        return new Set();
      }
      return new Set(candidateTracks.map((track) => track.track_id));
    });
  }

  async function openTrackFolder(track: TrackListItem) {
    if (!track.source_path) return;
    try {
      await invoke("open_parent_folder", { path: track.source_path });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  async function addTracksToDraft(draftId: string) {
    const trackIds = uniqueTrackIds(selectedTracks);
    if (!draftId || trackIds.length === 0) return;
    setPlaylistBusy(true);
    setErrorMessage("");
    setMessage("");

    try {
      const updatedTracks = await invoke<PlaylistCopilotTrack[]>("playlist_index_add_tracks_to_draft", {
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

  async function createPlaylistFromTracks(name: string, description: string) {
    const trackIds = uniqueTrackIds(selectedTracks);
    if (!activeLibraryId || trackIds.length === 0 || !name.trim()) return;
    setPlaylistBusy(true);
    setErrorMessage("");
    setMessage("");

    try {
      const draft = await invoke<PlaylistDraftOption>("playlist_index_create_draft", {
        libraryId: activeLibraryId,
        name,
        description: description || null
      });
      const updatedTracks = await invoke<PlaylistCopilotTrack[]>("playlist_index_add_tracks_to_draft", {
        draftId: draft.id,
        trackIds
      });
      await loadDrafts(activeLibraryId);
      setAddPlaylistDialogOpen(false);
      setMessage(t("Playlist creada: {name} con {count} tracks.", {
        name: draft.name,
        count: updatedTracks.length
      }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setPlaylistBusy(false);
    }
  }

  return (
    <main className="min-w-0 p-4 pb-20">
      {trackPlayer.audio}
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="min-w-0">
          <h1 className="m-0 flex items-center gap-2 text-2xl font-semibold tracking-normal">
            <Sparkles className="h-6 w-6" />
            {t("Playlist Copilot")}
          </h1>
          <p className="mt-1 max-w-[72vw] truncate text-xs text-muted-foreground lg:max-w-[58vw]">
            {activeLibrary?.source_path ?? t("Genera playlists sugeridas desde tu XML indexado.")}
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
          <Button variant="secondary" disabled={loading} onClick={() => void loadLibraries(activeLibraryId)}>
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
            {t("Primero indexa una libreria en Playlist Library para usar Playlist Copilot.")}
          </p>
        </Card>
      ) : null}

      {activeLibraryId ? (
        <section className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
          <Card className="min-w-0">
            <CardHeader>
              <div className="min-w-0">
                <CardTitle className="flex items-center gap-2">
                  <Bot className="h-4 w-4" />
                  {t("Brief interactivo")}
                </CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {response?.used_openai
                    ? t("Interpretacion AI activa con OpenAI.")
                    : t("Funciona con ranking local si no hay API key o embeddings.")}
                </p>
              </div>
            </CardHeader>
            <CardContent className="grid gap-3 p-3">
              <div className="h-[280px] overflow-y-auto rounded-md border border-border bg-background p-3">
                <div className="grid gap-2">
                  {messages.map((item) => (
                    <div
                      key={item.id}
                      className={cn(
                        "max-w-[86%] rounded-md px-3 py-2 text-sm",
                        item.role === "user" && "ml-auto bg-primary text-primary-foreground",
                        item.role === "assistant" && "mr-auto bg-secondary text-secondary-foreground",
                        item.role === "system" && "mx-auto border border-red-300 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200"
                      )}
                    >
                      {item.text}
                    </div>
                  ))}
                  <div ref={messagesEndRef} />
                </div>
              </div>

              <div className="flex flex-wrap gap-2">
                {copilotExamples.map((example) => (
                  <Button key={example} type="button" variant="secondary" size="sm" onClick={() => setPrompt(example)}>
                    {example}
                  </Button>
                ))}
              </div>

              <form className="grid gap-2" onSubmit={generateSuggestions}>
                <textarea
                  className="min-h-28 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  value={prompt}
                  placeholder={t("Ej: 30 tracks deep house, 120-124 BPM, vocales calidas, sin peak time.")}
                  onChange={(event) => setPrompt(event.currentTarget.value)}
                />
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <label className="flex items-center gap-2 text-sm">
                    <span className="font-semibold">{t("Cantidad")}</span>
                    <select
                      className="h-9 rounded-md border border-input bg-background px-2 text-sm"
                      value={targetCount}
                      onChange={(event) => setTargetCount(Number(event.currentTarget.value))}
                    >
                      {[10, 20, 30, 40, 60, 90, 120].map((value) => (
                        <option key={value} value={value}>
                          {value}
                        </option>
                      ))}
                    </select>
                  </label>
                  <Button disabled={loading || !prompt.trim() || !activeLibraryId}>
                    <Send className="h-4 w-4" />
                    {loading ? t("Generando") : t("Generar sugerencias")}
                  </Button>
                </div>
              </form>
            </CardContent>
          </Card>

          <Card className="min-w-0">
            <CardHeader>
              <CardTitle>{t("Interpretacion")}</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-3 p-3">
              <InterpretationPanel interpreted={response?.interpreted ?? null} />
              {response?.questions.length ? (
                <section className="grid gap-2">
                  <h3 className="text-sm font-semibold">{t("Preguntas sugeridas")}</h3>
                  {response.questions.map((question) => (
                    <Button
                      key={question}
                      type="button"
                      variant="secondary"
                      className="justify-start whitespace-normal text-left"
                      onClick={() => appendQuestion(question)}
                    >
                      {t(question)}
                    </Button>
                  ))}
                </section>
              ) : (
                <div className="rounded-md border border-border bg-secondary/60 p-3 text-sm text-muted-foreground">
                  {t("Aun no hay interpretacion. Escribe un brief y genera sugerencias.")}
                </div>
              )}
              <section className="rounded-md border border-border bg-secondary/60 p-3 text-xs text-muted-foreground">
                <strong className="block text-foreground">{activeLibrary?.source_name}</strong>
                {activeLibrary
                  ? t("{tracks} tracks · {playlists} playlists · {embeddings} vectores", {
                      tracks: activeLibrary.track_count,
                      playlists: activeLibrary.playlist_count,
                      embeddings: activeLibrary.embedded_track_count
                    })
                  : null}
              </section>
            </CardContent>
          </Card>

          <Card className="min-w-0 xl:col-span-2">
            <CardHeader>
              <div className="min-w-0">
                <CardTitle>{t("Candidatos")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {candidates.length
                    ? t("{selected}/{total} tracks seleccionados", { selected: selectedCount, total: candidates.length })
                    : t("Sin candidatos todavia.")}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                {candidates.length ? (
                  <Button type="button" variant="secondary" size="sm" onClick={toggleAllSelection}>
                    <CheckSquare className="h-4 w-4" />
                    {allSelected ? t("Deseleccionar") : t("Seleccionar")}
                  </Button>
                ) : null}
                <Button
                  type="button"
                  size="sm"
                  disabled={selectedCount === 0}
                  onClick={() => setAddPlaylistDialogOpen(true)}
                >
                  <ListPlus className="h-4 w-4" />
                  {t("Agregar a playlist")}
                </Button>
              </div>
            </CardHeader>
            <CardContent className="p-3">
              {candidates.length ? (
                <div className="mb-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-secondary px-3 py-2 text-xs text-muted-foreground">
                  <label className="flex items-center gap-2 font-semibold text-foreground">
                    <IndeterminateCheckbox
                      checked={allSelected}
                      indeterminate={someSelected}
                      onChange={toggleAllSelection}
                    />
                    {allSelected ? t("Deseleccionar todo") : t("Seleccionar todo")}
                  </label>
                  <span>
                    {selectedCount}/{candidates.length} {t("tracks seleccionados")}
                  </span>
                </div>
              ) : null}
              {loading && !candidates.length ? (
                <div className="flex min-h-24 items-center gap-2 text-sm text-muted-foreground">
                  <RefreshCcw className="h-4 w-4 animate-spin" />
                  {t("Generando sugerencias")}
                </div>
              ) : null}
              {!loading && !candidates.length ? (
                <div className="flex min-h-24 items-center text-sm text-muted-foreground">
                  {t("El Copilot mostrara aqui los tracks sugeridos.")}
                </div>
              ) : null}
              {candidates.length ? (
                <div className="grid gap-3">
                  <TrackTable
                    tracks={candidateTracks}
                    columns={["artist", "album", "genre", "bpm", "key", "kind"]}
                    selectedTrackIds={selectedTrackIds}
                    isPlaying={trackPlayer.isPlaying}
                    onDetails={(track) => setDetailTrack(track)}
                    onOpenFolder={openTrackFolder}
                    onPlay={trackPlayer.toggleTrackPlayback}
                    onToggleTrack={toggleTrackSelection}
                  />
                  <CandidateReasonList candidates={candidates} />
                </div>
              ) : null}
            </CardContent>
          </Card>
        </section>
      ) : null}

      <TrackDetailSheet
        track={detailTrack}
        onClose={() => setDetailTrack(null)}
        onOpenFolder={openTrackFolder}
        onPlay={trackPlayer.toggleTrackPlayback}
      />
      <PlaylistAddDialog
        open={addPlaylistDialogOpen}
        busy={playlistBusy}
        contextLabel={t("Playlist Copilot")}
        defaultName={suggestedPlaylistName(prompt)}
        drafts={drafts}
        trackCount={selectedTracks.length}
        onAddExisting={addTracksToDraft}
        onClose={() => setAddPlaylistDialogOpen(false)}
        onCreate={createPlaylistFromTracks}
      />
    </main>
  );
}

function InterpretationPanel({ interpreted }: { interpreted: PlaylistCopilotInterpretation | null }) {
  const { t } = useI18n();
  if (!interpreted) return null;

  const bpmRange = [interpreted.bpm_min, interpreted.bpm_max]
    .filter((value) => typeof value === "number")
    .map((value) => Math.round(Number(value)))
    .join("-");

  return (
    <section className="grid gap-2">
      <InterpretationGroup label={t("Generos")} values={interpreted.genres} />
      <InterpretationGroup label={t("Artistas")} values={interpreted.artists} />
      <InterpretationGroup label="Key" values={interpreted.keys} />
      <InterpretationGroup label="BPM" values={bpmRange ? [bpmRange] : []} />
      <InterpretationGroup label="Mood" values={interpreted.mood ? [interpreted.mood] : []} />
      <InterpretationGroup label={t("Energia")} values={interpreted.energy ? [interpreted.energy] : []} />
      <InterpretationGroup label={t("Excluir")} values={interpreted.exclude_terms} />
    </section>
  );
}

function InterpretationGroup({ label, values }: { label: string; values: string[] }) {
  const { t } = useI18n();
  return (
    <div className="rounded-md border border-border bg-background p-2">
      <div className="mb-2 text-xs font-semibold text-muted-foreground">{label}</div>
      {values.length ? (
        <div className="flex flex-wrap gap-1.5">
          {values.map((value) => (
            <span key={value} className="rounded-md border border-border bg-secondary px-2 py-1 text-xs font-semibold">
              {value}
            </span>
          ))}
        </div>
      ) : (
        <span className="text-xs text-muted-foreground">{t("Sin filtro")}</span>
      )}
    </div>
  );
}

function CandidateReasonList({
  candidates
}: {
  candidates: PlaylistCopilotCandidate[];
}) {
  const { t } = useI18n();
  return (
    <section className="rounded-md border border-border bg-secondary/50">
      <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{t("Por que estos tracks")}</h3>
      <div className="grid max-h-48 gap-2 overflow-y-auto p-3">
        {candidates.slice(0, 12).map((candidate, index) => (
          <div key={candidate.track.track_id} className="grid gap-1 rounded-md bg-background px-3 py-2 text-xs">
            <div className="flex items-center justify-between gap-2">
              <strong className="truncate">
                {index + 1}. {candidate.track.name ?? candidate.track.track_id}
              </strong>
              <span className="shrink-0 rounded-md border border-border px-2 py-0.5 font-semibold">
                {candidate.score.toFixed(1)}
              </span>
            </div>
            <span className="text-muted-foreground">{candidate.reasons.slice(0, 3).join(" · ")}</span>
          </div>
        ))}
      </div>
    </section>
  );
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

  return <input ref={ref} type="checkbox" checked={checked} onChange={onChange} />;
}

function uniqueTrackIds(tracks: Array<{ track_id: string }>) {
  return Array.from(new Set(tracks.map((track) => track.track_id)));
}

function suggestedPlaylistName(prompt: string) {
  const compact = prompt.trim().replace(/\s+/g, " ");
  if (!compact) return "Copilot Playlist";
  return `Copilot - ${compact.slice(0, 42)}`;
}
