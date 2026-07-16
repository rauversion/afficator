import { invoke } from "@tauri-apps/api/core";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type Context,
  type ReactNode
} from "react";

export type Locale = "es" | "en";

export type TranslationValues = Record<string, string | number | null | undefined>;

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
    "Creative audio workspace · local first": "Creative audio workspace · local first",
    "Tu música, lista para moverse.": "Your music, ready to move.",
    "Convierte, organiza, descubre y finaliza tu catálogo desde un solo estudio privado en tu Mac.":
      "Convert, organize, discover, and finish your catalog from one private studio on your Mac.",
    "Abrir Rekordbox Convert": "Open Rekordbox Convert",
    "Explorar playlists": "Explore playlists",
    "Flujo conectado": "Connected workflow",
    "Workspace": "Workspace",
    "Elige por dónde empezar": "Choose where to begin",
    "Todo ocurre localmente en tu equipo": "Everything happens locally on your computer",
    "Importa tu XML, revisa playlists y convierte audio a AIFF sin perder la estructura.":
      "Import your XML, review playlists, and convert audio to AIFF without losing structure.",
    "Convierte carpetas y archivos locales con una cola clara, rápida y controlable.":
      "Convert local folders and files with a clear, fast, controllable queue.",
    "Indexa, busca y organiza colecciones grandes desde una biblioteca local inteligente.":
      "Index, search, and organize large collections from a smart local library.",
    "Convierte una idea musical en una selección explicable y lista para trabajar.":
      "Turn a musical idea into an explainable selection ready to use.",
    "Prepara masters consistentes con perfiles, análisis y seguimiento del proceso.":
      "Prepare consistent masters with profiles, analysis, and process tracking.",
    "Transforma y prepara audio con un flujo visual pensado para sesiones largas.":
      "Transform and prepare audio with a visual workflow designed for long sessions.",
    "Enriquece tu catálogo": "Enrich your catalog",
    "Completa metadata y mejora todo lo que viene después.": "Complete metadata and improve everything that follows.",
    "Abrir Enrichment": "Open Enrichment",
    "Studio": "Studio",
    "Broadcast": "Broadcast",
    "Radio desde casa": "Radio from home",
    "Rau Studio reproduce tu cola local y mantiene un stream MP3 persistente hacia Icecast.":
      "Rau Studio plays your local queue and keeps a persistent MP3 stream connected to Icecast.",
    "Destino Icecast": "Icecast destination",
    "FFmpeg listo": "FFmpeg ready",
    "Revisar FFmpeg": "Check FFmpeg",
    "Revisando motor FFmpeg...": "Checking FFmpeg engine...",
    "Host": "Host",
    "Puerto": "Port",
    "Mountpoint MP3": "MP3 mountpoint",
    "Usuario source": "Source user",
    "Bitrate MP3": "MP3 bitrate",
    "Nombre de estación": "Station name",
    "Descripción": "Description",
    "Contraseña source": "Source password",
    "Nueva contraseña source (opcional)": "New source password (optional)",
    "Usar TLS": "Use TLS",
    "Listar públicamente": "List publicly",
    "Eliminar contraseña guardada": "Delete saved password",
    "Guardar perfil": "Save profile",
    "Perfil Icecast guardado.": "Icecast profile saved.",
    "Control de transmisión": "Broadcast control",
    "Salir al aire": "Go live",
    "Detener": "Stop",
    "Saltar": "Skip",
    "Ahora al aire": "Now on air",
    "Configura Icecast, agrega una playlist y sal al aire.":
      "Configure Icecast, add a playlist, and go live.",
    "La conexión sigue viva transmitiendo silencio hasta que haya una pista.":
      "The connection stays live by streaming silence until a track is available.",
    "Cola de broadcast": "Broadcast queue",
    "Selecciona biblioteca": "Select library",
    "Selecciona playlist": "Select playlist",
    "La cola está vacía.": "The queue is empty.",
    "Quitar de la cola": "Remove from queue",
    "Actividad reciente": "Recent activity",
    "Sin eventos todavía.": "No events yet.",
    "Estado": "Status",
    "Reproducidas": "Played",
    "Fallidas": "Failed",
    "Detenida": "Stopped",
    "En vivo": "Live",
    "Reconectando": "Reconnecting",
    "Deteniendo": "Stopping",
    "Radio detenida.": "Radio stopped.",
    "Radio en vivo · esperando audio.": "Radio live · waiting for audio.",
    "Deteniendo radio...": "Stopping radio...",
    "Configura la contraseña source de Icecast.": "Configure the Icecast source password.",
    "La radio no esta transmitiendo.": "The radio is not broadcasting.",
    "El broadcast ya esta iniciado o deteniendose.": "The broadcast is already running or stopping.",
    "Iniciando transmisión a Icecast.": "Starting Icecast broadcast.",
    "Se agregaron {count} pistas al broadcast. {skipped} omitidas.":
      "Added {count} tracks to the broadcast. {skipped} skipped.",
    "Se quitaron {count} entradas de la cola.": "Removed {count} queue entries.",
    "FFmpeg esta listo para transmitir MP3 a Icecast.": "FFmpeg is ready to stream MP3 to Icecast.",
    "FFmpeg no esta disponible.": "FFmpeg is not available.",
    "FFmpeg no incluye el encoder libmp3lame requerido para MP3.":
      "FFmpeg does not include the libmp3lame encoder required for MP3.",
    "FFmpeg no incluye el protocolo de salida icecast.":
      "FFmpeg does not include the Icecast output protocol.",
    "FFmpeg no incluye TLS, pero el perfil Icecast exige conexión segura.":
      "FFmpeg does not include TLS, but the Icecast profile requires a secure connection.",
    "Entrada de micrófono": "Microphone input",
    "Preparar micrófono al iniciar": "Prepare microphone on start",
    "Dispositivo de entrada": "Input device",
    "Ganancia del micrófono: {gain}%": "Microphone gain: {gain}%",
    "Se prepara silenciado. Actívalo desde Control de transmisión cuando quieras hablar.":
      "It starts muted. Enable it from Broadcast control when you want to speak.",
    "Cuando detecta tu voz, la música baja automáticamente y vuelve a subir al terminar.":
      "When it detects your voice, music lowers automatically and rises again when you finish.",
    "Activa esta opción para seleccionar un micrófono.": "Enable this option to select a microphone.",
    "No hay un dispositivo de entrada de audio disponible.": "No audio input device is available.",
    "Silenciar micrófono": "Mute microphone",
    "Micrófono al aire": "Microphone live",
    "Nivel de entrada": "Input level",
    "Sin señal": "No signal",
    "Micrófono esperando inicio.": "Microphone waiting for broadcast.",
    "Micrófono preparado y silenciado.": "Microphone ready and muted.",
    "Micrófono desactivado.": "Microphone disabled.",
    "Micrófono detenido.": "Microphone stopped.",
    "Micrófono silenciado.": "Microphone muted.",
    "ffmpeg / icecast / micrófono": "ffmpeg / Icecast / microphone",
    "Micrófono al aire · sin señal de entrada.": "Microphone live · no input signal.",
    "Micrófono al aire · estabilizando señal.": "Microphone live · stabilizing input.",
    "El micrófono no está preparado. Detén la radio y revisa su configuración.":
      "The microphone is not ready. Stop the radio and check its configuration.",
    "Inicio": "Home",
    "Abrir menú": "Open menu",
    "Cerrar menú": "Close menu",
    "Navegación principal": "Main navigation",
    "Rau Connect": "Rau Connect",
    "Compartir y descubrir": "Share and discover",
    "Prepara tu identidad y el catálogo que luego viajará directamente entre dispositivos.":
      "Prepare your identity and the catalog that will later travel directly between devices.",
    "Fundación local · red todavía desactivada": "Local foundation · network still disabled",
    "Red P2P activa": "P2P network active",
    "Red P2P detenida": "P2P network stopped",
    "Red P2P iniciada. Ya puedes compartir tu ticket.": "P2P network started. You can now share your ticket.",
    "Red P2P detenida.": "P2P network stopped.",
    "Ticket de conexión copiado.": "Connection ticket copied.",
    "Conexión autenticada con {name} en {rtt} ms.": "Authenticated connection to {name} in {rtt} ms.",
    "El portapapeles no está disponible.": "The clipboard is not available.",
    "Tráfico P2P": "P2P traffic",
    "Conexiones Iroh autenticadas, directas cuando es posible y con relay como respaldo.":
      "Authenticated Iroh connections, direct when possible with relay fallback.",
    "Detener red": "Stop network",
    "Iniciar red": "Start network",
    "Endpoint activo": "Endpoint active",
    "Endpoint detenido": "Endpoint stopped",
    "Relay disponible": "Relay available",
    "Esperando relay": "Waiting for relay",
    "{count} dirección(es)": "{count} address(es)",
    "Mi ticket de conexión": "My connection ticket",
    "Puedes enviarlo como texto; el QR de emparejamiento contendrá este mismo ticket.":
      "You can send it as text; the pairing QR will contain this same ticket.",
    "Copiar": "Copy",
    "Inicia la red para generar un ticket alcanzable y aceptar conexiones de otros dispositivos.":
      "Start the network to generate a reachable ticket and accept connections from other devices.",
    "Conectar otro dispositivo": "Connect another device",
    "Pega su ticket para comprobar identidad, ruta de red y latencia real.":
      "Paste its ticket to verify identity, network route, and actual latency.",
    "Pega aquí el ticket Iroh del otro dispositivo…": "Paste the other device's Iroh ticket here…",
    "Conectando…": "Connecting…",
    "{name} respondió en {rtt} ms": "{name} replied in {rtt} ms",
    "Dispositivos conocidos": "Known devices",
    "Los dispositivos aparecerán aquí después de la primera conexión autenticada.":
      "Devices will appear here after the first authenticated connection.",
    "Conectado": "Online",
    "Offline": "Offline",
    "Última actividad: {date}": "Last activity: {date}",
    "Sin actividad registrada": "No recorded activity",
    "Actividad de red": "Network activity",
    "Las contraseñas no coinciden.": "Passwords do not match.",
    "Identidad P2P creada y desbloqueada para esta sesión.": "P2P identity created and unlocked for this session.",
    "Identidad P2P desbloqueada.": "P2P identity unlocked.",
    "Identidad P2P bloqueada.": "P2P identity locked.",
    "Selecciona una carpeta para compartir.": "Choose a folder to share.",
    "Carpeta indexada. Ya está lista para publicarse cuando activemos la red.":
      "Folder indexed. It is ready to publish when networking is enabled.",
    "Índice de carpeta actualizado.": "Folder index updated.",
    "Carpeta habilitada.": "Folder enabled.",
    "Carpeta pausada.": "Folder paused.",
    "¿Dejar de compartir “{name}”? Los archivos originales no se eliminarán.":
      "Stop sharing “{name}”? Original files will not be deleted.",
    "Carpeta quitada del catálogo compartido.": "Folder removed from the shared catalog.",
    "Identidad": "Identity",
    "Contactos": "Contacts",
    "Hay contactos conectados": "Contacts are online",
    "Sin contactos conectados": "No contacts online",
    "Catálogo compartido": "Shared catalog",
    "{count} carpeta(s)": "{count} folder(s)",
    "Identidad del dispositivo": "Device identity",
    "Bloquear": "Lock",
    "Nombre público": "Public name",
    "Crear identidad P2P": "Create P2P identity",
    "La clave privada se cifra con tu contraseña y se guarda dentro de SQLite. Rau no puede recuperar una contraseña olvidada.":
      "The private key is encrypted with your password and stored inside SQLite. Rau cannot recover a forgotten password.",
    "Contraseña": "Password",
    "Mínimo 10 caracteres": "At least 10 characters",
    "Confirmar contraseña": "Confirm password",
    "Crear identidad": "Create identity",
    "Desbloquear identidad": "Unlock identity",
    "La identidad de {name} está cifrada en este dispositivo.": "{name}'s identity is encrypted on this device.",
    "Desbloquear": "Unlock",
    "Compartir una carpeta": "Share a folder",
    "Carpeta local": "Local folder",
    "Ninguna carpeta seleccionada": "No folder selected",
    "Nombre visible": "Visible name",
    "Visibilidad": "Visibility",
    "Todos mis contactos": "All my contacts",
    "Contactos seleccionados": "Selected contacts",
    "Comunidad general": "General community",
    "Solo mediante invitación": "Invitation only",
    "Solo se publica una ruta virtual. Las rutas absolutas y los archivos ocultos no entran al catálogo.":
      "Only a virtual path is published. Absolute paths and hidden files are not included in the catalog.",
    "Indexar carpeta": "Index folder",
    "Carpetas compartidas": "Shared folders",
    "Todavía no has preparado carpetas para compartir.": "You have not prepared any folders to share yet.",
    "Reindexar": "Reindex",
    "Pausar": "Pause",
    "Habilitar": "Enable",
    "Indexada {date}": "Indexed {date}",
    "Vista previa del catálogo": "Catalog preview",
    "Valida ahora los resultados que recibiría un peer remoto.": "Validate the results a remote peer would receive.",
    "Buscar por nombre, carpeta o extensión…": "Search by name, folder, or extension…",
    "Busca sin texto para revisar hasta 100 archivos del catálogo habilitado.":
      "Search with an empty query to inspect up to 100 files from the enabled catalog.",
    "No se encontraron archivos compartidos.": "No shared files found.",
    "Comunidad": "Community",
    "Seleccionados": "Selected",
    "Invitación": "Invitation",
    "Tamaño": "Size",
    "Leyendo y validando la colección de Rekordbox…": "Reading and validating the Rekordbox collection…",
    "Preparando playlists y archivos convertidos…": "Preparing playlists and converted files…",
    "Mostrando {visible} de {total} hallazgos": "Showing {visible} of {total} findings",
    "Mostrar más": "Show more",
    "Abrir carpeta": "Open folder",
    "Actualizar status": "Refresh status",
    "API key": "API key",
    "Apariencia": "Appearance",
    "Anterior": "Previous",
    "Audio tools": "Audio tools",
    "Ano": "Year",
    "Atributos XML": "XML attributes",
    "Base de datos local": "Local database",
    "Chequeando": "Checking",
    "Claro": "Light",
    "Columnas": "Columns",
    "Comentarios": "Comments",
    "Conectando": "Connecting",
    "Rau Studio usa su motor integrado; estas rutas son opcionales.": "Rau Studio uses its bundled audio engine; these paths are optional.",
    "Configurar credenciales en Settings": "Configure credentials in Settings",
    "Configurada": "Configured",
    "Contraer": "Collapse",
    "Contraer player": "Collapse player",
    "Contraer status": "Collapse status",
    "Creditos de Rau Studio": "Rau Studio credits",
    "Creado por": "Created by",
    "Cargando rutas...": "Loading paths...",
    "Desktop": "Desktop",
    "Disponible": "Available",
    "Eliminar key": "Delete key",
    "Eliminar": "Delete",
    "Enter an OpenAI API key.": "Enter an OpenAI API key.",
    "Error": "Error",
    "Eventos OK": "Events OK",
    "Expandir": "Expand",
    "Expandir player": "Expand player",
    "Expandir status": "Expand status",
    "FFmpeg": "FFmpeg",
    "FFprobe": "FFprobe",
    "File Conversion": "File Conversion",
    "Guardar key": "Save key",
    "Guardar": "Save",
    "Guardando rating...": "Saving rating...",
    "Rating guardado en SQLite.": "Rating saved to SQLite.",
    "Rating del track": "Track rating",
    "Tu rating": "Your rating",
    "Sin rating": "Unrated",
    "Flojo": "Weak",
    "Esta bien": "Okay",
    "Bueno": "Good",
    "Muy bueno": "Very good",
    "Favorito": "Favorite",
    "Importado desde XML": "Imported from XML",
    "Quitar rating": "Clear rating",
    "Asignar 1 estrella": "Set 1 star",
    "Asignar {count} estrellas": "Set {count} stars",
    "Se guarda localmente sin modificar el XML original.": "Saved locally without changing the original XML.",
    "Guardar rutas": "Save paths",
    "Herramienta local para preparar audio, playlists y visuales sin depender de servicios externos.":
      "A local tool for preparing audio, playlists, and visuals without depending on external services.",
    "Idioma": "Language",
    "Idioma guardado: {language}": "Language saved: {language}",
    "Ingresa un OpenAI API key.": "Enter an OpenAI API key.",
    "Incluido con Rau Studio": "Bundled with Rau Studio",
    "Limpiar": "Clear",
    "Mastering": "Mastering",
    "Mostrar": "Show",
    "No configurada": "Not configured",
    "No instalado": "Not installed",
    "Motor de audio no disponible": "Audio engine unavailable",
    "Ocultar": "Hide",
    "OpenAI API key": "OpenAI API key",
    "Oscuro": "Dark",
    "Preferencias generales de Rau Studio.": "General Rau Studio preferences.",
    "Probar conexión": "Test connection",
    "Probando...": "Testing...",
    "Quien creo Rau Studio": "Who created Rau Studio",
    "Refrescar": "Refresh",
    "Resultados": "Results",
    "Rekordbox Convert": "Rekordbox Convert",
    "Revisando estado...": "Checking status...",
    "Ruta ffmpeg": "ffmpeg path",
    "Ruta ffprobe": "ffprobe path",
    "Ruta configurada": "Configured path",
    "Rutas": "Paths",
    "Rutas de herramientas guardadas.": "Tool paths saved.",
    "Rutas de herramientas restauradas a autodeteccion.": "Tool paths restored to autodetection.",
    "Reinstala Rau Studio o configura rutas manuales en Settings.": "Reinstall Rau Studio or configure manual paths in Settings.",
    "Settings": "Settings",
    "Lista": "Ready",
    "Requiere credencial": "Credential required",
    "Fuentes de enrichment": "Enrichment sources",
    "Credenciales cifradas y capacidades declaradas por cada fuente.":
      "Encrypted credentials and capabilities declared by each source.",
    "Credencial de {provider} guardada.": "{provider} credential saved.",
    "Credencial de {provider} eliminada.": "{provider} credential deleted.",
    "Siguiente": "Next",
    "Sin eventos todavia.": "No events yet.",
    "Source file not found": "Source file not found",
    "Status": "Status",
    "Terminal": "Terminal",
    "Turn": "Turn",
    "Usar defaults": "Use defaults",
    "Ver status": "View status",
    "Volumen": "Volume",
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
    "Agregar a playlist": "Add to playlist",
    "Agrega archivos o escanea una carpeta para empezar.": "Add files or scan a folder to get started.",
    "AI sin key": "AI without key",
    "Albums": "Albums",
    "Archivo": "File",
    "Archivos": "Files",
    "Artista": "Artist",
    "Artistas": "Artists",
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
    "Fecha XML": "XML date",
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
    "Metadata": "Metadata",
    "Mockups de discos girando en MP4": "Spinning record mockups in MP4",
    "Mostrar columnas": "Show columns",
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
    "Original": "Original",
    "Restaurar columnas": "Reset columns",
    "Original encontrado": "Original found",
    "Original no encontrado": "Original not found",
    "Originales": "Originals",
    "Pause": "Pause",
    "Pausar preview": "Pause preview",
    "Pendiente": "Pending",
    "Pendientes": "Pending",
    "Plan": "Plan",
    "Plan seleccionado": "Selected plan",
    "Play": "Play",
    "Player": "Player",
    "Play preview": "Play preview",
    "Playlist Browser": "Playlist Browser",
    "Playlist Copilot": "Playlist Copilot",
    "Playlist Library": "Playlist Library",
    "Playlists origen": "Source playlists",
    "Playlist origen": "Source playlist",
    "Playlist nueva": "New playlist",
    "Playlists": "Playlists",
    "Taxonomias": "Taxonomies",
    "Indexa un XML para visualizar generos, BPM y relaciones.": "Index an XML to visualize genres, BPM, and relationships.",
    "Sin librerias indexadas": "No indexed libraries",
    "Primero indexa una libreria en Playlist Library para crear taxonomias locales.":
      "Index a library in Playlist Library first to create local taxonomies.",
    "Aun no hay interpretacion. Escribe un brief y genera sugerencias.":
      "No interpretation yet. Write a brief and generate suggestions.",
    "Aceptar todo": "Accept all",
    "Afterhours": "Afterhours",
    "Armonía por key": "Harmony by key",
    "Arriba, luminoso, con emocion.": "Bright, emotional, and lifted.",
    "Arriesgada": "Adventurous",
    "Artistas ancla": "Artist anchors",
    "Before I refine this further, answer this one thing.": "Before I refine this further, answer this one thing.",
    "Balanceada": "Balanced",
    "BPM cerrado": "Tight BPM",
    "Brief guiado": "Guided brief",
    "Brief interactivo": "Interactive brief",
    "Buscando candidatos con el brief completo...": "Searching candidates with the complete brief...",
    "Building a complete pass...": "Building a complete pass...",
    "Cantidad": "Count",
    "Candidatos": "Candidates",
    "Calido, bailable, no agresivo.": "Warm, danceable, not aggressive.",
    "Cobertura": "Coverage",
    "Club peak": "Club peak",
    "Continuando": "Continuing",
    "Construye alrededor de nombres de referencia.": "Build around reference artists.",
    "Curada, pero con sorpresas.": "Curated, but with surprises.",
    "Curva progresiva": "Progressive curve",
    "Dark & hypnotic": "Dark & hypnotic",
    "Describe la playlist que quieres generar.": "Describe the playlist you want to generate.",
    "Deseleccionar todo": "Deselect all",
    "Ej: 30 tracks deep house, 120-124 BPM, vocales calidas, sin peak time.":
      "Example: 30 deep house tracks, 120-124 BPM, warm vocals, no peak time.",
    "El Copilot mostrara aqui los tracks sugeridos.": "The Copilot will show suggested tracks here.",
    "El Copilot interpreta tu mensaje, infiere lo que ya respondiste y pregunta solo el siguiente dato util.":
      "The Copilot interprets your message, infers what you already answered, and asks only the next useful detail.",
    "Energia": "Energy",
    "Escucha editorial": "Editorial listening",
    "Escribe otra respuesta para la pregunta actual.": "Write another answer for the current question.",
    "Excluir": "Exclude",
    "Evitar repetidos": "Avoid repeats",
    "El Copilot decide segun el brief inicial.": "The Copilot decides from the initial brief.",
    "Funciona con ranking local si no hay API key o embeddings.":
      "Works with local ranking if there is no API key or embeddings.",
    "Genera playlists sugeridas desde tu XML indexado.": "Generate suggested playlists from your indexed XML.",
    "Generando": "Generating",
    "Generando sugerencias": "Generating suggestions",
    "Generar sugerencias": "Generate suggestions",
    "Genero primero": "Genre first",
    "Busca coherencia por escena o estilo.": "Search for scene or style coherence.",
    "Hora completa, buen balance.": "Full hour, good balance.",
    "Interpretacion": "Interpretation",
    "Interpretacion AI activa con OpenAI.": "AI interpretation active with OpenAI.",
    "Estoy razonando el brief por pasos antes de buscar.": "I am reasoning through the brief step by step before searching.",
    "Esto inferi del brief antes de hacer la siguiente pregunta.":
      "This is what I inferred from the brief before asking the next question.",
    "Esto inferi del brief antes de rankear candidatos.":
      "This is what I inferred from the brief before ranking candidates.",
    "Ideal para transiciones entre mundos.": "Ideal for transitions between worlds.",
    "Inicio de noche, groove claro, sin peak time.": "Start of the night, clear groove, no peak-time pressure.",
    "Intro, desarrollo, peak y salida.": "Intro, build, peak, and exit.",
    "Joyas escondidas": "Hidden gems",
    "Mantengo compatibilidad armonica por key?": "Should I keep harmonic compatibility by key?",
    "Mas denso, mental y nocturno.": "Denser, more mental, and late-night.",
    "Mas discovery, menos obvio.": "More discovery, less obvious.",
    "Mas profundo, nocturno y con espacio.": "Deeper, late-night, and spacious.",
    "Mas personalidad y descubrimiento.": "More personality and discovery.",
    "Mas variedad y aire.": "More variety and breathing room.",
    "Mezcla mas estable.": "More stable mixing.",
    "Mini set o bloque corto.": "Mini set or short block.",
    "Momento alto, energia firme y mezcla directa.": "High point, firm energy, direct mixing.",
    "Por que estos tracks": "Why these tracks",
    "Paso 1/6: primero ubico el contexto de uso.": "Step 1/6: first I place the usage context.",
    "Paso 2/6: defino el largo para no sobrecurar ni quedarme corto.": "Step 2/6: I define the length so the curation is neither too broad nor too thin.",
    "Paso 3/6: ahora fijo la direccion musical para buscar con intencion.": "Step 3/6: now I set the musical direction before searching.",
    "Paso 4/6: selecciono el mood para que la lista tenga caracter.": "Step 4/6: I choose the mood so the playlist has character.",
    "Paso 5/6: ajusto restricciones de mezcla para que funcione en cabina.": "Step 5/6: I tune DJ constraints so it works in the booth.",
    "Paso 6/6: cierro el nivel de riesgo antes de buscar candidatos.": "Step 6/6: I set the risk level before searching candidates.",
    "Paso {current}/{total}": "Step {current}/{total}",
    "Perfecto. Ya tengo el brief completo; ahora busco candidatos y preparo la interpretacion.":
      "Perfect. I have the complete brief; now I will search candidates and prepare the interpretation.",
    "Pocos riesgos, funciona rapido.": "Low risk, works quickly.",
    "Para escuchar, curar y descubrir.": "For listening, curating, and discovering.",
    "Preguntas sugeridas": "Suggested questions",
    "Preguntar todo": "Ask every step",
    "Primero indexa una libreria XML.": "Index an XML library first.",
    "Primero indexa una libreria en Playlist Library para usar Playlist Copilot.":
      "Index a library in Playlist Library first to use Playlist Copilot.",
    "Seleccionar todo": "Select all",
    "Set extendido con narrativa.": "Extended set with a narrative arc.",
    "Raw & weird": "Raw & weird",
    "Raro, sucio, con personalidad.": "Odd, raw, and full of character.",
    "Respeta el contador del formulario.": "Use the form counter.",
    "Responde la pregunta actual o elige una opcion.": "Answer the current question or choose an option.",
    "Sin candidatos todavia.": "No candidates yet.",
    "Sin filtro": "No filter",
    "Usa tu criterio": "Use your judgment",
    "Usar cantidad actual": "Use current count",
    "Transiciones armonicas.": "Harmonic transitions.",
    "Uplifting": "Uplifting",
    "Voy a construir el brief por etapas. Te preguntare una cosa a la vez y despues buscare candidatos en tu libreria.":
      "I will build the brief in stages. I will ask one thing at a time, then search candidates in your library.",
    "Warm & groovy": "Warm & groovy",
    "Warmup DJ": "DJ warmup",
    "¿Cuánto debe durar o cuántos tracks necesitas?": "How long should it be, or how many tracks do you need?",
    "¿Para qué contexto es la playlist?": "What context is the playlist for?",
    "¿Qué dirección musical debería mandar?": "What musical direction should lead?",
    "¿Qué mood o energía quieres?": "What mood or energy do you want?",
    "¿Qué regla DJ debería respetar más?": "Which DJ rule should matter most?",
    "¿Qué tan aventurada debe ser la selección?": "How adventurous should the selection be?",
    "{count} respuesta(s) capturadas": "{count} answer(s) captured",
    "{selected}/{total} tracks seleccionados": "{selected}/{total} selected tracks",
    "{tracks} tracks · {playlists} playlists · {embeddings} vectores":
      "{tracks} tracks · {playlists} playlists · {embeddings} vectors",
    "Quieres abrir criterios o incluir tracks con metadata incompleta?":
      "Should I widen criteria or include tracks with incomplete metadata?",
    "Quieres acotar un rango BPM?": "Do you want to narrow the BPM range?",
    "Quieres priorizar algun genero o subgenero?": "Do you want to prioritize a genre or subgenre?",
    "Generos": "Genres",
    "Grafo": "Graph",
    "BPM conocido": "Known BPM",
    "Archivos faltantes": "Missing files",
    "Top generos": "Top genres",
    "Promedio": "Average",
    "Calidad metadata": "Metadata quality",
    "Distribucion de generos": "Genre distribution",
    "Formatos": "Formats",
    "Anos": "Years",
    "Rangos de BPM": "BPM ranges",
    "Relaciones": "Relationships",
    "Genero, BPM y key conectados por co-ocurrencia. Haz click en un nodo para ver tracks.":
      "Genre, BPM, and key connected by co-occurrence. Click a node to view tracks.",
    "Ajustar": "Fit",
    "nodos": "nodes",
    "relaciones": "relationships",
    "{count} tracks en esta taxonomia": "{count} tracks in this taxonomy",
    "Haz click en una barra o nodo para explorar tracks.": "Click a bar or node to explore tracks.",
    "Cargando tracks": "Loading tracks",
    "Sin datos para mostrar.": "No data to display.",
    "Sin genero": "No genre",
    "Sin BPM": "No BPM",
    "Sin key": "No key",
    "Sin ano": "No year",
    "Formato desconocido": "Unknown format",
    "Archivo no encontrado": "File not found",
    "Preflight de conversion": "Conversion preflight",
    "Presets": "Presets",
    "Procesando": "Processing",
    "Progreso": "Progress",
    "Progreso de indexacion": "Indexing progress",
    "Referencia": "Reference",
    "Refs rotas": "Broken refs",
    "Reporte": "Report",
    "Revisa permisos del archivo o carpeta.": "Check the file or folder permissions.",
    "Resumen general": "General summary",
    "Agregar": "Add",
    "Agregar {count} tracks": "Add {count} tracks",
    "Agregar playlist": "Add playlist",
    "Agrega tracks desde la busqueda o desde una playlist origen.": "Add tracks from search or from a source playlist.",
    "Al buscar, envia solo el texto de busqueda a OpenAI para generar un embedding temporal y compara contra vectores guardados en SQLite local. No reindexa tracks.":
      "When searching, it sends only the search text to OpenAI to generate a temporary embedding and compares it against vectors stored in local SQLite. It does not reindex tracks.",
    "Busca tracks indexados o deja la busqueda vacia para listar.": "Search indexed tracks or leave search empty to list tracks.",
    "Buscar": "Search",
    "Buscar album": "Search album",
    "Buscar artista": "Search artist",
    "Buscar dentro del artista": "Search inside artist",
    "Buscar dentro del grupo": "Search inside group",
    "Buscar por titulo, artista, album, mood...": "Search by title, artist, album, mood...",
    "Cancelar": "Cancel",
    "{count} archivos no encontrados": "{count} missing files",
    "Crea o selecciona una playlist.": "Create or select a playlist.",
    "Crear playlist": "Create playlist",
    "Crear y agregar {count} tracks": "Create and add {count} tracks",
    "Descripcion opcional": "Optional description",
    "Descripcion": "Description",
    "Deseleccionar": "Deselect",
    "Drafts": "Drafts",
    "Duracion": "Duration",
    "Elegir XML": "Choose XML",
    "Elige una playlist origen.": "Choose a source playlist.",
    "Elige un XML para revisar sus playlists antes de indexar.": "Choose an XML to review its playlists before indexing.",
    "Eliminar indice": "Delete index",
    "Eliminar libreria indexada": "Delete indexed library",
    "Eliminar track": "Delete track",
    "Eliminar {count} tracks": "Delete {count} tracks",
    "Eliminar {count} indices": "Delete {count} indexes",
    "Eliminando": "Deleting",
    "Limpiando": "Cleaning",
    "Limpiar los no encontrados de la colección": "Clean missing files from the collection",
    "Embeddings listos: {count} generados con {model}.": "Embeddings ready: {count} generated with {model}.",
    "En cola": "Queued",
    "Esto elimina el indice SQLite de esta libreria, sus playlists, vectores y drafts. No elimina archivos de audio ni modifica el XML original.":
      "This deletes this library's SQLite index, playlists, vectors, and drafts. It does not delete audio files or modify the original XML.",
    "Esto elimina el indice SQLite de las playlists seleccionadas. No elimina archivos de audio ni modifica el XML original.":
      "This deletes the SQLite index for the selected playlists. It does not delete audio files or modify the original XML.",
    "Esto elimina los tracks seleccionados del indice SQLite, sus vectores y referencias en playlists/drafts locales. No elimina archivos de audio ni modifica el XML original.":
      "This deletes the selected tracks from the SQLite index, their vectors, and local playlist/draft references. It does not delete audio files or modify the original XML.",
    "Esto elimina del indice SQLite los tracks cuyo archivo no fue encontrado, junto con sus vectores y referencias locales. No elimina archivos de audio ni modifica el XML original.":
      "This deletes tracks whose files were not found from the SQLite index, along with their vectors and local references. It does not delete audio files or modify the original XML.",
    "Exportando XML": "Exporting XML",
    "Exportar": "Export",
    "Existente": "Existing",
    "Balanced": "Balanced",
    "Candidate set": "Candidate set",
    "Decision trace": "Decision trace",
    "Discovery mode": "Discovery mode",
    "Energy ramp": "Energy ramp",
    "Flat warmup": "Flat warmup",
    "Flexible tempo": "Flexible tempo",
    "Generando embeddings de tracks.": "Generating track embeddings.",
    "Genera embeddings de metadata de tracks con OpenAI y los guarda en SQLite local. No sube audio; solo texto como titulo, artista, album, playlists y location.":
      "Generates track metadata embeddings with OpenAI and stores them in local SQLite. It does not upload audio; only text like title, artist, album, playlists, and location.",
    "Guided choices": "Guided choices",
    "Ignore key": "Ignore key",
    "Indexa un XML para empezar.": "Index an XML to start.",
    "Indexada": "Indexed",
    "Indexando": "Indexing",
    "Indexando XML de Rekordbox.": "Indexing Rekordbox XML.",
    "Indexar": "Index",
    "Indexar XML": "Index XML",
    "Indexar todo": "Index all",
    "Indexar {count} playlists": "Index {count} playlists",
    "Indexar {count} vectores": "Index {count} vectors",
    "Indexar vector": "Index vector",
    "Indexar vectores": "Index vectors",
    "Indice actualizado: {tracks} tracks, {playlists} playlists.": "Index updated: {tracks} tracks, {playlists} playlists.",
    "Indice eliminado: {name}": "Index deleted: {name}",
    "Indice de playlists actualizado.": "Playlist index updated.",
    "Indices eliminados: {count}": "Indexes deleted: {count}",
    "Indexando playlists y relaciones.": "Indexing playlists and relations.",
    "Indexando tracks en SQLite.": "Indexing tracks in SQLite.",
    "Librerias": "Libraries",
    "Modo Vector": "Vector mode",
    "No hay playlists nuevas. Crea una playlist para agregar estos tracks.": "There are no new playlists. Create one to add these tracks.",
    "No se pudo controlar el player": "Could not control the player",
    "No se pudo reproducir": "Could not play",
    "Nueva playlist": "New playlist",
    "Nueva": "New",
    "Loose key flow": "Loose key flow",
    "More known artists": "More known artists",
    "Nombre": "Name",
    "Playlist": "Playlist",
    "Playlist creada: {name}": "Playlist created: {name}",
    "Playlist creada: {name} con {count} tracks.": "Playlist created: {name} with {count} tracks.",
    "Playlists del XML": "XML playlists",
    "Playlist sin tracks.": "Playlist has no tracks.",
    "Preparando indice SQLite.": "Preparing SQLite index.",
    "Quitar": "Remove",
    "Reconstruyendo indice de busqueda FTS.": "Rebuilding FTS search index.",
    "Reasoning": "Reasoning",
    "Reasoning summary": "Reasoning summary",
    "Score": "Score",
    "Seleccionar": "Select",
    "Slow build": "Slow build",
    "Sin libreria activa": "No active library",
    "Sin playlists indexadas": "No indexed playlists",
    "Sin playlists nuevas.": "No new playlists.",
    "Sin vector": "No vector",
    "Sin XML indexado": "No indexed XML",
    "Strict key flow": "Strict key flow",
    "Stop": "Stop",
    "Se agregaran {count} tracks seleccionados.": "{count} selected tracks will be added.",
    "Track indexado en SQLite": "Track indexed in SQLite",
    "Tracks eliminados del indice: {count}": "Tracks deleted from index: {count}",
    "Se limpiaron {count} archivos no encontrados de la colección.": "Cleaned {count} missing files from the collection.",
    "Usar embeddings si estan disponibles.": "Use embeddings when available.",
    "Vector": "Vector",
    "Vector %": "Vector %",
    "Vector en cola": "Vector queued",
    "Vector generandose": "Vector generating",
    "Vector indexado": "Vector indexed",
    "Vector listo": "Vector ready",
    "Vector pendiente": "Vector pending",
    "Vectores": "Vectors",
    "Vectores listos": "Vectors ready",
    "XML exportado: {count} tracks.": "XML exported: {count} tracks.",
    "XML cargado: {tracks} tracks, {playlists} playlists. Elige que indexar.":
      "XML loaded: {tracks} tracks, {playlists} playlists. Choose what to index.",
    "{count} resultados.": "{count} results.",
    "{count} tracks en la playlist.": "{count} tracks in the playlist.",
    "+ {count} mas": "+ {count} more",
    "{processed} de {total}": "{processed} of {total}",
    "{tracks} tracks en coleccion · {playlists} playlists disponibles":
      "{tracks} tracks in collection · {playlists} playlists available",
    "Selecciona play en una fila.": "Select play on a row.",
    "Selecciona un track para escucharlo.": "Select a track to listen.",
    "Seleccionadas:": "Selected:",
    "Severidad": "Severity",
    "Si no eliges ninguna, se planifica toda la libreria.": "If none are selected, the full library is planned.",
    "Si el archivo esta en un disco externo, permite a Rau Studio acceder a Volumenes extraibles o agregalo a Acceso total al disco en macOS. Tambien revisa que el disco no este en solo lectura.":
      "If the file is on an external drive, allow Rau Studio to access Removable Volumes or add it to Full Disk Access on macOS. Also verify that the drive is not read-only.",
    "Sin artista": "No artist",
    "Sin album": "No album",
    "Sin albums.": "No albums.",
    "Sin archivo cargado": "No file loaded",
    "Sin archivo seleccionado": "No file selected",
    "Sin carpeta activa": "No active folder",
    "Sin carpeta seleccionada": "No folder selected",
    "Sin jobs.": "No jobs.",
    "Sin masters todavia": "No masters yet",
    "Sin metadata": "No metadata",
    "Sin playlist seleccionada": "No playlist selected",
    "Sin resultados.": "No results.",
    "Sin tracks.": "No tracks.",
    "Sin videos todavia": "No videos yet",
    "Sin XML cargado": "No XML loaded",
    "Selecciona un grupo": "Select a group",
    "Tamano": "Size",
    "Tema": "Track",
    "Titulo": "Title",
    "Titulos": "Titles",
    "Titulos sugeridos": "Suggested titles",
    "Todavia no hay senales fuertes; por eso pregunto el siguiente dato.":
      "There are no strong signals yet, so I am asking for the next detail.",
    "Thinking through the next step...": "Thinking through the next step...",
    "Todos": "All",
    "Todos los archivos": "All files",
    "Todavía no hay grupos. Abre una carpeta o selecciona archivos para crear uno.":
      "There are no groups yet. Open a folder or select files to create one.",
    "Velocidad": "Speed",
    "Ver detalle": "View detail",
    "Vas a agregar {count} tracks.": "You are adding {count} tracks.",
    "Visual": "Visual",
    "XML recientes": "Recent XML",
    "{count} archivo(s)": "{count} file(s)",
    "{count} archivo(s) en la importación actual": "{count} file(s) in the current import",
    "albums": "albums",
    "grupos": "groups",
    "Seleccion actual": "Current selection",
    "tracks seleccionados": "selected tracks",
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

