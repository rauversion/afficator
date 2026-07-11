import { invoke } from "@tauri-apps/api/core";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type Locale = "es" | "en";

type TranslationValues = Record<string, string | number | null | undefined>;

type I18nContextValue = {
  locale: Locale;
  setLocale: (locale: Locale) => Promise<void>;
  t: (key: string, values?: TranslationValues) => string;
};

type LanguageSettings = {
  language: string;
};

const localeStorageKey = "aifficator.locale";
const defaultLocale: Locale = "es";

const translations: Record<Locale, Record<string, string>> = {
  es: {},
  en: {
    "Abrir carpeta": "Open folder",
    "Actualizar status": "Refresh status",
    "API key": "API key",
    "Apariencia": "Appearance",
    "Audio tools": "Audio tools",
    "Base de datos local": "Local database",
    "Chequeando": "Checking",
    "Claro": "Light",
    "Conectando": "Connecting",
    "Configura ffmpeg/ffprobe o deja autodeteccion.": "Configure ffmpeg/ffprobe or leave autodetection enabled.",
    "Contraer": "Collapse",
    "Creditos de Rau Studio": "Rau Studio credits",
    "Creado por": "Created by",
    "Cargando rutas...": "Loading paths...",
    "Desktop": "Desktop",
    "Disponible": "Available",
    "Eliminar key": "Delete key",
    "Enter an OpenAI API key.": "Enter an OpenAI API key.",
    "Error": "Error",
    "Eventos OK": "Events OK",
    "Expandir": "Expand",
    "FFmpeg": "FFmpeg",
    "FFprobe": "FFprobe",
    "File Conversion": "File Conversion",
    "Guardar key": "Save key",
    "Guardar rutas": "Save paths",
    "Herramienta local para preparar audio, playlists y visuales sin depender de servicios externos.":
      "A local tool for preparing audio, playlists, and visuals without depending on external services.",
    "Idioma": "Language",
    "Idioma guardado: {language}": "Language saved: {language}",
    "Ingresa un OpenAI API key.": "Enter an OpenAI API key.",
    "Incluye ffprobe. Puedes ajustar rutas en Settings.": "Includes ffprobe. You can adjust paths in Settings.",
    "Instala ffmpeg": "Install ffmpeg",
    "Limpiar": "Clear",
    "Mastering": "Mastering",
    "Mostrar": "Show",
    "No configurada": "Not configured",
    "No instalado": "Not installed",
    "Ocultar": "Hide",
    "OpenAI API key": "OpenAI API key",
    "Oscuro": "Dark",
    "Preferencias generales de Rau Studio.": "General Rau Studio preferences.",
    "Quien creo Rau Studio": "Who created Rau Studio",
    "Refrescar": "Refresh",
    "Rekordbox Convert": "Rekordbox Convert",
    "Revisando estado...": "Checking status...",
    "Ruta ffmpeg": "ffmpeg path",
    "Ruta ffprobe": "ffprobe path",
    "Rutas de herramientas guardadas.": "Tool paths saved.",
    "Rutas de herramientas restauradas a autodeteccion.": "Tool paths restored to autodetection.",
    "Settings": "Settings",
    "Sin eventos todavia.": "No events yet.",
    "Source file not found": "Source file not found",
    "Status": "Status",
    "Terminal": "Terminal",
    "Turn": "Turn",
    "Usar defaults": "Use defaults",
    "WebSocket": "WebSocket",
    "Esta seccion ya esta registrada en el router y lista para recibir su flujo.":
      "This section is already registered in the router and ready for its workflow.",
    "archivo(s)": "file(s)",
    "conversion engine": "conversion engine",
    "eventos": "events",
    "ffmpeg / ai / mastering": "ffmpeg / AI / mastering",
    "ffmpeg / file conversion": "ffmpeg / file conversion",
    "ffmpeg / turn": "ffmpeg / turn",
    "listeners Tauri": "Tauri listeners",
    "metadata probe": "metadata probe",
    "para la comunidad.": "for the community.",
    "ultimo": "last",
    "OpenAI API key guardada.": "OpenAI API key saved.",
    "OpenAI API key eliminada.": "OpenAI API key deleted.",
    "Guardada: {preview}": "Saved: {preview}",
    "v{version} · Desktop": "v{version} · Desktop",
    "Rauversion community build": "Rauversion community build",
    "Español": "Spanish",
    "Inglés": "English",
    "Abrir audio": "Open audio",
    "Abrir destino": "Open destination",
    "Abierto": "Open",
    "Abre una carpeta o un grupo para ver la importación actual.": "Open a folder or group to view the current import.",
    "Acciones": "Actions",
    "Agrega archivos o escanea una carpeta para empezar.": "Add files or scan a folder to get started.",
    "AI sin key": "AI without key",
    "Archivo": "File",
    "Archivos": "Files",
    "Artista": "Artist",
    "Audio y duracion": "Audio and duration",
    "Audio: {audio}. Video: {video}.": "Audio: {audio}. Video: {video}.",
    "Cambiar a modo claro": "Switch to light mode",
    "Cambiar a modo oscuro": "Switch to dark mode",
    "Carpeta": "Folder",
    "Cerrar": "Close",
    "Club, streaming, demo, vinilo, referencia sonora...": "Club, streaming, demo, vinyl, sonic reference...",
    "Codigo": "Code",
    "Comentario": "Comment",
    "Concurrencia": "Concurrency",
    "Convertibles": "Convertible",
    "Convertidos": "Converted",
    "Convertir": "Convert",
    "Convertir playlist": "Convert playlist",
    "Convertir seleccionados": "Convert selected",
    "Convertir {count} playlists": "Convert {count} playlists",
    "Convierte archivos locales a AIFF en carpetas converted.": "Convert local files to AIFF inside converted folders.",
    "Crear plan": "Create plan",
    "Cargando": "Loading",
    "Crea un plan para ver los tracks seleccionados.": "Create a plan to view the selected tracks.",
    "Defaults del sistema": "System defaults",
    "Descargar actual": "Download current",
    "Destino": "Destination",
    "Detecta tracks a convertir, AIFF existentes, archivos faltantes y formatos bloqueados.":
      "Detects tracks to convert, existing AIFF files, missing files, and blocked formats.",
    "Disco": "Disc",
    "Editor": "Editor",
    "Elegir": "Choose",
    "Elegir audio": "Choose audio",
    "El historial aparece cuando generes el primer master.": "History appears after you generate the first master.",
    "El plan revisa las playlists seleccionadas antes de convertir. No modifica archivos ni exporta XML.":
      "The plan checks selected playlists before conversion. It does not modify files or export XML.",
    "Elige un archivo de audio": "Choose an audio file",
    "Elige una carpeta para navegar archivos de audio originales.": "Choose a folder to browse original audio files.",
    "Elige una portada": "Choose a cover",
    "Errores": "Errors",
    "Escanear": "Scan",
    "Escuchar AIFF": "Listen to AIFF",
    "Escuchar archivo": "Listen to file",
    "Escuchar original": "Listen to original",
    "Explorar carpeta": "Explore folder",
    "Exportar XML": "Export XML",
    "Feedback": "Feedback",
    "Fondo": "Background",
    "Formato": "Format",
    "Formato y metadata": "Format and metadata",
    "Genera un turn para ver el MP4, sus eventos y el historial.": "Generate a turn to view the MP4, its events, and history.",
    "Generar master": "Generate master",
    "Generar video": "Generate video",
    "Genero": "Genre",
    "Grupos": "Groups",
    "Grupos de importación": "Import groups",
    "Historial": "History",
    "Importación actual": "Current import",
    "Importar XML": "Import XML",
    "JPG o PNG opcional.": "Optional JPG or PNG.",
    "Listo": "Done",
    "Mantener pegada, limpiar subgrave, suavizar hats...": "Keep punch, clean sub lows, soften hats...",
    "Master disponible": "Master available",
    "Mensaje": "Message",
    "Mockups de discos girando en MP4": "Spinning record mockups in MP4",
    "Mostrar en Finder": "Show in Finder",
    "No encontrados": "Missing",
    "No hay AIFF convertidos detectados para este XML.": "No converted AIFF files detected for this XML.",
    "No se encontraron archivos de audio originales.": "No original audio files found.",
    "No se reemplazan archivos fuente.": "Source files are not replaced.",
    "No soportados": "Unsupported",
    "Notas que quedaran embebidas en el AIFF...": "Notes that will be embedded in the AIFF...",
    "Nuevo master": "New master",
    "Nuevo turn": "New turn",
    "Olvidar": "Forget",
    "Olvidar XML": "Forget XML",
    "Originales": "Originals",
    "Pause": "Pause",
    "Pausar preview": "Pause preview",
    "Pendiente": "Pending",
    "Pendientes": "Pending",
    "Plan": "Plan",
    "Plan seleccionado": "Selected plan",
    "Play": "Play",
    "Play preview": "Play preview",
    "Playlists": "Playlists",
    "Preflight de conversion": "Conversion preflight",
    "Presets": "Presets",
    "Procesando": "Processing",
    "Progreso": "Progress",
    "Referencia": "Reference",
    "Refs rotas": "Broken refs",
    "Reporte": "Report",
    "Selecciona play en una fila.": "Select play on a row.",
    "Seleccionadas:": "Selected:",
    "Severidad": "Severity",
    "Si no eliges ninguna, se planifica toda la libreria.": "If none are selected, the full library is planned.",
    "Sin archivo cargado": "No file loaded",
    "Sin archivo seleccionado": "No file selected",
    "Sin carpeta activa": "No active folder",
    "Sin carpeta seleccionada": "No folder selected",
    "Sin jobs.": "No jobs.",
    "Sin masters todavia": "No masters yet",
    "Sin playlist seleccionada": "No playlist selected",
    "Sin videos todavia": "No videos yet",
    "Sin XML cargado": "No XML loaded",
    "Tamano": "Size",
    "Tema": "Track",
    "Titulo": "Title",
    "Todos": "All",
    "Todos los archivos": "All files",
    "Todavía no hay grupos. Abre una carpeta o selecciona archivos para crear uno.":
      "There are no groups yet. Open a folder or select files to create one.",
    "Velocidad": "Speed",
    "Ver detalle": "View detail",
    "Visual": "Visual",
    "XML recientes": "Recent XML",
    "{count} archivo(s)": "{count} file(s)",
    "{count} archivo(s) en la importación actual": "{count} file(s) in the current import",
    "{count} archivo(s) procesandose en esta playlist": "{count} file(s) processing in this playlist",
    "{count} archivos": "{count} files",
    "{count} bloqueados": "{count} blocked",
    "{count} carpetas o archivos no se pudieron leer.": "{count} folders or files could not be read.",
    "{count} conversiones": "{count} conversions",
    "{count} grupo(s) guardados": "{count} saved group(s)",
    "{count} hallazgos": "{count} findings",
    "{count} masters": "{count} masters",
    "{count} omitidos": "{count} skipped",
    "{count} playlists": "{count} playlists",
    "{count} referencias": "{count} references",
    "{count} referencia(s) guardadas": "{count} saved reference(s)",
    "{count} reutilizados": "{count} reused",
    "{count} seleccionadas": "{count} selected",
    "{count} seleccionados": "{count} selected",
    "{count} tracks": "{count} tracks",
    "{count} tracks unicos": "{count} unique tracks",
    "{converted} convertido(s) de {total} track(s)": "{converted} converted out of {total} track(s)",
    "{cores} core(s) detectado(s). Default: {recommended}.": "{cores} core(s) detected. Default: {recommended}.",
    "{cores} core(s) logico(s) detectado(s). Default recomendado: {recommended}.":
      "{cores} logical core(s) detected. Recommended default: {recommended}.",
    "{tracks} track(s) unico(s) pendientes en {playlists} playlist(s)":
      "{tracks} unique pending track(s) in {playlists} playlist(s)",
    "archivo": "file",
    "archivos": "files",
    "convertido": "converted",
    "en cola": "queued",
    "pendiente": "pending",
    "procesando": "processing",
    "ffmpeg / conversion / export": "ffmpeg / conversion / export"
  }
};

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => detectInitialLocale());

  useEffect(() => {
    let mounted = true;

    invoke<LanguageSettings>("get_language_settings")
      .then((settings) => {
        if (!mounted) return;
        const nextLocale = normalizeLocale(settings.language);
        setLocaleState(nextLocale);
        localStorage.setItem(localeStorageKey, nextLocale);
      })
      .catch(() => {
        localStorage.setItem(localeStorageKey, locale);
      });

    return () => {
      mounted = false;
    };
  }, []);

  async function setLocale(nextLocale: Locale) {
    const normalized = normalizeLocale(nextLocale);
    setLocaleState(normalized);
    localStorage.setItem(localeStorageKey, normalized);

    try {
      await invoke<LanguageSettings>("save_language_settings", { language: normalized });
    } catch (error) {
      console.error(error);
    }
  }

  const value = useMemo<I18nContextValue>(
    () => ({
      locale,
      setLocale,
      t: (key, values) => translate(locale, key, values)
    }),
    [locale]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used inside I18nProvider");
  }
  return context;
}

