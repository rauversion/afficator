import { FolderOpen, Play } from "lucide-react";
import { useEffect } from "react";
import type * as React from "react";
import { useI18n } from "../../i18n";
import { Button } from "../ui/button";
import { TrackCover } from "./TrackCover";
import { TrackPlaylistMemberships } from "./TrackPlaylistMemberships";
import type { TrackListItem } from "./types";

export function TrackDetailSheet({
  track,
  onClose,
  onOpenFolder,
  onPlay
}: {
  track: TrackListItem | null;
  onClose: () => void;
  onOpenFolder: (track: TrackListItem) => void;
  onPlay: (track: TrackListItem) => void;
}) {
  const { t } = useI18n();

  useEffect(() => {
    if (!track) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, track]);

  if (!track) return null;

  const rows: Array<[string, React.ReactNode]> = [
    ["Track ID", track.track_id],
    [t("Titulo"), track.name],
    [t("Artista"), track.artist],
    ["Album", track.album],
    [t("Genero"), track.genre],
    ["BPM", track.bpm],
    ["Key", track.key],
    [t("Ano"), track.year],
    ["Label", track.label],
    ["Rating", track.rating],
    [t("Comentarios"), track.comments],
    [t("Fecha XML"), track.date_added],
    [t("Formato"), track.kind],
    [t("Duracion"), track.total_time ? formatTime(track.total_time) : null],
    [t("Original"), track.source_path]
  ];
  const xmlAttributes = Object.entries(track.attributes ?? {}).filter(([, value]) => String(value).trim() !== "");

  return (
    <div className="fixed inset-0 z-[65]">
      <div className="absolute inset-0 bg-black/25 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="absolute right-0 top-0 z-[70] flex h-full w-[500px] max-w-[calc(100vw-16px)] flex-col border-l border-border bg-background shadow-2xl">
        <header className="border-b border-border bg-card px-4 py-4">
          <div className="flex items-start gap-3">
            <TrackCover sourcePath={track.source_path} title={track.name ?? track.track_id} className="h-24 w-24" />
            <div className="min-w-0 flex-1">
              <h2 className="truncate text-base font-semibold">{track.name ?? track.track_id}</h2>
              <p className="mt-1 truncate text-sm text-muted-foreground">{track.artist ?? t("Sin artista")}</p>
              <p className="mt-1 truncate text-xs text-muted-foreground">{track.album ?? t("Sin album")}</p>
            </div>
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("Cerrar")}
            </Button>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <section className="grid gap-2">
            {rows.map(([label, value]) => (
              <DetailRow key={label} label={label} value={value} />
            ))}
          </section>

          <TrackPlaylistMemberships track={track} />

          {xmlAttributes.length > 0 ? (
            <section className="mt-4 rounded-md border border-border bg-card">
              <h3 className="border-b border-border px-3 py-2 text-sm font-semibold">{t("Atributos XML")}</h3>
              <div className="grid gap-2 p-3">
                {xmlAttributes.map(([label, value]) => (
                  <DetailRow key={label} label={label} value={value} />
                ))}
              </div>
            </section>
          ) : null}

          <section className="mt-4 flex flex-wrap gap-2">
            <Button disabled={!track.source_exists || !track.source_path} onClick={() => onPlay(track)}>
              <Play className="h-4 w-4" />
              {t("Play")}
            </Button>
            <Button variant="secondary" disabled={!track.source_path} onClick={() => onOpenFolder(track)}>
              <FolderOpen className="h-4 w-4" />
              {t("Carpeta")}
            </Button>
          </section>
        </div>
      </aside>
    </div>
  );
}

function DetailRow({ label, value }: { label: string; value: React.ReactNode }) {
  if (value === undefined || value === null || value === "") return null;

  return (
    <div className="grid grid-cols-[112px_minmax(0,1fr)] gap-3 rounded-md bg-secondary/60 px-3 py-2 text-xs">
      <span className="truncate font-semibold text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words">{value}</span>
    </div>
  );
}

function formatTime(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const rest = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${rest}`;
}
