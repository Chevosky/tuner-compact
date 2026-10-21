use crate::diagnostics::signal_health_codes;
use crate::i18n::{Language, LocalizedMessage};
use pitch_core::{get_note_display, DetectionFrame, Note};
use std::sync::{Arc, Mutex};

pub(crate) type SharedTunerState = Arc<Mutex<TunerViewState>>;

#[derive(Clone, Default)]
pub(crate) struct TunerViewState {
    pub(crate) cents: f32,
    pub(crate) confidence: f32,
    /// Stable signal-quality diagnostic codes owned by the native client.
    pub(crate) diagnostics: Vec<&'static str>,
    /// Stream-loss recovery telemetry codes from the native backend
    /// (backend-stream-lost / backend-recovery-*).
    pub(crate) backend_diagnostics: Vec<&'static str>,
    pub(crate) error: Option<LocalizedMessage>,
    pub(crate) frame_id: u64,
    pub(crate) frequency: Option<f32>,
    pub(crate) in_tune: bool,
    pub(crate) is_power: bool,
    pub(crate) level: f32,
    pub(crate) note: Option<String>,
    pub(crate) target: Option<Note>,
    pub(crate) sample_rate: f32,
    pub(crate) spectrum: Arc<[f32]>,
    pub(crate) waveform: Arc<[f32]>,
}

impl TunerViewState {
    pub(crate) fn apply(&mut self, frame: DetectionFrame, waveform: &[f32], sample_rate: f32) {
        self.cents = frame.cents;
        self.confidence = normalized_ratio(frame.confidence);
        self.diagnostics = signal_health_codes(waveform, sample_rate);
        self.error = None;
        self.frame_id = self.frame_id.wrapping_add(1);
        self.frequency = frame.freq.filter(|value| value.is_finite() && *value > 0.0);
        self.in_tune = frame.in_tune && self.frequency.is_some();
        self.is_power = frame.is_power;
        self.level = normalized_ratio(frame.level);
        self.note = Some(frame.note);
        self.target = frame.target;
        self.sample_rate = sample_rate;
        self.spectrum = frame.spectrum.into();
        self.waveform = Arc::from(waveform);
    }

    pub(crate) fn clear_detection(&mut self) {
        self.cents = 0.0;
        self.confidence = 0.0;
        self.diagnostics.clear();
        self.backend_diagnostics.clear();
        self.frequency = None;
        self.in_tune = false;
        self.is_power = false;
        self.level = 0.0;
        self.note = None;
        self.target = None;
        self.spectrum = Arc::default();
        self.waveform = Arc::default();
    }

    /// The large note and the cents meter must describe the same target.
    /// In preset mode the engine reports cents against a string, while `note`
    /// remains the nearest chromatic note. Keep both values available, but
    /// use the actual reference for the main instrument display.
    pub(crate) fn display_note(&self) -> Option<String> {
        self.frequency?;
        self.target
            .as_ref()
            .map(get_note_display)
            .or_else(|| self.note.clone())
    }
}

