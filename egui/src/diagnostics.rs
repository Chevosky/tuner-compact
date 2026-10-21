//! Signal-quality watchdog for the egui frontend.
//!
//! Codes and thresholds here belong to the native client and are tested here;
//! the web client has an independent presentation contract.

use crate::i18n::Language;
use pitch_core::{FIXED_GATE_PEAK, FIXED_GATE_RMS};

pub(crate) const SIGNAL_SILENT: &str = "signal-silent";
pub(crate) const SIGNAL_CLIPPING: &str = "signal-clipping";
pub(crate) const SIGNAL_DC_OFFSET: &str = "signal-dc-offset";
pub(crate) const SIGNAL_HUM: &str = "signal-hum";

// Stream-loss recovery telemetry codes emitted by audio-input::recovery.
pub(crate) const BACKEND_STREAM_LOST: &str = "backend-stream-lost";
pub(crate) const BACKEND_RECOVERY_ATTEMPTED: &str = "backend-recovery-attempted";
pub(crate) const BACKEND_RECOVERY_SUCCEEDED: &str = "backend-recovery-succeeded";
pub(crate) const BACKEND_RECOVERY_FAILED: &str = "backend-recovery-failed";

const HUM_CANDIDATE_FREQUENCIES: [f32; 2] = [50.0, 60.0];
const HUM_ANALYSIS_MAX_SAMPLES: usize = 16384;
/// The hum component must carry at least this share of the signal RMS.
const HUM_DOMINANCE_RATIO: f32 = 0.35;
/// Absolute amplitude floor so near-silence never reports hum.
const HUM_MIN_AMPLITUDE: f32 = 0.005;
/// Peak at/above this fraction of full scale is treated as clipping.
const CLIPPING_PEAK_THRESHOLD: f32 = 0.97;
/// |mean| at/above this value means a harmful DC offset.
const DC_OFFSET_THRESHOLD: f32 = 0.02;

/// Evaluate signal health for one analysis window and return the stable
/// diagnostic codes that apply, in contract order.
pub(crate) fn signal_health_codes(samples: &[f32], sample_rate: f32) -> Vec<&'static str> {
    if samples.is_empty() || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Vec::new();
    }
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut peak = 0.0f32;
    for &sample in samples {
        sum += sample;
        sum_sq += sample * sample;
        peak = peak.max(sample.abs());
    }
    let length = samples.len() as f32;
    let dc_offset = sum / length;
    let rms = (sum_sq / length).sqrt();

    // Silence masks every other signal-quality finding.
    if rms < FIXED_GATE_RMS && peak < FIXED_GATE_PEAK {
        return vec![SIGNAL_SILENT];
    }

    let mut codes = Vec::new();
    if peak >= CLIPPING_PEAK_THRESHOLD {
        codes.push(SIGNAL_CLIPPING);
    }
    if dc_offset.abs() >= DC_OFFSET_THRESHOLD {
        codes.push(SIGNAL_DC_OFFSET);
    }

    let window = samples.len().min(HUM_ANALYSIS_MAX_SAMPLES);
    let mut hum_amplitude = 0.0f32;
    for &candidate in &HUM_CANDIDATE_FREQUENCIES {
        hum_amplitude = hum_amplitude.max(goertzel_amplitude(
            &samples[..window],
            sample_rate,
            candidate,
        ));
    }
    if rms > 0.0 && hum_amplitude / rms >= HUM_DOMINANCE_RATIO && hum_amplitude >= HUM_MIN_AMPLITUDE
    {
        codes.push(SIGNAL_HUM);
    }
    codes
}

/// Single-bin Goertzel amplitude estimate (2|G|/N) for `frequency`.
fn goertzel_amplitude(samples: &[f32], sample_rate: f32, frequency: f32) -> f32 {
    let coefficient = 2.0 * (2.0 * std::f32::consts::PI * frequency / sample_rate).cos();
    let mut previous = 0.0f32;
    let mut before_previous = 0.0f32;
    for &sample in samples {
        let current = sample + coefficient * previous - before_previous;
        before_previous = previous;
        previous = current;
    }
    let power = before_previous * before_previous + previous * previous
        - coefficient * previous * before_previous;
    2.0 * power.max(0.0).sqrt() / samples.len() as f32
}

/// Short actionable hint rendered in the current UI language. Codes stay
/// stable across language changes and never depend on audio-thread state.
pub(crate) fn diagnostic_hint(code: &str, language: Language) -> &'static str {
    match code {
        SIGNAL_SILENT => language.text("sin señal de entrada — revisá la entrada y la ganancia", "no input signal — check the input and gain"),
        SIGNAL_CLIPPING => language.text("la señal satura — bajá el volumen de entrada", "signal hits full scale — lower the input volume"),
        SIGNAL_DC_OFFSET => language.text("componente continua — revisá el cable y el conector de entrada", "DC offset — check the input cable and connector"),
        SIGNAL_HUM => language.text("zumbido eléctrico — alejate de fuentes de ruido y revisá la conexión a tierra", "mains hum — move away from hum sources, check grounding"),
        BACKEND_STREAM_LOST => language.text("se perdió el audio — reconectando la entrada…", "audio stream lost — reconnecting the input…"),
        BACKEND_RECOVERY_ATTEMPTED => language.text("reconectando la entrada de audio…", "reconnecting the audio input…"),
        BACKEND_RECOVERY_SUCCEEDED => language.text("entrada de audio reconectada", "audio input reconnected"),
        BACKEND_RECOVERY_FAILED => language.text("no se pudo recuperar la entrada de audio — reconectá el dispositivo y volvé a escuchar", "could not restore the audio input — reconnect the device and restart listening"),
        _ => language.text("consultá el código de diagnóstico", "see the diagnostic code"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f32 = 48_000.0;

    #[test]
    fn silence_reports_signal_silent() {
        assert_eq!(
            signal_health_codes(&vec![0.0; 8192], SAMPLE_RATE),
            vec![SIGNAL_SILENT]
        );
    }

    #[test]
    fn clipping_reports_signal_clipping() {
        let samples: Vec<f32> = (0..8192)
            .map(|index| {
                (1.2 * (std::f32::consts::TAU * 440.0 * index as f32 / SAMPLE_RATE).sin())
                    .clamp(-1.0, 1.0)
            })
            .collect();
        let codes = signal_health_codes(&samples, SAMPLE_RATE);
        assert!(codes.contains(&SIGNAL_CLIPPING), "{codes:?}");
    }

    #[test]
    fn hum_reports_signal_hum() {
        let samples: Vec<f32> = (0..8192)
            .map(|index| {
                0.1 * (std::f32::consts::TAU * 50.0 * index as f32 / SAMPLE_RATE).sin()
                    + 0.01 * (std::f32::consts::TAU * 440.0 * index as f32 / SAMPLE_RATE).sin()
            })
            .collect();
        assert_eq!(signal_health_codes(&samples, SAMPLE_RATE), vec![SIGNAL_HUM]);
    }

    #[test]
    fn clean_tone_reports_no_codes() {
        let samples: Vec<f32> = (0..8192)
            .map(|index| 0.3 * (std::f32::consts::TAU * 440.0 * index as f32 / SAMPLE_RATE).sin())
            .collect();
        assert!(signal_health_codes(&samples, SAMPLE_RATE).is_empty());
    }
}
