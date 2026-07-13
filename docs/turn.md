# Turn

Turn generates spinning-record MP4 mockups from local cover art and a local audio file. It ports the `turn` concept from Rauversion into Rau Studio, but everything stays local: no ActiveStorage upload, no email delivery, and results are opened from the folder that contains the generated file.

## Goals

- Choose cover art.
- Choose a local audio file.
- Preview the spinning record.
- Listen to the selected audio range.
- Adjust background color, record size, and rotation speed.
- Trim audio with a range slider.
- Generate a square `1080x1080` MP4.
- Show realtime progress.
- Store history, events, and output paths in SQLite.
- Reopen previous jobs, retry, or delete results.

## Workflow

1. Open **Turn** from the sidebar.
2. In the **Editor** tab, choose a **Cover**.
3. Choose an **Audio** file.
4. Adjust the audio range in **Audio and Duration**.
5. Click **Play Preview** to hear only the selected range.
6. Adjust background, record size, and RPM.
7. Click **Generate Video**.
8. Watch progress in the UI and bottom terminal.
9. When the job finishes, play the MP4 from the job detail.
10. Use **Open MP4 Folder** to open the folder containing the video.

## Tabs

### Editor

Contains the main form, preview, audio player, visual controls, trim controls, and active job detail.

The editor uses the available width so preview and MP4 detail do not compete with history.

### History

Shows every generated or failed video. Each row includes:

- cover;
- audio;
- state;
- progress;
- chips for MP4 availability, processing state, and events.

Clicking a row opens that job back in the **Editor** tab.

## Preview and Trim

The preview has two synchronized parts:

- the record spins with the selected cover art;
- the audio plays only the selected range.

The range controls follow the Rauversion behavior:

- loading a new audio file sets the range to `0..full_duration`;
- the left handle moves the start;
- the right handle moves the end;
- the center handle moves the full range while preserving duration;
- reset returns to the full audio file.

When preview playback reaches the range end, it pauses and seeks back to the range start. If the user changes the range while preview is paused, the audio seeks to the new start.

## Controls

| Control | Description |
| --- | --- |
| Cover | Image used as the spinning record |
| Audio | File that is trimmed and embedded in the video |
| Background | Solid video background color |
| Record | Record size as a percentage of the canvas |
| Speed | RPM used to calculate rotation |
| Audio and Duration | Range `start..end`; video duration is `end - start` |

Backend normalization:

- duration: `1..900` seconds;
- RPM: `1..78`;
- record size: `20..100`;
- color: valid hex color or an `ffmpeg`-accepted color name.

## Output

Each job writes files under the app data directory:

```text
<app-data>/turn/jobs/<job-id>/
```

Main file:

```text
turn-<cover-stem>.mp4
```

The path is stored in `turn_jobs.output_path`.

## ffmpeg Render

Turn renders with `ffmpeg` using separated arguments, without shell interpolation. Current output:

- video: H.264 (`libx264`);
- audio: AAC;
- pixel format: `yuv420p`;
- size: `1080x1080`;
- `+faststart` for better playback;
- realtime progress through `-progress pipe:1`.

The app creates a local circular mask in PGM format:

```text
<app-data>/turn/alpha-mask.pgm
```

That mask lets the app crop cover art into a record without depending on Rails assets.

Conceptual pipeline:

1. Create a solid `1080x1080` background.
2. Scale/crop the background.
3. Scale cover art according to record size.
4. Rotate cover art using RPM.
5. Apply the circular mask.
6. Overlay the record on the background.
7. Map trimmed audio.
8. Write the final MP4.

## Statuses

| Status | Meaning |
| --- | --- |
| `pending` | Job created but not started by the worker |
| `running` | `ffmpeg` is rendering |
| `completed` | MP4 is available |
| `failed` | Render failed with an error message |

## Terminal and Events

Events are stored in `turn_events` and emitted in realtime as `turn-progress`.

Each event includes:

- `event`
- `step`
- `level`
- `message`
- `progress`
- `payload_json`
- job snapshot

The fixed bottom terminal shows active job events, including relevant `ffmpeg` logs.

## SQLite Persistence

Everything is stored in local SQLite, currently the legacy `aifficator.sqlite3` file inside the app data directory.

Main tables:

- `turn_jobs`: settings, state, inputs, output, and timestamps.
- `turn_events`: persistent per-job timeline.

Relevant `turn_jobs` fields:

- `cover_image_path`
- `cover_image_name`
- `audio_file_path`
- `audio_file_name`
- `output_path`
- `state`
- `duration_seconds`
- `loop_speed`
- `audio_start`
- `audio_end`
- `background_color`
- `disc_size`
- `error_message`

## Tauri Commands

- `turn_list_jobs`
- `turn_get_job`
- `turn_job_events`
- `turn_start_job`
- `turn_retry_job`
- `turn_delete_job`

## Differences from Rauversion

| Rauversion | Rau Studio |
| --- | --- |
| Uploads files to ActiveStorage | Uses local paths |
| Enqueues a Rails job | Enqueues a local Tauri/Rust worker |
| Sends an email on completion | Shows progress and result in the app |
| Downloads from a URL | Opens the local MP4 folder |
| Uses `alpha_mask.png` asset | Generates `alpha-mask.pgm` locally |

## Relevant Files

- `src/TurnPage.tsx`
- `src-tauri/src/turn.rs`
- `src-tauri/src/lib.rs`
