# Architecture

Rau Studio is a local-first desktop app for audio preparation, Rekordbox conversion, mastering, release visuals, and playlist intelligence.

The application is built around a Tauri desktop shell, a Rust backend, a React/TypeScript frontend, local SQLite persistence, and local media tools (`ffmpeg`/`ffprobe`). macOS bundles carry pinned sidecars; other platforms currently resolve system or manually configured binaries. Original media files and source XML files are treated as immutable inputs.

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
- Icecast source profiles and an ordered, durable radio broadcast queue.
- Encrypted settings, including OpenAI and enrichment-provider credentials and audio tool paths.
- Encrypted P2P device identity, trusted peers, presence observations, shared folders, and virtual file catalogs.

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
- Icecast source lifecycle, MP3 publishing, queue recovery, and reconnect policy.
- P2P identity locking, share permissions, and filesystem-safe catalog indexing.

## Media Tool Resolution

The backend resolves each media tool in this order:

1. an explicit path saved in Settings;
2. the sidecar bundled with the macOS application;
3. the process `PATH` and known package-manager locations.

macOS sidecars are built from pinned FFmpeg, x264, and LAME source archives by
`scripts/prepare-ffmpeg-sidecars.sh`. The script validates source checksums,
architectures, dynamic dependencies, required encoders, filters, and network
protocols, plus real AIFF/MP3/MP4 smoke conversions before Tauri bundles them.

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
- `broadcast-progress`

Rau Connect emits `p2p-network-event` for endpoint lifecycle and authenticated diagnostic traffic. Presence observations, chat delivery, and transfer progress will extend this event boundary as their protocols are added.

The app shell listens to these events to report bridge health, while each feature page consumes the relevant stream for progress, row status, and terminal logs.

## Radio Broadcast Pipeline

The desktop app acts as an Icecast source client; listeners never connect to the
Mac directly. An outbound connection publishes to a reachable Icecast server,
which owns the public listener URL and fan-out.

```text
Indexed playlist -> SQLite queue -> per-track FFmpeg decoder --\
                                                               +-> Rust PCM mixer
Native microphone (CPAL/CoreAudio) -> bounded PCM buffer ------/
                                                                   |
                                                                   v
PCM pipe -> persistent FFmpeg/libmp3lame publisher -> Icecast -> listeners
```

1. The user saves one Icecast source profile. Its password is encrypted through
   the existing settings vault and is never returned to the frontend.
2. Adding an indexed playlist snapshots its playable local paths and original
   order into `broadcast_queue_entries`.
3. A per-track decoder normalizes audio to stereo 44.1 kHz signed 16-bit PCM.
4. One long-lived publisher encodes that PCM with `libmp3lame` and writes the
   configured Icecast mount. It emits silence while the queue is empty so the
   listener URL remains connected.
5. The optional microphone is opened by the Rust process through CPAL/CoreAudio,
   so macOS associates capture permission with Rau Studio instead of the FFmpeg
   sidecar. Native samples are resampled into a bounded stereo PCM buffer and
   mixed into music or silence only while the operator marks it live.
6. Track transitions update Icecast metadata through its admin endpoint.
7. Stop, skip, and microphone-live commands travel over an in-process channel. Interrupted
   `playing` rows return to `queued` when a new session starts.
8. A lost publisher is terminated and recreated with bounded reconnect delays;
   the current track returns to the queue instead of being marked as played.

See [Radio Broadcast](radio-broadcast.md) for setup, operation, and network
topologies.

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

## Metadata Enrichment Pipeline

Track enrichment is a modular provider pipeline. The planner routes missing fields only to providers that declare the relevant capability. Provider adapters own authentication, request construction, retries, and rate limits. Successful calls create append-only field observations; a field-aware resolver selects canonical suggestions while retaining every provider value as provenance.

Provider credentials are encrypted in local settings and loaded only by the Rust backend. The frontend receives configuration status and masked previews, never stored secret values. See [Metadata Enrichment](enrichment.md) for the provider contract, persistence model, and application policy.

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
- `src/BroadcastPage.tsx`
- `src/PlaylistIndexPage.tsx`
- `src/PlaylistBrowserPage.tsx`
- `src/TaxonomyPage.tsx`
- `src/PlaylistCopilotPage.tsx`
- `src/components/tracks/*`

## Key Backend Files

- `src-tauri/src/lib.rs`
- `src-tauri/src/local_conversion.rs`
- `src-tauri/src/enrichment/*`
- `src-tauri/src/mastering.rs`
- `src-tauri/src/playlist_copilot.rs`
- `src-tauri/src/playlist_index.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/system.rs`
- `src-tauri/src/turn.rs`
- `src-tauri/src/broadcast.rs`
- `crates/aifficator-core/src/*`
