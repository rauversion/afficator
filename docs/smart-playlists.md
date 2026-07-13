# Smart Playlists

Smart Playlists covers the playlist intelligence features in Rau Studio:

- Rekordbox XML indexing into SQLite.
- Lexical and vector search.
- Artist and album browsing.
- Taxonomy visualizations.
- Playlist Copilot suggestions.
- Local draft playlists that can be exported back to Rekordbox XML.

The feature is designed for large DJ libraries where metadata, BPM, key, genre, artist, and playlist membership are useful signals for building new sets.

## Data Flow

1. Import a Rekordbox XML in **Playlists > Playlist Library**.
2. Rau Studio parses tracks, playlist folders, playlist memberships, source paths, and XML attributes.
3. The data is stored in local SQLite.
4. SQLite FTS is rebuilt for lexical search.
5. Optional OpenAI embeddings can be generated for indexed tracks.
6. Search, browse, taxonomy, and Copilot views read from SQLite.
7. Selected tracks can be added to local draft playlists.
8. Draft playlists can be exported as a Rekordbox-compatible XML.

Audio files are not uploaded and are not stored in the database. SQLite stores metadata, paths, index state, embeddings, and draft playlist membership.

## Playlist Library

Path: **Playlists > Playlist Library**

Playlist Library is the control center for importing XML, indexing selected playlists, searching tracks, generating embeddings, and managing local draft playlists.

### Index XML

The **Index XML** tab lets the user:

- choose a Rekordbox XML;
- preview playlists before importing;
- select specific playlists instead of indexing the whole library;
- index selected playlists with row-level status feedback;
- delete indexed libraries, playlists, or tracks;
- inspect whether tracks are indexed and whether vectors are ready.

Indexing selected playlists stores only the tracks referenced by those playlists. Indexing the whole XML stores the full collection from the XML.

### Search

The **Search** tab supports:

- normal lexical search through SQLite FTS;
- vector search when embeddings are available;
- row-level embedding status;
- embedding one track at a time;
- embedding selected tracks in batches;
- deleting selected tracks from the local index;
- opening the track detail sheet by clicking the title;
- playing and opening source folders from each row.

Vector search sends only text metadata to OpenAI to create a temporary query embedding. It compares that temporary embedding against vectors stored locally in SQLite.

### Playlist Drafts

The **Playlist** tab manages local draft playlists:

- create a new draft;
- add selected tracks from search, browser, taxonomy, or Copilot views;
- remove tracks from a draft;
- export the draft playlist as a Rekordbox XML.

Drafts do not modify the original XML until the user explicitly exports.

## Artist and Album Browser

Paths:

- **Playlists > Artists**
- **Playlists > Albums**

The browser views let users explore indexed tracks by artist or album. They include:

- fixed-height scrollable group lists;
- group-level search;
- track tables with covers, player, checkboxes, and metadata sheet;
- bulk selection;
- shared **Add to Playlist** dialog for existing or new draft playlists.

Clicking an artist or album opens its tracks. Track titles open the metadata sheet.

## Taxonomies

Path: **Playlists > Taxonomies**

Taxonomies summarize the indexed library from metadata. Current views include:

- overview metrics;
- genre distribution;
- BPM and key distribution;
- a graph connecting genre, BPM bucket, and key;
- track lists behind each taxonomy selection.

The graph intentionally avoids playlist nodes so the visualization focuses on musical metadata instead of folder structure. It helps discover clusters such as:

- genres that share BPM ranges;
- keys that dominate specific genres;
- tempo zones with enough tracks for a set.

Selected taxonomy tracks can be played, inspected, selected, and added to draft playlists.

## Playlist Copilot

Path: **Playlists > Playlist Copilot**

Playlist Copilot is a chat-like generator for playlist suggestions. The user writes a brief such as:

```text
Warm up house 118-124 BPM with soft vocals
```

or:

```text
Peak time melodic techno in Am or Em, no aggressive tracks
```

The Copilot:

1. reads the active indexed library from SQLite;
2. builds a compact local profile from genres, artists, keys, and BPM range;
3. optionally asks OpenAI to interpret the prompt into filters;
4. falls back to local prompt parsing if OpenAI is unavailable;
5. ranks local tracks by metadata, BPM, key, prompt terms, optional vector similarity, source availability, and diversity;
6. returns candidate tracks with explanations;
7. preselects the candidates so they can be quickly added to a draft playlist.

### What Goes to OpenAI

When an API key is configured, Playlist Copilot sends:

- the user's prompt;
- a compact profile of the indexed library;
- target track count.

It does not send audio files. It does not upload the full collection. Track selection and scoring happen locally against SQLite.

If no API key is configured, Copilot still works with local parsing and ranking.

### Candidate Review

The candidate table uses the shared track-list components:

- cover extraction/cache;
- row-level play/stop;
- clickable title for the metadata sheet;
- checkbox selection;
- open source folder action;
- metadata columns for artist, album, genre, BPM, key, and format.

A reason panel explains why the top tracks were suggested.

### Adding to Playlists

Selected Copilot candidates use the same **Add to Playlist** dialog as browser and taxonomy views:

- add to an existing draft playlist;
- create a new draft playlist and add the selected tracks;
- show the selected track count before confirming.

## Privacy and Network Behavior

| Feature | Network use |
| --- | --- |
| XML indexing | None |
| Lexical search | None |
| Artist/album browsing | None |
| Taxonomies | None |
| Vector indexing | Sends text metadata to OpenAI embeddings |
| Vector search | Sends search query text to OpenAI embeddings |
| Playlist Copilot | Sends prompt + compact library profile to OpenAI chat, if configured |

Audio files stay local.

## SQLite Tables

Main playlist-intelligence tables:

- `playlist_index_libraries`
- `playlist_index_tracks`
- `playlist_index_playlists`
- `playlist_index_memberships`
- `playlist_track_fts`
- `playlist_track_embeddings`
- `playlist_drafts`
- `playlist_draft_tracks`

## Tauri Commands

Indexing and search:

- `playlist_index_libraries`
- `playlist_index_preview_xml`
- `playlist_index_import_xml`
- `playlist_index_library_playlists`
- `playlist_index_playlist_tracks`
- `playlist_index_search_tracks`
- `playlist_index_generate_embeddings`
- `playlist_index_delete_library`
- `playlist_index_delete_playlists`
- `playlist_index_delete_tracks`

Browsing and taxonomies:

- `playlist_index_track_groups`
- `playlist_index_group_tracks`
- `playlist_index_taxonomy_overview`
- `playlist_index_taxonomy_graph`
- `playlist_index_taxonomy_tracks`
- `playlist_index_track_cover`

Draft playlists:

- `playlist_index_drafts`
- `playlist_index_create_draft`
- `playlist_index_add_tracks_to_draft`
- `playlist_index_remove_draft_track`
- `playlist_index_delete_draft`
- `playlist_index_draft_tracks`
- `playlist_index_export_draft_xml`

Copilot:

- `playlist_copilot_generate`

## Relevant Files

- `src/PlaylistIndexPage.tsx`
- `src/PlaylistBrowserPage.tsx`
- `src/TaxonomyPage.tsx`
- `src/PlaylistCopilotPage.tsx`
- `src/components/tracks/TrackList.tsx`
- `src/components/tracks/TrackCover.tsx`
- `src/components/tracks/TrackDetailSheet.tsx`
- `src/components/tracks/PlaylistAddDialog.tsx`
- `src-tauri/src/playlist_index.rs`
