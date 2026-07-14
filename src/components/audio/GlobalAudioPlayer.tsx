import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ChevronDown, FolderOpen, Music2, Pause, Play, SkipBack, SkipForward, Square, Volume2, X } from "lucide-react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode
} from "react";
import { playbackErrorMessage } from "../../playback";
import { useI18n } from "../../i18n";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import type { TrackListItem } from "../tracks/types";

type AudioPlayerState = {
  label: string;
  path: string;
  url: string;
};

type PlaybackQueueItem = {
  id: string;
  libraryId?: string;
  path: string;
  label: string;
  sourceIndex: number;
  trackId: string;
};

type PlaybackQueueState = {
  id: string;
  label?: string | null;
  items: PlaybackQueueItem[];
  currentIndex: number;
};

type PlaybackQueueContext = {
  id: string;
  label?: string | null;
};

type PlaybackErrorHandler = (message: string) => void;

type GlobalAudioPlayerContextValue = {
  isPlaying: (trackOrPath?: TrackListItem | string | null) => boolean;
  player: AudioPlayerState | null;
  playing: boolean;
  stop: () => void;
  togglePathPlayback: (path?: string | null, label?: string | null, onError?: PlaybackErrorHandler) => Promise<void>;
  toggleTrackListPlayback: (
    tracks: TrackListItem[],
    track: TrackListItem,
    context?: PlaybackQueueContext,
    onError?: PlaybackErrorHandler
  ) => Promise<void>;
  toggleTrackPlayback: (track: TrackListItem, onError?: PlaybackErrorHandler) => Promise<void>;
};

type SidebarAudioPlayerContextValue = GlobalAudioPlayerContextValue & {
  clear: () => void;
  collapsed: boolean;
  currentTime: number;
  duration: number;
  hasNext: boolean;
  hasPrevious: boolean;
  playNext: () => Promise<void>;
  playPrevious: () => Promise<void>;
  progress: number;
  queue: PlaybackQueueState | null;
  queuePosition: number;
  queueTotal: number;
  setCollapsed: (collapsed: boolean) => void;
  setVolume: (volume: number) => void;
  togglePlayer: () => Promise<void>;
  volume: number;
};

const GlobalAudioPlayerContext = createContext<GlobalAudioPlayerContextValue | null>(null);
const SidebarAudioPlayerContext = createContext<SidebarAudioPlayerContextValue | null>(null);
const collapsedStorageKey = "rau-studio.audioPlayer.collapsed";
const volumeStorageKey = "rau-studio.audioPlayer.volume";

function readInitialCollapsed() {
  if (typeof window === "undefined") return false;
  return localStorage.getItem(collapsedStorageKey) === "true";
}

function readInitialVolume() {
  if (typeof window === "undefined") return 1;
  const storedValue = localStorage.getItem(volumeStorageKey);
  if (storedValue === null) return 1;
  const storedVolume = Number(storedValue);
  return Number.isFinite(storedVolume) ? clampVolume(storedVolume) : 1;
}

