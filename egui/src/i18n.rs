//! Small, presentation-only English/Spanish localization for the native app.
//!
//! No audio or updater worker reads the selected language. Messages retain
//! both translations so already-visible errors also change on the next frame.

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    Spanish,
    English,
}

impl Language {
    pub(crate) const fn text<'a>(self, spanish: &'a str, english: &'a str) -> &'a str {
        match self {
            Self::Spanish => spanish,
            Self::English => english,
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        self.text("Español", "English")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LanguagePreference {
    #[default]
    System,
    Spanish,
    English,
}

impl LanguagePreference {
    pub(crate) fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("es") => Self::Spanish,
            Some("en") => Self::English,
            _ => Self::System,
        }
    }

    pub(crate) const fn stored(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Spanish => "es",
            Self::English => "en",
        }
    }
}

pub(crate) struct Localization {
    pub(crate) preference: LanguagePreference,
    system_language: Language,
}

impl Default for Localization {
    fn default() -> Self {
        Self {
            preference: LanguagePreference::System,
            system_language: resolve_languages(&preferred_system_languages()),
        }
    }
}

impl Localization {
    pub(crate) fn language(&self) -> Language {
        match self.preference {
            LanguagePreference::System => self.system_language,
            LanguagePreference::Spanish => Language::Spanish,
            LanguagePreference::English => Language::English,
        }
    }

    pub(crate) fn system_language(&self) -> Language {
        self.system_language
    }

    pub(crate) fn set_preference(&mut self, preference: LanguagePreference) {
        if preference == LanguagePreference::System {
            // Refresh only on an explicit return to Automatic, never per
            // frame or from an audio callback. Startup also resolves once.
            self.system_language = resolve_languages(&preferred_system_languages());
        }
        self.preference = preference;
    }
}

fn resolve_languages<S: AsRef<str>>(preferred: &[S]) -> Language {
    preferred
        .iter()
        .find_map(|identifier| {
            let primary = identifier
                .as_ref()
                .trim()
                .split(['-', '_', '.', '@'])
                .next()?;
            if primary.eq_ignore_ascii_case("es") {
                Some(Language::Spanish)
            } else if primary.eq_ignore_ascii_case("en") {
                Some(Language::English)
            } else {
                None
            }
        })
        .unwrap_or(Language::English)
}

#[cfg(target_os = "macos")]
fn preferred_system_languages() -> Vec<String> {
    use std::ffi::{c_char, c_void, CStr};

    // CFLocaleCopyPreferredLanguages is available since macOS 10.5. Use the
    // system framework directly instead of a subprocess or an extra runtime.
    // The API returns canonicalized CFString IDs in preference order.
    type CFRef = *const c_void;
    const UTF8: u32 = 0x0800_0100;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> CFRef;
        fn CFArrayGetCount(array: CFRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
        fn CFStringGetLength(string: CFRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(string: CFRef, buffer: *mut c_char, size: isize, encoding: u32)
            -> u8;
        fn CFRelease(value: CFRef);
    }

    struct OwnedArray(CFRef);
    impl Drop for OwnedArray {
        fn drop(&mut self) {
            // SAFETY: this non-null array was returned by a Copy function,
            // has one owned reference, and is released exactly once.
            unsafe { CFRelease(self.0) };
        }
    }

    // SAFETY: Core Foundation documents the non-null result as an owned
    // array of CFString references. Elements remain alive while `array` is
    // held; no borrowed pointer leaves this function. All string buffers are
    // sized by CF and decoded only after successful NUL-terminated writing.
    unsafe {
        let array = CFLocaleCopyPreferredLanguages();
        if array.is_null() {
            return Vec::new();
        }
        let array = OwnedArray(array);
        let mut languages = Vec::new();
        for index in 0..CFArrayGetCount(array.0) {
            let string = CFArrayGetValueAtIndex(array.0, index);
            if string.is_null() {
                continue;
            }
            let Some(capacity) = CFStringGetMaximumSizeForEncoding(CFStringGetLength(string), UTF8)
                .checked_add(1)
                .filter(|capacity| *capacity > 0)
            else {
                continue;
            };
            let mut buffer = vec![0_u8; capacity as usize];
            if CFStringGetCString(string, buffer.as_mut_ptr().cast(), capacity, UTF8) != 0 {
                if let Ok(language) = CStr::from_ptr(buffer.as_ptr().cast()).to_str() {
                    languages.push(language.to_owned());
                }
            }
        }
        languages
    }
}

#[cfg(not(target_os = "macos"))]
fn preferred_system_languages() -> Vec<String> {
    // Useful for other native hosts and headless tests. A bundled macOS app
    // deliberately never relies on shell locale variables.
    let mut languages = Vec::new();
    if let Ok(value) = std::env::var("LANGUAGE") {
        languages.extend(value.split(':').map(str::to_owned));
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                languages.push(value);
                break;
            }
        }
    }
    languages
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalizedMessage {
    spanish: Cow<'static, str>,
    english: Cow<'static, str>,
}

