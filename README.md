# Rau Studio

Local native suite for preparing audio, converting files, managing Rekordbox playlists, generating masters, creating release visuals, and building smarter playlists from indexed metadata.

Rau Studio uses Tauri 2, Rust, React, TypeScript, SQLite, OpenAI-compatible AI features, and `ffmpeg`. The app is local-first: it does not replace original files, it stores operational history in a local SQLite database, and audio processing runs on the user's machine.

<img width="1262" height="783" alt="Rau Studio" src="https://github.com/user-attachments/assets/6f9d3936-4506-4246-9ddf-35682078e9b7" />

## Modules

- [Rekordbox Convert](docs/rekordbox-convert.md): import Rekordbox XML, select playlists, convert tracks to AIFF, and export a safe replacement XML.
- [File Importer](docs/file-importer.md): import local files or folders, create conversion groups, convert to AIFF, and keep history in SQLite.
- [Mastering](docs/mastering.md): generate AIFF masters with presets, metadata, cover art, technical analysis, realtime events, and retryable history.
- [Turn](docs/turn.md): generate MP4 spinning-record mockups from local cover art and audio, with range preview and realtime progress.
- [Smart Playlists](docs/smart-playlists.md): index Rekordbox XML into SQLite, run lexical/vector search, browse artists/albums, inspect taxonomies, and generate playlist suggestions with Playlist Copilot.
- [Metadata Enrichment](docs/enrichment.md): fill metadata gaps through capability-aware providers, encrypted credentials, durable observations, and field-level resolution.
- [Import Rau Studio XML into Rekordbox](docs/rekordbox-import/README.md): visual guide for importing exported XML back into Rekordbox.
- [macOS Signing and Notarization](docs/macos-signing.md): distribution notes for unsigned local builds and signed releases.
- [Architecture](docs/architecture.md): technical notes about the desktop, Rust, SQLite, and UI structure.

## Principles

- Original source files are never replaced.
- The original Rekordbox XML is never modified.
- Existing AIFF files are reused instead of overwritten.
- Operational state is stored in local SQLite.
- Long-running work reports realtime progress and terminal logs.
- Conversion jobs use controlled concurrency to avoid saturating CPU, disk, and memory.
- AI features are optional and work with local fallbacks when no OpenAI API key is configured.

## Stack

| Layer | Technology |
| --- | --- |
| Desktop | Tauri 2 |
| Core | Rust |
| UI | React + TypeScript |
| Styling | Tailwind + shadcn-style components |
| Audio/video | ffmpeg / ffprobe |
| Persistence | SQLite |
| Search | SQLite FTS + optional OpenAI embeddings |
| Frontend build | Vite |

## Requirements

- Stable Rust.
- Node.js and npm.
- `ffmpeg` and `ffprobe` available in `PATH`, or configured in **Settings**.

On macOS:

```sh
brew install ffmpeg
```

## Commands

Install dependencies:

```sh
npm install
```

Run the native app in development:

```sh
npm run tauri:dev
```

Run only the web UI:

```sh
npm run dev
```

Build the frontend:

```sh
npm run build
```

Build the native bundled app:

```sh
npm run tauri:build
```

Bundles are generated under:

```text
src-tauri/target/release/bundle/
```

Run the Rust core tests:

```sh
cargo test -p aifficator-core
```

## Releases

GitHub Actions builds downloadable installers for macOS, Windows, and Linux.

Release options:

- Run **Build installers** manually from the GitHub **Actions** tab.
- Push a `v*` tag to publish a GitHub Release with attached artifacts.

Example:

```sh
git tag v0.1.10
git push origin v0.1.10
```

Expected artifacts:

- macOS Apple Silicon: `_arm64.dmg`
- macOS Intel: `_x86_64.dmg`
- Windows: `.exe` / `.msi`
- Linux: `.AppImage` / `.deb`

## Signed macOS Builds

The release workflow signs macOS builds with **Developer ID Application**, notarizes
the app and DMG with Apple, staples the notarization tickets, and validates the
result with Gatekeeper. Details are in
[docs/macos-signing.md](docs/macos-signing.md).

## Project Structure

```text
.
|-- crates/aifficator-core/
|-- docs/
|-- src/
|-- src-tauri/
|-- Cargo.toml
|-- package.json
`-- README.md
```

## Quick Troubleshooting

Check `ffmpeg` and `ffprobe`:

```sh
ffmpeg -version
ffprobe -version
```

If the Vite websocket fails during development, restart the native dev server:

```sh
npm run tauri:dev
```

If Rekordbox cannot find files after importing XML, confirm that each exported `Location` points to an existing local file and that converted files still exist inside `converted/` folders.

If files live on an external macOS drive and playback/conversion fails, grant Rau Studio access to removable volumes or Full Disk Access, and verify the drive is not mounted read-only.

## License

MIT
