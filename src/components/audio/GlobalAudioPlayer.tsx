import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ChevronDown, FolderOpen, Music2, Pause, Play, Square, X } from "lucide-react";
import {
  createContext,
  useCallback,
  useContext,
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

type PlaybackErrorHandler = (message: string) => void;

type GlobalAudioPlayerContextValue = {
  clear: () => void;
  collapsed: boolean;
  currentTime: number;
  duration: number;
  isPlaying: (trackOrPath?: TrackListItem | string | null) => boolean;
  player: AudioPlayerState | null;
  playing: boolean;
  progress: number;
  setCollapsed: (collapsed: boolean) => void;
  stop: () => void;
  togglePathPlayback: (path?: string | null, label?: string | null, onError?: PlaybackErrorHandler) => Promise<void>;
  togglePlayer: () => Promise<void>;
  toggleTrackPlayback: (track: TrackListItem, onError?: PlaybackErrorHandler) => Promise<void>;
};

const GlobalAudioPlayerContext = createContext<GlobalAudioPlayerContextValue | null>(null);
const collapsedStorageKey = "rau-studio.audioPlayer.collapsed";

function readInitialCollapsed() {
  if (typeof window === "undefined") return false;
  return localStorage.getItem(collapsedStorageKey) === "true";
}

export function GlobalAudioPlayerProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  const audioElement = useRef<HTMLAudioElement | null>(null);
  const errorHandler = useRef<PlaybackErrorHandler | null>(null);
  const [player, setPlayer] = useState<AudioPlayerState | null>(null);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [collapsed, setCollapsedState] = useState(readInitialCollapsed);

  const progress = duration > 0 ? Math.min(100, (currentTime / duration) * 100) : 0;

  const setCollapsed = useCallback((nextCollapsed: boolean) => {
    setCollapsedState(nextCollapsed);
    localStorage.setItem(collapsedStorageKey, String(nextCollapsed));
  }, []);

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

  const playPath = useCallback(
    async (path?: string | null, label?: string | null, onError?: PlaybackErrorHandler) => {
      if (!path) return;
      const nextPlayer = { path, label: label || path, url: convertFileSrc(path) };
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
    errorHandler.current = null;
  }, []);

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

      await playPath(path, label, onError);
    },
    [playPath, player?.path, playing, stop, togglePlayer]
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
      clear,
      collapsed,
      currentTime,
      duration,
      isPlaying,
      player,
      playing,
      progress,
      setCollapsed,
      stop,
      togglePathPlayback,
      togglePlayer,
      toggleTrackPlayback
    }),
    [
      clear,
      collapsed,
      currentTime,
      duration,
      isPlaying,
      player,
      playing,
      progress,
      setCollapsed,
      stop,
      togglePathPlayback,
      togglePlayer,
      toggleTrackPlayback
    ]
  );

  return (
    <GlobalAudioPlayerContext.Provider value={value}>
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
            setPlaying(false);
            setCurrentTime(0);
          }}
          onError={() => reportError(player)}
        />
      ) : null}
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

export function SidebarAudioPlayer() {
  const { t } = useI18n();
  const player = useGlobalAudioPlayer();

  async function openFolder() {
    if (!player.player?.path) return;
    await invoke("open_parent_folder", { path: player.player.path }).catch(() => undefined);
  }

  return (
    <section className="rounded-md border border-border bg-background p-2">
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-2 text-xs font-semibold">
          <Music2 className="h-3.5 w-3.5 text-muted-foreground" />
          {t("Player")}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          aria-label={player.collapsed ? t("Expandir player") : t("Contraer player")}
          onClick={() => player.setCollapsed(!player.collapsed)}
        >
          <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", player.collapsed && "-rotate-90")} />
        </Button>
      </div>

      {player.collapsed ? (
        <div className="mt-1 flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground" title={player.player?.path ?? ""}>
            {player.player?.label ?? t("Sin archivo cargado")}
          </span>
          {player.player ? (
            <Button type="button" variant="secondary" size="icon" className="h-6 w-6" onClick={() => void player.togglePlayer()}>
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
          </div>

          <div className="h-1.5 overflow-hidden rounded-full bg-muted">
            <div className="h-full rounded-full bg-primary transition-[width]" style={{ width: `${player.progress}%` }} />
          </div>

          <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
            <span>{formatAudioTime(player.currentTime)}</span>
            <span>{formatAudioTime(player.duration)}</span>
          </div>

          <div className="flex items-center gap-1">
            <Button type="button" size="sm" className="flex-1" disabled={!player.player} onClick={() => void player.togglePlayer()}>
              {player.playing ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
              {player.playing ? t("Pause") : t("Play")}
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

function formatAudioTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${remainder}`;
}
