# File Importer

File Importer convierte archivos locales a AIFF sin depender de un XML de Rekordbox. Es util para preparar carpetas completas de musica o selecciones manuales antes de usarlas en otros flujos.

## Objetivo

- Abrir una carpeta o seleccionar archivos sueltos.
- Mostrar la importacion actual sin mezclarla con todo el historial.
- Guardar cada importacion como grupo navegable.
- Convertir solo cuando el usuario lo ordena.
- Mantener referencias, estados y eventos en SQLite.
- Mostrar progreso y logs de `ffmpeg` en tiempo real.

## Flujo de uso

1. Entra a **File Conversion > File Conversion**.
2. Usa **Carpeta** o **Archivos**.
3. Revisa la tab **Importacion actual**.
4. Selecciona archivos o usa el checkbox general.
5. Ajusta la concurrencia.
6. Ejecuta **Convertir seleccionados** o convierte por fila.
7. Revisa el terminal inferior para eventos y errores.
8. Abre **Grupos** para volver a una importacion anterior.
9. Abre **Todos** para ver el historial global.

Elegir carpeta o archivos no encola conversiones automaticamente. Los items quedan en estado `pending` hasta que se pulse una accion de conversion.

## Salida de archivos

Los AIFF se escriben dentro de una carpeta `converted/` ubicada junto al archivo fuente.

```text
/Music/Artist/Track.flac
/Music/Artist/converted/Track.aiff
```

Si el original ya es AIFF/AIF, se marca como `already_aiff` y no se duplica. Si el AIFF convertido ya existe, se marca como `already_converted` y se reutiliza.

## Formatos soportados

- FLAC
- MP3
- WAV / WAVE
- M4A
- ALAC
- AAC
- AIFF / AIF

AIFF/AIF se considera formato final.

## Conversion

La conversion usa el mismo perfil base del core:

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

El flag `-n` evita sobrescribir archivos existentes.

## Tabs

### Importacion actual

Muestra solo la ultima carpeta o seleccion manual abierta. Esta vista se refresca cada vez que se importa una carpeta nueva o se abre un grupo.

### Todos

Muestra todas las referencias guardadas en SQLite. Sirve como historial global.

### Grupos

Lista grupos persistidos:

- `folder`: una carpeta escaneada, con flag recursivo o no recursivo.
- `files`: una seleccion manual de archivos.

Al abrir un grupo, sus archivos pasan a ser la importacion actual.

## Estados

| Estado | Significado |
| --- | --- |
| `pending` | Registrado, aun no enviado a conversion |
| `queued` | En cola para `ffmpeg` |
| `running` | Conversion en proceso |
| `converted` | AIFF generado correctamente |
| `already_converted` | El AIFF destino ya existia |
| `already_aiff` | El original ya era AIFF/AIF |
| `failed` | Conversion o validacion fallida |

## Persistencia SQLite

Todo se guarda en `aifficator.sqlite3`, dentro del directorio de datos de la app.

Tablas principales:

- `local_conversion_items`: una referencia unica por `source_path`.
- `local_conversion_groups`: grupos de carpeta o seleccion manual.
- `local_conversion_group_items`: relacion many-to-many entre grupos y archivos.
- `local_conversion_events`: logs y eventos asociados a conversiones.

La separacion entre items y grupos permite que un archivo exista una sola vez en el historial global, pero aparezca en varias importaciones.

## Terminal y eventos

El terminal inferior muestra:

- inicio y fin de batches;
- errores de archivos no encontrados;
- lineas relevantes de `ffmpeg`;
- progreso por archivo;
- reutilizacion de AIFF existentes;
- fallas de escritura o permisos.

El terminal parte contraido y se puede expandir cuando se necesita inspeccionar detalles.

## Concurrencia

La UI propone una concurrencia default segun cores logicos:

```text
default = min(4, max(1, floor(cores_logicos / 2)))
```

El backend tambien limita la concurrencia entre `1` y `4`.

## Comandos Tauri

- `local_conversion_list_items`
- `local_conversion_list_groups`
- `local_conversion_group_items`
- `local_conversion_add_files`
- `local_conversion_scan_folder`
- `local_conversion_convert_items`
- `local_conversion_delete_item`

## Archivos relevantes

- `src/FileConversionPage.tsx`
- `src-tauri/src/local_conversion.rs`
- `crates/aifficator-core/src/conversion.rs`
