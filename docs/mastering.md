# Mastering

Mastering genera un AIFF masterizado desde un archivo local. El flujo combina presets, metadata embebida, cover opcional, analisis tecnico con `ffmpeg`/`ffprobe`, feedback del usuario, una receta de procesamiento y un historial explorable guardado en SQLite.

## Objetivo

- Elegir un archivo de audio local.
- Seleccionar un preset de destino.
- Agregar feedback y notas de referencia.
- Usar AI opcionalmente para interpretar el feedback y construir una politica de mastering.
- Renderizar un master AIFF.
- Escribir tags de metadata y cover art opcional.
- Guardar receta, analisis antes/despues, eventos y resultado.
- Reabrir cualquier job desde el historial.
- Reintentar jobs con feedback actualizado.

## Flujo de uso

1. Entra a **Mastering** desde el sidebar.
2. Pulsa **Elegir audio**.
3. Selecciona un preset.
4. Escribe feedback y notas de referencia si aplica.
5. Activa o desactiva **AI**.
6. Pulsa **Generar master**.
7. Revisa el progreso en la pantalla y en el terminal inferior.
8. Escucha original y master desde el detalle.
9. Abre la carpeta del resultado o descarga el AIFF.
10. Usa **Reintentar** para correr el mismo job con ajustes.

## Formatos de salida

La etapa DSP renderiza un WAV temporal de trabajo. Al final, la app empaqueta el resultado como AIFF y escribe metadata.

| Formato | Codec | Uso |
| --- | --- | --- |
| AIFF 24-bit | `pcm_s24be` | Master/archive con mas resolucion |
| AIFF CDJ safe 16-bit | `pcm_s16be`, 44.1 kHz, stereo | Compatibilidad conservadora con Rekordbox/CDJ/XDJ |

Los jobs antiguos que ya existian como WAV se siguen leyendo desde el historial.

## Metadata y cover

El formulario permite definir:

- titulo;
- artista;
- album;
- genero;
- ano;
- numero de track;
- BPM;
- tonalidad;
- ISRC;
- compositor;
- label;
- copyright;
- comentario;
- cover JPG/PNG.

La metadata se guarda en SQLite y se escribe en el AIFF usando ID3v2 dentro del contenedor AIFF. El cover se intenta incrustar como `attached_pic`; si falla, la app genera el AIFF sin cover, deja un warning en el reporte y mantiene el master utilizable.

## Presets

| Preset | Target | True peak | Uso |
| --- | ---: | ---: | --- |
| Streaming clean | -14 LUFS | -1.0 dB | Limpio, dinamico y seguro para plataformas |
| Club loud | -9 LUFS | -0.7 dB | Fuerte y energetico, cuidando transientes |
| Demo balanced | -11.5 LUFS | -1.0 dB | Presentable y balanceado |
| Vinyl premaster | -15 LUFS | -3.0 dB | Conservador, con headroom y sin hard limiting |

`Demo balanced` es el perfil default de la UI.

## Pipeline

El backend ejecuta un job asincronico con estas etapas:

1. `queue`: marca el job como `running`.
2. `source`: valida que el audio fuente exista.
3. `analysis_before`: analiza loudness, peaks, rango dinamico, clipping, DC offset y metadata.
4. `recipe`: genera una receta usando preset, feedback, referencia y AI opcional.
5. `render`: renderiza un WAV temporal 24-bit con la cadena DSP.
6. `analysis_after`: reanaliza el render temporal.
7. `loudness_correction`: aplica pasadas adicionales si quedo bajo el target y sigue siendo seguro.
8. `packaging`: empaqueta AIFF final, escribe metadata y valida tags/cover con `ffprobe`.
9. `completed`: guarda master final y sidecars JSON.

Si una etapa falla, el job queda en `failed` con `error_message` y el evento se registra en el historial.

## Analisis tecnico

El analisis usa:

- `ffprobe` para duracion, sample rate y canales.
- `ffmpeg` con `ebur128=peak=true` para LUFS integrado y true peak.
- `ffmpeg` con `astats` para sample peak, DC offset y crest factor.

Los datos se guardan como JSON en:

- `analysis_before_json`
- `analysis_after_json`

## AI

La AI es opcional. Cuando esta activa y hay API key configurada, el backend llama a OpenAI para:

- interpretar feedback del usuario;
- transformar notas de referencia en parametros;
- producir una politica de mastering compatible con el preset.

Si no se usa AI, el sistema genera una receta deterministica basada en preset y analisis.

La API key se configura en **Settings** y se guarda cifrada en SQLite local.

## Salidas

Cada job crea una carpeta bajo datos de la app:

```text
<app-data>/mastering/jobs/<job-id>/
```

Archivos principales:

- AIFF final masterizado.
- `recipe.json`
- `analysis_before.json`
- `analysis_after.json`
- `metadata.json`
- `package_report.json`

El path final queda guardado en `mastering_jobs.output_path`.

## Historial

El panel **Historial** permite:

- abrir jobs anteriores;
- ver estado;
- renderizar de nuevo con **Reintentar**;
- conservar feedback y notas de referencia;
- revisar eventos del job;
- eliminar jobs terminados o fallidos.

El historial se alimenta desde SQLite y no depende de que la app siga abierta durante la misma sesion.

## Estados

| Estado | Significado |
| --- | --- |
| `pending` | Job creado, aun no iniciado por el worker |
| `running` | Pipeline en ejecucion |
| `completed` | Master listo y reproducible |
| `failed` | Pipeline fallido con mensaje de error |

## Terminal y eventos

Los eventos se guardan en `mastering_events` y tambien se emiten en tiempo real como `mastering-progress`.

Cada evento incluye:

- `event`
- `step`
- `level`
- `message`
- `progress`
- `payload_json`
- snapshot del job

El terminal inferior muestra esos eventos para entender en que etapa esta el proceso.

## Persistencia SQLite

Tablas principales:

- `mastering_jobs`: estado, source, preset, feedback, output, receta y analisis.
- `mastering_events`: timeline persistente por job.

Campos relevantes de `mastering_jobs`:

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

## Comandos Tauri

- `mastering_profiles`
- `mastering_list_jobs`
- `mastering_get_job`
- `mastering_job_events`
- `mastering_start_job`
- `mastering_retry_job`
- `mastering_delete_job`

## Archivos relevantes

- `src/MasteringPage.tsx`
- `src-tauri/src/mastering.rs`
- `src-tauri/src/settings.rs`
