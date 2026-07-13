# Mastering

Mastering generates a mastered AIFF from a local audio file. The workflow combines presets, embedded metadata, optional cover art, technical analysis through `ffmpeg`/`ffprobe`, user feedback, a processing recipe, and an explorable SQLite history.

## Goals

- Choose a local audio file.
- Select a target preset.
- Add feedback and reference notes.
- Optionally use AI to interpret feedback and build a mastering policy.
- Render a mastered AIFF.
- Write metadata tags and optional cover art.
- Store the recipe, before/after analysis, events, and output path.
- Reopen any job from history.
- Retry jobs with updated feedback.

## Workflow

1. Open **Mastering** from the sidebar.
2. Click **Choose Audio**.
3. Select a preset.
4. Enter feedback and reference notes if needed.
5. Enable or disable **AI**.
6. Click **Generate Master**.
7. Watch progress in the screen and in the fixed bottom terminal.
8. Listen to the source and the master from the job detail.
9. Open the result folder or download the AIFF.
10. Use **Retry** to run the same job again with adjustments.

## Output Formats

The DSP stage renders a temporary working WAV. The final packaging stage writes the result as AIFF with metadata.

| Format | Codec | Use |
| --- | --- | --- |
| AIFF 24-bit | `pcm_s24be` | Master/archive with higher resolution |
| AIFF CDJ safe 16-bit | `pcm_s16be`, 44.1 kHz, stereo | Conservative Rekordbox/CDJ/XDJ compatibility |

Older jobs that already exist as WAV are still readable from history.

## Metadata and Cover Art

The form supports:

- title;
- artist;
- album;
- genre;
- year;
- track number;
- BPM;
- key;
- ISRC;
- composer;
- label;
- copyright;
- comment;
- JPG/PNG cover art.

Metadata is stored in SQLite and written into the AIFF using ID3v2 inside the AIFF container. Cover art is attempted as `attached_pic`; if that fails, the app still writes a usable AIFF, records a packaging warning, and keeps the master available.

## Presets

| Preset | Target | True peak | Use |
| --- | ---: | ---: | --- |
| Streaming clean | -14 LUFS | -1.0 dB | Clean, dynamic, platform-safe |
| Club loud | -9 LUFS | -0.7 dB | Loud and energetic while preserving transients |
| Demo balanced | -11.5 LUFS | -1.0 dB | Presentable and balanced |
| Vinyl premaster | -15 LUFS | -3.0 dB | Conservative, with headroom and no hard limiting |

`Demo balanced` is the default UI profile.

## Pipeline

The backend runs an asynchronous job with these stages:

1. `queue`: mark the job as `running`.
2. `source`: validate that the source audio exists.
3. `analysis_before`: analyze loudness, peaks, dynamic range, clipping, DC offset, and metadata.
4. `recipe`: generate a recipe from the preset, feedback, reference notes, and optional AI.
5. `render`: render a temporary 24-bit WAV with the DSP chain.
6. `analysis_after`: analyze the temporary render.
7. `loudness_correction`: apply additional passes if the render is below target and still safe.
8. `packaging`: package the final AIFF, write metadata, and validate tags/cover with `ffprobe`.
9. `completed`: store the final master and sidecar JSON files.

If a stage fails, the job becomes `failed` with an `error_message` and a persisted event.

## Technical Analysis

Analysis uses:

- `ffprobe` for duration, sample rate, and channels.
- `ffmpeg` with `ebur128=peak=true` for integrated LUFS and true peak.
- `ffmpeg` with `astats` for sample peak, DC offset, and crest factor.

Data is saved as JSON in:

- `analysis_before_json`
- `analysis_after_json`

## AI

AI is optional. When enabled and an OpenAI API key is configured, the backend calls OpenAI to:

- interpret user feedback;
- turn reference notes into parameters;
- produce a mastering policy compatible with the selected preset.

If AI is disabled or unavailable, the system generates a deterministic recipe from the preset and analysis.

The API key is configured in **Settings** and encrypted in local SQLite.

## Outputs

Each job creates a folder under app data:

```text
<app-data>/mastering/jobs/<job-id>/
```

Main files:

- final mastered AIFF;
- `recipe.json`;
- `analysis_before.json`;
- `analysis_after.json`;
- `metadata.json`;
- `package_report.json`.

The final path is stored in `mastering_jobs.output_path`.

## History

The **History** tab supports:

- opening previous jobs;
- viewing status;
- retrying with **Retry**;
- preserving feedback and reference notes;
- reviewing job events;
- deleting completed or failed jobs.

History is loaded from SQLite and does not depend on the app staying open in the same session.

## Statuses

| Status | Meaning |
| --- | --- |
| `pending` | Job created but not started by the worker |
| `running` | Pipeline is running |
| `completed` | Master is ready and playable |
| `failed` | Pipeline failed with an error message |

## Terminal and Events

Events are stored in `mastering_events` and emitted in realtime as `mastering-progress`.

Each event includes:

- `event`
- `step`
- `level`
- `message`
- `progress`
- `payload_json`
- job snapshot

The fixed bottom terminal shows those events so users can understand the current stage.

## SQLite Persistence

Main tables:

- `mastering_jobs`: state, source, preset, feedback, output, recipe, and analysis.
- `mastering_events`: persistent per-job timeline.

Relevant `mastering_jobs` fields:

- `feedback`
- `reference_notes`
- `output_format`
- `metadata_json`
- `cover_art_path`
- `recipe_json`
- `analysis_before_json`
- `analysis_after_json`
- `package_report_json`
- `error_message`
- `output_path`

## Tauri Commands

- `mastering_profiles`
- `mastering_list_jobs`
- `mastering_get_job`
- `mastering_job_events`
- `mastering_start_job`
- `mastering_retry_job`
- `mastering_delete_job`

## Relevant Files

- `src/MasteringPage.tsx`
- `src-tauri/src/mastering.rs`
- `src-tauri/src/settings.rs`
