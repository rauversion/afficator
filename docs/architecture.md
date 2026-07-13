# Architecture

Rau Studio is a local-first desktop app for audio preparation, Rekordbox conversion, mastering, release visuals, and playlist intelligence.

The application is built around a Tauri desktop shell, a Rust backend, a React/TypeScript frontend, local SQLite persistence, and external media tools (`ffmpeg`/`ffprobe`). Original media files and source XML files are treated as immutable inputs.

## Stack

| Layer | Technology |
| --- | --- |
| Desktop shell | Tauri 2 |
| Backend | Rust |
| Frontend | React + TypeScript |
| Styling | Tailwind + shadcn-style components |
| Persistence | SQLite |
| XML parsing/export | `quick-xml` and core Rekordbox helpers |
| Audio/video | `ffmpeg` / `ffprobe` |
| AI | Optional OpenAI API calls |

## Local-First Data Model

Rau Studio keeps operational state in a local SQLite database inside the app data directory. The historical database filename is `aifficator.sqlite3`.

Major domains:

- Rekordbox conversion plans and converted files.
- Local file conversion groups and events.
- Mastering jobs and events.
- Turn jobs and events.
- Indexed playlist libraries, memberships, draft playlists, and embeddings.
- Encrypted settings, including the OpenAI API key and audio tool paths.

The app never stores raw audio in SQLite. It stores file paths, metadata snapshots, derived analysis, and event logs.

## File Safety

The main file-safety rules are:

- source audio is never overwritten;
- source Rekordbox XML is never edited;
- converted AIFF files are written into sibling `converted/` folders;
- existing target AIFF files are reused or reported, not overwritten;
- exports are written as new XML files.

Example:

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

## Backend Boundaries

The Rust backend owns:

- filesystem access;
- XML parsing and export;
- SQLite migrations and queries;
- conversion/mastering/render workers;
- realtime event emission;
- ffmpeg/ffprobe command construction;
- optional OpenAI requests;
- encrypted settings persistence.

The frontend owns:

- navigation and layout;
- playlist and track selection;
- player UI;
- tables, sheets, dialogs, and terminal rendering;
- presenting realtime events from Tauri;
- user-facing validation messages.

## Realtime Events

Long-running tasks emit Tauri events:

- `conversion-progress`
- `conversion-log`
- `local-conversion-progress`
- `local-conversion-log`
- `mastering-progress`
- `playlist-index-progress`
- `turn-progress`

The app shell listens to these events to report bridge health, while each feature page consumes the relevant stream for progress, row status, and terminal logs.

## Rekordbox Conversion Pipeline

1. Import XML.
2. Parse `COLLECTION/TRACK`.
3. Parse `PLAYLISTS/NODE` and `TRACK Key` references.
4. Validate files:
   - missing or invalid `Location`;
   - missing source file;
   - unreadable metadata;
   - unsupported format;
   - existing converted target;
   - playlist references to unknown tracks;
   - target collisions.
5. Show playlists, tracks, and issues.
6. Build a conversion plan from selected playlists.
7. Convert with realtime progress.
8. Persist conversion results.
9. Export a new Rekordbox XML that preserves the full collection and only rewrites converted `Location` values.

## Playlist Intelligence Pipeline

1. Import a Rekordbox XML into the playlist index.
2. Store tracks, playlists, memberships, metadata attributes, and source paths in SQLite.
3. Rebuild SQLite FTS for lexical search.
4. Optionally generate OpenAI embeddings for selected tracks or the whole library.
5. Search with lexical FTS, vector similarity, metadata browsing, taxonomy graphs, or Playlist Copilot.
6. Select tracks and add them to local draft playlists.
7. Export draft playlists back to Rekordbox XML.

## AI Boundaries

AI is optional and scoped:

- Mastering can call OpenAI to interpret feedback and produce a processing policy.
- Vector search sends text metadata to OpenAI embeddings, never audio.
- Playlist Copilot sends the user's prompt and a compact library profile, not the full audio collection.

If OpenAI is not configured or a request fails, the app uses local deterministic fallbacks where available.

## Relevant Directories

```text
.
|-- crates/aifficator-core/  # Rekordbox parsing, planning, validation, conversion helpers
|-- src/                     # React UI
|-- src/components/          # Shared UI and track components
|-- src-tauri/               # Tauri/Rust backend
|-- docs/                    # Documentation
`-- .github/workflows/       # CI/release workflows
```

## Key Frontend Files

- `src/App.tsx`
- `src/FileConversionPage.tsx`
- `src/MasteringPage.tsx`
- `src/TurnPage.tsx`
- `src/PlaylistIndexPage.tsx`
- `src/PlaylistBrowserPage.tsx`
- `src/TaxonomyPage.tsx`
- `src/PlaylistCopilotPage.tsx`
- `src/components/tracks/*`

## Key Backend Files

- `src-tauri/src/lib.rs`
- `src-tauri/src/local_conversion.rs`
- `src-tauri/src/mastering.rs`
- `src-tauri/src/playlist_index.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/system.rs`
- `src-tauri/src/turn.rs`
- `crates/aifficator-core/src/*`
