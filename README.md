# Aifficator

Aplicacion nativa para preparar playlists de Rekordbox y convertir audio a AIFF sin reemplazar los archivos originales.

Aifficator importa un XML exportado desde Rekordbox, muestra playlists y tracks, detecta problemas de archivos, convierte audio a AIFF en carpetas `converted/`, y permite exportar un XML nuevo listo para importar de vuelta en Rekordbox.

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

## Estado actual

Aifficator ya incluye:

- Import de XML Rekordbox.
- Listado de playlists y tracks.
- Seleccion multiple de playlists.
- Player simple para escuchar originales y AIFF convertidos.
- Explorador de carpetas para revisar archivos originales.
- Validacion de archivos no encontrados, formatos no soportados y referencias rotas.
- Plan de conversion previo, tipo preflight.
- Conversion con `ffmpeg` y progreso en tiempo real.
- Terminal fijo y expandible para logs de conversion, errores y eventos.
- Concurrencia controlada, con default basado en cores logicos detectados.
- Indicadores por playlist: procesando, convertidos/total.
- Export de XML seguro apuntando a AIFF convertidos.

## Lo importante

Aifficator esta disenado para ser conservador:

- No reemplaza archivos originales.
- No pisa AIFF existentes por defecto.
- Escribe convertidos en una carpeta `converted/` junto al archivo fuente.
- Exporta un XML nuevo; no modifica el XML original.
- Mantiene el flujo de conversion visible con progreso y logs.

## Stack

| Capa | Tecnologia |
| --- | --- |
| Desktop | Tauri 2 |
| Core | Rust |
| UI | React + TypeScript |
| Estilos | Tailwind + componentes estilo shadcn |
| Audio | ffmpeg |
| XML | quick-xml |
| Build frontend | Vite |

## Requisitos

- macOS, Linux o Windows con soporte Tauri.
- Rust estable.
- Node.js y npm.
- `ffmpeg` disponible en `PATH`.

En macOS, una instalacion tipica de `ffmpeg` seria:

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

Compilar la app nativa:

```sh
npm run tauri:build
```

Los bundles generados quedan bajo:

```text
src-tauri/target/release/bundle/
```

En macOS, Tauri normalmente genera artefactos como `.app` y/o `.dmg` dentro de esa carpeta.

Probar el core Rust:

```sh
cargo test -p aifficator-core
```

## Flujo de uso

1. Exporta tu libreria o playlists desde Rekordbox como XML.
2. Abre Aifficator.
3. Importa el XML.
4. Revisa el reporte de problemas.
5. Selecciona una o varias playlists.
6. Crea un plan si quieres revisar antes de convertir.
7. Convierte una fila, una playlist o multiples playlists seleccionadas.
8. Revisa el terminal para progreso y errores de `ffmpeg`.
9. Exporta un XML nuevo cuando los AIFF necesarios ya existan.
10. Importa ese XML en Rekordbox.

Guia visual con capturas: [Importar XML en Rekordbox](docs/rekordbox-import/README.md).

## Conversion

La salida actual busca compatibilidad amplia con Rekordbox/CDJ/XDJ:

- Contenedor: AIFF.
- Codec: `pcm_s16be`.
- Sample rate: `44100`.
- Canales: `2`.
- Overwrite: desactivado.

Argumentos base usados para `ffmpeg`:

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

Formatos convertibles:

- FLAC
- MP3
- WAV / WAVE
- M4A
- ALAC
- AAC

AIFF/AIF se considera formato final y se omite.

## Plan de conversion

El boton **Crear plan** hace un preflight. No convierte y no exporta.

Sirve para revisar:

- tracks que se van a convertir;
- tracks que ya son AIFF;
- AIFF existentes en `converted/` que se pueden reutilizar;
- archivos faltantes;
- formatos no soportados;
- bloqueos antes de exportar.

Si no hay playlists seleccionadas, el plan puede revisar el set completo disponible desde el XML.

## Export XML

El export crea un XML nuevo con reemplazo seguro:

- El XML original se deja intacto.
- Los tracks convertibles apuntan al AIFF en `converted/`.
- Se preserva estructura relevante del XML original.
- Si faltan conversiones, hay colisiones o errores bloqueantes, el export falla con reporte.

El path sugerido usa este formato:

```text
original.aifficator.aiff.xml
```

## Interfaz

La UI principal esta organizada en:

