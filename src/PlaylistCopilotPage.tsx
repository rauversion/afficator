import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  BarChart3,
  Bot,
  ChevronDown,
  CheckSquare,
  Lightbulb,
  ListPlus,
  ListChecks,
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
  score_components: Record<string, number>;
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
  energy_curve: "flat" | "slow_build" | "ramp" | string;
  harmonic_policy: "ignore" | "soft" | "strict" | string;
  discovery_mode: "known" | "balanced" | "discovery" | string;
  tempo_policy: "tight" | "flexible" | string;
  source_policy: "available_only" | "prefer_available" | "allow_missing" | string;
  focus_policy: "genre" | "mood" | "balanced" | string;
  max_tracks_per_artist: number;
};

type PlaylistCopilotResponse = {
  session_id: string;
  candidate_set_id: string;
  message: string;
  interpreted: PlaylistCopilotInterpretation;
  questions: string[];
  guided_questions: PlaylistCopilotQuestion[];
  steps: PlaylistCopilotStep[];
  brief_changes: string[];
  search_trace: PlaylistCopilotSearchTrace[];
  reasoning_summary: string[];
  title_suggestions: PlaylistCopilotTitleSuggestion[];
  coverage: PlaylistCopilotCoverage;
  candidates: PlaylistCopilotCandidate[];
  used_openai: boolean;
  ranker_version: string;
};

type PlaylistCopilotSearchTrace = {
  id: string;
  label: string;
  candidate_count: number;
  top_similarity?: number | null;
  detail: string;
};

type PlaylistCopilotProgressEvent = {
  request_id: string;
  stage: string;
  status: "working" | "done" | "waiting" | "warning" | string;
  message: string;
  detail?: string | null;
  timestamp: string;
};

type PlaylistCopilotStep = {
  label: string;
  status: "done" | "warning" | string;
  detail: string;
};

type PlaylistCopilotQuestion = {
  id: string;
  question: string;
  options: PlaylistCopilotQuestionOption[];
};

type PlaylistCopilotQuestionOption = {
  label: string;
  value: string;
  description: string;
};

type PlaylistCopilotTitleSuggestion = {
  title: string;
  rationale: string;
};

type TaxonomyCount = {
  kind: string;
  value: string;
  name: string;
  count: number;
};

type PlaylistCopilotCoverage = {
  track_count: number;
  source_missing_count: number;
  bpm_min?: number | null;
  bpm_max?: number | null;
  bpm_average?: number | null;
  genres: TaxonomyCount[];
  keys: TaxonomyCount[];
  formats: TaxonomyCount[];
  top_artists: TaxonomyCount[];
};

type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  kind?: "text" | "thinking" | "progress" | "searches" | "steps" | "choices" | "findings" | "titles" | "inference";
  stage?: string;
  status?: string;
  steps?: PlaylistCopilotStep[];
  searches?: PlaylistCopilotSearchTrace[];
  briefChanges?: string[];
  questions?: PlaylistCopilotQuestion[];
  reasoning?: string[];
  coverage?: PlaylistCopilotCoverage;
  interpreted?: PlaylistCopilotInterpretation;
  titleSuggestions?: PlaylistCopilotTitleSuggestion[];
};

type CopilotResultTab = "candidates" | "interpretation";
type CopilotMode = "auto" | "guided";