fn normalized_ratio(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Reduce a stream-recovery event code into the visible backend diagnostics.
/// A fatal recovery failure also surfaces as the session error.
pub(crate) fn apply_recovery_code(state: &mut TunerViewState, code: &'static str) {
    use crate::diagnostics::{
        diagnostic_hint, BACKEND_RECOVERY_ATTEMPTED, BACKEND_RECOVERY_FAILED,
        BACKEND_RECOVERY_SUCCEEDED, BACKEND_STREAM_LOST,
    };
    state.backend_diagnostics = match code {
        BACKEND_STREAM_LOST => vec![BACKEND_STREAM_LOST],
        BACKEND_RECOVERY_ATTEMPTED => vec![BACKEND_STREAM_LOST, BACKEND_RECOVERY_ATTEMPTED],
        BACKEND_RECOVERY_SUCCEEDED => vec![BACKEND_RECOVERY_SUCCEEDED],
        BACKEND_RECOVERY_FAILED => vec![BACKEND_RECOVERY_FAILED],
        _ => return,
    };
    if code == BACKEND_RECOVERY_FAILED {
        state.error = Some(LocalizedMessage::new(
            diagnostic_hint(code, Language::Spanish),
            diagnostic_hint(code, Language::English),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::TunerViewState;
    use pitch_core::{
        canonical_note_name, DetectionFrame, DetectorConfig, EngineConfig, FrameContext, Note,
        TunerEngine,
    };
    use tuner_test_support::{cents_error, load_session_replay_contract, read_fixture_capture};

    #[test]
    fn clamps_detector_ratios_at_the_ui_boundary() {
        let mut state = TunerViewState::default();
        state.apply(
            DetectionFrame {
                confidence: f32::NAN,
                level: 1.5,
                ..DetectionFrame::default()
            },
            &[],
            48_000.0,
        );

        assert_eq!(state.confidence, 0.0);
        assert_eq!(state.level, 1.0);
    }

    #[test]
    fn preset_reading_labels_the_reference_used_by_the_cents_meter() {
        let mut state = TunerViewState::default();
        // Regression from the user's 0.1.13 screenshot: the display said
        // G#4 / +384.7 cents although the cents reference was E4.
        state.apply(
            DetectionFrame {
                freq: Some(411.650_57),
                note: "G#4".to_string(),
                cents: 384.7,
                target: Some(Note {
                    name: "E",
                    octave: 4,
                    frequency: 329.627_56,
                }),
                ..DetectionFrame::default()
            },
            &[],
            48_000.0,
        );
        assert_eq!(state.display_note().as_deref(), Some("E4"));
        assert_eq!(state.note.as_deref(), Some("G#4"));
        assert_eq!(state.cents, 384.7);

        state.apply(
            DetectionFrame {
                freq: Some(411.650_57),
                note: "G#4".to_string(),
                cents: -15.3,
                target: None,
                ..DetectionFrame::default()
            },
            &[],
            48_000.0,
        );
        assert_eq!(state.display_note().as_deref(), Some("G#4"));
        assert_eq!(state.cents, -15.3);
    }

    #[test]
    fn silence_cannot_leave_an_in_tune_indicator_or_a_stale_note() {
        let mut state = TunerViewState::default();
        state.apply(
            DetectionFrame {
                freq: None,
                note: "A4".to_string(),
                in_tune: true,
                ..DetectionFrame::default()
            },
            &[],
            48_000.0,
        );
        assert_eq!(state.display_note(), None);
        assert!(!state.in_tune);
    }

    #[test]
    fn cloned_view_state_shares_signal_snapshots() {
        let mut state = TunerViewState::default();
        state.apply(
            DetectionFrame {
                spectrum: vec![0.2, 0.4, 0.8],
                ..DetectionFrame::default()
            },
            &[0.1, -0.1, 0.2],
            48_000.0,
        );
        let cloned = state.clone();

        assert!(std::sync::Arc::ptr_eq(&state.waveform, &cloned.waveform));
        assert!(std::sync::Arc::ptr_eq(&state.spectrum, &cloned.spectrum));
    }

    #[test]
    fn licensed_session_replay_reaches_egui_view_state() {
        let contract = load_session_replay_contract();
        assert_eq!(contract.schema_version, 1);

        for replay_case in contract.cases {
            let capture = read_fixture_capture(&replay_case.capture);
            let sample_rate = capture.sample_rate;
            let samples = capture.samples;
            let target = Note {
                frequency: replay_case.target.frequency,
                name: canonical_note_name(&replay_case.target.name).expect("canonical target"),
                octave: replay_case.target.octave,
            };
            let mut engine = TunerEngine::with_config(EngineConfig {
                detector: DetectorConfig::default().with_frequency_range(
                    contract.range.min_frequency,
                    contract.range.max_frequency,
                ),
                frame_context: Some(FrameContext {
                    display_targets: vec![target.clone()],
                    idle_target: Some(target.clone()),
                    selected_target: Some(target.clone()),
                    tuning_targets: vec![target.clone()],
                    ..FrameContext::default()
                }),
                spectrum_bins: 0,
                ..EngineConfig::default()
            });
            let hop_samples = (sample_rate * contract.hop_seconds).round() as usize;
            let expected_note = format!("{}{}", replay_case.target.name, replay_case.target.octave);
            let mut state = TunerViewState::default();
            let mut published_frames = 0usize;
            let mut processed_frames = 0usize;

            for frame_index in 0..contract.maximum_frames {
                let start = frame_index * hop_samples;
                let end = start + contract.window_samples;
                if end > samples.len() {
                    break;
                }
                let waveform = &samples[start..end];
                let frame = engine.process(waveform, sample_rate);
                let frequency = frame.freq;
                state.apply(frame, waveform, sample_rate);
                processed_frames += 1;
                if let Some(frequency) = frequency {
                    published_frames += 1;
                    assert_eq!(
                        state.note.as_deref(),
                        Some(expected_note.as_str()),
                        "{} note",
                        replay_case.id
                    );
                    assert!(
                        cents_error(frequency, target.frequency) < 35.0,
                        "{} published {frequency:.3} Hz for {:.3} Hz target",
                        replay_case.id,
                        target.frequency
                    );
                }
            }

            assert_eq!(state.frame_id, processed_frames as u64);
            assert_eq!(state.sample_rate, sample_rate);
            assert_eq!(state.waveform.len(), contract.window_samples);
            assert!(published_frames > 0, "{} never acquired", replay_case.id);
        }
    }
}