- Toolbar: importar XML, explorar carpeta, crear plan, exportar XML, concurrencia.
- Metricas: tracks, convertibles, convertidos, pendientes y errores.
- Player: play/pause del archivo activo.
- Originales: explorador auxiliar de carpetas.
- Sidebar playlists: seleccion, procesando, convertidos/total.
- Tabs:
  - Playlist
  - Convertidos
  - Plan
  - Reporte
- Terminal: logs de conversion y `ffmpeg`, contraido por defecto.

## Concurrencia

La concurrencia controla cuantos procesos de `ffmpeg` pueden correr al mismo tiempo.

El default se calcula desde `navigator.hardwareConcurrency`:

```text
default = min(4, max(1, floor(cores_logicos / 2)))
```

El limite superior actual es `4`, incluso si la maquina tiene mas cores. Esto evita saturar CPU, disco y memoria cuando se convierten muchos archivos.

## Estructura del proyecto

```text
.
|-- crates/
|   `-- aifficator-core/
|       `-- src/
|           |-- conversion.rs
|           |-- exporter.rs
|           |-- planner.rs
|           |-- rekordbox.rs
|           `-- validation.rs
|-- docs/
|   |-- architecture.md
|   `-- rekordbox-import/
|       |-- README.md
|       |-- rekordbox-import-xml-file.png
|       |-- rekordbox-refresh-icon.png
|       |-- rekordbox-xml-display.png
|       |-- rekordbox-xml-library-tab.png
|       `-- rekordbox-xml-library.png
|-- src/
|   |-- App.tsx
|   |-- main.tsx
|   |-- styles.css
|   `-- components/ui/
|-- src-tauri/
|   |-- src/lib.rs
|   |-- tauri.conf.json
|   `-- capabilities/
|-- Cargo.toml
|-- package.json
`-- README.md
```

## Core Rust

`aifficator-core` contiene la logica sin UI:

- `rekordbox.rs`: parseo de XML Rekordbox.
- `validation.rs`: validacion de tracks y rutas de salida.
- `planner.rs`: plan de conversion por playlists.
- `conversion.rs`: configuracion y argumentos de `ffmpeg`.
- `exporter.rs`: generacion del XML final con reemplazos.

## Tauri

`src-tauri/src/lib.rs` conecta el core con la UI.

Comandos principales:

- `import_rekordbox_xml`
- `plan_conversion`
- `export_rekordbox_xml`
- `convert_tracks`
- `list_converted_files`
- `list_audio_files`
- `playlist_tracks`
- `reveal_path`
- `open_parent_folder`

Eventos principales:

- `conversion-progress`
- `conversion-log`

## UI

La UI vive en `src/App.tsx` y usa componentes locales estilo shadcn en `src/components/ui/`.

Estado local relevante:

- XML reciente guardado en `localStorage`.
- Playlists seleccionadas.
- Progreso de conversion.
- Cola de conversion.
- Logs del terminal.
- Player activo.
- Sheet de metadata de track.

## Seguridad operativa

Antes de convertir una libreria grande:

1. Importa el XML.
2. Revisa el reporte.
3. Crea un plan.
4. Convierte con concurrencia baja si el disco es externo o lento.
5. Exporta XML solo cuando los bloqueos esten resueltos.

El flujo esta pensado para evitar conversiones silenciosas y exports ambiguos.

## Troubleshooting

### `ffmpeg` no se puede ejecutar

Instala `ffmpeg` y confirma que esta en `PATH`:

```sh
ffmpeg -version
```

### El XML tiene archivos no encontrados

Revisa que las rutas `Location` del XML apunten a archivos reales. Si moviste la musica despues de exportar desde Rekordbox, exporta el XML nuevamente o restaura esas rutas.

### No aparece un AIFF convertido

Usa **Refrescar** en la tab **Convertidos**. La app tambien refresca automaticamente despues de batches de conversion.

### WebSocket de Vite falla en desarrollo

Reinicia la app dev:

```sh
npm run tauri:dev
```

## Roadmap

Ideas pendientes o candidatas:

- Persistencia en SQLite para historial profundo de imports, jobs y exports.
- Preferencias guardadas de concurrencia.
- Opcion de AIFF 24-bit.
- Busqueda y filtros por playlist/track.
- Reportes exportables.
- Mejor inspeccion de metadata tecnica.

## Licencia

MIT