type GenerateSuggestionsOptions = {
  appendUserMessage?: boolean;
  requestMode?: CopilotMode;
  thinkingText?: string;
  guidedAnswer?: {
    questionId: string;
    value: string;
  };
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
  const [sessionId, setSessionId] = useState("");
  const [selectedTitle, setSelectedTitle] = useState("");
  const [resultTab, setResultTab] = useState<CopilotResultTab>("candidates");
  const [copilotMode, setCopilotMode] = useState<CopilotMode>("auto");
  const [answeredQuestionIds, setAnsweredQuestionIds] = useState<Set<string>>(new Set());
  const [pendingOptionKey, setPendingOptionKey] = useState("");
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
  const activeRequestIdRef = useRef("");
  const trackPlayer = useTrackPlayer({ t, onError: setErrorMessage });

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const candidates = response?.candidates ?? [];
  const candidateTracks = useMemo(() => candidates.map((candidate) => candidate.track), [candidates]);
  const selectedTracks = useMemo(
    () => candidateTracks.filter((track) => selectedTrackIds.has(track.track_id)),
    [candidateTracks, selectedTrackIds]
  );
  const lastUserPrompt = useMemo(
    () => [...messages].reverse().find((item) => item.role === "user")?.text ?? "",
    [messages]
  );
  const conversationStarted = useMemo(() => messages.some((item) => item.role === "user"), [messages]);
  const selectedCount = selectedTracks.length;
  const allSelected = candidateTracks.length > 0 && selectedCount === candidateTracks.length;
  const someSelected = selectedCount > 0 && !allSelected;

  useEffect(() => {
    void loadLibraries();
  }, []);

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;
    void listen<PlaylistCopilotProgressEvent>("playlist-copilot-progress", ({ payload }) => {
      if (!payload.request_id || payload.request_id !== activeRequestIdRef.current) return;
      const id = `progress-${payload.request_id}-${payload.stage}`;
      const progressMessage: ChatMessage = {
        id,
        role: "assistant",
        kind: "progress",
        stage: payload.stage,
        status: payload.status,
        text: payload.message,
        reasoning: payload.detail ? [payload.detail] : []
      };
      setMessages((current) => {
        const index = current.findIndex((item) => item.id === id);
        if (index < 0) return [...current, progressMessage];
        const next = [...current];
        next[index] = progressMessage;
        return next;
      });
    }).then((stopListening) => {
      if (mounted) {
        unlisten = stopListening;
      } else {
        stopListening();
      }
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
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
    activeRequestIdRef.current = "";
    setActiveLibraryId(libraryId);
    setResponse(null);
    setSessionId("");
    setSelectedTitle("");
    setResultTab("candidates");
    setAnsweredQuestionIds(new Set());
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

  function handleCopilotSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const currentPrompt = prompt.trim();
    if (!currentPrompt) {
      setErrorMessage(t("Describe la playlist que quieres generar."));
      return;
    }

    void generateSuggestions(undefined, currentPrompt);
  }

  async function generateSuggestions(
    event?: React.FormEvent<HTMLFormElement>,
    overridePrompt?: string,
    answeredOverride?: Set<string>,
    options: GenerateSuggestionsOptions = {}
  ) {
    event?.preventDefault();
    const currentPrompt = (overridePrompt ?? prompt).trim();
    if (!activeLibraryId) {
      setErrorMessage(t("Primero indexa una libreria XML."));
      return;
    }
    if (!currentPrompt) {
      setErrorMessage(t("Describe la playlist que quieres generar."));
      return;
    }
    const answeredForResponse = answeredOverride ?? answeredQuestionIds;
    const requestMode = options.requestMode ?? copilotMode;
    const appendUserMessage = options.appendUserMessage !== false;
    const requestId = crypto.randomUUID();
    activeRequestIdRef.current = requestId;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      text: currentPrompt
    };
    const thinkingMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "assistant",
      kind: "thinking",
      text: options.thinkingText ?? (requestMode === "guided"
        ? "Thinking through the next step..."
        : "Building a complete pass...")
    };
    setMessages((current) => appendUserMessage ? [...current, userMessage, thinkingMessage] : [...current, thinkingMessage]);
    setLoading(true);
    setErrorMessage("");
    setMessage("");

    try {
      const result = await invoke<PlaylistCopilotResponse>("playlist_copilot_generate", {
        request: {
          libraryId: activeLibraryId,
          message: currentPrompt,
          requestId,
          targetCount,
          sessionId: sessionId || null,
          mode: requestMode,
          language: locale,
          answeredQuestionIds: Array.from(answeredForResponse),
          guidedAnswer: options.guidedAnswer ?? null
        }
      });
      setResponse(result);
      setSessionId(result.session_id);
      setSelectedTitle((current) => current || result.title_suggestions[0]?.title || "");
      setResultTab("candidates");
      setSelectedTrackIds(new Set(result.candidates.map((candidate) => candidate.track.track_id)));
      setMessages((current) => [
        ...current.filter((item) => item.id !== thinkingMessage.id),
        ...copilotResponseMessages(result, requestMode, answeredForResponse)
      ]);
      setPrompt("");
    } catch (error) {
      const translated = translateBackendMessage(locale, String(error));
      setErrorMessage(translated);
      setMessages((current) => [
        ...current
          .filter((item) => item.id !== thinkingMessage.id)
          .map((item) => item.id.startsWith(`progress-${requestId}-`) && item.status === "working"
            ? { ...item, status: "warning" }
            : item),
        {
          id: crypto.randomUUID(),
          role: "system",
          text: translated
        }
      ]);
    } finally {
      if (activeRequestIdRef.current === requestId) {
        activeRequestIdRef.current = "";
      }
      setLoading(false);
    }
  }

  function applyGuidedOption(question: PlaylistCopilotQuestion, option: PlaylistCopilotQuestionOption) {
    const optionKey = `${question.id}:${option.label}`;
    const nextAnsweredQuestionIds = new Set(answeredQuestionIds);
    nextAnsweredQuestionIds.add(question.id);
    setPendingOptionKey(optionKey);
    setAnsweredQuestionIds(nextAnsweredQuestionIds);
    generateSuggestions(undefined, option.label, nextAnsweredQuestionIds, {
      guidedAnswer: {
        questionId: question.id,
        value: option.value
      }
    }).finally(() => {
      setPendingOptionKey((current) => (current === optionKey ? "" : current));
    });
  }

  function continueWithContext(value: string) {
    void generateSuggestions(undefined, value);
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
    <main className="min-w-0 p-4">
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
        <section className="grid items-start gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(340px,400px)]">
          <Card className="flex min-w-0 flex-col overflow-hidden lg:h-[calc(100vh-7rem)]">
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
            <CardContent className="grid min-h-[620px] min-w-0 flex-1 grid-rows-[minmax(0,1fr)_auto] gap-3 overflow-hidden p-3 lg:h-[calc(100vh-10rem)] lg:min-h-0">
              <div className="h-full min-h-0 overflow-y-auto rounded-md border border-border bg-background p-3">
                <div className="grid gap-2">
                  {messages.map((item) => (
                    <CopilotChatBubble
                      key={item.id}
                      message={item}
                      selectedTitle={selectedTitle}
                      loading={loading}
                      pendingOptionKey={pendingOptionKey}
                      onApplyOption={applyGuidedOption}
                      onContinue={continueWithContext}
                      onSelectTitle={setSelectedTitle}
                    />
                  ))}
                  <div ref={messagesEndRef} />
                </div>
              </div>

              <div className="shrink-0 border-t border-border pt-3">
                {!conversationStarted ? (
                  <div className="mb-2 flex flex-wrap gap-2">
                    {copilotExamples.map((example) => (
                      <Button key={example} type="button" variant="secondary" size="sm" onClick={() => setPrompt(example)}>
                        {example}
                      </Button>
                    ))}
                  </div>
                ) : null}

                <form className="grid gap-2" onSubmit={handleCopilotSubmit}>
                  {copilotMode === "guided" ? (
                    <div className="rounded-md border border-border bg-secondary px-3 py-2 text-xs text-muted-foreground">
                      <strong className="text-foreground">{t("Brief guiado")}</strong>{" "}
                      {t("El Copilot interpreta tu mensaje, infiere lo que ya respondiste y pregunta solo el siguiente dato util.")}
                    </div>
                  ) : null}
                  <textarea
                    className="min-h-28 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    value={prompt}
                    placeholder={t("Ej: 30 tracks deep house, 120-124 BPM, vocales calidas, sin peak time.")}
                    onChange={(event) => setPrompt(event.currentTarget.value)}
                  />
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex flex-wrap items-center gap-2">
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
                      <div className="flex rounded-md border border-border bg-card p-1">
                        <Button
                          type="button"
                          variant={copilotMode === "guided" ? "default" : "ghost"}
                          size="sm"
                          onClick={() => setCopilotMode("guided")}
                        >
                          {t("Preguntar todo")}
                        </Button>
                        <Button
                          type="button"
                          variant={copilotMode === "auto" ? "default" : "ghost"}
                          size="sm"
                          onClick={() => setCopilotMode("auto")}
                        >
                          {t("Aceptar todo")}
                        </Button>
                      </div>
                    </div>
                    <Button disabled={loading || !prompt.trim() || !activeLibraryId}>
                      <Send className="h-4 w-4" />
                      {loading ? t("Generando") : t("Generar sugerencias")}
                    </Button>
                  </div>
                </form>
              </div>
            </CardContent>
          </Card>

          <Card className="min-w-0 xl:sticky xl:top-4 xl:max-h-[calc(100vh-7rem)] xl:overflow-hidden">
            <CardHeader className="flex-col items-stretch gap-2 py-3">
              <div className="min-w-0">
                <CardTitle>{t("Resultados")}</CardTitle>
                <p className="mt-1 text-xs text-muted-foreground">
                  {resultTab === "candidates" && candidates.length
                    ? t("{selected}/{total} tracks seleccionados", { selected: selectedCount, total: candidates.length })
                    : activeLibrary?.source_name}
                </p>
              </div>
              <div className="grid grid-cols-2 gap-1 rounded-md border border-border bg-card p-1">
                <CopilotResultTabButton active={resultTab === "candidates"} onClick={() => setResultTab("candidates")} icon={<ListChecks className="h-4 w-4" />}>
                  {t("Candidatos")}
                </CopilotResultTabButton>
                <CopilotResultTabButton active={resultTab === "interpretation"} onClick={() => setResultTab("interpretation")} icon={<Lightbulb className="h-4 w-4" />}>
                  {t("Interpretacion")}
                </CopilotResultTabButton>
              </div>
            </CardHeader>
            <CardContent className="min-h-0 overflow-y-auto p-3 xl:max-h-[calc(100vh-16rem)]">
              {resultTab === "candidates" ? (
                <div className="grid gap-3">
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

                  {candidates.length ? (
                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-secondary px-3 py-2 text-xs text-muted-foreground">
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
                    <div className="overflow-auto rounded-md border border-border">
                      <TrackTable
                        tracks={candidateTracks}
                        columns={["artist", "genre", "bpm", "key"]}
                        playbackContext={{
                          id: `copilot:${sessionId || activeLibraryId || "current"}`,
                          label: selectedTitle || t("Playlist Copilot")
                        }}
                        selectedTrackIds={selectedTrackIds}
                        isPlaying={trackPlayer.isPlaying}
                        onDetails={(track) => setDetailTrack(track)}
                        onOpenFolder={openTrackFolder}
                        onPlay={trackPlayer.toggleTrackPlayback}
                        onToggleTrack={toggleTrackSelection}
                      />
                    </div>
                  ) : null}
                </div>
              ) : (
                <CopilotInterpretationAccordion
                  activeLibrary={activeLibrary}
                  response={response}
                  selectedTitle={selectedTitle}
                  onSelectTitle={setSelectedTitle}
                />
              )}
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
        defaultName={selectedTitle || suggestedPlaylistName(lastUserPrompt || prompt)}
        drafts={drafts}
        trackCount={selectedTracks.length}
        onAddExisting={addTracksToDraft}
        onClose={() => setAddPlaylistDialogOpen(false)}
        onCreate={createPlaylistFromTracks}
      />
    </main>
  );
}

