import { invoke } from "@tauri-apps/api/core";
import { ListMusic, LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { useI18n } from "../../i18n";
import type { TrackListItem } from "./types";

type TrackIdentity = Pick<TrackListItem, "library_id" | "track_id">;

export function TrackPlaylistMemberships({ track }: { track: TrackIdentity }) {
  const { t } = useI18n();
  const [playlistPaths, setPlaylistPaths] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  useEffect(() => {
    const libraryId = track.library_id?.trim();
    if (!libraryId) {
      setPlaylistPaths([]);
      setLoading(false);
      setError(false);
      return;
    }

    let cancelled = false;
    setPlaylistPaths([]);
    setLoading(true);
    setError(false);

    void invoke<string[]>("playlist_index_track_playlists", {
      libraryId,
      trackId: track.track_id
    })
      .then((paths) => {
        if (!cancelled) setPlaylistPaths(paths);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [track.library_id, track.track_id]);

  if (!track.library_id) return null;

  return (
    <section className="mt-4 rounded-md border border-border bg-card">
      <header className="flex items-center justify-between gap-3 border-b border-border px-3 py-2">
        <h3 className="text-sm font-semibold">{t("Playlists")}</h3>
        {!loading && !error ? (
          <span className="rounded-full bg-secondary px-2 py-0.5 text-[11px] font-semibold tabular-nums text-muted-foreground">
            {playlistPaths.length}
          </span>
        ) : null}
      </header>

      <div className="p-3">
        {loading ? (
          <div className="flex items-center gap-2 text-xs text-muted-foreground" role="status">
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            {t("Cargando playlists...")}
          </div>
        ) : null}

        {!loading && error ? (
          <p className="text-xs text-destructive">{t("No se pudieron cargar las playlists.")}</p>
        ) : null}

        {!loading && !error && playlistPaths.length === 0 ? (
          <p className="text-xs text-muted-foreground">{t("No pertenece a ninguna playlist indexada.")}</p>
        ) : null}

        {!loading && !error && playlistPaths.length > 0 ? (
          <ul className="grid gap-2">
            {playlistPaths.map((path) => (
              <li
                key={path}
                className="flex min-w-0 items-start gap-2 rounded-md bg-secondary/60 px-3 py-2 text-xs"
                title={path}
              >
                <ListMusic className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 break-words">{path}</span>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}
