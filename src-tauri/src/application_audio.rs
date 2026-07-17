use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const APPLICATION_AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const SYSTEM_AUDIO_TARGET_ID: &str = "__system_audio__";
pub const SYSTEM_AUDIO_LABEL: &str = "Salida completa del Mac";
const APPLICATION_AUDIO_BUFFER_SECONDS: usize = 2;

#[derive(Debug, Clone)]
pub struct ApplicationAudioDevice {
    pub id: String,
    pub label: String,
    pub process_id: i32,
}

pub struct ApplicationAudioCapture {
    #[cfg(target_os = "macos")]
    stream: screencapturekit::stream::SCStream,
}

pub struct ApplicationAudioCaptureParts {
    pub capture: ApplicationAudioCapture,
    pub buffer: Arc<Mutex<VecDeque<[i16; 2]>>>,
    pub stream_error: Arc<Mutex<Option<String>>>,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use screencapturekit::prelude::*;
    use screencapturekit::stream::delegate_trait::ErrorHandler;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    struct AudioHandler {
        buffer: Arc<Mutex<VecDeque<[i16; 2]>>>,
        stream_error: Arc<Mutex<Option<String>>>,
        maximum_frames: usize,
    }

    impl SCStreamOutputTrait for AudioHandler {
        fn did_output_sample_buffer(
            &self,
            sample: CMSampleBuffer,
            output_type: SCStreamOutputType,
        ) {
            if output_type != SCStreamOutputType::Audio {
                return;
            }
            let supported = sample.format_description().is_some_and(|format| {
                format.audio_is_float() && format.audio_bits_per_channel() == Some(32)
            });
            if !supported {
                if let Ok(mut target) = self.stream_error.lock() {
                    *target = Some(
                        "La aplicación entregó un formato de audio no soportado; se esperaba PCM Float32."
                            .to_string(),
                    );
                }
                return;
            }
            let Some(audio_buffers) = sample.audio_buffer_list() else {
                return;
            };
            let Ok(mut target) = self.buffer.lock() else {
                return;
            };
            append_float32_audio(&audio_buffers, &mut target);
            if target.len() > self.maximum_frames {
                let excess = target.len() - self.maximum_frames;
                target.drain(..excess);
            }
        }
    }