type I18nContextRegistry = typeof globalThis & {
  __RAU_STUDIO_I18N_CONTEXT__?: Context<I18nContextValue | null>;
};

const i18nContextRegistry = globalThis as I18nContextRegistry;
const I18nContext = i18nContextRegistry.__RAU_STUDIO_I18N_CONTEXT__
  ?? createContext<I18nContextValue | null>(null);
i18nContextRegistry.__RAU_STUDIO_I18N_CONTEXT__ = I18nContext;

const fallbackLocale = safeInitialLocale();
const fallbackI18n: I18nContextValue = {
  locale: fallbackLocale,
  setLocale: async (locale) => {
    try {
      localStorage.setItem(localeStorageKey, normalizeLocale(locale));
    } catch {
      // The root provider will restore persistence when the webview is ready.
    }
  },
  t: (key, values) => translate(fallbackLocale, key, values)
};

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
  return useContext(I18nContext) ?? fallbackI18n;
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
    [/^Conectando con (.+)\.\.\.$/, (match) => `Connecting to ${match[1]}...`],
    [/^En vivo: (.+)$/, (match) => `Live: ${match[1]}`],
    [/^Saltada: (.+)$/, (match) => `Skipped: ${match[1]}`],
    [/^Reproducida: (.+)$/, (match) => `Played: ${match[1]}`],
    [/^Icecast desconectado\. Reintentando en (\d+)s: (.+)$/, (match) => `Icecast disconnected. Retrying in ${match[1]}s: ${match[2]}`],
    [/^Icecast rechazo metadata con HTTP (.+)\.$/, (match) => `Icecast rejected metadata with HTTP ${match[1]}.`],
    [/^No se pudo actualizar metadata Icecast: (.+)$/, (match) => `Could not update Icecast metadata: ${match[1]}`],
    [/^Conversion local iniciada: (\d+) archivo\(s\), concurrencia maxima (\d+)$/, (match) => `Local conversion started: ${match[1]} file(s), max concurrency ${match[2]}`],
    [/^Conversion local terminada: (\d+) convertidos, (\d+) existentes, (\d+) AIFF originales, (\d+) errores$/, (match) => `Local conversion finished: ${match[1]} converted, ${match[2]} existing, ${match[3]} original AIFF, ${match[4]} errors`],
    [/^Conversion iniciada: (\d+) track\(s\), concurrencia maxima (\d+)$/, (match) => `Conversion started: ${match[1]} track(s), max concurrency ${match[2]}`],
    [/^Conversion terminada: (\d+) convertidos, (\d+) existentes, (\d+) AIFF originales, (\d+) errores$/, (match) => `Conversion finished: ${match[1]} converted, ${match[2]} existing, ${match[3]} original AIFF, ${match[4]} errors`],
    [/^Reutilizando AIFF existente: (.+)$/, (match) => `Reusing existing AIFF: ${match[1]}`],
    [/^Conversion completada: (.+)$/, (match) => `Conversion completed: ${match[1]}`],
    [/^ffmpeg iniciado: (.+) -> (.+)$/, (match) => `ffmpeg started: ${match[1]} -> ${match[2]}`],
    [/^Archivo local no encontrado en SQLite: (.+)$/, (match) => `Local file not found in SQLite: ${match[1]}`],
    [/^Indices de playlists eliminados: (\d+)$/, (match) => `Playlist indexes deleted: ${match[1]}`],
    [/^Tracks indexados eliminados: (\d+)$/, (match) => `Indexed tracks deleted: ${match[1]}`],
    [/^Indexando playlist: (.+)$/, (match) => `Indexing playlist: ${match[1]}`],
    [/^Indice de libreria eliminado\.$/, () => "Library index deleted."],
    [/^Playlist indexada: (.+)$/, (match) => `Playlist indexed: ${match[1]}`],
    [/^Generando embedding: (.+)$/, (match) => `Generating embedding: ${match[1]}`],
    [/^Embedding listo: (.+)$/, (match) => `Embedding ready: ${match[1]}`],
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

function safeInitialLocale(): Locale {
  try {
    return detectInitialLocale();
  } catch {
    return defaultLocale;
  }
}

function interpolate(template: string, values?: TranslationValues) {
  if (!values) return template;
  return template.replace(/\{([^}]+)\}/g, (_, key: string) => String(values[key] ?? ""));
}
