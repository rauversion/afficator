import { FolderOpen, Play, Square } from "lucide-react";
import type * as React from "react";
import { useI18n } from "../../i18n";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { TrackCover } from "./TrackCover";
import type { TrackListColumn, TrackListItem } from "./types";

const defaultColumns: TrackListColumn[] = ["artist", "album", "genre", "bpm", "key"];

export function TrackTable({
  tracks,
  columns = defaultColumns,
  selectedTrackIds,
  empty,
  isPlaying,
  onDetails,
  onOpenFolder,
  onPlay,
  onToggleTrack
}: {
  tracks: TrackListItem[];
  columns?: TrackListColumn[];
  selectedTrackIds?: Set<string>;
  empty?: React.ReactNode;
  isPlaying?: (track: TrackListItem) => boolean;
  onDetails?: (track: TrackListItem) => void;
  onOpenFolder?: (track: TrackListItem) => void;
  onPlay?: (track: TrackListItem) => void;
  onToggleTrack?: (track: TrackListItem) => void;
}) {
  const { t } = useI18n();
  const template = trackGridTemplate(columns, Boolean(onToggleTrack), Boolean(onOpenFolder));

  if (tracks.length === 0) {
    return <>{empty}</>;
  }

  return (
    <div className="overflow-x-auto">
      <div className="min-w-[860px]">
        <div
          className="grid gap-2 border-b border-border px-2 py-2 text-xs font-semibold text-muted-foreground"
          style={{ gridTemplateColumns: template }}
        >
          {onToggleTrack ? <span /> : null}
          <span />
          <span />
          <span>{t("Tema")}</span>
          {columns.map((column) => (
            <span key={column}>{trackColumnLabel(t, column)}</span>
          ))}
          {onOpenFolder ? <span className="text-right">{t("Acciones")}</span> : null}
        </div>
        {tracks.map((track) => (
          <TrackListRow
            key={track.track_id}
            track={track}
            columns={columns}
            gridTemplate={template}
            selected={selectedTrackIds?.has(track.track_id) ?? false}
            playing={isPlaying?.(track) ?? false}
            onDetails={onDetails ? () => onDetails(track) : undefined}
            onOpenFolder={onOpenFolder ? () => onOpenFolder(track) : undefined}
            onPlay={onPlay ? () => onPlay(track) : undefined}
            onToggle={onToggleTrack ? () => onToggleTrack(track) : undefined}
          />
        ))}
      </div>
    </div>
  );
}

export function TrackListRow({
  track,
  columns,
  gridTemplate,
  selected,
  playing,
  onDetails,
  onOpenFolder,
  onPlay,
  onToggle
}: {
  track: TrackListItem;
  columns: TrackListColumn[];
  gridTemplate: string;
  selected: boolean;
  playing: boolean;
  onDetails?: () => void;
  onOpenFolder?: () => void;
  onPlay?: () => void;
  onToggle?: () => void;
}) {
  return (
    <div
      className={cn(
        "grid min-h-14 items-center gap-2 border-b border-border px-2 text-xs",
        !track.source_exists && "bg-red-50 dark:bg-red-950/30"
      )}
      style={{ gridTemplateColumns: gridTemplate }}
    >
      {onToggle ? <input type="checkbox" checked={selected} onChange={onToggle} /> : null}
      <Button
        variant={playing ? "default" : "secondary"}
        size="icon"
        disabled={!track.source_exists || !track.source_path || !onPlay}
        onClick={onPlay}
      >
        {playing ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </Button>
      <TrackCover sourcePath={track.source_path} title={track.name ?? track.track_id} className="h-10 w-10" />
      <div className="min-w-0">
        {onDetails ? (
          <button
            type="button"
            className="block max-w-full truncate text-left font-semibold underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            title={track.name ?? track.track_id}
            onClick={onDetails}
          >
            {track.name ?? track.track_id}
          </button>
        ) : (
          <span className="block truncate font-semibold" title={track.name ?? track.track_id}>
            {track.name ?? track.track_id}
          </span>
        )}
        <span className="block truncate text-[11px] text-muted-foreground" title={trackMetadataSummary(track)}>
          {trackMetadataSummary(track)}
        </span>
      </div>
      {columns.map((column) => (
        <span key={column} className="truncate" title={trackColumnValue(track, column)}>
          {trackColumnValue(track, column)}
        </span>
      ))}
      {onOpenFolder ? (
        <div className="flex justify-end">
          <Button variant="secondary" size="icon" disabled={!track.source_path} onClick={onOpenFolder}>
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function trackGridTemplate(columns: TrackListColumn[], selectable: boolean, actions: boolean) {
  return [
    selectable ? "24px" : null,
    "36px",
    "44px",
    "minmax(220px,1.35fr)",
    ...columns.map((column) => trackColumnWidth(column)),
    actions ? "48px" : null
  ]
    .filter(Boolean)
    .join(" ");
}

function trackColumnWidth(column: TrackListColumn) {
  switch (column) {
    case "artist":
    case "album":
      return "minmax(150px,0.85fr)";
    case "genre":
      return "minmax(120px,0.7fr)";
    case "bpm":
    case "key":
    case "year":
      return "78px";
    case "kind":
      return "minmax(120px,0.7fr)";
  }
}

function trackColumnLabel(t: (key: string) => string, column: TrackListColumn) {
  switch (column) {
    case "artist":
      return t("Artista");
    case "album":
      return "Album";
    case "genre":
      return t("Genero");
    case "bpm":
      return "BPM";
    case "key":
      return "Key";
    case "year":
      return t("Ano");
    case "kind":
      return t("Formato");
  }
}

function trackColumnValue(track: TrackListItem, column: TrackListColumn) {
  switch (column) {
    case "artist":
      return track.artist ?? "";
    case "album":
      return track.album ?? "";
    case "genre":
      return track.genre ?? "";
    case "bpm":
      return track.bpm ?? "";
    case "key":
      return track.key ?? "";
    case "year":
      return track.year ?? "";
    case "kind":
      return track.kind ?? "";
  }
}

function trackMetadataSummary(track: TrackListItem) {
  return [
    track.genre,
    track.bpm ? `${track.bpm} BPM` : null,
    track.key,
    track.year,
    track.kind
  ]
    .map((value) => value?.trim())
    .filter(Boolean)
    .join(" · ");
}
