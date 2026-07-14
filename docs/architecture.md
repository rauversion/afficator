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

## Playback Queue

Audio playback is coordinated in the frontend by the global audio player provider. The provider owns the current
track, the current playback queue, the queue index, progress state, and the hidden browser audio element.

Track list surfaces pass their visible ordered tracks as playback context when a user starts a track. The player
normalizes that context into a playable queue by filtering out missing files, stores the clicked track as the current
index, and automatically advances to the next playable item when the current audio ends. Direct previews that do not
come from a track list continue to use single-path playback and do not create a queue.

This keeps ordering behavior local to the UI context that the user is seeing while preserving one global player across
navigation, detail sheets, the sidebar, and playlist tools.

The provider publishes stable playback controls separately from sidebar-only progress and preference state. Track
lists therefore update when playback identity changes, but not for every audio `timeupdate` or volume slider change.

## Track Cover Pipeline

Track covers are requested only when a row approaches the visible viewport. A shared intersection observer and a
two-request scheduler deduplicate paths, bound concurrent extraction, and cancel work that has not started when its
last visible consumer disappears.

The Tauri cover command runs blocking filesystem and `ffmpeg` work on the async runtime's blocking pool. Cache misses
produce a versioned JPEG thumbnail no larger than 256 by 256 pixels through a temporary file; cache hits and known
missing covers return immediately. The UI lets WebKit defer and asynchronously decode the resulting local image.

## Realtime Events

Long-running tasks emit Tauri events:

- `conversion-progress`
- `conversion-log`
- `local-conversion-progress`
- `local-conversion-log`
- `mastering-progress`
- `playlist-index-progress`
- `playlist-copilot-progress`
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
5. Search with lexical FTS, vector similarity, metadata browsing, or taxonomy graphs.
6. Playlist Copilot reduces each message or guided answer into a revision of the persisted structured intent.
7. A search planner creates focused probes for the brief, style, feel, mix constraints, and adjacent discovery.
8. The backend batches probe embeddings, loads local vectors once, and fuses the focused top lists with reciprocal-rank fusion; metadata probes provide the offline fallback.
9. Its pure Rust planner ranks and rotates candidates by relevance, recent history, BPM, harmonic compatibility, energy curve, source policy, artist diversity, and conditional genre diversity.
10. The backend emits `playlist-copilot-progress` events so the chat can show brief changes, searches, ranking, and sequencing while the run is active.
11. Select tracks and add them to local draft playlists.
12. Export draft playlists back to Rekordbox XML.

## AI Boundaries

AI is optional and scoped:

- Mastering can call OpenAI to interpret feedback and produce a processing policy.
- Vector search sends text metadata to OpenAI embeddings, never audio.
- Playlist Copilot sends the user's prompt and a compact library profile, not the full audio collection.
- UI language instructions and rendered chat history are not used as embedding or ranking input.
- OpenAI interprets intent only; local deterministic code owns filtering, scoring, sequencing, and persistence.

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
- `src-tauri/src/playlist_copilot.rs`
- `src-tauri/src/playlist_index.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/system.rs`
- `src-tauri/src/turn.rs`
- `crates/aifficator-core/src/*`
