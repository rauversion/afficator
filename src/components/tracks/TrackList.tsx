import { FolderOpen, Play, Square } from "lucide-react";
import type * as React from "react";
import { useI18n } from "../../i18n";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { TrackCover } from "./TrackCover";
import { BroadcastAddButton } from "./BroadcastAddButton";
import type { TrackListColumn, TrackListItem, TrackPlaybackContext } from "./types";

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
  playbackContext,
  onToggleTrack,
  renderActions,
  renderTitleAccessory,
  showPosition = false
}: {
  tracks: TrackListItem[];
  columns?: TrackListColumn[];
  selectedTrackIds?: Set<string>;
  empty?: React.ReactNode;
  isPlaying?: (track: TrackListItem) => boolean;
  onDetails?: (track: TrackListItem) => void;
  onOpenFolder?: (track: TrackListItem) => void;
  onPlay?: (track: TrackListItem, context: TrackPlaybackContext) => void;
  playbackContext?: {
    id: string;
    label?: string | null;
  };
  onToggleTrack?: (track: TrackListItem) => void;
  renderActions?: (track: TrackListItem, index: number) => React.ReactNode;
  renderTitleAccessory?: (track: TrackListItem) => React.ReactNode;
  showPosition?: boolean;
}) {
  const { t } = useI18n();
  const hasActions = true;
  const template = trackGridTemplate(columns, Boolean(onToggleTrack), hasActions, showPosition);
  const trackPlaybackContext: TrackPlaybackContext = {
    id: playbackContext?.id ?? "track-table",
    label: playbackContext?.label,
    tracks
  };

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
          {showPosition ? <span>#</span> : null}
          <span />
          <span />
          <span>{t("Tema")}</span>
          {columns.map((column) => (
            <span key={column}>{trackColumnLabel(t, column)}</span>
          ))}
          {hasActions ? (
            <span className="sticky right-0 z-20 flex h-full items-center justify-end border-l border-border bg-secondary px-2">
              {t("Acciones")}
            </span>
          ) : null}
        </div>
        {tracks.map((track, index) => (
          <TrackListRow
            key={`${track.library_id ?? "library"}-${track.track_id}-${index}`}
            track={track}
            columns={columns}
            gridTemplate={template}
            position={showPosition ? index + 1 : undefined}
            selected={selectedTrackIds?.has(track.track_id) ?? false}
            playing={isPlaying?.(track) ?? false}
            actions={renderActions?.(track, index)}
            titleAccessory={renderTitleAccessory?.(track)}
            onDetails={onDetails ? () => onDetails(track) : undefined}
            onOpenFolder={onOpenFolder ? () => onOpenFolder(track) : undefined}
            onPlay={onPlay ? () => onPlay(track, trackPlaybackContext) : undefined}
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
  position,
  selected,
  playing,
  actions,
  titleAccessory,
  onDetails,
  onOpenFolder,
  onPlay,
  onToggle
}: {
  track: TrackListItem;
  columns: TrackListColumn[];
  gridTemplate: string;
  position?: number;
  selected: boolean;
  playing: boolean;
  actions?: React.ReactNode;
  titleAccessory?: React.ReactNode;
  onDetails?: () => void;
  onOpenFolder?: () => void;
  onPlay?: () => void;
  onToggle?: () => void;
}) {
  return (
    <div
      className={cn(
        "grid min-h-14 items-center gap-2 border-b border-border bg-background px-2 text-xs",
        !track.source_exists && "bg-red-50 dark:bg-red-950/30"
      )}
      style={{ gridTemplateColumns: gridTemplate }}
    >
      {onToggle ? <input type="checkbox" checked={selected} onChange={onToggle} /> : null}
      {position ? <span className="text-right tabular-nums text-muted-foreground">{position}</span> : null}
      <Button
        variant={playing ? "default" : "secondary"}
        size="icon"
        disabled={!track.source_exists || !track.source_path || !onPlay}
        onClick={onPlay}
      >
        {playing ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </Button>
      <TrackCover sourcePath={track.source_path} title={track.name ?? track.track_id} className="h-10 w-10" />
      <div className="flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1">
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
        {titleAccessory}
      </div>
      {columns.map((column) => (
        <span key={column} className="truncate" title={trackColumnValue(track, column)}>
          {trackColumnValue(track, column)}
        </span>
      ))}
      {onOpenFolder || actions ? (
        <div className="sticky right-0 z-10 flex h-full items-center justify-end gap-1 border-l border-border bg-inherit px-2">
          <BroadcastAddButton track={track} />
          {actions ?? (onOpenFolder ? (
            <Button variant="secondary" size="icon" disabled={!track.source_path} onClick={onOpenFolder}>
              <FolderOpen className="h-3.5 w-3.5" />
            </Button>
          ) : null)}
        </div>
      ) : (
        <div className="sticky right-0 z-10 flex h-full items-center justify-end border-l border-border bg-inherit px-2">
          <BroadcastAddButton track={track} />
        </div>
      )}
    </div>
  );
}

function trackGridTemplate(columns: TrackListColumn[], selectable: boolean, actions: boolean, showPosition: boolean) {
  return [
    selectable ? "24px" : null,
    showPosition ? "32px" : null,
    "36px",
    "44px",
    "minmax(220px,1.35fr)",
    ...columns.map((column) => trackColumnWidth(column)),
    actions ? "minmax(48px,max-content)" : null
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
    case "rating":
      return "96px";
    case "label":
      return "minmax(130px,0.75fr)";
    case "comments":
      return "minmax(180px,1fr)";
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
    case "rating":
      return "Rating";
    case "year":
      return t("Ano");
    case "label":
      return "Label";
    case "comments":
      return t("Comentarios");
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
    case "rating": {
      const rating = trackRating(track);
      return rating > 0 ? `${"★".repeat(rating)}${"☆".repeat(5 - rating)}` : "—";
    }
    case "year":
      return track.year ?? "";
    case "label":
      return track.label ?? "";
    case "comments":
      return track.comments ?? "";
    case "kind":
      return track.kind ?? "";
  }
}

function trackRating(track: TrackListItem) {
  if (track.user_rating !== undefined && track.user_rating !== null) {
    return Math.max(0, Math.min(5, Math.round(track.user_rating)));
  }
  const raw = Number.parseFloat(track.rating ?? "");
  if (!Number.isFinite(raw)) return 0;
  return Math.max(0, Math.min(5, Math.round(raw <= 5 ? raw : raw / 51)));
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
