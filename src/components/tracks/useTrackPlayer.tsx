import { useCallback } from "react";
import { useGlobalAudioPlayer } from "../audio/GlobalAudioPlayer";
import type { TrackListItem } from "./types";

export function useTrackPlayer({
  onError
}: {
  onError: (message: string) => void;
  t: (key: string, values?: Record<string, string | number | null | undefined>) => string;
}) {
  const globalPlayer = useGlobalAudioPlayer();

  const stop = useCallback(() => {
    globalPlayer.stop();
  }, [globalPlayer]);

  const togglePathPlayback = useCallback(
    async (path?: string | null, label?: string | null) => {
      await globalPlayer.togglePathPlayback(path, label, onError);
    },
    [globalPlayer, onError]
  );

  const toggleTrackPlayback = useCallback(
    (track: TrackListItem) => togglePathPlayback(track.source_path, track.name ?? track.source_path),
    [togglePathPlayback]
  );

  const isPlaying = useCallback(
    (trackOrPath?: TrackListItem | string | null) => {
      return globalPlayer.isPlaying(trackOrPath);
    },
    [globalPlayer]
  );

  return {
    audio: null,
    isPlaying,
    player: globalPlayer.player,
    playing: globalPlayer.playing,
    stop,
    togglePathPlayback,
    toggleTrackPlayback
  };
}
