use crate::settings;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

#[derive(Debug, Serialize)]
pub(crate) struct BinaryStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub configured_path: Option<String>,
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
    let command_path = configured_path
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| resolve_binary(app, name));
    let mut command = match command_path.as_ref() {
        Some(path) => Command::new(path),
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
                path: command_path
                    .map(path_to_string)
                    .or_else(|| Some(name.to_string())),
                configured_path,
                message: Some(format!("{name} esta disponible.")),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            BinaryStatus {
                installed: false,
                version: None,
                path: command_path.map(path_to_string),
                configured_path,
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
            path: command_path.map(path_to_string),
            configured_path,
            message: Some(format!("No se pudo ejecutar {name}: {error}")),
        },
    }
}

fn binary_command(app: &AppHandle, name: &str) -> Command {
    if let Ok(Some(path)) = configured_binary_path(app, name) {
        return Command::new(path);
    }

    match resolve_binary(app, name) {
        Some(path) => Command::new(path),
        None => Command::new(name),
    }
}

fn resolve_binary(app: &AppHandle, name: &str) -> Option<PathBuf> {
    binary_candidates(app, name)
        .into_iter()
        .find(|path| is_executable_candidate(path))
}

fn binary_candidates(app: &AppHandle, name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(Some(path)) = configured_binary_path(app, name) {
        candidates.push(PathBuf::from(path));
        return candidates;
    }

    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            push_binary_candidate(&mut candidates, &directory, name);
        }
    }

    for path in settings::default_binary_paths(name) {
        let candidate = PathBuf::from(path);
        if candidate.components().count() > 1 {
            candidates.push(candidate);
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

fn push_binary_candidate(candidates: &mut Vec<PathBuf>, directory: &Path, name: &str) {
    candidates.push(directory.join(name));

    #[cfg(windows)]
    if !name.to_ascii_lowercase().ends_with(".exe") {
        candidates.push(directory.join(format!("{name}.exe")));
    }
}

fn is_executable_candidate(path: &Path) -> bool {
    path.is_file()
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}
