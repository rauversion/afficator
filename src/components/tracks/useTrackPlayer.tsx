import { convertFileSrc } from "@tauri-apps/api/core";
import { useCallback, useRef, useState } from "react";
import { playbackErrorMessage } from "../../playback";
import type { TrackListItem } from "./types";

type PlayerState = {
  label: string;
  path: string;
  url: string;
};

export function useTrackPlayer({
  onError,
  t
}: {
  onError: (message: string) => void;
  t: (key: string, values?: Record<string, string | number | null | undefined>) => string;
}) {
  const audioElement = useRef<HTMLAudioElement | null>(null);
  const [player, setPlayer] = useState<PlayerState | null>(null);
  const [playing, setPlaying] = useState(false);

  const stop = useCallback(() => {
    audioElement.current?.pause();
    if (audioElement.current) {
      audioElement.current.currentTime = 0;
    }
    setPlaying(false);
  }, []);

  const togglePathPlayback = useCallback(
    async (path?: string | null, label?: string | null) => {
      if (!path) return;
      const nextLabel = label || path;

      if (player?.path === path && playing) {
        audioElement.current?.pause();
        setPlaying(false);
        return;
      }

      setPlayer({ path, label: nextLabel, url: convertFileSrc(path) });
      window.setTimeout(() => {
        void audioElement.current?.play().catch((error) => {
          onError(playbackErrorMessage(t, nextLabel, path, error));
        });
      }, 30);
    },
    [onError, player?.path, playing, t]
  );

  const toggleTrackPlayback = useCallback(
    (track: TrackListItem) => togglePathPlayback(track.source_path, track.name ?? track.source_path),
    [togglePathPlayback]
  );

  const isPlaying = useCallback(
    (trackOrPath?: TrackListItem | string | null) => {
      const path = typeof trackOrPath === "string" ? trackOrPath : trackOrPath?.source_path;
      return Boolean(path && player?.path === path && playing);
    },
    [player?.path, playing]
  );

  const audio = player ? (
    <audio
      className="hidden"
      ref={audioElement}
      src={player.url}
      onPlay={() => setPlaying(true)}
      onPause={() => setPlaying(false)}
      onEnded={() => setPlaying(false)}
      onError={() => onError(playbackErrorMessage(t, player.label, player.path))}
    />
  ) : null;

  return {
    audio,
    isPlaying,
    player,
    playing,
    stop,
    togglePathPlayback,
    toggleTrackPlayback
  };
}
