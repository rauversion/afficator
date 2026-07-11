# Aifficator

Aplicacion nativa para preparar audio para Rekordbox, convertir archivos a AIFF y experimentar con mastering local asistido por AI.

El proyecto usa Tauri 2, Rust, React, TypeScript, SQLite y `ffmpeg`. La app evita modificar archivos originales: los convertidos se escriben en salidas nuevas y el historial queda guardado localmente.

## Modulos

- [Rekordbox Convert](docs/rekordbox-convert.md): importa XML de Rekordbox, convierte tracks de playlists a AIFF y exporta un XML seguro.
- [File Importer](docs/file-importer.md): importa archivos o carpetas locales, crea grupos de trabajo, convierte a AIFF y mantiene historial en SQLite.
- [Mastering](docs/mastering.md): genera masters AIFF con presets, metadata, cover, analisis tecnico, eventos en tiempo real y reintentos.
- [Turn](docs/turn.md): genera videos MP4 de discos girando desde cover y audio local, con preview por rango, progreso realtime e historial.
- [Importar XML en Rekordbox](docs/rekordbox-import/README.md): guia visual para importar el XML exportado por Aifficator.
- [Arquitectura](docs/architecture.md): notas tecnicas de la estructura interna.

## Principios

- No reemplaza archivos fuente.
- No modifica el XML original de Rekordbox.
- No pisa AIFF existentes por defecto.
- Guarda estado operativo en SQLite local.
- Muestra progreso y logs en tiempo real.
- Usa concurrencia controlada para evitar saturar CPU, disco y memoria.

## Stack

| Capa | Tecnologia |
| --- | --- |
| Desktop | Tauri 2 |
| Core | Rust |
| UI | React + TypeScript |
| Estilos | Tailwind + componentes estilo shadcn |
| Audio | ffmpeg / ffprobe |
| Persistencia | SQLite |
| Build frontend | Vite |

## Requisitos

- Rust estable.
- Node.js y npm.
- `ffmpeg` y `ffprobe` disponibles en `PATH`.

En macOS:

```sh
brew install ffmpeg
```

## Comandos

Instalar dependencias:

```sh
npm install
```

Levantar la app nativa en desarrollo:

```sh
npm run tauri:dev
```

Levantar solo la UI web:

```sh
npm run dev
```

Compilar frontend:

```sh
npm run build
```

Compilar la app nativa bundleada:

```sh
npm run tauri:build
```

Los bundles quedan bajo:

```text
target/release/bundle/
```

## Releases

GitHub Actions genera instaladores descargables para macOS, Windows y Linux.

Opciones:

- Ejecutar manualmente **Build installers** desde la tab **Actions**.
- Crear un tag `v*` para publicar un GitHub Release con artefactos adjuntos.

Ejemplo:

```sh
git tag v0.1.4
git push origin v0.1.4
```

### macOS no firmado

Los builds de macOS publicados actualmente no estan firmados ni notarizados con Apple Developer ID. Gatekeeper puede mostrar el aviso "Apple could not verify..." al abrir la app descargada.

Para probarla localmente, descomprime el `.app.tar.gz` y quita la marca de quarantine:

```sh
cd ~/Downloads
tar -xzf Aifficator_0.1.4_arm64.app.tar.gz
xattr -dr com.apple.quarantine Aifficator.app
open Aifficator.app
```

Si ya la copiaste a `/Applications`:

```sh
xattr -dr com.apple.quarantine /Applications/Aifficator.app
open /Applications/Aifficator.app
```

El bypass es solo para testing local. Para distribuir sin advertencias hay que firmar y notarizar; los detalles estan en [docs/macos-signing.md](docs/macos-signing.md).

Artefactos esperados:

- macOS Apple Silicon: `_arm64.app.tar.gz`
- macOS Intel: `_x86_64.app.tar.gz`
- Windows: `.exe` / `.msi`
- Linux: `.AppImage` / `.deb`

Probar el core Rust:

```sh
cargo test -p aifficator-core
```

## Estructura

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

## Troubleshooting rapido

Si `ffmpeg` no corre:

```sh
ffmpeg -version
ffprobe -version
```

Si el WebSocket de Vite falla en desarrollo, reinicia:

```sh
npm run tauri:dev
```

Si Rekordbox no encuentra archivos luego de importar XML, revisa que los `Location` del XML exportado apunten a archivos reales en disco.

## Licencia

MIT