function CopilotInterpretationAccordion({
  activeLibrary,
  response,
  selectedTitle,
  onSelectTitle
}: {
  activeLibrary: PlaylistIndexLibrary | null;
  response: PlaylistCopilotResponse | null;
  selectedTitle: string;
  onSelectTitle: (title: string) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="grid gap-2">
      <section className="rounded-md border border-border bg-secondary/60 p-3 text-xs text-muted-foreground">
        <strong className="block truncate text-foreground">{activeLibrary?.source_name}</strong>
        {activeLibrary
          ? t("{tracks} tracks · {playlists} playlists · {embeddings} vectores", {
              tracks: activeLibrary.track_count,
              playlists: activeLibrary.playlist_count,
              embeddings: activeLibrary.embedded_track_count
            })
          : null}
      </section>

      {!response ? (
        <div className="rounded-md border border-border bg-secondary/60 p-3 text-sm text-muted-foreground">
          {t("Aun no hay interpretacion. Escribe un brief y genera sugerencias.")}
        </div>
      ) : null}

      {response ? (
        <>
          <AccordionBlock title={t("Interpretacion")} icon={<Bot className="h-4 w-4" />} defaultOpen>
            <InterpretationPanel interpreted={response.interpreted} />
          </AccordionBlock>
          <AccordionBlock
            title={t("Decision trace")}
            icon={<ListChecks className="h-4 w-4" />}
            defaultOpen={response.candidates.length === 0}
          >
            <CopilotStepsPanel steps={response.steps} framed={false} />
          </AccordionBlock>
          <AccordionBlock title={t("Reasoning")} icon={<Lightbulb className="h-4 w-4" />}>
            <ReasoningPanel response={response} compact />
          </AccordionBlock>
          <AccordionBlock title={t("Cobertura")} icon={<BarChart3 className="h-4 w-4" />}>
            <CoveragePanel coverage={response.coverage} />
          </AccordionBlock>
          <AccordionBlock title={t("Titulos")} icon={<Sparkles className="h-4 w-4" />}>
            <TitleSuggestionPanel
              selectedTitle={selectedTitle}
              suggestions={response.title_suggestions}
              onSelectTitle={onSelectTitle}
              showHeading={false}
            />
          </AccordionBlock>
        </>
      ) : null}
    </div>
  );
}

