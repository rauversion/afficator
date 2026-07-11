use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const DB_FILE: &str = "aifficator.sqlite3";
const SECRET_FILE: &str = "settings-secret.bin";
const OPENAI_API_KEY_SETTING: &str = "openai_api_key";
const FFMPEG_PATH_SETTING: &str = "ffmpeg_path";
const FFPROBE_PATH_SETTING: &str = "ffprobe_path";
const LANGUAGE_SETTING: &str = "language";
const CIPHER_VERSION: u8 = 1;
const SECRET_LEN: usize = 32;

#[derive(Debug, Serialize)]
pub struct OpenAiApiKeyStatus {
    configured: bool,
    preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioToolPaths {
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AudioToolSettings {
    ffmpeg_path: Option<String>,
    ffprobe_path: Option<String>,
    default_ffmpeg_paths: Vec<String>,
    default_ffprobe_paths: Vec<String>,
    database_path: String,
    database_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageOption {
    value: String,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageSettings {
    language: String,
    available_languages: Vec<LanguageOption>,
}

pub(crate) fn get_language_settings(app: &AppHandle) -> Result<LanguageSettings, String> {
    language_settings(load_language(app)?)
}

pub(crate) fn save_language_settings(
    app: &AppHandle,
    language: String,
) -> Result<LanguageSettings, String> {
    let language = normalize_language(&language).ok_or_else(|| {
        localized(
            app,
            "Idioma no soportado. Usa es o en.",
            "Unsupported language. Use es or en.",
        )
    })?;
    save_optional_text_setting(app, LANGUAGE_SETTING, Some(language.clone()))?;
    language_settings(language)
}

pub(crate) fn load_language(app: &AppHandle) -> Result<String, String> {
    let language = load_text_setting(app, LANGUAGE_SETTING)?;
    Ok(language
        .as_deref()
        .and_then(normalize_language)
        .unwrap_or_else(|| "es".to_string()))
}

pub(crate) fn localized(app: &AppHandle, spanish: &str, english: &str) -> String {
    match load_language(app) {
        Ok(language) if language == "en" => english.to_string(),
        _ => spanish.to_string(),
    }
}

pub(crate) fn get_openai_api_key_status(app: &AppHandle) -> Result<OpenAiApiKeyStatus, String> {
    match load_openai_api_key(app)? {
        Some(api_key) if !api_key.trim().is_empty() => Ok(openai_key_status(&api_key)),
        _ => Ok(OpenAiApiKeyStatus {
            configured: false,
            preview: None,
        }),
    }
}

pub(crate) fn save_openai_api_key(
    app: &AppHandle,
    api_key: String,
) -> Result<OpenAiApiKeyStatus, String> {
    let api_key = api_key.trim();

    if api_key.is_empty() {
        return Err(localized(
            app,
            "Ingresa un OpenAI API key.",
            "Enter an OpenAI API key.",
        ));
    }

    let conn = open_settings_db(app)?;
    let encrypted = encrypt_setting(app, OPENAI_API_KEY_SETTING, api_key.as_bytes())?;
    conn.execute(
        "INSERT INTO app_settings (key, value_ciphertext, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_ciphertext = excluded.value_ciphertext, updated_at = excluded.updated_at",
        params![OPENAI_API_KEY_SETTING, encrypted, timestamp()],
    )
    .map_err(|error| format!("No se pudo guardar OpenAI API key cifrada: {error}"))?;

    Ok(openai_key_status(api_key))
}

pub(crate) fn clear_openai_api_key(app: &AppHandle) -> Result<OpenAiApiKeyStatus, String> {
    let conn = open_settings_db(app)?;
    conn.execute(
        "DELETE FROM app_settings WHERE key = ?1",
        params![OPENAI_API_KEY_SETTING],
    )
    .map_err(|error| format!("No se pudo borrar OpenAI API key: {error}"))?;

    Ok(OpenAiApiKeyStatus {
        configured: false,
        preview: None,
    })
}

pub(crate) fn get_audio_tool_settings(app: &AppHandle) -> Result<AudioToolSettings, String> {
    let paths = load_audio_tool_paths(app)?;
    Ok(AudioToolSettings {
        ffmpeg_path: paths.ffmpeg_path,
        ffprobe_path: paths.ffprobe_path,
        default_ffmpeg_paths: default_binary_paths("ffmpeg"),
        default_ffprobe_paths: default_binary_paths("ffprobe"),
        database_path: settings_db_path(app)?.to_string_lossy().into_owned(),
        database_dir: app_data_dir(app)?.to_string_lossy().into_owned(),
    })
}

pub(crate) fn save_audio_tool_settings(
    app: &AppHandle,
    ffmpeg_path: Option<String>,
    ffprobe_path: Option<String>,
) -> Result<AudioToolSettings, String> {
    save_optional_text_setting(
        app,
        FFMPEG_PATH_SETTING,
        normalize_path_setting(ffmpeg_path),
    )?;
    save_optional_text_setting(
        app,
        FFPROBE_PATH_SETTING,
        normalize_path_setting(ffprobe_path),
    )?;
    get_audio_tool_settings(app)
}

pub(crate) fn load_audio_tool_paths(app: &AppHandle) -> Result<AudioToolPaths, String> {
    Ok(AudioToolPaths {
        ffmpeg_path: load_text_setting(app, FFMPEG_PATH_SETTING)?,
        ffprobe_path: load_text_setting(app, FFPROBE_PATH_SETTING)?,
    })
}

pub(crate) fn load_openai_api_key(app: &AppHandle) -> Result<Option<String>, String> {
    let conn = open_settings_db(app)?;
    let encrypted = conn
        .query_row(
            "SELECT value_ciphertext FROM app_settings WHERE key = ?1",
            params![OPENAI_API_KEY_SETTING],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer OpenAI API key cifrada: {error}"))?;

    let Some(encrypted) = encrypted else {
        return Ok(None);
    };

    let decrypted = decrypt_setting(app, OPENAI_API_KEY_SETTING, &encrypted)?;
    let api_key = String::from_utf8(decrypted)
        .map_err(|error| format!("OpenAI API key descifrada no es UTF-8 valido: {error}"))?;

    if api_key.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(api_key))
    }
}

fn load_text_setting(app: &AppHandle, key: &str) -> Result<Option<String>, String> {
    let conn = open_settings_db(app)?;
    let encrypted = conn
        .query_row(
            "SELECT value_ciphertext FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("No se pudo leer setting {key}: {error}"))?;

    let Some(encrypted) = encrypted else {
        return Ok(None);
    };

    let decrypted = decrypt_setting(app, key, &encrypted)?;
    let value = String::from_utf8(decrypted)
        .map_err(|error| format!("Setting {key} descifrado no es UTF-8 valido: {error}"))?;
    let value = value.trim().to_string();

    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn save_optional_text_setting(
    app: &AppHandle,
    key: &str,
    value: Option<String>,
) -> Result<(), String> {
    let conn = open_settings_db(app)?;

    let Some(value) = value else {
        conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])
            .map_err(|error| format!("No se pudo borrar setting {key}: {error}"))?;
        return Ok(());
    };

    let encrypted = encrypt_setting(app, key, value.as_bytes())?;
    conn.execute(
        "INSERT INTO app_settings (key, value_ciphertext, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_ciphertext = excluded.value_ciphertext, updated_at = excluded.updated_at",
        params![key, encrypted, timestamp()],
    )
    .map_err(|error| format!("No se pudo guardar setting {key}: {error}"))?;

    Ok(())
}

fn normalize_path_setting(value: Option<String>) -> Option<String> {
    value
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty() && !path.eq_ignore_ascii_case("auto"))
}

pub(crate) fn default_binary_paths(name: &str) -> Vec<String> {
    default_binary_dirs()
        .iter()
        .map(|directory| format!("{directory}/{name}"))
        .chain(std::iter::once(name.to_string()))
        .collect()
}

fn default_binary_dirs() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
            "/usr/bin",
            "/bin",
        ]
    }

    #[cfg(target_os = "linux")]
    {
        &["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"]
    }

    #[cfg(target_os = "windows")]
    {
        &[
            "C:/ProgramData/chocolatey/bin",
            "C:/ffmpeg/bin",
            "C:/Program Files/ffmpeg/bin",
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        &[]
    }
}

