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
- Icecast/RTMP destination profiles and an ordered, durable broadcast queue.
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
- Icecast/RTMP publisher lifecycle, encoding, queue recovery, and reconnect policy.
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

Rau Connect emits `p2p-network-event` for endpoint lifecycle and diagnostics, `p2p-chat-event` for persisted message delivery, and `p2p-transfer-event` for download progress and completion.

The app shell listens to these events to report bridge health, while each feature page consumes the relevant stream for progress, row status, and terminal logs.

## Broadcast Pipeline

The desktop app publishes one mixed PCM stream through a destination-specific
FFmpeg publisher. Icecast owns the public listener URL and audio fan-out. An
RTMP service such as Instagram receives a vertical live-video signal and owns
the preview and final go-live step. Both connections are outbound from the Mac.

```text
Indexed playlist -> SQLite queue -> per-track FFmpeg decoder ----\
Native line input (CPAL/CoreAudio) -> bounded PCM buffer ---------+-> primary source selector --\
Mac/system audio (ScreenCaptureKit) -> bounded PCM buffer -------/                            +-> Rust PCM mixer
Native microphone (CPAL/CoreAudio) -> bounded PCM buffer ------------------------------------/
                                                                                                   |
                                                                                                   v
                                                      /-> libmp3lame -> Icecast -> listeners
PCM pipe -> persistent destination publisher --------+
                                                      \-> AAC + paced branded video + visual layer/libx264 -> RTMP service -> viewers
```

1. The user saves one Broadcast profile with an `output_kind`. Icecast stores
   its source password in the existing encrypted settings vault. RTMP stores
   the server URL and encoding preset, but the stream key is supplied only to
   the start command and is never persisted.
2. Adding an indexed playlist snapshots its playable local paths and original
   order into `broadcast_queue_entries`.
3. A per-track decoder normalizes audio to stereo 44.1 kHz signed 16-bit PCM.
4. One long-lived publisher consumes the PCM. Icecast encodes it with
   `libmp3lame` and writes the configured mount. RTMP encodes the PCM as AAC and
   combines it with an independently paced monochrome source in a 720 × 1280,
   30 fps H.264/FLV signal. A `drawtext` overlay reloads atomically written
   station and current-track text without restarting the publisher. Builds
   without `drawtext` retain the animated visual as a compatibility fallback.
   RTMP emits silence while the queue is empty so the destination remains
   connected.
5. The optional cross-platform visual compositor captures a camera with `getUserMedia` and a display or application window
   with the operating system's `getDisplayMedia` picker. Both streams can remain enabled simultaneously. The webview draws
   both sources in persisted Z order on a transparent 360 × 640 canvas. Each layer owns its layout,
   position, size, fit/crop, orientation, mirror, effect, and opacity. Paced RGBA frames cross local Tauri IPC and Rust converts
   them directly to BGRA for the publisher's named pipe, preserving transparency without a browser-dependent image codec. Preview/Program fader commands then change
   the combined layer alpha frame by frame. The same path works on macOS, Windows, and Linux and leaves RTMP connected while
   sources or composition controls change. A native AVFoundation camera path remains available for legacy profiles.
   The Preview monitor overlays pointer-only editing bounds: dragging or resizing writes bounded integer canvas geometry,
   switches the selected layer to Free layout, and commits on pointer release. These editing bounds are never rendered into
   the encoded canvas or Program monitor.
6. The optional microphone is opened by the Rust process through CPAL/CoreAudio,
   so macOS associates capture permission with Rau Studio instead of the FFmpeg
   sidecar. Native samples are resampled into a bounded stereo PCM buffer and
   mixed into music or silence only while the operator marks it live.
7. The optional direct line input is a second CoreAudio capture with explicit
   mono-channel or stereo-pair routing. It replaces the playlist as the primary
   source without ducking. The active track decoder is held while line is live,
   so the queue does not advance, and resumes when the operator returns to the
   Playlist source. Its PCM writer uses a monotonic 50 ms deadline so processing
   time is deducted from the wait instead of accumulating capture latency.
8. The optional Mac-output source uses ScreenCaptureKit on macOS 13+ to capture
   the complete system output by default, excluding Rau Studio itself to avoid
   feedback. It can alternatively filter capture to one selected running
   application. It is stereo, does not apply ducking or mix the microphone, and
   holds the playlist decoder just as direct line does. The OS Screen & System
   Audio Recording permission gates capture and application discovery.
9. Track transitions update metadata through the Icecast admin endpoint only;
   RTMP destinations keep the continuous audio/video signal without that call.
10. Stop, skip, selected-track playback, microphone-live, source-mode, and camera-fader commands travel over an
    in-process channel. Selecting a queue row stops only the current decoder and hands its id to the same worker;
    the persistent publisher remains connected. Queue reordering updates only the position slots of `queued` rows,
    leaving the current and historical rows protected. Interrupted `playing` rows return to `queued` when a new
    session starts.
11. A lost publisher is terminated and recreated with bounded reconnect delays;
    the current track returns to the queue instead of being marked as played.
    Logs redact the active Icecast password or RTMP stream key.

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