export function GlobalAudioPlayerProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  const audioElement = useRef<HTMLAudioElement | null>(null);
  const errorHandler = useRef<PlaybackErrorHandler | null>(null);
  const [player, setPlayer] = useState<AudioPlayerState | null>(null);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [queue, setQueue] = useState<PlaybackQueueState | null>(null);
  const queueRef = useRef<PlaybackQueueState | null>(null);
  const [collapsed, setCollapsedState] = useState(readInitialCollapsed);
  const [volume, setVolumeState] = useState(readInitialVolume);

  const progress = duration > 0 ? Math.min(100, (currentTime / duration) * 100) : 0;
  const queuePosition = queue ? queue.currentIndex + 1 : 0;
  const queueTotal = queue?.items.length ?? 0;
  const hasPrevious = Boolean(queue && queue.currentIndex > 0);
  const hasNext = Boolean(queue && queue.currentIndex < queue.items.length - 1);

  const setPlaybackQueue = useCallback((nextQueue: PlaybackQueueState | null) => {
    queueRef.current = nextQueue;
    setQueue(nextQueue);
  }, []);

  const setCollapsed = useCallback((nextCollapsed: boolean) => {
    setCollapsedState(nextCollapsed);
    localStorage.setItem(collapsedStorageKey, String(nextCollapsed));
  }, []);

  const setVolume = useCallback((nextVolume: number) => {
    const clampedVolume = clampVolume(nextVolume);
    setVolumeState(clampedVolume);
    localStorage.setItem(volumeStorageKey, String(clampedVolume));
    if (audioElement.current) {
      audioElement.current.volume = clampedVolume;
    }
  }, []);

  useEffect(() => {
    if (!audioElement.current) return;
    audioElement.current.volume = volume;
  }, [player, volume]);

  const reportError = useCallback(
    (nextPlayer: AudioPlayerState, error?: unknown) => {
      const message = playbackErrorMessage(t, nextPlayer.label, nextPlayer.path, error);
      errorHandler.current?.(message);
    },
    [t]
  );

  const syncTime = useCallback((audio: HTMLAudioElement | null = audioElement.current) => {
    if (!audio) return;
    setCurrentTime(audio.currentTime || 0);
    setDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
  }, []);

  const beginPlayback = useCallback(
    (nextPlayer: AudioPlayerState, onError?: PlaybackErrorHandler | null) => {
      errorHandler.current = onError ?? null;
      setPlayer(nextPlayer);
      setPlaying(false);
      setCurrentTime(0);
      setDuration(0);

      window.setTimeout(() => {
        void audioElement.current?.play().catch((error) => {
          setPlaying(false);
          reportError(nextPlayer, error);
        });
      }, 30);
    },
    [reportError]
  );

  const playPath = useCallback(
    async (path?: string | null, label?: string | null, onError?: PlaybackErrorHandler) => {
      if (!path) return;
      beginPlayback({ path, label: label || path, url: convertFileSrc(path) }, onError);
    },
    [beginPlayback]
  );

  const stop = useCallback(() => {
    audioElement.current?.pause();
    if (audioElement.current) {
      audioElement.current.currentTime = 0;
    }
    setPlaying(false);
    setCurrentTime(0);
  }, []);

  const clear = useCallback(() => {
    audioElement.current?.pause();
    setPlaying(false);
    setCurrentTime(0);
    setDuration(0);
    setPlayer(null);
    setPlaybackQueue(null);
    errorHandler.current = null;
  }, [setPlaybackQueue]);

  const togglePlayer = useCallback(async () => {
    if (!audioElement.current || !player) return;

    try {
      if (audioElement.current.paused) {
        await audioElement.current.play();
        setPlaying(true);
      } else {
        audioElement.current.pause();
        setPlaying(false);
      }
    } catch (error) {
      reportError(player, error);
    }
  }, [player, reportError]);

  const playQueueIndex = useCallback(
    async (nextQueue: PlaybackQueueState, nextIndex: number, onError?: PlaybackErrorHandler | null) => {
      const item = nextQueue.items[nextIndex];
      if (!item) return;

      setPlaybackQueue({ ...nextQueue, currentIndex: nextIndex });
      beginPlayback({ path: item.path, label: item.label, url: convertFileSrc(item.path) }, onError ?? errorHandler.current);
    },
    [beginPlayback, setPlaybackQueue]
  );

  const playNext = useCallback(async () => {
    const currentQueue = queueRef.current;
    if (!currentQueue) return;

    await playQueueIndex(currentQueue, currentQueue.currentIndex + 1, errorHandler.current);
  }, [playQueueIndex]);

  const playPrevious = useCallback(async () => {
    const currentQueue = queueRef.current;
    if (!currentQueue) return;

    await playQueueIndex(currentQueue, currentQueue.currentIndex - 1, errorHandler.current);
  }, [playQueueIndex]);

  const togglePathPlayback = useCallback(
    async (path?: string | null, label?: string | null, onError?: PlaybackErrorHandler) => {
      if (!path) return;
      errorHandler.current = onError ?? null;

      if (player?.path === path && playing) {
        stop();
        return;
      }

      if (player?.path === path) {
        await togglePlayer();
        return;
      }

      setPlaybackQueue(null);
      await playPath(path, label, onError);
    },
    [playPath, player?.path, playing, setPlaybackQueue, stop, togglePlayer]
  );

  const toggleTrackListPlayback = useCallback(
    async (
      tracks: TrackListItem[],
      track: TrackListItem,
      context?: PlaybackQueueContext,
      onError?: PlaybackErrorHandler
    ) => {
      if (!track.source_path || !track.source_exists) return;
      errorHandler.current = onError ?? errorHandler.current;

      const items = buildPlaybackQueueItems(tracks);
      if (items.length === 0) return;

      const clickedSourceIndex = tracks.findIndex((candidate) => candidate === track);
      const fallbackIndex = items.findIndex(
        (item) =>
          item.path === track.source_path &&
          item.trackId === track.track_id &&
          item.libraryId === track.library_id
      );
      const startIndex =
        clickedSourceIndex >= 0
          ? items.findIndex((item) => item.sourceIndex === clickedSourceIndex)
          : fallbackIndex;
      const currentIndex = startIndex >= 0 ? startIndex : fallbackIndex;
      if (currentIndex < 0) return;

      const nextQueue: PlaybackQueueState = {
        id: context?.id ?? "current-list",
        label: context?.label,
        items,
        currentIndex
      };

      setPlaybackQueue(nextQueue);

      if (player?.path === track.source_path && playing) {
        stop();
        return;
      }

      if (player?.path === track.source_path) {
        await togglePlayer();
        return;
      }

      await playQueueIndex(nextQueue, currentIndex, onError);
    },
    [playQueueIndex, player?.path, playing, setPlaybackQueue, stop, togglePlayer]
  );

  const toggleTrackPlayback = useCallback(
    (track: TrackListItem, onError?: PlaybackErrorHandler) =>
      togglePathPlayback(track.source_path, track.name ?? track.source_path, onError),
    [togglePathPlayback]
  );

  const isPlaying = useCallback(
    (trackOrPath?: TrackListItem | string | null) => {
      const path = typeof trackOrPath === "string" ? trackOrPath : trackOrPath?.source_path;
      return Boolean(path && player?.path === path && playing);
    },
    [player?.path, playing]
  );

  const value = useMemo<GlobalAudioPlayerContextValue>(
    () => ({
      isPlaying,
      player,
      playing,
      stop,
      togglePathPlayback,
      toggleTrackListPlayback,
      toggleTrackPlayback
    }),
    [
      isPlaying,
      player,
      playing,
      stop,
      togglePathPlayback,
      toggleTrackListPlayback,
      toggleTrackPlayback
    ]
  );

  const sidebarValue = useMemo<SidebarAudioPlayerContextValue>(
    () => ({
      ...value,
      clear,
      collapsed,
      currentTime,
      duration,
      hasNext,
      hasPrevious,
      playNext,
      playPrevious,
      progress,
      queue,
      queuePosition,
      queueTotal,
      setCollapsed,
      setVolume,
      togglePlayer,
      volume
    }),
    [
      value,
      clear,
      collapsed,
      currentTime,
      duration,
      hasNext,
      hasPrevious,
      playNext,
      playPrevious,
      progress,
      queue,
      queuePosition,
      queueTotal,
      setCollapsed,
      setVolume,
      togglePlayer,
      volume
    ]
  );

  return (
    <GlobalAudioPlayerContext.Provider value={value}>
      <SidebarAudioPlayerContext.Provider value={sidebarValue}>
        {children}
        {player ? (
          <audio
            className="hidden"
            ref={audioElement}
            src={player.url}
            onLoadedMetadata={(event) => syncTime(event.currentTarget)}
            onTimeUpdate={(event) => syncTime(event.currentTarget)}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onEnded={() => {
              const currentQueue = queueRef.current;
              if (currentQueue && currentQueue.currentIndex < currentQueue.items.length - 1) {
                void playNext();
                return;
              }

              setPlaying(false);
              setCurrentTime(0);
            }}
            onError={() => reportError(player)}
          />
        ) : null}
      </SidebarAudioPlayerContext.Provider>
    </GlobalAudioPlayerContext.Provider>
  );
}