function AccordionBlock({
  title,
  icon,
  defaultOpen = false,
  children
}: {
  title: string;
  icon: React.ReactNode;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  return (
    <details className="group rounded-md border border-border bg-background" open={defaultOpen}>
      <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-sm font-semibold [&::-webkit-details-marker]:hidden">
        {icon}
        <span className="min-w-0 flex-1 truncate">{title}</span>
        <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
      </summary>
      <div className="border-t border-border p-3">{children}</div>
    </details>
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
      <InterpretationGroup label={t("Curva")} values={[policyLabel(interpreted.energy_curve)]} />
      <InterpretationGroup label={t("Armonia")} values={[policyLabel(interpreted.harmonic_policy)]} />
      <InterpretationGroup label={t("Discovery")} values={[policyLabel(interpreted.discovery_mode)]} />
      <InterpretationGroup label={t("Tempo")} values={[policyLabel(interpreted.tempo_policy)]} />
      <InterpretationGroup label={t("Archivos")} values={[policyLabel(interpreted.source_policy)]} />
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

function copilotResponseMessages(
  result: PlaylistCopilotResponse,
  mode: CopilotMode,
  answeredQuestionIds: Set<string>
): ChatMessage[] {
  const isCollectingBrief = result.candidates.length === 0 && result.guided_questions.length > 0;
  const messages: ChatMessage[] = [
    {
      id: crypto.randomUUID(),
      role: "assistant",
      kind: "text",
      text: result.message
    }
  ];

  messages.push({
    id: crypto.randomUUID(),
    role: "assistant",
    kind: "inference",
    text: isCollectingBrief
      ? "Esto inferi del brief antes de hacer la siguiente pregunta."
      : "Esto inferi del brief antes de rankear candidatos.",
    interpreted: result.interpreted,
    briefChanges: result.brief_changes,
    reasoning: result.reasoning_summary
  });

  messages.push({
    id: crypto.randomUUID(),
    role: "assistant",
    kind: "steps",
    text: isCollectingBrief
      ? "Estoy razonando el brief por pasos antes de buscar."
      : "I worked through it in steps.",
    steps: result.steps
  });

  if (!isCollectingBrief) {
    if (result.search_trace.length > 0) {
      messages.push({
        id: crypto.randomUUID(),
        role: "assistant",
        kind: "searches",
        text: "Busque en varias direcciones y fusione los resultados.",
        searches: result.search_trace
      });
    }
    messages.push({
      id: crypto.randomUUID(),
      role: "assistant",
      kind: "findings",
      text: "Here is what I found in your library.",
      reasoning: result.reasoning_summary,
      coverage: result.coverage
    });
  }

  const nextQuestion = result.guided_questions.find((question) => !answeredQuestionIds.has(question.id));
  if (nextQuestion) {
    messages.push({
      id: crypto.randomUUID(),
      role: "assistant",
      kind: "choices",
      text: mode === "guided"
        ? "Before I refine this further, answer this one thing."
        : "Optional next moves if you want to refine it.",
      questions: [nextQuestion]
    });
  }

  if (!isCollectingBrief && result.title_suggestions.length > 0) {
    messages.push({
      id: crypto.randomUUID(),
      role: "assistant",
      kind: "titles",
      text: "I also drafted a few playlist titles.",
      titleSuggestions: result.title_suggestions
    });
  }

  return messages;
}

function CopilotChatBubble({
  message,
  selectedTitle,
  loading,
  pendingOptionKey,
  onApplyOption,
  onContinue,
  onSelectTitle
}: {
  message: ChatMessage;
  selectedTitle: string;
  loading: boolean;
  pendingOptionKey: string;
  onApplyOption: (question: PlaylistCopilotQuestion, option: PlaylistCopilotQuestionOption) => void;
  onContinue: (value: string) => void;
  onSelectTitle: (title: string) => void;
}) {
  const { t } = useI18n();
  const isAssistant = message.role === "assistant";
  const isUser = message.role === "user";

  return (
    <div
      className={cn(
        "max-w-[92%] rounded-md px-3 py-2 text-sm",
        isUser && "ml-auto bg-primary text-primary-foreground",
        isAssistant && "mr-auto bg-secondary text-secondary-foreground",
        message.role === "system" && "mx-auto border border-red-300 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200"
      )}
    >
      {message.kind === "thinking" ? (
        <div className="flex items-center gap-2">
          <RefreshCcw className="h-3.5 w-3.5 animate-spin" />
          <span>{t(message.text)}</span>
          <span className="inline-flex gap-0.5">
            <span className="animate-pulse">.</span>
            <span className="animate-pulse [animation-delay:120ms]">.</span>
            <span className="animate-pulse [animation-delay:240ms]">.</span>
          </span>
        </div>
      ) : null}

      {message.kind === "progress" ? (
        <div className="grid grid-cols-[18px_minmax(0,1fr)] gap-2">
          {message.status === "working" ? (
            <RefreshCcw className="mt-0.5 h-3.5 w-3.5 animate-spin text-primary" />
          ) : message.status === "done" ? (
            <CheckSquare className="mt-0.5 h-3.5 w-3.5 text-emerald-600" />
          ) : (
            <span className="mt-1.5 h-2 w-2 rounded-full bg-amber-500" />
          )}
          <div className="min-w-0">
            <strong className="block text-xs">{t(message.text)}</strong>
            {(message.reasoning ?? []).map((detail) => (
              <span key={detail} className="block text-[11px] text-muted-foreground">
                {t(detail)}
              </span>
            ))}
          </div>
        </div>
      ) : null}

      {!message.kind || message.kind === "text" ? <span className="whitespace-pre-wrap">{t(message.text)}</span> : null}

      {message.kind === "inference" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          <CopilotInferencePanel
            interpreted={message.interpreted ?? null}
            reasoning={message.reasoning ?? []}
            briefChanges={message.briefChanges ?? []}
          />
        </div>
      ) : null}

      {message.kind === "searches" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          <div className="grid gap-1.5">
            {(message.searches ?? []).map((search) => (
              <div key={search.id} className="grid grid-cols-[18px_minmax(0,1fr)] gap-2 rounded-md bg-background/70 px-2 py-2 text-xs">
                <CheckSquare className="mt-0.5 h-3.5 w-3.5 text-emerald-600" />
                <div className="min-w-0">
                  <strong className="block">{t(search.label)}</strong>
                  <span className="text-muted-foreground">{t(search.detail)}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {message.kind === "steps" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          {(message.steps ?? []).map((step, index) => (
            <div key={`${step.label}-${index}`} className="grid grid-cols-[18px_minmax(0,1fr)] gap-2 rounded-md bg-background/70 px-2 py-2 text-xs">
              <span className={cn("mt-1 h-2 w-2 rounded-full", step.status === "warning" ? "bg-amber-500" : "bg-emerald-500")} />
              <div className="min-w-0">
                <strong className="block">{t(step.label)}</strong>
                <span className="text-muted-foreground">{t(step.detail)}</span>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {message.kind === "findings" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          {(message.reasoning ?? []).slice(0, 5).map((item) => (
            <p key={item} className="m-0 rounded-md bg-background/70 px-2 py-2 text-xs text-muted-foreground">
              {t(item)}
            </p>
          ))}
          {message.coverage ? (
            <div className="grid gap-2 rounded-md bg-background/70 p-2">
              <div className="flex flex-wrap gap-1.5">
                <CopilotMetricChip label="Tracks" value={message.coverage.track_count} />
                <CopilotMetricChip label="BPM" value={formatBpmCoverage(message.coverage)} />
                <CopilotMetricChip label={t("Archivos faltantes")} value={message.coverage.source_missing_count} />
              </div>
              <CopilotDiscoveryChips
                label={t("Artistas")}
                items={message.coverage.top_artists}
                onContinue={(artist) => onContinue(`Explore more around artist ${artist}, but keep the same playlist direction.`)}
              />
              <CopilotDiscoveryChips
                label={t("Generos")}
                items={message.coverage.genres}
                onContinue={(genre) => onContinue(`Lean further into ${genre} and reduce weaker genre matches.`)}
              />
            </div>
          ) : null}
        </div>
      ) : null}

      {message.kind === "choices" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          {(message.questions ?? []).map((question) => (
            <div key={question.id} className="grid gap-1 rounded-md bg-background/70 p-2">
              <span className="text-xs font-semibold">{t(question.question)}</span>
              <div className="flex flex-wrap gap-1.5">
                {question.options.map((option) => {
                  const optionKey = `${question.id}:${option.label}`;
                  const isPendingOption = pendingOptionKey === optionKey;

                  return (
                    <Button
                      key={option.label}
                      type="button"
                      variant="secondary"
                      size="sm"
                      className="h-auto items-start whitespace-normal py-1.5 text-left text-xs"
                      disabled={loading}
                      onClick={() => onApplyOption(question, option)}
                    >
                      {isPendingOption ? <RefreshCcw className="h-3 w-3 animate-spin" /> : null}
                      <span className="grid gap-0.5">
                        <span className="font-semibold">{isPendingOption ? t("Continuando") : t(option.label)}</span>
                        {option.description ? (
                          <span className="text-[11px] font-normal text-muted-foreground">
                            {t(option.description)}
                          </span>
                        ) : null}
                      </span>
                    </Button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {message.kind === "titles" ? (
        <div className="grid gap-2">
          <strong>{t(message.text)}</strong>
          <div className="flex flex-wrap gap-1.5">
            {(message.titleSuggestions ?? []).map((suggestion, index) => (
              <Button
                key={`${suggestion.title}-${index}`}
                type="button"
                variant={selectedTitle === suggestion.title ? "default" : "secondary"}
                size="sm"
                onClick={() => onSelectTitle(suggestion.title)}
              >
                {suggestion.title}
              </Button>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function CopilotMetricChip({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <span className="rounded-md border border-border bg-secondary px-2 py-1 text-[11px]">
      <strong>{label}:</strong> {value}
    </span>
  );
}

function CopilotInferencePanel({
  interpreted,
  reasoning,
  briefChanges
}: {
  interpreted: PlaylistCopilotInterpretation | null;
  reasoning: string[];
  briefChanges: string[];
}) {
  const { t } = useI18n();

  if (!interpreted) {
    return <span className="text-xs text-muted-foreground">{t("Sin datos para mostrar.")}</span>;
  }

  const bpmRange = [interpreted.bpm_min, interpreted.bpm_max]
    .filter((value) => typeof value === "number")
    .map((value) => Math.round(Number(value)))
    .join("-");
  const signals = [
    { label: t("Generos"), values: interpreted.genres },
    { label: t("Artistas"), values: interpreted.artists },
    { label: "Key", values: interpreted.keys },
    { label: "BPM", values: bpmRange ? [bpmRange] : [] },
    { label: "Mood", values: interpreted.mood ? [interpreted.mood] : [] },
    { label: t("Energia"), values: interpreted.energy ? [interpreted.energy] : [] },
    { label: t("Excluir"), values: interpreted.exclude_terms },
    { label: t("Curva"), values: interpreted.energy_curve !== "flat" ? [policyLabel(interpreted.energy_curve)] : [] },
    { label: t("Armonia"), values: [policyLabel(interpreted.harmonic_policy)] },
    { label: t("Discovery"), values: [policyLabel(interpreted.discovery_mode)] },
    { label: t("Tempo"), values: [policyLabel(interpreted.tempo_policy)] }
  ].filter((signal) => signal.values.length > 0);

  return (
    <div className="grid gap-2 rounded-md bg-background/70 p-2">
      {briefChanges.length > 0 ? (
        <div className="grid gap-1 border-b border-border pb-2">
          <span className="text-[11px] font-semibold text-muted-foreground">
            {t("Cambios aplicados al brief")}
          </span>
          {briefChanges.map((change) => (
            <span key={change} className="text-xs">
              {t(change)}
            </span>
          ))}
        </div>
      ) : null}
      {signals.length ? (
        <div className="flex flex-wrap gap-1.5">
          {signals.map((signal) => (
            <span key={signal.label} className="rounded-md border border-border bg-secondary px-2 py-1 text-[11px]">
              <strong>{signal.label}:</strong> {signal.values.slice(0, 4).join(", ")}
            </span>
          ))}
        </div>
      ) : (
        <span className="text-xs text-muted-foreground">
          {t("Todavia no hay senales fuertes; por eso pregunto el siguiente dato.")}
        </span>
      )}

      {reasoning.slice(0, 2).map((item) => (
        <p key={item} className="m-0 text-xs text-muted-foreground">
          {t(item)}
        </p>
      ))}
    </div>
  );
}

function CopilotDiscoveryChips({
  label,
  items,
  onContinue
}: {
  label: string;
  items: TaxonomyCount[];
  onContinue: (value: string) => void;
}) {
  if (items.length === 0) return null;

  return (
    <div className="grid gap-1">
      <span className="text-[11px] font-semibold text-muted-foreground">{label}</span>
      <div className="flex flex-wrap gap-1.5">
        {items.slice(0, 6).map((item) => (
          <button
            key={`${item.kind}-${item.value}`}
            type="button"
            className="rounded-md border border-border bg-secondary px-2 py-1 text-[11px] font-semibold hover:bg-muted"
            onClick={() => onContinue(item.name)}
          >
            {item.name} · {item.count}
          </button>
        ))}
      </div>
    </div>
  );
}

function CopilotStepsPanel({ steps, framed = true }: { steps: PlaylistCopilotStep[]; framed?: boolean }) {
  const { t } = useI18n();
  if (steps.length === 0) return null;

  const content = (
    <div className="grid gap-2">
      {steps.map((step, index) => (
        <div key={`${step.label}-${index}`} className="grid grid-cols-[22px_minmax(0,1fr)] gap-2 text-xs">
          <span
            className={cn(
              "mt-0.5 h-2.5 w-2.5 rounded-full",
              step.status === "warning" ? "bg-amber-500" : "bg-emerald-500"
            )}
          />
          <div className="min-w-0">
            <strong className="block truncate">{t(step.label)}</strong>
            <span className="block text-muted-foreground">{t(step.detail)}</span>
          </div>
        </div>
      ))}
    </div>
  );

  if (!framed) return content;

  return (
    <section className="rounded-md border border-border bg-background">
      <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{t("Decision trace")}</h3>
      <div className="p-3">{content}</div>
    </section>
  );
}

function GuidedQuestionsPanel({
  questions,
  onApplyOption
}: {
  questions: PlaylistCopilotQuestion[];
  onApplyOption: (option: PlaylistCopilotQuestionOption) => void;
}) {
  const { t } = useI18n();
  if (questions.length === 0) return null;

  return (
    <section className="grid gap-2">
      <h3 className="text-sm font-semibold">{t("Guided choices")}</h3>
      {questions.map((question) => (
        <div key={question.id} className="rounded-md border border-border bg-background p-2">
          <strong className="block text-xs">{t(question.question)}</strong>
          <div className="mt-2 grid gap-1.5">
            {question.options.map((option) => (
              <button
                key={option.label}
                type="button"
                className="rounded-md border border-border bg-secondary px-2 py-2 text-left text-xs hover:bg-muted"
                onClick={() => onApplyOption(option)}
              >
                <span className="block font-semibold">{t(option.label)}</span>
                <span className="block text-muted-foreground">{t(option.description)}</span>
              </button>
            ))}
          </div>
        </div>
      ))}
    </section>
  );
}

function TitleSuggestionPanel({
  selectedTitle,
  suggestions,
  expanded = false,
  showHeading = true,
  onSelectTitle
}: {
  selectedTitle: string;
  suggestions: PlaylistCopilotTitleSuggestion[];
  expanded?: boolean;
  showHeading?: boolean;
  onSelectTitle: (title: string) => void;
}) {
  const { t } = useI18n();
  if (suggestions.length === 0) {
    return <span className="text-xs text-muted-foreground">{t("Sin datos para mostrar.")}</span>;
  }

  return (
    <section className="grid gap-2">
      {showHeading ? <h3 className="text-sm font-semibold">{t("Titulos sugeridos")}</h3> : null}
      <div className={cn("grid gap-2", expanded && "md:grid-cols-2")}>
        {suggestions.map((suggestion, index) => (
          <button
            key={`${suggestion.title}-${index}`}
            type="button"
            className={cn(
              "rounded-md border px-3 py-2 text-left text-xs",
              selectedTitle === suggestion.title
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border bg-background hover:bg-secondary"
            )}
            onClick={() => onSelectTitle(suggestion.title)}
          >
            <span className="block font-semibold">{suggestion.title}</span>
            <span className={cn("mt-1 block", selectedTitle === suggestion.title ? "text-primary-foreground/75" : "text-muted-foreground")}>
              {t(suggestion.rationale)}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

function ReasoningPanel({ response, compact = false }: { response: PlaylistCopilotResponse; compact?: boolean }) {
  const { t } = useI18n();
  const summary = (
    <ul className="grid gap-2 text-sm">
      {response.reasoning_summary.length === 0 ? (
        <li className="text-xs text-muted-foreground">{t("Sin datos para mostrar.")}</li>
      ) : null}
      {response.reasoning_summary.map((item) => (
        <li key={item} className="rounded-md bg-background px-3 py-2 text-xs text-muted-foreground">
          {t(item)}
        </li>
      ))}
    </ul>
  );

  if (compact) {
    return (
      <div className="grid gap-3">
        {summary}
        <CandidateReasonList candidates={response.candidates} />
      </div>
    );
  }

  return (
    <div className="grid gap-3">
      <section className="rounded-md border border-border bg-secondary/50">
        <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{t("Reasoning summary")}</h3>
        <div className="p-3">{summary}</div>
      </section>
      <CandidateReasonList candidates={response.candidates} />
    </div>
  );
}

function CoveragePanel({ coverage }: { coverage: PlaylistCopilotCoverage }) {
  const { t } = useI18n();
  return (
    <div className="grid gap-3">
      <section className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <CoverageMetric label={t("Tracks")} value={coverage.track_count} />
        <CoverageMetric label="BPM" value={formatBpmCoverage(coverage)} />
        <CoverageMetric label={t("Generos")} value={coverage.genres.length} />
        <CoverageMetric label={t("Archivos faltantes")} value={coverage.source_missing_count} danger={coverage.source_missing_count > 0} />
      </section>
      <section className="grid gap-3 lg:grid-cols-2">
        <CoverageList title={t("Generos")} items={coverage.genres} />
        <CoverageList title="Keys" items={coverage.keys} />
        <CoverageList title={t("Artistas")} items={coverage.top_artists} />
        <CoverageList title={t("Formatos")} items={coverage.formats} />
      </section>
    </div>
  );
}

function CoverageMetric({ label, value, danger = false }: { label: string; value: React.ReactNode; danger?: boolean }) {
  return (
    <div className={cn("rounded-md border border-border bg-secondary px-3 py-2", danger && "border-red-300 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="text-lg">{value}</strong>
    </div>
  );
}

function CoverageList({ title, items }: { title: string; items: TaxonomyCount[] }) {
  const { t } = useI18n();
  return (
    <div className="rounded-md border border-border bg-background">
      <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{title}</h3>
      <div className="grid gap-1 p-3">
        {items.length === 0 ? <span className="text-xs text-muted-foreground">{t("Sin datos para mostrar.")}</span> : null}
        {items.map((item) => (
          <div key={`${item.kind}-${item.value}`} className="grid grid-cols-[minmax(0,1fr)_56px] gap-2 text-xs">
            <span className="truncate">{item.name}</span>
            <strong className="text-right tabular-nums">{item.count}</strong>
          </div>
        ))}
      </div>
    </div>
  );
}

function CopilotResultTabButton({
  active,
  children,
  icon,
  onClick
}: {
  active: boolean;
  children: React.ReactNode;
  icon: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      variant={active ? "default" : "ghost"}
      size="sm"
      className="w-full min-w-0 justify-center whitespace-normal px-2 text-center"
      onClick={onClick}
    >
      {icon}
      {children}
    </Button>
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
        {candidates.length === 0 ? (
          <span className="text-xs text-muted-foreground">{t("Sin candidatos todavia.")}</span>
        ) : null}
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
              <span className="text-[10px] text-muted-foreground">
                {topScoreComponents(candidate.score_components)}
              </span>
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

function policyLabel(value: string) {
  return value.split("_").join(" ");
}

function topScoreComponents(components: Record<string, number>) {
  return Object.entries(components)
    .filter(([, value]) => Math.abs(value) >= 0.1)
    .sort((left, right) => Math.abs(right[1]) - Math.abs(left[1]))
    .slice(0, 3)
    .map(([name, value]) => `${policyLabel(name)} ${value >= 0 ? "+" : ""}${value.toFixed(1)}`)
    .join(" · ");
}

function formatBpmCoverage(coverage: PlaylistCopilotCoverage) {
  if (typeof coverage.bpm_min !== "number" || typeof coverage.bpm_max !== "number") return "n/d";
  const average = typeof coverage.bpm_average === "number" ? ` · avg ${Math.round(coverage.bpm_average)}` : "";
  return `${Math.round(coverage.bpm_min)}-${Math.round(coverage.bpm_max)}${average}`;
}

function suggestedPlaylistName(prompt: string) {
  const compact = prompt.trim().replace(/\s+/g, " ");
  if (!compact) return "Copilot Playlist";
  return `Copilot - ${compact.slice(0, 42)}`;
}