    pub fn list_applications() -> Result<Vec<ApplicationAudioDevice>, String> {
        let content = shareable_content()?;
        let own_process_id = std::process::id() as i32;
        let mut applications = content
            .applications()
            .into_iter()
            .filter_map(|application| {
                let id = application.bundle_identifier();
                let label = application.application_name();
                let process_id = application.process_id();
                (!id.trim().is_empty() && !label.trim().is_empty() && process_id != own_process_id)
                    .then_some(ApplicationAudioDevice {
                        id,
                        label,
                        process_id,
                    })
            })
            .collect::<Vec<_>>();
        applications.sort_by(|left, right| {
            left.label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.process_id.cmp(&right.process_id))
        });
        applications.dedup_by(|left, right| left.id == right.id);
        Ok(applications)
    }

    pub fn start_capture(target_id: &str) -> Result<ApplicationAudioCaptureParts, String> {
        let content = shareable_content()?;
        let displays = content.displays();
        let display = displays
            .first()
            .ok_or_else(|| "No hay una pantalla disponible para capturar audio.".to_string())?;
        let filter = if target_id == SYSTEM_AUDIO_TARGET_ID {
            SCContentFilter::create()
                .with_display(display)
                .with_excluding_applications(&[], &[])
                .build()
        } else {
            let applications = content.applications();
            let application = applications
                .iter()
                .find(|application| application.bundle_identifier() == target_id)
                .ok_or_else(|| {
                    "La aplicación seleccionada ya no está abierta o disponible.".to_string()
                })?;
            SCContentFilter::create()
                .with_display(display)
                .with_including_applications(&[application], &[])
                .build()
        };
        let configuration = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_queue_depth(1)
            .with_shows_cursor(false)
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(APPLICATION_AUDIO_SAMPLE_RATE as i32)
            .with_channel_count(2);
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let stream_error = Arc::new(Mutex::new(None));
        let delegate_error = Arc::clone(&stream_error);
        let delegate = ErrorHandler::new(move |error| {
            if let Ok(mut target) = delegate_error.lock() {
                *target = Some(format!("La captura de audio del Mac se detuvo: {error}"));
            }
        });
        let mut stream = SCStream::new_with_delegate(&filter, &configuration, delegate);
        stream.add_output_handler(
            AudioHandler {
                buffer: Arc::clone(&buffer),
                stream_error: Arc::clone(&stream_error),
                maximum_frames: APPLICATION_AUDIO_SAMPLE_RATE as usize
                    * APPLICATION_AUDIO_BUFFER_SECONDS,
            },
            SCStreamOutputType::Audio,
        );
        stream.start_capture().map_err(|error| {
            format!(
                "No se pudo capturar el audio del Mac. Revisa el permiso de Grabación de pantalla y audio para Rau Studio: {error}"
            )
        })?;
        Ok(ApplicationAudioCaptureParts {
            capture: ApplicationAudioCapture { stream },
            buffer,
            stream_error,
        })
    }

    pub fn open_permission_settings() -> Result<(), String> {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudieron abrir los ajustes de privacidad: {error}"))
    }

    fn shareable_content() -> Result<SCShareableContent, String> {
        let granted = unsafe { CGPreflightScreenCaptureAccess() }
            || unsafe { CGRequestScreenCaptureAccess() };
        if !granted {
            return Err(
                "macOS no autorizó la captura. Activa Rau Studio en Privacidad y seguridad → Grabación de pantalla y audio del sistema, cierra completamente la app y vuelve a abrirla."
                    .to_string(),
            );
        }
        SCShareableContent::get().map_err(|error| {
            format!(
                "No se pudieron consultar aplicaciones. Autoriza Grabación de pantalla y audio para Rau Studio y vuelve a intentarlo: {error}"
            )
        })
    }

    fn append_float32_audio(
        audio_buffers: &screencapturekit::cm::AudioBufferList,
        target: &mut VecDeque<[i16; 2]>,
    ) {
        if audio_buffers.num_buffers() >= 2 {
            let Some(left) = audio_buffers.get(0) else {
                return;
            };
            let Some(right) = audio_buffers.get(1) else {
                return;
            };
            for (left, right) in float32_samples(left.data()).zip(float32_samples(right.data())) {
                target.push_back([float_to_i16(left), float_to_i16(right)]);
            }
            return;
        }

        let Some(buffer) = audio_buffers.get(0) else {
            return;
        };
        let channels = usize::try_from(buffer.number_channels).unwrap_or(0);
        if channels == 0 {
            return;
        }
        let samples = float32_samples(buffer.data()).collect::<Vec<_>>();
        for frame in samples.chunks_exact(channels) {
            let left = float_to_i16(frame[0]);
            let right = if channels > 1 {
                float_to_i16(frame[1])
            } else {
                left
            };
            target.push_back([left, right]);
        }
    }

    fn float32_samples(bytes: &[u8]) -> impl Iterator<Item = f32> + '_ {
        bytes
            .chunks_exact(4)
            .map(|sample| f32::from_ne_bytes([sample[0], sample[1], sample[2], sample[3]]))
    }

    fn float_to_i16(sample: f32) -> i16 {
        (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
    }

    impl ApplicationAudioCapture {
        pub fn stop(self) {
            let _ = self.stream.stop_capture();
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{list_applications, open_permission_settings, start_capture};

#[cfg(not(target_os = "macos"))]
pub fn list_applications() -> Result<Vec<ApplicationAudioDevice>, String> {
    Err("La captura de audio del sistema solo está disponible en macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn start_capture(_bundle_id: &str) -> Result<ApplicationAudioCaptureParts, String> {
    Err("La captura de audio del sistema solo está disponible en macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn open_permission_settings() -> Result<(), String> {
    Err("Los ajustes de captura de audio del sistema solo están disponibles en macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
impl ApplicationAudioCapture {
    pub fn stop(self) {}
}

#[cfg(test)]
mod tests {
    use super::{SYSTEM_AUDIO_LABEL, SYSTEM_AUDIO_TARGET_ID};

    #[test]
    fn float_pcm_conversion_is_clamped() {
        let convert = |sample: f32| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        assert_eq!(convert(2.0), i16::MAX);
        assert_eq!(convert(-2.0), -i16::MAX);
        assert_eq!(convert(0.0), 0);
    }

    #[test]
    fn system_audio_target_has_a_stable_persisted_id() {
        assert_eq!(SYSTEM_AUDIO_TARGET_ID, "__system_audio__");
        assert_eq!(SYSTEM_AUDIO_LABEL, "Salida completa del Mac");
    }
}