impl LocalizedMessage {
    pub(crate) fn new(
        spanish: impl Into<Cow<'static, str>>,
        english: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            spanish: spanish.into(),
            english: english.into(),
        }
    }

    pub(crate) fn text(&self, language: Language) -> &str {
        language.text(&self.spanish, &self.english)
    }

    /// The shared audio-input API intentionally stays language-neutral and
    /// returns technical strings. Translate its known presentation prefixes
    /// at the UI boundary; preserve driver messages and device names intact.
    pub(crate) fn audio(error: String) -> Self {
        const EXACT: &[(&str, &str)] = &[
            (
                "Audio worker is unavailable",
                "El proceso de audio no está disponible",
            ),
            (
                "Audio worker stopped unexpectedly",
                "El proceso de audio terminó inesperadamente",
            ),
            (
                "No output device available",
                "No hay un dispositivo de salida disponible",
            ),
            (
                "No input microphone found",
                "No se encontró un micrófono de entrada",
            ),
            (
                "Could not initialize audio buffer pool",
                "No se pudo preparar la memoria de audio",
            ),
            (
                "Audio input stream is closed",
                "La entrada de audio está cerrada",
            ),
            (
                "Audio input supervisor ended before startup completed",
                "La supervisión de audio terminó antes de completar el inicio",
            ),
            (
                "Audio input start was cancelled",
                "Se canceló el inicio de la entrada de audio",
            ),
            (
                "Input stream stalled — no audio data from the device",
                "La entrada se detuvo: el dispositivo no envía audio",
            ),
        ];
        const PREFIXES: &[(&str, &str)] = &[
            (
                "Could not start audio worker: ",
                "No se pudo iniciar el proceso de audio: ",
            ),
            (
                "Could not read output config: ",
                "No se pudo leer la configuración de salida: ",
            ),
            (
                "Unsupported output sample format: ",
                "Formato de audio de salida no compatible: ",
            ),
            (
                "Could not play reference tone: ",
                "No se pudo reproducir el tono de referencia: ",
            ),
            (
                "Could not create reference tone stream: ",
                "No se pudo crear la salida del tono de referencia: ",
            ),
            (
                "Could not read microphone config: ",
                "No se pudo leer la configuración del micrófono: ",
            ),
            (
                "Could not start microphone stream: ",
                "No se pudo iniciar la entrada del micrófono: ",
            ),
            (
                "Input device is no longer available: ",
                "El dispositivo de entrada ya no está disponible: ",
            ),
            (
                "Unsupported microphone sample format: ",
                "Formato de audio del micrófono no compatible: ",
            ),
            (
                "Could not create microphone stream: ",
                "No se pudo crear la entrada del micrófono: ",
            ),
        ];
        if let Some((_, spanish)) = EXACT.iter().find(|(english, _)| *english == error) {
            return Self::new(*spanish, error);
        }
        for (english, spanish) in PREFIXES {
            if let Some(detail) = error.strip_prefix(english) {
                return Self::new(format!("{spanish}{detail}"), error);
            }
        }
        if let Some(recovery) =
            error.strip_prefix("Audio input stream could not be recovered after ")
        {
            if let Some((attempts, detail)) = recovery.split_once(" attempts: ") {
                let detail = Self::audio(detail.to_owned());
                return Self::new(
                    format!("No se pudo recuperar la entrada de audio después de {attempts} intentos: {}", detail.text(Language::Spanish)),
                    error,
                );
            }
        }
        Self::new(
            format!("Error de audio: {error}"),
            format!("Audio error: {error}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_preferences_match_supported_regional_languages() {
        assert_eq!(resolve_languages(&["es-AR", "en-US"]), Language::Spanish);
        assert_eq!(resolve_languages(&["EN_gb", "es"]), Language::English);
        assert_eq!(
            resolve_languages(&["fr-FR", "es_MX", "en"]),
            Language::Spanish
        );
        assert_eq!(resolve_languages(&["es_ES.UTF-8"]), Language::Spanish);
        assert_eq!(resolve_languages(&["de", "ja"]), Language::English);
        assert_eq!(resolve_languages::<&str>(&[]), Language::English);
    }

    #[test]
    fn storage_values_are_stable_and_legacy_settings_default_to_system() {
        for preference in [
            LanguagePreference::System,
            LanguagePreference::Spanish,
            LanguagePreference::English,
        ] {
            assert_eq!(
                LanguagePreference::from_stored(Some(preference.stored())),
                preference
            );
        }
        assert_eq!(
            LanguagePreference::from_stored(None),
            LanguagePreference::System
        );
        assert_eq!(
            LanguagePreference::from_stored(Some("unknown")),
            LanguagePreference::System
        );
    }

    #[test]
    fn manual_preference_overrides_system_without_restarting_any_service() {
        let mut locale = Localization {
            preference: LanguagePreference::System,
            system_language: Language::Spanish,
        };
        assert_eq!(locale.language(), Language::Spanish);
        locale.set_preference(LanguagePreference::English);
        assert_eq!(locale.language(), Language::English);
        assert_eq!(locale.system_language(), Language::Spanish);
        locale.set_preference(LanguagePreference::Spanish);
        assert_eq!(locale.language(), Language::Spanish);
    }

    #[test]
    fn existing_audio_error_switches_language_and_keeps_external_detail() {
        let message =
            LocalizedMessage::audio("Could not read microphone config: USB device 7".to_owned());
        assert_eq!(
            message.text(Language::Spanish),
            "No se pudo leer la configuración del micrófono: USB device 7"
        );
        assert_eq!(
            message.text(Language::English),
            "Could not read microphone config: USB device 7"
        );
        let unknown = LocalizedMessage::audio("CoreAudio -50".to_owned());
        assert_eq!(
            unknown.text(Language::Spanish),
            "Error de audio: CoreAudio -50"
        );
        assert_eq!(
            unknown.text(Language::English),
            "Audio error: CoreAudio -50"
        );
    }
}