export function useGlobalAudioPlayer() {
  const context = useContext(GlobalAudioPlayerContext);
  if (!context) {
    throw new Error("useGlobalAudioPlayer must be used inside GlobalAudioPlayerProvider");
  }
  return context;
}

function useSidebarAudioPlayer() {
  const context = useContext(SidebarAudioPlayerContext);
  if (!context) {
    throw new Error("useSidebarAudioPlayer must be used inside GlobalAudioPlayerProvider");
  }
  return context;
}

export function SidebarAudioPlayer() {
  const { t } = useI18n();
  const player = useSidebarAudioPlayer();

  async function openFolder() {
    if (!player.player?.path) return;
    await invoke("open_parent_folder", { path: player.player.path }).catch(() => undefined);
  }

  return (
    <section className="min-w-0 overflow-hidden rounded-md border border-border bg-background p-2">
      <div className="flex min-w-0 items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-2 text-xs font-semibold">
          <Music2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate">{t("Player")}</span>
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-6 w-6 shrink-0"
          aria-label={player.collapsed ? t("Expandir player") : t("Contraer player")}
          onClick={() => player.setCollapsed(!player.collapsed)}
        >
          <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", player.collapsed && "-rotate-90")} />
        </Button>
      </div>

      {player.collapsed ? (
        <div className="mt-1 flex min-w-0 items-center gap-2 overflow-hidden">
          <span
            className="block min-w-0 max-w-full flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-muted-foreground"
            title={player.player?.label ?? player.player?.path ?? ""}
          >
            {player.player?.label ?? t("Sin archivo cargado")}
          </span>
          {player.queueTotal > 1 ? (
            <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
              {player.queuePosition}/{player.queueTotal}
            </span>
          ) : null}
          {player.player ? (
            <Button
              type="button"
              variant="secondary"
              size="icon"
              className="h-6 w-6 shrink-0"
              aria-label={player.playing ? t("Pause") : t("Play")}
              onClick={() => void player.togglePlayer()}
            >
              {player.playing ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="mt-2 grid gap-2">
          <div className="min-w-0">
            <strong className="block truncate text-xs" title={player.player?.path ?? ""}>
              {player.player?.label ?? t("Sin archivo cargado")}
            </strong>
            <span className="block truncate text-[10px] text-muted-foreground" title={player.player?.path ?? ""}>
              {player.player?.path ?? t("Selecciona un track para escucharlo.")}
            </span>
            {player.queueTotal > 1 ? (
              <span className="mt-1 block truncate text-[10px] text-muted-foreground" title={player.queue?.label ?? ""}>
                {[player.queue?.label, `${player.queuePosition}/${player.queueTotal}`].filter(Boolean).join(" · ")}
              </span>
            ) : null}
          </div>

          <div className="h-1.5 overflow-hidden rounded-full bg-muted">
            <div className="h-full rounded-full bg-primary transition-[width]" style={{ width: `${player.progress}%` }} />
          </div>

          <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
            <span>{formatAudioTime(player.currentTime)}</span>
            <span>{formatAudioTime(player.duration)}</span>
          </div>

          <div className="flex items-center gap-2">
            <Volume2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={Math.round(player.volume * 100)}
              aria-label={t("Volumen")}
              className="h-1.5 min-w-0 flex-1 accent-primary"
              onChange={(event) => player.setVolume(Number(event.currentTarget.value) / 100)}
            />
            <span className="w-8 text-right text-[10px] tabular-nums text-muted-foreground">
              {Math.round(player.volume * 100)}%
            </span>
          </div>

          <div className="flex items-center gap-1">
            <Button type="button" variant="secondary" size="icon" disabled={!player.hasPrevious} aria-label={t("Anterior")} onClick={() => void player.playPrevious()}>
              <SkipBack className="h-3.5 w-3.5" />
            </Button>
            <Button type="button" size="sm" className="flex-1" disabled={!player.player} onClick={() => void player.togglePlayer()}>
              {player.playing ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
              {player.playing ? t("Pause") : t("Play")}
            </Button>
            <Button type="button" variant="secondary" size="icon" disabled={!player.hasNext} aria-label={t("Siguiente")} onClick={() => void player.playNext()}>
              <SkipForward className="h-3.5 w-3.5" />
            </Button>
            <Button type="button" variant="secondary" size="icon" disabled={!player.player} onClick={player.stop}>
              <Square className="h-3.5 w-3.5" />
            </Button>
            <Button type="button" variant="secondary" size="icon" disabled={!player.player} onClick={() => void openFolder()}>
              <FolderOpen className="h-3.5 w-3.5" />
            </Button>
            <Button type="button" variant="ghost" size="icon" disabled={!player.player} onClick={player.clear}>
              <X className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

function buildPlaybackQueueItems(tracks: TrackListItem[]) {
  return tracks.reduce<PlaybackQueueItem[]>((items, track, sourceIndex) => {
    if (!track.source_exists || !track.source_path) return items;

    items.push({
      id: `${track.library_id ?? "library"}:${track.track_id}:${sourceIndex}`,
      libraryId: track.library_id,
      path: track.source_path,
      label: track.name ?? track.source_path,
      sourceIndex,
      trackId: track.track_id
    });

    return items;
  }, []);
}

function clampVolume(volume: number) {
  if (!Number.isFinite(volume)) return 1;
  return Math.min(1, Math.max(0, volume));
}

function formatAudioTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${remainder}`;
}
