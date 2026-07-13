# Rekordbox Convert

Rekordbox Convert imports a Rekordbox XML export, lets the user select playlists, converts the required audio files to AIFF, and exports a new XML with safe `Location` replacements.

## Workflow

1. Export a library or playlist set from Rekordbox as XML.
2. Open **File Conversion > Rekordbox Convert** in Rau Studio.
3. Import the XML.
4. Review playlists, tracks, converted files, the conversion plan, and the report.
5. Select one or more playlists.
6. Create a plan if you want to run a preflight first.
7. Convert by row, by active playlist, or by multiple selected playlists.
8. Watch the fixed terminal for `ffmpeg` progress and errors.
9. Export a new XML.
10. Import the exported XML into Rekordbox.

Visual import guide: [Import Rau Studio XML into Rekordbox](rekordbox-import/README.md).

## Safe XML Export

The original XML is never modified. Rau Studio writes a new XML with a suggested suffix:

```text
original.rau-studio.aiff.xml
```

The exported XML keeps the full collection from the original XML:

- tracks that were not converted keep their original `Location`;
- converted tracks point to the AIFF file inside `converted/`;
- playlists and folder structure are preserved.

If required conversions are missing or blocking issues exist, the app reports the problem before writing an ambiguous export.

## AIFF Conversion

Conversion uses `ffmpeg` with a conservative compatibility profile:

- AIFF container;
- `pcm_s16be`;
- 44.1 kHz;
- stereo;
- no overwrite.

Original files are not replaced.

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

## Plan

The **Create Plan** button runs a preflight. It does not convert files and it does not export XML.

The plan helps review:

- tracks that will be converted;
- tracks that are already AIFF;
- existing converted AIFF files that can be reused;
- missing files;
- unsupported formats;
- blocking issues before export.

## Interface

- Scrollable playlist sidebar with selection, processing indicators, and converted counters.
- Track table for the active playlist.
- Row-level player and actions.
- Tabs for playlist files, converted files, plan, and report.
- Fixed expandable terminal for conversion and export logs.
- Controlled concurrency selector.

## Relevant Files

- `src/App.tsx`
- `src-tauri/src/lib.rs`
- `crates/aifficator-core/src/rekordbox.rs`
- `crates/aifficator-core/src/planner.rs`
- `crates/aifficator-core/src/exporter.rs`
- `crates/aifficator-core/src/validation.rs`