export function normalizeLocale(value: string | null | undefined): Locale {
  return value === "en" ? "en" : defaultLocale;
}

export function languageLabel(locale: Locale) {
  return locale === "en" ? "English" : "Español";
}

export function translate(locale: Locale, key: string, values?: TranslationValues) {
  const template = locale === "es" ? key : translations[locale][key] ?? key;
  return interpolate(template, values);
}

export function translateBackendMessage(locale: Locale, message: string) {
  if (locale === "es") return message;

  const exact = translations.en[message];
  if (exact) return exact;

  const replacements: Array<[RegExp, (match: RegExpMatchArray) => string]> = [
    [/^Conversion local iniciada: (\d+) archivo\(s\), concurrencia maxima (\d+)$/, (match) => `Local conversion started: ${match[1]} file(s), max concurrency ${match[2]}`],
    [/^Conversion local terminada: (\d+) convertidos, (\d+) existentes, (\d+) AIFF originales, (\d+) errores$/, (match) => `Local conversion finished: ${match[1]} converted, ${match[2]} existing, ${match[3]} original AIFF, ${match[4]} errors`],
    [/^Conversion iniciada: (\d+) track\(s\), concurrencia maxima (\d+)$/, (match) => `Conversion started: ${match[1]} track(s), max concurrency ${match[2]}`],
    [/^Conversion terminada: (\d+) convertidos, (\d+) existentes, (\d+) AIFF originales, (\d+) errores$/, (match) => `Conversion finished: ${match[1]} converted, ${match[2]} existing, ${match[3]} original AIFF, ${match[4]} errors`],
    [/^Reutilizando AIFF existente: (.+)$/, (match) => `Reusing existing AIFF: ${match[1]}`],
    [/^Conversion completada: (.+)$/, (match) => `Conversion completed: ${match[1]}`],
    [/^ffmpeg iniciado: (.+) -> (.+)$/, (match) => `ffmpeg started: ${match[1]} -> ${match[2]}`],
    [/^Archivo local no encontrado en SQLite: (.+)$/, (match) => `Local file not found in SQLite: ${match[1]}`],
    [/^TrackID no existe en COLLECTION: (.+)$/, (match) => `TrackID does not exist in COLLECTION: ${match[1]}`]
  ];

  for (const [pattern, replacement] of replacements) {
    const match = message.match(pattern);
    if (match) return replacement(match);
  }

  return message;
}

function detectInitialLocale(): Locale {
  if (typeof window === "undefined") return defaultLocale;
  return normalizeLocale(localStorage.getItem(localeStorageKey));
}

function interpolate(template: string, values?: TranslationValues) {
  if (!values) return template;
  return template.replace(/\{([^}]+)\}/g, (_, key: string) => String(values[key] ?? ""));
}
