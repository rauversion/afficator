# Aifficator

Aplicacion nativa para preparar audio para Rekordbox, convertir archivos a AIFF y experimentar con mastering local asistido por AI.

El proyecto usa Tauri 2, Rust, React, TypeScript, SQLite y `ffmpeg`. La app evita modificar archivos originales: los convertidos se escriben en salidas nuevas y el historial queda guardado localmente.

## Modulos

- [Rekordbox Convert](docs/rekordbox-convert.md): importa XML de Rekordbox, convierte tracks de playlists a AIFF y exporta un XML seguro.
- [File Importer](docs/file-importer.md): importa archivos o carpetas locales, crea grupos de trabajo, convierte a AIFF y mantiene historial en SQLite.
- [Mastering](docs/mastering.md): genera masters AIFF con presets, metadata, cover, analisis tecnico, eventos en tiempo real y reintentos.
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
src-tauri/target/release/bundle/
```

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
