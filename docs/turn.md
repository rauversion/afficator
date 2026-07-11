# Turn

Turn genera mockups de discos girando en video MP4 desde una portada y un archivo de audio local. Esta seccion porta la idea del flujo `turn` de Rauversion, pero en Aifficator todo queda local: no se sube a ActiveStorage, no se envia email y el resultado se abre desde la carpeta del archivo generado.

## Objetivo

- Elegir una imagen de portada.
- Elegir un archivo de audio local.
- Previsualizar el disco girando.
- Escuchar el preview respetando el rango de audio seleccionado.
- Ajustar color de fondo, tamano del disco y velocidad de rotacion.
- Recortar el audio con un slider de rango.
- Generar un MP4 cuadrado `1080x1080`.
- Ver progreso en tiempo real.
- Guardar historial, eventos y salida en SQLite.
- Reabrir jobs anteriores, reintentar o eliminar resultados.

## Flujo de uso

1. Entra a **Turn** desde el sidebar.
2. En la tab **Editor**, elige un **Cover**.
3. Elige un **Audio**.
4. Ajusta el rango de audio en **Audio y duracion**.
5. Pulsa **Play preview** para escuchar solo el tramo seleccionado.
6. Ajusta fondo, tamano del disco y RPM.
7. Pulsa **Generar video**.
8. Revisa el progreso en pantalla y en el terminal inferior.
9. Cuando termine, reproduce el MP4 en el detalle.
10. Usa **Abrir carpeta MP4** para abrir la carpeta que contiene el video.

## Tabs

### Editor

Contiene el formulario principal, preview, audio player, controles visuales, trim y detalle del job activo.

El editor usa todo el ancho disponible para que el preview y el detalle del MP4 no compitan con el historial.

### Historial

Muestra todos los videos generados o fallidos. Cada fila indica:

- cover;
- audio;
- estado;
- progreso;
- chips de MP4 disponible, procesando y eventos.

Al hacer click en una fila, el job se abre de vuelta en la tab **Editor**.

## Preview y trim

El preview tiene dos partes sincronizadas:

- El disco gira con la portada seleccionada.
- El audio reproduce solo el rango seleccionado.

El trim funciona como en Rauversion:

- Al cargar un audio nuevo, el rango queda `0..duracion completa`.
- El handle izquierdo mueve el inicio.
- El handle derecho mueve el fin.
- El handle central mueve todo el rango manteniendo la duracion.
- El boton de reset vuelve a usar el audio completo.

Cuando el preview llega al fin del rango, pausa y vuelve al inicio del rango. Si el usuario mueve el rango mientras el preview esta pausado, el audio se posiciona en el nuevo inicio.

## Controles

| Control | Descripcion |
| --- | --- |
| Cover | Imagen que se usa como disco girando |
| Audio | Archivo que se corta y se incrusta en el video |
| Fondo | Color solido del video |
| Disco | Tamano del disco como porcentaje del canvas |
| Velocidad | RPM usadas para calcular la rotacion |
| Audio y duracion | Rango `inicio..fin`; la duracion del video es `fin - inicio` |

Valores normalizados en backend:

- duracion: `1..900` segundos;
- RPM: `1..78`;
- disco: `20..100`;
- color: hex valido o nombre de color aceptado por `ffmpeg`.

## Salida

Cada job escribe sus archivos bajo el directorio de datos de la app:

```text
<app-data>/turn/jobs/<job-id>/
```

Archivo principal:

```text
turn-<cover-stem>.mp4
```

El path queda guardado en `turn_jobs.output_path`.

## Render con ffmpeg

Turn renderiza con `ffmpeg` usando argumentos separados, sin shell. La salida actual es:

- video: H.264 (`libx264`);
- audio: AAC;
- pixel format: `yuv420p`;
- dimensiones: `1080x1080`;
- `+faststart` para mejor reproduccion;
- progreso realtime con `-progress pipe:1`.

La app genera una mascara circular local en formato PGM:

```text
<app-data>/turn/alpha-mask.pgm
```

Esa mascara permite recortar la portada como disco sin depender de assets externos del repo Rails.

Pipeline conceptual:

1. Crear fondo solido `1080x1080`.
2. Escalar/cropear fondo.
3. Escalar portada segun tamano de disco.
4. Rotar portada usando RPM.
5. Aplicar mascara circular.
6. Overlay del disco sobre el fondo.
7. Mapear audio recortado.
8. Escribir MP4 final.

## Estados

| Estado | Significado |
| --- | --- |
| `pending` | Job creado, aun no iniciado por el worker |
| `running` | ffmpeg esta renderizando |
| `completed` | MP4 generado y disponible |
| `failed` | Render fallido con mensaje de error |

## Terminal y eventos

Los eventos se guardan en `turn_events` y tambien se emiten en tiempo real como `turn-progress`.

Cada evento incluye:

- `event`
- `step`
- `level`
- `message`
- `progress`
- `payload_json`
- snapshot del job

El terminal inferior muestra eventos del job activo, incluyendo logs relevantes de `ffmpeg`.

## Persistencia SQLite

Todo se guarda en `aifficator.sqlite3`, dentro del directorio de datos de la app.

Tablas principales:

- `turn_jobs`: configuracion, estado, entradas, salida y timestamps.
- `turn_events`: timeline persistente por job.

Campos relevantes de `turn_jobs`:

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

## Comandos Tauri

- `turn_list_jobs`
- `turn_get_job`
- `turn_job_events`
- `turn_start_job`
- `turn_retry_job`
- `turn_delete_job`

## Diferencias con Rauversion

| Rauversion | Aifficator |
| --- | --- |
| Sube archivos a ActiveStorage | Usa paths locales |
| Encola job Rails | Encola worker local Tauri/Rust |
| Envia email al terminar | Muestra progreso y resultado en la app |
| Descarga desde URL | Abre la carpeta local del MP4 |
| Usa asset `alpha_mask.png` | Genera `alpha-mask.pgm` localmente |

## Archivos relevantes

- `src/TurnPage.tsx`
- `src-tauri/src/turn.rs`
- `src-tauri/src/lib.rs`
