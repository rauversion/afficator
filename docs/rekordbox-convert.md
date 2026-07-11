# Rekordbox Convert

Rekordbox Convert importa un XML exportado desde Rekordbox, permite seleccionar playlists, convierte los archivos necesarios a AIFF y exporta un XML nuevo con reemplazos seguros.

## Flujo

1. Exporta tu libreria o playlists desde Rekordbox como XML.
2. Abre Aifficator en **File Conversion > Rekordbox Convert**.
3. Importa el XML.
4. Revisa playlists, tracks y reporte.
5. Selecciona una o varias playlists.
6. Crea un plan si quieres revisar el preflight.
7. Convierte por fila, por playlist o por multiples playlists.
8. Revisa el terminal de `ffmpeg`.
9. Exporta un XML nuevo.
10. Importa ese XML en Rekordbox.

Guia visual de importacion: [Importar XML en Rekordbox](rekordbox-import/README.md).

## Export XML seguro

El XML original queda intacto. El export genera un archivo nuevo con extension sugerida:

```text
original.aifficator.aiff.xml
```

El export mantiene toda la coleccion del XML original:

- tracks no convertidos quedan apuntando a su `Location` original;
- tracks convertidos apuntan al AIFF en `converted/`;
- playlists y estructura se preservan.

Si faltan conversiones necesarias o hay problemas bloqueantes, la app reporta el error antes de escribir un export ambiguo.

## Conversion AIFF

Los archivos se convierten con `ffmpeg` al perfil compatible:

- AIFF
- `pcm_s16be`
- 44.1 kHz
- stereo
- sin overwrite

Los originales no se reemplazan.

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

## Plan

El boton **Crear plan** hace un preflight. No convierte y no exporta.

Sirve para revisar:

- tracks que se convertiran;
- tracks que ya son AIFF;
- AIFF existentes reutilizables;
- archivos faltantes;
- formatos no soportados;
- bloqueos antes de exportar.

## Interfaz

- Sidebar de playlists con seleccion y progreso.
- Tabla de tracks de la playlist activa.
- Player por fila.
- Tabs de playlist, convertidos, plan y reporte.
- Terminal fijo y expandible.
- Concurrencia controlada.

## Archivos relevantes

- `src/App.tsx`
- `src-tauri/src/lib.rs`
- `crates/aifficator-core/src/rekordbox.rs`
- `crates/aifficator-core/src/planner.rs`
- `crates/aifficator-core/src/exporter.rs`
- `crates/aifficator-core/src/validation.rs`
