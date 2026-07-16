use crate::settings;
use serde::Serialize;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BinarySource {
    Configured,
    Bundled,
    System,
}

#[derive(Debug)]
struct ResolvedBinary {
    path: PathBuf,
    source: BinarySource,
}

#[derive(Debug, Serialize)]
pub(crate) struct BinaryStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub configured_path: Option<String>,
    pub source: Option<BinarySource>,
    pub message: Option<String>,
}

pub(crate) fn ffmpeg_command(app: &AppHandle) -> Command {
    binary_command(app, "ffmpeg")
}

pub(crate) fn ffprobe_command(app: &AppHandle) -> Command {
    binary_command(app, "ffprobe")
}

pub(crate) fn binary_status(app: &AppHandle, name: &str) -> BinaryStatus {
    let configured_path = configured_binary_path(app, name).ok().flatten();
    let resolved = resolve_binary(app, name);
    let mut command = match resolved.as_ref() {
        Some(binary) => Command::new(&binary.path),
        None => Command::new(name),
    };

    match command.arg("-version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = stdout
                .lines()
                .next()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty());

            BinaryStatus {
                installed: true,
                version,
                path: resolved
                    .as_ref()
                    .map(|binary| path_to_string(binary.path.clone()))
                    .or_else(|| Some(name.to_string())),
                configured_path,
                source: resolved.as_ref().map(|binary| binary.source),
                message: Some(format!("{name} esta disponible.")),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            BinaryStatus {
                installed: false,
                version: None,
                path: resolved
                    .as_ref()
                    .map(|binary| path_to_string(binary.path.clone())),
                configured_path,
                source: resolved.as_ref().map(|binary| binary.source),
                message: Some(format!(
                    "{name} respondio con estado {}. {}",
                    output.status,
                    stderr.trim()
                )),
            }
        }
        Err(error) => BinaryStatus {
            installed: false,
            version: None,
            path: resolved
                .as_ref()
                .map(|binary| path_to_string(binary.path.clone())),
            configured_path,
            source: resolved.as_ref().map(|binary| binary.source),
            message: Some(format!("No se pudo ejecutar {name}: {error}")),
        },
    }
}

pub(crate) fn create_dir_error_message(app: &AppHandle, path: &Path, error: &io::Error) -> String {
    let base_es = format!("No se pudo crear la carpeta {}: {error}", path.display());
    let base_en = format!("Could not create folder {}: {error}", path.display());

    if error.kind() != io::ErrorKind::PermissionDenied {
        return settings::localized(app, &base_es, &base_en);
    }

    let hint_es = if is_external_volume_path(path) {
        "macOS bloqueo el acceso al disco externo. En Ajustes del Sistema > Privacidad y seguridad, permite a Rau Studio acceder a Volumenes extraibles o agrega Rau Studio a Acceso total al disco. Tambien verifica en Finder que el disco no este en solo lectura y que puedas crear carpetas ahi."
    } else {
        "macOS bloqueo el acceso a esa carpeta. Revisa permisos de la carpeta o agrega Rau Studio a Acceso total al disco en Ajustes del Sistema > Privacidad y seguridad."
    };
    let hint_en = if is_external_volume_path(path) {
        "macOS blocked access to the external drive. In System Settings > Privacy & Security, allow Rau Studio to access Removable Volumes or add Rau Studio to Full Disk Access. Also verify in Finder that the drive is not read-only and that you can create folders there."
    } else {
        "macOS blocked access to that folder. Check the folder permissions or add Rau Studio to Full Disk Access in System Settings > Privacy & Security."
    };

    settings::localized(
        app,
        &format!("{base_es}. {hint_es}"),
        &format!("{base_en}. {hint_en}"),
    )
}

pub(crate) fn is_external_volume_path(path: &Path) -> bool {
    path.starts_with("/Volumes")
}

fn binary_command(app: &AppHandle, name: &str) -> Command {
    match resolve_binary(app, name) {
        Some(binary) => Command::new(binary.path),
        None => Command::new(name),
    }
}

fn resolve_binary(app: &AppHandle, name: &str) -> Option<ResolvedBinary> {
    select_binary(binary_candidates(app, name))
}

fn select_binary(candidates: Vec<ResolvedBinary>) -> Option<ResolvedBinary> {
    candidates.into_iter().find(|binary| {
        binary.source == BinarySource::Configured || is_executable_candidate(&binary.path)
    })
}

fn binary_candidates(app: &AppHandle, name: &str) -> Vec<ResolvedBinary> {
    let mut candidates = Vec::new();

    if let Ok(Some(path)) = configured_binary_path(app, name) {
        candidates.push(ResolvedBinary {
            path: PathBuf::from(path),
            source: BinarySource::Configured,
        });
        return candidates;
    }

    candidates.extend(
        bundled_binary_candidates(name)
            .into_iter()
            .map(|path| ResolvedBinary {
                path,
                source: BinarySource::Bundled,
            }),
    );

    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            push_binary_candidate(&mut candidates, &directory, name, BinarySource::System);
        }
    }

    for path in settings::default_binary_paths(name) {
        let candidate = PathBuf::from(path);
        if candidate.components().count() > 1 {
            candidates.push(ResolvedBinary {
                path: candidate,
                source: BinarySource::System,
            });
        }
    }

    candidates
}

fn configured_binary_path(app: &AppHandle, name: &str) -> Result<Option<String>, String> {
    let paths = settings::load_audio_tool_paths(app)?;
    match name {
        "ffmpeg" => Ok(paths.ffmpeg_path),
        "ffprobe" => Ok(paths.ffprobe_path),
        _ => Ok(None),
    }
}

fn push_binary_candidate(
    candidates: &mut Vec<ResolvedBinary>,
    directory: &Path,
    name: &str,
    source: BinarySource,
) {
    candidates.push(ResolvedBinary {
        path: directory.join(name),
        source,
    });

    #[cfg(windows)]
    if !name.to_ascii_lowercase().ends_with(".exe") {
        candidates.push(ResolvedBinary {
            path: directory.join(format!("{name}.exe")),
            source,
        });
    }
}

fn is_executable_candidate(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    return metadata.permissions().mode() & 0o111 != 0;

    #[cfg(not(unix))]
    true
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(target_os = "macos")]
fn bundled_binary_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let suffixed_name = format!("{name}-{}-apple-darwin", env::consts::ARCH);

    if let Ok(executable) = env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join(name));
            candidates.push(directory.join(&suffixed_name));
        }
    }

    if cfg!(debug_assertions) {
        candidates.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join(suffixed_name),
        );
    }

    candidates
}

#[cfg(not(target_os = "macos"))]
fn bundled_binary_candidates(_name: &str) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{select_binary, BinarySource, ResolvedBinary};
    use std::path::PathBuf;

    #[test]
    fn configured_path_remains_a_strict_override_when_missing() {
        let configured = PathBuf::from("/definitely/missing/ffmpeg");
        let selected = select_binary(vec![ResolvedBinary {
            path: configured.clone(),
            source: BinarySource::Configured,
        }])
        .expect("configured candidates are selected so the UI can report their error");

        assert_eq!(selected.path, configured);
        assert_eq!(selected.source, BinarySource::Configured);
    }

    #[test]
    fn missing_automatic_candidates_are_skipped() {
        let selected = select_binary(vec![ResolvedBinary {
            path: PathBuf::from("/definitely/missing/bundled-ffmpeg"),
            source: BinarySource::Bundled,
        }]);

        assert!(selected.is_none());
    }
}