fn open_settings_db(app: &AppHandle) -> Result<Connection, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let conn = Connection::open(dir.join(DB_FILE))
        .map_err(|error| format!("No se pudo abrir SQLite settings: {error}"))?;
    init_settings_db(&conn)?;
    Ok(conn)
}

fn init_settings_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS app_settings (
          key TEXT PRIMARY KEY,
          value_ciphertext TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        ",
    )
    .map_err(|error| format!("No se pudo inicializar SQLite settings: {error}"))
}

fn encrypt_setting(app: &AppHandle, setting_key: &str, plaintext: &[u8]) -> Result<String, String> {
    let secret = load_or_create_secret(app)?;
    let key = aead_key(&secret)?;
    let nonce_bytes = random_nonce()?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = plaintext.to_vec();

    key.seal_in_place_append_tag(nonce, Aad::from(setting_key.as_bytes()), &mut in_out)
        .map_err(|_| "No se pudo cifrar setting.".to_string())?;

    let mut blob = Vec::with_capacity(1 + NONCE_LEN + in_out.len());
    blob.push(CIPHER_VERSION);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&in_out);

    Ok(BASE64.encode(blob))
}

fn decrypt_setting(app: &AppHandle, setting_key: &str, encrypted: &str) -> Result<Vec<u8>, String> {
    let secret = load_or_create_secret(app)?;
    let key = aead_key(&secret)?;
    let blob = BASE64
        .decode(encrypted)
        .map_err(|error| format!("Setting cifrado tiene base64 invalido: {error}"))?;

    if blob.len() <= 1 + NONCE_LEN {
        return Err("Setting cifrado esta incompleto.".to_string());
    }

    if blob[0] != CIPHER_VERSION {
        return Err("Version de cifrado de settings no soportada.".to_string());
    }

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&blob[1..1 + NONCE_LEN]);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = blob[1 + NONCE_LEN..].to_vec();
    let decrypted = key
        .open_in_place(nonce, Aad::from(setting_key.as_bytes()), &mut in_out)
        .map_err(|_| "No se pudo descifrar setting.".to_string())?;

    Ok(decrypted.to_vec())
}

