# File Importer

File Importer converts local audio files to AIFF without requiring a Rekordbox XML. It is useful for preparing folders, external drives, or manual selections before using them in other workflows.

## Goals

- Open a folder or select individual files.
- Show the current import without mixing it with global history.
- Save each import as a browsable group.
- Convert only when the user explicitly starts conversion.
- Keep file references, statuses, and events in SQLite.
- Show `ffmpeg` progress and logs in realtime.

## Workflow

1. Open **File Conversion > File Conversion**.
2. Use **Folder** or **Files**.
3. Review the **Current Import** tab.
4. Select files or use the header checkbox.
5. Adjust concurrency if needed.
6. Run **Convert Selected** or convert a single row.
7. Watch the bottom terminal for events and errors.
8. Open **Groups** to revisit a previous import.
9. Open **All** to inspect the global file history.

Choosing a folder or files does not enqueue conversion automatically. Items stay `pending` until the user starts a conversion action.

## Output

AIFF files are written inside a `converted/` folder next to the source file.

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

If the source is already AIFF/AIF, it is marked `already_aiff` and not duplicated. If the target AIFF already exists, it is marked `already_converted` and reused.

## Supported Formats

- FLAC
- MP3
- WAV / WAVE
- M4A
- ALAC
- AAC
- AIFF / AIF

AIFF/AIF is considered a final format.

## Conversion Command

The conversion uses the same core profile:

```sh
ffmpeg \
  -hide_banner \
  -nostdin \
  -n \
  -i input \
  -map 0:a:0 \
  -vn \
  -ac 2 \
  -ar 44100 \
  -c:a pcm_s16be \
  -progress pipe:1 \
  -nostats \
  output.aiff
```

The `-n` flag prevents overwriting existing files.

## Tabs

### Current Import

Shows only the latest folder or manual selection. This view refreshes whenever a new folder is imported or a saved group is opened.

### All

Shows every file reference stored in SQLite. This is the global history.

### Groups

Lists persisted import groups:

- `folder`: a scanned folder, recursive or non-recursive;
- `files`: a manual file selection.

Opening a group makes its files the current import.

## Statuses

| Status | Meaning |
| --- | --- |
| `pending` | Registered but not sent to conversion |
| `queued` | Waiting for `ffmpeg` |
| `running` | Conversion in progress |
| `converted` | AIFF generated successfully |
| `already_converted` | Target AIFF already existed |
| `already_aiff` | Source was already AIFF/AIF |
| `failed` | Conversion or validation failed |

## SQLite Persistence

Everything is stored in the local SQLite database, currently the legacy `aifficator.sqlite3` file inside the app data directory.

Main tables:

- `local_conversion_items`: one unique reference per `source_path`.
- `local_conversion_groups`: folder or manual-selection groups.
- `local_conversion_group_items`: many-to-many relation between groups and files.
- `local_conversion_events`: logs and conversion events.

Separating items from groups lets a file exist once in global history while appearing in multiple imports.

## Terminal and Events

The fixed bottom terminal shows:

- batch start and finish;
- missing-file errors;
- relevant `ffmpeg` lines;
- per-file progress;
- reuse of existing AIFF files;
- write and permission failures.

The terminal starts collapsed and can be expanded for detailed inspection.

## Concurrency

The UI proposes a default concurrency based on logical CPU cores:

```text
default = min(4, max(1, floor(logical_cores / 2)))
```

The backend also clamps concurrency between `1` and `4`.

## Tauri Commands

- `local_conversion_list_items`
- `local_conversion_list_groups`
- `local_conversion_group_items`
- `local_conversion_add_files`
- `local_conversion_scan_folder`
- `local_conversion_convert_items`
- `local_conversion_delete_item`

## Relevant Files

- `src/FileConversionPage.tsx`
- `src-tauri/src/local_conversion.rs`
- `crates/aifficator-core/src/conversion.rs`
