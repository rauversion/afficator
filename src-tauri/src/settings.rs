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
const CIPHER_VERSION: u8 = 1;
const SECRET_LEN: usize = 32;

#[derive(Debug, Serialize)]
pub struct OpenAiApiKeyStatus {
    configured: bool,
    preview: Option<String>,
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
        return Err("Ingresa un OpenAI API key.".to_string());
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