fn aead_key(secret: &[u8; SECRET_LEN]) -> Result<LessSafeKey, String> {
    let unbound = UnboundKey::new(&AES_256_GCM, secret)
        .map_err(|_| "Secreto de settings invalido.".to_string())?;
    Ok(LessSafeKey::new(unbound))
}

fn load_or_create_secret(app: &AppHandle) -> Result<[u8; SECRET_LEN], String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|error| format!("No se pudo crear app data dir: {error}"))?;
    let path = dir.join(SECRET_FILE);

    if path.is_file() {
        return read_secret(path);
    }

    let mut secret = [0u8; SECRET_LEN];
    SystemRandom::new()
        .fill(&mut secret)
        .map_err(|_| "No se pudo generar secreto local de settings.".to_string())?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    match options.open(&path) {
        Ok(mut file) => {
            file.write_all(&secret).map_err(|error| {
                format!("No se pudo escribir secreto local de settings: {error}")
            })?;
            Ok(secret)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_secret(path),
        Err(error) => Err(format!(
            "No se pudo crear secreto local de settings: {error}"
        )),
    }
}

fn read_secret(path: PathBuf) -> Result<[u8; SECRET_LEN], String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("No se pudo leer secreto local de settings: {error}"))?;
    if bytes.len() != SECRET_LEN {
        return Err("Secreto local de settings tiene tamano invalido.".to_string());
    }

    let mut secret = [0u8; SECRET_LEN];
    secret.copy_from_slice(&bytes);
    Ok(secret)
}

fn random_nonce() -> Result<[u8; NONCE_LEN], String> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "No se pudo generar nonce de settings.".to_string())?;
    Ok(nonce)
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("No se pudo resolver app data dir: {error}"))
}

fn settings_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(DB_FILE))
}

fn language_settings(language: String) -> Result<LanguageSettings, String> {
    Ok(LanguageSettings {
        language,
        available_languages: vec![
            LanguageOption {
                value: "es".to_string(),
                label: "Español".to_string(),
            },
            LanguageOption {
                value: "en".to_string(),
                label: "English".to_string(),
            },
        ],
    })
}

fn normalize_language(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "es" | "es-cl" | "es_es" | "spanish" | "español" => Some("es".to_string()),
        "en" | "en-us" | "en_us" | "english" => Some("en".to_string()),
        _ => None,
    }
}

fn openai_key_status(api_key: &str) -> OpenAiApiKeyStatus {
    OpenAiApiKeyStatus {
        configured: true,
        preview: Some(mask_secret(api_key)),
    }
}

fn mask_secret(value: &str) -> String {
    let value = value.trim();
    let prefix = value.chars().take(7).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    if value.chars().count() <= 12 {
        "********".to_string()
    } else {
        format!("{prefix}...{suffix}")
    }
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}
