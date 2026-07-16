import { invoke } from "@tauri-apps/api/core";
import {
  Album,
  FolderOpen,
  ListMusic,
  Play,
  RefreshCcw,
  Search,
  Square,
  UserRound
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type * as React from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { useGlobalAudioPlayer } from "./components/audio/GlobalAudioPlayer";
import { PlaylistAddDialog } from "./components/tracks/PlaylistAddDialog";
import { TrackCover } from "./components/tracks/TrackCover";
import { translateBackendMessage, useI18n } from "./i18n";
import { cn } from "./lib/utils";

type BrowserKind = "artist" | "album";
type ArtistDetailTab = "tracks" | "albums";

type PlaylistIndexLibrary = {
  id: string;
  source_path: string;
  source_name: string;
  track_count: number;
  playlist_count: number;
};

type PlaylistIndexGroup = {
  library_id: string;
  kind: BrowserKind;
  value: string;
  name: string;
  track_count: number;
};

type PlaylistIndexTrack = {
  library_id: string;
  track_id: string;
  name?: string | null;
  artist?: string | null;
  album?: string | null;
  kind?: string | null;
  source_path?: string | null;
  total_time?: number | null;
  genre?: string | null;
  comments?: string | null;
  bpm?: string | null;
  key?: string | null;
  rating?: string | null;
  user_rating?: number | null;
  year?: string | null;
  label?: string | null;
  date_added?: string | null;
  attributes?: Record<string, string>;
  source_exists: boolean;
};

type PlaylistDraft = {
  id: string;
  library_id: string;
  name: string;
  description?: string | null;
  track_count: number;
};

export function PlaylistBrowserPage({ kind }: { kind: BrowserKind }) {
  const { locale, t } = useI18n();
  const audioPlayer = useGlobalAudioPlayer();
  const [libraries, setLibraries] = useState<PlaylistIndexLibrary[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState("");
  const [drafts, setDrafts] = useState<PlaylistDraft[]>([]);
  const [groups, setGroups] = useState<PlaylistIndexGroup[]>([]);
  const [activeGroupValue, setActiveGroupValue] = useState("");
  const [tracks, setTracks] = useState<PlaylistIndexTrack[]>([]);
  const [selectedTrackIds, setSelectedTrackIds] = useState<Set<string>>(new Set());
  const [artistDetailTab, setArtistDetailTab] = useState<ArtistDetailTab>("tracks");
  const [activeArtistAlbumValue, setActiveArtistAlbumValue] = useState("");
  const [groupQuery, setGroupQuery] = useState("");
  const [trackQuery, setTrackQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const [addPlaylistDialogOpen, setAddPlaylistDialogOpen] = useState(false);
  const [detailTrack, setDetailTrack] = useState<PlaylistIndexTrack | null>(null);

  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const activeGroup = groups.find((group) => group.value === activeGroupValue) ?? null;
  const artistAlbums = useMemo(() => artistAlbumsFromTracks(tracks), [tracks]);
  const displayedTracks = useMemo(() => {
    if (kind !== "artist" || artistDetailTab !== "albums" || !activeArtistAlbumValue) return tracks;
    return tracks.filter((track) => albumValue(track.album) === activeArtistAlbumValue);
  }, [activeArtistAlbumValue, artistDetailTab, kind, tracks]);
  const selectedTracks = useMemo(
    () => displayedTracks.filter((track) => selectedTrackIds.has(track.track_id)),
    [displayedTracks, selectedTrackIds]
  );
  const allVisibleSelected = displayedTracks.length > 0 && selectedTracks.length === displayedTracks.length;
  const title = kind === "artist" ? t("Artistas") : t("Albums");
  const groupLabel = kind === "artist" ? t("Artista") : "Album";
  const activeAlbum = artistAlbums.find((album) => album.value === activeArtistAlbumValue);
  const playlistDialogDefaultName =
    kind === "artist" && artistDetailTab === "albums" && activeAlbum
      ? `${activeGroup?.name ?? ""} - ${activeAlbum.name}`.trim()
      : activeGroup?.name ?? "";
  const playbackContext = useMemo(
    () => ({
      id: `browser:${kind}:${activeLibraryId}:${activeGroupValue || "none"}:${artistDetailTab}:${activeArtistAlbumValue || "all"}`,
      label: playlistDialogDefaultName || title
    }),
    [activeArtistAlbumValue, activeGroupValue, activeLibraryId, artistDetailTab, kind, playlistDialogDefaultName, title]
  );

  useEffect(() => {
    setGroups([]);
    setTracks([]);
    setSelectedTrackIds(new Set());
    setActiveGroupValue("");
    setArtistDetailTab("tracks");
    setActiveArtistAlbumValue("");
    void loadLibraries();
  }, [kind]);

  useEffect(() => {
    if (kind !== "artist" || artistDetailTab !== "albums") return;

    setActiveArtistAlbumValue((current) => {
      if (artistAlbums.some((album) => album.value === current)) return current;
      return artistAlbums[0]?.value ?? "";
    });
    setSelectedTrackIds(new Set());
  }, [artistAlbums, artistDetailTab, kind]);

  async function loadLibraries(selectLibraryId = activeLibraryId) {
    setBusy(true);
    setErrorMessage("");

    try {
      const response = await invoke<PlaylistIndexLibrary[]>("playlist_index_libraries");
      setLibraries(response);
      const nextLibraryId = response.some((library) => library.id === selectLibraryId)
        ? selectLibraryId
        : response[0]?.id ?? "";
      setActiveLibraryId(nextLibraryId);
      if (nextLibraryId) {
        await Promise.all([
          loadDrafts(nextLibraryId),
          loadGroups(nextLibraryId, groupQuery, "")
        ]);
      } else {
        setDrafts([]);
        setGroups([]);
        setTracks([]);
      }
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function changeLibrary(libraryId: string) {
    setActiveLibraryId(libraryId);
    setActiveGroupValue("");
    setSelectedTrackIds(new Set());
    setArtistDetailTab("tracks");
    setActiveArtistAlbumValue("");
    setTracks([]);
    setBusy(true);
    setErrorMessage("");

    try {
      await Promise.all([
        loadDrafts(libraryId),
        loadGroups(libraryId, groupQuery, "")
      ]);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function loadDrafts(libraryId = activeLibraryId) {
    if (!libraryId) return;
    const response = await invoke<PlaylistDraft[]>("playlist_index_drafts", { libraryId });
    setDrafts(response);
  }

  async function loadGroups(libraryId = activeLibraryId, query = groupQuery, preferredGroupValue = activeGroupValue) {
    if (!libraryId) return;
    const response = await invoke<PlaylistIndexGroup[]>("playlist_index_track_groups", {
      libraryId,
      kind,
      query,
      limit: 500
    });
    setGroups(response);

    const nextGroup = response.find((group) => group.value === preferredGroupValue) ?? response[0] ?? null;
    setActiveGroupValue(nextGroup?.value ?? "");
    setSelectedTrackIds(new Set());
    setArtistDetailTab("tracks");
    setActiveArtistAlbumValue("");
    if (nextGroup) {
      await loadGroupTracks(libraryId, nextGroup.value, trackQuery);
    } else {
      setTracks([]);
    }
  }

  async function loadGroupTracks(libraryId = activeLibraryId, groupValue = activeGroupValue, query = trackQuery, resetAlbum = true) {
    if (!libraryId && !activeLibraryId) return;
    const response = await invoke<PlaylistIndexTrack[]>("playlist_index_group_tracks", {
      libraryId: libraryId || activeLibraryId,
      kind,
      value: groupValue,
      query,
      limit: 1500
    });
    setTracks(response);
    setSelectedTrackIds(new Set());
    if (resetAlbum) setActiveArtistAlbumValue("");
  }

  async function submitGroupSearch(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeLibraryId) return;
    setBusy(true);
    setErrorMessage("");

    try {
      await loadGroups(activeLibraryId, groupQuery, "");
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function submitTrackSearch(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeLibraryId || !activeGroup) return;
    setBusy(true);
    setErrorMessage("");

    try {
      await loadGroupTracks(activeLibraryId, activeGroup.value, trackQuery, false);
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  async function selectGroup(group: PlaylistIndexGroup) {
    setActiveGroupValue(group.value);
    setTrackQuery("");
    setArtistDetailTab("tracks");
    setActiveArtistAlbumValue("");
    setBusy(true);
    setErrorMessage("");

    try {
      await loadGroupTracks(activeLibraryId, group.value, "");
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    } finally {
      setBusy(false);
    }
  }

  function toggleTrack(trackId: string) {
    setSelectedTrackIds((current) => {
      const next = new Set(current);
      if (next.has(trackId)) {
        next.delete(trackId);
      } else {
        next.add(trackId);
      }
      return next;
    });
  }

  function toggleAllTracks() {
    setSelectedTrackIds(() => {
      if (allVisibleSelected) return new Set();
      return new Set(displayedTracks.map((track) => track.track_id));
    });
  }

  async function addSelectedToExistingDraft(draftId: string) {
    if (!draftId || selectedTracks.length === 0) return;
    const added = await addTrackIdsToDraft(draftId, selectedTracks.map((track) => track.track_id));
    if (added) setAddPlaylistDialogOpen(false);
  }

  async function addTrackIdsToDraft(draftId: string, trackIds: string[]) {
    setBusy(true);
    setErrorMessage("");

    try {
      const updatedTracks = await invoke<PlaylistIndexTrack[]>("playlist_index_add_tracks_to_draft", {
        draftId,
        trackIds
      });
      await loadDrafts(activeLibraryId);
      setSelectedTrackIds(new Set());
      setMessage(t("{count} tracks en la playlist.", { count: updatedTracks.length }));
      return true;
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
      return false;
    } finally {
      setBusy(false);
    }
  }

  function openAddPlaylistDialog() {
    setAddPlaylistDialogOpen(true);
  }

  async function createDraftWithSelectedTracks(name: string, description: string) {
    if (!activeLibraryId || selectedTracks.length === 0 || !name.trim()) return;
    setBusy(true);
    setErrorMessage("");

    try {
      const draft = await invoke<PlaylistDraft>("playlist_index_create_draft", {
        libraryId: activeLibraryId,
        name,
        description: description || null
      });
      const selectedIds = selectedTracks.map((track) => track.track_id);
      const added = await addTrackIdsToDraft(draft.id, selectedIds);
      if (!added) return;
      setAddPlaylistDialogOpen(false);
      setMessage(t("Playlist creada: {name} con {count} tracks.", {
        name: draft.name,
        count: selectedIds.length
      }));
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
      setBusy(false);
    }
  }

  async function toggleDisplayedTrackPlayback(track: PlaylistIndexTrack) {
    await audioPlayer.toggleTrackListPlayback(displayedTracks, track, playbackContext, setErrorMessage);
  }

  async function openFolder(path?: string | null) {
    if (!path) return;
    try {
      await invoke("open_parent_folder", { path });
    } catch (error) {
      setErrorMessage(translateBackendMessage(locale, String(error)));
    }
  }

  return (
    <main className="flex min-h-screen flex-col bg-background p-4 text-foreground lg:h-screen lg:min-h-0 lg:overflow-hidden">
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-muted-foreground">
            {kind === "artist" ? <UserRound className="h-5 w-5" /> : <Album className="h-5 w-5" />}
            <span className="text-sm font-semibold">{t("Playlist Browser")}</span>
          </div>
          <h1 className="m-0 truncate text-2xl font-semibold tracking-normal">{title}</h1>
          <p className="mt-1 max-w-[72vw] truncate text-xs text-muted-foreground">
            {activeLibrary?.source_path ?? t("Sin XML indexado")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            className="h-10 min-w-[240px] rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            value={activeLibraryId}
            onChange={(event) => void changeLibrary(event.currentTarget.value)}
          >
            {libraries.length === 0 ? <option value="">{t("Sin libreria activa")}</option> : null}
            {libraries.map((library) => (
              <option key={library.id} value={library.id}>
                {library.source_name}
              </option>
            ))}
          </select>
          <Button variant="secondary" disabled={busy} onClick={() => void loadLibraries(activeLibraryId)}>
            <RefreshCcw className="h-4 w-4" />
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

      <section
        data-playlist-browser-layout
        className="grid min-h-0 flex-1 grid-cols-[320px_minmax(0,1fr)] gap-3 max-lg:h-[640px] max-lg:min-h-[640px] max-lg:flex-none max-lg:grid-cols-1"
      >
        <Card className="flex h-full min-h-0 flex-col overflow-hidden">
          <CardHeader>
            <div className="min-w-0">
              <CardTitle>{title}</CardTitle>
              <span className="block truncate text-xs text-muted-foreground">
                {groups.length} {t("grupos")}
              </span>
            </div>
          </CardHeader>
          <form className="border-b border-border p-3" onSubmit={submitGroupSearch}>
            <div className="grid grid-cols-[minmax(0,1fr)_40px] gap-2">
              <input
                className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                value={groupQuery}
                placeholder={kind === "artist" ? t("Buscar artista") : t("Buscar album")}
                onChange={(event) => setGroupQuery(event.currentTarget.value)}
              />
              <Button type="submit" size="icon" disabled={busy || !activeLibraryId}>
                <Search className="h-4 w-4" />
              </Button>
            </div>
          </form>
          <CardContent className="min-h-0 flex-1 overflow-y-auto p-0">
            {!activeLibraryId ? <EmptyRow>{t("Indexa un XML para empezar.")}</EmptyRow> : null}
            {activeLibraryId && groups.length === 0 ? <EmptyRow>{t("Sin resultados.")}</EmptyRow> : null}
            {groups.map((group) => (
              <button
                key={group.value || "__empty__"}
                type="button"
                className={cn(
                  "grid w-full grid-cols-[28px_minmax(0,1fr)_56px] items-center gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-secondary",
                  group.value === activeGroupValue && "bg-muted"
                )}
                onClick={() => void selectGroup(group)}
              >
                {kind === "artist" ? <UserRound className="h-4 w-4 text-muted-foreground" /> : <Album className="h-4 w-4 text-muted-foreground" />}
                <span className="truncate font-semibold" title={group.name}>{translateMissingGroupName(t, group.name)}</span>
                <span className="text-right tabular-nums text-muted-foreground">{group.track_count}</span>
              </button>
            ))}
          </CardContent>
        </Card>

        <Card className="flex h-full min-h-0 flex-col overflow-hidden">
          <CardHeader>
            <div className="min-w-0">
              <CardTitle>{activeGroup ? translateMissingGroupName(t, activeGroup.name) : t("Tracks")}</CardTitle>
              <span className="block truncate text-xs text-muted-foreground">
                {displayedTracks.length} {t("tracks")} · {selectedTracks.length} {t("seleccionados")}
                {kind === "artist" && activeGroup ? ` · ${artistAlbums.length} ${t("albums")}` : ""}
              </span>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              <Button variant="secondary" size="sm" disabled={displayedTracks.length === 0} onClick={toggleAllTracks}>
                {allVisibleSelected ? t("Deseleccionar") : t("Todos")}
              </Button>
              <Button size="sm" disabled={selectedTracks.length === 0 || busy} onClick={openAddPlaylistDialog}>
                <ListMusic className="h-3.5 w-3.5" />
                {t("Agregar a playlist")}
              </Button>
            </div>
          </CardHeader>
          {kind === "artist" && activeGroup ? (
            <div className="flex items-center gap-1 border-b border-border bg-card p-1">
              <BrowserSubTabButton
                active={artistDetailTab === "tracks"}
                onClick={() => {
                  setArtistDetailTab("tracks");
                  setSelectedTrackIds(new Set());
                }}
              >
                {t("Tracks")}
              </BrowserSubTabButton>
              <BrowserSubTabButton
                active={artistDetailTab === "albums"}
                onClick={() => {
                  setArtistDetailTab("albums");
                  setSelectedTrackIds(new Set());
                }}
              >
                {t("Albums")}
              </BrowserSubTabButton>
            </div>
          ) : null}
          <form className="border-b border-border p-3" onSubmit={submitTrackSearch}>
            <div className="grid grid-cols-[minmax(0,1fr)_40px] gap-2">
              <input
                className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                value={trackQuery}
                placeholder={kind === "artist" ? t("Buscar dentro del artista") : t("Buscar dentro del grupo")}
                onChange={(event) => setTrackQuery(event.currentTarget.value)}
              />
              <Button type="submit" size="icon" disabled={busy || !activeGroup}>
                <Search className="h-4 w-4" />
              </Button>
            </div>
          </form>
          <CardContent className="min-h-0 flex-1 overflow-y-auto p-0">
            {!activeGroup ? <EmptyRow>{t("Selecciona un grupo")}</EmptyRow> : null}
            {activeGroup && displayedTracks.length === 0 ? <EmptyRow>{t("Sin tracks.")}</EmptyRow> : null}
            {activeGroup && kind === "artist" && artistDetailTab === "albums" ? (
              <section className="grid h-full min-h-0 grid-cols-[240px_minmax(0,1fr)] max-lg:min-h-[420px] max-lg:grid-cols-1">
                <div className="min-h-0 overflow-y-auto border-r border-border max-lg:max-h-56 max-lg:border-b max-lg:border-r-0">
                  {artistAlbums.length === 0 ? <EmptyRow>{t("Sin albums.")}</EmptyRow> : null}
                  {artistAlbums.map((album) => (
                    <button
                      key={album.value || "__empty_album__"}
                      type="button"
                      className={cn(
                        "grid w-full grid-cols-[28px_minmax(0,1fr)_48px] items-center gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-secondary",
                        album.value === activeArtistAlbumValue && "bg-muted"
                      )}
                      onClick={() => {
                        setActiveArtistAlbumValue(album.value);
                        setSelectedTrackIds(new Set());
                      }}
                    >
                      <Album className="h-4 w-4 text-muted-foreground" />
                      <span className="truncate font-semibold" title={album.name}>{translateMissingGroupName(t, album.name)}</span>
                      <span className="text-right tabular-nums text-muted-foreground">{album.tracks.length}</span>
                    </button>
                  ))}
                </div>
                <div className="min-h-0 overflow-y-auto">
                  {displayedTracks.map((track) => (
                    <BrowseTrackRow
                      key={track.track_id}
                      track={track}
                      selected={selectedTrackIds.has(track.track_id)}
                      playing={audioPlayer.isPlaying(track.source_path)}
                      onToggle={() => toggleTrack(track.track_id)}
                      onDetails={() => setDetailTrack(track)}
                      onPlay={() => void toggleDisplayedTrackPlayback(track)}
                      onOpenFolder={() => void openFolder(track.source_path)}
                    />
                  ))}
                </div>
              </section>
            ) : (
              displayedTracks.map((track) => (
                <BrowseTrackRow
                  key={track.track_id}
                  track={track}
                  selected={selectedTrackIds.has(track.track_id)}
                  playing={audioPlayer.isPlaying(track.source_path)}
                  onToggle={() => toggleTrack(track.track_id)}
                  onDetails={() => setDetailTrack(track)}
                  onPlay={() => void toggleDisplayedTrackPlayback(track)}
                  onOpenFolder={() => void openFolder(track.source_path)}
                />
              ))
            )}
          </CardContent>
        </Card>
      </section>

      <BrowseTrackDetailSheet
        track={detailTrack}
        onClose={() => setDetailTrack(null)}
        onPlay={(track) => void toggleDisplayedTrackPlayback(track)}
        onOpenFolder={(track) => void openFolder(track.source_path)}
      />

      <PlaylistAddDialog
        open={addPlaylistDialogOpen}
        busy={busy}
        contextLabel={activeGroup ? translateMissingGroupName(t, activeGroup.name) : t("Seleccion actual")}
        defaultName={playlistDialogDefaultName}
        drafts={drafts}
        trackCount={selectedTracks.length}
        onClose={() => setAddPlaylistDialogOpen(false)}
        onAddExisting={(draftId) => void addSelectedToExistingDraft(draftId)}
        onCreate={(name, description) => void createDraftWithSelectedTracks(name, description)}
      />
    </main>
  );
}

function BrowserSubTabButton({
  active,
  children,
  onClick
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <Button variant={active ? "default" : "ghost"} size="sm" className="h-8 px-3" onClick={onClick}>
      {children}
    </Button>
  );
}

function BrowseTrackRow({
  track,
  selected,
  playing,
  onToggle,
  onDetails,
  onPlay,
  onOpenFolder
}: {
  track: PlaylistIndexTrack;
  selected: boolean;
  playing: boolean;
  onToggle: () => void;
  onDetails: () => void;
  onPlay: () => void;
  onOpenFolder: () => void;
}) {
  const metadataSummary = trackMetadataSummary(track);

  return (
    <div className={cn(
      "grid min-h-14 grid-cols-[24px_36px_44px_minmax(0,1.35fr)_minmax(0,0.8fr)_minmax(0,0.8fr)_92px_40px] items-center gap-2 border-b border-border px-3 text-xs",
      !track.source_exists && "bg-red-50 dark:bg-red-950/30"
    )}>
      <input type="checkbox" checked={selected} onChange={onToggle} />
      <Button variant={playing ? "default" : "secondary"} size="icon" disabled={!track.source_exists || !track.source_path} onClick={onPlay}>
        {playing ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </Button>
      <TrackCover sourcePath={track.source_path} title={track.name ?? track.track_id} className="h-10 w-10" />
      <div className="min-w-0">
        <button
          type="button"
          className="block max-w-full truncate text-left font-semibold underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          title={track.name ?? track.track_id}
          onClick={onDetails}
        >
          {track.name ?? track.track_id}
        </button>
        {metadataSummary ? (
          <span className="block truncate text-[11px] text-muted-foreground" title={metadataSummary}>{metadataSummary}</span>
        ) : null}
      </div>
      <span className="truncate" title={track.artist ?? ""}>{track.artist ?? ""}</span>
      <span className="truncate" title={track.album ?? ""}>{track.album ?? ""}</span>
      <span className="truncate text-muted-foreground">{track.total_time ? formatTime(track.total_time) : track.kind ?? ""}</span>
      <Button variant="secondary" size="icon" disabled={!track.source_path} onClick={onOpenFolder}>
        <FolderOpen className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

function BrowseTrackDetailSheet({
  track,
  onClose,
  onPlay,
  onOpenFolder
}: {
  track: PlaylistIndexTrack | null;
  onClose: () => void;
  onPlay: (track: PlaylistIndexTrack) => void;
  onOpenFolder: (track: PlaylistIndexTrack) => void;
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

function EmptyRow({ children }: { children: React.ReactNode }) {
  return <div className="flex min-h-11 items-center px-3 text-sm text-muted-foreground">{children}</div>;
}

function trackMetadataSummary(track: PlaylistIndexTrack) {
  return [
    track.genre,
    track.bpm ? `${track.bpm} BPM` : null,
    track.key,
    track.year,
    track.label
  ]
    .map((value) => value?.trim())
    .filter(Boolean)
    .join(" · ");
}

function artistAlbumsFromTracks(tracks: PlaylistIndexTrack[]) {
  const groups = new Map<string, { value: string; name: string; tracks: PlaylistIndexTrack[] }>();

  for (const track of tracks) {
    const value = albumValue(track.album);
    const existing = groups.get(value);
    if (existing) {
      existing.tracks.push(track);
    } else {
      groups.set(value, {
        value,
        name: value || "Sin album",
        tracks: [track]
      });
    }
  }

  return Array.from(groups.values()).sort((left, right) => {
    if (!left.value) return 1;
    if (!right.value) return -1;
    return left.name.localeCompare(right.name);
  });
}

function albumValue(album?: string | null) {
  return album?.trim() ?? "";
}

function translateMissingGroupName(t: (key: string) => string, name: string) {
  if (name === "Sin artista" || name === "Sin album" || name === "Sin metadata") return t(name);
  return name;
}

function formatTime(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const rest = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${rest}`;
}
